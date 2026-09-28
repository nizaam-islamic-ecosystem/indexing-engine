//! Indexing-specific authorization requirements over the Core security boundary.
//!
//! Phase 6 does not introduce a second authorization framework for the
//! Indexing Engine. Core remains authoritative for authentication,
//! authorization evaluation, security context propagation, and authorization
//! decisions.
//!
//! This module supplies only the Indexing-specific target boundary:
//!
//! ```text
//! EngineId
//!     +
//! EngineInstanceId
//! ```
//!
//! The generic authorization decision is delegated to Core's [`Authorizer`].
//! This module does not define roles, permissions, credentials, authentication,
//! or domain-specific authorization semantics.

use std::fmt;

use nizaam_core::{
    identity::{CapabilityId, EngineId, EngineInstanceId},
    security::{
        AuthorizationDecision, AuthorizationError, AuthorizationRequest, Authorizer,
        SecurityContext,
    },
};

/// Indexing-specific authorization target.
///
/// The requirement identifies the logical Indexing engine and the concrete
/// engine instance for which authorization is being evaluated.
///
/// This type deliberately contains no domain-specific authorization policy.
/// Core remains responsible for the generic authorization decision.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IndexingAuthorizationRequirement {
    engine_id: EngineId,
    engine_instance_id: EngineInstanceId,
}

impl IndexingAuthorizationRequirement {
    /// Creates an Indexing authorization requirement for one engine instance.
    #[must_use]
    pub fn new(engine_id: EngineId, engine_instance_id: EngineInstanceId) -> Self {
        Self {
            engine_id,
            engine_instance_id,
        }
    }

    /// Returns the logical engine identity being authorized.
    #[must_use]
    pub fn engine_id(&self) -> &EngineId {
        &self.engine_id
    }

    /// Returns the concrete engine-instance identity being authorized.
    #[must_use]
    pub fn engine_instance_id(&self) -> &EngineInstanceId {
        &self.engine_instance_id
    }

    /// Returns whether the supplied target identifies this exact engine
    /// instance.
    #[must_use]
    pub fn matches_target(
        &self,
        engine_id: &EngineId,
        engine_instance_id: &EngineInstanceId,
    ) -> bool {
        self.engine_id == *engine_id && self.engine_instance_id == *engine_instance_id
    }

    /// Evaluates the requirement through the Core authorization boundary.
    ///
    /// Target validation occurs before the Core [`Authorizer`] is invoked.
    /// Therefore a request addressed to a different logical engine or engine
    /// instance cannot reach authorization evaluation or capability
    /// execution.
    ///
    /// The supplied [`SecurityContext`] is trusted request-scoped security
    /// information established by Core. This method does not authenticate the
    /// caller or construct another security context.
    pub fn authorize(
        &self,
        authorizer: &dyn Authorizer,
        security: &SecurityContext,
        capability: &CapabilityId,
        target_engine_id: &EngineId,
        target_engine_instance_id: &EngineInstanceId,
    ) -> Result<AuthorizationDecision, IndexingAuthorizationError> {
        if self.engine_id != *target_engine_id {
            return Err(IndexingAuthorizationError::EngineMismatch);
        }

        if self.engine_instance_id != *target_engine_instance_id {
            return Err(IndexingAuthorizationError::InstanceMismatch);
        }

        let request =
            AuthorizationRequest::new(security.principal(), security.calling_service(), capability);

        authorizer
            .authorize(&request)
            .map_err(IndexingAuthorizationError::Core)
    }
}

/// Errors produced while enforcing the Indexing authorization boundary.
///
/// A target mismatch is an Indexing integration-boundary failure. A Core
/// authorization failure is preserved separately so callers can distinguish a
/// target-addressing problem from a failure inside the Core authorization
/// mechanism.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum IndexingAuthorizationError {
    /// The request targets a different logical engine.
    EngineMismatch,

    /// The request targets a different concrete engine instance.
    InstanceMismatch,

    /// Core could not complete authorization evaluation.
    Core(AuthorizationError),
}

impl fmt::Display for IndexingAuthorizationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EngineMismatch => {
                formatter.write_str("authorization target engine does not match")
            }
            Self::InstanceMismatch => {
                formatter.write_str("authorization target engine instance does not match")
            }
            Self::Core(error) => write!(formatter, "Core authorization failed: {error}"),
        }
    }
}

impl std::error::Error for IndexingAuthorizationError {}

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use nizaam_core::security::{
        AuthorizationDecision, AuthorizationRequest, PrincipalId, PrincipalIdentity, PrincipalType,
    };

    fn engine_id() -> EngineId {
        EngineId::new("nizaam.indexing").expect("test engine id must be valid")
    }

    fn other_engine_id() -> EngineId {
        EngineId::new("nizaam.other").expect("test engine id must be valid")
    }

    fn engine_instance_id() -> EngineInstanceId {
        EngineInstanceId::new("nizaam.indexing.instance")
            .expect("test engine instance id must be valid")
    }

    fn other_engine_instance_id() -> EngineInstanceId {
        EngineInstanceId::new("nizaam.other.instance")
            .expect("test engine instance id must be valid")
    }

    fn capability_id() -> CapabilityId {
        CapabilityId::new("index.read").expect("test capability id must be valid")
    }

    fn principal() -> PrincipalIdentity {
        PrincipalIdentity::new(
            PrincipalType::User,
            PrincipalId::new("user-1").expect("test principal id must be valid"),
        )
    }

    fn calling_service() -> PrincipalIdentity {
        PrincipalIdentity::new(
            PrincipalType::Service,
            PrincipalId::new("gateway-1").expect("test service id must be valid"),
        )
    }

    fn security_context() -> SecurityContext {
        SecurityContext::new(principal(), Some(calling_service()))
    }

    fn requirement() -> IndexingAuthorizationRequirement {
        IndexingAuthorizationRequirement::new(engine_id(), engine_instance_id())
    }

    #[derive(Debug)]
    struct AllowingAuthorizer;

    impl Authorizer for AllowingAuthorizer {
        fn authorize(
            &self,
            _request: &AuthorizationRequest<'_>,
        ) -> Result<AuthorizationDecision, AuthorizationError> {
            Ok(AuthorizationDecision::Allow)
        }
    }

    #[derive(Debug)]
    struct DenyingAuthorizer;

    impl Authorizer for DenyingAuthorizer {
        fn authorize(
            &self,
            _request: &AuthorizationRequest<'_>,
        ) -> Result<AuthorizationDecision, AuthorizationError> {
            Ok(AuthorizationDecision::Deny)
        }
    }

    #[derive(Debug)]
    struct FailingAuthorizer;

    impl Authorizer for FailingAuthorizer {
        fn authorize(
            &self,
            _request: &AuthorizationRequest<'_>,
        ) -> Result<AuthorizationDecision, AuthorizationError> {
            Err(AuthorizationError::Failed)
        }
    }

    #[derive(Debug)]
    struct RecordingAuthorizer {
        calls: Arc<AtomicUsize>,
        expected_principal: PrincipalIdentity,
        expected_calling_service: Option<PrincipalIdentity>,
        expected_capability: CapabilityId,
    }

    impl Authorizer for RecordingAuthorizer {
        fn authorize(
            &self,
            request: &AuthorizationRequest<'_>,
        ) -> Result<AuthorizationDecision, AuthorizationError> {
            self.calls.fetch_add(1, Ordering::SeqCst);

            assert_eq!(request.principal(), &self.expected_principal);
            assert_eq!(
                request.calling_service(),
                self.expected_calling_service.as_ref()
            );
            assert_eq!(request.capability(), &self.expected_capability);

            Ok(AuthorizationDecision::Allow)
        }
    }

    #[test]
    fn construction_preserves_engine_and_instance_identity() {
        let requirement = requirement();

        assert_eq!(requirement.engine_id(), &engine_id());
        assert_eq!(requirement.engine_instance_id(), &engine_instance_id());
    }

    #[test]
    fn target_matching_requires_both_engine_and_instance_identity() {
        let requirement = requirement();

        assert!(requirement.matches_target(&engine_id(), &engine_instance_id()));

        assert!(!requirement.matches_target(&other_engine_id(), &engine_instance_id()));

        assert!(!requirement.matches_target(&engine_id(), &other_engine_instance_id()));
    }

    #[test]
    fn matching_target_allows_core_authorization() {
        let requirement = requirement();

        let decision = requirement
            .authorize(
                &AllowingAuthorizer,
                &security_context(),
                &capability_id(),
                &engine_id(),
                &engine_instance_id(),
            )
            .expect("matching target should reach Core authorization");

        assert_eq!(decision, AuthorizationDecision::Allow);
    }

    #[test]
    fn engine_mismatch_blocks_core_authorization() {
        let requirement = requirement();
        let calls = Arc::new(AtomicUsize::new(0));

        let authorizer = RecordingAuthorizer {
            calls: Arc::clone(&calls),
            expected_principal: principal(),
            expected_calling_service: Some(calling_service()),
            expected_capability: capability_id(),
        };

        let result = requirement.authorize(
            &authorizer,
            &security_context(),
            &capability_id(),
            &other_engine_id(),
            &engine_instance_id(),
        );

        assert_eq!(result, Err(IndexingAuthorizationError::EngineMismatch));
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn instance_mismatch_blocks_core_authorization() {
        let requirement = requirement();
        let calls = Arc::new(AtomicUsize::new(0));

        let authorizer = RecordingAuthorizer {
            calls: Arc::clone(&calls),
            expected_principal: principal(),
            expected_calling_service: Some(calling_service()),
            expected_capability: capability_id(),
        };

        let result = requirement.authorize(
            &authorizer,
            &security_context(),
            &capability_id(),
            &engine_id(),
            &other_engine_instance_id(),
        );

        assert_eq!(result, Err(IndexingAuthorizationError::InstanceMismatch));
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn core_authorizer_receives_trusted_security_context_and_capability() {
        let requirement = requirement();
        let calls = Arc::new(AtomicUsize::new(0));

        let authorizer = RecordingAuthorizer {
            calls: Arc::clone(&calls),
            expected_principal: principal(),
            expected_calling_service: Some(calling_service()),
            expected_capability: capability_id(),
        };

        let decision = requirement
            .authorize(
                &authorizer,
                &security_context(),
                &capability_id(),
                &engine_id(),
                &engine_instance_id(),
            )
            .expect("Core authorizer should be invoked");

        assert_eq!(decision, AuthorizationDecision::Allow);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn core_denial_is_preserved() {
        let requirement = requirement();

        let result = requirement
            .authorize(
                &DenyingAuthorizer,
                &security_context(),
                &capability_id(),
                &engine_id(),
                &engine_instance_id(),
            )
            .expect("authorization denial is a completed Core decision");

        assert_eq!(result, AuthorizationDecision::Deny);
    }

    #[test]
    fn core_authorization_failure_is_preserved() {
        let requirement = requirement();

        let result = requirement.authorize(
            &FailingAuthorizer,
            &security_context(),
            &capability_id(),
            &engine_id(),
            &engine_instance_id(),
        );

        assert_eq!(
            result,
            Err(IndexingAuthorizationError::Core(AuthorizationError::Failed))
        );
    }

    #[test]
    fn target_boundary_is_checked_before_core_authorization() {
        let requirement = requirement();
        let calls = Arc::new(AtomicUsize::new(0));

        let authorizer = RecordingAuthorizer {
            calls: Arc::clone(&calls),
            expected_principal: principal(),
            expected_calling_service: Some(calling_service()),
            expected_capability: capability_id(),
        };

        let result = requirement.authorize(
            &authorizer,
            &security_context(),
            &capability_id(),
            &other_engine_id(),
            &other_engine_instance_id(),
        );

        assert_eq!(result, Err(IndexingAuthorizationError::EngineMismatch));
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }
}
