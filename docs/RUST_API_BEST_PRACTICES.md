# Rust API Best Practices (Gateway Pattern)

Use this as the reference pattern when refactoring other modules behind the Rust gateway.

## 1) Keep boundaries explicit
- **AuthN/AuthZ at middleware boundary** (not scattered in handlers).
- **Validation at API edge** (schema-first request parsing).
- **Business logic in service layer** (thin handlers).
- **Transport concerns isolated** (HTTP mapping separate from domain code).

## 2) Standard middleware stack (recommended order)
1. Request ID / trace context
2. Structured logging context
3. Auth middleware
4. CSRF middleware (for browser-session writes)
5. Rate limiting (route-bucketed, read/write lanes)
6. Handler dispatch

Reason: preserve observability, enforce security early, and avoid work on requests that will be rejected.

## 3) Auth + identity contract
- Normalize identity once into a typed principal (user/session/org/roles).
- Prefer **session-aware keys** for controls (e.g., rate limits), with safe fallback to IP.
- Never trust forwarded identity headers unless they are signed and gateway-originated.

## 4) CSRF and browser/programmatic split
- Browser writes: require CSRF token + same-site session semantics.
- Programmatic API clients: bearer token path without CSRF.
- Keep these paths explicit in docs and tests.

## 5) Rate limiting strategy
- Use **separate read/write lanes** with different RPS/BURST values.
- Bucket by route class (`tasks_read`, `tasks_write`, etc.) to avoid one endpoint starving all traffic.
- Key by stable session fingerprint first; IP fallback only when needed.
- Return consistent 429 response shape with retry guidance.

## 6) Error model + response shape
- Use a stable JSON error envelope with:
  - machine code
  - human-readable message
  - request ID
- Do not leak stack traces/secrets in client responses.
- Keep OpenAPI/schema docs synced with actual error payloads.

## 7) Observability defaults
- Structured logs with request ID, route, principal type, decision path.
- Audit events for privileged actions (`allowed/denied/completed/failed`).
- Fail-open behavior only where explicitly intended (e.g., external telemetry) and document it.

## 8) Config hygiene
- Centralize config in typed structs with defaults + env mapping.
- Treat new env vars as contract changes:
  - document in `DEPLOYMENT.md`
  - add test coverage for parsing/defaults
- Example already in use: read/write limiter envs (`RATE_LIMIT_READ_*`, `RATE_LIMIT_WRITE_*`).

## 9) Security-by-default coding rules
- Deny-by-default for tool execution and privileged endpoints.
- Redact sensitive fields in logs/audit payloads.
- Validate and sanitize rich text/HTML at boundary, not just UI.
- Keep debug/migration endpoints disabled or strongly gated in prod.

## 10) Testing contract (minimum)
- Middleware behavior tests (auth, CSRF, rate limit) for allow/deny paths.
- Route contract tests (status codes + response schema).
- Regression tests for known incidents (e.g., request storms, auth bypass cases).
- Add one test whenever adding a new security control or env flag.

## 11) Panic-safety enforcement (runtime)
- Runtime request paths must not introduce `unwrap(` / `expect(` / `panic!(`.
- Enforce with `./scripts/check-runtime-panics.sh` locally and in CI.
- Test-only regions are excluded (`#[cfg(test)]`).
- Startup-fail invariants are the only allowed exceptions and must be explicitly documented in `scripts/runtime-panic-allowlist.txt` with narrow matching.
- Any new allowlist entry requires a short reason in PR notes and should remain startup-only (never request-path).

---

## Refactor checklist for new module migrations
- [ ] Endpoint added to OpenAPI + schema docs
- [ ] Request/response validated at boundary
- [ ] Principal resolved via shared auth middleware
- [ ] CSRF policy correct for browser writes
- [ ] Route assigned to read/write limiter bucket
- [ ] Audit event emitted for privileged mutations
- [ ] Request ID propagated end-to-end
- [ ] Tests added for happy path + deny path + 429 path
- [ ] Deployment env vars documented
- [ ] Rollback path documented
