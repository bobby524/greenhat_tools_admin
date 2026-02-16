use std::net::SocketAddr;

use axum::extract::State;
use metrics_exporter_prometheus::PrometheusHandle;
use tracing::info;

use gateway::config::GatewayConfig;
use gateway::{SERVICE_NAME, VERSION};

mod metrics;
mod telemetry;

// ---------------------------------------------------------------------------
// Shared state (metrics handle needed by the /metrics route)
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct MetricsState {
    handle: PrometheusHandle,
}

/// Prometheus scrape endpoint — returns `text/plain` exposition format.
async fn metrics_handler(State(state): State<MetricsState>) -> String {
    state.handle.render()
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() {
    // 1. Structured tracing (+ optional OTel).
    //    Dropping the guard flushes the OTel pipeline.
    let _telemetry_guard = telemetry::init();

    // 2. Prometheus metrics recorder
    let metrics_handle = metrics::setup_recorder();

    // 3. Build config + router from the library crate
    let config = GatewayConfig::from_env();
    let mut router = gateway::app(&config, None);

    // Bolt on /metrics (needs its own state; lives outside the auth stack)
    router = router
        .route(
            "/metrics",
            axum::routing::get(metrics_handler).with_state(MetricsState {
                handle: metrics_handle,
            }),
        )
        // Per-request metrics middleware — outermost to capture full latency
        .layer(axum::middleware::from_fn(metrics::track_metrics));

    // 4. Bind & serve
    let addr = SocketAddr::from(([0, 0, 0, 0], config.port));
    info!(%addr, service = SERVICE_NAME, version = VERSION, "starting gateway");

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();

    // Graceful shutdown
    let shutdown = async {
        tokio::signal::ctrl_c().await.ok();
        info!("shutdown signal received");
        // _telemetry_guard will be dropped after serve returns, flushing OTel
    };

    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown)
        .await
        .unwrap();
}
