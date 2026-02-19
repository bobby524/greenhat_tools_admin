# GB-06-02 — Rust-native GreenBooks core WRITE handlers (gateway)

Timestamp: 2026-02-18 20:18:18 -0800

## Scope implemented
Moved core GreenBooks write flows off tools-side `/api/greenbooks/{*path}` proxy and into first-class Rust handlers in `gateway/src/lib.rs`.

### New native handlers
- `GET /api/greenbooks/payments`
- `GET /api/greenbooks/invoices/{id}/payments`
- `POST /api/greenbooks/invoices/{id}/payments`
- `POST /api/greenbooks/invoices/{id}/post`
- `POST /api/greenbooks/invoices/{id}/post-gl`
- `GET /api/greenbooks/bank-accounts/{id}/reconcile`
- `POST /api/greenbooks/bank-accounts/{id}/reconcile`
- `PATCH /api/greenbooks/bank-accounts/{id}/reconcile`
- `POST /api/greenbooks/bank-accounts/transfer`

All routes are wired ahead of `"/api/greenbooks/{*path}"` catch-all, so these calls no longer depend on tools Next.js route handlers.

## Auth/CSRF parity
- Handlers require gateway principal (same parity as existing gateway-owned handlers) and return `401 {"error":"Unauthorized"}` when no principal.
- Existing gateway middleware stack remains unchanged:
  - auth middleware still validates BetterAuth/JWT
  - CSRF middleware still enforces write-method double-submit for cookie-auth flows
  - Bearer stays CSRF-exempt

## Response-contract parity notes
- Preserved tools-side validation/error messages for key write flows:
  - invoice payment `amount` positive check
  - invoice payment `payment_date` required check
  - reconcile `statement_date` + `statement_balance` required
  - reconcile patch requires `reconciliation_id`, `transaction_id`, `cleared`
  - bank transfer requires `from_bank_id`, `to_bank_id`, positive `amount`
- `POST /invoices/{id}/post` and `/post-gl` both map to same handler for parity with tools alias behavior.

## Build/test evidence
- `cargo check -p gateway` ✅
- `cargo test -p gateway --lib` ✅ (84 passed)

## Deploy evidence
- Deployed to Fly app `greenhat-tools-admin-cmmugg`
- Deployment image: `registry.fly.io/greenhat-tools-admin-cmmugg:deployment-01KHT1T70N4GRMMJ6M2FNZE30E`
- Machine `2863225c620038` reached healthy state

## Live verification against `api.greenhatsec.com`
### Completed
- Endpoint auth gate verified from tools environment token path:
  - `GET /api/greenbooks/payments` → `401 invalid or expired session`
  - `POST /api/greenbooks/invoices/000.../post` → `401 invalid or expired session`

### Blocker
Could not complete fully authenticated browser-session verification in this subagent session because OpenClaw Chrome relay had no attached tab:
- Browser control reported: `Chrome extension relay is running, but no tab is connected`.
- Without an attached tools session tab (or valid live bearer), cookie-auth + CSRF flow cannot be executed from this automation context.

### Workaround to finish final live auth verification
1. In user Chrome, open tools/admin session already logged in.
2. Click OpenClaw Browser Relay extension icon on that tab (badge ON).
3. Re-run live checks for:
   - invoice post/post-gl
   - invoice payment create/list
   - reconcile start/patch/complete
   - bank transfer

## Files changed
- `gateway/src/lib.rs`
- `docs/artifacts/greenbooks/GB-06-02-rust-native-greenbooks-writes-20260218T201818-0800.md`
