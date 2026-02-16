//! Prometheus metrics: recorder setup + per-request middleware.
//!
//! Exposed metrics:
//!
//! | Name                              | Type      | Labels                     |
//! |-----------------------------------|-----------|----------------------------|
//! | `http_requests_total`             | counter   | method, path, status       |
//! | `http_request_duration_seconds`   | histogram | method, path               |
//! | `http_requests_in_flight`         | gauge     | —                          |

use axum::{extract::Request, middleware::Next, response::Response};
use metrics::{counter, gauge, histogram};
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use std::time::Instant;

/// Install the global `metrics` recorder backed by Prometheus and return a
/// handle whose `.render()` produces the scrape-ready text.
pub fn setup_recorder() -> PrometheusHandle {
    PrometheusBuilder::new()
        .install_recorder()
        .expect("failed to install Prometheus metrics recorder")
}

/// Axum middleware that records request count, latency, and in-flight gauge.
///
/// **Layer ordering:** place *outside* the tracing layer so the timer
/// captures the full request lifecycle including auth and serialisation.
pub async fn track_metrics(request: Request, next: Next) -> Response {
    let method = request.method().to_string();
    let path = request.uri().path().to_string();
    let start = Instant::now();

    gauge!("http_requests_in_flight").increment(1.0);

    let response = next.run(request).await;

    gauge!("http_requests_in_flight").decrement(1.0);

    let status = response.status().as_u16().to_string();
    let duration = start.elapsed().as_secs_f64();

    counter!(
        "http_requests_total",
        "method" => method.clone(),
        "path" => path.clone(),
        "status" => status,
    )
    .increment(1);

    histogram!(
        "http_request_duration_seconds",
        "method" => method,
        "path" => path,
    )
    .record(duration);

    response
}
