# RBAC — Deny-by-Default Authorization

> **Status:** Shipped (Milestone 2)
> **Schema:** [`docs/schemas/policy.v0.schema.json`](schemas/policy.v0.schema.json)
> **Example:** [`docs/examples/policy.local.json`](examples/policy.local.json)

---

## Overview

The gateway enforces a **deny-by-default** RBAC model.  Every request that
reaches a protected route must carry an authenticated `Principal` (injected by
the auth middleware) whose roles grant the required permissions.  If no policy
file is loaded, RBAC is not enforced — but when active, everything not
explicitly allowed is denied.

```
Request → Auth → RBAC → Handler
                  │
                  ├─ route-level check (method + path → permission)
                  └─ tool-level check  (POST /v1/tools/call → tool allowlist)
```

## Quick Start

1. Create a policy file (or copy the example):

   ```bash
   cp docs/examples/policy.local.json my-policy.json
   ```

2. Point the gateway at it:

   ```bash
   export POLICY_FILE=./my-policy.json
   cargo run -p gateway
   ```

   On startup you'll see:

   ```
   INFO RBAC policy loaded  policy_file=./my-policy.json schema_version=0.1.0
   ```

3. Requests without an appropriate role will receive a `403 Forbidden`:

   ```json
   {
     "error": {
       "code": 403,
       "kind": "forbidden",
       "message": "tool 'db_query' not in allowlist",
       "request_id": "xxxxxxxx-xxxx-…"
     }
   }
   ```

## Configuration

| Env Var | Required | Description |
|---------|----------|-------------|
| `POLICY_FILE` | No | Path to policy JSON file.  When unset, RBAC middleware is **not loaded**. |

The policy file is read once at startup.  The gateway panics if the file is
set but unreadable or contains an unsupported `schema_version`.

## Policy File Structure

The RBAC middleware reads two top-level sections from the policy file:

### `tools` — Tool Access Control

```jsonc
{
  "tools": {
    "default_policy": "deny",       // "deny" | "allow"
    "allowlist": {
      "web_search": {
        "enabled": true,            // kill-switch
        "allowed_roles": ["analyst", "admin"]
      }
    }
  }
}
```

| Field | Default | Description |
|-------|---------|-------------|
| `default_policy` | `"deny"` | What happens to tools **not** in the allowlist |
| `allowlist.<tool>.enabled` | `true` | Set `false` to instantly disable a tool |
| `allowlist.<tool>.allowed_roles` | `[]` | Roles that may invoke this tool.  **Empty = no-one.** |

### `roles` — Permission Grants

```jsonc
{
  "roles": {
    "admin":   { "permissions": ["*"] },
    "analyst": { "permissions": ["tools:invoke", "tools:list", "data:read"] },
    "viewer":  { "permissions": ["data:read"] }
  }
}
```

A principal's roles (from the auth token/session) are looked up in this map.
Each role grants a set of permission strings.  The wildcard `"*"` matches
everything.

### Permission Vocabulary

| Permission | Grants |
|------------|--------|
| `tools:invoke` | Call MCP tools (`POST /v1/tools/call`) |
| `tools:list` | List available tools (`GET /v1/tools`) |
| `data:read` | Read application data (`GET /v1/*`) |
| `data:write` | Mutate application data (`POST/PUT/PATCH/DELETE /v1/*`) |
| `admin:policy` | View/update policy config (`/v1/admin/policy*`) |
| `admin:audit` | Read audit logs (`/v1/admin/audit*`) |
| `*` | Wildcard — all permissions |

## Evaluation Logic

### Route-level Check

Every non-exempt request is mapped to a required permission:

1. Infrastructure routes (`/health`, `/version`, `/metrics`) → **exempt**
2. `/v1/tools/call` POST → `tools:invoke`
3. `/v1/tools` GET → `tools:list`
4. `/v1/admin/policy*` → `admin:policy`
5. `/v1/admin/audit*` → `admin:audit`
6. `/v1/*` GET/HEAD/OPTIONS → `data:read`
7. `/v1/*` POST/PUT/PATCH/DELETE → `data:write`
8. Everything else → **no restriction at RBAC layer** (will 404 at the router)

If the principal's roles don't grant the required permission → `403`.

### Tool-level Check

For `POST /v1/tools/call`, the middleware **also** inspects the request body
to extract the tool name from `{"tool": "…"}`.  It then checks:

1. Is the tool in `tools.allowlist`?  If not → apply `default_policy`.
2. Is the tool `enabled`?  If `false` → deny.
3. Does the principal have at least one role listed in `allowed_roles`?

A principal with the `"*"` (wildcard) permission bypasses **all** tool-level
checks — this is the admin escape hatch.

### Deny by Default

- No `Principal` in request extensions → `403`
- No `tools` section in policy → deny all tool calls
- Empty `allowed_roles` → deny (even if the tool is in the allowlist)
- Unknown tool + `default_policy: "deny"` → deny

## Audit Events

The RBAC middleware emits `authz.denied` audit events on every denial:

```json
{
  "event_type": "authz.denied",
  "request_id": "abc-123",
  "source_ip": "203.0.113.42",
  "actor": {
    "user_id": "usr_1",
    "roles": ["viewer"],
    "auth_mode": "bearer_jwt"
  },
  "payload": {
    "reason": "missing permission 'tools:invoke' for POST /v1/tools/call",
    "action": "route_access",
    "method": "POST",
    "path": "/v1/tools/call"
  }
}
```

Audit events never contain secrets, credentials, or request bodies.

## Error Format

All RBAC denials return a structured `403 Forbidden` with `request_id`:

```json
{
  "error": {
    "code": 403,
    "kind": "forbidden",
    "message": "principal lacks required role for tool 'db_query'",
    "request_id": "xxxxxxxx-xxxx-…"
  }
}
```

## Security Notes

- **Secrets are never logged.**  The middleware logs `user_id`, roles, and
  the denied action — never tokens, cookies, or request bodies.
- The policy engine is **stateless and infallible**: it returns `Allow` or
  `Deny` without I/O, panics, or side effects.
- The admin `"*"` wildcard should be assigned sparingly.  In production,
  prefer explicit permission grants.

## Key Files

| File | Description |
|------|-------------|
| `gateway/src/rbac/policy.rs` | Policy types + loader |
| `gateway/src/rbac/engine.rs` | Stateless evaluation engine (25 unit tests) |
| `gateway/src/rbac/types.rs` | Domain primitives (`Action`, `Decision`, `Scope`) |
| `gateway/src/middleware/rbac.rs` | Axum middleware (audit integration) |
| `gateway/tests/rbac.rs` | 13 integration tests |
| `docs/schemas/policy.v0.schema.json` | JSON Schema (source of truth) |
| `docs/examples/policy.local.json` | Example policy (no secrets) |
