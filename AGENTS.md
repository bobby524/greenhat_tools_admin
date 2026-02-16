# AGENTS.md — API_MCP_Gateway Development Playbook

This repo is the Rust-first API gateway + embedded MCP runtime.
Goal: ship incrementally without weakening auth, RBAC, or data boundaries.

## Prime Directives

1) **Security before features.** The gateway is a choke-point; a bug is org-wide.
2) **Deny by default.** No implicit access. No ambient authority.
3) **Never trust Origin/Referer/CORS** as security.
4) **No secrets in git.** No creds in logs. No keys in chat output.
5) **Observability is a feature.** If we can’t audit it, we can’t ship it.

---

## Architecture Baselines

### Components
- **Gateway (Rust / axum)**
  - AuthN: validates BetterAuth session (cookie) or bearer/JWT for programmatic clients
  - AuthZ: RBAC + module entitlements (deny-by-default)
  - Policy enforcement: input validation, rate limits, header hardening
  - Audit logging: append-only events
- **Embedded MCP runtime**
  - Tool registry + invoke endpoint
  - Per-tool allowlists, timeouts, concurrency limits
  - Egress controls (SSRF/exfil prevention)
- **BetterAuth service** (short-term)
  - Source of truth for cookie session validation

### Hard requirements
- **CSRF** for cookie-auth writes (double-submit or explicit CSRF token header)
- **Header hardening**: strip/ignore spoofable identity headers at the edge
- **Rate limiting** at multiple layers (IP + user + tool + endpoint)
- **Schema validation** for every request body and MCP tool args (JSONSchema)
- **Audit events** for: auth decisions, RBAC decisions, tool invocations, mutations

---

## Gotchas (learned already)

1) **Cookie auth implies CSRF**. If we skip CSRF, we ship a vulnerability.
2) **ID type mismatches (TEXT vs UUID)** are real.
   - Decide a canonical identity representation early; avoid hot-path casts.
3) **Shadow mode mirroring is hard**.
   - Canonicalize responses (timestamps, ordering, cursors) before diffing.
4) **Service-role sprawl**.
   - Gateway must not become a “god token”. Plan least-priv DB roles / RLS.
5) **MCP is an exfil surface**.
   - Egress allowlists + DLP + output filters + logging are mandatory.

---

## Development Workflow

### Repo memory
- Keep `progress.md` updated as we ship milestones (what changed + why + pointers).
- Keep the **Exponential project dashboard** (Rust Gateway + Embedded MCP) accurate: mark tasks DONE and leave a short shipped comment (commit + key files).
- Treat `docs/*` as the source of truth for contracts/schemas.

### Code graph
- Use CodeGraphContext (codegraph MCP) for deep code navigation when making changes.
- Setup/usage: `docs/CODEGRAPH.md`.


### Before you code
- Write the contract first: route + request/response types + JSONSchema.
- Decide the auth mode for the endpoint: cookie+CSRF or bearer.
- Write the audit event shape for the operation.

### Every PR must include
- Tests for auth bypass attempts (unauth, wrong role, missing CSRF)
- Logging fields: request_id, user_id (if authed), route, outcome
- Clear rollback plan if it touches auth/policy

---

## Local Setup (expected)

- Rust stable + clippy + rustfmt.
- Vendor MCP rust sdk is included as submodule: `vendor/mcp-rust-sdk`.

---

## Tooling Notes

- Codegraph: if used, treat as advisory; verify by grep + compile.
- Prefer deterministic validation over “it seems right”.
