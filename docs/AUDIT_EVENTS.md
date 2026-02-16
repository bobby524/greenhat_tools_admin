# Audit Event Schema

> **Schema version:** `0.1.0`
> **Status:** Draft — minimal but extensible
> **Machine-readable:** [`schemas/audit_event.v0.schema.json`](schemas/audit_event.v0.schema.json)

---

## Design Principles

1. **Append-only.** Events are immutable once written. No updates, no deletes.
2. **Structured.** Every event is a self-contained JSON object with a fixed envelope + type-specific payload.
3. **Correlatable.** `request_id` threads through the entire request lifecycle.
4. **Deny-by-default auditable.** Every auth/authz/tool decision emits an event — especially denials.
5. **No secrets in events.** Credentials, tokens, and PII are never logged. Sanitize before emit.

---

## Event Envelope

Every audit event shares this envelope:

```jsonc
{
  // ── Envelope (required on every event) ─────────────────────
  "event_id":    "evt_01HXYZ...",        // unique, lexicographically sortable (ULIDv2 or UUIDv7)
  "event_type":  "auth.login_success",   // dotted type string (see catalog below)
  "timestamp":   "2026-02-14T21:32:00.000Z",  // RFC 3339, UTC always
  "schema_version": "0.1.0",

  // ── Request context ────────────────────────────────────────
  "request_id":  "req_abcdef12-3456-...",   // from x-request-id header
  "source_ip":   "203.0.113.42",
  "user_agent":  "Mozilla/5.0 ...",

  // ── Identity (null if unauthenticated) ─────────────────────
  "actor": {
    "user_id":   "usr_abc123",
    "roles":     ["analyst"],
    "auth_mode": "bearer_jwt"            // "session_cookie" | "bearer_jwt" | null
  },

  // ── Type-specific payload ──────────────────────────────────
  "payload": { ... }
}
```

### Envelope field reference

| Field | Type | Required | Description |
|---|---|---|---|
| `event_id` | `string` | yes | Globally unique, sortable ID |
| `event_type` | `string` | yes | Dotted event type from the catalog |
| `timestamp` | `string` (RFC 3339) | yes | Event time in UTC |
| `schema_version` | `string` | yes | Audit schema version |
| `request_id` | `string` | yes | Correlation ID from the gateway |
| `source_ip` | `string` | yes | Client IP (after proxy unwrap) |
| `user_agent` | `string` | no | Raw User-Agent header |
| `actor` | `object \| null` | yes | Authenticated identity or `null` |
| `payload` | `object` | yes | Event-type-specific data |

---

## Event Type Catalog

### Namespace convention

```
<domain>.<action>
```

Domains: `auth`, `authz`, `tool`, `policy`, `gateway`.

---

### `auth.*` — Authentication Events

#### `auth.login_success`
Caller successfully authenticated.

```jsonc
{ "auth_mode": "bearer_jwt", "claims_sub": "usr_abc123" }
```

#### `auth.login_failure`
Authentication attempt rejected.

```jsonc
{ "auth_mode": "session_cookie", "reason": "expired_session" }
```

#### `auth.csrf_reject`
CSRF validation failed on a mutating request.

```jsonc
{ "method": "POST", "path": "/mcp/invoke", "reason": "missing_csrf_header" }
```

---

### `authz.*` — Authorization / RBAC Events

#### `authz.allowed`
Access check passed.

```jsonc
{ "permission": "tools:invoke", "resource": "web_search" }
```

#### `authz.denied`
Access check failed (403).

```jsonc
{
  "permission": "tools:invoke",
  "resource": "db_query",
  "reason": "role_not_permitted",
  "required_roles": ["admin"],
  "actor_roles": ["viewer"]
}
```

---

### `tool.*` — MCP Tool Invocation Events

#### `tool.invoke_start`
Tool invocation started (after admission + permit acquisition).

```jsonc
{
  "tool_name": "web_search",
  "args_hash": "sha256:abcdef...",     // hash of args (never raw args — may contain PII)
  "timeout_ms": 10000,
  "queue_wait_ms": 12                   // time spent waiting for concurrency permits
}
```

#### `tool.invoke_success`
Tool completed successfully.

```jsonc
{
  "tool_name": "web_search",
  "duration_ms": 842,
  "output_bytes": 4096
}
```

#### `tool.invoke_failure`
Tool invocation failed.

```jsonc
{
  "tool_name": "web_search",
  "duration_ms": 10001,
  "error_kind": "timeout",             // "timeout" | "cancelled" | "runtime_error" | "egress_blocked" | "validation_error"
  "error_message": "deadline exceeded"
}
```

#### `tool.invoke_rejected`
Tool invocation rejected before dispatch (policy/validation).

```jsonc
{
  "tool_name": "unknown_tool",
  "reason": "tool_not_in_allowlist"     // e.g. "tool_not_implemented", "disabled", "args_validation_failed",
                                          //      "queue_full", "queue_timeout", "cancelled"
}
```

---

### `policy.*` — Policy Lifecycle Events

#### `policy.loaded`
Policy document loaded or reloaded.

```jsonc
{
  "policy_id": "prod-default",
  "schema_version": "0.1.0",
  "source": "file:///etc/gateway/policy.json",
  "hash": "sha256:deadbeef..."
}
```

#### `policy.reload_failed`
Policy reload attempted but rejected.

```jsonc
{
  "source": "file:///etc/gateway/policy.json",
  "reason": "schema_version_mismatch",
  "detail": "expected major 0, got 1"
}
```

#### `policy.rate_limit_hit`
Rate limiter triggered.

```jsonc
{
  "layer": "user",                      // "ip" | "user" | "tool" | "endpoint"
  "key": "usr_abc123",
  "limit_rps": 20,
  "path": "/mcp/invoke"
}
```

---

### `gateway.*` — Infrastructure Events

#### `gateway.started`
Gateway process started.

```jsonc
{
  "version": "0.1.0",
  "listen_addr": "0.0.0.0:8080",
  "policy_id": "prod-default"
}
```

#### `gateway.stopped`
Gateway shutting down gracefully.

```jsonc
{ "reason": "SIGTERM", "uptime_secs": 86400 }
```

#### `gateway.egress_blocked`
Outbound request blocked by egress policy.

```jsonc
{
  "tool_name": "web_search",
  "target_host": "evil.example.com",
  "target_port": 443,
  "reason": "host_not_in_allowlist"
}
```

---

## Storage Requirements

| Requirement | Detail |
|---|---|
| **Immutability** | Write-once. Backend should enforce append-only (e.g., write-only DB role, immutable object storage). |
| **Retention** | Configurable; recommend ≥ 90 days for compliance. |
| **Indexing** | Must be queryable by: `event_type`, `request_id`, `actor.user_id`, `timestamp` range. |
| **Integrity** | Consider hash-chaining or external signing for tamper evidence (future). |

### Recommended backends (pick one)

- **MVP:** Append to JSONL file, rotate daily → ship to S3/GCS.
- **Production:** Insert into append-only Postgres table (with `request_id`, `event_type`, `timestamp` indexes) or structured log pipeline (e.g., Vector → ClickHouse).

---

## Versioning

- `schema_version` in the envelope follows **semver**.
- New event types or new optional payload fields → minor bump.
- Envelope field changes or payload field removals → major bump.
- Consumers must tolerate unknown `event_type` values and unknown payload fields (forward-compatible).

---

## Extension Points

Future event types to add as needed:

| Event type | When |
|---|---|
| `auth.token_refreshed` | JWT refresh flow added |
| `auth.session_revoked` | Session invalidation implemented |
| `authz.role_changed` | RBAC management API ships |
| `tool.output_redacted` | DLP/output filtering added |
| `policy.updated` | Remote policy management API |
| `gateway.egress_allowed` | Verbose mode for egress audit trail |
| `data.read` / `data.write` | Application data access tracking |

Add new dotted types under existing namespaces, or introduce new namespaces. Existing consumers ignore unknown types.
