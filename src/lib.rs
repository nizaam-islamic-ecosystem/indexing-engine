//! Public library boundary for the Nizaam Indexing Engine.
//!
//! The crate root exposes the established Indexing module tree and the public
//! logical contracts implemented across Phase 1, Phase 2, and Phase 3.
//!
//! The public boundary keeps ownership explicit:
//! - `identity` defines Indexing identities and logical namespaces.
//! - `index` defines logical index families, logical index contracts, and the
//!   Phase 3 index-version lifecycle contract.
//! - `requirement` defines the source-to-Indexing requirement contract.
//! - Core-owned runtime, capability, lifecycle, contract, and execution
//!   infrastructure remains owned by `nizaam-core` and is surfaced here only
//!   through the Indexing engine API where required.
//!
//! Physical storage, index construction, retrieval algorithms, provider
//! implementations, and domain semantics remain outside these logical
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phase3_version_lifecycle_contract_is_publicly_reachable() {
        let version_id =
            IndexVersionId::new("crate-root-phase3-v1").expect("test version ID should be valid");
        let version = IndexVersion::new(version_id);
        let mut state = IndexVersionState::new(version);

        assert_eq!(state.lifecycle(), VersionLifecycle::Building);
        assert!(state.is_candidate());
        assert!(!state.is_published());

        state
            .transition_to(VersionLifecycle::Validating)
            .expect("building should transition to validating");
        state
            .mark_ready()
            .expect("validating should transition to ready");

        assert_eq!(state.lifecycle(), VersionLifecycle::Ready);
        assert!(VersionLifecycle::Ready.can_transition_to(VersionLifecycle::Published));

        // Compile-time reachability checks for the newly exported public error
        // types without constructing an invalid value.
        let _: Option<VersionLifecycleTransitionError> = None;
        let _: Option<VersionValueValidationError> = None;
    }
}
