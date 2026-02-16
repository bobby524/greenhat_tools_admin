# Redis Cache Layer

Optional Redis-backed cache for the API MCP Gateway.  
Used for session tokens, permission lookups, and other hot data that benefits
from sub-millisecond reads.

---

## Quick Start

```bash
# Start everything (gateway + Redis)
docker compose up -d

# Or just Redis for local dev
docker compose up -d redis
export REDIS_URL=redis://127.0.0.1:6379
cargo run -p gateway
```

## Feature Flag

The cache module is gated behind the **`redis-cache`** Cargo feature (enabled
by default):

```bash
# Build WITH Redis support (default)
cargo build -p gateway

# Build WITHOUT Redis support
cargo build -p gateway --no-default-features
```

When compiled **with** the feature, the gateway inspects `REDIS_URL` at
startup:

| `REDIS_URL`         | Behaviour                              |
| ------------------- | -------------------------------------- |
| Set (e.g. `redis://…`) | Pool created, cache helpers available |
| Absent / empty      | Cache disabled, gateway runs normally  |

When compiled **without** the feature, the `cache` module is not included at
all — zero overhead.

## Environment Variables

| Variable    | Required | Default | Description                              |
| ----------- | -------- | ------- | ---------------------------------------- |
| `REDIS_URL` | No       | —       | Redis connection string. Omit to disable |

## Docker Compose

The `docker-compose.yml` provisions a Redis 7 Alpine container with:

- **Append-only persistence** (`appendonly yes`)
- **128 MB memory cap** with LRU eviction (`allkeys-lru`)
- Health-checked via `redis-cli ping`
- Named volume `redis-data` for data durability across restarts

The gateway service depends on Redis being healthy before starting.

## Cache Module API

The `Cache` struct lives in `gateway/src/cache.rs` and exposes:

```rust
impl Cache {
    // Construction
    fn from_env() -> Option<Self>;           // reads REDIS_URL
    fn new(redis_url: &str) -> Result<Self>; // explicit URL

    // Key/value helpers
    async fn get(&self, key: &str) -> Result<Option<String>>;
    async fn set(&self, key: &str, value: &str, ttl: Duration) -> Result<()>;
    async fn del(&self, key: &str) -> Result<u64>;
    async fn exists(&self, key: &str) -> Result<bool>;
    async fn ttl(&self, key: &str) -> Result<Option<i64>>;
    async fn ping(&self) -> Result<()>;
}
```

All methods are `&self` — the handle is cheaply cloneable and safe to share
across tasks via `Arc<AppState>`.

### Planned Key Namespaces

| Prefix          | TTL     | Purpose                         |
| --------------- | ------- | ------------------------------- |
| `sess:<id>`     | 1 hour  | Session data / JWT claims cache |
| `perm:<sub>`    | 5 min   | Permission/policy lookup cache  |
| `rl:<ip>`       | sliding | Rate-limit counters             |

## Connection Pool

Uses [`deadpool-redis`](https://crates.io/crates/deadpool-redis) with Tokio
runtime. The pool is created once at startup and shared through `AppState`.

Default pool settings (deadpool defaults):

| Setting         | Value |
| --------------- | ----- |
| Max connections | 16    |
| Wait timeout    | 30 s  |

Override by building the pool programmatically if needed.

## Health Check Integration

Future: the `/health` endpoint should include a `cache` field:

```json
{
  "status": "ok",
  "cache": "connected"   // or "disabled" / "degraded"
}
```

Use `cache.ping()` to verify connectivity.

## Local Development (no Docker)

```bash
# Install Redis via Homebrew
brew install redis
brew services start redis

# Point gateway at local Redis
export REDIS_URL=redis://127.0.0.1:6379
export AUTH_SECRET=dev-secret
export DATABASE_URL=postgres://localhost/gateway_dev
cargo run -p gateway
```

Or simply omit `REDIS_URL` to run without caching.
