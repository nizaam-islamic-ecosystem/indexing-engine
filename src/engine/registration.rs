//! Indexing Engine registration boundary.
//!
//! Phase 0 keeps engine registration deliberately separate from capability
//! registration and from runtime/lifecycle orchestration. The actual registry
//! state remains owned by the Core Control Plane registry.
//!
//! ```text
//! Indexing Engine
//!       |
//!       +-- EngineId
//!       +-- EngineInstanceId
//!       |
//!       v
//! Core EngineRegistration
//!       |
//!       v
//! Core EngineRegistry
//! ```
//!
//! This module does not create a second global registry, routing mechanism,
//! scheduler, or capability registry.

use nizaam_core::control_plane::registration::EngineRegistration as CoreEngineRegistration;
use nizaam_core::control_plane::registry::{EngineRegistry, RegistryError};
use nizaam_core::identity::{EngineId, EngineInstanceId};
use std::fmt;

/// Indexing registration errors are owned by the Core registry.
///
/// The alias intentionally does not introduce an Indexing-specific error
/// taxonomy for failures that Core already defines.
pub type RegistrationResult<T> = Result<T, RegistryError>;

/// Declarative registration for one Indexing Engine instance.
///
/// `EngineId` identifies the logical Indexing Engine, while
/// `EngineInstanceId` identifies one concrete runtime instance.
///
/// The registration value itself is immutable after construction. Actual
/// registration state is maintained by the supplied Core [`EngineRegistry`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IndexingRegistration {
    registration: CoreEngineRegistration,
}

impl IndexingRegistration {
    /// Creates the declarative registration for one Indexing Engine instance.
    #[must_use]
    pub fn new(engine_id: EngineId, engine_instance_id: EngineInstanceId) -> Self {
        Self {
            registration: CoreEngineRegistration::new(engine_id, engine_instance_id),
        }
    }

    /// Returns the logical engine identity.
    #[must_use]
    pub fn engine_id(&self) -> &EngineId {
        self.registration.engine_id()
    }

    /// Returns the concrete engine-instance identity.
    #[must_use]
    pub fn engine_instance_id(&self) -> &EngineInstanceId {
        self.registration.engine_instance_id()
    }

    /// Returns the Core registration value used at the Control Plane boundary.
    ///
    /// This is crate-visible because the engine composition layer may need to
    /// enrich the declarative registration before submitting it to Core. It
    /// does not expose a second registration model to downstream consumers.
    #[cfg(test)]
    #[must_use]
    pub(crate) fn as_core(&self) -> &CoreEngineRegistration {
        &self.registration
    }

    /// Returns an owned Core registration value.
    ///
    /// The returned value is the Core registration itself, not an Indexing
    /// replacement for it.
    #[cfg(test)]
    #[must_use]
    pub(crate) fn into_core(self) -> CoreEngineRegistration {
        self.registration
    }

    /// Registers this engine instance through the Core Control Plane registry.
    ///
    /// The Core registry is keyed by `EngineInstanceId`, so the same concrete
    /// instance cannot be silently registered twice or reassigned to another
    /// logical engine.
    pub fn register(&self, registry: &EngineRegistry) -> RegistrationResult<()> {
        registry.register(self.registration.clone())
    }
}

impl fmt::Display for IndexingRegistration {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "IndexingRegistration(engine_id={}, engine_instance_id={})",
            self.engine_id(),
            self.engine_instance_id()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{IndexingRegistration, RegistrationResult};
    use nizaam_core::control_plane::registry::{EngineRegistry, RegistryError};
    use nizaam_core::identity::{EngineId, EngineInstanceId};

    fn engine_id(value: &str) -> EngineId {
        EngineId::new(value).expect("test engine id must be valid")
    }

    fn instance_id(value: &str) -> EngineInstanceId {
        EngineInstanceId::new(value).expect("test engine instance id must be valid")
    }

    fn registration(engine: &str, instance: &str) -> IndexingRegistration {
        IndexingRegistration::new(engine_id(engine), instance_id(instance))
    }

    #[test]
    fn construction_preserves_logical_engine_and_instance_identity() {
        let registration = registration("nizaam.indexing", "indexing-01");

        assert_eq!(registration.engine_id().as_str(), "nizaam.indexing");
        assert_eq!(registration.engine_instance_id().as_str(), "indexing-01");
        assert_ne!(
            registration.engine_id().as_str(),
            registration.engine_instance_id().as_str()
        );
    }

    #[test]
    fn core_registration_value_preserves_both_identities() {
        let registration = registration("nizaam.indexing", "indexing-01");
        let core_registration = registration.as_core();

        assert_eq!(core_registration.engine_id().as_str(), "nizaam.indexing");
        assert_eq!(
            core_registration.engine_instance_id().as_str(),
            "indexing-01"
        );
    }

    #[test]
    fn registration_uses_the_core_registry_as_the_authoritative_state() {
        let registry = EngineRegistry::new();
        let registration = registration("nizaam.indexing", "indexing-01");

        registration
            .register(&registry)
            .expect("registration should succeed");

        let record = registry
            .get(&instance_id("indexing-01"))
            .expect("registered instance should be discoverable");

        assert_eq!(record.engine_id().as_str(), "nizaam.indexing");
        assert_eq!(record.engine_instance_id().as_str(), "indexing-01");
        assert_eq!(record.registration(), registration.as_core());
    }

    #[test]
    fn duplicate_instance_registration_is_rejected_by_core() {
        let registry = EngineRegistry::new();
        let registration = registration("nizaam.indexing", "indexing-01");

        registration
            .register(&registry)
            .expect("first registration should succeed");

        let result: RegistrationResult<()> = registration.register(&registry);

        assert_eq!(
            result,
            Err(RegistryError::AlreadyRegistered(instance_id("indexing-01")))
        );
    }

    #[test]
    fn one_logical_engine_can_have_multiple_concrete_instances() {
        let registry = EngineRegistry::new();

        registration("nizaam.indexing", "indexing-01")
            .register(&registry)
            .expect("first instance should register");
        registration("nizaam.indexing", "indexing-02")
            .register(&registry)
            .expect("second instance should register");

        assert!(registry.contains(&instance_id("indexing-01")));
        assert!(registry.contains(&instance_id("indexing-02")));
        assert_eq!(
            registry
                .get(&instance_id("indexing-01"))
                .expect("first instance")
                .engine_id()
                .as_str(),
            "nizaam.indexing"
        );
        assert_eq!(
            registry
                .get(&instance_id("indexing-02"))
                .expect("second instance")
                .engine_id()
                .as_str(),
            "nizaam.indexing"
        );
    }

    #[test]
    fn one_instance_cannot_be_reassigned_to_a_different_logical_engine() {
        let registry = EngineRegistry::new();

        registration("nizaam.indexing", "indexing-01")
            .register(&registry)
            .expect("initial registration should succeed");

        let result = registration("nizaam.other", "indexing-01").register(&registry);

        assert_eq!(
            result,
            Err(RegistryError::EngineIdentityMismatch {
                instance_id: instance_id("indexing-01"),
                registered_engine_id: engine_id("nizaam.indexing"),
                requested_engine_id: engine_id("nizaam.other"),
            })
        );
    }

    #[test]
    fn cloning_registration_does_not_change_identity() {
        let registration = registration("nizaam.indexing", "indexing-01");
        let clone = registration.clone();

        assert_eq!(clone, registration);
        assert_eq!(clone.engine_id(), registration.engine_id());
        assert_eq!(
            clone.engine_instance_id(),
            registration.engine_instance_id()
        );
    }

    #[test]
    fn display_contains_both_identity_roles() {
        let registration = registration("nizaam.indexing", "indexing-01");
        let rendered = registration.to_string();

        assert!(rendered.contains("nizaam.indexing"));
        assert!(rendered.contains("indexing-01"));
    }

    #[test]
    fn consumed_core_registration_remains_the_same_registration_value() {
        let registration = registration("nizaam.indexing", "indexing-01");
        let expected = registration.as_core().clone();
        let consumed = registration.into_core();

        assert_eq!(consumed, expected);
    }
}
