//! Core runtime integration for the Nizaam Indexing Engine.
//!
//! Phase 0 intentionally does not implement a second runtime system. This
//! module is a thin Indexing-facing boundary over `nizaam_core::runtime::EngineRuntime`.
//! Core remains authoritative for lifecycle validity, request admission,
//! cancellation, deadlines, execution context, capability dispatch, and
//! shutdown coordination.
//!
//! The runtime lifecycle established here is:
//!
//! ```text
//! Created -> Starting -> Configuring -> Dependencies -> Capabilities -> Registering -> Ready -> Serving -> Draining -> Stopped
//! ```
//!
//! Engine registration and capability registration remain coordinated by the
//! surrounding engine composition layer. This module only exposes the runtime
//! boundary needed to make that composition explicit.

use nizaam_core::capability::{CapabilityDispatchResult, CapabilityInvocation};
use nizaam_core::error::InvalidTransition;
use nizaam_core::identity::{EngineId, EngineInstanceId};
use nizaam_core::operation::OperationContext;
use nizaam_core::runtime::{EngineContext, EngineRuntime, LifecycleState, RequestAdmissionError};

use super::capability::CapabilitySet;

/// Result returned by the Indexing runtime after lifecycle admission.
///
/// A request that is not admitted fails with the Core
/// [`RequestAdmissionError`]. Once admitted, capability lookup, cancellation,
/// deadline checks, and handler execution remain represented by Core's
/// [`CapabilityDispatchResult`].
pub type RuntimeDispatchResult = Result<CapabilityDispatchResult, RequestAdmissionError>;

/// Indexing-facing runtime adapter over the Core [`EngineRuntime`].
///
/// This type deliberately contains only the Core runtime. It does not define
/// its own lifecycle state machine, cancellation token, deadline mechanism, or
/// shutdown coordinator.
#[derive(Debug)]
pub struct IndexingRuntime {
    core: EngineRuntime,
}

impl IndexingRuntime {
    /// Creates a new Indexing runtime in the Core `Created` state.
    ///
    /// The logical `EngineId` and concrete `EngineInstanceId` are supplied to
    /// Core at construction and become the authoritative runtime identities.
    #[must_use]
    pub fn new(engine_id: EngineId, engine_instance_id: EngineInstanceId) -> Self {
        Self {
            core: EngineRuntime::new(engine_id, engine_instance_id),
        }
    }

    /// Returns the logical engine identity owned by the Core runtime.
    #[must_use]
    pub fn engine_id(&self) -> &EngineId {
        self.core.engine_id()
    }

    /// Returns the concrete engine-instance identity owned by the Core runtime.
    #[must_use]
    pub fn engine_instance_id(&self) -> &EngineInstanceId {
        self.core.instance_id()
    }

    /// Returns the current Core lifecycle state.
    #[must_use]
    pub fn state(&self) -> LifecycleState {
        self.core.state()
    }

    /// Applies one lifecycle transition through the Core runtime.
    ///
    /// Core remains responsible for validating the legal lifecycle graph and
    /// for preventing invalid or terminal transitions.
    pub fn transition(&self, next: LifecycleState) -> Result<(), InvalidTransition> {
        self.core.transition(next)
    }

    /// Performs the non-registration portion of Phase 0 startup.
    ///
    /// This advances the runtime through the Core-managed initialization states
    /// up to `Capabilities`:
    ///
    /// ```text
    /// Created
    ///   -> Starting
    ///   -> Configuring
    ///   -> Dependencies
    ///   -> Capabilities
    /// ```
    ///
    /// Engine registration is intentionally not hidden inside this method.
    /// The engine composition layer must keep registration distinct and must
    /// establish `Registering` before the runtime can become `Ready`.
    pub fn start(&self) -> Result<(), InvalidTransition> {
        for next in [
            LifecycleState::Starting,
            LifecycleState::Configuring,
            LifecycleState::Dependencies,
            LifecycleState::Capabilities,
        ] {
            self.transition(next)?;
        }

        Ok(())
    }

    /// Enters the Core `Registering` lifecycle state.
    ///
    /// Actual engine registration is owned by `registration.rs`. Keeping the
    /// transition separate ensures registration is not confused with startup
    /// mechanics or capability registration.
    pub(crate) fn begin_registration(&self) -> Result<(), InvalidTransition> {
        self.transition(LifecycleState::Registering)
    }

    /// Marks the runtime `Ready` after required Phase 0 registration work has
    /// completed successfully.
    ///
    /// This method is crate-visible so the engine composition layer remains the
    /// authority that sequences registration before readiness.
    pub(crate) fn mark_ready(&self) -> Result<(), InvalidTransition> {
        self.transition(LifecycleState::Ready)
    }

    /// Explicitly transitions a ready runtime into `Serving`.
    ///
    /// `Ready` and `Serving` remain separate. Normal request admission is not
    /// enabled merely by reaching `Ready`.
    pub(crate) fn serve(&self) -> Result<(), InvalidTransition> {
        self.transition(LifecycleState::Serving)
    }

    /// Explicitly begins draining through the Core lifecycle.
    ///
    /// After this transition, new normal requests are rejected by
    /// [`Self::admit_request`]. Work that already passed admission continues to
    /// use its existing Core execution context.
    pub fn drain(&self) -> Result<(), InvalidTransition> {
        self.transition(LifecycleState::Draining)
    }

    /// Applies Core request admission for one normal request.
    ///
    /// Core is authoritative here: only `Serving` admits new normal work.
    /// Admission is deliberately separate from capability dispatch so a
    /// rejected request never reaches capability resolution or a handler.
    pub fn admit_request(&self) -> Result<(), RequestAdmissionError> {
        self.core.admit_request()
    }

    /// Creates a Core [`EngineContext`] from an existing [`OperationContext`].
    ///
    /// No Indexing-specific operation identity, cancellation token, deadline,
    /// security container, or provenance container is created. Callers may
    /// continue enriching the returned Core context with the Core-provided
    /// `with_deadline`, `with_security`, `with_provenance`, or child-context
    /// APIs before capability execution.
    #[must_use]
    pub fn context(&self, operation: OperationContext) -> EngineContext {
        EngineContext::new(operation)
    }

    /// Admits and then dispatches one capability invocation.
    ///
    /// The ordering is intentionally:
    ///
    /// ```text
    /// runtime admission
    ///       -> Core capability dispatch
    ///       -> cancellation/deadline checks
    ///       -> capability resolution
    ///       -> handler invocation
    /// ```
    ///
    /// The runtime performs only the local admission boundary. Core remains
    /// responsible for the capability execution rules represented by
    /// [`CapabilityDispatchResult`].
    pub fn dispatch(
        &self,
        capabilities: &CapabilitySet,
        context: &EngineContext,
        invocation: &CapabilityInvocation,
    ) -> RuntimeDispatchResult {
        self.admit_request()?;
        Ok(capabilities.dispatch(context, invocation))
    }

    /// Gracefully shuts down through the Core runtime.
    ///
    /// Core owns the shutdown coordination, including transition into
    /// `Draining`, runtime-owned cleanup, and the terminal `Stopped` state.
    /// The boolean result is passed through unchanged because the Core runtime
    /// distinguishes normal external completion from shutdown initiated by a
    /// runtime-owned background task.
    pub fn shutdown(&self) -> Result<bool, InvalidTransition> {
        self.core.shutdown()
    }

    /// Returns the Core runtime shutdown cancellation token.
    #[must_use]
    pub fn shutdown_token(&self) -> &nizaam_core::runtime::CancellationToken {
        self.core.shutdown_token()
    }

    /// Returns the Core runtime owned background-task manager.
    ///
    /// This is exposed only as a direct Core integration surface. Phase 0 does
    /// not define an Indexing-specific task manager.
    #[must_use]
    pub fn background_tasks(&self) -> &nizaam_core::runtime::BackgroundTasks {
        self.core.background_tasks()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use nizaam_core::capability::{
        CapabilityDefinition, CapabilityError, CapabilityOutcome, arc_handler,
    };
    use nizaam_core::contracts::Version;
    use nizaam_core::identity::{CapabilityId, ContractId, CorrelationId, OperationId};
    use nizaam_core::operation::{Deadline, Operation, OperationContext};

    fn engine_id() -> EngineId {
        EngineId::new("nizaam.indexing.test").expect("test engine id must be valid")
    }

    fn instance_id() -> EngineInstanceId {
        EngineInstanceId::new("nizaam.indexing.test.instance")
            .expect("test engine instance id must be valid")
    }

    fn runtime() -> IndexingRuntime {
        IndexingRuntime::new(engine_id(), instance_id())
    }

    fn operation_context(name: &str) -> OperationContext {
        OperationContext::new(Operation::new(
            OperationId::new(format!("{name}.operation")).expect("test operation id must be valid"),
            CorrelationId::new(format!("{name}.correlation"))
                .expect("test correlation id must be valid"),
        ))
    }

    fn serving_runtime() -> IndexingRuntime {
        let runtime = runtime();
        runtime.start().unwrap();
        runtime.begin_registration().unwrap();
        runtime.mark_ready().unwrap();
        runtime.serve().unwrap();
        runtime
    }

    fn phase0_capability(
        runtime: &IndexingRuntime,
        invocation_count: Arc<AtomicUsize>,
    ) -> (CapabilitySet, CapabilityInvocation) {
        let capabilities = CapabilitySet::new();
        let capability_id =
            CapabilityId::new("nizaam.indexing.runtime.phase0").expect("valid test capability id");
        let contract_id = ContractId::new("nizaam.indexing.runtime.phase0.contract")
            .expect("valid test contract id");

        let definition = CapabilityDefinition::new(
            capability_id.clone(),
            runtime.engine_id().clone(),
            "Phase 0 runtime test capability",
        )
        .expect("test capability definition must be valid")
        .with_version(Version::new(1, 0, 0));

        let handler_count = Arc::clone(&invocation_count);
        let handler = arc_handler(move |_context, invocation| {
            handler_count.fetch_add(1, Ordering::SeqCst);
            Ok(CapabilityOutcome::new(invocation.payload_bytes().to_vec()))
        });

        capabilities
            .register(definition, handler)
            .expect("test capability registration must succeed");

        let invocation = CapabilityInvocation::new(capability_id, contract_id, b"phase0".to_vec());

        (capabilities, invocation)
    }

    #[test]
    fn construction_delegates_identity_to_the_core_runtime() {
        let runtime = runtime();

        assert_eq!(runtime.engine_id(), &engine_id());
        assert_eq!(runtime.engine_instance_id(), &instance_id());
        assert_eq!(runtime.state(), LifecycleState::Created);
    }

    #[test]
    fn start_preserves_the_core_startup_sequence() {
        let runtime = runtime();

        runtime.start().unwrap();

        assert_eq!(runtime.state(), LifecycleState::Capabilities);
    }

    #[test]
    fn readiness_and_serving_remain_separate() {
        let runtime = runtime();

        runtime.start().unwrap();
        runtime.begin_registration().unwrap();
        runtime.mark_ready().unwrap();

        assert_eq!(runtime.state(), LifecycleState::Ready);
        assert_eq!(
            runtime.admit_request(),
            Err(RequestAdmissionError::NotServing(LifecycleState::Ready))
        );

        runtime.serve().unwrap();
        assert_eq!(runtime.state(), LifecycleState::Serving);
        assert!(runtime.admit_request().is_ok());
    }

    #[test]
    fn non_serving_dispatch_is_rejected_before_capability_execution() {
        let runtime = runtime();
        let invocation_count = Arc::new(AtomicUsize::new(0));
        let (capabilities, invocation) = phase0_capability(&runtime, Arc::clone(&invocation_count));
        let context = runtime.context(operation_context("not-serving"));

        let result = runtime.dispatch(&capabilities, &context, &invocation);

        assert!(matches!(
            result,
            Err(RequestAdmissionError::NotServing(LifecycleState::Created))
        ));
        assert_eq!(invocation_count.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn serving_dispatch_reaches_core_capability_handler() {
        let runtime = serving_runtime();
        let invocation_count = Arc::new(AtomicUsize::new(0));
        let (capabilities, invocation) = phase0_capability(&runtime, Arc::clone(&invocation_count));
        let context = runtime.context(operation_context("serving"));

        let result = runtime
            .dispatch(&capabilities, &context, &invocation)
            .expect("serving runtime should admit the request");

        match result {
            CapabilityDispatchResult::Outcome(outcome) => {
                assert_eq!(outcome.as_bytes(), b"phase0");
            }
            CapabilityDispatchResult::Error(error) => {
                panic!("unexpected Core capability error: {error:?}");
            }
        }

        assert_eq!(invocation_count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn draining_rejects_new_dispatch_without_invoking_the_handler() {
        let runtime = serving_runtime();
        let invocation_count = Arc::new(AtomicUsize::new(0));
        let (capabilities, invocation) = phase0_capability(&runtime, Arc::clone(&invocation_count));
        let context = runtime.context(operation_context("draining"));

        runtime.drain().unwrap();

        let result = runtime.dispatch(&capabilities, &context, &invocation);

        assert!(matches!(
            result,
            Err(RequestAdmissionError::NotServing(LifecycleState::Draining))
        ));
        assert_eq!(invocation_count.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn context_preserves_the_supplied_operation_context() {
        let runtime = runtime();
        let operation = operation_context("context");
        let context = runtime.context(operation.clone());

        assert_eq!(context.operation(), &operation);
        assert!(context.deadline().is_none());
    }

    #[test]
    fn cancelled_context_is_rejected_by_core_dispatch_before_handler_execution() {
        let runtime = serving_runtime();
        let invocation_count = Arc::new(AtomicUsize::new(0));
        let (capabilities, invocation) = phase0_capability(&runtime, Arc::clone(&invocation_count));
        let context = runtime.context(operation_context("cancelled"));
        context.cancellation().cancel();

        let result = runtime
            .dispatch(&capabilities, &context, &invocation)
            .expect("serving runtime admission should succeed");

        assert!(matches!(
            result,
            CapabilityDispatchResult::Error(CapabilityError::Cancelled)
        ));
        assert_eq!(invocation_count.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn expired_context_is_rejected_by_core_dispatch_before_handler_execution() {
        let runtime = serving_runtime();
        let invocation_count = Arc::new(AtomicUsize::new(0));
        let (capabilities, invocation) = phase0_capability(&runtime, Arc::clone(&invocation_count));
        let context = runtime
            .context(operation_context("expired"))
            .with_deadline(Deadline::from_now(std::time::Duration::ZERO).unwrap());

        let result = runtime
            .dispatch(&capabilities, &context, &invocation)
            .expect("serving runtime admission should succeed");

        assert!(matches!(
            result,
            CapabilityDispatchResult::Error(CapabilityError::DeadlineExpired)
        ));
        assert_eq!(invocation_count.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn shutdown_delegates_to_core_and_reaches_stopped() {
        let runtime = serving_runtime();

        let completed = runtime.shutdown().expect("Core shutdown should succeed");

        assert!(completed);
        assert_eq!(runtime.state(), LifecycleState::Stopped);
        assert!(runtime.shutdown_token().is_cancelled());
    }

    #[test]
    fn stopped_runtime_remains_terminal_and_shutdown_is_idempotent() {
        let runtime = serving_runtime();

        runtime.shutdown().unwrap();
        runtime.shutdown().unwrap();

        assert_eq!(runtime.state(), LifecycleState::Stopped);
        assert!(runtime.transition(LifecycleState::Serving).is_err());
    }

    #[test]
    fn runtime_does_not_define_a_second_capability_error_boundary() {
        let runtime = serving_runtime();
        let invocation_count = Arc::new(AtomicUsize::new(0));
        let capabilities = CapabilitySet::new();
        let capability_id =
            CapabilityId::new("nizaam.indexing.runtime.failure").expect("valid capability id");
        let definition = CapabilityDefinition::new(
            capability_id.clone(),
            runtime.engine_id().clone(),
            "Phase 0 capability failure test",
        )
        .expect("valid definition");

        let count = Arc::clone(&invocation_count);
        capabilities
            .register(
                definition,
                arc_handler(move |_context, _invocation| {
                    count.fetch_add(1, Ordering::SeqCst);
                    Err(CapabilityError::HandlerFailed(
                        "phase 0 test failure".to_owned(),
                    ))
                }),
            )
            .unwrap();

        let invocation = CapabilityInvocation::new(
            capability_id,
            ContractId::new("nizaam.indexing.runtime.failure.contract").unwrap(),
            b"failure".to_vec(),
        );
        let context = runtime.context(operation_context("capability-error"));

        let dispatch_result = runtime
            .dispatch(&capabilities, &context, &invocation)
            .unwrap();

        assert!(matches!(
            dispatch_result,
            CapabilityDispatchResult::Error(CapabilityError::HandlerFailed(_))
        ));
        assert_eq!(invocation_count.load(Ordering::SeqCst), 1);
    }
}
