// ---------------------------------------------------------------------------
// Gateway configuration — loaded from environment variables
// ---------------------------------------------------------------------------

use std::path::Path;

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

    // ── CORS ─────────────────────────────────────────────────────────────
    /// Allowed CORS origins (exact origin strings).
    pub cors_allow_origins: Vec<String>,

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigValidationError {
    pub field: &'static str,
    pub message: String,
}

impl ConfigValidationError {
    fn new(field: &'static str, message: impl Into<String>) -> Self {
        Self {
            field,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for ConfigValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.field, self.message)
    }
}

impl GatewayConfig {
    /// Build config from environment, falling back to sane defaults.
    pub fn from_env() -> Self {
        let cors_allow_origins = parse_cors_allow_origins(
            std::env::var("CORS_ALLOW_ORIGINS").ok(),
            vec![
                "https://tools.greenhatsec.com".to_owned(),
                "https://admin.greenhatsec.com".to_owned(),
            ],
        );

        Self {
            port: parse_env("PORT", 8080),
            // Keep backward compatibility with RATE_LIMIT_RPS/BURST, but allow
            // separate tuning for read vs write traffic.
            rate_limit_read_rps: parse_env(
                "RATE_LIMIT_READ_RPS",
                parse_env("RATE_LIMIT_RPS", 12.0),
            ),
            rate_limit_read_burst: parse_env(
                "RATE_LIMIT_READ_BURST",
                parse_env("RATE_LIMIT_BURST", 40),
            ),
            rate_limit_write_rps: parse_env(
                "RATE_LIMIT_WRITE_RPS",
                parse_env("RATE_LIMIT_RPS", 8.0),
            ),
            rate_limit_write_burst: parse_env(
                "RATE_LIMIT_WRITE_BURST",
                parse_env("RATE_LIMIT_BURST", 20),
            ),
            max_body_size: parse_env("MAX_BODY_SIZE_BYTES", 1_048_576), // 1 MiB

            cors_allow_origins,

            csrf_enabled: parse_env("CSRF_ENABLED", true),
            csrf_cookie_name: parse_env("CSRF_COOKIE_NAME", "csrf_token".to_owned()),
            csrf_header_name: parse_env("CSRF_HEADER_NAME", "x-csrf-token".to_owned()),
            csrf_cookie_domain: std::env::var("CSRF_COOKIE_DOMAIN")
                .ok()
                .map(|v| v.trim().to_owned())
                .filter(|v| !v.is_empty()),

            policy_file: std::env::var("POLICY_FILE")
                .ok()
                .map(|v| v.trim().to_owned())
                .filter(|v| !v.is_empty()),

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

    /// Build config and validate startup invariants.
    pub fn from_env_validated() -> Result<Self, Vec<ConfigValidationError>> {
        let cfg = Self::from_env();
        cfg.validate().map(|_| cfg)
    }

    /// Validate startup invariants. Returns all failures to improve diagnostics.
    pub fn validate(&self) -> Result<(), Vec<ConfigValidationError>> {
        let mut errors = Vec::new();

        // URL checks
        validate_origin_url(
            "BETTERAUTH_BASE_URL",
            &self.betterauth_base_url,
            &mut errors,
        );
        validate_origin_url(
            "PROXY_UPSTREAM_BASE_URL",
            &self.proxy_upstream_base_url,
            &mut errors,
        );

        if self.csrf_cookie_name.trim().is_empty() {
            errors.push(ConfigValidationError::new(
                "CSRF_COOKIE_NAME",
                "must not be empty",
            ));
        }
        if self.betterauth_cookie_name.trim().is_empty() {
            errors.push(ConfigValidationError::new(
                "BETTERAUTH_COOKIE_NAME",
                "must not be empty",
            ));
        }

        // CORS origin sanity
        if self.cors_allow_origins.is_empty() {
            errors.push(ConfigValidationError::new(
                "CORS_ALLOW_ORIGINS",
                "must contain at least one origin",
            ));
        } else {
            for origin in &self.cors_allow_origins {
                validate_origin_url("CORS_ALLOW_ORIGINS", origin, &mut errors);
            }
        }

        // Numeric bounds (wide enough to stay backward compatible, strict enough to catch nonsense)
        if self.port == 0 {
            errors.push(ConfigValidationError::new(
                "PORT",
                "must be between 1 and 65535",
            ));
        }

        validate_rps("RATE_LIMIT_READ_RPS", self.rate_limit_read_rps, &mut errors);
        validate_rps(
            "RATE_LIMIT_WRITE_RPS",
            self.rate_limit_write_rps,
            &mut errors,
        );

        if self.rate_limit_read_burst == 0 {
            errors.push(ConfigValidationError::new(
                "RATE_LIMIT_READ_BURST",
                "must be > 0",
            ));
        }
        if self.rate_limit_write_burst == 0 {
            errors.push(ConfigValidationError::new(
                "RATE_LIMIT_WRITE_BURST",
                "must be > 0",
            ));
        }

        // 1 KiB .. 100 MiB
        if !(1_024..=104_857_600).contains(&self.max_body_size) {
            errors.push(ConfigValidationError::new(
                "MAX_BODY_SIZE_BYTES",
                "must be between 1024 and 104857600 bytes",
            ));
        }

        if let Some(policy_file) = self.policy_file.as_ref() {
            let path = Path::new(policy_file);
            if !path.exists() {
                errors.push(ConfigValidationError::new(
                    "POLICY_FILE",
                    format!("file does not exist: {}", path.display()),
                ));
            } else if !path.is_file() {
                errors.push(ConfigValidationError::new(
                    "POLICY_FILE",
                    format!("path is not a file: {}", path.display()),
                ));
            } else if let Err(e) = std::fs::File::open(path) {
                errors.push(ConfigValidationError::new(
                    "POLICY_FILE",
                    format!("file is not readable: {} ({e})", path.display()),
                ));
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

fn validate_rps(field: &'static str, value: f64, errors: &mut Vec<ConfigValidationError>) {
    if !value.is_finite() || value <= 0.0 || value > 10_000.0 {
        errors.push(ConfigValidationError::new(
            field,
            "must be > 0 and <= 10000",
        ));
    }
}

fn validate_origin_url(field: &'static str, raw: &str, errors: &mut Vec<ConfigValidationError>) {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        errors.push(ConfigValidationError::new(field, "must not be empty"));
        return;
    }

    match url::Url::parse(trimmed) {
        Ok(url) => {
            if url.scheme() != "http" && url.scheme() != "https" {
                errors.push(ConfigValidationError::new(
                    field,
                    format!(
                        "must use http:// or https:// (got scheme '{}')",
                        url.scheme()
                    ),
                ));
            }
            if url.host_str().is_none() {
                errors.push(ConfigValidationError::new(field, "must include a host"));
            }
        }
        Err(e) => errors.push(ConfigValidationError::new(
            field,
            format!("must be a valid URL ({e})"),
        )),
    }
}

fn parse_cors_allow_origins(raw: Option<String>, default: Vec<String>) -> Vec<String> {
    match raw {
        Some(v) => {
            let parsed: Vec<String> = v
                .split(',')
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .map(ToOwned::to_owned)
                .collect();
            if parsed.is_empty() {
                default
            } else {
                parsed
            }
        }
        None => default,
    }
}

fn parse_env<T: std::str::FromStr>(key: &str, default: T) -> T {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_config() -> GatewayConfig {
        GatewayConfig {
            port: 8080,
            rate_limit_read_rps: 12.0,
            rate_limit_read_burst: 40,
            rate_limit_write_rps: 8.0,
            rate_limit_write_burst: 20,
            max_body_size: 1_048_576,
            cors_allow_origins: vec!["https://tools.greenhatsec.com".to_owned()],
            csrf_enabled: true,
            csrf_cookie_name: "csrf_token".to_owned(),
            csrf_header_name: "x-csrf-token".to_owned(),
            csrf_cookie_domain: None,
            policy_file: None,
            auth_enabled: true,
            betterauth_base_url: "http://localhost:3000".to_owned(),
            proxy_upstream_base_url: "http://localhost:3000".to_owned(),
            betterauth_cookie_name: "better-auth.session_token".to_owned(),
            betterauth_timeout_ms: 2_000,
        }
    }

    #[test]
    fn accepts_valid_config() {
        let cfg = base_config();
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn rejects_invalid_urls_and_empty_cookie_names() {
        let mut cfg = base_config();
        cfg.betterauth_base_url = "not-a-url".to_owned();
        cfg.proxy_upstream_base_url = "ftp://localhost".to_owned();
        cfg.csrf_cookie_name = "   ".to_owned();
        cfg.betterauth_cookie_name = "".to_owned();

        let err = cfg.validate().unwrap_err();
        let rendered = err
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n");

        assert!(rendered.contains("BETTERAUTH_BASE_URL"));
        assert!(rendered.contains("PROXY_UPSTREAM_BASE_URL"));
        assert!(rendered.contains("CSRF_COOKIE_NAME"));
        assert!(rendered.contains("BETTERAUTH_COOKIE_NAME"));
    }

    #[test]
    fn rejects_bad_limits() {
        let mut cfg = base_config();
        cfg.port = 0;
        cfg.rate_limit_read_rps = 0.0;
        cfg.rate_limit_write_rps = f64::NAN;
        cfg.rate_limit_read_burst = 0;
        cfg.rate_limit_write_burst = 0;
        cfg.max_body_size = 32;

        let err = cfg.validate().unwrap_err();
        let rendered = err
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n");

        assert!(rendered.contains("PORT"));
        assert!(rendered.contains("RATE_LIMIT_READ_RPS"));
        assert!(rendered.contains("RATE_LIMIT_WRITE_RPS"));
        assert!(rendered.contains("RATE_LIMIT_READ_BURST"));
        assert!(rendered.contains("RATE_LIMIT_WRITE_BURST"));
        assert!(rendered.contains("MAX_BODY_SIZE_BYTES"));
    }

    #[test]
    fn rejects_invalid_cors_origins() {
        let mut cfg = base_config();
        cfg.cors_allow_origins = vec!["ftp://example.com".to_owned(), " ".to_owned()];

        let err = cfg.validate().unwrap_err();
        let rendered = err
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n");

        assert!(rendered.contains("CORS_ALLOW_ORIGINS"));
    }

    #[test]
    fn parse_cors_allow_origins_fallbacks_to_default_when_empty() {
        let defaults = vec!["https://a.example".to_owned()];
        assert_eq!(
            parse_cors_allow_origins(Some(" , ".to_owned()), defaults.clone()),
            defaults
        );
    }
}
