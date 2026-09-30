//! Phase 5 integration tests for Indexing failure classification and recovery.
//!
//! These tests verify the deterministic recovery boundary without introducing
//! a second retry system, scheduler, or active-version registry.

use nizaam_indexing::{
    ClassifiedFailure, FailureClass, FailureClassifier, IndexVersionId, RecoveryAction,
    RecoveryExecutionError, RecoveryExecutor, RecoveryHandler, RecoveryOutcome, RecoveryRequest,
    action_for, classify, execute_recovery,
};

#[derive(Default)]
struct RecordingHandler {
    actions: Vec<RecoveryAction>,
    observed_active_versions: Vec<Option<IndexVersionId>>,
    fail_action: Option<RecoveryAction>,
}

impl RecordingHandler {
    fn record(
        &mut self,
        action: RecoveryAction,
        request: &RecoveryRequest,
    ) -> Result<(), RecoveryExecutionError> {
        self.actions.push(action);
        self.observed_active_versions
            .push(request.active_version().cloned());

        if self.fail_action == Some(action) {
            return Err(RecoveryExecutionError::new(
                action,
                "simulated recovery handler failure",
            ));
        }

        Ok(())
    }
}

impl RecoveryHandler for RecordingHandler {
    fn synchronize(&mut self, request: &RecoveryRequest) -> Result<(), RecoveryExecutionError> {
        self.record(RecoveryAction::Synchronize, request)
    }

    fn rebuild(&mut self, request: &RecoveryRequest) -> Result<(), RecoveryExecutionError> {
        self.record(RecoveryAction::Rebuild, request)
    }

    fn invalidate_and_rebuild(
        &mut self,
        request: &RecoveryRequest,
    ) -> Result<(), RecoveryExecutionError> {
        self.record(RecoveryAction::InvalidateAndRebuild, request)
    }

    fn restore_availability(
        &mut self,
        request: &RecoveryRequest,
    ) -> Result<(), RecoveryExecutionError> {
        self.record(RecoveryAction::RestoreAvailability, request)
    }

    fn throttle(&mut self, request: &RecoveryRequest) -> Result<(), RecoveryExecutionError> {
        self.record(RecoveryAction::Throttle, request)
    }

    fn delegate_to_core_retry(
        &mut self,
        request: &RecoveryRequest,
    ) -> Result<(), RecoveryExecutionError> {
        self.record(RecoveryAction::DelegateToCoreRetry, request)
    }

    fn preserve_safe_state(
        &mut self,
        request: &RecoveryRequest,
    ) -> Result<(), RecoveryExecutionError> {
        self.record(RecoveryAction::PreserveSafeState, request)
    }

    fn surface_query_failure(
        &mut self,
        request: &RecoveryRequest,
    ) -> Result<(), RecoveryExecutionError> {
        self.record(RecoveryAction::SurfaceQueryFailure, request)
    }
}

fn version_id(value: &str) -> IndexVersionId {
    IndexVersionId::new(value).expect("test version id should be valid")
}

#[test]
fn all_phase5_failure_classes_are_distinct() {
    assert_eq!(FailureClass::ALL.len(), 8);

    for (index, class) in FailureClass::ALL.iter().enumerate() {
        assert!(
            FailureClass::ALL[..index]
                .iter()
                .all(|previous| previous != class),
            "failure class {class:?} should be unique"
        );
    }
}

#[test]
fn failure_classification_preserves_the_indexing_failure_kind() {
    let classifier = FailureClassifier::new();

    for class in FailureClass::ALL {
        let classified = classifier.classify(class);

        assert_eq!(classified.class(), class);
        assert_eq!(classified, ClassifiedFailure::new(class));
        assert_eq!(classify(class), classified);
    }
}

#[test]
fn retryability_is_an_adapter_not_a_retry_decision() {
    for class in FailureClass::ALL {
        let classified = classify(class);

        assert_eq!(classified.retryability(), class.retryability());
        assert_eq!(classified.may_retry(), class.may_retry());
    }

    assert!(!FailureClass::InvalidIndex.may_retry());
    assert!(!FailureClass::StaleIndex.may_retry());
    assert!(!FailureClass::CorruptIndex.may_retry());
    assert!(!FailureClass::SourceDataFailure.may_retry());
    assert!(!FailureClass::ResourceExhaustion.may_retry());

    assert!(FailureClass::UnavailableIndex.may_retry());
    assert!(FailureClass::ProviderFailure.may_retry());
    assert!(FailureClass::QueryFailure.may_retry());
}

#[test]
fn every_failure_class_maps_to_the_scope_defined_recovery_action() {
    let expected = [
        (FailureClass::StaleIndex, RecoveryAction::Synchronize),
        (FailureClass::CorruptIndex, RecoveryAction::Rebuild),
        (
            FailureClass::InvalidIndex,
            RecoveryAction::InvalidateAndRebuild,
        ),
        (
            FailureClass::UnavailableIndex,
            RecoveryAction::RestoreAvailability,
        ),
        (FailureClass::ResourceExhaustion, RecoveryAction::Throttle),
        (
            FailureClass::ProviderFailure,
            RecoveryAction::DelegateToCoreRetry,
        ),
        (
            FailureClass::SourceDataFailure,
            RecoveryAction::PreserveSafeState,
        ),
        (
            FailureClass::QueryFailure,
            RecoveryAction::SurfaceQueryFailure,
        ),
    ];

    for (class, action) in expected {
        assert_eq!(action_for(class), action);
        assert_eq!(RecoveryExecutor::new().action_for(class), action);
    }
}

#[test]
fn recovery_request_keeps_active_version_as_observational_lineage() {
    let active = version_id("active-v7");
    let request = RecoveryRequest::new(classify(FailureClass::CorruptIndex))
        .with_active_version(active.clone());

    assert_eq!(request.failure().class(), FailureClass::CorruptIndex);
    assert_eq!(request.active_version(), Some(&active));
    assert_eq!(request.action(), RecoveryAction::Rebuild);
}

#[test]
fn executor_dispatches_each_recovery_action_to_the_matching_handler() {
    let cases = [
        (FailureClass::StaleIndex, RecoveryAction::Synchronize),
        (FailureClass::CorruptIndex, RecoveryAction::Rebuild),
        (
            FailureClass::InvalidIndex,
            RecoveryAction::InvalidateAndRebuild,
        ),
        (
            FailureClass::UnavailableIndex,
            RecoveryAction::RestoreAvailability,
        ),
        (FailureClass::ResourceExhaustion, RecoveryAction::Throttle),
        (
            FailureClass::ProviderFailure,
            RecoveryAction::DelegateToCoreRetry,
        ),
        (
            FailureClass::SourceDataFailure,
            RecoveryAction::PreserveSafeState,
        ),
        (
            FailureClass::QueryFailure,
            RecoveryAction::SurfaceQueryFailure,
        ),
    ];

    for (class, expected_action) in cases {
        let request = RecoveryRequest::new(classify(class));
        let mut handler = RecordingHandler::default();

        let outcome = RecoveryExecutor::new()
            .execute(&request, &mut handler)
            .expect("matching recovery handler should succeed");

        assert_eq!(outcome.action(), expected_action);
        assert_eq!(handler.actions, vec![expected_action]);
        assert_eq!(handler.observed_active_versions, vec![None]);
    }
}

#[test]
fn execute_recovery_preserves_the_known_good_active_version() {
    let active = version_id("active-v7");
    let mut handler = RecordingHandler::default();

    let outcome = execute_recovery(
        classify(FailureClass::CorruptIndex),
        Some(active.clone()),
        &mut handler,
    )
    .expect("rebuild dispatch should succeed");

    assert_eq!(outcome, RecoveryOutcome::Rebuilt);
    assert_eq!(handler.actions, vec![RecoveryAction::Rebuild]);
    assert_eq!(handler.observed_active_versions, vec![Some(active.clone())]);

    // The recovery API exposes no mutation operation for the active version.
    // The handler only observes the same known-good lineage value.
    assert_eq!(handler.observed_active_versions[0].as_ref(), Some(&active));
}

#[test]
fn source_data_failure_preserves_safe_state() {
    let active = version_id("active-v7");
    let request = RecoveryRequest::new(classify(FailureClass::SourceDataFailure))
        .with_active_version(active.clone());
    let mut handler = RecordingHandler::default();

    let outcome = RecoveryExecutor::new()
        .execute(&request, &mut handler)
        .expect("safe-state preservation should be dispatchable");

    assert_eq!(outcome, RecoveryOutcome::SafeStatePreserved);
    assert_eq!(handler.actions, vec![RecoveryAction::PreserveSafeState]);
    assert_eq!(handler.observed_active_versions, vec![Some(active)]);
}

#[test]
fn provider_failure_is_delegated_to_core_retry_boundary() {
    let request = RecoveryRequest::new(classify(FailureClass::ProviderFailure));
    let mut handler = RecordingHandler::default();

    let outcome = RecoveryExecutor::new()
        .execute(&request, &mut handler)
        .expect("provider delegation should succeed");

    assert_eq!(outcome, RecoveryOutcome::DelegatedToCoreRetry);
    assert_eq!(handler.actions, vec![RecoveryAction::DelegateToCoreRetry]);
}

#[test]
fn resource_exhaustion_selects_bounded_throttle_action() {
    let request = RecoveryRequest::new(classify(FailureClass::ResourceExhaustion));
    let mut handler = RecordingHandler::default();

    let outcome = RecoveryExecutor::new()
        .execute(&request, &mut handler)
        .expect("resource-exhaustion handling should succeed");

    assert_eq!(outcome, RecoveryOutcome::Throttled);
    assert_eq!(handler.actions, vec![RecoveryAction::Throttle]);
}

#[test]
fn handler_failure_is_propagated_without_retrying_automatically() {
    let request = RecoveryRequest::new(classify(FailureClass::CorruptIndex));
    let mut handler = RecordingHandler {
        fail_action: Some(RecoveryAction::Rebuild),
        ..RecordingHandler::default()
    };

    let error = RecoveryExecutor::new()
        .execute(&request, &mut handler)
        .expect_err("simulated rebuild failure should be returned");

    assert_eq!(error.action(), RecoveryAction::Rebuild);
    assert_eq!(error.message(), "simulated recovery handler failure");
    assert_eq!(handler.actions, vec![RecoveryAction::Rebuild]);
}

#[test]
fn recovery_outcomes_map_back_to_their_selected_actions() {
    let outcomes = [
        (RecoveryOutcome::Synchronized, RecoveryAction::Synchronize),
        (RecoveryOutcome::Rebuilt, RecoveryAction::Rebuild),
        (
            RecoveryOutcome::InvalidatedAndRebuildRequested,
            RecoveryAction::InvalidateAndRebuild,
        ),
        (
            RecoveryOutcome::AvailabilityRestored,
            RecoveryAction::RestoreAvailability,
        ),
        (RecoveryOutcome::Throttled, RecoveryAction::Throttle),
        (
            RecoveryOutcome::DelegatedToCoreRetry,
            RecoveryAction::DelegateToCoreRetry,
        ),
        (
            RecoveryOutcome::SafeStatePreserved,
            RecoveryAction::PreserveSafeState,
        ),
        (
            RecoveryOutcome::QueryFailureSurfaced,
            RecoveryAction::SurfaceQueryFailure,
        ),
    ];

    for (outcome, action) in outcomes {
        assert_eq!(outcome.action(), action);
    }
}

#[test]
fn recovery_action_names_are_stable_and_machine_readable() {
    let expected = [
        (RecoveryAction::Synchronize, "synchronize"),
        (RecoveryAction::Rebuild, "rebuild"),
        (
            RecoveryAction::InvalidateAndRebuild,
            "invalidate_and_rebuild",
        ),
        (RecoveryAction::RestoreAvailability, "restore_availability"),
        (RecoveryAction::Throttle, "throttle"),
        (
            RecoveryAction::DelegateToCoreRetry,
            "delegate_to_core_retry",
        ),
        (RecoveryAction::PreserveSafeState, "preserve_safe_state"),
        (RecoveryAction::SurfaceQueryFailure, "surface_query_failure"),
    ];

    for (action, expected_name) in expected {
        assert_eq!(action.as_str(), expected_name);
        assert_eq!(action.to_string(), expected_name);
    }
}

#[test]
fn handler_error_is_normalized_to_the_action_selected_by_the_executor() {
    struct MismatchedErrorHandler;

    impl RecoveryHandler for MismatchedErrorHandler {
        fn synchronize(&mut self, _: &RecoveryRequest) -> Result<(), RecoveryExecutionError> {
            Err(RecoveryExecutionError::new(
                RecoveryAction::Rebuild,
                "synchronize failed",
            ))
        }

        fn rebuild(&mut self, _: &RecoveryRequest) -> Result<(), RecoveryExecutionError> {
            Ok(())
        }

        fn invalidate_and_rebuild(
            &mut self,
            _: &RecoveryRequest,
        ) -> Result<(), RecoveryExecutionError> {
            Ok(())
        }

        fn restore_availability(
            &mut self,
            _: &RecoveryRequest,
        ) -> Result<(), RecoveryExecutionError> {
            Ok(())
        }

        fn throttle(&mut self, _: &RecoveryRequest) -> Result<(), RecoveryExecutionError> {
            Ok(())
        }

        fn delegate_to_core_retry(
            &mut self,
            _: &RecoveryRequest,
        ) -> Result<(), RecoveryExecutionError> {
            Ok(())
        }

        fn preserve_safe_state(
            &mut self,
            _: &RecoveryRequest,
        ) -> Result<(), RecoveryExecutionError> {
            Ok(())
        }

        fn surface_query_failure(
            &mut self,
            _: &RecoveryRequest,
        ) -> Result<(), RecoveryExecutionError> {
            Ok(())
        }
    }

    let request = RecoveryRequest::new(classify(FailureClass::StaleIndex));
    let mut handler = MismatchedErrorHandler;

    let error = RecoveryExecutor::new()
        .execute(&request, &mut handler)
        .expect_err("the handler failure should be propagated");

    assert_eq!(error.action(), RecoveryAction::Synchronize);
    assert_eq!(error.message(), "synchronize failed");
}

#[test]
fn recovery_request_without_active_version_keeps_lineage_absent() {
    let request = RecoveryRequest::new(classify(FailureClass::UnavailableIndex));

    assert_eq!(request.failure().class(), FailureClass::UnavailableIndex);
    assert_eq!(request.active_version(), None);
    assert_eq!(request.action(), RecoveryAction::RestoreAvailability);
}
