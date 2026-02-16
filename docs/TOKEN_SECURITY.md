# Token Security (Bearer/JWT)

This doc defines the token security posture for programmatic gateway access.

## Goal
Make programmatic MCP auth safe and operable:
- JWT validation that supports key rotation (`kid`)
- Revocation strategy
- Replay protections where needed

## Issuer model
Current direction: **BetterAuth (or another auth service) issues JWTs**, the gateway validates them locally via **JWKS**.

## Validation
- `Authorization: Bearer <jwt>`
- Signature validated against JWKS
- Claims validated:
  - `exp` (required)
  - optional `iss` / `aud` (env-configured)
  - `sub` used as `Principal.user_id`

## Key rotation
- JWKS must expose `kid`
- Gateway caches keys by `kid` and refreshes when an unknown `kid` is seen

## Revocation

Implemented: **Redis jti denylist**.

- Gateway checks `EXISTS ${JWT_REVOCATION_KEY_PREFIX}${jti}` on bearer-authenticated requests.
- If present, the token is rejected (`invalid or expired token` / `revoked_token`).
- Store entries should have TTL ≤ token lifetime.

Future option:
- Per-user revocation via `token_version` claim.

## Replay protections (planned)
For sensitive actions (tool execution, mutations):
- require `jti`
- optionally enforce one-time use for certain endpoints (nonce/jti cache)

## Configuration
- `JWT_JWKS_URL` (required to enable local JWT validation)
- `JWT_ISSUER` (optional)
- `JWT_AUDIENCE` (optional)
- `JWT_JWKS_TIMEOUT_MS` (optional, default 2000)
- `JWT_REQUIRE_JTI` (optional, default false)
- `JWT_REVOCATION_ENABLED` (optional; defaults to true when `REDIS_URL` is set)
- `JWT_REVOCATION_KEY_PREFIX` (optional, default `revoked:jti:`)
- `REDIS_URL` (required for revocation)
