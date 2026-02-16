//! Optional Redis-backed cache layer.
//!
//! Compiled only when the `redis-cache` Cargo feature is enabled.
//! At runtime the pool is initialised **only** when `REDIS_URL` is set,
//! so the gateway can always start without Redis.
//!
//! # Usage (future)
//!
//! ```ignore
//! use std::time::Duration;
//!
//! // In an Axum handler that has State<AppState>:
//! if let Some(cache) = &state.cache {
//!     cache.set("sess:abc", r#"{"user":"bob"}"#, Duration::from_secs(3600)).await?;
//!     let val = cache.get("sess:abc").await?;
//! }
//! ```

use deadpool_redis::{Config, Connection, Pool, Runtime};
use std::time::Duration;
use tracing::{debug, info};

// Re-export so callers don't need to depend on deadpool-redis directly.
pub use deadpool_redis::redis::RedisError;

// ---------------------------------------------------------------------------
// Error
// ---------------------------------------------------------------------------

/// Lightweight cache error — wraps pool & redis errors.
#[derive(Debug)]
pub enum CacheError {
    Pool(deadpool_redis::PoolError),
    Redis(deadpool_redis::redis::RedisError),
}

impl std::fmt::Display for CacheError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CacheError::Pool(e) => write!(f, "cache pool error: {e}"),
            CacheError::Redis(e) => write!(f, "redis error: {e}"),
        }
    }
}

impl std::error::Error for CacheError {}

impl From<deadpool_redis::PoolError> for CacheError {
    fn from(e: deadpool_redis::PoolError) -> Self {
        CacheError::Pool(e)
    }
}

impl From<deadpool_redis::redis::RedisError> for CacheError {
    fn from(e: deadpool_redis::redis::RedisError) -> Self {
        CacheError::Redis(e)
    }
}

// ---------------------------------------------------------------------------
// Cache handle
// ---------------------------------------------------------------------------

/// Thin wrapper around a `deadpool_redis::Pool`.
///
/// All public helpers are intentionally `&self` so the handle can live inside
/// shared `Arc<AppState>`.
#[derive(Clone)]
pub struct Cache {
    pool: Pool,
}

impl Cache {
    // -- Construction -------------------------------------------------------

    /// Try to build a cache pool from the `REDIS_URL` env var.
    ///
    /// Returns `None` (with a log line) when the var is absent, so the
    /// gateway can run without Redis.
    pub fn from_env() -> Option<Self> {
        let url = match std::env::var("REDIS_URL") {
            Ok(u) if !u.is_empty() => u,
            _ => {
                info!("REDIS_URL not set — cache layer disabled");
                return None;
            }
        };
        Self::new(&url).ok()
    }

    /// Build a pool from an explicit URL (e.g. `redis://127.0.0.1:6379`).
    pub fn new(redis_url: &str) -> Result<Self, CacheError> {
        let cfg = Config::from_url(redis_url);
        let pool = cfg.create_pool(Some(Runtime::Tokio1)).map_err(|e| {
            CacheError::Redis(deadpool_redis::redis::RedisError::from(
                std::io::Error::new(std::io::ErrorKind::Other, e.to_string()),
            ))
        })?;
        info!(url = %redis_url, "redis cache pool created");
        Ok(Cache { pool })
    }

    // -- Helpers ------------------------------------------------------------

    /// Obtain a connection from the pool.
    async fn conn(&self) -> Result<Connection, CacheError> {
        Ok(self.pool.get().await?)
    }

    /// `GET key` — returns `None` on cache miss.
    pub async fn get(&self, key: &str) -> Result<Option<String>, CacheError> {
        let mut conn = self.conn().await?;
        let val: Option<String> = deadpool_redis::redis::cmd("GET")
            .arg(key)
            .query_async(&mut conn)
            .await?;
        debug!(key, hit = val.is_some(), "cache GET");
        Ok(val)
    }

    /// `SET key value EX ttl_secs`.
    pub async fn set(&self, key: &str, value: &str, ttl: Duration) -> Result<(), CacheError> {
        let mut conn = self.conn().await?;
        deadpool_redis::redis::cmd("SET")
            .arg(key)
            .arg(value)
            .arg("EX")
            .arg(ttl.as_secs())
            .query_async::<()>(&mut conn)
            .await?;
        debug!(key, ttl_secs = ttl.as_secs(), "cache SET");
        Ok(())
    }

    /// `DEL key` — returns the number of keys removed.
    pub async fn del(&self, key: &str) -> Result<u64, CacheError> {
        let mut conn = self.conn().await?;
        let removed: u64 = deadpool_redis::redis::cmd("DEL")
            .arg(key)
            .query_async(&mut conn)
            .await?;
        debug!(key, removed, "cache DEL");
        Ok(removed)
    }

    /// `EXISTS key`.
    pub async fn exists(&self, key: &str) -> Result<bool, CacheError> {
        let mut conn = self.conn().await?;
        let exists: bool = deadpool_redis::redis::cmd("EXISTS")
            .arg(key)
            .query_async(&mut conn)
            .await?;
        Ok(exists)
    }

    /// `TTL key` — returns remaining seconds (`None` if key missing or no expiry).
    pub async fn ttl(&self, key: &str) -> Result<Option<i64>, CacheError> {
        let mut conn = self.conn().await?;
        let secs: i64 = deadpool_redis::redis::cmd("TTL")
            .arg(key)
            .query_async(&mut conn)
            .await?;
        // Redis returns -2 (key missing) or -1 (no expiry)
        Ok(if secs >= 0 { Some(secs) } else { None })
    }

    /// Ping Redis — useful for health checks.
    pub async fn ping(&self) -> Result<(), CacheError> {
        let mut conn = self.conn().await?;
        deadpool_redis::redis::cmd("PING")
            .query_async::<()>(&mut conn)
            .await?;
        Ok(())
    }
}
