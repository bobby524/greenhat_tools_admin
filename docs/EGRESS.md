# Egress Firewall

The egress firewall controls **all outbound HTTP requests** made by the gateway on behalf of tool calls.  It is the primary defense against SSRF, data exfiltration, and runaway responses.

## Design Principles

| Principle | Implementation |
|-----------|---------------|
| **Fail-closed** | Empty allowlist → zero outbound requests permitted |
| **Defence in depth** | Allowlist + private-IP denial + size caps + timeouts |
| **No auto-follow** | Redirect responses are returned as-is (no `3xx` chasing) |
| **Streaming size cap** | Response body is capped *during* streaming, not just via `Content-Length` |

## Architecture

```
┌──────────────┐     preflight()     ┌────────────────┐
│  ToolRouter  │ ──────────────────► │  EgressClient   │
│  (dispatch)  │                     │                 │
│              │ ◄────── response ── │  reqwest::Client│
└──────────────┘                     └───────┬────────┘
                                             │
                                    ┌────────▼────────┐
                                    │  Preflight checks│
                                    │  1. Parse URL    │
                                    │  2. Allowlist    │
                                    │  3. DNS resolve  │
                                    │  4. Private IP?  │
                                    │  5. Body size    │
                                    └─────────────────┘
```

### Modules

| File | Purpose |
|------|---------|
| `gateway/src/egress.rs` | `EgressClient`, `EgressConfig`, `is_private_ip()` |
| `gateway/src/tool_router.rs` | Stubbed tool dispatcher wiring calls through egress |
| `gateway/tests/egress.rs` | Integration tests |

## Configuration

All knobs are set via environment variables (see `.env.example`):

| Variable | Default | Description |
|----------|---------|-------------|
| `EGRESS_ALLOWED_HOSTS` | *(empty — deny all)* | Comma-separated hostnames (e.g. `api.openai.com,api.anthropic.com`) |
| `EGRESS_TIMEOUT_SECS` | `30` | Total per-request timeout (connect + transfer) |
| `EGRESS_CONNECT_TIMEOUT_SECS` | `10` | TCP connect-phase timeout |
| `EGRESS_MAX_RESPONSE_BYTES` | `5242880` (5 MiB) | Max response body; streams are aborted if exceeded |
| `EGRESS_MAX_REQUEST_BODY_BYTES` | `1048576` (1 MiB) | Max outbound request body; checked before send |
| `EGRESS_DENY_PRIVATE_IPS` | `true` | Block RFC 1918 / loopback / link-local / CGNAT resolved IPs |

### Host allowlist format

- Lowercase hostnames, no scheme, no port, no path.
- Entries are compared after lowercasing the URL host.
- Wildcards are **not** supported — list each hostname explicitly.

```bash
EGRESS_ALLOWED_HOSTS=api.openai.com,api.anthropic.com,httpbin.org
```

## Private IP Ranges Blocked

When `EGRESS_DENY_PRIVATE_IPS=true` (default), the client resolves DNS *before* connecting and rejects any address in these ranges:

| Range | RFC / Purpose |
|-------|---------------|
| `127.0.0.0/8` | IPv4 loopback |
| `10.0.0.0/8` | RFC 1918 private |
| `172.16.0.0/12` | RFC 1918 private |
| `192.168.0.0/16` | RFC 1918 private |
| `100.64.0.0/10` | RFC 6598 CGNAT / shared |
| `169.254.0.0/16` | Link-local |
| `0.0.0.0/8` | "This" network |
| `::1` | IPv6 loopback |
| `fc00::/7` | IPv6 unique-local |
| `fe80::/10` | IPv6 link-local |

> **Note:** There is a TOCTOU window between our DNS check and `reqwest`'s own resolution.  For production hardening, consider a custom `reqwest` connector that pins resolved IPs or a network-level firewall rule.  The current implementation is an effective baseline that catches the vast majority of SSRF vectors.

## Tool Router

The `ToolRouter` dispatches tool calls through the egress client.  Currently two built-in pseudo-tools exist for integration testing:

| Tool | Parameters | Behaviour |
|------|-----------|-----------|
| `http_get` | `{ "url": "…" }` | GET through egress |
| `http_post` | `{ "url": "…", "body": "…" }` | POST through egress |

Unknown tool names return `{ "success": false, "data": "unknown tool: …" }`.

As real MCP tool definitions are integrated, each tool's outbound requests will flow through `EgressClient::request()`, inheriting all firewall rules automatically.

## Running Tests

```bash
cd gateway
cargo test --test egress       # integration tests
cargo test egress               # all egress-related unit + integration tests
cargo test tool_router          # tool router unit tests
```

## Future Work

- [ ] Wildcard / suffix-based host matching (`*.openai.com`)
- [ ] Per-tool allowlists (different tools get different host sets)
- [ ] Custom `reqwest` DNS resolver to eliminate TOCTOU gap
- [ ] Metrics: `egress_requests_total`, `egress_blocked_total` counters
- [ ] Circuit-breaker / retry policy per upstream host
