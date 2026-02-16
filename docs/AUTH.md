# Auth (BetterAuth) — Gateway Session Validation Contract

This doc defines how **API_MCP_Gateway** authenticates callers.

## Goals
- Support **browser cookie sessions** (tools.greenhatsec.com) and **programmatic** MCP clients.
- Keep it **deny-by-default** and avoid spoofable signals (Origin/Referer are not security).
- Make auth **testable** (trait-based validator) and observable (request_id in errors).

## Auth modes

### 1) Browser (cookie session)
- The gateway expects the BetterAuth session cookie (default):
  - `better-auth.session_token`
- The gateway forwards that cookie to BetterAuth for validation.

### 2) Programmatic (Bearer)
- The gateway accepts:
  - `Authorization: Bearer <token>`
- The gateway forwards it upstream to BetterAuth for validation.

## Validation mechanism

The gateway calls BetterAuth:
- `GET /api/auth/get-session`

BetterAuth behavior assumed:
- Returns `200` with a JSON payload containing `session` and `user` when valid
- Returns `200` with `null` when invalid/expired

Implementation:
- `gateway/src/auth/session.rs` (`BetterAuthClient`)
- Auth middleware:
  - `gateway/src/middleware/auth.rs`

On success the middleware inserts a `Principal` into request extensions.

## Exempt endpoints
The following paths bypass auth:
- `/health`
- `/version`
- `/metrics`

## Configuration

| Env var | Default | Notes |
|---|---:|---|
| `AUTH_ENABLED` | `true` | Master switch for auth middleware |
| `BETTERAUTH_BASE_URL` | `http://localhost:3000` | Where BetterAuth runs (tools app) |
| `BETTERAUTH_COOKIE_NAME` | `better-auth.session_token` | Session cookie name |
| `BETTERAUTH_TIMEOUT_MS` | `2000` | Upstream request timeout |

## CSRF boundary
CSRF enforcement is separate middleware (`docs/CSRF.md`).
- Cookie-authenticated **write** requests must satisfy CSRF.
- Bearer-authenticated requests are **CSRF-exempt** (non-browser clients).

## Notes / future work
- Add caching for session validations (Redis) once the auth contract is stable.
- Clarify organization membership / roles mapping with BetterAuth org plugin.
- Ensure auth failures emit audit events when the audit pipeline is wired.
