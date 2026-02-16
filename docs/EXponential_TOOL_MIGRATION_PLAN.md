# Exponential tool migration plan (Option A shims)

This doc captures the **tool-by-tool** inventory for Exponential, using canonical naming:

- **issues → tasks**
- **cycles → sprints**

Option A (“shim”) means the gateway exposes these MCP tools but implements them by calling the existing Tools app HTTP APIs.

## Canonical tool surface

### Tasks

#### list_tasks
- Args: projectId?, assigneeId?, status?, sprintId?, teamId?, search?, includeArchived?, limit?, cursor?
- Upstream: `GET /api/exponential/tasks`

#### create_task
- Args: projectId, title, description?, status?, priority?, assigneeId?, sprintId?, dueAt?, labels?, milestone?, position?
- Upstream: `POST /api/exponential/tasks`
- Mapping (gateway → upstream body):
  - projectId → project_id
  - assigneeId → assignee_id
  - sprintId → sprint_id
  - dueAt → due_at

#### get_task
- Args: taskId
- Upstream: `GET /api/issues/:taskId`

#### update_task
- Args: taskId, title?, description?, status?, priority?, assigneeId?, sprintId?, dueAt?, labels?, milestone?, position?
- Upstream: `PATCH /api/exponential/tasks/:taskId`
- Mapping (gateway → upstream body):
  - assigneeId → assignee_id
  - sprintId → sprint_id
  - dueAt → due_at
  - action → action (archive/unarchive)

#### delete_task
- Args: taskId
- Upstream: `DELETE /api/issues/:taskId`

### Sprints

#### list_sprints
- Args: projectId?, state?
- Upstream: `GET /api/exponential/sprints`

#### create_sprint
- Args: projectId, name?, startDate?, endDate?
- Upstream: `POST /api/exponential/sprints`
- Mapping:
  - projectId → project_id
  - startDate → start_date
  - endDate → end_date

#### get_sprint
- Args: sprintId
- Upstream: `GET /api/exponential/sprints/:sprintId`

### Projects

#### list_projects
- Args: teamId?, includeArchived?
- Upstream: `GET /api/exponential/projects`

#### create_project
- Args: teamId, name, description?, color?, icon?, sprintDurationDays?, startDate?
- Upstream: `POST /api/exponential/projects`
- Mapping:
  - teamId → team_id
  - sprintDurationDays → sprint_duration_days
  - startDate → start_date

#### get_project
- Args: projectId
- Upstream: `GET /api/exponential/projects/:projectId`

### Audit logs

#### get_audit_logs
Kept as a gateway-native tool (not shimmed). It should query the gateway’s audit sink or in-memory ring buffer once implemented.

## Required egress
Option A shims require egress allowlist for:
- `tools.greenhatsec.com` (or the internal service host)

Env:
- `EXPONENTIAL_API_BASE_URL` (default: `https://tools.greenhatsec.com`)
- `EGRESS_ALLOWED_HOSTS` must include `tools.greenhatsec.com`

## Next implementation steps in gateway
1) Add JSONSchema files for each tool input.
2) Add policy entries (deny-by-default) for the tool allowlist.
3) Implement shim handlers that call the upstream endpoints with the mappings above.
4) Implement `get_audit_logs` tool to read gateway audit events.
