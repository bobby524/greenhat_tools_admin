//! Core RBAC types: [`Principal`], [`Action`], [`Resource`], [`Scope`],
//! [`Decision`].
//!
//! These are the building blocks of the deny-by-default authorization model.
//! They are intentionally transport-agnostic so that both HTTP middleware and
//! MCP tool handlers can share the same evaluation logic.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Principal — *who* is making the request
// ---------------------------------------------------------------------------

/// An authenticated identity with zero or more roles and an optional
/// organisational scope.
///
/// The auth layer is responsible for populating this and inserting it into
/// [`http::Extensions`] before the RBAC middleware runs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Principal {
    /// Subject identifier — user id, service-account id, API-key id, etc.
    pub sub: String,

    /// Roles assigned to this principal (e.g. `["admin"]`, `["developer"]`).
    pub roles: Vec<String>,

    /// Hierarchical scope the principal operates within.
    #[serde(default)]
    pub scope: Scope,
}

// ---------------------------------------------------------------------------
// Scope — org / team / project hierarchy
// ---------------------------------------------------------------------------

/// Hierarchical organisational scope: **org ⊃ team ⊃ project**.
///
/// A `None` value at any level means "unrestricted at that level".
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Scope {
    pub org: Option<String>,
    pub team: Option<String>,
    pub project: Option<String>,
}

impl Scope {
    /// Returns `true` when `self` encompasses `other`.
    ///
    /// Rules (checked top-down):
    /// * If `self.org` is `None` → unrestricted, encompasses any org.
    /// * If `self.org` is `Some(o)` → `other.org` must be `Some(o)`.
    /// * Same logic for team and project.
    pub fn encompasses(&self, other: &Self) -> bool {
        if !field_encompasses(&self.org, &other.org) {
            return false;
        }
        if !field_encompasses(&self.team, &other.team) {
            return false;
        }
        if !field_encompasses(&self.project, &other.project) {
            return false;
        }
        true
    }
}

/// `None` (unrestricted) encompasses anything; `Some(a)` encompasses only
/// `Some(a)` or `None`.
fn field_encompasses(parent: &Option<String>, child: &Option<String>) -> bool {
    match (parent, child) {
        (None, _) => true,       // unrestricted parent
        (Some(_), None) => true, // child has no restriction at this level
        (Some(p), Some(c)) => p == c,
    }
}

// ---------------------------------------------------------------------------
// Action — *what* the principal wants to do
// ---------------------------------------------------------------------------

/// A requested operation.
#[derive(Debug, Clone)]
pub enum Action {
    /// Invoke an MCP tool by name.
    ToolCall { tool: String },
    /// Access an HTTP route.
    RouteAccess { method: String, path: String },
}

// ---------------------------------------------------------------------------
// Resource — *what* is being acted upon
// ---------------------------------------------------------------------------

/// The kind of resource being accessed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResourceKind {
    Tool,
    Route,
}

/// A target resource with optional scope.
#[derive(Debug, Clone)]
pub struct Resource {
    pub kind: ResourceKind,
    pub name: String,
    pub scope: Scope,
}

// ---------------------------------------------------------------------------
// Decision — the engine's verdict
// ---------------------------------------------------------------------------

/// Result of a policy evaluation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    /// Access is granted.
    Allow,
    /// Access is denied, with a human-readable reason.
    Deny(String),
}

impl Decision {
    /// Convenience predicate.
    pub fn is_allowed(&self) -> bool {
        matches!(self, Decision::Allow)
    }
}
