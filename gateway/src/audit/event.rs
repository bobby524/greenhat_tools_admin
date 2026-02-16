//! Audit event struct matching `docs/schemas/audit_event.v0.schema.json`.
//!
//! Every audit event is a self-contained JSON object with a fixed envelope
//! plus a type-specific `payload`.  See `docs/AUDIT_EVENTS.md` for the full
//! event type catalog.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Current audit schema version (semver).
pub const SCHEMA_VERSION: &str = "0.1.0";

// ---------------------------------------------------------------------------
// Envelope
// ---------------------------------------------------------------------------

/// Top-level audit event envelope.
///
/// Matches every required field in `audit_event.v0.schema.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEvent {
    /// Globally unique, lexicographically sortable ID (UUIDv4 for now).
    pub event_id: String,

    /// Dotted event type: `<domain>.<action>`.
    pub event_type: String,

    /// RFC 3339 timestamp in UTC.
    pub timestamp: String,

    /// Audit schema semver.
    pub schema_version: String,

    /// Correlation ID from the `x-request-id` header.
    pub request_id: String,

    /// Client IP address (after proxy unwrap).
    pub source_ip: String,

    /// Raw User-Agent header (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_agent: Option<String>,

    /// Authenticated identity, or `null` for unauthenticated requests.
    pub actor: Option<Actor>,

    /// Event-type-specific data.
    pub payload: serde_json::Value,
}

/// Authenticated identity within an audit event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Actor {
    pub user_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub roles: Option<Vec<String>>,
    pub auth_mode: String,
}

// ---------------------------------------------------------------------------
// Builder
// ---------------------------------------------------------------------------

impl AuditEvent {
    /// Create a new audit event with the required envelope fields.
    ///
    /// `payload` is the type-specific data.  Use [`serde_json::json!`] to
    /// build it inline.
    pub fn new(
        event_type: impl Into<String>,
        request_id: impl Into<String>,
        source_ip: impl Into<String>,
        actor: Option<Actor>,
        payload: serde_json::Value,
    ) -> Self {
        Self {
            event_id: Uuid::new_v4().to_string(),
            event_type: event_type.into(),
            timestamp: now_utc_rfc3339(),
            schema_version: SCHEMA_VERSION.to_owned(),
            request_id: request_id.into(),
            source_ip: source_ip.into(),
            user_agent: None,
            actor,
            payload,
        }
    }

    /// Attach a User-Agent string.
    pub fn with_user_agent(mut self, ua: impl Into<String>) -> Self {
        self.user_agent = Some(ua.into());
        self
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Returns the current UTC time as an RFC 3339 string with millisecond
/// precision, e.g. `"2026-02-14T21:32:00.123Z"`.
fn now_utc_rfc3339() -> String {
    // Use `std::time` + manual formatting to avoid pulling in `chrono`.
    use std::time::SystemTime;
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .expect("system clock before epoch");
    let secs = now.as_secs();
    let millis = now.subsec_millis();

    // Break into date/time components (simplified — no leap-second handling).
    let days = secs / 86400;
    let day_secs = secs % 86400;
    let h = day_secs / 3600;
    let m = (day_secs % 3600) / 60;
    let s = day_secs % 60;

    // Civil date from days since 1970-01-01 (algorithm from Howard Hinnant).
    let (y, mo, d) = civil_from_days(days as i64);

    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{m:02}:{s:02}.{millis:03}Z")
}

/// Convert days since 1970-01-01 to (year, month 1-12, day 1-31).
/// Algorithm by Howard Hinnant.
fn civil_from_days(mut z: i64) -> (i64, u32, u32) {
    z += 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mo = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if mo <= 2 { y + 1 } else { y };
    (y, mo, d)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn event_serializes_to_expected_shape() {
        let evt = AuditEvent::new(
            "auth.login_success",
            "req-123",
            "203.0.113.42",
            Some(Actor {
                user_id: "usr_abc".into(),
                roles: Some(vec!["admin".into()]),
                auth_mode: "bearer_jwt".into(),
            }),
            json!({ "auth_mode": "bearer_jwt", "claims_sub": "usr_abc" }),
        );

        let json_str = serde_json::to_string(&evt).unwrap();
        let v: serde_json::Value = serde_json::from_str(&json_str).unwrap();

        assert_eq!(v["event_type"], "auth.login_success");
        assert_eq!(v["request_id"], "req-123");
        assert_eq!(v["source_ip"], "203.0.113.42");
        assert_eq!(v["schema_version"], SCHEMA_VERSION);
        assert_eq!(v["actor"]["user_id"], "usr_abc");
        assert_eq!(v["actor"]["auth_mode"], "bearer_jwt");
        assert_eq!(v["payload"]["claims_sub"], "usr_abc");
        assert!(v["event_id"].as_str().unwrap().len() > 10);
    }

    #[test]
    fn event_without_actor_serializes_actor_as_null() {
        let evt = AuditEvent::new("auth.csrf_reject", "req-456", "10.0.0.1", None, json!({}));
        let v: serde_json::Value = serde_json::to_value(&evt).unwrap();
        assert!(v["actor"].is_null());
    }

    #[test]
    fn timestamp_is_rfc3339_utc() {
        let ts = now_utc_rfc3339();
        assert!(ts.ends_with('Z'), "timestamp must end with Z: {ts}");
        assert!(ts.contains('T'), "timestamp must contain T: {ts}");
        // Basic length check: "2026-02-14T21:32:00.123Z" = 24 chars
        assert_eq!(ts.len(), 24, "unexpected timestamp length: {ts}");
    }

    #[test]
    fn civil_from_days_epoch() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
    }

    #[test]
    fn civil_from_days_known_date() {
        // 2026-02-14 is 20498 days after epoch
        assert_eq!(civil_from_days(20498), (2026, 2, 14));
    }

    #[test]
    fn with_user_agent_sets_field() {
        let evt = AuditEvent::new("auth.login_success", "r", "ip", None, json!({}))
            .with_user_agent("Mozilla/5.0");
        assert_eq!(evt.user_agent.as_deref(), Some("Mozilla/5.0"));
    }
}
