pub mod audit;
pub mod auth;
pub mod config;
pub mod egress;
pub mod error;
pub mod middleware;
pub mod rbac;
pub mod tool_router;

use axum::{
    body::{to_bytes, Body},
    extract::{Path, Query, Request},
    http::{HeaderName, HeaderValue, Method, StatusCode},
    middleware as axum_mw,
    response::{IntoResponse, Json, Response},
    Router,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use tower_http::{
    cors::CorsLayer,
    request_id::{MakeRequestId, PropagateRequestIdLayer, RequestId, SetRequestIdLayer},
    trace::TraceLayer,
};
use tracing::info_span;
use uuid::Uuid;

use crate::audit::AuditLog;
use crate::auth::{AuthState, BetterAuthClient};
use crate::config::GatewayConfig;
use crate::error::AppError;
use crate::middleware::auth::auth_middleware;
use crate::middleware::csrf::{csrf_middleware, CsrfConfig};
use crate::middleware::headers::header_hardening_middleware;
use crate::middleware::rate_limit::{rate_limit_middleware, RateLimiter};
use crate::middleware::rbac::{rbac_middleware, RbacState};
use crate::middleware::validate::{validate_request, ValidationConfig};
use crate::rbac::{Policy, PolicyEngine};
use crate::tool_router::{ToolAuditCtx, ToolRequest, ToolRouter};
use axum::extract::State;
use axum::http::header;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const SERVICE_NAME: &str = "api-mcp-gateway";

// ---------------------------------------------------------------------------
// Request-ID generator
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct MakeRequestUuid;

impl MakeRequestId for MakeRequestUuid {
    fn make_request_id<B>(&mut self, _request: &hyper::Request<B>) -> Option<RequestId> {
        let id = Uuid::new_v4().to_string();
        id.parse().ok().map(RequestId::new)
    }
}

// ---------------------------------------------------------------------------
// Route handlers
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
}

#[derive(Serialize)]
struct VersionResponse {
    service: &'static str,
    version: &'static str,
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse { status: "ok" })
}

async fn version() -> Json<VersionResponse> {
    Json(VersionResponse {
        service: SERVICE_NAME,
        version: VERSION,
    })
}

#[derive(Clone)]
struct ApiProxyState {
    client: reqwest::Client,
    upstream_base: String,
}

async fn proxy_my_tasks(
    State(state): State<ApiProxyState>,
    headers: axum::http::HeaderMap,
    request: Request,
) -> Response {
    let query = request.uri().query().unwrap_or("");
    let url = if query.is_empty() {
        format!("{}/api/my-tasks", state.upstream_base)
    } else {
        format!("{}/api/my-tasks?{}", state.upstream_base, query)
    };

    let mut upstream_req = state.client.get(url);

    if let Some(cookie) = headers.get(header::COOKIE) {
        upstream_req = upstream_req.header(header::COOKIE, cookie);
    }
    if let Some(authz) = headers.get(header::AUTHORIZATION) {
        upstream_req = upstream_req.header(header::AUTHORIZATION, authz);
    }

    match upstream_req.send().await {
        Ok(upstream) => {
            let status = upstream.status();
            let content_type = upstream
                .headers()
                .get(header::CONTENT_TYPE)
                .cloned()
                .unwrap_or_else(|| HeaderValue::from_static("application/json"));
            let body = upstream.bytes().await.unwrap_or_default();

            let mut resp = Response::new(Body::from(body));
            *resp.status_mut() =
                StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
            resp.headers_mut()
                .insert(header::CONTENT_TYPE, content_type);
            resp
        }
        Err(err) => {
            let payload = serde_json::json!({
                "error": {
                    "code": 502,
                    "kind": "upstream_unavailable",
                    "message": format!("failed to reach tools API: {err}"),
                }
            });
            (StatusCode::BAD_GATEWAY, Json(payload)).into_response()
        }
    }
}

async fn proxy_exponential(
    State(state): State<ApiProxyState>,
    Path(path): Path<String>,
    headers: axum::http::HeaderMap,
    request: Request,
) -> Response {
    proxy_api_path(state, format!("/api/exponential/{path}"), headers, request).await
}

async fn proxy_greenbooks(
    State(state): State<ApiProxyState>,
    Path(path): Path<String>,
    headers: axum::http::HeaderMap,
    request: Request,
) -> Response {
    proxy_api_path(state, format!("/api/greenbooks/{path}"), headers, request).await
}

async fn proxy_api_path(
    state: ApiProxyState,
    upstream_path: String,
    headers: axum::http::HeaderMap,
    request: Request,
) -> Response {
    let query = request.uri().query().unwrap_or("");
    let base = format!("{}{}", state.upstream_base, upstream_path);
    let url = if query.is_empty() { base } else { format!("{base}?{query}") };

    let method = request.method().clone();
    let body = match to_bytes(request.into_body(), 10 * 1024 * 1024).await {
        Ok(bytes) => bytes,
        Err(err) => {
            let payload = serde_json::json!({
                "error": {
                    "code": 400,
                    "kind": "invalid_request_body",
                    "message": format!("failed to read request body: {err}"),
                }
            });
            return (StatusCode::BAD_REQUEST, Json(payload)).into_response();
        }
    };

    let mut upstream_req = state.client.request(method, url).body(body);

    for h in [
        header::COOKIE,
        header::AUTHORIZATION,
        header::CONTENT_TYPE,
        header::ACCEPT,
        header::HeaderName::from_static("x-csrf-token"),
    ] {
        if let Some(val) = headers.get(&h) {
            upstream_req = upstream_req.header(&h, val);
        }
    }

    match upstream_req.send().await {
        Ok(upstream) => {
            let status = upstream.status();
            let upstream_headers = upstream.headers().clone();
            let body = upstream.bytes().await.unwrap_or_default();

            let mut resp = Response::new(Body::from(body));
            *resp.status_mut() =
                StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);

            for h in [
                header::CONTENT_TYPE,
                header::CACHE_CONTROL,
                header::ETAG,
                header::LOCATION,
                header::HeaderName::from_static("set-cookie"),
            ] {
                for value in upstream_headers.get_all(&h).iter() {
                    resp.headers_mut().append(&h, value.clone());
                }
            }

            resp
        }
        Err(err) => {
            let payload = serde_json::json!({
                "error": {
                    "code": 502,
                    "kind": "upstream_unavailable",
                    "message": format!("failed to reach tools upstream API: {err}"),
                }
            });
            (StatusCode::BAD_GATEWAY, Json(payload)).into_response()
        }
    }
}

// ---------------------------------------------------------------------------
// Tool routes (v1)
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct ToolsListResponse {
    tools: Vec<String>,
}

async fn list_tools(State(router): State<ToolRouter>) -> Json<ToolsListResponse> {
    // v0: list tools supported by this build.
    // (Authorization + enablement happens via RBAC + tool runtime config.)
    Json(ToolsListResponse {
        tools: router.supported_tool_names(),
    })
}

fn extract_source_ip(headers: &axum::http::HeaderMap) -> String {
    // X-Forwarded-For → X-Real-IP → "unknown"
    if let Some(val) = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok()) {
        if let Some(first) = val.split(',').next() {
            let t = first.trim();
            if !t.is_empty() {
                return t.to_owned();
            }
        }
    }
    if let Some(val) = headers.get("x-real-ip").and_then(|v| v.to_str().ok()) {
        let t = val.trim();
        if !t.is_empty() {
            return t.to_owned();
        }
    }
    "unknown".to_owned()
}

fn extract_user_agent(headers: &axum::http::HeaderMap) -> Option<String> {
    headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_owned())
}

async fn call_tool(
    State(router): State<ToolRouter>,
    headers: axum::http::HeaderMap,
    request_id: Option<axum::extract::Extension<RequestId>>,
    principal: Option<axum::extract::Extension<crate::auth::Principal>>,
    Json(payload): Json<ToolRequest>,
) -> Result<impl IntoResponse, AppError> {
    let request_id = request_id
        .as_ref()
        .and_then(|id| id.header_value().to_str().ok())
        .unwrap_or("unknown")
        .to_owned();

    let source_ip = extract_source_ip(&headers);
    let user_agent = extract_user_agent(&headers);

    let actor = principal.map(|p| crate::audit::actor_from_principal(&p.0));

    let upstream_authorization = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_owned());

    let upstream_cookie = headers
        .get(header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_owned());

    let ctx = ToolAuditCtx {
        request_id,
        source_ip,
        user_agent,
        actor,
        upstream_authorization,
        upstream_cookie,
        cancel: None,
    };

    let result = router.execute(payload, ctx).await;
    Ok(Json(result))
}

#[derive(Deserialize)]
struct DashboardQuery {
    limit: Option<usize>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct DashboardLog {
    id: String,
    timestamp: String,
    session_id: String,
    tool_name: String,
    params: Value,
    result: String,
    error: Option<String>,
    duration_ms: u64,
    acl_level: &'static str,
    risk_flags: RiskFlags,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct RiskFlags {
    read_private_data: bool,
    write_operation: bool,
    external_communication: bool,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct SessionMetrics {
    session_id: String,
    start_time: String,
    tool_calls: u64,
    errors: u64,
    blocked: u64,
    max_acl_level: &'static str,
    lethal_trifecta: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FirewallConfig {
    enabled: bool,
    default_policy: &'static str,
    tools_configured: usize,
    blocked_sessions: usize,
    data_leak_prevention: bool,
    lethal_trifecta_protection: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DashboardResponse {
    session: SessionMetrics,
    recent_logs: Vec<DashboardLog>,
    risk_level: &'static str,
    alerts: Vec<String>,
    firewall: FirewallConfig,
    all_sessions: Vec<SessionMetrics>,
}

async fn mcp_dashboard(
    State(router): State<ToolRouter>,
    Query(query): Query<DashboardQuery>,
) -> Json<DashboardResponse> {
    let limit = query.limit.unwrap_or(100).clamp(1, 500);

    let mut logs: Vec<DashboardLog> = Vec::new();
    let mut alerts: Vec<String> = Vec::new();

    let audit_path = std::env::var("AUDIT_LOG_FILE").unwrap_or_default();
    let audit_path = audit_path.trim().to_owned();

    if !audit_path.is_empty() {
        match tokio::fs::read_to_string(&audit_path).await {
            Ok(text) => {
                let lines: Vec<&str> = text.lines().collect();
                let start = lines.len().saturating_sub(limit);
                for (idx, line) in lines[start..].iter().enumerate() {
                    let Ok(evt) = serde_json::from_str::<Value>(line) else {
                        continue;
                    };

                    let event_type = evt
                        .get("event_type")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown");
                    let payload = evt.get("payload").cloned().unwrap_or_else(|| Value::Object(Default::default()));
                    let tool_name = payload
                        .get("tool_name")
                        .and_then(|v| v.as_str())
                        .unwrap_or(event_type)
                        .to_owned();

                    let result = match event_type {
                        "tool.invoke_success" => "success",
                        "tool.invoke_rejected" | "gateway.egress_blocked" => "blocked",
                        "tool.invoke_failure" => "error",
                        _ => "success",
                    }
                    .to_owned();

                    let error = payload
                        .get("error_message")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_owned())
                        .or_else(|| payload.get("reason").and_then(|v| v.as_str()).map(|s| s.to_owned()));

                    let duration_ms = payload
                        .get("duration_ms")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);

                    let session_id = evt
                        .get("actor")
                        .and_then(|a| a.get("user_id"))
                        .and_then(|v| v.as_str())
                        .unwrap_or_else(|| {
                            evt.get("request_id")
                                .and_then(|v| v.as_str())
                                .unwrap_or("unknown")
                        })
                        .to_owned();

                    let write_operation = tool_name.contains("create")
                        || tool_name.contains("update")
                        || tool_name.contains("delete")
                        || tool_name.contains("write")
                        || tool_name.contains("patch");
                    let external_communication = event_type.starts_with("gateway.egress");

                    logs.push(DashboardLog {
                        id: evt
                            .get("event_id")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_owned())
                            .unwrap_or_else(|| format!("{session_id}-{idx}")),
                        timestamp: evt
                            .get("timestamp")
                            .and_then(|v| v.as_str())
                            .unwrap_or("1970-01-01T00:00:00Z")
                            .to_owned(),
                        session_id,
                        tool_name,
                        params: payload,
                        result,
                        error,
                        duration_ms,
                        acl_level: "PUBLIC",
                        risk_flags: RiskFlags {
                            read_private_data: false,
                            write_operation,
                            external_communication,
                        },
                    });
                }
            }
            Err(err) => {
                alerts.push(format!("audit log unavailable: {err}"));
            }
        }
    } else {
        alerts.push("AUDIT_LOG_FILE is not configured; showing live empty state".to_string());
    }

    let mut sessions: HashMap<String, SessionMetrics> = HashMap::new();
    for log in &logs {
        let entry = sessions.entry(log.session_id.clone()).or_insert(SessionMetrics {
            session_id: log.session_id.clone(),
            start_time: log.timestamp.clone(),
            tool_calls: 0,
            errors: 0,
            blocked: 0,
            max_acl_level: "PUBLIC",
            lethal_trifecta: false,
        });

        entry.tool_calls += 1;
        if log.result == "error" {
            entry.errors += 1;
        }
        if log.result == "blocked" {
            entry.blocked += 1;
        }
        if log.timestamp < entry.start_time {
            entry.start_time = log.timestamp.clone();
        }
        let rf = &log.risk_flags;
        if rf.read_private_data && rf.write_operation && rf.external_communication {
            entry.lethal_trifecta = true;
        }
    }

    let mut all_sessions: Vec<SessionMetrics> = sessions.values().cloned().collect();
    all_sessions.sort_by(|a, b| b.start_time.cmp(&a.start_time));

    let total_calls = logs.len() as u64;
    let total_errors: u64 = all_sessions.iter().map(|s| s.errors).sum();
    let total_blocked: u64 = all_sessions.iter().map(|s| s.blocked).sum();

    let risk_level = if total_blocked > 10 || total_errors > 15 {
        "high"
    } else if total_blocked > 0 || total_errors > 0 {
        "medium"
    } else {
        "low"
    };

    let blocked_sessions = all_sessions.iter().filter(|s| s.blocked > 0).count();

    let default_session = all_sessions.first().cloned().unwrap_or(SessionMetrics {
        session_id: "none".to_string(),
        start_time: "1970-01-01T00:00:00Z".to_string(),
        tool_calls: 0,
        errors: 0,
        blocked: 0,
        max_acl_level: "PUBLIC",
        lethal_trifecta: false,
    });

    if total_calls == 0 {
        alerts.push("No recent audit events found in configured log".to_string());
    }

    Json(DashboardResponse {
        session: default_session,
        recent_logs: logs.into_iter().rev().take(20).collect(),
        risk_level,
        alerts,
        firewall: FirewallConfig {
            enabled: true,
            default_policy: "deny",
            tools_configured: router.supported_tool_names().len(),
            blocked_sessions,
            data_leak_prevention: true,
            lethal_trifecta_protection: true,
        },
        all_sessions,
    })
}

// ---------------------------------------------------------------------------
// Structured fallback (404 with request_id)
// ---------------------------------------------------------------------------

async fn fallback_handler(request: Request) -> AppError {
    let request_id = request
        .extensions()
        .get::<RequestId>()
        .and_then(|id| id.header_value().to_str().ok())
        .map(String::from);

    let path = request.uri().path().to_string();
    let err = AppError::not_found(format!("no route matches {path}"));
    match request_id {
        Some(id) => err.with_request_id(id),
        None => err,
    }
}

// ---------------------------------------------------------------------------
// App builder
// ---------------------------------------------------------------------------

/// Build the axum [`Router`] with all middleware wired up.
///
/// Middleware execution order (outermost → innermost):
///   SetRequestId → Trace → PropagateRequestId → RateLimit → Validate → CSRF → Auth → RBAC → Handler
///
/// The `audit_log` parameter is optional; pass `None` to use the default
/// env-configured sink (stdout + optional file).
pub fn app(config: &GatewayConfig, audit_log: Option<AuditLog>) -> Router {
    let x_request_id = HeaderName::from_static("x-request-id");

    let audit = audit_log.unwrap_or_else(AuditLog::from_env);

    let rate_limiter = RateLimiter::new(config.rate_limit_rps, config.rate_limit_burst);
    let validation = ValidationConfig {
        max_body_size: config.max_body_size,
        audit: audit.clone(),
    };
    let csrf = CsrfConfig {
        enabled: config.csrf_enabled,
        cookie_name: config.csrf_cookie_name.clone(),
        header_name: config.csrf_header_name.clone(),
        audit: audit.clone(),
        ..CsrfConfig::default()
    };

    // Load RBAC policy (if present) once and reuse it for RBAC + tool runtime bounds.
    let policy: Option<Policy> = config.policy_file.as_ref().map(|policy_path| {
        let path = std::path::Path::new(policy_path);
        let policy = Policy::load_from_file(path).unwrap_or_else(|e| {
            panic!("failed to load policy from {policy_path}: {e}");
        });
        tracing::info!(
            policy_file = %policy_path,
            schema_version = %policy.schema_version,
            policy_id = policy.id.as_deref().unwrap_or("-"),
            "RBAC policy loaded"
        );
        policy
    });

    // Tool router (Option-A shims + audit log access)
    let tool_cfg = policy
        .as_ref()
        .and_then(crate::tool_router::ToolRuntimeConfig::from_rbac_policy)
        .unwrap_or_else(crate::tool_router::ToolRuntimeConfig::builtins);

    let tool_router = ToolRouter::new_with_config(
        crate::egress::EgressClient::new(crate::egress::EgressConfig::from_env()),
        tool_cfg,
    )
    .with_audit(audit.clone());

    let proxy_state = ApiProxyState {
        client: reqwest::Client::new(),
        upstream_base: config.betterauth_base_url.trim_end_matches('/').to_string(),
    };

    let mut router = Router::new()
        .route("/health", axum::routing::get(health))
        .route("/version", axum::routing::get(version))
        .route(
            "/api/my-tasks",
            axum::routing::get(proxy_my_tasks).with_state(proxy_state.clone()),
        )
        .route(
            "/api/exponential/{*path}",
            axum::routing::any(proxy_exponential).with_state(proxy_state.clone()),
        )
        .route(
            "/api/greenbooks/{*path}",
            axum::routing::any(proxy_greenbooks).with_state(proxy_state),
        )
        .route(
            "/api/mcp/dashboard",
            axum::routing::get(mcp_dashboard).with_state(tool_router.clone()),
        )
        .route(
            "/v1/tools",
            axum::routing::get(list_tools).with_state(tool_router.clone()),
        )
        .route(
            "/v1/tools/call",
            axum::routing::post(call_tool).with_state(tool_router.clone()),
        )
        .fallback(fallback_handler);

    // --- innermost first ---

    // RBAC layer (runs after auth, before handlers).
    if let Some(policy) = policy.clone() {
        let engine = std::sync::Arc::new(PolicyEngine::new(policy));
        let rbac_state = RbacState::new(engine, audit.clone());
        router = router.layer(axum_mw::from_fn_with_state(rbac_state, rbac_middleware));
    }

    // Auth layer (runs before RBAC).
    if config.auth_enabled {
        // Auth validation strategy:
        // - Cookie sessions: always validated via BetterAuth upstream.
        // - Bearer tokens: prefer local JWT validation via JWKS when configured; otherwise
        //   fall back to BetterAuth upstream.
        let validator: std::sync::Arc<dyn crate::auth::SessionValidator> = {
            // BetterAuth client (cookie validation always uses this).
            let ba: std::sync::Arc<dyn crate::auth::SessionValidator> =
                std::sync::Arc::new(BetterAuthClient::new(
                    config.betterauth_base_url.clone(),
                    std::time::Duration::from_millis(config.betterauth_timeout_ms),
                    config.betterauth_cookie_name.clone(),
                ));

            // If JWT is configured, validate bearer tokens locally, but keep cookie sessions
            // going through BetterAuth.
            if let Some(jwt_cfg) = crate::auth::jwt::JwtAuthConfig::from_env() {
                match crate::auth::jwt::JwtValidator::new(jwt_cfg) {
                    Ok(jwt) => {
                        let jwt: std::sync::Arc<dyn crate::auth::SessionValidator> =
                            std::sync::Arc::new(jwt);
                        std::sync::Arc::new(crate::auth::SplitValidator {
                            cookie: ba,
                            bearer: jwt,
                        })
                    }
                    Err(e) => {
                        tracing::warn!(error = ?e, "JWT validator init failed; using BetterAuth for all auth");
                        ba
                    }
                }
            } else {
                ba
            }
        };

        let auth_state =
            AuthState::with_cookie_name(validator, config.betterauth_cookie_name.clone())
                .with_audit(audit.clone());
        router = router.layer(axum_mw::from_fn_with_state(auth_state, auth_middleware));
    }

    let rate_state = crate::middleware::rate_limit::RateLimitState {
        limiter: rate_limiter,
        audit: audit.clone(),
    };

    let cors = CorsLayer::new()
        .allow_origin([
            "https://tools.greenhatsec.com"
                .parse::<HeaderValue>()
                .expect("valid tools origin"),
            "https://admin.greenhatsec.com"
                .parse::<HeaderValue>()
                .expect("valid admin origin"),
        ])
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::PATCH,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([
            header::CONTENT_TYPE,
            header::AUTHORIZATION,
            header::COOKIE,
            header::ACCEPT,
            header::HeaderName::from_static("x-csrf-token"),
        ])
        .allow_credentials(true);

    router
        .layer(axum_mw::from_fn_with_state(csrf, csrf_middleware))
        .layer(axum_mw::from_fn_with_state(validation, validate_request))
        .layer(axum_mw::from_fn_with_state(
            rate_state,
            rate_limit_middleware,
        ))
        // --- observability / request-id (outermost) ---
        // Header hardening should run at the edge.
        .layer(axum_mw::from_fn(header_hardening_middleware))
        .layer(cors)
        .layer(PropagateRequestIdLayer::new(x_request_id.clone()))
        .layer(
            TraceLayer::new_for_http().make_span_with(|req: &hyper::Request<_>| {
                let request_id = req
                    .extensions()
                    .get::<RequestId>()
                    .and_then(|id| id.header_value().to_str().ok())
                    .unwrap_or("unknown")
                    .to_owned();

                info_span!(
                    "http_request",
                    method = %req.method(),
                    uri = %req.uri(),
                    request_id = %request_id,
                )
            }),
        )
        .layer(SetRequestIdLayer::new(x_request_id, MakeRequestUuid))
}
