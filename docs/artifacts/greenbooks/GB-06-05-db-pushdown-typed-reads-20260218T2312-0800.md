# GB-06-05 — DB Pushdown + Typed Read Payloads (Rust)

## Scope
Optimized migrated Rust-native GreenBooks read paths in `gateway/src/lib.rs`:
- `GET /api/greenbooks/accounts`
- `GET /api/greenbooks/accounts/{id}`
- `GET /api/greenbooks/customers`
- `GET /api/greenbooks/customers/{id}`
- `GET /api/greenbooks/invoices`
- `GET /api/greenbooks/invoices/{id}`
- `GET /api/greenbooks/payments`
- `GET /api/greenbooks/invoices/{id}/payments`

## Changes
1. **Projection pushdown**
   - Replaced `select=*` with explicit projected column lists per resource:
     - `GREENBOOKS_ACCOUNT_SELECT`
     - `GREENBOOKS_CUSTOMER_SELECT`
     - `GREENBOOKS_INVOICE_SELECT`
     - `GREENBOOKS_INVOICE_ITEM_SELECT`
     - `GREENBOOKS_PAYMENT_SELECT`

2. **Narrow typed DTOs (hybrid model)**
   - Added typed response DTOs for accounts/customers/invoices/items/payments.
   - Kept compatibility for unknown/admin-defined/additional fields via `#[serde(flatten)] extras: Map<String, Value>`.
   - Added generic parsing helpers (`parse_rows`, `parse_one`) to remove repetitive `Vec<Value>` shaping.

3. **Detail endpoint shaping**
   - `GET /api/greenbooks/invoices/{id}` now parses invoice + item rows into typed DTOs and attaches typed `items`.

4. **Regression test**
   - Added `greenbooks_invoice_dto_preserves_dynamic_fields` to ensure dynamic passthrough fields survive typed parsing.

## Validation
- `cargo fmt`
- `cargo check`
- `cargo test -p gateway` (all pass)

## Expected performance impact
- Lower payload bytes over wire due to projection pushdown (especially list endpoints).
- Reduced JSON re-shaping overhead in handlers by deserializing directly into typed structs.
- Better CPU/cache behavior vs repeated `serde_json::Value` map traversal.

## Tradeoffs
- DTO definitions increase maintenance when schema evolves.
- `extras` passthrough is retained to minimize contract risk and preserve forward compatibility.

## Notes
- Greenspot read paths (`/api/greenspot/*`) remain proxy-routed in this changeset; DB pushdown/typed migration there requires route cutover from wildcard proxy to Rust-native handlers to be safe.
