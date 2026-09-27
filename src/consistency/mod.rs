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

#[cfg(test)]
mod tests {
    use super::*;

    use core::time::Duration;

    use crate::build::UpdateSequence;
    use crate::identity::{IndexDefinitionId, IndexDefinitionIdentity, IndexNamespace};
    use crate::index::{
        ConsistencyRequirement, IndexDefinition, IndexFamily, IndexVersion, IndexVersionId,
        KeyDefinition, SourceVersion, TargetReferenceType, Uniqueness, VersionLifecycle,
    };
    use nizaam_core::error::ErrorEvent;
    use nizaam_core::identity::{CorrelationId, OperationId};
    use nizaam_core::operation::{Operation, OperationContext};

    fn version_id(value: &str) -> IndexVersionId {
        IndexVersionId::new(value).expect("test version ID should be valid")
    }

    fn source_version(value: &str) -> SourceVersion {
        SourceVersion::new(value).expect("test source version should be valid")
    }

    fn version(value: &str) -> IndexVersion {
        IndexVersion::new(version_id(value))
    }

    fn version_with_source(value: &str, source: &str) -> IndexVersion {
        IndexVersion::with_metadata(version_id(value), Some(source_version(source)), None, None)
            .expect("test version metadata should be valid")
    }

    fn definition() -> IndexDefinition {
        IndexDefinition::new(
            IndexDefinitionIdentity::new(
                IndexDefinitionId::new("consistency.level2.test")
                    .expect("definition ID should be valid"),
                IndexNamespace::new("consistency.level2").expect("namespace should be valid"),
                IndexFamily::Inverted,
            ),
            KeyDefinition::new(["text"]).expect("key definition should be valid"),
            TargetReferenceType::new("object").expect("target reference type should be valid"),
            Uniqueness::NonUnique,
            ConsistencyRequirement::new("eventual")
                .expect("consistency requirement should be valid"),
            None,
            None,
        )
        .expect("definition should be valid")
    }

    fn context() -> nizaam_core::error::ErrorContext {
        nizaam_core::error::ErrorContext::new(OperationContext::new(Operation::new(
            OperationId::new("consistency-mod-level2-operation")
                .expect("test operation ID should be valid"),
            CorrelationId::new("consistency-mod-level2-correlation")
                .expect("test correlation ID should be valid"),
        )))
    }

    #[test]
    fn public_boundary_composes_phase3_versioning_and_phase4_consistency() {
        let candidate = version("candidate-v2");
        let active = version_id("active-v1");

        validate_candidate_compatibility(&candidate, &definition())
            .expect("candidate should satisfy the unconstrained definition");

        validate_candidate_lineage(&candidate, Some(&active), Some(&active))
            .expect("candidate derived from the active version should remain lineaged");

        let snapshot = SynchronizationSnapshot::from_sequences(
            UpdateSequence::new(10),
            UpdateSequence::new(10),
        )
        .expect("equal source/index sequences should be valid");

        let evaluation = snapshot
            .evaluate_with_policy(
                &ConsistencyMode::Current,
                &candidate,
                VersionLifecycle::Published,
            )
            .expect("published zero-lag state should satisfy current consistency");

        assert_eq!(evaluation.state(), ConsistencyEvaluationState::Fresh);
        assert_eq!(evaluation.update_sequence_lag(), Some(0));
    }

    #[test]
    fn current_policy_rejects_a_lagged_public_boundary_observation() {
        let snapshot = SynchronizationSnapshot::from_sequences(
            UpdateSequence::new(20),
            UpdateSequence::new(18),
        )
        .expect("indexed state behind source should be representable");

        assert_eq!(
            snapshot.state(),
            IndexSynchronizationState::Lagged { lag: 2 }
        );

        let error = snapshot
            .evaluate_with_policy(
                &ConsistencyMode::current(),
                &version("published-v1"),
                VersionLifecycle::Published,
            )
            .expect_err("Current must reject positive update-sequence lag");

        assert!(matches!(
            error,
            ConsistencyPolicyError::CurrentStateNotFresh { lag: 2 }
        ));
    }

    #[test]
    fn stale_allowed_composes_sequence_time_and_source_version_constraints() {
        let indexed_source = source_version("source-v1");
        let snapshot = SynchronizationSnapshot::from_sequences(
            UpdateSequence::new(25),
            UpdateSequence::new(23),
        )
        .expect("indexed state behind source should be valid")
        .with_time_lag(Duration::from_secs(2))
        .with_source_versions(
            Some(source_version("source-v1")),
            Some(indexed_source.clone()),
        );

        assert_eq!(
            snapshot.source_version_state(),
            SourceVersionSynchronizationState::Matching
        );

        let mode = ConsistencyMode::stale_allowed(
            FreshnessPolicy::new(2)
                .with_max_time_lag(Duration::from_secs(3))
                .with_required_source_version(indexed_source.clone()),
        );

        let evaluation = snapshot
            .evaluate_with_policy(
                &mode,
                &version_with_source("published-v3", "source-v1"),
                VersionLifecycle::Published,
            )
            .expect("all declared freshness constraints should pass");

        assert_eq!(
            evaluation.state(),
            ConsistencyEvaluationState::StaleAccepted
        );
        assert_eq!(evaluation.update_sequence_lag(), Some(2));
        assert_eq!(evaluation.time_lag(), Some(Duration::from_secs(2)));
        assert_eq!(evaluation.indexed_source_version(), Some(&indexed_source));
    }

    #[test]
    fn pinned_queries_can_use_public_sync_boundary_without_sequence_observations() {
        let requested = version_id("published-v7");
        let snapshot = SynchronizationSnapshot::new(None, None)
            .expect("missing sequence observations remain representable for pinned queries");

        let evaluation = snapshot
            .evaluate_with_policy(
                &ConsistencyMode::version_pinned(requested.clone()),
                &version("published-v7"),
                VersionLifecycle::Published,
            )
            .expect("exact pinned version should not require freshness observations");

        assert_eq!(
            evaluation.state(),
            ConsistencyEvaluationState::VersionPinned
        );
        assert_eq!(evaluation.mode().pinned_version(), Some(&requested));
        assert_eq!(evaluation.update_sequence_lag(), None);
    }

    #[test]
    fn public_boundary_never_makes_an_unpublished_version_queryable() {
        let snapshot = SynchronizationSnapshot::from_sequences(
            UpdateSequence::new(12),
            UpdateSequence::new(12),
        )
        .expect("equal sequences should be valid");

        let error = snapshot
            .evaluate_with_policy(
                &ConsistencyMode::current(),
                &version("candidate-v2"),
                VersionLifecycle::Ready,
            )
            .expect_err("READY candidates must not satisfy normal query consistency");

        assert!(matches!(
            error,
            ConsistencyPolicyError::UnqueryableVersion {
                lifecycle: VersionLifecycle::Ready,
                ..
            }
        ));
    }

    #[test]
    fn phase3_publication_eligibility_and_phase4_queryability_remain_distinct() {
        let candidate = version("candidate-v3");
        let active = version_id("active-v2");

        validate_publication_eligibility(&candidate, &definition(), Some(&active), Some(&active))
            .expect("matching Phase 3 lineage should permit publication eligibility");

        let snapshot = SynchronizationSnapshot::from_sequences(
            UpdateSequence::new(30),
            UpdateSequence::new(30),
        )
        .expect("equal sequences should be valid");

        let before_publication = snapshot
            .evaluate_with_policy(
                &ConsistencyMode::current(),
                &candidate,
                VersionLifecycle::Ready,
            )
            .expect_err("publication eligibility must not imply immediate query visibility");

        assert!(matches!(
            before_publication,
            ConsistencyPolicyError::UnqueryableVersion {
                lifecycle: VersionLifecycle::Ready,
                ..
            }
        ));

        let after_publication = snapshot
            .evaluate_with_policy(
                &ConsistencyMode::current(),
                &candidate,
                VersionLifecycle::Published,
            )
            .expect("a published zero-lag version should become queryable");

        assert_eq!(after_publication.state(), ConsistencyEvaluationState::Fresh);
    }

    #[test]
    fn phase3_lineage_staleness_is_not_the_same_as_phase4_query_staleness() {
        let candidate = version("candidate-v4");
        let original_active = version_id("active-v3");
        let newer_active = version_id("active-v4");

        let lineage_error =
            validate_candidate_lineage(&candidate, Some(&original_active), Some(&newer_active))
                .expect_err(
                    "candidate derived from an older active version must be stale for publication",
                );

        assert!(matches!(
            lineage_error,
            VersioningError::StaleCandidate { .. }
        ));

        let query_snapshot = SynchronizationSnapshot::from_sequences(
            UpdateSequence::new(40),
            UpdateSequence::new(40),
        )
        .expect("zero-lag query observation should be valid");

        let query_evaluation = query_snapshot
            .evaluate_with_policy(
                &ConsistencyMode::current(),
                &version("published-v4"),
                VersionLifecycle::Published,
            )
            .expect("query freshness is independently established from sequence observations");

        assert_eq!(query_evaluation.state(), ConsistencyEvaluationState::Fresh);
        assert_eq!(query_evaluation.update_sequence_lag(), Some(0));
    }

    #[test]
    fn public_boundary_preserves_the_existing_core_error_adapter() {
        let candidate = version("candidate-v5");
        let constrained_definition = IndexDefinition::new(
            IndexDefinitionIdentity::new(
                IndexDefinitionId::new("consistency.level2.core")
                    .expect("definition ID should be valid"),
                IndexNamespace::new("consistency.level2.core").expect("namespace should be valid"),
                IndexFamily::Inverted,
            ),
            KeyDefinition::new(["text"]).expect("key definition should be valid"),
            TargetReferenceType::new("object").expect("target reference type should be valid"),
            Uniqueness::NonUnique,
            ConsistencyRequirement::new("eventual")
                .expect("consistency requirement should be valid"),
            Some(source_version("source-v1")),
            None,
        )
        .expect("definition should be valid");

        let result = validate_candidate_source_compatibility_result(
            &candidate,
            constrained_definition.source_version(),
            context(),
        );

        match result {
            Ok(()) => panic!("candidate without the required source version must fail"),
            Err(event) => {
                assert_eq!(event.error().code.as_str(), "INDEXING.VERSIONING.001");
                assert_eq!(event.event_type(), "error");
                let _: ErrorEvent = event;
            }
        }
    }
}
