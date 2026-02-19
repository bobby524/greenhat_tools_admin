# GB-06-04 — GreenBooks Rust-native parity/regression pack

Timestamp: 2026-02-18 20:39:45 -0800

## Scope
Extended deterministic parity/regression coverage for Rust-native GreenBooks routes migrated in GB-06-01/02 (read + write + error envelope/auth+csrf behavior), and added cutover-route guard coverage in tools repo for migrated endpoint set.

## Repos changed

### 1) `greenhat_tools_admin`
- **File:** `gateway/src/lib.rs`
- Added 4 gateway unit tests and router helper covering migrated Rust-native endpoints:
  - `greenbooks_rust_routes_require_principal_for_migrated_reads_and_writes`
    - Verifies `GET /api/greenbooks/payments` and `POST /api/greenbooks/invoices/{id}/payments` return deterministic `401 {"error":"Unauthorized"}` without principal.
  - `greenbooks_write_routes_enforce_csrf_error_envelope_for_cookie_auth`
    - Verifies write route CSRF rejection for cookie-auth path returns structured envelope (`error.kind=forbidden`, message `CSRF token missing or invalid`).
  - `greenbooks_write_validation_contract_uses_legacy_error_shape_when_authed`
    - Verifies authenticated write validation preserves legacy body shape (`{"error":"amount must be a positive number"}`).
  - `greenbooks_read_validation_contract_is_deterministic`
    - Verifies deterministic read validation parity for invalid account type (`400 {"error":"Invalid account type: wat"}`).

### 2) `greenhat_tools`
- **File:** `scripts/regression-tests/greenbooks-rust-native-cutover-routes.test.js`
- Added regression guard to keep GB-06 migrated endpoints mode-proxy gated for rollback/failover safety:
  - accounts (+ by id)
  - customers (+ by id)
  - invoices (+ by id)
  - payments
  - invoice payments
  - invoice post / post-gl
  - bank reconcile
  - bank transfer

## Validation commands + results

### `greenhat_tools_admin`
```bash
cargo check -p gateway
cargo test -p gateway --lib
```
- ✅ `cargo check -p gateway` passed
- ✅ `cargo test -p gateway --lib` passed (88 passed, 0 failed)

### `greenhat_tools`
```bash
npm test
npm run build
```
- ✅ `npm test` passed (50 passed, 0 failed)
- ✅ `npm run build` passed (Next.js production build successful)

## Notes
- Existing unrelated dirty files were present in `greenhat_tools_admin`; only targeted GB-06-04 files are intended for commit.
- Existing unrelated untracked artifacts were present in `greenhat_tools`; only the new GB-06-04 regression test file is intended for commit.
