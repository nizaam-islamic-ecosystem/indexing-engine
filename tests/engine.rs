//! Repository-level tests for the Phase 0 Indexing Engine shell.
//!
//! These tests exercise the public engine facade as a whole. They focus on
//! construction, identity, lifecycle orchestration, registration boundaries,
//! readiness/serving admission, draining, and shutdown.
//!
//! Capability execution and the complete UniversalRequest ->
//! UniversalResponse path belong to `integration.rs`. Architectural boundary
//! checks belong to `conformance.rs`.

mod common;

use common::{
    engine, engine_id, engine_instance_id, engine_registry, test_engine, test_engine_instance_id,
};
use nizaam_core::control_plane::registry::RegistryError;
use nizaam_core::error::InvalidTransition;
use nizaam_core::runtime::{LifecycleState, RequestAdmissionError};
use nizaam_indexing::engine::EngineSetupError;

#[test]
fn engine_construction_exposes_stable_engine_and_instance_identity() {
    let engine = test_engine();

    assert_eq!(engine.engine_id().as_str(), "nizaam.indexing.test");
    assert_eq!(
        engine.engine_instance_id().as_str(),
        "nizaam.indexing.test.instance"
    );
    assert_eq!(engine.engine_id(), engine.runtime().engine_id());
    assert_eq!(
        engine.engine_instance_id(),
        engine.runtime().engine_instance_id()
    );
    assert_eq!(engine.registration().engine_id(), engine.engine_id());
    assert_eq!(
        engine.registration().engine_instance_id(),
        engine.engine_instance_id()
    );
}

#[test]
fn engine_starts_with_a_core_created_runtime() {
    let engine = test_engine();

    assert_eq!(engine.runtime().state(), LifecycleState::Created);
    assert!(engine.capabilities().is_empty());
}

#[test]
fn engine_start_initializes_the_core_runtime_through_capabilities() {
    let engine = test_engine();

    engine.start().expect("Phase 0 startup should succeed");

    assert_eq!(engine.runtime().state(), LifecycleState::Capabilities);
}

#[test]
fn engine_rejects_registration_lifecycle_entry_before_startup() {
    let engine = test_engine();

    let result = engine.begin_registration();

    assert!(result.is_err());
    assert_eq!(engine.runtime().state(), LifecycleState::Created);
}

#[test]
fn engine_rejects_ready_and_serving_before_registering_state() {
    let engine = test_engine();

    engine.start().unwrap();

    assert!(engine.mark_ready().is_err());
    assert!(engine.serve().is_err());
    assert_eq!(engine.runtime().state(), LifecycleState::Capabilities);
}

#[test]
fn engine_registration_is_rejected_outside_the_core_registering_state() {
    let engine = test_engine();
    let registry = engine_registry();

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
    let engine = test_engine();

    let result = engine.register_phase0_capability();

    assert!(matches!(
        result,
        Err(EngineSetupError::Lifecycle(InvalidTransition { from, to }))
            if from == "Created" && to == "Registering"
    ));
    assert!(engine.capabilities().is_empty());
}

#[test]
fn engine_registration_is_established_before_readiness_is_attempted() {
    let engine = test_engine();
    let registry = engine_registry();

    engine.start().unwrap();
    engine.begin_registration().unwrap();
    engine
        .register_engine(&registry)
        .expect("engine registration should succeed");

    assert!(registry.contains(&test_engine_instance_id()));
    assert_eq!(engine.runtime().state(), LifecycleState::Registering);

    assert!(engine.mark_ready().is_err());

    engine
        .register_phase0_capability()
        .expect("Phase 0 capability registration should succeed");
    engine.mark_ready().unwrap();
    assert_eq!(engine.runtime().state(), LifecycleState::Ready);
}

#[test]
fn capability_registration_is_a_separate_engine_shell_step() {
    let engine = test_engine();
    let registry = engine_registry();

    engine.start().unwrap();
    engine.begin_registration().unwrap();
    engine.register_engine(&registry).unwrap();

    assert!(registry.contains(engine.runtime().engine_instance_id()));
    assert!(engine.capabilities().is_empty());
    assert_eq!(engine.runtime().state(), LifecycleState::Registering);

    let capability_id = engine
        .register_phase0_capability()
        .expect("Phase 0 capability registration should succeed");

    assert!(engine.capabilities().contains(&capability_id));
    assert_eq!(engine.capabilities().len(), 1);
    assert_eq!(engine.runtime().state(), LifecycleState::Registering);
}

#[test]
fn ready_requires_successful_engine_registration_and_phase0_capability_registration() {
    let engine = test_engine();
    let registry = engine_registry();

    engine.start().unwrap();
    engine.begin_registration().unwrap();

    assert!(engine.mark_ready().is_err());
    assert_eq!(engine.runtime().state(), LifecycleState::Registering);

    engine.register_engine(&registry).unwrap();
    assert!(engine.mark_ready().is_err());
    assert_eq!(engine.runtime().state(), LifecycleState::Registering);

    engine.register_phase0_capability().unwrap();
    engine.mark_ready().unwrap();
    assert_eq!(engine.runtime().state(), LifecycleState::Ready);
}

#[test]
fn successful_registration_and_capability_setup_can_reach_ready_then_serving() {
    let engine = test_engine();
    let registry = engine_registry();

    engine.start().unwrap();
    engine.begin_registration().unwrap();
    engine.register_engine(&registry).unwrap();
    let capability_id = engine.register_phase0_capability().unwrap();
    engine.mark_ready().unwrap();
    engine.serve().unwrap();

    assert!(registry.contains(engine.engine_instance_id()));
    assert!(engine.capabilities().contains(&capability_id));
    assert_eq!(engine.runtime().state(), LifecycleState::Serving);
    assert!(engine.runtime().admit_request().is_ok());
}

#[test]
fn duplicate_engine_instance_registration_does_not_complete_required_setup() {
    let registry = engine_registry();

    let first = test_engine();
    first.start().unwrap();
    first.begin_registration().unwrap();
    first.register_engine(&registry).unwrap();

    let second = engine(
        engine_id("nizaam.indexing.test"),
        engine_instance_id("nizaam.indexing.test.instance"),
    );
    second.start().unwrap();
    second.begin_registration().unwrap();

    let result = second.register_engine(&registry);

    assert!(matches!(
        result,
        Err(EngineSetupError::Registry(
            RegistryError::AlreadyRegistered(_)
        ))
    ));

    second.register_phase0_capability().unwrap();

    assert!(second.mark_ready().is_err());
    assert_eq!(second.runtime().state(), LifecycleState::Registering);
    assert!(!second.runtime().shutdown_token().is_cancelled());
}

#[test]
fn ready_does_not_admit_normal_requests_until_serving() {
    let engine = test_engine();
    let registry = engine_registry();

    engine.start().unwrap();
    engine.begin_registration().unwrap();
    engine.register_engine(&registry).unwrap();
    engine.register_phase0_capability().unwrap();
    engine.mark_ready().unwrap();

    assert_eq!(engine.runtime().state(), LifecycleState::Ready);
    assert_eq!(
        engine.runtime().admit_request(),
        Err(RequestAdmissionError::NotServing(LifecycleState::Ready))
    );

    engine.serve().unwrap();

    assert_eq!(engine.runtime().state(), LifecycleState::Serving);
    assert!(engine.runtime().admit_request().is_ok());
}

#[test]
fn draining_rejects_new_requests_after_serving() {
    let engine = test_engine();
    let registry = engine_registry();

    engine.start().unwrap();
    engine.begin_registration().unwrap();
    engine.register_engine(&registry).unwrap();
    engine.register_phase0_capability().unwrap();
    engine.mark_ready().unwrap();
    engine.serve().unwrap();

    engine.drain().expect("serving engine should drain");

    assert_eq!(engine.runtime().state(), LifecycleState::Draining);
    assert_eq!(
        engine.runtime().admit_request(),
        Err(RequestAdmissionError::NotServing(LifecycleState::Draining))
    );
}

#[test]
fn shutdown_reaches_stopped_and_cancels_core_shutdown_token() {
    let engine = test_engine();
    let registry = engine_registry();

    engine.start().unwrap();
    engine.begin_registration().unwrap();
    engine.register_engine(&registry).unwrap();
    engine.register_phase0_capability().unwrap();
    engine.mark_ready().unwrap();
    engine.serve().unwrap();

    let completed = engine.shutdown().expect("shutdown should succeed");

    assert!(completed);
    assert_eq!(engine.runtime().state(), LifecycleState::Stopped);
    assert!(engine.runtime().shutdown_token().is_cancelled());
}

#[test]
fn stopped_engine_is_terminal_and_shutdown_remains_idempotent() {
    let engine = test_engine();
    let registry = engine_registry();

    engine.start().unwrap();
    engine.begin_registration().unwrap();
    engine.register_engine(&registry).unwrap();
    engine.register_phase0_capability().unwrap();
    engine.mark_ready().unwrap();
    engine.serve().unwrap();

    engine.shutdown().unwrap();
    engine.shutdown().unwrap();

    assert_eq!(engine.runtime().state(), LifecycleState::Stopped);
    assert!(engine.serve().is_err());
    assert!(engine.drain().is_err());
}

#[test]
fn distinct_engine_instances_can_share_one_logical_engine_identity() {
    let registry = engine_registry();

    let first = engine(
        engine_id("nizaam.indexing.shared"),
        engine_instance_id("nizaam.indexing.shared.instance.1"),
    );
    let second = engine(
        engine_id("nizaam.indexing.shared"),
        engine_instance_id("nizaam.indexing.shared.instance.2"),
    );

    first.start().unwrap();
    first.begin_registration().unwrap();
    second.start().unwrap();
    second.begin_registration().unwrap();

    first.register_engine(&registry).unwrap();
    second.register_engine(&registry).unwrap();

    assert_eq!(first.engine_id(), second.engine_id());
    assert_ne!(first.engine_instance_id(), second.engine_instance_id());
    assert!(registry.contains(&engine_instance_id("nizaam.indexing.shared.instance.1")));
    assert!(registry.contains(&engine_instance_id("nizaam.indexing.shared.instance.2")));
}
