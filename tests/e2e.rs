//! Phase 6 end-to-end integration tests for the Nizaam Indexing Engine.
//!
//! These tests compose the real public Core and Indexing boundaries rather than
//! introducing a test-only runtime, registry, dispatcher, security framework,
//! or transport implementation.
//!
//! The intended path is:
//!
//! ```text
//! Core Control Plane membership / observation
//!             ↓
//! routing candidates derived from membership
//!             ↓
//! Core routing policy
//!             ↓
//! selected Indexing instance
//!             ↓
//! Core EngineRuntime admission
//!             ↓
//! Core SecurityMiddleware / ExecutionPipeline
//!             ↓
//! IndexingEngine request boundary
//!             ↓
//! Core capability dispatch
//!             ↓
//! UniversalResponse
//!             ↓
//! Core-backed observability
//! ```
//!
//! The current Core Control Plane intentionally exposes its routing pieces
//! separately rather than one stateful `execute` operation. The E2E test
//! therefore composes those public owners explicitly.
//!
//! Destination eligibility is intentionally outside this file's scope. The
//! tests verify that membership and observations establish the real Core
//! routing state, then exercise routing policy over candidates derived from
//! that state without fabricating eligibility metadata.
//!
//! The Indexing Engine currently exposes `handle_request` as its public request
//! execution boundary. Security is therefore composed immediately around that
//! boundary in this E2E test through the real Core `ExecutionPipeline`.
//!
//! This test does not invent an Indexing-owned middleware or runtime.

use nizaam_indexing::engine::runtime::IndexingEngine;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use nizaam_core::contracts::descriptor::{
    ContractDescriptor, EncodedPayload, Interaction, PayloadDescriptor, Version,
};
use nizaam_core::contracts::envelope::MessageEnvelope;
use nizaam_core::contracts::metadata::{ContractMetadata, Participants};
use nizaam_core::contracts::{UniversalRequest, UniversalResponse};
use nizaam_core::control_plane::{
    EngineObservation, EngineRegistration, EngineRegistry, Membership, Observations, PolicyInput,
    RoutingCandidate, RoutingConstraints, RoutingPolicy,
};
use nizaam_core::health::{HealthReport, LivenessReport, ReadinessReport};
use nizaam_core::identity::{
    AttemptId, CapabilityId, ContractId, CorrelationId, EngineId, EngineInstanceId, MessageId,
    NodeId, OperationId,
};
use nizaam_core::operation::{Operation, OperationContext};
use nizaam_core::runtime::pipeline::{ExecutionPipeline, RequestPipelineError};
use nizaam_core::runtime::{EngineContext, LifecycleState};
use nizaam_core::security::{
    AuthenticationError, AuthenticationRequest, Authenticator, AuthorizationDecision,
    AuthorizationError, AuthorizationRequest, Authorizer, CredentialExtractor, PrincipalId,
    PrincipalIdentity, PrincipalType, SecurityContext, SecurityMiddleware,
};
use nizaam_core::status::Status;

use nizaam_indexing::security::{IndexingAuthorizationError, IndexingAuthorizationRequirement};

const ENGINE_ID: &str = "nizaam.indexing.e2e";
const ENGINE_INSTANCE_ID: &str = "nizaam.indexing.e2e.instance";
const CAPABILITY_ID: &str = "nizaam.indexing.phase0.probe";
const CONTRACT_ID: &str = "nizaam.indexing.phase0.e2e";
const MEDIA_TYPE: &str = "application/octet-stream";

fn engine_id(value: &str) -> EngineId {
    EngineId::new(value).expect("test engine id must be valid")
}

fn instance_id(value: &str) -> EngineInstanceId {
    EngineInstanceId::new(value).expect("test engine instance id must be valid")
}

fn capability_id() -> CapabilityId {
    CapabilityId::new(CAPABILITY_ID).expect("test capability id must be valid")
}

fn contract_id() -> ContractId {
    ContractId::new(CONTRACT_ID).expect("test contract id must be valid")
}

fn operation_context(name: &str) -> OperationContext {
    OperationContext::new(Operation::new(
        OperationId::new(format!("{name}.operation")).expect("operation id must be valid"),
        CorrelationId::new(format!("{name}.correlation")).expect("correlation id must be valid"),
    ))
    .for_attempt(
        NodeId::new(format!("{name}.node")).expect("node id must be valid"),
        AttemptId::new(format!("{name}.attempt")).expect("attempt id must be valid"),
    )
}

fn descriptor(interaction: Interaction) -> ContractDescriptor {
    ContractDescriptor::new(
        contract_id(),
        capability_id(),
        Version::new(1, 0, 0),
        interaction,
        PayloadDescriptor::new(MEDIA_TYPE, Version::new(1, 0, 0))
            .expect("payload descriptor must be valid"),
    )
}

fn request(name: &str, payload: &[u8]) -> UniversalRequest {
    let descriptor = descriptor(Interaction::Request);

    let participants = Participants::new(engine_id("e2e.caller"), engine_id(ENGINE_ID))
        .with_target_instance(instance_id(ENGINE_INSTANCE_ID));

    UniversalRequest::new(MessageEnvelope::new(
        MessageId::new(format!("{name}.message")).expect("message id must be valid"),
        operation_context(name),
        ContractMetadata::new(descriptor.clone(), participants),
        EncodedPayload::new(descriptor.payload, payload.to_vec()),
    ))
}

fn healthy_observation() -> EngineObservation {
    let engine = engine_id(ENGINE_ID);
    let instance = instance_id(ENGINE_INSTANCE_ID);

    let health = HealthReport::new(
        engine.clone(),
        LifecycleState::Serving,
        LivenessReport::healthy(),
        ReadinessReport::from_lifecycle(LifecycleState::Serving),
        Vec::new(),
        Vec::new(),
    )
    .expect("health report must be valid");

    EngineObservation::new(engine, instance, health)
        .expect("observation identity must match health identity")
}

fn control_plane_registration() -> EngineRegistration {
    EngineRegistration::new(engine_id(ENGINE_ID), instance_id(ENGINE_INSTANCE_ID))
}

fn prepare_engine() -> IndexingEngine {
    let engine = IndexingEngine::new(engine_id(ENGINE_ID), instance_id(ENGINE_INSTANCE_ID));

    let registry = EngineRegistry::new();

    engine
        .start()
        .expect("Indexing Engine must start through Core");

    engine
        .begin_registration()
        .expect("engine must enter Core registration state");

    engine
        .register_engine(&registry)
        .expect("engine registration must use Core registry");

    engine
        .register_phase0_capability()
        .expect("phase 0 capability must register through Core");

    engine
        .mark_ready()
        .expect("engine must become ready after registration");

    engine
        .serve()
        .expect("engine must enter Core serving state");

    assert!(registry.contains(engine.engine_instance_id()));

    engine
}

#[derive(Clone)]
struct StaticAuthenticator {
    principal: PrincipalIdentity,
}

impl Authenticator for StaticAuthenticator {
    fn authenticate(
        &self,
        _request: &AuthenticationRequest<'_>,
    ) -> Result<PrincipalIdentity, AuthenticationError> {
        Ok(self.principal.clone())
    }
}

#[derive(Clone, Copy)]
struct StaticAuthorizer {
    decision: AuthorizationDecision,
}

impl Authorizer for StaticAuthorizer {
    fn authorize(
        &self,
        _request: &AuthorizationRequest<'_>,
    ) -> Result<AuthorizationDecision, AuthorizationError> {
        Ok(self.decision)
    }
}

#[derive(Clone)]
struct StaticCredentials(Option<Vec<u8>>);

impl CredentialExtractor for StaticCredentials {
    fn extract(&self, _context: &EngineContext, _request: &UniversalRequest) -> Option<Vec<u8>> {
        self.0.clone()
    }
}

fn principal(value: &str) -> PrincipalIdentity {
    PrincipalIdentity::new(
        PrincipalType::User,
        PrincipalId::new(value).expect("principal id must be valid"),
    )
}

fn security_pipeline(decision: AuthorizationDecision) -> ExecutionPipeline {
    ExecutionPipeline::new().with_middleware(SecurityMiddleware::new(
        StaticAuthenticator {
            principal: principal("e2e-user"),
        },
        StaticAuthorizer { decision },
        StaticCredentials(Some(b"e2e-credentials".to_vec())),
    ))
}

fn select_instance() -> EngineInstanceId {
    let membership = Membership::new();
    let observations = Observations::new();

    membership
        .register(control_plane_registration())
        .expect("Control Plane membership registration must succeed");

    observations
        .update(healthy_observation())
        .expect("Control Plane observation must succeed");

    let membership_snapshot = membership.snapshot();
    let observation_snapshot = observations.snapshot();
    let expected_instance = instance_id(ENGINE_INSTANCE_ID);

    assert!(
        membership_snapshot.contains(&expected_instance),
        "registered Indexing instance must appear in the membership snapshot",
    );
    assert!(
        observation_snapshot.contains(&expected_instance),
        "registered Indexing instance must appear in the observation snapshot",
    );

    // Destination eligibility is intentionally not exercised here because the
    // current Indexing registration boundary does not advertise capability /
    // contract metadata to the Core Control Plane eligibility layer. Routing
    // policy consumes candidates derived from the authoritative membership
    // snapshot after membership and health observation have been established.
    let candidates = membership_snapshot
        .instances_for_engine(&engine_id(ENGINE_ID))
        .map(|record| RoutingCandidate::new(record.engine_instance_id().clone()))
        .collect::<Vec<_>>();

    RoutingPolicy::deterministic()
        .evaluate(&PolicyInput::new(&candidates, &RoutingConstraints::new()))
        .expect("registered Control Plane candidate must be selectable")
        .into_instance_id()
}

#[test]
fn e2e_core_control_plane_runtime_security_capability_and_response() {
    let engine = prepare_engine();

    let selected = select_instance();

    assert_eq!(selected, *engine.engine_instance_id());

    let request = request("e2e-happy", b"hello-from-e2e");

    // Exercise Core runtime admission before the security pipeline.
    //
    // The public Indexing request boundary performs its own Core admission
    // check again before capability dispatch.
    engine
        .runtime()
        .admit_request()
        .expect("serving Indexing runtime must admit the request");

    let principal = principal("e2e-user");

    let mut context = EngineContext::new(request.event.envelope.operation_context.clone());

    let mut request = request;

    let pipeline = security_pipeline(AuthorizationDecision::Allow);

    let response: Result<UniversalResponse, RequestPipelineError<_>> =
        pipeline.run_request(&mut context, &mut request, |context, request| {
            assert_eq!(
                context.security().map(SecurityContext::principal),
                Some(&principal)
            );

            engine
                .handle_request(request)
                .map_err(|error| format!("Indexing runtime admission failed: {error:?}"))
                .and_then(|result| {
                    result.map_err(|error| format!("Indexing capability failed: {error:?}"))
                })
        });

    let response = response.expect("Core security and Indexing execution must succeed");

    assert_eq!(response.status, Status::Success);

    assert_eq!(response.event.envelope.payload.bytes(), b"hello-from-e2e");

    assert_eq!(
        response.event.envelope.operation_context,
        request.event.envelope.operation_context
    );

    assert_eq!(
        response.event.envelope.metadata.descriptor.interaction,
        Interaction::Response
    );

    engine.shutdown().expect("engine shutdown must succeed");
}

#[test]
fn e2e_indexing_authorization_rejects_mismatched_target_before_dispatch() {
    let engine = prepare_engine();
    let request = request("e2e-target-mismatch", b"must-not-dispatch");

    let mut context = EngineContext::new(request.event.envelope.operation_context.clone());
    let mut request = request;
    let pipeline = security_pipeline(AuthorizationDecision::Allow);

    let downstream_called = Arc::new(AtomicBool::new(false));
    let downstream_called_by_handler = Arc::clone(&downstream_called);
    let authorization_rejected = Arc::new(AtomicBool::new(false));
    let authorization_rejected_by_boundary = Arc::clone(&authorization_rejected);

    let result: Result<UniversalResponse, RequestPipelineError<IndexingAuthorizationError>> =
        pipeline.run_request(&mut context, &mut request, |context, request| {
            let requirement = IndexingAuthorizationRequirement::new(
                engine.engine_id().clone(),
                engine.engine_instance_id().clone(),
            );
            let wrong_instance = instance_id("nizaam.indexing.e2e.wrong-instance");

            let decision = match requirement.authorize(
                &StaticAuthorizer {
                    decision: AuthorizationDecision::Allow,
                },
                context
                    .security()
                    .expect("Core security pipeline must establish SecurityContext"),
                &capability_id(),
                engine.engine_id(),
                &wrong_instance,
            ) {
                Err(IndexingAuthorizationError::InstanceMismatch) => {
                    authorization_rejected_by_boundary.store(true, Ordering::SeqCst);
                    return Err(IndexingAuthorizationError::InstanceMismatch);
                }
                Err(error) => return Err(error),
                Ok(decision) => decision,
            };

            match decision {
                AuthorizationDecision::Allow => {
                    downstream_called_by_handler.store(true, Ordering::SeqCst);

                    engine
                        .handle_request(request)
                        .map_err(|_| IndexingAuthorizationError::Core(AuthorizationError::Failed))?
                        .map_err(|_| IndexingAuthorizationError::Core(AuthorizationError::Failed))
                }
                AuthorizationDecision::Deny => {
                    Err(IndexingAuthorizationError::Core(AuthorizationError::Failed))
                }
            }
        });

    assert!(result.is_err());
    assert!(authorization_rejected.load(Ordering::SeqCst));
    assert!(!downstream_called.load(Ordering::SeqCst));

    engine.shutdown().expect("engine shutdown must succeed");
}

#[test]
fn e2e_core_security_denial_stops_indexing_execution() {
    let engine = prepare_engine();

    let request = request("e2e-denied", b"must-not-execute");

    let mut context = EngineContext::new(request.event.envelope.operation_context.clone());

    let mut request = request;

    let pipeline = security_pipeline(AuthorizationDecision::Deny);

    let downstream_called = Arc::new(AtomicBool::new(false));
    let downstream_called_by_handler = Arc::clone(&downstream_called);

    let result: Result<UniversalResponse, RequestPipelineError<()>> =
        pipeline.run_request(&mut context, &mut request, move |_context, _request| {
            downstream_called_by_handler.store(true, Ordering::SeqCst);

            Ok(UniversalResponse::new(
                MessageEnvelope::new(
                    MessageId::generate(),
                    _request.event.envelope.operation_context.clone(),
                    _request.event.envelope.metadata.clone(),
                    _request.event.envelope.payload.clone(),
                ),
                Status::Success,
            ))
        });

    assert!(matches!(
        result,
        Err(RequestPipelineError::Middleware(
            nizaam_core::middleware::chain::MiddlewareChainError::Rejected(_)
        ))
    ));

    assert!(!downstream_called.load(Ordering::SeqCst));

    assert_eq!(
        context.security().map(SecurityContext::principal),
        Some(&principal("e2e-user"))
    );

    engine.shutdown().expect("engine shutdown must succeed");
}

#[test]
fn e2e_control_plane_selection_does_not_override_runtime_admission() {
    let engine = prepare_engine();

    let selected = select_instance();

    assert_eq!(selected, *engine.engine_instance_id());

    engine.drain().expect("engine must enter draining state");

    let admission = engine.runtime().admit_request();

    assert_eq!(
        admission,
        Err(nizaam_core::runtime::RequestAdmissionError::NotServing(
            LifecycleState::Draining,
        ))
    );

    engine.shutdown().expect("draining engine must shut down");
}
