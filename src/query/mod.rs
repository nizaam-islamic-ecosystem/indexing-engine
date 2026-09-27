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

#[cfg(test)]
mod tests {
    use super::*;

    use crate::identity::index::INDEX_ID_BYTE_LEN;
    use crate::index::IndexVersionId;
    use crate::provider::{ProviderAvailability, ProviderCapabilities, ProviderCapability};

    fn index_id(seed: u8) -> crate::identity::IndexId {
        crate::identity::IndexId::from_bytes([seed; INDEX_ID_BYTE_LEN])
    }

    fn version_id(value: &str) -> IndexVersionId {
        IndexVersionId::new(value).expect("test version ID should be valid")
    }

    #[test]
    fn query_boundary_reexports_canonical_request_contracts() {
        let request =
            QueryRequest::exact(index_id(0x11), crate::index::KeyMaterial::text("lookup"))
                .expect("exact query request should be valid");

        assert_eq!(request.index_id(), &index_id(0x11));
        assert!(matches!(request.query(), QueryKind::Exact { .. }));
        assert_eq!(request.result_mode(), ResultMode::ReferencesOnly);
    }

    #[test]
    fn query_boundary_exposes_planner_capability_mapping() {
        let request = QueryRequest::similarity(
            index_id(0x12),
            crate::index::KeyMaterial::text("representation"),
        )
        .expect("similarity query request should be valid");

        let requirement = QueryCapabilityRequirement::for_query(request.query())
            .expect("similarity query should map to a capability requirement");

        assert_eq!(
            requirement.alternatives(),
            &[vec![
                ProviderCapability::SimilarityLookup,
                ProviderCapability::Ranking
            ]]
        );

        let capabilities = ProviderCapabilities::new()
            .with_capability(ProviderCapability::SimilarityLookup)
            .with_capability(ProviderCapability::Ranking);

        assert!(requirement.is_satisfied_by(&capabilities));
    }

    #[test]
    fn query_boundary_connects_request_to_planner_entrypoint() {
        let request = QueryRequest::exact(index_id(0x13), crate::index::KeyMaterial::text("term"))
            .expect("exact query request should be valid");

        let capabilities =
            ProviderCapabilities::new().with_capability(ProviderCapability::ExactLookup);

        let error = plan_query(
            &request,
            Vec::<IndexCandidate>::new(),
            &capabilities,
            ProviderAvailability::Available,
        )
        .expect_err("an empty candidate set must fail planning");

        assert!(matches!(error, QueryPlanningError::NoIndexCandidate { .. }));
    }

    #[test]
    fn query_boundary_reexports_reference_oriented_result_contracts() {
        let reference = crate::index::ObjectReference::new("source-a", "object-1")
            .expect("test object reference should be valid");
        let hit = QueryHit::new(reference.clone());
        let version = crate::index::IndexVersion::new(version_id("v1"));

        let result = QueryResult::new(index_id(0x14), version, vec![hit])
            .expect("query result should be valid");

        assert_eq!(result.index_id(), &index_id(0x14));
        assert_eq!(result.hits().len(), 1);
        assert_eq!(result.hits()[0].reference(), &reference);
        assert!(result.consistency().is_none());
    }

    #[test]
    fn query_boundary_exposes_retrieval_validation_entrypoint() {
        let validator: fn(&RetrievalPlan) -> Result<(), RetrievalPlanValidationError> =
            validate_plan;
        let _ = validator;

        let display = RetrievalPlanValidationError::EmptyHybridPlan.to_string();
        assert!(display.contains("hybrid retrieval plan"));
    }

    #[test]
    fn query_boundary_keeps_child_module_paths_and_root_paths_equivalent() {
        let root_request: QueryRequest = request::QueryRequest::new(
            index_id(0x15),
            crate::index::KeyMaterial::text("canonical"),
        )
        .expect("root request should be valid");

        let child_request: request::QueryRequest = root_request;

        assert!(matches!(
            child_request.query(),
            request::QueryKind::Exact { .. }
        ));
        assert_eq!(
            child_request.result_mode(),
            request::ResultMode::ReferencesOnly
        );
    }
}
