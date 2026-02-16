//! Egress firewall — hardened outbound HTTP client for tool execution.
//!
//! Every outbound HTTP request made on behalf of a tool call **must** go through
//! [`EgressClient`].  It enforces:
//!
//! - **Host allowlist** — only pre-approved hosts may be contacted.
//! - **Private-IP denial** — resolved IPs in RFC 1918/6598/loopback/link-local
//!   ranges are blocked to prevent SSRF.
//! - **Timeouts** — per-request connect + total timeout.
//! - **Response size cap** — streaming reads are aborted once the limit is hit.
//! - **Request body size cap** — callers cannot send oversized payloads.

use std::collections::HashSet;
use std::net::IpAddr;
use std::time::Duration;

use bytes::Bytes;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use reqwest::{Client, Method, RequestBuilder, Response};
use tracing::{debug, warn};
use url::Url;

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Everything that can go wrong with an egress request.
#[derive(Debug)]
pub enum EgressError {
    /// The target host is not in the configured allowlist.
    HostNotAllowed(String),
    /// DNS resolved to a private / loopback / link-local address.
    PrivateIpBlocked(IpAddr),
    /// DNS resolution returned no addresses.
    DnsResolutionFailed(String),
    /// The URL could not be parsed or has no host.
    InvalidUrl(String),
    /// The request body exceeds [`EgressConfig::max_request_body_bytes`].
    RequestBodyTooLarge { size: usize, max: usize },
    /// The response body exceeds [`EgressConfig::max_response_bytes`].
    ResponseTooLarge { max: usize },
    /// A timeout or transport-level error from `reqwest`.
    Http(reqwest::Error),
    /// An I/O error during streaming reads.
    Io(std::io::Error),
}

impl std::fmt::Display for EgressError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::HostNotAllowed(h) => write!(f, "egress denied: host {h:?} not in allowlist"),
            Self::PrivateIpBlocked(ip) => {
                write!(f, "egress denied: resolved IP {ip} is in a private range")
            }
            Self::DnsResolutionFailed(h) => {
                write!(f, "egress denied: DNS resolution failed for {h}")
            }
            Self::InvalidUrl(msg) => write!(f, "egress denied: invalid URL — {msg}"),
            Self::RequestBodyTooLarge { size, max } => {
                write!(
                    f,
                    "egress denied: request body {size} bytes exceeds max {max}"
                )
            }
            Self::ResponseTooLarge { max } => {
                write!(f, "egress denied: response body exceeds max {max} bytes")
            }
            Self::Http(e) => write!(f, "egress HTTP error: {e}"),
            Self::Io(e) => write!(f, "egress I/O error: {e}"),
        }
    }
}

impl std::error::Error for EgressError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Http(e) => Some(e),
            Self::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<reqwest::Error> for EgressError {
    fn from(e: reqwest::Error) -> Self {
        Self::Http(e)
    }
}

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Tuning knobs for the egress firewall.  Loaded from env vars via
/// [`EgressConfig::from_env`].
#[derive(Debug, Clone)]
pub struct EgressConfig {
    /// Set of allowed hostnames (lowercase, no port).
    /// If empty, **all** hosts are denied (fail-closed).
    pub allowed_hosts: HashSet<String>,
    /// Per-request timeout (connect + transfer).
    pub timeout: Duration,
    /// Connect-phase timeout.
    pub connect_timeout: Duration,
    /// Maximum response body size in bytes (streamed check).
    pub max_response_bytes: usize,
    /// Maximum request body size in bytes (pre-flight check).
    pub max_request_body_bytes: usize,
    /// When `true`, private/loopback/link-local IPs are blocked even if the
    /// host is in the allowlist.  Default: `true`.
    pub deny_private_ips: bool,
}

impl Default for EgressConfig {
    fn default() -> Self {
        Self {
            allowed_hosts: HashSet::new(),
            timeout: Duration::from_secs(30),
            connect_timeout: Duration::from_secs(10),
            max_response_bytes: 5 * 1024 * 1024,     // 5 MiB
            max_request_body_bytes: 1 * 1024 * 1024, // 1 MiB
            deny_private_ips: true,
        }
    }
}

impl EgressConfig {
    /// Build config from environment variables.
    ///
    /// | Env var | Default | Description |
    /// |---|---|---|
    /// | `EGRESS_ALLOWED_HOSTS` | *(empty → deny all)* | Comma-separated hostnames |
    /// | `EGRESS_TIMEOUT_SECS` | `30` | Total request timeout |
    /// | `EGRESS_CONNECT_TIMEOUT_SECS` | `10` | TCP connect timeout |
    /// | `EGRESS_MAX_RESPONSE_BYTES` | `5242880` (5 MiB) | Max response body |
    /// | `EGRESS_MAX_REQUEST_BODY_BYTES` | `1048576` (1 MiB) | Max request body |
    /// | `EGRESS_DENY_PRIVATE_IPS` | `true` | Block RFC 1918 / loopback |
    pub fn from_env() -> Self {
        let allowed_hosts: HashSet<String> = std::env::var("EGRESS_ALLOWED_HOSTS")
            .unwrap_or_default()
            .split(',')
            .map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty())
            .collect();

        Self {
            allowed_hosts,
            timeout: Duration::from_secs(parse_env("EGRESS_TIMEOUT_SECS", 30)),
            connect_timeout: Duration::from_secs(parse_env("EGRESS_CONNECT_TIMEOUT_SECS", 10)),
            max_response_bytes: parse_env("EGRESS_MAX_RESPONSE_BYTES", 5 * 1024 * 1024),
            max_request_body_bytes: parse_env("EGRESS_MAX_REQUEST_BODY_BYTES", 1024 * 1024),
            deny_private_ips: parse_env("EGRESS_DENY_PRIVATE_IPS", true),
        }
    }
}

fn parse_env<T: std::str::FromStr>(key: &str, default: T) -> T {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

// ---------------------------------------------------------------------------
// Private-IP detection
// ---------------------------------------------------------------------------

/// Returns `true` if `ip` falls in a range that should never be reached from
/// tool-initiated outbound requests.
///
/// Blocked ranges:
///  - `127.0.0.0/8`   — IPv4 loopback
///  - `10.0.0.0/8`    — RFC 1918
///  - `172.16.0.0/12` — RFC 1918
///  - `192.168.0.0/16` — RFC 1918
///  - `100.64.0.0/10` — RFC 6598 (CGNAT / shared)
///  - `169.254.0.0/16` — link-local
///  - `0.0.0.0/8`     — "this" network
///  - `::1`           — IPv6 loopback
///  - `fc00::/7`      — IPv6 unique-local
///  - `fe80::/10`     — IPv6 link-local
pub fn is_private_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let octets = v4.octets();
            // loopback 127.0.0.0/8
            octets[0] == 127
            // 10.0.0.0/8
            || octets[0] == 10
            // 172.16.0.0/12
            || (octets[0] == 172 && (octets[1] & 0xF0) == 16)
            // 192.168.0.0/16
            || (octets[0] == 192 && octets[1] == 168)
            // 100.64.0.0/10  (CGNAT)
            || (octets[0] == 100 && (octets[1] & 0xC0) == 64)
            // 169.254.0.0/16  (link-local)
            || (octets[0] == 169 && octets[1] == 254)
            // 0.0.0.0/8
            || octets[0] == 0
        }
        IpAddr::V6(v6) => {
            // ::1  (loopback)
            v6.is_loopback()
            // fc00::/7  (unique local)
            || (v6.segments()[0] & 0xFE00) == 0xFC00
            // fe80::/10  (link-local)
            || (v6.segments()[0] & 0xFFC0) == 0xFE80
        }
    }
}

// ---------------------------------------------------------------------------
// DNS resolver helper
// ---------------------------------------------------------------------------

/// Resolve a hostname to IP addresses using the system resolver.
/// Returns the set of resolved addresses.
pub async fn resolve_host(host: &str, port: u16) -> Result<Vec<IpAddr>, EgressError> {
    let addr = format!("{host}:{port}");
    let addrs: Vec<IpAddr> = tokio::net::lookup_host(&addr)
        .await
        .map_err(|_| EgressError::DnsResolutionFailed(host.to_string()))?
        .map(|sa| sa.ip())
        .collect();

    if addrs.is_empty() {
        return Err(EgressError::DnsResolutionFailed(host.to_string()));
    }

    Ok(addrs)
}

// ---------------------------------------------------------------------------
// EgressClient
// ---------------------------------------------------------------------------

/// Hardened HTTP client for outbound tool requests.
///
/// # Usage
///
/// ```rust,ignore
/// let cfg = EgressConfig::from_env();
/// let client = EgressClient::new(cfg);
///
/// let resp = client.request(Method::GET, "https://api.example.com/data", None).await?;
/// ```
#[derive(Clone)]
pub struct EgressClient {
    inner: Client,
    config: EgressConfig,
    #[cfg(test)]
    static_response: Option<EgressResponse>,
}

impl EgressClient {
    /// Create a new egress client with the given configuration.
    pub fn new(config: EgressConfig) -> Self {
        let inner = Client::builder()
            .timeout(config.timeout)
            .connect_timeout(config.connect_timeout)
            .redirect(reqwest::redirect::Policy::none()) // no auto-follow
            .build()
            .expect("failed to build reqwest client");

        Self {
            inner,
            config,
            #[cfg(test)]
            static_response: None,
        }
    }

    #[cfg(test)]
    pub fn with_static_response(mut self, status: u16, body: impl Into<Bytes>) -> Self {
        self.static_response = Some(EgressResponse {
            status,
            headers: HeaderMap::new(),
            body: body.into(),
        });
        self
    }

    /// Borrow the underlying config.
    pub fn config(&self) -> &EgressConfig {
        &self.config
    }

    // -- Pre-flight checks ------------------------------------------------

    /// Validate a URL against the allowlist and private-IP rules.
    /// Returns the parsed [`Url`] on success.
    pub async fn preflight(&self, raw_url: &str) -> Result<Url, EgressError> {
        // 1. Parse URL
        let url = Url::parse(raw_url).map_err(|e| EgressError::InvalidUrl(e.to_string()))?;

        // 2. Must have a host
        let host = url
            .host_str()
            .ok_or_else(|| EgressError::InvalidUrl("URL has no host".into()))?
            .to_lowercase();

        // 3. Only http / https
        match url.scheme() {
            "http" | "https" => {}
            other => {
                return Err(EgressError::InvalidUrl(format!(
                    "unsupported scheme: {other}"
                )));
            }
        }

        // 4. Check allowlist (fail-closed: empty list → deny all)
        if !self.config.allowed_hosts.contains(&host) {
            warn!(host = %host, "egress: host not in allowlist");
            return Err(EgressError::HostNotAllowed(host));
        }

        // 5. Resolve DNS and check for private IPs
        if self.config.deny_private_ips {
            let port = url.port_or_known_default().unwrap_or(443);
            let ips = resolve_host(&host, port).await?;
            for ip in &ips {
                if is_private_ip(*ip) {
                    warn!(host = %host, ip = %ip, "egress: private IP blocked");
                    return Err(EgressError::PrivateIpBlocked(*ip));
                }
            }
            debug!(host = %host, resolved = ?ips, "egress: DNS check passed");
        }

        Ok(url)
    }

    /// Execute a full egress HTTP request.
    ///
    /// `body` is optional; if provided its length is checked against
    /// `max_request_body_bytes` *before* sending.
    pub async fn request(
        &self,
        method: Method,
        raw_url: &str,
        body: Option<Bytes>,
    ) -> Result<EgressResponse, EgressError> {
        self.request_with_headers(method, raw_url, body, None).await
    }

    /// Execute a full egress HTTP request with optional headers.
    pub async fn request_with_headers(
        &self,
        method: Method,
        raw_url: &str,
        body: Option<Bytes>,
        headers: Option<HeaderMap>,
    ) -> Result<EgressResponse, EgressError> {
        // Pre-flight
        let url = self.preflight(raw_url).await?;

        #[cfg(test)]
        if let Some(ref resp) = self.static_response {
            return Ok(resp.clone());
        }

        // Body size check
        if let Some(ref b) = body {
            if b.len() > self.config.max_request_body_bytes {
                return Err(EgressError::RequestBodyTooLarge {
                    size: b.len(),
                    max: self.config.max_request_body_bytes,
                });
            }
        }

        // Build request
        let mut builder: RequestBuilder = self.inner.request(method.clone(), url.as_str());

        // Mark all gateway-originated upstream calls so tools-side middleware can
        // bypass public rewrites and avoid proxy loops.
        let internal_header_name = HeaderName::from_static("x-gateway-internal");
        let internal_header_value = HeaderValue::from_static("1");

        let mut merged_headers = headers.unwrap_or_default();
        merged_headers.insert(internal_header_name, internal_header_value);
        builder = builder.headers(merged_headers);

        if let Some(b) = body {
            builder = builder.body(b);
        }

        debug!(method = %method, url = %url, "egress: sending request");

        let resp: Response = builder.send().await?;
        let status = resp.status().as_u16();
        let headers = resp.headers().clone();

        // Stream response body with size cap
        let resp_bytes = self.read_body_capped(resp).await?;

        Ok(EgressResponse {
            status,
            headers,
            body: resp_bytes,
        })
    }

    /// Read the response body up to `max_response_bytes`, aborting if exceeded.
    async fn read_body_capped(&self, resp: Response) -> Result<Bytes, EgressError> {
        let max = self.config.max_response_bytes;

        // Check Content-Length hint first (avoid streaming if obviously too big)
        if let Some(cl) = resp.content_length() {
            if cl as usize > max {
                return Err(EgressError::ResponseTooLarge { max });
            }
        }

        // Stream chunks
        let mut buf = Vec::with_capacity(std::cmp::min(
            resp.content_length().unwrap_or(0) as usize,
            max,
        ));

        let mut stream = resp;
        while let Some(chunk) = stream.chunk().await? {
            if buf.len() + chunk.len() > max {
                return Err(EgressError::ResponseTooLarge { max });
            }
            buf.extend_from_slice(&chunk);
        }

        Ok(Bytes::from(buf))
    }
}

// ---------------------------------------------------------------------------
// Response wrapper
// ---------------------------------------------------------------------------

/// The result of a successful egress request.
#[derive(Debug, Clone)]
pub struct EgressResponse {
    pub status: u16,
    pub headers: reqwest::header::HeaderMap,
    pub body: Bytes,
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, Ipv6Addr};

    // -- is_private_ip -----------------------------------------------------

    #[test]
    fn loopback_v4_is_private() {
        assert!(is_private_ip(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1))));
    }

    #[test]
    fn rfc1918_10_is_private() {
        assert!(is_private_ip(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1))));
        assert!(is_private_ip(IpAddr::V4(Ipv4Addr::new(10, 255, 255, 255))));
    }

    #[test]
    fn rfc1918_172_16_is_private() {
        assert!(is_private_ip(IpAddr::V4(Ipv4Addr::new(172, 16, 0, 1))));
        assert!(is_private_ip(IpAddr::V4(Ipv4Addr::new(172, 31, 255, 255))));
        // 172.32.x.x is NOT private
        assert!(!is_private_ip(IpAddr::V4(Ipv4Addr::new(172, 32, 0, 1))));
    }

    #[test]
    fn rfc1918_192_168_is_private() {
        assert!(is_private_ip(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1))));
    }

    #[test]
    fn cgnat_is_private() {
        assert!(is_private_ip(IpAddr::V4(Ipv4Addr::new(100, 64, 0, 1))));
        assert!(is_private_ip(IpAddr::V4(Ipv4Addr::new(100, 127, 255, 255))));
        // 100.128.x.x is NOT CGNAT
        assert!(!is_private_ip(IpAddr::V4(Ipv4Addr::new(100, 128, 0, 1))));
    }

    #[test]
    fn link_local_is_private() {
        assert!(is_private_ip(IpAddr::V4(Ipv4Addr::new(169, 254, 1, 1))));
    }

    #[test]
    fn zero_network_is_private() {
        assert!(is_private_ip(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0))));
    }

    #[test]
    fn public_ipv4_is_not_private() {
        assert!(!is_private_ip(IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8))));
        assert!(!is_private_ip(IpAddr::V4(Ipv4Addr::new(1, 1, 1, 1))));
        assert!(!is_private_ip(IpAddr::V4(Ipv4Addr::new(93, 184, 216, 34))));
    }

    #[test]
    fn loopback_v6_is_private() {
        assert!(is_private_ip(IpAddr::V6(Ipv6Addr::LOCALHOST)));
    }

    #[test]
    fn unique_local_v6_is_private() {
        // fd12::1  (fd00::/8 subset of fc00::/7)
        assert!(is_private_ip(IpAddr::V6(Ipv6Addr::new(
            0xfd12, 0, 0, 0, 0, 0, 0, 1
        ))));
    }

    #[test]
    fn link_local_v6_is_private() {
        assert!(is_private_ip(IpAddr::V6(Ipv6Addr::new(
            0xfe80, 0, 0, 0, 0, 0, 0, 1
        ))));
    }

    #[test]
    fn public_v6_is_not_private() {
        // 2606:4700::1111 (Cloudflare)
        assert!(!is_private_ip(IpAddr::V6(Ipv6Addr::new(
            0x2606, 0x4700, 0, 0, 0, 0, 0, 0x1111
        ))));
    }

    // -- EgressConfig defaults --------------------------------------------

    #[test]
    fn default_config_denies_all_hosts() {
        let cfg = EgressConfig::default();
        assert!(cfg.allowed_hosts.is_empty());
        assert!(cfg.deny_private_ips);
        assert_eq!(cfg.max_response_bytes, 5 * 1024 * 1024);
        assert_eq!(cfg.max_request_body_bytes, 1024 * 1024);
    }

    // -- Preflight checks (async) -----------------------------------------

    fn test_client(hosts: &[&str]) -> EgressClient {
        let mut cfg = EgressConfig::default();
        cfg.allowed_hosts = hosts.iter().map(|h| h.to_string()).collect();
        // Disable DNS check for unit tests (can't resolve real hosts)
        cfg.deny_private_ips = false;
        EgressClient::new(cfg)
    }

    #[tokio::test]
    async fn preflight_rejects_unlisted_host() {
        let client = test_client(&["api.example.com"]);
        let err = client
            .preflight("https://evil.example.com/path")
            .await
            .unwrap_err();
        assert!(matches!(err, EgressError::HostNotAllowed(_)));
    }

    #[tokio::test]
    async fn preflight_accepts_listed_host() {
        let client = test_client(&["api.example.com"]);
        let url = client
            .preflight("https://api.example.com/v1/data")
            .await
            .unwrap();
        assert_eq!(url.host_str().unwrap(), "api.example.com");
    }

    #[tokio::test]
    async fn preflight_rejects_invalid_scheme() {
        let client = test_client(&["example.com"]);
        let err = client
            .preflight("ftp://example.com/file")
            .await
            .unwrap_err();
        assert!(matches!(err, EgressError::InvalidUrl(_)));
    }

    #[tokio::test]
    async fn preflight_rejects_missing_host() {
        let client = test_client(&[]);
        let err = client.preflight("not-a-url").await.unwrap_err();
        assert!(matches!(
            err,
            EgressError::InvalidUrl(_) | EgressError::HostNotAllowed(_)
        ));
    }

    #[tokio::test]
    async fn preflight_host_check_is_case_insensitive() {
        let client = test_client(&["api.example.com"]);
        let url = client
            .preflight("https://API.Example.COM/path")
            .await
            .unwrap();
        assert_eq!(url.host_str().unwrap(), "api.example.com");
    }

    #[tokio::test]
    async fn request_body_too_large_is_rejected() {
        let mut cfg = EgressConfig::default();
        cfg.allowed_hosts.insert("api.example.com".into());
        cfg.deny_private_ips = false;
        cfg.max_request_body_bytes = 100;
        let client = EgressClient::new(cfg);

        let big_body = Bytes::from(vec![0u8; 200]);
        let err = client
            .request(
                Method::POST,
                "https://api.example.com/endpoint",
                Some(big_body),
            )
            .await
            .unwrap_err();
        assert!(matches!(err, EgressError::RequestBodyTooLarge { .. }));
    }

    #[tokio::test]
    async fn empty_allowlist_denies_everything() {
        let client = test_client(&[]);
        let err = client.preflight("https://google.com/").await.unwrap_err();
        assert!(matches!(err, EgressError::HostNotAllowed(_)));
    }
}
