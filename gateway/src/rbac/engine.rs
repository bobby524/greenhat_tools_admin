//! Stateless RBAC policy engine.
//!
//! Evaluates access decisions against the loaded [`Policy`].  All methods are
//! pure and infallible: they return [`Decision::Allow`] or [`Decision::Deny`]
//! based solely on the policy and the provided inputs.
//!
//! The engine never panics, never performs I/O, and **never logs secrets**.

use super::policy::{DefaultPolicy, Policy};
use super::types::{Action, Decision};
use crate::auth::Principal;

// ---------------------------------------------------------------------------
// Engine
// ---------------------------------------------------------------------------

/// Deny-by-default RBAC policy engine.
#[derive(Debug, Clone)]
pub struct PolicyEngine {
    policy: Policy,
}

impl PolicyEngine {
    /// Wrap a validated [`Policy`].
    pub fn new(policy: Policy) -> Self {
        Self { policy }
    }

    // ── Top-level dispatch ───────────────────────────────────────────────

    /// Evaluate whether `principal` may perform `action`.
    pub fn evaluate(&self, principal: &Principal, action: &Action) -> Decision {
        match action {
            Action::ToolCall { tool } => self.evaluate_tool_call(principal, tool),
            Action::RouteAccess { method, path } => {
                self.evaluate_route_access(principal, method, path)
            }
        }
    }

    // ── Tool-call evaluation ─────────────────────────────────────────────

    /// Check whether `principal` may invoke the named MCP tool.
    pub fn evaluate_tool_call(&self, principal: &Principal, tool: &str) -> Decision {
        // Wildcard admin — any role with `"*"` permission bypasses tool checks.
        if self.principal_has_permission(principal, "*") {
            return Decision::Allow;
        }

        let tools_policy = match &self.policy.tools {
            Some(tp) => tp,
            // No tools section ⇒ deny (deny-by-default).
            None => return Decision::Deny("no tools policy configured".into()),
        };

        let entry = match tools_policy.allowlist.get(tool) {
            Some(e) => e,
            None => {
                return match tools_policy.default_policy {
                    DefaultPolicy::Allow => Decision::Allow,
                    DefaultPolicy::Deny => {
                        Decision::Deny(format!("tool '{tool}' not in allowlist"))
                    }
                };
            }
        };

        // Kill-switch.
        if !entry.enabled {
            return Decision::Deny(format!("tool '{tool}' is disabled"));
        }

        // Empty allowed_roles means nobody can use it.
        if entry.allowed_roles.is_empty() {
            return Decision::Deny(format!("tool '{tool}' has no allowed roles configured"));
        }

        // Check principal's roles against tool's allowed_roles.
        for role in &principal.roles {
            if entry.allowed_roles.iter().any(|r| r == role) {
                return Decision::Allow;
            }
        }

        Decision::Deny(format!("principal lacks required role for tool '{tool}'"))
    }

    // ── Route-access evaluation ──────────────────────────────────────────

    /// Check whether `principal` may access the given HTTP route.
    pub fn evaluate_route_access(
        &self,
        principal: &Principal,
        method: &str,
        path: &str,
    ) -> Decision {
        // Wildcard admin.
        if self.principal_has_permission(principal, "*") {
            return Decision::Allow;
        }

        match Self::route_to_permission(method, path) {
            Some(perm) => {
                if self.principal_has_permission(principal, perm) {
                    Decision::Allow
                } else {
                    Decision::Deny(format!("missing permission '{perm}' for {method} {path}"))
                }
            }
            // No permission mapping → infrastructure/public endpoint, allow.
            None => Decision::Allow,
        }
    }

    // ── Helpers ──────────────────────────────────────────────────────────

    /// Returns `true` if any of the principal's roles grant `permission`.
    pub fn principal_has_permission(&self, principal: &Principal, permission: &str) -> bool {
        let roles_map = match &self.policy.roles {
            Some(r) => r,
            None => return false,
        };

        for role_name in &principal.roles {
            if let Some(entry) = roles_map.get(role_name) {
                if entry
                    .permissions
                    .iter()
                    .any(|p| p == "*" || p == permission)
                {
                    return true;
                }
            }
        }

        false
    }

    /// Map an HTTP method + path to the required permission string.
    ///
    /// Returns `None` for infrastructure routes that don't require RBAC
    /// (health, version, metrics) — the RBAC middleware treats `None` as
    /// "no restriction at this layer".
    fn route_to_permission(method: &str, path: &str) -> Option<&'static str> {
        // Infrastructure endpoints — always allowed (also skipped by middleware).
        if matches!(path, "/health" | "/version" | "/metrics") {
            return None;
        }

        // Tool routes.
        if path.starts_with("/v1/tools/call") {
            return Some("tools:invoke");
        }
        if path.starts_with("/v1/tools") && method == "GET" {
            return Some("tools:list");
        }

        // Admin routes.
        if path.starts_with("/v1/admin/policy") {
            return Some("admin:policy");
        }
        if path.starts_with("/v1/admin/audit") {
            return Some("admin:audit");
        }

        if path.starts_with("/api/exponential") {
            return match method {
                "GET" | "HEAD" | "OPTIONS" => Some("data:read"),
                "POST" | "PUT" | "PATCH" | "DELETE" => Some("data:write"),
                _ => None,
            };
        }

        // Generic API routes.
        match method {
            "GET" | "HEAD" | "OPTIONS" if path.starts_with("/v1/") => Some("data:read"),
            "POST" | "PUT" | "PATCH" | "DELETE" if path.starts_with("/v1/") => Some("data:write"),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::{AuthMethod, Principal};

    fn test_policy() -> Policy {
        Policy::load_from_str(
            r#"{
                "schema_version": "0.1.0",
                "tools": {
                    "default_policy": "deny",
                    "allowlist": {
                        "web_search": {
                            "enabled": true,
                            "allowed_roles": ["analyst", "admin"]
                        },
                        "db_query": {
                            "enabled": true,
                            "allowed_roles": ["admin"]
                        },
                        "disabled_tool": {
                            "enabled": false,
                            "allowed_roles": ["admin"]
                        },
                        "no_roles_tool": {
                            "enabled": true,
                            "allowed_roles": []
                        }
                    }
                },
                "roles": {
                    "admin":   { "permissions": ["*"] },
                    "analyst": { "permissions": ["tools:invoke", "tools:list", "data:read"] },
                    "viewer":  { "permissions": ["data:read"] }
                }
            }"#,
        )
        .unwrap()
    }

    fn principal(roles: &[&str]) -> Principal {
        Principal {
            user_id: "u-1".into(),
            org_id: None,
            roles: roles.iter().map(|s| s.to_string()).collect(),
            session_id: "s-1".into(),
            auth_method: AuthMethod::Bearer,
        }
    }

    // ── Tool-call tests ──────────────────────────────────────────────────

    #[test]
    fn tool_allowed_for_matching_role() {
        let engine = PolicyEngine::new(test_policy());
        let p = principal(&["analyst"]);
        assert!(engine.evaluate_tool_call(&p, "web_search").is_allowed());
    }

    #[test]
    fn tool_denied_for_wrong_role() {
        let engine = PolicyEngine::new(test_policy());
        let p = principal(&["viewer"]);
        let d = engine.evaluate_tool_call(&p, "web_search");
        assert!(!d.is_allowed());
        assert!(matches!(d, Decision::Deny(ref msg) if msg.contains("lacks required role")));
    }

    #[test]
    fn tool_denied_when_not_in_allowlist() {
        let engine = PolicyEngine::new(test_policy());
        let p = principal(&["admin"]);
        // admin has wildcard, so this actually allows.  Test with analyst instead.
        let p2 = principal(&["analyst"]);
        let d = engine.evaluate_tool_call(&p2, "unknown_tool");
        assert!(!d.is_allowed());
        assert!(matches!(d, Decision::Deny(ref msg) if msg.contains("not in allowlist")));
        // admin wildcard still allows
        assert!(engine.evaluate_tool_call(&p, "unknown_tool").is_allowed());
    }

    #[test]
    fn tool_denied_when_disabled() {
        let engine = PolicyEngine::new(test_policy());
        // Even analyst can't use a disabled tool (no wildcard).
        let p = principal(&["analyst"]);
        let d = engine.evaluate_tool_call(&p, "disabled_tool");
        assert!(!d.is_allowed());
        assert!(matches!(d, Decision::Deny(ref msg) if msg.contains("disabled")));
    }

    #[test]
    fn tool_denied_when_no_allowed_roles() {
        let engine = PolicyEngine::new(test_policy());
        let p = principal(&["analyst"]);
        let d = engine.evaluate_tool_call(&p, "no_roles_tool");
        assert!(!d.is_allowed());
        assert!(matches!(d, Decision::Deny(ref msg) if msg.contains("no allowed roles")));
    }

    #[test]
    fn admin_wildcard_bypasses_tool_check() {
        let engine = PolicyEngine::new(test_policy());
        let p = principal(&["admin"]);
        // Admin can call anything, even unlisted tools.
        assert!(engine.evaluate_tool_call(&p, "web_search").is_allowed());
        assert!(engine.evaluate_tool_call(&p, "db_query").is_allowed());
        assert!(engine.evaluate_tool_call(&p, "not_listed").is_allowed());
    }

    #[test]
    fn admin_wildcard_overrides_disabled_tool() {
        let engine = PolicyEngine::new(test_policy());
        let p = principal(&["admin"]);
        // Wildcard skips the allowlist check entirely (including enabled flag).
        assert!(engine.evaluate_tool_call(&p, "disabled_tool").is_allowed());
    }

    #[test]
    fn no_tools_section_denies() {
        let policy = Policy::load_from_str(r#"{ "schema_version": "0.1.0" }"#).unwrap();
        let engine = PolicyEngine::new(policy);
        let p = principal(&["analyst"]);
        let d = engine.evaluate_tool_call(&p, "anything");
        assert!(!d.is_allowed());
        assert!(matches!(d, Decision::Deny(ref msg) if msg.contains("no tools policy")));
    }

    #[test]
    fn no_roles_denies_everything() {
        let engine = PolicyEngine::new(test_policy());
        let p = principal(&[]);
        assert!(!engine.evaluate_tool_call(&p, "web_search").is_allowed());
    }

    #[test]
    fn multiple_roles_checked() {
        let engine = PolicyEngine::new(test_policy());
        let p = principal(&["viewer", "analyst"]);
        // analyst role grants access to web_search
        assert!(engine.evaluate_tool_call(&p, "web_search").is_allowed());
    }

    // ── Route-access tests ───────────────────────────────────────────────

    #[test]
    fn route_allowed_with_matching_permission() {
        let engine = PolicyEngine::new(test_policy());
        let p = principal(&["analyst"]);
        // analyst has "tools:invoke"
        assert!(engine
            .evaluate_route_access(&p, "POST", "/v1/tools/call")
            .is_allowed());
        // analyst has "data:read"
        assert!(engine
            .evaluate_route_access(&p, "GET", "/v1/data")
            .is_allowed());
    }

    #[test]
    fn route_denied_without_permission() {
        let engine = PolicyEngine::new(test_policy());
        let p = principal(&["viewer"]);
        // viewer only has "data:read", not "tools:invoke"
        let d = engine.evaluate_route_access(&p, "POST", "/v1/tools/call");
        assert!(!d.is_allowed());
    }

    #[test]
    fn exponential_routes_require_data_permissions() {
        let engine = PolicyEngine::new(test_policy());
        let analyst = principal(&["analyst"]);
        assert!(engine
            .evaluate_route_access(&analyst, "GET", "/api/exponential/tasks")
            .is_allowed());
        let d = engine.evaluate_route_access(&analyst, "POST", "/api/exponential/tasks");
        assert!(!d.is_allowed());
    }

    #[test]
    fn infrastructure_routes_always_allowed() {
        let engine = PolicyEngine::new(test_policy());
        let p = principal(&[]); // no roles at all
        assert!(engine
            .evaluate_route_access(&p, "GET", "/health")
            .is_allowed());
        assert!(engine
            .evaluate_route_access(&p, "GET", "/version")
            .is_allowed());
        assert!(engine
            .evaluate_route_access(&p, "GET", "/metrics")
            .is_allowed());
    }

    #[test]
    fn admin_route_requires_admin_policy() {
        let engine = PolicyEngine::new(test_policy());
        let analyst = principal(&["analyst"]);
        let admin = principal(&["admin"]);
        assert!(!engine
            .evaluate_route_access(&analyst, "GET", "/v1/admin/policy")
            .is_allowed());
        assert!(engine
            .evaluate_route_access(&admin, "GET", "/v1/admin/policy")
            .is_allowed());
    }

    #[test]
    fn write_route_requires_data_write() {
        let engine = PolicyEngine::new(test_policy());
        let viewer = principal(&["viewer"]);
        let d = engine.evaluate_route_access(&viewer, "POST", "/v1/resources");
        assert!(!d.is_allowed());
    }

    // ── Permission helper tests ──────────────────────────────────────────

    #[test]
    fn wildcard_permission_matches_everything() {
        let engine = PolicyEngine::new(test_policy());
        let admin = principal(&["admin"]);
        assert!(engine.principal_has_permission(&admin, "tools:invoke"));
        assert!(engine.principal_has_permission(&admin, "data:read"));
        assert!(engine.principal_has_permission(&admin, "anything"));
    }

    #[test]
    fn specific_permission_matches_only_listed() {
        let engine = PolicyEngine::new(test_policy());
        let analyst = principal(&["analyst"]);
        assert!(engine.principal_has_permission(&analyst, "tools:invoke"));
        assert!(engine.principal_has_permission(&analyst, "data:read"));
        assert!(!engine.principal_has_permission(&analyst, "data:write"));
        assert!(!engine.principal_has_permission(&analyst, "admin:policy"));
    }

    #[test]
    fn unknown_role_has_no_permissions() {
        let engine = PolicyEngine::new(test_policy());
        let p = principal(&["ghost"]);
        assert!(!engine.principal_has_permission(&p, "tools:invoke"));
    }
}
