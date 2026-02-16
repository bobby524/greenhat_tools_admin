# Observability

The API MCP Gateway ships with a built-in observability stack:

| Signal | Implementation | Status |
|--------|---------------|--------|
| **Metrics** | Prometheus exposition via `/metrics` | ✅ Always on |
| **Tracing** | Structured JSON logs (`tracing`) | ✅ Always on |
| **Distributed tracing** | OpenTelemetry OTLP export | ✅ Optional (feature + env) |
| **Structured errors** | JSON error bodies with `request_id` | ✅ Always on |
| **Request ID** | UUID v4, propagated end-to-end | ✅ Always on |

---

## Quick Start

```bash
# Default — metrics + JSON logs, no OTel
cargo run --package gateway

# With OpenTelemetry (requires an OTLP collector)
cargo build --package gateway --features otel
OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4317 cargo run --package gateway --features otel
```

---

## Metrics (`/metrics`)

The gateway exposes a Prometheus-compatible scrape endpoint at **`GET /metrics`**.

### Exposed Metrics

| Metric | Type | Labels | Description |
|--------|------|--------|-------------|
| `http_requests_total` | counter | `method`, `path`, `status` | Total HTTP requests served |
| `http_request_duration_seconds` | summary | `method`, `path` | Request latency distribution |
| `http_requests_in_flight` | gauge | — | Currently active requests |

### Scrape Config (Prometheus)

```yaml
scrape_configs:
  - job_name: api-mcp-gateway
    scrape_interval: 15s
    static_configs:
      - targets: ['gateway:8080']
    metrics_path: /metrics
```

### Cardinality Note

The `path` label uses the raw request URI. As long as routes are fixed
(`/health`, `/version`, etc.) this is safe. When dynamic path segments are
added (e.g. `/tools/:id`), switch to `MatchedPath` or a normalisation
function to avoid cardinality explosion.

---

## Structured Logging

Every log line is **JSON** (via `tracing-subscriber` with the `json` feature).

```json
{
  "timestamp": "2026-02-15T05:56:44.052824Z",
  "level": "DEBUG",
  "fields": { "message": "started processing request" },
  "target": "tower_http::trace::on_request",
  "span": {
    "method": "GET",
    "uri": "/health",
    "request_id": "e11eee43-9008-4585-88fe-a0fe209a6d84",
    "name": "http_request"
  }
}
```

### Controlling Log Level

Set the `RUST_LOG` environment variable:

```bash
RUST_LOG=gateway=info,tower_http=warn cargo run --package gateway
```

Default (no `RUST_LOG`): `gateway=debug,tower_http=debug`.

---

## Request ID Propagation

Every inbound request is assigned a **UUID v4** `x-request-id` (unless the
client sends one). The ID flows through:

1. **Request headers** → `x-request-id` (set by `SetRequestIdLayer`)
2. **Tracing spans** → `request_id` field in every log line
3. **Response headers** → `x-request-id` echoed back (via `PropagateRequestIdLayer`)
4. **Error bodies** → `request_id` field in JSON error responses

```
$ curl -i http://localhost:8080/nonexistent

HTTP/1.1 404 Not Found
content-type: application/json
x-request-id: 1207bc95-69aa-473d-ad9e-6518c56a93f9

{
  "error": {
    "code": 404,
    "kind": "not_found",
    "message": "no route matches /nonexistent",
    "request_id": "1207bc95-69aa-473d-ad9e-6518c56a93f9"
  }
}
```

**Client-supplied IDs:** If a client sends `x-request-id`, the gateway
honours it (the `SetRequestIdLayer` only generates one when the header is
absent).

---

## Structured Errors

All error responses follow a uniform JSON envelope:

```json
{
  "error": {
    "code": 429,
    "kind": "rate_limited",
    "message": "rate limit exceeded — try again later",
    "request_id": "xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"
  }
}
```

| Field | Type | Description |
|-------|------|-------------|
| `code` | `u16` | HTTP status code |
| `kind` | `string` | Machine-readable error category |
| `message` | `string` | Human-readable description |
| `request_id` | `string?` | Present when available (omitted for pre-middleware errors) |

### Error Kinds

| Kind | HTTP Status |
|------|-------------|
| `bad_request` | 400 |
| `unauthorized` | 401 |
| `forbidden` | 403 |
| `not_found` | 404 |
| `payload_too_large` | 413 |
| `unsupported_media_type` | 415 |
| `unprocessable_entity` | 422 |
| `rate_limited` | 429 |
| `internal` | 500 |

---

## OpenTelemetry (Optional)

Distributed tracing via OTLP/gRPC is available behind the **`otel`** Cargo
feature flag. It is **dual-gated**: the feature must be compiled in *and*
the `OTEL_EXPORTER_OTLP_ENDPOINT` environment variable must be set at
runtime.

### Building with OTel

```bash
cargo build --release --package gateway --features otel
```

### Enabling at Runtime

```bash
# Required — OTLP collector gRPC endpoint
export OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4317

# Optional — standard OTel env vars are respected:
# OTEL_SERVICE_NAME          (default: api-mcp-gateway)
# OTEL_RESOURCE_ATTRIBUTES   (e.g. deployment.environment=staging)
```

If `OTEL_EXPORTER_OTLP_ENDPOINT` is **not set**, the OTel layer is skipped
entirely — zero overhead.

### How It Works

```
┌──────────┐   tracing spans    ┌───────────────────┐   OTLP/gRPC   ┌──────────┐
│  Gateway  │ ──────────────────▶│ tracing-opentelemetry│ ────────────▶│ Collector│
│  (axum)   │                    │ + opentelemetry-otlp │              │ (Jaeger/ │
└──────────┘                    └───────────────────┘              │  Tempo)  │
                                                                   └──────────┘
```

Every `tracing` span (including the per-request `http_request` span with
its `request_id`, `method`, and `uri` fields) is automatically exported as
an OTel span. No code changes needed — just enable the feature and set the
env var.

### Graceful Shutdown

The `SdkTracerProvider` is held in a guard returned by `telemetry::init()`.
When the process receives `SIGINT` (Ctrl-C), the guard is dropped, which
flushes all pending spans to the collector before exiting.

### Docker

To build a Docker image with OTel support:

```dockerfile
# In the builder stage, change the build command:
RUN cargo build --release --package gateway --features otel
```

Then pass the env var at runtime:

```bash
docker run -e OTEL_EXPORTER_OTLP_ENDPOINT=http://collector:4317 gateway
```

---

## Environment Variables Reference

| Variable | Default | Description |
|----------|---------|-------------|
| `PORT` | `8080` | TCP listen port |
| `RUST_LOG` | `gateway=debug,tower_http=debug` | Log filter (tracing `EnvFilter`) |
| `OTEL_EXPORTER_OTLP_ENDPOINT` | *(unset)* | OTLP gRPC endpoint; enables OTel when set (requires `otel` feature) |
| `OTEL_SERVICE_NAME` | `api-mcp-gateway` | Service name reported to OTel |
| `OTEL_RESOURCE_ATTRIBUTES` | *(unset)* | Extra resource attributes (e.g. `deployment.environment=prod`) |

---

## Architecture

```
                    Incoming Request
                          │
                 ┌────────▼────────┐
                 │ SetRequestIdLayer│  ← generates x-request-id
                 └────────┬────────┘
                 ┌────────▼────────┐
                 │ Metrics middleware│  ← records http_requests_total, latency, in-flight
                 └────────┬────────┘
                 ┌────────▼────────┐
                 │   TraceLayer     │  ← creates tracing span with request_id
                 └────────┬────────┘
                 ┌────────▼────────┐
                 │PropagateRequestId│  ← copies x-request-id to response
                 └────────┬────────┘
                 ┌────────▼────────┐
                 │   Rate Limiter   │
                 └────────┬────────┘
                 ┌────────▼────────┐
                 │   Validation     │
                 └────────┬────────┘
                 ┌────────▼────────┐
                 │      Auth        │
                 └────────┬────────┘
                 ┌────────▼────────┐
                 │     Handler      │  ← /health, /version, /metrics, fallback (404)
                 └─────────────────┘
```
