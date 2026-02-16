use axum::{
    extract::Request, http::StatusCode, middleware as axum_mw, response::IntoResponse,
    routing::get, Router,
};
use tower::ServiceExt;

use gateway::middleware::headers::header_hardening_middleware;

async fn echo_sensitive_headers(req: Request) -> impl IntoResponse {
    let has_spoof = req.headers().get("x-user-id").is_some()
        || req.headers().get("x-org-id").is_some()
        || req.headers().get("x-roles").is_some();

    if has_spoof {
        return (StatusCode::BAD_REQUEST, "spoofable headers still present");
    }

    (StatusCode::OK, "ok")
}

#[tokio::test]
async fn strips_spoofable_identity_headers() {
    let app = Router::new()
        .route("/t", get(echo_sensitive_headers))
        .layer(axum_mw::from_fn(header_hardening_middleware));

    let res = app
        .oneshot(
            Request::builder()
                .uri("/t")
                .header("x-user-id", "attacker")
                .header("x-org-id", "attacker")
                .header("x-roles", "admin")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn adds_baseline_security_headers() {
    let app = Router::new()
        .route("/t", get(|| async { "ok" }))
        .layer(axum_mw::from_fn(header_hardening_middleware));

    let res = app
        .oneshot(
            Request::builder()
                .uri("/t")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let h = res.headers();
    assert_eq!(h.get("x-content-type-options").unwrap(), "nosniff");
    assert_eq!(h.get("x-frame-options").unwrap(), "DENY");
    assert_eq!(h.get("referrer-policy").unwrap(), "no-referrer");
    assert!(h.get("content-security-policy").is_some());
    assert!(h.get("permissions-policy").is_some());
    assert_eq!(h.get("cross-origin-resource-policy").unwrap(), "same-site");
}
