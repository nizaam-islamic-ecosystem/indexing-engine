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
//! The Indexing Engine exposes both the universal `handle_request` boundary and
//! the typed `handle_index_event` execution path. The E2E tests exercise both
//! paths and, for IndexEvent execution, inspect the real durable filesystem
//! artifacts owned by Indexing.
//!
//! This test does not invent an Indexing-owned middleware or runtime.
//!
//! IndexEvent E2E coverage also exercises the Control Plane-facing typed ingress
//! after instance selection, proving that selection is followed by delivery into
//! the existing Indexing execution path without creating a second identity layer.

mod common;

use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};

use nizaam_indexing::engine::runtime::{IndexEventHandlingError, IndexingEngine};
use nizaam_indexing::identity::IndexId;

use nizaam_core::capability::{CapabilityHandler, CapabilityOutcome, arc_handler};
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
const E2E_SOURCE_PAYLOAD: &[u8] = b"e2e-index-event-payload";

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

fn prepare_engine_with_handler(
    operation_root: PathBuf,
    extra_capability_handler: Option<Arc<dyn CapabilityHandler>>,
) -> IndexingEngine {
    let engine = IndexingEngine::new_with_operation_root(
        engine_id(ENGINE_ID),
        instance_id(ENGINE_INSTANCE_ID),
        operation_root,
    );

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

    if let Some(handler) = extra_capability_handler {
        let capability =
            common::capability_definition(&engine_id(ENGINE_ID), &common::test_capability_id());
        engine
            .register_capability(capability, handler)
            .expect("E2E IndexEvent capability must register through Core");
    }

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

fn prepare_engine(operation_root: PathBuf) -> IndexingEngine {
    prepare_engine_with_handler(operation_root, None)
}

fn prepare_index_event_engine(
    operation_root: PathBuf,
    handler: Arc<dyn CapabilityHandler>,
) -> IndexingEngine {
    prepare_engine_with_handler(operation_root, Some(handler))
}

fn unique_e2e_name(prefix: &str) -> String {
    static E2E_COUNTER: AtomicUsize = AtomicUsize::new(0);
    let sequence = E2E_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{prefix}-{}-{sequence}", std::process::id())
}

fn e2e_index_event(name: &str) -> nizaam_indexing::event::IndexEvent {
    common::index_event(
        name,
        common::engine_id(common::TEST_SOURCE_ENGINE_ID),
        Some(common::engine_instance_id(
            "nizaam.indexing.test.source.instance",
        )),
        engine_id(ENGINE_ID),
        Some(instance_id(ENGINE_INSTANCE_ID)),
        "semantic",
        common::test_index_requirement(),
        common::test_object_reference("object:e2e"),
        nizaam_indexing::index::KeyMaterial::text(format!("e2e-key-{name}")),
        E2E_SOURCE_PAYLOAD,
    )
}

fn e2e_index_id(event: &nizaam_indexing::event::IndexEvent) -> IndexId {
    let definition = common::test_index_definition();
    IndexId::generate(
        definition.namespace(),
        definition.definition_id(),
        definition.family(),
        event.key_material(),
    )
    .expect("E2E IndexId generation must succeed")
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);

    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }

    output
}

fn e2e_operation_directory(root: &std::path::Path, index_id: &IndexId) -> PathBuf {
    root.join(format!(
        "index-assignment-{}",
        hex_encode(index_id.as_bytes())
    ))
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
    let root = common::test_operation_root();
    let engine = prepare_engine(root.clone());

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
    common::remove_test_operation_root(&root);
}

#[test]
fn e2e_indexing_authorization_rejects_mismatched_target_before_dispatch() {
    let root = common::test_operation_root();
    let engine = prepare_engine(root.clone());
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
    common::remove_test_operation_root(&root);
}

#[test]
fn e2e_core_security_denial_stops_indexing_execution() {
    let root = common::test_operation_root();
    let engine = prepare_engine(root.clone());

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
    common::remove_test_operation_root(&root);
}

#[test]
fn e2e_control_plane_selection_does_not_override_runtime_admission() {
    let root = common::test_operation_root();
    let engine = prepare_engine(root.clone());

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
    common::remove_test_operation_root(&root);
}

#[test]
fn e2e_control_plane_selection_reaches_indexing_typed_ingress_without_replacing_core_identity() {
    let counter = Arc::new(AtomicUsize::new(0));
    let root = common::test_operation_root();
    let engine =
        prepare_index_event_engine(root.clone(), common::counting_echo_handler(counter.clone()));

    let selected = select_instance();
    assert_eq!(selected, *engine.engine_instance_id());

    let event = e2e_index_event(&unique_e2e_name("e2e-control-plane-index-event"));
    let request = common::control_plane_request_for_event(&event);
    let expected_message_id = request.message_id().clone();
    let expected_event_id = request.event_id().clone();
    let expected_operation = request.universal_event().envelope.operation_context.clone();

    let response = engine
        .handle_control_plane_index_event(
            &request,
            &common::test_index_definition(),
            &common::test_capacity_accounting(),
            common::test_index_event_capacity_request(),
        )
        .expect("selected Indexing instance must accept the Control Plane IndexEvent");

    assert_eq!(response.event().message_id(), &expected_message_id);
    assert_eq!(response.event().event_id(), &expected_event_id);
    assert_eq!(response.event().operation_context(), &expected_operation);
    assert_eq!(response.event().entity_type(), event.entity_type());
    assert_eq!(response.event().requirement(), event.requirement());
    assert_eq!(
        response.event().object_reference(),
        event.object_reference()
    );
    assert_eq!(response.event().key_material(), event.key_material());
    assert_eq!(response.event().source_payload(), event.source_payload());
    assert_eq!(counter.load(Ordering::SeqCst), 1);

    engine.shutdown().expect("engine shutdown must succeed");
    common::remove_test_operation_root(&root);
}

#[test]
fn e2e_default_constructor_binds_to_platform_home_operation_root() {
    let engine = IndexingEngine::new(engine_id(ENGINE_ID), instance_id(ENGINE_INSTANCE_ID))
        .expect("default IndexingEngine construction should resolve a platform home directory");

    let expected_home = if cfg!(windows) {
        std::env::var_os("USERPROFILE")
            .map(PathBuf::from)
            .or_else(|| {
                let drive = std::env::var_os("HOMEDRIVE")?;
                let path = std::env::var_os("HOMEPATH")?;
                Some(PathBuf::from(format!(
                    "{}{}",
                    drive.to_string_lossy(),
                    path.to_string_lossy()
                )))
            })
            .expect("Windows test environment must expose a platform home directory")
    } else {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .expect("Unix-like test environment must expose HOME")
    };

    assert_eq!(
        engine.operation_root(),
        expected_home.join(".nizaam").join("indexing").as_path(),
    );
}

#[test]
fn e2e_index_assignment_persists_the_complete_durable_operation_record() {
    let root = common::test_operation_root();
    let event = e2e_index_event(&unique_e2e_name("e2e-index-persistence"));
    let definition = common::test_index_definition();
    let index_id = e2e_index_id(&event);

    // The lock is intentionally observed while the real capability handler is
    // executing. `OperationLock::drop` removes the marker before
    // `handle_index_event` returns, so checking only after execution would miss
    // the fact that the lock was actually created.
    let lock_observed = Arc::new(AtomicBool::new(false));
    let lock_observed_by_handler = Arc::clone(&lock_observed);
    let event_for_handler = event.clone();
    let root_for_handler = root.clone();
    let handler = arc_handler(move |_context, invocation| {
        let operation_directory =
            e2e_operation_directory(&root_for_handler, &e2e_index_id(&event_for_handler));
        let lock_directory = operation_directory.join("locks");
        let lock_path = lock_directory.join(format!(
            "event-{}.lock",
            hex_encode(event_for_handler.event_id().as_str().as_bytes())
        ));

        assert!(
            lock_directory.is_dir(),
            "real Indexing execution must create the operation lock directory"
        );
        assert!(
            lock_path.is_file(),
            "real Indexing execution must create the per-event .lock file before capability dispatch"
        );
        lock_observed_by_handler.store(true, Ordering::SeqCst);
        Ok(CapabilityOutcome::new(invocation.payload_bytes().to_vec()))
    });

    let engine = prepare_index_event_engine(root.clone(), handler);

    let response = engine
        .handle_index_event(
            &event,
            &definition,
            &common::test_capacity_accounting(),
            common::test_index_event_capacity_request(),
        )
        .expect("real IndexEvent execution should succeed");

    let operation_directory = e2e_operation_directory(&root, &index_id);
    let lock_directory = operation_directory.join("locks");
    let event_id = hex_encode(event.event_id().as_str().as_bytes());
    let lock_path = lock_directory.join(format!("event-{event_id}.lock"));
    let event_snapshot = operation_directory
        .join("events")
        .join(format!("received-{event_id}.snapshot"));
    let response_snapshot = operation_directory
        .join("responses")
        .join(format!("response-{event_id}.snapshot"));
    let journal = root.join(format!(
        "index-assignment-{}.jsonl",
        hex_encode(index_id.as_bytes())
    ));

    assert_eq!(
        engine.operation_root(),
        root.as_path(),
        "the persistence test must use its isolated operation root",
    );
    assert!(root.is_dir(), "the isolated operation root must exist");
    assert!(operation_directory.is_dir());
    assert!(
        lock_directory.is_dir(),
        "the operation locks directory must exist"
    );
    assert!(
        lock_observed.load(Ordering::SeqCst),
        "the capability handler must have observed the real lock"
    );
    assert!(event_snapshot.is_file());
    assert!(response_snapshot.is_file());
    assert!(journal.is_file());
    assert!(
        !lock_path.exists(),
        "the ephemeral per-event lock file must be removed after execution completes"
    );

    let event_contents = common::read_test_file(&event_snapshot);
    assert!(event_contents.starts_with("kind=IndexEvent\n"));
    assert!(event_contents.contains(&format!("event_id={}\n", event.event_id().as_str())));
    assert!(event_contents.contains(&format!(
        "operation_id={}\n",
        event.operation_context().operation.id.as_str()
    )));
    let expected_source_payload = hex_encode(E2E_SOURCE_PAYLOAD);
    assert!(event_contents.contains(&format!("source_payload={expected_source_payload}\n")));

    let response_contents = common::read_test_file(&response_snapshot);
    assert!(response_contents.starts_with("kind=IndexEventResponse\n"));
    assert!(response_contents.contains(&format!("event_id={}\n", event.event_id().as_str())));
    assert!(response_contents.contains(&format!("assigned_id={}\n", response.assigned_id())));

    let journal_contents = common::read_test_file(&journal);
    assert_eq!(journal_contents.lines().count(), 2);
    assert!(journal_contents.contains("\"status\":\"started\""));
    assert!(journal_contents.contains("\"status\":\"completed\""));
    assert!(journal_contents.contains(&*event_snapshot.to_string_lossy()));
    assert!(journal_contents.contains(&*response_snapshot.to_string_lossy()));

    for path in common::test_files_recursive(&root) {
        assert_ne!(
            path.extension().and_then(|value| value.to_str()),
            Some("tmp")
        );
        assert_ne!(
            path.extension().and_then(|value| value.to_str()),
            Some("lock")
        );
    }

    engine.shutdown().expect("engine shutdown should succeed");
    common::remove_test_operation_root(&root);
}

#[test]
fn e2e_failed_index_assignment_preserves_recovery_snapshot_without_completion() {
    let root = common::test_operation_root();
    let engine = prepare_index_event_engine(
        root.clone(),
        common::failing_handler("E2E intentional capability failure"),
    );
    let event = e2e_index_event(&unique_e2e_name("e2e-index-failure"));
    let index_id = e2e_index_id(&event);
    let definition = common::test_index_definition();

    let error = engine
        .handle_index_event(
            &event,
            &definition,
            &common::test_capacity_accounting(),
            common::test_index_event_capacity_request(),
        )
        .expect_err("failing capability must propagate through the real IndexEvent path");

    assert!(matches!(error, IndexEventHandlingError::Capability(_)));

    let operation_directory = e2e_operation_directory(&root, &index_id);
    let event_id = hex_encode(event.event_id().as_str().as_bytes());
    let event_snapshot = operation_directory
        .join("events")
        .join(format!("received-{event_id}.snapshot"));
    let response_snapshot = operation_directory
        .join("responses")
        .join(format!("response-{event_id}.snapshot"));
    let journal = root.join(format!(
        "index-assignment-{}.jsonl",
        hex_encode(index_id.as_bytes())
    ));

    assert!(event_snapshot.is_file());
    assert!(journal.is_file());
    assert!(!response_snapshot.exists());

    let journal_contents = common::read_test_file(&journal);
    assert_eq!(journal_contents.lines().count(), 1);
    assert!(journal_contents.contains("\"status\":\"started\""));
    assert!(!journal_contents.contains("\"status\":\"completed\""));

    engine.shutdown().expect("engine shutdown should succeed");
    common::remove_test_operation_root(&root);
}

#[test]
fn e2e_completed_index_assignment_is_rejected_from_persisted_journal() {
    let counter = Arc::new(AtomicUsize::new(0));
    let root = common::test_operation_root();
    let engine =
        prepare_index_event_engine(root.clone(), common::counting_echo_handler(counter.clone()));
    let event = e2e_index_event(&unique_e2e_name("e2e-index-duplicate"));
    let definition = common::test_index_definition();
    let capacity = common::test_capacity_accounting();

    engine
        .handle_index_event(
            &event,
            &definition,
            &capacity,
            common::test_index_event_capacity_request(),
        )
        .expect("first IndexEvent execution should succeed");

    let index_id = e2e_index_id(&event);
    let journal = root.join(format!(
        "index-assignment-{}.jsonl",
        hex_encode(index_id.as_bytes())
    ));
    let before = common::read_test_file(&journal);

    let duplicate = engine
        .handle_index_event(
            &event,
            &definition,
            &capacity,
            common::test_index_event_capacity_request(),
        )
        .expect_err("completed operation must be rejected by the durable journal");

    assert!(matches!(
        duplicate,
        IndexEventHandlingError::OperationAlreadyCompleted { .. }
    ));
    assert_eq!(counter.load(Ordering::SeqCst), 1);
    assert_eq!(common::read_test_file(&journal), before);

    engine.shutdown().expect("engine shutdown should succeed");
    common::remove_test_operation_root(&root);
}

#[test]
fn e2e_completed_index_assignment_survives_a_fresh_engine_instance() {
    let counter = Arc::new(AtomicUsize::new(0));
    let event = e2e_index_event(&unique_e2e_name("e2e-index-restart"));
    let definition = common::test_index_definition();

    let root = common::test_operation_root();
    let first_engine =
        prepare_index_event_engine(root.clone(), common::counting_echo_handler(counter.clone()));
    first_engine
        .handle_index_event(
            &event,
            &definition,
            &common::test_capacity_accounting(),
            common::test_index_event_capacity_request(),
        )
        .expect("first engine instance should complete the operation");
    first_engine
        .shutdown()
        .expect("first engine instance should shut down cleanly");

    let second_engine =
        prepare_index_event_engine(root.clone(), common::counting_echo_handler(counter.clone()));
    assert_eq!(
        second_engine.operation_root(),
        root.as_path(),
        "a fresh engine instance must use the same explicitly configured durable operation root",
    );
    let duplicate = second_engine
        .handle_index_event(
            &event,
            &definition,
            &common::test_capacity_accounting(),
            common::test_index_event_capacity_request(),
        )
        .expect_err("a fresh engine instance must honor the persisted completion record");

    assert!(matches!(
        duplicate,
        IndexEventHandlingError::OperationAlreadyCompleted { .. }
    ));
    assert_eq!(counter.load(Ordering::SeqCst), 1);

    second_engine
        .shutdown()
        .expect("second engine instance should shut down cleanly");
    common::remove_test_operation_root(&root);
}
