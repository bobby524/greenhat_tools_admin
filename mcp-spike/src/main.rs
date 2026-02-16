//! MCP SDK Compile-and-Run Spike
//!
//! Proves that:
//!   1. We can compile against the vendored rmcp crate (vendor/mcp-rust-sdk).
//!   2. The #[tool] / #[tool_router] / #[tool_handler] macros work.
//!   3. An in-process duplex transport connects client ↔ server with zero I/O.
//!   4. Tool invocation round-trips correctly.
//!
//! This is *not* production code. It is a minimal proof-of-concept.

use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{CallToolRequestParams, ClientInfo, ServerCapabilities, ServerInfo},
    schemars, tool, tool_handler, tool_router, ClientHandler, ServerHandler, ServiceExt,
};
use serde::Deserialize;
use tracing_subscriber::EnvFilter;

// ---------------------------------------------------------------------------
// Tool parameter schemas (auto-derived via schemars)
// ---------------------------------------------------------------------------

/// Input for the `echo` tool.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct EchoRequest {
    /// The text to echo back.
    message: String,
}

/// Input for the `add` tool.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct AddRequest {
    /// Left operand.
    a: i64,
    /// Right operand.
    b: i64,
}

// ---------------------------------------------------------------------------
// Server (tool registry)
// ---------------------------------------------------------------------------

/// A tiny MCP server that registers two tools: `echo` and `add`.
#[derive(Debug, Clone)]
struct SpikeServer {
    tool_router: ToolRouter<Self>,
}

#[tool_router]
impl SpikeServer {
    fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }

    /// Echo back the provided message (proves string args work).
    #[tool(description = "Echo back the provided message")]
    fn echo(&self, Parameters(req): Parameters<EchoRequest>) -> String {
        format!("echo: {}", req.message)
    }

    /// Add two integers (proves numeric args + computation work).
    #[tool(description = "Add two integers")]
    fn add(&self, Parameters(req): Parameters<AddRequest>) -> String {
        (req.a + req.b).to_string()
    }
}

#[tool_handler]
impl ServerHandler for SpikeServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some("MCP spike server — compile proof".into()),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }
}

// ---------------------------------------------------------------------------
// Client handler (minimal — just needs to satisfy the trait)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
struct SpikeClient;

impl ClientHandler for SpikeClient {
    fn get_info(&self) -> ClientInfo {
        ClientInfo {
            ..Default::default()
        }
    }
}

// ---------------------------------------------------------------------------
// Main — wire up duplex transport, invoke tools, verify results
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive(tracing::Level::INFO.into()))
        .with_writer(std::io::stderr)
        .init();

    println!("=== MCP SDK Spike ===");
    println!();

    // 1. Create an in-process duplex byte channel (no TCP, no stdio).
    //    This is the transport pattern we will use in the gateway.
    let (server_io, client_io) = tokio::io::duplex(4096);

    // 2. Start the server on one half of the duplex.
    let server = SpikeServer::new();
    let server_handle = tokio::spawn(async move {
        let svc = server.serve(server_io).await.expect("server serve failed");
        svc.waiting().await.expect("server waiting failed");
    });

    // 3. Start the client on the other half.
    let client = SpikeClient.serve(client_io).await?;

    // 4. List tools (proves the server registered them).
    let tools = client.list_all_tools().await?;
    println!("Registered tools ({}):", tools.len());
    for t in &tools {
        println!(
            "  - {} : {}",
            t.name,
            t.description.as_deref().unwrap_or("(no desc)")
        );
    }
    println!();

    // 5. Invoke `echo`.
    let echo_result = client
        .call_tool(CallToolRequestParams {
            meta: None,
            name: "echo".into(),
            arguments: Some(
                serde_json::json!({ "message": "hello from gateway spike" })
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
            task: None,
        })
        .await?;
    let echo_text = echo_result
        .content
        .first()
        .and_then(|c| c.raw.as_text())
        .map(|t| t.text.as_str())
        .unwrap_or("??");
    println!("echo result : {echo_text}");

    // 6. Invoke `add`.
    let add_result = client
        .call_tool(CallToolRequestParams {
            meta: None,
            name: "add".into(),
            arguments: Some(
                serde_json::json!({ "a": 17, "b": 25 })
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
            task: None,
        })
        .await?;
    let add_text = add_result
        .content
        .first()
        .and_then(|c| c.raw.as_text())
        .map(|t| t.text.as_str())
        .unwrap_or("??");
    println!("add result  : {add_text}");
    println!();

    // 7. Verify correctness.
    assert_eq!(echo_text, "echo: hello from gateway spike");
    assert_eq!(add_text, "42");
    println!("✅ All assertions passed. SDK compiles and runs in-process.");

    // Cleanup.
    drop(client);
    let _ = server_handle.await;

    Ok(())
}
