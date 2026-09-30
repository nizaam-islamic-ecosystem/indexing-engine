//! Level 3 integration tests for the production `IndexEvent` execution path.
//!
//! These tests exercise the real Indexing Engine boundary:
//!
//! `IndexEvent`
//! -> Core lifecycle admission
//! -> target validation
//! -> typed event validation
//! -> requirement/definition binding
//! -> capacity admission
//! -> Indexing-owned ID assignment
//! -> capability dispatch
//! -> `IndexEventResponse`
//!
//! Durable filesystem artifacts are tested separately in `persistence.rs`.
//! Idempotency, concurrency, and recovery are also intentionally kept in their
//! dedicated test modules.

mod common;

use common::{
    capability_definition, counting_echo_handler, engine, engine_registry, invocation_count,
    invocation_counter, register_engine, remove_test_operation_root, test_capability_id,
    test_capacity_accounting, test_engine_id, test_index_definition, test_index_event,
    test_index_event_capacity_request, test_index_instance_id,
};
use nizaam_core::runtime::LifecycleState;
use nizaam_indexing::IndexEvent;
use nizaam_indexing::identity::IndexAssignedId;

fn prepare_serving_engine() -> (
    nizaam_indexing::engine::runtime::IndexingEngine,
    nizaam_core::control_plane::registry::EngineRegistry,
    std::sync::Arc<std::sync::atomic::AtomicUsize>,
) {
    let engine = engine(test_engine_id(), test_index_instance_id());
    let registry = engine_registry();
    let counter = invocation_counter();

    engine.start().expect("engine startup should succeed");
    engine
        .begin_registration()
        .expect("engine should enter registration");
    register_engine(&engine, &registry).expect("engine registration should succeed");

    let capability_definition = capability_definition(&test_engine_id(), &test_capability_id());
    engine
        .register_capability(
            capability_definition,
            counting_echo_handler(counter.clone()),
        )
        .expect("test capability registration should succeed");

    engine
        .register_phase0_capability()
        .expect("Phase 0 capability registration should succeed");
    engine.mark_ready().expect("engine should become ready");
    engine.serve().expect("engine should enter serving state");

    assert_eq!(engine.runtime().state(), LifecycleState::Serving);

    (engine, registry, counter)
}

fn successful_event(name: &str) -> IndexEvent {
    test_index_event(name)
}

#[test]
fn successful_index_event_is_processed_through_the_real_runtime_boundary() {
    let (engine, _registry, counter) = prepare_serving_engine();
    let event = successful_event("index-event-success");

    let capacity = test_capacity_accounting();
    let response = engine
        .handle_index_event(
            &event,
            &test_index_definition(),
            &capacity,
            test_index_event_capacity_request(),
        )
        .expect("valid IndexEvent should be processed successfully");

    assert_eq!(response.event(), &event);
    assert_eq!(invocation_count(&counter), 1);
    assert_eq!(engine.runtime().state(), LifecycleState::Serving);

    let expected_assigned_id = IndexAssignedId::generate(
        event.requirement().target_reference_type(),
        event.object_reference(),
    );
    assert_eq!(response.assigned_id(), &expected_assigned_id);

    engine.shutdown().expect("engine shutdown should succeed");
    remove_test_operation_root(engine.operation_root());
}

#[test]
fn index_event_preserves_entity_type_through_the_response_boundary() {
    let (engine, _registry, _counter) = prepare_serving_engine();
    let event = successful_event("index-event-entity-type");

    let capacity = test_capacity_accounting();
    let response = engine
        .handle_index_event(
            &event,
            &test_index_definition(),
            &capacity,
            test_index_event_capacity_request(),
        )
        .expect("valid IndexEvent should be processed successfully");

    assert_eq!(response.event().entity_type(), event.entity_type());
    assert_eq!(response.event().source_payload(), event.source_payload());
    assert_eq!(
        response.event().object_reference(),
        event.object_reference()
    );
    assert_eq!(response.event().key_material(), event.key_material());

    engine.shutdown().expect("engine shutdown should succeed");
    remove_test_operation_root(engine.operation_root());
}

#[test]
fn index_event_preserves_core_event_identity_and_operation_context() {
    let (engine, _registry, _counter) = prepare_serving_engine();
    let event = successful_event("index-event-core-identity");

    let expected_event_id = event.event_id().clone();
    let expected_message_id = event.message_id().clone();
    let expected_operation_context = event.operation_context().clone();

    let capacity = test_capacity_accounting();
    let response = engine
        .handle_index_event(
            &event,
            &test_index_definition(),
            &capacity,
            test_index_event_capacity_request(),
        )
        .expect("valid IndexEvent should be processed successfully");

    assert_eq!(response.event().event_id(), &expected_event_id);
    assert_eq!(response.event().message_id(), &expected_message_id);
    assert_eq!(
        response.event().operation_context(),
        &expected_operation_context
    );

    engine.shutdown().expect("engine shutdown should succeed");
    remove_test_operation_root(engine.operation_root());
}

#[test]
fn index_event_dispatches_the_registered_capability_exactly_once() {
    let (engine, _registry, counter) = prepare_serving_engine();
    let event = successful_event("index-event-capability-once");

    let capacity = test_capacity_accounting();
    let _response = engine
        .handle_index_event(
            &event,
            &test_index_definition(),
            &capacity,
            test_index_event_capacity_request(),
        )
        .expect("valid IndexEvent should be processed successfully");

    assert_eq!(invocation_count(&counter), 1);

    engine.shutdown().expect("engine shutdown should succeed");
    remove_test_operation_root(engine.operation_root());
}

#[test]
fn index_event_uses_the_supplied_definition_and_does_not_infer_definition_fields_from_payload() {
    let (engine, _registry, counter) = prepare_serving_engine();

    let event = successful_event("index-event-definition-binding");
    let definition = test_index_definition();

    // The payload intentionally remains opaque source-owned bytes. The runtime
    // receives the logical definition separately and validates it against the
    // typed IndexRequirement carried by the event.
    assert_eq!(event.source_payload(), b"source-owned-payload");
    assert_eq!(
        definition.identity().namespace(),
        event.requirement().namespace()
    );
    assert_eq!(definition.identity().family(), event.requirement().family());
    assert_eq!(
        definition.key_definition(),
        event.requirement().key_definition()
    );
    assert_eq!(
        definition.target_reference_type(),
        event.requirement().target_reference_type()
    );

    let capacity = test_capacity_accounting();
    engine
        .handle_index_event(
            &event,
            &definition,
            &capacity,
            test_index_event_capacity_request(),
        )
        .expect("matching definition and requirement should be accepted");

    assert_eq!(invocation_count(&counter), 1);

    engine.shutdown().expect("engine shutdown should succeed");
    remove_test_operation_root(engine.operation_root());
}
