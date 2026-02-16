//! Deny-by-default RBAC subsystem.
//!
//! - [`policy`] — policy file loading and serde types
//! - [`engine`] — stateless evaluation logic
//! - [`types`]  — transport-agnostic domain primitives

pub mod engine;
pub mod policy;
pub mod types;

pub use engine::PolicyEngine;
pub use policy::Policy;
