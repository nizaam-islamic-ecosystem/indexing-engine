//! Immediate local recovery orchestration for Phase 5.
//!
//! Recovery is the operational layer between failure classification and the
//! existing Phase 3 construction/publication machinery:
//!
//! ```text
//! failure
//!    ↓
//! classify
//!    ↓
//! RecoveryExecutor
//!    ↓
//! recovery action
//!    ↓
//! Phase 3 build/update/rebuild path when required
//!    ↓
//! validate
//!    ↓
//! publish
//! ```
//!
//! This module deliberately does not own:
//! - Core retry policy, retry budgets, or retry scheduling;
//! - Core runtime or task scheduling;
//! - the active-version registry;
//! - physical provider mechanics;
//! - source-engine semantics;
//! - validation or publication implementation.
//!
//! The executor therefore selects and dispatches deterministic local recovery
//! actions. The supplied [`RecoveryHandler`] performs the subsystem-specific
//! operation. This keeps recovery immediate and composable without creating a
//! second scheduler or retry system.

use super::failure::{ClassifiedFailure, FailureClass};
use crate::index::IndexVersionId;
use core::fmt;
use std::error::Error;

/// Recovery action selected for an Indexing failure.
///
/// The action describes the next Indexing-owned recovery operation. It does
/// not itself perform the operation and it does not encode Core retry policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecoveryAction {
    /// Synchronize or incrementally update a stale index when that path is
    /// known to be safe.
    Synchronize,

    /// Isolate the affected state and construct a replacement through the
    /// Phase 3 rebuild path.
    Rebuild,

    /// Invalidate the unusable candidate and rebuild or restore it through the
    /// normal validated publication path.
    InvalidateAndRebuild,

    /// Restore availability without replacing the logical index when the
    /// surrounding lifecycle/provider boundary can safely do so.
    RestoreAvailability,

    /// Apply bounded capacity behavior. This means defer/throttle/reject at
    /// the local admission boundary, not queue indefinitely.
    Throttle,

    /// Surface the provider failure through Core's retry/reliability boundary.
    /// This action does not perform a retry itself.
    DelegateToCoreRetry,

    /// Preserve the last known-good state and surface the source-data failure.
    PreserveSafeState,

    /// Surface a query failure without changing index state.
    SurfaceQueryFailure,
}

impl RecoveryAction {
    /// Returns the stable logical name of the recovery action.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Synchronize => "synchronize",
            Self::Rebuild => "rebuild",
            Self::InvalidateAndRebuild => "invalidate_and_rebuild",
            Self::RestoreAvailability => "restore_availability",
            Self::Throttle => "throttle",
            Self::DelegateToCoreRetry => "delegate_to_core_retry",
            Self::PreserveSafeState => "preserve_safe_state",
            Self::SurfaceQueryFailure => "surface_query_failure",
        }
    }
}

impl fmt::Display for RecoveryAction {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Deterministically selects the initial recovery action for an Indexing
/// failure class.
///
/// The mapping follows the Phase 5 recovery scope. It is intentionally pure so
/// callers can inspect the selected action before invoking execution.
#[must_use]
pub const fn action_for(class: FailureClass) -> RecoveryAction {
    match class {
        FailureClass::StaleIndex => RecoveryAction::Synchronize,
        FailureClass::CorruptIndex => RecoveryAction::Rebuild,
        FailureClass::InvalidIndex => RecoveryAction::InvalidateAndRebuild,
        FailureClass::UnavailableIndex => RecoveryAction::RestoreAvailability,
        FailureClass::ResourceExhaustion => RecoveryAction::Throttle,
        FailureClass::ProviderFailure => RecoveryAction::DelegateToCoreRetry,
        FailureClass::SourceDataFailure => RecoveryAction::PreserveSafeState,
        FailureClass::QueryFailure => RecoveryAction::SurfaceQueryFailure,
    }
}

/// Input to one recovery execution.
///
/// `active_version` is observational lineage only. The executor never exposes
/// a mutation operation for it. A recovery implementation must construct a
/// candidate separately and return through validation/publication before that
/// candidate can replace the active version.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecoveryRequest {
    failure: ClassifiedFailure,
    active_version: Option<IndexVersionId>,
}

impl RecoveryRequest {
    /// Creates a recovery request for the supplied classified failure.
    #[must_use]
    pub const fn new(failure: ClassifiedFailure) -> Self {
        Self {
            failure,
            active_version: None,
        }
    }

    /// Associates the last known-good active version observed by the caller.
    ///
    /// This is carried as protection/lineage context only. Recovery never
    /// destroys or mutates this version.
    #[must_use]
    pub fn with_active_version(mut self, version: IndexVersionId) -> Self {
        self.active_version = Some(version);
        self
    }

    /// Returns the classified failure.
    #[must_use]
    pub const fn failure(&self) -> ClassifiedFailure {
        self.failure
    }

    /// Returns the observed active version, when supplied.
    #[must_use]
    pub fn active_version(&self) -> Option<&IndexVersionId> {
        self.active_version.as_ref()
    }

    /// Returns the deterministic initial action for this request.
    #[must_use]
    pub const fn action(&self) -> RecoveryAction {
        action_for(self.failure.class())
    }
}

/// Result of a successfully dispatched recovery operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecoveryOutcome {
    /// A synchronization/update operation was dispatched.
    Synchronized,

    /// An isolated rebuild operation was dispatched.
    Rebuilt,

    /// An invalid candidate was rejected and a rebuild was dispatched.
    InvalidatedAndRebuildRequested,

    /// Availability restoration was dispatched.
    AvailabilityRestored,

    /// Capacity handling was applied.
    Throttled,

    /// Provider failure was handed to Core's retry/reliability boundary.
    DelegatedToCoreRetry,

    /// The known-good state was deliberately preserved and the source failure
    /// was surfaced.
    SafeStatePreserved,

    /// The query failure was surfaced without index-state mutation.
    QueryFailureSurfaced,
}

impl RecoveryOutcome {
    /// Returns the action represented by this outcome.
    #[must_use]
    pub const fn action(self) -> RecoveryAction {
        match self {
            Self::Synchronized => RecoveryAction::Synchronize,
            Self::Rebuilt => RecoveryAction::Rebuild,
            Self::InvalidatedAndRebuildRequested => RecoveryAction::InvalidateAndRebuild,
            Self::AvailabilityRestored => RecoveryAction::RestoreAvailability,
            Self::Throttled => RecoveryAction::Throttle,
            Self::DelegatedToCoreRetry => RecoveryAction::DelegateToCoreRetry,
            Self::SafeStatePreserved => RecoveryAction::PreserveSafeState,
            Self::QueryFailureSurfaced => RecoveryAction::SurfaceQueryFailure,
        }
    }
}

/// Error returned by a recovery handler.
#[derive(Debug, Eq, PartialEq)]
pub struct RecoveryExecutionError {
    action: RecoveryAction,
    message: String,
}

impl RecoveryExecutionError {
    /// Creates a handler failure for the supplied recovery action.
    pub fn new(action: RecoveryAction, message: impl Into<String>) -> Self {
        Self {
            action,
            message: message.into(),
        }
    }

    /// Returns the action that failed.
    #[must_use]
    pub const fn action(&self) -> RecoveryAction {
        self.action
    }

    /// Returns the handler-provided failure message.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for RecoveryExecutionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "recovery action {} failed: {}",
            self.action, self.message
        )
    }
}

impl Error for RecoveryExecutionError {}

/// Immediate local recovery operation boundary.
///
/// Implementations perform the actual local operation using existing
/// Indexing/Phase 3 facilities. In particular, a rebuild handler should use
/// [`crate::build::rebuild::IndexRebuilder`] and return its unpublished result
/// to the normal validation/publication flow rather than modifying active
/// state directly.
pub trait RecoveryHandler {
    /// Synchronizes or incrementally updates a stale index.
    fn synchronize(&mut self, request: &RecoveryRequest) -> Result<(), RecoveryExecutionError>;

    /// Starts/executes an isolated rebuild for the affected index.
    fn rebuild(&mut self, request: &RecoveryRequest) -> Result<(), RecoveryExecutionError>;

    /// Invalidates the unusable candidate and starts its replacement rebuild.
    fn invalidate_and_rebuild(
        &mut self,
        request: &RecoveryRequest,
    ) -> Result<(), RecoveryExecutionError>;

    /// Restores availability without assuming that a rebuild is necessary.
    fn restore_availability(
        &mut self,
        request: &RecoveryRequest,
    ) -> Result<(), RecoveryExecutionError>;

    /// Applies bounded local capacity handling.
    fn throttle(&mut self, request: &RecoveryRequest) -> Result<(), RecoveryExecutionError>;

    /// Delegates provider retry handling to Core.
    fn delegate_to_core_retry(
        &mut self,
        request: &RecoveryRequest,
    ) -> Result<(), RecoveryExecutionError>;

    /// Preserves the last known-good state and surfaces the source failure.
    fn preserve_safe_state(
        &mut self,
        request: &RecoveryRequest,
    ) -> Result<(), RecoveryExecutionError>;

    /// Surfaces a query failure without mutating index state.
    fn surface_query_failure(
        &mut self,
        request: &RecoveryRequest,
    ) -> Result<(), RecoveryExecutionError>;
}

/// Stateless dispatcher for immediate Indexing-local recovery.
///
/// The executor owns dispatch semantics, not the underlying runtime. It calls
/// one handler operation immediately and returns only after that operation has
/// completed or failed.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RecoveryExecutor;

impl RecoveryExecutor {
    /// Creates the stateless recovery executor.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Returns the deterministic action for a classified failure.
    #[must_use]
    pub const fn action_for(&self, failure: FailureClass) -> RecoveryAction {
        action_for(failure)
    }

    /// Executes the selected local recovery operation immediately.
    ///
    /// No retry loop is performed here. Provider retry is explicitly delegated
    /// to Core, and every replacement-index path remains unpublished until the
    /// normal validate → publish boundary is completed by the caller.
    pub fn execute<H: RecoveryHandler>(
        &self,
        request: &RecoveryRequest,
        handler: &mut H,
    ) -> Result<RecoveryOutcome, RecoveryExecutionError> {
        match request.action() {
            RecoveryAction::Synchronize => {
                handler.synchronize(request)?;
                Ok(RecoveryOutcome::Synchronized)
            }
            RecoveryAction::Rebuild => {
                handler.rebuild(request)?;
                Ok(RecoveryOutcome::Rebuilt)
            }
            RecoveryAction::InvalidateAndRebuild => {
                handler.invalidate_and_rebuild(request)?;
                Ok(RecoveryOutcome::InvalidatedAndRebuildRequested)
            }
            RecoveryAction::RestoreAvailability => {
                handler.restore_availability(request)?;
                Ok(RecoveryOutcome::AvailabilityRestored)
            }
            RecoveryAction::Throttle => {
                handler.throttle(request)?;
                Ok(RecoveryOutcome::Throttled)
            }
            RecoveryAction::DelegateToCoreRetry => {
                handler.delegate_to_core_retry(request)?;
                Ok(RecoveryOutcome::DelegatedToCoreRetry)
            }
            RecoveryAction::PreserveSafeState => {
                handler.preserve_safe_state(request)?;
                Ok(RecoveryOutcome::SafeStatePreserved)
            }
            RecoveryAction::SurfaceQueryFailure => {
                handler.surface_query_failure(request)?;
                Ok(RecoveryOutcome::QueryFailureSurfaced)
            }
        }
    }
}

/// Selects and immediately executes recovery for a classified failure.
pub fn execute_recovery<H: RecoveryHandler>(
    failure: ClassifiedFailure,
    active_version: Option<IndexVersionId>,
    handler: &mut H,
) -> Result<RecoveryOutcome, RecoveryExecutionError> {
    let request = match active_version {
        Some(version) => RecoveryRequest::new(failure).with_active_version(version),
        None => RecoveryRequest::new(failure),
    };

    RecoveryExecutor::new().execute(&request, handler)
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn phase_five_failure_mapping_is_deterministic() {
        assert_eq!(
            action_for(FailureClass::StaleIndex),
            RecoveryAction::Synchronize
        );
        assert_eq!(
            action_for(FailureClass::CorruptIndex),
            RecoveryAction::Rebuild
        );
        assert_eq!(
            action_for(FailureClass::InvalidIndex),
            RecoveryAction::InvalidateAndRebuild
        );
        assert_eq!(
            action_for(FailureClass::UnavailableIndex),
            RecoveryAction::RestoreAvailability
        );
        assert_eq!(
            action_for(FailureClass::ResourceExhaustion),
            RecoveryAction::Throttle
        );
        assert_eq!(
            action_for(FailureClass::ProviderFailure),
            RecoveryAction::DelegateToCoreRetry
        );
        assert_eq!(
            action_for(FailureClass::SourceDataFailure),
            RecoveryAction::PreserveSafeState
        );
        assert_eq!(
            action_for(FailureClass::QueryFailure),
            RecoveryAction::SurfaceQueryFailure
        );
    }

    #[test]
    fn executor_dispatches_immediately_without_retry_loop() {
        let executor = RecoveryExecutor::new();
        let mut handler = RecordingHandler::default();

        for class in FailureClass::ALL {
            let request = RecoveryRequest::new(ClassifiedFailure::new(class));
            let outcome = executor
                .execute(&request, &mut handler)
                .expect("recovery succeeds");
            assert_eq!(outcome.action(), action_for(class));
        }

        assert_eq!(handler.actions.len(), FailureClass::ALL.len());
    }

    #[test]
    fn active_version_is_observational_and_preserved() {
        let active = IndexVersionId::new("v7").expect("valid version");
        let request = RecoveryRequest::new(ClassifiedFailure::new(FailureClass::CorruptIndex))
            .with_active_version(active.clone());

        assert_eq!(request.active_version(), Some(&active));
        assert_eq!(request.action(), RecoveryAction::Rebuild);
    }

    #[test]
    fn source_failure_preserves_safe_state() {
        let mut handler = RecordingHandler::default();
        let outcome = execute_recovery(
            ClassifiedFailure::new(FailureClass::SourceDataFailure),
            None,
            &mut handler,
        )
        .expect("recovery dispatch succeeds");

        assert_eq!(outcome, RecoveryOutcome::SafeStatePreserved);
        assert_eq!(handler.actions, vec![RecoveryAction::PreserveSafeState]);
    }

    #[test]
    fn provider_failure_is_delegated_instead_of_retried_locally() {
        let mut handler = RecordingHandler::default();
        let outcome = execute_recovery(
            ClassifiedFailure::new(FailureClass::ProviderFailure),
            None,
            &mut handler,
        )
        .expect("delegation succeeds");

        assert_eq!(outcome, RecoveryOutcome::DelegatedToCoreRetry);
        assert_eq!(handler.actions, vec![RecoveryAction::DelegateToCoreRetry]);
    }

    #[test]
    fn handler_failure_is_propagated_without_fallback_or_second_attempt() {
        struct FailingHandler;

        impl RecoveryHandler for FailingHandler {
            fn synchronize(&mut self, _: &RecoveryRequest) -> Result<(), RecoveryExecutionError> {
                Err(RecoveryExecutionError::new(
                    RecoveryAction::Synchronize,
                    "synchronization unavailable",
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

        let mut handler = FailingHandler;
        let error = execute_recovery(
            ClassifiedFailure::new(FailureClass::StaleIndex),
            None,
            &mut handler,
        )
        .expect_err("handler failure must propagate");

        assert_eq!(error.action(), RecoveryAction::Synchronize);
        assert_eq!(error.message(), "synchronization unavailable");
    }
}
