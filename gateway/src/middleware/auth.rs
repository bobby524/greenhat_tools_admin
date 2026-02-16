use axum::extract::{Request, State};
use axum::http::{header, HeaderMap, Method};
use axum::middleware::Next;
use axum::response::Response;
use tower_http::request_id::RequestId;

use crate::audit::{actor_from_principal, AuditEvent};
use crate::auth::{AuthError, AuthState, Principal, SessionCredential};
use crate::error::AppError;

// ---------------------------------------------------------------------------
// Helpers: extract request context for audit events
// ---------------------------------------------------------------------------

fn extract_request_id(req: &Request) -> String {
    req.extensions()
        .get::<RequestId>()
        .and_then(|id| id.header_value().to_str().ok())
        .unwrap_or("unknown")
        .to_owned()
}

fn extract_source_ip(req: &Request) -> String {
    // X-Forwarded-For → X-Real-IP → ConnectInfo → "unknown"
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
    if let Some(axum::extract::ConnectInfo(addr)) = req
        .extensions()
        .get::<axum::extract::ConnectInfo<std::net::SocketAddr>>()
    {
        return addr.ip().to_string();
    }
    "unknown".to_owned()
}

fn extract_user_agent(req: &Request) -> Option<String> {
    req.headers()
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_owned())
}

// ---------------------------------------------------------------------------
// Middleware
// ---------------------------------------------------------------------------

/// Authentication middleware.
///
/// - Extracts a session credential (BetterAuth cookie or Authorization bearer token)
/// - Validates it via the configured [`SessionValidator`]
/// - Inserts the resulting [`Principal`] into request extensions
/// - Emits `auth.login_success` / `auth.login_failure` audit events
pub async fn auth_middleware(
    State(state): State<AuthState>,
    mut req: Request,
    next: Next,
) -> Result<Response, AppError> {
    let path = req.uri().path().to_owned();

    // Always allow CORS preflight requests through.
    if req.method() == Method::OPTIONS {
        return Ok(next.run(req).await);
    }

    // Exempt infrastructure probes and unauthenticated endpoints.
    if state.exempt_paths.iter().any(|p| path == *p) {
        return Ok(next.run(req).await);
    }

    let request_id = extract_request_id(&req);
    let source_ip = extract_source_ip(&req);
    let user_agent = extract_user_agent(&req);

    let credential = extract_credential(req.headers(), &state.cookie_name);

    let credential = match credential {
        Some(c) => c,
        None => {
            // No credential at all → auth failure
            if let Some(ref audit) = state.audit {
                let mut evt = AuditEvent::new(
                    "auth.login_failure",
                    &request_id,
                    &source_ip,
                    None,
                    serde_json::json!({
                        "reason": "missing_credential",
                        "path": path,
                    }),
                );
                if let Some(ua) = user_agent {
                    evt = evt.with_user_agent(ua);
                }
                audit.emit(evt);
            }
            return Err(AppError::unauthorized("missing session credential"));
        }
    };

    let auth_mode_str = match &credential {
        SessionCredential::Cookie(_) => "session_cookie",
        SessionCredential::Bearer(_) => "bearer_jwt",
    };

    // Fail-closed if the configured validator doesn't support the presented credential type.
    match &credential {
        SessionCredential::Cookie(_) if !state.validator.supports_cookie() => {
            return Err(AppError::unauthorized("cookie auth not supported"));
        }
        SessionCredential::Bearer(_) if !state.validator.supports_bearer() => {
            return Err(AppError::unauthorized("bearer auth not supported"));
        }
        _ => {}
    }

    match state.validator.validate_session(&credential).await {
        Ok(principal) => {
            // Emit auth.login_success
            if let Some(ref audit) = state.audit {
                let actor = actor_from_principal(&principal);
                let mut evt = AuditEvent::new(
                    "auth.login_success",
                    &request_id,
                    &source_ip,
                    Some(actor),
                    serde_json::json!({
                        "auth_mode": auth_mode_str,
                        "claims_sub": &principal.user_id,
                    }),
                );
                if let Some(ua) = &user_agent {
                    evt = evt.with_user_agent(ua);
                }
                audit.emit(evt);
            }

            req.extensions_mut().insert::<Principal>(principal);
            Ok(next.run(req).await)
        }
        Err(e) => {
            let reason = match &e {
                AuthError::InvalidSession(msg) => msg.clone(),
                AuthError::Upstream(msg) => format!("upstream_error: {msg}"),
            };

            if let Some(ref audit) = state.audit {
                let mut evt = AuditEvent::new(
                    "auth.login_failure",
                    &request_id,
                    &source_ip,
                    None,
                    serde_json::json!({
                        "auth_mode": auth_mode_str,
                        "reason": reason,
                        "path": path,
                    }),
                );
                if let Some(ua) = user_agent {
                    evt = evt.with_user_agent(ua);
                }
                audit.emit(evt);
            }

            Err(map_auth_error(e))
        }
    }
}

fn map_auth_error(err: AuthError) -> AppError {
    match err {
        AuthError::InvalidSession(_) => AppError::unauthorized("invalid or expired session"),
        AuthError::Upstream(msg) => {
            AppError::service_unavailable(format!("auth upstream error: {msg}"))
        }
    }
}

fn extract_credential(headers: &HeaderMap, cookie_name: &str) -> Option<SessionCredential> {
    // Prefer bearer token for programmatic clients.
    if let Some(authz) = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
    {
        let a = authz.trim();
        if let Some(token) = a.strip_prefix("Bearer ") {
            let token = token.trim();
            if !token.is_empty() {
                return Some(SessionCredential::Bearer(token.to_owned()));
            }
        }
    }

    // Cookie-based (browser) session.
    let cookie_header = headers.get(header::COOKIE)?.to_str().ok()?;
    parse_cookie(cookie_header, cookie_name).map(SessionCredential::Cookie)
}

fn parse_cookie(cookie_header: &str, name: &str) -> Option<String> {
    cookie_header.split(';').find_map(|pair| {
        let p = pair.trim();
        let (k, v) = p.split_once('=')?;
        if k.trim() == name {
            Some(v.trim().to_owned())
        } else {
            None
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cookie_parser_extracts_value() {
        let h = "a=1; better-auth.session_token=abc123; b=2";
        assert_eq!(
            parse_cookie(h, "better-auth.session_token"),
            Some("abc123".into())
        );
    }
}
