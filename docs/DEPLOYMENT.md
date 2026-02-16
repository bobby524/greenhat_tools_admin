# Deployment Guide

## Overview

`api-mcp-gateway` ships as a single static binary inside a minimal
Debian-based Docker image.  CI builds on every push to `main` and publishes
the image to **GitHub Container Registry (GHCR)**.

### Image coordinates

```
ghcr.io/<owner>/api-mcp-gateway:<tag>
```

| Tag format    | Meaning                              |
|---------------|--------------------------------------|
| `sha-abc1234` | Immutable — tied to a specific commit |
| `main`        | Rolling — latest successful `main` build |
| `latest`      | Alias for `main` (convenience)       |

> **Rule of thumb:** staging pulls `main`; production pins `sha-<commit>`.

---

## Environments

### Staging

| Property       | Value                                             |
|----------------|---------------------------------------------------|
| Image tag      | `main` (auto-deploys on merge)                    |
| Replicas       | 1                                                 |
| Health check   | `GET /health` → `{"status":"ok"}`                 |
| Purpose        | Integration testing, smoke tests, demo            |

Staging can be a single `docker compose` host, a Fly.io machine, or a
Kubernetes namespace — whatever is cheapest.  The important thing is that it
**always runs the latest `main` image** so regressions surface immediately.

#### Quick local staging with Compose

```bash
docker compose up --build -d
curl http://localhost:8080/health   # {"status":"ok"}
curl http://localhost:8080/version  # {"service":"api-mcp-gateway","version":"0.1.0"}
```

### Production

| Property       | Value                                            |
|----------------|--------------------------------------------------|
| Image tag      | `sha-<commit>` (explicit promotion)             |
| Replicas       | ≥ 2 (behind load balancer)                       |
| Health check   | Same `/health` endpoint                          |
| TLS            | Terminated at LB / reverse proxy                 |

Production images are **never `latest`** — always pin to a specific SHA tag
that has been validated in staging.

---

## Promotion flow

```
  PR ──► merge to main ──► CI builds image ──► staging auto-deploys (tag: main)
                                                       │
                                              manual validation
                                                       │
                                              promote sha tag ──► production
```

### Step-by-step

1. **Merge PR** — CI runs fmt → clippy → test → Docker build → push to GHCR.
2. **Staging auto-deploys** — a webhook / cron / watcher pulls `main` tag.
   (Can be a simple `docker compose pull && docker compose up -d` cron.)
3. **Validate staging** — smoke tests, integration tests, manual QA.
4. **Promote to prod** — update the production deploy config to the new
   `sha-<commit>` tag and roll out.

---

## Canary / rolling deploys

Even without Kubernetes, a basic canary is straightforward:

### Manual canary (Docker Compose / single-host)

1. **Start canary instance** on a different port:
   ```bash
   docker run -d --name gateway-canary \
     -p 8081:8080 \
     -e PORT=8080 \
     ghcr.io/<owner>/api-mcp-gateway:sha-<new>
   ```
2. **Route a fraction of traffic** (10 %) to `:8081` via nginx/Caddy
   upstream weights or manual DNS split.
3. **Monitor** — check logs, error rates, latency for 15–30 min.
4. **If healthy:** update the primary container to the new SHA, remove canary.
5. **If unhealthy:** kill canary, keep primary on the old SHA.

### Kubernetes (future)

When the project moves to K8s, the natural path is:

- **Deployment** with `maxSurge: 1, maxUnavailable: 0` for zero-downtime
  rolling updates.
- Or a dedicated **canary Deployment** (10 % replica weight) managed by
  Argo Rollouts / Flagger.

---

## Rollback

### Instant rollback

Because every build produces an immutable `sha-<commit>` tag, rollback is:

```bash
# Point prod back to the last known-good SHA
docker pull ghcr.io/<owner>/api-mcp-gateway:sha-<old>
docker compose up -d            # or update K8s manifest
```

No rebuild required — the old image is still in GHCR.

### Checklist before rolling back

- [ ] Confirm the regression is in the gateway (not downstream services).
- [ ] Note the bad SHA for post-mortem.
- [ ] After rollback, verify `/health` and `/version` return expected values.

---

## Configuration

The gateway reads configuration from **environment variables**:

| Variable    | Default                                 | Description                   |
|-------------|-----------------------------------------|-------------------------------|
| `PORT`      | `8080`                                  | HTTP listen port              |
| `RUST_LOG`  | `gateway=debug,tower_http=debug`        | Tracing filter                |

> Future: secrets (DB credentials, API keys) will come from a secrets
> manager (e.g., GH Actions secrets → Docker env, or Vault / AWS SSM in
> production).  **Never commit secrets to the repo.**

---

## Secrets in CI

The GitHub Actions workflow uses these secrets:

| Secret              | Source                        | Purpose                     |
|---------------------|-------------------------------|-----------------------------|
| `GITHUB_TOKEN`      | Auto-provided by GH Actions  | Push images to GHCR         |

No additional secrets are required today.  When external services are added
(database, auth provider, etc.), store credentials as **repository secrets**
in GitHub → Settings → Secrets and reference them as
`${{ secrets.SECRET_NAME }}` in the workflow.

---

## Health checks

| Endpoint    | Method | Expected response          |
|-------------|--------|----------------------------|
| `/health`   | GET    | `200 {"status":"ok"}`      |
| `/version`  | GET    | `200 {"service":…,"version":…}` |

Use `/health` for load-balancer probes and container orchestrator liveness
checks.  `/version` is useful for verifying which build is actually running
after a deploy.

---

## Monitoring (future)

Planned additions:

- **Prometheus metrics** endpoint (`/metrics`) — request count, latency
  histograms, error rates.
- **Structured JSON logs** — already enabled (`tracing-subscriber` JSON
  formatter).  Ship to Loki / CloudWatch / Datadog.
- **Alerting** — PagerDuty / Slack webhook on error rate spike or health
  check failure.
