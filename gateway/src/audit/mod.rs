//! Audit event pipeline — structured, append-only audit logging.
//!
//! # Architecture
//!
//! ```text
//! middleware / tool_router
//!       │
//!       ▼
//!   AuditEvent::new(…)     ← build the event
//!       │
//!       ▼
//!   AuditLog::emit(event)  ← fan-out to sinks
//!       │
//!       ├─▶ StdoutSink  (always)
//!       └─▶ FileSink    (when AUDIT_LOG_FILE is set)
//! ```
//!
//! # Usage from middleware
//!
//! ```rust,ignore
//! use crate::audit::{AuditLog, AuditEvent, Actor};
//!
//! let audit = AuditLog::from_env();
//! audit.emit(AuditEvent::new(
//!     "auth.login_success",
//!     &request_id,
//!     &source_ip,
//!     Some(actor),
//!     serde_json::json!({ "auth_mode": "bearer_jwt" }),
//! ));
//! ```

pub mod event;
pub mod redact;
pub mod sink;

pub use event::{Actor, AuditEvent, SCHEMA_VERSION};
pub use redact::{hash_args, hash_credential, redact_string};
pub use sink::{build_sink_from_env, AuditSink};

use std::sync::Arc;

// ---------------------------------------------------------------------------
// AuditLog — the handle that middleware / handlers hold
// ---------------------------------------------------------------------------

/// Central audit log handle.  Cheaply cloneable (inner `Arc`).
///
/// Inject this into middleware via axum `State` or `Extension`.
#[derive(Clone)]
pub struct AuditLog {
    sink: Arc<dyn AuditSink>,
}

impl std::fmt::Debug for AuditLog {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuditLog").finish_non_exhaustive()
    }
}

impl AuditLog {
    /// Create an `AuditLog` with the given sink.
    pub fn new(sink: Arc<dyn AuditSink>) -> Self {
        Self { sink }
    }

    /// Build from environment variables (stdout + optional file).
    pub fn from_env() -> Self {
        Self {
            sink: build_sink_from_env(),
        }
    }

    /// Emit an audit event to all configured sinks.
    pub fn emit(&self, event: AuditEvent) {
        self.sink.emit(&event);
    }
}

// Allow `AuditLog` to be used as an `AuditSink` directly.
impl AuditSink for AuditLog {
    fn emit(&self, event: &AuditEvent) {
        self.sink.emit(event);
    }
}

// ---------------------------------------------------------------------------
// Helper: extract Actor from a Principal (auth module)
// ---------------------------------------------------------------------------

use crate::auth::principal::{AuthMethod, Principal};

/// Convert a [`Principal`] (from the auth middleware) into an audit [`Actor`].
pub fn actor_from_principal(p: &Principal) -> Actor {
    Actor {
        user_id: p.user_id.clone(),
        roles: if p.roles.is_empty() {
            None
        } else {
            Some(p.roles.clone())
        },
        auth_mode: match p.auth_method {
            AuthMethod::Cookie => "session_cookie".into(),
            AuthMethod::Bearer => "bearer_jwt".into(),
        },
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audit::sink::tests::CaptureSink;
    use serde_json::json;

    fn test_log() -> (AuditLog, Arc<sink::tests::CaptureSink>) {
        let capture = Arc::new(CaptureSink::default());
        let log = AuditLog::new(capture.clone());
        (log, capture)
    }

    #[test]
    fn emit_sends_to_sink() {
        let (log, capture) = test_log();
        log.emit(AuditEvent::new(
            "test.emit",
            "r-1",
            "127.0.0.1",
            None,
            json!({}),
        ));
        assert_eq!(capture.events.lock().unwrap().len(), 1);
    }

    #[test]
    fn actor_from_principal_maps_correctly() {
        let p = Principal {
            user_id: "usr_1".into(),
            email: None,
            org_id: None,
            roles: vec!["admin".into(), "viewer".into()],
            session_id: "sess_1".into(),
            auth_method: AuthMethod::Bearer,
        };
        let actor = actor_from_principal(&p);
        assert_eq!(actor.user_id, "usr_1");
        assert_eq!(actor.auth_mode, "bearer_jwt");
        assert_eq!(actor.roles, Some(vec!["admin".into(), "viewer".into()]));
    }

    #[test]
    fn actor_from_principal_empty_roles() {
        let p = Principal {
            user_id: "usr_2".into(),
            email: None,
            org_id: None,
            roles: vec![],
            session_id: "sess_2".into(),
            auth_method: AuthMethod::Cookie,
        };
        let actor = actor_from_principal(&p);
        assert!(actor.roles.is_none());
        assert_eq!(actor.auth_mode, "session_cookie");
    }
}
