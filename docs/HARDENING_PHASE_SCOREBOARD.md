# Hardening Scoreboard (Phases 1-7)

## Status at a glance

| Phase | Enforcement delivered | Status | Residual risk |
|---|---|---|---|
| 1 | Baseline inventory + migration ledger (`docs/exponential-phase1-ledger.md`) | ✅ Enforced in process/docs | Drift if new tools/routes are added without ledger updates |
| 2 | API/tool surface reduction and policy tightening | ✅ Enforced in code/policy path | Coverage gaps possible for newly introduced endpoints |
| 3 | Removal ledger and cleanup (`docs/exponential-phase3-removal-ledger.md`) | ✅ Enforced via tracked removals | Dead code can re-enter without review discipline |
| 4 | Runtime defensive controls (auth/rbac/csrf/headers/egress middleware) | ✅ Enforced by integration tests | Misconfiguration risk in environment-specific deploys |
| 5 | Observability + auditability hardening | ✅ Enforced by structured logging/audit schema | Alerting depth depends on downstream log pipeline quality |
| 6 | Runtime panic guard (`scripts/check-runtime-panics.sh` + allowlist) | ✅ Enforced in CI | Guard scope is runtime path-focused, not full-code panic elimination |
| 7 | Unified Safety Quality Gate (`scripts/safety-quality-gate.sh`, `make safety-gate`) required in CI + deploy workflow | ✅ Enforced as required pre-publish/deploy gate | False confidence if smoke set is not updated with new risk areas |

## Phase 7 deliverables

- Single gate script chaining panic guard + fmt + compile checks + tests + key smoke validations.
- Top-level Make target: `make safety-gate`.
- CI job renamed to **Safety Quality Gate** and required before Docker publish.
- Fly deploy workflow now runs the same unified gate before deploy.
- Incident response + rollback runbook: `docs/SAFETY_QUALITY_GATE_RUNBOOK.md`.

## Next tightening opportunities

1. Add negative smoke tests for newest high-risk routes each sprint.
2. Promote smoke suite to tagged "must-pass" matrix for faster triage visibility.
3. Add policy/config drift detection between repo baseline and runtime environment.
