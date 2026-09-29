//! Foundational index-family module boundary.
//!
//! This module exposes the logical families of indexes supported by the
//! indexing subsystem. The family definitions live in [`family`] so this
//! module remains focused on module declarations, public exports, and
//! module-boundary tests.
//! Phase 1 families:
//! - [`IndexFamily::Identity`]
//! - [`IndexFamily::Inverted`]
//! - [`IndexFamily::Relationship`]
//! - [`IndexFamily::Similarity`]

// Phase 2 logical index contracts.
pub mod definition;
pub mod entry;
pub mod key;
pub mod query;
pub mod reference;
pub mod similarity;
pub mod version;

pub use definition::{
    ConsistencyRequirement, ConsistencyRequirementValidationError, IndexDefinition,
    IndexDefinitionValidationError, TargetReferenceType, TargetReferenceTypeValidationError,
    Uniqueness,
};
pub use entry::{IndexEntry, IndexEntryValidationError};
pub use key::{
    KeyDefinition, KeyDefinitionValidationError, KeyField, KeyMaterial, KeyMaterialValidationError,
};
pub use query::{
    MetricKind, QueryHit, QueryHitValidationError, QueryRequest, QueryRequestValidationError,
    QueryResult, QueryResultValidationError,
};
pub use reference::{ObjectReference, ObjectReferenceValidationError};
pub use similarity::{SimilarityEntry, SimilarityEntryValidationError};
pub use version::{
    IndexVersion, IndexVersionId, IndexVersionIdValidationError, IndexVersionState,
    IndexVersionValidationError, SchemaVersion, SchemaVersionValidationError, SourceVersion,
    SourceVersionValidationError, VersionLifecycle, VersionLifecycleTransitionError,
    VersionValueValidationError,
};

pub mod family;

pub use family::IndexFamily;

#[cfg(test)]
mod phase_two_module_tests {
    use super::*;

    #[test]
    fn phase_two_logical_index_contracts_are_publicly_reachable() {
        let namespace =
            crate::identity::IndexNamespace::new("quran").expect("test namespace should be valid");
        let definition_id = crate::identity::IndexDefinitionId::new("verse-term")
            .expect("test definition id should be valid");
        let identity = crate::identity::IndexDefinitionIdentity::new(
            definition_id,
            namespace,
            IndexFamily::Inverted,
        );

        let key_definition =
            KeyDefinition::new(["term"]).expect("test key definition should be valid");
        let target_type =
            TargetReferenceType::new("quran-verse").expect("test target type should be valid");
        let consistency =
            ConsistencyRequirement::new("logical").expect("test consistency should be valid");

        let definition = IndexDefinition::new(
            identity,
            key_definition,
            target_type,
            Uniqueness::NonUnique,
            consistency,
            None,
            None,
        )
        .expect("test definition should be valid");

        assert_eq!(definition.family(), IndexFamily::Inverted);
    }

    #[test]
    fn entry_and_reference_exports_are_connected() {
        let reference =
            ObjectReference::new("quran", "verse:2:255").expect("test reference should be valid");
        let entry = IndexEntry::new(KeyMaterial::text("term"), reference)
            .expect("test entry should be valid");

        assert_eq!(entry.key(), &KeyMaterial::text("term"));
    }

    #[test]
    fn phase_three_version_lifecycle_exports_are_connected() {
        let version =
            IndexVersion::new(IndexVersionId::new("v2").expect("test version id should be valid"));
        let mut state = IndexVersionState::new(version);

        assert_eq!(state.lifecycle(), VersionLifecycle::Building);
        state
            .transition_to(VersionLifecycle::Validating)
            .expect("building should transition to validating");
        state
            .mark_ready()
            .expect("validating should transition to ready");

        assert_eq!(state.lifecycle(), VersionLifecycle::Ready);
        assert!(VersionLifecycle::Ready.can_transition_to(VersionLifecycle::Published));
        assert!(!VersionLifecycle::Published.can_transition_to(VersionLifecycle::Ready));

        let _transition_error_type: Option<VersionLifecycleTransitionError> = None;
        let _validation_error_type: Option<VersionValueValidationError> = None;
    }

    #[test]
    fn query_and_similarity_exports_are_connected() {
        let reference =
            ObjectReference::new("quran", "verse:2:255").expect("test reference should be valid");

        let similarity = SimilarityEntry::new(KeyMaterial::bytes(vec![1, 2, 3]), reference.clone())
            .expect("test similarity entry should be valid");

        let version_id = IndexVersionId::new("v1").expect("test version id should be valid");
        let version = IndexVersion::new(version_id);

        let definition_id = crate::identity::IndexDefinitionId::new("query-export-definition")
            .expect("test definition id should be valid");
        let namespace =
            crate::identity::IndexNamespace::new("quran").expect("test namespace should be valid");
        let definition_identity = crate::identity::IndexDefinitionIdentity::new(
            definition_id,
            namespace,
            IndexFamily::Inverted,
        );

        let request = QueryRequest::new(definition_identity.clone(), KeyMaterial::text("term"))
            .expect("test query request should be valid");

        let hit = QueryHit::new(reference);
        let result = QueryResult::new(definition_identity, version, vec![hit])
            .expect("test query result should be valid");

        assert_eq!(request.query(), &KeyMaterial::text("term"));
        assert_eq!(similarity.metadata(), None);
        assert_eq!(result.hits().len(), 1);
    }
}
