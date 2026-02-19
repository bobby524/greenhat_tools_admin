use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Instant;

use axum::extract::{ConnectInfo, Request, State};
use axum::middleware::Next;
use axum::response::Response;
use metrics::counter;
use tower_http::request_id::RequestId;

use crate::audit::{AuditEvent, AuditLog};
use crate::error::AppError;

// ---------------------------------------------------------------------------
// Token-bucket per-IP rate limiter (in-memory)
// ---------------------------------------------------------------------------

/// Shared rate-limiter state.  Cloning is cheap (inner Arc).
#[derive(Clone)]
pub struct RateLimiter {
    buckets: Arc<Mutex<HashMap<String, TokenBucket>>>,
    rps: f64,
    burst: f64,
}

struct TokenBucket {
    tokens: f64,
    last_refill: Instant,
}

impl RateLimiter {
    pub fn new(rps: f64, burst: u32) -> Self {
        Self {
            buckets: Arc::new(Mutex::new(HashMap::new())),
            rps,
            burst: burst as f64,
        }
    }

    /// Try to consume one token for `key`.  Returns `true` if the request is
    /// allowed, `false` if the caller should be throttled.
    pub fn check(&self, key: &str) -> bool {
        let mut map = lock_buckets_or_recover(&self.buckets);
        let now = Instant::now();

        let bucket = map.entry(key.to_owned()).or_insert(TokenBucket {
            tokens: self.burst,
            last_refill: now,
        });

        // Refill based on elapsed time
        let elapsed = now.duration_since(bucket.last_refill).as_secs_f64();
        bucket.tokens = (bucket.tokens + elapsed * self.rps).min(self.burst);
        bucket.last_refill = now;

        if bucket.tokens >= 1.0 {
            bucket.tokens -= 1.0;
            true
        } else {
            false
        }
    }

    /// Return the configured RPS limit (for audit payloads).
    pub fn rps(&self) -> f64 {
        self.rps
    }
}

fn lock_buckets_or_recover<'a>(
    buckets: &'a Mutex<HashMap<String, TokenBucket>>,
) -> MutexGuard<'a, HashMap<String, TokenBucket>> {
    match buckets.lock() {
        Ok(g) => g,
        Err(poisoned) => {
            counter!(
                "lock_poison_recoveries_total",
                "component" => "rate_limiter",
                "lock" => "buckets"
            )
            .increment(1);
            tracing::error!("rate limiter bucket lock poisoned; recovering with inner state");
            poisoned.into_inner()
        }
    }
}

// ---------------------------------------------------------------------------
// Combined state for the middleware
// ---------------------------------------------------------------------------

/// Rate-limit middleware state: limiter + audit log.
#[derive(Clone)]
pub struct RateLimitState {
    pub read_limiter: RateLimiter,
    pub write_limiter: RateLimiter,
    pub audit: AuditLog,
}

// ---------------------------------------------------------------------------
// Middleware function
// ---------------------------------------------------------------------------

/// Axum middleware: reject requests that exceed the per-IP rate limit.
///
/// Emits `policy.rate_limit_hit` audit event on rejection.
pub async fn rate_limit_middleware(
    State(state): State<RateLimitState>,
    request: Request,
    next: Next,
) -> Result<Response, AppError> {
    let ip = extract_client_ip(&request);
    let method = request.method().clone();
    let path = request.uri().path().to_owned();
    let key = extract_principal_key(&request, &ip, &path);

    let (limiter, layer) =
        if method == axum::http::Method::GET || method == axum::http::Method::HEAD {
            (&state.read_limiter, "read")
        } else {
            (&state.write_limiter, "write")
        };

    if !limiter.check(&key) {
        tracing::warn!(client_ip = %ip, rate_key = %key, path = %path, layer = %layer, "rate limit exceeded");

        let request_id = request
            .extensions()
            .get::<RequestId>()
            .and_then(|id| id.header_value().to_str().ok())
            .unwrap_or("unknown")
            .to_owned();

        state.audit.emit(AuditEvent::new(
            "policy.rate_limit_hit",
            &request_id,
            &ip,
            None,
            serde_json::json!({
                "layer": layer,
                "key": key,
                "limit_rps": limiter.rps(),
                "path": path,
            }),
        ));

        return Err(AppError::rate_limited());
    }

    Ok(next.run(request).await)
}

// ---------------------------------------------------------------------------
// Key extraction helpers
// ---------------------------------------------------------------------------

fn extract_principal_key(req: &Request, ip: &str, path: &str) -> String {
    // Prefer stable session-cookie fingerprint to avoid penalizing NAT-shared IPs.
    let session_fingerprint = req
        .headers()
        .get("cookie")
        .and_then(|v| v.to_str().ok())
        .and_then(extract_session_cookie)
        .map(hash_str)
        .unwrap_or_else(|| format!("ip:{}", ip));

    let route_bucket = route_bucket(path);
    format!("{}:{}", session_fingerprint, route_bucket)
}

fn extract_session_cookie(cookie_header: &str) -> Option<&str> {
    cookie_header.split(';').map(|p| p.trim()).find_map(|part| {
        let (name, value) = part.split_once('=')?;
        if name == "better-auth.session_token" || name == "__Secure-better-auth.session_token" {
            Some(value)
        } else {
            None
        }
    })
}

fn hash_str(input: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut h = DefaultHasher::new();
    input.hash(&mut h);
    format!("u:{:x}", h.finish())
}

fn route_bucket(path: &str) -> &str {
    if path.starts_with("/api/exponential/tasks") {
        "/api/exponential/tasks"
    } else if path.starts_with("/api/exponential/projects") {
        "/api/exponential/projects"
    } else if path.starts_with("/api/exponential/teams") {
        "/api/exponential/teams"
    } else {
        "other"
    }
}

// ---------------------------------------------------------------------------
// IP extraction helpers
// ---------------------------------------------------------------------------

/// Best-effort client IP: check proxy headers first, then ConnectInfo, then
/// fall back to `"unknown"`.
fn extract_client_ip(req: &Request) -> String {
    // 1. X-Forwarded-For (first entry = original client)
    if let Some(val) = req
        .headers()
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
    {
        if let Some(first) = val.split(',').next() {
            let trimmed = first.trim();
            if !trimmed.is_empty() {
                return trimmed.to_owned();
            }
        }
    }

    // 2. X-Real-IP
    if let Some(val) = req.headers().get("x-real-ip").and_then(|v| v.to_str().ok()) {
        let trimmed = val.trim();
        if !trimmed.is_empty() {
            return trimmed.to_owned();
        }
    }

    // 3. ConnectInfo (injected by axum::serve + TcpListener)
    if let Some(ConnectInfo(addr)) = req.extensions().get::<ConnectInfo<SocketAddr>>() {
        return addr.ip().to_string();
    }

    "unknown".to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn rate_limiter_recovers_after_poisoned_lock() {
        let limiter = RateLimiter::new(50.0, 3);
        let poisoned = limiter.clone();

        let _ = thread::spawn(move || {
            let _guard = poisoned.buckets.lock().unwrap();
            panic!("intentional poison");
        })
        .join();

        assert!(limiter.check("ip:127.0.0.1:other"));
    }
}
