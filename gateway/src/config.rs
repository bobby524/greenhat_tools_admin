// ---------------------------------------------------------------------------
// Gateway configuration — loaded from environment variables
// ---------------------------------------------------------------------------

/// Central configuration for the gateway, populated from env vars with defaults.
#[derive(Debug, Clone)]
pub struct GatewayConfig {
    /// TCP port to listen on.
    pub port: u16,
    /// Sustained requests-per-second allowed for read routes (GET/HEAD).
    pub rate_limit_read_rps: f64,
    /// Burst capacity for read routes.
    pub rate_limit_read_burst: u32,
    /// Sustained requests-per-second allowed for write routes.
    pub rate_limit_write_rps: f64,
    /// Burst capacity for write routes.
    pub rate_limit_write_burst: u32,
    /// Maximum allowed request body size in bytes.
    pub max_body_size: usize,

    // ── CSRF ─────────────────────────────────────────────────────────────
    /// Enable / disable double-submit cookie CSRF protection.
    pub csrf_enabled: bool,
    /// Cookie name for the CSRF token (must be JS-readable, NOT HttpOnly).
    pub csrf_cookie_name: String,
    /// Header name the SPA must echo the token into.
    pub csrf_header_name: String,
    /// Optional CSRF cookie domain (e.g. `.greenhatsec.com`).
    pub csrf_cookie_domain: Option<String>,

    // ── RBAC / Policy ────────────────────────────────────────────────────
    /// Path to a policy JSON file (see `docs/schemas/policy.v0.schema.json`).
    /// When set, the RBAC middleware is enabled and enforces deny-by-default.
    pub policy_file: Option<String>,

    // ── Auth (BetterAuth) ───────────────────────────────────────────────
    /// Master switch for authentication middleware.
    pub auth_enabled: bool,
    /// BetterAuth base URL (e.g. `http://localhost:3000` for tools app).
    pub betterauth_base_url: String,
    /// Upstream API base URL for /api/* proxy routes.
    /// Keep this separate from BetterAuth to avoid self-referential proxy loops.
    pub proxy_upstream_base_url: String,
    /// BetterAuth session cookie name.
    pub betterauth_cookie_name: String,
    /// Upstream timeout in milliseconds for BetterAuth validation calls.
    pub betterauth_timeout_ms: u64,
}

impl GatewayConfig {
    /// Build config from environment, falling back to sane defaults.
    pub fn from_env() -> Self {
        Self {
            port: parse_env("PORT", 8080),
            // Keep backward compatibility with RATE_LIMIT_RPS/BURST, but allow
            // separate tuning for read vs write traffic.
            rate_limit_read_rps: parse_env("RATE_LIMIT_READ_RPS", parse_env("RATE_LIMIT_RPS", 12.0)),
            rate_limit_read_burst: parse_env("RATE_LIMIT_READ_BURST", parse_env("RATE_LIMIT_BURST", 40)),
            rate_limit_write_rps: parse_env("RATE_LIMIT_WRITE_RPS", parse_env("RATE_LIMIT_RPS", 8.0)),
            rate_limit_write_burst: parse_env("RATE_LIMIT_WRITE_BURST", parse_env("RATE_LIMIT_BURST", 20)),
            max_body_size: parse_env("MAX_BODY_SIZE_BYTES", 1_048_576), // 1 MiB

            csrf_enabled: parse_env("CSRF_ENABLED", true),
            csrf_cookie_name: parse_env("CSRF_COOKIE_NAME", "csrf_token".to_owned()),
            csrf_header_name: parse_env("CSRF_HEADER_NAME", "x-csrf-token".to_owned()),
            csrf_cookie_domain: std::env::var("CSRF_COOKIE_DOMAIN")
                .ok()
                .map(|v| v.trim().to_owned())
                .filter(|v| !v.is_empty()),

            policy_file: std::env::var("POLICY_FILE").ok(),

            auth_enabled: parse_env("AUTH_ENABLED", true),
            betterauth_base_url: parse_env(
                "BETTERAUTH_BASE_URL",
                "http://localhost:3000".to_owned(),
            ),
            proxy_upstream_base_url: parse_env(
                "PROXY_UPSTREAM_BASE_URL",
                parse_env("BETTERAUTH_BASE_URL", "http://localhost:3000".to_owned()),
            ),
            betterauth_cookie_name: parse_env(
                "BETTERAUTH_COOKIE_NAME",
                "better-auth.session_token".to_owned(),
            ),
            betterauth_timeout_ms: parse_env("BETTERAUTH_TIMEOUT_MS", 2_000u64),
        }
    }
}

fn parse_env<T: std::str::FromStr>(key: &str, default: T) -> T {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}
