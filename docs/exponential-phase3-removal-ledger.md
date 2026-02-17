# Exponential Phase 3 Removal Ledger

Date: 2026-02-16

## Current reality after Phase 2
- Frontend traffic is fully gateway-routed (`tools` middleware rewrites `/api/exponential/*` to `api.greenhatsec.com`).
- Gateway now has explicit handlers for core reads/writes and previously-proxied reads.
- **Important:** Gateway tool handlers currently call `tools.greenhatsec.com/api/exponential/*` as upstream for data execution.

That means tools-side Exponential handlers are still runtime dependencies until gateway execution path is moved off tools upstream.

## Remove vs Retain (this moment)

### Retain (required now)
- `/api/exponential/tasks`
- `/api/exponential/tasks/[id]`
- `/api/exponential/tasks/[id]/comments`
- `/api/exponential/projects`
- `/api/exponential/projects/[id]`
- `/api/exponential/projects/[id]/tasks`
- `/api/exponential/projects/[id]/members`
- `/api/exponential/projects/[id]/permissions`
- `/api/exponential/teams`
- `/api/exponential/teams/[id]`
- `/api/exponential/teams/[id]/members`
- `/api/exponential/teams/[id]/permissions`
- `/api/exponential/sprints`
- `/api/exponential/sprints/[id]`

Reason: directly called by gateway tool handlers today.

### Retain (UI-only paths still referenced)
- `/api/exponential/labels`
- `/api/exponential/views`
- `/api/exponential/views/[id]`
- `/api/exponential/projects/[id]/assignees`

Reason: frontend code still calls these and gateway does not yet have explicit owners for all of them.

### Candidate for later removal
- `/api/exponential/my-tasks`

Condition to remove:
1. Confirm zero runtime calls in logs for >=7 days and
2. Remove/replace regression tests expecting this route.

## Phase 3 execution sequence
1. Add explicit gateway owners for labels/views/assignees (if missing) with auth/RBAC parity.
2. Re-point gateway tool execution for Exponential off tools upstream (or keep explicit retained list if intentionally upstream-owned).
3. Delete only routes that are no longer runtime dependencies.
4. Re-run regression + smoke + Better Stack correlation.
