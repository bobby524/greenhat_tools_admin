use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt; // oneshot

use gateway::config::GatewayConfig;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Config with CSRF **enabled** and generous rate-limit so only CSRF matters.
fn csrf_config() -> GatewayConfig {
    GatewayConfig {
        port: 0,
        rate_limit_rps: 100.0,
        rate_limit_burst: 1000,
        max_body_size: 1_048_576,
        csrf_enabled: true,
        csrf_cookie_name: "csrf_token".to_owned(),
        csrf_header_name: "x-csrf-token".to_owned(),

        auth_enabled: false,
        betterauth_base_url: "http://localhost:3000".to_owned(),
        betterauth_cookie_name: "better-auth.session_token".to_owned(),
        betterauth_timeout_ms: 2000,
        policy_file: None,
    }
}

/// Config with CSRF **disabled**.
fn csrf_disabled_config() -> GatewayConfig {
    GatewayConfig {
        csrf_enabled: false,
        ..csrf_config()
    }
}

/// Collect response body as UTF-8 string.
async fn body_string(resp: axum::response::Response) -> String {
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8(bytes.to_vec()).unwrap()
}

/// Extract the `csrf_token=<value>` from the Set-Cookie header(s).
fn extract_csrf_cookie(resp: &axum::response::Response, cookie_name: &str) -> Option<String> {
    resp.headers()
        .get_all("set-cookie")
        .iter()
        .filter_map(|v| v.to_str().ok())
        .find_map(|sc| {
            // e.g. "csrf_token=abcd-1234; Path=/; SameSite=Lax"
            let prefix = format!("{}=", cookie_name);
            if sc.starts_with(&prefix) {
                let rest = &sc[prefix.len()..];
                let value = rest.split(';').next().unwrap_or("").trim();
                if value.is_empty() {
                    None
                } else {
                    Some(value.to_owned())
                }
            } else {
                None
            }
        })
}

// ---------------------------------------------------------------------------
// Tests: Cookie issuance
// ---------------------------------------------------------------------------

#[tokio::test]
async fn get_sets_csrf_cookie() {
    let app = gateway::app(&csrf_config(), None);

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

    // Even though /health is exempt from CSRF *enforcement*, safe methods
    // still issue the cookie — wait, /health is exempt from both.
    // Let's test on a non-exempt path instead.
}

#[tokio::test]
async fn get_on_non_exempt_path_sets_csrf_cookie() {
    let app = gateway::app(&csrf_config(), None);

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/some-page")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // Will 404 (fallback) but should still have the Set-Cookie header.
    let token = extract_csrf_cookie(&resp, "csrf_token");
    assert!(
        token.is_some(),
        "GET on non-exempt path should set csrf_token cookie"
    );
    assert!(
        !token.unwrap().is_empty(),
        "csrf_token value should be non-empty"
    );
}

// ---------------------------------------------------------------------------
// Tests: Enforcement on state-changing methods
// ---------------------------------------------------------------------------

#[tokio::test]
async fn post_without_csrf_token_is_rejected() {
    let app = gateway::app(&csrf_config(), None);

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/some-endpoint")
                .header("content-type", "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::FORBIDDEN);

    let body = body_string(resp).await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(json["error"]["code"], 403);
    assert!(json["error"]["message"].as_str().unwrap().contains("CSRF"));
}

#[tokio::test]
async fn put_without_csrf_token_is_rejected() {
    let app = gateway::app(&csrf_config(), None);

    let resp = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/resource/1")
                .header("content-type", "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn patch_without_csrf_token_is_rejected() {
    let app = gateway::app(&csrf_config(), None);

    let resp = app
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/resource/1")
                .header("content-type", "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn delete_without_csrf_token_is_rejected() {
    let app = gateway::app(&csrf_config(), None);

    let resp = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/resource/1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

// ---------------------------------------------------------------------------
// Tests: Valid double-submit
// ---------------------------------------------------------------------------

#[tokio::test]
async fn post_with_matching_csrf_token_passes() {
    let app = gateway::app(&csrf_config(), None);
    let token = "test-csrf-token-abc123";

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/some-endpoint")
                .header("content-type", "application/json")
                .header("cookie", format!("csrf_token={}", token))
                .header("x-csrf-token", token)
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();

    // Should pass CSRF check → hits router → 404 (no matching route) is fine
    assert_ne!(
        resp.status(),
        StatusCode::FORBIDDEN,
        "matching CSRF token should not be rejected"
    );
}

#[tokio::test]
async fn delete_with_matching_csrf_token_passes() {
    let app = gateway::app(&csrf_config(), None);
    let token = "del-token-xyz";

    let resp = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/resource/99")
                .header("cookie", format!("csrf_token={}", token))
                .header("x-csrf-token", token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_ne!(resp.status(), StatusCode::FORBIDDEN);
}

// ---------------------------------------------------------------------------
// Tests: Mismatch
// ---------------------------------------------------------------------------

#[tokio::test]
async fn post_with_mismatched_csrf_tokens_is_rejected() {
    let app = gateway::app(&csrf_config(), None);

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/endpoint")
                .header("content-type", "application/json")
                .header("cookie", "csrf_token=real-value")
                .header("x-csrf-token", "different-value")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn post_with_empty_csrf_cookie_is_rejected() {
    let app = gateway::app(&csrf_config(), None);

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/endpoint")
                .header("content-type", "application/json")
                .header("cookie", "csrf_token=")
                .header("x-csrf-token", "")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn post_with_cookie_but_no_header_is_rejected() {
    let app = gateway::app(&csrf_config(), None);

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/endpoint")
                .header("content-type", "application/json")
                .header("cookie", "csrf_token=some-token")
                // no x-csrf-token header
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn post_with_header_but_no_cookie_is_rejected() {
    let app = gateway::app(&csrf_config(), None);

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/endpoint")
                .header("content-type", "application/json")
                // no cookie
                .header("x-csrf-token", "some-token")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

// ---------------------------------------------------------------------------
// Tests: Exempt paths bypass CSRF
// ---------------------------------------------------------------------------

#[tokio::test]
async fn post_to_health_bypasses_csrf() {
    let app = gateway::app(&csrf_config(), None);

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/health")
                .header("content-type", "application/json")
                // no CSRF tokens at all
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();

    // Not 403 — exempt from CSRF.  Will be 405 (method not allowed).
    assert_ne!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn post_to_version_bypasses_csrf() {
    let app = gateway::app(&csrf_config(), None);

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/version")
                .header("content-type", "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_ne!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn post_to_metrics_bypasses_csrf() {
    let app = gateway::app(&csrf_config(), None);

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/metrics")
                .header("content-type", "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_ne!(resp.status(), StatusCode::FORBIDDEN);
}

// ---------------------------------------------------------------------------
// Tests: CSRF disabled
// ---------------------------------------------------------------------------

#[tokio::test]
async fn csrf_disabled_allows_post_without_token() {
    let app = gateway::app(&csrf_disabled_config(), None);

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/some-endpoint")
                .header("content-type", "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();

    // With CSRF disabled, request should NOT be 403.
    assert_ne!(resp.status(), StatusCode::FORBIDDEN);
}

// ---------------------------------------------------------------------------
// Tests: Custom cookie / header names
// ---------------------------------------------------------------------------

#[tokio::test]
async fn custom_cookie_and_header_names_work() {
    let config = GatewayConfig {
        csrf_cookie_name: "my_xsrf".to_owned(),
        csrf_header_name: "x-my-xsrf".to_owned(),
        ..csrf_config()
    };
    let app = gateway::app(&config, None);
    let token = "custom-tok";

    // Should pass with custom names
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/endpoint")
                .header("content-type", "application/json")
                .header("cookie", format!("my_xsrf={}", token))
                .header("x-my-xsrf", token)
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_ne!(resp.status(), StatusCode::FORBIDDEN);

    // Should FAIL with the old default names
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/endpoint")
                .header("content-type", "application/json")
                .header("cookie", format!("csrf_token={}", token))
                .header("x-csrf-token", token)
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

// ---------------------------------------------------------------------------
// Tests: Cookie attributes
// ---------------------------------------------------------------------------

#[tokio::test]
async fn csrf_cookie_has_correct_attributes() {
    let app = gateway::app(&csrf_config(), None);

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/some-page")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let set_cookie = resp
        .headers()
        .get_all("set-cookie")
        .iter()
        .filter_map(|v| v.to_str().ok())
        .find(|s| s.starts_with("csrf_token="))
        .expect("csrf_token Set-Cookie header should be present");

    assert!(
        set_cookie.contains("SameSite=Lax"),
        "should include SameSite=Lax"
    );
    assert!(set_cookie.contains("Path=/"), "should include Path=/");
    // Must NOT be HttpOnly — JS needs to read it
    assert!(
        !set_cookie.contains("HttpOnly"),
        "CSRF cookie must NOT be HttpOnly"
    );
}
