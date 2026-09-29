//! Public requirement boundary for the Indexing Engine.
//!
//! This module exposes the source-to-Indexing logical requirement contract.
//! The implementation remains in [`requirement`]; this file is intentionally
//! limited to module composition and public re-exports.

#[allow(clippy::module_inception)]
pub mod requirement;

pub use requirement::{IndexRequirement, IndexRequirementValidationError};
