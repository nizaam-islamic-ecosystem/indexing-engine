//! Repository-level integration tests for the Phase 0 and Phase 2 public API.
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
use nizaam_indexing::identity::{
    IndexDefinitionId, IndexDefinitionIdentity, IndexId, IndexNamespace,
};
use nizaam_indexing::index::{
    ConsistencyRequirement, IndexDefinition, IndexEntry, IndexFamily, IndexVersion, IndexVersionId,
    KeyDefinition, KeyMaterial, ObjectReference, QueryHit, QueryRequest, QueryResult,
    SchemaVersion, SimilarityEntry, SourceVersion, TargetReferenceType, Uniqueness,
};
use nizaam_indexing::requirement::IndexRequirement;
use nizaam_indexing::{IndexingEngine, RequestHandlingError};

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
    let index_id =
        phase2_generated_index_id(
            definition.namespace(),
            definition.definition_id(),
            definition.family(),
            &key,
        );
    let repeated_index_id =
        phase2_generated_index_id(
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
