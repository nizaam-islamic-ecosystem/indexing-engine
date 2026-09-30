//! Indexing Engine module declarations and Level 2 integration tests.
//!
//! Level 1 tests live inside the individual engine implementation modules.
//! This file contains only Level 2 tests that verify interactions between the
//! existing engine modules. No Indexing Engine implementation or production
//! orchestration logic belongs here.

pub mod capability;
pub mod registration;
pub mod runtime;

pub use capability::CapabilitySet;
pub use registration::{IndexingRegistration, RegistrationResult};
pub use runtime::{
    EngineSetupError, IndexEventHandlingError, IndexingEngine, IndexingRuntime,
    RequestHandlingError, RuntimeDispatchResult, UniversalRequestResult,
};

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    };

    use nizaam_core::capability::{
        CapabilityDispatchResult, CapabilityError, CapabilityInvocation, CapabilityOutcome,
        arc_handler,
    };
    use nizaam_core::contracts::Version;
    use nizaam_core::control_plane::registry::EngineRegistry;
    use nizaam_core::identity::{
        CapabilityId, ContractId, CorrelationId, EngineId, EngineInstanceId, OperationId,
    };
    use nizaam_core::operation::{Operation, OperationContext};
    use nizaam_core::runtime::{LifecycleState, RequestAdmissionError};

    use super::capability::CapabilitySet;
    use super::registration::IndexingRegistration;
    use super::runtime::IndexingRuntime;

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

    fn invocation(capability_id: CapabilityId, payload: &[u8]) -> CapabilityInvocation {
        CapabilityInvocation::new(
            capability_id,
            ContractId::new("nizaam.indexing.engine.level2.contract")
                .expect("test contract id must be valid"),
            payload.to_vec(),
        )
    }

    fn serving_runtime(
        engine_id: EngineId,
        engine_instance_id: EngineInstanceId,
    ) -> (IndexingRuntime, EngineRegistry) {
        let runtime = IndexingRuntime::new(engine_id, engine_instance_id);
        runtime.start().expect("runtime should start");
        runtime
            .begin_registration()
            .expect("runtime should enter registering");
        runtime
            .mark_ready()
            .expect("runtime should reach ready without facade-owned state");
        runtime.serve().expect("runtime should enter serving");

        (runtime, EngineRegistry::new())
    }

    #[test]
    fn registration_and_runtime_share_the_same_engine_identity() {
        let engine = engine_id("nizaam.indexing.level2");
        let instance = instance_id("nizaam.indexing.level2.instance");

        let registration = IndexingRegistration::new(engine.clone(), instance.clone());
        let runtime = IndexingRuntime::new(engine.clone(), instance.clone());

        assert_eq!(registration.engine_id(), runtime.engine_id());
        assert_eq!(
            registration.engine_instance_id(),
            runtime.engine_instance_id()
        );
        assert_eq!(runtime.state(), LifecycleState::Created);
    }

    #[test]
    fn registration_updates_core_registry_without_changing_runtime_lifecycle() {
        let engine = engine_id("nizaam.indexing.level2.registration");
        let instance = instance_id("nizaam.indexing.level2.registration.instance");
        let registration = IndexingRegistration::new(engine, instance.clone());
        let runtime = IndexingRuntime::new(
            registration.engine_id().clone(),
            registration.engine_instance_id().clone(),
        );
        let registry = EngineRegistry::new();

        runtime.start().unwrap();
        runtime.begin_registration().unwrap();
        registration
            .register(&registry)
            .expect("registration should succeed");

        assert!(registry.contains(&instance));
        assert_eq!(runtime.state(), LifecycleState::Registering);
    }

    #[test]
    fn control_plane_index_event_ingress_is_exposed_by_the_runtime_boundary() {
        use super::runtime::IndexingEngine;

        let engine = engine_id("nizaam.indexing.level2.control-plane");
        let instance = instance_id("nizaam.indexing.level2.control-plane.instance");
        let engine = IndexingEngine::new(engine, instance)
            .expect("default IndexingEngine construction should resolve a platform home directory");

        // The assertion is intentionally type-level at Level 2: orchestration
        // remains in `runtime.rs`, while `mod.rs` only composes and exports it.
        let _ingress: fn(
            &IndexingEngine,
            &nizaam_core::contracts::UniversalRequest,
            &crate::index::IndexDefinition,
            &crate::capacity::CapacityAccounting,
            crate::capacity::CapacityRequest,
        ) -> Result<
            crate::event::IndexEventResponse,
            super::runtime::IndexEventHandlingError,
        > = IndexingEngine::handle_control_plane_index_event;

        let _ = engine;
    }

    #[test]
    fn capability_registration_uses_the_same_engine_identity_as_runtime() {
        let engine = engine_id("nizaam.indexing.level2.capability");
        let instance = instance_id("nizaam.indexing.level2.capability.instance");
        let runtime = IndexingRuntime::new(engine.clone(), instance);
        let capabilities = CapabilitySet::new();

        runtime.start().unwrap();
        runtime.begin_registration().unwrap();

        let capability_id = capabilities
            .register_phase0_capability(&engine)
            .expect("Phase 0 capability registration should succeed");

        let entry = capabilities
            .registry()
            .get(&capability_id)
            .expect("registered capability must exist");

        assert_eq!(entry.definition().owning_engine(), &engine);
        assert_eq!(capabilities.len(), 1);
    }

    #[test]
    fn serving_runtime_dispatches_through_capability_set() {
        let engine = engine_id("nizaam.indexing.level2.dispatch");
        let instance = instance_id("nizaam.indexing.level2.dispatch.instance");
        let (runtime, _registry) = serving_runtime(engine.clone(), instance);
        let capabilities = CapabilitySet::new();

        let capability_id = capabilities
            .register_phase0_capability(&engine)
            .expect("Phase 0 capability registration should succeed");

        let context = runtime.context(operation_context("level2-dispatch"));
        let result = runtime
            .dispatch(
                &capabilities,
                &context,
                &invocation(capability_id, b"level2-payload"),
            )
            .expect("Serving runtime should admit dispatch");

        match result {
            CapabilityDispatchResult::Outcome(outcome) => {
                assert_eq!(outcome.as_bytes(), b"level2-payload");
            }
            CapabilityDispatchResult::Error(error) => {
                panic!("unexpected capability error: {error:?}");
            }
        }
    }

    #[test]
    fn runtime_context_reaches_the_capability_handler_unchanged() {
        let engine = engine_id("nizaam.indexing.level2.context");
        let instance = instance_id("nizaam.indexing.level2.context.instance");
        let (runtime, _registry) = serving_runtime(engine.clone(), instance);
        let capabilities = CapabilitySet::new();

        let capability_id = CapabilityId::new("nizaam.indexing.level2.context")
            .expect("test capability id must be valid");
        let contract_id = ContractId::new("nizaam.indexing.level2.context.contract")
            .expect("test contract id must be valid");
        let observed_operation = Arc::new(Mutex::new(None::<String>));
        let observed_operation_by_handler = Arc::clone(&observed_operation);

        capabilities
            .registry()
            .register(
                nizaam_core::capability::CapabilityDefinition::new(
                    capability_id.clone(),
                    engine,
                    "Level 2 context propagation",
                )
                .expect("test capability definition must be valid")
                .with_version(Version::new(1, 0, 0)),
                arc_handler(move |context, invocation| {
                    *observed_operation_by_handler.lock().unwrap() =
                        Some(context.operation().operation.id.as_str().to_owned());
                    Ok(CapabilityOutcome::new(invocation.payload_bytes().to_vec()))
                }),
            )
            .expect("capability registration should succeed");

        let operation = operation_context("level2-context");
        let context = runtime.context(operation.clone());
        let invocation =
            CapabilityInvocation::new(capability_id, contract_id, b"context-payload".to_vec());

        let result = runtime
            .dispatch(&capabilities, &context, &invocation)
            .expect("Serving runtime should admit dispatch");

        assert!(result.is_ok());
        assert_eq!(context.operation(), &operation);
        assert_eq!(
            observed_operation.lock().unwrap().as_deref(),
            Some("level2-context.operation")
        );
    }

    #[test]
    fn non_serving_runtime_blocks_capability_execution() {
        let engine = engine_id("nizaam.indexing.level2.admission");
        let instance = instance_id("nizaam.indexing.level2.admission.instance");
        let runtime = IndexingRuntime::new(engine.clone(), instance);
        let capabilities = CapabilitySet::new();
        let invocation_count = Arc::new(AtomicUsize::new(0));

        let capability_id = CapabilityId::new("nizaam.indexing.level2.admission")
            .expect("test capability id must be valid");
        let definition = nizaam_core::capability::CapabilityDefinition::new(
            capability_id.clone(),
            engine,
            "Level 2 admission",
        )
        .expect("test capability definition must be valid");
        let count = Arc::clone(&invocation_count);

        capabilities
            .registry()
            .register(
                definition,
                arc_handler(move |_context, invocation| {
                    count.fetch_add(1, Ordering::SeqCst);
                    Ok(CapabilityOutcome::new(invocation.payload_bytes().to_vec()))
                }),
            )
            .expect("capability registration should succeed");

        let context = runtime.context(operation_context("level2-blocked"));
        let result = runtime.dispatch(
            &capabilities,
            &context,
            &invocation(capability_id, b"blocked-payload"),
        );

        assert!(matches!(
            result,
            Err(RequestAdmissionError::NotServing(LifecycleState::Created))
        ));
        assert_eq!(invocation_count.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn draining_runtime_rejects_new_work() {
        let engine = engine_id("nizaam.indexing.level2.draining");
        let instance = instance_id("nizaam.indexing.level2.draining.instance");
        let (runtime, _registry) = serving_runtime(engine.clone(), instance);
        let capabilities = CapabilitySet::new();

        let capability_id = capabilities
            .register_phase0_capability(&engine)
            .expect("Phase 0 capability registration should succeed");
        let context = runtime.context(operation_context("level2-draining"));
        let invocation = invocation(capability_id, b"draining-payload");

        runtime.drain().expect("runtime should enter draining");

        assert_eq!(runtime.state(), LifecycleState::Draining);
        assert!(matches!(
            runtime.dispatch(&capabilities, &context, &invocation),
            Err(RequestAdmissionError::NotServing(LifecycleState::Draining))
        ));
    }

    #[test]
    fn shutdown_stops_runtime_and_preserves_registration_metadata() {
        let engine = engine_id("nizaam.indexing.level2.shutdown");
        let instance = instance_id("nizaam.indexing.level2.shutdown.instance");
        let registration = IndexingRegistration::new(engine.clone(), instance.clone());
        let runtime = IndexingRuntime::new(engine, instance.clone());
        let registry = EngineRegistry::new();

        runtime.start().unwrap();
        runtime.begin_registration().unwrap();
        registration.register(&registry).unwrap();
        runtime.mark_ready().unwrap();
        runtime.serve().unwrap();

        let completed = runtime.shutdown().expect("runtime shutdown should succeed");

        assert!(completed);
        assert_eq!(runtime.state(), LifecycleState::Stopped);
        assert!(runtime.shutdown_token().is_cancelled());
        assert!(registry.contains(&instance));
    }

    #[test]
    fn capability_handler_errors_remain_core_capability_errors() {
        let engine = engine_id("nizaam.indexing.level2.failure");
        let instance = instance_id("nizaam.indexing.level2.failure.instance");
        let (runtime, _registry) = serving_runtime(engine.clone(), instance);
        let capabilities = CapabilitySet::new();

        let capability_id = CapabilityId::new("nizaam.indexing.level2.failure")
            .expect("test capability id must be valid");
        let definition = nizaam_core::capability::CapabilityDefinition::new(
            capability_id.clone(),
            engine,
            "Level 2 failure propagation",
        )
        .expect("test capability definition must be valid");

        capabilities
            .registry()
            .register(
                definition,
                arc_handler(move |_context, _invocation| {
                    Err(CapabilityError::HandlerFailed(
                        "module-level failure".to_owned(),
                    ))
                }),
            )
            .expect("capability registration should succeed");

        let context = runtime.context(operation_context("level2-failure"));
        let result = runtime
            .dispatch(
                &capabilities,
                &context,
                &invocation(capability_id, b"failure-payload"),
            )
            .unwrap();

        assert!(matches!(
            result,
            CapabilityDispatchResult::Error(CapabilityError::HandlerFailed(message))
                if message == "module-level failure"
        ));
    }
}
