pub mod audit;
pub mod auth;
pub mod config;
pub mod egress;
pub mod error;
pub mod middleware;
pub mod rbac;
pub mod tool_router;

use axum::{
    body::Body,
    extract::Request,
    http::{HeaderName, HeaderValue, Method, StatusCode},
    middleware as axum_mw,
    response::{IntoResponse, Json, Response},
    Router,
};
use serde::Serialize;
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
            *resp.status_mut() = StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
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
            axum::routing::get(proxy_my_tasks).with_state(proxy_state),
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
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([header::CONTENT_TYPE, header::AUTHORIZATION, header::COOKIE])
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
