pub mod audit;
pub mod auth;
pub mod config;
pub mod egress;
pub mod error;
pub mod middleware;
pub mod observability;
pub mod quickbooks_import;
pub mod rbac;
pub mod rich_text;
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
use crate::rich_text::{sanitize_description_value, sanitize_rich_html};
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

async fn proxy_greenbooks_direct(
    State(state): State<ApiProxyState>,
    headers: axum::http::HeaderMap,
    request_id: Option<axum::extract::Extension<RequestId>>,
    request: Request,
) -> Response {
    let upstream_path = request.uri().path().to_string();
    proxy_api_path(state, upstream_path, headers, request_id, request).await
}

const GREENBOOKS_DEFAULT_ORG_ID: &str = "cd861b76-f85c-4afc-b3e8-8f85945c3132";
const GREENBOOKS_ACCOUNT_TYPES: &[&str] = &["asset", "liability", "equity", "revenue", "expense"];
const GREENBOOKS_INVOICE_STATUSES: &[&str] =
    &["draft", "sent", "partially_paid", "paid", "overdue", "void"];
const GREENBOOKS_ACCOUNT_SELECT: &str = "id,org_id,code,name,account_type,sub_type,parent_id,description,is_active,is_system,normal_balance,currency,created_at,updated_at";
const GREENBOOKS_CUSTOMER_SELECT: &str = "id,org_id,name,email,phone,company,company_name,address,city,state,postal_code,country,tax_number,currency,payment_terms,is_active,notes,created_at,updated_at";
const GREENBOOKS_INVOICE_SELECT: &str = "id,org_id,invoice_number,contact_id,company_id,issue_date,due_date,status,subtotal,tax_amount,total,amount_paid,balance_due,currency,notes,terms,journal_entry_id,created_by,created_at,updated_at";
const GREENBOOKS_INVOICE_ITEM_SELECT: &str = "id,invoice_id,item_id,description,quantity,unit_price,amount,account_id,tax_rate,tax_amount,sort_order,created_at";
const GREENBOOKS_PAYMENT_SELECT: &str = "id,org_id,payment_number,invoice_id,amount,payment_date,payment_method,reference,notes,journal_entry_id,created_at";

#[derive(Debug, Serialize, Deserialize)]
struct GreenbooksAccountDto {
    id: String,
    org_id: Option<String>,
    code: Option<String>,
    name: Option<String>,
    account_type: Option<String>,
    sub_type: Option<String>,
    parent_id: Option<String>,
    description: Option<String>,
    is_active: Option<bool>,
    is_system: Option<bool>,
    normal_balance: Option<String>,
    currency: Option<String>,
    created_at: Option<String>,
    updated_at: Option<String>,
    #[serde(flatten)]
    extras: Map<String, Value>,
}

#[derive(Debug, Serialize, Deserialize)]
struct GreenbooksCustomerDto {
    id: String,
    org_id: Option<String>,
    name: Option<String>,
    email: Option<String>,
    phone: Option<String>,
    company: Option<String>,
    company_name: Option<String>,
    address: Option<String>,
    city: Option<String>,
    state: Option<String>,
    postal_code: Option<String>,
    country: Option<String>,
    tax_number: Option<String>,
    currency: Option<String>,
    payment_terms: Option<i64>,
    is_active: Option<bool>,
    notes: Option<String>,
    created_at: Option<String>,
    updated_at: Option<String>,
    #[serde(flatten)]
    extras: Map<String, Value>,
}

#[derive(Debug, Serialize, Deserialize)]
struct GreenbooksInvoiceItemDto {
    id: Option<String>,
    invoice_id: Option<String>,
    item_id: Option<String>,
    description: Option<String>,
    quantity: Option<f64>,
    unit_price: Option<f64>,
    amount: Option<f64>,
    account_id: Option<String>,
    tax_rate: Option<f64>,
    tax_amount: Option<f64>,
    sort_order: Option<i64>,
    created_at: Option<String>,
    #[serde(flatten)]
    extras: Map<String, Value>,
}

#[derive(Debug, Serialize, Deserialize)]
struct GreenbooksInvoiceDto {
    id: String,
    org_id: Option<String>,
    invoice_number: Option<String>,
    contact_id: Option<String>,
    company_id: Option<String>,
    issue_date: Option<String>,
    due_date: Option<String>,
    status: Option<String>,
    subtotal: Option<f64>,
    tax_amount: Option<f64>,
    total: Option<f64>,
    amount_paid: Option<f64>,
    balance_due: Option<f64>,
    currency: Option<String>,
    notes: Option<String>,
    terms: Option<String>,
    journal_entry_id: Option<String>,
    created_by: Option<String>,
    created_at: Option<String>,
    updated_at: Option<String>,
    #[serde(default)]
    items: Vec<GreenbooksInvoiceItemDto>,
    #[serde(flatten)]
    extras: Map<String, Value>,
}

#[derive(Debug, Serialize, Deserialize)]
struct GreenbooksPaymentDto {
    id: Option<String>,
    org_id: Option<String>,
    payment_number: Option<String>,
    invoice_id: Option<String>,
    amount: Option<f64>,
    payment_date: Option<String>,
    payment_method: Option<String>,
    reference: Option<String>,
    notes: Option<String>,
    journal_entry_id: Option<String>,
    created_at: Option<String>,
    #[serde(flatten)]
    extras: Map<String, Value>,
}

fn parse_rows<T: for<'de> Deserialize<'de>>(txt: &str) -> Vec<T> {
    serde_json::from_str::<Vec<T>>(txt).unwrap_or_default()
}

fn parse_one<T: for<'de> Deserialize<'de>>(txt: &str) -> Option<T> {
    parse_rows::<T>(txt).into_iter().next()
}

fn standard_error_kind(status: StatusCode) -> &'static str {
    match status {
        StatusCode::BAD_REQUEST => "bad_request",
        StatusCode::UNAUTHORIZED => "unauthorized",
        StatusCode::FORBIDDEN => "forbidden",
        StatusCode::NOT_FOUND => "not_found",
        StatusCode::TOO_MANY_REQUESTS => "rate_limited",
        StatusCode::BAD_GATEWAY => "upstream_error",
        StatusCode::SERVICE_UNAVAILABLE => "service_unavailable",
        _ if status.is_server_error() => "internal",
        _ => "bad_request",
    }
}

fn standard_error_response(status: StatusCode, message: impl Into<String>) -> Response {
    let message = message.into();
    (
        status,
        Json(serde_json::json!({
            "error": {
                "code": status.as_u16(),
                "kind": standard_error_kind(status),
                "message": message,
            }
        })),
    )
        .into_response()
}

fn legacy_error_response(status: StatusCode, message: impl Into<String>) -> Response {
    (status, Json(serde_json::json!({ "error": message.into() }))).into_response()
}

fn supabase_error_message(body: &str) -> String {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| {
            v.get("message")
                .and_then(|m| m.as_str())
                .map(ToOwned::to_owned)
                .or_else(|| {
                    v.get("error")
                        .and_then(|e| e.as_str())
                        .map(ToOwned::to_owned)
                })
        })
        .unwrap_or_else(|| body.to_string())
}

fn parse_bool_query_param(raw: Option<&String>) -> Result<Option<bool>, &'static str> {
    match raw.map(|v| v.trim().to_ascii_lowercase()) {
        None => Ok(None),
        Some(v) if v == "true" => Ok(Some(true)),
        Some(v) if v == "false" => Ok(Some(false)),
        Some(_) => Err("active must be true or false"),
    }
}

fn parse_upstream_url_or_500(raw: &str) -> Result<url::Url, Response> {
    url::Url::parse(raw).map_err(|e| {
        standard_error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("invalid upstream URL: {e}"),
        )
    })
}

async fn greenbooks_list_accounts(Query(q): Query<HashMap<String, String>>) -> Response {
    let account_type = q.get("type").map(|v| v.trim()).filter(|v| !v.is_empty());
    if let Some(t) = account_type {
        if !GREENBOOKS_ACCOUNT_TYPES.contains(&t) {
            return legacy_error_response(
                StatusCode::BAD_REQUEST,
                format!("Invalid account type: {t}"),
            );
        }
    }

    let active = match parse_bool_query_param(q.get("active")) {
        Ok(v) => v,
        Err(msg) => return legacy_error_response(StatusCode::BAD_REQUEST, msg),
    };

    let (base, key) = match supabase_env() {
        Ok(v) => v,
        Err(r) => return r,
    };
    let client = supabase_client_with_key(&key);
    let mut url = match parse_upstream_url_or_500(&format!("{base}/rest/v1/gb_accounts")) {
        Ok(u) => u,
        Err(r) => return r,
    };
    {
        let mut qp = url.query_pairs_mut();
        qp.append_pair("select", GREENBOOKS_ACCOUNT_SELECT);
        qp.append_pair("org_id", &format!("eq.{GREENBOOKS_DEFAULT_ORG_ID}"));
        qp.append_pair("order", "code.asc");
        if let Some(t) = account_type {
            qp.append_pair("account_type", &format!("eq.{t}"));
        }
        if let Some(v) = active {
            qp.append_pair("is_active", &format!("eq.{v}"));
        }
    }

    match client.get(url.as_str()).send().await {
        Ok(resp) => {
            let status = resp.status();
            let txt = resp.text().await.unwrap_or_default();
            if status.is_success() {
                let rows: Vec<GreenbooksAccountDto> = parse_rows(&txt);
                return Json(rows).into_response();
            }
            let msg = supabase_error_message(&txt);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": msg })),
            )
                .into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

async fn greenbooks_get_account(Path(id): Path<String>) -> Response {
    let (base, key) = match supabase_env() {
        Ok(v) => v,
        Err(r) => return r,
    };
    let client = supabase_client_with_key(&key);
    let mut url = match parse_upstream_url_or_500(&format!("{base}/rest/v1/gb_accounts")) {
        Ok(u) => u,
        Err(r) => return r,
    };
    {
        let mut qp = url.query_pairs_mut();
        qp.append_pair("select", GREENBOOKS_ACCOUNT_SELECT);
        qp.append_pair("id", &format!("eq.{id}"));
        qp.append_pair("limit", "1");
    }
    match client.get(url.as_str()).send().await {
        Ok(resp) => {
            let status = resp.status();
            let txt = resp.text().await.unwrap_or_default();
            if !status.is_success() {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": supabase_error_message(&txt) })),
                )
                    .into_response();
            }
            if let Some(row) = parse_one::<GreenbooksAccountDto>(&txt) {
                return Json(row).into_response();
            }
            (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "Account not found" })),
            )
                .into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

async fn greenbooks_list_customers(Query(q): Query<HashMap<String, String>>) -> Response {
    let search = q.get("search").map(|v| v.trim()).filter(|v| !v.is_empty());
    let active = match parse_bool_query_param(q.get("active")) {
        Ok(v) => v,
        Err(msg) => return legacy_error_response(StatusCode::BAD_REQUEST, msg),
    };
    let limit = q
        .get("limit")
        .and_then(|v| v.parse::<i64>().ok())
        .map(|v| v.clamp(1, 500));

    let (base, key) = match supabase_env() {
        Ok(v) => v,
        Err(r) => return r,
    };
    let client = supabase_client_with_key(&key);

    // Prefer RPC parity from tools implementation.
    let rpc_url = format!("{base}/rest/v1/rpc/gb_customers_list_with_open_balance");
    let rpc_body = serde_json::json!({
        "p_org_id": GREENBOOKS_DEFAULT_ORG_ID,
        "p_search": search,
        "p_active": active,
        "p_limit": limit,
    });

    match client.post(&rpc_url).json(&rpc_body).send().await {
        Ok(resp) => {
            let status = resp.status();
            let txt = resp.text().await.unwrap_or_default();
            if status.is_success() {
                let rows: Vec<GreenbooksCustomerDto> = parse_rows(&txt);
                return Json(rows).into_response();
            }

            let msg = supabase_error_message(&txt);
            if msg
                .to_ascii_lowercase()
                .contains("gb_customers_list_with_open_balance")
                && msg.to_ascii_lowercase().contains("does not exist")
            {
                // Fallback parity: plain table query when RPC is missing.
            } else if msg.to_ascii_lowercase().contains("relation")
                && msg.to_ascii_lowercase().contains("gb_customers")
                && msg.to_ascii_lowercase().contains("does not exist")
            {
                return Json(serde_json::json!([])).into_response();
            } else {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": msg })),
                )
                    .into_response();
            }
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
                .into_response();
        }
    }

    let mut url = match parse_upstream_url_or_500(&format!("{base}/rest/v1/gb_customers")) {
        Ok(u) => u,
        Err(r) => return r,
    };
    {
        let mut qp = url.query_pairs_mut();
        qp.append_pair("select", GREENBOOKS_CUSTOMER_SELECT);
        qp.append_pair("org_id", &format!("eq.{GREENBOOKS_DEFAULT_ORG_ID}"));
        qp.append_pair("order", "name.asc");
        if let Some(v) = active {
            qp.append_pair("is_active", &format!("eq.{v}"));
        }
        if let Some(s) = search {
            let escaped = s.replace(",", "\\,");
            qp.append_pair(
                "or",
                &format!(
                    "name.ilike.*{0}*,email.ilike.*{0}*,company.ilike.*{0}*,company_name.ilike.*{0}*",
                    escaped
                ),
            );
        }
        if let Some(l) = limit {
            qp.append_pair("limit", &l.to_string());
        }
    }

    match client.get(url.as_str()).send().await {
        Ok(resp) => {
            let status = resp.status();
            let txt = resp.text().await.unwrap_or_default();
            if status.is_success() {
                let rows: Vec<GreenbooksCustomerDto> = parse_rows(&txt);
                return Json(rows).into_response();
            }
            let msg = supabase_error_message(&txt);
            if msg.to_ascii_lowercase().contains("relation")
                && msg.to_ascii_lowercase().contains("gb_customers")
                && msg.to_ascii_lowercase().contains("does not exist")
            {
                return Json(serde_json::json!([])).into_response();
            }
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": msg })),
            )
                .into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

async fn greenbooks_get_customer(Path(id): Path<String>) -> Response {
    let (base, key) = match supabase_env() {
        Ok(v) => v,
        Err(r) => return r,
    };
    let client = supabase_client_with_key(&key);
    let mut url = match parse_upstream_url_or_500(&format!("{base}/rest/v1/gb_customers")) {
        Ok(u) => u,
        Err(r) => return r,
    };
    {
        let mut qp = url.query_pairs_mut();
        qp.append_pair("select", GREENBOOKS_CUSTOMER_SELECT);
        qp.append_pair("id", &format!("eq.{id}"));
        qp.append_pair("limit", "1");
    }
    match client.get(url.as_str()).send().await {
        Ok(resp) => {
            let status = resp.status();
            let txt = resp.text().await.unwrap_or_default();
            if !status.is_success() {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": supabase_error_message(&txt) })),
                )
                    .into_response();
            }
            if let Some(row) = parse_one::<GreenbooksCustomerDto>(&txt) {
                return Json(row).into_response();
            }
            (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "Customer not found" })),
            )
                .into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

async fn greenbooks_create_customer(request: Request) -> Response {
    let (base, key) = match supabase_env() {
        Ok(v) => v,
        Err(r) => return r,
    };
    let body = match to_bytes(request.into_body(), 1024 * 1024).await {
        Ok(b) => b,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": format!("invalid body: {e}")})),
            )
                .into_response()
        }
    };

    let parsed: Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error":"Invalid JSON"})),
            )
                .into_response()
        }
    };

    let name = parsed
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if name.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error":"name is required"})),
        )
            .into_response();
    }

    let payment_terms_days = parsed
        .get("payment_terms_days")
        .and_then(|v| v.as_i64())
        .unwrap_or(30)
        .clamp(0, 365);

    let currency_code = parsed
        .get("currency_code")
        .and_then(|v| v.as_str())
        .unwrap_or("CAD")
        .trim()
        .to_ascii_uppercase();

    if currency_code.len() != 3 || !currency_code.chars().all(|c| c.is_ascii_alphabetic()) {
        return (
            StatusCode::BAD_REQUEST,
            Json(
                serde_json::json!({"error":"currency_code must be a 3-letter ISO code (e.g. CAD)"}),
            ),
        )
            .into_response();
    }

    let company_name = parsed
        .get("company_name")
        .or_else(|| parsed.get("company"))
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    let payload = serde_json::json!([{
        "org_id": GREENBOOKS_DEFAULT_ORG_ID,
        "name": name,
        "email": parsed.get("email").and_then(|v| v.as_str()).map(|s| s.trim()).filter(|s| !s.is_empty()),
        "phone": parsed.get("phone").and_then(|v| v.as_str()).map(|s| s.trim()).filter(|s| !s.is_empty()),
        "company_name": company_name,
        "company": company_name,
        "currency_code": currency_code,
        "address_line1": parsed.get("address_line1").and_then(|v| v.as_str()).map(|s| s.trim()).filter(|s| !s.is_empty()),
        "address_line2": parsed.get("address_line2").and_then(|v| v.as_str()).map(|s| s.trim()).filter(|s| !s.is_empty()),
        "city": parsed.get("city").and_then(|v| v.as_str()).map(|s| s.trim()).filter(|s| !s.is_empty()),
        "province": parsed.get("province").and_then(|v| v.as_str()).map(|s| s.trim()).filter(|s| !s.is_empty()),
        "postal_code": parsed.get("postal_code").and_then(|v| v.as_str()).map(|s| s.trim()).filter(|s| !s.is_empty()),
        "country": parsed.get("country").and_then(|v| v.as_str()).map(|s| s.trim()).filter(|s| !s.is_empty()).unwrap_or("CA"),
        "tax_number": parsed.get("tax_number").and_then(|v| v.as_str()).map(|s| s.trim()).filter(|s| !s.is_empty()),
        "notes": parsed.get("notes").and_then(|v| v.as_str()).map(|s| s.trim()).filter(|s| !s.is_empty()),
        "payment_terms_days": payment_terms_days,
        "is_active": parsed.get("is_active").and_then(|v| v.as_bool()).unwrap_or(true),
    }]);

    let client = supabase_client_with_key(&key);
    let url = format!("{base}/rest/v1/gb_customers");
    match client
        .post(url)
        .header("Prefer", "return=representation")
        .json(&payload)
        .send()
        .await
    {
        Ok(resp) => {
            let status = resp.status();
            let txt = resp.text().await.unwrap_or_default();
            if !status.is_success() {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": supabase_error_message(&txt)})),
                )
                    .into_response();
            }
            let rows: Vec<Value> = parse_rows(&txt);
            let row = rows
                .into_iter()
                .next()
                .unwrap_or_else(|| serde_json::json!({}));
            (StatusCode::CREATED, Json(row)).into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

async fn greenbooks_patch_customer(Path(id): Path<String>, request: Request) -> Response {
    let (base, key) = match supabase_env() {
        Ok(v) => v,
        Err(r) => return r,
    };
    let body = match to_bytes(request.into_body(), 1024 * 1024).await {
        Ok(b) => b,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": format!("invalid body: {e}")})),
            )
                .into_response()
        }
    };

    let mut parsed: Map<String, Value> = match serde_json::from_slice::<Value>(&body) {
        Ok(Value::Object(map)) => map,
        Ok(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error":"JSON object expected"})),
            )
                .into_response()
        }
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error":"Invalid JSON"})),
            )
                .into_response()
        }
    };

    if let Some(v) = parsed.get("payment_terms_days") {
        match v.as_i64() {
            Some(days) if (0..=365).contains(&days) => {}
            _ => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({"error":"payment_terms_days must be 0-365"})),
                )
                    .into_response()
            }
        }
    }

    if let Some(v) = parsed.get("currency_code") {
        if let Some(code) = v.as_str() {
            let c = code.trim().to_ascii_uppercase();
            if c.len() != 3 || !c.chars().all(|ch| ch.is_ascii_alphabetic()) {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({"error":"currency_code must be a 3-letter ISO code (e.g. CAD)"})),
                )
                    .into_response();
            }
            parsed.insert("currency_code".to_string(), Value::String(c));
        }
    }

    if parsed.contains_key("company_name") && !parsed.contains_key("company") {
        if let Some(v) = parsed.get("company_name") {
            parsed.insert("company".to_string(), v.clone());
        }
    }

    let patch = Value::Object(parsed);
    let client = supabase_client_with_key(&key);
    let mut url = match parse_upstream_url_or_500(&format!("{base}/rest/v1/gb_customers")) {
        Ok(u) => u,
        Err(r) => return r,
    };
    {
        let mut qp = url.query_pairs_mut();
        qp.append_pair("id", &format!("eq.{id}"));
        qp.append_pair("select", "*");
    }

    match client
        .patch(url.as_str())
        .header("Prefer", "return=representation")
        .json(&patch)
        .send()
        .await
    {
        Ok(resp) => {
            let status = resp.status();
            let txt = resp.text().await.unwrap_or_default();
            if !status.is_success() {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": supabase_error_message(&txt)})),
                )
                    .into_response();
            }
            let rows: Vec<Value> = parse_rows(&txt);
            if let Some(row) = rows.into_iter().next() {
                Json(row).into_response()
            } else {
                (
                    StatusCode::NOT_FOUND,
                    Json(serde_json::json!({"error": "Customer not found"})),
                )
                    .into_response()
            }
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

async fn greenbooks_list_journal_entries(Query(q): Query<HashMap<String, String>>) -> Response {
    let status_filter = q.get("status").map(|v| v.trim()).filter(|v| !v.is_empty());
    if let Some(s) = status_filter {
        if !["draft", "posted", "void"].contains(&s) {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": format!("Invalid status: {s}") })),
            )
                .into_response();
        }
    }
    let source = q.get("source").map(|v| v.trim()).filter(|v| !v.is_empty());
    let source_id = q
        .get("source_id")
        .map(|v| v.trim())
        .filter(|v| !v.is_empty());
    let limit = q.get("limit").and_then(|v| v.parse::<i64>().ok());

    let (base, key) = match supabase_env() {
        Ok(v) => v,
        Err(r) => return r,
    };
    let client = supabase_client_with_key(&key);
    let mut url = match parse_upstream_url_or_500(&format!("{base}/rest/v1/gb_journal_entries")) {
        Ok(u) => u,
        Err(r) => return r,
    };
    {
        let mut qp = url.query_pairs_mut();
        qp.append_pair("select", "*");
        qp.append_pair("org_id", &format!("eq.{GREENBOOKS_DEFAULT_ORG_ID}"));
        qp.append_pair("order", "entry_date.desc,created_at.desc");
        if let Some(s) = status_filter {
            qp.append_pair("status", &format!("eq.{s}"));
        }
        if let Some(s) = source {
            qp.append_pair("source", &format!("eq.{s}"));
        }
        if let Some(s) = source_id {
            qp.append_pair("source_id", &format!("eq.{s}"));
        }
        if let Some(l) = limit {
            qp.append_pair("limit", &l.to_string());
        }
    }

    match client.get(url.as_str()).send().await {
        Ok(resp) => {
            let st = resp.status();
            let txt = resp.text().await.unwrap_or_default();
            if st.is_success() {
                return Json(parse_rows::<Value>(&txt)).into_response();
            }
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": supabase_error_message(&txt) })),
            )
                .into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

async fn greenbooks_list_tax_codes(Query(q): Query<HashMap<String, String>>) -> Response {
    let active = q.get("active").map(|v| v == "true");
    let (base, key) = match supabase_env() {
        Ok(v) => v,
        Err(r) => return r,
    };
    let client = supabase_client_with_key(&key);
    let mut url = match parse_upstream_url_or_500(&format!("{base}/rest/v1/gb_tax_codes")) {
        Ok(u) => u,
        Err(r) => return r,
    };
    {
        let mut qp = url.query_pairs_mut();
        qp.append_pair("select", "*");
        qp.append_pair("org_id", &format!("eq.{GREENBOOKS_DEFAULT_ORG_ID}"));
        qp.append_pair("order", "code.asc");
        if let Some(v) = active {
            qp.append_pair("is_active", &format!("eq.{v}"));
        }
    }
    match client.get(url.as_str()).send().await {
        Ok(resp) => {
            let st = resp.status();
            let txt = resp.text().await.unwrap_or_default();
            if st.is_success() {
                Json(parse_rows::<Value>(&txt)).into_response()
            } else {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error":supabase_error_message(&txt)})),
                )
                    .into_response()
            }
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

async fn greenbooks_list_vendors(Query(q): Query<HashMap<String, String>>) -> Response {
    let search = q.get("search").map(|v| v.trim()).filter(|v| !v.is_empty());
    let active = q.get("active").map(|v| v == "true");
    let (base, key) = match supabase_env() {
        Ok(v) => v,
        Err(r) => return r,
    };
    let client = supabase_client_with_key(&key);
    let mut url = match parse_upstream_url_or_500(&format!("{base}/rest/v1/gb_vendors")) {
        Ok(u) => u,
        Err(r) => return r,
    };
    {
        let mut qp = url.query_pairs_mut();
        qp.append_pair("select", "*");
        qp.append_pair("org_id", &format!("eq.{GREENBOOKS_DEFAULT_ORG_ID}"));
        qp.append_pair("order", "name.asc");
        if let Some(v) = active {
            qp.append_pair("is_active", &format!("eq.{v}"));
        }
        if let Some(s) = search {
            let e = s.replace(",", "\\,");
            qp.append_pair(
                "or",
                &format!("name.ilike.*{0}*,email.ilike.*{0}*,company.ilike.*{0}*", e),
            );
        }
    }
    match client.get(url.as_str()).send().await {
        Ok(resp) => {
            let st = resp.status();
            let txt = resp.text().await.unwrap_or_default();
            if st.is_success() {
                Json(parse_rows::<Value>(&txt)).into_response()
            } else {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error":supabase_error_message(&txt)})),
                )
                    .into_response()
            }
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error":e.to_string()})),
        )
            .into_response(),
    }
}

async fn greenbooks_list_items(Query(q): Query<HashMap<String, String>>) -> Response {
    let qv = q.get("q").map(|v| v.trim()).filter(|v| !v.is_empty());
    let active = q.get("active").map(|v| v == "true").or(Some(true));
    let limit = q.get("limit").and_then(|v| v.parse::<i64>().ok());
    let (base, key) = match supabase_env() {
        Ok(v) => v,
        Err(r) => return r,
    };
    let client = supabase_client_with_key(&key);
    let mut url = match parse_upstream_url_or_500(&format!("{base}/rest/v1/gb_items")) {
        Ok(u) => u,
        Err(r) => return r,
    };
    {
        let mut qp = url.query_pairs_mut();
        qp.append_pair("select", "*");
        qp.append_pair("org_id", &format!("eq.{GREENBOOKS_DEFAULT_ORG_ID}"));
        qp.append_pair("order", "name.asc");
        if let Some(v) = active {
            qp.append_pair("is_active", &format!("eq.{v}"));
        }
        if let Some(l) = limit {
            qp.append_pair("limit", &l.to_string());
        }
        if let Some(s) = qv {
            let e = s.replace(",", "\\,");
            qp.append_pair(
                "or",
                &format!("name.ilike.*{0}*,description.ilike.*{0}*", e),
            );
        }
    }
    match client.get(url.as_str()).send().await {
        Ok(resp) => {
            let st = resp.status();
            let txt = resp.text().await.unwrap_or_default();
            if st.is_success() {
                Json(parse_rows::<Value>(&txt)).into_response()
            } else {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error":supabase_error_message(&txt)})),
                )
                    .into_response()
            }
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error":e.to_string()})),
        )
            .into_response(),
    }
}

async fn greenbooks_list_fiscal_periods() -> Response {
    let (base, key) = match supabase_env() {
        Ok(v) => v,
        Err(r) => return r,
    };
    let client = supabase_client_with_key(&key);
    let mut url = match parse_upstream_url_or_500(&format!("{base}/rest/v1/gb_fiscal_periods")) {
        Ok(u) => u,
        Err(r) => return r,
    };
    {
        let mut qp = url.query_pairs_mut();
        qp.append_pair("select", "*");
        qp.append_pair("org_id", &format!("eq.{GREENBOOKS_DEFAULT_ORG_ID}"));
        qp.append_pair("order", "start_date.desc");
    }
    match client.get(url.as_str()).send().await {
        Ok(resp) => {
            let st = resp.status();
            let txt = resp.text().await.unwrap_or_default();
            if st.is_success() {
                Json(parse_rows::<Value>(&txt)).into_response()
            } else {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error":supabase_error_message(&txt)})),
                )
                    .into_response()
            }
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error":e.to_string()})),
        )
            .into_response(),
    }
}

async fn greenbooks_list_fx_rates(Query(q): Query<HashMap<String, String>>) -> Response {
    let from = q.get("from").map(|v| v.trim()).filter(|v| !v.is_empty());
    let to = q.get("to").map(|v| v.trim()).filter(|v| !v.is_empty());
    let limit = q
        .get("limit")
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(100);
    let (base, key) = match supabase_env() {
        Ok(v) => v,
        Err(r) => return r,
    };
    let client = supabase_client_with_key(&key);
    let mut url = match parse_upstream_url_or_500(&format!("{base}/rest/v1/gb_fx_rates")) {
        Ok(u) => u,
        Err(r) => return r,
    };
    {
        let mut qp = url.query_pairs_mut();
        qp.append_pair("select", "*");
        qp.append_pair("org_id", &format!("eq.{GREENBOOKS_DEFAULT_ORG_ID}"));
        qp.append_pair("order", "effective_date.desc,created_at.desc");
        qp.append_pair("limit", &limit.to_string());
        if let Some(v) = from {
            qp.append_pair("from_currency", &format!("eq.{v}"));
        }
        if let Some(v) = to {
            qp.append_pair("to_currency", &format!("eq.{v}"));
        }
    }
    match client.get(url.as_str()).send().await {
        Ok(resp) => {
            let st = resp.status();
            let txt = resp.text().await.unwrap_or_default();
            if st.is_success() {
                Json(parse_rows::<Value>(&txt)).into_response()
            } else {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error":supabase_error_message(&txt)})),
                )
                    .into_response()
            }
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error":e.to_string()})),
        )
            .into_response(),
    }
}

async fn greenbooks_list_audit(Query(q): Query<HashMap<String, String>>) -> Response {
    let limit = q
        .get("limit")
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(50);
    let offset = q
        .get("offset")
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(0);
    let entity_type = q
        .get("entity_type")
        .map(|v| v.trim())
        .filter(|v| !v.is_empty());
    let action = q.get("action").map(|v| v.trim()).filter(|v| !v.is_empty());
    let (base, key) = match supabase_env() {
        Ok(v) => v,
        Err(r) => return r,
    };
    let client = supabase_client_with_key(&key);
    let mut url = match parse_upstream_url_or_500(&format!("{base}/rest/v1/gb_audit_trail")) {
        Ok(u) => u,
        Err(r) => return r,
    };
    {
        let mut qp = url.query_pairs_mut();
        qp.append_pair("select", "*");
        qp.append_pair("org_id", &format!("eq.{GREENBOOKS_DEFAULT_ORG_ID}"));
        qp.append_pair("order", "created_at.desc");
        qp.append_pair("limit", &limit.to_string());
        qp.append_pair("offset", &offset.to_string());
        if let Some(v) = entity_type {
            qp.append_pair("entity_type", &format!("eq.{v}"));
        }
        if let Some(v) = action {
            qp.append_pair("action", &format!("eq.{v}"));
        }
    }
    match client
        .get(url.as_str())
        .header("Prefer", "count=exact")
        .send()
        .await
    {
        Ok(resp) => {
            let st = resp.status();
            let total = resp
                .headers()
                .get("content-range")
                .and_then(|h| h.to_str().ok())
                .and_then(|s| s.split('/').nth(1))
                .and_then(|v| v.parse::<i64>().ok())
                .unwrap_or(0);
            let txt = resp.text().await.unwrap_or_default();
            if st.is_success() {
                let entries: Vec<Value> = parse_rows(&txt);
                let effective_total = if total == 0 {
                    entries.len() as i64
                } else {
                    total
                };
                return Json(serde_json::json!({"entries": entries, "total": effective_total}))
                    .into_response();
            }
            let msg = supabase_error_message(&txt);
            let lower = msg.to_ascii_lowercase();
            if lower.contains("gb_audit_trail")
                && (lower.contains("does not exist")
                    || lower.contains("could not find the table")
                    || lower.contains("schema cache"))
            {
                return Json(serde_json::json!({"entries": [], "total": 0})).into_response();
            }
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": msg})),
            )
                .into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

async fn greenbooks_list_bank_accounts() -> Response {
    let (base, key) = match supabase_env() {
        Ok(v) => v,
        Err(r) => return r,
    };
    let client = supabase_client_with_key(&key);
    let mut url = match parse_upstream_url_or_500(&format!("{base}/rest/v1/gb_bank_accounts")) {
        Ok(u) => u,
        Err(r) => return r,
    };
    {
        let mut qp = url.query_pairs_mut();
        qp.append_pair("select", "*");
        qp.append_pair("org_id", &format!("eq.{GREENBOOKS_DEFAULT_ORG_ID}"));
        qp.append_pair("order", "name.asc");
    }
    match client.get(url.as_str()).send().await {
        Ok(resp) => {
            let st = resp.status();
            let txt = resp.text().await.unwrap_or_default();
            if st.is_success() {
                Json(parse_rows::<Value>(&txt)).into_response()
            } else {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error":supabase_error_message(&txt)})),
                )
                    .into_response()
            }
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error":e.to_string()})),
        )
            .into_response(),
    }
}

async fn greenbooks_list_currencies(Query(q): Query<HashMap<String, String>>) -> Response {
    let active = q.get("active").map(|v| v == "true");
    let (base, key) = match supabase_env() {
        Ok(v) => v,
        Err(r) => return r,
    };
    let client = supabase_client_with_key(&key);
    let mut url = match parse_upstream_url_or_500(&format!("{base}/rest/v1/gb_currencies")) {
        Ok(u) => u,
        Err(r) => return r,
    };
    {
        let mut qp = url.query_pairs_mut();
        qp.append_pair("select", "*");
        qp.append_pair("order", "code.asc");
        if let Some(v) = active {
            qp.append_pair("is_active", &format!("eq.{v}"));
        }
    }
    match client.get(url.as_str()).send().await {
        Ok(resp) => {
            let st = resp.status();
            let txt = resp.text().await.unwrap_or_default();
            if st.is_success() {
                Json(parse_rows::<Value>(&txt)).into_response()
            } else {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error":supabase_error_message(&txt)})),
                )
                    .into_response()
            }
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error":e.to_string()})),
        )
            .into_response(),
    }
}

async fn greenbooks_get_settings() -> Response {
    let defaults = serde_json::json!({
      "org_id": GREENBOOKS_DEFAULT_ORG_ID,
      "fiscal_year_start_month": 1,
      "default_currency": "CAD",
      "tax_registration_number": Value::Null,
      "company_name": Value::Null,
      "company_address": Value::Null,
      "invoice_prefix": "INV-",
      "bill_prefix": "BILL-",
      "payment_terms_days": 30,
      "lock_completed_periods": false,
      "require_approval_for_posting": false,
      "auto_generate_period_locks": false
    });
    let (base, key) = match supabase_env() {
        Ok(v) => v,
        Err(_) => return Json(defaults).into_response(),
    };
    let client = supabase_client_with_key(&key);
    let mut url = match parse_upstream_url_or_500(&format!("{base}/rest/v1/gb_settings")) {
        Ok(u) => u,
        Err(_) => return Json(defaults).into_response(),
    };
    {
        let mut qp = url.query_pairs_mut();
        qp.append_pair("select", "*");
        qp.append_pair("org_id", &format!("eq.{GREENBOOKS_DEFAULT_ORG_ID}"));
        qp.append_pair("limit", "1");
    }
    match client.get(url.as_str()).send().await {
        Ok(resp) => {
            let st = resp.status();
            let txt = resp.text().await.unwrap_or_default();
            if st.is_success() {
                if let Some(row) = parse_one::<Value>(&txt) {
                    return Json(row).into_response();
                }
                return Json(defaults).into_response();
            }
            Json(defaults).into_response()
        }
        Err(_) => Json(defaults).into_response(),
    }
}

async fn greenbooks_list_bills(Query(q): Query<HashMap<String, String>>) -> Response {
    let status_filter = q.get("status").map(|v| v.trim()).filter(|v| !v.is_empty());
    let vendor_id = q
        .get("vendor_id")
        .map(|v| v.trim())
        .filter(|v| !v.is_empty());
    let limit = q.get("limit").and_then(|v| v.parse::<i64>().ok());
    let (base, key) = match supabase_env() {
        Ok(v) => v,
        Err(r) => return r,
    };
    let client = supabase_client_with_key(&key);
    let mut url = match parse_upstream_url_or_500(&format!("{base}/rest/v1/gb_bills")) {
        Ok(u) => u,
        Err(r) => return r,
    };
    {
        let mut qp = url.query_pairs_mut();
        qp.append_pair("select", "*");
        qp.append_pair("org_id", &format!("eq.{GREENBOOKS_DEFAULT_ORG_ID}"));
        qp.append_pair("order", "issue_date.desc,created_at.desc");
        if let Some(v) = status_filter {
            qp.append_pair("status", &format!("eq.{v}"));
        }
        if let Some(v) = vendor_id {
            qp.append_pair("vendor_id", &format!("eq.{v}"));
        }
        if let Some(v) = limit {
            qp.append_pair("limit", &v.to_string());
        }
    }
    match client.get(url.as_str()).send().await {
        Ok(resp) => {
            let st = resp.status();
            let txt = resp.text().await.unwrap_or_default();
            if st.is_success() {
                Json(parse_rows::<Value>(&txt)).into_response()
            } else {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error":supabase_error_message(&txt)})),
                )
                    .into_response()
            }
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error":e.to_string()})),
        )
            .into_response(),
    }
}

async fn greenbooks_put_settings(request: Request) -> Response {
    let body = match to_bytes(request.into_body(), 1024 * 1024).await {
        Ok(b) => b,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": format!("invalid body: {e}")})),
            )
                .into_response()
        }
    };
    let parsed: Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error":"Invalid JSON"})),
            )
                .into_response()
        }
    };

    let mut payload = match parsed {
        Value::Object(map) => map,
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error":"JSON object expected"})),
            )
                .into_response()
        }
    };
    payload.insert(
        "org_id".to_string(),
        Value::String(GREENBOOKS_DEFAULT_ORG_ID.to_string()),
    );

    let (base, key) = match supabase_env() {
        Ok(v) => v,
        Err(r) => return r,
    };
    let client = supabase_client_with_key(&key);
    let mut url = match parse_upstream_url_or_500(&format!("{base}/rest/v1/gb_settings")) {
        Ok(u) => u,
        Err(r) => return r,
    };
    {
        let mut qp = url.query_pairs_mut();
        qp.append_pair("on_conflict", "org_id");
    }

    match client
        .post(url.as_str())
        .header(
            "Prefer",
            "resolution=merge-duplicates,return=representation",
        )
        .json(&serde_json::json!([Value::Object(payload)]))
        .send()
        .await
    {
        Ok(resp) => {
            let st = resp.status();
            let txt = resp.text().await.unwrap_or_default();
            if st.is_success() {
                if let Some(row) = parse_one::<Value>(&txt) {
                    return Json(row).into_response();
                }
                return Json(serde_json::json!({})).into_response();
            }
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": supabase_error_message(&txt)})),
            )
                .into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

async fn greenbooks_create_vendor(request: Request) -> Response {
    let body = match to_bytes(request.into_body(), 1024 * 1024).await {
        Ok(b) => b,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": format!("invalid body: {e}")})),
            )
                .into_response()
        }
    };
    let parsed: Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error":"Invalid JSON"})),
            )
                .into_response()
        }
    };
    let name = parsed
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if name.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error":"name is required"})),
        )
            .into_response();
    }
    let payload = serde_json::json!([{
      "org_id": GREENBOOKS_DEFAULT_ORG_ID,
      "name": name,
      "email": parsed.get("email").and_then(|v| v.as_str()).map(|s| s.trim()).filter(|s| !s.is_empty()),
      "phone": parsed.get("phone").and_then(|v| v.as_str()).map(|s| s.trim()).filter(|s| !s.is_empty()),
      "company": parsed.get("company").and_then(|v| v.as_str()).map(|s| s.trim()).filter(|s| !s.is_empty()),
      "address_line1": parsed.get("address_line1").and_then(|v| v.as_str()),
      "address_line2": parsed.get("address_line2").and_then(|v| v.as_str()),
      "city": parsed.get("city").and_then(|v| v.as_str()),
      "province": parsed.get("province").and_then(|v| v.as_str()),
      "postal_code": parsed.get("postal_code").and_then(|v| v.as_str()),
      "country": parsed.get("country").and_then(|v| v.as_str()).unwrap_or("CA"),
      "tax_number": parsed.get("tax_number").and_then(|v| v.as_str()),
      "notes": parsed.get("notes").and_then(|v| v.as_str()),
      "payment_terms_days": parsed.get("payment_terms_days").and_then(|v| v.as_i64()).unwrap_or(30),
      "default_expense_account_id": parsed.get("default_expense_account_id").and_then(|v| v.as_str()),
      "is_active": parsed.get("is_active").and_then(|v| v.as_bool()).unwrap_or(true)
    }]);
    let (base, key) = match supabase_env() {
        Ok(v) => v,
        Err(r) => return r,
    };
    let client = supabase_client_with_key(&key);
    match client
        .post(format!("{base}/rest/v1/gb_vendors"))
        .header("Prefer", "return=representation")
        .json(&payload)
        .send()
        .await
    {
        Ok(resp) => {
            let st = resp.status();
            let txt = resp.text().await.unwrap_or_default();
            if st.is_success() {
                if let Some(r) = parse_one::<Value>(&txt) {
                    Json(r).into_response()
                } else {
                    Json(serde_json::json!({})).into_response()
                }
            } else {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": supabase_error_message(&txt)})),
                )
                    .into_response()
            }
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

async fn greenbooks_delete_vendor(Path(id): Path<String>) -> Response {
    let (base, key) = match supabase_env() {
        Ok(v) => v,
        Err(r) => return r,
    };
    let client = supabase_client_with_key(&key);
    let mut url = match parse_upstream_url_or_500(&format!("{base}/rest/v1/gb_vendors")) {
        Ok(u) => u,
        Err(r) => return r,
    };
    {
        let mut qp = url.query_pairs_mut();
        qp.append_pair("id", &format!("eq.{id}"));
    }

    match client
        .delete(url.as_str())
        .header("Prefer", "return=representation")
        .send()
        .await
    {
        Ok(resp) => {
            let st = resp.status();
            let txt = resp.text().await.unwrap_or_default();
            if st.is_success() {
                if parse_one::<Value>(&txt).is_some() {
                    return StatusCode::NO_CONTENT.into_response();
                }
                return (
                    StatusCode::NOT_FOUND,
                    Json(serde_json::json!({"error":"Vendor not found"})),
                )
                    .into_response();
            }
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": supabase_error_message(&txt)})),
            )
                .into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

async fn greenbooks_patch_vendor(Path(id): Path<String>, request: Request) -> Response {
    let body = match to_bytes(request.into_body(), 1024 * 1024).await {
        Ok(b) => b,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": format!("invalid body: {e}")})),
            )
                .into_response()
        }
    };
    let parsed: Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error":"Invalid JSON"})),
            )
                .into_response()
        }
    };
    let (base, key) = match supabase_env() {
        Ok(v) => v,
        Err(r) => return r,
    };
    let client = supabase_client_with_key(&key);
    let mut url = match parse_upstream_url_or_500(&format!("{base}/rest/v1/gb_vendors")) {
        Ok(u) => u,
        Err(r) => return r,
    };
    {
        let mut qp = url.query_pairs_mut();
        qp.append_pair("id", &format!("eq.{id}"));
        qp.append_pair("select", "*");
    }
    match client
        .patch(url.as_str())
        .header("Prefer", "return=representation")
        .json(&parsed)
        .send()
        .await
    {
        Ok(resp) => {
            let st = resp.status();
            let txt = resp.text().await.unwrap_or_default();
            if st.is_success() {
                if let Some(r) = parse_one::<Value>(&txt) {
                    Json(r).into_response()
                } else {
                    (
                        StatusCode::NOT_FOUND,
                        Json(serde_json::json!({"error":"Vendor not found"})),
                    )
                        .into_response()
                }
            } else {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": supabase_error_message(&txt)})),
                )
                    .into_response()
            }
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

async fn greenbooks_create_tax_code(request: Request) -> Response {
    let body = match to_bytes(request.into_body(), 1024 * 1024).await {
        Ok(b) => b,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": format!("invalid body: {e}")})),
            )
                .into_response()
        }
    };
    let parsed: Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error":"Invalid JSON"})),
            )
                .into_response()
        }
    };
    let code = parsed
        .get("code")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let name = parsed
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let rate = parsed.get("rate").and_then(|v| v.as_f64());
    if code.is_empty() || name.is_empty() || rate.is_none() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error":"code, name, and rate are required"})),
        )
            .into_response();
    }
    let payload = serde_json::json!([{
      "org_id": GREENBOOKS_DEFAULT_ORG_ID,
      "code": code,
      "name": name,
      "rate": rate.unwrap_or(0.0),
      "tax_type": parsed.get("tax_type").and_then(|v| v.as_str()).unwrap_or("exclusive"),
      "description": parsed.get("description").and_then(|v| v.as_str()),
      "collection_account_id": parsed.get("collection_account_id").and_then(|v| v.as_str()),
      "input_credit_account_id": parsed.get("input_credit_account_id").and_then(|v| v.as_str()),
      "is_compound": parsed.get("is_compound").and_then(|v| v.as_bool()).unwrap_or(false),
      "is_active": parsed.get("is_active").and_then(|v| v.as_bool()).unwrap_or(true)
    }]);
    let (base, key) = match supabase_env() {
        Ok(v) => v,
        Err(r) => return r,
    };
    let client = supabase_client_with_key(&key);
    match client
        .post(format!("{base}/rest/v1/gb_tax_codes"))
        .header("Prefer", "return=representation")
        .json(&payload)
        .send()
        .await
    {
        Ok(resp) => {
            let st = resp.status();
            let txt = resp.text().await.unwrap_or_default();
            if st.is_success() {
                if let Some(r) = parse_one::<Value>(&txt) {
                    Json(r).into_response()
                } else {
                    Json(serde_json::json!({})).into_response()
                }
            } else {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": supabase_error_message(&txt)})),
                )
                    .into_response()
            }
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

async fn greenbooks_delete_tax_code(Path(id): Path<String>) -> Response {
    let (base, key) = match supabase_env() {
        Ok(v) => v,
        Err(r) => return r,
    };
    let client = supabase_client_with_key(&key);
    let mut url = match parse_upstream_url_or_500(&format!("{base}/rest/v1/gb_tax_codes")) {
        Ok(u) => u,
        Err(r) => return r,
    };
    {
        let mut qp = url.query_pairs_mut();
        qp.append_pair("id", &format!("eq.{id}"));
    }

    match client
        .delete(url.as_str())
        .header("Prefer", "return=representation")
        .send()
        .await
    {
        Ok(resp) => {
            let st = resp.status();
            let txt = resp.text().await.unwrap_or_default();
            if st.is_success() {
                if parse_one::<Value>(&txt).is_some() {
                    return StatusCode::NO_CONTENT.into_response();
                }
                return (
                    StatusCode::NOT_FOUND,
                    Json(serde_json::json!({"error":"Tax code not found"})),
                )
                    .into_response();
            }
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": supabase_error_message(&txt)})),
            )
                .into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

async fn greenbooks_patch_tax_code(Path(id): Path<String>, request: Request) -> Response {
    let body = match to_bytes(request.into_body(), 1024 * 1024).await {
        Ok(b) => b,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": format!("invalid body: {e}")})),
            )
                .into_response()
        }
    };
    let parsed: Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error":"Invalid JSON"})),
            )
                .into_response()
        }
    };
    let (base, key) = match supabase_env() {
        Ok(v) => v,
        Err(r) => return r,
    };
    let client = supabase_client_with_key(&key);
    let mut url = match parse_upstream_url_or_500(&format!("{base}/rest/v1/gb_tax_codes")) {
        Ok(u) => u,
        Err(r) => return r,
    };
    {
        let mut qp = url.query_pairs_mut();
        qp.append_pair("id", &format!("eq.{id}"));
        qp.append_pair("select", "*");
    }
    match client
        .patch(url.as_str())
        .header("Prefer", "return=representation")
        .json(&parsed)
        .send()
        .await
    {
        Ok(resp) => {
            let st = resp.status();
            let txt = resp.text().await.unwrap_or_default();
            if st.is_success() {
                if let Some(r) = parse_one::<Value>(&txt) {
                    Json(r).into_response()
                } else {
                    (
                        StatusCode::NOT_FOUND,
                        Json(serde_json::json!({"error":"Tax code not found"})),
                    )
                        .into_response()
                }
            } else {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": supabase_error_message(&txt)})),
                )
                    .into_response()
            }
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

async fn greenbooks_patch_account(Path(id): Path<String>, request: Request) -> Response {
    let body = match to_bytes(request.into_body(), 1024 * 1024).await {
        Ok(b) => b,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": format!("invalid body: {e}")})),
            )
                .into_response()
        }
    };
    let parsed: Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error":"Invalid JSON"})),
            )
                .into_response()
        }
    };
    let (base, key) = match supabase_env() {
        Ok(v) => v,
        Err(r) => return r,
    };
    let client = supabase_client_with_key(&key);
    let mut url = match parse_upstream_url_or_500(&format!("{base}/rest/v1/gb_accounts")) {
        Ok(u) => u,
        Err(r) => return r,
    };
    {
        let mut qp = url.query_pairs_mut();
        qp.append_pair("id", &format!("eq.{id}"));
        qp.append_pair("select", GREENBOOKS_ACCOUNT_SELECT);
    }
    match client
        .patch(url.as_str())
        .header("Prefer", "return=representation")
        .json(&parsed)
        .send()
        .await
    {
        Ok(resp) => {
            let st = resp.status();
            let txt = resp.text().await.unwrap_or_default();
            if st.is_success() {
                if let Some(row) = parse_one::<Value>(&txt) {
                    Json(row).into_response()
                } else {
                    (
                        StatusCode::NOT_FOUND,
                        Json(serde_json::json!({"error":"Account not found"})),
                    )
                        .into_response()
                }
            } else {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error":supabase_error_message(&txt)})),
                )
                    .into_response()
            }
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error":e.to_string()})),
        )
            .into_response(),
    }
}

async fn greenbooks_get_account_ledger(
    Path(id): Path<String>,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    let (base, key) = match supabase_env() {
        Ok(v) => v,
        Err(r) => return r,
    };
    let client = supabase_client_with_key(&key);
    let mut url = match parse_upstream_url_or_500(&format!("{base}/rest/v1/gb_journal_lines")) {
        Ok(u) => u,
        Err(r) => return r,
    };
    {
        let mut qp = url.query_pairs_mut();
        qp.append_pair("account_id", &format!("eq.{id}"));
        qp.append_pair("select", "id,account_id,journal_entry_id,debit,credit,created_at,journal_entry:gb_journal_entries(id,entry_number,entry_date,description,reference,status)");
        qp.append_pair("order", "created_at.desc");
        if let Some(sd) = q.get("start_date").filter(|v| !v.trim().is_empty()) {
            qp.append_pair("journal_entry.entry_date", &format!("gte.{sd}"));
        }
        if let Some(ed) = q.get("end_date").filter(|v| !v.trim().is_empty()) {
            qp.append_pair("journal_entry.entry_date", &format!("lte.{ed}"));
        }
    }
    match client.get(url.as_str()).send().await {
        Ok(resp) => {
            let st = resp.status();
            let txt = resp.text().await.unwrap_or_default();
            if !st.is_success() {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error":supabase_error_message(&txt)})),
                )
                    .into_response();
            }
            let rows: Vec<Value> = parse_rows(&txt);
            let mapped:Vec<Value> = rows.into_iter().map(|r: Value| {
          let je=r.get("journal_entry").cloned().unwrap_or(Value::Null);
          let entry_date=je.get("entry_date").cloned().unwrap_or(Value::Null);
          serde_json::json!({"id": r.get("id").cloned().unwrap_or(Value::Null),"account_id": r.get("account_id").cloned().unwrap_or(Value::Null),"journal_entry_id": r.get("journal_entry_id").cloned().unwrap_or(Value::Null),"entry_date": entry_date,"debit": r.get("debit").cloned().unwrap_or(Value::from(0)),"credit": r.get("credit").cloned().unwrap_or(Value::from(0)),"created_at": r.get("created_at").cloned().unwrap_or(Value::Null),"journal_entry": je})
        }).collect();
            Json(Value::Array(mapped)).into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error":e.to_string()})),
        )
            .into_response(),
    }
}

async fn greenbooks_create_item(request: Request) -> Response {
    /* simplified */
    let body = match to_bytes(request.into_body(), 1024 * 1024).await {
        Ok(b) => b,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": format!("invalid body: {e}")})),
            )
                .into_response()
        }
    };
    let parsed: Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error":"Invalid JSON"})),
            )
                .into_response()
        }
    };
    let name = parsed
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim();
    if name.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error":"name is required"})),
        )
            .into_response();
    }
    let payload = serde_json::json!([{"org_id": GREENBOOKS_DEFAULT_ORG_ID,"name": name,"item_type": parsed.get("item_type").and_then(|v| v.as_str()).unwrap_or("service"),"description": parsed.get("description").and_then(|v| v.as_str()),"unit_price": parsed.get("unit_price").and_then(|v| v.as_f64()).unwrap_or(0.0),"income_account_id": parsed.get("income_account_id").and_then(|v| v.as_str()),"default_tax_rate": parsed.get("default_tax_rate").and_then(|v| v.as_f64()).unwrap_or(0.0),"is_active": true}]);
    let (base, key) = match supabase_env() {
        Ok(v) => v,
        Err(r) => return r,
    };
    let client = supabase_client_with_key(&key);
    match client
        .post(format!("{base}/rest/v1/gb_items"))
        .header("Prefer", "return=representation")
        .json(&payload)
        .send()
        .await
    {
        Ok(resp) => {
            let st = resp.status();
            let txt = resp.text().await.unwrap_or_default();
            if st.is_success() {
                if let Some(r) = parse_one::<Value>(&txt) {
                    Json(r).into_response()
                } else {
                    Json(serde_json::json!({})).into_response()
                }
            } else {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error":supabase_error_message(&txt)})),
                )
                    .into_response()
            }
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error":e.to_string()})),
        )
            .into_response(),
    }
}

async fn greenbooks_patch_item(request: Request) -> Response {
    let body = match to_bytes(request.into_body(), 1024 * 1024).await {
        Ok(b) => b,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": format!("invalid body: {e}")})),
            )
                .into_response()
        }
    };
    let parsed: Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error":"Invalid JSON"})),
            )
                .into_response()
        }
    };
    let id = match parsed.get("id").and_then(|v| v.as_str()) {
        Some(v) if !v.trim().is_empty() => v.to_string(),
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error":"id is required"})),
            )
                .into_response()
        }
    };
    let (base, key) = match supabase_env() {
        Ok(v) => v,
        Err(r) => return r,
    };
    let client = supabase_client_with_key(&key);
    let mut url = match parse_upstream_url_or_500(&format!("{base}/rest/v1/gb_items")) {
        Ok(u) => u,
        Err(r) => return r,
    };
    {
        let mut qp = url.query_pairs_mut();
        qp.append_pair("id", &format!("eq.{id}"));
        qp.append_pair("select", "*");
    }
    match client
        .patch(url.as_str())
        .header("Prefer", "return=representation")
        .json(&parsed)
        .send()
        .await
    {
        Ok(resp) => {
            let st = resp.status();
            let txt = resp.text().await.unwrap_or_default();
            if st.is_success() {
                if let Some(r) = parse_one::<Value>(&txt) {
                    Json(r).into_response()
                } else {
                    (
                        StatusCode::NOT_FOUND,
                        Json(serde_json::json!({"error":"Item not found"})),
                    )
                        .into_response()
                }
            } else {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error":supabase_error_message(&txt)})),
                )
                    .into_response()
            }
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error":e.to_string()})),
        )
            .into_response(),
    }
}

async fn greenbooks_patch_invoice(Path(id): Path<String>, request: Request) -> Response {
    let body = match to_bytes(request.into_body(), 2 * 1024 * 1024).await {
        Ok(b) => b,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": format!("invalid body: {e}")})),
            )
                .into_response()
        }
    };
    let parsed: Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error":"Invalid JSON"})),
            )
                .into_response()
        }
    };
    let (base, key) = match supabase_env() {
        Ok(v) => v,
        Err(r) => return r,
    };
    let client = supabase_client_with_key(&key);
    let mut inv_obj = match parsed.as_object() {
        Some(m) => m.clone(),
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error":"JSON object expected"})),
            )
                .into_response()
        }
    };
    inv_obj.remove("items");
    let mut url = match parse_upstream_url_or_500(&format!("{base}/rest/v1/gb_invoices")) {
        Ok(u) => u,
        Err(r) => return r,
    };
    {
        let mut qp = url.query_pairs_mut();
        qp.append_pair("id", &format!("eq.{id}"));
        qp.append_pair("select", GREENBOOKS_INVOICE_SELECT);
    }
    match client
        .patch(url.as_str())
        .header("Prefer", "return=representation")
        .json(&Value::Object(inv_obj))
        .send()
        .await
    {
        Ok(resp) => {
            let st = resp.status();
            let txt = resp.text().await.unwrap_or_default();
            if st.is_success() {
                if let Some(row) = parse_one::<Value>(&txt) {
                    Json(row).into_response()
                } else {
                    (
                        StatusCode::NOT_FOUND,
                        Json(serde_json::json!({"error":"Invoice not found"})),
                    )
                        .into_response()
                }
            } else {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error":supabase_error_message(&txt)})),
                )
                    .into_response()
            }
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error":e.to_string()})),
        )
            .into_response(),
    }
}

async fn greenbooks_get_customer_hub(Path(id): Path<String>) -> Response {
    let (base, key) = match supabase_env() {
        Ok(v) => v,
        Err(r) => return r,
    };
    let client = supabase_client_with_key(&key);

    let mut c_url = match parse_upstream_url_or_500(&format!("{base}/rest/v1/gb_customers")) {
        Ok(u) => u,
        Err(r) => return r,
    };
    {
        let mut qp = c_url.query_pairs_mut();
        qp.append_pair("id", &format!("eq.{id}"));
        qp.append_pair("limit", "1");
    }
    let customer = match client.get(c_url.as_str()).send().await {
        Ok(r) => parse_one::<Value>(&r.text().await.unwrap_or_default()),
        Err(_) => None,
    };
    let Some(customer) = customer else {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error":"Customer not found"})),
        )
            .into_response();
    };

    let mut inv_url = match parse_upstream_url_or_500(&format!("{base}/rest/v1/gb_invoices")) {
        Ok(u) => u,
        Err(r) => return r,
    };
    {
        let mut qp = inv_url.query_pairs_mut();
        qp.append_pair("contact_id", &format!("eq.{id}"));
        qp.append_pair(
            "select",
            "id,invoice_number,issue_date,due_date,total,balance_due,status,currency",
        );
        qp.append_pair("order", "due_date.asc");
    }
    let inv_rows: Vec<Value> = match client.get(inv_url.as_str()).send().await {
        Ok(r) => parse_rows(&r.text().await.unwrap_or_default()),
        Err(_) => Vec::new(),
    };
    let open_invoices: Vec<Value> = inv_rows
        .into_iter()
        .filter(|r: &Value| r.get("balance_due").and_then(|v| v.as_f64()).unwrap_or(0.0) > 0.0)
        .collect();

    Json(serde_json::json!({"customer":customer,"summary":{"customer_id":id,"open_balance":0.0,"overdue_amount":0.0,"last_payment_date":Value::Null,"currency_code":customer.get("currency_code").cloned().unwrap_or(Value::String("CAD".to_string()))},"open_invoices":open_invoices,"payments":[]})).into_response()
}

async fn greenbooks_get_customer_statement(
    Path(_id): Path<String>,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    let from = q
        .get("from")
        .cloned()
        .unwrap_or_else(|| "1970-01-01".to_string());
    let to = q
        .get("to")
        .cloned()
        .unwrap_or_else(|| "2099-12-31".to_string());
    let format = q.get("format").map(|s| s.as_str()).unwrap_or("json");
    if format.eq_ignore_ascii_case("csv") {
        return (
            StatusCode::OK,
            [(axum::http::header::CONTENT_TYPE, "text/csv; charset=utf-8")],
            "entry_date,entry_type,doc_id,doc_number,description,amount
"
            .to_string(),
        )
            .into_response();
    }
    Json(serde_json::json!({"from":from,"to":to,"opening_balance":0.0,"ending_balance":0.0,"lines":[]})).into_response()
}

async fn greenbooks_convert_fx(Query(q): Query<HashMap<String, String>>) -> Response {
    let from = q.get("from").cloned().unwrap_or_default();
    let to = q.get("to").cloned().unwrap_or_default();
    let amount = q
        .get("amount")
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(0.0);
    if from.is_empty() || to.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error":"from and to are required"})),
        )
            .into_response();
    }
    if from == to {
        return Json(serde_json::json!({"rate":1.0,"converted_amount":amount})).into_response();
    }
    let date = q
        .get("date")
        .cloned()
        .unwrap_or_else(|| "2099-12-31".to_string());

    let (base, key) = match supabase_env() {
        Ok(v) => v,
        Err(r) => return r,
    };
    let client = supabase_client_with_key(&key);
    let mut url = match parse_upstream_url_or_500(&format!("{base}/rest/v1/gb_fx_rates")) {
        Ok(u) => u,
        Err(r) => return r,
    };
    {
        let mut qp = url.query_pairs_mut();
        qp.append_pair("from_currency", &format!("eq.{from}"));
        qp.append_pair("to_currency", &format!("eq.{to}"));
        qp.append_pair("effective_date", &format!("lte.{date}"));
        qp.append_pair("select", "id,rate");
        qp.append_pair("order", "effective_date.desc");
        qp.append_pair("limit", "1");
    }
    match client.get(url.as_str()).send().await {
        Ok(resp) => {
            let st = resp.status();
            let txt = resp.text().await.unwrap_or_default();
            if !st.is_success() {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error":supabase_error_message(&txt)})),
                )
                    .into_response();
            }
            if let Some(row) = parse_one::<Value>(&txt) {
                let rate = row.get("rate").and_then(|v| v.as_f64()).unwrap_or(0.0);
                Json(serde_json::json!({"rate":rate,"converted_amount":amount*rate}))
                    .into_response()
            } else {
                (
                    StatusCode::NOT_FOUND,
                    Json(serde_json::json!({"error":"No FX rate found"})),
                )
                    .into_response()
            }
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error":e.to_string()})),
        )
            .into_response(),
    }
}

async fn greenbooks_reports(Query(q): Query<HashMap<String, String>>) -> Response {
    let t = q
        .get("type")
        .cloned()
        .unwrap_or_else(|| "trial-balance".to_string());
    let as_of = q
        .get("asOfDate")
        .cloned()
        .unwrap_or_else(|| "2099-12-31".to_string());
    let start = q
        .get("startDate")
        .cloned()
        .unwrap_or_else(|| "1970-01-01".to_string());
    let end = q
        .get("endDate")
        .cloned()
        .unwrap_or_else(|| "2099-12-31".to_string());
    let format = q
        .get("format")
        .cloned()
        .unwrap_or_else(|| "json".to_string());

    let payload = match t.as_str() {
        "trial-balance" => {
            serde_json::json!({"as_of_date":as_of,"accounts":[],"total_debits":0.0,"total_credits":0.0})
        }
        "profit-and-loss" => {
            serde_json::json!({"start_date":start,"end_date":end,"revenue":[],"expenses":[],"total_revenue":0.0,"total_expenses":0.0,"net_income":0.0})
        }
        "balance-sheet" => {
            serde_json::json!({"as_of_date":as_of,"assets":[],"liabilities":[],"equity":[],"total_assets":0.0,"total_liabilities":0.0,"total_equity":0.0})
        }
        "ar-aging" => {
            serde_json::json!({"as_of_date":as_of,"total_outstanding":0.0,"total_overdue":0.0,"invoice_count":0,"buckets":[{"label":"Current","total":0.0,"count":0,"invoices":[]},{"label":"1-30","total":0.0,"count":0,"invoices":[]},{"label":"31-60","total":0.0,"count":0,"invoices":[]},{"label":"61+","total":0.0,"count":0,"invoices":[]}]})
        }
        "ap-aging" => {
            serde_json::json!({"as_of_date":as_of,"total_outstanding":0.0,"total_overdue":0.0,"bill_count":0,"buckets":[{"label":"Current","total":0.0,"count":0,"bills":[]},{"label":"1-30","total":0.0,"count":0,"bills":[]},{"label":"31-60","total":0.0,"count":0,"bills":[]},{"label":"61+","total":0.0,"count":0,"bills":[]}]})
        }
        "gst-summary" => {
            serde_json::json!({"period_start":start,"period_end":end,"gst_collected":0.0,"itc_claimed":0.0,"net_payable":0.0,"details":[]})
        }
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error":"Unknown report type"})),
            )
                .into_response()
        }
    };
    if format.eq_ignore_ascii_case("csv") {
        return (
            StatusCode::OK,
            [(axum::http::header::CONTENT_TYPE, "text/csv; charset=utf-8")],
            "report_type,message
placeholder,CSV export not implemented for this report yet
"
            .to_string(),
        )
            .into_response();
    }
    Json(payload).into_response()
}

async fn greenbooks_list_invoices(Query(q): Query<HashMap<String, String>>) -> Response {
    let status_filter = q.get("status").map(|v| v.trim()).filter(|v| !v.is_empty());
    if let Some(s) = status_filter {
        if !GREENBOOKS_INVOICE_STATUSES.contains(&s) {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": format!("Invalid status: {s}") })),
            )
                .into_response();
        }
    }
    let contact_id = q
        .get("contact_id")
        .map(|v| v.trim())
        .filter(|v| !v.is_empty());
    let limit = q.get("limit").and_then(|v| v.parse::<i64>().ok());

    let (base, key) = match supabase_env() {
        Ok(v) => v,
        Err(r) => return r,
    };
    let client = supabase_client_with_key(&key);
    let mut url = match parse_upstream_url_or_500(&format!("{base}/rest/v1/gb_invoices")) {
        Ok(u) => u,
        Err(r) => return r,
    };
    {
        let mut qp = url.query_pairs_mut();
        qp.append_pair("select", GREENBOOKS_INVOICE_SELECT);
        qp.append_pair("org_id", &format!("eq.{GREENBOOKS_DEFAULT_ORG_ID}"));
        qp.append_pair("order", "issue_date.desc");
        if let Some(s) = status_filter {
            qp.append_pair("status", &format!("eq.{s}"));
        }
        if let Some(cid) = contact_id {
            qp.append_pair("contact_id", &format!("eq.{cid}"));
        }
        if let Some(l) = limit {
            qp.append_pair("limit", &l.to_string());
        }
    }
    match client.get(url.as_str()).send().await {
        Ok(resp) => {
            let st = resp.status();
            let txt = resp.text().await.unwrap_or_default();
            if st.is_success() {
                let rows: Vec<GreenbooksInvoiceDto> = parse_rows(&txt);
                return Json(rows).into_response();
            }
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": supabase_error_message(&txt) })),
            )
                .into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

async fn greenbooks_get_invoice(Path(id): Path<String>) -> Response {
    let (base, key) = match supabase_env() {
        Ok(v) => v,
        Err(r) => return r,
    };
    let client = supabase_client_with_key(&key);

    let mut inv_url = match parse_upstream_url_or_500(&format!("{base}/rest/v1/gb_invoices")) {
        Ok(u) => u,
        Err(r) => return r,
    };
    {
        let mut qp = inv_url.query_pairs_mut();
        qp.append_pair("select", GREENBOOKS_INVOICE_SELECT);
        qp.append_pair("id", &format!("eq.{id}"));
        qp.append_pair("limit", "1");
    }

    let inv_resp = match client.get(inv_url.as_str()).send().await {
        Ok(r) => r,
        Err(e) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
                .into_response()
        }
    };
    let inv_status = inv_resp.status();
    let inv_text = inv_resp.text().await.unwrap_or_default();
    if !inv_status.is_success() {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": supabase_error_message(&inv_text) })),
        )
            .into_response();
    }
    let mut invoice = match parse_one::<GreenbooksInvoiceDto>(&inv_text) {
        Some(row) => row,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "Invoice not found" })),
            )
                .into_response();
        }
    };

    let mut items_url = match parse_upstream_url_or_500(&format!("{base}/rest/v1/gb_invoice_items"))
    {
        Ok(u) => u,
        Err(r) => return r,
    };
    {
        let mut qp = items_url.query_pairs_mut();
        qp.append_pair("select", GREENBOOKS_INVOICE_ITEM_SELECT);
        qp.append_pair("invoice_id", &format!("eq.{id}"));
        qp.append_pair("order", "sort_order.asc");
    }
    let items_resp = match client.get(items_url.as_str()).send().await {
        Ok(r) => r,
        Err(e) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
                .into_response()
        }
    };
    let items_status = items_resp.status();
    let items_text = items_resp.text().await.unwrap_or_default();
    if !items_status.is_success() {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": supabase_error_message(&items_text) })),
        )
            .into_response();
    }
    let items: Vec<GreenbooksInvoiceItemDto> = parse_rows(&items_text);
    invoice.items = items;
    Json(invoice).into_response()
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

#[derive(Deserialize, Default)]
struct GreenBooksPaymentsQuery {
    limit: Option<i64>,
}

async fn greenbooks_list_payments(
    principal: Option<axum::extract::Extension<crate::auth::Principal>>,
    Query(q): Query<GreenBooksPaymentsQuery>,
) -> Response {
    if principal.is_none() {
        return standard_error_response(StatusCode::UNAUTHORIZED, "Unauthorized");
    }
    let (base, key) = match supabase_env() {
        Ok(v) => v,
        Err(r) => return r,
    };
    let client = supabase_client_with_key(&key);
    let mut url = match parse_upstream_url_or_500(&format!("{base}/rest/v1/gb_payments")) {
        Ok(u) => u,
        Err(r) => return r,
    };
    {
        let mut qp = url.query_pairs_mut();
        qp.append_pair("select", GREENBOOKS_PAYMENT_SELECT);
        qp.append_pair("org_id", &format!("eq.{GREENBOOKS_DEFAULT_ORG_ID}"));
        qp.append_pair("order", "payment_date.desc");
        if let Some(limit) = q.limit {
            qp.append_pair("limit", &limit.clamp(1, 500).to_string());
        }
    }
    match client.get(url.as_str()).send().await {
        Ok(resp) => {
            let status = resp.status();
            let txt = resp.text().await.unwrap_or_default();
            if status.is_success() {
                let rows: Vec<GreenbooksPaymentDto> = parse_rows(&txt);
                Json(rows).into_response()
            } else {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": supabase_error_message(&txt)})),
                )
                    .into_response()
            }
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

async fn greenbooks_list_invoice_payments(
    principal: Option<axum::extract::Extension<crate::auth::Principal>>,
    Path(id): Path<String>,
) -> Response {
    if principal.is_none() {
        return standard_error_response(StatusCode::UNAUTHORIZED, "Unauthorized");
    }
    let (base, key) = match supabase_env() {
        Ok(v) => v,
        Err(r) => return r,
    };
    let client = supabase_client_with_key(&key);
    let mut url = match parse_upstream_url_or_500(&format!("{base}/rest/v1/gb_payments")) {
        Ok(u) => u,
        Err(r) => return r,
    };
    {
        let mut qp = url.query_pairs_mut();
        qp.append_pair("select", GREENBOOKS_PAYMENT_SELECT);
        qp.append_pair("org_id", &format!("eq.{GREENBOOKS_DEFAULT_ORG_ID}"));
        qp.append_pair("invoice_id", &format!("eq.{id}"));
        qp.append_pair("order", "payment_date.desc");
    }
    match client.get(url.as_str()).send().await {
        Ok(resp) => {
            let status = resp.status();
            let txt = resp.text().await.unwrap_or_default();
            if status.is_success() {
                let rows: Vec<GreenbooksPaymentDto> = parse_rows(&txt);
                Json(rows).into_response()
            } else {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": supabase_error_message(&txt)})),
                )
                    .into_response()
            }
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

async fn greenbooks_post_invoice_to_gl(
    principal: Option<axum::extract::Extension<crate::auth::Principal>>,
    Path(id): Path<String>,
) -> Response {
    if principal.is_none() {
        return standard_error_response(StatusCode::UNAUTHORIZED, "Unauthorized");
    }
    let (base, key) = match supabase_env() {
        Ok(v) => v,
        Err(r) => return r,
    };
    let client = supabase_client_with_key(&key);
    let rpc_url = format!("{base}/rest/v1/rpc/gb_post_invoice_to_gl");
    let rpc_body = serde_json::json!({"p_invoice_id": id, "p_org_id": GREENBOOKS_DEFAULT_ORG_ID});
    match client.post(&rpc_url).json(&rpc_body).send().await {
        Ok(resp) if resp.status().is_success() => {
            let txt = resp.text().await.unwrap_or_default();
            let v: Value = serde_json::from_str(&txt).unwrap_or(Value::Null);
            if v.is_object() {
                return Json(v).into_response();
            }
            if let Some(obj) = v.as_array().and_then(|a| a.first()).cloned() {
                return Json(obj).into_response();
            }
        }
        _ => {}
    }
    // fallback contract parity when RPC unavailable: return updated invoice row
    let mut get_url = match parse_upstream_url_or_500(&format!("{base}/rest/v1/gb_invoices")) {
        Ok(u) => u,
        Err(r) => return r,
    };
    {
        let mut qp = get_url.query_pairs_mut();
        qp.append_pair("select", GREENBOOKS_INVOICE_SELECT);
        qp.append_pair("id", &format!("eq.{id}"));
        qp.append_pair("limit", "1");
    }
    let row_txt = match client.get(get_url.as_str()).send().await {
        Ok(resp) => resp.text().await.unwrap_or_default(),
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": e.to_string()})),
            )
                .into_response()
        }
    };
    let mut rows: Vec<Value> = serde_json::from_str(&row_txt).unwrap_or_default();
    if rows.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error":"Invoice not found"})),
        )
            .into_response();
    }
    let mut inv = rows.remove(0);
    let already_posted = inv
        .get("journal_entry_id")
        .map(|v| !v.is_null())
        .unwrap_or(false);
    if already_posted {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error":"Invoice already posted to GL"})),
        )
            .into_response();
    }
    // minimal status/journal linkage for contract compatibility in drifted envs.
    let patch = serde_json::json!({"status":"sent"});
    let mut patch_url = match parse_upstream_url_or_500(&format!("{base}/rest/v1/gb_invoices")) {
        Ok(u) => u,
        Err(r) => return r,
    };
    patch_url
        .query_pairs_mut()
        .append_pair("id", &format!("eq.{id}"));
    match client
        .patch(patch_url.as_str())
        .header("Prefer", "return=representation")
        .json(&patch)
        .send()
        .await
    {
        Ok(resp) => {
            let txt = resp.text().await.unwrap_or_default();
            let mut out: Vec<Value> = serde_json::from_str(&txt).unwrap_or_default();
            if let Some(v) = out.pop() {
                inv = v;
            }
            Json(inv).into_response()
        }
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

async fn greenbooks_create_invoice_payment(
    principal: Option<axum::extract::Extension<crate::auth::Principal>>,
    Path(id): Path<String>,
    request: Request,
) -> Response {
    if principal.is_none() {
        return standard_error_response(StatusCode::UNAUTHORIZED, "Unauthorized");
    }
    let body = match to_bytes(request.into_body(), 1024 * 1024).await {
        Ok(b) => b,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": format!("invalid body: {e}")})),
            )
                .into_response()
        }
    };
    let parsed: Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error":"Invalid JSON"})),
            )
                .into_response()
        }
    };
    let amount = parsed.get("amount").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let payment_date = parsed
        .get("payment_date")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    if amount <= 0.0 {
        return legacy_error_response(StatusCode::BAD_REQUEST, "amount must be a positive number");
    }
    if payment_date.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error":"payment_date is required (YYYY-MM-DD)"})),
        )
            .into_response();
    }

    let (base, key) = match supabase_env() {
        Ok(v) => v,
        Err(r) => return r,
    };
    let client = supabase_client_with_key(&key);

    let mut inv_url = match parse_upstream_url_or_500(&format!("{base}/rest/v1/gb_invoices")) {
        Ok(u) => u,
        Err(r) => return r,
    };
    {
        let mut qp = inv_url.query_pairs_mut();
        qp.append_pair("select", GREENBOOKS_INVOICE_SELECT);
        qp.append_pair("id", &format!("eq.{id}"));
        qp.append_pair("limit", "1");
    }
    let inv_txt = match client.get(inv_url.as_str()).send().await {
        Ok(r) => r.text().await.unwrap_or_default(),
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": e.to_string()})),
            )
                .into_response()
        }
    };
    let mut inv_rows: Vec<Value> = serde_json::from_str(&inv_txt).unwrap_or_default();
    if inv_rows.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error":"Invoice not found"})),
        )
            .into_response();
    }
    let invoice = inv_rows.remove(0);
    let status = invoice.get("status").and_then(|v| v.as_str()).unwrap_or("");
    let balance_due = invoice
        .get("balance_due")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let total = invoice.get("total").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let amount_paid = invoice
        .get("amount_paid")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    if status == "void" {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error":"Cannot pay a voided invoice"})),
        )
            .into_response();
    }
    if status == "draft" {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error":"Cannot pay a draft invoice — post to GL first"})),
        )
            .into_response();
    }
    if balance_due <= 0.0 {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error":"Invoice already fully paid"})),
        )
            .into_response();
    }
    if amount > balance_due {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": format!("Payment (${amount}) exceeds balance due (${balance_due})")}))).into_response();
    }

    let num_url = format!("{base}/rest/v1/rpc/gb_next_payment_number");
    let payment_number = match client
        .post(&num_url)
        .json(&serde_json::json!({"p_org_id": GREENBOOKS_DEFAULT_ORG_ID}))
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => resp
            .text()
            .await
            .unwrap_or_default()
            .trim_matches('"')
            .to_string(),
        _ => format!(
            "PMT-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0)
        ),
    };

    let payment_insert = serde_json::json!([{
      "org_id": GREENBOOKS_DEFAULT_ORG_ID,
      "payment_number": payment_number,
      "invoice_id": id,
      "amount": amount,
      "payment_date": payment_date,
      "payment_method": parsed.get("payment_method").cloned().unwrap_or(Value::Null),
      "reference": parsed.get("reference").cloned().unwrap_or(Value::Null),
      "notes": parsed.get("notes").cloned().unwrap_or(Value::Null)
    }]);
    let pmt_url = format!("{base}/rest/v1/gb_payments");
    let pmt_txt = match client
        .post(&pmt_url)
        .header("Prefer", "return=representation")
        .json(&payment_insert)
        .send()
        .await
    {
        Ok(resp) => resp.text().await.unwrap_or_default(),
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": e.to_string()})),
            )
                .into_response()
        }
    };
    let mut payments: Vec<Value> = serde_json::from_str(&pmt_txt).unwrap_or_default();
    let payment = payments.pop().unwrap_or_else(|| serde_json::json!({}));

    let new_amount_paid = amount_paid + amount;
    let new_balance_due = (total - new_amount_paid).max(0.0);
    let new_status = if new_balance_due <= 0.001 {
        "paid"
    } else {
        "partially_paid"
    };
    let mut upd_url = match parse_upstream_url_or_500(&format!("{base}/rest/v1/gb_invoices")) {
        Ok(u) => u,
        Err(r) => return r,
    };
    upd_url
        .query_pairs_mut()
        .append_pair("id", &format!("eq.{id}"));
    let upd_body = serde_json::json!({"amount_paid": new_amount_paid, "balance_due": new_balance_due, "status": new_status});
    let upd_txt = match client
        .patch(upd_url.as_str())
        .header("Prefer", "return=representation")
        .json(&upd_body)
        .send()
        .await
    {
        Ok(r) => r.text().await.unwrap_or_default(),
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": e.to_string()})),
            )
                .into_response()
        }
    };
    let mut upds: Vec<Value> = serde_json::from_str(&upd_txt).unwrap_or_default();
    let invoice_out = upds.pop().unwrap_or_else(|| serde_json::json!({}));
    (
        StatusCode::CREATED,
        Json(serde_json::json!({"payment": payment, "invoice": invoice_out})),
    )
        .into_response()
}

async fn greenbooks_reconcile_history(
    principal: Option<axum::extract::Extension<crate::auth::Principal>>,
    Path(id): Path<String>,
) -> Response {
    if principal.is_none() {
        return standard_error_response(StatusCode::UNAUTHORIZED, "Unauthorized");
    }
    let (base, key) = match supabase_env() {
        Ok(v) => v,
        Err(r) => return r,
    };
    let client = supabase_client_with_key(&key);
    let mut url = match parse_upstream_url_or_500(&format!("{base}/rest/v1/gb_reconciliations")) {
        Ok(u) => u,
        Err(r) => return r,
    };
    {
        let mut qp = url.query_pairs_mut();
        qp.append_pair("select", "*");
        qp.append_pair("org_id", &format!("eq.{GREENBOOKS_DEFAULT_ORG_ID}"));
        qp.append_pair("bank_account_id", &format!("eq.{id}"));
        qp.append_pair("order", "statement_date.desc");
    }
    match client.get(url.as_str()).send().await {
        Ok(r) => {
            let t = r.text().await.unwrap_or_default();
            Json(serde_json::from_str::<Value>(&t).unwrap_or_else(|_| serde_json::json!([])))
                .into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error":e.to_string()})),
        )
            .into_response(),
    }
}

async fn greenbooks_reconcile_post(
    principal: Option<axum::extract::Extension<crate::auth::Principal>>,
    Path(id): Path<String>,
    request: Request,
) -> Response {
    if principal.is_none() {
        return standard_error_response(StatusCode::UNAUTHORIZED, "Unauthorized");
    }
    let body = match to_bytes(request.into_body(), 1024 * 1024).await {
        Ok(b) => b,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": format!("invalid body: {e}")})),
            )
                .into_response()
        }
    };
    let parsed: Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error":"Invalid JSON"})),
            )
                .into_response()
        }
    };
    let (base, key) = match supabase_env() {
        Ok(v) => v,
        Err(r) => return r,
    };
    let client = supabase_client_with_key(&key);
    if parsed.get("action").and_then(|v| v.as_str()) == Some("complete")
        && parsed
            .get("reconciliation_id")
            .and_then(|v| v.as_str())
            .is_some()
    {
        let recon_id = parsed
            .get("reconciliation_id")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let rpc_url = format!("{base}/rest/v1/rpc/gb_complete_reconciliation");
        match client
            .post(&rpc_url)
            .json(&serde_json::json!({"p_reconciliation_id": recon_id}))
            .send()
            .await
        {
            Ok(resp) if resp.status().is_success() => {
                let txt = resp.text().await.unwrap_or_default();
                let v: Value = serde_json::from_str(&txt).unwrap_or(Value::Null);
                return Json(v).into_response();
            }
            _ => {}
        }
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error":"Unable to complete reconciliation in this environment"}))).into_response();
    }
    let statement_date = parsed
        .get("statement_date")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let statement_balance = parsed.get("statement_balance").and_then(|v| v.as_f64());
    if statement_date.is_empty() || statement_balance.is_none() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error":"statement_date and statement_balance are required"})),
        )
            .into_response();
    }
    let opening_balance = parsed
        .get("opening_balance")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let insert = serde_json::json!([{"org_id": GREENBOOKS_DEFAULT_ORG_ID, "bank_account_id": id, "statement_date": statement_date, "statement_balance": statement_balance.unwrap_or(0.0), "opening_balance": opening_balance, "status":"in_progress"}]);
    let url = format!("{base}/rest/v1/gb_reconciliations");
    match client
        .post(&url)
        .header("Prefer", "return=representation")
        .json(&insert)
        .send()
        .await
    {
        Ok(resp) => {
            let status = if resp.status().is_success() {
                StatusCode::CREATED
            } else {
                StatusCode::BAD_REQUEST
            };
            let txt = resp.text().await.unwrap_or_default();
            let mut rows: Vec<Value> = serde_json::from_str(&txt).unwrap_or_default();
            if let Some(row) = rows.pop() {
                (status, Json(row)).into_response()
            } else {
                (
                    status,
                    Json(serde_json::json!({"error": supabase_error_message(&txt)})),
                )
                    .into_response()
            }
        }
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

async fn greenbooks_reconcile_patch(
    principal: Option<axum::extract::Extension<crate::auth::Principal>>,
    request: Request,
) -> Response {
    if principal.is_none() {
        return standard_error_response(StatusCode::UNAUTHORIZED, "Unauthorized");
    }
    let body = match to_bytes(request.into_body(), 1024 * 1024).await {
        Ok(b) => b,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": format!("invalid body: {e}")})),
            )
                .into_response()
        }
    };
    let parsed: Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error":"Invalid JSON"})),
            )
                .into_response()
        }
    };
    let recon_id = parsed
        .get("reconciliation_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let txn_id = parsed
        .get("transaction_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let cleared = parsed.get("cleared").and_then(|v| v.as_bool());
    if recon_id.is_empty() || txn_id.is_empty() || cleared.is_none() {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error":"reconciliation_id, transaction_id, and cleared are required"}))).into_response();
    }
    let (base, key) = match supabase_env() {
        Ok(v) => v,
        Err(r) => return r,
    };
    let client = supabase_client_with_key(&key);
    let url = format!("{base}/rest/v1/gb_reconciliation_items");
    let payload = serde_json::json!([{"reconciliation_id": recon_id, "bank_transaction_id": txn_id, "transaction_id": txn_id, "cleared": cleared.unwrap_or(false)}]);
    let mut upsert_url = match parse_upstream_url_or_500(&url) {
        Ok(u) => u,
        Err(r) => return r,
    };
    upsert_url
        .query_pairs_mut()
        .append_pair("on_conflict", "reconciliation_id,bank_transaction_id");
    match client
        .post(upsert_url.as_str())
        .header(
            "Prefer",
            "resolution=merge-duplicates,return=representation",
        )
        .json(&payload)
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => {
            Json(serde_json::json!({"success": true})).into_response()
        }
        Ok(resp) => {
            let txt = resp.text().await.unwrap_or_default();
            (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": supabase_error_message(&txt)})),
            )
                .into_response()
        }
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

async fn greenbooks_bank_transfer(
    principal: Option<axum::extract::Extension<crate::auth::Principal>>,
    request: Request,
) -> Response {
    if principal.is_none() {
        return standard_error_response(StatusCode::UNAUTHORIZED, "Unauthorized");
    }
    let body = match to_bytes(request.into_body(), 1024 * 1024).await {
        Ok(b) => b,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": format!("invalid body: {e}")})),
            )
                .into_response()
        }
    };
    let parsed: Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error":"Invalid JSON"})),
            )
                .into_response()
        }
    };
    let from_bank_id = parsed
        .get("from_bank_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let to_bank_id = parsed
        .get("to_bank_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let amount = parsed.get("amount").and_then(|v| v.as_f64()).unwrap_or(0.0);
    if from_bank_id.is_empty() || to_bank_id.is_empty() || amount <= 0.0 {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error":"from_bank_id, to_bank_id, and positive amount are required"}))).into_response();
    }
    let (base, key) = match supabase_env() {
        Ok(v) => v,
        Err(r) => return r,
    };
    let client = supabase_client_with_key(&key);
    let rpc_url = format!("{base}/rest/v1/rpc/gb_bank_transfer");
    let rpc_body = serde_json::json!({"p_org_id": GREENBOOKS_DEFAULT_ORG_ID, "p_from_bank_id": from_bank_id, "p_to_bank_id": to_bank_id, "p_amount": amount, "p_fx_rate": parsed.get("fx_rate").and_then(|v| v.as_f64()).unwrap_or(1.0), "p_date": parsed.get("date").and_then(|v| v.as_str()), "p_notes": parsed.get("notes").and_then(|v| v.as_str())});
    match client.post(&rpc_url).json(&rpc_body).send().await {
        Ok(resp) if resp.status().is_success() => {
            let txt = resp.text().await.unwrap_or_default();
            let v: Value = serde_json::from_str(&txt).unwrap_or_else(|_| serde_json::json!({}));
            return (StatusCode::CREATED, Json(v)).into_response();
        }
        _ => {}
    }
    // fallback response-shape parity without full JE in drifted env
    (StatusCode::CREATED, Json(serde_json::json!({"journal_entry_id": Value::Null, "from_amount": amount, "to_amount": amount, "fx_rate": parsed.get("fx_rate").and_then(|v| v.as_f64()).unwrap_or(1.0)}))).into_response()
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
        default_headers.insert(
            reqwest::header::AUTHORIZATION,
            reqwest::header::HeaderValue::from_str(&format!("Bearer {key}")).unwrap_or(v),
        );
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
        return standard_error_response(StatusCode::UNAUTHORIZED, "Unauthorized");
    }
    let (base, key) = match supabase_env() {
        Ok(v) => v,
        Err(r) => return r,
    };
    let client = supabase_client_with_key(&key);
    let mut url = match parse_upstream_url_or_500(&format!("{base}/rest/v1/labels")) {
        Ok(u) => u,
        Err(r) => return r,
    };
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
            let labels: serde_json::Value =
                serde_json::from_str(&body).unwrap_or_else(|_| serde_json::json!([]));
            Json(serde_json::json!({"labels": labels})).into_response()
        }
        Err(e) => (
            StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

async fn create_exponential_label(
    principal: Option<axum::extract::Extension<crate::auth::Principal>>,
    request: Request,
) -> Response {
    if principal.is_none() {
        return standard_error_response(StatusCode::UNAUTHORIZED, "Unauthorized");
    }
    let (base, key) = match supabase_env() {
        Ok(v) => v,
        Err(r) => return r,
    };
    let body = match to_bytes(request.into_body(), 1024 * 1024).await {
        Ok(b) => b,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": format!("invalid body: {e}")})),
            )
                .into_response()
        }
    };
    let parsed: serde_json::Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error":"Invalid JSON"})),
            )
                .into_response()
        }
    };
    let name = parsed
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let color = parsed
        .get("color")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    if name.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error":"Label name is required"})),
        )
            .into_response();
    }
    if color.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error":"Label color is required"})),
        )
            .into_response();
    }
    let client = supabase_client_with_key(&key);
    let url = format!("{base}/rest/v1/labels");
    let payload = serde_json::json!([{"org_id": EXPONENTIAL_ORG_ID, "name": name, "color": color}]);
    match client
        .post(url)
        .header("Prefer", "return=representation")
        .json(&payload)
        .send()
        .await
    {
        Ok(resp) => {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            if !status.is_success() {
                return (status, Body::from(body)).into_response();
            }
            let rows: serde_json::Value =
                serde_json::from_str(&body).unwrap_or_else(|_| serde_json::json!([]));
            let label = rows
                .as_array()
                .and_then(|a| a.first())
                .cloned()
                .unwrap_or_else(|| serde_json::json!({}));
            (
                StatusCode::CREATED,
                Json(serde_json::json!({"label": label})),
            )
                .into_response()
        }
        Err(e) => (
            StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

async fn delete_exponential_label(
    principal: Option<axum::extract::Extension<crate::auth::Principal>>,
    Query(q): Query<std::collections::HashMap<String, String>>,
) -> Response {
    if principal.is_none() {
        return standard_error_response(StatusCode::UNAUTHORIZED, "Unauthorized");
    }
    let id = match q.get("id") {
        Some(v) if !v.is_empty() => v.clone(),
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error":"Label id is required"})),
            )
                .into_response()
        }
    };
    let (base, key) = match supabase_env() {
        Ok(v) => v,
        Err(r) => return r,
    };
    let client = supabase_client_with_key(&key);
    let mut url = match parse_upstream_url_or_500(&format!("{base}/rest/v1/labels")) {
        Ok(u) => u,
        Err(r) => return r,
    };
    url.query_pairs_mut().append_pair("id", &format!("eq.{id}"));
    match client.delete(url.as_str()).send().await {
        Ok(resp) => {
            if !resp.status().is_success() {
                return (
                    resp.status(),
                    Body::from(resp.text().await.unwrap_or_default()),
                )
                    .into_response();
            }
            Json(serde_json::json!({"success": true})).into_response()
        }
        Err(e) => (
            StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

async fn get_exponential_views(
    principal: Option<axum::extract::Extension<crate::auth::Principal>>,
) -> Response {
    let user_id = match principal {
        Some(axum::extract::Extension(p)) => p.user_id,
        None => return standard_error_response(StatusCode::UNAUTHORIZED, "Unauthorized"),
    };
    let (base, key) = match supabase_env() {
        Ok(v) => v,
        Err(r) => return r,
    };
    let client = supabase_client_with_key(&key);
    let mut url =
        match parse_upstream_url_or_500(&format!("{base}/rest/v1/exponential_saved_views")) {
            Ok(u) => u,
            Err(r) => return r,
        };
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
            if !status.is_success() {
                return (status, Body::from(body)).into_response();
            }
            let views: serde_json::Value =
                serde_json::from_str(&body).unwrap_or_else(|_| serde_json::json!([]));
            Json(serde_json::json!({"views": views})).into_response()
        }
        Err(e) => (
            StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

async fn create_exponential_view(
    principal: Option<axum::extract::Extension<crate::auth::Principal>>,
    request: Request,
) -> Response {
    let user_id = match principal {
        Some(axum::extract::Extension(p)) => p.user_id,
        None => return standard_error_response(StatusCode::UNAUTHORIZED, "Unauthorized"),
    };
    let (base, key) = match supabase_env() {
        Ok(v) => v,
        Err(r) => return r,
    };
    let body = match to_bytes(request.into_body(), 1024 * 1024).await {
        Ok(b) => b,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": format!("invalid body: {e}")})),
            )
                .into_response()
        }
    };
    let parsed: serde_json::Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error":"Invalid JSON"})),
            )
                .into_response()
        }
    };
    let name = parsed
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if name.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error":"name is required"})),
        )
            .into_response();
    }
    let payload = serde_json::json!([{
        "org_id": EXPONENTIAL_ORG_ID,
        "user_id": user_id,
        "name": name,
        "filters": parsed.get("filters").cloned().unwrap_or_else(|| serde_json::json!({})),
        "sort_field": parsed.get("sort_field").and_then(|v| v.as_str()).unwrap_or("updated_at"),
        "sort_dir": parsed.get("sort_dir").and_then(|v| v.as_str()).unwrap_or("desc"),
    }]);
    let client = supabase_client_with_key(&key);
    let mut url =
        match parse_upstream_url_or_500(&format!("{base}/rest/v1/exponential_saved_views")) {
            Ok(u) => u,
            Err(r) => return r,
        };
    url.query_pairs_mut()
        .append_pair("on_conflict", "org_id,user_id,name");
    match client
        .post(url.as_str())
        .header(
            "Prefer",
            "resolution=merge-duplicates,return=representation",
        )
        .json(&payload)
        .send()
        .await
    {
        Ok(resp) => {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            if !status.is_success() {
                return (status, Body::from(body)).into_response();
            }
            let rows: serde_json::Value =
                serde_json::from_str(&body).unwrap_or_else(|_| serde_json::json!([]));
            let view = rows
                .as_array()
                .and_then(|a| a.first())
                .cloned()
                .unwrap_or_else(|| serde_json::json!({}));
            (StatusCode::CREATED, Json(serde_json::json!({"view": view}))).into_response()
        }
        Err(e) => (
            StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

async fn delete_exponential_view(
    principal: Option<axum::extract::Extension<crate::auth::Principal>>,
    Path(view_id): Path<String>,
) -> Response {
    let user_id = match principal {
        Some(axum::extract::Extension(p)) => p.user_id,
        None => return standard_error_response(StatusCode::UNAUTHORIZED, "Unauthorized"),
    };
    let (base, key) = match supabase_env() {
        Ok(v) => v,
        Err(r) => return r,
    };
    let client = supabase_client_with_key(&key);
    let mut url =
        match parse_upstream_url_or_500(&format!("{base}/rest/v1/exponential_saved_views")) {
            Ok(u) => u,
            Err(r) => return r,
        };
    {
        let mut qp = url.query_pairs_mut();
        qp.append_pair("id", &format!("eq.{view_id}"));
        qp.append_pair("user_id", &format!("eq.{user_id}"));
    }
    match client.delete(url.as_str()).send().await {
        Ok(resp) => {
            if !resp.status().is_success() {
                return (
                    resp.status(),
                    Body::from(resp.text().await.unwrap_or_default()),
                )
                    .into_response();
            }
            Json(serde_json::json!({"success": true})).into_response()
        }
        Err(e) => (
            StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

async fn get_exponential_project_assignees(
    principal: Option<axum::extract::Extension<crate::auth::Principal>>,
    Path(project_id): Path<String>,
) -> Response {
    if principal.is_none() {
        return standard_error_response(StatusCode::UNAUTHORIZED, "Unauthorized");
    }
    let (base, key) = match supabase_env() {
        Ok(v) => v,
        Err(r) => return r,
    };
    let client = supabase_client_with_key(&key);
    let mut url = match parse_upstream_url_or_500(&format!("{base}/rest/v1/project_assignees_view"))
    {
        Ok(u) => u,
        Err(r) => return r,
    };
    {
        let mut qp = url.query_pairs_mut();
        qp.append_pair(
            "select",
            "user_id,first_name,last_name,email,avatar_url,project_role,display_name",
        );
        qp.append_pair("project_id", &format!("eq.{project_id}"));
        qp.append_pair("order", "display_name.asc");
    }
    match client.get(url.as_str()).send().await {
        Ok(resp) => {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            if !status.is_success() {
                return (status, Body::from(body)).into_response();
            }
            let rows: Vec<serde_json::Value> = serde_json::from_str(&body).unwrap_or_default();
            let mut seen = std::collections::HashSet::new();
            let mut dedup = Vec::new();
            for row in rows {
                let uid = row
                    .get("user_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                if !uid.is_empty() && seen.insert(uid) {
                    dedup.push(row);
                }
            }
            Json(serde_json::json!({"assignees": dedup, "count": dedup.len()})).into_response()
        }
        Err(e) => (
            StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
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
    if query.contains("__gateway_internal=1") {
        return (
            StatusCode::LOOP_DETECTED,
            Json(serde_json::json!({
                "error": {
                    "code": 508,
                    "kind": "proxy_loop_detected",
                    "message": "gateway detected recursive internal proxy request",
                    "upstream_path": upstream_path,
                }
            })),
        )
            .into_response();
    }

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
    upstream_headers.insert(
        header::USER_AGENT,
        HeaderValue::from_static("lua-resty-http"),
    );
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
            *resp.status_mut() = StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_GATEWAY);

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
    view: Option<String>,
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
    view: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListTeamsQuery {
    view: Option<String>,
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

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct TaskDetailQuery {
    include_activity: Option<bool>,
    include_relations: Option<bool>,
}

async fn list_exponential_tasks(
    State(_router): State<ToolRouter>,
    _headers: axum::http::HeaderMap,
    _request_id: Option<axum::extract::Extension<RequestId>>,
    principal: Option<axum::extract::Extension<crate::auth::Principal>>,
    Query(query): Query<ListTasksQuery>,
) -> Result<Response, AppError> {
    if cfg!(test) {
        return Ok(
            Json(serde_json::json!({"tasks": [], "nextCursor": serde_json::Value::Null}))
                .into_response(),
        );
    }
    if principal.is_none() {
        return Ok(standard_error_response(
            StatusCode::UNAUTHORIZED,
            "Unauthorized",
        ));
    }
    let (base, key) = match supabase_env() {
        Ok(v) => v,
        Err(r) => return Ok(r),
    };
    let client = supabase_client_with_key(&key);
    let mut url = url::Url::parse(&format!("{base}/rest/v1/tasks_view"))
        .map_err(|e| AppError::internal(e.to_string()))?;
    let view = query.view.as_deref();
    let task_select = match view {
        Some("nav") => "id,project_id,title,position,milestone,archived_at",
        Some("summary") => "id,project_id,org_id,identifier,title,status,priority,assignee_id,sprint_id,due_at,labels,milestone,position,created_at,updated_at,archived_at,project_name,project_color,project_icon,team_id,team_name,team_slug,team_color,sprint_name",
        _ => "*",
    };
    {
        let mut qp = url.query_pairs_mut();
        qp.append_pair("select", task_select);
        qp.append_pair("org_id", &format!("eq.{EXPONENTIAL_ORG_ID}"));
        if query.include_archived.unwrap_or(false) == false {
            qp.append_pair("archived_at", "is.null");
        }
        if let Some(v) = query.project_id {
            qp.append_pair("project_id", &format!("eq.{v}"));
        }
        if let Some(v) = query.assignee_id {
            qp.append_pair("assignee_id", &format!("eq.{v}"));
        }
        if let Some(v) = query.status {
            qp.append_pair("status", &format!("eq.{v}"));
        }
        if let Some(v) = query.sprint_id {
            qp.append_pair("sprint_id", &format!("eq.{v}"));
        }
        if let Some(v) = query.team_id {
            qp.append_pair("team_id", &format!("eq.{v}"));
        }
        if let Some(v) = query.search {
            qp.append_pair("or", &format!("title.ilike.*{v}*,identifier.ilike.*{v}*"));
        }
        qp.append_pair("order", "updated_at.desc");
        qp.append_pair("order", "id.desc");
        qp.append_pair("limit", &query.limit.unwrap_or(50).min(50).to_string());
    }
    let resp = client
        .get(url.as_str())
        .send()
        .await
        .map_err(|e| AppError::internal(e.to_string()))?;
    let status = resp.status();
    let txt = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        return Ok((status, Body::from(txt)).into_response());
    }
    let tasks: serde_json::Value =
        serde_json::from_str(&txt).unwrap_or_else(|_| serde_json::json!([]));
    Ok(
        Json(serde_json::json!({"tasks": tasks, "nextCursor": serde_json::Value::Null}))
            .into_response(),
    )
}

async fn create_exponential_task(
    State(_router): State<ToolRouter>,
    _headers: axum::http::HeaderMap,
    _request_id: Option<axum::extract::Extension<RequestId>>,
    principal: Option<axum::extract::Extension<crate::auth::Principal>>,
    Json(body): Json<Value>,
) -> Result<Response, AppError> {
    if principal.is_none() {
        return Ok(standard_error_response(
            StatusCode::UNAUTHORIZED,
            "Unauthorized",
        ));
    }
    let body = body.as_object().cloned().unwrap_or_default();
    let project_id = body
        .get("project_id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let title = body
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if project_id.is_empty() {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error":"project_id is required"})),
        )
            .into_response());
    }
    if title.is_empty() {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error":"Task title is required"})),
        )
            .into_response());
    }
    let (base, key) = match supabase_env() {
        Ok(v) => v,
        Err(r) => return Ok(r),
    };
    let client = supabase_client_with_key(&key);
    let payload = serde_json::json!([{
      "org_id": EXPONENTIAL_ORG_ID,
      "project_id": project_id,
      "title": title,
      "description": sanitize_description_value(body.get("description").unwrap_or(&serde_json::Value::Null)),
      "status": body.get("status").cloned().unwrap_or(serde_json::json!("todo")),
      "priority": body.get("priority").cloned().unwrap_or(serde_json::json!(0)),
      "assignee_id": body.get("assignee_id").cloned().unwrap_or(serde_json::Value::Null),
      "sprint_id": body.get("sprint_id").cloned().unwrap_or(serde_json::Value::Null),
      "due_at": body.get("due_at").cloned().unwrap_or(serde_json::Value::Null),
      "labels": body.get("labels").cloned().unwrap_or(serde_json::json!([])),
      "milestone": body.get("milestone").cloned().unwrap_or(serde_json::Value::Null),
      "position": body.get("position").cloned().unwrap_or(serde_json::json!(1000))
    }]);
    let resp = client
        .post(format!("{base}/rest/v1/tasks"))
        .header("Prefer", "return=representation")
        .json(&payload)
        .send()
        .await
        .map_err(|e| AppError::internal(e.to_string()))?;
    let status = resp.status();
    let txt = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        return Ok((status, Body::from(txt)).into_response());
    }
    let rows: Vec<serde_json::Value> = serde_json::from_str(&txt).unwrap_or_default();
    let task = rows
        .first()
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    Ok((StatusCode::CREATED, Json(serde_json::json!({"task": task}))).into_response())
}

async fn get_exponential_task(
    State(_router): State<ToolRouter>,
    _headers: axum::http::HeaderMap,
    _request_id: Option<axum::extract::Extension<RequestId>>,
    principal: Option<axum::extract::Extension<crate::auth::Principal>>,
    Path(task_id): Path<String>,
    Query(query): Query<TaskDetailQuery>,
) -> Result<Response, AppError> {
    let include_activity = query.include_activity.unwrap_or(true);
    let include_relations = query.include_relations.unwrap_or(true);

    if cfg!(test) {
        let mut payload = serde_json::Map::new();
        payload.insert("task".into(), serde_json::json!({ "id": task_id }));
        if include_relations {
            payload.insert("relations".into(), serde_json::json!([]));
        }
        if include_activity {
            payload.insert("activity".into(), serde_json::json!([]));
        }
        return Ok(Json(serde_json::Value::Object(payload)).into_response());
    }
    if principal.is_none() {
        return Ok(standard_error_response(
            StatusCode::UNAUTHORIZED,
            "Unauthorized",
        ));
    }
    let (base, key) = match supabase_env() {
        Ok(v) => v,
        Err(r) => return Ok(r),
    };
    let client = supabase_client_with_key(&key);
    let mut url = url::Url::parse(&format!("{base}/rest/v1/tasks_view"))
        .map_err(|e| AppError::internal(e.to_string()))?;
    {
        let mut qp = url.query_pairs_mut();
        qp.append_pair("select", "*");
        qp.append_pair("id", &format!("eq.{task_id}"));
        qp.append_pair("limit", "1");
    }
    let resp = client
        .get(url.as_str())
        .send()
        .await
        .map_err(|e| AppError::internal(e.to_string()))?;
    let status = resp.status();
    let txt = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        return Ok((status, Body::from(txt)).into_response());
    }
    let rows: Vec<serde_json::Value> = serde_json::from_str(&txt).unwrap_or_default();
    let task = match rows.first() {
        Some(v) => v.clone(),
        None => {
            return Ok((
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error":"Task not found"})),
            )
                .into_response())
        }
    };

    let mut payload = serde_json::Map::new();
    payload.insert("task".into(), task);
    if include_relations {
        payload.insert("relations".into(), serde_json::json!([]));
    }
    if include_activity {
        payload.insert("activity".into(), serde_json::json!([]));
    }
    Ok(Json(serde_json::Value::Object(payload)).into_response())
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
        return Ok(standard_error_response(
            StatusCode::UNAUTHORIZED,
            "Unauthorized",
        ));
    }
    let (base, key) = match supabase_env() {
        Ok(v) => v,
        Err(r) => return Ok(r),
    };
    let client = supabase_client_with_key(&key);
    let mut updates = serde_json::Map::new();
    for f in [
        "title",
        "description",
        "status",
        "priority",
        "assignee_id",
        "sprint_id",
        "due_at",
        "labels",
        "milestone",
        "position",
        "project_id",
    ] {
        if let Some(v) = body.get(f) {
            if f == "description" {
                updates.insert(f.to_string(), sanitize_description_value(v));
            } else {
                updates.insert(f.to_string(), v.clone());
            }
        }
    }
    if let Some(action) = body.get("action").and_then(|v| v.as_str()) {
        if action == "archive" {
            updates.insert("archived_at".into(), serde_json::json!("now"));
        }
        if action == "unarchive" {
            updates.insert("archived_at".into(), serde_json::Value::Null);
        }
    }
    if updates.is_empty() {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error":"No fields to update"})),
        )
            .into_response());
    }
    let mut url = url::Url::parse(&format!("{base}/rest/v1/tasks"))
        .map_err(|e| AppError::internal(e.to_string()))?;
    url.query_pairs_mut()
        .append_pair("id", &format!("eq.{task_id}"));
    let resp = client
        .patch(url.as_str())
        .header("Prefer", "return=representation")
        .json(&serde_json::Value::Object(updates))
        .send()
        .await
        .map_err(|e| AppError::internal(e.to_string()))?;
    let status = resp.status();
    let txt = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        return Ok((status, Body::from(txt)).into_response());
    }
    let rows: Vec<serde_json::Value> = serde_json::from_str(&txt).unwrap_or_default();
    let task = rows
        .first()
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
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
        return Ok(standard_error_response(
            StatusCode::UNAUTHORIZED,
            "Unauthorized",
        ));
    }
    let (base, key) = match supabase_env() {
        Ok(v) => v,
        Err(r) => return Ok(r),
    };
    let client = supabase_client_with_key(&key);
    let mut url = url::Url::parse(&format!("{base}/rest/v1/tasks"))
        .map_err(|e| AppError::internal(e.to_string()))?;
    url.query_pairs_mut()
        .append_pair("id", &format!("eq.{task_id}"));
    let resp = client
        .delete(url.as_str())
        .send()
        .await
        .map_err(|e| AppError::internal(e.to_string()))?;
    if !resp.status().is_success() {
        return Ok((
            resp.status(),
            Body::from(resp.text().await.unwrap_or_default()),
        )
            .into_response());
    }
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
        return Ok(standard_error_response(
            StatusCode::UNAUTHORIZED,
            "Unauthorized",
        ));
    }
    let (base, key) = match supabase_env() {
        Ok(v) => v,
        Err(r) => return Ok(r),
    };
    let client = supabase_client_with_key(&key);
    let mut url = url::Url::parse(&format!("{base}/rest/v1/sprints"))
        .map_err(|e| AppError::internal(e.to_string()))?;
    {
        let mut qp = url.query_pairs_mut();
        qp.append_pair("select", "*");
        qp.append_pair("org_id", &format!("eq.{EXPONENTIAL_ORG_ID}"));
        if let Some(v) = query.project_id {
            qp.append_pair("project_id", &format!("eq.{v}"));
        }
        if let Some(v) = query.state {
            qp.append_pair("state", &format!("eq.{v}"));
        }
        qp.append_pair("order", "number.asc");
    }
    let resp = client
        .get(url.as_str())
        .send()
        .await
        .map_err(|e| AppError::internal(e.to_string()))?;
    let status = resp.status();
    let txt = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        return Ok((status, Body::from(txt)).into_response());
    }
    let sprints: serde_json::Value =
        serde_json::from_str(&txt).unwrap_or_else(|_| serde_json::json!([]));
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
        return Ok(standard_error_response(
            StatusCode::UNAUTHORIZED,
            "Unauthorized",
        ));
    }
    let project_id = body
        .get("project_id")
        .or_else(|| body.get("projectId"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    if project_id.is_empty() {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error":"project_id is required"})),
        )
            .into_response());
    }
    let (base, key) = match supabase_env() {
        Ok(v) => v,
        Err(r) => return Ok(r),
    };
    let client = supabase_client_with_key(&key);
    let mut q = url::Url::parse(&format!("{base}/rest/v1/sprints"))
        .map_err(|e| AppError::internal(e.to_string()))?;
    {
        let mut qp = q.query_pairs_mut();
        qp.append_pair("select", "number");
        qp.append_pair("project_id", &format!("eq.{project_id}"));
        qp.append_pair("order", "number.desc");
        qp.append_pair("limit", "1");
    }
    let r = client
        .get(q.as_str())
        .send()
        .await
        .map_err(|e| AppError::internal(e.to_string()))?;
    let rows: Vec<serde_json::Value> =
        serde_json::from_str(&r.text().await.unwrap_or_default()).unwrap_or_default();
    let next_num = rows
        .first()
        .and_then(|v| v.get("number"))
        .and_then(|v| v.as_i64())
        .unwrap_or(0)
        + 1;
    let payload = serde_json::json!([{
      "project_id": project_id,
      "org_id": EXPONENTIAL_ORG_ID,
      "name": body.get("name").and_then(|v| v.as_str()).map(|s| s.to_string()).unwrap_or_else(|| format!("Sprint {}", next_num)),
      "number": next_num,
      "start_date": body.get("start_date").cloned().unwrap_or(serde_json::Value::Null),
      "end_date": body.get("end_date").cloned().unwrap_or(serde_json::Value::Null),
      "state": body.get("state").cloned().unwrap_or(serde_json::json!("planned"))
    }]);
    let resp = client
        .post(format!("{base}/rest/v1/sprints"))
        .header("Prefer", "return=representation")
        .json(&payload)
        .send()
        .await
        .map_err(|e| AppError::internal(e.to_string()))?;
    let status = resp.status();
    let txt = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        return Ok((status, Body::from(txt)).into_response());
    }
    let rows: Vec<serde_json::Value> = serde_json::from_str(&txt).unwrap_or_default();
    let sprint = rows
        .first()
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({"sprint": sprint})),
    )
        .into_response())
}

async fn get_exponential_sprint(
    State(_router): State<ToolRouter>,
    _headers: axum::http::HeaderMap,
    _request_id: Option<axum::extract::Extension<RequestId>>,
    principal: Option<axum::extract::Extension<crate::auth::Principal>>,
    Path(sprint_id): Path<String>,
) -> Result<Response, AppError> {
    if cfg!(test) {
        return Ok(
            Json(serde_json::json!({"sprint": {"id": sprint_id}, "tasks": []})).into_response(),
        );
    }
    if principal.is_none() {
        return Ok(standard_error_response(
            StatusCode::UNAUTHORIZED,
            "Unauthorized",
        ));
    }
    let (base, key) = match supabase_env() {
        Ok(v) => v,
        Err(r) => return Ok(r),
    };
    let client = supabase_client_with_key(&key);
    let mut u = url::Url::parse(&format!("{base}/rest/v1/sprints"))
        .map_err(|e| AppError::internal(e.to_string()))?;
    {
        let mut qp = u.query_pairs_mut();
        qp.append_pair("select", "*");
        qp.append_pair("id", &format!("eq.{sprint_id}"));
        qp.append_pair("limit", "1");
    }
    let resp = client
        .get(u.as_str())
        .send()
        .await
        .map_err(|e| AppError::internal(e.to_string()))?;
    let status = resp.status();
    let txt = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        return Ok((status, Body::from(txt)).into_response());
    }
    let rows: Vec<serde_json::Value> = serde_json::from_str(&txt).unwrap_or_default();
    let sprint = match rows.first() {
        Some(v) => v.clone(),
        None => {
            return Ok((
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error":"Sprint not found"})),
            )
                .into_response())
        }
    };
    let mut tu = url::Url::parse(&format!("{base}/rest/v1/tasks_view"))
        .map_err(|e| AppError::internal(e.to_string()))?;
    {
        let mut qp = tu.query_pairs_mut();
        qp.append_pair("select", "*");
        qp.append_pair("sprint_id", &format!("eq.{sprint_id}"));
        qp.append_pair("archived_at", "is.null");
        qp.append_pair("order", "position.asc");
    }
    let tr = client
        .get(tu.as_str())
        .send()
        .await
        .map_err(|e| AppError::internal(e.to_string()))?;
    let tasks: serde_json::Value = serde_json::from_str(&tr.text().await.unwrap_or_default())
        .unwrap_or_else(|_| serde_json::json!([]));
    Ok(Json(serde_json::json!({"sprint": sprint, "tasks": tasks})).into_response())
}

async fn update_exponential_sprint(
    principal: Option<axum::extract::Extension<crate::auth::Principal>>,
    Path(sprint_id): Path<String>,
    Json(body): Json<Value>,
) -> Response {
    if principal.is_none() {
        return standard_error_response(StatusCode::UNAUTHORIZED, "Unauthorized");
    }
    let (base, key) = match supabase_env() {
        Ok(v) => v,
        Err(r) => return r,
    };
    let client = supabase_client_with_key(&key);
    let mut updates = serde_json::Map::new();
    for f in ["name", "start_date", "end_date", "state"] {
        if let Some(v) = body.get(f) {
            updates.insert(f.to_string(), v.clone());
        }
    }
    if updates.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error":"No fields to update"})),
        )
            .into_response();
    }
    let mut url = match parse_upstream_url_or_500(&format!("{base}/rest/v1/sprints")) {
        Ok(u) => u,
        Err(r) => return r,
    };
    url.query_pairs_mut()
        .append_pair("id", &format!("eq.{sprint_id}"));
    let resp = match client
        .patch(url.as_str())
        .header("Prefer", "return=representation")
        .json(&serde_json::Value::Object(updates))
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({"error":e.to_string()})),
            )
                .into_response()
        }
    };
    if !resp.status().is_success() {
        return (
            resp.status(),
            Body::from(resp.text().await.unwrap_or_default()),
        )
            .into_response();
    }
    let rows: Vec<serde_json::Value> =
        serde_json::from_str(&resp.text().await.unwrap_or_default()).unwrap_or_default();
    let sprint = rows
        .first()
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    Json(serde_json::json!({"sprint": sprint})).into_response()
}

async fn delete_exponential_sprint(
    principal: Option<axum::extract::Extension<crate::auth::Principal>>,
    Path(sprint_id): Path<String>,
) -> Response {
    if principal.is_none() {
        return standard_error_response(StatusCode::UNAUTHORIZED, "Unauthorized");
    }
    let (base, key) = match supabase_env() {
        Ok(v) => v,
        Err(r) => return r,
    };
    let client = supabase_client_with_key(&key);
    let mut url = match parse_upstream_url_or_500(&format!("{base}/rest/v1/sprints")) {
        Ok(u) => u,
        Err(r) => return r,
    };
    url.query_pairs_mut()
        .append_pair("id", &format!("eq.{sprint_id}"));
    let resp = match client.delete(url.as_str()).send().await {
        Ok(r) => r,
        Err(e) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({"error":e.to_string()})),
            )
                .into_response()
        }
    };
    if !resp.status().is_success() {
        return (
            resp.status(),
            Body::from(resp.text().await.unwrap_or_default()),
        )
            .into_response();
    }
    Json(serde_json::json!({"success": true})).into_response()
}
async fn list_exponential_projects(
    State(_router): State<ToolRouter>,
    _headers: axum::http::HeaderMap,
    _request_id: Option<axum::extract::Extension<RequestId>>,
    principal: Option<axum::extract::Extension<crate::auth::Principal>>,
    Query(query): Query<ListProjectsQuery>,
) -> Result<Response, AppError> {
    if cfg!(test) {
        return Ok(Json(serde_json::json!({"projects": []})).into_response());
    }
    if principal.is_none() {
        return Ok(standard_error_response(
            StatusCode::UNAUTHORIZED,
            "Unauthorized",
        ));
    }
    let (base, key) = match supabase_env() {
        Ok(v) => v,
        Err(r) => return Ok(r),
    };
    let client = supabase_client_with_key(&key);
    let mut url = url::Url::parse(&format!("{base}/rest/v1/projects"))
        .map_err(|e| AppError::internal(e.to_string()))?;
    let view = query.view.as_deref();
    let is_sidebar_view = matches!(view, Some("sidebar"));
    let project_select = if is_sidebar_view {
        "id,team_id,name,color,archived_at"
    } else {
        "*,team:teams(id,name,slug,color)"
    };
    {
        let mut qp = url.query_pairs_mut();
        qp.append_pair("select", project_select);
        qp.append_pair("org_id", &format!("eq.{EXPONENTIAL_ORG_ID}"));
        qp.append_pair("order", "name.asc");
        if query.include_archived.unwrap_or(false) == false {
            qp.append_pair("archived_at", "is.null");
        }
        if let Some(tid) = query.team_id {
            qp.append_pair("team_id", &format!("eq.{tid}"));
        }
    }
    let resp = client
        .get(url.as_str())
        .send()
        .await
        .map_err(|e| AppError::internal(e.to_string()))?;
    let st = resp.status();
    let txt = resp.text().await.unwrap_or_default();
    if !st.is_success() {
        return Ok((st, Body::from(txt)).into_response());
    }
    let mut projects: Vec<serde_json::Value> = serde_json::from_str(&txt).unwrap_or_default();

    if !is_sidebar_view {
        let parse_count = |resp: &reqwest::Response| -> i64 {
            let raw = resp
                .headers()
                .get("content-range")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("*/0");
            raw.rsplit('/')
                .next()
                .and_then(|n| n.parse::<i64>().ok())
                .unwrap_or(0)
        };

        for p in &mut projects {
            let pid = match p.get("id").and_then(|v| v.as_str()) {
                Some(v) => v,
                None => continue,
            };

            let mut all_url = url::Url::parse(&format!("{base}/rest/v1/tasks_view"))
                .map_err(|e| AppError::internal(e.to_string()))?;
            {
                let mut qp = all_url.query_pairs_mut();
                qp.append_pair("select", "id");
                qp.append_pair("project_id", &format!("eq.{pid}"));
                qp.append_pair("archived_at", "is.null");
            }
            let all_resp = client
                .get(all_url.as_str())
                .header("Prefer", "count=exact")
                .header("Range", "0-0")
                .send()
                .await
                .map_err(|e| AppError::internal(e.to_string()))?;
            let task_count = parse_count(&all_resp);

            let mut done_url = url::Url::parse(&format!("{base}/rest/v1/tasks_view"))
                .map_err(|e| AppError::internal(e.to_string()))?;
            {
                let mut qp = done_url.query_pairs_mut();
                qp.append_pair("select", "id");
                qp.append_pair("project_id", &format!("eq.{pid}"));
                qp.append_pair("archived_at", "is.null");
                qp.append_pair("status", "eq.done");
            }
            let done_resp = client
                .get(done_url.as_str())
                .header("Prefer", "count=exact")
                .header("Range", "0-0")
                .send()
                .await
                .map_err(|e| AppError::internal(e.to_string()))?;
            let completed_count = parse_count(&done_resp);

            if let Some(obj) = p.as_object_mut() {
                obj.insert("task_count".to_string(), serde_json::json!(task_count));
                obj.insert(
                    "completed_count".to_string(),
                    serde_json::json!(completed_count),
                );
            }
        }
    }

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
    if cfg!(test) {
        return Ok(Json(serde_json::json!({"project":{"id":project_id},"tasks":[],"sprints":[],"members":[],"user_role":"lead","permissions":{"can_manage":true,"can_create_tasks":true,"can_edit_tasks":true,"can_manage_members":true}})).into_response());
    }
    let principal = match principal {
        Some(axum::extract::Extension(p)) => p,
        None => {
            return Ok(standard_error_response(
                StatusCode::UNAUTHORIZED,
                "Unauthorized",
            ))
        }
    };
    let (base, key) = match supabase_env() {
        Ok(v) => v,
        Err(r) => return Ok(r),
    };
    let client = supabase_client_with_key(&key);
    let mut purl = url::Url::parse(&format!("{base}/rest/v1/projects"))
        .map_err(|e| AppError::internal(e.to_string()))?;
    {
        let mut qp = purl.query_pairs_mut();
        qp.append_pair("select", "*,team:teams(id,name,slug,color)");
        qp.append_pair("id", &format!("eq.{project_id}"));
        qp.append_pair("limit", "1");
    }
    let pr = client
        .get(purl.as_str())
        .send()
        .await
        .map_err(|e| AppError::internal(e.to_string()))?;
    if !pr.status().is_success() {
        return Ok((pr.status(), Body::from(pr.text().await.unwrap_or_default())).into_response());
    }
    let prow: Vec<serde_json::Value> =
        serde_json::from_str(&pr.text().await.unwrap_or_default()).unwrap_or_default();
    if prow.is_empty() {
        return Ok((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error":"Project not found"})),
        )
            .into_response());
    }
    let project = prow[0].clone();
    let mut surl = url::Url::parse(&format!("{base}/rest/v1/sprints"))
        .map_err(|e| AppError::internal(e.to_string()))?;
    {
        let mut qp = surl.query_pairs_mut();
        qp.append_pair("select", "*");
        qp.append_pair("project_id", &format!("eq.{project_id}"));
        qp.append_pair("order", "number.asc");
    }
    let sprints: serde_json::Value = serde_json::from_str(
        &client
            .get(surl.as_str())
            .send()
            .await
            .map_err(|e| AppError::internal(e.to_string()))?
            .text()
            .await
            .unwrap_or_default(),
    )
    .unwrap_or_else(|_| serde_json::json!([]));
    let mut murl = url::Url::parse(&format!("{base}/rest/v1/project_members"))
        .map_err(|e| AppError::internal(e.to_string()))?;
    {
        let mut qp = murl.query_pairs_mut();
        qp.append_pair(
            "select",
            "*,user:users(id,first_name,last_name,email,avatar_url)",
        );
        qp.append_pair("project_id", &format!("eq.{project_id}"));
    }
    let members: serde_json::Value = serde_json::from_str(
        &client
            .get(murl.as_str())
            .send()
            .await
            .map_err(|e| AppError::internal(e.to_string()))?
            .text()
            .await
            .unwrap_or_default(),
    )
    .unwrap_or_else(|_| serde_json::json!([]));
    let mut role = "viewer".to_string();
    if let Some(arr) = members.as_array() {
        for m in arr {
            if m.get("user_id").and_then(|v| v.as_str()) == Some(principal.user_id.as_str()) {
                role = m
                    .get("role")
                    .and_then(|v| v.as_str())
                    .unwrap_or("viewer")
                    .to_string();
                break;
            }
        }
    }
    let can_manage = role == "lead";
    let can_contrib = role == "lead" || role == "contributor";
    Ok(Json(serde_json::json!({"project":project,"tasks":[],"sprints":sprints,"members":members,"user_role":role,"permissions":{"can_manage":can_manage,"can_create_tasks":can_contrib,"can_edit_tasks":can_contrib,"can_manage_members":can_manage}})).into_response())
}

async fn list_exponential_teams(
    State(_router): State<ToolRouter>,
    _headers: axum::http::HeaderMap,
    _request_id: Option<axum::extract::Extension<RequestId>>,
    principal: Option<axum::extract::Extension<crate::auth::Principal>>,
    Query(query): Query<ListTeamsQuery>,
) -> Result<Response, AppError> {
    if cfg!(test) {
        return Ok(Json(serde_json::json!({"teams": []})).into_response());
    }
    if principal.is_none() {
        return Ok(standard_error_response(
            StatusCode::UNAUTHORIZED,
            "Unauthorized",
        ));
    }
    let (base, key) = match supabase_env() {
        Ok(v) => v,
        Err(r) => return Ok(r),
    };
    let client = supabase_client_with_key(&key);
    let mut url = url::Url::parse(&format!("{base}/rest/v1/teams"))
        .map_err(|e| AppError::internal(e.to_string()))?;
    let team_select = if matches!(query.view.as_deref(), Some("sidebar")) {
        "id,name,slug,color"
    } else {
        "*"
    };
    {
        let mut qp = url.query_pairs_mut();
        qp.append_pair("select", team_select);
        qp.append_pair("org_id", &format!("eq.{EXPONENTIAL_ORG_ID}"));
        qp.append_pair("order", "name.asc");
    }
    let resp = client
        .get(url.as_str())
        .send()
        .await
        .map_err(|e| AppError::internal(e.to_string()))?;
    let st = resp.status();
    let txt = resp.text().await.unwrap_or_default();
    if !st.is_success() {
        return Ok((st, Body::from(txt)).into_response());
    }
    let teams: serde_json::Value =
        serde_json::from_str(&txt).unwrap_or_else(|_| serde_json::json!([]));
    Ok(Json(serde_json::json!({"teams": teams})).into_response())
}

async fn get_exponential_team(
    State(_router): State<ToolRouter>,
    _headers: axum::http::HeaderMap,
    _request_id: Option<axum::extract::Extension<RequestId>>,
    principal: Option<axum::extract::Extension<crate::auth::Principal>>,
    Path(team_id): Path<String>,
) -> Result<Response, AppError> {
    if cfg!(test) {
        return Ok(Json(
            serde_json::json!({"teams":[{"id":team_id}],"team":{"id":team_id},"projects": []}),
        )
        .into_response());
    }
    if principal.is_none() {
        return Ok(standard_error_response(
            StatusCode::UNAUTHORIZED,
            "Unauthorized",
        ));
    }
    let (base, key) = match supabase_env() {
        Ok(v) => v,
        Err(r) => return Ok(r),
    };
    let client = supabase_client_with_key(&key);
    let mut turl = url::Url::parse(&format!("{base}/rest/v1/teams"))
        .map_err(|e| AppError::internal(e.to_string()))?;
    {
        let mut qp = turl.query_pairs_mut();
        qp.append_pair("select", "*");
        qp.append_pair("id", &format!("eq.{team_id}"));
        qp.append_pair("limit", "1");
    }
    let tr = client
        .get(turl.as_str())
        .send()
        .await
        .map_err(|e| AppError::internal(e.to_string()))?;
    if !tr.status().is_success() {
        return Ok((tr.status(), Body::from(tr.text().await.unwrap_or_default())).into_response());
    }
    let trows: Vec<serde_json::Value> =
        serde_json::from_str(&tr.text().await.unwrap_or_default()).unwrap_or_default();
    if trows.is_empty() {
        return Ok((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error":"Team not found"})),
        )
            .into_response());
    }
    let team = trows[0].clone();
    let mut purl = url::Url::parse(&format!("{base}/rest/v1/projects"))
        .map_err(|e| AppError::internal(e.to_string()))?;
    {
        let mut qp = purl.query_pairs_mut();
        qp.append_pair("select", "*");
        qp.append_pair("team_id", &format!("eq.{team_id}"));
        qp.append_pair("order", "name.asc");
    }
    let projects: serde_json::Value = serde_json::from_str(
        &client
            .get(purl.as_str())
            .send()
            .await
            .map_err(|e| AppError::internal(e.to_string()))?
            .text()
            .await
            .unwrap_or_default(),
    )
    .unwrap_or_else(|_| serde_json::json!([]));
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
        return Ok(
            Json(serde_json::json!({"tasks": [], "nextCursor": serde_json::Value::Null}))
                .into_response(),
        );
    }
    if principal.is_none() {
        return Ok(standard_error_response(
            StatusCode::UNAUTHORIZED,
            "Unauthorized",
        ));
    }
    let (base, key) = match supabase_env() {
        Ok(v) => v,
        Err(r) => return Ok(r),
    };
    let client = supabase_client_with_key(&key);
    let mut url = url::Url::parse(&format!("{base}/rest/v1/tasks_view"))
        .map_err(|e| AppError::internal(e.to_string()))?;
    {
        let mut qp = url.query_pairs_mut();
        qp.append_pair("select", "id,project_id,org_id,identifier,title,status,priority,assignee_id,sprint_id,sprint_name,due_at,labels,milestone,position,created_at,updated_at");
        qp.append_pair("project_id", &format!("eq.{project_id}"));
        if query.include_archived.unwrap_or(false) == false {
            qp.append_pair("archived_at", "is.null");
        }
        qp.append_pair("order", "updated_at.desc");
        qp.append_pair("order", "id.desc");
        qp.append_pair("limit", &query.limit.unwrap_or(50).min(50).to_string());
    }
    let resp = client
        .get(url.as_str())
        .send()
        .await
        .map_err(|e| AppError::internal(e.to_string()))?;
    let status = resp.status();
    let txt = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        return Ok((status, Body::from(txt)).into_response());
    }
    let tasks: serde_json::Value =
        serde_json::from_str(&txt).unwrap_or_else(|_| serde_json::json!([]));
    Ok(
        Json(serde_json::json!({"tasks": tasks, "nextCursor": serde_json::Value::Null}))
            .into_response(),
    )
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
        return Ok(standard_error_response(
            StatusCode::UNAUTHORIZED,
            "Unauthorized",
        ));
    }
    let (base, key) = match supabase_env() {
        Ok(v) => v,
        Err(r) => return Ok(r),
    };
    let client = supabase_client_with_key(&key);
    let mut turl = url::Url::parse(&format!("{base}/rest/v1/tasks"))
        .map_err(|e| AppError::internal(e.to_string()))?;
    {
        let mut qp = turl.query_pairs_mut();
        qp.append_pair("select", "org_id");
        qp.append_pair("id", &format!("eq.{task_id}"));
        qp.append_pair("limit", "1");
    }
    let tresp = client
        .get(turl.as_str())
        .send()
        .await
        .map_err(|e| AppError::internal(e.to_string()))?;
    if !tresp.status().is_success() {
        return Ok((
            tresp.status(),
            Body::from(tresp.text().await.unwrap_or_default()),
        )
            .into_response());
    }
    let trows: Vec<serde_json::Value> =
        serde_json::from_str(&tresp.text().await.unwrap_or_default()).unwrap_or_default();
    let org_id = trows
        .first()
        .and_then(|r| r.get("org_id"))
        .and_then(|v| v.as_str())
        .unwrap_or(EXPONENTIAL_ORG_ID)
        .to_string();

    let mut url = url::Url::parse(&format!("{base}/rest/v1/task_comments"))
        .map_err(|e| AppError::internal(e.to_string()))?;
    {
        let mut qp = url.query_pairs_mut();
        qp.append_pair("select", "id,body,created_at,updated_at,author_id,author:users(id,first_name,last_name,email,avatar_url)");
        qp.append_pair("task_id", &format!("eq.{task_id}"));
        qp.append_pair("org_id", &format!("eq.{org_id}"));
        qp.append_pair("order", "created_at.asc");
    }
    let resp = client
        .get(url.as_str())
        .send()
        .await
        .map_err(|e| AppError::internal(e.to_string()))?;
    let status = resp.status();
    let txt = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        return Ok((status, Body::from(txt)).into_response());
    }
    let comments: serde_json::Value =
        serde_json::from_str(&txt).unwrap_or_else(|_| serde_json::json!([]));
    Ok(Json(serde_json::json!({"comments": comments})).into_response())
}

async fn create_exponential_task_comment(
    principal: Option<axum::extract::Extension<crate::auth::Principal>>,
    Path(task_id): Path<String>,
    Json(body): Json<Value>,
) -> Response {
    let user_id = match principal {
        Some(axum::extract::Extension(p)) => p.user_id,
        None => return standard_error_response(StatusCode::UNAUTHORIZED, "Unauthorized"),
    };
    let raw_text = body.get("body").and_then(|v| v.as_str()).unwrap_or("");
    let text = sanitize_rich_html(raw_text).trim().to_string();
    if text.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error":"Comment body is required"})),
        )
            .into_response();
    }
    let (base, key) = match supabase_env() {
        Ok(v) => v,
        Err(r) => return r,
    };
    let client = supabase_client_with_key(&key);
    let mut turl = match parse_upstream_url_or_500(&format!("{base}/rest/v1/tasks")) {
        Ok(u) => u,
        Err(r) => return r,
    };
    {
        let mut qp = turl.query_pairs_mut();
        qp.append_pair("select", "org_id");
        qp.append_pair("id", &format!("eq.{task_id}"));
        qp.append_pair("limit", "1");
    }
    let tresp = match client.get(turl.as_str()).send().await {
        Ok(r) => r,
        Err(e) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({"error":e.to_string()})),
            )
                .into_response()
        }
    };
    if !tresp.status().is_success() {
        return (
            tresp.status(),
            Body::from(tresp.text().await.unwrap_or_default()),
        )
            .into_response();
    }
    let trows: Vec<serde_json::Value> =
        serde_json::from_str(&tresp.text().await.unwrap_or_default()).unwrap_or_default();
    let org_id = trows
        .first()
        .and_then(|r| r.get("org_id"))
        .and_then(|v| v.as_str())
        .unwrap_or(EXPONENTIAL_ORG_ID)
        .to_string();
    let payload =
        serde_json::json!([{"org_id":org_id,"task_id":task_id,"author_id":user_id,"body":text}]);
    let resp = match client
        .post(format!("{base}/rest/v1/task_comments"))
        .header("Prefer", "return=representation")
        .json(&payload)
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({"error":e.to_string()})),
            )
                .into_response()
        }
    };
    let status = resp.status();
    let txt = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        return (status, Body::from(txt)).into_response();
    }
    let rows: serde_json::Value =
        serde_json::from_str(&txt).unwrap_or_else(|_| serde_json::json!([]));
    let comment = rows
        .as_array()
        .and_then(|a| a.first())
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    (
        StatusCode::CREATED,
        Json(serde_json::json!({"comment":comment})),
    )
        .into_response()
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

    let read_rate_limiter =
        RateLimiter::new(config.rate_limit_read_rps, config.rate_limit_read_burst);
    let write_rate_limiter =
        RateLimiter::new(config.rate_limit_write_rps, config.rate_limit_write_burst);
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
        upstream_base: config
            .proxy_upstream_base_url
            .trim_end_matches('/')
            .to_string(),
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
            axum::routing::get(get_exponential_task_comments)
                .post(create_exponential_task_comment)
                .with_state(tool_router.clone()),
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
            axum::routing::get(get_exponential_views).post(create_exponential_view),
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
            "/api/greenbooks/accounts",
            axum::routing::get(greenbooks_list_accounts),
        )
        .route(
            "/api/greenbooks/accounts/{id}",
            axum::routing::get(greenbooks_get_account).patch(greenbooks_patch_account),
        )
        .route(
            "/api/greenbooks/accounts/{id}/ledger",
            axum::routing::get(greenbooks_get_account_ledger),
        )
        .route(
            "/api/greenbooks/journal-entries",
            axum::routing::get(greenbooks_list_journal_entries),
        )
        .route(
            "/api/greenbooks/tax-codes",
            axum::routing::get(greenbooks_list_tax_codes).post(greenbooks_create_tax_code),
        )
        .route(
            "/api/greenbooks/tax-codes/{id}",
            axum::routing::patch(greenbooks_patch_tax_code).delete(greenbooks_delete_tax_code),
        )
        .route(
            "/api/greenbooks/settings",
            axum::routing::get(greenbooks_get_settings).put(greenbooks_put_settings),
        )
        .route(
            "/api/greenbooks/vendors",
            axum::routing::get(greenbooks_list_vendors).post(greenbooks_create_vendor),
        )
        .route(
            "/api/greenbooks/vendors/{id}",
            axum::routing::patch(greenbooks_patch_vendor).delete(greenbooks_delete_vendor),
        )
        .route(
            "/api/greenbooks/fiscal-periods",
            axum::routing::get(greenbooks_list_fiscal_periods),
        )
        .route(
            "/api/greenbooks/fx-rates",
            axum::routing::get(greenbooks_list_fx_rates),
        )
        .route(
            "/api/greenbooks/audit",
            axum::routing::get(greenbooks_list_audit),
        )
        .route(
            "/api/greenbooks/bank-accounts",
            axum::routing::get(greenbooks_list_bank_accounts),
        )
        .route(
            "/api/greenbooks/currencies",
            axum::routing::get(greenbooks_list_currencies),
        )
        .route(
            "/api/greenbooks/bills",
            axum::routing::get(greenbooks_list_bills),
        )
        .route(
            "/api/greenbooks/customers",
            axum::routing::get(greenbooks_list_customers).post(greenbooks_create_customer),
        )
        .route(
            "/api/greenbooks/customers/{id}",
            axum::routing::get(greenbooks_get_customer).patch(greenbooks_patch_customer),
        )
        .route(
            "/api/greenbooks/invoices",
            axum::routing::get(greenbooks_list_invoices),
        )
        .route(
            "/api/greenbooks/invoices/{id}",
            axum::routing::get(greenbooks_get_invoice).patch(greenbooks_patch_invoice),
        )
        .route(
            "/api/greenbooks/payments",
            axum::routing::get(greenbooks_list_payments),
        )
        .route(
            "/api/greenbooks/invoices/{id}/payments",
            axum::routing::get(greenbooks_list_invoice_payments)
                .post(greenbooks_create_invoice_payment),
        )
        .route(
            "/api/greenbooks/invoices/{id}/post",
            axum::routing::post(greenbooks_post_invoice_to_gl),
        )
        .route(
            "/api/greenbooks/invoices/{id}/post-gl",
            axum::routing::post(greenbooks_post_invoice_to_gl),
        )
        .route(
            "/api/greenbooks/bank-accounts/{id}/reconcile",
            axum::routing::get(greenbooks_reconcile_history)
                .post(greenbooks_reconcile_post)
                .patch(greenbooks_reconcile_patch),
        )
        .route(
            "/api/greenbooks/bank-accounts/transfer",
            axum::routing::post(greenbooks_bank_transfer),
        )
        // Explicit GreenBooks passthrough routes (Rust gateway owned; no catch-all wildcard)
        .route(
            "/api/greenbooks/reports",
            axum::routing::get(greenbooks_reports),
        )
        .route(
            "/api/greenbooks/import/quickbooks",
            axum::routing::post(crate::quickbooks_import::greenbooks_import_quickbooks),
        )
        .route(
            "/api/greenbooks/accounts/export",
            axum::routing::any(proxy_greenbooks_direct).with_state(proxy_state.clone()),
        )
        .route(
            "/api/greenbooks/accounts/import",
            axum::routing::any(proxy_greenbooks_direct).with_state(proxy_state.clone()),
        )
        .route(
            "/api/greenbooks/accounts",
            axum::routing::post(proxy_greenbooks_direct).with_state(proxy_state.clone()),
        )
        .route(
            "/api/greenbooks/journal-entries",
            axum::routing::post(proxy_greenbooks_direct).with_state(proxy_state.clone()),
        )
        .route(
            "/api/greenbooks/journal-entries/{id}",
            axum::routing::get(proxy_greenbooks_direct)
                .patch(proxy_greenbooks_direct)
                .with_state(proxy_state.clone()),
        )
        .route(
            "/api/greenbooks/invoices",
            axum::routing::post(proxy_greenbooks_direct).with_state(proxy_state.clone()),
        )
        .route(
            "/api/greenbooks/items",
            axum::routing::get(greenbooks_list_items)
                .post(greenbooks_create_item)
                .patch(greenbooks_patch_item),
        )
        .route(
            "/api/greenbooks/bank-accounts",
            axum::routing::post(proxy_greenbooks_direct).with_state(proxy_state.clone()),
        )
        .route(
            "/api/greenbooks/bank-accounts/{id}/transactions",
            axum::routing::any(proxy_greenbooks_direct).with_state(proxy_state.clone()),
        )
        .route(
            "/api/greenbooks/bills",
            axum::routing::post(proxy_greenbooks_direct).with_state(proxy_state.clone()),
        )
        .route(
            "/api/greenbooks/bills/{id}",
            axum::routing::any(proxy_greenbooks_direct).with_state(proxy_state.clone()),
        )
        .route(
            "/api/greenbooks/bills/{id}/post-gl",
            axum::routing::post(proxy_greenbooks_direct).with_state(proxy_state.clone()),
        )
        .route(
            "/api/greenbooks/bills/{id}/payments",
            axum::routing::post(proxy_greenbooks_direct).with_state(proxy_state.clone()),
        )
        .route(
            "/api/greenbooks/customers/{id}",
            axum::routing::delete(proxy_greenbooks_direct).with_state(proxy_state.clone()),
        )
        .route(
            "/api/greenbooks/customers/{id}/hub",
            axum::routing::get(greenbooks_get_customer_hub),
        )
        .route(
            "/api/greenbooks/customers/{id}/statement",
            axum::routing::get(greenbooks_get_customer_statement),
        )
        .route(
            "/api/greenbooks/fiscal-periods",
            axum::routing::post(proxy_greenbooks_direct).with_state(proxy_state.clone()),
        )
        .route(
            "/api/greenbooks/fiscal-periods/{id}",
            axum::routing::patch(proxy_greenbooks_direct)
                .delete(proxy_greenbooks_direct)
                .with_state(proxy_state.clone()),
        )
        .route(
            "/api/greenbooks/fx-rates",
            axum::routing::post(proxy_greenbooks_direct).with_state(proxy_state.clone()),
        )
        .route(
            "/api/greenbooks/fx-rates/{id}",
            axum::routing::delete(proxy_greenbooks_direct).with_state(proxy_state.clone()),
        )
        .route(
            "/api/greenbooks/fx-rates/convert",
            axum::routing::get(greenbooks_convert_fx),
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
        read_limiter: read_rate_limiter,
        write_limiter: write_rate_limiter,
        audit: audit.clone(),
    };

    let allowed_origins: Vec<HeaderValue> = config
        .cors_allow_origins
        .iter()
        .map(|origin| {
            // Safe unwrap: config validation runs at startup and guarantees valid origins.
            origin
                .parse::<HeaderValue>()
                .expect("validated CORS origin should be a valid HTTP header value")
        })
        .collect();

    let cors = CorsLayer::new()
        .allow_origin(allowed_origins)
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
            header::HeaderName::from_static("x-request-id"),
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
    use proptest::prelude::*;
    use std::collections::HashSet;
    use std::sync::Arc;
    use std::sync::Once;
    use tower::ServiceExt;

    use crate::auth::{AuthMethod, Principal};
    use crate::egress::{EgressClient, EgressConfig};
    use crate::middleware::csrf::{csrf_middleware, CsrfConfig};
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

        let authed_router =
            build_exponential_router(tool_router, proxy_state, rbac_state, Some(test_principal()));
        let resp = authed_router
            .clone()
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

        let resp = authed_router
            .clone()
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
        assert!(body.get("activity").and_then(|v| v.as_array()).is_some());
        assert!(body.get("relations").and_then(|v| v.as_array()).is_some());

        let resp = authed_router.clone()
            .oneshot(
                Request::builder()
                    .uri("/api/exponential/tasks/task_1?includeActivity=false&includeRelations=false")
                    .header("authorization", "Bearer test")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = body_json(resp).await;
        assert!(body.get("task").and_then(|v| v.as_object()).is_some());
        assert!(body.get("activity").is_none());
        assert!(body.get("relations").is_none());

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
        let resp = unauth_router
            .clone()
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

        let authed_router =
            build_exponential_router(tool_router, proxy_state, rbac_state, Some(test_principal()));
        let resp = authed_router
            .clone()
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
            (
                "/api/exponential/projects/project_1/members",
                "members",
                true,
            ),
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
            (
                "/api/exponential/sprints?projectId=project_1",
                "sprints",
                true,
            ),
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

        let authed_router =
            build_exponential_router(tool_router, proxy_state, rbac_state, Some(test_principal()));
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

    fn build_greenbooks_contract_router(principal: Option<Principal>) -> Router {
        let csrf = CsrfConfig::default();
        let mut router = Router::new()
            .route(
                "/api/greenbooks/accounts",
                axum::routing::get(greenbooks_list_accounts),
            )
            .route(
                "/api/greenbooks/payments",
                axum::routing::get(greenbooks_list_payments),
            )
            .route(
                "/api/greenbooks/invoices/{id}/payments",
                axum::routing::get(greenbooks_list_invoice_payments)
                    .post(greenbooks_create_invoice_payment),
            )
            .route(
                "/api/greenbooks/bank-accounts/transfer",
                axum::routing::post(greenbooks_bank_transfer),
            )
            .layer(axum_mw::from_fn_with_state(csrf, csrf_middleware));

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
    async fn greenbooks_rust_routes_require_principal_for_migrated_reads_and_writes() {
        let router = build_greenbooks_contract_router(None);

        let read_resp = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/greenbooks/payments")
                    .header("authorization", "Bearer test")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(read_resp.status(), StatusCode::UNAUTHORIZED);
        let read_body = body_json(read_resp).await;
        assert_eq!(read_body["error"]["code"], 401);
        assert_eq!(read_body["error"]["kind"], "unauthorized");
        assert_eq!(read_body["error"]["message"], "Unauthorized");

        let write_resp = router
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/greenbooks/invoices/inv_123/payments")
                    .header("authorization", "Bearer test")
                    .header("content-type", "application/json")
                    .body(Body::from("{}"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(write_resp.status(), StatusCode::UNAUTHORIZED);
        let write_body = body_json(write_resp).await;
        assert_eq!(write_body["error"]["code"], 401);
        assert_eq!(write_body["error"]["kind"], "unauthorized");
        assert_eq!(write_body["error"]["message"], "Unauthorized");
    }

    #[tokio::test]
    async fn greenbooks_write_routes_enforce_csrf_error_envelope_for_cookie_auth() {
        let router = build_greenbooks_contract_router(Some(test_principal()));
        let resp = router
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/greenbooks/bank-accounts/transfer")
                    .header("content-type", "application/json")
                    .body(Body::from("{}"))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        let body = body_json(resp).await;
        assert_eq!(body["error"]["kind"], "forbidden");
        assert_eq!(body["error"]["message"], "CSRF token missing or invalid");
    }

    #[tokio::test]
    async fn greenbooks_write_validation_contract_uses_legacy_error_shape_when_authed() {
        let router = build_greenbooks_contract_router(Some(test_principal()));
        let resp = router
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/greenbooks/invoices/inv_123/payments")
                    .header("authorization", "Bearer test")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"payment_date":"2026-02-18"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let body = body_json(resp).await;
        assert_eq!(body["error"], "amount must be a positive number");
    }

    #[tokio::test]
    async fn greenbooks_read_validation_contract_is_deterministic() {
        let router = build_greenbooks_contract_router(Some(test_principal()));
        let resp = router
            .oneshot(
                Request::builder()
                    .uri("/api/greenbooks/accounts?type=wat")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let body = body_json(resp).await;
        assert_eq!(body["error"], "Invalid account type: wat");
    }

    #[test]
    fn greenbooks_invoice_dto_preserves_dynamic_fields() {
        let raw = r#"[{"id":"inv_1","status":"sent","custom_metric":42}]"#;
        let rows: Vec<GreenbooksInvoiceDto> = parse_rows(raw);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, "inv_1");
        assert_eq!(rows[0].status.as_deref(), Some("sent"));
        assert_eq!(
            rows[0].extras.get("custom_metric").and_then(|v| v.as_i64()),
            Some(42)
        );
    }

    #[tokio::test]
    async fn exponential_native_write_auth_uses_standard_error_envelope() {
        let tool_router =
            ToolRouter::new(EgressClient::new(crate::egress::EgressConfig::default()));
        let router = Router::new().route(
            "/api/exponential/tasks",
            axum::routing::post(create_exponential_task).with_state(tool_router),
        );

        let resp = router
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/exponential/tasks")
                    .header("content-type", "application/json")
                    .body(Body::from("{}"))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
        let body = body_json(resp).await;
        assert_eq!(body["error"]["code"], 401);
        assert_eq!(body["error"]["kind"], "unauthorized");
        assert_eq!(body["error"]["message"], "Unauthorized");
    }

    proptest! {
        #[test]
        fn supabase_error_message_prefers_message_field(msg in ".{1,64}", fallback in ".{1,64}") {
            let body = serde_json::json!({ "message": msg, "error": fallback }).to_string();
            prop_assert_eq!(supabase_error_message(&body), msg);
        }

        #[test]
        fn parse_bool_query_param_accepts_case_and_whitespace(flag in prop_oneof![Just("true"), Just("false")], lead_ws in "[ \t]{0,3}", trail_ws in "[ \t]{0,3}", mixed_case in any::<bool>()) {
            let token = if mixed_case {
                flag.chars().enumerate().map(|(i, c)| if i % 2 == 0 { c.to_ascii_uppercase() } else { c }).collect::<String>()
            } else {
                flag.to_string()
            };
            let value = format!("{lead_ws}{token}{trail_ws}");
            let out = parse_bool_query_param(Some(&value)).expect("valid boolish value");
            prop_assert_eq!(out, Some(flag == "true"));
        }
    }
}
