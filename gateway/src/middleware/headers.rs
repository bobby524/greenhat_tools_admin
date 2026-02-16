//! Header hardening middleware.
//!
//! Goals:
//! - Strip spoofable identity headers coming from the client.
//! - Add baseline security headers to every HTTP response.
//!
//! This middleware should run near the edge (outermost) so downstream
//! layers cannot be confused by attacker-controlled identity metadata.

use axum::extract::Request;
use axum::middleware::Next;
use axum::response::Response;

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Request headers that must never be trusted from untrusted clients.
///
/// These are stripped to prevent privilege escalation via header spoofing.
///
/// Note: We intentionally do **not** strip `x-forwarded-for` / `x-real-ip`
/// because those are expected when behind a reverse proxy; callers should
/// still treat them as advisory unless the proxy boundary is trusted.
const SPOOFABLE_REQUEST_HEADERS: &[&str] = &[
    // Common identity injection patterns
    "x-user-id",
    "x-user",
    "x-user-email",
    "x-email",
    "x-org-id",
    "x-org",
    "x-roles",
    "x-role",
    "x-permissions",
    "x-session-id",
    "x-auth-user",
    "x-auth-email",
    "x-auth-roles",
    "x-forwarded-user",
    "x-forwarded-email",
    "x-forwarded-roles",
    // Supabase / PostgREST style headers (never trust from client)
    "x-gotrue-claims",
    "x-supabase-role",
    // Generic proxy-auth headers
    "proxy-authorization",
];

// ---------------------------------------------------------------------------
// Middleware
// ---------------------------------------------------------------------------

/// Strip spoofable identity headers and add baseline security headers.
pub async fn header_hardening_middleware(mut req: Request, next: Next) -> Response {
    // ── Request: strip spoofable identity headers ───────────────────────
    for &h in SPOOFABLE_REQUEST_HEADERS {
        req.headers_mut().remove(h);
    }

    let mut res = next.run(req).await;

    // ── Response: set baseline security headers ─────────────────────────
    // Best-effort: do not overwrite if already explicitly set.
    let headers = res.headers_mut();

    headers
        .entry("x-content-type-options")
        .or_insert("nosniff".parse().unwrap());

    headers
        .entry("x-frame-options")
        .or_insert("DENY".parse().unwrap());

    headers
        .entry("referrer-policy")
        .or_insert("no-referrer".parse().unwrap());

    // Disable powerful browser features by default.
    headers
        .entry("permissions-policy")
        .or_insert("accelerometer=(), autoplay=(), camera=(), clipboard-read=(), clipboard-write=(), geolocation=(), gyroscope=(), magnetometer=(), microphone=(), payment=(), usb=()".parse().unwrap());

    // Avoid cross-origin embedding / unintended leaks for API responses.
    headers
        .entry("cross-origin-resource-policy")
        .or_insert("same-site".parse().unwrap());

    // Conservative default CSP for an API gateway. This should not break
    // JSON responses; it mainly protects browsers that might render errors.
    headers.entry("content-security-policy").or_insert(
        "default-src 'none'; frame-ancestors 'none'; base-uri 'none'"
            .parse()
            .unwrap(),
    );

    res
}
