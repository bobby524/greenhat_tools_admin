use axum::extract::{Request, State};
use axum::http::{header, Method};
use axum::middleware::Next;
use axum::response::Response;
use tower_http::request_id::RequestId;

use crate::audit::{AuditEvent, AuditLog};
use crate::error::AppError;

// ---------------------------------------------------------------------------
// Validation config
// ---------------------------------------------------------------------------

/// Knobs for the request-validation middleware layer.
#[derive(Clone)]
pub struct ValidationConfig {
    /// Maximum request body size in bytes (checked via Content-Length header).
    pub max_body_size: usize,
    /// Audit log handle.
    pub audit: AuditLog,
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
// Middleware function
// ---------------------------------------------------------------------------

/// Axum middleware: enforce body-size limit (via Content-Length) and require
/// `Content-Type: application/json` for mutating HTTP methods.
///
/// Emits `tool.invoke_rejected` audit events on validation failures.
pub async fn validate_request(
    State(config): State<ValidationConfig>,
    request: Request,
    next: Next,
) -> Result<Response, AppError> {
    let request_id = extract_request_id(&request);
    let source_ip = extract_source_ip(&request);
    let path = request.uri().path().to_owned();

    // ---- Body size guard (Content-Length) ---------------------------------
    if let Some(content_length) = request
        .headers()
        .get(header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<usize>().ok())
    {
        if content_length > config.max_body_size {
            config.audit.emit(AuditEvent::new(
                "tool.invoke_rejected",
                &request_id,
                &source_ip,
                None,
                serde_json::json!({
                    "reason": "payload_too_large",
                    "content_length": content_length,
                    "max_body_size": config.max_body_size,
                    "path": path,
                }),
            ));
            return Err(AppError::payload_too_large(config.max_body_size));
        }
    }

    // ---- Content-Type guard for POST / PUT / PATCH -----------------------
    if is_mutating(request.method()) {
        let content_type = request
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");

        if !content_type.starts_with("application/json") {
            config.audit.emit(AuditEvent::new(
                "tool.invoke_rejected",
                &request_id,
                &source_ip,
                None,
                serde_json::json!({
                    "reason": "unsupported_media_type",
                    "content_type": content_type,
                    "path": path,
                }),
            ));
            return Err(AppError::unsupported_media_type(
                "Content-Type must be application/json",
            ));
        }
    }

    Ok(next.run(request).await)
}

/// Returns `true` for HTTP methods that carry a request body we care about.
fn is_mutating(method: &Method) -> bool {
    matches!(*method, Method::POST | Method::PUT | Method::PATCH)
}
