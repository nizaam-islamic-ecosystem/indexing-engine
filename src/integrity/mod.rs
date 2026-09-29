//! Public logical-integrity boundary for the Indexing Engine.
//!
//! Phase 5 keeps integrity validation separate from physical storage,
//! provider behavior, source-engine lookup, and recovery execution.
//!
//! The [`validation`] module owns the logical trust boundary:
//!
//! ```text
//! index metadata
//!      ↓
//! index definition
//!      ↓
//! index version
//!      ↓
//! index entries
//!      ↓
//! object references
//!      ↓
//! version compatibility
//!      ↓
//! publication preconditions
//! ```
//!
//! This module is intentionally a thin facade. It composes and re-exports the
//! validator and its result/error contracts without introducing another
//! validation model, provider, source callback, lifecycle manager, or runtime.
//!
//! Level 1 validation tests remain in `validation.rs`. The tests here are
//! Level 2 module-boundary tests and verify that the public integrity surface
//! composes definition, entry, reference, version, and publication checks.

pub mod validation;

pub use validation::{
    IntegrityResult, IntegrityValidationError, IntegrityValidator, validate_definition,
    validate_entry, validate_publication_preconditions, validate_reference, validate_version,
    validate_version_compatibility,
};
