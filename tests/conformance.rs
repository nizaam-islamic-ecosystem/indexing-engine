//! Phase 0 architectural conformance tests.
//!
//! These tests protect the boundaries established between `nizaam-indexing`
//! and `nizaam-core`. They focus on ownership, identity roles, universal
//! contracts, Core-backed runtime/capability infrastructure, external engine
//! registration, and the deliberately opaque Phase 0 capability.
//!
//! They do not attempt to test future index construction, query execution,
//! storage providers, physical index algorithms, or domain semantics.

mod common;

use common::{
    capability_definition, capability_id, capability_invocation, engine_id, engine_instance_id,
    engine_registry, operation_context, register_engine, test_engine, universal_request,
    universal_response,
};
use nizaam_core::capability::{CapabilityDispatchResult, CapabilityError};
use nizaam_core::contracts::{
    ContractDescriptor, ContractMetadata, EncodedPayload, Interaction, MessageEnvelope,
    Participants, PayloadDescriptor, Version,
};
use nizaam_core::identity::{
    AttemptId, CapabilityId, ContractId, CorrelationId, EngineId, EngineInstanceId, MessageId,
    NodeId, OperationId,
};
use nizaam_core::operation::OperationContext;
use nizaam_core::runtime::{LifecycleState, RequestAdmissionError};
use nizaam_core::status::Status;
use nizaam_indexing::{IndexingEngine, IndexingRegistration, IndexingRuntime};

fn core_shutdown_token(_: &nizaam_core::runtime::CancellationToken) {}

fn core_background_tasks(_: &nizaam_core::runtime::BackgroundTasks) {}

fn envelope(
    sender: EngineId,
    target: EngineId,
    operation_context: OperationContext,
    capability_id: CapabilityId,
    interaction: Interaction,
    message_id: &str,
    payload: &[u8],
) -> MessageEnvelope {
    let descriptor = ContractDescriptor::new(
        ContractId::new("nizaam.indexing.conformance.contract")
            .expect("test contract id must be valid"),
        capability_id,
        Version::new(1, 0, 0),
        interaction,
        PayloadDescriptor::new("application/octet-stream", Version::new(1, 0, 0))
            .expect("payload descriptor must be valid"),
    );

    let metadata = ContractMetadata::new(descriptor.clone(), Participants::new(sender, target));

    MessageEnvelope::new(
        MessageId::new(message_id).expect("test message id must be valid"),
        operation_context,
        metadata,
        EncodedPayload::new(descriptor.payload, payload.to_vec()),
    )
}

#[test]
fn indexing_engine_keeps_core_engine_and_instance_identity_roles_distinct() {
    let engine_id = engine_id("nizaam.indexing.conformance.identity");
    let instance_id = engine_instance_id("nizaam.indexing.conformance.identity.instance");
    let engine = IndexingEngine::new(engine_id.clone(), instance_id.clone());

    assert_eq!(engine.engine_id(), &engine_id);
    assert_eq!(engine.engine_instance_id(), &instance_id);
    assert_ne!(
        engine.engine_id().as_str(),
        engine.engine_instance_id().as_str()
    );
    assert_eq!(engine.registration().engine_id(), &engine_id);
    assert_eq!(engine.registration().engine_instance_id(), &instance_id);
}

#[test]
fn operation_and_attempt_identity_roles_remain_separate_core_values() {
    let operation_id = OperationId::new("nizaam.indexing.conformance.operation")
        .expect("operation id must be valid");
    let correlation_id = CorrelationId::new("nizaam.indexing.conformance.correlation")
        .expect("correlation id must be valid");
    let node_id = NodeId::new("nizaam.indexing.conformance.node").expect("node id must be valid");
    let attempt_id =
        AttemptId::new("nizaam.indexing.conformance.attempt").expect("attempt id must be valid");

    let operation = nizaam_core::operation::Operation::new(operation_id.clone(), correlation_id);
    let context = OperationContext::new(operation.clone()).for_attempt(node_id, attempt_id.clone());

    assert_eq!(context.operation.id, operation_id);
    assert_eq!(context.attempt_id.as_ref(), Some(&attempt_id));
    assert_ne!(
        context.operation.id.as_str(),
        context.attempt_id.as_ref().unwrap().as_str()
    );
}

#[test]
fn universal_request_and_response_use_the_core_contract_boundary() {
    let engine = test_engine();
    let capability_id = CapabilityId::new("nizaam.indexing.conformance.contract-capability")
        .expect("capability id must be valid");
    let operation = operation_context("conformance-universal-contracts");

    let request = universal_request(envelope(
        engine_id("nizaam.conformance.caller"),
        engine.engine_id().clone(),
        operation.clone(),
        capability_id.clone(),
        Interaction::Request,
        "conformance-request-message",
        b"request-payload",
    ));

    let response = universal_response(
        envelope(
            engine.engine_id().clone(),
            engine_id("nizaam.conformance.caller"),
            operation,
            capability_id,
            Interaction::Response,
            "conformance-response-message",
            b"response-payload",
        ),
        Status::Success,
    );

    assert!(request.has_request_interaction());
    assert!(response.has_response_interaction());
    assert_eq!(request.message_id().as_str(), "conformance-request-message");
    assert_eq!(
        response.message_id().as_str(),
        "conformance-response-message"
    );
    assert!(!request.event_id().as_str().is_empty());
    assert!(!response.event_id().as_str().is_empty());
    assert_ne!(request.event_id(), response.event_id());
    assert_ne!(request.event_id().as_str(), request.message_id().as_str());
    assert_ne!(response.event_id().as_str(), response.message_id().as_str());
}

#[test]
fn capability_set_uses_the_core_capability_registry_through_the_engine_boundary() {
    let engine = test_engine();

    engine.start().unwrap();
    engine.begin_registration().unwrap();

    let capability_id = capability_id("nizaam.indexing.conformance.capability");
    engine
        .register_capability(
            capability_definition(engine.engine_id(), &capability_id),
            common::echo_handler(),
        )
        .expect("Core capability registry registration should succeed");

    assert!(engine.capabilities().contains(&capability_id));
    assert_eq!(engine.capabilities().len(), 1);
}

#[test]
fn indexing_runtime_exposes_core_owned_runtime_surfaces_without_a_second_runtime_system() {
    let engine = test_engine();
    let runtime: &IndexingRuntime = engine.runtime();

    core_shutdown_token(runtime.shutdown_token());
    core_background_tasks(runtime.background_tasks());
    assert_eq!(runtime.state(), LifecycleState::Created);
}

#[test]
fn engine_registration_requires_an_external_core_registry() {
    let engine = test_engine();
    let first_registry = engine_registry();
    let second_registry = engine_registry();

    assert!(!first_registry.contains(engine.engine_instance_id()));
    assert!(!second_registry.contains(engine.engine_instance_id()));

    engine.start().unwrap();
    engine.begin_registration().unwrap();
    register_engine(&engine, &first_registry).unwrap();

    assert!(first_registry.contains(engine.engine_instance_id()));
    assert!(!second_registry.contains(engine.engine_instance_id()));
}

#[test]
fn engine_registration_and_capability_registration_are_separate_core_boundaries() {
    let engine = test_engine();
    let registry = engine_registry();

    engine.start().unwrap();
    engine.begin_registration().unwrap();
    register_engine(&engine, &registry).unwrap();

    assert!(registry.contains(engine.engine_instance_id()));
    assert!(engine.capabilities().is_empty());
    assert_eq!(engine.runtime().state(), LifecycleState::Registering);

    let capability_id = engine.register_phase0_capability().unwrap();

    assert!(engine.capabilities().contains(&capability_id));
    assert!(registry.contains(engine.engine_instance_id()));
    assert_eq!(engine.runtime().state(), LifecycleState::Registering);
}

#[test]
fn capability_state_is_not_a_global_registry_shared_by_independent_engines() {
    let first = IndexingEngine::new(
        engine_id("nizaam.indexing.conformance.first"),
        engine_instance_id("nizaam.indexing.conformance.first.instance"),
    );
    let second = IndexingEngine::new(
        engine_id("nizaam.indexing.conformance.second"),
        engine_instance_id("nizaam.indexing.conformance.second.instance"),
    );

    first.start().unwrap();
    first.begin_registration().unwrap();
    first.register_phase0_capability().unwrap();

    assert_eq!(first.capabilities().len(), 1);
    assert!(second.capabilities().is_empty());
}

#[test]
fn core_runtime_admission_remains_the_authoritative_request_gate() {
    let engine = test_engine();
    let registry = engine_registry();

    engine.start().unwrap();

    assert_eq!(
        engine.runtime().admit_request(),
        Err(RequestAdmissionError::NotServing(
            LifecycleState::Capabilities
        ))
    );

    engine.begin_registration().unwrap();
    register_engine(&engine, &registry).unwrap();
    engine.register_phase0_capability().unwrap();
    engine.mark_ready().unwrap();

    assert_eq!(
        engine.runtime().admit_request(),
        Err(RequestAdmissionError::NotServing(LifecycleState::Ready))
    );

    engine.serve().unwrap();
    assert!(engine.runtime().admit_request().is_ok());

    engine.drain().unwrap();
    assert_eq!(
        engine.runtime().admit_request(),
        Err(RequestAdmissionError::NotServing(LifecycleState::Draining))
    );
}

#[test]
fn phase0_capability_remains_opaque_and_domain_neutral() {
    let engine = test_engine();
    let registry = engine_registry();

    engine.start().unwrap();
    engine.begin_registration().unwrap();
    register_engine(&engine, &registry).unwrap();
    let capability_id = engine.register_phase0_capability().unwrap();
    engine.mark_ready().unwrap();
    engine.serve().unwrap();

    let context = engine
        .runtime()
        .context(operation_context("conformance-opaque-payload"));
    let invocation = capability_invocation(
        capability_id.clone(),
        b"opaque-bytes-without-domain-meaning",
    );

    let result = engine
        .runtime()
        .dispatch(engine.capabilities(), &context, &invocation)
        .expect("Serving runtime should admit dispatch");

    match result {
        CapabilityDispatchResult::Outcome(outcome) => {
            assert_eq!(outcome.as_bytes(), b"opaque-bytes-without-domain-meaning");
        }
        CapabilityDispatchResult::Error(error) => {
            panic!("unexpected Core capability error: {error:?}");
        }
    }
}

#[test]
fn unknown_capability_produces_a_core_error_without_indexing_specific_error_translation() {
    let engine = test_engine();
    let registry = engine_registry();

    engine.start().unwrap();
    engine.begin_registration().unwrap();
    register_engine(&engine, &registry).unwrap();
    engine.register_phase0_capability().unwrap();
    engine.mark_ready().unwrap();
    engine.serve().unwrap();

    let unknown = CapabilityId::new("nizaam.indexing.conformance.unknown")
        .expect("unknown capability id must be valid");
    let context = engine
        .runtime()
        .context(operation_context("conformance-core-error"));
    let invocation = capability_invocation(unknown, b"opaque-payload");

    let result = engine
        .runtime()
        .dispatch(engine.capabilities(), &context, &invocation)
        .expect("runtime admission should succeed while Serving");

    assert!(matches!(
        result,
        CapabilityDispatchResult::Error(CapabilityError::Unknown)
    ));
}

#[test]
fn published_engine_registration_can_be_observed_without_turning_the_registry_into_a_router() {
    let engine = test_engine();
    let registry = engine_registry();

    engine.start().unwrap();
    engine.begin_registration().unwrap();
    register_engine(&engine, &registry).unwrap();

    let registration: &IndexingRegistration = engine.registration();

    assert_eq!(registration.engine_id(), engine.engine_id());
    assert_eq!(
        registration.engine_instance_id(),
        engine.engine_instance_id()
    );
    assert!(registry.contains(engine.engine_instance_id()));
    assert_eq!(engine.runtime().state(), LifecycleState::Registering);

    // Registration establishes discoverable identity only. Capability
    // execution still requires the separate runtime/capability path.
    assert!(engine.capabilities().is_empty());
}

#[test]
fn core_identity_types_are_used_directly_at_the_registration_boundary() {
    let engine_id =
        EngineId::new("nizaam.indexing.conformance.registration").expect("engine id must be valid");
    let instance_id = EngineInstanceId::new("nizaam.indexing.conformance.registration.instance")
        .expect("engine instance id must be valid");
    let registration = IndexingRegistration::new(engine_id.clone(), instance_id.clone());

    assert_eq!(registration.engine_id(), &engine_id);
    assert_eq!(registration.engine_instance_id(), &instance_id);
}
