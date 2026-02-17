# Exponential Phase 3 Removal Ledger

Date: 2026-02-16 (completed)

## Outcome
- ✅ Tools-side `app/api/exponential/**` route handlers fully removed.
- ✅ Gateway owns Exponential API surface for reads/writes used by UI.
- ✅ Authenticated production smoke checks passed after each deletion batch.

## Removed from tools app
All previous Exponential route handlers were removed (19/19).

## Retained
- None in `tools` under `app/api/exponential/**`.

## Validation
- Gateway health and route checks passed.
- Gateway `http_request_complete` logs include `x_request_id`, `user_id`, and `roles`.
- Exponential UI paths (`projects`, `teams`, `views`, `tasks`, `sprints`) remained functional in authenticated checks.

## Post-cutover notes
- Some internal gateway legacy helper code remains (non-functional/dead-path cleanup can continue in follow-up).
- Canonical Exponential API execution is now gateway-first.
