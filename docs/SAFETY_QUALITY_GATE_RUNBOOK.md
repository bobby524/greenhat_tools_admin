# Safety Quality Gate Runbook (Phase 7)

## Purpose
Single enforced pre-merge and pre-deploy gate for hardening controls.

Gate command:

```bash
make safety-gate
# or
./scripts/safety-quality-gate.sh
```

The gate runs, in order:
1. Runtime panic guard (`scripts/check-runtime-panics.sh`)
2. `cargo fmt --all -- --check`
3. `cargo check --workspace --all-targets`
4. `cargo test -p gateway -p mcp-spike`
5. Security smoke tests (`headers`, `csrf`, `rbac`, `egress`, `middleware`)

---

## Incident Response (gate failing)

1. **Freeze deploys**
   - Do not merge to `main` until green.
2. **Triage failing stage**
   - Panic guard: inspect diff for new `unwrap/expect/panic!` in runtime path.
   - fmt/check: fix compile/format regressions.
   - tests/smoke: identify broken control and failing assertion.
3. **Hotfix path**
   - Create fix branch.
   - Run `make safety-gate` locally.
   - Open PR with root cause + mitigation.
4. **Verify in CI**
   - Ensure `Safety Quality Gate` job passes before merge/deploy.

---

## Rollback

If a bad deploy passed gate but regressed runtime behavior:

1. Roll back Fly app to last known-good image release:
   ```bash
   flyctl releases -a greenhat-tools-admin-cmmugg
   flyctl releases rollback <VERSION> -a greenhat-tools-admin-cmmugg
   ```
2. Validate:
   - `/health` returns 200
   - Critical auth/policy/tool routes behave as expected
3. Open post-incident follow-up:
   - Add/extend smoke tests for missed regression class.
   - Tighten allowlist/policies if needed.

---

## Emergency bypass policy

Bypass is **not allowed** on protected `main`.
Any temporary override requires explicit human approval and a follow-up PR restoring strict gating immediately after incident stabilization.
