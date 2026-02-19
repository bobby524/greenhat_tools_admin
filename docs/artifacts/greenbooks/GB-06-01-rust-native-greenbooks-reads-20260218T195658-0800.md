# GB-06-01 — Rust-native GreenBooks core READ handlers (gateway)

Timestamp: 2026-02-18 19:56:58 -0800

## Scope implemented
- Added native Rust GET handlers in gateway for core GreenBooks endpoints:
  - `GET /api/greenbooks/accounts`
  - `GET /api/greenbooks/accounts/{id}`
  - `GET /api/greenbooks/customers`
  - `GET /api/greenbooks/customers/{id}`
  - `GET /api/greenbooks/invoices`
  - `GET /api/greenbooks/invoices/{id}`
- Wired these routes ahead of wildcard `"/api/greenbooks/{*path}"`, so these core reads no longer depend on tools-side `/api/greenbooks` proxy routing.
- Preserved auth/session parity by keeping these handlers under existing gateway auth + RBAC middleware stack.

## Response-contract parity notes
- Accounts list: validates `type` exactly against `asset|liability|equity|revenue|expense` and returns `400 { error: ... }` on invalid input.
- Accounts by id: returns `404 { error: "Account not found" }` when absent.
- Customers list:
  - Uses `gb_customers_list_with_open_balance` RPC first (matching tools behavior).
  - Falls back to table read when RPC is missing.
  - Returns `[]` when `gb_customers` relation is missing (compat behavior).
  - Enforces `limit` clamp to `1..500` and parses `active/search`.
- Customers by id: returns `404 { error: "Customer not found" }` when absent.
- Invoices list: validates status against `draft|sent|partially_paid|paid|overdue|void`; returns `400 { error: ... }` when invalid.
- Invoice by id: returns invoice with embedded `items` array by joining `gb_invoices` + `gb_invoice_items`.

## Files changed
- `gateway/src/lib.rs`

## Build/test evidence
- `cargo check -p gateway` ✅
- `cargo test -p gateway --lib` ✅ (84 passed)

Command run:
```bash
cargo check -p gateway
cargo test -p gateway --lib
```

## Risk/rollback
- Change is additive and route-specific.
- Rollback path: revert `gateway/src/lib.rs` route + handler additions.
