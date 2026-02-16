//! JWT validation for programmatic bearer tokens.
//!
//! This module provides a `SessionValidator` implementation that verifies
//! Bearer JWTs locally using a JWKS endpoint (kid-based key rotation).
//!
//! Design goals:
//! - Fail-closed on signature / claim validation failures.
//! - Support key rotation via `kid` (JWKS refresh).
//! - Provide a hook for revocation (jti) checks (optional, Redis-backed).

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde::Deserialize;

use super::principal::{AuthMethod, Principal};
use super::session::{AuthError, SessionCredential, SessionValidator};

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct JwtAuthConfig {
    pub jwks_url: String,
    pub issuer: Option<String>,
    pub audience: Option<String>,
    pub http_timeout: Duration,

    /// If true, reject tokens missing a `jti` claim.
    pub require_jti: bool,

    /// Enable Redis-backed token revocation checks (jti denylist).
    pub revocation_enabled: bool,
}

impl JwtAuthConfig {
    pub fn from_env() -> Option<Self> {
        let jwks_url = std::env::var("JWT_JWKS_URL").ok()?;
        let issuer = std::env::var("JWT_ISSUER")
            .ok()
            .filter(|s| !s.trim().is_empty());
        let audience = std::env::var("JWT_AUDIENCE")
            .ok()
            .filter(|s| !s.trim().is_empty());
        let http_timeout = Duration::from_millis(
            std::env::var("JWT_JWKS_TIMEOUT_MS")
                .ok()
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(2_000),
        );
        let require_jti = std::env::var("JWT_REQUIRE_JTI")
            .ok()
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);

        // Revocation is only meaningful when Redis is configured.
        let redis_present = std::env::var("REDIS_URL")
            .ok()
            .is_some_and(|s| !s.trim().is_empty());

        let revocation_enabled = std::env::var("JWT_REVOCATION_ENABLED")
            .ok()
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(redis_present);

        Some(Self {
            jwks_url,
            issuer,
            audience,
            http_timeout,
            require_jti,
            revocation_enabled,
        })
    }
}

// ---------------------------------------------------------------------------
// JWKS
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
struct Jwks {
    keys: Vec<Jwk>,
}

#[derive(Debug, Clone, Deserialize)]
struct Jwk {
    kid: Option<String>,
    kty: String,
    #[serde(default)]
    n: Option<String>,
    #[serde(default)]
    e: Option<String>,
}

// ---------------------------------------------------------------------------
// Claims
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
struct Claims {
    sub: String,
    #[serde(default)]
    exp: Option<u64>,
    #[serde(default)]
    iss: Option<String>,
    #[serde(default)]
    aud: Option<serde_json::Value>,
    #[serde(default)]
    jti: Option<String>,
    #[serde(default)]
    roles: Option<Vec<String>>,
}

// ---------------------------------------------------------------------------
// Validator
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct JwtValidator {
    cfg: JwtAuthConfig,
    http: reqwest::Client,
    // kid -> decoding key
    keys: Arc<tokio::sync::RwLock<HashMap<String, jsonwebtoken::DecodingKey>>>,
    revocation: Arc<dyn RevocationStore>,
}

impl std::fmt::Debug for JwtValidator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JwtValidator")
            .field("jwks_url", &self.cfg.jwks_url)
            .field("issuer", &self.cfg.issuer)
            .field("audience", &self.cfg.audience)
            .field("require_jti", &self.cfg.require_jti)
            .field("revocation_enabled", &self.cfg.revocation_enabled)
            .finish_non_exhaustive()
    }
}

// ---------------------------------------------------------------------------
// Revocation store
// ---------------------------------------------------------------------------

#[async_trait]
pub trait RevocationStore: Send + Sync {
    async fn is_revoked(&self, jti: &str) -> Result<bool, AuthError>;
}

/// Default store: never revoked.
#[derive(Debug, Default)]
pub struct NoopRevocationStore;

#[async_trait]
impl RevocationStore for NoopRevocationStore {
    async fn is_revoked(&self, _jti: &str) -> Result<bool, AuthError> {
        Ok(false)
    }
}

/// Redis-backed jti denylist.
#[derive(Debug, Clone)]
pub struct RedisJtiDenylist {
    pool: deadpool_redis::Pool,
    key_prefix: String,
}

impl RedisJtiDenylist {
    pub fn from_env() -> Result<Self, AuthError> {
        let redis_url = std::env::var("REDIS_URL")
            .map_err(|_| AuthError::Upstream("REDIS_URL not set".into()))?;

        let key_prefix = std::env::var("JWT_REVOCATION_KEY_PREFIX")
            .unwrap_or_else(|_| "revoked:jti:".to_owned());

        let cfg = deadpool_redis::Config::from_url(redis_url);
        let pool = cfg
            .create_pool(Some(deadpool_redis::Runtime::Tokio1))
            .map_err(|e| AuthError::Upstream(format!("redis pool error: {e}")))?;

        Ok(Self { pool, key_prefix })
    }

    fn key(&self, jti: &str) -> String {
        format!("{}{}", self.key_prefix, jti)
    }
}

#[async_trait]
impl RevocationStore for RedisJtiDenylist {
    async fn is_revoked(&self, jti: &str) -> Result<bool, AuthError> {
        let mut conn = self
            .pool
            .get()
            .await
            .map_err(|e| AuthError::Upstream(format!("redis pool get: {e}")))?;

        let exists: bool = deadpool_redis::redis::cmd("EXISTS")
            .arg(self.key(jti))
            .query_async(&mut conn)
            .await
            .map_err(|e| AuthError::Upstream(format!("redis EXISTS error: {e}")))?;

        Ok(exists)
    }
}

impl JwtValidator {
    pub fn new(cfg: JwtAuthConfig) -> Result<Self, AuthError> {
        let store: Arc<dyn RevocationStore> = if cfg.revocation_enabled {
            match RedisJtiDenylist::from_env() {
                Ok(s) => Arc::new(s),
                Err(e) => {
                    tracing::warn!(error = ?e, "JWT revocation enabled but redis not available; disabling revocation");
                    Arc::new(NoopRevocationStore)
                }
            }
        } else {
            Arc::new(NoopRevocationStore)
        };

        Self::new_with_store(cfg, store)
    }

    pub(crate) fn new_with_store(
        cfg: JwtAuthConfig,
        store: Arc<dyn RevocationStore>,
    ) -> Result<Self, AuthError> {
        let http = reqwest::Client::builder()
            .timeout(cfg.http_timeout)
            .build()
            .map_err(|e| AuthError::Upstream(e.to_string()))?;

        Ok(Self {
            cfg,
            http,
            keys: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
            revocation: store,
        })
    }

    async fn refresh_jwks(&self) -> Result<(), AuthError> {
        let resp = self
            .http
            .get(&self.cfg.jwks_url)
            .send()
            .await
            .map_err(|e| AuthError::Upstream(format!("jwks fetch failed: {e}")))?;

        if !resp.status().is_success() {
            return Err(AuthError::Upstream(format!(
                "jwks fetch status {}",
                resp.status()
            )));
        }

        let jwks: Jwks = resp
            .json()
            .await
            .map_err(|e| AuthError::Upstream(format!("jwks parse failed: {e}")))?;

        let mut map = HashMap::new();
        for k in jwks.keys {
            if k.kty != "RSA" {
                continue;
            }
            let kid = match k.kid {
                Some(k) => k,
                None => continue,
            };
            let (n, e) = match (k.n, k.e) {
                (Some(n), Some(e)) => (n, e),
                _ => continue,
            };
            if let Ok(key) = jsonwebtoken::DecodingKey::from_rsa_components(&n, &e) {
                map.insert(kid, key);
            }
        }

        let mut guard = self.keys.write().await;
        *guard = map;
        Ok(())
    }

    async fn decode(&self, token: &str) -> Result<Claims, AuthError> {
        let header = jsonwebtoken::decode_header(token)
            .map_err(|_| AuthError::InvalidSession("invalid jwt header".into()))?;
        let kid = header
            .kid
            .clone()
            .ok_or_else(|| AuthError::InvalidSession("missing kid".into()))?;

        // Try cached key first.
        if let Some(key) = self.keys.read().await.get(&kid).cloned() {
            return self.decode_with_key(token, &key);
        }

        // Refresh keys and retry once.
        self.refresh_jwks().await?;
        let key = self
            .keys
            .read()
            .await
            .get(&kid)
            .cloned()
            .ok_or_else(|| AuthError::InvalidSession("unknown kid".into()))?;
        self.decode_with_key(token, &key)
    }

    fn decode_with_key(
        &self,
        token: &str,
        key: &jsonwebtoken::DecodingKey,
    ) -> Result<Claims, AuthError> {
        let mut validation = jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::RS256);
        validation.validate_exp = true;

        if let Some(ref iss) = self.cfg.issuer {
            validation.set_issuer(&[iss]);
        }
        if let Some(ref aud) = self.cfg.audience {
            validation.set_audience(&[aud]);
        }

        let data = jsonwebtoken::decode::<Claims>(token, key, &validation)
            .map_err(|_| AuthError::InvalidSession("invalid or expired token".into()))?;

        Ok(data.claims)
    }
}

impl JwtValidator {
    async fn validate_claims(&self, claims: Claims) -> Result<Principal, AuthError> {
        if self.cfg.require_jti && claims.jti.as_deref().unwrap_or("").is_empty() {
            return Err(AuthError::InvalidSession("missing jti".into()));
        }

        if self.cfg.revocation_enabled {
            if let Some(ref jti) = claims.jti {
                if self.revocation.is_revoked(jti).await? {
                    return Err(AuthError::InvalidSession("revoked_token".into()));
                }
            }
        }

        Ok(Principal {
            user_id: claims.sub,
            org_id: None,
            roles: claims.roles.unwrap_or_default(),
            session_id: claims.jti.unwrap_or_else(|| "-".into()),
            auth_method: AuthMethod::Bearer,
        })
    }
}

#[async_trait]
impl SessionValidator for JwtValidator {
    fn supports_cookie(&self) -> bool {
        false
    }
    fn supports_bearer(&self) -> bool {
        true
    }
    async fn validate_session(
        &self,
        credential: &SessionCredential,
    ) -> Result<Principal, AuthError> {
        let token = match credential {
            SessionCredential::Bearer(t) => t,
            SessionCredential::Cookie(_) => {
                return Err(AuthError::InvalidSession(
                    "jwt validator only supports bearer tokens".into(),
                ))
            }
        };

        let claims = self.decode(token).await?;
        self.validate_claims(claims).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::collections::HashSet;
    use std::sync::Mutex;

    #[derive(Debug, Default)]
    struct MemStore {
        revoked: Mutex<HashSet<String>>,
    }

    #[async_trait]
    impl RevocationStore for MemStore {
        async fn is_revoked(&self, jti: &str) -> Result<bool, AuthError> {
            Ok(self.revoked.lock().unwrap().contains(jti))
        }
    }

    #[tokio::test]
    async fn revoked_jti_is_rejected() {
        let cfg = JwtAuthConfig {
            jwks_url: "https://jwks.example.com".into(),
            issuer: None,
            audience: None,
            http_timeout: Duration::from_millis(1000),
            require_jti: true,
            revocation_enabled: true,
        };

        let store = Arc::new(MemStore::default());
        store.revoked.lock().unwrap().insert("jti-1".to_string());

        let v = JwtValidator::new_with_store(cfg, store).unwrap();

        let claims = Claims {
            sub: "u1".into(),
            exp: Some(9999999999),
            iss: None,
            aud: None,
            jti: Some("jti-1".into()),
            roles: None,
        };

        let err = v.validate_claims(claims).await.unwrap_err();
        match err {
            AuthError::InvalidSession(msg) => assert!(msg.contains("revoked")),
            other => panic!("expected InvalidSession, got {other:?}"),
        }
    }
}
