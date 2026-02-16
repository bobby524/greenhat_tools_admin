# API MCP Gateway

Rust-first API gateway with an embedded MCP (Model Context Protocol) runtime.

## Quick Start

```bash
# Clone & enter
git clone <repo-url> && cd API_MCP_Gateway

# Copy the env template and fill in real values
cp .env.example .env
$EDITOR .env

# Run with Docker Compose
docker compose up --build

# — or — run locally
source .env
cargo run -p gateway
```

The gateway listens on `PORT` (default `8080`).
Health check: `GET /health` → `{"status":"ok"}`

---

## Environment Variables & Secrets

All runtime configuration is read from **environment variables**.
Secret values are **never committed** and are **redacted in all log output**.

### Injection Methods

| Method | When to use |
|---|---|
| `.env` file + `docker compose up` | Local development |
| `source .env && cargo run` | Running outside Docker |
| Kubernetes Secrets / Vault / SSM | Production deployments |

### Setup

1. **Copy the template** — `.env.example` is committed with placeholder values:
   ```bash
   cp .env.example .env
   ```
2. **Fill in real secrets** — edit `.env` with your actual `AUTH_SECRET`,
   `DATABASE_URL`, etc.
3. **Never commit `.env`** — it's already in `.gitignore`.

### Docker Compose Wiring

`docker-compose.yml` loads your `.env` via the `env_file` directive:

```yaml
services:
  gateway:
    env_file:
      - .env
    environment:
      # Explicit overrides always win
      - RUST_LOG=gateway=debug,tower_http=debug
```

Variables from `.env` are injected into the container at runtime — they are
**not** baked into the image.


### Better Stack Observability (Gateway)

Gateway request logging is the source of truth for `api.greenhatsec.com`.

Required / optional env vars:

- `BETTERSTACK_ENABLED` (`true|false`, default `false`)
- `BETTERSTACK_SOURCE_TOKEN` (required when enabled)
- `BETTERSTACK_INGEST_HOST` (required when enabled, e.g. `in.logs.betterstack.com`)

Each request emits a structured log with this schema:

- `service`
- `route`
- `method`
- `status`
- `x_request_id`
- `latency_ms`
- `upstream_status` (when proxied)
- `upstream_latency_ms` (when proxied)
- `timeout_hit`
- `error_kind` (set for failures / error responses)

`x-request-id` is canonicalized at ingress (generated when missing), propagated to upstream requests, and echoed in gateway responses.

### Required vs Optional

See [docs/SECRETS.md](docs/SECRETS.md) for the full variable reference,
rotation checklist, and instructions for adding new secrets.

### What Happens If a Secret Is Missing?

The gateway validates all required env vars **at startup**. If any are
missing it logs every missing key and exits immediately — no half-configured
gateway ever serves traffic.

```
fatal: configuration error — exiting
  missing_keys: ["AUTH_SECRET", "DATABASE_URL"]
```

---

## Architecture

See [AGENTS.md](AGENTS.md) for architecture baselines, security
conventions, and development workflow.

## Documentation

- [docs/SECRETS.md](docs/SECRETS.md) — Secrets management & env var reference
- [docs/AUDIT_EVENTS.md](docs/AUDIT_EVENTS.md) — Audit event schema
- [docs/MCP_INTEGRATION.md](docs/MCP_INTEGRATION.md) — MCP runtime design
- [docs/MCP_HTTP_API.md](docs/MCP_HTTP_API.md) — HTTP API surface
- [docs/POLICY_SCHEMA.md](docs/POLICY_SCHEMA.md) — Policy engine schema
