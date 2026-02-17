use axum::extract::{Request, State};
use axum::http::{header, HeaderMap, Method};
use axum::middleware::Next;
use axum::response::Response;
use tower_http::request_id::RequestId;

use crate::audit::{actor_from_principal, AuditEvent};
use crate::auth::{AuthError, AuthState, Principal, SessionCredential};
use crate::error::AppError;
use crate::observability::RequestActor;

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

            let request_actor = RequestActor {
                user_id: principal.user_id.clone(),
                roles: principal.roles.clone(),
            };
            if let Ok(user_id) = axum::http::HeaderValue::from_str(&request_actor.user_id) {
                req.headers_mut().insert("x-auth-user-id", user_id);
            }
            if !request_actor.roles.is_empty() {
                let joined = request_actor.roles.join(",");
                if let Ok(roles) = axum::http::HeaderValue::from_str(&joined) {
                    req.headers_mut().insert("x-auth-roles", roles);
                }
            }

            req.extensions_mut().insert::<Principal>(principal);
            let mut response = next.run(req).await;
            response.extensions_mut().insert(request_actor);
            Ok(response)
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
    // IMPORTANT: preserve the full cookie header; Better Auth may rely on
    // companion cookies in addition to the session token cookie itself.
    let cookie_header = headers.get(header::COOKIE)?.to_str().ok()?;

    // 1) Honor configured cookie name first.
    if has_cookie(cookie_header, cookie_name) {
        return Some(SessionCredential::Cookie(cookie_header.to_owned()));
    }

    // 2) Compatibility fallbacks for historical BetterAuth cookie names.
    for alias in [
        "__Secure-greenhat_tools.session_token",
        "greenhat_tools.session_token",
        "better-auth.session_token",
    ] {
        if alias != cookie_name && has_cookie(cookie_header, alias) {
            return Some(SessionCredential::Cookie(cookie_header.to_owned()));
        }
    }

    None
}

fn has_cookie(cookie_header: &str, name: &str) -> bool {
    cookie_header.split(';').any(|pair| {
        let p = pair.trim();
        let Some((k, _)) = p.split_once('=') else {
            return false;
        };
        k.trim() == name
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn has_cookie_detects_cookie_name() {
        let h = "a=1; better-auth.session_token=abc123; b=2";
        assert!(has_cookie(h, "better-auth.session_token"));
        assert!(!has_cookie(h, "missing"));
    }

    #[test]
    fn extract_credential_supports_cookie_aliases() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::COOKIE,
            "a=1; __Secure-greenhat_tools.session_token=tok123; b=2"
                .parse()
                .expect("valid cookie header"),
        );

        let cred = extract_credential(&headers, "better-auth.session_token");
        match cred {
            Some(SessionCredential::Cookie(h)) => {
                assert!(h.contains("__Secure-greenhat_tools.session_token=tok123"))
            }
            _ => panic!("expected cookie credential"),
        }
    }
}
