//! Capability integration for the Nizaam Indexing Engine.
//!
//! Phase 0 uses the capability mechanisms already provided by `nizaam-core`.
//! This module owns only the Indexing-side composition boundary: it keeps the
//! Core capability registry behind an Indexing-facing abstraction, registers
//! engine-owned handlers, and delegates dispatch to Core.
//!
//! No Indexing-specific capability registry or dispatch algorithm is created
//! here. Core remains responsible for capability resolution, cancellation and
//! deadline checks, and handler invocation.

use std::sync::Arc;

use nizaam_core::capability::{
    CapabilityDefinition, CapabilityDispatchResult, CapabilityHandler, CapabilityInvocation,
    CapabilityOutcome, CapabilityRegistry, RegistryError, arc_handler,
};
use nizaam_core::identity::{CapabilityId, EngineId};
use nizaam_core::runtime::EngineContext;

/// Private bootstrap capability used only to prove the Phase 0 capability
/// plumbing.
///
/// This is not a final public Indexing capability name. Later phases will
/// define the real capability surface once the indexing domain model exists.
const PHASE0_CAPABILITY_ID: &str = "nizaam.indexing.phase0.probe";
const PHASE0_CAPABILITY_NAME: &str = "Phase 0 capability probe";

/// Indexing-side capability integration over the Core capability system.
///
/// The registry stored here is the real `nizaam-core::CapabilityRegistry`.
/// `CapabilitySet` does not replace or duplicate that registry; it provides the
/// Indexing-side boundary through which engine-owned capabilities are
/// registered and dispatched.
pub struct CapabilitySet {
    registry: CapabilityRegistry,
}

impl CapabilitySet {
    /// Creates an empty Indexing capability set backed by the Core registry.
    #[must_use]
    pub fn new() -> Self {
        Self {
            registry: CapabilityRegistry::new(),
        }
    }

    /// Returns the underlying Core capability registry to crate-internal
    /// integration code and tests.
    ///
    /// It is not exposed through the public engine API because the Core registry
    /// itself can mutate through shared access. Public callers use the guarded
    /// `IndexingEngine` registration boundary instead.
    #[must_use]
    pub(crate) fn registry(&self) -> &CapabilityRegistry {
        &self.registry
    }

    /// Registers one capability with the Core registry from within the Indexing
    /// Engine crate. Public callers must use the guarded engine facade.
    pub(crate) fn register(
        &self,
        definition: CapabilityDefinition,
        handler: Arc<dyn CapabilityHandler>,
    ) -> Result<(), RegistryError> {
        self.registry.register(definition, handler)
    }

    /// Registers the private Phase 0 bootstrap capability from within the
    /// Indexing Engine crate.
    ///
    /// The handler deliberately performs no indexing work. It simply returns
    /// the opaque request payload so the engine can prove:
    ///
    /// ```text
    /// runtime
    ///     ↓
    /// capability resolution
    ///     ↓
    /// Core dispatch
    ///     ↓
    /// handler
    ///     ↓
    /// CapabilityOutcome
    /// ```
    pub(crate) fn register_phase0_capability(
        &self,
        engine_id: &EngineId,
    ) -> Result<CapabilityId, RegistryError> {
        let capability_id = CapabilityId::new(PHASE0_CAPABILITY_ID)
            .expect("Phase 0 capability identifier must be valid");

        let definition = CapabilityDefinition::new(
            capability_id.clone(),
            engine_id.clone(),
            PHASE0_CAPABILITY_NAME,
        )
        .expect("Phase 0 capability definition must be valid");

        let handler = arc_handler(
            |_context: &EngineContext, invocation: &CapabilityInvocation| {
                Ok(CapabilityOutcome::new(invocation.payload_bytes().to_vec()))
            },
        );

        self.register(definition, handler)?;

        Ok(capability_id)
    }

    /// Dispatches an invocation through the Core capability system.
    ///
    /// Core remains authoritative for cancellation, deadline, lookup, and
    /// handler invocation ordering.
    #[must_use]
    pub fn dispatch(
        &self,
        context: &EngineContext,
        invocation: &CapabilityInvocation,
    ) -> CapabilityDispatchResult {
        nizaam_core::capability::dispatch(self.registry(), context, invocation)
    }

    /// Returns whether a capability is currently registered.
    #[must_use]
    pub fn contains(&self, capability_id: &CapabilityId) -> bool {
        self.registry.contains(capability_id)
    }

    /// Returns the number of capabilities registered in the Core registry.
    #[must_use]
    pub fn len(&self) -> usize {
        self.registry.len()
    }

    /// Returns whether no capabilities are registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.registry.is_empty()
    }
}

impl Default for CapabilitySet {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use nizaam_core::capability::CapabilityError;
    use nizaam_core::identity::{ContractId, CorrelationId, OperationId};
    use nizaam_core::operation::{Operation, OperationContext};
    use nizaam_core::runtime::Deadline;

    fn engine_id() -> EngineId {
        EngineId::new("nizaam.indexing.test").expect("test engine id must be valid")
    }

    fn context(operation: &str) -> EngineContext {
        let operation = Operation::new(
            OperationId::new(operation).expect("test operation id must be valid"),
            CorrelationId::new(format!("{operation}.correlation"))
                .expect("test correlation id must be valid"),
        );

        EngineContext::new(OperationContext::new(operation))
    }

    fn invocation(capability_id: CapabilityId, payload: &[u8]) -> CapabilityInvocation {
        CapabilityInvocation::new(
            capability_id,
            ContractId::new("nizaam.indexing.phase0.test").expect("test contract id must be valid"),
            payload.to_vec(),
        )
    }

    fn definition(engine_id: &EngineId, capability_id: &str) -> CapabilityDefinition {
        CapabilityDefinition::new(
            CapabilityId::new(capability_id).expect("test capability id must be valid"),
            engine_id.clone(),
            "Test capability",
        )
        .expect("test capability definition must be valid")
    }

    #[test]
    fn new_starts_with_empty_core_registry() {
        let capabilities = CapabilitySet::new();

        assert!(capabilities.is_empty());
        assert_eq!(capabilities.len(), 0);
    }

    #[test]
    fn register_phase0_capability_uses_the_indexing_engine_identity() {
        let capabilities = CapabilitySet::new();
        let capability_id = capabilities
            .register_phase0_capability(&engine_id())
            .expect("Phase 0 capability registration must succeed");

        let entry = capabilities
            .registry()
            .get(&capability_id)
            .expect("registered capability must be present");

        assert_eq!(entry.definition().capability_id(), &capability_id);
        assert_eq!(entry.definition().owning_engine(), &engine_id());
        assert_eq!(entry.definition().name(), PHASE0_CAPABILITY_NAME);
        assert_eq!(capabilities.len(), 1);
    }

    #[test]
    fn phase0_capability_dispatch_returns_the_opaque_payload() {
        let capabilities = CapabilitySet::new();
        let capability_id = capabilities
            .register_phase0_capability(&engine_id())
            .expect("Phase 0 capability registration must succeed");
        let context = context("phase0-dispatch-operation");
        let invocation = invocation(capability_id, b"phase0-payload");

        let result = capabilities.dispatch(&context, &invocation);

        assert!(result.is_ok());
        assert_eq!(
            result
                .into_outcome()
                .expect("dispatch should produce an outcome")
                .into_bytes(),
            b"phase0-payload"
        );
    }

    #[test]
    fn custom_handler_receives_the_same_engine_context() {
        let engine = engine_id();
        let capabilities = CapabilitySet::new();
        let capability_id = CapabilityId::new("nizaam.indexing.test.context")
            .expect("test capability id must be valid");
        let observed_operation = Arc::new(Mutex::new(None::<String>));
        let observed_operation_by_handler = Arc::clone(&observed_operation);

        let handler = arc_handler(
            move |context: &EngineContext, invocation: &CapabilityInvocation| {
                *observed_operation_by_handler.lock().unwrap() =
                    Some(context.operation().operation.id.as_str().to_owned());
                Ok(CapabilityOutcome::new(invocation.payload_bytes().to_vec()))
            },
        );

        capabilities
            .register(definition(&engine, "nizaam.indexing.test.context"), handler)
            .expect("custom capability registration must succeed");

        let context = context("phase0-context-operation");
        let invocation = invocation(capability_id, b"context-payload");

        let result = capabilities.dispatch(&context, &invocation);

        assert!(result.is_ok());
        assert_eq!(
            observed_operation.lock().unwrap().as_deref(),
            Some("phase0-context-operation")
        );
    }

    #[test]
    fn duplicate_capability_registration_is_rejected_by_core() {
        let engine = engine_id();
        let capabilities = CapabilitySet::new();
        let capability_id = CapabilityId::new("nizaam.indexing.test.duplicate")
            .expect("test capability id must be valid");

        let first_handler = arc_handler(|_: &EngineContext, _: &CapabilityInvocation| {
            Ok(CapabilityOutcome::new(b"first".to_vec()))
        });
        let second_handler = arc_handler(|_: &EngineContext, _: &CapabilityInvocation| {
            Ok(CapabilityOutcome::new(b"second".to_vec()))
        });

        capabilities
            .register(
                definition(&engine, "nizaam.indexing.test.duplicate"),
                first_handler,
            )
            .expect("first registration must succeed");

        let result = capabilities.register(
            definition(&engine, "nizaam.indexing.test.duplicate"),
            second_handler,
        );

        assert_eq!(
            result,
            Err(RegistryError::AlreadyRegistered(capability_id.clone()))
        );
        assert_eq!(capabilities.len(), 1);
    }

    #[test]
    fn unknown_capability_is_rejected_without_handler_execution() {
        let capabilities = CapabilitySet::new();
        let unknown = CapabilityId::new("nizaam.indexing.test.unknown")
            .expect("test capability id must be valid");
        let context = context("phase0-unknown-operation");
        let invocation = invocation(unknown, b"payload");

        let result = capabilities.dispatch(&context, &invocation);

        assert!(matches!(result.as_error(), Some(CapabilityError::Unknown)));
    }

    #[test]
    fn cancelled_context_never_reaches_the_handler() {
        let engine = engine_id();
        let capabilities = CapabilitySet::new();
        let capability_id = CapabilityId::new("nizaam.indexing.test.cancelled")
            .expect("test capability id must be valid");
        let handler_called = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let handler_called_by_handler = Arc::clone(&handler_called);

        let handler = arc_handler(move |_: &EngineContext, _: &CapabilityInvocation| {
            handler_called_by_handler.store(true, std::sync::atomic::Ordering::SeqCst);
            Ok(CapabilityOutcome::new(b"must-not-run".to_vec()))
        });

        capabilities
            .register(
                definition(&engine, "nizaam.indexing.test.cancelled"),
                handler,
            )
            .expect("cancelled capability registration must succeed");

        let context = context("phase0-cancelled-operation");
        context.cancellation().cancel();
        let invocation = invocation(capability_id, b"payload");

        let result = capabilities.dispatch(&context, &invocation);

        assert!(matches!(
            result.as_error(),
            Some(CapabilityError::Cancelled)
        ));
        assert!(!handler_called.load(std::sync::atomic::Ordering::SeqCst));
    }

    #[test]
    fn expired_context_never_reaches_the_handler() {
        let engine = engine_id();
        let capabilities = CapabilitySet::new();
        let capability_id = CapabilityId::new("nizaam.indexing.test.deadline")
            .expect("test capability id must be valid");
        let handler_called = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let handler_called_by_handler = Arc::clone(&handler_called);

        let handler = arc_handler(move |_: &EngineContext, _: &CapabilityInvocation| {
            handler_called_by_handler.store(true, std::sync::atomic::Ordering::SeqCst);
            Ok(CapabilityOutcome::new(b"must-not-run".to_vec()))
        });

        capabilities
            .register(
                definition(&engine, "nizaam.indexing.test.deadline"),
                handler,
            )
            .expect("deadline capability registration must succeed");

        let context = context("phase0-deadline-operation")
            .with_deadline(Deadline::from_now(Duration::ZERO).expect("deadline must be valid"));
        let invocation = invocation(capability_id, b"payload");

        let result = capabilities.dispatch(&context, &invocation);

        assert!(matches!(
            result.as_error(),
            Some(CapabilityError::DeadlineExpired)
        ));
        assert!(!handler_called.load(std::sync::atomic::Ordering::SeqCst));
    }
}
