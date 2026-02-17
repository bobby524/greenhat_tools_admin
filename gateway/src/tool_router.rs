//! Tool execution router — wires MCP tool calls through the egress firewall
//! and enforces **runtime isolation** (timeouts + concurrency + backpressure).
//!
//! This module currently provides a minimal tool runtime for integration tests
//! and early HTTP wiring.
//!
//! ## Resource-isolation features
//!
//! - **Deny-by-default tool allowlist** (unknown / unlisted tools rejected).
//! - **Global + per-tool concurrency limits** (Tokio semaphores).
//! - **Bounded request queue / backpressure** (max queued+running tool calls).
//! - **Per-tool timeouts** (tool-level deadline independent of egress timeouts).
//! - **Cancellation** (optional [`CancellationToken`] cooperatively aborts waits
//!   and in-flight tool work).
//!
//! ## Audit events emitted
//!
//! | Event type | When |
//! |---|---|
//! | `tool.invoke_start` | Tool dispatch starts (after admission + permit acquisition) |
//! | `tool.invoke_success` | Tool completed successfully |
//! | `tool.invoke_failure` | Tool failed (timeout, cancelled, egress error, etc.) |
//! | `tool.invoke_rejected` | Tool rejected before dispatch (unknown/disabled/queue full/etc.) |
//! | `gateway.egress_blocked` | Egress firewall blocked the outbound request |

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use bytes::Bytes;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue, AUTHORIZATION, CONTENT_TYPE, COOKIE};
use reqwest::Method;
use serde::{Deserialize, Serialize};
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;

use crate::audit::{hash_args, Actor, AuditEvent, AuditLog};
use crate::egress::{EgressClient, EgressError};

// ---------------------------------------------------------------------------
// Request / Response envelopes
// ---------------------------------------------------------------------------

/// Inbound tool call request (simplified).
#[derive(Debug, Deserialize)]
pub struct ToolRequest {
    /// Tool name (e.g. `"http_get"`, `"http_post"`).
    pub tool: String,
    /// Tool-specific parameters.
    pub params: serde_json::Value,
}

/// Outbound tool call result.
#[derive(Debug, Serialize)]
pub struct ToolResult {
    /// `true` when the tool executed successfully.
    pub success: bool,
    /// HTTP status code (if the tool made an outbound request).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<u16>,
    /// Body returned by the remote, or an error message.
    pub data: String,
}

// ---------------------------------------------------------------------------
// Audit context — passed in by the HTTP handler
// ---------------------------------------------------------------------------

/// Request-scoped context for audit event emission within tool calls.
#[derive(Debug, Clone, Default)]
pub struct ToolAuditCtx {
    pub request_id: String,
    pub source_ip: String,
    pub user_agent: Option<String>,
    pub actor: Option<Actor>,

    /// Optional Authorization header (Bearer JWT) from the incoming request.
    ///
    /// Exponential Option-A shims may forward this header upstream.
    pub upstream_authorization: Option<String>,

    /// Optional Cookie header from the incoming request.
    ///
    /// When the upstream Tools API authenticates via BetterAuth cookie, the gateway
    /// must forward the cookie header for Option-A shims.
    pub upstream_cookie: Option<String>,

    /// Optional cancellation token.
    ///
    /// If provided, the router will stop waiting for permits and abort
    /// in-flight tool execution when cancelled.
    pub cancel: Option<CancellationToken>,
}

// ---------------------------------------------------------------------------
// Runtime configuration
// ---------------------------------------------------------------------------

/// Default policy for tools absent from the runtime allowlist.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DefaultToolPolicy {
    Deny,
    Allow,
}

/// Runtime bounds for a single tool.
#[derive(Debug, Clone)]
pub struct ToolRuntimeToolConfig {
    pub enabled: bool,
    pub timeout: Duration,
    pub max_concurrent: usize,
}

/// Tool-runtime resource isolation configuration.
#[derive(Debug, Clone)]
pub struct ToolRuntimeConfig {
    /// Maximum number of tool calls admitted at once (queued + running).
    ///
    /// If this is exhausted, calls are rejected immediately (backpressure).
    pub max_queue: usize,

    /// Maximum time a request may wait to acquire execution permits once
    /// admitted. Exceeding this rejects the request (bounded queue).
    pub queue_timeout: Duration,

    /// Global max concurrent tool executions.
    pub max_concurrent_global: usize,

    /// Default tool timeout when not specified in `tools` and default policy
    /// allows.
    pub default_timeout: Duration,

    /// Default per-tool concurrency when not specified in `tools` and default
    /// policy allows.
    pub default_max_concurrent: usize,

    /// Whether tools not present in `tools` are denied or allowed.
    pub default_policy: DefaultToolPolicy,

    /// Per-tool runtime bounds.
    pub tools: HashMap<String, ToolRuntimeToolConfig>,
}

impl ToolRuntimeConfig {
    /// Conservative built-in defaults for the currently-supported tools.
    ///
    /// Note: allowlist defaults are here so that unit/integration tests can
    /// exercise the egress firewall without requiring a policy file.
    pub fn builtins() -> Self {
        let mut tools = HashMap::new();
        tools.insert(
            "http_get".into(),
            ToolRuntimeToolConfig {
                enabled: true,
                timeout: Duration::from_secs(30),
                max_concurrent: 8,
            },
        );
        tools.insert(
            "http_post".into(),
            ToolRuntimeToolConfig {
                enabled: true,
                timeout: Duration::from_secs(30),
                max_concurrent: 4,
            },
        );

        #[cfg(test)]
        {
            // Deterministic local pseudo-tool for exercising timeouts,
            // concurrency, queueing, and cancellation.
            tools.insert(
                "sleep".into(),
                ToolRuntimeToolConfig {
                    enabled: true,
                    timeout: Duration::from_secs(5),
                    max_concurrent: 1,
                },
            );
        }

        Self {
            max_queue: 64,
            queue_timeout: Duration::from_secs(2),
            max_concurrent_global: 32,
            default_timeout: Duration::from_secs(30),
            default_max_concurrent: 8,
            default_policy: DefaultToolPolicy::Deny,
            tools,
        }
    }

    /// Build tool runtime bounds from the RBAC policy file (if present).
    ///
    /// This only maps resource-bound fields (timeouts and concurrency). RBAC
    /// role checks are enforced separately in middleware.
    pub fn from_rbac_policy(policy: &crate::rbac::Policy) -> Option<Self> {
        use crate::rbac::policy::DefaultPolicy;

        let tools_policy = policy.tools.as_ref()?;

        let mut tools = HashMap::new();
        for (name, entry) in &tools_policy.allowlist {
            tools.insert(
                name.clone(),
                ToolRuntimeToolConfig {
                    enabled: entry.enabled,
                    timeout: Duration::from_millis(entry.timeout_ms),
                    max_concurrent: entry.max_concurrent as usize,
                },
            );
        }

        let default_policy = match tools_policy.default_policy {
            DefaultPolicy::Deny => DefaultToolPolicy::Deny,
            DefaultPolicy::Allow => DefaultToolPolicy::Allow,
        };

        // Queue sizing is not yet expressed in policy schema.
        // We keep conservative bounds and scale with global concurrency.
        let max_concurrent_global = tools_policy.max_concurrent_global.unwrap_or(32) as usize;

        Some(Self {
            max_queue: std::cmp::max(8, max_concurrent_global.saturating_mul(4)),
            queue_timeout: Duration::from_secs(2),
            max_concurrent_global,
            default_timeout: Duration::from_secs(30),
            default_max_concurrent: 8,
            default_policy,
            tools,
        })
    }
}

impl Default for ToolRuntimeConfig {
    fn default() -> Self {
        Self::builtins()
    }
}

// ---------------------------------------------------------------------------
// ToolRouter
// ---------------------------------------------------------------------------

struct ToolRuntime {
    cfg: ToolRuntimeConfig,
    queue: Arc<Semaphore>,
    global: Arc<Semaphore>,
    per_tool: HashMap<String, Arc<Semaphore>>, // tool_name → semaphore
}

impl ToolRuntime {
    fn new(cfg: ToolRuntimeConfig) -> Self {
        let queue = Arc::new(Semaphore::new(cfg.max_queue));
        let global = Arc::new(Semaphore::new(cfg.max_concurrent_global));

        let mut per_tool: HashMap<String, Arc<Semaphore>> = HashMap::new();
        for &tool in supported_tool_names() {
            let max = cfg
                .tools
                .get(tool)
                .map(|t| t.max_concurrent)
                .unwrap_or(cfg.default_max_concurrent);
            // `Semaphore::new(0)` is legal but would cause tasks to wait
            // forever. Treat 0 as "disabled" and still construct a 1-permit
            // semaphore to avoid surprises.
            let permits = std::cmp::max(1, max);
            per_tool.insert(tool.to_string(), Arc::new(Semaphore::new(permits)));
        }

        Self {
            cfg,
            queue,
            global,
            per_tool,
        }
    }

    fn resolve_tool(&self, tool: &str) -> Result<ResolvedTool, Reject> {
        if !is_supported_tool(tool) {
            return Err(Reject {
                reason: "tool_not_implemented",
                message: format!("unknown tool: {tool}"),
                extra: serde_json::json!({ "tool_name": tool }),
            });
        }

        match self.cfg.tools.get(tool) {
            Some(entry) => {
                if !entry.enabled {
                    return Err(Reject {
                        reason: "disabled",
                        message: format!("tool '{tool}' is disabled"),
                        extra: serde_json::json!({
                            "tool_name": tool,
                            "max_concurrent_tool": entry.max_concurrent,
                            "timeout_ms": entry.timeout.as_millis() as u64
                        }),
                    });
                }

                Ok(ResolvedTool {
                    timeout: entry.timeout,
                    max_concurrent_tool: entry.max_concurrent,
                    semaphore: self
                        .per_tool
                        .get(tool)
                        .cloned()
                        .expect("supported tool semaphore missing"),
                })
            }
            None => match self.cfg.default_policy {
                DefaultToolPolicy::Deny => Err(Reject {
                    reason: "tool_not_in_allowlist",
                    message: format!("tool '{tool}' not in allowlist"),
                    extra: serde_json::json!({ "tool_name": tool }),
                }),
                DefaultToolPolicy::Allow => Ok(ResolvedTool {
                    timeout: self.cfg.default_timeout,
                    max_concurrent_tool: self.cfg.default_max_concurrent,
                    semaphore: self
                        .per_tool
                        .get(tool)
                        .cloned()
                        .expect("supported tool semaphore missing"),
                }),
            },
        }
    }
}

#[derive(Clone)]
pub struct ToolRouter {
    egress: EgressClient,
    audit: Option<AuditLog>,
    runtime: Arc<ToolRuntime>,
}

impl ToolRouter {
    pub fn new(egress: EgressClient) -> Self {
        Self::new_with_config(egress, ToolRuntimeConfig::builtins())
    }

    pub fn new_with_config(egress: EgressClient, cfg: ToolRuntimeConfig) -> Self {
        Self {
            egress,
            audit: None,
            runtime: Arc::new(ToolRuntime::new(cfg)),
        }
    }

    /// Attach an audit log to the tool router.
    pub fn with_audit(mut self, audit: AuditLog) -> Self {
        self.audit = Some(audit);
        self
    }

    /// Borrow the runtime config.
    pub fn runtime_config(&self) -> &ToolRuntimeConfig {
        &self.runtime.cfg
    }

    /// List all tools supported by this gateway build.
    ///
    /// Note: whether a tool is actually **enabled** is governed by runtime
    /// allowlist/config + RBAC policy.
    pub fn supported_tool_names(&self) -> Vec<String> {
        supported_tool_names()
            .iter()
            .map(|s| (*s).to_owned())
            .collect()
    }

    /// Execute a tool call, enforcing allowlists, backpressure, concurrency
    /// caps, and timeouts.
    pub async fn execute(&self, req: ToolRequest, mut ctx: ToolAuditCtx) -> ToolResult {
        let cancel = ctx.cancel.take().unwrap_or_else(CancellationToken::new);

        // 1) Resolve tool runtime config / allowlist.
        let resolved = match self.runtime.resolve_tool(&req.tool) {
            Ok(r) => r,
            Err(reject) => {
                self.emit_rejected(&req, &ctx, reject.reason, Some(reject.extra));
                return ToolResult {
                    success: false,
                    status: None,
                    data: reject.message,
                };
            }
        };

        // 2) Validate args (fast, no I/O). Validation failures are rejections.
        let validated = match validate_args(&req.tool, &req.params, self.egress.config()) {
            Ok(v) => v,
            Err(msg) => {
                self.emit_rejected(
                    &req,
                    &ctx,
                    "args_validation_failed",
                    Some(serde_json::json!({ "detail": msg })),
                );
                return ToolResult {
                    success: false,
                    status: None,
                    data: msg,
                };
            }
        };

        // 2.5) Some shims require a bearer token to forward upstream.
        //
        // For fast local iteration, allow an explicit fallback bearer token from env when
        // the caller doesn't provide an Authorization header (e.g. using a BetterAuth cookie
        // session in the browser).
        if matches!(
            validated,
            ValidatedArgs::HttpRequest {
                require_bearer: true,
                ..
            }
        ) {
            // If we already have a cookie to forward upstream, we can authenticate upstream
            // without needing a bearer token.
            if ctx.upstream_authorization.is_none() && ctx.upstream_cookie.is_none() {
                if let Ok(tok) = std::env::var("EXPONENTIAL_UPSTREAM_BEARER") {
                    let t = tok.trim();
                    if !t.is_empty() {
                        ctx.upstream_authorization = Some(format!("Bearer {t}"));
                    }
                } else if let Ok(tok) = std::env::var("SUPABASE_SERVICE_ROLE_KEY") {
                    // Back-compat: reuse Tools' env var name.
                    let t = tok.trim();
                    if !t.is_empty() {
                        ctx.upstream_authorization = Some(format!("Bearer {t}"));
                    }
                }
            }

            if ctx.upstream_authorization.is_none() && ctx.upstream_cookie.is_none() {
                self.emit_rejected(
                    &req,
                    &ctx,
                    "missing_upstream_bearer",
                    Some(serde_json::json!({
                        "detail": "Authorization: Bearer <token> (or a forwarded session Cookie) required for upstream shim",
                    })),
                );
                return ToolResult {
                    success: false,
                    status: None,
                    data: "missing Authorization bearer token for upstream".into(),
                };
            }
        }

        // 3) Backpressure: bounded queue admission (queued + running).
        let queue_permit = match self.runtime.queue.clone().try_acquire_owned() {
            Ok(p) => p,
            Err(_) => {
                self.emit_rejected(
                    &req,
                    &ctx,
                    "queue_full",
                    Some(serde_json::json!({
                        "max_queue": self.runtime.cfg.max_queue,
                        "max_concurrent_global": self.runtime.cfg.max_concurrent_global,
                        "max_concurrent_tool": resolved.max_concurrent_tool
                    })),
                );
                return ToolResult {
                    success: false,
                    status: None,
                    data: "tool queue full; backpressure applied".into(),
                };
            }
        };

        // 4) Acquire execution permits (global + per-tool), with bounded wait.
        let wait_start = Instant::now();
        let acquire_fut = {
            let global = self.runtime.global.clone();
            let tool = resolved.semaphore.clone();
            async move {
                let gp = global
                    .acquire_owned()
                    .await
                    .expect("global semaphore closed");
                let tp = tool.acquire_owned().await.expect("tool semaphore closed");
                (gp, tp)
            }
        };

        let permits = tokio::select! {
            _ = cancel.cancelled() => {
                self.emit_rejected(
                    &req,
                    &ctx,
                    "cancelled",
                    Some(serde_json::json!({ "phase": "waiting_for_permits" })),
                );
                return ToolResult {
                    success: false,
                    status: None,
                    data: "cancelled".into(),
                };
            }
            r = tokio::time::timeout(self.runtime.cfg.queue_timeout, acquire_fut) => {
                match r {
                    Ok(p) => p,
                    Err(_) => {
                        self.emit_rejected(
                            &req,
                            &ctx,
                            "queue_timeout",
                            Some(serde_json::json!({
                                "queue_timeout_ms": self.runtime.cfg.queue_timeout.as_millis() as u64,
                                "queue_wait_ms": wait_start.elapsed().as_millis() as u64,
                                "max_concurrent_global": self.runtime.cfg.max_concurrent_global,
                                "max_concurrent_tool": resolved.max_concurrent_tool
                            })),
                        );
                        return ToolResult {
                            success: false,
                            status: None,
                            data: "timed out waiting for tool execution permits".into(),
                        };
                    }
                }
            }
        };

        let queue_wait_ms = wait_start.elapsed().as_millis() as u64;

        // 5) Emit invoke_start (dispatch is now starting).
        let args_hash = hash_args(&req.params);
        self.emit_start(&req, &ctx, &args_hash, resolved.timeout, queue_wait_ms);

        // 6) Run tool with per-tool timeout + cancellation.
        let start = Instant::now();

        let outcome: Outcome = tokio::select! {
            _ = cancel.cancelled() => Outcome::Cancelled,
            r = tokio::time::timeout(resolved.timeout, self.dispatch(validated, &ctx)) => {
                match r {
                    Ok(inner) => match inner {
                        Ok(ok) => Outcome::Ok(ok),
                        Err(e) => Outcome::EgressErr(e),
                    },
                    Err(_) => Outcome::Timeout,
                }
            }
        };

        // Ensure permits are held for the full duration of the tool execution.
        drop(permits);
        drop(queue_permit);

        let duration_ms = start.elapsed().as_millis() as u64;

        match outcome {
            Outcome::Ok(ok) => {
                let output_bytes = ok.data.len();
                self.emit_success(&req, &ctx, duration_ms, output_bytes);
                ToolResult {
                    success: true,
                    status: ok.status,
                    data: ok.data,
                }
            }
            Outcome::Timeout => {
                let msg = "tool timeout";
                self.emit_failure(&req, &ctx, duration_ms, "timeout", msg);
                ToolResult {
                    success: false,
                    status: None,
                    data: msg.into(),
                }
            }
            Outcome::Cancelled => {
                let msg = "cancelled";
                self.emit_failure(&req, &ctx, duration_ms, "cancelled", msg);
                ToolResult {
                    success: false,
                    status: None,
                    data: msg.into(),
                }
            }
            Outcome::EgressErr(e) => {
                let (kind, msg) = classify_egress_error(&e);
                if kind == "egress_blocked" {
                    self.emit_egress_blocked_from_error(&req, &ctx, &e);
                }
                self.emit_failure(&req, &ctx, duration_ms, kind, &msg);
                ToolResult {
                    success: false,
                    status: None,
                    data: msg,
                }
            }
        }
    }

    async fn dispatch(
        &self,
        validated: ValidatedArgs,
        ctx: &ToolAuditCtx,
    ) -> Result<ToolOk, EgressError> {
        match validated {
            ValidatedArgs::HttpGet { url } => {
                let resp = self.egress.request(Method::GET, &url, None).await?;
                Ok(ToolOk {
                    status: Some(resp.status),
                    data: String::from_utf8_lossy(&resp.body).into_owned(),
                })
            }
            ValidatedArgs::HttpPost { url, body } => {
                let resp = self.egress.request(Method::POST, &url, body).await?;
                Ok(ToolOk {
                    status: Some(resp.status),
                    data: String::from_utf8_lossy(&resp.body).into_owned(),
                })
            }
            ValidatedArgs::HttpRequest {
                method,
                url,
                body,
                content_type_json,
                require_bearer: _,
            } => {
                let mut headers = HeaderMap::new();

                // Prevent Tools middleware from rewriting this request back to
                // api.greenhatsec.com, which would create an infinite loop
                // (gateway → tools → gateway → tools → …).
                headers.insert(
                    HeaderName::from_static("x-gateway-internal"),
                    HeaderValue::from_static("1"),
                );

                if let Some(ref authz) = ctx.upstream_authorization {
                    if let Ok(v) = HeaderValue::from_str(authz) {
                        headers.insert(AUTHORIZATION, v);
                    }
                }
                if let Some(ref cookie) = ctx.upstream_cookie {
                    if let Ok(v) = HeaderValue::from_str(cookie) {
                        headers.insert(COOKIE, v);
                    }
                }
                if content_type_json {
                    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
                }

                let resp = self
                    .egress
                    .request_with_headers(method, &url, body, Some(headers))
                    .await?;

                Ok(ToolOk {
                    status: Some(resp.status),
                    data: String::from_utf8_lossy(&resp.body).into_owned(),
                })
            }
            ValidatedArgs::GetAuditLogs => {
                let path = std::env::var("AUDIT_LOG_FILE").unwrap_or_default();
                let path = path.trim().to_owned();
                if path.is_empty() {
                    return Ok(ToolOk {
                        status: None,
                        data: "AUDIT_LOG_FILE not set; audit log query not available".into(),
                    });
                }

                let text = match tokio::fs::read_to_string(&path).await {
                    Ok(t) => t,
                    Err(e) => {
                        return Ok(ToolOk {
                            status: None,
                            data: format!("failed to read audit log file: {e}"),
                        })
                    }
                };

                let mut lines: Vec<&str> = text.lines().collect();
                let take = 50usize;
                if lines.len() > take {
                    lines = lines[lines.len() - take..].to_vec();
                }

                Ok(ToolOk {
                    status: None,
                    data: lines.join("\n"),
                })
            }
            #[cfg(test)]
            ValidatedArgs::Sleep { ms } => {
                tokio::time::sleep(Duration::from_millis(ms)).await;
                Ok(ToolOk {
                    status: None,
                    data: format!("slept {ms}ms"),
                })
            }
        }
    }

    // ── Audit emission helpers ───────────────────────────────────────────

    fn emit_start(
        &self,
        req: &ToolRequest,
        ctx: &ToolAuditCtx,
        args_hash: &str,
        timeout: Duration,
        queue_wait_ms: u64,
    ) {
        if let Some(ref audit) = self.audit {
            let mut evt = AuditEvent::new(
                "tool.invoke_start",
                &ctx.request_id,
                &ctx.source_ip,
                ctx.actor.clone(),
                serde_json::json!({
                    "tool_name": req.tool,
                    "args_hash": args_hash,
                    "timeout_ms": timeout.as_millis() as u64,
                    "queue_wait_ms": queue_wait_ms,
                }),
            );
            if let Some(ua) = &ctx.user_agent {
                evt = evt.with_user_agent(ua);
            }
            audit.emit(evt);
        }
    }

    fn emit_success(
        &self,
        req: &ToolRequest,
        ctx: &ToolAuditCtx,
        duration_ms: u64,
        output_bytes: usize,
    ) {
        if let Some(ref audit) = self.audit {
            audit.emit(AuditEvent::new(
                "tool.invoke_success",
                &ctx.request_id,
                &ctx.source_ip,
                ctx.actor.clone(),
                serde_json::json!({
                    "tool_name": req.tool,
                    "duration_ms": duration_ms,
                    "output_bytes": output_bytes,
                }),
            ));
        }
    }

    fn emit_failure(
        &self,
        req: &ToolRequest,
        ctx: &ToolAuditCtx,
        duration_ms: u64,
        error_kind: &str,
        error_message: &str,
    ) {
        if let Some(ref audit) = self.audit {
            audit.emit(AuditEvent::new(
                "tool.invoke_failure",
                &ctx.request_id,
                &ctx.source_ip,
                ctx.actor.clone(),
                serde_json::json!({
                    "tool_name": req.tool,
                    "duration_ms": duration_ms,
                    "error_kind": error_kind,
                    "error_message": error_message,
                }),
            ));
        }
    }

    fn emit_rejected(
        &self,
        req: &ToolRequest,
        ctx: &ToolAuditCtx,
        reason: &str,
        extra: Option<serde_json::Value>,
    ) {
        if let Some(ref audit) = self.audit {
            let mut payload = serde_json::json!({
                "tool_name": req.tool,
                "reason": reason,
            });
            if let Some(extra) = extra {
                if let Some(obj) = payload.as_object_mut() {
                    if let Some(extra_obj) = extra.as_object() {
                        for (k, v) in extra_obj {
                            obj.insert(k.clone(), v.clone());
                        }
                    } else {
                        obj.insert("extra".into(), extra);
                    }
                }
            }
            audit.emit(AuditEvent::new(
                "tool.invoke_rejected",
                &ctx.request_id,
                &ctx.source_ip,
                ctx.actor.clone(),
                payload,
            ));
        }
    }

    fn emit_egress_blocked_from_error(
        &self,
        req: &ToolRequest,
        ctx: &ToolAuditCtx,
        e: &EgressError,
    ) {
        if let Some(ref audit) = self.audit {
            let (target_host, target_port) = req
                .params
                .get("url")
                .and_then(|v| v.as_str())
                .and_then(|u| url::Url::parse(u).ok())
                .map(|u| {
                    let host = u.host_str().unwrap_or("unknown").to_owned();
                    let port = u.port_or_known_default().unwrap_or(443);
                    (host, port)
                })
                .unwrap_or_else(|| ("unknown".into(), 0));

            audit.emit(AuditEvent::new(
                "gateway.egress_blocked",
                &ctx.request_id,
                &ctx.source_ip,
                ctx.actor.clone(),
                serde_json::json!({
                    "tool_name": req.tool,
                    "target_host": target_host,
                    "target_port": target_port,
                    "reason": e.to_string(),
                }),
            ));
        }
    }
}

// ---------------------------------------------------------------------------
// Validation + dispatch helpers
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct ToolOk {
    status: Option<u16>,
    data: String,
}

#[derive(Debug)]
struct ResolvedTool {
    timeout: Duration,
    max_concurrent_tool: usize,
    semaphore: Arc<Semaphore>,
}

#[derive(Debug)]
struct Reject {
    reason: &'static str,
    message: String,
    extra: serde_json::Value,
}

#[derive(Debug)]
enum ValidatedArgs {
    HttpGet {
        url: String,
    },
    HttpPost {
        url: String,
        body: Option<Bytes>,
    },

    /// Generic HTTP request used by higher-level shims (Exponential tools).
    HttpRequest {
        method: Method,
        url: String,
        body: Option<Bytes>,
        content_type_json: bool,
        require_bearer: bool,
    },

    /// Read recent gateway audit events (best-effort; v0 uses AUDIT_LOG_FILE).
    GetAuditLogs,

    #[cfg(test)]
    Sleep {
        ms: u64,
    },
}

fn supported_tool_names() -> &'static [&'static str] {
    #[cfg(test)]
    {
        return &[
            "http_get",
            "http_post",
            "sleep",
            // Exponential shims
            "list_tasks",
            "create_task",
            "get_task",
            "update_task",
            "delete_task",
            "list_sprints",
            "create_sprint",
            "get_sprint",
            "list_projects",
            "create_project",
            "get_project",
            "list_teams",
            "get_team",
            "get_project_tasks",
            "get_project_members",
            "get_project_permissions",
            "get_team_members",
            "get_team_permissions",
            "get_task_comments",
            // Greenspot shims
            "list_greenspot_contacts",
            "create_greenspot_contact",
            "get_greenspot_contact",
            "list_greenspot_companies",
            "create_greenspot_company",
            "get_greenspot_company",
            "list_greenspot_deals",
            "create_greenspot_deal",
            "get_greenspot_deal",
            "list_greenspot_tasks",
            "create_greenspot_task",
            "get_greenspot_task",
            // GreenBooks shims
            "list_greenbooks_accounts",
            "get_greenbooks_account",
            "list_greenbooks_customers",
            "get_greenbooks_customer",
            "list_greenbooks_vendors",
            "get_greenbooks_vendor",
            "list_greenbooks_invoices",
            "get_greenbooks_invoice",
            "create_greenbooks_invoice",
            "list_greenbooks_bills",
            "get_greenbooks_bill",
            "create_greenbooks_bill",
            "list_greenbooks_journal_entries",
            "get_greenbooks_journal_entry",
            "create_greenbooks_journal_entry",
            "list_greenbooks_bank_accounts",
            "get_greenbooks_bank_account",
            "list_greenbooks_reports",
            "post_greenbooks_invoice_gl",
            "post_greenbooks_bill_gl",
            "post_greenbooks_journal_entry",
            "create_greenbooks_invoice_payment",
            "create_greenbooks_bill_payment",
            "create_greenbooks_bank_transfer",
            "post_greenbooks_bank_reconcile",
            "list_greenbooks_bank_transactions",
            "list_greenbooks_payments",
            "list_greenbooks_currencies",
            "list_greenbooks_tax_codes",
            "create_greenbooks_tax_code",
            "get_greenbooks_tax_code",
            "update_greenbooks_tax_code",
            "list_greenbooks_fx_rates",
            "create_greenbooks_fx_rate",
            "update_greenbooks_fx_rate",
            "delete_greenbooks_fx_rate",
            "convert_greenbooks_fx",
            "run_greenbooks_fx_revaluation",
            "list_greenbooks_fiscal_periods",
            "create_greenbooks_fiscal_period",
            "get_greenbooks_fiscal_period",
            "update_greenbooks_fiscal_period",
            "delete_greenbooks_fiscal_period",
            "check_greenbooks_fiscal_lock",
            "list_greenbooks_items",
            "create_greenbooks_item",
            "update_greenbooks_item",
            "update_greenbooks_invoice",
            "post_greenbooks_invoice",
            "update_greenbooks_bill",
            "update_greenbooks_customer",
            "delete_greenbooks_customer",
            "get_greenbooks_customer_statement",
            "get_greenbooks_customer_hub",
            "update_greenbooks_vendor",
            "list_greenbooks_recurring",
            "create_greenbooks_recurring",
            "create_greenbooks_bank_account",
            "get_greenbooks_bank_account_reconciliations",
            "patch_greenbooks_bank_reconcile",
            "update_greenbooks_bank_account",
            "create_greenbooks_bank_transaction",
            "get_greenbooks_account_ledger",
            "create_greenbooks_account",
            "update_greenbooks_account",
            "import_greenbooks_accounts",
            "export_greenbooks_accounts",
            "list_greenbooks_audit_events",
            "get_greenbooks_audit_entity",
            "import_greenbooks_quickbooks",
            "get_greenbooks_settings",
            "update_greenbooks_settings",
            "list_greenbooks_statements",
            "get_greenbooks_crm_link",
            "sync_greenbooks_crm_link",
            "get_greenbooks_report_aging",
            "get_greenbooks_report_ap_aging",
            "get_greenbooks_report_fx_summary",
            "get_greenbooks_report_gst_summary",
            "export_greenbooks_reports",
            // Gateway-native
            "get_audit_logs",
        ];
    }
    #[cfg(not(test))]
    {
        return &[
            "http_get",
            "http_post",
            // Exponential shims
            "list_tasks",
            "create_task",
            "get_task",
            "update_task",
            "delete_task",
            "list_sprints",
            "create_sprint",
            "get_sprint",
            "list_projects",
            "create_project",
            "get_project",
            "list_teams",
            "get_team",
            "get_project_tasks",
            "get_project_members",
            "get_project_permissions",
            "get_team_members",
            "get_team_permissions",
            "get_task_comments",
            // Greenspot shims
            "list_greenspot_contacts",
            "create_greenspot_contact",
            "get_greenspot_contact",
            "list_greenspot_companies",
            "create_greenspot_company",
            "get_greenspot_company",
            "list_greenspot_deals",
            "create_greenspot_deal",
            "get_greenspot_deal",
            "list_greenspot_tasks",
            "create_greenspot_task",
            "get_greenspot_task",
            // GreenBooks shims
            "list_greenbooks_accounts",
            "get_greenbooks_account",
            "list_greenbooks_customers",
            "get_greenbooks_customer",
            "list_greenbooks_vendors",
            "get_greenbooks_vendor",
            "list_greenbooks_invoices",
            "get_greenbooks_invoice",
            "create_greenbooks_invoice",
            "list_greenbooks_bills",
            "get_greenbooks_bill",
            "create_greenbooks_bill",
            "list_greenbooks_journal_entries",
            "get_greenbooks_journal_entry",
            "create_greenbooks_journal_entry",
            "list_greenbooks_bank_accounts",
            "get_greenbooks_bank_account",
            "list_greenbooks_reports",
            "post_greenbooks_invoice_gl",
            "post_greenbooks_bill_gl",
            "post_greenbooks_journal_entry",
            "create_greenbooks_invoice_payment",
            "create_greenbooks_bill_payment",
            "create_greenbooks_bank_transfer",
            "post_greenbooks_bank_reconcile",
            "list_greenbooks_bank_transactions",
            "list_greenbooks_payments",
            "list_greenbooks_currencies",
            "list_greenbooks_tax_codes",
            "create_greenbooks_tax_code",
            "get_greenbooks_tax_code",
            "update_greenbooks_tax_code",
            "list_greenbooks_fx_rates",
            "create_greenbooks_fx_rate",
            "update_greenbooks_fx_rate",
            "delete_greenbooks_fx_rate",
            "convert_greenbooks_fx",
            "run_greenbooks_fx_revaluation",
            "list_greenbooks_fiscal_periods",
            "create_greenbooks_fiscal_period",
            "get_greenbooks_fiscal_period",
            "update_greenbooks_fiscal_period",
            "delete_greenbooks_fiscal_period",
            "check_greenbooks_fiscal_lock",
            "list_greenbooks_items",
            "create_greenbooks_item",
            "update_greenbooks_item",
            "update_greenbooks_invoice",
            "post_greenbooks_invoice",
            "update_greenbooks_bill",
            "update_greenbooks_customer",
            "delete_greenbooks_customer",
            "get_greenbooks_customer_statement",
            "get_greenbooks_customer_hub",
            "update_greenbooks_vendor",
            "list_greenbooks_recurring",
            "create_greenbooks_recurring",
            "create_greenbooks_bank_account",
            "get_greenbooks_bank_account_reconciliations",
            "patch_greenbooks_bank_reconcile",
            "update_greenbooks_bank_account",
            "create_greenbooks_bank_transaction",
            "get_greenbooks_account_ledger",
            "create_greenbooks_account",
            "update_greenbooks_account",
            "import_greenbooks_accounts",
            "export_greenbooks_accounts",
            "list_greenbooks_audit_events",
            "get_greenbooks_audit_entity",
            "import_greenbooks_quickbooks",
            "get_greenbooks_settings",
            "update_greenbooks_settings",
            "list_greenbooks_statements",
            "get_greenbooks_crm_link",
            "sync_greenbooks_crm_link",
            "get_greenbooks_report_aging",
            "get_greenbooks_report_ap_aging",
            "get_greenbooks_report_fx_summary",
            "get_greenbooks_report_gst_summary",
            "export_greenbooks_reports",
            // Gateway-native
            "get_audit_logs",
        ];
    }
}

fn is_supported_tool(tool: &str) -> bool {
    supported_tool_names().iter().any(|t| *t == tool)
}

fn exponential_base_url() -> String {
    std::env::var("EXPONENTIAL_API_BASE_URL")
        .ok()
        .map(|s| s.trim().trim_end_matches('/').to_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "https://tools.greenhatsec.com".to_owned())
}

fn validate_args(
    tool: &str,
    params: &serde_json::Value,
    egress_cfg: &crate::egress::EgressConfig,
) -> Result<ValidatedArgs, String> {
    let base = exponential_base_url();

    let mut make_url = |path: &str| -> Result<String, String> {
        let url = format!("{base}{path}");
        url::Url::parse(&url).map_err(|_| "invalid url".to_string())?;
        Ok(url)
    };

    let json_body = |v: serde_json::Value| {
        let s = serde_json::to_string(&v).expect("json serialize");
        Bytes::from(s)
    };

    match tool {
        "http_get" => {
            let url = params
                .get("url")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "missing required param: url".to_string())?;
            url::Url::parse(url).map_err(|_| "invalid url".to_string())?;
            Ok(ValidatedArgs::HttpGet { url: url.into() })
        }
        "http_post" => {
            let url = params
                .get("url")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "missing required param: url".to_string())?;
            url::Url::parse(url).map_err(|_| "invalid url".to_string())?;

            let body = params
                .get("body")
                .and_then(|v| v.as_str())
                .map(|s| Bytes::from(s.to_owned()));

            if let Some(ref b) = body {
                if b.len() > egress_cfg.max_request_body_bytes {
                    return Err(format!(
                        "request body {} bytes exceeds max {}",
                        b.len(),
                        egress_cfg.max_request_body_bytes
                    ));
                }
            }

            Ok(ValidatedArgs::HttpPost {
                url: url.into(),
                body,
            })
        }

        // --- Gateway-native -------------------------------------------------
        "get_audit_logs" => Ok(ValidatedArgs::GetAuditLogs),

        // --- Exponential Option-A shims (canonical naming) ------------------
        "list_tasks" => {
            let mut url = url::Url::parse(&make_url("/api/exponential/tasks")?).unwrap();
            {
                let mut qp = url.query_pairs_mut();

                // Greenhat Tools Exponential v2 query params
                if let Some(v) = params.get("projectId").and_then(|v| v.as_str()) {
                    qp.append_pair("projectId", v);
                }
                if let Some(v) = params.get("assigneeId").and_then(|v| v.as_str()) {
                    qp.append_pair("assigneeId", v);
                }
                if let Some(v) = params.get("status").and_then(|v| v.as_str()) {
                    qp.append_pair("status", v);
                }
                if let Some(v) = params.get("sprintId").and_then(|v| v.as_str()) {
                    qp.append_pair("sprintId", v);
                }
                if let Some(v) = params.get("teamId").and_then(|v| v.as_str()) {
                    qp.append_pair("teamId", v);
                }
                if let Some(v) = params.get("search").and_then(|v| v.as_str()) {
                    qp.append_pair("search", v);
                }
                if let Some(v) = params.get("includeArchived").and_then(|v| v.as_bool()) {
                    qp.append_pair("includeArchived", if v { "true" } else { "false" });
                }
                if let Some(v) = params.get("limit").and_then(|v| v.as_u64()) {
                    qp.append_pair("limit", &v.to_string());
                }
                if let Some(v) = params.get("cursor").and_then(|v| v.as_str()) {
                    qp.append_pair("cursor", v);
                }
            }
            Ok(ValidatedArgs::HttpRequest {
                method: Method::GET,
                url: url.to_string(),
                body: None,
                content_type_json: false,
                require_bearer: true,
            })
        }

        "create_task" => {
            let project_id = params
                .get("projectId")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "missing required param: projectId".to_string())?;
            let title = params
                .get("title")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "missing required param: title".to_string())?;

            // Body matches Greenhat Tools `/api/exponential/tasks` (snake_case)
            let mut body = serde_json::json!({
                "project_id": project_id,
                "title": title,
            });

            if let Some(v) = params.get("description").and_then(|v| v.as_str()) {
                body["description"] = serde_json::Value::String(v.to_owned());
            }
            if let Some(v) = params.get("status").and_then(|v| v.as_str()) {
                body["status"] = serde_json::Value::String(v.to_owned());
            }
            if let Some(v) = params.get("priority").and_then(|v| v.as_i64()) {
                body["priority"] = serde_json::Value::Number(v.into());
            }
            if let Some(v) = params.get("assigneeId").and_then(|v| v.as_str()) {
                body["assignee_id"] = serde_json::Value::String(v.to_owned());
            }
            if let Some(v) = params.get("sprintId").and_then(|v| v.as_str()) {
                body["sprint_id"] = serde_json::Value::String(v.to_owned());
            }
            if let Some(v) = params.get("dueAt").and_then(|v| v.as_str()) {
                body["due_at"] = serde_json::Value::String(v.to_owned());
            }
            if let Some(v) = params.get("labels") {
                body["labels"] = v.clone();
            }
            if let Some(v) = params.get("milestone").and_then(|v| v.as_str()) {
                body["milestone"] = serde_json::Value::String(v.to_owned());
            }
            if let Some(v) = params.get("position").and_then(|v| v.as_i64()) {
                body["position"] = serde_json::Value::Number(v.into());
            }

            let body_bytes = json_body(body);
            if body_bytes.len() > egress_cfg.max_request_body_bytes {
                return Err(format!(
                    "request body {} bytes exceeds max {}",
                    body_bytes.len(),
                    egress_cfg.max_request_body_bytes
                ));
            }

            Ok(ValidatedArgs::HttpRequest {
                method: Method::POST,
                url: make_url("/api/exponential/tasks")?,
                body: Some(body_bytes),
                content_type_json: true,
                require_bearer: true,
            })
        }

        "get_task" => {
            let task_id = params
                .get("taskId")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "missing required param: taskId".to_string())?;
            Ok(ValidatedArgs::HttpRequest {
                method: Method::GET,
                url: make_url(&format!("/api/exponential/tasks/{task_id}"))?,
                body: None,
                content_type_json: false,
                require_bearer: true,
            })
        }

        "update_task" => {
            let task_id = params
                .get("taskId")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "missing required param: taskId".to_string())?;

            let mut body = serde_json::Map::new();
            if let Some(v) = params.get("title").and_then(|v| v.as_str()) {
                body.insert("title".into(), serde_json::Value::String(v.to_owned()));
            }
            if let Some(v) = params.get("description").and_then(|v| v.as_str()) {
                body.insert(
                    "description".into(),
                    serde_json::Value::String(v.to_owned()),
                );
            }
            if let Some(v) = params.get("status").and_then(|v| v.as_str()) {
                body.insert("status".into(), serde_json::Value::String(v.to_owned()));
            }
            if let Some(v) = params.get("priority").and_then(|v| v.as_i64()) {
                body.insert("priority".into(), serde_json::Value::Number(v.into()));
            }
            if let Some(v) = params.get("assigneeId").and_then(|v| v.as_str()) {
                body.insert(
                    "assignee_id".into(),
                    serde_json::Value::String(v.to_owned()),
                );
            }
            if let Some(v) = params.get("projectId").and_then(|v| v.as_str()) {
                body.insert("project_id".into(), serde_json::Value::String(v.to_owned()));
            }
            if let Some(v) = params.get("sprintId").and_then(|v| v.as_str()) {
                body.insert("sprint_id".into(), serde_json::Value::String(v.to_owned()));
            }
            if let Some(v) = params.get("dueAt").and_then(|v| v.as_str()) {
                body.insert("due_at".into(), serde_json::Value::String(v.to_owned()));
            }
            if let Some(v) = params.get("labels") {
                body.insert("labels".into(), v.clone());
            }
            if let Some(v) = params.get("milestone").and_then(|v| v.as_str()) {
                body.insert("milestone".into(), serde_json::Value::String(v.to_owned()));
            }
            if let Some(v) = params.get("position").and_then(|v| v.as_i64()) {
                body.insert("position".into(), serde_json::Value::Number(v.into()));
            }
            if let Some(v) = params.get("action").and_then(|v| v.as_str()) {
                body.insert("action".into(), serde_json::Value::String(v.to_owned()));
            }

            let body_bytes = json_body(serde_json::Value::Object(body));
            if body_bytes.len() > egress_cfg.max_request_body_bytes {
                return Err(format!(
                    "request body {} bytes exceeds max {}",
                    body_bytes.len(),
                    egress_cfg.max_request_body_bytes
                ));
            }

            Ok(ValidatedArgs::HttpRequest {
                method: Method::PATCH,
                url: make_url(&format!("/api/exponential/tasks/{task_id}"))?,
                body: Some(body_bytes),
                content_type_json: true,
                require_bearer: true,
            })
        }

        "delete_task" => {
            let task_id = params
                .get("taskId")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "missing required param: taskId".to_string())?;
            Ok(ValidatedArgs::HttpRequest {
                method: Method::DELETE,
                url: make_url(&format!("/api/exponential/tasks/{task_id}"))?,
                body: None,
                content_type_json: false,
                require_bearer: true,
            })
        }

        "list_sprints" => {
            let mut url = url::Url::parse(&make_url("/api/exponential/sprints")?).unwrap();
            {
                let mut qp = url.query_pairs_mut();

                // Greenhat Tools Exponential v2 query params
                if let Some(v) = params.get("projectId").and_then(|v| v.as_str()) {
                    qp.append_pair("projectId", v);
                }
                if let Some(v) = params.get("state").and_then(|v| v.as_str()) {
                    qp.append_pair("state", v);
                }
            }
            Ok(ValidatedArgs::HttpRequest {
                method: Method::GET,
                url: url.to_string(),
                body: None,
                content_type_json: false,
                require_bearer: true,
            })
        }

        "create_sprint" => {
            let project_id = params
                .get("projectId")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "missing required param: projectId".to_string())?;

            let mut body = serde_json::json!({
                "project_id": project_id,
            });

            if let Some(v) = params.get("name").and_then(|v| v.as_str()) {
                body["name"] = serde_json::Value::String(v.to_owned());
            }
            if let Some(v) = params.get("startDate").and_then(|v| v.as_str()) {
                body["start_date"] = serde_json::Value::String(v.to_owned());
            }
            if let Some(v) = params.get("endDate").and_then(|v| v.as_str()) {
                body["end_date"] = serde_json::Value::String(v.to_owned());
            }

            let body_bytes = json_body(body);
            if body_bytes.len() > egress_cfg.max_request_body_bytes {
                return Err(format!(
                    "request body {} bytes exceeds max {}",
                    body_bytes.len(),
                    egress_cfg.max_request_body_bytes
                ));
            }

            Ok(ValidatedArgs::HttpRequest {
                method: Method::POST,
                url: make_url("/api/exponential/sprints")?,
                body: Some(body_bytes),
                content_type_json: true,
                require_bearer: true,
            })
        }

        "get_sprint" => {
            let sprint_id = params
                .get("sprintId")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "missing required param: sprintId".to_string())?;
            Ok(ValidatedArgs::HttpRequest {
                method: Method::GET,
                url: make_url(&format!("/api/exponential/sprints/{sprint_id}"))?,
                body: None,
                content_type_json: false,
                require_bearer: true,
            })
        }

        "list_projects" => {
            let mut url = url::Url::parse(&make_url("/api/exponential/projects")?).unwrap();
            {
                let mut qp = url.query_pairs_mut();

                // Greenhat Tools Exponential v2 query params
                if let Some(v) = params.get("teamId").and_then(|v| v.as_str()) {
                    qp.append_pair("teamId", v);
                }
                if let Some(v) = params.get("includeArchived").and_then(|v| v.as_bool()) {
                    qp.append_pair("includeArchived", if v { "true" } else { "false" });
                }
            }
            Ok(ValidatedArgs::HttpRequest {
                method: Method::GET,
                url: url.to_string(),
                body: None,
                content_type_json: false,
                require_bearer: true,
            })
        }

        "create_project" => {
            let team_id = params
                .get("teamId")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "missing required param: teamId".to_string())?;
            let name = params
                .get("name")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "missing required param: name".to_string())?;

            let mut body = serde_json::json!({
                "team_id": team_id,
                "name": name,
            });
            if let Some(v) = params.get("description").and_then(|v| v.as_str()) {
                body["description"] = serde_json::Value::String(v.to_owned());
            }
            if let Some(v) = params.get("color").and_then(|v| v.as_str()) {
                body["color"] = serde_json::Value::String(v.to_owned());
            }
            if let Some(v) = params.get("icon").and_then(|v| v.as_str()) {
                body["icon"] = serde_json::Value::String(v.to_owned());
            }
            if let Some(v) = params.get("sprintDurationDays").and_then(|v| v.as_i64()) {
                body["sprint_duration_days"] = serde_json::Value::Number(v.into());
            }
            if let Some(v) = params.get("startDate").and_then(|v| v.as_str()) {
                body["start_date"] = serde_json::Value::String(v.to_owned());
            }

            let body_bytes = json_body(body);
            if body_bytes.len() > egress_cfg.max_request_body_bytes {
                return Err(format!(
                    "request body {} bytes exceeds max {}",
                    body_bytes.len(),
                    egress_cfg.max_request_body_bytes
                ));
            }

            Ok(ValidatedArgs::HttpRequest {
                method: Method::POST,
                url: make_url("/api/exponential/projects")?,
                body: Some(body_bytes),
                content_type_json: true,
                require_bearer: true,
            })
        }

        "get_project" => {
            let project_id = params
                .get("projectId")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "missing required param: projectId".to_string())?;
            Ok(ValidatedArgs::HttpRequest {
                method: Method::GET,
                url: make_url(&format!("/api/exponential/projects/{project_id}"))?,
                body: None,
                content_type_json: false,
                require_bearer: true,
            })
        }
        "list_teams" => Ok(ValidatedArgs::HttpRequest {
            method: Method::GET,
            url: make_url("/api/exponential/teams")?,
            body: None,
            content_type_json: false,
            require_bearer: true,
        }),
        "get_team" => {
            let team_id = params
                .get("teamId")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "missing required param: teamId".to_string())?;
            Ok(ValidatedArgs::HttpRequest {
                method: Method::GET,
                url: make_url(&format!("/api/exponential/teams/{team_id}"))?,
                body: None,
                content_type_json: false,
                require_bearer: true,
            })
        }
        "get_project_tasks" => {
            let project_id = params
                .get("projectId")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "missing required param: projectId".to_string())?;
            let mut url = url::Url::parse(&make_url(&format!("/api/exponential/projects/{project_id}/tasks"))?).unwrap();
            {
                let mut qp = url.query_pairs_mut();
                if let Some(v) = params.get("limit").and_then(|v| v.as_u64()) {
                    qp.append_pair("limit", &v.to_string());
                }
                if let Some(v) = params.get("cursor").and_then(|v| v.as_str()) {
                    qp.append_pair("cursor", v);
                }
                if let Some(v) = params.get("includeArchived").and_then(|v| v.as_bool()) {
                    qp.append_pair("includeArchived", if v { "true" } else { "false" });
                }
            }
            Ok(ValidatedArgs::HttpRequest {
                method: Method::GET,
                url: url.to_string(),
                body: None,
                content_type_json: false,
                require_bearer: true,
            })
        }
        "get_project_members" => {
            let project_id = params
                .get("projectId")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "missing required param: projectId".to_string())?;
            Ok(ValidatedArgs::HttpRequest {
                method: Method::GET,
                url: make_url(&format!("/api/exponential/projects/{project_id}/members"))?,
                body: None,
                content_type_json: false,
                require_bearer: true,
            })
        }
        "get_project_permissions" => {
            let project_id = params
                .get("projectId")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "missing required param: projectId".to_string())?;
            let action = params
                .get("action")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "missing required param: action".to_string())?;
            let mut url = url::Url::parse(&make_url(&format!("/api/exponential/projects/{project_id}/permissions"))?).unwrap();
            url.query_pairs_mut().append_pair("action", action);
            Ok(ValidatedArgs::HttpRequest {
                method: Method::GET,
                url: url.to_string(),
                body: None,
                content_type_json: false,
                require_bearer: true,
            })
        }
        "get_team_members" => {
            let team_id = params
                .get("teamId")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "missing required param: teamId".to_string())?;
            Ok(ValidatedArgs::HttpRequest {
                method: Method::GET,
                url: make_url(&format!("/api/exponential/teams/{team_id}/members"))?,
                body: None,
                content_type_json: false,
                require_bearer: true,
            })
        }
        "get_team_permissions" => {
            let team_id = params
                .get("teamId")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "missing required param: teamId".to_string())?;
            let action = params
                .get("action")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "missing required param: action".to_string())?;
            let mut url = url::Url::parse(&make_url(&format!("/api/exponential/teams/{team_id}/permissions"))?).unwrap();
            url.query_pairs_mut().append_pair("action", action);
            Ok(ValidatedArgs::HttpRequest {
                method: Method::GET,
                url: url.to_string(),
                body: None,
                content_type_json: false,
                require_bearer: true,
            })
        }
        "get_task_comments" => {
            let task_id = params
                .get("taskId")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "missing required param: taskId".to_string())?;
            Ok(ValidatedArgs::HttpRequest {
                method: Method::GET,
                url: make_url(&format!("/api/exponential/tasks/{task_id}/comments"))?,
                body: None,
                content_type_json: false,
                require_bearer: true,
            })
        }

        // --- Greenspot shims ----------------------------------------------
        "list_greenspot_contacts" => {
            let mut url = url::Url::parse(&make_url("/api/greenspot/contacts")?).unwrap();
            {
                let mut qp = url.query_pairs_mut();
                if let Some(v) = params.get("search").and_then(|v| v.as_str()) {
                    qp.append_pair("search", v);
                }
                if let Some(v) = params.get("companyId").and_then(|v| v.as_str()) {
                    qp.append_pair("companyId", v);
                }
                if let Some(v) = params.get("includeArchived").and_then(|v| v.as_bool()) {
                    qp.append_pair("includeArchived", if v { "true" } else { "false" });
                }
                if let Some(v) = params.get("limit").and_then(|v| v.as_u64()) {
                    qp.append_pair("limit", &v.to_string());
                }
            }
            Ok(ValidatedArgs::HttpRequest {
                method: Method::GET,
                url: url.to_string(),
                body: None,
                content_type_json: false,
                require_bearer: true,
            })
        }
        "create_greenspot_contact" => {
            let name = params
                .get("name")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "missing required param: name".to_string())?;
            let mut body = serde_json::json!({ "name": name });
            if let Some(v) = params.get("email").and_then(|v| v.as_str()) {
                body["email"] = serde_json::Value::String(v.to_owned());
            }
            if let Some(v) = params.get("phone").and_then(|v| v.as_str()) {
                body["phone"] = serde_json::Value::String(v.to_owned());
            }
            if let Some(v) = params.get("title").and_then(|v| v.as_str()) {
                body["title"] = serde_json::Value::String(v.to_owned());
            }
            if let Some(v) = params.get("companyId").and_then(|v| v.as_str()) {
                body["companyId"] = serde_json::Value::String(v.to_owned());
            }
            let body_bytes = json_body(body);
            if body_bytes.len() > egress_cfg.max_request_body_bytes {
                return Err(format!(
                    "request body {} bytes exceeds max {}",
                    body_bytes.len(),
                    egress_cfg.max_request_body_bytes
                ));
            }
            Ok(ValidatedArgs::HttpRequest {
                method: Method::POST,
                url: make_url("/api/greenspot/contacts")?,
                body: Some(body_bytes),
                content_type_json: true,
                require_bearer: true,
            })
        }
        "get_greenspot_contact" => {
            let id = params
                .get("id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "missing required param: id".to_string())?;
            Ok(ValidatedArgs::HttpRequest {
                method: Method::GET,
                url: make_url(&format!("/api/greenspot/contacts/{id}"))?,
                body: None,
                content_type_json: false,
                require_bearer: true,
            })
        }
        "list_greenspot_companies" => {
            let mut url = url::Url::parse(&make_url("/api/greenspot/companies")?).unwrap();
            {
                let mut qp = url.query_pairs_mut();
                if let Some(v) = params.get("search").and_then(|v| v.as_str()) {
                    qp.append_pair("search", v);
                }
                if let Some(v) = params.get("includeArchived").and_then(|v| v.as_bool()) {
                    qp.append_pair("includeArchived", if v { "true" } else { "false" });
                }
                if let Some(v) = params.get("limit").and_then(|v| v.as_u64()) {
                    qp.append_pair("limit", &v.to_string());
                }
            }
            Ok(ValidatedArgs::HttpRequest {
                method: Method::GET,
                url: url.to_string(),
                body: None,
                content_type_json: false,
                require_bearer: true,
            })
        }
        "create_greenspot_company" => {
            let name = params
                .get("name")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "missing required param: name".to_string())?;
            let mut body = serde_json::json!({ "name": name });
            if let Some(v) = params.get("domain").and_then(|v| v.as_str()) {
                body["domain"] = serde_json::Value::String(v.to_owned());
            }
            if let Some(v) = params.get("industry").and_then(|v| v.as_str()) {
                body["industry"] = serde_json::Value::String(v.to_owned());
            }
            let body_bytes = json_body(body);
            if body_bytes.len() > egress_cfg.max_request_body_bytes {
                return Err(format!(
                    "request body {} bytes exceeds max {}",
                    body_bytes.len(),
                    egress_cfg.max_request_body_bytes
                ));
            }
            Ok(ValidatedArgs::HttpRequest {
                method: Method::POST,
                url: make_url("/api/greenspot/companies")?,
                body: Some(body_bytes),
                content_type_json: true,
                require_bearer: true,
            })
        }
        "get_greenspot_company" => {
            let id = params
                .get("id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "missing required param: id".to_string())?;
            Ok(ValidatedArgs::HttpRequest {
                method: Method::GET,
                url: make_url(&format!("/api/greenspot/companies/{id}"))?,
                body: None,
                content_type_json: false,
                require_bearer: true,
            })
        }
        "list_greenspot_deals" => {
            let mut url = url::Url::parse(&make_url("/api/greenspot/deals")?).unwrap();
            {
                let mut qp = url.query_pairs_mut();
                if let Some(v) = params.get("search").and_then(|v| v.as_str()) {
                    qp.append_pair("search", v);
                }
                if let Some(v) = params.get("stage").and_then(|v| v.as_str()) {
                    qp.append_pair("stage", v);
                }
                if let Some(v) = params.get("includeArchived").and_then(|v| v.as_bool()) {
                    qp.append_pair("includeArchived", if v { "true" } else { "false" });
                }
                if let Some(v) = params.get("limit").and_then(|v| v.as_u64()) {
                    qp.append_pair("limit", &v.to_string());
                }
            }
            Ok(ValidatedArgs::HttpRequest {
                method: Method::GET,
                url: url.to_string(),
                body: None,
                content_type_json: false,
                require_bearer: true,
            })
        }
        "create_greenspot_deal" => {
            let name = params
                .get("name")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "missing required param: name".to_string())?;
            let mut body = serde_json::json!({ "name": name });
            if let Some(v) = params.get("stage").and_then(|v| v.as_str()) {
                body["stage"] = serde_json::Value::String(v.to_owned());
            }
            if let Some(v) = params.get("amount").and_then(|v| v.as_f64()) {
                body["amount"] = serde_json::json!(v);
            }
            if let Some(v) = params.get("companyId").and_then(|v| v.as_str()) {
                body["companyId"] = serde_json::Value::String(v.to_owned());
            }
            if let Some(v) = params.get("primaryContactId").and_then(|v| v.as_str()) {
                body["primaryContactId"] = serde_json::Value::String(v.to_owned());
            }
            let body_bytes = json_body(body);
            if body_bytes.len() > egress_cfg.max_request_body_bytes {
                return Err(format!(
                    "request body {} bytes exceeds max {}",
                    body_bytes.len(),
                    egress_cfg.max_request_body_bytes
                ));
            }
            Ok(ValidatedArgs::HttpRequest {
                method: Method::POST,
                url: make_url("/api/greenspot/deals")?,
                body: Some(body_bytes),
                content_type_json: true,
                require_bearer: true,
            })
        }
        "get_greenspot_deal" => {
            let id = params
                .get("id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "missing required param: id".to_string())?;
            Ok(ValidatedArgs::HttpRequest {
                method: Method::GET,
                url: make_url(&format!("/api/greenspot/deals/{id}"))?,
                body: None,
                content_type_json: false,
                require_bearer: true,
            })
        }
        "list_greenspot_tasks" => {
            let mut url = url::Url::parse(&make_url("/api/greenspot/tasks")?).unwrap();
            {
                let mut qp = url.query_pairs_mut();
                if let Some(v) = params.get("search").and_then(|v| v.as_str()) {
                    qp.append_pair("search", v);
                }
                if let Some(v) = params.get("status").and_then(|v| v.as_str()) {
                    qp.append_pair("status", v);
                }
                if let Some(v) = params.get("assigneeId").and_then(|v| v.as_str()) {
                    qp.append_pair("assigneeId", v);
                }
                if let Some(v) = params.get("includeArchived").and_then(|v| v.as_bool()) {
                    qp.append_pair("includeArchived", if v { "true" } else { "false" });
                }
                if let Some(v) = params.get("limit").and_then(|v| v.as_u64()) {
                    qp.append_pair("limit", &v.to_string());
                }
            }
            Ok(ValidatedArgs::HttpRequest {
                method: Method::GET,
                url: url.to_string(),
                body: None,
                content_type_json: false,
                require_bearer: true,
            })
        }
        "create_greenspot_task" => {
            let title = params
                .get("title")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "missing required param: title".to_string())?;
            let mut body = serde_json::json!({ "title": title });
            if let Some(v) = params.get("status").and_then(|v| v.as_str()) {
                body["status"] = serde_json::Value::String(v.to_owned());
            }
            if let Some(v) = params.get("dueDate").and_then(|v| v.as_str()) {
                body["dueDate"] = serde_json::Value::String(v.to_owned());
            }
            if let Some(v) = params.get("assigneeId").and_then(|v| v.as_str()) {
                body["assigneeId"] = serde_json::Value::String(v.to_owned());
            }
            let body_bytes = json_body(body);
            if body_bytes.len() > egress_cfg.max_request_body_bytes {
                return Err(format!(
                    "request body {} bytes exceeds max {}",
                    body_bytes.len(),
                    egress_cfg.max_request_body_bytes
                ));
            }
            Ok(ValidatedArgs::HttpRequest {
                method: Method::POST,
                url: make_url("/api/greenspot/tasks")?,
                body: Some(body_bytes),
                content_type_json: true,
                require_bearer: true,
            })
        }
        "get_greenspot_task" => {
            let id = params
                .get("id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "missing required param: id".to_string())?;
            Ok(ValidatedArgs::HttpRequest {
                method: Method::GET,
                url: make_url(&format!("/api/greenspot/tasks/{id}"))?,
                body: None,
                content_type_json: false,
                require_bearer: true,
            })
        }

        // --- GreenBooks shims ---------------------------------------------
        "list_greenbooks_accounts" => {
            let mut url = url::Url::parse(&make_url("/api/greenbooks/accounts")?).unwrap();
            if let Some(v) = params.get("q").and_then(|v| v.as_str()) {
                url.query_pairs_mut().append_pair("q", v);
            }
            Ok(ValidatedArgs::HttpRequest {
                method: Method::GET,
                url: url.to_string(),
                body: None,
                content_type_json: false,
                require_bearer: true,
            })
        }
        "get_greenbooks_account" => {
            let id = params
                .get("id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "missing required param: id".to_string())?;
            Ok(ValidatedArgs::HttpRequest {
                method: Method::GET,
                url: make_url(&format!("/api/greenbooks/accounts/{id}"))?,
                body: None,
                content_type_json: false,
                require_bearer: true,
            })
        }
        "list_greenbooks_customers" => {
            let mut url = url::Url::parse(&make_url("/api/greenbooks/customers")?).unwrap();
            if let Some(v) = params.get("q").and_then(|v| v.as_str()) {
                url.query_pairs_mut().append_pair("q", v);
            }
            Ok(ValidatedArgs::HttpRequest {
                method: Method::GET,
                url: url.to_string(),
                body: None,
                content_type_json: false,
                require_bearer: true,
            })
        }
        "get_greenbooks_customer" => {
            let id = params
                .get("id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "missing required param: id".to_string())?;
            Ok(ValidatedArgs::HttpRequest {
                method: Method::GET,
                url: make_url(&format!("/api/greenbooks/customers/{id}"))?,
                body: None,
                content_type_json: false,
                require_bearer: true,
            })
        }
        "list_greenbooks_vendors" => {
            let mut url = url::Url::parse(&make_url("/api/greenbooks/vendors")?).unwrap();
            if let Some(v) = params.get("q").and_then(|v| v.as_str()) {
                url.query_pairs_mut().append_pair("q", v);
            }
            Ok(ValidatedArgs::HttpRequest {
                method: Method::GET,
                url: url.to_string(),
                body: None,
                content_type_json: false,
                require_bearer: true,
            })
        }
        "get_greenbooks_vendor" => {
            let id = params
                .get("id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "missing required param: id".to_string())?;
            Ok(ValidatedArgs::HttpRequest {
                method: Method::GET,
                url: make_url(&format!("/api/greenbooks/vendors/{id}"))?,
                body: None,
                content_type_json: false,
                require_bearer: true,
            })
        }
        "list_greenbooks_invoices" => {
            let mut url = url::Url::parse(&make_url("/api/greenbooks/invoices")?).unwrap();
            if let Some(v) = params.get("status").and_then(|v| v.as_str()) {
                url.query_pairs_mut().append_pair("status", v);
            }
            Ok(ValidatedArgs::HttpRequest {
                method: Method::GET,
                url: url.to_string(),
                body: None,
                content_type_json: false,
                require_bearer: true,
            })
        }
        "get_greenbooks_invoice" => {
            let id = params
                .get("id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "missing required param: id".to_string())?;
            Ok(ValidatedArgs::HttpRequest {
                method: Method::GET,
                url: make_url(&format!("/api/greenbooks/invoices/{id}"))?,
                body: None,
                content_type_json: false,
                require_bearer: true,
            })
        }
        "create_greenbooks_invoice" => {
            let payload = params
                .get("payload")
                .cloned()
                .unwrap_or_else(|| params.clone());
            let body_bytes = json_body(payload);
            if body_bytes.len() > egress_cfg.max_request_body_bytes {
                return Err(format!(
                    "request body {} bytes exceeds max {}",
                    body_bytes.len(),
                    egress_cfg.max_request_body_bytes
                ));
            }
            Ok(ValidatedArgs::HttpRequest {
                method: Method::POST,
                url: make_url("/api/greenbooks/invoices")?,
                body: Some(body_bytes),
                content_type_json: true,
                require_bearer: true,
            })
        }
        "list_greenbooks_bills" => {
            let mut url = url::Url::parse(&make_url("/api/greenbooks/bills")?).unwrap();
            if let Some(v) = params.get("status").and_then(|v| v.as_str()) {
                url.query_pairs_mut().append_pair("status", v);
            }
            Ok(ValidatedArgs::HttpRequest {
                method: Method::GET,
                url: url.to_string(),
                body: None,
                content_type_json: false,
                require_bearer: true,
            })
        }
        "get_greenbooks_bill" => {
            let id = params
                .get("id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "missing required param: id".to_string())?;
            Ok(ValidatedArgs::HttpRequest {
                method: Method::GET,
                url: make_url(&format!("/api/greenbooks/bills/{id}"))?,
                body: None,
                content_type_json: false,
                require_bearer: true,
            })
        }
        "create_greenbooks_bill" => {
            let payload = params
                .get("payload")
                .cloned()
                .unwrap_or_else(|| params.clone());
            let body_bytes = json_body(payload);
            if body_bytes.len() > egress_cfg.max_request_body_bytes {
                return Err(format!(
                    "request body {} bytes exceeds max {}",
                    body_bytes.len(),
                    egress_cfg.max_request_body_bytes
                ));
            }
            Ok(ValidatedArgs::HttpRequest {
                method: Method::POST,
                url: make_url("/api/greenbooks/bills")?,
                body: Some(body_bytes),
                content_type_json: true,
                require_bearer: true,
            })
        }
        "list_greenbooks_journal_entries" => Ok(ValidatedArgs::HttpRequest {
            method: Method::GET,
            url: make_url("/api/greenbooks/journal-entries")?,
            body: None,
            content_type_json: false,
            require_bearer: true,
        }),
        "get_greenbooks_journal_entry" => {
            let id = params
                .get("id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "missing required param: id".to_string())?;
            Ok(ValidatedArgs::HttpRequest {
                method: Method::GET,
                url: make_url(&format!("/api/greenbooks/journal-entries/{id}"))?,
                body: None,
                content_type_json: false,
                require_bearer: true,
            })
        }
        "create_greenbooks_journal_entry" => {
            let payload = params
                .get("payload")
                .cloned()
                .unwrap_or_else(|| params.clone());
            let body_bytes = json_body(payload);
            if body_bytes.len() > egress_cfg.max_request_body_bytes {
                return Err(format!(
                    "request body {} bytes exceeds max {}",
                    body_bytes.len(),
                    egress_cfg.max_request_body_bytes
                ));
            }
            Ok(ValidatedArgs::HttpRequest {
                method: Method::POST,
                url: make_url("/api/greenbooks/journal-entries")?,
                body: Some(body_bytes),
                content_type_json: true,
                require_bearer: true,
            })
        }
        "list_greenbooks_bank_accounts" => Ok(ValidatedArgs::HttpRequest {
            method: Method::GET,
            url: make_url("/api/greenbooks/bank-accounts")?,
            body: None,
            content_type_json: false,
            require_bearer: true,
        }),
        "get_greenbooks_bank_account" => {
            let id = params
                .get("id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "missing required param: id".to_string())?;
            Ok(ValidatedArgs::HttpRequest {
                method: Method::GET,
                url: make_url(&format!("/api/greenbooks/bank-accounts/{id}"))?,
                body: None,
                content_type_json: false,
                require_bearer: true,
            })
        }
        "list_greenbooks_reports" => Ok(ValidatedArgs::HttpRequest {
            method: Method::GET,
            url: make_url("/api/greenbooks/reports")?,
            body: None,
            content_type_json: false,
            require_bearer: true,
        }),
        "post_greenbooks_invoice_gl" => {
            let id = params
                .get("id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "missing required param: id".to_string())?;
            Ok(ValidatedArgs::HttpRequest {
                method: Method::POST,
                url: make_url(&format!("/api/greenbooks/invoices/{id}/post-gl"))?,
                body: None,
                content_type_json: false,
                require_bearer: true,
            })
        }
        "post_greenbooks_bill_gl" => {
            let id = params
                .get("id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "missing required param: id".to_string())?;
            Ok(ValidatedArgs::HttpRequest {
                method: Method::POST,
                url: make_url(&format!("/api/greenbooks/bills/{id}/post-gl"))?,
                body: None,
                content_type_json: false,
                require_bearer: true,
            })
        }
        "post_greenbooks_journal_entry" => {
            let id = params
                .get("id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "missing required param: id".to_string())?;
            Ok(ValidatedArgs::HttpRequest {
                method: Method::POST,
                url: make_url(&format!("/api/greenbooks/journal-entries/{id}/post"))?,
                body: None,
                content_type_json: false,
                require_bearer: true,
            })
        }
        "create_greenbooks_invoice_payment" => {
            let id = params
                .get("id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "missing required param: id".to_string())?;
            let payload = params
                .get("payload")
                .cloned()
                .unwrap_or_else(|| params.clone());
            let body_bytes = json_body(payload);
            if body_bytes.len() > egress_cfg.max_request_body_bytes {
                return Err(format!(
                    "request body {} bytes exceeds max {}",
                    body_bytes.len(),
                    egress_cfg.max_request_body_bytes
                ));
            }
            Ok(ValidatedArgs::HttpRequest {
                method: Method::POST,
                url: make_url(&format!("/api/greenbooks/invoices/{id}/payments"))?,
                body: Some(body_bytes),
                content_type_json: true,
                require_bearer: true,
            })
        }
        "create_greenbooks_bill_payment" => {
            let id = params
                .get("id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "missing required param: id".to_string())?;
            let payload = params
                .get("payload")
                .cloned()
                .unwrap_or_else(|| params.clone());
            let body_bytes = json_body(payload);
            if body_bytes.len() > egress_cfg.max_request_body_bytes {
                return Err(format!(
                    "request body {} bytes exceeds max {}",
                    body_bytes.len(),
                    egress_cfg.max_request_body_bytes
                ));
            }
            Ok(ValidatedArgs::HttpRequest {
                method: Method::POST,
                url: make_url(&format!("/api/greenbooks/bills/{id}/payments"))?,
                body: Some(body_bytes),
                content_type_json: true,
                require_bearer: true,
            })
        }
        "create_greenbooks_bank_transfer" => {
            let payload = params
                .get("payload")
                .cloned()
                .unwrap_or_else(|| params.clone());
            let body_bytes = json_body(payload);
            if body_bytes.len() > egress_cfg.max_request_body_bytes {
                return Err(format!(
                    "request body {} bytes exceeds max {}",
                    body_bytes.len(),
                    egress_cfg.max_request_body_bytes
                ));
            }
            Ok(ValidatedArgs::HttpRequest {
                method: Method::POST,
                url: make_url("/api/greenbooks/bank-accounts/transfer")?,
                body: Some(body_bytes),
                content_type_json: true,
                require_bearer: true,
            })
        }
        "post_greenbooks_bank_reconcile" => {
            let id = params
                .get("id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "missing required param: id".to_string())?;
            let payload = params
                .get("payload")
                .cloned()
                .unwrap_or_else(|| params.clone());
            let body_bytes = json_body(payload);
            if body_bytes.len() > egress_cfg.max_request_body_bytes {
                return Err(format!(
                    "request body {} bytes exceeds max {}",
                    body_bytes.len(),
                    egress_cfg.max_request_body_bytes
                ));
            }
            Ok(ValidatedArgs::HttpRequest {
                method: Method::POST,
                url: make_url(&format!("/api/greenbooks/bank-accounts/{id}/reconcile"))?,
                body: Some(body_bytes),
                content_type_json: true,
                require_bearer: true,
            })
        }
        "list_greenbooks_bank_transactions" => {
            let id = params
                .get("id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "missing required param: id".to_string())?;
            let mut url = url::Url::parse(&make_url(&format!(
                "/api/greenbooks/bank-accounts/{id}/transactions"
            ))?)
            .unwrap();
            {
                let mut qp = url.query_pairs_mut();
                if let Some(v) = params.get("reconciled").and_then(|v| v.as_bool()) {
                    qp.append_pair("reconciled", if v { "true" } else { "false" });
                }
                if let Some(v) = params.get("startDate").and_then(|v| v.as_str()) {
                    qp.append_pair("startDate", v);
                }
                if let Some(v) = params.get("endDate").and_then(|v| v.as_str()) {
                    qp.append_pair("endDate", v);
                }
                if let Some(v) = params.get("limit").and_then(|v| v.as_u64()) {
                    qp.append_pair("limit", &v.to_string());
                }
            }
            Ok(ValidatedArgs::HttpRequest {
                method: Method::GET,
                url: url.to_string(),
                body: None,
                content_type_json: false,
                require_bearer: true,
            })
        }
        "list_greenbooks_payments" => Ok(ValidatedArgs::HttpRequest {
            method: Method::GET,
            url: make_url("/api/greenbooks/payments")?,
            body: None,
            content_type_json: false,
            require_bearer: true,
        }),
        "list_greenbooks_currencies" => Ok(ValidatedArgs::HttpRequest {
            method: Method::GET,
            url: make_url("/api/greenbooks/currencies")?,
            body: None,
            content_type_json: false,
            require_bearer: true,
        }),
        "list_greenbooks_tax_codes" => Ok(ValidatedArgs::HttpRequest {
            method: Method::GET,
            url: make_url("/api/greenbooks/tax-codes")?,
            body: None,
            content_type_json: false,
            require_bearer: true,
        }),
        "create_greenbooks_tax_code" => {
            let payload = params
                .get("payload")
                .cloned()
                .unwrap_or_else(|| params.clone());
            let body_bytes = json_body(payload);
            if body_bytes.len() > egress_cfg.max_request_body_bytes {
                return Err(format!(
                    "request body {} bytes exceeds max {}",
                    body_bytes.len(),
                    egress_cfg.max_request_body_bytes
                ));
            }
            Ok(ValidatedArgs::HttpRequest {
                method: Method::POST,
                url: make_url("/api/greenbooks/tax-codes")?,
                body: Some(body_bytes),
                content_type_json: true,
                require_bearer: true,
            })
        }
        "get_greenbooks_tax_code" => {
            let id = params
                .get("id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "missing required param: id".to_string())?;
            Ok(ValidatedArgs::HttpRequest {
                method: Method::GET,
                url: make_url(&format!("/api/greenbooks/tax-codes/{id}"))?,
                body: None,
                content_type_json: false,
                require_bearer: true,
            })
        }
        "update_greenbooks_tax_code" => {
            let id = params
                .get("id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "missing required param: id".to_string())?;
            let payload = params
                .get("payload")
                .cloned()
                .unwrap_or_else(|| params.clone());
            let body_bytes = json_body(payload);
            if body_bytes.len() > egress_cfg.max_request_body_bytes {
                return Err(format!(
                    "request body {} bytes exceeds max {}",
                    body_bytes.len(),
                    egress_cfg.max_request_body_bytes
                ));
            }
            Ok(ValidatedArgs::HttpRequest {
                method: Method::PATCH,
                url: make_url(&format!("/api/greenbooks/tax-codes/{id}"))?,
                body: Some(body_bytes),
                content_type_json: true,
                require_bearer: true,
            })
        }
        "list_greenbooks_fx_rates" => Ok(ValidatedArgs::HttpRequest {
            method: Method::GET,
            url: make_url("/api/greenbooks/fx-rates")?,
            body: None,
            content_type_json: false,
            require_bearer: true,
        }),
        "create_greenbooks_fx_rate" => {
            let payload = params
                .get("payload")
                .cloned()
                .unwrap_or_else(|| params.clone());
            let body_bytes = json_body(payload);
            if body_bytes.len() > egress_cfg.max_request_body_bytes {
                return Err(format!(
                    "request body {} bytes exceeds max {}",
                    body_bytes.len(),
                    egress_cfg.max_request_body_bytes
                ));
            }
            Ok(ValidatedArgs::HttpRequest {
                method: Method::POST,
                url: make_url("/api/greenbooks/fx-rates")?,
                body: Some(body_bytes),
                content_type_json: true,
                require_bearer: true,
            })
        }
        "update_greenbooks_fx_rate" => {
            let id = params
                .get("id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "missing required param: id".to_string())?;
            let payload = params
                .get("payload")
                .cloned()
                .unwrap_or_else(|| params.clone());
            let body_bytes = json_body(payload);
            if body_bytes.len() > egress_cfg.max_request_body_bytes {
                return Err(format!(
                    "request body {} bytes exceeds max {}",
                    body_bytes.len(),
                    egress_cfg.max_request_body_bytes
                ));
            }
            Ok(ValidatedArgs::HttpRequest {
                method: Method::PATCH,
                url: make_url(&format!("/api/greenbooks/fx-rates/{id}"))?,
                body: Some(body_bytes),
                content_type_json: true,
                require_bearer: true,
            })
        }
        "delete_greenbooks_fx_rate" => {
            let id = params
                .get("id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "missing required param: id".to_string())?;
            Ok(ValidatedArgs::HttpRequest {
                method: Method::DELETE,
                url: make_url(&format!("/api/greenbooks/fx-rates/{id}"))?,
                body: None,
                content_type_json: false,
                require_bearer: true,
            })
        }
        "convert_greenbooks_fx" => {
            let mut url = url::Url::parse(&make_url("/api/greenbooks/fx-rates/convert")?).unwrap();
            {
                let mut qp = url.query_pairs_mut();
                if let Some(v) = params.get("from").and_then(|v| v.as_str()) {
                    qp.append_pair("from", v);
                }
                if let Some(v) = params.get("to").and_then(|v| v.as_str()) {
                    qp.append_pair("to", v);
                }
                if let Some(v) = params.get("amount").and_then(|v| v.as_f64()) {
                    qp.append_pair("amount", &v.to_string());
                }
            }
            Ok(ValidatedArgs::HttpRequest {
                method: Method::GET,
                url: url.to_string(),
                body: None,
                content_type_json: false,
                require_bearer: true,
            })
        }
        "run_greenbooks_fx_revaluation" => {
            let payload = params
                .get("payload")
                .cloned()
                .unwrap_or_else(|| params.clone());
            let body_bytes = json_body(payload);
            if body_bytes.len() > egress_cfg.max_request_body_bytes {
                return Err(format!(
                    "request body {} bytes exceeds max {}",
                    body_bytes.len(),
                    egress_cfg.max_request_body_bytes
                ));
            }
            Ok(ValidatedArgs::HttpRequest {
                method: Method::POST,
                url: make_url("/api/greenbooks/fx-revaluation")?,
                body: Some(body_bytes),
                content_type_json: true,
                require_bearer: true,
            })
        }
        "list_greenbooks_fiscal_periods" => Ok(ValidatedArgs::HttpRequest {
            method: Method::GET,
            url: make_url("/api/greenbooks/fiscal-periods")?,
            body: None,
            content_type_json: false,
            require_bearer: true,
        }),
        "create_greenbooks_fiscal_period" => {
            let payload = params
                .get("payload")
                .cloned()
                .unwrap_or_else(|| params.clone());
            let body_bytes = json_body(payload);
            if body_bytes.len() > egress_cfg.max_request_body_bytes {
                return Err(format!(
                    "request body {} bytes exceeds max {}",
                    body_bytes.len(),
                    egress_cfg.max_request_body_bytes
                ));
            }
            Ok(ValidatedArgs::HttpRequest {
                method: Method::POST,
                url: make_url("/api/greenbooks/fiscal-periods")?,
                body: Some(body_bytes),
                content_type_json: true,
                require_bearer: true,
            })
        }
        "get_greenbooks_fiscal_period" => {
            let id = params
                .get("id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "missing required param: id".to_string())?;
            Ok(ValidatedArgs::HttpRequest {
                method: Method::GET,
                url: make_url(&format!("/api/greenbooks/fiscal-periods/{id}"))?,
                body: None,
                content_type_json: false,
                require_bearer: true,
            })
        }
        "update_greenbooks_fiscal_period" => {
            let id = params
                .get("id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "missing required param: id".to_string())?;
            let payload = params
                .get("payload")
                .cloned()
                .unwrap_or_else(|| params.clone());
            let body_bytes = json_body(payload);
            if body_bytes.len() > egress_cfg.max_request_body_bytes {
                return Err(format!(
                    "request body {} bytes exceeds max {}",
                    body_bytes.len(),
                    egress_cfg.max_request_body_bytes
                ));
            }
            Ok(ValidatedArgs::HttpRequest {
                method: Method::PATCH,
                url: make_url(&format!("/api/greenbooks/fiscal-periods/{id}"))?,
                body: Some(body_bytes),
                content_type_json: true,
                require_bearer: true,
            })
        }
        "delete_greenbooks_fiscal_period" => {
            let id = params
                .get("id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "missing required param: id".to_string())?;
            Ok(ValidatedArgs::HttpRequest {
                method: Method::DELETE,
                url: make_url(&format!("/api/greenbooks/fiscal-periods/{id}"))?,
                body: None,
                content_type_json: false,
                require_bearer: true,
            })
        }
        "check_greenbooks_fiscal_lock" => {
            let mut url =
                url::Url::parse(&make_url("/api/greenbooks/fiscal-periods/check")?).unwrap();
            if let Some(v) = params.get("date").and_then(|v| v.as_str()) {
                url.query_pairs_mut().append_pair("date", v);
            }
            Ok(ValidatedArgs::HttpRequest {
                method: Method::GET,
                url: url.to_string(),
                body: None,
                content_type_json: false,
                require_bearer: true,
            })
        }
        "list_greenbooks_items" => Ok(ValidatedArgs::HttpRequest {
            method: Method::GET,
            url: make_url("/api/greenbooks/items")?,
            body: None,
            content_type_json: false,
            require_bearer: true,
        }),
        "create_greenbooks_item" => {
            let payload = params
                .get("payload")
                .cloned()
                .unwrap_or_else(|| params.clone());
            let body_bytes = json_body(payload);
            if body_bytes.len() > egress_cfg.max_request_body_bytes {
                return Err(format!(
                    "request body {} bytes exceeds max {}",
                    body_bytes.len(),
                    egress_cfg.max_request_body_bytes
                ));
            }
            Ok(ValidatedArgs::HttpRequest {
                method: Method::POST,
                url: make_url("/api/greenbooks/items")?,
                body: Some(body_bytes),
                content_type_json: true,
                require_bearer: true,
            })
        }
        "update_greenbooks_item" => {
            let payload = params
                .get("payload")
                .cloned()
                .unwrap_or_else(|| params.clone());
            let body_bytes = json_body(payload);
            if body_bytes.len() > egress_cfg.max_request_body_bytes {
                return Err(format!(
                    "request body {} bytes exceeds max {}",
                    body_bytes.len(),
                    egress_cfg.max_request_body_bytes
                ));
            }
            Ok(ValidatedArgs::HttpRequest {
                method: Method::PATCH,
                url: make_url("/api/greenbooks/items")?,
                body: Some(body_bytes),
                content_type_json: true,
                require_bearer: true,
            })
        }
        "update_greenbooks_invoice" => {
            let id = params
                .get("id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "missing required param: id".to_string())?;
            let payload = params
                .get("payload")
                .cloned()
                .unwrap_or_else(|| params.clone());
            let body_bytes = json_body(payload);
            if body_bytes.len() > egress_cfg.max_request_body_bytes {
                return Err(format!(
                    "request body {} bytes exceeds max {}",
                    body_bytes.len(),
                    egress_cfg.max_request_body_bytes
                ));
            }
            Ok(ValidatedArgs::HttpRequest {
                method: Method::PATCH,
                url: make_url(&format!("/api/greenbooks/invoices/{id}"))?,
                body: Some(body_bytes),
                content_type_json: true,
                require_bearer: true,
            })
        }
        "post_greenbooks_invoice" => {
            let id = params
                .get("id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "missing required param: id".to_string())?;
            Ok(ValidatedArgs::HttpRequest {
                method: Method::POST,
                url: make_url(&format!("/api/greenbooks/invoices/{id}/post"))?,
                body: None,
                content_type_json: false,
                require_bearer: true,
            })
        }
        "update_greenbooks_bill" => {
            let id = params
                .get("id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "missing required param: id".to_string())?;
            let payload = params
                .get("payload")
                .cloned()
                .unwrap_or_else(|| params.clone());
            let body_bytes = json_body(payload);
            if body_bytes.len() > egress_cfg.max_request_body_bytes {
                return Err(format!(
                    "request body {} bytes exceeds max {}",
                    body_bytes.len(),
                    egress_cfg.max_request_body_bytes
                ));
            }
            Ok(ValidatedArgs::HttpRequest {
                method: Method::PATCH,
                url: make_url(&format!("/api/greenbooks/bills/{id}"))?,
                body: Some(body_bytes),
                content_type_json: true,
                require_bearer: true,
            })
        }
        "update_greenbooks_customer" => {
            let id = params
                .get("id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "missing required param: id".to_string())?;
            let payload = params
                .get("payload")
                .cloned()
                .unwrap_or_else(|| params.clone());
            let body_bytes = json_body(payload);
            if body_bytes.len() > egress_cfg.max_request_body_bytes {
                return Err(format!(
                    "request body {} bytes exceeds max {}",
                    body_bytes.len(),
                    egress_cfg.max_request_body_bytes
                ));
            }
            Ok(ValidatedArgs::HttpRequest {
                method: Method::PATCH,
                url: make_url(&format!("/api/greenbooks/customers/{id}"))?,
                body: Some(body_bytes),
                content_type_json: true,
                require_bearer: true,
            })
        }
        "delete_greenbooks_customer" => {
            let id = params
                .get("id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "missing required param: id".to_string())?;
            Ok(ValidatedArgs::HttpRequest {
                method: Method::DELETE,
                url: make_url(&format!("/api/greenbooks/customers/{id}"))?,
                body: None,
                content_type_json: false,
                require_bearer: true,
            })
        }
        "get_greenbooks_customer_statement" => {
            let id = params
                .get("id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "missing required param: id".to_string())?;
            Ok(ValidatedArgs::HttpRequest {
                method: Method::GET,
                url: make_url(&format!("/api/greenbooks/customers/{id}/statement"))?,
                body: None,
                content_type_json: false,
                require_bearer: true,
            })
        }
        "get_greenbooks_customer_hub" => {
            let id = params
                .get("id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "missing required param: id".to_string())?;
            Ok(ValidatedArgs::HttpRequest {
                method: Method::GET,
                url: make_url(&format!("/api/greenbooks/customers/{id}/hub"))?,
                body: None,
                content_type_json: false,
                require_bearer: true,
            })
        }
        "update_greenbooks_vendor" => {
            let id = params
                .get("id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "missing required param: id".to_string())?;
            let payload = params
                .get("payload")
                .cloned()
                .unwrap_or_else(|| params.clone());
            let body_bytes = json_body(payload);
            if body_bytes.len() > egress_cfg.max_request_body_bytes {
                return Err(format!(
                    "request body {} bytes exceeds max {}",
                    body_bytes.len(),
                    egress_cfg.max_request_body_bytes
                ));
            }
            Ok(ValidatedArgs::HttpRequest {
                method: Method::PATCH,
                url: make_url(&format!("/api/greenbooks/vendors/{id}"))?,
                body: Some(body_bytes),
                content_type_json: true,
                require_bearer: true,
            })
        }
        "list_greenbooks_recurring" => Ok(ValidatedArgs::HttpRequest {
            method: Method::GET,
            url: make_url("/api/greenbooks/recurring")?,
            body: None,
            content_type_json: false,
            require_bearer: true,
        }),
        "create_greenbooks_recurring" => {
            let payload = params
                .get("payload")
                .cloned()
                .unwrap_or_else(|| params.clone());
            let body_bytes = json_body(payload);
            if body_bytes.len() > egress_cfg.max_request_body_bytes {
                return Err(format!(
                    "request body {} bytes exceeds max {}",
                    body_bytes.len(),
                    egress_cfg.max_request_body_bytes
                ));
            }
            Ok(ValidatedArgs::HttpRequest {
                method: Method::POST,
                url: make_url("/api/greenbooks/recurring")?,
                body: Some(body_bytes),
                content_type_json: true,
                require_bearer: true,
            })
        }
        "create_greenbooks_bank_account" => {
            let payload = params
                .get("payload")
                .cloned()
                .unwrap_or_else(|| params.clone());
            let body_bytes = json_body(payload);
            if body_bytes.len() > egress_cfg.max_request_body_bytes {
                return Err(format!(
                    "request body {} bytes exceeds max {}",
                    body_bytes.len(),
                    egress_cfg.max_request_body_bytes
                ));
            }
            Ok(ValidatedArgs::HttpRequest {
                method: Method::POST,
                url: make_url("/api/greenbooks/bank-accounts")?,
                body: Some(body_bytes),
                content_type_json: true,
                require_bearer: true,
            })
        }
        "get_greenbooks_bank_account_reconciliations" => {
            let id = params
                .get("id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "missing required param: id".to_string())?;
            Ok(ValidatedArgs::HttpRequest {
                method: Method::GET,
                url: make_url(&format!("/api/greenbooks/bank-accounts/{id}/reconcile"))?,
                body: None,
                content_type_json: false,
                require_bearer: true,
            })
        }
        "patch_greenbooks_bank_reconcile" => {
            let id = params
                .get("id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "missing required param: id".to_string())?;
            let payload = params
                .get("payload")
                .cloned()
                .unwrap_or_else(|| params.clone());
            let body_bytes = json_body(payload);
            if body_bytes.len() > egress_cfg.max_request_body_bytes {
                return Err(format!(
                    "request body {} bytes exceeds max {}",
                    body_bytes.len(),
                    egress_cfg.max_request_body_bytes
                ));
            }
            Ok(ValidatedArgs::HttpRequest {
                method: Method::PATCH,
                url: make_url(&format!("/api/greenbooks/bank-accounts/{id}/reconcile"))?,
                body: Some(body_bytes),
                content_type_json: true,
                require_bearer: true,
            })
        }
        "update_greenbooks_bank_account" => {
            let id = params
                .get("id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "missing required param: id".to_string())?;
            let payload = params
                .get("payload")
                .cloned()
                .unwrap_or_else(|| params.clone());
            let body_bytes = json_body(payload);
            if body_bytes.len() > egress_cfg.max_request_body_bytes {
                return Err(format!(
                    "request body {} bytes exceeds max {}",
                    body_bytes.len(),
                    egress_cfg.max_request_body_bytes
                ));
            }
            Ok(ValidatedArgs::HttpRequest {
                method: Method::PATCH,
                url: make_url(&format!("/api/greenbooks/bank-accounts/{id}"))?,
                body: Some(body_bytes),
                content_type_json: true,
                require_bearer: true,
            })
        }
        "create_greenbooks_bank_transaction" => {
            let id = params
                .get("id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "missing required param: id".to_string())?;
            let payload = params
                .get("payload")
                .cloned()
                .unwrap_or_else(|| params.clone());
            let body_bytes = json_body(payload);
            if body_bytes.len() > egress_cfg.max_request_body_bytes {
                return Err(format!(
                    "request body {} bytes exceeds max {}",
                    body_bytes.len(),
                    egress_cfg.max_request_body_bytes
                ));
            }
            Ok(ValidatedArgs::HttpRequest {
                method: Method::POST,
                url: make_url(&format!("/api/greenbooks/bank-accounts/{id}/transactions"))?,
                body: Some(body_bytes),
                content_type_json: true,
                require_bearer: true,
            })
        }
        "get_greenbooks_account_ledger" => {
            let id = params
                .get("id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "missing required param: id".to_string())?;
            Ok(ValidatedArgs::HttpRequest {
                method: Method::GET,
                url: make_url(&format!("/api/greenbooks/accounts/{id}/ledger"))?,
                body: None,
                content_type_json: false,
                require_bearer: true,
            })
        }
        "create_greenbooks_account" => {
            let payload = params
                .get("payload")
                .cloned()
                .unwrap_or_else(|| params.clone());
            let body_bytes = json_body(payload);
            if body_bytes.len() > egress_cfg.max_request_body_bytes {
                return Err(format!(
                    "request body {} bytes exceeds max {}",
                    body_bytes.len(),
                    egress_cfg.max_request_body_bytes
                ));
            }
            Ok(ValidatedArgs::HttpRequest {
                method: Method::POST,
                url: make_url("/api/greenbooks/accounts")?,
                body: Some(body_bytes),
                content_type_json: true,
                require_bearer: true,
            })
        }
        "update_greenbooks_account" => {
            let id = params
                .get("id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "missing required param: id".to_string())?;
            let payload = params
                .get("payload")
                .cloned()
                .unwrap_or_else(|| params.clone());
            let body_bytes = json_body(payload);
            if body_bytes.len() > egress_cfg.max_request_body_bytes {
                return Err(format!(
                    "request body {} bytes exceeds max {}",
                    body_bytes.len(),
                    egress_cfg.max_request_body_bytes
                ));
            }
            Ok(ValidatedArgs::HttpRequest {
                method: Method::PATCH,
                url: make_url(&format!("/api/greenbooks/accounts/{id}"))?,
                body: Some(body_bytes),
                content_type_json: true,
                require_bearer: true,
            })
        }
        "import_greenbooks_accounts" => {
            let payload = params
                .get("payload")
                .cloned()
                .unwrap_or_else(|| params.clone());
            let body_bytes = json_body(payload);
            if body_bytes.len() > egress_cfg.max_request_body_bytes {
                return Err(format!(
                    "request body {} bytes exceeds max {}",
                    body_bytes.len(),
                    egress_cfg.max_request_body_bytes
                ));
            }
            Ok(ValidatedArgs::HttpRequest {
                method: Method::POST,
                url: make_url("/api/greenbooks/accounts/import")?,
                body: Some(body_bytes),
                content_type_json: true,
                require_bearer: true,
            })
        }
        "export_greenbooks_accounts" => Ok(ValidatedArgs::HttpRequest {
            method: Method::GET,
            url: make_url("/api/greenbooks/accounts/export")?,
            body: None,
            content_type_json: false,
            require_bearer: true,
        }),
        "list_greenbooks_audit_events" => Ok(ValidatedArgs::HttpRequest {
            method: Method::GET,
            url: make_url("/api/greenbooks/audit")?,
            body: None,
            content_type_json: false,
            require_bearer: true,
        }),
        "get_greenbooks_audit_entity" => {
            let entity_type = params
                .get("entityType")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "missing required param: entityType".to_string())?;
            let entity_id = params
                .get("entityId")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "missing required param: entityId".to_string())?;
            Ok(ValidatedArgs::HttpRequest {
                method: Method::GET,
                url: make_url(&format!("/api/greenbooks/audit/{entity_type}/{entity_id}"))?,
                body: None,
                content_type_json: false,
                require_bearer: true,
            })
        }
        "import_greenbooks_quickbooks" => {
            let payload = params
                .get("payload")
                .cloned()
                .unwrap_or_else(|| params.clone());
            let body_bytes = json_body(payload);
            if body_bytes.len() > egress_cfg.max_request_body_bytes {
                return Err(format!(
                    "request body {} bytes exceeds max {}",
                    body_bytes.len(),
                    egress_cfg.max_request_body_bytes
                ));
            }
            Ok(ValidatedArgs::HttpRequest {
                method: Method::POST,
                url: make_url("/api/greenbooks/import/quickbooks")?,
                body: Some(body_bytes),
                content_type_json: true,
                require_bearer: true,
            })
        }
        "get_greenbooks_settings" => Ok(ValidatedArgs::HttpRequest {
            method: Method::GET,
            url: make_url("/api/greenbooks/settings")?,
            body: None,
            content_type_json: false,
            require_bearer: true,
        }),
        "update_greenbooks_settings" => {
            let payload = params
                .get("payload")
                .cloned()
                .unwrap_or_else(|| params.clone());
            let body_bytes = json_body(payload);
            if body_bytes.len() > egress_cfg.max_request_body_bytes {
                return Err(format!(
                    "request body {} bytes exceeds max {}",
                    body_bytes.len(),
                    egress_cfg.max_request_body_bytes
                ));
            }
            Ok(ValidatedArgs::HttpRequest {
                method: Method::PUT,
                url: make_url("/api/greenbooks/settings")?,
                body: Some(body_bytes),
                content_type_json: true,
                require_bearer: true,
            })
        }
        "list_greenbooks_statements" => Ok(ValidatedArgs::HttpRequest {
            method: Method::GET,
            url: make_url("/api/greenbooks/statements")?,
            body: None,
            content_type_json: false,
            require_bearer: true,
        }),
        "get_greenbooks_crm_link" => Ok(ValidatedArgs::HttpRequest {
            method: Method::GET,
            url: make_url("/api/greenbooks/crm-link")?,
            body: None,
            content_type_json: false,
            require_bearer: true,
        }),
        "sync_greenbooks_crm_link" => {
            let payload = params
                .get("payload")
                .cloned()
                .unwrap_or_else(|| params.clone());
            let body_bytes = json_body(payload);
            if body_bytes.len() > egress_cfg.max_request_body_bytes {
                return Err(format!(
                    "request body {} bytes exceeds max {}",
                    body_bytes.len(),
                    egress_cfg.max_request_body_bytes
                ));
            }
            Ok(ValidatedArgs::HttpRequest {
                method: Method::POST,
                url: make_url("/api/greenbooks/crm-link")?,
                body: Some(body_bytes),
                content_type_json: true,
                require_bearer: true,
            })
        }
        "get_greenbooks_report_aging" => Ok(ValidatedArgs::HttpRequest {
            method: Method::GET,
            url: make_url("/api/greenbooks/reports?type=ar-aging")?,
            body: None,
            content_type_json: false,
            require_bearer: true,
        }),
        "get_greenbooks_report_ap_aging" => Ok(ValidatedArgs::HttpRequest {
            method: Method::GET,
            url: make_url("/api/greenbooks/reports?type=ap-aging")?,
            body: None,
            content_type_json: false,
            require_bearer: true,
        }),
        "get_greenbooks_report_fx_summary" => Ok(ValidatedArgs::HttpRequest {
            method: Method::GET,
            url: make_url("/api/greenbooks/reports?type=fx-summary")?,
            body: None,
            content_type_json: false,
            require_bearer: true,
        }),
        "get_greenbooks_report_gst_summary" => Ok(ValidatedArgs::HttpRequest {
            method: Method::GET,
            url: make_url("/api/greenbooks/reports?type=gst-summary")?,
            body: None,
            content_type_json: false,
            require_bearer: true,
        }),
        "export_greenbooks_reports" => {
            let mut url = url::Url::parse(&make_url("/api/greenbooks/reports")?).unwrap();
            {
                let mut qp = url.query_pairs_mut();
                qp.append_pair("format", "csv");
                if let Some(v) = params.get("type").and_then(|v| v.as_str()) {
                    qp.append_pair("type", v);
                }
                if let Some(v) = params.get("asOfDate").and_then(|v| v.as_str()) {
                    qp.append_pair("asOfDate", v);
                }
                if let Some(v) = params.get("startDate").and_then(|v| v.as_str()) {
                    qp.append_pair("startDate", v);
                }
                if let Some(v) = params.get("endDate").and_then(|v| v.as_str()) {
                    qp.append_pair("endDate", v);
                }
            }
            Ok(ValidatedArgs::HttpRequest {
                method: Method::GET,
                url: url.to_string(),
                body: None,
                content_type_json: false,
                require_bearer: true,
            })
        }

        #[cfg(test)]
        "sleep" => {
            let ms = params
                .get("ms")
                .and_then(|v| v.as_u64())
                .ok_or_else(|| "missing required param: ms".to_string())?;
            Ok(ValidatedArgs::Sleep { ms })
        }

        _ => Err("unsupported tool".into()),
    }
}

#[derive(Debug)]
enum Outcome {
    Ok(ToolOk),
    Timeout,
    Cancelled,
    EgressErr(EgressError),
}

fn classify_egress_error(e: &EgressError) -> (&'static str, String) {
    match e {
        EgressError::HostNotAllowed(_) | EgressError::PrivateIpBlocked(_) => {
            ("egress_blocked", e.to_string())
        }
        EgressError::RequestBodyTooLarge { .. } | EgressError::InvalidUrl(_) => {
            ("validation_error", e.to_string())
        }
        EgressError::Http(inner) if inner.is_timeout() => ("timeout", e.to_string()),
        _ => ("runtime_error", e.to_string()),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audit::sink::tests::CaptureSink;
    use crate::egress::EgressConfig;
    use std::sync::Arc;

    fn router_denying_all() -> ToolRouter {
        let cfg = EgressConfig {
            deny_private_ips: false,
            ..EgressConfig::default()
        };
        ToolRouter::new(EgressClient::new(cfg))
    }

    fn router_with_audit() -> (ToolRouter, Arc<CaptureSink>) {
        let cfg = EgressConfig {
            deny_private_ips: false,
            ..EgressConfig::default()
        };
        let capture = Arc::new(CaptureSink::default());
        let audit = AuditLog::new(capture.clone());
        let router = ToolRouter::new(EgressClient::new(cfg)).with_audit(audit);
        (router, capture)
    }

    fn router_with_runtime(cfg: ToolRuntimeConfig) -> ToolRouter {
        let egress_cfg = EgressConfig {
            deny_private_ips: false,
            ..EgressConfig::default()
        };
        ToolRouter::new_with_config(EgressClient::new(egress_cfg), cfg)
    }

    fn default_ctx() -> ToolAuditCtx {
        ToolAuditCtx {
            request_id: "req-test".into(),
            source_ip: "127.0.0.1".into(),
            user_agent: None,
            actor: None,
            upstream_authorization: None,
            upstream_cookie: None,
            cancel: None,
        }
    }

    #[tokio::test]
    async fn unknown_tool_returns_error() {
        let router = router_denying_all();
        let result = router
            .execute(
                ToolRequest {
                    tool: "nope".into(),
                    params: serde_json::json!({}),
                },
                default_ctx(),
            )
            .await;
        assert!(!result.success);
        assert!(result.data.contains("unknown tool"));
    }

    #[tokio::test]
    async fn http_get_denied_host() {
        let router = router_denying_all();
        let result = router
            .execute(
                ToolRequest {
                    tool: "http_get".into(),
                    params: serde_json::json!({ "url": "https://evil.example.com" }),
                },
                default_ctx(),
            )
            .await;
        assert!(!result.success);
        assert!(result.data.contains("not in allowlist"));
    }

    #[tokio::test]
    async fn http_post_denied_host() {
        let router = router_denying_all();
        let result = router
            .execute(
                ToolRequest {
                    tool: "http_post".into(),
                    params: serde_json::json!({
                        "url": "https://evil.example.com",
                        "body": "hi"
                    }),
                },
                default_ctx(),
            )
            .await;
        assert!(!result.success);
        assert!(result.data.contains("not in allowlist"));
    }

    #[tokio::test]
    async fn http_get_missing_url_param() {
        let router = router_denying_all();
        let result = router
            .execute(
                ToolRequest {
                    tool: "http_get".into(),
                    params: serde_json::json!({}),
                },
                default_ctx(),
            )
            .await;
        assert!(!result.success);
        assert!(result.data.contains("missing required param"));
    }

    #[tokio::test]
    async fn http_post_body_too_large_rejected_before_dispatch() {
        let mut egress_cfg = EgressConfig::default();
        egress_cfg.allowed_hosts.insert("api.example.com".into());
        egress_cfg.deny_private_ips = false;
        egress_cfg.max_request_body_bytes = 10;

        let router = ToolRouter::new(EgressClient::new(egress_cfg));

        let result = router
            .execute(
                ToolRequest {
                    tool: "http_post".into(),
                    params: serde_json::json!({
                        "url": "https://api.example.com/v1",
                        "body": "this is way more than ten bytes long"
                    }),
                },
                default_ctx(),
            )
            .await;
        assert!(!result.success);
        assert!(result.data.contains("request body"));
    }

    #[tokio::test]
    async fn unknown_tool_emits_rejected_audit() {
        let (router, capture) = router_with_audit();
        let _result = router
            .execute(
                ToolRequest {
                    tool: "nope".into(),
                    params: serde_json::json!({}),
                },
                default_ctx(),
            )
            .await;

        let events = capture.events.lock().unwrap();
        assert_eq!(events.len(), 1);
        assert!(events[0].contains("tool.invoke_rejected"));
        assert!(events[0].contains("tool_not_implemented"));
    }

    #[tokio::test]
    async fn denied_host_emits_start_and_egress_blocked() {
        let (router, capture) = router_with_audit();
        let _result = router
            .execute(
                ToolRequest {
                    tool: "http_get".into(),
                    params: serde_json::json!({ "url": "https://evil.example.com" }),
                },
                default_ctx(),
            )
            .await;

        let events = capture.events.lock().unwrap();
        // Should have: invoke_start, egress_blocked, invoke_failure
        assert!(
            events.len() >= 2,
            "got {} events: {:?}",
            events.len(),
            *events
        );
        assert!(events.iter().any(|e| e.contains("tool.invoke_start")));
        assert!(events.iter().any(|e| e.contains("gateway.egress_blocked")));
    }

    #[tokio::test]
    async fn queue_full_rejected_and_audited() {
        let mut cfg = ToolRuntimeConfig::builtins();
        cfg.max_queue = 1;
        cfg.queue_timeout = Duration::from_secs(1);
        cfg.max_concurrent_global = 1;
        // Ensure sleep is enabled with a long timeout.
        cfg.tools.insert(
            "sleep".into(),
            ToolRuntimeToolConfig {
                enabled: true,
                timeout: Duration::from_secs(5),
                max_concurrent: 1,
            },
        );

        let capture = Arc::new(CaptureSink::default());
        let audit = AuditLog::new(capture.clone());
        let router = router_with_runtime(cfg).with_audit(audit);

        // Start one long-running call that occupies the only queue slot.
        let r1 = router.clone();
        let h1 = tokio::spawn(async move {
            r1.execute(
                ToolRequest {
                    tool: "sleep".into(),
                    params: serde_json::json!({ "ms": 200 }),
                },
                default_ctx(),
            )
            .await
        });

        // Give it a moment to admit.
        tokio::time::sleep(Duration::from_millis(10)).await;

        // Second call should be rejected immediately.
        let result2 = router
            .execute(
                ToolRequest {
                    tool: "sleep".into(),
                    params: serde_json::json!({ "ms": 10 }),
                },
                default_ctx(),
            )
            .await;
        assert!(!result2.success);
        assert!(result2.data.contains("queue full"));

        let _ = h1.await;

        let events = capture.events.lock().unwrap();
        assert!(events
            .iter()
            .any(|e| e.contains("tool.invoke_rejected") && e.contains("queue_full")));
    }

    #[tokio::test]
    async fn queue_timeout_when_waiting_for_permits() {
        let mut cfg = ToolRuntimeConfig::builtins();
        cfg.max_queue = 2;
        cfg.queue_timeout = Duration::from_millis(20);
        cfg.max_concurrent_global = 1;
        cfg.tools.insert(
            "sleep".into(),
            ToolRuntimeToolConfig {
                enabled: true,
                timeout: Duration::from_secs(5),
                max_concurrent: 1,
            },
        );

        let router = router_with_runtime(cfg);

        let r1 = router.clone();
        let h1 = tokio::spawn(async move {
            r1.execute(
                ToolRequest {
                    tool: "sleep".into(),
                    params: serde_json::json!({ "ms": 200 }),
                },
                default_ctx(),
            )
            .await
        });

        tokio::time::sleep(Duration::from_millis(10)).await;

        let result2 = router
            .execute(
                ToolRequest {
                    tool: "sleep".into(),
                    params: serde_json::json!({ "ms": 10 }),
                },
                default_ctx(),
            )
            .await;

        assert!(!result2.success);
        assert!(result2.data.contains("timed out waiting"));

        let _ = h1.await;
    }

    #[tokio::test]
    async fn per_tool_timeout_enforced() {
        let mut cfg = ToolRuntimeConfig::builtins();
        cfg.max_queue = 8;
        cfg.max_concurrent_global = 8;
        cfg.tools.insert(
            "sleep".into(),
            ToolRuntimeToolConfig {
                enabled: true,
                timeout: Duration::from_millis(25),
                max_concurrent: 1,
            },
        );

        let router = router_with_runtime(cfg);

        let result = router
            .execute(
                ToolRequest {
                    tool: "sleep".into(),
                    params: serde_json::json!({ "ms": 250 }),
                },
                default_ctx(),
            )
            .await;

        assert!(!result.success);
        assert!(result.data.contains("timeout"));
    }

    #[tokio::test]
    async fn cancellation_aborts_tool() {
        let mut cfg = ToolRuntimeConfig::builtins();
        cfg.max_queue = 8;
        cfg.max_concurrent_global = 8;
        cfg.tools.insert(
            "sleep".into(),
            ToolRuntimeToolConfig {
                enabled: true,
                timeout: Duration::from_secs(5),
                max_concurrent: 1,
            },
        );

        let router = router_with_runtime(cfg);

        let cancel = CancellationToken::new();
        let mut ctx = default_ctx();
        ctx.cancel = Some(cancel.clone());

        let r1 = router.clone();
        let h1 = tokio::spawn(async move {
            r1.execute(
                ToolRequest {
                    tool: "sleep".into(),
                    params: serde_json::json!({ "ms": 500 }),
                },
                ctx,
            )
            .await
        });

        tokio::time::sleep(Duration::from_millis(20)).await;
        cancel.cancel();

        let result = h1.await.unwrap();
        assert!(!result.success);
        assert_eq!(result.data, "cancelled");
    }
}
