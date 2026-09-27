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

#[cfg(test)]
mod tests {
    use super::*;

    use crate::identity::{IndexDefinitionId, IndexDefinitionIdentity, IndexNamespace};
    use crate::index::{
        ConsistencyRequirement, IndexDefinition, IndexEntry, IndexFamily, IndexVersion,
        IndexVersionId, IndexVersionState, KeyDefinition, KeyMaterial, ObjectReference,
        SchemaVersion, SourceVersion, TargetReferenceType, Uniqueness, VersionLifecycle,
    };

    fn definition(source_version: Option<&str>, schema_version: Option<&str>) -> IndexDefinition {
        IndexDefinition::new(
            IndexDefinitionIdentity::new(
                IndexDefinitionId::new("documents.v1").expect("test definition ID should be valid"),
                IndexNamespace::new("search.documents").expect("test namespace should be valid"),
                IndexFamily::Inverted,
            ),
            KeyDefinition::new(["term"]).expect("test key definition should be valid"),
            TargetReferenceType::new("source.document")
                .expect("test target reference type should be valid"),
            Uniqueness::NonUnique,
            ConsistencyRequirement::new("logical-v1")
                .expect("test consistency requirement should be valid"),
            source_version.map(|value| {
                SourceVersion::new(value).expect("test source version should be valid")
            }),
            schema_version.map(|value| {
                SchemaVersion::new(value).expect("test schema version should be valid")
            }),
        )
        .expect("test definition should be valid")
    }

    fn version(
        value: &str,
        source_version: Option<&str>,
        schema_version: Option<&str>,
    ) -> IndexVersion {
        IndexVersion::with_metadata(
            IndexVersionId::new(value).expect("test version ID should be valid"),
            source_version.map(|source| {
                SourceVersion::new(source).expect("test source version should be valid")
            }),
            schema_version.map(|schema| {
                SchemaVersion::new(schema).expect("test schema version should be valid")
            }),
            None,
        )
        .expect("test version should be valid")
    }

    fn entry(key: &str, reference: &str) -> IndexEntry {
        IndexEntry::new(
            KeyMaterial::text(key),
            ObjectReference::new("documents", reference)
                .expect("test object reference should be valid"),
        )
        .expect("test entry should be valid")
    }

    #[test]
    fn public_boundary_validates_metadata_and_entry() {
        let validator = IntegrityValidator::new();
        let definition = definition(Some("source-v1"), Some("schema-v1"));
        let version = version("candidate-v1", Some("source-v1"), Some("schema-v1"));
        let entry = entry("alpha", "document:1");

        validate_definition(&definition)
            .expect("valid index metadata should pass through the public boundary");
        validate_version(&version)
            .expect("valid version metadata should pass through the public boundary");
        validate_entry(&entry).expect("valid entry should pass through the public boundary");

        validator
            .validate_index(&definition, &version, std::iter::once(&entry))
            .expect("complete logical index state should validate");
    }

    #[test]
    fn public_boundary_rejects_invalid_version_metadata() {
        let result = IndexVersion::with_metadata(
            IndexVersionId::new("candidate-v1").expect("test version ID should be valid"),
            Some(SourceVersion::new("source-v1").expect("test source version should be valid")),
            Some(SchemaVersion::new("schema-v1").expect("test schema version should be valid")),
            Some(KeyMaterial::text("invalid\nmetadata")),
        );

        assert!(
            result.is_err(),
            "invalid version metadata must be rejected before it reaches integrity validation"
        );
    }

    #[test]
    fn public_boundary_reports_reference_type_mismatch() {
        let expected = TargetReferenceType::new("source.document")
            .expect("expected reference type should be valid");
        let actual = TargetReferenceType::new("source.hadith")
            .expect("actual reference type should be valid");

        let error = IntegrityValidator::new()
            .validate_reference_type(&expected, &actual)
            .expect_err("different declared reference types must be rejected");

        assert!(matches!(
            error,
            IntegrityValidationError::ReferenceTypeMismatch { .. }
        ));
    }

    #[test]
    fn public_boundary_reports_version_mismatch() {
        let definition = definition(Some("source-v2"), Some("schema-v2"));
        let candidate = version("candidate-v1", Some("source-v1"), Some("schema-v1"));

        let error = validate_version_compatibility(&candidate, &definition)
            .expect_err("incompatible source/schema versions must be rejected");

        assert!(matches!(
            error,
            IntegrityValidationError::VersionCompatibility(_)
        ));
    }

    #[test]
    fn public_boundary_requires_ready_candidate_for_publication() {
        let definition = definition(None, None);
        let candidate = version("candidate-v1", None, None);
        let state = IndexVersionState::new(candidate.clone());

        let error = validate_publication_preconditions(&candidate, &state, &definition, None, None)
            .expect_err("building candidates must not satisfy publication preconditions");

        assert!(matches!(
            error,
            IntegrityValidationError::CandidateNotReady {
                lifecycle: VersionLifecycle::Building,
                ..
            }
        ));
    }

    #[test]
    fn public_boundary_accepts_ready_candidate_without_active_predecessor() {
        let definition = definition(None, None);
        let candidate = version("candidate-v1", None, None);
        let mut state = IndexVersionState::new(candidate.clone());

        state
            .transition_to(VersionLifecycle::Validating)
            .expect("candidate should transition to validating");
        state
            .mark_ready()
            .expect("candidate should transition to ready");

        validate_publication_preconditions(&candidate, &state, &definition, None, None)
            .expect("initial ready candidate should satisfy publication preconditions");
    }

    #[test]
    fn public_boundary_preserves_invalid_candidate_state_rejection() {
        let definition = definition(None, None);
        let candidate = version("candidate-v2", None, None);
        let base = IndexVersionId::new("active-v1").expect("base version should be valid");
        let active = IndexVersionId::new("active-v2").expect("active version should be valid");

        let mut state = IndexVersionState::new(candidate.clone());
        state
            .transition_to(VersionLifecycle::Validating)
            .expect("candidate should transition to validating");
        state
            .mark_ready()
            .expect("candidate should transition to ready");

        let error = validate_publication_preconditions(
            &candidate,
            &state,
            &definition,
            Some(&base),
            Some(&active),
        )
        .expect_err("stale candidate lineage must block publication");

        assert!(matches!(
            error,
            IntegrityValidationError::VersionCompatibility(_)
        ));
    }

    #[test]
    fn public_boundary_reexports_free_functions_and_validator_consistently() {
        let validator = IntegrityValidator::new();
        let reference = ObjectReference::new("documents", "document:42")
            .expect("test reference should be valid");

        validate_reference(&reference)
            .expect("free reference validator should accept a valid reference");
        validator
            .validate_reference(&reference)
            .expect("validator method should agree with the free function");
    }
}
