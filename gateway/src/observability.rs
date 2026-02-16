use std::sync::Arc;
use std::time::Instant;

use axum::extract::{MatchedPath, Request, State};
use axum::middleware::Next;
use axum::response::Response;
use serde::Serialize;
use tokio::sync::mpsc;

use crate::SERVICE_NAME;

pub const X_REQUEST_ID: &str = "x-request-id";

#[derive(Clone, Debug, Default)]
pub struct UpstreamTrace {
    pub status: Option<u16>,
    pub latency_ms: Option<u64>,
    pub timeout_hit: bool,
    pub error_kind: Option<String>,
}

#[derive(Clone)]
pub struct Observability {
    inner: Arc<ObservabilityInner>,
}

#[derive(Clone)]
struct ObservabilityInner {
    service: String,
    shipper: Option<mpsc::Sender<BetterStackEnvelope>>,
}

#[derive(Debug, Clone)]
struct BetterStackEnvelope {
    message: String,
}

#[derive(Debug, Serialize)]
struct RequestLog {
    service: String,
    route: String,
    method: String,
    status: u16,
    x_request_id: String,
    latency_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    upstream_status: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    upstream_latency_ms: Option<u64>,
    timeout_hit: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    error_kind: Option<String>,
}

impl Observability {
    pub fn from_env() -> Self {
        let service = SERVICE_NAME.to_owned();
        let shipper = betterstack_shipper_from_env();

        Self {
            inner: Arc::new(ObservabilityInner { service, shipper }),
        }
    }

    fn emit(&self, log: RequestLog) {
        tracing::info!(
            service = %log.service,
            route = %log.route,
            method = %log.method,
            status = log.status,
            x_request_id = %log.x_request_id,
            latency_ms = log.latency_ms,
            upstream_status = ?log.upstream_status,
            upstream_latency_ms = ?log.upstream_latency_ms,
            timeout_hit = log.timeout_hit,
            error_kind = ?log.error_kind,
            "http_request_complete"
        );

        let Some(tx) = &self.inner.shipper else {
            return;
        };

        let Ok(message) = serde_json::to_string(&log) else {
            return;
        };

        // Non-blocking and fail-open by design.
        let _ = tx.try_send(BetterStackEnvelope { message });
    }
}

pub async fn request_log_middleware(
    State(obs): State<Observability>,
    req: Request,
    next: Next,
) -> Response {
    let start = Instant::now();

    let method = req.method().to_string();
    let route = req
        .extensions()
        .get::<MatchedPath>()
        .map(|p| p.as_str().to_owned())
        .unwrap_or_else(|| req.uri().path().to_owned());

    let request_id = req
        .headers()
        .get(X_REQUEST_ID)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
        .to_owned();

    let response = next.run(req).await;

    let status = response.status().as_u16();
    let latency_ms = start.elapsed().as_millis() as u64;
    let upstream = response
        .extensions()
        .get::<UpstreamTrace>()
        .cloned()
        .unwrap_or_default();

    let error_kind = upstream.error_kind.or_else(|| classify_http_error(status));

    obs.emit(RequestLog {
        service: obs.inner.service.clone(),
        route,
        method,
        status,
        x_request_id: request_id,
        latency_ms,
        upstream_status: upstream.status,
        upstream_latency_ms: upstream.latency_ms,
        timeout_hit: upstream.timeout_hit,
        error_kind,
    });

    response
}

fn classify_http_error(status: u16) -> Option<String> {
    match status {
        400..=499 => Some("client_error".to_owned()),
        500..=599 => Some("server_error".to_owned()),
        _ => None,
    }
}

fn betterstack_shipper_from_env() -> Option<mpsc::Sender<BetterStackEnvelope>> {
    let enabled = std::env::var("BETTERSTACK_ENABLED")
        .ok()
        .map(|v| {
            let v = v.trim().to_ascii_lowercase();
            v == "1" || v == "true" || v == "yes" || v == "on"
        })
        .unwrap_or(false);

    if !enabled {
        tracing::info!("Better Stack shipping disabled");
        return None;
    }

    let token = std::env::var("BETTERSTACK_SOURCE_TOKEN").ok()?;
    let host = std::env::var("BETTERSTACK_INGEST_HOST").ok()?;

    let host = host.trim().trim_end_matches('/').to_owned();
    if host.is_empty() {
        return None;
    }

    let url = if host.starts_with("http://") || host.starts_with("https://") {
        host
    } else {
        format!("https://{host}")
    };

    let (tx, mut rx) = mpsc::channel::<BetterStackEnvelope>(2048);

    let log_url = url.clone();
    tokio::spawn(async move {
        let client = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(2))
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        while let Some(envelope) = rx.recv().await {
            let send = client
                .post(&log_url)
                .header("Authorization", format!("Bearer {token}"))
                .header("Content-Type", "application/json")
                .body(envelope.message)
                .send()
                .await;

            if let Err(err) = send {
                tracing::warn!(error = %err, "failed to ship log to Better Stack");
            }
        }
    });

    tracing::info!(ingest_url = %url, "Better Stack shipping enabled");
    Some(tx)
}
