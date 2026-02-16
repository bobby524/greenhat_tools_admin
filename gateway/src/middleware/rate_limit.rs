use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use axum::extract::{ConnectInfo, Request, State};
use axum::middleware::Next;
use axum::response::Response;
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
        let mut map = self.buckets.lock().expect("rate-limiter lock poisoned");
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

// ---------------------------------------------------------------------------
// Combined state for the middleware
// ---------------------------------------------------------------------------

/// Rate-limit middleware state: limiter + audit log.
#[derive(Clone)]
pub struct RateLimitState {
    pub limiter: RateLimiter,
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

    if !state.limiter.check(&ip) {
        tracing::warn!(client_ip = %ip, "rate limit exceeded");

        let request_id = request
            .extensions()
            .get::<RequestId>()
            .and_then(|id| id.header_value().to_str().ok())
            .unwrap_or("unknown")
            .to_owned();

        let path = request.uri().path().to_owned();

        state.audit.emit(AuditEvent::new(
            "policy.rate_limit_hit",
            &request_id,
            &ip,
            None,
            serde_json::json!({
                "layer": "ip",
                "key": ip,
                "limit_rps": state.limiter.rps(),
                "path": path,
            }),
        ));

        return Err(AppError::rate_limited());
    }

    Ok(next.run(request).await)
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
