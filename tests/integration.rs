//! Repository-level integration tests for the Phase 0, Phase 2, Phase 3, Phase 4,
//! Phase 5, and Phase 6 public API.
//!
//! Phase 0 coverage preserves the complete Core-backed execution path:
//!
//! ```text
//! UniversalRequest
//!       ↓
//! Indexing Engine public API
//!       ↓
//! lifecycle admission
//!       ↓
//! EngineContext
//!       ↓
//! CapabilityInvocation
//!       ↓
//! Core capability dispatch
//!       ↓
//! CapabilityOutcome
//!       ↓
//! UniversalResponse
//! ```
//!
//! Phase 2 coverage verifies the complete logical data-model composition:
//!
//! ```text
//! IndexRequirement
//!       ↓
//! validation / normalization
//!       ↓
//! IndexDefinition
//!       ↓
//! IndexEntry
//!       ↓
//! ObjectReference
//!
//! IndexDefinition
//!       ↓
//! IndexVersion
//!       ↓
//! QueryRequest
//!       ↓
//! QueryResult
//!       ↓
//! ObjectReference
//! ```
//!
//! These tests intentionally stop at logical contracts and do not perform
//! physical index construction, provider execution, storage, embedding
//! generation, or domain-object hydration.
//!
//! Phase 6 additionally verifies the real Core integration boundaries without
//! introducing a second runtime, Control Plane, security framework, or
//! observability system. Security-context propagation into the Indexing
//! capability is intentionally not asserted here until the public request
//! boundary accepts the same Core EngineContext established by middleware.

mod common;

use common::{
    contract_id, engine_id, engine_registry, operation_context, register_engine, test_engine,
    universal_request,
};
use nizaam_core::capability::CapabilityError;
use nizaam_core::contracts::{
    ContractDescriptor, ContractMetadata, EncodedPayload, Interaction, MessageEnvelope,
    Participants, PayloadDescriptor, UniversalRequest, Version,
};
use nizaam_core::identity::{CapabilityId, EngineInstanceId, MessageId};
use nizaam_core::runtime::{LifecycleState, RequestAdmissionError};
use nizaam_core::status::Status;
use nizaam_indexing::build::{
    BatchError, BatchExecutor, BatchOptions, BuildCandidate, BuildInput, BuildSnapshot,
    CandidateUpdater, IndexBuilder, IndexMutation, IndexPublisher, IndexRebuilder,
    PublicationError, RebuildInput, UpdateJournal,
};
use nizaam_indexing::identity::{
    IndexDefinitionId, IndexDefinitionIdentity, IndexId, IndexNamespace,
};
use nizaam_indexing::index::{
    ConsistencyRequirement, IndexDefinition, IndexEntry, IndexFamily, IndexVersion, IndexVersionId,
    IndexVersionState, KeyDefinition, KeyMaterial, ObjectReference, QueryHit, QueryResult,
    SchemaVersion, SimilarityEntry, SourceVersion, TargetReferenceType, Uniqueness,
    VersionLifecycle,
};
use nizaam_indexing::query::QueryRequest;
use nizaam_indexing::requirement::IndexRequirement;
use nizaam_indexing::{
    CapacityAccounting, CapacityOperation, CapacityRequest, FailureClass, IndexEvent,
    IndexEventResponse, IndexLifecycle, IndexLifecycleState, IndexingConfiguration, IndexingEngine,
    IntegrityValidationError, IntegrityValidator, RecoveryAction, RecoveryRequest,
    RequestHandlingError, action_for, classify,
};

fn request_envelope(
    engine: &IndexingEngine,
    operation: nizaam_core::operation::OperationContext,
    capability_id: CapabilityId,
    message_id: &str,
    payload: &[u8],
) -> MessageEnvelope {
    let descriptor = ContractDescriptor::new(
        contract_id("nizaam.indexing.phase0.request"),
        capability_id,
        Version::new(1, 0, 0),
        Interaction::Request,
        PayloadDescriptor::new("application/octet-stream", Version::new(1, 0, 0))
            .expect("test payload descriptor must be valid"),
    );

    let metadata = ContractMetadata::new(
        descriptor.clone(),
        Participants::new(engine_id("nizaam.test.caller"), engine.engine_id().clone())
            .with_target_instance(engine.engine_instance_id().clone()),
    );

    MessageEnvelope::new(
        MessageId::new(message_id).expect("test message id must be valid"),
        operation,
        metadata,
        EncodedPayload::new(descriptor.payload, payload.to_vec()),
    )
}

fn prepared_engine() -> (
    IndexingEngine,
    nizaam_core::control_plane::registry::EngineRegistry,
    CapabilityId,
) {
    let engine = test_engine();
    let registry = engine_registry();

    engine.start().expect("startup should succeed");
    engine
        .begin_registration()
        .expect("registration lifecycle entry should succeed");
    register_engine(&engine, &registry).expect("engine registration should succeed");
    let capability_id = engine
        .register_phase0_capability()
        .expect("Phase 0 capability registration should succeed");
    engine
        .mark_ready()
        .expect("ready transition should succeed");
    engine.serve().expect("serving transition should succeed");

    (engine, registry, capability_id)
}

fn phase2_index_id(byte: u8) -> IndexId {
    IndexId::from_bytes([byte; 64])
}

fn phase2_generated_index_id(
    namespace: &IndexNamespace,
    definition: &IndexDefinitionId,
    family: IndexFamily,
    key_material: &KeyMaterial,
) -> IndexId {
    IndexId::generate(namespace, definition, family, key_material)
        .expect("test key material must produce a valid index ID")
}

fn phase2_namespace(value: &str) -> IndexNamespace {
    IndexNamespace::new(value).expect("test namespace must be valid")
}

fn phase2_definition_id(value: &str) -> IndexDefinitionId {
    IndexDefinitionId::new(value).expect("test definition ID must be valid")
}

fn phase2_key_definition(fields: &[&str]) -> KeyDefinition {
    KeyDefinition::new(fields.iter().copied()).expect("test key definition must be valid")
}

fn phase2_target_reference_type() -> TargetReferenceType {
    TargetReferenceType::new("source.object").expect("test target reference type must be valid")
}

fn phase2_consistency() -> ConsistencyRequirement {
    ConsistencyRequirement::new("logical").expect("test consistency requirement must be valid")
}

fn phase2_source_version() -> SourceVersion {
    SourceVersion::new("source-v2").expect("test source version must be valid")
}

fn phase2_schema_version() -> SchemaVersion {
    SchemaVersion::new("schema-v3").expect("test schema version must be valid")
}

fn phase2_reference(source: &str, object: &str) -> ObjectReference {
    ObjectReference::new(source, object).expect("test object reference must be valid")
}

fn phase2_index_version(id: &str) -> IndexVersion {
    IndexVersion::new(IndexVersionId::new(id).expect("test index version ID must be valid"))
}

#[test]
fn phase0_public_api_completes_universal_request_to_response_flow() {
    let (engine, registry, capability_id) = prepared_engine();
    let operation = operation_context("integration-request-response");

    let request: UniversalRequest = universal_request(request_envelope(
        &engine,
        operation.clone(),
        capability_id,
        "integration-request-message",
        b"phase0-integration-payload",
    ));

    let request_operation = request.universal_event().envelope.operation_context.clone();

    assert!(request.has_request_interaction());
    assert!(!request.event_id().as_str().is_empty());
    assert!(registry.contains(engine.engine_instance_id()));
    assert_eq!(engine.runtime().state(), LifecycleState::Serving);

    let response = engine
        .handle_request(&request)
        .expect("Serving runtime should admit the request")
        .expect("Phase 0 probe should produce a successful response");

    assert_eq!(response.status, Status::Success);
    assert!(response.has_response_interaction());
    assert_eq!(
        response.universal_event().envelope.payload.bytes(),
        b"phase0-integration-payload"
    );
    assert_eq!(
        response.universal_event().envelope.operation_context,
        request_operation
    );
    assert_ne!(request.event_id(), response.event_id());

    engine.shutdown().expect("shutdown should succeed");
    assert_eq!(engine.runtime().state(), LifecycleState::Stopped);
}

#[test]
fn universal_request_context_survives_the_public_runtime_boundary() {
    let (engine, _registry, capability_id) = prepared_engine();
    let operation = operation_context("integration-context");

    let request = universal_request(request_envelope(
        &engine,
        operation.clone(),
        capability_id,
        "integration-context-message",
        b"context-payload",
    ));

    let response = engine
        .handle_request(&request)
        .expect("Serving runtime should admit the request")
        .expect("Phase 0 probe should produce a response");

    assert_eq!(
        request.universal_event().envelope.operation_context,
        operation
    );
    assert_eq!(
        response.universal_event().envelope.operation_context,
        operation
    );
}

#[test]
fn full_request_path_respects_ready_and_draining_admission_boundaries() {
    let engine = test_engine();
    let registry = engine_registry();

    engine.start().unwrap();
    engine.begin_registration().unwrap();
    register_engine(&engine, &registry).unwrap();
    let capability_id = engine.register_phase0_capability().unwrap();
    engine.mark_ready().unwrap();

    let request = universal_request(request_envelope(
        &engine,
        operation_context("integration-admission"),
        capability_id.clone(),
        "integration-admission-message",
        b"admission-payload",
    ));

    assert!(matches!(
        engine.handle_request(&request),
        Err(RequestHandlingError::Admission(
            RequestAdmissionError::NotServing(LifecycleState::Ready),
        ))
    ));

    engine.serve().unwrap();

    let response = engine
        .handle_request(&request)
        .expect("Serving runtime should admit the request")
        .expect("Phase 0 probe should produce a response");
    assert_eq!(response.status, Status::Success);

    engine.drain().unwrap();

    assert!(matches!(
        engine.handle_request(&request),
        Err(RequestHandlingError::Admission(
            RequestAdmissionError::NotServing(LifecycleState::Draining),
        ))
    ));
}

#[test]
fn capability_resolution_comes_from_the_request_contract_without_local_routing() {
    let (engine, _registry, _capability_id) = prepared_engine();
    let unknown_capability = CapabilityId::new("nizaam.indexing.phase0.unknown")
        .expect("unknown capability id must be valid");
    let request = universal_request(request_envelope(
        &engine,
        operation_context("integration-unknown-capability"),
        unknown_capability.clone(),
        "integration-unknown-message",
        b"opaque-payload",
    ));

    let result = engine.handle_request(&request);

    assert!(matches!(result, Ok(Err(CapabilityError::Unknown))));
    assert!(!engine.capabilities().contains(&unknown_capability));
}

#[test]
fn capability_dispatch_remains_core_backed_through_the_public_request_boundary() {
    let (engine, _registry, capability_id) = prepared_engine();
    let request = universal_request(request_envelope(
        &engine,
        operation_context("integration-core-dispatch"),
        capability_id.clone(),
        "integration-core-dispatch-message",
        b"core-dispatch-payload",
    ));

    let response = engine
        .handle_request(&request)
        .expect("Serving runtime should admit the request")
        .expect("Phase 0 probe should return a response");

    assert_eq!(response.status, Status::Success);
    assert_eq!(
        response
            .universal_event()
            .envelope
            .metadata
            .descriptor
            .capability_id,
        capability_id
    );
}

#[test]
fn independent_engine_instances_keep_their_complete_integration_state_separate() {
    let first = IndexingEngine::new(
        engine_id("nizaam.indexing.integration.shared"),
        EngineInstanceId::new("nizaam.indexing.integration.instance.1")
            .expect("test instance id must be valid"),
    );
    let second = IndexingEngine::new(
        engine_id("nizaam.indexing.integration.shared"),
        EngineInstanceId::new("nizaam.indexing.integration.instance.2")
            .expect("test instance id must be valid"),
    );

    assert_eq!(first.engine_id(), second.engine_id());
    assert_ne!(first.engine_instance_id(), second.engine_instance_id());
    assert!(first.capabilities().is_empty());
    assert!(second.capabilities().is_empty());

    first.start().unwrap();
    first.begin_registration().unwrap();
    let registry = engine_registry();
    register_engine(&first, &registry).unwrap();
    first.register_phase0_capability().unwrap();

    assert_eq!(first.capabilities().len(), 1);
    assert_eq!(second.capabilities().len(), 0);
    assert!(registry.contains(first.engine_instance_id()));
    assert!(!registry.contains(second.engine_instance_id()));
}

#[test]
fn phase2_requirement_to_definition_to_entry_to_reference_composes_end_to_end() {
    let requirement = IndexRequirement::new(
        phase2_namespace("integration.lexical"),
        IndexFamily::Inverted,
        phase2_key_definition(&["term"]),
        phase2_target_reference_type(),
        Uniqueness::NonUnique,
        phase2_consistency(),
        Some(phase2_source_version()),
        Some(phase2_schema_version()),
    )
    .expect("requirement should be valid");

    requirement
        .validate()
        .expect("valid requirement should validate");

    let definition = requirement
        .normalize(phase2_definition_id("integration.lexical.v1"))
        .expect("normalization should produce a definition");

    assert_eq!(definition.namespace().as_str(), "integration.lexical");
    assert_eq!(definition.family(), IndexFamily::Inverted);
    assert_eq!(
        definition.definition_id().as_str(),
        "integration.lexical.v1"
    );
    assert_eq!(definition.key_definition().len(), 1);
    assert_eq!(definition.source_version().unwrap().as_str(), "source-v2");
    assert_eq!(definition.schema_version().unwrap().as_str(), "schema-v3");

    let key = KeyMaterial::text("bismillah");
    let index_id = phase2_generated_index_id(
        definition.namespace(),
        definition.definition_id(),
        definition.family(),
        &key,
    );
    let repeated_index_id = phase2_generated_index_id(
        definition.namespace(),
        definition.definition_id(),
        definition.family(),
        &key,
    );
    assert_eq!(index_id, repeated_index_id);
    assert_eq!(index_id.as_bytes().len(), 64);

    let reference = phase2_reference("quran", "verse:1:1");
    let entry = IndexEntry::new(key.clone(), reference.clone()).expect("entry should be valid");

    assert_eq!(entry.key(), &key);
    assert_eq!(entry.target(), &reference);
    entry.validate().expect("valid entry should validate");

    // The complete logical path is composed from data contracts only. No
    // physical index or storage provider is required.
}

#[test]
fn phase2_definition_version_query_and_result_compose_end_to_end() {
    let identity = IndexDefinitionIdentity::new(
        phase2_definition_id("integration.similarity"),
        phase2_namespace("integration.similarity"),
        IndexFamily::Similarity,
    );

    let definition = IndexDefinition::new(
        identity,
        phase2_key_definition(&["representation"]),
        phase2_target_reference_type(),
        Uniqueness::NonUnique,
        phase2_consistency(),
        Some(phase2_source_version()),
        Some(phase2_schema_version()),
    )
    .expect("definition should be valid");

    let index_version = phase2_index_version("index-v7");
    assert_eq!(index_version.id().as_str(), "index-v7");
    assert_eq!(definition.family(), IndexFamily::Similarity);

    let query_key = KeyMaterial::Sequence(vec![KeyMaterial::Unsigned(1), KeyMaterial::Unsigned(2)]);
    let index_id = phase2_generated_index_id(
        definition.namespace(),
        definition.definition_id(),
        definition.family(),
        &query_key,
    );
    let request = QueryRequest::with_options(
        index_id,
        query_key,
        std::num::NonZeroUsize::new(5),
        Some(KeyMaterial::text("logical-query")),
    )
    .expect("query request should be valid");

    assert_eq!(request.limit(), std::num::NonZeroUsize::new(5));
    request
        .validate()
        .expect("valid query request should validate");

    let reference = phase2_reference("similarity-source", "object-17");
    let hit = QueryHit::new(reference.clone())
        .with_metrics(Some(0.88), Some(0.12))
        .expect("query hit should be valid");

    let result = QueryResult::with_continuation(
        index_id,
        index_version.clone(),
        vec![hit],
        Some(vec![0x01, 0x02]),
    )
    .expect("query result should be valid");

    assert_eq!(result.index_id(), request.index_id());
    assert_eq!(result.index_version(), &index_version);
    assert_eq!(result.hits().len(), 1);
    assert_eq!(result.hits()[0].reference(), &reference);
    assert_eq!(result.hits()[0].score(), Some(0.88));
    assert_eq!(result.hits()[0].distance(), Some(0.12));
    assert_eq!(result.continuation(), Some(&[0x01, 0x02][..]));

    // Result composition ends at an ObjectReference. No domain object is
    // loaded or hydrated, and the request itself is not an executor.
}

#[test]
fn phase2_similarity_entry_remains_generic_and_reference_oriented() {
    let representation = KeyMaterial::map([
        ("feature_a", KeyMaterial::Unsigned(10)),
        ("feature_b", KeyMaterial::Unsigned(20)),
    ])
    .expect("representation must be valid");
    let reference = phase2_reference("source-a", "object-3");

    let entry = SimilarityEntry::with_metadata(
        representation.clone(),
        reference.clone(),
        Some(KeyMaterial::text("metadata")),
    )
    .expect("similarity entry should be valid");

    assert_eq!(entry.representation(), &representation);
    assert_eq!(entry.target(), &reference);
    assert_eq!(entry.metadata(), Some(&KeyMaterial::text("metadata")));
    entry
        .validate()
        .expect("valid similarity entry should validate");

    // No embedding generator/model/provider is exercised by the logical
    // contract integration test.
}

#[test]
fn phase2_relationship_and_similarity_families_do_not_share_domain_semantics() {
    let relationship_identity = IndexDefinitionIdentity::new(
        phase2_definition_id("integration.relationship"),
        phase2_namespace("integration.relationship"),
        IndexFamily::Relationship,
    );
    let similarity_identity = IndexDefinitionIdentity::new(
        phase2_definition_id("integration.similarity-family"),
        phase2_namespace("integration.similarity"),
        IndexFamily::Similarity,
    );

    let relationship = IndexDefinition::new(
        relationship_identity,
        phase2_key_definition(&["field_a", "field_b"]),
        phase2_target_reference_type(),
        Uniqueness::NonUnique,
        phase2_consistency(),
        None,
        None,
    )
    .expect("relationship definition should be valid");

    let similarity = IndexDefinition::new(
        similarity_identity,
        phase2_key_definition(&["representation"]),
        phase2_target_reference_type(),
        Uniqueness::NonUnique,
        phase2_consistency(),
        None,
        None,
    )
    .expect("similarity definition should be valid");

    assert_eq!(relationship.family(), IndexFamily::Relationship);
    assert_eq!(similarity.family(), IndexFamily::Similarity);
    assert_ne!(relationship.family(), similarity.family());

    // Neither family imports a semantic predicate model, graph storage model,
    // embedding model, or physical algorithm into the Phase 2 contract.
}

#[test]
fn phase2_index_identity_and_definition_identity_remain_distinct_in_integration() {
    let index_id = phase2_index_id(0x77);
    let definition_identity = IndexDefinitionIdentity::new(
        phase2_definition_id("integration.identity"),
        phase2_namespace("integration.identity"),
        IndexFamily::Identity,
    );

    let definition = IndexDefinition::new(
        definition_identity.clone(),
        phase2_key_definition(&["object"]),
        phase2_target_reference_type(),
        Uniqueness::Unique,
        phase2_consistency(),
        None,
        None,
    )
    .expect("definition should be valid");

    assert_eq!(definition.identity(), &definition_identity);
    assert_eq!(index_id.as_bytes(), &[0x77; 64]);
    assert_eq!(definition.definition_id().as_str(), "integration.identity");

    // Concrete index identity and logical definition identity coexist as
    // separate Phase 1/Phase 2 concepts.
}
fn phase3_definition() -> IndexDefinition {
    IndexDefinition::new(
        IndexDefinitionIdentity::new(
            phase2_definition_id("integration.phase3.definition"),
            phase2_namespace("integration.phase3"),
            IndexFamily::Inverted,
        ),
        phase2_key_definition(&["term"]),
        phase2_target_reference_type(),
        Uniqueness::NonUnique,
        phase2_consistency(),
        Some(phase2_source_version()),
        Some(phase2_schema_version()),
    )
    .expect("phase3 test definition should be valid")
}

fn phase3_entry(term: &str, object: &str) -> IndexEntry {
    IndexEntry::new(
        KeyMaterial::text(term),
        phase2_reference("documents", object),
    )
    .expect("phase3 test entry should be valid")
}

fn ready_state(candidate: &BuildCandidate) -> IndexVersionState {
    let mut state = IndexVersionState::new(candidate.version().clone());
    state
        .transition_to(VersionLifecycle::Validating)
        .expect("candidate state should enter validation");
    state
        .mark_ready()
        .expect("candidate state should become ready");
    state
}

fn phase3_candidate(index_seed: u8, version: &str, entries: Vec<IndexEntry>) -> BuildCandidate {
    let definition = phase3_definition();

    IndexBuilder::new()
        .build(BuildInput::new(
            phase2_index_id(index_seed),
            definition,
            IndexVersionId::new(version).expect("phase3 version ID should be valid"),
            BuildSnapshot::with_versions(
                Some(phase2_source_version()),
                Some(phase2_schema_version()),
                entries,
            ),
        ))
        .expect("phase3 candidate should build")
}

#[test]
fn phase3_build_update_and_explicit_publication_keep_active_state_separate() {
    let index_id = phase2_index_id(0x90);
    let base = phase3_candidate(0x90, "index-v1", vec![phase3_entry("alpha", "doc:1")]);

    let updater = CandidateUpdater::new();
    let mut journal = UpdateJournal::new();

    let update_candidate = updater
        .create_candidate(
            &base,
            IndexVersionId::new("index-v2").expect("candidate version ID should be valid"),
            None,
            None,
        )
        .expect("update candidate should be created");

    let (updated_candidate, sequence) = updater
        .apply_and_record(
            &update_candidate,
            IndexMutation::Insert(phase3_entry("beta", "doc:2")),
            &mut journal,
        )
        .expect("logical update should succeed");

    assert_eq!(sequence.value(), 1);
    assert_eq!(journal.current_sequence().value(), 1);
    assert_eq!(journal.len(), 1);

    // The previously built candidate remains unchanged and no publication
    // occurs merely because an update was accepted.
    assert_eq!(base.len(), 1);
    assert_eq!(update_candidate.len(), 1);
    assert_eq!(updated_candidate.len(), 2);

    let publisher = IndexPublisher::new();
    let prepared = publisher
        .prepare(
            updated_candidate.clone(),
            ready_state(&updated_candidate),
            &phase3_definition(),
            Some(base.version().id().clone()),
            Some(base.version().id().clone()),
        )
        .expect("updated candidate should be publication-eligible");

    let published = publisher
        .publish(prepared, Some(base.clone()), None)
        .expect("explicit publication should succeed");

    assert_eq!(published.active().index_id(), &index_id);
    assert_eq!(published.active().version().id().as_str(), "index-v2");
    assert_eq!(
        published
            .previous_active()
            .expect("previous active should be preserved")
            .version()
            .id()
            .as_str(),
        "index-v1"
    );
}

#[test]
fn phase3_bounded_batch_commits_complete_chunks_without_partial_success() {
    let base = phase3_candidate(0x91, "index-v1", vec![phase3_entry("base", "doc:0")]);
    let journal = UpdateJournal::new();

    let mutations = vec![
        IndexMutation::Insert(phase3_entry("alpha", "doc:1")),
        IndexMutation::Insert(phase3_entry("beta", "doc:2")),
        IndexMutation::Insert(phase3_entry("gamma", "doc:3")),
    ];

    let result = BatchExecutor::new()
        .execute(
            &base,
            &journal,
            mutations,
            BatchOptions::new(2).expect("chunk size should be valid"),
        )
        .expect("all bounded chunks should succeed");

    assert_eq!(result.completed_chunks(), 2);
    assert_eq!(result.applied_mutations(), 3);
    assert_eq!(result.candidate().len(), 4);
    assert_eq!(result.journal().len(), 3);
    assert_eq!(result.journal().current_sequence().value(), 3);

    // Batch execution only produces another candidate; publication remains
    // outside this boundary.
    assert_eq!(base.len(), 1);
}

#[test]
fn phase3_failed_batch_chunk_preserves_the_previously_committed_prefix() {
    let base = phase3_candidate(0x92, "index-v1", vec![phase3_entry("base", "doc:0")]);
    let journal = UpdateJournal::new();
    let duplicated = phase3_entry("duplicate", "doc:1");

    let result = BatchExecutor::new().execute(
        &base,
        &journal,
        vec![
            IndexMutation::Insert(phase3_entry("alpha", "doc:1")),
            IndexMutation::Insert(phase3_entry("beta", "doc:2")),
            IndexMutation::Insert(duplicated.clone()),
            IndexMutation::Insert(duplicated),
        ],
        BatchOptions::new(2).expect("chunk size should be valid"),
    );

    let error = result.expect_err("the second chunk should fail transactionally");

    match error {
        BatchError::ChunkFailed {
            chunk_index,
            committed,
            ..
        } => {
            assert_eq!(chunk_index, 1);
            assert_eq!(committed.completed_chunks(), 1);
            assert_eq!(committed.applied_mutations(), 2);
            assert_eq!(committed.candidate().len(), 3);
            assert_eq!(committed.journal().len(), 2);
        }
        other => panic!("unexpected batch error: {other:?}"),
    }

    assert_eq!(base.len(), 1);
}

#[test]
fn phase3_rebuild_replays_post_snapshot_updates_before_publication() {
    let active = phase3_candidate(0x93, "index-v1", vec![phase3_entry("base", "doc:0")]);
    let index_id = *active.index_id();
    let active_version = active.version().id().clone();

    let mut journal = UpdateJournal::new();
    let rebuilder = IndexRebuilder::new();

    let snapshot = BuildSnapshot::with_versions(
        Some(phase2_source_version()),
        Some(phase2_schema_version()),
        vec![phase3_entry("base", "doc:0")],
    );

    let input = RebuildInput::new(
        index_id,
        phase3_definition(),
        IndexVersionId::new("index-v2").expect("candidate version ID should be valid"),
        snapshot,
        Some(active_version.clone()),
        Some(active_version.clone()),
    );

    let progress = rebuilder
        .start(input, &journal)
        .expect("rebuild should start from the source snapshot");

    assert_eq!(progress.captured_sequence().value(), 0);
    assert_eq!(progress.replayed_updates(), 0);
    assert_eq!(progress.candidate().len(), 1);

    journal
        .append(IndexMutation::Insert(phase3_entry("alpha", "doc:1")))
        .expect("journal append should succeed");
    journal
        .append(IndexMutation::Insert(phase3_entry("beta", "doc:2")))
        .expect("journal append should succeed");
    journal
        .append(IndexMutation::Insert(phase3_entry("gamma", "doc:3")))
        .expect("journal append should succeed");

    let progress = rebuilder
        .replay(&progress, &journal)
        .expect("all post-snapshot updates should replay");

    assert_eq!(progress.captured_sequence().value(), 0);
    assert_eq!(progress.replayed_through().value(), 3);
    assert_eq!(progress.replayed_updates(), 3);
    assert!(rebuilder.is_caught_up(&progress, &journal));

    let rebuilt = rebuilder
        .finish(&progress, &journal)
        .expect("caught-up rebuild should finish");
    assert_eq!(rebuilt.candidate().len(), 4);
    assert_eq!(rebuilt.replayed_updates(), 3);

    let publisher = IndexPublisher::new();
    let rebuilt_candidate = rebuilt.candidate().clone();
    let prepared = publisher
        .prepare_rebuild(
            rebuilt,
            ready_state(&rebuilt_candidate),
            &phase3_definition(),
            Some(active_version.clone()),
        )
        .expect("rebuilt candidate should be publication-eligible");

    let published = publisher
        .publish(prepared, Some(active.clone()), Some(&journal))
        .expect("explicit publication should succeed");

    assert_eq!(published.active().version().id().as_str(), "index-v2");
    assert_eq!(
        published
            .previous_active()
            .expect("previous active should be preserved")
            .version()
            .id()
            .as_str(),
        "index-v1"
    );
}

#[test]
fn phase3_stale_prepared_candidate_cannot_overwrite_a_newer_active_version() {
    let active_v1 = phase3_candidate(0x94, "index-v1", vec![phase3_entry("base", "doc:0")]);
    let candidate_v2 = phase3_candidate(0x94, "index-v2", vec![phase3_entry("v2", "doc:2")]);
    let candidate_v3 = phase3_candidate(0x94, "index-v3", vec![phase3_entry("v3", "doc:3")]);

    let publisher = IndexPublisher::new();
    let definition = phase3_definition();

    let prepared_v2 = publisher
        .prepare(
            candidate_v2.clone(),
            ready_state(&candidate_v2),
            &definition,
            Some(IndexVersionId::new("index-v1").expect("version ID should be valid")),
            Some(IndexVersionId::new("index-v1").expect("version ID should be valid")),
        )
        .expect("v2 should be prepared against v1");

    let prepared_v3 = publisher
        .prepare(
            candidate_v3.clone(),
            ready_state(&candidate_v3),
            &definition,
            Some(IndexVersionId::new("index-v1").expect("version ID should be valid")),
            Some(IndexVersionId::new("index-v1").expect("version ID should be valid")),
        )
        .expect("v3 should be prepared against v1");

    let published_v3 = publisher
        .publish(prepared_v3, Some(active_v1.clone()), None)
        .expect("v3 should publish against the original active version");

    let error = publisher
        .publish(prepared_v2, Some(published_v3.active().clone()), None)
        .expect_err("stale prepared v2 must not overwrite v3");

    assert!(matches!(
        error,
        PublicationError::ActiveVersionChanged { .. }
    ));
    assert_eq!(published_v3.active().version().id().as_str(), "index-v3");
}

// -----------------------------------------------------------------------------
// Phase 4 query integration coverage
// -----------------------------------------------------------------------------

use std::fmt::{Display, Formatter};
use std::sync::{Arc, Mutex};

use nizaam_indexing::consistency::{ConsistencyMode, SynchronizationSnapshot};
use nizaam_indexing::provider::{
    ProviderAvailability, ProviderCapabilities, ProviderCapability, RankingCandidate,
};
use nizaam_indexing::query::{
    CapabilityResolution, ExactRetrievalPlan, FilteredRetrievalPlan, HybridRetrievalPlan,
    IndexCandidate, NeighborhoodRetrievalPlan, ProviderRetriever, ResultMode, RetrievalPlan,
    SimilarityRetrievalPlan, StructuredRetrievalPlan, TextRetrievalPlan, execute, plan_query,
};

fn phase4_definition(name: &str, family: IndexFamily) -> IndexDefinition {
    IndexDefinition::new(
        IndexDefinitionIdentity::new(
            phase2_definition_id(name),
            phase2_namespace("integration.phase4"),
            family,
        ),
        phase2_key_definition(&["value"]),
        phase2_target_reference_type(),
        Uniqueness::NonUnique,
        phase2_consistency(),
        Some(phase2_source_version()),
        Some(phase2_schema_version()),
    )
    .expect("phase4 definition should be valid")
}

fn phase4_candidate(
    seed: u8,
    definition_name: &str,
    version: &str,
    source_sequence: u64,
    indexed_sequence: u64,
    lifecycle: VersionLifecycle,
) -> IndexCandidate {
    let definition = phase4_definition(definition_name, IndexFamily::Identity);
    let index_id = phase2_generated_index_id(
        definition.namespace(),
        definition.definition_id(),
        definition.family(),
        &KeyMaterial::Unsigned(u128::from(seed)),
    );
    let index_version = IndexVersion::with_metadata(
        IndexVersionId::new(version).expect("phase4 version should be valid"),
        Some(phase2_source_version()),
        Some(phase2_schema_version()),
        None,
    )
    .expect("phase4 index version should be valid");
    let mut state = IndexVersionState::new(index_version);
    if lifecycle != VersionLifecycle::Building {
        state
            .transition_to(VersionLifecycle::Validating)
            .expect("version should enter validation");
        state.mark_ready().expect("version should become ready");
    }
    if lifecycle == VersionLifecycle::Published {
        state
            .mark_published()
            .expect("version should become published");
    }

    let synchronization = SynchronizationSnapshot::from_sequences(
        nizaam_indexing::build::UpdateSequence::new(source_sequence),
        nizaam_indexing::build::UpdateSequence::new(indexed_sequence),
    )
    .expect("phase4 synchronization should be valid");

    IndexCandidate::new(index_id, definition, state, synchronization)
}

#[derive(Clone, Debug)]
struct IntegrationProvider {
    capabilities: ProviderCapabilities,
    calls: Arc<Mutex<Vec<String>>>,
    candidates: Vec<RankingCandidate>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct IntegrationProviderError(&'static str);

impl Display for IntegrationProviderError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.0)
    }
}

impl std::error::Error for IntegrationProviderError {}

impl IntegrationProvider {
    fn new(capabilities: ProviderCapabilities, candidates: Vec<RankingCandidate>) -> Self {
        Self {
            capabilities,
            calls: Arc::new(Mutex::new(Vec::new())),
            candidates,
        }
    }

    fn candidate(source: &str, object: &str) -> RankingCandidate {
        RankingCandidate::new(
            ObjectReference::new(source, object).expect("test reference should be valid"),
        )
    }

    fn calls(&self) -> Vec<String> {
        self.calls
            .lock()
            .expect("provider call log should not be poisoned")
            .clone()
    }

    fn record(&self, name: &str) {
        self.calls
            .lock()
            .expect("provider call log should not be poisoned")
            .push(name.to_owned());
    }

    fn clone_candidates(&self) -> Vec<RankingCandidate> {
        self.candidates.clone()
    }
}

impl ProviderRetriever for IntegrationProvider {
    type Error = IntegrationProviderError;

    fn capabilities(&self) -> &ProviderCapabilities {
        &self.capabilities
    }

    fn availability(&self) -> ProviderAvailability {
        ProviderAvailability::Available
    }

    fn retrieve_exact(
        &self,
        _plan: &ExactRetrievalPlan,
        _context: &nizaam_core::operation::OperationContext,
    ) -> Result<Vec<RankingCandidate>, Self::Error> {
        self.record("exact");
        Ok(self.clone_candidates())
    }

    fn retrieve_text(
        &self,
        _plan: &TextRetrievalPlan,
        _context: &nizaam_core::operation::OperationContext,
    ) -> Result<Vec<RankingCandidate>, Self::Error> {
        self.record("text");
        Ok(self.clone_candidates())
    }

    fn retrieve_structured(
        &self,
        _plan: &StructuredRetrievalPlan,
        _context: &nizaam_core::operation::OperationContext,
    ) -> Result<Vec<RankingCandidate>, Self::Error> {
        self.record("structured");
        Ok(self.clone_candidates())
    }

    fn retrieve_neighborhood(
        &self,
        _plan: &NeighborhoodRetrievalPlan,
        _context: &nizaam_core::operation::OperationContext,
    ) -> Result<Vec<RankingCandidate>, Self::Error> {
        self.record("neighborhood");
        Ok(self.clone_candidates())
    }

    fn retrieve_similarity(
        &self,
        _plan: &SimilarityRetrievalPlan,
        _context: &nizaam_core::operation::OperationContext,
    ) -> Result<Vec<RankingCandidate>, Self::Error> {
        self.record("similarity");
        Ok(self.clone_candidates())
    }

    fn retrieve_filtered(
        &self,
        _plan: &FilteredRetrievalPlan,
        _context: &nizaam_core::operation::OperationContext,
    ) -> Result<Vec<RankingCandidate>, Self::Error> {
        self.record("filtered");
        Ok(self.clone_candidates())
    }

    fn retrieve_hybrid(
        &self,
        _plan: &HybridRetrievalPlan,
        _context: &nizaam_core::operation::OperationContext,
    ) -> Result<Vec<RankingCandidate>, Self::Error> {
        self.record("hybrid");
        Ok(self.clone_candidates())
    }

    fn retrieve_hybrid_component(
        &self,
        _component: &nizaam_indexing::query::PlannedHybridComponent,
        _context: &nizaam_core::operation::OperationContext,
    ) -> Result<Vec<RankingCandidate>, Self::Error> {
        self.record("hybrid-component");
        Ok(self.clone_candidates())
    }
}

#[test]
fn phase4_request_consistency_planner_provider_and_reference_result_compose_end_to_end() {
    let candidate = phase4_candidate(
        0xa1,
        "integration.phase4.exact",
        "published-v1",
        12,
        12,
        VersionLifecycle::Published,
    );
    let request = QueryRequest::with_spec(
        *candidate.index_id(),
        None,
        None,
        None,
        nizaam_indexing::query::QueryKind::Exact {
            key: KeyMaterial::text("mercy"),
        },
        std::num::NonZeroUsize::new(8),
        ConsistencyMode::Current,
        ResultMode::ReferencesOnly,
        None,
    )
    .expect("phase4 request should be valid");

    let provider = IntegrationProvider::new(
        ProviderCapabilities::with(ProviderCapability::ExactLookup),
        vec![IntegrationProvider::candidate("quran", "verse:1:1")],
    );

    let plan = plan_query(
        &request,
        vec![candidate.clone()],
        provider.capabilities(),
        provider.availability(),
    )
    .expect("current exact request should produce a logical plan");

    assert_eq!(
        plan.target()
            .expect("single-index plan should have target")
            .index_id(),
        candidate.index_id()
    );
    assert_eq!(
        plan.capability_resolution(),
        &CapabilityResolution::Direct(ProviderCapability::ExactLookup)
    );

    let operation = operation_context("phase4-integration-query");
    let result = execute(&plan, &provider, &operation).expect("provider execution should succeed");

    assert_eq!(result.index_id(), candidate.index_id());
    assert_eq!(result.index_version().id().as_str(), "published-v1");
    assert_eq!(result.hits().len(), 1);
    assert_eq!(result.hits()[0].reference().source(), "quran");
    assert_eq!(result.hits()[0].reference().object_reference(), "verse:1:1");
    assert_eq!(
        result
            .consistency()
            .expect("consistency metadata should exist")
            .update_sequence_lag(),
        Some(0)
    );
    assert_eq!(provider.calls(), vec!["exact"]);
}

#[test]
fn phase4_current_query_selects_the_fresh_published_version_and_ignores_unpublished_candidates() {
    let old = phase4_candidate(
        0xa2,
        "integration.phase4.current",
        "published-v1",
        20,
        18,
        VersionLifecycle::Published,
    );
    let current = phase4_candidate(
        0xa2,
        "integration.phase4.current",
        "published-v2",
        20,
        20,
        VersionLifecycle::Published,
    );
    let rebuilding = phase4_candidate(
        0xa2,
        "integration.phase4.current",
        "candidate-v3",
        20,
        20,
        VersionLifecycle::Ready,
    );

    let request = QueryRequest::exact(*old.index_id(), KeyMaterial::text("term"))
        .expect("current exact request should be valid");
    let provider_capabilities = ProviderCapabilities::with(ProviderCapability::ExactLookup);

    let plan = plan_query(
        &request,
        vec![old, rebuilding, current.clone()],
        &provider_capabilities,
        ProviderAvailability::Available,
    )
    .expect("published fresh version should satisfy Current");

    assert_eq!(
        plan.target()
            .expect("single-index plan should have target")
            .version()
            .id()
            .as_str(),
        "published-v2"
    );
    assert_eq!(
        plan.target().expect("target should exist").lifecycle(),
        VersionLifecycle::Published
    );
}

#[test]
fn phase4_version_pinned_query_does_not_fallback_to_another_published_version() {
    let v1 = phase4_candidate(
        0xa3,
        "integration.phase4.pinned",
        "published-v1",
        30,
        30,
        VersionLifecycle::Published,
    );
    let v2 = phase4_candidate(
        0xa3,
        "integration.phase4.pinned",
        "published-v2",
        30,
        30,
        VersionLifecycle::Published,
    );
    let request = QueryRequest::with_spec(
        *v1.index_id(),
        None,
        None,
        None,
        nizaam_indexing::query::QueryKind::Exact {
            key: KeyMaterial::text("term"),
        },
        None,
        ConsistencyMode::VersionPinned(
            IndexVersionId::new("published-v1").expect("version ID should be valid"),
        ),
        ResultMode::ReferencesOnly,
        None,
    )
    .expect("pinned request should be valid");

    let plan = plan_query(
        &request,
        vec![v2, v1.clone()],
        &ProviderCapabilities::with(ProviderCapability::ExactLookup),
        ProviderAvailability::Available,
    )
    .expect("exact requested published version should be selected");

    assert_eq!(
        plan.target()
            .expect("single-index plan should have target")
            .version()
            .id()
            .as_str(),
        "published-v1"
    );
}

#[test]
fn phase4_stale_allowed_query_accepts_bounded_lag_and_records_stale_result_metadata() {
    let stale = phase4_candidate(
        0xa4,
        "integration.phase4.stale",
        "published-v1",
        40,
        37,
        VersionLifecycle::Published,
    );
    let request = QueryRequest::with_spec(
        *stale.index_id(),
        None,
        None,
        None,
        nizaam_indexing::query::QueryKind::Text {
            query: KeyMaterial::text("knowledge"),
            parameters: None,
        },
        None,
        ConsistencyMode::StaleAllowed(nizaam_indexing::consistency::FreshnessPolicy::new(3)),
        ResultMode::ReferencesOnly,
        None,
    )
    .expect("stale-allowed request should be valid");

    let provider = IntegrationProvider::new(
        ProviderCapabilities::with(ProviderCapability::TextLookup),
        vec![IntegrationProvider::candidate("hadith", "book:1")],
    );
    let plan = plan_query(
        &request,
        vec![stale.clone()],
        provider.capabilities(),
        provider.availability(),
    )
    .expect("bounded stale lag should be accepted");

    let result = execute(&plan, &provider, &operation_context("phase4-stale-query"))
        .expect("stale retrieval should succeed");

    assert_eq!(
        result
            .consistency()
            .expect("consistency metadata should exist")
            .state(),
        nizaam_indexing::query::ConsistencyState::StaleAccepted
    );
    assert_eq!(
        result
            .consistency()
            .expect("consistency metadata should exist")
            .update_sequence_lag(),
        Some(3)
    );
}

#[test]
fn phase4_hybrid_query_composes_into_a_reference_result_without_domain_hydration() {
    let candidate = phase4_candidate(
        0xa5,
        "integration.phase4.hybrid",
        "published-v1",
        50,
        50,
        VersionLifecycle::Published,
    );
    let request = QueryRequest::hybrid(
        *candidate.index_id(),
        vec![
            nizaam_indexing::query::HybridQueryComponent::new(
                nizaam_indexing::query::AtomicQuery::Text {
                    query: KeyMaterial::text("rust"),
                    parameters: None,
                },
            )
            .expect("text component should be valid"),
            nizaam_indexing::query::HybridQueryComponent::new(
                nizaam_indexing::query::AtomicQuery::Exact {
                    key: KeyMaterial::text("book"),
                },
            )
            .expect("exact component should be valid"),
        ],
    )
    .expect("hybrid request should be valid");

    let provider = IntegrationProvider::new(
        ProviderCapabilities::with(ProviderCapability::HybridRetrieval),
        vec![IntegrationProvider::candidate("source", "object-42")],
    );

    let plan = plan_query(
        &request,
        vec![candidate.clone()],
        provider.capabilities(),
        provider.availability(),
    )
    .expect("native hybrid should plan");

    assert!(matches!(plan, RetrievalPlan::Hybrid(_)));
    let result = execute(&plan, &provider, &operation_context("phase4-hybrid-query"))
        .expect("hybrid retrieval should succeed");

    assert_eq!(result.hits().len(), 1);
    assert_eq!(result.hits()[0].reference().object_reference(), "object-42");
    assert_eq!(provider.calls(), vec!["hybrid"]);
}

// -----------------------------------------------------------------------------
// Phase 5 operational integration coverage
// -----------------------------------------------------------------------------

#[test]
fn phase5_build_validate_capacity_and_publication_compose_without_merging_ownership() {
    let candidate = phase3_candidate(
        0xb1,
        "integration.phase5.v1",
        vec![
            phase3_entry("alpha", "doc:1"),
            phase3_entry("beta", "doc:2"),
        ],
    );

    IntegrityValidator::new()
        .validate_index(
            candidate.definition(),
            candidate.version(),
            candidate.entries().iter(),
        )
        .expect("candidate should pass logical integrity validation");

    let configuration =
        IndexingConfiguration::new(2, 1, 1, 1, 2, 4, 8, 4).expect("configuration should be valid");
    let accounting = CapacityAccounting::from_configuration(configuration);
    let lease = accounting
        .try_acquire(CapacityRequest::new(CapacityOperation::Build, 2))
        .expect("build should pass local capacity admission");

    assert_eq!(accounting.usage().active_builds(), 1);
    drop(lease);
    assert_eq!(accounting.usage().active_builds(), 0);

    let publisher = IndexPublisher::new();
    let prepared = publisher
        .prepare(
            candidate.clone(),
            ready_state(&candidate),
            candidate.definition(),
            None,
            None,
        )
        .expect("validated candidate should prepare for publication");

    let publication = publisher
        .publish(prepared, None, None)
        .expect("prepared candidate should publish");

    assert_eq!(
        publication.active().version().id().as_str(),
        "integration.phase5.v1"
    );

    let mut lifecycle = IndexLifecycle::new(*candidate.index_id());
    lifecycle
        .mark_building()
        .expect("Creating -> Building should be valid");
    lifecycle
        .mark_validating()
        .expect("Building -> Validating should be valid");
    lifecycle
        .mark_ready()
        .expect("Validating -> Ready should be valid");
    lifecycle
        .mark_active()
        .expect("Ready -> Active should be valid");
    assert_eq!(lifecycle.state(), IndexLifecycleState::Active);
}

#[test]
fn phase5_integrity_failure_blocks_an_incompatible_candidate_before_publication() {
    let definition = phase3_definition();
    let incompatible = IndexVersion::with_metadata(
        IndexVersionId::new("integration.phase5.incompatible").expect("version ID should be valid"),
        Some(SourceVersion::new("source-v9").expect("source version should be valid")),
        Some(SchemaVersion::new("schema-v3").expect("schema version should be valid")),
        None,
    )
    .expect("version should be structurally valid");

    let error = IntegrityValidator::new()
        .validate_version_compatibility(&incompatible, &definition)
        .expect_err("source-version mismatch must stop the candidate");

    assert!(matches!(
        error,
        IntegrityValidationError::VersionCompatibility(_)
    ));
}

#[test]
fn phase5_recovery_keeps_the_known_good_active_version_observationally_protected() {
    let active = IndexVersionId::new("integration.phase5.active-v7")
        .expect("active version ID should be valid");
    let request = RecoveryRequest::new(classify(FailureClass::CorruptIndex))
        .with_active_version(active.clone());

    assert_eq!(request.action(), RecoveryAction::Rebuild);
    assert_eq!(
        action_for(FailureClass::CorruptIndex),
        RecoveryAction::Rebuild
    );
    assert_eq!(request.active_version(), Some(&active));

    // Recovery receives the active version only as lineage/protection context.
    // No mutation API is exposed by RecoveryRequest.
    assert_eq!(active.as_str(), "integration.phase5.active-v7");
}

// -----------------------------------------------------------------------------
// Phase 6 Core integration coverage
// -----------------------------------------------------------------------------

#[test]
fn phase6_control_plane_selection_integrates_with_registered_indexing_identity_without_executing_work()
 {
    use nizaam_core::control_plane::{
        EngineObservation, EngineRegistration, Membership, Observations, PolicyInput,
        RoutingCandidate, RoutingConstraints, RoutingPolicy,
    };
    use nizaam_core::health::{HealthReport, LivenessReport, ReadinessReport};

    let (engine, _registry, _capability_id) = prepared_engine();
    let membership = Membership::new();
    let observations = Observations::new();

    membership
        .register(EngineRegistration::new(
            engine.engine_id().clone(),
            engine.engine_instance_id().clone(),
        ))
        .expect("Core membership registration should succeed");

    let health = HealthReport::new(
        engine.engine_id().clone(),
        LifecycleState::Serving,
        LivenessReport::healthy(),
        ReadinessReport::from_lifecycle(LifecycleState::Serving),
        Vec::new(),
        Vec::new(),
    )
    .expect("serving health report should be valid");

    observations
        .update(
            EngineObservation::new(
                engine.engine_id().clone(),
                engine.engine_instance_id().clone(),
                health,
            )
            .expect("engine observation should be valid"),
        )
        .expect("Core observation update should succeed");

    // Core routing policy operates only on destinations that have already
    // passed the separate Control Plane eligibility boundary. The Indexing
    // registration boundary does not publish a capability/contract
    // advertisement for `eligible_destinations`, so this test must not
    // fabricate one merely to make eligibility succeed.
    //
    // Membership and observation registration above still establish the
    // concrete Indexing identity in the Control Plane's routing view. The
    // routing-policy layer then consumes that already-eligible identity.
    let candidates = vec![RoutingCandidate::new(engine.engine_instance_id().clone())];

    let selection = RoutingPolicy::deterministic()
        .evaluate(&PolicyInput::new(&candidates, &RoutingConstraints::new()))
        .expect("registered Indexing instance should be selectable");

    assert_eq!(
        selection.into_instance_id(),
        engine.engine_instance_id().clone()
    );

    // Selection is not execution. No request has been dispatched and the
    // Core-backed Indexing state remains exactly as it was before routing.
    assert_eq!(engine.runtime().state(), LifecycleState::Serving);
    assert_eq!(engine.capabilities().len(), 1);

    engine.shutdown().expect("engine shutdown should succeed");
}

#[test]
fn phase6_core_health_observation_does_not_mutate_indexing_lifecycle() {
    use nizaam_core::control_plane::{EngineObservation, Observations};
    use nizaam_core::health::{HealthReport, LivenessReport, ReadinessReport};

    let (engine, _registry, _capability_id) = prepared_engine();
    let mut index_lifecycle = IndexLifecycle::new(phase2_index_id(0xc1));

    index_lifecycle
        .mark_building()
        .expect("Creating -> Building should be valid");
    index_lifecycle
        .mark_validating()
        .expect("Building -> Validating should be valid");
    index_lifecycle
        .mark_ready()
        .expect("Validating -> Ready should be valid");
    index_lifecycle
        .mark_active()
        .expect("Ready -> Active should be valid");

    let health = HealthReport::new(
        engine.engine_id().clone(),
        LifecycleState::Serving,
        LivenessReport::healthy(),
        ReadinessReport::from_lifecycle(LifecycleState::Serving),
        Vec::new(),
        Vec::new(),
    )
    .expect("health report should be valid");

    Observations::new()
        .update(
            EngineObservation::new(
                engine.engine_id().clone(),
                engine.engine_instance_id().clone(),
                health,
            )
            .expect("observation should be valid"),
        )
        .expect("health observation should succeed");

    // Phase 6 health/readiness is observational. It cannot promote, drain,
    // retire, or otherwise mutate either lifecycle owner.
    assert_eq!(engine.runtime().state(), LifecycleState::Serving);
    assert_eq!(index_lifecycle.state(), IndexLifecycleState::Active);

    engine.shutdown().expect("engine shutdown should succeed");
}

#[test]
fn phase5_index_event_and_response_stay_inside_the_existing_core_request_boundary() {
    let capability_id = CapabilityId::new("nizaam.indexing.integration.phase5.event")
        .expect("capability ID should be valid");
    let operation = operation_context("integration-phase5-event");
    let request = universal_request(request_envelope(
        &test_engine(),
        operation.clone(),
        capability_id,
        "integration-phase5-event-message",
        b"source-owned-payload",
    ));

    let event_id = request.event_id().clone();
    let message_id = request.message_id().clone();

    let requirement = IndexRequirement::new(
        phase2_namespace("integration.phase5.event"),
        IndexFamily::Inverted,
        phase2_key_definition(&["term"]),
        phase2_target_reference_type(),
        Uniqueness::NonUnique,
        phase2_consistency(),
        None,
        None,
    )
    .expect("requirement should be valid");

    let event = IndexEvent::new(
        request,
        requirement,
        phase2_reference("source", "object:1"),
        KeyMaterial::text("term"),
    )
    .expect("IndexEvent should be valid");

    assert_eq!(event.event_id(), &event_id);
    assert_eq!(event.message_id(), &message_id);
    assert_eq!(event.operation_context(), &operation);
    assert_eq!(event.source_payload(), b"source-owned-payload");

    let response = IndexEventResponse::with_version(
        phase2_index_id(0xb2),
        IndexVersionId::new("integration.phase5.response-v1")
            .expect("response version ID should be valid"),
    );

    assert_eq!(response.index_id(), &phase2_index_id(0xb2));
    assert_eq!(
        response
            .version()
            .expect("response should carry logical version metadata")
            .as_str(),
        "integration.phase5.response-v1"
    );
}
