# Tool Execution Isolation

The gateway treats MCP tool calls as **untrusted workloads**. Even “safe” tools (like outbound HTTP) can be abused for SSRF, resource exhaustion, or data exfiltration.

This document describes the runtime isolation controls implemented in `gateway/src/tool_router.rs`.

## Goals

- Prevent a single caller/tool from consuming all gateway resources.
- Bound latency and memory usage under load.
- Fail closed by default (unknown/unlisted tools do not run).
- Emit audit events for all tool decisions (start/success/failure/rejection), including limit hits.

## What’s enforced today

### 1) Deny-by-default tool allowlist

`ToolRouter` only executes tools that are:

1. **Implemented** in the gateway runtime (hard-coded supported tools), and
2. **Allowed** by the tool runtime config (`ToolRuntimeConfig`).

Unimplemented tools are rejected with audit reason `tool_not_implemented`.

### 2) Bounded queue / backpressure

Tool calls are admitted into a bounded “queue” (implemented via a semaphore) that counts:

- requests waiting for permits, plus
- requests currently executing.

If `max_queue` is exhausted, the call is rejected immediately (`tool.invoke_rejected` with reason `queue_full`).

If a call is admitted but cannot acquire execution permits within `queue_timeout`, it is rejected (`queue_timeout`).

### 3) Global + per-tool concurrency caps

Two semaphores are acquired before a tool starts:

- **Global**: `max_concurrent_global`
- **Per-tool**: `max_concurrent` for the specific tool

This prevents:

- a traffic spike from exhausting all Tokio tasks/threads, and
- a single tool from starving all others.

### 4) Per-tool timeouts

Each tool execution is wrapped in a tool-level timeout (`timeout_ms`).

Notes:

- This is **independent** of the egress firewall’s `reqwest` timeouts.
- The shorter deadline wins.

### 5) Cancellation

If the caller provides a `CancellationToken` (typically tied to HTTP request lifetime), the tool router will:

- stop waiting for permits, and/or
- abort an in-flight tool call

with `tool.invoke_failure` error_kind `cancelled`.

### 6) Sandbox boundary: egress firewall

For outbound HTTP tools, all network access goes through `EgressClient` (`gateway/src/egress.rs`), which enforces:

- host allowlist (fail-closed when empty)
- private-IP denial (SSRF protection)
- request/response size caps
- connect + total request timeouts

## Configuration sources

### ToolRuntimeConfig (runtime)

`ToolRouter` is constructed with a `ToolRuntimeConfig`. By default, tests use conservative built-in values.

### Policy file (recommended)

`ToolRuntimeConfig::from_rbac_policy(&Policy)` can derive runtime bounds from the `tools` section of the RBAC policy file:

- `tools.max_concurrent_global`
- `tools.allowlist[tool].timeout_ms`
- `tools.allowlist[tool].max_concurrent`

Role-based tool access is still enforced by the RBAC middleware.

## Audit events

See `docs/AUDIT_EVENTS.md`.

Key events emitted by the tool router:

- `tool.invoke_start` (includes `timeout_ms` and `queue_wait_ms`)
- `tool.invoke_success`
- `tool.invoke_failure` (includes `error_kind` like `timeout`, `cancelled`, `egress_blocked`)
- `tool.invoke_rejected` (includes `reason` like `queue_full`, `queue_timeout`, `args_validation_failed`)
- `gateway.egress_blocked`
