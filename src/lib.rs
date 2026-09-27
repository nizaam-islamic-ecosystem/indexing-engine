//! Public library boundary for the Nizaam Indexing Engine.
//!
//! The crate root exposes the established Indexing module tree and the public
//! logical contracts implemented across Phase 1, Phase 2, Phase 3, and Phase 4.
//!
//! The public boundary keeps ownership explicit:
//! - `identity` defines Indexing identities and logical namespaces.
//! - `index` defines logical index families, logical index contracts, and the
//!   Phase 3 index-version lifecycle contract. Its legacy query path remains a
//!   compatibility facade over the canonical Phase 4 query contracts.
//! - `consistency` defines Indexing-owned Phase 3/Phase 4 consistency contracts.
//! - `provider` defines the shared provider capability/ranking boundary.
//! - `query` defines the canonical Phase 4 logical query, planning, retrieval,
//!   and reference-oriented result contracts.
//! - `requirement` defines the source-to-Indexing requirement contract.
//! - Core-owned runtime, capability, lifecycle, contract, and execution
//!   infrastructure remains owned by `nizaam-core` and is surfaced here only
//!   through the Indexing engine API where required.
//!
//! Physical storage, index construction, physical retrieval algorithms,
//! provider implementations, and domain semantics remain outside these logical
//! contracts.

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
pub mod provider;
pub mod query;
pub mod recovery;
pub mod requirement;
pub mod security;

pub use engine::{
    CapabilitySet, EngineSetupError, IndexingEngine, IndexingRegistration, IndexingRuntime,
    RegistrationResult, RequestHandlingError, RuntimeDispatchResult, UniversalRequestResult,
};
pub use error::IndexingResult;
pub use identity::{
    INDEX_ID_BIT_LEN, INDEX_ID_BYTE_LEN, IndexDefinitionId, IndexDefinitionIdValidationError,
    IndexDefinitionIdentity, IndexId, IndexIdGenerationError, IndexIdGenerationVersion,
    IndexNamespace, MAX_NAMESPACE_BYTES, NamespaceRegistry, NamespaceRegistryError,
    NamespaceValidationError,
};

// Keep the established Phase 1-3 index exports intact. The query types here
// resolve through the compatibility facade in `index::query` and therefore
// remain the same canonical Phase 4 types.
pub use index::{
    ConsistencyRequirement, ConsistencyRequirementValidationError, IndexDefinition,
    IndexDefinitionValidationError, IndexEntry, IndexEntryValidationError, IndexFamily,
    IndexVersion, IndexVersionId, IndexVersionIdValidationError, IndexVersionState,
    IndexVersionValidationError, KeyDefinition, KeyDefinitionValidationError, KeyField,
    KeyMaterial, KeyMaterialValidationError, MetricKind, ObjectReference,
    ObjectReferenceValidationError, QueryHit, QueryHitValidationError, QueryRequest,
    QueryRequestValidationError, QueryResult, QueryResultValidationError, SchemaVersion,
    SchemaVersionValidationError, SimilarityEntry, SimilarityEntryValidationError, SourceVersion,
    SourceVersionValidationError, TargetReferenceType, TargetReferenceTypeValidationError,
    Uniqueness, VersionLifecycle, VersionLifecycleTransitionError, VersionValueValidationError,
};
pub use requirement::{IndexRequirement, IndexRequirementValidationError};

// -----------------------------------------------------------------------------
// Phase 4 consistency public surface
// -----------------------------------------------------------------------------

pub use consistency::{
    ConsistencyEvaluation, ConsistencyEvaluationState, ConsistencyMode, ConsistencyPolicyError,
    FreshnessEvaluation, FreshnessPolicy, FreshnessPolicyError, IndexSynchronizationState,
    SourceVersionSynchronizationState, SynchronizationError, SynchronizationEvaluation,
    SynchronizationSnapshot, VersioningError, validate_active_version_compatibility,
    validate_active_version_compatibility_result, validate_candidate_compatibility,
    validate_candidate_compatibility_result, validate_candidate_lineage,
    validate_candidate_lineage_result, validate_candidate_schema_compatibility,
    validate_candidate_schema_compatibility_result, validate_candidate_source_compatibility,
    validate_candidate_source_compatibility_result, validate_publication_eligibility,
    validate_publication_eligibility_result,
};

// -----------------------------------------------------------------------------
// Phase 4 shared provider public surface
// -----------------------------------------------------------------------------

pub use provider::{
    ProviderAvailability, ProviderCapabilities, ProviderCapability, ProviderCapabilityError,
    ProviderRankingError, RankingCandidate, RankingCandidateValidationError, RankingCriterion,
    RankingDirection, RankingError, RankingPolicy, rank,
};

// -----------------------------------------------------------------------------
// Phase 4 canonical query public surface
// -----------------------------------------------------------------------------
//
// QueryRequest / QueryHit / QueryResult remain re-exported above through
// `index` for backward compatibility. The additional Phase 4 contracts are
// surfaced here without creating duplicate root names.

pub use query::{
    AtomicQuery, AtomicQueryValidationError, CandidateValidationError, CapabilityResolution,
    ConsistencyMetadata, ConsistencyMetadataValidationError, ConsistencyState, ExactRetrievalPlan,
    FilteredRetrievalPlan, HybridQueryComponent, HybridQueryComponentValidationError,
    HybridRetrievalPlan, IndexCandidate, NeighborhoodRetrievalPlan, PlannedHybridComponent,
    PlannedIndex, ProviderRetriever, QueryCapabilityRequirement, QueryCapabilityRequirementError,
    QueryKind, QueryKindValidationError, QueryPlanningError, ResultMode, RetrievalError,
    RetrievalPlan, RetrievalPlanContext, RetrievalPlanValidationError, RetrievalResultError,
    SimilarityRetrievalPlan, StructuredRetrievalPlan, TextRetrievalPlan, execute, plan_query,
    validate_plan,
};
