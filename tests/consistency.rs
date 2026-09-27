//! Level 3 integration coverage for Phase 3 versioning and Phase 4 query-time consistency rules.
//!
//! Phase 3 versioning tests are preserved unchanged. The additional Phase 4
//! tests exercise the public consistency policy and synchronization boundaries:
//! current/fresh queries, exact version pinning, stale-allowed freshness,
//! combined freshness constraints, synchronization observations, and the
//! guarantee that unpublished versions remain unqueryable.

use core::time::Duration;

use nizaam_indexing::build::UpdateSequence;
use nizaam_indexing::consistency::{
    ConsistencyEvaluationState, ConsistencyMode, ConsistencyPolicyError, FreshnessEvaluation,
    FreshnessPolicy, FreshnessPolicyError, IndexSynchronizationState,
    SourceVersionSynchronizationState, SynchronizationError, SynchronizationSnapshot,
    VersioningError, is_candidate_stale, validate_active_version_compatibility,
    validate_candidate_compatibility, validate_candidate_lineage,
    validate_candidate_schema_compatibility, validate_candidate_source_compatibility,
    validate_publication_eligibility,
};
use nizaam_indexing::identity::{IndexDefinitionId, IndexDefinitionIdentity, IndexNamespace};
use nizaam_indexing::index::{
    ConsistencyRequirement, IndexDefinition, IndexFamily, IndexVersion, IndexVersionId,
    KeyDefinition, SchemaVersion, SourceVersion, TargetReferenceType, Uniqueness, VersionLifecycle,
};

fn version_id(value: &str) -> IndexVersionId {
    IndexVersionId::new(value).expect("test version ID should be valid")
}

fn source_version(value: &str) -> SourceVersion {
    SourceVersion::new(value).expect("test source version should be valid")
}

fn schema_version(value: &str) -> SchemaVersion {
    SchemaVersion::new(value).expect("test schema version should be valid")
}

fn version(value: &str, source: Option<&str>, schema: Option<&str>) -> IndexVersion {
    IndexVersion::with_metadata(
        version_id(value),
        source.map(source_version),
        schema.map(schema_version),
        None,
    )
    .expect("test version should be valid")
}

fn definition(source: Option<&str>, schema: Option<&str>) -> IndexDefinition {
    IndexDefinition::new(
        IndexDefinitionIdentity::new(
            IndexDefinitionId::new("phase3.consistency.documents")
                .expect("definition ID should be valid"),
            IndexNamespace::new("phase3.consistency").expect("namespace should be valid"),
            IndexFamily::Inverted,
        ),
        KeyDefinition::new(["term"]).expect("key definition should be valid"),
        TargetReferenceType::new("documents.document")
            .expect("target reference type should be valid"),
        Uniqueness::NonUnique,
        ConsistencyRequirement::new("logical-v1").expect("consistency requirement should be valid"),
        source.map(source_version),
        schema.map(schema_version),
    )
    .expect("definition should be valid")
}

#[test]
fn source_version_mismatch_is_rejected_without_interpreting_version_order() {
    let candidate = version("candidate-v1", Some("source-v2"), Some("schema-v1"));
    let expected = source_version("source-v1");

    let error = validate_candidate_source_compatibility(&candidate, Some(&expected))
        .expect_err("different source versions must be rejected");

    assert!(matches!(
        error,
        VersioningError::SourceVersionMismatch { .. }
    ));
}

#[test]
fn schema_version_mismatch_is_rejected_exactly() {
    let candidate = version("candidate-v1", Some("source-v1"), Some("schema-v2"));
    let expected = schema_version("schema-v1");

    let error = validate_candidate_schema_compatibility(&candidate, Some(&expected))
        .expect_err("different schema versions must be rejected");

    assert!(matches!(
        error,
        VersioningError::SchemaVersionMismatch { .. }
    ));
}

#[test]
fn candidate_lineage_is_exactly_anchored_to_the_observed_active_version() {
    let candidate = version("candidate-v2", Some("source-v1"), Some("schema-v1"));
    let active = version_id("active-v1");

    validate_candidate_lineage(&candidate, Some(&active), Some(&active))
        .expect("matching base and active versions should be accepted");

    let newer_active = version_id("active-v2");
    let error = validate_candidate_lineage(&candidate, Some(&active), Some(&newer_active))
        .expect_err("a candidate based on an older active version must be stale");

    assert_eq!(
        error,
        VersioningError::StaleCandidate {
            candidate: version_id("candidate-v2"),
            base_version: Some(active),
            active_version: Some(newer_active),
        }
    );
}

#[test]
fn initial_candidate_requires_an_empty_lineage_before_first_publication() {
    let candidate = version("candidate-v1", None, None);

    assert!(!is_candidate_stale(None, None));
    validate_candidate_lineage(&candidate, None, None)
        .expect("an initial candidate should be valid without a predecessor");

    let active = version_id("active-v1");
    assert!(is_candidate_stale(None, Some(&active)));
    assert!(matches!(
        validate_candidate_lineage(&candidate, None, Some(&active)),
        Err(VersioningError::StaleCandidate { .. })
    ));
}

#[test]
fn multiple_candidates_can_be_valid_against_the_same_active_version_independently() {
    let active = version_id("active-v1");
    let candidate_a = version("candidate-v2", Some("source-v1"), Some("schema-v1"));
    let candidate_b = version("candidate-v3", Some("source-v1"), Some("schema-v1"));

    validate_active_version_compatibility(&candidate_a, Some(&active), Some(&active))
        .expect("first candidate should remain independently valid");
    validate_active_version_compatibility(&candidate_b, Some(&active), Some(&active))
        .expect("second candidate should remain independently valid");
}

#[test]
fn already_active_version_is_not_a_publication_target() {
    let active = version_id("active-v2");
    let candidate = version("active-v2", Some("source-v1"), Some("schema-v1"));

    let error = validate_active_version_compatibility(&candidate, Some(&active), Some(&active))
        .expect_err("the current active version cannot be republished as a candidate");

    assert_eq!(
        error,
        VersioningError::AlreadyActive {
            candidate: version_id("active-v2"),
        }
    );
}

#[test]
fn publication_eligibility_requires_both_metadata_and_lineage_to_match() {
    let active = version_id("active-v1");
    let candidate = version("candidate-v2", Some("source-v1"), Some("schema-v1"));
    let definition = definition(Some("source-v1"), Some("schema-v1"));

    validate_candidate_compatibility(&candidate, &definition)
        .expect("candidate metadata should satisfy the definition");
    validate_publication_eligibility(&candidate, &definition, Some(&active), Some(&active))
        .expect("matching metadata and lineage should be publication eligible");

    let newer_active = version_id("active-v2");
    assert!(matches!(
        validate_publication_eligibility(
            &candidate,
            &definition,
            Some(&active),
            Some(&newer_active),
        ),
        Err(VersioningError::StaleCandidate { .. })
    ));
}

#[test]
fn current_policy_accepts_a_published_zero_lag_state() {
    let selected = version("current-v1", Some("source-v1"), Some("schema-v1"));
    let mode = ConsistencyMode::Current;

    let evaluation = mode
        .evaluate(
            &selected,
            VersionLifecycle::Published,
            Some(12),
            Some(12),
            selected.source_version(),
            None,
        )
        .expect("zero-lag published state should satisfy Current");

    assert_eq!(evaluation.state(), ConsistencyEvaluationState::Fresh);
    assert_eq!(evaluation.source_update_sequence(), Some(12));
    assert_eq!(evaluation.indexed_update_sequence(), Some(12));
    assert_eq!(evaluation.update_sequence_lag(), Some(0));
    assert_eq!(
        evaluation
            .indexed_source_version()
            .map(SourceVersion::as_str),
        Some("source-v1")
    );
    assert_eq!(evaluation.time_lag(), None);
    assert_eq!(evaluation.mode(), &mode);
}

#[test]
fn current_policy_requires_an_update_sequence_observation() {
    let selected = version("current-v1", None, None);

    let error = ConsistencyMode::Current
        .evaluate(
            &selected,
            VersionLifecycle::Published,
            None,
            None,
            selected.source_version(),
            None,
        )
        .expect_err("Current must not be accepted without sequence observations");

    assert_eq!(error, ConsistencyPolicyError::MissingUpdateSequence);
}

#[test]
fn current_policy_rejects_positive_update_sequence_lag() {
    let selected = version("current-v1", None, None);

    let error = ConsistencyMode::Current
        .evaluate(
            &selected,
            VersionLifecycle::Published,
            Some(15),
            Some(14),
            selected.source_version(),
            None,
        )
        .expect_err("Current requires zero update-sequence lag");

    assert_eq!(
        error,
        ConsistencyPolicyError::CurrentStateNotFresh { lag: 1 }
    );
}

#[test]
fn current_policy_rejects_any_unpublished_version() {
    let selected = version("building-v1", None, None);

    let error = ConsistencyMode::Current
        .evaluate(
            &selected,
            VersionLifecycle::Building,
            Some(10),
            Some(10),
            selected.source_version(),
            None,
        )
        .expect_err("unpublished versions must never be queryable");

    assert_eq!(
        error,
        ConsistencyPolicyError::UnqueryableVersion {
            version: version_id("building-v1"),
            lifecycle: VersionLifecycle::Building,
        }
    );
}

#[test]
fn version_pinned_accepts_the_exact_published_version_without_sequence_observations() {
    let selected = version("pinned-v7", None, None);
    let mode = ConsistencyMode::version_pinned(version_id("pinned-v7"));

    let evaluation = mode
        .evaluate(
            &selected,
            VersionLifecycle::Published,
            None,
            None,
            selected.source_version(),
            None,
        )
        .expect("an exact pinned published version does not require freshness observations");

    assert_eq!(
        evaluation.state(),
        ConsistencyEvaluationState::VersionPinned
    );
    assert_eq!(evaluation.update_sequence_lag(), None);
    assert_eq!(evaluation.source_update_sequence(), None);
    assert_eq!(evaluation.indexed_update_sequence(), None);
    assert_eq!(evaluation.mode(), &mode);
}

#[test]
fn version_pinned_rejects_a_different_version_without_fallback() {
    let selected = version("selected-v7", None, None);
    let requested = version_id("requested-v7");

    let error = ConsistencyMode::version_pinned(requested.clone())
        .evaluate(
            &selected,
            VersionLifecycle::Published,
            Some(9),
            Some(9),
            selected.source_version(),
            None,
        )
        .expect_err("VersionPinned must require an exact version match");

    assert_eq!(
        error,
        ConsistencyPolicyError::PinnedVersionMismatch {
            requested,
            actual: version_id("selected-v7"),
        }
    );
}

#[test]
fn version_pinned_rejects_an_unpublished_exact_version() {
    let selected = version("pinned-v8", None, None);
    let mode = ConsistencyMode::version_pinned(version_id("pinned-v8"));

    let error = mode
        .evaluate(
            &selected,
            VersionLifecycle::Ready,
            None,
            None,
            selected.source_version(),
            None,
        )
        .expect_err("an exact match does not override the publication boundary");

    assert_eq!(
        error,
        ConsistencyPolicyError::UnqueryableVersion {
            version: version_id("pinned-v8"),
            lifecycle: VersionLifecycle::Ready,
        }
    );
}

#[test]
fn stale_allowed_accepts_lag_within_the_declared_sequence_bound() {
    let selected = version("stale-v1", Some("source-v1"), None);
    let mode = ConsistencyMode::stale_allowed(FreshnessPolicy::new(2));

    let evaluation = mode
        .evaluate(
            &selected,
            VersionLifecycle::Published,
            Some(10),
            Some(8),
            selected.source_version(),
            None,
        )
        .expect("lag within the freshness policy should be accepted");

    let freshness = FreshnessPolicy::new(2)
        .evaluate(Some(10), Some(8), None, selected.source_version())
        .expect("the lower-level freshness policy should accept the same observation");
    assert_eq!(freshness, FreshnessEvaluation::StaleAccepted);

    assert_eq!(
        evaluation.state(),
        ConsistencyEvaluationState::StaleAccepted
    );
    assert_eq!(evaluation.update_sequence_lag(), Some(2));
    assert_eq!(
        mode.freshness_policy().unwrap().max_update_sequence_lag(),
        2
    );
}

#[test]
fn stale_allowed_rejects_lag_outside_the_declared_sequence_bound() {
    let selected = version("stale-v2", None, None);
    let mode = ConsistencyMode::stale_allowed(FreshnessPolicy::new(2));

    let error = mode
        .evaluate(
            &selected,
            VersionLifecycle::Published,
            Some(10),
            Some(7),
            selected.source_version(),
            None,
        )
        .expect_err("lag above the policy bound must be rejected");

    assert_eq!(
        error,
        ConsistencyPolicyError::Freshness(FreshnessPolicyError::UpdateSequenceLagExceeded {
            lag: 3,
            max_lag: 2,
        })
    );
}

#[test]
fn stale_allowed_requires_a_time_observation_when_a_time_bound_is_declared() {
    let selected = version("stale-v3", None, None);
    let mode = ConsistencyMode::stale_allowed(
        FreshnessPolicy::new(2).with_max_time_lag(Duration::from_secs(60)),
    );

    let error = mode
        .evaluate(
            &selected,
            VersionLifecycle::Published,
            Some(10),
            Some(9),
            selected.source_version(),
            None,
        )
        .expect_err("a declared time bound requires an observed time lag");

    assert_eq!(
        error,
        ConsistencyPolicyError::Freshness(FreshnessPolicyError::MissingTimeLag)
    );
}

#[test]
fn stale_allowed_rejects_excessive_time_lag_even_when_sequence_lag_is_acceptable() {
    let selected = version("stale-v4", None, None);
    let mode = ConsistencyMode::stale_allowed(
        FreshnessPolicy::new(2).with_max_time_lag(Duration::from_secs(60)),
    );

    let observed = Duration::from_secs(61);
    let error = mode
        .evaluate(
            &selected,
            VersionLifecycle::Published,
            Some(10),
            Some(9),
            selected.source_version(),
            Some(observed),
        )
        .expect_err("every declared freshness constraint must pass");

    assert_eq!(
        error,
        ConsistencyPolicyError::Freshness(FreshnessPolicyError::TimeLagExceeded {
            lag: observed,
            max_lag: Duration::from_secs(60),
        })
    );
}

#[test]
fn stale_allowed_requires_and_checks_declared_source_version_compatibility() {
    let required = source_version("source-v2");
    let selected_without_source = version("stale-v5", None, None);

    let mode = ConsistencyMode::stale_allowed(
        FreshnessPolicy::new(2).with_required_source_version(required.clone()),
    );

    let missing = mode
        .evaluate(
            &selected_without_source,
            VersionLifecycle::Published,
            Some(10),
            Some(10),
            selected_without_source.source_version(),
            None,
        )
        .expect_err("a required source version cannot be satisfied by missing metadata");

    assert_eq!(
        missing,
        ConsistencyPolicyError::Freshness(FreshnessPolicyError::MissingIndexedSourceVersion {
            required: required.clone(),
        })
    );

    let mismatched = version("stale-v6", Some("source-v1"), None);
    let mismatch = mode
        .evaluate(
            &mismatched,
            VersionLifecycle::Published,
            Some(10),
            Some(10),
            mismatched.source_version(),
            None,
        )
        .expect_err("a mismatched source version must be rejected");

    assert_eq!(
        mismatch,
        ConsistencyPolicyError::Freshness(FreshnessPolicyError::SourceVersionMismatch {
            required,
            actual: source_version("source-v1"),
        })
    );
}

#[test]
fn stale_allowed_combines_sequence_time_and_source_constraints_conjunctively() {
    let mode = ConsistencyMode::stale_allowed(
        FreshnessPolicy::new(2)
            .with_max_time_lag(Duration::from_secs(120))
            .with_required_source_version(source_version("source-v3")),
    );
    let selected = version("stale-v7", Some("source-v3"), None);

    let accepted = mode
        .evaluate(
            &selected,
            VersionLifecycle::Published,
            Some(10),
            Some(8),
            selected.source_version(),
            Some(Duration::from_secs(90)),
        )
        .expect("all declared freshness constraints should pass");

    assert_eq!(accepted.state(), ConsistencyEvaluationState::StaleAccepted);
    assert_eq!(accepted.update_sequence_lag(), Some(2));
    assert_eq!(accepted.time_lag(), Some(Duration::from_secs(90)));

    let time_violation = mode
        .evaluate(
            &selected,
            VersionLifecycle::Published,
            Some(10),
            Some(8),
            selected.source_version(),
            Some(Duration::from_secs(121)),
        )
        .expect_err("one violated freshness dimension must reject the query");

    assert!(matches!(
        time_violation,
        ConsistencyPolicyError::Freshness(FreshnessPolicyError::TimeLagExceeded { .. })
    ));
}

#[test]
fn synchronization_snapshot_reports_sequence_lag_time_lag_and_source_version_state() {
    let snapshot =
        SynchronizationSnapshot::from_sequences(UpdateSequence::new(10), UpdateSequence::new(7))
            .expect("source/index sequence ordering should be valid")
            .with_time_lag(Duration::from_secs(30))
            .with_source_versions(
                Some(source_version("source-v4")),
                Some(source_version("source-v4")),
            );

    assert_eq!(snapshot.update_sequence_lag(), Some(3));
    assert_eq!(
        snapshot.state(),
        IndexSynchronizationState::Lagged { lag: 3 }
    );
    assert_eq!(
        snapshot.source_version_state(),
        SourceVersionSynchronizationState::Matching
    );
    assert_eq!(snapshot.time_lag(), Some(Duration::from_secs(30)));
    assert_eq!(
        snapshot.source_version().map(SourceVersion::as_str),
        Some("source-v4")
    );
    assert_eq!(
        snapshot.indexed_source_version().map(SourceVersion::as_str),
        Some("source-v4")
    );

    let evaluation = snapshot.evaluate();
    assert_eq!(
        evaluation.state(),
        IndexSynchronizationState::Lagged { lag: 3 }
    );
    assert_eq!(
        evaluation.source_version_state(),
        SourceVersionSynchronizationState::Matching
    );
}

#[test]
fn synchronization_snapshot_rejects_an_index_sequence_ahead_of_the_source() {
    let error =
        SynchronizationSnapshot::from_sequences(UpdateSequence::new(4), UpdateSequence::new(5))
            .expect_err("an indexed sequence ahead of source cannot define a valid lag boundary");

    assert_eq!(
        error,
        SynchronizationError::IndexedSequenceAhead {
            source: UpdateSequence::new(4),
            indexed: UpdateSequence::new(5),
        }
    );
}

#[test]
fn synchronization_policy_bridge_applies_the_declared_consistency_rules() {
    let snapshot =
        SynchronizationSnapshot::from_sequences(UpdateSequence::new(12), UpdateSequence::new(11))
            .expect("sequence snapshot should be valid")
            .with_time_lag(Duration::from_secs(20))
            .with_source_version(source_version("source-v5"))
            .with_indexed_source_version(source_version("source-v5"));

    let selected = version("bridge-v1", Some("source-v5"), None);
    let mode = ConsistencyMode::stale_allowed(
        FreshnessPolicy::new(2)
            .with_max_time_lag(Duration::from_secs(60))
            .with_required_source_version(source_version("source-v5")),
    );

    let evaluation = snapshot
        .evaluate_with_policy(&mode, &selected, VersionLifecycle::Published)
        .expect("the synchronization bridge should delegate to policy evaluation");

    assert_eq!(
        evaluation.state(),
        ConsistencyEvaluationState::StaleAccepted
    );
    assert_eq!(evaluation.source_update_sequence(), Some(12));
    assert_eq!(evaluation.indexed_update_sequence(), Some(11));
    assert_eq!(evaluation.update_sequence_lag(), Some(1));
    assert_eq!(evaluation.time_lag(), Some(Duration::from_secs(20)));
}

#[test]
fn missing_sequence_observations_are_allowed_by_synchronization_but_rejected_by_freshness_policy() {
    let snapshot = SynchronizationSnapshot::new(None, None)
        .expect("missing sequence observations are a valid synchronization snapshot");

    assert_eq!(snapshot.state(), IndexSynchronizationState::Unknown);
    assert!(!snapshot.has_update_sequence_observation());
    assert_eq!(snapshot.update_sequence_lag(), None);

    let selected = version("freshness-v1", None, None);
    let mode = ConsistencyMode::stale_allowed(FreshnessPolicy::new(0));

    let error = snapshot
        .evaluate_with_policy(&mode, &selected, VersionLifecycle::Published)
        .expect_err("freshness policy requires concrete sequence observations");

    assert_eq!(
        error,
        ConsistencyPolicyError::Freshness(FreshnessPolicyError::MissingUpdateSequence)
    );
}
