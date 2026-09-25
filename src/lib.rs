//! Public library boundary for the Nizaam Indexing Engine.
//!
//! The crate root declares the current Indexing module tree up front so later
//! phases can populate their already-established module directories without
//! repeatedly changing the top-level module declarations.
//!
//! The future-phase modules declared below are structural scaffolding only in
//! Phase 0. Their declarations do not implement any future Indexing behavior.
//! `src/main.rs` / binary targets are intentionally outside this library
//! boundary for Phase 0.

pub mod build;
pub mod capacity;
pub mod configuration;
pub mod consistency;
pub mod engine;
pub mod error;
pub mod identity;
pub mod index;
pub mod integrity;
pub mod lifecycle;
pub mod observability;
pub mod query;
pub mod recovery;
pub mod requirement;
pub mod security;

pub use engine::{
    CapabilitySet, IndexingEngine, IndexingRegistration, IndexingRuntime, RegistrationResult,
    RuntimeDispatchResult,
};
pub use error::IndexingResult;
