//! BA_DECOUPLE_TAG: Session validation trait and BetterAuth HTTP implementation.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use metrics::counter;
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
    // small in-memory cache to avoid hammering BetterAuth on every parallel API call
    cache: Arc<Mutex<HashMap<String, (Instant, Principal)>>>,
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
            cache: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    fn cache_get(&self, cache_key: &str) -> Option<(Instant, Principal)> {
        match self.cache.lock() {
            Ok(cache) => cache.get(cache_key).cloned(),
            Err(poisoned) => {
                counter!(
                    "lock_poison_recoveries_total",
                    "component" => "session_cache",
                    "lock" => "cache"
                )
                .increment(1);
                tracing::error!("session cache lock poisoned on read; using recovered inner state");
                poisoned.into_inner().get(cache_key).cloned()
            }
        }
    }

    fn cache_insert(&self, cache_key: String, principal: Principal) {
        let mut cache = match self.cache.lock() {
            Ok(cache) => cache,
            Err(poisoned) => {
                counter!(
                    "lock_poison_recoveries_total",
                    "component" => "session_cache",
                    "lock" => "cache"
                )
                .increment(1);
                tracing::error!(
                    "session cache lock poisoned on write; using recovered inner state"
                );
                poisoned.into_inner()
            }
        };

        if cache.len() > 10_000 {
            counter!(
                "session_cache_evictions_total",
                "reason" => "max_size"
            )
            .increment(1);
            cache.clear();
        }
        cache.insert(cache_key, (Instant::now(), principal));
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
        let cache_key: String;
        let mut stale_fallback: Option<Principal> = None;

        match credential {
            SessionCredential::Cookie(raw_cookie_header) => {
                // Forward only the session cookie variants expected by Better Auth.
                // Passing the full browser Cookie header can trigger upstream 5xx
                // in some deployments (oversized/invalid companion cookies).
                let secure_cookie_name = format!("__Secure-{}", self.cookie_name);
                let candidates = [
                    self.cookie_name.as_str(),
                    secure_cookie_name.as_str(),
                    "better-auth.session_token",
                    "__Secure-better-auth.session_token",
                ];

                let mut forwarded: Vec<String> = Vec::new();
                let mut token_val: Option<String> = None;
                for pair in raw_cookie_header.split(';') {
                    let pair = pair.trim();
                    let Some((k, v)) = pair.split_once('=') else {
                        continue;
                    };
                    let key = k.trim();
                    let val = v.trim();
                    if candidates.iter().any(|c| *c == key) {
                        forwarded.push(format!("{}={}", key, val));
                        if token_val.is_none() {
                            token_val = Some(val.to_string());
                        }
                    }
                }

                let Some(token_val) = token_val else {
                    return Err(AuthError::InvalidSession("missing session cookie".into()));
                };

                cache_key = format!("cookie:{}", token_val);

                if let Some((ts, principal)) = self.cache_get(&cache_key) {
                    if ts.elapsed() < Duration::from_secs(20) {
                        return Ok(principal);
                    }
                    if ts.elapsed() < Duration::from_secs(600) {
                        stale_fallback = Some(principal);
                    }
                }

                req = req.header("cookie", forwarded.join("; "));
            }
            SessionCredential::Bearer(token) => {
                cache_key = format!("bearer:{}", token);
                if let Some((ts, principal)) = self.cache_get(&cache_key) {
                    if ts.elapsed() < Duration::from_secs(20) {
                        return Ok(principal);
                    }
                    if ts.elapsed() < Duration::from_secs(600) {
                        stale_fallback = Some(principal);
                    }
                }
                req = req.header("authorization", format!("Bearer {token}"));
            }
        }

        let resp = match req.send().await {
            Ok(resp) => resp,
            Err(e) => {
                // NOTE: do NOT log the credential/token value — secrets stay out of logs.
                tracing::error!(
                    error = %e,
                    "BetterAuth upstream request failed"
                );
                if let Some(principal) = stale_fallback {
                    tracing::warn!("BetterAuth upstream unreachable; using stale session fallback");
                    return Ok(principal);
                }
                return Err(AuthError::Upstream(e.to_string()));
            }
        };

        if !resp.status().is_success() {
            if resp.status().is_server_error() {
                if let Some(principal) = stale_fallback {
                    tracing::warn!(status = %resp.status(), "BetterAuth 5xx; using stale session fallback");
                    return Ok(principal);
                }
                return Err(AuthError::Upstream(format!(
                    "BetterAuth returned status {}",
                    resp.status()
                )));
            }
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

        let principal = Principal {
            user_id: data.user.id,
            org_id: data.user.organization_id,
            roles,
            session_id: data.session.id,
            auth_method,
        };

        self.cache_insert(cache_key, principal.clone());

        Ok(principal)
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    fn sample_principal() -> Principal {
        Principal {
            user_id: "u1".into(),
            org_id: None,
            roles: vec![],
            session_id: "s1".into(),
            auth_method: AuthMethod::Cookie,
        }
    }

    #[test]
    fn session_cache_read_recovers_after_poison() {
        let client = BetterAuthClient::new(
            "http://localhost:3000",
            Duration::from_millis(100),
            "better-auth.session_token",
        );
        client.cache_insert("cookie:t1".into(), sample_principal());

        let poisoned_cache = client.cache.clone();
        let _ = thread::spawn(move || {
            let _guard = poisoned_cache.lock().unwrap();
            panic!("intentional poison");
        })
        .join();

        let found = client.cache_get("cookie:t1");
        assert!(found.is_some());
    }

    #[test]
    fn session_cache_write_recovers_after_poison() {
        let client = BetterAuthClient::new(
            "http://localhost:3000",
            Duration::from_millis(100),
            "better-auth.session_token",
        );

        let poisoned_cache = client.cache.clone();
        let _ = thread::spawn(move || {
            let _guard = poisoned_cache.lock().unwrap();
            panic!("intentional poison");
        })
        .join();

        client.cache_insert("cookie:t2".into(), sample_principal());
        assert!(client.cache_get("cookie:t2").is_some());
    }
}
