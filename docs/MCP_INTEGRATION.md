# Embedded MCP Integration Plan

> Status: **Spike complete** — the vendored rmcp SDK compiles and runs in-process.
> This document describes how we will integrate the MCP runtime into the gateway.

---

## 1. What the Spike Proved

| Checkpoint | Result |
|---|---|
| `rmcp` crate compiles from `vendor/mcp-rust-sdk` path dep | ✅ |
| `#[tool]` / `#[tool_router]` / `#[tool_handler]` macros work | ✅ |
| In-process duplex transport (`tokio::io::duplex`) connects client ↔ server | ✅ |
| Tool registration, listing, and invocation round-trip correctly | ✅ |
| Docker build (multi-stage, `rust:1.90-bookworm`) succeeds | ✅ |
| Release binary runs assertions inside container | ✅ |

Minimum Rust version: **1.90** (required by `darling` + `process-wrap` deps via rmcp).

---

## 2. Architecture Overview

```
┌──────────────────────────────────────────────────┐
│                    axum Gateway                   │
│  ┌──────────┐   ┌──────────────┐   ┌───────────┐│
│  │  AuthN   │──▶│ AuthZ / RBAC │──▶│  Router   ││
│  └──────────┘   └──────────────┘   └─────┬─────┘│
│                                          │       │
│                    ┌─────────────────────▼──┐    │
│                    │   MCP Dispatcher       │    │
│                    │  (embedded client)     │    │
│                    └──┬──────┬──────┬───────┘    │
│          duplex       │      │      │            │
│        ┌──────────────┘      │      └──────────┐ │
│        ▼                     ▼                  ▼ │
│  ┌───────────┐      ┌───────────┐      ┌──────┐ │
│  │ ToolSvc A │      │ ToolSvc B │      │ ...  │ │
│  │ (server)  │      │ (server)  │      │      │ │
│  └───────────┘      └───────────┘      └──────┘ │
└──────────────────────────────────────────────────┘
```

Each **ToolSvc** is an `impl ServerHandler` with its own tool registry.
The **MCP Dispatcher** holds an embedded client connected to each via `tokio::io::duplex`.

---

## 3. Transport: In-Process Duplex

The SDK supports multiple transports. For embedded use we need **zero-network overhead**:

```rust
// Create a bidirectional in-memory pipe
let (server_io, client_io) = tokio::io::duplex(buf_size);

// Server side
let server_handle = tool_service.serve(server_io).await?;

// Client side (the gateway's dispatcher)
let client = dispatcher_handler.serve(client_io).await?;

// Now: client.call_tool(...) → server handles it in-process
```

**Why duplex?**
- Zero serialization overhead (JSON-RPC over bytes, but no TCP/TLS).
- Each tool service gets its own isolated channel (no cross-contamination).
- Same API as external MCP servers — we can swap in child-process or HTTP
  transport later without changing tool implementations.

**Why not call `ServerHandler` directly?**
- The MCP protocol includes initialization handshake, capability negotiation,
  and progress/cancellation semantics. Going through the transport preserves
  all of these for free.
- Keeps the door open for moving tools to external processes (security boundary).

---

## 4. Key Interfaces

### 4.1 Tool Definition (server side)

Every tool service implements `ServerHandler` using the rmcp macros:

```rust
use rmcp::{
    ServerHandler, tool, tool_handler, tool_router,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{ServerCapabilities, ServerInfo},
    schemars,
};

#[derive(Debug, Clone)]
pub struct MyToolService {
    tool_router: ToolRouter<Self>,
    // ... any state the tools need (DB pools, HTTP clients, etc.)
}

#[tool_router]
impl MyToolService {
    pub fn new(/* deps */) -> Self {
        Self { tool_router: Self::tool_router() }
    }

    #[tool(description = "Do something useful")]
    async fn my_tool(&self, Parameters(req): Parameters<MyReq>) -> String {
        // tool logic
    }
}

#[tool_handler]
impl ServerHandler for MyToolService {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some("My tool service".into()),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }
}
```

**Schema auto-derivation**: The `schemars::JsonSchema` derive on request types
automatically generates JSON Schema for tool arguments. This feeds into:
- MCP `tools/list` responses (clients see schemas)
- Gateway-side input validation (pre-dispatch)

### 4.2 Tool Invocation (client / dispatcher side)

```rust
use rmcp::model::CallToolRequestParams;

let result = client.call_tool(CallToolRequestParams {
    meta: None,
    name: "my_tool".into(),
    arguments: Some(validated_args),
    task: None,
}).await?;
```

### 4.3 Tool Registry / Discovery

```rust
let tools = client.list_all_tools().await?;
// Returns Vec<Tool> with name, description, inputSchema
```

This is what the gateway will expose at its own `/tools/list` endpoint.

---

## 5. Integration Steps (Milestone Roadmap)

### M0 — Spike (this PR) ✅
- [x] Compile rmcp from vendored submodule
- [x] In-process duplex transport proof
- [x] Docker build proof
- [x] Integration design doc

### M1 — Gateway Skeleton
- [ ] `gateway` crate: axum HTTP server with health endpoint
- [ ] Embed one real tool service (e.g., echo)
- [ ] Wire dispatcher: startup creates duplex, connects client ↔ server
- [ ] Expose `POST /mcp/tools/list` and `POST /mcp/tools/call`
- [ ] Request ID propagation (tracing)

### M2 — Auth + RBAC
- [ ] AuthN middleware (BetterAuth session cookie + bearer JWT)
- [ ] Per-tool RBAC allowlists (deny-by-default)
- [ ] CSRF protection for cookie-auth writes
- [ ] Audit logging for tool invocations

### M3 — Security Hardening
- [ ] Tool-level timeouts + concurrency limits
- [ ] Egress controls per tool (SSRF/exfil prevention)
- [ ] Input validation (JSON Schema) before dispatch
- [ ] Output filtering / DLP hooks
- [ ] Rate limiting (IP + user + tool)

### M4 — External Tool Support
- [ ] Child-process transport (`transport-child-process` feature)
- [ ] HTTP/SSE transport for remote MCP servers
- [ ] Tool health checks + circuit breakers

---

## 6. Workspace Configuration Notes

The vendored SDK uses workspace inheritance (`edition = { workspace = true }`, etc.).
When consumed as a path dep from our workspace, cargo resolves these from **our** root
`Cargo.toml`. We mirror the SDK's `[workspace.package]` fields:

```toml
[workspace.package]
edition      = "2024"
version      = "0.15.0"
license      = "Apache-2.0"
license-file = "LICENSE"
# ... etc
```

Our own crates (gateway, etc.) set `edition = "2021"` explicitly, so they are
not affected by this.

---

## 7. Feature Flags We Use

| Feature | Why |
|---|---|
| `server` | `ServerHandler` trait + tool macros |
| `client` | `ClientHandler` trait + `call_tool` / `list_all_tools` |
| `macros` | `#[tool]`, `#[tool_router]`, `#[tool_handler]` |
| `transport-async-rw` | `tokio::io::duplex` → transport adapter |

Features we'll add later:
- `transport-child-process` — spawn external tool servers
- `transport-streamable-http-server` — expose MCP over HTTP/SSE
- `transport-streamable-http-client` — connect to remote MCP servers
- `auth` — OAuth2 for remote server auth

---

## 8. Open Questions

1. **Tool hot-reload**: Can we add/remove tool services at runtime without
   restarting the gateway? The duplex model supports this (spin up new
   channel), but we need a registry abstraction.

2. **Shared state across tools**: Some tools may need access to the same DB
   pool or cache. Decide: inject at construction, or use a shared `Arc<AppState>`?

3. **Error mapping**: rmcp returns `ServiceError` / `McpError`. Define our
   gateway error taxonomy and map MCP errors to HTTP status codes.

4. **Streaming results**: MCP supports progress notifications. Do we expose
   these as SSE to HTTP clients, or buffer until complete?
