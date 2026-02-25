//! Integration tests for the deny-by-default RBAC middleware.
//!
//! These tests exercise the full middleware stack (request-id → RBAC → handler)
//! without a real auth backend.  A small helper middleware injects a
//! configurable [`Principal`] into request extensions, simulating what the
//! auth layer does in production.

use std::sync::Arc;

use axum::body::Body;
use axum::extract::Request;
use axum::http::StatusCode;
use axum::middleware::{self as axum_mw, Next};
use axum::response::Response;
use axum::routing::{get, post};
use axum::Router;
use http_body_util::BodyExt;
use tower::ServiceExt;

use gateway::audit::AuditLog;
use gateway::auth::{AuthMethod, Principal};
use gateway::middleware::rbac::{rbac_middleware, RbacState};
use gateway::rbac::{Policy, PolicyEngine};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Collect a response body into a UTF-8 string.
async fn body_string(resp: Response) -> String {
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8(bytes.to_vec()).unwrap()
}

/// Parse the response body as JSON.
async fn body_json(resp: Response) -> serde_json::Value {
    let s = body_string(resp).await;
    serde_json::from_str(&s).unwrap()
}

/// Build a test [`Principal`] with the given roles.
fn test_principal(roles: &[&str]) -> Principal {
    Principal {
        user_id: "test-user".into(),
        email: None,
        org_id: None,
        roles: roles.iter().map(|s| s.to_string()).collect(),
        session_id: "test-session".into(),
        auth_method: AuthMethod::Bearer,
    }
}

/// Middleware that injects a given [`Principal`] into request extensions.
async fn inject_principal(mut req: Request, next: Next) -> Response {
    if let Some(p) = req.extensions().get::<Principal>().cloned() {
        // Already set — don't override (allows per-request customisation).
        let _ = p;
    } else {
        // Default: inject the principal stored in extensions by the test harness.
        // (We use a shared Arc to pass the principal to the middleware.)
    }
    next.run(req).await
}

/// Build a test router with RBAC middleware and a configurable principal injector.
///
/// The returned router has:
///   - `GET /health` (infrastructure, RBAC-exempt)
///   - `GET /v1/data` (requires `data:read`)
///   - `POST /v1/tools/call` (requires `tools:invoke` + tool-level check)
///   - `POST /v1/resources` (requires `data:write`)
fn test_app(policy_json: &str, principal: Option<Principal>) -> Router {
    let policy = Policy::load_from_str(policy_json).unwrap();
    let engine = Arc::new(PolicyEngine::new(policy));
    let audit = AuditLog::from_env();
    let rbac_state = RbacState::new(engine, audit);

    let mut router = Router::new()
        .route("/health", get(|| async { "ok" }))
        .route("/v1/data", get(|| async { "data" }))
        .route("/v1/tools/call", post(|| async { "tool_result" }))
        .route("/v1/resources", post(|| async { "created" }));

    // RBAC (inner)
    router = router.layer(axum_mw::from_fn_with_state(rbac_state, rbac_middleware));

    // Fake auth: inject principal (outer, runs first)
    if let Some(p) = principal {
        router = router.layer(axum_mw::from_fn(move |mut req: Request, next: Next| {
            let p = p.clone();
            async move {
                req.extensions_mut().insert(p);
                next.run(req).await
            }
        }));
    }

    router
}

const TEST_POLICY: &str = r#"{
    "schema_version": "0.1.0",
    "id": "test-policy",
    "tools": {
        "default_policy": "deny",
        "allowlist": {
            "web_search": {
                "enabled": true,
                "allowed_roles": ["analyst", "admin"]
            },
            "disabled_tool": {
                "enabled": false,
                "allowed_roles": ["admin"]
            }
        }
    },
    "roles": {
        "admin":   { "permissions": ["*"] },
        "analyst": { "permissions": ["tools:invoke", "tools:list", "data:read"] },
        "viewer":  { "permissions": ["data:read"] }
    }
}"#;

// ---------------------------------------------------------------------------
// Tests — infrastructure endpoints bypass RBAC
// ---------------------------------------------------------------------------

#[tokio::test]
async fn health_bypasses_rbac_without_principal() {
    let app = test_app(TEST_POLICY, None);
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

// ---------------------------------------------------------------------------
// Tests — no principal → 403
// ---------------------------------------------------------------------------

#[tokio::test]
async fn missing_principal_returns_403() {
    let app = test_app(TEST_POLICY, None);
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/data")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);

    let json = body_json(resp).await;
    assert_eq!(json["error"]["code"], 403);
    assert!(json["error"]["message"]
        .as_str()
        .unwrap()
        .contains("authentication required"));
}

// ---------------------------------------------------------------------------
// Tests — route-level RBAC
// ---------------------------------------------------------------------------

#[tokio::test]
async fn analyst_can_read_data() {
    let app = test_app(TEST_POLICY, Some(test_principal(&["analyst"])));
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/data")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(body_string(resp).await, "data");
}

#[tokio::test]
async fn viewer_can_read_data() {
    let app = test_app(TEST_POLICY, Some(test_principal(&["viewer"])));
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/data")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn viewer_cannot_write_resources() {
    let app = test_app(TEST_POLICY, Some(test_principal(&["viewer"])));
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/resources")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);

    let json = body_json(resp).await;
    assert_eq!(json["error"]["kind"], "forbidden");
}

#[tokio::test]
async fn admin_can_do_anything() {
    let app = test_app(TEST_POLICY, Some(test_principal(&["admin"])));

    // Read
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/data")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Write
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/resources")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

// ---------------------------------------------------------------------------
// Tests — tool-level RBAC (POST /v1/tools/call with body inspection)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn analyst_can_call_allowed_tool() {
    let app = test_app(TEST_POLICY, Some(test_principal(&["analyst"])));
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/tools/call")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"tool":"web_search","params":{}}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(body_string(resp).await, "tool_result");
}

#[tokio::test]
async fn viewer_cannot_call_tool_route_level() {
    // viewer lacks "tools:invoke" permission → denied at route level
    let app = test_app(TEST_POLICY, Some(test_principal(&["viewer"])));
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/tools/call")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"tool":"web_search","params":{}}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn analyst_denied_unlisted_tool() {
    let app = test_app(TEST_POLICY, Some(test_principal(&["analyst"])));
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/tools/call")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"tool":"not_in_allowlist","params":{}}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);

    let json = body_json(resp).await;
    assert!(json["error"]["message"]
        .as_str()
        .unwrap()
        .contains("not in allowlist"));
}

#[tokio::test]
async fn analyst_denied_disabled_tool() {
    let app = test_app(TEST_POLICY, Some(test_principal(&["analyst"])));
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/tools/call")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"tool":"disabled_tool","params":{}}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);

    let json = body_json(resp).await;
    assert!(json["error"]["message"]
        .as_str()
        .unwrap()
        .contains("disabled"));
}

#[tokio::test]
async fn admin_bypasses_tool_level_checks() {
    let app = test_app(TEST_POLICY, Some(test_principal(&["admin"])));
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/tools/call")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"tool":"not_listed_at_all","params":{}}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    // Admin wildcard bypasses everything
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn malformed_body_passes_through_to_handler() {
    // If the body can't be parsed as JSON, tool-level RBAC is skipped
    // (route-level check already passed).
    let app = test_app(TEST_POLICY, Some(test_principal(&["analyst"])));
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/tools/call")
                .header("content-type", "application/json")
                .body(Body::from("not json"))
                .unwrap(),
        )
        .await
        .unwrap();
    // Route-level "tools:invoke" passed; body parse failed → handler gets the request
    assert_eq!(resp.status(), StatusCode::OK);
}

// ---------------------------------------------------------------------------
// Tests — error response includes request_id when present
// ---------------------------------------------------------------------------

#[tokio::test]
async fn error_includes_request_id_field() {
    // Even without the request-id middleware layer, the field is present (as null or populated).
    let app = test_app(TEST_POLICY, None);
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/data")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);

    let json = body_json(resp).await;
    assert_eq!(json["error"]["code"], 403);
    // request_id may be "unknown" or present — verify the error shape is correct
    assert!(json["error"]["request_id"].is_string());
}
