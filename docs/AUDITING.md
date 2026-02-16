# Auditing

> **Schema version:** `0.1.0`
> **Schema reference:** [`schemas/audit_event.v0.schema.json`](schemas/audit_event.v0.schema.json)
> **Event catalog:** [`AUDIT_EVENTS.md`](AUDIT_EVENTS.md)

---

## Overview

The gateway emits **structured, append-only audit events** as JSONL (one JSON
object per line) to every configured **sink**.  Events cover the full request
lifecycle:

| Domain | Events |
|--------|--------|
| **auth** | `auth.login_success`, `auth.login_failure`, `auth.csrf_reject` |
| **authz** | `authz.allowed`, `authz.denied` *(wired when RBAC engine lands)* |
| **tool** | `tool.invoke_start`, `tool.invoke_success`, `tool.invoke_failure`, `tool.invoke_rejected` |
| **policy** | `policy.rate_limit_hit` |
| **gateway** | `gateway.egress_blocked` |

Every event carries the full **request envelope** (event ID, request ID,
timestamp, source IP, actor, payload) as defined in `audit_event.v0.schema.json`.

---

## Configuration

| Env var | Default | Description |
|---------|---------|-------------|
| `AUDIT_LOG_FILE` | *(empty — disabled)* | Path to append-only JSONL audit log file |

### Sinks

| Sink | Always active? | Description |
|------|:--------------:|-------------|
| **Stdout JSONL** | ✅ | One JSON object per line to stdout |
| **File JSONL** | When `AUDIT_LOG_FILE` is set | Append-only file; rotation is external (logrotate) |

Both sinks are behind the `AuditSink` trait — adding Postgres, S3, or a
structured-log pipeline (Vector → ClickHouse) is a matter of implementing the
trait and registering it in `build_sink_from_env()`.

---

## Architecture

```
                  ┌─────────────┐
                  │  Middleware  │
                  │  (auth, csrf│
                  │  rate_limit,│
                  │  validate)  │
                  └──────┬──────┘
                         │
              AuditEvent::new(…)
                         │
                  ┌──────▼──────┐
                  │  AuditLog   │ ← cheaply cloneable (Arc)
                  │  .emit(evt) │
                  └──────┬──────┘
                         │
            ┌────────────┼────────────┐
            │            │            │
    ┌───────▼──┐  ┌──────▼───┐  ┌────▼─────┐
    │ StdoutSink│  │ FileSink │  │ Future   │
    │ (always) │  │ (env var)│  │ sinks…   │
    └──────────┘  └──────────┘  └──────────┘
```

### Key types

| Type | Location | Purpose |
|------|----------|---------|
| `AuditEvent` | `audit/event.rs` | The event struct — matches the JSON schema |
| `Actor` | `audit/event.rs` | Authenticated identity within an event |
| `AuditLog` | `audit/mod.rs` | Central handle; inject via axum State/Extension |
| `AuditSink` (trait) | `audit/sink.rs` | Where events go |
| `StdoutSink` | `audit/sink.rs` | Writes to stdout |
| `FileSink` | `audit/sink.rs` | Writes to a file |
| `CompositeSink` | `audit/sink.rs` | Fans out to multiple sinks |

### Secret redaction

| Helper | Location | Purpose |
|--------|----------|---------|
| `hash_args()` | `audit/redact.rs` | Hash tool call arguments (never stored raw) |
| `hash_credential()` | `audit/redact.rs` | Hash tokens/credentials for correlation |
| `redact_string()` | `audit/redact.rs` | Partial redaction (keep start/end only) |

Per `docs/SECRETS.md` — **no raw secrets, credentials, or PII appear in any
audit event**.  Tool arguments are hashed (`args_hash`); credentials are never
logged; user IDs are opaque identifiers only.

---

## Emission Points

### Auth middleware (`middleware/auth.rs`)

| When | Event type |
|------|-----------|
| Session validated successfully | `auth.login_success` |
| Missing credential | `auth.login_failure` |
| Invalid/expired session | `auth.login_failure` |
| Upstream auth service error | `auth.login_failure` |

### CSRF middleware (`middleware/csrf.rs`)

| When | Event type |
|------|-----------|
| CSRF cookie missing | `auth.csrf_reject` |
| CSRF header missing | `auth.csrf_reject` |
| Cookie/header mismatch | `auth.csrf_reject` |

### Rate limiter (`middleware/rate_limit.rs`)

| When | Event type |
|------|-----------|
| Per-IP rate limit exceeded | `policy.rate_limit_hit` |

### Validation middleware (`middleware/validate.rs`)

| When | Event type |
|------|-----------|
| Content-Length exceeds max | `tool.invoke_rejected` |
| Missing/wrong Content-Type | `tool.invoke_rejected` |

### Tool router (`tool_router.rs`)

| When | Event type |
|------|-----------|
| Tool dispatch accepted | `tool.invoke_start` |
| Tool completed OK | `tool.invoke_success` |
| Tool failed (timeout, error) | `tool.invoke_failure` |
| Unknown tool / bad params | `tool.invoke_rejected` |
| Egress firewall blocked | `gateway.egress_blocked` |

---

## Request ID Propagation

Every audit event includes the `request_id` from the `x-request-id` header
(set by the `SetRequestIdLayer` at the outermost middleware layer).  This
allows correlating all events for a single HTTP request across auth, CSRF,
rate-limit, validation, tool dispatch, and egress layers.

---

## Adding a New Event Type

1. Define the event type string in `docs/AUDIT_EVENTS.md` under the
   appropriate namespace.
2. At the emission point, build an `AuditEvent::new(…)` with the right
   `event_type` and `payload`.
3. Call `audit.emit(event)`.
4. Add a test that verifies the event is emitted with the correct type and
   payload shape.
5. If the payload contains anything that might be sensitive, use
   `hash_args()`, `hash_credential()`, or `redact_string()` before emitting.

---

## Example Event (JSONL)

```json
{
  "event_id": "b9256972-5347-48d1-a06f-f0b7714019cf",
  "event_type": "auth.csrf_reject",
  "timestamp": "2026-02-15T06:30:02.094Z",
  "schema_version": "0.1.0",
  "request_id": "9c4c2f6d-8bfd-4e63-b7ca-543729e8a28d",
  "source_ip": "203.0.113.42",
  "actor": null,
  "payload": {
    "method": "POST",
    "path": "/mcp/invoke",
    "reason": "missing_csrf_header"
  }
}
```

---

## Testing

```bash
# Run all audit-related unit tests
cargo test -p gateway audit

# Run all gateway tests (including integration)
cargo test -p gateway
```

The audit module includes a `CaptureSink` test helper that records events
in memory for assertion.  Use it in tests:

```rust
use gateway::audit::sink::tests::CaptureSink;
use gateway::audit::AuditLog;

let capture = std::sync::Arc::new(CaptureSink::default());
let audit = AuditLog::new(capture.clone());
// … do something that should emit …
let events = capture.events.lock().unwrap();
assert_eq!(events.len(), 1);
```
