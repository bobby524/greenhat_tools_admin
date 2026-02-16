//! Authentication domain — types, traits, and BetterAuth integration.
//!
//! The gateway validates sessions through the [`SessionValidator`] trait.
//! The production implementation ([`BetterAuthClient`]) calls BetterAuth's
//! `GET /api/auth/get-session` endpoint.  A [`NoopValidator`] is provided
//! for testing.

pub mod jwt;
pub mod principal;
pub mod session;

pub use principal::{AuthMethod, Principal};
pub use session::{
    AuthError, AuthState, BetterAuthClient, NoopValidator, SessionCredential, SessionValidator,
    SplitValidator,
};
