# Secrets Management

> **Prime directive:** No secrets in git. No credentials in logs. No keys in chat output.
> — AGENTS.md §1

## Overview

The gateway reads all runtime configuration from **environment variables**.
Secret values are redacted automatically whenever the config is logged or
printed via `Debug`.

## Quick Start

```bash
# 1. Create your local .env from the committed template
cp .env.example .env

# 2. Fill in real values
$EDITOR .env

# 3. Start the gateway (Docker)
docker compose up

# 4. — or — start locally (reads .env via your shell / direnv)
source .env && cargo run -p gateway
```

## Required Variables

| Variable        | Secret? | Description                             |
| --------------- | ------- | --------------------------------------- |
| `AUTH_SECRET`   | **yes** | BetterAuth session / JWT signing key    |
| `DATABASE_URL`  | **yes** | Postgres connection string              |

The gateway **refuses to start** if any required variable is missing or empty
and exits with a clear error listing every missing key.

## Optional Variables

| Variable                       | Default                          | Description                     |
| ------------------------------ | -------------------------------- | ------------------------------- |
| `PORT`                         | `8080`                           | Listen port                     |
| `RUST_LOG`                     | `gateway=debug,tower_http=debug` | tracing filter                  |
| `MCP_ALLOWED_ORIGINS`          | *(empty — none)*                 | Comma-separated CORS origins    |
| `RATE_LIMIT_RPS`               | `50`                             | Per-caller requests per second  |
| `OTEL_EXPORTER_OTLP_ENDPOINT`  | *(disabled)*                     | OpenTelemetry collector URL     |

## How Secrets Stay Safe

### 1. `.env` is git-ignored

`.gitignore` contains `.env`. Only `.env.example` (no real values) is
committed.

### 2. Config loader redacts secrets in logs

`config::AppConfig` implements a custom `Debug` that prints `[REDACTED]` for
any field backed by a key in the `SECRET_KEYS` list. The `log_summary()`
method does the same via structured tracing fields.

```
"auth_secret":"[REDACTED]","database_url":"[REDACTED]"
```

### 3. `AppError::MissingEnv` — fail-fast, fail-safe

If required vars are missing the gateway emits:

```
fatal: configuration error — exiting
  missing_keys: ["AUTH_SECRET", "DATABASE_URL"]
```

…and exits **before** binding any port. This prevents a half-configured
gateway from accepting traffic.

### 4. Docker Compose env wiring

`docker-compose.yml` uses `env_file: [.env]` so secrets flow from the local
`.env` into the container **without baking them into the image**.

For production, replace the `.env` file with your orchestrator's secret
injection (Kubernetes Secrets, AWS SSM, Vault, etc.).

## Rotation Checklist

1. Generate a new value for the key being rotated.
2. Update `.env` (local) or your secret store (prod).
3. Restart the gateway — the new value takes effect immediately.
4. Verify via `/health` + audit logs that the gateway is serving.
5. Revoke the old value.

## Adding a New Secret

1. Add the key to `.env.example` with a placeholder value.
2. Add the key to `SECRET_KEYS` in `gateway/src/config.rs`.
3. If required, add it to `REQUIRED_KEYS` as well.
4. Add a field to `AppConfig` and load it in `from_env()`.
5. Update the `Debug` impl to redact the new field.
6. Update this doc's table above.

## CI / Production Notes

- **Never** pass secrets as build args (`--build-arg`).
  They persist in image layers.
- Prefer mounted secret files or orchestrator-injected env vars.
- In CI, use your platform's encrypted secrets (GitHub Actions secrets,
  GitLab CI variables, etc.) and expose them as env vars at runtime only.
- Audit the gateway's startup log to confirm `[REDACTED]` appears for
  every secret field.
