use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt; // for oneshot

use gateway::config::GatewayConfig;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build a gateway with tight rate-limit settings for deterministic testing.
/// CSRF is disabled by default so validation / rate-limit tests are unaffected.
fn test_config(burst: u32) -> GatewayConfig {
    GatewayConfig {
        port: 0,
        rate_limit_read_rps: 0.0, // no refill → deterministic exhaustion
        rate_limit_read_burst: burst,
        rate_limit_write_rps: 0.0,
        rate_limit_write_burst: burst,
        max_body_size: 256,
        cors_allow_origins: vec!["https://tools.greenhatsec.com".to_owned()],
        csrf_enabled: false,
        csrf_cookie_name: "csrf_token".to_owned(),
        csrf_cookie_domain: None,
        csrf_header_name: "x-csrf-token".to_owned(),

        auth_enabled: false,
        betterauth_base_url: "http://localhost:3000".to_owned(),
        proxy_upstream_base_url: "http://localhost:3000".to_owned(),
        betterauth_cookie_name: "better-auth.session_token".to_owned(),
        betterauth_timeout_ms: 2000,
        policy_file: None,
    }
}

/// Collect a response body as a UTF-8 String.
async fn body_string(resp: axum::response::Response) -> String {
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8(bytes.to_vec()).unwrap()
}

// ---------------------------------------------------------------------------
// Rate-limit tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn rate_limit_allows_requests_within_burst() {
    let config = test_config(3);
    let app = gateway::app(&config, None);

    // All 3 requests should succeed (burst=3, rps=0 → no refill).
    // Router::clone() shares the same Arc<Mutex<…>> rate-limiter state.
    for i in 0..3 {
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::OK,
            "request {i} should succeed within burst",
        );
    }
}

#[tokio::test]
async fn rate_limit_rejects_after_burst_exhausted() {
    let config = test_config(2);
    let app = gateway::app(&config, None);

    // Exhaust the 2-token burst
    for _ in 0..2 {
        let resp = app
            .clone()
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

    // 3rd request must be throttled
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);

    // Verify structured error body
    let body = body_string(resp).await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(json["error"]["code"], 429);
    assert_eq!(json["error"]["kind"], "rate_limited");
    assert_eq!(
        json["error"]["message"],
        "rate limit exceeded \u{2014} try again later"
    );
}

// ---------------------------------------------------------------------------
// Content-Type validation tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn post_with_wrong_content_type_is_rejected() {
    let config = test_config(100);
    let app = gateway::app(&config, None);

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/anything")
                .header("content-type", "text/plain")
                .body(Body::from("hello"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);

    let body = body_string(resp).await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(json["error"]["code"], 415);
    assert!(json["error"]["message"]
        .as_str()
        .unwrap()
        .contains("application/json"));
}

#[tokio::test]
async fn post_missing_content_type_is_rejected() {
    let config = test_config(100);
    let app = gateway::app(&config, None);

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/anything")
                // no Content-Type header
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
}

#[tokio::test]
async fn post_with_json_content_type_passes_validation() {
    let config = test_config(100);
    let app = gateway::app(&config, None);

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/health")
                .header("content-type", "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();

    // Passes content-type check → hits router → GET-only route → 405
    assert_eq!(resp.status(), StatusCode::METHOD_NOT_ALLOWED);
}

#[tokio::test]
async fn post_with_json_charset_passes_validation() {
    let config = test_config(100);
    let app = gateway::app(&config, None);

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/health")
                .header("content-type", "application/json; charset=utf-8")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();

    // Should NOT be 415 — our check uses starts_with
    assert_ne!(resp.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
}

#[tokio::test]
async fn get_without_content_type_passes() {
    let config = test_config(100);
    let app = gateway::app(&config, None);

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

#[tokio::test]
async fn request_id_is_generated_when_missing() {
    let config = test_config(100);
    let app = gateway::app(&config, None);

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert!(resp.headers().get("x-request-id").is_some());
}

#[tokio::test]
async fn request_id_is_preserved_when_provided() {
    let config = test_config(100);
    let app = gateway::app(&config, None);

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .header("x-request-id", "req-test-123")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.headers().get("x-request-id").unwrap(), "req-test-123");
}

// ---------------------------------------------------------------------------
// Body-size validation tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn oversized_content_length_is_rejected() {
    let config = test_config(100);
    // max_body_size = 256 bytes (set in test_config)
    let app = gateway::app(&config, None);

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/endpoint")
                .header("content-type", "application/json")
                .header("content-length", "999999")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::PAYLOAD_TOO_LARGE);

    let body = body_string(resp).await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(json["error"]["code"], 413);
    assert!(json["error"]["message"].as_str().unwrap().contains("256"));
}

#[tokio::test]
async fn content_length_within_limit_passes() {
    let config = test_config(100);
    let app = gateway::app(&config, None);

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/health")
                .header("content-type", "application/json")
                .header("content-length", "2")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();

    // Passes validation → 405 from router (POST on GET-only route)
    assert_ne!(resp.status(), StatusCode::PAYLOAD_TOO_LARGE);
}
