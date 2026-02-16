//! Initialises the `tracing` subscriber stack.
//!
//! * **Always:** structured JSON formatter + [`EnvFilter`] from `RUST_LOG`.
//! * **With feature `otel` AND `OTEL_EXPORTER_OTLP_ENDPOINT` set:**
//!   adds an OpenTelemetry tracing layer that exports spans via OTLP/gRPC.
//!
//! [`init()`] returns a guard. Dropping the guard flushes and shuts down the
//! OTel pipeline (if enabled).

use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

/// Opaque guard returned by [`init()`].
///
/// Dropping this guard triggers a graceful shutdown of the OTel exporter (if
/// the `otel` feature is active and OTel was actually initialised).
pub struct TelemetryGuard {
    #[cfg(feature = "otel")]
    _provider: Option<opentelemetry_sdk::trace::SdkTracerProvider>,
}

/// Build a shared [`EnvFilter`] from `RUST_LOG` or a sensible default.
fn env_filter() -> EnvFilter {
    EnvFilter::try_from_default_env().unwrap_or_else(|_| "gateway=debug,tower_http=debug".into())
}

// ── With OpenTelemetry compiled in ──────────────────────────────────────

#[cfg(feature = "otel")]
pub fn init() -> TelemetryGuard {
    use opentelemetry::trace::TracerProvider;
    use opentelemetry_sdk::trace::SdkTracerProvider;

    let fmt_layer = tracing_subscriber::fmt::layer().json();

    if std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").is_ok() {
        let exporter = opentelemetry_otlp::SpanExporter::builder()
            .with_tonic()
            .build()
            .expect("failed to build OTLP span exporter");

        let resource = opentelemetry_sdk::Resource::builder()
            .with_service_name(gateway::SERVICE_NAME)
            .build();

        let provider = SdkTracerProvider::builder()
            .with_batch_exporter(exporter)
            .with_resource(resource)
            .build();

        let tracer = provider.tracer(gateway::SERVICE_NAME);

        let otel_layer = tracing_opentelemetry::layer().with_tracer(tracer);

        tracing_subscriber::registry()
            .with(env_filter())
            .with(fmt_layer)
            .with(otel_layer)
            .init();

        tracing::info!("OpenTelemetry tracing enabled");

        TelemetryGuard {
            _provider: Some(provider),
        }
    } else {
        tracing_subscriber::registry()
            .with(env_filter())
            .with(fmt_layer)
            .init();

        tracing::info!(
            "OpenTelemetry compiled-in but disabled (OTEL_EXPORTER_OTLP_ENDPOINT not set)"
        );

        TelemetryGuard { _provider: None }
    }
}

// ── Without OpenTelemetry ───────────────────────────────────────────────

#[cfg(not(feature = "otel"))]
pub fn init() -> TelemetryGuard {
    let fmt_layer = tracing_subscriber::fmt::layer().json();

    tracing_subscriber::registry()
        .with(env_filter())
        .with(fmt_layer)
        .init();

    TelemetryGuard {}
}
