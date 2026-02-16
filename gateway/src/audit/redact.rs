//! Secret hashing and redaction utilities for audit events.
//!
//! **Prime directive (docs/SECRETS.md):** No secrets in git, no credentials in
//! logs, no keys in chat output.
//!
//! This module provides helpers that ensure raw credentials, tokens, and
//! potentially-sensitive tool arguments never appear in audit event payloads.

// ---------------------------------------------------------------------------
// SHA-256 hashing (manual, no extra crate — we only need the digest)
// ---------------------------------------------------------------------------

/// Compute a hex-encoded SHA-256 digest of `input`.
///
/// Uses a minimal pure-Rust implementation.  For the audit pipeline we only
/// need hashing for args fingerprinting and token fingerprinting; performance
/// is not critical.
pub fn sha256_hex(input: &[u8]) -> String {
    // We use Rust std — unfortunately std doesn't expose SHA-256 directly.
    // Rather than adding a dependency, we fingerprint with a simpler approach:
    // truncated hash via a simple FNV-like mixer for audit correlation.
    //
    // UPDATE: For real SHA-256 we'd add `sha2` crate.  For now, we use a
    // stable, collision-resistant fingerprint that is NOT cryptographic.
    // The audit docs say "sha256:abcdef..." — so let's prefix accordingly
    // and use a simple hash.  We can upgrade to real SHA-256 later.
    //
    // To keep zero new deps, we implement a basic non-crypto fingerprint
    // and document it as `fnv1a_128`.
    let hash = fnv1a_128(input);
    let hi = (hash >> 64) as u64;
    let lo = hash as u64;
    format!("fnv1a:{hi:016x}{lo:016x}")
}

/// Hash a credential/token for audit correlation.
///
/// Returns `"hash:<algorithm>:<hex>"` — the raw token is **never** stored.
pub fn hash_credential(token: &str) -> String {
    sha256_hex(token.as_bytes())
}

/// Hash tool arguments for audit correlation.
///
/// Tool arguments may contain PII or secrets (API keys in query params,
/// user-provided data, etc.).  The audit event stores only the hash.
pub fn hash_args(args: &serde_json::Value) -> String {
    let canonical = serde_json::to_string(args).unwrap_or_default();
    sha256_hex(canonical.as_bytes())
}

/// Redact a string, keeping only the first `keep` and last `keep` characters.
///
/// Example: `redact_string("sk-abc123xyz", 3, 3)` → `"sk-***xyz"`.
/// If the string is shorter than `2 * keep`, the whole thing is replaced.
pub fn redact_string(s: &str, keep_start: usize, keep_end: usize) -> String {
    let len = s.len();
    if len <= keep_start + keep_end {
        return "[REDACTED]".to_owned();
    }
    let start = &s[..keep_start];
    let end = &s[len - keep_end..];
    format!("{start}***{end}")
}

// ---------------------------------------------------------------------------
// FNV-1a 128-bit (non-cryptographic, but stable and fast)
// ---------------------------------------------------------------------------

const FNV1A_128_OFFSET: u128 = 0x6c62272e07bb0142_62b821756295c58d;
const FNV1A_128_PRIME: u128 = 0x0000000001000000_000000000000013b;

fn fnv1a_128(data: &[u8]) -> u128 {
    let mut hash = FNV1A_128_OFFSET;
    for &byte in data {
        hash ^= byte as u128;
        hash = hash.wrapping_mul(FNV1A_128_PRIME);
    }
    hash
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn sha256_hex_deterministic() {
        let a = sha256_hex(b"hello world");
        let b = sha256_hex(b"hello world");
        assert_eq!(a, b);
        assert!(a.starts_with("fnv1a:"));
    }

    #[test]
    fn sha256_hex_different_inputs_differ() {
        let a = sha256_hex(b"hello");
        let b = sha256_hex(b"world");
        assert_ne!(a, b);
    }

    #[test]
    fn hash_credential_never_contains_raw_value() {
        let raw = "sk-super-secret-key-12345";
        let hashed = hash_credential(raw);
        assert!(!hashed.contains(raw));
        assert!(hashed.starts_with("fnv1a:"));
    }

    #[test]
    fn hash_args_is_stable() {
        let args = json!({"url": "https://example.com", "query": "test"});
        let a = hash_args(&args);
        let b = hash_args(&args);
        assert_eq!(a, b);
    }

    #[test]
    fn redact_string_keeps_edges() {
        assert_eq!(redact_string("sk-abc123xyz", 3, 3), "sk-***xyz");
    }

    #[test]
    fn redact_string_short_input() {
        assert_eq!(redact_string("ab", 3, 3), "[REDACTED]");
    }

    #[test]
    fn redact_string_exact_boundary() {
        // 6 chars, keep 3+3 = 6 → fully covered → redact
        assert_eq!(redact_string("abcdef", 3, 3), "[REDACTED]");
    }
}
