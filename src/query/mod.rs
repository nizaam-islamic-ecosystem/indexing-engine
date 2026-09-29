//! Public query boundary for the Phase 4 Indexing Engine.
//!
//! The `query` module composes the canonical Phase 4 logical query contracts
//! without introducing another planning, execution, consistency, or provider
//! abstraction. Its child modules own distinct stages of the query lifecycle:
//!
//! ```text
//! request.rs
//!     ↓
//! planner.rs
//!     ↓
//! retrieval.rs
//!     ↓
//! result.rs
//! ```
//!
//! More precisely:
//!
//! - [`request`] defines what the Indexing Engine is asked to retrieve;
//! - [`planner`] resolves that request against caller-supplied logical
//!   index/version candidates and provider capabilities;
//! - [`retrieval`] executes the resulting logical plan through a concrete
//!   provider adapter;
//! - [`result`] defines the canonical reference-oriented result contract.
//!
//! This module is intentionally only a module/export boundary. It does not:
//!
//! - select physical storage or database technology;
//! - implement query planning or retrieval itself;
//! - evaluate consistency policies;
//! - own index publication or an active-version registry;
//! - calculate domain-specific relevance or hydrate domain objects;
//! - recreate Core runtime, cancellation, deadline, or routing machinery.
//!
//! The explicit re-exports below provide a stable `crate::query::*` public
//! surface while preserving the child-module paths for callers that prefer
//! stage-specific imports.

/// Canonical logical query-request contracts.
pub mod request;

/// Canonical reference-oriented query-result contracts.
pub mod result;

/// Logical query planning and provider-capability negotiation.
pub mod planner;

/// Execution of validated logical retrieval plans through a provider adapter.
pub mod retrieval;

// -----------------------------------------------------------------------------
// Canonical request surface
// -----------------------------------------------------------------------------

pub use request::{
    AtomicQuery, AtomicQueryValidationError, HybridQueryComponent,
    HybridQueryComponentValidationError, QueryKind, QueryKindValidationError, QueryRequest,
    QueryRequestValidationError, ResultMode,
};

// -----------------------------------------------------------------------------
// Canonical result surface
// -----------------------------------------------------------------------------

pub use result::{
    ConsistencyMetadata, ConsistencyMetadataValidationError, ConsistencyState, MetricKind,
    QueryHit, QueryHitValidationError, QueryResult, QueryResultValidationError,
};

// -----------------------------------------------------------------------------
// Canonical planning surface
// -----------------------------------------------------------------------------

pub use planner::{
    CandidateValidationError, CapabilityResolution, ExactRetrievalPlan, FilteredRetrievalPlan,
    HybridRetrievalPlan, IndexCandidate, NeighborhoodRetrievalPlan, PlannedHybridComponent,
    PlannedIndex, QueryCapabilityRequirement, QueryCapabilityRequirementError, QueryPlanningError,
    RetrievalPlan, RetrievalPlanContext, SimilarityRetrievalPlan, StructuredRetrievalPlan,
    TextRetrievalPlan, plan_query,
};

// -----------------------------------------------------------------------------
// Canonical retrieval surface
// -----------------------------------------------------------------------------

pub use retrieval::{
    ProviderRetriever, RetrievalError, RetrievalPlanValidationError, RetrievalResultError, execute,
    validate_plan,
};
