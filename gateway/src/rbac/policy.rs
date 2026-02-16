//! Policy document types and loader.
//!
//! A policy file is a JSON document conforming to
//! [`docs/schemas/policy.v0.schema.json`](../../docs/schemas/policy.v0.schema.json).
//! The gateway reads the file path from the `POLICY_FILE` environment variable
//! at startup.

use std::collections::HashMap;
use std::path::Path;

use serde::Deserialize;

// ---------------------------------------------------------------------------
// Top-level policy
// ---------------------------------------------------------------------------

/// Root policy document.
///
/// Unknown top-level keys (e.g. `egress`, `rate_limits`) are silently ignored
/// so that the same file can serve multiple subsystems.
#[derive(Debug, Clone, Deserialize)]
pub struct Policy {
    /// Semver schema version — must start with `"0."`.
    pub schema_version: String,

    /// Human-readable label.
    #[serde(default)]
    pub id: Option<String>,

    /// Tool-access controls.
    #[serde(default)]
    pub tools: Option<ToolsPolicy>,

    /// Named RBAC roles → permission sets.
    #[serde(default)]
    pub roles: Option<HashMap<String, RoleEntry>>,
}

// ---------------------------------------------------------------------------
// Tools section
// ---------------------------------------------------------------------------

/// Controls which MCP tools are available and their resource bounds.
#[derive(Debug, Clone, Deserialize)]
pub struct ToolsPolicy {
    /// `"deny"` (default) or `"allow"`.
    #[serde(default = "default_deny")]
    pub default_policy: DefaultPolicy,

    /// Gateway-wide tool concurrency cap.
    #[serde(default)]
    pub max_concurrent_global: Option<u32>,

    /// Per-tool configuration keyed by tool name.
    #[serde(default)]
    pub allowlist: HashMap<String, ToolEntry>,
}

/// Whether unlisted items are denied or allowed.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DefaultPolicy {
    Deny,
    Allow,
}

fn default_deny() -> DefaultPolicy {
    DefaultPolicy::Deny
}

/// Configuration for a single tool in the allowlist.
#[derive(Debug, Clone, Deserialize)]
pub struct ToolEntry {
    /// Quick kill-switch — `false` denies all callers regardless of role.
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Per-invocation timeout in milliseconds.
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,

    /// Max parallel invocations for this tool.
    #[serde(default = "default_max_concurrent")]
    pub max_concurrent: u32,

    /// Roles permitted to invoke; **empty = no-one**.
    #[serde(default)]
    pub allowed_roles: Vec<String>,

    /// Optional path/URI to a JSON Schema for argument validation.
    #[serde(default)]
    pub args_schema: Option<String>,
}

fn default_true() -> bool {
    true
}
fn default_timeout_ms() -> u64 {
    30_000
}
fn default_max_concurrent() -> u32 {
    8
}

// ---------------------------------------------------------------------------
// Roles section
// ---------------------------------------------------------------------------

/// A named RBAC role.
#[derive(Debug, Clone, Deserialize)]
pub struct RoleEntry {
    /// Permission strings (e.g. `"tools:invoke"`, `"*"`).
    pub permissions: Vec<String>,

    /// Human-readable description.
    #[serde(default)]
    pub description: Option<String>,
}

// ---------------------------------------------------------------------------
// Loading
// ---------------------------------------------------------------------------

/// Errors that can occur when loading a policy file.
#[derive(Debug)]
pub enum PolicyLoadError {
    /// File I/O failure.
    Io(String),
    /// JSON parse failure.
    Parse(String),
    /// Schema version not supported.
    UnsupportedVersion(String),
}

impl std::fmt::Display for PolicyLoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(msg) => write!(f, "policy I/O error: {msg}"),
            Self::Parse(msg) => write!(f, "policy parse error: {msg}"),
            Self::UnsupportedVersion(v) => {
                write!(f, "unsupported policy schema version: {v}")
            }
        }
    }
}

impl Policy {
    /// Load and validate a policy from a file path.
    pub fn load_from_file(path: &Path) -> Result<Self, PolicyLoadError> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| PolicyLoadError::Io(format!("{}: {e}", path.display())))?;
        Self::load_from_str(&content)
    }

    /// Parse and validate a policy from a JSON string.
    pub fn load_from_str(json: &str) -> Result<Self, PolicyLoadError> {
        let policy: Self =
            serde_json::from_str(json).map_err(|e| PolicyLoadError::Parse(e.to_string()))?;
        policy.validate()?;
        Ok(policy)
    }

    /// Semantic validation beyond what serde can express.
    fn validate(&self) -> Result<(), PolicyLoadError> {
        // Schema version must be 0.x.y (our only supported major).
        if !self.schema_version.starts_with("0.") {
            return Err(PolicyLoadError::UnsupportedVersion(
                self.schema_version.clone(),
            ));
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_policy() {
        let json = r#"{ "schema_version": "0.1.0" }"#;
        let p = Policy::load_from_str(json).unwrap();
        assert_eq!(p.schema_version, "0.1.0");
        assert!(p.tools.is_none());
        assert!(p.roles.is_none());
    }

    #[test]
    fn parse_full_policy() {
        let json = r#"{
            "schema_version": "0.1.0",
            "id": "test",
            "tools": {
                "default_policy": "deny",
                "allowlist": {
                    "web_search": {
                        "enabled": true,
                        "allowed_roles": ["analyst", "admin"]
                    }
                }
            },
            "roles": {
                "admin": { "permissions": ["*"] },
                "analyst": { "permissions": ["tools:invoke", "tools:list"] }
            }
        }"#;
        let p = Policy::load_from_str(json).unwrap();
        let tools = p.tools.unwrap();
        assert_eq!(tools.default_policy, DefaultPolicy::Deny);
        assert!(tools.allowlist.contains_key("web_search"));
        let roles = p.roles.unwrap();
        assert!(roles.contains_key("admin"));
    }

    #[test]
    fn reject_unsupported_version() {
        let json = r#"{ "schema_version": "1.0.0" }"#;
        let err = Policy::load_from_str(json).unwrap_err();
        assert!(err.to_string().contains("unsupported"));
    }

    #[test]
    fn reject_invalid_json() {
        let err = Policy::load_from_str("not json").unwrap_err();
        assert!(err.to_string().contains("parse error"));
    }

    #[test]
    fn default_policy_is_deny() {
        let json = r#"{
            "schema_version": "0.1.0",
            "tools": { "allowlist": {} }
        }"#;
        let p = Policy::load_from_str(json).unwrap();
        assert_eq!(p.tools.unwrap().default_policy, DefaultPolicy::Deny);
    }

    #[test]
    fn tool_entry_defaults() {
        let json = r#"{
            "schema_version": "0.1.0",
            "tools": {
                "allowlist": {
                    "t": {}
                }
            }
        }"#;
        let p = Policy::load_from_str(json).unwrap();
        let t = &p.tools.unwrap().allowlist["t"];
        assert!(t.enabled);
        assert_eq!(t.timeout_ms, 30_000);
        assert_eq!(t.max_concurrent, 8);
        assert!(t.allowed_roles.is_empty());
    }

    #[test]
    fn extra_top_level_keys_are_ignored() {
        let json = r#"{
            "schema_version": "0.1.0",
            "egress": { "default_policy": "deny" },
            "rate_limits": {}
        }"#;
        Policy::load_from_str(json).unwrap(); // should not error
    }
}
