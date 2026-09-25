//! Repository-level integration tests for the complete Phase 0 execution path.
//!
//! These tests exercise the public `nizaam-indexing` API together with the
//! universal Core contract types. The goal is to verify composition:
//!
//! ```text
//! UniversalRequest
//!       ↓
//! Indexing Engine public API
//!       ↓
//! lifecycle admission
//!       ↓
//! EngineContext
//!       ↓
//! CapabilityInvocation
//!       ↓
//! Core capability dispatch
//!       ↓
//! CapabilityOutcome
//!       ↓
//! UniversalResponse
//! ```
//!
//! Phase 0 does not implement actual indexing. The registered capability is an
//! opaque bootstrap probe used only to prove the infrastructure boundary.

mod common;

use common::{
    contract_id, engine_id, engine_registry, operation_context, register_engine, test_engine,
    universal_request,
};
use nizaam_core::capability::CapabilityError;
use nizaam_core::contracts::{
    ContractDescriptor, ContractMetadata, EncodedPayload, Interaction, MessageEnvelope,
    Participants, PayloadDescriptor, UniversalRequest, Version,
};
use nizaam_core::identity::{CapabilityId, EngineInstanceId, MessageId};
use nizaam_core::runtime::{LifecycleState, RequestAdmissionError};
use nizaam_core::status::Status;
use nizaam_indexing::{IndexingEngine, RequestHandlingError};

fn request_envelope(
    engine: &IndexingEngine,
    operation: nizaam_core::operation::OperationContext,
    capability_id: CapabilityId,
    message_id: &str,
    payload: &[u8],
) -> MessageEnvelope {
    let descriptor = ContractDescriptor::new(
        contract_id("nizaam.indexing.phase0.request"),
        capability_id,
        Version::new(1, 0, 0),
        Interaction::Request,
        PayloadDescriptor::new("application/octet-stream", Version::new(1, 0, 0))
            .expect("test payload descriptor must be valid"),
    );

    let metadata = ContractMetadata::new(
        descriptor.clone(),
        Participants::new(engine_id("nizaam.test.caller"), engine.engine_id().clone())
            .with_target_instance(engine.engine_instance_id().clone()),
    );

    MessageEnvelope::new(
        MessageId::new(message_id).expect("test message id must be valid"),
        operation,
        metadata,
        EncodedPayload::new(descriptor.payload, payload.to_vec()),
    )
}

fn prepared_engine() -> (
    IndexingEngine,
    nizaam_core::control_plane::registry::EngineRegistry,
    CapabilityId,
) {
    let engine = test_engine();
    let registry = engine_registry();

    engine.start().expect("startup should succeed");
    engine
        .begin_registration()
        .expect("registration lifecycle entry should succeed");
    register_engine(&engine, &registry).expect("engine registration should succeed");
    let capability_id = engine
        .register_phase0_capability()
        .expect("Phase 0 capability registration should succeed");
    engine
        .mark_ready()
        .expect("ready transition should succeed");
    engine.serve().expect("serving transition should succeed");

    (engine, registry, capability_id)
}

#[test]
fn phase0_public_api_completes_universal_request_to_response_flow() {
    let (engine, registry, capability_id) = prepared_engine();
    let operation = operation_context("integration-request-response");

    let request: UniversalRequest = universal_request(request_envelope(
        &engine,
        operation.clone(),
        capability_id,
        "integration-request-message",
        b"phase0-integration-payload",
    ));

    let request_operation = request.universal_event().envelope.operation_context.clone();

    assert!(request.has_request_interaction());
    assert!(!request.event_id().as_str().is_empty());
    assert!(registry.contains(engine.engine_instance_id()));
    assert_eq!(engine.runtime().state(), LifecycleState::Serving);

    let response = engine
        .handle_request(&request)
        .expect("Serving runtime should admit the request")
        .expect("Phase 0 probe should produce a successful response");

    assert_eq!(response.status, Status::Success);
    assert!(response.has_response_interaction());
    assert_eq!(
        response.universal_event().envelope.payload.bytes(),
        b"phase0-integration-payload"
    );
    assert_eq!(
        response.universal_event().envelope.operation_context,
        request_operation
    );
    assert_ne!(request.event_id(), response.event_id());

    engine.shutdown().expect("shutdown should succeed");
    assert_eq!(engine.runtime().state(), LifecycleState::Stopped);
}

#[test]
fn universal_request_context_survives_the_public_runtime_boundary() {
    let (engine, _registry, capability_id) = prepared_engine();
    let operation = operation_context("integration-context");

    let request = universal_request(request_envelope(
        &engine,
        operation.clone(),
        capability_id,
        "integration-context-message",
        b"context-payload",
    ));

    let response = engine
        .handle_request(&request)
        .expect("Serving runtime should admit the request")
        .expect("Phase 0 probe should produce a response");

    assert_eq!(
        request.universal_event().envelope.operation_context,
        operation
    );
    assert_eq!(
        response.universal_event().envelope.operation_context,
        operation
    );
}

#[test]
fn full_request_path_respects_ready_and_draining_admission_boundaries() {
    let engine = test_engine();
    let registry = engine_registry();

    engine.start().unwrap();
    engine.begin_registration().unwrap();
    register_engine(&engine, &registry).unwrap();
    let capability_id = engine.register_phase0_capability().unwrap();
    engine.mark_ready().unwrap();

    let request = universal_request(request_envelope(
        &engine,
        operation_context("integration-admission"),
        capability_id.clone(),
        "integration-admission-message",
        b"admission-payload",
    ));

    assert!(matches!(
        engine.handle_request(&request),
        Err(RequestHandlingError::Admission(
            RequestAdmissionError::NotServing(LifecycleState::Ready),
        ))
    ));

    engine.serve().unwrap();

    let response = engine
        .handle_request(&request)
        .expect("Serving runtime should admit the request")
        .expect("Phase 0 probe should produce a response");
    assert_eq!(response.status, Status::Success);

    engine.drain().unwrap();

    assert!(matches!(
        engine.handle_request(&request),
        Err(RequestHandlingError::Admission(
            RequestAdmissionError::NotServing(LifecycleState::Draining),
        ))
    ));
}

#[test]
fn capability_resolution_comes_from_the_request_contract_without_local_routing() {
    let (engine, _registry, _capability_id) = prepared_engine();
    let unknown_capability = CapabilityId::new("nizaam.indexing.phase0.unknown")
        .expect("unknown capability id must be valid");
    let request = universal_request(request_envelope(
        &engine,
        operation_context("integration-unknown-capability"),
        unknown_capability.clone(),
        "integration-unknown-message",
        b"opaque-payload",
    ));

    let result = engine.handle_request(&request);

    assert!(matches!(result, Ok(Err(CapabilityError::Unknown))));
    assert!(!engine.capabilities().contains(&unknown_capability));
}

#[test]
fn capability_dispatch_remains_core_backed_through_the_public_request_boundary() {
    let (engine, _registry, capability_id) = prepared_engine();
    let request = universal_request(request_envelope(
        &engine,
        operation_context("integration-core-dispatch"),
        capability_id.clone(),
        "integration-core-dispatch-message",
        b"core-dispatch-payload",
    ));

    let response = engine
        .handle_request(&request)
        .expect("Serving runtime should admit the request")
        .expect("Phase 0 probe should return a response");

    assert_eq!(response.status, Status::Success);
    assert_eq!(
        response
            .universal_event()
            .envelope
            .metadata
            .descriptor
            .capability_id,
        capability_id
    );
}

#[test]
fn independent_engine_instances_keep_their_complete_integration_state_separate() {
    let first = IndexingEngine::new(
        engine_id("nizaam.indexing.integration.shared"),
        EngineInstanceId::new("nizaam.indexing.integration.instance.1")
            .expect("test instance id must be valid"),
    );
    let second = IndexingEngine::new(
        engine_id("nizaam.indexing.integration.shared"),
        EngineInstanceId::new("nizaam.indexing.integration.instance.2")
            .expect("test instance id must be valid"),
    );

    assert_eq!(first.engine_id(), second.engine_id());
    assert_ne!(first.engine_instance_id(), second.engine_instance_id());
    assert!(first.capabilities().is_empty());
    assert!(second.capabilities().is_empty());

    first.start().unwrap();
    first.begin_registration().unwrap();
    let registry = engine_registry();
    register_engine(&first, &registry).unwrap();
    first.register_phase0_capability().unwrap();

    assert_eq!(first.capabilities().len(), 1);
    assert_eq!(second.capabilities().len(), 0);
    assert!(registry.contains(first.engine_instance_id()));
    assert!(!registry.contains(second.engine_instance_id()));
}
