//! Public consistency boundary for Phase 3 versioning and Phase 4 query-time consistency.
//!
//! This module composes the Indexing-owned consistency subsystems behind one
//! stable public boundary.
//!
//! The child modules have deliberately separate responsibilities:
//!
//! - [`versioning`] owns Phase 3 version relationships, candidate lineage,
//!   source/schema compatibility, and publication eligibility.
//! - [`policy`] owns Phase 4 query-time consistency modes and freshness
//!   acceptance rules.
//! - [`synchronization`] owns Phase 4 source/index observations and the derived
//!   synchronization facts consumed by the policy layer.
//!
//! The module itself does not introduce another consistency model. Its job is
//! to expose the existing logical contracts together so higher-level Indexing
//! workflows can compose them.
//!
//! Core remains responsible for universal runtime, execution context,
//! capability dispatch, cancellation, deadline, error-event, and lifecycle
//! infrastructure. Indexing continues to own the consistency semantics that
//! are specific to logical index versions and query-time freshness.

pub mod policy;
pub mod synchronization;
pub mod versioning;

pub use policy::{
    ConsistencyEvaluation, ConsistencyEvaluationState, ConsistencyMode, ConsistencyPolicyError,
    FreshnessEvaluation, FreshnessPolicy, FreshnessPolicyError,
};
pub use synchronization::{
    IndexSynchronizationState, SourceVersionSynchronizationState, SynchronizationError,
    SynchronizationEvaluation, SynchronizationSnapshot,
};
pub use versioning::{
    VersioningError, is_candidate_stale, validate_active_version_compatibility,
    validate_active_version_compatibility_result, validate_candidate_compatibility,
    validate_candidate_compatibility_result, validate_candidate_lineage,
    validate_candidate_lineage_result, validate_candidate_schema_compatibility,
    validate_candidate_schema_compatibility_result, validate_candidate_source_compatibility,
    validate_candidate_source_compatibility_result, validate_publication_eligibility,
    validate_publication_eligibility_result,
};
