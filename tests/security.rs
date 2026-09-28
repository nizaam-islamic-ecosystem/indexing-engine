//! Phase 6 security integration tests for the Nizaam Indexing Engine.
//!
//! These tests verify the Indexing-specific security boundary while exercising
//! the generic security mechanisms supplied by `nizaam_core`.
//!
//! The tests deliberately do not create an Indexing authentication system,
//! credential store, authorization policy engine, or middleware framework.
//! Core remains authoritative for authentication, `SecurityContext`
//! propagation, generic authorization, and security middleware.
//!
//! Runtime-level proof that `IndexingEngine::handle_request` cannot bypass
//! Core security belongs to the later integration/E2E tests once the engine
//! request path is wired to a configured Core `ExecutionPipeline`.

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use nizaam_core::{
    identity::{CapabilityId, EngineId, EngineInstanceId},
    security::{
        AuthenticationError, AuthenticationRequest, Authenticator, AuthorizationDecision,
        AuthorizationError, AuthorizationRequest, Authorizer, CredentialExtractor, PrincipalId,
        PrincipalIdentity, PrincipalType, SecurityContext, SecurityMiddleware,
    },
};

use nizaam_core::contracts::{
    ContractDescriptor, ContractMetadata, EncodedPayload, Interaction, MessageEnvelope,
    Participants, PayloadDescriptor, UniversalRequest,
};
use nizaam_core::identity::{ContractId, CorrelationId, MessageId, OperationId};
use nizaam_core::middleware::stages::{Middleware, MiddlewareResult};
use nizaam_core::operation::{Operation, OperationContext};
use nizaam_core::prelude::Version;
use nizaam_core::runtime::EngineContext;

use nizaam_indexing::security::{IndexingAuthorizationError, IndexingAuthorizationRequirement};

fn engine_id(value: &str) -> EngineId {
    EngineId::new(value).expect("test engine id must be valid")
}

fn instance_id(value: &str) -> EngineInstanceId {
    EngineInstanceId::new(value).expect("test engine instance id must be valid")
}

fn user_principal(value: &str) -> PrincipalIdentity {
    PrincipalIdentity::new(
        PrincipalType::User,
        PrincipalId::new(value).expect("test principal id must be valid"),
    )
}

fn service_principal(value: &str) -> PrincipalIdentity {
    PrincipalIdentity::new(
        PrincipalType::Service,
        PrincipalId::new(value).expect("test service id must be valid"),
    )
}

fn operation_context(name: &str) -> OperationContext {
    OperationContext::new(Operation::new(
        OperationId::new(format!("{name}.operation")).expect("operation id must be valid"),
        CorrelationId::new(format!("{name}.correlation")).expect("correlation id must be valid"),
    ))
}

fn context(name: &str) -> EngineContext {
    EngineContext::new(operation_context(name))
}

fn request_with_capability(name: &str, capability: &str) -> UniversalRequest {
    let payload_descriptor =
        PayloadDescriptor::new("application/octet-stream", Version::new(1, 0, 0))
            .expect("payload descriptor must be valid");

    let descriptor = ContractDescriptor::new(
        ContractId::new("indexing.security.test").expect("contract id must be valid"),
        CapabilityId::new(capability).expect("capability id must be valid"),
        Version::new(1, 0, 0),
        Interaction::Request,
        payload_descriptor.clone(),
    );

    let metadata = ContractMetadata::new(
        descriptor,
        Participants::new(
            engine_id("indexing.security.sender"),
            engine_id("indexing.security.receiver"),
        ),
    );

    UniversalRequest::new(MessageEnvelope::new(
        MessageId::new(format!("{name}.message")).expect("message id must be valid"),
        operation_context(name),
        metadata,
        EncodedPayload::new(payload_descriptor, b"opaque".to_vec()),
    ))
}

#[derive(Clone)]
struct StaticAuthenticator {
    result: Result<PrincipalIdentity, AuthenticationError>,
}

impl Authenticator for StaticAuthenticator {
    fn authenticate(
        &self,
        _request: &AuthenticationRequest<'_>,
    ) -> Result<PrincipalIdentity, AuthenticationError> {
        self.result.clone()
    }
}

#[derive(Clone)]
struct RecordingAuthorizer {
    expected_principal: PrincipalIdentity,
    expected_calling_service: Option<PrincipalIdentity>,
    expected_capability: CapabilityId,
    observed: Arc<AtomicBool>,
    decision: Result<AuthorizationDecision, AuthorizationError>,
}

impl Authorizer for RecordingAuthorizer {
    fn authorize(
        &self,
        request: &AuthorizationRequest<'_>,
    ) -> Result<AuthorizationDecision, AuthorizationError> {
        assert_eq!(request.principal(), &self.expected_principal);
        assert_eq!(
            request.calling_service(),
            self.expected_calling_service.as_ref()
        );
        assert_eq!(request.capability(), &self.expected_capability);

        self.observed.store(true, Ordering::SeqCst);
        self.decision
    }
}

#[derive(Clone)]
struct StaticCredentials(Option<Vec<u8>>);

impl CredentialExtractor for StaticCredentials {
    fn extract(&self, _context: &EngineContext, _request: &UniversalRequest) -> Option<Vec<u8>> {
        self.0.clone()
    }
}

#[test]
fn indexing_authorization_requirement_exposes_only_engine_target_boundary() {
    let requirement =
        IndexingAuthorizationRequirement::new(engine_id("indexing"), instance_id("indexing-1"));

    assert_eq!(requirement.engine_id(), &engine_id("indexing"));
    assert_eq!(requirement.engine_instance_id(), &instance_id("indexing-1"));

    assert!(requirement.matches_target(&engine_id("indexing"), &instance_id("indexing-1")));
    assert!(!requirement.matches_target(&engine_id("other"), &instance_id("indexing-1")));
    assert!(!requirement.matches_target(&engine_id("indexing"), &instance_id("other")));
}

#[test]
fn target_mismatch_is_rejected_before_core_authorization() {
    let observed = Arc::new(AtomicBool::new(false));

    let authorizer = RecordingAuthorizer {
        expected_principal: user_principal("target-user"),
        expected_calling_service: None,
        expected_capability: CapabilityId::new("index.query").expect("capability id must be valid"),
        observed: Arc::clone(&observed),
        decision: Ok(AuthorizationDecision::Allow),
    };

    let requirement =
        IndexingAuthorizationRequirement::new(engine_id("indexing"), instance_id("indexing-1"));

    let security = SecurityContext::new(user_principal("target-user"), None);

    let result = requirement.authorize(
        &authorizer,
        &security,
        &CapabilityId::new("index.query").expect("capability id must be valid"),
        &engine_id("other"),
        &instance_id("indexing-1"),
    );

    assert_eq!(result, Err(IndexingAuthorizationError::EngineMismatch));
    assert!(!observed.load(Ordering::SeqCst));
}

#[test]
fn instance_mismatch_is_rejected_before_core_authorization() {
    let observed = Arc::new(AtomicBool::new(false));

    let authorizer = RecordingAuthorizer {
        expected_principal: user_principal("instance-user"),
        expected_calling_service: None,
        expected_capability: CapabilityId::new("index.query").expect("capability id must be valid"),
        observed: Arc::clone(&observed),
        decision: Ok(AuthorizationDecision::Allow),
    };

    let requirement =
        IndexingAuthorizationRequirement::new(engine_id("indexing"), instance_id("indexing-1"));

    let security = SecurityContext::new(user_principal("instance-user"), None);

    let result = requirement.authorize(
        &authorizer,
        &security,
        &CapabilityId::new("index.query").expect("capability id must be valid"),
        &engine_id("indexing"),
        &instance_id("other"),
    );

    assert_eq!(result, Err(IndexingAuthorizationError::InstanceMismatch));
    assert!(!observed.load(Ordering::SeqCst));
}

#[test]
fn core_authorizer_receives_trusted_security_context_and_capability() {
    let observed = Arc::new(AtomicBool::new(false));
    let principal = user_principal("query-user");
    let calling_service = service_principal("gateway");
    let capability = CapabilityId::new("index.query").expect("capability id must be valid");

    let authorizer = RecordingAuthorizer {
        expected_principal: principal.clone(),
        expected_calling_service: Some(calling_service.clone()),
        expected_capability: capability.clone(),
        observed: Arc::clone(&observed),
        decision: Ok(AuthorizationDecision::Allow),
    };

    let requirement =
        IndexingAuthorizationRequirement::new(engine_id("indexing"), instance_id("indexing-1"));

    let security = SecurityContext::new(principal, Some(calling_service));

    let result = requirement.authorize(
        &authorizer,
        &security,
        &capability,
        &engine_id("indexing"),
        &instance_id("indexing-1"),
    );

    assert_eq!(result, Ok(AuthorizationDecision::Allow));
    assert!(observed.load(Ordering::SeqCst));
}

#[test]
fn core_authorization_denial_is_preserved() {
    let requirement =
        IndexingAuthorizationRequirement::new(engine_id("indexing"), instance_id("indexing-1"));

    let authorizer = RecordingAuthorizer {
        expected_principal: user_principal("denied-user"),
        expected_calling_service: None,
        expected_capability: CapabilityId::new("index.delete")
            .expect("capability id must be valid"),
        observed: Arc::new(AtomicBool::new(false)),
        decision: Ok(AuthorizationDecision::Deny),
    };

    let security = SecurityContext::new(user_principal("denied-user"), None);

    let result = requirement.authorize(
        &authorizer,
        &security,
        &CapabilityId::new("index.delete").expect("capability id must be valid"),
        &engine_id("indexing"),
        &instance_id("indexing-1"),
    );

    assert_eq!(result, Ok(AuthorizationDecision::Deny));
}

#[test]
fn core_authorization_failure_is_preserved_as_core_error() {
    let requirement =
        IndexingAuthorizationRequirement::new(engine_id("indexing"), instance_id("indexing-1"));

    let authorizer = RecordingAuthorizer {
        expected_principal: user_principal("failure-user"),
        expected_calling_service: None,
        expected_capability: CapabilityId::new("index.query").expect("capability id must be valid"),
        observed: Arc::new(AtomicBool::new(false)),
        decision: Err(AuthorizationError::Failed),
    };

    let security = SecurityContext::new(user_principal("failure-user"), None);

    let result = requirement.authorize(
        &authorizer,
        &security,
        &CapabilityId::new("index.query").expect("capability id must be valid"),
        &engine_id("indexing"),
        &instance_id("indexing-1"),
    );

    assert_eq!(
        result,
        Err(IndexingAuthorizationError::Core(AuthorizationError::Failed))
    );
}

#[test]
fn core_security_middleware_requires_authentication_material() {
    let observed = Arc::new(AtomicBool::new(false));

    let middleware = SecurityMiddleware::new(
        StaticAuthenticator {
            result: Ok(user_principal("missing-credential-user")),
        },
        RecordingAuthorizer {
            expected_principal: user_principal("missing-credential-user"),
            expected_calling_service: None,
            expected_capability: CapabilityId::new("index.query")
                .expect("capability id must be valid"),
            observed: Arc::clone(&observed),
            decision: Ok(AuthorizationDecision::Allow),
        },
        StaticCredentials(None),
    );

    let mut execution_context = context("security-missing-credentials");
    let mut request = request_with_capability("security-missing-credentials", "index.query");

    assert!(matches!(
        middleware.on_request(&mut execution_context, &mut request),
        MiddlewareResult::Reject(_)
    ));
    assert!(!observed.load(Ordering::SeqCst));
    assert!(execution_context.security().is_none());
}

#[test]
fn core_security_middleware_preserves_trusted_calling_service() {
    let observed = Arc::new(AtomicBool::new(false));
    let calling_service = service_principal("gateway");

    let middleware = SecurityMiddleware::new(
        StaticAuthenticator {
            result: Ok(user_principal("authenticated-user")),
        },
        RecordingAuthorizer {
            expected_principal: user_principal("authenticated-user"),
            expected_calling_service: Some(calling_service.clone()),
            expected_capability: CapabilityId::new("index.query")
                .expect("capability id must be valid"),
            observed: Arc::clone(&observed),
            decision: Ok(AuthorizationDecision::Allow),
        },
        StaticCredentials(Some(b"opaque-credential".to_vec())),
    );

    let mut execution_context = context("security-calling-service");
    execution_context = execution_context.with_security(SecurityContext::new(
        user_principal("preexisting-principal"),
        Some(calling_service.clone()),
    ));

    let mut request = request_with_capability("security-calling-service", "index.query");

    assert_eq!(
        middleware.on_request(&mut execution_context, &mut request),
        MiddlewareResult::Continue
    );
    assert!(observed.load(Ordering::SeqCst));

    let security = execution_context
        .security()
        .expect("Core security context must remain present");

    assert_eq!(security.calling_service(), Some(&calling_service));
    assert_eq!(security.principal(), &user_principal("authenticated-user"));
}

#[test]
fn core_security_middleware_authenticates_before_authorization() {
    let observed = Arc::new(AtomicBool::new(false));

    let middleware = SecurityMiddleware::new(
        StaticAuthenticator {
            result: Ok(user_principal("authenticated-user")),
        },
        RecordingAuthorizer {
            expected_principal: user_principal("authenticated-user"),
            expected_calling_service: None,
            expected_capability: CapabilityId::new("index.query")
                .expect("capability id must be valid"),
            observed: Arc::clone(&observed),
            decision: Ok(AuthorizationDecision::Allow),
        },
        StaticCredentials(Some(b"opaque-credential".to_vec())),
    );

    let mut execution_context = context("security-pipeline");
    let mut request = request_with_capability("security-pipeline", "index.query");

    assert_eq!(
        middleware.on_request(&mut execution_context, &mut request),
        MiddlewareResult::Continue
    );

    assert!(observed.load(Ordering::SeqCst));

    let security = execution_context
        .security()
        .expect("Core security middleware must establish SecurityContext");

    assert_eq!(security.principal(), &user_principal("authenticated-user"));
}

#[test]
fn core_authentication_failure_prevents_authorization() {
    let observed = Arc::new(AtomicBool::new(false));

    let middleware = SecurityMiddleware::new(
        StaticAuthenticator {
            result: Err(AuthenticationError::Failed),
        },
        RecordingAuthorizer {
            expected_principal: user_principal("never-authorized"),
            expected_calling_service: None,
            expected_capability: CapabilityId::new("index.query")
                .expect("capability id must be valid"),
            observed: Arc::clone(&observed),
            decision: Ok(AuthorizationDecision::Allow),
        },
        StaticCredentials(Some(b"opaque-credential".to_vec())),
    );

    let mut execution_context = context("security-auth-failure");
    let mut request = request_with_capability("security-auth-failure", "index.query");

    assert!(matches!(
        middleware.on_request(&mut execution_context, &mut request),
        MiddlewareResult::Fail(_)
    ));
    assert!(!observed.load(Ordering::SeqCst));
    assert!(execution_context.security().is_none());
}

#[test]
fn core_authorization_denial_is_terminal_before_downstream_execution() {
    let observed = Arc::new(AtomicBool::new(false));

    let middleware = SecurityMiddleware::new(
        StaticAuthenticator {
            result: Ok(user_principal("denied-user")),
        },
        RecordingAuthorizer {
            expected_principal: user_principal("denied-user"),
            expected_calling_service: None,
            expected_capability: CapabilityId::new("index.query")
                .expect("capability id must be valid"),
            observed: Arc::clone(&observed),
            decision: Ok(AuthorizationDecision::Deny),
        },
        StaticCredentials(Some(b"opaque-credential".to_vec())),
    );

    let mut execution_context = context("security-denial");
    let mut request = request_with_capability("security-denial", "index.query");

    assert!(matches!(
        middleware.on_request(&mut execution_context, &mut request),
        MiddlewareResult::Reject(_)
    ));

    assert!(observed.load(Ordering::SeqCst));

    // Core middleware terminates the request with Reject; no capability
    // dispatch is performed by this test. Actual Indexing capability
    // non-execution is verified at the engine/E2E boundary.
}
