# Policy Configuration Schema

> **Schema version:** `0.1.0`
> **Status:** Draft — minimal but extensible
> **Machine-readable:** [`schemas/policy.v0.schema.json`](schemas/policy.v0.schema.json)

---

## Overview

A **policy document** is a single JSON (or TOML) object that the gateway loads at startup (and optionally hot-reloads via signal/watch). It governs:

| Concern | Section | Description |
|---|---|---|
| **Rate limiting** | `rate_limits` | Per-layer throttles (IP, user, tool, endpoint) |
| **Tool access** | `tools` | Per-tool allowlists, timeouts, concurrency caps |
| **Egress control** | `egress` | Outbound network allowlists (SSRF/exfil prevention) |
| **Authentication** | `auth` | Supported auth modes + session config |
| **RBAC** | `roles` | Named roles → permission sets |

All sections are **deny-by-default**: omitting a section means that capability is disabled / blocked.

---

## Top-Level Structure

```jsonc
{
  "schema_version": "0.1.0",       // semver — loader rejects unknown majors
  "id": "prod-default",             // human label, optional
  "rate_limits": { ... },
  "tools": { ... },
  "egress": { ... },
  "auth": { ... },
  "roles": { ... }
}
```

---

## `rate_limits`

Hierarchical token-bucket config. Each layer is optional; absent = unlimited at that layer (but a higher layer still applies).

```jsonc
{
  "rate_limits": {
    // Applies per source IP
    "ip": {
      "requests_per_second": 50,
      "burst": 100
    },
    // Applies per authenticated user ID
    "user": {
      "requests_per_second": 20,
      "burst": 40
    },
    // Applies per tool name (MCP invoke)
    "tool": {
      "requests_per_second": 5,
      "burst": 10
    },
    // Per-endpoint overrides (route pattern → limits)
    "endpoint_overrides": {
      "/health": null,               // null = exempt from rate limiting
      "/mcp/invoke": {
        "requests_per_second": 10,
        "burst": 20
      }
    }
  }
}
```

### Rate-limit fields

| Field | Type | Required | Description |
|---|---|---|---|
| `requests_per_second` | `number` | yes | Sustained rate (tokens/sec) |
| `burst` | `integer` | yes | Max token bucket size |

---

## `tools`

Controls which MCP tools are available and their resource bounds.

```jsonc
{
  "tools": {
    "default_policy": "deny",             // "deny" | "allow" — MUST be "deny" in prod
    "max_concurrent_global": 32,          // gateway-wide concurrency cap

    "allowlist": {
      "web_search": {
        "enabled": true,
        "timeout_ms": 10000,
        "max_concurrent": 4,
        "allowed_roles": ["analyst", "admin"],
        "args_schema": "schemas/tool_args/web_search.json"  // optional JSONSchema ref
      },
      "db_query": {
        "enabled": true,
        "timeout_ms": 30000,
        "max_concurrent": 2,
        "allowed_roles": ["admin"],
        "args_schema": null
      }
    }
  }
}
```

### Tool entry fields

| Field | Type | Required | Default | Description |
|---|---|---|---|---|
| `enabled` | `bool` | no | `true` | Quick kill-switch |
| `timeout_ms` | `integer` | no | `30000` | Per-invocation timeout |
| `max_concurrent` | `integer` | no | `8` | Max parallel invocations for this tool |
| `allowed_roles` | `string[]` | no | `[]` (none) | Roles permitted to invoke; empty = no one |
| `args_schema` | `string\|null` | no | `null` | Path/URI to JSONSchema for argument validation |

---

## `egress`

Outbound network allowlist. **Default: deny all outbound.** Only destinations listed here are reachable from tool execution contexts.

```jsonc
{
  "egress": {
    "default_policy": "deny",

    "allowlist": [
      {
        "host": "api.openai.com",
        "ports": [443],
        "protocol": "https"
      },
      {
        "host": "*.googleapis.com",
        "ports": [443],
        "protocol": "https"
      },
      {
        "cidr": "10.0.0.0/8",
        "ports": [5432],
        "protocol": "tcp",
        "label": "internal-postgres"
      }
    ],

    // Max response body the gateway will buffer from an egress call
    "max_response_bytes": 10485760   // 10 MiB
  }
}
```

### Egress entry fields

| Field | Type | Required | Description |
|---|---|---|---|
| `host` | `string` | * | DNS name or glob pattern |
| `cidr` | `string` | * | CIDR range (mutually exclusive with `host`) |
| `ports` | `integer[]` | yes | Allowed destination ports |
| `protocol` | `string` | yes | `"https"` \| `"tcp"` \| `"http"` |
| `label` | `string` | no | Human-readable name for audit logs |

\* One of `host` or `cidr` is required.

---

## `auth`

Defines which authentication mechanisms the gateway accepts.

```jsonc
{
  "auth": {
    "modes": ["session_cookie", "bearer_jwt"],

    "session_cookie": {
      "cookie_name": "better_auth_session",
      "validation_url": "http://localhost:3000/api/auth/session",
      "csrf": {
        "enabled": true,
        "header": "x-csrf-token",
        "method": "double_submit"    // "double_submit" | "signed_token"
      }
    },

    "bearer_jwt": {
      "issuer": "https://auth.example.com",
      "audience": "api-mcp-gateway",
      "jwks_url": "https://auth.example.com/.well-known/jwks.json",
      "jwks_refresh_interval_secs": 3600,
      "required_claims": ["sub", "roles"]
    },

    // Future: API key, mTLS, etc. — add new objects here.

    "anonymous_routes": [
      "/health",
      "/version"
    ]
  }
}
```

### Auth mode fields

| Mode | Key fields | Notes |
|---|---|---|
| `session_cookie` | `cookie_name`, `validation_url`, `csrf.*` | Cookie auth **must** have CSRF enabled for mutating requests |
| `bearer_jwt` | `issuer`, `audience`, `jwks_url` | Standard OIDC/JWT validation |

---

## `roles`

Named RBAC roles mapped to permission sets. A user's role(s) come from the auth token/session.

```jsonc
{
  "roles": {
    "admin": {
      "permissions": ["*"],
      "description": "Full access"
    },
    "analyst": {
      "permissions": [
        "tools:invoke",
        "tools:list",
        "data:read"
      ],
      "description": "Read + tool invocation"
    },
    "viewer": {
      "permissions": [
        "data:read"
      ],
      "description": "Read-only"
    }
  }
}
```

### Permission string format

Colon-delimited: `<resource>:<action>` (e.g., `tools:invoke`, `data:write`).
Wildcard `*` matches everything. **Only assign to `admin`.**

Initial permission vocabulary:

| Permission | Description |
|---|---|
| `tools:list` | List available MCP tools |
| `tools:invoke` | Invoke an MCP tool |
| `data:read` | Read application data |
| `data:write` | Mutate application data |
| `admin:policy` | View/update policy config |
| `admin:audit` | Read audit logs |

---

## Versioning & Migration

- `schema_version` uses **semver**.
- The gateway loader checks `schema_version` at startup:
  - **Same major** → load (warn on minor mismatch).
  - **Different major** → reject with clear error.
- Breaking changes (field removals, semantic changes) bump the major.
- New optional fields bump the minor.

---

## Extension Points

The schema is intentionally flat and composable. Future additions:

- **`dlp`** — data-loss-prevention rules (output scanning, PII redaction)
- **`quotas`** — per-org / per-user usage quotas (tokens, invocations/day)
- **`transforms`** — request/response rewriting rules
- **`notifications`** — alert channels for policy violations

Add new top-level keys; existing parsers ignore unknown keys (forward-compatible).

---

## File format

The gateway accepts **JSON** (canonical) or **TOML** (for human editing). The JSON schema file is the source of truth.
