//! Session validation trait and BetterAuth HTTP implementation.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde::Deserialize;

use super::principal::{AuthMethod, Principal};

// ---------------------------------------------------------------------------
// Credential & Error types
// ---------------------------------------------------------------------------

/// A credential extracted from an incoming HTTP request.
#[derive(Debug)]
pub enum SessionCredential {
    /// Session token from the BetterAuth cookie.
    Cookie(String),
    /// Bearer token from the `Authorization` header.
    Bearer(String),
}

/// Errors that can occur during session validation.
#[derive(Debug)]
pub enum AuthError {
    /// The credential is invalid, expired, or revoked.
    InvalidSession(String),
    /// The upstream auth service is unreachable or returned an unexpected response.
    Upstream(String),
}

// ---------------------------------------------------------------------------
// Trait
// ---------------------------------------------------------------------------

/// Abstraction over session validation so the middleware is testable
/// without a running BetterAuth server.
#[async_trait]
pub trait SessionValidator: Send + Sync {
    async fn validate_session(
        &self,
        credential: &SessionCredential,
    ) -> Result<Principal, AuthError>;

    /// Optional hint for middleware to understand which credential types
    /// the validator expects.
    fn supports_cookie(&self) -> bool {
        true
    }
    fn supports_bearer(&self) -> bool {
        true
    }
}

// ---------------------------------------------------------------------------
// Shared auth state (passed to the middleware via axum `State`)
// ---------------------------------------------------------------------------

/// State injected into the auth middleware layer.
#[derive(Clone)]
pub struct AuthState {
    /// The session validator implementation.
    pub validator: Arc<dyn SessionValidator>,
    /// Name of the BetterAuth session cookie (default: `better-auth.session_token`).
    pub cookie_name: String,
    /// Paths that are exempt from authentication checks.
    pub exempt_paths: Vec<String>,
    /// Audit log handle (optional — `None` disables audit emission).
    pub audit: Option<crate::audit::AuditLog>,
}

impl AuthState {
    /// Convenience constructor with the default cookie name.
    pub fn new(validator: Arc<dyn SessionValidator>) -> Self {
        Self {
            validator,
            cookie_name: "better-auth.session_token".into(),
            exempt_paths: vec!["/health".into(), "/version".into(), "/metrics".into()],
            audit: None,
        }
    }

    /// Convenience constructor with a custom cookie name.
    pub fn with_cookie_name(
        validator: Arc<dyn SessionValidator>,
        cookie_name: impl Into<String>,
    ) -> Self {
        let mut s = Self::new(validator);
        s.cookie_name = cookie_name.into();
        s
    }

    /// Attach an audit log handle.
    pub fn with_audit(mut self, audit: crate::audit::AuditLog) -> Self {
        self.audit = Some(audit);
        self
    }

    /// Create an auth state that approves every request.
    ///
    /// **For testing only — never use in production.**
    pub fn noop() -> Self {
        Self::new(Arc::new(NoopValidator))
    }
}

// ---------------------------------------------------------------------------
// BetterAuth HTTP client
// ---------------------------------------------------------------------------

/// Validates sessions by calling BetterAuth's `GET /api/auth/get-session`.
///
/// Works for both cookie-based (browser) and Bearer-token (server-to-server)
/// authentication: the gateway forwards the appropriate credential header and
/// BetterAuth returns the session + user payload, or `null` if invalid.
#[derive(Debug, Clone)]
pub struct BetterAuthClient {
    http: reqwest::Client,
    base_url: String,
    cookie_name: String,
}

/// Composite validator: use one validator for cookie credentials and another for bearer credentials.
///
/// This matches the gateway strategy:
/// - Cookies (browser sessions) validated upstream via BetterAuth
/// - Bearer tokens validated locally via JWT (when configured)
#[derive(Clone)]
pub struct SplitValidator {
    pub cookie: std::sync::Arc<dyn SessionValidator>,
    pub bearer: std::sync::Arc<dyn SessionValidator>,
}

/// Mirrors the BetterAuth `get-session` response shape.
#[derive(Deserialize)]
struct BetterAuthSessionResponse {
    session: BaSession,
    user: BaUser,
}

#[derive(Deserialize)]
struct BaSession {
    id: String,
    #[serde(rename = "userId")]
    user_id: String,
}

#[derive(Deserialize)]
struct BaUser {
    id: String,
    #[serde(default)]
    role: Option<String>,
    /// Populated when the BetterAuth organization plugin is active.
    #[serde(rename = "organizationId", default)]
    organization_id: Option<String>,
}

impl BetterAuthClient {
    /// Create a new client targeting `base_url` (e.g. `http://localhost:3000`).
    ///
    /// `cookie_name` must match BetterAuth's configured cookie name (e.g. `greenhat_tools.session_token`).
    pub fn new(
        base_url: impl Into<String>,
        timeout: Duration,
        cookie_name: impl Into<String>,
    ) -> Self {
        let http = reqwest::Client::builder()
            .timeout(timeout)
            .build()
            .expect("failed to build BetterAuth HTTP client");
        Self {
            http,
            base_url: base_url.into().trim_end_matches('/').to_owned(),
            cookie_name: cookie_name.into(),
        }
    }
}

#[async_trait]
impl SessionValidator for BetterAuthClient {
    fn supports_cookie(&self) -> bool {
        true
    }
    fn supports_bearer(&self) -> bool {
        true
    }
    async fn validate_session(
        &self,
        credential: &SessionCredential,
    ) -> Result<Principal, AuthError> {
        let url = format!("{}/api/auth/get-session", self.base_url);

        let mut req = self.http.get(&url);

        match credential {
            SessionCredential::Cookie(token) => {
                // Forward the session cookie so BetterAuth can read it.
                req = req.header("cookie", format!("{}={token}", self.cookie_name));
            }
            SessionCredential::Bearer(token) => {
                req = req.header("authorization", format!("Bearer {token}"));
            }
        }

        let resp = req.send().await.map_err(|e| {
            // NOTE: do NOT log the credential/token value — secrets stay out of logs.
            tracing::error!(
                error = %e,
                "BetterAuth upstream request failed"
            );
            AuthError::Upstream(e.to_string())
        })?;

        if !resp.status().is_success() {
            return Err(AuthError::InvalidSession(format!(
                "BetterAuth returned status {}",
                resp.status()
            )));
        }

        // BetterAuth returns `null` for invalid sessions (200 with null body).
        let body: Option<BetterAuthSessionResponse> = resp.json().await.map_err(|e| {
            tracing::error!(
                error = %e,
                "failed to deserialize BetterAuth session response"
            );
            AuthError::Upstream(e.to_string())
        })?;

        let data = body.ok_or_else(|| AuthError::InvalidSession("no active session".into()))?;

        let auth_method = match credential {
            SessionCredential::Cookie(_) => AuthMethod::Cookie,
            SessionCredential::Bearer(_) => AuthMethod::Bearer,
        };

        let mut roles = Vec::new();
        if let Some(role) = data.user.role {
            roles.push(role);
        }

        Ok(Principal {
            user_id: data.user.id,
            org_id: data.user.organization_id,
            roles,
            session_id: data.session.id,
            auth_method,
        })
    }
}

// ---------------------------------------------------------------------------
// Split validator (cookie vs bearer)
// ---------------------------------------------------------------------------

#[async_trait]
impl SessionValidator for SplitValidator {
    fn supports_cookie(&self) -> bool {
        self.cookie.supports_cookie()
    }
    fn supports_bearer(&self) -> bool {
        self.bearer.supports_bearer()
    }
    async fn validate_session(
        &self,
        credential: &SessionCredential,
    ) -> Result<Principal, AuthError> {
        match credential {
            SessionCredential::Cookie(_) => self.cookie.validate_session(credential).await,
            SessionCredential::Bearer(_) => self.bearer.validate_session(credential).await,
        }
    }
}

// ---------------------------------------------------------------------------
// Noop validator (for tests)
// ---------------------------------------------------------------------------

/// A validator that always succeeds — **testing only, never use in production**.
///
/// Returns a `Principal` whose [`AuthMethod`] matches the credential type,
/// so CSRF boundary checks work correctly in test scenarios.
pub struct NoopValidator;

#[async_trait]
impl SessionValidator for NoopValidator {
    fn supports_cookie(&self) -> bool {
        true
    }
    fn supports_bearer(&self) -> bool {
        true
    }
    async fn validate_session(
        &self,
        credential: &SessionCredential,
    ) -> Result<Principal, AuthError> {
        let auth_method = match credential {
            SessionCredential::Cookie(_) => AuthMethod::Cookie,
            SessionCredential::Bearer(_) => AuthMethod::Bearer,
        };
        Ok(Principal {
            user_id: "test-user".into(),
            org_id: None,
            roles: vec![],
            session_id: "test-session".into(),
            auth_method,
        })
    }
}

/// A validator that always rejects — **testing only**.
pub struct AlwaysInvalidValidator;

#[async_trait]
impl SessionValidator for AlwaysInvalidValidator {
    async fn validate_session(
        &self,
        _credential: &SessionCredential,
    ) -> Result<Principal, AuthError> {
        Err(AuthError::InvalidSession("mock: always invalid".into()))
    }
}
