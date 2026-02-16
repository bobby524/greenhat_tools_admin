//! Deny-by-default RBAC middleware.
//!
//! Runs **after** the authentication layer (which inserts a [`Principal`]
//! into request extensions).  Evaluates the loaded policy against the
//! principal's roles and the requested action.
//!
//! For tool-call routes (`POST /v1/tools/call`), the middleware buffers the
//! request body, extracts the tool name, checks tool-level RBAC, then
//! reconstructs the body so the downstream handler receives it intact.

use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::response::Response;
use http_body_util::BodyExt;
use tower_http::request_id::RequestId;

use crate::audit::{actor_from_principal, AuditEvent, AuditLog};
use crate::auth::Principal;
use crate::error::AppError;
use crate::rbac::engine::PolicyEngine;
use crate::rbac::types::{Action, Decision};

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

/// Shared state for the RBAC middleware layer.
#[derive(Clone)]
pub struct RbacState {
    /// The policy engine shared across all requests.
    pub engine: Arc<PolicyEngine>,
    /// Paths that bypass RBAC entirely (infrastructure endpoints).
    pub exempt_paths: Vec<String>,
    /// Audit log handle.
    pub audit: AuditLog,
}

impl RbacState {
    pub fn new(engine: Arc<PolicyEngine>, audit: AuditLog) -> Self {
        Self {
            engine,
            exempt_paths: vec!["/health".into(), "/version".into(), "/metrics".into()],
            audit,
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

/// Axum middleware: deny-by-default RBAC enforcement.
///
/// # Errors
///
/// Returns a 403 `AppError` (with `request_id`) if the principal lacks
/// the required role or permission.  Returns 403 if no [`Principal`] is
/// present in request extensions (authentication required).
///
/// Emits `authz.denied` audit events on rejection.
pub async fn rbac_middleware(
    State(state): State<RbacState>,
    req: Request,
    next: Next,
) -> Result<Response, AppError> {
    let path = req.uri().path().to_owned();
    let method = req.method().to_string();

    // ── Skip infrastructure endpoints ────────────────────────────────────
    if state.exempt_paths.iter().any(|p| path == *p) {
        return Ok(next.run(req).await);
    }

    // ── Extract identifiers for structured errors / audit ────────────────
    let request_id = extract_request_id(&req);
    let source_ip = extract_source_ip(&req);

    let deny =
        |msg: String| -> AppError { AppError::forbidden(msg).with_request_id(request_id.clone()) };

    // ── Require authenticated principal ──────────────────────────────────
    let principal = req
        .extensions()
        .get::<Principal>()
        .cloned()
        .ok_or_else(|| {
            state.audit.emit(AuditEvent::new(
                "authz.denied",
                &request_id,
                &source_ip,
                None,
                serde_json::json!({
                    "reason": "no_principal",
                    "method": method,
                    "path": path,
                }),
            ));
            deny("authentication required for this resource".into())
        })?;

    let actor = actor_from_principal(&principal);

    // ── Route-level permission check ─────────────────────────────────────
    let route_action = Action::RouteAccess {
        method: method.clone(),
        path: path.clone(),
    };
    if let Decision::Deny(reason) = state.engine.evaluate(&principal, &route_action) {
        tracing::warn!(
            user_id = %principal.user_id,
            %method,
            %path,
            %reason,
            "RBAC denied route access"
        );
        state.audit.emit(AuditEvent::new(
            "authz.denied",
            &request_id,
            &source_ip,
            Some(actor),
            serde_json::json!({
                "reason": reason,
                "action": "route_access",
                "method": method,
                "path": path,
            }),
        ));
        return Err(deny(reason));
    }

    // ── Tool-level permission check (body inspection) ────────────────────
    if path.starts_with("/v1/tools/call") && method == "POST" {
        let (parts, body) = req.into_parts();

        let bytes = body
            .collect()
            .await
            .map_err(|_| {
                AppError::internal("failed to read request body for RBAC check")
                    .with_request_id(request_id.clone())
            })?
            .to_bytes();

        // Best-effort tool name extraction — if parsing fails, route-level
        // check already passed; the handler will reject malformed bodies.
        if let Ok(parsed) = serde_json::from_slice::<serde_json::Value>(&bytes) {
            if let Some(tool_name) = parsed.get("tool").and_then(|v| v.as_str()) {
                let tool_action = Action::ToolCall {
                    tool: tool_name.to_string(),
                };
                if let Decision::Deny(reason) = state.engine.evaluate(&principal, &tool_action) {
                    tracing::warn!(
                        user_id = %principal.user_id,
                        tool = %tool_name,
                        %reason,
                        "RBAC denied tool call"
                    );
                    state.audit.emit(AuditEvent::new(
                        "authz.denied",
                        &request_id,
                        &source_ip,
                        Some(actor),
                        serde_json::json!({
                            "reason": reason,
                            "action": "tool_call",
                            "tool": tool_name,
                        }),
                    ));
                    return Err(deny(reason));
                }
            }
        }

        // Reconstruct the request with the buffered body.
        let req = Request::from_parts(parts, Body::from(bytes));
        return Ok(next.run(req).await);
    }

    Ok(next.run(req).await)
}
