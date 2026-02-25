//! The authenticated identity extracted from a validated session.

/// How the caller proved their identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthMethod {
    /// BetterAuth session cookie (`better-auth.session_token`).
    Cookie,
    /// `Authorization: Bearer <token>` header.
    Bearer,
}

/// The authenticated principal attached to a request after session validation.
///
/// Inserted into [`axum::extract::Request::extensions`] by the auth
/// middleware — downstream handlers can access it via
/// `Extension<Principal>` or by reading extensions directly.
#[derive(Debug, Clone)]
pub struct Principal {
    /// Canonical user ID used for ownership/authorization checks.
    pub user_id: String,
    /// Trusted user email (normalized lowercase when present).
    pub email: Option<String>,
    /// Organisation ID (populated when the BetterAuth org plugin is active).
    pub org_id: Option<String>,
    /// Roles / claims attached to the user (e.g. `["admin", "member"]`).
    pub roles: Vec<String>,
    /// BetterAuth session ID.
    pub session_id: String,
    /// How the caller authenticated.
    pub auth_method: AuthMethod,
}
