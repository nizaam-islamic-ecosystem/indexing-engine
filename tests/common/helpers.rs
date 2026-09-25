//! Shared integration-test construction helpers for `nizaam-indexing`.
//!
//! These helpers are intentionally limited to neutral test fixtures. They do
//! not implement Indexing behavior, lifecycle orchestration, routing, storage,
//! planning, or domain semantics.
//!
//! The helpers centralize repeated construction of the public Core and
//! Indexing values required by repository-level tests while leaving the actual
//! behavior under test visible at the call site.

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use nizaam_core::capability::{
    CapabilityDefinition, CapabilityError, CapabilityHandler, CapabilityInvocation,
    CapabilityOutcome, arc_handler,
};
use nizaam_core::contracts::{MessageEnvelope, UniversalRequest, UniversalResponse, Version};
use nizaam_core::control_plane::registry::EngineRegistry;
use nizaam_core::identity::{
    CapabilityId, ContractId, CorrelationId, EngineId, EngineInstanceId, OperationId,
};
use nizaam_core::operation::{Operation, OperationContext};
use nizaam_core::status::Status;

use nizaam_indexing::{IndexingEngine, IndexingRegistration};

/// Stable logical engine identity used by repository-level tests.
pub const TEST_ENGINE_ID: &str = "nizaam.indexing.test";

/// Stable concrete engine-instance identity used by repository-level tests.
pub const TEST_ENGINE_INSTANCE_ID: &str = "nizaam.indexing.test.instance";

/// Default test contract identity used for opaque capability invocations.
pub const TEST_CONTRACT_ID: &str = "nizaam.indexing.test.contract";

/// Default test capability identity for custom integration-test handlers.
pub const TEST_CAPABILITY_ID: &str = "nizaam.indexing.test.capability";

/// Creates the standard logical Indexing engine identity for tests.
#[must_use]
pub fn test_engine_id() -> EngineId {
    EngineId::new(TEST_ENGINE_ID).expect("test engine id must be valid")
}

/// Creates the standard concrete Indexing engine-instance identity for tests.
#[must_use]
pub fn test_engine_instance_id() -> EngineInstanceId {
    EngineInstanceId::new(TEST_ENGINE_INSTANCE_ID).expect("test engine instance id must be valid")
}

/// Creates an explicitly named logical engine identity for tests.
#[must_use]
pub fn engine_id(value: &str) -> EngineId {
    EngineId::new(value).expect("test engine id must be valid")
}

/// Creates an explicitly named concrete engine-instance identity for tests.
#[must_use]
pub fn engine_instance_id(value: &str) -> EngineInstanceId {
    EngineInstanceId::new(value).expect("test engine instance id must be valid")
}

/// Creates a fresh Indexing Engine facade using the standard test identities.
#[must_use]
pub fn test_engine() -> IndexingEngine {
    IndexingEngine::new(test_engine_id(), test_engine_instance_id())
}

/// Creates a fresh Indexing Engine facade using explicit test identities.
#[must_use]
pub fn engine(engine_id: EngineId, instance_id: EngineInstanceId) -> IndexingEngine {
    IndexingEngine::new(engine_id, instance_id)
}

/// Creates an empty Core-owned engine registry for a test.
#[must_use]
pub fn engine_registry() -> EngineRegistry {
    EngineRegistry::new()
}

/// Returns the declarative registration carried by an Indexing Engine.
#[must_use]
pub fn registration_fixture(engine: &IndexingEngine) -> IndexingRegistration {
    engine.registration().clone()
}

/// Registers an Indexing Engine through the Core engine registry.
///
/// This helper deliberately performs only the registration call. Runtime
/// lifecycle sequencing remains visible in the test so a test cannot silently
/// hide startup or readiness behavior inside a fixture.
pub fn register_engine(
    engine: &IndexingEngine,
    registry: &EngineRegistry,
) -> Result<(), nizaam_indexing::engine::EngineSetupError> {
    engine.register_engine(registry)
}

/// Creates a logical operation with deterministic test identities.
#[must_use]
pub fn operation(name: &str) -> Operation {
    Operation::new(
        OperationId::new(format!("{name}.operation")).expect("test operation id must be valid"),
        CorrelationId::new(format!("{name}.correlation"))
            .expect("test correlation id must be valid"),
    )
}

/// Creates an OperationContext around a deterministic test operation.
#[must_use]
pub fn operation_context(name: &str) -> OperationContext {
    OperationContext::new(operation(name))
}

/// Creates the standard test contract identity.
#[must_use]
pub fn test_contract_id() -> ContractId {
    ContractId::new(TEST_CONTRACT_ID).expect("test contract id must be valid")
}

/// Creates an explicitly named test contract identity.
#[must_use]
pub fn contract_id(value: &str) -> ContractId {
    ContractId::new(value).expect("test contract id must be valid")
}

/// Creates the standard custom capability identity.
#[must_use]
pub fn test_capability_id() -> CapabilityId {
    CapabilityId::new(TEST_CAPABILITY_ID).expect("test capability id must be valid")
}

/// Creates an explicitly named test capability identity.
#[must_use]
pub fn capability_id(value: &str) -> CapabilityId {
    CapabilityId::new(value).expect("test capability id must be valid")
}

/// Creates a minimal Core capability definition owned by `engine_id`.
#[must_use]
pub fn capability_definition(
    engine_id: &EngineId,
    capability_id: &CapabilityId,
) -> CapabilityDefinition {
    CapabilityDefinition::new(
        capability_id.clone(),
        engine_id.clone(),
        "Nizaam Indexing integration-test capability",
    )
    .expect("test capability definition must be valid")
}

/// Creates a capability definition with an explicit name and Core version.
#[must_use]
pub fn versioned_capability_definition(
    engine_id: &EngineId,
    capability_id: &CapabilityId,
    name: &str,
    version: Version,
) -> CapabilityDefinition {
    CapabilityDefinition::new(capability_id.clone(), engine_id.clone(), name)
        .expect("test capability definition must be valid")
        .with_version(version)
}

/// Creates an opaque Core capability invocation using the standard test
/// contract identity.
#[must_use]
pub fn capability_invocation(capability_id: CapabilityId, payload: &[u8]) -> CapabilityInvocation {
    CapabilityInvocation::new(capability_id, test_contract_id(), payload.to_vec())
}

/// Creates an opaque Core capability invocation with an explicit contract.
#[must_use]
pub fn capability_invocation_with_contract(
    capability_id: CapabilityId,
    contract_id: ContractId,
    payload: &[u8],
) -> CapabilityInvocation {
    CapabilityInvocation::new(capability_id, contract_id, payload.to_vec())
}

/// Creates the standard echo test handler.
///
/// The handler performs no Indexing work. It only returns the opaque payload
/// so integration tests can prove the Core dispatch boundary was reached.
#[must_use]
pub fn echo_handler() -> Arc<dyn CapabilityHandler> {
    arc_handler(|_context, invocation| {
        Ok(CapabilityOutcome::new(invocation.payload_bytes().to_vec()))
    })
}

/// Creates a handler that always returns a Core capability failure.
#[must_use]
pub fn failing_handler(message: &str) -> Arc<dyn CapabilityHandler> {
    let message = message.to_owned();

    arc_handler(move |_context, _invocation| Err(CapabilityError::HandlerFailed(message.clone())))
}

/// Creates a handler that records invocation count and echoes the payload.
///
/// This is useful for negative admission and cancellation/deadline tests that
/// must prove the handler was not reached.
#[must_use]
pub fn counting_echo_handler(counter: Arc<AtomicUsize>) -> Arc<dyn CapabilityHandler> {
    arc_handler(move |_context, invocation| {
        counter.fetch_add(1, Ordering::SeqCst);
        Ok(CapabilityOutcome::new(invocation.payload_bytes().to_vec()))
    })
}

/// Creates an invocation counter suitable for [`counting_echo_handler`].
#[must_use]
pub fn invocation_counter() -> Arc<AtomicUsize> {
    Arc::new(AtomicUsize::new(0))
}

/// Reads an invocation counter created by [`invocation_counter`].
#[must_use]
pub fn invocation_count(counter: &AtomicUsize) -> usize {
    counter.load(Ordering::SeqCst)
}

/// Wraps an already-constructed Core message envelope as a UniversalRequest.
///
/// `UniversalRequest::new` remains responsible for construction of the
/// request occurrence metadata, including its generated EventId. The test
/// helper therefore accepts the envelope instead of manufacturing an EventId.
#[must_use]
pub fn universal_request(envelope: MessageEnvelope) -> UniversalRequest {
    UniversalRequest::new(envelope)
}

/// Wraps an already-constructed Core message envelope as a UniversalResponse.
///
/// `UniversalResponse::new` remains responsible for construction of the
/// response occurrence metadata, including its generated EventId.
#[must_use]
pub fn universal_response(envelope: MessageEnvelope, status: Status) -> UniversalResponse {
    UniversalResponse::new(envelope, status)
}
