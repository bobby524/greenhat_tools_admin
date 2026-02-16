//! Structured API error type with `request_id` propagation.
//!
//! Every error response is returned as:
//!
//! ```json
//! {
//!   "error": {
//!     "code": 404,
//!     "kind": "not_found",
//!     "message": "resource not found",
//!     "request_id": "xxxxxxxx-xxxx-…"
//!   }
//! }
//! ```

use axum::{
    http::StatusCode,
    response::{IntoResponse, Json, Response},
};
use serde::Serialize;

// ---------------------------------------------------------------------------
// Public error type
// ---------------------------------------------------------------------------

/// Structured API error.
///
/// Construct with the convenience methods ([`AppError::bad_request`],
/// [`AppError::unauthorized`], etc.) and optionally chain
/// [`.with_request_id()`](AppError::with_request_id).
#[derive(Debug)]
pub struct AppError {
    pub kind: ErrorKind,
    pub request_id: Option<String>,
}

/// The category of the error — maps 1:1 to an HTTP status code.
#[derive(Debug)]
pub enum ErrorKind {
    /// 400
    BadRequest(String),
    /// 401
    Unauthorized(String),
    /// 403
    Forbidden(String),
    /// 404
    NotFound(String),
    /// 413
    PayloadTooLarge(usize),
    /// 415
    UnsupportedMediaType(String),
    /// 422
    UnprocessableEntity(String),
    /// 429
    RateLimited,
    /// 503
    ServiceUnavailable(String),
    /// 500
    Internal(String),
}

// ---------------------------------------------------------------------------
// Constructors & builder
// ---------------------------------------------------------------------------

impl AppError {
    pub fn bad_request(msg: impl Into<String>) -> Self {
        Self {
            kind: ErrorKind::BadRequest(msg.into()),
            request_id: None,
        }
    }

    pub fn unauthorized(msg: impl Into<String>) -> Self {
        Self {
            kind: ErrorKind::Unauthorized(msg.into()),
            request_id: None,
        }
    }

    pub fn forbidden(msg: impl Into<String>) -> Self {
        Self {
            kind: ErrorKind::Forbidden(msg.into()),
            request_id: None,
        }
    }

    pub fn not_found(msg: impl Into<String>) -> Self {
        Self {
            kind: ErrorKind::NotFound(msg.into()),
            request_id: None,
        }
    }

    pub fn payload_too_large(max_bytes: usize) -> Self {
        Self {
            kind: ErrorKind::PayloadTooLarge(max_bytes),
            request_id: None,
        }
    }

    pub fn unsupported_media_type(msg: impl Into<String>) -> Self {
        Self {
            kind: ErrorKind::UnsupportedMediaType(msg.into()),
            request_id: None,
        }
    }

    pub fn unprocessable(msg: impl Into<String>) -> Self {
        Self {
            kind: ErrorKind::UnprocessableEntity(msg.into()),
            request_id: None,
        }
    }

    pub fn rate_limited() -> Self {
        Self {
            kind: ErrorKind::RateLimited,
            request_id: None,
        }
    }

    pub fn service_unavailable(msg: impl Into<String>) -> Self {
        Self {
            kind: ErrorKind::ServiceUnavailable(msg.into()),
            request_id: None,
        }
    }

    pub fn internal(msg: impl Into<String>) -> Self {
        Self {
            kind: ErrorKind::Internal(msg.into()),
            request_id: None,
        }
    }

    /// Attach a `request_id` that will appear in the JSON error body.
    pub fn with_request_id(mut self, id: impl Into<String>) -> Self {
        self.request_id = Some(id.into());
        self
    }
}

// ---------------------------------------------------------------------------
// Wire format
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct ErrorBody {
    error: ErrorDetail,
}

#[derive(Serialize)]
struct ErrorDetail {
    code: u16,
    kind: &'static str,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    request_id: Option<String>,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, kind_str, message) = match &self.kind {
            ErrorKind::BadRequest(msg) => (StatusCode::BAD_REQUEST, "bad_request", msg.clone()),
            ErrorKind::Unauthorized(msg) => (StatusCode::UNAUTHORIZED, "unauthorized", msg.clone()),
            ErrorKind::Forbidden(msg) => (StatusCode::FORBIDDEN, "forbidden", msg.clone()),
            ErrorKind::NotFound(msg) => (StatusCode::NOT_FOUND, "not_found", msg.clone()),
            ErrorKind::PayloadTooLarge(max) => (
                StatusCode::PAYLOAD_TOO_LARGE,
                "payload_too_large",
                format!("request body exceeds maximum size of {max} bytes"),
            ),
            ErrorKind::UnsupportedMediaType(msg) => (
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                "unsupported_media_type",
                msg.clone(),
            ),
            ErrorKind::UnprocessableEntity(msg) => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "unprocessable_entity",
                msg.clone(),
            ),
            ErrorKind::RateLimited => (
                StatusCode::TOO_MANY_REQUESTS,
                "rate_limited",
                "rate limit exceeded \u{2014} try again later".into(),
            ),
            ErrorKind::ServiceUnavailable(msg) => (
                StatusCode::SERVICE_UNAVAILABLE,
                "service_unavailable",
                msg.clone(),
            ),
            ErrorKind::Internal(msg) => {
                tracing::error!(error = %msg, "internal server error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal",
                    "internal error".into(),
                )
            }
        };

        let body = ErrorBody {
            error: ErrorDetail {
                code: status.as_u16(),
                kind: kind_str,
                message,
                request_id: self.request_id,
            },
        };

        (status, Json(body)).into_response()
    }
}

// ---------------------------------------------------------------------------
// Blanket conversion: any `std::error::Error` ↦ AppError::Internal
// ---------------------------------------------------------------------------

impl<E> From<E> for AppError
where
    E: std::error::Error + Send + Sync + 'static,
{
    fn from(err: E) -> Self {
        AppError::internal(err.to_string())
    }
}
