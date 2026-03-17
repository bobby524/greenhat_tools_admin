# MCP-over-HTTP API (Gateway Contract)

This document defines the minimal HTTP contract the Rust gateway exposes for MCP clients.

Goals:
- Stable, versioned endpoints.
- Strict JSONSchema validation for all payloads.
- Works for both browser-session users (cookie + CSRF) and programmatic clients (bearer/JWT).

Non-goals (v0):
- Streaming tool output.
- Server-sent events.
- Long-running job orchestration.

---

## Base

- Base path: `/mcp/v1`
- Content-Type: `application/json`
- Auth:
  - Browser: cookie session + CSRF header (writes)
  - Programmatic: `Authorization: Bearer <token>`

### Request IDs
Gateway MUST:
- accept optional `X-Request-Id`
- otherwise generate one
- echo it back in responses

---

## Endpoints

### `GET /mcp/v1/tools`
List tools available to the authenticated actor after entitlements + policy filtering.

Response:
```json
{
  "tools": [
    {
      "name": "echo",
      "title": "Echo",
      "description": "Returns the provided string.",
      "input_schema": { "type": "object", "properties": {"text": {"type": "string"}}, "required": ["text"] }
    }
  ]
}
```

### `POST /mcp/v1/tools/{tool_name}:call`
Invoke a tool.

Request (v0):
```json
{
  "call_id": "01J...", 
  "arguments": { "text": "hello" },
  "context": {
    "origin": "gateway",
    "source": "ui|api|mcp",
    "trace": { "request_id": "..." }
  }
}
```

Response (success):
```json
{
  "call_id": "01J...",
  "tool_name": "echo",
  "result": {
    "content": [
      { "type": "text", "text": "hello" }
    ]
  }
}
```

Response (error):
```json
{
  "call_id": "01J...",
  "tool_name": "echo",
  "error": {
    "code": "TOOL_DENIED|TOOL_NOT_FOUND|VALIDATION_ERROR|TIMEOUT|INTERNAL",
    "message": "Human readable message"
  }
}
```

### `POST /mcp/v1/validate`
Dry-run validation:
- auth/authz decision
- tool allowlist
- JSONSchema validation of arguments
- policy ceilings

Returns `200` with validation outcome.

---

## Security / Policy hooks

- Tool visibility filtered by:
  1) module entitlements / RBAC
  2) policy allowlist
  3) environment (prod vs dev)

- Every tool call emits audit events:
  - `tool.call.requested`
  - `tool.call.allowed|tool.call.denied`
  - `tool.call.completed|tool.call.failed`

- Tool arguments should be hashed in audit logs; do not log raw secrets.

---

## Schemas

See:
- `docs/schemas/mcp_tool_call_request.v0.schema.json`
- `docs/schemas/mcp_tool_call_response.v0.schema.json`
- `docs/openapi/mcp.v1.yaml`

## Live Production Verification (Agent Tokens)

Use this flow to verify token auth + MCP tool invocation + audit correlation in production.

1. In Admin UI (`admin.greenhatsec.com`), create a token for Bobby with scope `tools:invoke`.
2. Call MCP with `Authorization: Bearer agt_...` to `POST /v1/tools/call`.
3. Verify response + logs using a fixed `x-request-id`.

Example request:

```http
POST /v1/tools/call
Authorization: Bearer agt_<token>
Content-Type: application/json
x-request-id: verify-bobby-001
```

```json
{
  "tool": "http_get",
  "params": {
    "url": "https://example.com"
  }
}
```

Expected in hardened env (host not allowlisted):
- HTTP status `200`
- Body indicates tool-level failure, e.g. `{"success":false,"data":"egress denied: host \"example.com\" not in allowlist"}`

Audit evidence to confirm (same `request_id`):
- `auth.login_success` with `auth_mode: "agent_token"`, `actor_type: "agent"`, token id/name
- `tool.invoke_start`
- `gateway.egress_blocked`
- `tool.invoke_failure` (error kind `egress_blocked`)

Verification checklist:
- [ ] Token created for Bobby in admin UI
- [ ] MCP call sent via `POST /v1/tools/call` with Bearer `agt_...`
- [ ] Response received with expected policy behavior
- [ ] Audit events present and correlated by `x-request-id`

## Implementation best-practice reference

For refactoring additional modules into the Rust gateway pattern, use:
- `docs/RUST_API_BEST_PRACTICES.md`
