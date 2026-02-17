# Exponential Phase 1 Ledger (Gateway Ownership)

Date: 2026-02-16

## Why this exists
From product behavior, migration is already largely complete: frontend traffic is routed to `api.greenhatsec.com` via middleware rewrite.

What remains is **ownership cleanup**:
- Some Exponential endpoints are gateway-native tool routes.
- Some still proxy from gateway -> tools handlers.
- Legacy handlers still exist in `greenhat_tools/app/api/exponential/*`.

## Gateway route ownership (current)

### A) Gateway-native (tool-router)
- `GET /api/exponential/tasks`
- `POST /api/exponential/tasks`
- `GET /api/exponential/tasks/{task_id}`
- `PATCH /api/exponential/tasks/{task_id}`
- `DELETE /api/exponential/tasks/{task_id}`
- `GET /api/exponential/sprints`
- `POST /api/exponential/sprints`
- `GET /api/exponential/sprints/{sprint_id}`
- `GET /api/exponential/projects`
- `POST /api/exponential/projects`
- `GET /api/exponential/projects/{project_id}`

### B) Gateway proxy -> tools upstream
- `GET /api/exponential/tasks/{task_id}/comments`
- `GET /api/exponential/projects/{project_id}/tasks`
- `GET /api/exponential/projects/{project_id}/members`
- `GET /api/exponential/projects/{project_id}/permissions`
- `GET /api/exponential/teams`
- `GET /api/exponential/teams/{team_id}`
- `GET /api/exponential/teams/{team_id}/members`
- `GET /api/exponential/teams/{team_id}/permissions`

## Tools legacy handlers still present
`greenhat_tools/app/api/exponential/*` currently includes:
- labels
- my-tasks
- projects (+ [id], [id]/assignees, [id]/members, [id]/permissions, [id]/tasks)
- sprints (+ [id])
- tasks (+ [id], [id]/comments)
- teams (+ [id], [id]/members, [id]/permissions)
- views (+ [id])

Total files: 19 route handlers.

## Phase 1 outputs

### 1) Mapping freeze (this doc)
This file is the initial ownership map and baseline for removed-vs-retained decisions.

### 2) Cleanup candidates identified (NOT removed yet)
Candidate for gateway-native move next:
- team list/detail/members/permissions
- project members/permissions/tasks (read)
- task comments (read)

Candidate to keep legacy (temporarily) until parity is verified:
- labels
- views
- assignees
- my-tasks (if not yet covered by gateway-native path)

### 3) Safety constraints
- No deletions in Phase 1.
- Preserve auth/RBAC parity.
- Preserve request-id + Better Stack identity fields.

## Next step (Phase 2 prep)
For each candidate endpoint above:
1. Implement gateway-native handler.
2. Run local + prod smoke.
3. Confirm Better Stack request traces.
4. Mark corresponding tools handler as decommissionable.
