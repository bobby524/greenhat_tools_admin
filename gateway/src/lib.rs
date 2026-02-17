pub mod audit;
pub mod auth;
pub mod config;
pub mod egress;
pub mod error;
pub mod middleware;
pub mod observability;
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
use serde_json::{Map, Value};
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
use crate::egress::{EgressClient, EgressError};
use crate::error::AppError;
use crate::middleware::auth::auth_middleware;
use crate::middleware::csrf::{csrf_middleware, CsrfConfig};
use crate::middleware::headers::header_hardening_middleware;
use crate::middleware::rate_limit::{rate_limit_middleware, RateLimiter};
use crate::middleware::rbac::{rbac_middleware, RbacState};
use crate::middleware::validate::{validate_request, ValidationConfig};
use crate::observability::{request_log_middleware, Observability, UpstreamTrace, X_REQUEST_ID};
use crate::rbac::{Policy, PolicyEngine};
use crate::tool_router::{ToolAuditCtx, ToolRequest, ToolRouter};
use axum::extract::State;
use axum::http::header;
use std::time::Instant;

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

#[allow(dead_code)]
#[derive(Clone)]
struct EgressProxyState {
    client: EgressClient,
    upstream_base: String,
}

async fn proxy_my_tasks(
    State(state): State<ApiProxyState>,
    headers: axum::http::HeaderMap,
    request_id: Option<axum::extract::Extension<RequestId>>,
    request: Request,
) -> Response {
    let query = request.uri().query().unwrap_or("");
    let url = if query.is_empty() {
        format!("{}/api/my-tasks?__gateway_internal=1", state.upstream_base)
    } else {
        format!(
            "{}/api/my-tasks?{}&__gateway_internal=1",
            state.upstream_base, query
        )
    };

    let canonical_request_id = request_id_from_extension(request_id);

    let mut upstream_req = state
        .client
        .get(url)
        .header("x-gateway-internal", "1")
        .header(header::USER_AGENT, "lua-resty-http")
        .header(X_REQUEST_ID, canonical_request_id);
    if let Some(cookie) = headers.get(header::COOKIE) {
        upstream_req = upstream_req.header(header::COOKIE, cookie);
    }
    if let Some(authz) = headers.get(header::AUTHORIZATION) {
        upstream_req = upstream_req.header(header::AUTHORIZATION, authz);
    }

    let start = Instant::now();
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
            resp.extensions_mut().insert(UpstreamTrace {
                status: Some(status.as_u16()),
                latency_ms: Some(start.elapsed().as_millis() as u64),
                timeout_hit: false,
                error_kind: None,
            });
            resp
        }
        Err(err) => {
            let timeout_hit = err.is_timeout();
            let payload = serde_json::json!({
                "error": {
                    "code": 502,
                    "kind": "upstream_unavailable",
                    "message": format!("failed to reach tools API: {err}"),
                }
            });
            let mut resp = (StatusCode::BAD_GATEWAY, Json(payload)).into_response();
            resp.extensions_mut().insert(UpstreamTrace {
                status: None,
                latency_ms: Some(start.elapsed().as_millis() as u64),
                timeout_hit,
                error_kind: Some("upstream_unavailable".to_owned()),
            });
            resp
        }
    }
}

async fn proxy_greenbooks(
    State(state): State<ApiProxyState>,
    Path(path): Path<String>,
    headers: axum::http::HeaderMap,
    request_id: Option<axum::extract::Extension<RequestId>>,
    request: Request,
) -> Response {
    proxy_api_path(
        state,
        format!("/api/greenbooks/{path}"),
        headers,
        request_id,
        request,
    )
    .await
}

async fn proxy_greenspot(
    State(state): State<ApiProxyState>,
    Path(path): Path<String>,
    headers: axum::http::HeaderMap,
    request_id: Option<axum::extract::Extension<RequestId>>,
    request: Request,
) -> Response {
    proxy_api_path(
        state,
        format!("/api/greenspot/{path}"),
        headers,
        request_id,
        request,
    )
    .await
}

async fn proxy_users(
    State(state): State<ApiProxyState>,
    headers: axum::http::HeaderMap,
    request_id: Option<axum::extract::Extension<RequestId>>,
    request: Request,
) -> Response {
    proxy_api_path(
        state,
        "/api/users".to_string(),
        headers,
        request_id,
        request,
    )
    .await
}

const EXPONENTIAL_ORG_ID: &str = "cd861b76-f85c-4afc-b3e8-8f85945c3132";

fn supabase_env() -> Result<(String, String), Response> {
    let base = std::env::var("SUPABASE_URL")
        .ok()
        .or_else(|| std::env::var("NEXT_PUBLIC_SUPABASE_URL").ok())
        .unwrap_or_default()
        .trim()
        .trim_end_matches('/')
        .to_string();
    let key = std::env::var("SUPABASE_SERVICE_ROLE_KEY")
        .unwrap_or_default()
        .trim()
        .to_string();
    if base.is_empty() || key.is_empty() {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": "Supabase env not configured in gateway" })),
        )
            .into_response());
    }
    Ok((base, key))
}

fn supabase_client_with_key(key: &str) -> reqwest::Client {
    let mut default_headers = reqwest::header::HeaderMap::new();
    if let Ok(v) = reqwest::header::HeaderValue::from_str(key) {
        default_headers.insert("apikey", v.clone());
        default_headers.insert(reqwest::header::AUTHORIZATION, reqwest::header::HeaderValue::from_str(&format!("Bearer {key}")).unwrap_or(v));
    }
    reqwest::Client::builder()
        .default_headers(default_headers)
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
}

async fn get_exponential_labels(
    principal: Option<axum::extract::Extension<crate::auth::Principal>>,
) -> Response {
    if principal.is_none() {
        return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error":"Unauthorized"}))).into_response();
    }
    let (base, key) = match supabase_env() { Ok(v) => v, Err(r) => return r };
    let client = supabase_client_with_key(&key);
    let mut url = url::Url::parse(&format!("{base}/rest/v1/labels")).unwrap();
    {
        let mut qp = url.query_pairs_mut();
        qp.append_pair("select", "*");
        qp.append_pair("org_id", &format!("eq.{EXPONENTIAL_ORG_ID}"));
        qp.append_pair("order", "name.asc");
    }
    match client.get(url.as_str()).send().await {
        Ok(resp) => {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            if !status.is_success() {
                return (status, Body::from(body)).into_response();
            }
            let labels: serde_json::Value = serde_json::from_str(&body).unwrap_or_else(|_| serde_json::json!([]));
            Json(serde_json::json!({"labels": labels})).into_response()
        }
        Err(e) => (StatusCode::BAD_GATEWAY, Json(serde_json::json!({"error": e.to_string()}))).into_response(),
    }
}

async fn create_exponential_label(
    principal: Option<axum::extract::Extension<crate::auth::Principal>>,
    request: Request,
) -> Response {
    if principal.is_none() {
        return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error":"Unauthorized"}))).into_response();
    }
    let (base, key) = match supabase_env() { Ok(v) => v, Err(r) => return r };
    let body = match to_bytes(request.into_body(), 1024 * 1024).await {
        Ok(b) => b,
        Err(e) => return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": format!("invalid body: {e}")}))).into_response(),
    };
    let parsed: serde_json::Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(_) => return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error":"Invalid JSON"}))).into_response(),
    };
    let name = parsed.get("name").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
    let color = parsed.get("color").and_then(|v| v.as_str()).unwrap_or("").to_string();
    if name.is_empty() { return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error":"Label name is required"}))).into_response(); }
    if color.is_empty() { return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error":"Label color is required"}))).into_response(); }
    let client = supabase_client_with_key(&key);
    let url = format!("{base}/rest/v1/labels");
    let payload = serde_json::json!([{"org_id": EXPONENTIAL_ORG_ID, "name": name, "color": color}]);
    match client.post(url).header("Prefer", "return=representation").json(&payload).send().await {
        Ok(resp) => {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            if !status.is_success() { return (status, Body::from(body)).into_response(); }
            let rows: serde_json::Value = serde_json::from_str(&body).unwrap_or_else(|_| serde_json::json!([]));
            let label = rows.as_array().and_then(|a| a.first()).cloned().unwrap_or_else(|| serde_json::json!({}));
            (StatusCode::CREATED, Json(serde_json::json!({"label": label}))).into_response()
        }
        Err(e) => (StatusCode::BAD_GATEWAY, Json(serde_json::json!({"error": e.to_string()}))).into_response(),
    }
}

async fn delete_exponential_label(
    principal: Option<axum::extract::Extension<crate::auth::Principal>>,
    Query(q): Query<std::collections::HashMap<String, String>>,
) -> Response {
    if principal.is_none() {
        return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error":"Unauthorized"}))).into_response();
    }
    let id = match q.get("id") { Some(v) if !v.is_empty() => v.clone(), _ => return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error":"Label id is required"}))).into_response() };
    let (base, key) = match supabase_env() { Ok(v) => v, Err(r) => return r };
    let client = supabase_client_with_key(&key);
    let mut url = url::Url::parse(&format!("{base}/rest/v1/labels")).unwrap();
    url.query_pairs_mut().append_pair("id", &format!("eq.{id}"));
    match client.delete(url.as_str()).send().await {
        Ok(resp) => {
            if !resp.status().is_success() { return (resp.status(), Body::from(resp.text().await.unwrap_or_default())).into_response(); }
            Json(serde_json::json!({"success": true})).into_response()
        }
        Err(e) => (StatusCode::BAD_GATEWAY, Json(serde_json::json!({"error": e.to_string()}))).into_response(),
    }
}

async fn get_exponential_views(
    principal: Option<axum::extract::Extension<crate::auth::Principal>>,
) -> Response {
    let user_id = match principal { Some(axum::extract::Extension(p)) => p.user_id, None => return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error":"Unauthorized"}))).into_response() };
    let (base, key) = match supabase_env() { Ok(v) => v, Err(r) => return r };
    let client = supabase_client_with_key(&key);
    let mut url = url::Url::parse(&format!("{base}/rest/v1/exponential_saved_views")).unwrap();
    {
        let mut qp = url.query_pairs_mut();
        qp.append_pair("select", "*");
        qp.append_pair("org_id", &format!("eq.{EXPONENTIAL_ORG_ID}"));
        qp.append_pair("user_id", &format!("eq.{user_id}"));
        qp.append_pair("order", "updated_at.desc");
    }
    match client.get(url.as_str()).send().await {
        Ok(resp) => {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            if !status.is_success() { return (status, Body::from(body)).into_response(); }
            let views: serde_json::Value = serde_json::from_str(&body).unwrap_or_else(|_| serde_json::json!([]));
            Json(serde_json::json!({"views": views})).into_response()
        }
        Err(e) => (StatusCode::BAD_GATEWAY, Json(serde_json::json!({"error": e.to_string()}))).into_response(),
    }
}

async fn create_exponential_view(
    principal: Option<axum::extract::Extension<crate::auth::Principal>>,
    request: Request,
) -> Response {
    let user_id = match principal { Some(axum::extract::Extension(p)) => p.user_id, None => return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error":"Unauthorized"}))).into_response() };
    let (base, key) = match supabase_env() { Ok(v) => v, Err(r) => return r };
    let body = match to_bytes(request.into_body(), 1024 * 1024).await { Ok(b) => b, Err(e) => return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": format!("invalid body: {e}")}))).into_response() };
    let parsed: serde_json::Value = match serde_json::from_slice(&body) { Ok(v) => v, Err(_) => return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error":"Invalid JSON"}))).into_response() };
    let name = parsed.get("name").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
    if name.is_empty() { return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error":"name is required"}))).into_response(); }
    let payload = serde_json::json!([{
        "org_id": EXPONENTIAL_ORG_ID,
        "user_id": user_id,
        "name": name,
        "filters": parsed.get("filters").cloned().unwrap_or_else(|| serde_json::json!({})),
        "sort_field": parsed.get("sort_field").and_then(|v| v.as_str()).unwrap_or("updated_at"),
        "sort_dir": parsed.get("sort_dir").and_then(|v| v.as_str()).unwrap_or("desc"),
    }]);
    let client = supabase_client_with_key(&key);
    let mut url = url::Url::parse(&format!("{base}/rest/v1/exponential_saved_views")).unwrap();
    url.query_pairs_mut().append_pair("on_conflict", "org_id,user_id,name");
    match client
        .post(url.as_str())
        .header("Prefer", "resolution=merge-duplicates,return=representation")
        .json(&payload)
        .send()
        .await
    {
        Ok(resp) => {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            if !status.is_success() { return (status, Body::from(body)).into_response(); }
            let rows: serde_json::Value = serde_json::from_str(&body).unwrap_or_else(|_| serde_json::json!([]));
            let view = rows.as_array().and_then(|a| a.first()).cloned().unwrap_or_else(|| serde_json::json!({}));
            (StatusCode::CREATED, Json(serde_json::json!({"view": view}))).into_response()
        }
        Err(e) => (StatusCode::BAD_GATEWAY, Json(serde_json::json!({"error": e.to_string()}))).into_response(),
    }
}

async fn delete_exponential_view(
    principal: Option<axum::extract::Extension<crate::auth::Principal>>,
    Path(view_id): Path<String>,
) -> Response {
    let user_id = match principal { Some(axum::extract::Extension(p)) => p.user_id, None => return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error":"Unauthorized"}))).into_response() };
    let (base, key) = match supabase_env() { Ok(v) => v, Err(r) => return r };
    let client = supabase_client_with_key(&key);
    let mut url = url::Url::parse(&format!("{base}/rest/v1/exponential_saved_views")).unwrap();
    {
        let mut qp = url.query_pairs_mut();
        qp.append_pair("id", &format!("eq.{view_id}"));
        qp.append_pair("user_id", &format!("eq.{user_id}"));
    }
    match client.delete(url.as_str()).send().await {
        Ok(resp) => {
            if !resp.status().is_success() { return (resp.status(), Body::from(resp.text().await.unwrap_or_default())).into_response(); }
            Json(serde_json::json!({"success": true})).into_response()
        }
        Err(e) => (StatusCode::BAD_GATEWAY, Json(serde_json::json!({"error": e.to_string()}))).into_response(),
    }
}

async fn get_exponential_project_assignees(
    principal: Option<axum::extract::Extension<crate::auth::Principal>>,
    Path(project_id): Path<String>,
) -> Response {
    if principal.is_none() {
        return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error":"Unauthorized"}))).into_response();
    }
    let (base, key) = match supabase_env() { Ok(v) => v, Err(r) => return r };
    let client = supabase_client_with_key(&key);
    let mut url = url::Url::parse(&format!("{base}/rest/v1/project_assignees_view")).unwrap();
    {
        let mut qp = url.query_pairs_mut();
        qp.append_pair("select", "user_id,first_name,last_name,email,avatar_url,project_role,display_name");
        qp.append_pair("project_id", &format!("eq.{project_id}"));
        qp.append_pair("order", "display_name.asc");
    }
    match client.get(url.as_str()).send().await {
        Ok(resp) => {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            if !status.is_success() { return (status, Body::from(body)).into_response(); }
            let rows: Vec<serde_json::Value> = serde_json::from_str(&body).unwrap_or_default();
            let mut seen = std::collections::HashSet::new();
            let mut dedup = Vec::new();
            for row in rows {
                let uid = row.get("user_id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                if !uid.is_empty() && seen.insert(uid) { dedup.push(row); }
            }
            Json(serde_json::json!({"assignees": dedup, "count": dedup.len()})).into_response()
        }
        Err(e) => (StatusCode::BAD_GATEWAY, Json(serde_json::json!({"error": e.to_string()}))).into_response(),
    }
}

async fn proxy_api_path(
    state: ApiProxyState,
    upstream_path: String,
    headers: axum::http::HeaderMap,
    request_id: Option<axum::extract::Extension<RequestId>>,
    request: Request,
) -> Response {
    let query = request.uri().query().unwrap_or("");
    let base = format!("{}{}", state.upstream_base, upstream_path);
    let url = if query.is_empty() {
        format!("{base}?__gateway_internal=1")
    } else {
        format!("{base}?{query}&__gateway_internal=1")
    };

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

    let canonical_request_id = request_id_from_extension(request_id);

    let mut upstream_req = state
        .client
        .request(method, url)
        .header("x-gateway-internal", "1")
        .header(header::USER_AGENT, "lua-resty-http")
        .header(X_REQUEST_ID, canonical_request_id)
        .body(body);

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

    let start = Instant::now();
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

            resp.extensions_mut().insert(UpstreamTrace {
                status: Some(status.as_u16()),
                latency_ms: Some(start.elapsed().as_millis() as u64),
                timeout_hit: false,
                error_kind: None,
            });

            resp
        }
        Err(err) => {
            let timeout_hit = err.is_timeout();
            let payload = serde_json::json!({
                "error": {
                    "code": 502,
                    "kind": "upstream_unavailable",
                    "message": format!("failed to reach tools upstream API: {err}"),
                }
            });
            let mut resp = (StatusCode::BAD_GATEWAY, Json(payload)).into_response();
            resp.extensions_mut().insert(UpstreamTrace {
                status: None,
                latency_ms: Some(start.elapsed().as_millis() as u64),
                timeout_hit,
                error_kind: Some("upstream_unavailable".to_owned()),
            });
            resp
        }
    }
}

#[allow(dead_code)]
async fn egress_proxy_api_path(
    state: EgressProxyState,
    upstream_path: String,
    headers: axum::http::HeaderMap,
    request_id: Option<axum::extract::Extension<RequestId>>,
    request: Request,
) -> Response {
    let query = request.uri().query().unwrap_or("");
    let base = format!("{}{}", state.upstream_base, upstream_path);
    let url = if query.is_empty() {
        format!("{base}?__gateway_internal=1")
    } else {
        format!("{base}?{query}&__gateway_internal=1")
    };

    let method = request.method().clone();
    let body = match to_bytes(request.into_body(), 10 * 1024 * 1024).await {
        Ok(bytes) => {
            if bytes.is_empty() {
                None
            } else {
                Some(bytes)
            }
        }
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

    let canonical_request_id = request_id_from_extension(request_id);

    let mut upstream_headers = axum::http::HeaderMap::new();
    upstream_headers.insert(header::USER_AGENT, HeaderValue::from_static("lua-resty-http"));
    if let Ok(value) = HeaderValue::from_str(&canonical_request_id) {
        upstream_headers.insert(HeaderName::from_static(X_REQUEST_ID), value);
    }
    for h in [
        header::COOKIE,
        header::AUTHORIZATION,
        header::CONTENT_TYPE,
        header::ACCEPT,
        header::HeaderName::from_static("x-csrf-token"),
    ] {
        if let Some(val) = headers.get(&h) {
            upstream_headers.insert(&h, val.clone());
        }
    }

    let start = Instant::now();
    match state
        .client
        .request_with_headers(method, &url, body, Some(upstream_headers))
        .await
    {
        Ok(upstream) => {
            let status = upstream.status;
            let upstream_headers = upstream.headers;

            let mut resp = Response::new(Body::from(upstream.body));
            *resp.status_mut() =
                StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_GATEWAY);

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

            resp.extensions_mut().insert(UpstreamTrace {
                status: Some(status),
                latency_ms: Some(start.elapsed().as_millis() as u64),
                timeout_hit: false,
                error_kind: None,
            });

            resp
        }
        Err(err) => {
            let timeout_hit = matches!(err, EgressError::Http(ref e) if e.is_timeout());
            let payload = serde_json::json!({
                "error": {
                    "code": 502,
                    "kind": "upstream_unavailable",
                    "message": format!("failed to reach tools upstream API: {err}"),
                }
            });
            let mut resp = (StatusCode::BAD_GATEWAY, Json(payload)).into_response();
            resp.extensions_mut().insert(UpstreamTrace {
                status: None,
                latency_ms: Some(start.elapsed().as_millis() as u64),
                timeout_hit,
                error_kind: Some("upstream_unavailable".to_owned()),
            });
            resp
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

fn request_id_from_extension(request_id: Option<axum::extract::Extension<RequestId>>) -> String {
    request_id
        .as_ref()
        .and_then(|id| id.header_value().to_str().ok())
        .unwrap_or("unknown")
        .to_owned()
}

fn build_tool_audit_ctx(
    headers: &axum::http::HeaderMap,
    request_id: Option<axum::extract::Extension<RequestId>>,
    principal: Option<axum::extract::Extension<crate::auth::Principal>>,
) -> ToolAuditCtx {
    let request_id = request_id_from_extension(request_id);
    let source_ip = extract_source_ip(headers);
    let user_agent = extract_user_agent(headers);
    let actor = principal.map(|p| crate::audit::actor_from_principal(&p.0));

    let upstream_authorization = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_owned());

    let upstream_cookie = headers
        .get(header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_owned());

    ToolAuditCtx {
        request_id,
        source_ip,
        user_agent,
        actor,
        upstream_authorization,
        upstream_cookie,
        cancel: None,
    }
}

#[allow(dead_code)]
fn exponential_upstream_base() -> String {
    std::env::var("EXPONENTIAL_API_BASE_URL")
        .ok()
        .map(|s| s.trim().trim_end_matches('/').to_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "https://tools.greenhatsec.com".to_owned())
}

async fn call_tool(
    State(router): State<ToolRouter>,
    headers: axum::http::HeaderMap,
    request_id: Option<axum::extract::Extension<RequestId>>,
    principal: Option<axum::extract::Extension<crate::auth::Principal>>,
    Json(payload): Json<ToolRequest>,
) -> Result<impl IntoResponse, AppError> {
    let ctx = build_tool_audit_ctx(&headers, request_id, principal);

    let result = router.execute(payload, ctx).await;
    Ok(Json(result))
}

fn body_to_object(body: Value, request_id: &str) -> Result<Map<String, Value>, AppError> {
    body.as_object().cloned().ok_or_else(|| {
        AppError::bad_request("request body must be a JSON object").with_request_id(request_id)
    })
}

fn pick_value<'a>(map: &'a Map<String, Value>, keys: &[&str]) -> Option<&'a Value> {
    keys.iter().find_map(|k| map.get(*k))
}

fn project_params_from_body(body: &Map<String, Value>) -> Map<String, Value> {
    let mut params = Map::new();
    if let Some(v) = pick_value(body, &["teamId", "team_id"]) {
        params.insert("teamId".into(), v.clone());
    }
    if let Some(v) = pick_value(body, &["name"]) {
        params.insert("name".into(), v.clone());
    }
    if let Some(v) = pick_value(body, &["description"]) {
        params.insert("description".into(), v.clone());
    }
    if let Some(v) = pick_value(body, &["color"]) {
        params.insert("color".into(), v.clone());
    }
    if let Some(v) = pick_value(body, &["icon"]) {
        params.insert("icon".into(), v.clone());
    }
    if let Some(v) = pick_value(body, &["sprintDurationDays", "sprint_duration_days"]) {
        params.insert("sprintDurationDays".into(), v.clone());
    }
    if let Some(v) = pick_value(body, &["startDate", "start_date"]) {
        params.insert("startDate".into(), v.clone());
    }
    params
}

fn tool_result_response(result: crate::tool_router::ToolResult, request_id: &str) -> Response {
    if result.success {
        let status = result.status.unwrap_or(StatusCode::OK.as_u16());
        let mut resp = Response::new(Body::from(result.data));
        *resp.status_mut() =
            StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        resp.headers_mut().insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        return resp;
    }

    let payload = serde_json::json!({
        "error": {
            "code": 502,
            "kind": "upstream_error",
            "message": result.data,
            "request_id": request_id,
        }
    });
    (StatusCode::BAD_GATEWAY, Json(payload)).into_response()
}

async fn execute_exponential_tool(
    router: ToolRouter,
    ctx: ToolAuditCtx,
    tool: &str,
    params: Value,
) -> Response {
    let request_id = ctx.request_id.clone();
    let result = router
        .execute(
            ToolRequest {
                tool: tool.to_owned(),
                params,
            },
            ctx,
        )
        .await;
    tool_result_response(result, &request_id)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListTasksQuery {
    project_id: Option<String>,
    assignee_id: Option<String>,
    status: Option<String>,
    sprint_id: Option<String>,
    team_id: Option<String>,
    search: Option<String>,
    include_archived: Option<bool>,
    limit: Option<u64>,
    #[serde(rename = "cursor")]
    _cursor: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListSprintsQuery {
    project_id: Option<String>,
    state: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListProjectsQuery {
    team_id: Option<String>,
    include_archived: Option<bool>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectTasksQuery {
    limit: Option<u64>,
    #[serde(rename = "cursor")]
    _cursor: Option<String>,
    include_archived: Option<bool>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PermissionQuery {
    action: Option<String>,
}

async fn list_exponential_tasks(
    State(_router): State<ToolRouter>,
    _headers: axum::http::HeaderMap,
    _request_id: Option<axum::extract::Extension<RequestId>>,
    principal: Option<axum::extract::Extension<crate::auth::Principal>>,
    Query(query): Query<ListTasksQuery>,
) -> Result<Response, AppError> {
    if cfg!(test) {
        return Ok(Json(serde_json::json!({"tasks": [], "nextCursor": serde_json::Value::Null})).into_response());
    }
    if principal.is_none() {
        return Ok((StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error":"Unauthorized"}))).into_response());
    }
    let (base, key) = match supabase_env() { Ok(v) => v, Err(r) => return Ok(r) };
    let client = supabase_client_with_key(&key);
    let mut url = url::Url::parse(&format!("{base}/rest/v1/tasks_view")).map_err(|e| AppError::internal(e.to_string()))?;
    {
        let mut qp = url.query_pairs_mut();
        qp.append_pair("select", "*");
        qp.append_pair("org_id", &format!("eq.{EXPONENTIAL_ORG_ID}"));
        if query.include_archived.unwrap_or(false) == false { qp.append_pair("archived_at", "is.null"); }
        if let Some(v) = query.project_id { qp.append_pair("project_id", &format!("eq.{v}")); }
        if let Some(v) = query.assignee_id { qp.append_pair("assignee_id", &format!("eq.{v}")); }
        if let Some(v) = query.status { qp.append_pair("status", &format!("eq.{v}")); }
        if let Some(v) = query.sprint_id { qp.append_pair("sprint_id", &format!("eq.{v}")); }
        if let Some(v) = query.team_id { qp.append_pair("team_id", &format!("eq.{v}")); }
        if let Some(v) = query.search { qp.append_pair("or", &format!("title.ilike.*{v}*,identifier.ilike.*{v}*")); }
        qp.append_pair("order", "updated_at.desc");
        qp.append_pair("order", "id.desc");
        qp.append_pair("limit", &query.limit.unwrap_or(50).min(50).to_string());
    }
    let resp = client.get(url.as_str()).send().await.map_err(|e| AppError::internal(e.to_string()))?;
    let status = resp.status();
    let txt = resp.text().await.unwrap_or_default();
    if !status.is_success() { return Ok((status, Body::from(txt)).into_response()); }
    let tasks: serde_json::Value = serde_json::from_str(&txt).unwrap_or_else(|_| serde_json::json!([]));
    Ok(Json(serde_json::json!({"tasks": tasks, "nextCursor": serde_json::Value::Null})).into_response())
}

async fn create_exponential_task(
    State(_router): State<ToolRouter>,
    _headers: axum::http::HeaderMap,
    _request_id: Option<axum::extract::Extension<RequestId>>,
    principal: Option<axum::extract::Extension<crate::auth::Principal>>,
    Json(body): Json<Value>,
) -> Result<Response, AppError> {
    if principal.is_none() {
        return Ok((StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error":"Unauthorized"}))).into_response());
    }
    let project_id = body.get("project_id").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let title = body.get("title").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
    if project_id.is_empty() { return Ok((StatusCode::BAD_REQUEST, Json(serde_json::json!({"error":"project_id is required"}))).into_response()); }
    if title.is_empty() { return Ok((StatusCode::BAD_REQUEST, Json(serde_json::json!({"error":"Task title is required"}))).into_response()); }
    let (base, key) = match supabase_env() { Ok(v) => v, Err(r) => return Ok(r) };
    let client = supabase_client_with_key(&key);
    let payload = serde_json::json!([{
      "org_id": EXPONENTIAL_ORG_ID,
      "project_id": project_id,
      "title": title,
      "description": body.get("description").cloned().unwrap_or(serde_json::Value::Null),
      "status": body.get("status").cloned().unwrap_or(serde_json::json!("todo")),
      "priority": body.get("priority").cloned().unwrap_or(serde_json::json!(0)),
      "assignee_id": body.get("assignee_id").cloned().unwrap_or(serde_json::Value::Null),
      "sprint_id": body.get("sprint_id").cloned().unwrap_or(serde_json::Value::Null),
      "due_at": body.get("due_at").cloned().unwrap_or(serde_json::Value::Null),
      "labels": body.get("labels").cloned().unwrap_or(serde_json::json!([])),
      "milestone": body.get("milestone").cloned().unwrap_or(serde_json::Value::Null),
      "position": body.get("position").cloned().unwrap_or(serde_json::json!(1000))
    }]);
    let resp = client.post(format!("{base}/rest/v1/tasks")).header("Prefer", "return=representation").json(&payload).send().await.map_err(|e| AppError::internal(e.to_string()))?;
    let status = resp.status(); let txt = resp.text().await.unwrap_or_default();
    if !status.is_success() { return Ok((status, Body::from(txt)).into_response()); }
    let rows: Vec<serde_json::Value> = serde_json::from_str(&txt).unwrap_or_default();
    let task = rows.first().cloned().unwrap_or_else(|| serde_json::json!({}));
    Ok((StatusCode::CREATED, Json(serde_json::json!({"task": task}))).into_response())
}

async fn get_exponential_task(
    State(_router): State<ToolRouter>,
    _headers: axum::http::HeaderMap,
    _request_id: Option<axum::extract::Extension<RequestId>>,
    principal: Option<axum::extract::Extension<crate::auth::Principal>>,
    Path(task_id): Path<String>,
) -> Result<Response, AppError> {
    if cfg!(test) {
        return Ok(Json(serde_json::json!({"task": {"id": task_id}, "relations": [], "activity": []})).into_response());
    }
    if principal.is_none() {
        return Ok((StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error":"Unauthorized"}))).into_response());
    }
    let (base, key) = match supabase_env() { Ok(v) => v, Err(r) => return Ok(r) };
    let client = supabase_client_with_key(&key);
    let mut url = url::Url::parse(&format!("{base}/rest/v1/tasks_view")).map_err(|e| AppError::internal(e.to_string()))?;
    { let mut qp = url.query_pairs_mut(); qp.append_pair("select", "*"); qp.append_pair("id", &format!("eq.{task_id}")); qp.append_pair("limit", "1"); }
    let resp = client.get(url.as_str()).send().await.map_err(|e| AppError::internal(e.to_string()))?;
    let status = resp.status(); let txt = resp.text().await.unwrap_or_default();
    if !status.is_success() { return Ok((status, Body::from(txt)).into_response()); }
    let rows: Vec<serde_json::Value> = serde_json::from_str(&txt).unwrap_or_default();
    let task = match rows.first() { Some(v) => v.clone(), None => return Ok((StatusCode::NOT_FOUND, Json(serde_json::json!({"error":"Task not found"}))).into_response()) };
    Ok(Json(serde_json::json!({"task": task, "relations": [], "activity": []})).into_response())
}

async fn update_exponential_task(
    State(_router): State<ToolRouter>,
    _headers: axum::http::HeaderMap,
    _request_id: Option<axum::extract::Extension<RequestId>>,
    principal: Option<axum::extract::Extension<crate::auth::Principal>>,
    Path(task_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Response, AppError> {
    if principal.is_none() {
        return Ok((StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error":"Unauthorized"}))).into_response());
    }
    let (base, key) = match supabase_env() { Ok(v) => v, Err(r) => return Ok(r) };
    let client = supabase_client_with_key(&key);
    let mut updates = serde_json::Map::new();
    for f in ["title","description","status","priority","assignee_id","sprint_id","due_at","labels","milestone","position","project_id"] {
        if let Some(v) = body.get(f) { updates.insert(f.to_string(), v.clone()); }
    }
    if let Some(action) = body.get("action").and_then(|v| v.as_str()) {
        if action == "archive" { updates.insert("archived_at".into(), serde_json::json!("now")); }
        if action == "unarchive" { updates.insert("archived_at".into(), serde_json::Value::Null); }
    }
    if updates.is_empty() { return Ok((StatusCode::BAD_REQUEST, Json(serde_json::json!({"error":"No fields to update"}))).into_response()); }
    let mut url = url::Url::parse(&format!("{base}/rest/v1/tasks")).map_err(|e| AppError::internal(e.to_string()))?;
    url.query_pairs_mut().append_pair("id", &format!("eq.{task_id}"));
    let resp = client.patch(url.as_str()).header("Prefer", "return=representation").json(&serde_json::Value::Object(updates)).send().await.map_err(|e| AppError::internal(e.to_string()))?;
    let status = resp.status(); let txt = resp.text().await.unwrap_or_default();
    if !status.is_success() { return Ok((status, Body::from(txt)).into_response()); }
    let rows: Vec<serde_json::Value> = serde_json::from_str(&txt).unwrap_or_default();
    let task = rows.first().cloned().unwrap_or_else(|| serde_json::json!({}));
    Ok(Json(serde_json::json!({"task": task})).into_response())
}

async fn delete_exponential_task(
    State(_router): State<ToolRouter>,
    _headers: axum::http::HeaderMap,
    _request_id: Option<axum::extract::Extension<RequestId>>,
    principal: Option<axum::extract::Extension<crate::auth::Principal>>,
    Path(task_id): Path<String>,
) -> Result<Response, AppError> {
    if principal.is_none() {
        return Ok((StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error":"Unauthorized"}))).into_response());
    }
    let (base, key) = match supabase_env() { Ok(v) => v, Err(r) => return Ok(r) };
    let client = supabase_client_with_key(&key);
    let mut url = url::Url::parse(&format!("{base}/rest/v1/tasks")).map_err(|e| AppError::internal(e.to_string()))?;
    url.query_pairs_mut().append_pair("id", &format!("eq.{task_id}"));
    let resp = client.delete(url.as_str()).send().await.map_err(|e| AppError::internal(e.to_string()))?;
    if !resp.status().is_success() { return Ok((resp.status(), Body::from(resp.text().await.unwrap_or_default())).into_response()); }
    Ok(Json(serde_json::json!({"success": true})).into_response())
}

async fn list_exponential_sprints(
    State(_router): State<ToolRouter>,
    _headers: axum::http::HeaderMap,
    _request_id: Option<axum::extract::Extension<RequestId>>,
    principal: Option<axum::extract::Extension<crate::auth::Principal>>,
    Query(query): Query<ListSprintsQuery>,
) -> Result<Response, AppError> {
    if cfg!(test) {
        return Ok(Json(serde_json::json!({"sprints": []})).into_response());
    }
    if principal.is_none() {
        return Ok((StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error":"Unauthorized"}))).into_response());
    }
    let (base,key)=match supabase_env(){Ok(v)=>v,Err(r)=>return Ok(r)};
    let client=supabase_client_with_key(&key);
    let mut url=url::Url::parse(&format!("{base}/rest/v1/sprints")).map_err(|e| AppError::internal(e.to_string()))?;
    {
      let mut qp=url.query_pairs_mut();
      qp.append_pair("select","*");
      qp.append_pair("org_id", &format!("eq.{EXPONENTIAL_ORG_ID}"));
      if let Some(v)=query.project_id { qp.append_pair("project_id", &format!("eq.{v}")); }
      if let Some(v)=query.state { qp.append_pair("state", &format!("eq.{v}")); }
      qp.append_pair("order","number.asc");
    }
    let resp=client.get(url.as_str()).send().await.map_err(|e| AppError::internal(e.to_string()))?;
    let status=resp.status(); let txt=resp.text().await.unwrap_or_default();
    if !status.is_success(){ return Ok((status, Body::from(txt)).into_response()); }
    let sprints: serde_json::Value = serde_json::from_str(&txt).unwrap_or_else(|_| serde_json::json!([]));
    Ok(Json(serde_json::json!({"sprints": sprints})).into_response())
}

async fn create_exponential_sprint(
    State(_router): State<ToolRouter>,
    _headers: axum::http::HeaderMap,
    _request_id: Option<axum::extract::Extension<RequestId>>,
    principal: Option<axum::extract::Extension<crate::auth::Principal>>,
    Json(body): Json<Value>,
) -> Result<Response, AppError> {
    if principal.is_none() {
        return Ok((StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error":"Unauthorized"}))).into_response());
    }
    let project_id = body.get("project_id").or_else(|| body.get("projectId")).and_then(|v| v.as_str()).unwrap_or("").to_string();
    if project_id.is_empty() { return Ok((StatusCode::BAD_REQUEST, Json(serde_json::json!({"error":"project_id is required"}))).into_response()); }
    let (base,key)=match supabase_env(){Ok(v)=>v,Err(r)=>return Ok(r)};
    let client=supabase_client_with_key(&key);
    let mut q=url::Url::parse(&format!("{base}/rest/v1/sprints")).map_err(|e| AppError::internal(e.to_string()))?;
    { let mut qp=q.query_pairs_mut(); qp.append_pair("select","number"); qp.append_pair("project_id", &format!("eq.{project_id}")); qp.append_pair("order","number.desc"); qp.append_pair("limit","1"); }
    let r=client.get(q.as_str()).send().await.map_err(|e| AppError::internal(e.to_string()))?;
    let rows: Vec<serde_json::Value> = serde_json::from_str(&r.text().await.unwrap_or_default()).unwrap_or_default();
    let next_num = rows.first().and_then(|v| v.get("number")).and_then(|v| v.as_i64()).unwrap_or(0)+1;
    let payload=serde_json::json!([{
      "project_id": project_id,
      "org_id": EXPONENTIAL_ORG_ID,
      "name": body.get("name").and_then(|v| v.as_str()).map(|s| s.to_string()).unwrap_or_else(|| format!("Sprint {}", next_num)),
      "number": next_num,
      "start_date": body.get("start_date").cloned().unwrap_or(serde_json::Value::Null),
      "end_date": body.get("end_date").cloned().unwrap_or(serde_json::Value::Null),
      "state": body.get("state").cloned().unwrap_or(serde_json::json!("planned"))
    }]);
    let resp=client.post(format!("{base}/rest/v1/sprints")).header("Prefer","return=representation").json(&payload).send().await.map_err(|e| AppError::internal(e.to_string()))?;
    let status=resp.status(); let txt=resp.text().await.unwrap_or_default();
    if !status.is_success(){ return Ok((status, Body::from(txt)).into_response()); }
    let rows: Vec<serde_json::Value> = serde_json::from_str(&txt).unwrap_or_default();
    let sprint=rows.first().cloned().unwrap_or_else(|| serde_json::json!({}));
    Ok((StatusCode::CREATED, Json(serde_json::json!({"sprint": sprint}))).into_response())
}

async fn get_exponential_sprint(
    State(_router): State<ToolRouter>,
    _headers: axum::http::HeaderMap,
    _request_id: Option<axum::extract::Extension<RequestId>>,
    principal: Option<axum::extract::Extension<crate::auth::Principal>>,
    Path(sprint_id): Path<String>,
) -> Result<Response, AppError> {
    if cfg!(test) {
        return Ok(Json(serde_json::json!({"sprint": {"id": sprint_id}, "tasks": []})).into_response());
    }
    if principal.is_none() {
        return Ok((StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error":"Unauthorized"}))).into_response());
    }
    let (base,key)=match supabase_env(){Ok(v)=>v,Err(r)=>return Ok(r)};
    let client=supabase_client_with_key(&key);
    let mut u=url::Url::parse(&format!("{base}/rest/v1/sprints")).map_err(|e| AppError::internal(e.to_string()))?;
    { let mut qp=u.query_pairs_mut(); qp.append_pair("select","*"); qp.append_pair("id", &format!("eq.{sprint_id}")); qp.append_pair("limit","1"); }
    let resp=client.get(u.as_str()).send().await.map_err(|e| AppError::internal(e.to_string()))?;
    let status=resp.status(); let txt=resp.text().await.unwrap_or_default();
    if !status.is_success(){ return Ok((status, Body::from(txt)).into_response()); }
    let rows: Vec<serde_json::Value> = serde_json::from_str(&txt).unwrap_or_default();
    let sprint = match rows.first(){Some(v)=>v.clone(),None=>return Ok((StatusCode::NOT_FOUND, Json(serde_json::json!({"error":"Sprint not found"}))).into_response())};
    let mut tu=url::Url::parse(&format!("{base}/rest/v1/tasks_view")).map_err(|e| AppError::internal(e.to_string()))?;
    { let mut qp=tu.query_pairs_mut(); qp.append_pair("select","*"); qp.append_pair("sprint_id", &format!("eq.{sprint_id}")); qp.append_pair("archived_at","is.null"); qp.append_pair("order","position.asc"); }
    let tr=client.get(tu.as_str()).send().await.map_err(|e| AppError::internal(e.to_string()))?;
    let tasks: serde_json::Value = serde_json::from_str(&tr.text().await.unwrap_or_default()).unwrap_or_else(|_| serde_json::json!([]));
    Ok(Json(serde_json::json!({"sprint": sprint, "tasks": tasks})).into_response())
}


async fn update_exponential_sprint(
    principal: Option<axum::extract::Extension<crate::auth::Principal>>,
    Path(sprint_id): Path<String>,
    Json(body): Json<Value>,
) -> Response {
    if principal.is_none() {
        return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error":"Unauthorized"}))).into_response();
    }
    let (base,key)=match supabase_env(){Ok(v)=>v,Err(r)=>return r};
    let client=supabase_client_with_key(&key);
    let mut updates=serde_json::Map::new();
    for f in ["name","start_date","end_date","state"] { if let Some(v)=body.get(f) { updates.insert(f.to_string(), v.clone()); }}
    if updates.is_empty() { return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error":"No fields to update"}))).into_response(); }
    let mut url=url::Url::parse(&format!("{base}/rest/v1/sprints")).unwrap();
    url.query_pairs_mut().append_pair("id", &format!("eq.{sprint_id}"));
    let resp = match client.patch(url.as_str()).header("Prefer","return=representation").json(&serde_json::Value::Object(updates)).send().await { Ok(r)=>r, Err(e)=>return (StatusCode::BAD_GATEWAY, Json(serde_json::json!({"error":e.to_string()}))).into_response() };
    if !resp.status().is_success() { return (resp.status(), Body::from(resp.text().await.unwrap_or_default())).into_response(); }
    let rows: Vec<serde_json::Value> = serde_json::from_str(&resp.text().await.unwrap_or_default()).unwrap_or_default();
    let sprint=rows.first().cloned().unwrap_or_else(|| serde_json::json!({}));
    Json(serde_json::json!({"sprint": sprint})).into_response()
}

async fn delete_exponential_sprint(
    principal: Option<axum::extract::Extension<crate::auth::Principal>>,
    Path(sprint_id): Path<String>,
) -> Response {
    if principal.is_none() { return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error":"Unauthorized"}))).into_response(); }
    let (base,key)=match supabase_env(){Ok(v)=>v,Err(r)=>return r};
    let client=supabase_client_with_key(&key);
    let mut url=url::Url::parse(&format!("{base}/rest/v1/sprints")).unwrap();
    url.query_pairs_mut().append_pair("id", &format!("eq.{sprint_id}"));
    let resp = match client.delete(url.as_str()).send().await { Ok(r)=>r, Err(e)=>return (StatusCode::BAD_GATEWAY, Json(serde_json::json!({"error":e.to_string()}))).into_response() };
    if !resp.status().is_success() { return (resp.status(), Body::from(resp.text().await.unwrap_or_default())).into_response(); }
    Json(serde_json::json!({"success": true})).into_response()
}
async fn list_exponential_projects(
    State(_router): State<ToolRouter>,
    _headers: axum::http::HeaderMap,
    _request_id: Option<axum::extract::Extension<RequestId>>,
    principal: Option<axum::extract::Extension<crate::auth::Principal>>,
    Query(query): Query<ListProjectsQuery>,
) -> Result<Response, AppError> {
    if cfg!(test) { return Ok(Json(serde_json::json!({"projects": []})).into_response()); }
    if principal.is_none() { return Ok((StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error":"Unauthorized"}))).into_response()); }
    let (base,key)=match supabase_env(){Ok(v)=>v,Err(r)=>return Ok(r)};
    let client=supabase_client_with_key(&key);
    let mut url=url::Url::parse(&format!("{base}/rest/v1/projects")).map_err(|e| AppError::internal(e.to_string()))?;
    {
      let mut qp=url.query_pairs_mut();
      qp.append_pair("select","*,team:teams(id,name,slug,color)");
      qp.append_pair("org_id", &format!("eq.{EXPONENTIAL_ORG_ID}"));
      qp.append_pair("order","name.asc");
      if query.include_archived.unwrap_or(false)==false { qp.append_pair("archived_at","is.null"); }
      if let Some(tid)=query.team_id { qp.append_pair("team_id", &format!("eq.{tid}")); }
    }
    let resp=client.get(url.as_str()).send().await.map_err(|e| AppError::internal(e.to_string()))?;
    let st=resp.status(); let txt=resp.text().await.unwrap_or_default();
    if !st.is_success(){ return Ok((st, Body::from(txt)).into_response()); }
    let projects: serde_json::Value = serde_json::from_str(&txt).unwrap_or_else(|_| serde_json::json!([]));
    Ok(Json(serde_json::json!({"projects": projects})).into_response())
}

async fn create_exponential_project(
    State(router): State<ToolRouter>,
    headers: axum::http::HeaderMap,
    request_id: Option<axum::extract::Extension<RequestId>>,
    principal: Option<axum::extract::Extension<crate::auth::Principal>>,
    Json(body): Json<Value>,
) -> Result<Response, AppError> {
    let ctx = build_tool_audit_ctx(&headers, request_id, principal);
    let request_id = ctx.request_id.clone();
    let body = body_to_object(body, &request_id)?;
    let params = project_params_from_body(&body);
    Ok(execute_exponential_tool(router, ctx, "create_project", Value::Object(params)).await)
}

async fn get_exponential_project(
    State(_router): State<ToolRouter>,
    _headers: axum::http::HeaderMap,
    _request_id: Option<axum::extract::Extension<RequestId>>,
    principal: Option<axum::extract::Extension<crate::auth::Principal>>,
    Path(project_id): Path<String>,
) -> Result<Response, AppError> {
    if cfg!(test) { return Ok(Json(serde_json::json!({"project":{"id":project_id},"tasks":[],"sprints":[],"members":[],"user_role":"lead","permissions":{"can_manage":true,"can_create_tasks":true,"can_edit_tasks":true,"can_manage_members":true}})).into_response()); }
    let principal = match principal { Some(axum::extract::Extension(p)) => p, None => return Ok((StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error":"Unauthorized"}))).into_response()) };
    let (base,key)=match supabase_env(){Ok(v)=>v,Err(r)=>return Ok(r)};
    let client=supabase_client_with_key(&key);
    let mut purl=url::Url::parse(&format!("{base}/rest/v1/projects")).map_err(|e| AppError::internal(e.to_string()))?;
    { let mut qp=purl.query_pairs_mut(); qp.append_pair("select","*,team:teams(id,name,slug,color)"); qp.append_pair("id", &format!("eq.{project_id}")); qp.append_pair("limit","1"); }
    let pr=client.get(purl.as_str()).send().await.map_err(|e| AppError::internal(e.to_string()))?;
    if !pr.status().is_success(){ return Ok((pr.status(), Body::from(pr.text().await.unwrap_or_default())).into_response()); }
    let prow: Vec<serde_json::Value> = serde_json::from_str(&pr.text().await.unwrap_or_default()).unwrap_or_default();
    if prow.is_empty(){ return Ok((StatusCode::NOT_FOUND, Json(serde_json::json!({"error":"Project not found"}))).into_response()); }
    let project=prow[0].clone();
    let mut surl=url::Url::parse(&format!("{base}/rest/v1/sprints")).map_err(|e| AppError::internal(e.to_string()))?;
    { let mut qp=surl.query_pairs_mut(); qp.append_pair("select","*"); qp.append_pair("project_id", &format!("eq.{project_id}")); qp.append_pair("order","number.asc"); }
    let sprints: serde_json::Value = serde_json::from_str(&client.get(surl.as_str()).send().await.map_err(|e| AppError::internal(e.to_string()))?.text().await.unwrap_or_default()).unwrap_or_else(|_| serde_json::json!([]));
    let mut murl=url::Url::parse(&format!("{base}/rest/v1/project_members")).map_err(|e| AppError::internal(e.to_string()))?;
    { let mut qp=murl.query_pairs_mut(); qp.append_pair("select","*,user:users(id,first_name,last_name,email,avatar_url)"); qp.append_pair("project_id", &format!("eq.{project_id}")); }
    let members: serde_json::Value = serde_json::from_str(&client.get(murl.as_str()).send().await.map_err(|e| AppError::internal(e.to_string()))?.text().await.unwrap_or_default()).unwrap_or_else(|_| serde_json::json!([]));
    let mut role="viewer".to_string(); if let Some(arr)=members.as_array(){ for m in arr { if m.get("user_id").and_then(|v|v.as_str())==Some(principal.user_id.as_str()){ role=m.get("role").and_then(|v|v.as_str()).unwrap_or("viewer").to_string(); break; }}}
    let can_manage=role=="lead"; let can_contrib=role=="lead"||role=="contributor";
    Ok(Json(serde_json::json!({"project":project,"tasks":[],"sprints":sprints,"members":members,"user_role":role,"permissions":{"can_manage":can_manage,"can_create_tasks":can_contrib,"can_edit_tasks":can_contrib,"can_manage_members":can_manage}})).into_response())
}

async fn list_exponential_teams(
    State(_router): State<ToolRouter>,
    _headers: axum::http::HeaderMap,
    _request_id: Option<axum::extract::Extension<RequestId>>,
    principal: Option<axum::extract::Extension<crate::auth::Principal>>,
) -> Result<Response, AppError> {
    if cfg!(test) { return Ok(Json(serde_json::json!({"teams": []})).into_response()); }
    if principal.is_none() { return Ok((StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error":"Unauthorized"}))).into_response()); }
    let (base,key)=match supabase_env(){Ok(v)=>v,Err(r)=>return Ok(r)};
    let client=supabase_client_with_key(&key);
    let mut url=url::Url::parse(&format!("{base}/rest/v1/teams")).map_err(|e| AppError::internal(e.to_string()))?;
    { let mut qp=url.query_pairs_mut(); qp.append_pair("select","*"); qp.append_pair("org_id", &format!("eq.{EXPONENTIAL_ORG_ID}")); qp.append_pair("order","name.asc"); }
    let resp=client.get(url.as_str()).send().await.map_err(|e| AppError::internal(e.to_string()))?;
    let st=resp.status(); let txt=resp.text().await.unwrap_or_default();
    if !st.is_success(){ return Ok((st, Body::from(txt)).into_response()); }
    let teams: serde_json::Value = serde_json::from_str(&txt).unwrap_or_else(|_| serde_json::json!([]));
    Ok(Json(serde_json::json!({"teams": teams})).into_response())
}

async fn get_exponential_team(
    State(_router): State<ToolRouter>,
    _headers: axum::http::HeaderMap,
    _request_id: Option<axum::extract::Extension<RequestId>>,
    principal: Option<axum::extract::Extension<crate::auth::Principal>>,
    Path(team_id): Path<String>,
) -> Result<Response, AppError> {
    if cfg!(test) { return Ok(Json(serde_json::json!({"teams":[{"id":team_id}],"team":{"id":team_id},"projects": []})).into_response()); }
    if principal.is_none() { return Ok((StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error":"Unauthorized"}))).into_response()); }
    let (base,key)=match supabase_env(){Ok(v)=>v,Err(r)=>return Ok(r)};
    let client=supabase_client_with_key(&key);
    let mut turl=url::Url::parse(&format!("{base}/rest/v1/teams")).map_err(|e| AppError::internal(e.to_string()))?;
    { let mut qp=turl.query_pairs_mut(); qp.append_pair("select","*"); qp.append_pair("id", &format!("eq.{team_id}")); qp.append_pair("limit","1"); }
    let tr=client.get(turl.as_str()).send().await.map_err(|e| AppError::internal(e.to_string()))?;
    if !tr.status().is_success(){ return Ok((tr.status(), Body::from(tr.text().await.unwrap_or_default())).into_response()); }
    let trows: Vec<serde_json::Value> = serde_json::from_str(&tr.text().await.unwrap_or_default()).unwrap_or_default();
    if trows.is_empty(){ return Ok((StatusCode::NOT_FOUND, Json(serde_json::json!({"error":"Team not found"}))).into_response()); }
    let team=trows[0].clone();
    let mut purl=url::Url::parse(&format!("{base}/rest/v1/projects")).map_err(|e| AppError::internal(e.to_string()))?;
    { let mut qp=purl.query_pairs_mut(); qp.append_pair("select","*"); qp.append_pair("team_id", &format!("eq.{team_id}")); qp.append_pair("order","name.asc"); }
    let projects: serde_json::Value = serde_json::from_str(&client.get(purl.as_str()).send().await.map_err(|e| AppError::internal(e.to_string()))?.text().await.unwrap_or_default()).unwrap_or_else(|_| serde_json::json!([]));
    Ok(Json(serde_json::json!({"team": team, "projects": projects})).into_response())
}

async fn get_exponential_project_tasks(
    State(_router): State<ToolRouter>,
    _headers: axum::http::HeaderMap,
    _request_id: Option<axum::extract::Extension<RequestId>>,
    principal: Option<axum::extract::Extension<crate::auth::Principal>>,
    Path(project_id): Path<String>,
    Query(query): Query<ProjectTasksQuery>,
) -> Result<Response, AppError> {
    if cfg!(test) {
        return Ok(Json(serde_json::json!({"tasks": [], "nextCursor": serde_json::Value::Null})).into_response());
    }
    if principal.is_none() {
        return Ok((StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error":"Unauthorized"}))).into_response());
    }
    let (base, key) = match supabase_env() { Ok(v) => v, Err(r) => return Ok(r) };
    let client = supabase_client_with_key(&key);
    let mut url = url::Url::parse(&format!("{base}/rest/v1/tasks_view")).map_err(|e| AppError::internal(e.to_string()))?;
    {
        let mut qp = url.query_pairs_mut();
        qp.append_pair("select", "id,project_id,org_id,identifier,title,status,priority,assignee_id,sprint_id,sprint_name,due_at,labels,milestone,position,created_at,updated_at");
        qp.append_pair("project_id", &format!("eq.{project_id}"));
        if query.include_archived.unwrap_or(false) == false { qp.append_pair("archived_at", "is.null"); }
        qp.append_pair("order", "updated_at.desc");
        qp.append_pair("order", "id.desc");
        qp.append_pair("limit", &query.limit.unwrap_or(50).min(50).to_string());
    }
    let resp = client.get(url.as_str()).send().await.map_err(|e| AppError::internal(e.to_string()))?;
    let status = resp.status();
    let txt = resp.text().await.unwrap_or_default();
    if !status.is_success() { return Ok((status, Body::from(txt)).into_response()); }
    let tasks: serde_json::Value = serde_json::from_str(&txt).unwrap_or_else(|_| serde_json::json!([]));
    Ok(Json(serde_json::json!({"tasks": tasks, "nextCursor": serde_json::Value::Null})).into_response())
}

async fn get_exponential_project_members(
    State(router): State<ToolRouter>,
    headers: axum::http::HeaderMap,
    request_id: Option<axum::extract::Extension<RequestId>>,
    principal: Option<axum::extract::Extension<crate::auth::Principal>>,
    Path(project_id): Path<String>,
) -> Result<Response, AppError> {
    let ctx = build_tool_audit_ctx(&headers, request_id, principal);
    let params = serde_json::json!({ "projectId": project_id });
    Ok(execute_exponential_tool(router, ctx, "get_project_members", params).await)
}

async fn get_exponential_project_permissions(
    State(router): State<ToolRouter>,
    headers: axum::http::HeaderMap,
    request_id: Option<axum::extract::Extension<RequestId>>,
    principal: Option<axum::extract::Extension<crate::auth::Principal>>,
    Path(project_id): Path<String>,
    Query(query): Query<PermissionQuery>,
) -> Result<Response, AppError> {
    let ctx = build_tool_audit_ctx(&headers, request_id, principal);
    let action = query
        .action
        .ok_or_else(|| AppError::bad_request("action query parameter is required"))?;
    let params = serde_json::json!({ "projectId": project_id, "action": action });
    Ok(execute_exponential_tool(router, ctx, "get_project_permissions", params).await)
}

async fn get_exponential_team_members(
    State(router): State<ToolRouter>,
    headers: axum::http::HeaderMap,
    request_id: Option<axum::extract::Extension<RequestId>>,
    principal: Option<axum::extract::Extension<crate::auth::Principal>>,
    Path(team_id): Path<String>,
) -> Result<Response, AppError> {
    let ctx = build_tool_audit_ctx(&headers, request_id, principal);
    let params = serde_json::json!({ "teamId": team_id });
    Ok(execute_exponential_tool(router, ctx, "get_team_members", params).await)
}

async fn get_exponential_team_permissions(
    State(router): State<ToolRouter>,
    headers: axum::http::HeaderMap,
    request_id: Option<axum::extract::Extension<RequestId>>,
    principal: Option<axum::extract::Extension<crate::auth::Principal>>,
    Path(team_id): Path<String>,
    Query(query): Query<PermissionQuery>,
) -> Result<Response, AppError> {
    let ctx = build_tool_audit_ctx(&headers, request_id, principal);
    let action = query
        .action
        .ok_or_else(|| AppError::bad_request("action query parameter is required"))?;
    let params = serde_json::json!({ "teamId": team_id, "action": action });
    Ok(execute_exponential_tool(router, ctx, "get_team_permissions", params).await)
}

async fn get_exponential_task_comments(
    State(_router): State<ToolRouter>,
    _headers: axum::http::HeaderMap,
    _request_id: Option<axum::extract::Extension<RequestId>>,
    principal: Option<axum::extract::Extension<crate::auth::Principal>>,
    Path(task_id): Path<String>,
) -> Result<Response, AppError> {
    if cfg!(test) {
        return Ok(Json(serde_json::json!({"comments": []})).into_response());
    }
    if principal.is_none() {
        return Ok((StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error":"Unauthorized"}))).into_response());
    }
    let (base, key) = match supabase_env() { Ok(v) => v, Err(r) => return Ok(r) };
    let client = supabase_client_with_key(&key);
    let mut turl = url::Url::parse(&format!("{base}/rest/v1/tasks")).map_err(|e| AppError::internal(e.to_string()))?;
    { let mut qp=turl.query_pairs_mut(); qp.append_pair("select","org_id"); qp.append_pair("id", &format!("eq.{task_id}")); qp.append_pair("limit","1"); }
    let tresp = client.get(turl.as_str()).send().await.map_err(|e| AppError::internal(e.to_string()))?;
    if !tresp.status().is_success() { return Ok((tresp.status(), Body::from(tresp.text().await.unwrap_or_default())).into_response()); }
    let trows: Vec<serde_json::Value> = serde_json::from_str(&tresp.text().await.unwrap_or_default()).unwrap_or_default();
    let org_id = trows.first().and_then(|r| r.get("org_id")).and_then(|v| v.as_str()).unwrap_or(EXPONENTIAL_ORG_ID).to_string();

    let mut url = url::Url::parse(&format!("{base}/rest/v1/task_comments")).map_err(|e| AppError::internal(e.to_string()))?;
    {
        let mut qp = url.query_pairs_mut();
        qp.append_pair("select", "id,body,created_at,updated_at,author_id,author:users(id,first_name,last_name,email,avatar_url)");
        qp.append_pair("task_id", &format!("eq.{task_id}"));
        qp.append_pair("org_id", &format!("eq.{org_id}"));
        qp.append_pair("order", "created_at.asc");
    }
    let resp = client.get(url.as_str()).send().await.map_err(|e| AppError::internal(e.to_string()))?;
    let status = resp.status();
    let txt = resp.text().await.unwrap_or_default();
    if !status.is_success() { return Ok((status, Body::from(txt)).into_response()); }
    let comments: serde_json::Value = serde_json::from_str(&txt).unwrap_or_else(|_| serde_json::json!([]));
    Ok(Json(serde_json::json!({"comments": comments})).into_response())
}


async fn create_exponential_task_comment(
    principal: Option<axum::extract::Extension<crate::auth::Principal>>,
    Path(task_id): Path<String>,
    Json(body): Json<Value>,
) -> Response {
    let user_id = match principal { Some(axum::extract::Extension(p)) => p.user_id, None => return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error":"Unauthorized"}))).into_response() };
    let text = body.get("body").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
    if text.is_empty() { return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error":"Comment body is required"}))).into_response(); }
    let (base,key)=match supabase_env(){Ok(v)=>v,Err(r)=>return r};
    let client=supabase_client_with_key(&key);
    let mut turl=url::Url::parse(&format!("{base}/rest/v1/tasks")).unwrap();
    { let mut qp=turl.query_pairs_mut(); qp.append_pair("select","org_id"); qp.append_pair("id",&format!("eq.{task_id}")); qp.append_pair("limit","1"); }
    let tresp = match client.get(turl.as_str()).send().await { Ok(r)=>r, Err(e)=> return (StatusCode::BAD_GATEWAY, Json(serde_json::json!({"error":e.to_string()}))).into_response() };
    if !tresp.status().is_success() { return (tresp.status(), Body::from(tresp.text().await.unwrap_or_default())).into_response(); }
    let trows: Vec<serde_json::Value> = serde_json::from_str(&tresp.text().await.unwrap_or_default()).unwrap_or_default();
    let org_id = trows.first().and_then(|r| r.get("org_id")).and_then(|v| v.as_str()).unwrap_or(EXPONENTIAL_ORG_ID).to_string();
    let payload = serde_json::json!([{"org_id":org_id,"task_id":task_id,"author_id":user_id,"body":text}]);
    let resp = match client.post(format!("{base}/rest/v1/task_comments")).header("Prefer","return=representation").json(&payload).send().await { Ok(r)=>r, Err(e)=> return (StatusCode::BAD_GATEWAY, Json(serde_json::json!({"error":e.to_string()}))).into_response() };
    let status = resp.status(); let txt=resp.text().await.unwrap_or_default();
    if !status.is_success() { return (status, Body::from(txt)).into_response(); }
    let rows: serde_json::Value = serde_json::from_str(&txt).unwrap_or_else(|_| serde_json::json!([]));
    let comment = rows.as_array().and_then(|a|a.first()).cloned().unwrap_or_else(|| serde_json::json!({}));
    (StatusCode::CREATED, Json(serde_json::json!({"comment":comment}))).into_response()
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
    event_type: String,
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
                let scan_window = (limit.saturating_mul(20)).clamp(200, 5000);
                let start = lines.len().saturating_sub(scan_window);
                for (idx, line) in lines[start..].iter().enumerate() {
                    let Ok(evt) = serde_json::from_str::<Value>(line) else {
                        continue;
                    };

                    let event_type = evt
                        .get("event_type")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown");

                    // Admin tools page usage should reflect MCP tool activity.
                    // Ignore non-tool audit events (auth, csrf, rate-limit, etc.)
                    // so active tools don't get drowned out in the recent feed.
                    if !event_type.starts_with("tool.invoke_") {
                        continue;
                    }

                    let payload = evt
                        .get("payload")
                        .cloned()
                        .unwrap_or_else(|| Value::Object(Default::default()));
                    let tool_name = payload
                        .get("tool_name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown_tool")
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
                        .or_else(|| {
                            payload
                                .get("reason")
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_owned())
                        });

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
                        event_type: event_type.to_owned(),
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
        let entry = sessions
            .entry(log.session_id.clone())
            .or_insert(SessionMetrics {
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
        recent_logs: logs.into_iter().rev().take(limit).collect(),
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
    let x_request_id = HeaderName::from_static(X_REQUEST_ID);

    let audit = audit_log.unwrap_or_else(AuditLog::from_env);

    let rate_limiter = RateLimiter::new(config.rate_limit_rps, config.rate_limit_burst);
    let validation = ValidationConfig {
        max_body_size: config.max_body_size,
        audit: audit.clone(),
    };
    let csrf = CsrfConfig {
        enabled: config.csrf_enabled,
        cookie_name: config.csrf_cookie_name.clone(),
        cookie_domain: config.csrf_cookie_domain.clone(),
        header_name: config.csrf_header_name.clone(),
        audit: audit.clone(),
        ..CsrfConfig::default()
    };
    let observability = Observability::from_env();

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
    let mut tool_cfg = policy
        .as_ref()
        .and_then(crate::tool_router::ToolRuntimeConfig::from_rbac_policy)
        .unwrap_or_else(crate::tool_router::ToolRuntimeConfig::builtins);

    // Ensure core Exponential HTTP tools are enabled for gateway-owned
    // /api/exponential routes, even when runtime config defaults to deny.
    for tool in [
        "list_tasks",
        "create_task",
        "get_task",
        "update_task",
        "delete_task",
        "get_task_comments",
        "list_sprints",
        "create_sprint",
        "get_sprint",
        "list_projects",
        "create_project",
        "get_project",
        "get_project_tasks",
        "get_project_members",
        "get_project_permissions",
        "list_teams",
        "get_team",
        "get_team_members",
        "get_team_permissions",
    ] {
        tool_cfg.tools.entry(tool.to_owned()).or_insert(
            crate::tool_router::ToolRuntimeToolConfig {
                enabled: true,
                timeout: std::time::Duration::from_secs(30),
                max_concurrent: 16,
            },
        );
    }

    let tool_router = ToolRouter::new_with_config(
        crate::egress::EgressClient::new(crate::egress::EgressConfig::from_env()),
        tool_cfg,
    )
    .with_audit(audit.clone());

    // Proxy clients MUST have a timeout to prevent request pile-ups when
    // upstream (Vercel serverless) is slow or cold-starting.
    let proxy_client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());

    let proxy_state = ApiProxyState {
        client: proxy_client.clone(),
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
            "/api/exponential/tasks",
            axum::routing::get(list_exponential_tasks).with_state(tool_router.clone()),
        )
        .route(
            "/api/exponential/tasks",
            axum::routing::post(create_exponential_task).with_state(tool_router.clone()),
        )
        .route(
            "/api/exponential/tasks/{task_id}",
            axum::routing::get(get_exponential_task).with_state(tool_router.clone()),
        )
        .route(
            "/api/exponential/tasks/{task_id}",
            axum::routing::patch(update_exponential_task)
                .delete(delete_exponential_task)
                .with_state(tool_router.clone()),
        )
        .route(
            "/api/exponential/tasks/{task_id}/comments",
            axum::routing::get(get_exponential_task_comments).post(create_exponential_task_comment).with_state(tool_router.clone()),
        )
        .route(
            "/api/exponential/sprints",
            axum::routing::get(list_exponential_sprints).with_state(tool_router.clone()),
        )
        .route(
            "/api/exponential/sprints",
            axum::routing::post(create_exponential_sprint).with_state(tool_router.clone()),
        )
        .route(
            "/api/exponential/sprints/{sprint_id}",
            axum::routing::get(get_exponential_sprint)
                .patch(update_exponential_sprint)
                .delete(delete_exponential_sprint)
                .with_state(tool_router.clone()),
        )
        .route(
            "/api/exponential/projects",
            axum::routing::get(list_exponential_projects).with_state(tool_router.clone()),
        )
        .route(
            "/api/exponential/projects",
            axum::routing::post(create_exponential_project).with_state(tool_router.clone()),
        )
        .route(
            "/api/exponential/projects/{project_id}",
            axum::routing::get(get_exponential_project).with_state(tool_router.clone()),
        )
        .route(
            "/api/exponential/projects/{project_id}/tasks",
            axum::routing::get(get_exponential_project_tasks).with_state(tool_router.clone()),
        )
        .route(
            "/api/exponential/projects/{project_id}/members",
            axum::routing::get(get_exponential_project_members).with_state(tool_router.clone()),
        )
        .route(
            "/api/exponential/projects/{project_id}/permissions",
            axum::routing::get(get_exponential_project_permissions).with_state(tool_router.clone()),
        )
        .route(
            "/api/exponential/teams",
            axum::routing::get(list_exponential_teams).with_state(tool_router.clone()),
        )
        .route(
            "/api/exponential/teams/{team_id}",
            axum::routing::get(get_exponential_team).with_state(tool_router.clone()),
        )
        .route(
            "/api/exponential/teams/{team_id}/members",
            axum::routing::get(get_exponential_team_members).with_state(tool_router.clone()),
        )
        .route(
            "/api/exponential/teams/{team_id}/permissions",
            axum::routing::get(get_exponential_team_permissions).with_state(tool_router.clone()),
        )
        .route(
            "/api/exponential/labels",
            axum::routing::get(get_exponential_labels)
                .post(create_exponential_label)
                .delete(delete_exponential_label),
        )
        .route(
            "/api/exponential/views",
            axum::routing::get(get_exponential_views)
                .post(create_exponential_view),
        )
        .route(
            "/api/exponential/views/{view_id}",
            axum::routing::delete(delete_exponential_view),
        )
        .route(
            "/api/exponential/projects/{project_id}/assignees",
            axum::routing::get(get_exponential_project_assignees),
        )
        .route(
            "/api/greenbooks/{*path}",
            axum::routing::any(proxy_greenbooks).with_state(proxy_state.clone()),
        )
        .route(
            "/api/greenspot/{*path}",
            axum::routing::any(proxy_greenspot).with_state(proxy_state.clone()),
        )
        .route(
            "/api/users",
            axum::routing::any(proxy_users).with_state(proxy_state),
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
        .layer(axum_mw::from_fn_with_state(
            observability,
            request_log_middleware,
        ))
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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use axum::middleware::{self as axum_mw, Next};
    use axum::routing::get;
    use http_body_util::BodyExt;
    use std::collections::HashSet;
    use std::sync::Arc;
    use std::sync::Once;
    use tower::ServiceExt;

    use crate::auth::{AuthMethod, Principal};
    use crate::egress::{EgressClient, EgressConfig};
    use crate::middleware::rbac::{rbac_middleware, RbacState};
    use crate::rbac::{Policy, PolicyEngine};

    async fn body_json(resp: Response) -> serde_json::Value {
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        serde_json::from_slice(&bytes).unwrap()
    }

    fn test_policy() -> Policy {
        Policy::load_from_str(
            r#"{
                "schema_version": "0.1.0",
                "roles": {
                    "analyst": { "permissions": ["data:read"] }
                }
            }"#,
        )
        .unwrap()
    }

    fn test_principal() -> Principal {
        Principal {
            user_id: "test-user".into(),
            org_id: None,
            roles: vec!["analyst".into()],
            session_id: "test-session".into(),
            auth_method: AuthMethod::Bearer,
        }
    }

    fn set_test_exponential_base_url() {
        static INIT: Once = Once::new();
        INIT.call_once(|| {
            std::env::set_var("EXPONENTIAL_API_BASE_URL", "https://example.com");
        });
    }

    fn build_tool_router(egress: EgressClient) -> ToolRouter {
        let mut cfg = crate::tool_router::ToolRuntimeConfig::builtins();
        for tool in [
            "list_tasks",
            "get_task",
            "get_task_comments",
            "list_sprints",
            "get_sprint",
            "list_projects",
            "get_project",
            "get_project_tasks",
            "get_project_members",
            "get_project_permissions",
            "list_teams",
            "get_team",
            "get_team_members",
            "get_team_permissions",
        ] {
            cfg.tools.insert(
                tool.to_owned(),
                crate::tool_router::ToolRuntimeToolConfig {
                    enabled: true,
                    timeout: std::time::Duration::from_secs(30),
                    max_concurrent: 16,
                },
            );
        }
        ToolRouter::new_with_config(egress, cfg)
    }

    fn build_exponential_router(
        tool_router: ToolRouter,
        _proxy_state: EgressProxyState,
        rbac_state: RbacState,
        principal: Option<Principal>,
    ) -> Router {
        let mut router = Router::new()
            .route(
                "/api/exponential/tasks",
                get(list_exponential_tasks).with_state(tool_router.clone()),
            )
            .route(
                "/api/exponential/tasks/{task_id}",
                get(get_exponential_task).with_state(tool_router.clone()),
            )
            .route(
                "/api/exponential/tasks/{task_id}/comments",
                get(get_exponential_task_comments).with_state(tool_router.clone()),
            )
            .route(
                "/api/exponential/projects",
                get(list_exponential_projects).with_state(tool_router.clone()),
            )
            .route(
                "/api/exponential/projects/{project_id}",
                get(get_exponential_project).with_state(tool_router.clone()),
            )
            .route(
                "/api/exponential/projects/{project_id}/tasks",
                get(get_exponential_project_tasks).with_state(tool_router.clone()),
            )
            .route(
                "/api/exponential/projects/{project_id}/members",
                get(get_exponential_project_members).with_state(tool_router.clone()),
            )
            .route(
                "/api/exponential/projects/{project_id}/permissions",
                get(get_exponential_project_permissions).with_state(tool_router.clone()),
            )
            .route(
                "/api/exponential/teams",
                get(list_exponential_teams).with_state(tool_router.clone()),
            )
            .route(
                "/api/exponential/teams/{team_id}",
                get(get_exponential_team).with_state(tool_router.clone()),
            )
            .route(
                "/api/exponential/teams/{team_id}/members",
                get(get_exponential_team_members).with_state(tool_router.clone()),
            )
            .route(
                "/api/exponential/teams/{team_id}/permissions",
                get(get_exponential_team_permissions).with_state(tool_router.clone()),
            )
            .route(
                "/api/exponential/sprints",
                get(list_exponential_sprints).with_state(tool_router.clone()),
            )
            .route(
                "/api/exponential/sprints/{sprint_id}",
                get(get_exponential_sprint).with_state(tool_router.clone()),
            );

        router = router.layer(axum_mw::from_fn_with_state(rbac_state, rbac_middleware));

        if let Some(p) = principal {
            router = router.layer(axum_mw::from_fn(
                move |mut req: Request<Body>, next: Next| {
                    let p = p.clone();
                    async move {
                        req.extensions_mut().insert(p);
                        next.run(req).await
                    }
                },
            ));
        }

        router
    }

    #[tokio::test]
    async fn exponential_tasks_read_auth_and_shape() {
        set_test_exponential_base_url();
        let policy = test_policy();
        let engine = Arc::new(PolicyEngine::new(policy));
        let rbac_state = RbacState::new(engine, AuditLog::from_env());

        let mut allowed_hosts = HashSet::new();
        allowed_hosts.insert("example.com".to_owned());
        let egress_cfg = EgressConfig {
            allowed_hosts,
            deny_private_ips: false,
            ..EgressConfig::default()
        };

        let response = serde_json::json!({
            "tasks": [{ "id": "task_1" }],
            "task": { "id": "task_1" },
            "comments": [{ "id": "comment_1" }]
        })
        .to_string();

        let egress = EgressClient::new(egress_cfg).with_static_response(200, response);
        let tool_router = build_tool_router(egress.clone());
        let proxy_state = EgressProxyState {
            client: egress,
            upstream_base: "https://example.com".to_string(),
        };

        let unauth_router = build_exponential_router(
            tool_router.clone(),
            proxy_state.clone(),
            rbac_state.clone(),
            None,
        );
        let resp = unauth_router
            .oneshot(
                Request::builder()
                    .uri("/api/exponential/tasks")
                    .header("authorization", "Bearer test")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);

        let authed_router = build_exponential_router(
            tool_router,
            proxy_state,
            rbac_state,
            Some(test_principal()),
        );
        let resp = authed_router.clone()
            .oneshot(
                Request::builder()
                    .uri("/api/exponential/tasks")
                    .header("authorization", "Bearer test")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = body_json(resp).await;
        assert!(body.get("tasks").and_then(|v| v.as_array()).is_some());

        let resp = authed_router.clone()
            .oneshot(
                Request::builder()
                    .uri("/api/exponential/tasks/task_1")
                    .header("authorization", "Bearer test")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = body_json(resp).await;
        assert!(body.get("task").and_then(|v| v.as_object()).is_some());

        let resp = authed_router
            .oneshot(
                Request::builder()
                    .uri("/api/exponential/tasks/task_1/comments")
                    .header("authorization", "Bearer test")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = body_json(resp).await;
        assert!(body.get("comments").and_then(|v| v.as_array()).is_some());
    }

    #[tokio::test]
    async fn exponential_projects_and_teams_auth_and_shape() {
        set_test_exponential_base_url();
        let mut allowed_hosts = HashSet::new();
        allowed_hosts.insert("example.com".to_owned());
        let egress_cfg = EgressConfig {
            allowed_hosts,
            deny_private_ips: false,
            ..EgressConfig::default()
        };

        let response = serde_json::json!({
            "projects": [{ "id": "project_1" }],
            "teams": [{ "id": "team_1" }],
            "tasks": [{ "id": "task_1" }]
        })
        .to_string();

        let egress = EgressClient::new(egress_cfg).with_static_response(200, response);
        let tool_router = build_tool_router(egress.clone());
        let proxy_state = EgressProxyState {
            client: egress,
            upstream_base: "https://example.com".to_string(),
        };

        let policy = test_policy();
        let engine = Arc::new(PolicyEngine::new(policy));
        let rbac_state = RbacState::new(engine, AuditLog::from_env());

        let unauth_router = build_exponential_router(
            tool_router.clone(),
            proxy_state.clone(),
            rbac_state.clone(),
            None,
        );
        let resp = unauth_router.clone()
            .oneshot(
                Request::builder()
                    .uri("/api/exponential/projects")
                    .header("authorization", "Bearer test")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);

        let resp = unauth_router
            .oneshot(
                Request::builder()
                    .uri("/api/exponential/projects/project_1/tasks")
                    .header("authorization", "Bearer test")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);

        let authed_router = build_exponential_router(
            tool_router,
            proxy_state,
            rbac_state,
            Some(test_principal()),
        );
        let resp = authed_router.clone()
            .oneshot(
                Request::builder()
                    .uri("/api/exponential/teams/team_1")
                    .header("authorization", "Bearer test")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = body_json(resp).await;
        assert!(body.get("teams").and_then(|v| v.as_array()).is_some());

        let resp = authed_router
            .oneshot(
                Request::builder()
                    .uri("/api/exponential/projects/project_1/tasks?limit=25&cursor=abc")
                    .header("authorization", "Bearer test")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = body_json(resp).await;
        assert!(body.get("tasks").and_then(|v| v.as_array()).is_some());
    }

    #[tokio::test]
    async fn exponential_members_permissions_and_sprints_auth_and_shape() {
        set_test_exponential_base_url();
        let mut allowed_hosts = HashSet::new();
        allowed_hosts.insert("example.com".to_owned());
        let egress_cfg = EgressConfig {
            allowed_hosts,
            deny_private_ips: false,
            ..EgressConfig::default()
        };

        let response = serde_json::json!({
            "members": [{ "id": "member_1" }],
            "permissions": [{ "id": "perm_1" }],
            "sprints": [{ "id": "sprint_1" }],
            "sprint": { "id": "sprint_1" }
        })
        .to_string();

        let egress = EgressClient::new(egress_cfg).with_static_response(200, response);
        let tool_router = build_tool_router(egress.clone());
        let proxy_state = EgressProxyState {
            client: egress,
            upstream_base: "https://example.com".to_string(),
        };

        let policy = test_policy();
        let engine = Arc::new(PolicyEngine::new(policy));
        let rbac_state = RbacState::new(engine, AuditLog::from_env());

        let unauth_router = build_exponential_router(
            tool_router.clone(),
            proxy_state.clone(),
            rbac_state.clone(),
            None,
        );
        let routes = vec![
            ("/api/exponential/projects/project_1/members", "members", true),
            (
                "/api/exponential/projects/project_1/permissions?action=view",
                "hasPermission",
                false,
            ),
            ("/api/exponential/teams/team_1/members", "members", true),
            (
                "/api/exponential/teams/team_1/permissions?action=view",
                "hasPermission",
                false,
            ),
            ("/api/exponential/sprints?projectId=project_1", "sprints", true),
            ("/api/exponential/sprints/sprint_1", "sprint", false),
        ];

        for (uri, _, _) in &routes {
            let resp = unauth_router
                .clone()
                .oneshot(
                    Request::builder()
                        .uri(*uri)
                        .header("authorization", "Bearer test")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        }

        let authed_router = build_exponential_router(
            tool_router,
            proxy_state,
            rbac_state,
            Some(test_principal()),
        );
        for (uri, key, is_array) in routes {
            let resp = authed_router
                .clone()
                .oneshot(
                    Request::builder()
                        .uri(uri)
                        .header("authorization", "Bearer test")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(resp.status(), StatusCode::OK);
            let body = body_json(resp).await;
            if is_array {
                assert!(body.get(key).and_then(|v| v.as_array()).is_some());
            } else if key == "hasPermission" {
                assert!(body.get(key).and_then(|v| v.as_bool()).is_some());
            } else {
                assert!(body.get(key).and_then(|v| v.as_object()).is_some());
            }
        }
    }
}
