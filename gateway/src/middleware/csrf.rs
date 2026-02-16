use axum::extract::{Request, State};
use axum::http::{header, HeaderValue, Method};
use axum::middleware::Next;
use axum::response::Response;
use tower_http::request_id::RequestId;

use crate::audit::{AuditEvent, AuditLog};
use crate::error::AppError;

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Double-submit cookie CSRF protection settings.
#[derive(Clone, Debug)]
pub struct CsrfConfig {
    /// Master switch — when `false` the middleware is a no-op.
    pub enabled: bool,
    /// Name of the cookie that carries the CSRF token
    /// (must be readable by JS → **not** HttpOnly).
    pub cookie_name: String,
    /// Name of the request header the SPA echoes the token into.
    pub header_name: String,
    /// Paths that are unconditionally exempt from CSRF checks
    /// (health / readiness / metrics probes).
    pub exempt_paths: Vec<String>,
    /// Audit log handle.
    pub audit: AuditLog,
}

impl Default for CsrfConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            cookie_name: "csrf_token".to_owned(),
            header_name: "x-csrf-token".to_owned(),
            exempt_paths: vec![
                "/health".to_owned(),
                "/version".to_owned(),
                "/metrics".to_owned(),
            ],
            audit: AuditLog::from_env(),
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn extract_request_id(req: &Request) -> String {
    req.extensions()
        .get::<RequestId>()
        .and_then(|id| id.header_value().to_str().ok())
        .unwrap_or("unknown")
        .to_owned()
}

fn extract_source_ip(req: &Request) -> String {
    if let Some(val) = req
        .headers()
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
    {
        if let Some(first) = val.split(',').next() {
            let t = first.trim();
            if !t.is_empty() {
                return t.to_owned();
            }
        }
    }
    if let Some(val) = req.headers().get("x-real-ip").and_then(|v| v.to_str().ok()) {
        let t = val.trim();
        if !t.is_empty() {
            return t.to_owned();
        }
    }
    "unknown".to_owned()
}

// ---------------------------------------------------------------------------
// Middleware
// ---------------------------------------------------------------------------

/// Axum middleware implementing the **double-submit cookie** CSRF pattern.
///
/// Emits `auth.csrf_reject` audit events on CSRF validation failures.
pub async fn csrf_middleware(
    State(config): State<CsrfConfig>,
    request: Request,
    next: Next,
) -> Result<Response, AppError> {
    // ── Disabled → pass-through ──────────────────────────────────────────
    if !config.enabled {
        return Ok(next.run(request).await);
    }

    let path = request.uri().path().to_owned();

    // ── Exempt paths → pass-through ──────────────────────────────────────
    if config.exempt_paths.iter().any(|p| path == *p) {
        return Ok(next.run(request).await);
    }

    let method = request.method().clone();

    // ── State-changing → enforce double-submit (cookie-auth only) ───────
    if is_state_changing(&method) {
        // If the request is bearer-authenticated, CSRF doesn't apply.
        if request
            .headers()
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|h| h.trim_start().starts_with("Bearer "))
        {
            return Ok(next.run(request).await);
        }
        let cookie_token = extract_cookie_value(&request, &config.cookie_name);
        let header_token = request
            .headers()
            .get(config.header_name.as_str())
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_owned());

        let request_id = extract_request_id(&request);
        let source_ip = extract_source_ip(&request);

        let csrf_ok = matches!(
            (&cookie_token, &header_token),
            (Some(c), Some(h)) if !c.is_empty() && c == h
        );

        if !csrf_ok {
            let reason = match (&cookie_token, &header_token) {
                (None, _) => "missing_csrf_cookie",
                (_, None) => "missing_csrf_header",
                _ => "csrf_token_mismatch",
            };

            tracing::warn!(path = %path, method = %method, "CSRF token missing or mismatch");

            config.audit.emit(AuditEvent::new(
                "auth.csrf_reject",
                &request_id,
                &source_ip,
                None,
                serde_json::json!({
                    "method": method.as_str(),
                    "path": path,
                    "reason": reason,
                }),
            ));

            return Err(AppError::forbidden("CSRF token missing or invalid"));
        }
    }

    // ── Run the inner handler ────────────────────────────────────────────
    let mut response = next.run(request).await;

    // ── Safe methods → issue / refresh the CSRF cookie ───────────────────
    if !is_state_changing(&method) {
        let token = uuid::Uuid::new_v4().to_string();
        let cookie = format!("{}={}; Path=/; SameSite=Lax", config.cookie_name, token);
        if let Ok(hv) = HeaderValue::from_str(&cookie) {
            response.headers_mut().append(header::SET_COOKIE, hv);
        }
    }

    Ok(response)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Returns `true` for HTTP methods that mutate server state.
fn is_state_changing(method: &Method) -> bool {
    matches!(
        *method,
        Method::POST | Method::PUT | Method::PATCH | Method::DELETE
    )
}

/// Extract a single cookie value by name from the `Cookie` header.
fn extract_cookie_value(req: &Request, name: &str) -> Option<String> {
    req.headers()
        .get(header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .and_then(|cookies| {
            cookies.split(';').find_map(|pair| {
                let pair = pair.trim();
                let (k, v) = pair.split_once('=')?;
                if k.trim() == name {
                    Some(v.trim().to_owned())
                } else {
                    None
                }
            })
        })
}
