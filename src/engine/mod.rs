//! Indexing Engine module facade and Level 2 module-integration test surface.
//!
//! Phase 0 keeps the Indexing Engine deliberately thin. The facade composes
//! the three engine-boundary modules without introducing a second runtime,
//! registry, capability dispatcher, or Control Plane.
//!
//! ```text
//! IndexingEngine
//!      |
//!      +-- registration.rs  -> Core EngineRegistration / EngineRegistry
//!      +-- runtime.rs       -> Core EngineRuntime
//!      +-- capability.rs    -> Core CapabilityRegistry / dispatch
//! ```
//!
//! Level 1 unit tests live in each implementation file. The tests in this
//! module are Level 2 tests and therefore focus on interactions among
//! registration, runtime, and capability integration.

pub mod capability;
pub mod registration;
pub mod runtime;

pub use capability::CapabilitySet;
pub use registration::{IndexingRegistration, RegistrationResult};
pub use runtime::{IndexingRuntime, RuntimeDispatchResult};

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use nizaam_core::capability::{
    CapabilityDefinition, CapabilityDispatchResult, CapabilityError, CapabilityHandler,
    CapabilityInvocation, CapabilityOutcome, RegistryError as CapabilityRegistryError,
};
use nizaam_core::contracts::{
    EncodedPayload, Interaction, MessageEnvelope, Participants, UniversalRequest, UniversalResponse,
};
use nizaam_core::control_plane::registry::{
    EngineRegistry, RegistryError as ControlPlaneRegistryError,
};
use nizaam_core::error::InvalidTransition;
use nizaam_core::identity::{EngineId, EngineInstanceId, MessageId};
use nizaam_core::runtime::{LifecycleState, RequestAdmissionError};
use nizaam_core::status::Status;

/// Coherent Phase 0 facade for one Indexing Engine instance.
///
/// The facade owns the Indexing-side composition of:
///
/// - logical engine and concrete instance identity;
/// - declarative engine registration;
/// - the Core-backed runtime adapter;
/// - the Core-backed capability integration.
///
/// It does not implement indexing algorithms, storage, planning semantics, or
/// a replacement for any Core infrastructure system.
pub struct IndexingEngine {
    registration: IndexingRegistration,
    runtime: IndexingRuntime,
    capabilities: CapabilitySet,
    engine_registered: AtomicBool,
    phase0_capability_registered: AtomicBool,
}

/// Thin facade-level composition error for setup operations.
///
/// Core lifecycle and registry failures are preserved directly. The ownership
/// mismatch is a local composition invariant of the Indexing Engine boundary,
/// where a public capability registration request must belong to this engine.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EngineSetupError {
    Lifecycle(InvalidTransition),
    Registry(ControlPlaneRegistryError),
    CapabilityRegistry(CapabilityRegistryError),
    CapabilityOwnerMismatch {
        capability_id: nizaam_core::identity::CapabilityId,
        expected_engine: EngineId,
        actual_engine: EngineId,
    },
}

impl std::fmt::Display for EngineSetupError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Lifecycle(error) => error.fmt(formatter),
            Self::Registry(error) => error.fmt(formatter),
            Self::CapabilityRegistry(error) => error.fmt(formatter),
            Self::CapabilityOwnerMismatch {
                capability_id,
                expected_engine,
                actual_engine,
            } => write!(
                formatter,
                "capability {} is owned by engine {}, expected {}",
                capability_id.as_str(),
                actual_engine.as_str(),
                expected_engine.as_str(),
            ),
        }
    }
}

impl std::error::Error for EngineSetupError {}

impl From<InvalidTransition> for EngineSetupError {
    fn from(error: InvalidTransition) -> Self {
        Self::Lifecycle(error)
    }
}

impl From<ControlPlaneRegistryError> for EngineSetupError {
    fn from(error: ControlPlaneRegistryError) -> Self {
        Self::Registry(error)
    }
}

impl From<CapabilityRegistryError> for EngineSetupError {
    fn from(error: CapabilityRegistryError) -> Self {
        Self::CapabilityRegistry(error)
    }
}

/// Error produced while handling a request at the Indexing Engine boundary.
///
/// Core remains authoritative for request admission. The additional target
/// errors here describe a local-engine routing invariant that is not part of
/// Core's lifecycle admission error contract.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RequestHandlingError {
    Admission(RequestAdmissionError),
    TargetEngineMismatch {
        expected: EngineId,
        actual: EngineId,
    },
    TargetInstanceMismatch {
        expected: EngineInstanceId,
        actual: EngineInstanceId,
    },
}

impl std::fmt::Display for RequestHandlingError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Admission(error) => error.fmt(formatter),
            Self::TargetEngineMismatch { expected, actual } => write!(
                formatter,
                "request target engine {} does not match local engine {}",
                actual.as_str(),
                expected.as_str(),
            ),
            Self::TargetInstanceMismatch { expected, actual } => write!(
                formatter,
                "request target instance {} does not match local instance {}",
                actual.as_str(),
                expected.as_str(),
            ),
        }
    }
}

impl std::error::Error for RequestHandlingError {}

impl From<RequestAdmissionError> for RequestHandlingError {
    fn from(error: RequestAdmissionError) -> Self {
        Self::Admission(error)
    }
}

/// Result of handling one admitted UniversalRequest.
///
/// Core's admission error remains preserved inside [`RequestHandlingError`],
/// while local request-target validation has its own boundary error. Successful
/// capability execution yields a Core `UniversalResponse`.
pub type UniversalRequestResult =
    Result<Result<UniversalResponse, CapabilityError>, RequestHandlingError>;

impl IndexingEngine {
    /// Creates a new Indexing Engine facade in the Core `Created` state.
    #[must_use]
    pub fn new(engine_id: EngineId, engine_instance_id: EngineInstanceId) -> Self {
        Self {
            registration: IndexingRegistration::new(engine_id.clone(), engine_instance_id.clone()),
            runtime: IndexingRuntime::new(engine_id, engine_instance_id),
            capabilities: CapabilitySet::new(),
            engine_registered: AtomicBool::new(false),
            phase0_capability_registered: AtomicBool::new(false),
        }
    }

    /// Returns the logical engine identity shared by registration and runtime.
    #[must_use]
    pub fn engine_id(&self) -> &EngineId {
        self.registration.engine_id()
    }

    /// Returns the concrete engine-instance identity shared by registration
    /// and runtime.
    #[must_use]
    pub fn engine_instance_id(&self) -> &EngineInstanceId {
        self.registration.engine_instance_id()
    }

    /// Returns the declarative Indexing registration boundary.
    #[must_use]
    pub fn registration(&self) -> &IndexingRegistration {
        &self.registration
    }

    /// Returns the Indexing runtime adapter.
    #[must_use]
    pub fn runtime(&self) -> &IndexingRuntime {
        &self.runtime
    }

    /// Returns the Indexing capability integration boundary.
    #[must_use]
    pub fn capabilities(&self) -> &CapabilitySet {
        &self.capabilities
    }

    /// Advances the runtime through the non-registration startup states.
    pub fn start(&self) -> Result<(), InvalidTransition> {
        self.runtime.start()
    }

    /// Enters the Core `Registering` lifecycle state.
    ///
    /// Engine registration remains separate from the lifecycle transition and
    /// from capability registration, while the facade preserves the underlying
    /// Core lifecycle and registry error types in [`EngineSetupError`].
    pub fn begin_registration(&self) -> Result<(), InvalidTransition> {
        self.runtime.begin_registration()
    }

    /// Registers this engine instance through the Core Control Plane registry.
    ///
    /// Registration is only valid during the Core `Registering` lifecycle state.
    /// The returned error is a thin composition of the existing Core lifecycle
    /// and Core registry errors. No new failure category is introduced.
    pub fn register_engine(&self, registry: &EngineRegistry) -> Result<(), EngineSetupError> {
        self.require_state(LifecycleState::Registering)?;

        self.registration
            .register(registry)
            .map_err(EngineSetupError::Registry)?;

        self.engine_registered.store(true, Ordering::Release);

        Ok(())
    }

    /// Registers one capability for this Indexing Engine.
    ///
    /// Capability registration is accepted only while the Core runtime is in
    /// `Registering`, and the definition must name this engine as its owner.
    /// The underlying registry and registration errors remain Core-owned.
    pub fn register_capability(
        &self,
        definition: CapabilityDefinition,
        handler: Arc<dyn CapabilityHandler>,
    ) -> Result<(), EngineSetupError> {
        self.require_state(LifecycleState::Registering)?;

        if definition.owning_engine() != self.engine_id() {
            return Err(EngineSetupError::CapabilityOwnerMismatch {
                capability_id: definition.capability_id().clone(),
                expected_engine: self.engine_id().clone(),
                actual_engine: definition.owning_engine().clone(),
            });
        }

        self.capabilities
            .register(definition, handler)
            .map_err(EngineSetupError::CapabilityRegistry)
    }

    /// Registers the private Phase 0 bootstrap capability under this engine's
    /// logical `EngineId`.
    ///
    /// Capability registration remains separate from engine registration, but
    /// both are required before the engine may become `Ready`.
    pub fn register_phase0_capability(
        &self,
    ) -> Result<nizaam_core::identity::CapabilityId, EngineSetupError> {
        self.require_state(LifecycleState::Registering)?;

        let capability_id = self
            .capabilities
            .register_phase0_capability(self.engine_id())
            .map_err(EngineSetupError::CapabilityRegistry)?;

        self.phase0_capability_registered
            .store(true, Ordering::Release);

        Ok(capability_id)
    }

    /// Marks the engine ready only after required Phase 0 registration state
    /// has been established successfully.
    pub fn mark_ready(&self) -> Result<(), InvalidTransition> {
        if !self.engine_registered.load(Ordering::Acquire)
            || !self.phase0_capability_registered.load(Ordering::Acquire)
        {
            return Err(InvalidTransition::new(
                format!("{:?}", self.runtime.state()),
                format!("{:?}", LifecycleState::Ready),
            ));
        }

        self.runtime.mark_ready()
    }

    /// Explicitly enters `Serving` after the engine has reached `Ready`.
    pub fn serve(&self) -> Result<(), InvalidTransition> {
        self.runtime.serve()
    }

    /// Handles one universal request through the complete Phase 0 public
    /// request boundary.
    ///
    /// The facade preserves the Core request contract and constructs the
    /// corresponding `EngineContext` from the request's `OperationContext`.
    /// Core remains authoritative for lifecycle admission, capability
    /// resolution, cancellation, deadlines, and handler invocation.
    pub fn handle_request(&self, request: &UniversalRequest) -> UniversalRequestResult {
        self.runtime.admit_request()?;

        let envelope = &request.universal_event().envelope;
        let participants = &envelope.metadata.participants;

        if participants.target != self.engine_id().clone() {
            return Err(RequestHandlingError::TargetEngineMismatch {
                expected: self.engine_id().clone(),
                actual: participants.target.clone(),
            });
        }

        if let Some(target_instance) = participants.target_instance.as_ref()
            && target_instance != self.engine_instance_id()
        {
            return Err(RequestHandlingError::TargetInstanceMismatch {
                expected: self.engine_instance_id().clone(),
                actual: target_instance.clone(),
            });
        }

        let descriptor = &envelope.metadata.descriptor;

        let context = self.runtime.context(envelope.operation_context.clone());
        let invocation = CapabilityInvocation::new(
            descriptor.capability_id.clone(),
            descriptor.contract_id.clone(),
            envelope.payload.bytes().to_vec(),
        );

        match self.capabilities.dispatch(&context, &invocation) {
            CapabilityDispatchResult::Outcome(outcome) => {
                Ok(Ok(self.response_from_outcome(request, outcome)))
            }
            CapabilityDispatchResult::Error(error) => Ok(Err(error)),
        }
    }

    fn response_from_outcome(
        &self,
        request: &UniversalRequest,
        outcome: CapabilityOutcome,
    ) -> UniversalResponse {
        let request_envelope = &request.universal_event().envelope;
        let request_participants = &request_envelope.metadata.participants;
        let mut metadata = request_envelope.metadata.clone();
        metadata.descriptor.interaction = Interaction::Response;

        let mut participants = Participants::new(
            self.engine_id().clone(),
            request_participants.sender.clone(),
        )
        .with_sender_instance(self.engine_instance_id().clone());

        if let Some(instance) = request_participants.sender_instance.clone() {
            participants = participants.with_target_instance(instance);
        }

        metadata.participants = participants;

        let payload_descriptor = metadata.descriptor.payload.clone();
        let response_envelope = MessageEnvelope::new(
            MessageId::generate(),
            request_envelope.operation_context.clone(),
            metadata,
            EncodedPayload::new(payload_descriptor, outcome.into_bytes()),
        );

        UniversalResponse::new(response_envelope, Status::Success)
    }

    fn require_state(&self, expected: LifecycleState) -> Result<(), EngineSetupError> {
        let current = self.runtime.state();
        if current == expected {
            Ok(())
        } else {
            Err(EngineSetupError::Lifecycle(InvalidTransition::new(
                format!("{current:?}"),
                format!("{expected:?}"),
            )))
        }
    }

    /// Explicitly enters `Draining` through the Core runtime.
    pub fn drain(&self) -> Result<(), InvalidTransition> {
        self.runtime.drain()
    }

    /// Gracefully shuts down through the Core runtime.
    pub fn shutdown(&self) -> Result<bool, InvalidTransition> {
        self.runtime.shutdown()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    };

    use nizaam_core::capability::{
        CapabilityDefinition, CapabilityDispatchResult, CapabilityError, CapabilityInvocation,
        CapabilityOutcome, arc_handler,
    };
    use nizaam_core::contracts::{
        ContractDescriptor, EncodedPayload, Interaction, MessageEnvelope, Participants,
        PayloadDescriptor, UniversalRequest, Version,
    };
    use nizaam_core::control_plane::registry::RegistryError as ControlPlaneRegistryError;
    use nizaam_core::identity::{CapabilityId, ContractId, CorrelationId, MessageId, OperationId};
    use nizaam_core::operation::{Operation, OperationContext};

    fn engine_id(value: &str) -> EngineId {
        EngineId::new(value).expect("test engine id must be valid")
    }

    fn instance_id(value: &str) -> EngineInstanceId {
        EngineInstanceId::new(value).expect("test instance id must be valid")
    }

    fn operation_context(name: &str) -> OperationContext {
        OperationContext::new(Operation::new(
            OperationId::new(format!("{name}.operation")).expect("test operation id must be valid"),
            CorrelationId::new(format!("{name}.correlation"))
                .expect("test correlation id must be valid"),
        ))
    }

    fn new_engine() -> IndexingEngine {
        IndexingEngine::new(
            engine_id("nizaam.indexing.test"),
            instance_id("nizaam.indexing.test.instance"),
        )
    }

    fn prepare_serving_engine() -> (IndexingEngine, EngineRegistry, CapabilityId) {
        let engine = new_engine();
        let registry = EngineRegistry::new();

        engine.start().unwrap();
        engine.begin_registration().unwrap();
        engine.register_engine(&registry).unwrap();
        let capability_id = engine.register_phase0_capability().unwrap();
        engine.mark_ready().unwrap();
        engine.serve().unwrap();

        (engine, registry, capability_id)
    }

    fn invocation(capability_id: CapabilityId, payload: &[u8]) -> CapabilityInvocation {
        CapabilityInvocation::new(
            capability_id,
            ContractId::new("nizaam.indexing.phase0.module-test.contract")
                .expect("test contract id must be valid"),
            payload.to_vec(),
        )
    }

    #[test]
    fn construction_keeps_identity_aligned_across_module_boundaries() {
        let engine = new_engine();

        assert_eq!(engine.engine_id().as_str(), "nizaam.indexing.test");
        assert_eq!(
            engine.engine_instance_id().as_str(),
            "nizaam.indexing.test.instance"
        );
        assert_eq!(
            engine.registration().engine_id(),
            engine.runtime().engine_id()
        );
        assert_eq!(
            engine.registration().engine_instance_id(),
            engine.runtime().engine_instance_id()
        );
        assert_eq!(engine.runtime().state(), LifecycleState::Created);
        assert!(engine.capabilities().is_empty());
    }

    #[test]
    fn runtime_start_precedes_engine_registration() {
        let engine = new_engine();
        let registry = EngineRegistry::new();

        let result = engine.begin_registration();

        assert!(result.is_err());
        assert_eq!(engine.runtime().state(), LifecycleState::Created);
        assert!(!registry.contains(&instance_id("nizaam.indexing.test.instance")));

        engine.start().unwrap();
        assert_eq!(engine.runtime().state(), LifecycleState::Capabilities);

        engine.begin_registration().unwrap();
        engine.register_engine(&registry).unwrap();
        assert_eq!(engine.runtime().state(), LifecycleState::Registering);
        assert!(registry.contains(&instance_id("nizaam.indexing.test.instance")));
    }

    #[test]
    fn engine_registration_and_capability_registration_remain_distinct() {
        let engine = new_engine();
        let registry = EngineRegistry::new();

        engine.start().unwrap();
        engine.begin_registration().unwrap();
        engine.register_engine(&registry).unwrap();

        assert!(registry.contains(engine.runtime().engine_instance_id()));
        assert!(engine.capabilities().is_empty());
        assert_eq!(engine.runtime().state(), LifecycleState::Registering);

        let capability_id = engine.register_phase0_capability().unwrap();

        assert!(engine.capabilities().contains(&capability_id));
        assert_eq!(
            engine
                .capabilities()
                .registry()
                .get(&capability_id)
                .expect("registered capability must exist")
                .definition()
                .owning_engine(),
            engine.engine_id()
        );
        assert_eq!(engine.capabilities().len(), 1);
        assert!(registry.contains(engine.registration().engine_instance_id()));
        assert_eq!(engine.runtime().state(), LifecycleState::Registering);
    }

    #[test]
    fn engine_registration_is_rejected_outside_the_core_registering_state() {
        let engine = new_engine();
        let registry = EngineRegistry::new();

        let result = engine.register_engine(&registry);

        assert!(matches!(
            result,
            Err(EngineSetupError::Lifecycle(InvalidTransition { from, to }))
                if from == "Created" && to == "Registering"
        ));
        assert!(!registry.contains(engine.engine_instance_id()));
    }

    #[test]
    fn phase0_capability_registration_is_rejected_outside_the_core_registering_state() {
        let engine = new_engine();

        let result = engine.register_phase0_capability();

        assert!(matches!(
            result,
            Err(EngineSetupError::Lifecycle(InvalidTransition { from, to }))
                if from == "Created" && to == "Registering"
        ));
        assert!(engine.capabilities().is_empty());
    }

    #[test]
    fn ready_requires_successful_engine_registration_and_phase0_capability_registration() {
        let engine = new_engine();
        let registry = EngineRegistry::new();

        engine.start().unwrap();
        engine.begin_registration().unwrap();

        assert!(engine.mark_ready().is_err());

        engine.register_engine(&registry).unwrap();
        assert!(engine.mark_ready().is_err());

        engine.register_phase0_capability().unwrap();
        engine.mark_ready().unwrap();
        assert_eq!(engine.runtime().state(), LifecycleState::Ready);
    }

    #[test]
    fn successful_registration_and_capability_setup_can_reach_ready_then_serving() {
        let (engine, registry, capability_id) = prepare_serving_engine();

        assert!(registry.contains(engine.registration().engine_instance_id()));
        assert!(engine.capabilities().contains(&capability_id));
        assert_eq!(engine.runtime().state(), LifecycleState::Serving);
        assert!(engine.runtime().admit_request().is_ok());
    }

    #[test]
    fn full_phase0_request_flow_crosses_runtime_and_capability_modules() {
        let (engine, _registry, capability_id) = prepare_serving_engine();
        let context = engine.runtime().context(operation_context("full-flow"));
        let invocation = invocation(capability_id, b"module-level-payload");

        let result = engine
            .runtime()
            .dispatch(engine.capabilities(), &context, &invocation)
            .expect("Serving runtime should admit the request");

        match result {
            CapabilityDispatchResult::Outcome(outcome) => {
                assert_eq!(outcome.as_bytes(), b"module-level-payload");
            }
            CapabilityDispatchResult::Error(error) => {
                panic!("unexpected capability error: {error:?}");
            }
        }
    }

    fn universal_request(payload: &[u8]) -> UniversalRequest {
        let capability_id = CapabilityId::new("nizaam.indexing.phase0.probe")
            .expect("test capability id must be valid");
        let contract_id = ContractId::new("nizaam.indexing.phase0.public.request")
            .expect("test contract id must be valid");
        let version = Version::new(1, 0, 0);
        let payload_descriptor =
            PayloadDescriptor::new("application/octet-stream", version.clone())
                .expect("test payload descriptor must be valid");
        let descriptor = ContractDescriptor::new(
            contract_id,
            capability_id,
            version,
            Interaction::Request,
            payload_descriptor.clone(),
        );
        let metadata = nizaam_core::contracts::ContractMetadata::new(
            descriptor,
            Participants::new(
                EngineId::new("nizaam.test.sender").expect("sender id must be valid"),
                EngineId::new("nizaam.indexing.test").expect("target id must be valid"),
            )
            .with_sender_instance(
                EngineInstanceId::new("nizaam.test.sender.instance")
                    .expect("sender instance id must be valid"),
            )
            .with_target_instance(
                EngineInstanceId::new("nizaam.indexing.test.instance")
                    .expect("target instance id must be valid"),
            ),
        );
        let operation = Operation::new(
            OperationId::new("public-request.operation").expect("operation id must be valid"),
            CorrelationId::new("public-request.correlation").expect("correlation id must be valid"),
        );
        let envelope = MessageEnvelope::new(
            MessageId::new("public-request.message").expect("message id must be valid"),
            OperationContext::new(operation),
            metadata,
            EncodedPayload::new(payload_descriptor, payload.to_vec()),
        );

        UniversalRequest::new(envelope)
    }

    #[test]
    fn public_universal_request_boundary_produces_a_universal_response() {
        let (engine, _registry, _capability_id) = prepare_serving_engine();
        let request = universal_request(b"public-phase0-payload");
        let request_operation = request.universal_event().envelope.operation_context.clone();
        let request_message_id = request.message_id().clone();
        let request_event_id = request.event_id().clone();

        let result = engine.handle_request(&request);
        let response = result
            .expect("Serving runtime should admit the universal request")
            .expect("Phase 0 probe should produce a successful response");

        assert_eq!(response.status, Status::Success);
        assert!(response.has_response_interaction());
        assert_ne!(response.message_id(), &request_message_id);
        assert_eq!(response.event_scope(), "global");
        assert_ne!(response.event_id(), &request_event_id);
        assert_eq!(
            response.universal_event().envelope.operation_context,
            request_operation
        );
        assert_eq!(
            response.universal_event().envelope.payload.bytes(),
            b"public-phase0-payload"
        );

        let participants = &response.universal_event().envelope.metadata.participants;
        assert_eq!(
            participants.sender,
            EngineId::new("nizaam.indexing.test").expect("engine id must be valid")
        );
        assert_eq!(
            participants.target,
            EngineId::new("nizaam.test.sender").expect("sender id must be valid")
        );
        assert_eq!(
            participants.sender_instance.as_ref(),
            Some(
                &EngineInstanceId::new("nizaam.indexing.test.instance")
                    .expect("engine instance id must be valid")
            )
        );
        assert_eq!(
            participants.target_instance.as_ref(),
            Some(
                &EngineInstanceId::new("nizaam.test.sender.instance")
                    .expect("sender instance id must be valid")
            )
        );
    }

    #[test]
    fn public_universal_request_respects_core_admission_before_capability_dispatch() {
        let engine = new_engine();
        let request = universal_request(b"must-not-run");
        let result = engine.handle_request(&request);

        assert!(matches!(
            result,
            Err(RequestHandlingError::Admission(
                RequestAdmissionError::NotServing(LifecycleState::Created)
            ))
        ));
    }

    #[test]
    fn misaddressed_logical_engine_request_is_rejected_before_capability_dispatch() {
        let (engine, _registry, _capability_id) = prepare_serving_engine();
        let mut request = universal_request(b"wrong-engine");
        request.event.envelope.metadata.participants.target =
            EngineId::new("nizaam.other.engine").expect("test engine id must be valid");

        let result = engine.handle_request(&request);

        assert!(matches!(
            result,
            Err(RequestHandlingError::TargetEngineMismatch { expected, actual })
                if expected == engine.engine_id().clone()
                    && actual.as_str() == "nizaam.other.engine"
        ));
    }

    #[test]
    fn misaddressed_engine_instance_request_is_rejected_before_capability_dispatch() {
        let (engine, _registry, _capability_id) = prepare_serving_engine();
        let mut request = universal_request(b"wrong-instance");
        request.event.envelope.metadata.participants.target_instance = Some(
            EngineInstanceId::new("nizaam.other.instance")
                .expect("test engine instance id must be valid"),
        );

        let result = engine.handle_request(&request);

        assert!(matches!(
            result,
            Err(RequestHandlingError::TargetInstanceMismatch { expected, actual })
                if expected == engine.engine_instance_id().clone()
                    && actual.as_str() == "nizaam.other.instance"
        ));
    }

    #[test]
    fn public_capability_registration_rejects_a_foreign_engine_owner() {
        let engine = new_engine();
        let registry = EngineRegistry::new();

        engine.start().unwrap();
        engine.begin_registration().unwrap();

        let capability_id = CapabilityId::new("nizaam.indexing.module.foreign-owner")
            .expect("test capability id must be valid");
        let foreign_engine =
            EngineId::new("nizaam.foreign.engine").expect("test engine id must be valid");
        let definition = CapabilityDefinition::new(
            capability_id.clone(),
            foreign_engine.clone(),
            "Foreign-owner capability",
        )
        .expect("test capability definition must be valid");

        let result = engine.register_capability(
            definition,
            arc_handler(|_, _| Ok(CapabilityOutcome::new(b"must-not-run".to_vec()))),
        );

        assert_eq!(
            result,
            Err(EngineSetupError::CapabilityOwnerMismatch {
                capability_id,
                expected_engine: engine.engine_id().clone(),
                actual_engine: foreign_engine,
            })
        );
        assert!(engine.capabilities().is_empty());
        assert!(!registry.contains(engine.engine_instance_id()));
    }

    #[test]
    fn public_capability_registration_accepts_a_local_engine_owner() {
        let engine = new_engine();
        let registry = EngineRegistry::new();

        engine.start().unwrap();
        engine.begin_registration().unwrap();
        engine.register_engine(&registry).unwrap();

        let capability_id = CapabilityId::new("nizaam.indexing.module.local-owner")
            .expect("test capability id must be valid");
        let definition = CapabilityDefinition::new(
            capability_id.clone(),
            engine.engine_id().clone(),
            "Local-owner capability",
        )
        .expect("test capability definition must be valid");

        engine
            .register_capability(
                definition,
                arc_handler(|_, _| Ok(CapabilityOutcome::new(b"local".to_vec()))),
            )
            .unwrap();

        assert!(engine.capabilities().contains(&capability_id));
        assert_eq!(engine.capabilities().len(), 1);
    }

    #[test]
    fn runtime_context_reaches_the_capability_handler_unchanged() {
        let engine = new_engine();
        let registry = EngineRegistry::new();

        engine.start().unwrap();
        engine.begin_registration().unwrap();
        engine.register_engine(&registry).unwrap();

        let capability_id = CapabilityId::new("nizaam.indexing.module.context")
            .expect("test capability id must be valid");
        let contract_id = ContractId::new("nizaam.indexing.module.context.contract")
            .expect("test contract id must be valid");
        let observed_operation = Arc::new(Mutex::new(None::<String>));
        let observed_operation_by_handler = Arc::clone(&observed_operation);

        let definition = CapabilityDefinition::new(
            capability_id.clone(),
            engine.engine_id().clone(),
            "Level 2 context propagation test capability",
        )
        .expect("test capability definition must be valid")
        .with_version(Version::new(1, 0, 0));

        engine.register_phase0_capability().unwrap();

        engine
            .capabilities()
            .register(
                definition,
                arc_handler(move |context, invocation| {
                    *observed_operation_by_handler.lock().unwrap() =
                        Some(context.operation().operation.id.as_str().to_owned());
                    Ok(CapabilityOutcome::new(invocation.payload_bytes().to_vec()))
                }),
            )
            .unwrap();

        engine.mark_ready().unwrap();
        engine.serve().unwrap();

        let operation = operation_context("cross-module-context");
        let context = engine.runtime().context(operation.clone());
        let invocation =
            CapabilityInvocation::new(capability_id, contract_id, b"context-payload".to_vec());

        let result = engine
            .runtime()
            .dispatch(engine.capabilities(), &context, &invocation)
            .unwrap();

        assert!(result.is_ok());
        assert_eq!(context.operation(), &operation);
        assert_eq!(
            observed_operation.lock().unwrap().as_deref(),
            Some("cross-module-context.operation")
        );
    }

    #[test]
    fn non_serving_runtime_blocks_capability_execution_at_the_module_boundary() {
        let engine = new_engine();
        let invocation_count = Arc::new(AtomicUsize::new(0));
        let capability_id = CapabilityId::new("nizaam.indexing.module.admission")
            .expect("test capability id must be valid");
        let definition = CapabilityDefinition::new(
            capability_id.clone(),
            engine.engine_id().clone(),
            "Level 2 admission test capability",
        )
        .expect("test capability definition must be valid");
        let count = Arc::clone(&invocation_count);

        engine
            .capabilities()
            .register(
                definition,
                arc_handler(move |_context, invocation| {
                    count.fetch_add(1, Ordering::SeqCst);
                    Ok(CapabilityOutcome::new(invocation.payload_bytes().to_vec()))
                }),
            )
            .unwrap();

        let context = engine.runtime().context(operation_context("blocked"));
        let invocation = invocation(capability_id, b"blocked-payload");

        let result = engine
            .runtime()
            .dispatch(engine.capabilities(), &context, &invocation);

        assert!(matches!(
            result,
            Err(RequestAdmissionError::NotServing(LifecycleState::Created))
        ));
        assert_eq!(invocation_count.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn draining_runtime_rejects_new_work_while_core_registration_remains_present() {
        let (engine, registry, capability_id) = prepare_serving_engine();
        let context = engine.runtime().context(operation_context("draining"));
        let invocation = invocation(capability_id, b"draining-payload");

        engine.drain().unwrap();

        assert_eq!(engine.runtime().state(), LifecycleState::Draining);
        assert!(registry.contains(engine.registration().engine_instance_id()));
        assert!(matches!(
            engine
                .runtime()
                .dispatch(engine.capabilities(), &context, &invocation),
            Err(RequestAdmissionError::NotServing(LifecycleState::Draining))
        ));
    }

    #[test]
    fn core_registration_failure_does_not_turn_the_runtime_into_serving() {
        let registry = EngineRegistry::new();

        let first = new_engine();
        first.start().unwrap();
        first.begin_registration().unwrap();
        first.register_engine(&registry).unwrap();

        let second = IndexingEngine::new(
            engine_id("nizaam.indexing.test"),
            instance_id("nizaam.indexing.test.instance"),
        );
        second.start().unwrap();
        second.begin_registration().unwrap();

        let result = second.register_engine(&registry);

        assert_eq!(
            result,
            Err(EngineSetupError::Registry(
                ControlPlaneRegistryError::AlreadyRegistered(instance_id(
                    "nizaam.indexing.test.instance"
                ))
            ))
        );
        assert_eq!(second.runtime().state(), LifecycleState::Registering);
        assert!(second.serve().is_err());
        assert_eq!(second.runtime().state(), LifecycleState::Registering);
    }

    #[test]
    fn shutdown_completes_the_composed_runtime_without_removing_core_registration_metadata() {
        let (engine, registry, _capability_id) = prepare_serving_engine();

        let completed = engine.shutdown().expect("Core shutdown should succeed");

        assert!(completed);
        assert_eq!(engine.runtime().state(), LifecycleState::Stopped);
        assert!(engine.runtime().shutdown_token().is_cancelled());
        assert!(registry.contains(engine.registration().engine_instance_id()));
    }

    #[test]
    fn shutdown_keeps_registration_and_capability_state_as_separate_concerns() {
        let (engine, registry, capability_id) = prepare_serving_engine();

        engine.shutdown().unwrap();

        assert!(registry.contains(engine.registration().engine_instance_id()));
        assert!(engine.capabilities().contains(&capability_id));
        assert_eq!(engine.runtime().state(), LifecycleState::Stopped);
    }

    #[test]
    fn capability_handler_errors_remain_core_capability_errors_across_the_module_facade() {
        let engine = new_engine();
        let registry = EngineRegistry::new();

        engine.start().unwrap();
        engine.begin_registration().unwrap();
        engine.register_engine(&registry).unwrap();

        let capability_id = CapabilityId::new("nizaam.indexing.module.failure")
            .expect("test capability id must be valid");
        let definition = CapabilityDefinition::new(
            capability_id.clone(),
            engine.engine_id().clone(),
            "Level 2 capability error test",
        )
        .expect("test capability definition must be valid");

        engine.register_phase0_capability().unwrap();

        engine
            .capabilities()
            .register(
                definition,
                arc_handler(move |_context, _invocation| {
                    Err(CapabilityError::HandlerFailed(
                        "module-level failure".to_owned(),
                    ))
                }),
            )
            .unwrap();

        engine.mark_ready().unwrap();
        engine.serve().unwrap();

        let context = engine.runtime().context(operation_context("failure"));
        let invocation = invocation(capability_id, b"failure-payload");

        let result = engine
            .runtime()
            .dispatch(engine.capabilities(), &context, &invocation)
            .unwrap();

        assert!(matches!(
            result,
            CapabilityDispatchResult::Error(
                CapabilityError::HandlerFailed(message)
            ) if message == "module-level failure"
        ));
    }

    #[test]
    fn module_facade_does_not_invent_an_indexing_specific_capability_dispatch_error_type() {
        let engine = new_engine();
        let registry = EngineRegistry::new();

        engine.start().unwrap();
        engine.begin_registration().unwrap();
        engine.register_engine(&registry).unwrap();
        let capability_id = engine.register_phase0_capability().unwrap();
        engine.mark_ready().unwrap();
        engine.serve().unwrap();

        let context = engine
            .runtime()
            .context(operation_context("unknown-capability"));
        let unknown_id = CapabilityId::new("nizaam.indexing.module.unknown")
            .expect("test capability id must be valid");
        let invocation = invocation(unknown_id, b"unknown-payload");

        let result = engine
            .runtime()
            .dispatch(engine.capabilities(), &context, &invocation)
            .unwrap();

        assert!(matches!(
            result,
            CapabilityDispatchResult::Error(CapabilityError::Unknown)
        ));
        assert!(engine.capabilities().contains(&capability_id));
    }
}
