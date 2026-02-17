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

## Implementation best-practice reference

For refactoring additional modules into the Rust gateway pattern, use:
- `docs/RUST_API_BEST_PRACTICES.md`
