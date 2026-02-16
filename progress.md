# progress.md — API_MCP_Gateway

This file is the running, human-readable log of what we shipped and why.
Keep it short, factual, and link to commits/files.

---

## 2026-02-14 — Milestone 0 completed ✅

### Gateway skeleton (Docker-first)
- Axum gateway crate in `gateway/`
- Endpoints:
  - `GET /health`
  - `GET /version`
- Request-id + structured logging (`tracing`)
- Multi-stage `Dockerfile` + `docker-compose.yml` (runs on `:8080`)

### MCP Rust SDK integrated
- MCP Rust SDK added as submodule: `vendor/mcp-rust-sdk`
- Spike crate `mcp-spike/` proves:
  - tool registration + discovery + invocation
  - in-process transport via `tokio::io::duplex`

### Docs delivered
- `docs/ARCHITECTURE.md`
- `docs/POLICY_SCHEMA.md` + `docs/AUDIT_EVENTS.md`
- `docs/schemas/*` (policy + audit JSON Schemas)
- MCP-over-HTTP contract:
  - `docs/MCP_HTTP_API.md`
  - `docs/openapi/mcp.v1.yaml`
  - `docs/schemas/mcp_tool_call_{request,response}.v0.schema.json`

---

## 2026-02-14 — Milestone 1 (in progress)

### Secrets management ✅
- `.env.example`
- `gateway/src/config.rs` (required env validation + redacted logs)
- `docs/SECRETS.md`

### Redis + cache helpers ✅
- `docker-compose.yml` includes redis
- `gateway/src/cache.rs` feature-flagged cache helpers
- `docs/REDIS.md`

### CI/CD + deployment baseline ✅
- `.github/workflows/ci.yml` builds/tests Rust and builds/publishes Docker images to GHCR on `main`.
- `docs/DEPLOYMENT.md` describes staging/prod flows + canary/rollback.

### Rate limiting + request/input validation middleware ✅
- Token-bucket per-IP rate limiting + JSON-only enforcement for write methods + body size checks.
- Integration tests in `gateway/tests/middleware.rs`.

### Observability (metrics + tracing/OTel) ✅
- Baseline observability docs + scaffolding for `/metrics` and tracing config.

### Remaining Milestone 1 items
- (none)

---

## 2026-02-14 — Milestone 2 (in progress)

### Header hardening ✅
- Added edge middleware that strips spoofable identity headers and sets baseline security response headers.
- Files: `gateway/src/middleware/headers.rs`, `gateway/tests/headers.rs`
- Docs: `docs/HEADERS.md`
- Commit: `6e861aa`

### Egress / tool firewall baseline ✅
- `gateway/src/egress.rs` — hardened `EgressClient` outbound HTTP wrapper:
  - Host allowlist via `EGRESS_ALLOWED_HOSTS` env var (fail-closed: empty = deny all)
  - Timeouts: per-request (`EGRESS_TIMEOUT_SECS`) + connect (`EGRESS_CONNECT_TIMEOUT_SECS`)
  - Max response body (`EGRESS_MAX_RESPONSE_BYTES`, 5 MiB default) — streaming abort
  - Max request body (`EGRESS_MAX_REQUEST_BODY_BYTES`, 1 MiB default) — pre-flight check
  - Private IP denial (RFC 1918 / 6598 / loopback / link-local / CGNAT) via DNS pre-resolve
  - No redirect following (redirect responses returned as-is)
- `gateway/src/tool_router.rs` — stubbed tool dispatch (`http_get`, `http_post`) wired through egress
- 48 total tests passing (25 unit in egress + tool_router, 14 integration in `tests/egress.rs`, 9 existing middleware)
- Docs: `docs/EGRESS.md`
- `.env.example` updated with all egress env vars

---

## 2026-02-14 — Milestone 2 (in progress)

### CSRF enforcement for cookie-auth flows ✅
- Double-submit cookie CSRF middleware in `gateway/src/middleware/csrf.rs`.
- Enforced on POST / PUT / PATCH / DELETE; exempt: `/health`, `/version`, `/metrics`.
- Config knobs via env vars: `CSRF_ENABLED`, `CSRF_COOKIE_NAME`, `CSRF_HEADER_NAME`.
- Wired into middleware stack between validation and auth layers.
- 18 integration tests in `gateway/tests/csrf.rs` (all passing).
- Documentation: `docs/CSRF.md`.
- `.env.example` updated with CSRF variables.

### AuthN: BetterAuth session validation in gateway ✅
- Auth domain + BetterAuth validator client: `gateway/src/auth/*`.
- Auth middleware: `gateway/src/middleware/auth.rs` (cookie or Bearer; exempts health/version/metrics).
- Config knobs in `.env.example`: `AUTH_ENABLED`, `BETTERAUTH_BASE_URL`, `BETTERAUTH_COOKIE_NAME`, `BETTERAUTH_TIMEOUT_MS`.
- Contract doc: `docs/AUTH.md`.

### Token security (bearer/JWT) ✅
- Local JWT validation via JWKS (kid-based rotation) + Redis jti denylist revocation.
- Commits: `48f1170`, `037942e`.
- Docs: `docs/TOKEN_SECURITY.md`.

### Auth modes (cookie + bearer/JWT) ✅
- CSRF exemption for Bearer clients.
- Fail-closed gating so a validator must explicitly support the presented credential type.
- Commits: `3362026`, `6b9ad05`.

### DB roles + RLS bootstrap (GUC impersonation backstop) ✅
- Supabase migration in Tools DB adds `request_user_id()`/`request_org_id()` helpers and updates RLS policies to support actor identity via Postgres GUCs (`request.user_id`, `request.org_id`) while still supporting `auth.uid()`.
- Commit (greenhat_tools): `0a66e0f`.

### Tool execution isolation ✅
- Enforced per-tool timeouts + global/per-tool concurrency limits + bounded queue/backpressure.
- Optional cancellation support for queued + in-flight tool calls.
- Improved audit fields (`queue_wait_ms`, `timeout_ms`) and structured egress-blocked classification.
- Docs: `docs/TOOL_EXECUTION.md` (plus updates in `docs/AUDIT_EVENTS.md`).
- Commits: `22baa5d`, `10cf3d2`.

---

### Audit event pipeline ✅
- **Audit event struct** (`gateway/src/audit/event.rs`): Matches `docs/schemas/audit_event.v0.schema.json` — envelope with event_id, event_type, timestamp, schema_version, request_id, source_ip, actor, payload.
- **Sink abstraction** (`gateway/src/audit/sink.rs`): `AuditSink` trait + `StdoutSink` (always on) + `FileSink` (via `AUDIT_LOG_FILE` env) + `CompositeSink` fan-out.
- **Secret redaction** (`gateway/src/audit/redact.rs`): `hash_args()`, `hash_credential()`, `redact_string()` — no raw secrets/PII in events per `docs/SECRETS.md`.
- **AuditLog handle** (`gateway/src/audit/mod.rs`): Cheaply cloneable `Arc<dyn AuditSink>`, injected into all middleware via axum State.
- **Emission points wired:**
  - Auth middleware → `auth.login_success`, `auth.login_failure`
  - CSRF middleware → `auth.csrf_reject`
  - Rate limiter → `policy.rate_limit_hit`
  - Validation middleware → `tool.invoke_rejected` (payload too large, wrong content-type)
  - Tool router → `tool.invoke_start`, `tool.invoke_success`, `tool.invoke_failure`, `tool.invoke_rejected`, `gateway.egress_blocked`
- **`request_id` propagation**: All events carry the `x-request-id` from the outer middleware layer.
- **88 tests passing** (47 unit + 18 CSRF integration + 14 egress integration + 9 middleware integration).
- Docs: `docs/AUDITING.md`.
- `.env.example` updated with `AUDIT_LOG_FILE`.

### AuthZ: RBAC + module entitlements middleware (deny-by-default) ✅
- **Policy loading:** `POLICY_FILE` env var → JSON matching `docs/schemas/policy.v0.schema.json`.
  Loaded at startup; panics on missing/invalid file.  Schema version validated (must be `0.x.y`).
- **RBAC engine:** `gateway/src/rbac/engine.rs` — stateless, infallible evaluation:
  - Route-level permission check (method + path → required permission string).
  - Tool-level check (POST `/v1/tools/call` → body-inspect tool name → `tools.allowlist` + `allowed_roles`).
  - Admin wildcard (`"*"`) bypasses all checks.
  - Deny-by-default: no principal → 403; no tools section → deny all; empty `allowed_roles` → deny.
- **Middleware:** `gateway/src/middleware/rbac.rs` — sits between auth and handlers.
  Emits `authz.denied` audit events. Structured 403 errors include `request_id`.
  Infrastructure endpoints (`/health`, `/version`, `/metrics`) are exempt.
- **Example policy:** `docs/examples/policy.local.json` (no secrets).
- **Tests:** 25 unit tests (engine + policy) + 13 integration tests = 38 new tests (126 total).
- **Docs:** `docs/RBAC.md`.
- **Config:** `POLICY_FILE` in `.env.example` + `GatewayConfig.policy_file`.

---

## 2026-02-15 — Exponential gateway migration (phase 1)

### Gateway-owned Exponential surface ✅
- Added first-class `/api/exponential/*` handlers for tasks/sprints/projects that route through the tool router (egress allowlist + audit), with JSON body mapping from snake_case payloads to canonical tool params.
- Updated RBAC mapping to enforce `data:read`/`data:write` on `/api/exponential/*` routes.
- Added update-task mapping parity (status/priority/dueAt/labels/milestone/position/action) and schema sync for `action`.
- Added auth/shape test for `/api/exponential/tasks` and route mapping test for RBAC.
- Files: `gateway/src/lib.rs`, `gateway/src/rbac/engine.rs`, `gateway/src/tool_router.rs`, `gateway/src/lib.rs` (tests), `docs/schemas/exponential_tools.v0.schema.json`, `docs/EXponential_TOOL_MIGRATION_PLAN.md`.

## Tooling

### CodeGraphContext (codegraph MCP)
- Target: install CodeGraphContext MCP server for deep code graph queries during development.
- Repo: https://github.com/CodeGraphContext/CodeGraphContext

Status: installed + indexed; MCP server runnable via `cgc mcp start`.
