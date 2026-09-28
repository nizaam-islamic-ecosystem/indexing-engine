//! Security integration boundary for the Nizaam Indexing Engine.
//!
//! Phase 6 reuses the security mechanisms provided by `nizaam_core` rather
//! than introducing a second Indexing-specific security framework.
//!
//! Core remains authoritative for:
//!
//! - authentication,
//! - trusted [`nizaam_core::security::SecurityContext`] propagation,
//! - authorization evaluation,
//! - authorization decisions, and
//! - security middleware execution.
//!
//! The Indexing layer contributes only the authorization requirement needed to
//! bind a request to its logical engine and concrete engine instance.
//!
//! ```text
//! Core Security
//!     │
//!     ├── SecurityContext
//!     ├── Authorizer
//!     ├── AuthorizationRequest
//!     └── AuthorizationDecision
//!             │
//!             ▼
//!     IndexingAuthorizationRequirement
//!             │
//!             ├── EngineId
//!             └── EngineInstanceId
//! ```
//!
//! No Indexing-specific authentication, credentials, security context,
//! authorization policy, or middleware framework is defined here.

pub mod authorization;

pub use authorization::{IndexingAuthorizationError, IndexingAuthorizationRequirement};
