//! Phase 5 recovery boundary for the Indexing Engine.
//!
//! This module is intentionally a thin composition facade over the two recovery
//! concerns:
//!
//! ```text
//! failure.rs
//!     -> classify Indexing failures
//!     -> expose Core retryability hint
//!             |
//!             v
//! execution.rs
//!     -> select deterministic recovery action
//!     -> execute one local recovery operation
//!             |
//!             v
//! Phase 3 validate -> publish
//! ```
//!
//! The recovery module does not create a second retry system, scheduler,
//! runtime, active-version registry, or publication mechanism. Core remains the
//! owner of retry policy and execution infrastructure, while Phase 3 remains
//! the owner of candidate construction, validation, and publication.

pub mod execution;
pub mod failure;

pub use failure::{ClassifiedFailure, FailureClass, FailureClassifier, classify, retryability};

pub use execution::{
    RecoveryAction, RecoveryExecutionError, RecoveryExecutor, RecoveryHandler, RecoveryOutcome,
    RecoveryRequest, action_for, execute_recovery,
};

#[cfg(test)]
mod tests {
    use super::*;
    use nizaam_core::status::Retryability;

    #[derive(Default)]
    struct RecordingHandler {
        actions: Vec<RecoveryAction>,
    }

    impl RecoveryHandler for RecordingHandler {
        fn synchronize(&mut self, _: &RecoveryRequest) -> Result<(), RecoveryExecutionError> {
            self.actions.push(RecoveryAction::Synchronize);
            Ok(())
        }

        fn rebuild(&mut self, _: &RecoveryRequest) -> Result<(), RecoveryExecutionError> {
            self.actions.push(RecoveryAction::Rebuild);
            Ok(())
        }

        fn invalidate_and_rebuild(
            &mut self,
            _: &RecoveryRequest,
        ) -> Result<(), RecoveryExecutionError> {
            self.actions.push(RecoveryAction::InvalidateAndRebuild);
            Ok(())
        }

        fn restore_availability(
            &mut self,
            _: &RecoveryRequest,
        ) -> Result<(), RecoveryExecutionError> {
            self.actions.push(RecoveryAction::RestoreAvailability);
            Ok(())
        }

        fn throttle(&mut self, _: &RecoveryRequest) -> Result<(), RecoveryExecutionError> {
            self.actions.push(RecoveryAction::Throttle);
            Ok(())
        }

        fn delegate_to_core_retry(
            &mut self,
            _: &RecoveryRequest,
        ) -> Result<(), RecoveryExecutionError> {
            self.actions.push(RecoveryAction::DelegateToCoreRetry);
            Ok(())
        }

        fn preserve_safe_state(
            &mut self,
            _: &RecoveryRequest,
        ) -> Result<(), RecoveryExecutionError> {
            self.actions.push(RecoveryAction::PreserveSafeState);
            Ok(())
        }

        fn surface_query_failure(
            &mut self,
            _: &RecoveryRequest,
        ) -> Result<(), RecoveryExecutionError> {
            self.actions.push(RecoveryAction::SurfaceQueryFailure);
            Ok(())
        }
    }

    #[test]
    fn public_boundary_exposes_all_required_failure_classes() {
        assert_eq!(FailureClass::ALL.len(), 8);

        let expected = [
            FailureClass::InvalidIndex,
            FailureClass::StaleIndex,
            FailureClass::UnavailableIndex,
            FailureClass::CorruptIndex,
            FailureClass::SourceDataFailure,
            FailureClass::ResourceExhaustion,
            FailureClass::ProviderFailure,
            FailureClass::QueryFailure,
        ];

        assert_eq!(FailureClass::ALL, expected);
    }

    #[test]
    fn public_boundary_preserves_failure_to_action_mapping() {
        let expected = [
            (
                FailureClass::InvalidIndex,
                RecoveryAction::InvalidateAndRebuild,
            ),
            (FailureClass::StaleIndex, RecoveryAction::Synchronize),
            (
                FailureClass::UnavailableIndex,
                RecoveryAction::RestoreAvailability,
            ),
            (FailureClass::CorruptIndex, RecoveryAction::Rebuild),
            (
                FailureClass::SourceDataFailure,
                RecoveryAction::PreserveSafeState,
            ),
            (FailureClass::ResourceExhaustion, RecoveryAction::Throttle),
            (
                FailureClass::ProviderFailure,
                RecoveryAction::DelegateToCoreRetry,
            ),
            (
                FailureClass::QueryFailure,
                RecoveryAction::SurfaceQueryFailure,
            ),
        ];

        for (failure, action) in expected {
            let classified = classify(failure);
            assert_eq!(action_for(classified.class()), action);
            assert_eq!(RecoveryRequest::new(classified).action(), action);
        }
    }

    #[test]
    fn public_boundary_preserves_core_retryability_adapter() {
        assert_eq!(
            retryability(FailureClass::InvalidIndex),
            Retryability::NonRetryable
        );
        assert_eq!(
            retryability(FailureClass::StaleIndex),
            Retryability::NonRetryable
        );
        assert_eq!(
            retryability(FailureClass::CorruptIndex),
            Retryability::NonRetryable
        );
        assert_eq!(
            retryability(FailureClass::SourceDataFailure),
            Retryability::NonRetryable
        );
        assert_eq!(
            retryability(FailureClass::ResourceExhaustion),
            Retryability::NonRetryable
        );

        assert_eq!(
            retryability(FailureClass::UnavailableIndex),
            Retryability::Retryable
        );
        assert_eq!(
            retryability(FailureClass::ProviderFailure),
            Retryability::Retryable
        );
        assert_eq!(
            retryability(FailureClass::QueryFailure),
            Retryability::Retryable
        );
    }

    #[test]
    fn public_boundary_executes_each_recovery_action_once() {
        let mut handler = RecordingHandler::default();
        let executor = RecoveryExecutor::new();

        for failure in FailureClass::ALL {
            let classified = ClassifiedFailure::new(failure);
            let request = RecoveryRequest::new(classified);
            let outcome = executor
                .execute(&request, &mut handler)
                .expect("recovery action should execute");

            assert_eq!(outcome.action(), action_for(failure));
        }

        assert_eq!(handler.actions.len(), FailureClass::ALL.len());
    }

    #[test]
    fn public_boundary_free_execution_matches_executor() {
        let mut handler = RecordingHandler::default();

        let outcome = execute_recovery(
            ClassifiedFailure::new(FailureClass::SourceDataFailure),
            None,
            &mut handler,
        )
        .expect("source failure recovery should dispatch");

        assert_eq!(outcome, RecoveryOutcome::SafeStatePreserved);
        assert_eq!(handler.actions, vec![RecoveryAction::PreserveSafeState]);
    }

    #[test]
    fn public_boundary_keeps_failure_and_recovery_layers_distinct() {
        let classified = classify(FailureClass::ProviderFailure);
        let action = action_for(classified.class());

        assert_eq!(classified.class(), FailureClass::ProviderFailure);
        assert_eq!(classified.retryability(), Retryability::Retryable);
        assert_eq!(action, RecoveryAction::DelegateToCoreRetry);
        assert_ne!(action, RecoveryAction::Rebuild);
    }
}
