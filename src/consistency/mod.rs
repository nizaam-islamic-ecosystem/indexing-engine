//! Public consistency boundary for Phase 3 versioning rules.
//!
//! This module composes the Indexing-owned version-consistency implementation
//! and exposes its public contract through one stable module boundary.
//!
//! The child [`versioning`] module owns the logical checks themselves:
//! source/schema compatibility, candidate lineage, active-version
//! compatibility, and publication eligibility. This module intentionally does
//! not introduce a second consistency model or duplicate any of those rules.
//!
//! Core remains the owner of universal runtime, execution context, error-event,
//! and lifecycle infrastructure. Indexing only exposes its version-specific
//! logical consistency rules here.

pub mod versioning;

pub use versioning::{
    VersioningError, is_candidate_stale, validate_active_version_compatibility,
    validate_active_version_compatibility_result, validate_candidate_compatibility,
    validate_candidate_compatibility_result, validate_candidate_lineage,
    validate_candidate_lineage_result, validate_candidate_schema_compatibility,
    validate_candidate_schema_compatibility_result, validate_candidate_source_compatibility,
    validate_candidate_source_compatibility_result, validate_publication_eligibility,
    validate_publication_eligibility_result,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::{IndexDefinitionId, IndexDefinitionIdentity, IndexNamespace};
    use crate::index::{
        ConsistencyRequirement, IndexDefinition, IndexFamily, IndexVersion, IndexVersionId,
        KeyDefinition, TargetReferenceType, Uniqueness,
    };
    use nizaam_core::error::ErrorEvent;
    use nizaam_core::identity::{CorrelationId, OperationId};
    use nizaam_core::operation::{Operation, OperationContext};

    fn version_id(value: &str) -> IndexVersionId {
        IndexVersionId::new(value).expect("test version ID should be valid")
    }

    fn candidate(value: &str) -> IndexVersion {
        IndexVersion::new(version_id(value))
    }

    fn definition() -> IndexDefinition {
        definition_with_versions(None)
    }

    fn definition_with_versions(source_version: Option<&str>) -> IndexDefinition {
        IndexDefinition::new(
            IndexDefinitionIdentity::new(
                IndexDefinitionId::new("consistency.test").expect("definition ID should be valid"),
                IndexNamespace::new("consistency").expect("namespace should be valid"),
                IndexFamily::Inverted,
            ),
            KeyDefinition::new(["text"]).expect("key definition should be valid"),
            TargetReferenceType::new("object").expect("target reference type should be valid"),
            Uniqueness::NonUnique,
            ConsistencyRequirement::new("eventual")
                .expect("consistency requirement should be valid"),
            source_version.map(|value| {
                crate::index::SourceVersion::new(value)
                    .expect("test source version should be valid")
            }),
            None,
        )
        .expect("definition should be valid")
    }

    fn context() -> nizaam_core::error::ErrorContext {
        nizaam_core::error::ErrorContext::new(OperationContext::new(Operation::new(
            OperationId::new("consistency-mod-test-operation")
                .expect("test operation ID should be valid"),
            CorrelationId::new("consistency-mod-test-correlation")
                .expect("test correlation ID should be valid"),
        )))
    }

    #[test]
    fn consistency_boundary_reexports_versioning_and_lineage_rules() {
        let candidate = candidate("candidate-v2");
        let active = version_id("active-v1");

        assert!(!is_candidate_stale(Some(&active), Some(&active)));
        assert!(is_candidate_stale(None, Some(&active)));

        validate_candidate_compatibility(&candidate, &definition())
            .expect("candidate should satisfy an unconstrained definition");

        validate_active_version_compatibility(&candidate, Some(&active), Some(&active))
            .expect("candidate derived from the current active version should be compatible");

        let stale =
            validate_candidate_lineage(&candidate, Some(&active), Some(&version_id("active-v2")))
                .expect_err("candidate must be rejected after active lineage changes");

        assert!(matches!(stale, VersioningError::StaleCandidate { .. }));
    }

    #[test]
    fn consistency_boundary_combines_definition_and_publication_checks() {
        let candidate = candidate("candidate-v2");
        let active = version_id("active-v1");
        let definition = definition();

        validate_publication_eligibility(&candidate, &definition, Some(&active), Some(&active))
            .expect("matching logical lineage should be publication-eligible");

        let stale_active = version_id("active-v2");
        let error = validate_publication_eligibility(
            &candidate,
            &definition,
            Some(&active),
            Some(&stale_active),
        )
        .expect_err("a candidate based on an older active version must be rejected");

        assert!(matches!(error, VersioningError::StaleCandidate { .. }));
    }

    #[test]
    fn consistency_boundary_exposes_core_result_adapter() {
        let candidate = candidate("candidate-v1");
        let constrained_definition = definition_with_versions(Some("source-v1"));

        let result = validate_candidate_source_compatibility_result(
            &candidate,
            constrained_definition.source_version(),
            context(),
        );

        match result {
            Ok(()) => panic!("a candidate missing the required source version must fail"),
            Err(event) => {
                assert_eq!(event.error().code.as_str(), "INDEXING.VERSIONING.001");
                assert_eq!(event.event_type(), "error");
                let _: ErrorEvent = event;
            }
        }
    }
}
