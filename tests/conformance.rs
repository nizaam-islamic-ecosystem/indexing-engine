//! Phase 2 architectural conformance tests.
//!
//! These tests protect the boundaries established between `nizaam-indexing`
//! and `nizaam-core`. They preserve the Phase 0 runtime/capability checks while
//! extending conformance coverage to the Phase 2 logical indexing contracts:
//! requirements, definitions, entries, references, versions, similarity
//! entries, and provider-neutral query/result contracts.
//!
//! They deliberately do not test physical index construction, storage
//! providers, query execution, embedding generation, domain-object hydration,
//! or domain-specific semantic models.

mod common;

use common::{
    capability_definition, capability_id, capability_invocation, engine_id, engine_instance_id,
    engine_registry, operation_context, register_engine, test_engine, universal_request,
    universal_response,
};
use nizaam_core::capability::{CapabilityDispatchResult, CapabilityError};
use nizaam_core::contracts::{
    ContractDescriptor, ContractMetadata, EncodedPayload, Interaction, MessageEnvelope,
    Participants, PayloadDescriptor, Version,
};
use nizaam_core::identity::{
    AttemptId, CapabilityId, ContractId, CorrelationId, EngineId, EngineInstanceId, MessageId,
    NodeId, OperationId,
};
use nizaam_core::operation::OperationContext;
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
use nizaam_indexing::{IndexingEngine, IndexingRegistration, IndexingRuntime};

fn core_shutdown_token(_: &nizaam_core::runtime::CancellationToken) {}

fn core_background_tasks(_: &nizaam_core::runtime::BackgroundTasks) {}

fn envelope(
    sender: EngineId,
    target: EngineId,
    operation_context: OperationContext,
    capability_id: CapabilityId,
    interaction: Interaction,
    message_id: &str,
    payload: &[u8],
) -> MessageEnvelope {
    let descriptor = ContractDescriptor::new(
        ContractId::new("nizaam.indexing.conformance.contract")
            .expect("test contract id must be valid"),
        capability_id,
        Version::new(1, 0, 0),
        interaction,
        PayloadDescriptor::new("application/octet-stream", Version::new(1, 0, 0))
            .expect("payload descriptor must be valid"),
    );

    let metadata = ContractMetadata::new(descriptor.clone(), Participants::new(sender, target));

    MessageEnvelope::new(
        MessageId::new(message_id).expect("test message id must be valid"),
        operation_context,
        metadata,
        EncodedPayload::new(descriptor.payload, payload.to_vec()),
    )
}

fn phase2_index_id(byte: u8) -> IndexId {
    IndexId::from_bytes([byte; 64])
}

fn phase2_namespace(value: &str) -> IndexNamespace {
    IndexNamespace::new(value).expect("test namespace must be valid")
}

fn phase2_definition_id(value: &str) -> IndexDefinitionId {
    IndexDefinitionId::new(value).expect("test definition ID must be valid")
}

fn phase2_key_definition() -> KeyDefinition {
    KeyDefinition::new(["field"]).expect("test key definition must be valid")
}

fn phase2_target_reference_type() -> TargetReferenceType {
    TargetReferenceType::new("source.object").expect("test target reference type must be valid")
}

fn phase2_consistency() -> ConsistencyRequirement {
    ConsistencyRequirement::new("logical").expect("test consistency requirement must be valid")
}

fn phase2_object_reference(source: &str, object_reference: &str) -> ObjectReference {
    ObjectReference::new(source, object_reference).expect("test object reference must be valid")
}

fn phase2_index_version(id: &str) -> IndexVersion {
    IndexVersion::new(IndexVersionId::new(id).expect("test index version ID must be valid"))
}

#[test]
fn indexing_engine_keeps_core_engine_and_instance_identity_roles_distinct() {
    let engine_id = engine_id("nizaam.indexing.conformance.identity");
    let instance_id = engine_instance_id("nizaam.indexing.conformance.identity.instance");
    let engine = IndexingEngine::new(engine_id.clone(), instance_id.clone());

    assert_eq!(engine.engine_id(), &engine_id);
    assert_eq!(engine.engine_instance_id(), &instance_id);
    assert_ne!(
        engine.engine_id().as_str(),
        engine.engine_instance_id().as_str()
    );
    assert_eq!(engine.registration().engine_id(), &engine_id);
    assert_eq!(engine.registration().engine_instance_id(), &instance_id);
}

#[test]
fn operation_and_attempt_identity_roles_remain_separate_core_values() {
    let operation_id = OperationId::new("nizaam.indexing.conformance.operation")
        .expect("operation id must be valid");
    let correlation_id = CorrelationId::new("nizaam.indexing.conformance.correlation")
        .expect("correlation id must be valid");
    let node_id = NodeId::new("nizaam.indexing.conformance.node").expect("node id must be valid");
    let attempt_id =
        AttemptId::new("nizaam.indexing.conformance.attempt").expect("attempt id must be valid");

    let operation = nizaam_core::operation::Operation::new(operation_id.clone(), correlation_id);
    let context = OperationContext::new(operation.clone()).for_attempt(node_id, attempt_id.clone());

    assert_eq!(context.operation.id, operation_id);
    assert_eq!(context.attempt_id.as_ref(), Some(&attempt_id));
    assert_ne!(
        context.operation.id.as_str(),
        context.attempt_id.as_ref().unwrap().as_str()
    );
}

#[test]
fn universal_request_and_response_use_the_core_contract_boundary() {
    let engine = test_engine();
    let capability_id = CapabilityId::new("nizaam.indexing.conformance.contract-capability")
        .expect("capability id must be valid");
    let operation = operation_context("conformance-universal-contracts");

    let request = universal_request(envelope(
        engine_id("nizaam.conformance.caller"),
        engine.engine_id().clone(),
        operation.clone(),
        capability_id.clone(),
        Interaction::Request,
        "conformance-request-message",
        b"request-payload",
    ));

    let response = universal_response(
        envelope(
            engine.engine_id().clone(),
            engine_id("nizaam.conformance.caller"),
            operation,
            capability_id,
            Interaction::Response,
            "conformance-response-message",
            b"response-payload",
        ),
        Status::Success,
    );

    assert!(request.has_request_interaction());
    assert!(response.has_response_interaction());
    assert_eq!(request.message_id().as_str(), "conformance-request-message");
    assert_eq!(
        response.message_id().as_str(),
        "conformance-response-message"
    );
    assert!(!request.event_id().as_str().is_empty());
    assert!(!response.event_id().as_str().is_empty());
    assert_ne!(request.event_id(), response.event_id());
    assert_ne!(request.event_id().as_str(), request.message_id().as_str());
    assert_ne!(response.event_id().as_str(), response.message_id().as_str());
}

#[test]
fn capability_set_uses_the_core_capability_registry_through_the_engine_boundary() {
    let engine = test_engine();

    engine.start().unwrap();
    engine.begin_registration().unwrap();

    let capability_id = capability_id("nizaam.indexing.conformance.capability");
    engine
        .register_capability(
            capability_definition(engine.engine_id(), &capability_id),
            common::echo_handler(),
        )
        .expect("Core capability registry registration should succeed");

    assert!(engine.capabilities().contains(&capability_id));
    assert_eq!(engine.capabilities().len(), 1);
}

#[test]
fn indexing_runtime_exposes_core_owned_runtime_surfaces_without_a_second_runtime_system() {
    let engine = test_engine();
    let runtime: &IndexingRuntime = engine.runtime();

    core_shutdown_token(runtime.shutdown_token());
    core_background_tasks(runtime.background_tasks());
    assert_eq!(runtime.state(), LifecycleState::Created);
}

#[test]
fn engine_registration_requires_an_external_core_registry() {
    let engine = test_engine();
    let first_registry = engine_registry();
    let second_registry = engine_registry();

    assert!(!first_registry.contains(engine.engine_instance_id()));
    assert!(!second_registry.contains(engine.engine_instance_id()));

    engine.start().unwrap();
    engine.begin_registration().unwrap();
    register_engine(&engine, &first_registry).unwrap();

    assert!(first_registry.contains(engine.engine_instance_id()));
    assert!(!second_registry.contains(engine.engine_instance_id()));
}

#[test]
fn engine_registration_and_capability_registration_are_separate_core_boundaries() {
    let engine = test_engine();
    let registry = engine_registry();

    engine.start().unwrap();
    engine.begin_registration().unwrap();
    register_engine(&engine, &registry).unwrap();

    assert!(registry.contains(engine.engine_instance_id()));
    assert!(engine.capabilities().is_empty());
    assert_eq!(engine.runtime().state(), LifecycleState::Registering);

    let capability_id = engine.register_phase0_capability().unwrap();

    assert!(engine.capabilities().contains(&capability_id));
    assert!(registry.contains(engine.engine_instance_id()));
    assert_eq!(engine.runtime().state(), LifecycleState::Registering);
}

#[test]
fn capability_state_is_not_a_global_registry_shared_by_independent_engines() {
    let first = IndexingEngine::new(
        engine_id("nizaam.indexing.conformance.first"),
        engine_instance_id("nizaam.indexing.conformance.first.instance"),
    );
    let second = IndexingEngine::new(
        engine_id("nizaam.indexing.conformance.second"),
        engine_instance_id("nizaam.indexing.conformance.second.instance"),
    );

    first.start().unwrap();
    first.begin_registration().unwrap();
    first.register_phase0_capability().unwrap();

    assert_eq!(first.capabilities().len(), 1);
    assert!(second.capabilities().is_empty());
}

#[test]
fn core_runtime_admission_remains_the_authoritative_request_gate() {
    let engine = test_engine();
    let registry = engine_registry();

    engine.start().unwrap();

    assert_eq!(
        engine.runtime().admit_request(),
        Err(RequestAdmissionError::NotServing(
            LifecycleState::Capabilities
        ))
    );

    engine.begin_registration().unwrap();
    register_engine(&engine, &registry).unwrap();
    engine.register_phase0_capability().unwrap();
    engine.mark_ready().unwrap();

    assert_eq!(
        engine.runtime().admit_request(),
        Err(RequestAdmissionError::NotServing(LifecycleState::Ready))
    );

    engine.serve().unwrap();
    assert!(engine.runtime().admit_request().is_ok());

    engine.drain().unwrap();
    assert_eq!(
        engine.runtime().admit_request(),
        Err(RequestAdmissionError::NotServing(LifecycleState::Draining))
    );
}

#[test]
fn phase0_capability_remains_opaque_and_domain_neutral() {
    let engine = test_engine();
    let registry = engine_registry();

    engine.start().unwrap();
    engine.begin_registration().unwrap();
    register_engine(&engine, &registry).unwrap();
    let capability_id = engine.register_phase0_capability().unwrap();
    engine.mark_ready().unwrap();
    engine.serve().unwrap();

    let context = engine
        .runtime()
        .context(operation_context("conformance-opaque-payload"));
    let invocation = capability_invocation(
        capability_id.clone(),
        b"opaque-bytes-without-domain-meaning",
    );

    let result = engine
        .runtime()
        .dispatch(engine.capabilities(), &context, &invocation)
        .expect("Serving runtime should admit dispatch");

    match result {
        CapabilityDispatchResult::Outcome(outcome) => {
            assert_eq!(outcome.as_bytes(), b"opaque-bytes-without-domain-meaning");
        }
        CapabilityDispatchResult::Error(error) => {
            panic!("unexpected Core capability error: {error:?}");
        }
    }
}

#[test]
fn unknown_capability_produces_a_core_error_without_indexing_specific_error_translation() {
    let engine = test_engine();
    let registry = engine_registry();

    engine.start().unwrap();
    engine.begin_registration().unwrap();
    register_engine(&engine, &registry).unwrap();
    engine.register_phase0_capability().unwrap();
    engine.mark_ready().unwrap();
    engine.serve().unwrap();

    let unknown = CapabilityId::new("nizaam.indexing.conformance.unknown")
        .expect("unknown capability id must be valid");
    let context = engine
        .runtime()
        .context(operation_context("conformance-core-error"));
    let invocation = capability_invocation(unknown, b"opaque-payload");

    let result = engine
        .runtime()
        .dispatch(engine.capabilities(), &context, &invocation)
        .expect("runtime admission should succeed while Serving");

    assert!(matches!(
        result,
        CapabilityDispatchResult::Error(CapabilityError::Unknown)
    ));
}

#[test]
fn published_engine_registration_can_be_observed_without_turning_the_registry_into_a_router() {
    let engine = test_engine();
    let registry = engine_registry();

    engine.start().unwrap();
    engine.begin_registration().unwrap();
    register_engine(&engine, &registry).unwrap();

    let registration: &IndexingRegistration = engine.registration();

    assert_eq!(registration.engine_id(), engine.engine_id());
    assert_eq!(
        registration.engine_instance_id(),
        engine.engine_instance_id()
    );
    assert!(registry.contains(engine.engine_instance_id()));
    assert_eq!(engine.runtime().state(), LifecycleState::Registering);

    // Registration establishes discoverable identity only. Capability
    // execution still requires the separate runtime/capability path.
    assert!(engine.capabilities().is_empty());
}

#[test]
fn phase1_index_identity_namespace_and_definition_identity_remain_distinct() {
    use nizaam_indexing::identity::{
        IndexDefinitionId, IndexDefinitionIdentity, IndexId, IndexNamespace,
    };
    use nizaam_indexing::index::IndexFamily;

    let index_id = IndexId::from_bytes([0x61; 64]);
    let namespace = IndexNamespace::new("conformance.identity").expect("namespace must be valid");
    let definition_id =
        IndexDefinitionId::new("conformance-index").expect("definition id must be valid");

    let definition = IndexDefinitionIdentity::new(
        definition_id.clone(),
        namespace.clone(),
        IndexFamily::Identity,
    );

    assert_eq!(index_id.as_bytes(), &[0x61; 64]);
    assert_eq!(definition.definition_id(), &definition_id);
    assert_eq!(definition.namespace(), &namespace);
    assert_eq!(definition.family(), IndexFamily::Identity);

    // These are deliberately separate Phase 1 identity roles. The
    // conformance test does not introduce a conversion or equality relation
    // between them.
    assert_ne!(
        index_id.as_bytes(),
        definition.definition_id().as_str().as_bytes()
    );
}

#[test]
fn phase1_namespace_registry_is_logical_and_separate_from_engine_registration() {
    use nizaam_indexing::identity::{IndexNamespace, NamespaceRegistry};

    let engine = test_engine();
    let engine_registry = engine_registry();
    let mut namespace_registry = NamespaceRegistry::new();

    let namespace =
        IndexNamespace::new("conformance.logical-space").expect("namespace must be valid");

    engine.start().unwrap();
    engine.begin_registration().unwrap();
    register_engine(&engine, &engine_registry).unwrap();

    namespace_registry
        .register(namespace.clone())
        .expect("namespace registration should succeed");

    assert!(engine_registry.contains(engine.engine_instance_id()));
    assert!(namespace_registry.contains(&namespace));

    // The two registries represent different concerns: engine discovery
    // versus logical index namespaces.
    assert_eq!(namespace_registry.len(), 1);
    assert!(!namespace_registry.contains(
        &IndexNamespace::new("conformance.other-space").expect("namespace must be valid")
    ));
}

#[test]
fn phase1_index_family_is_not_a_semantic_mapping_or_physical_implementation() {
    use nizaam_indexing::identity::IndexNamespace;
    use nizaam_indexing::index::IndexFamily;

    let namespace =
        IndexNamespace::new("conformance.relationships").expect("namespace must be valid");

    let relationship = (namespace.clone(), IndexFamily::Relationship);
    let similarity = (namespace, IndexFamily::Similarity);

    assert_eq!(relationship.1, IndexFamily::Relationship);
    assert_eq!(similarity.1, IndexFamily::Similarity);

    // Phase 1 exposes only the generic family classification. No semantic
    // predicate, graph relation, embedding model, vector database, ANN
    // structure, or physical storage implementation is represented here.
    assert_eq!(relationship.0.as_str(), "conformance.relationships");
    assert_eq!(similarity.0.as_str(), "conformance.relationships");
}

#[test]
fn phase1_logical_index_space_is_independent_from_core_engine_identity() {
    use nizaam_indexing::identity::{IndexNamespace, NamespaceRegistry};

    let engine = test_engine();
    let mut namespace_registry = NamespaceRegistry::new();

    let namespace =
        IndexNamespace::new("conformance.index-space").expect("namespace must be valid");
    namespace_registry.register(namespace.clone()).unwrap();

    assert!(namespace_registry.contains(&namespace));
    assert_eq!(engine.engine_id().as_str(), "nizaam.indexing.test");
    assert_ne!(namespace.as_str(), engine.engine_id().as_str());

    // A logical namespace is not an EngineId, EngineInstanceId, node, or
    // physical partition. Core remains the owner of engine/runtime identity.
}

#[test]
fn phase1_index_identity_is_not_a_core_object_identity() {
    use nizaam_indexing::identity::IndexId;

    let index_id = IndexId::from_bytes([0x72; 64]);
    let operation_id = OperationId::new("nizaam.indexing.conformance.phase1-operation")
        .expect("operation id must be valid");

    assert_eq!(index_id.as_bytes(), &[0x72; 64]);
    assert_ne!(index_id.as_bytes(), operation_id.as_str().as_bytes());

    // `IndexId` is the indexing crate's opaque concrete-index identity;
    // Core operation identity remains an independent contract.
}

#[test]
fn core_identity_types_are_used_directly_at_the_registration_boundary() {
    let engine_id =
        EngineId::new("nizaam.indexing.conformance.registration").expect("engine id must be valid");
    let instance_id = EngineInstanceId::new("nizaam.indexing.conformance.registration.instance")
        .expect("engine instance id must be valid");
    let registration = IndexingRegistration::new(engine_id.clone(), instance_id.clone());

    assert_eq!(registration.engine_id(), &engine_id);
    assert_eq!(registration.engine_instance_id(), &instance_id);
}

#[test]
fn phase2_requirement_is_source_owned_input_and_normalizes_into_an_index_definition() {
    let requirement = IndexRequirement::new(
        phase2_namespace("conformance.requirement"),
        IndexFamily::Inverted,
        phase2_key_definition(),
        phase2_target_reference_type(),
        Uniqueness::NonUnique,
        phase2_consistency(),
        Some(SourceVersion::new("source-v2").expect("source version must be valid")),
        Some(SchemaVersion::new("schema-v3").expect("schema version must be valid")),
    )
    .expect("requirement should be valid");

    let definition = requirement
        .normalize(phase2_definition_id("conformance.requirement.v1"))
        .expect("normalization should produce a valid definition");

    assert_eq!(definition.namespace(), requirement.namespace());
    assert_eq!(definition.family(), requirement.family());
    assert_eq!(definition.key_definition(), requirement.key_definition());
    assert_eq!(
        definition.target_reference_type(),
        requirement.target_reference_type()
    );
    assert_eq!(
        definition.definition_id().as_str(),
        "conformance.requirement.v1"
    );

    // Normalization creates a logical IndexDefinition. It does not create a
    // concrete IndexId, select a provider, or execute index construction.
}

#[test]
fn phase2_index_definition_is_logical_and_provider_neutral() {
    let identity = IndexDefinitionIdentity::new(
        phase2_definition_id("conformance.logical-definition"),
        phase2_namespace("conformance.logical"),
        IndexFamily::Inverted,
    );

    let definition = IndexDefinition::new(
        identity.clone(),
        phase2_key_definition(),
        phase2_target_reference_type(),
        Uniqueness::Unique,
        phase2_consistency(),
        Some(SourceVersion::new("source-v1").expect("source version must be valid")),
        Some(SchemaVersion::new("schema-v1").expect("schema version must be valid")),
    )
    .expect("definition should be valid");

    assert_eq!(definition.identity(), &identity);
    assert_eq!(definition.family(), IndexFamily::Inverted);
    assert_eq!(definition.uniqueness(), Uniqueness::Unique);
    assert_eq!(definition.source_version().unwrap().as_str(), "source-v1");
    assert_eq!(definition.schema_version().unwrap().as_str(), "schema-v1");

    // The public contract contains logical fields only. No provider,
    // partition, storage engine, physical algorithm, or execution handle is
    // required to construct the definition.
}

#[test]
fn phase2_index_entry_is_a_generic_key_to_object_reference_association() {
    let key = KeyMaterial::map([
        ("field", KeyMaterial::text("value")),
        ("ordinal", KeyMaterial::Unsigned(1)),
    ])
    .expect("structured key material must be valid");
    let reference = phase2_object_reference("source-a", "object-42");

    let entry =
        IndexEntry::new(key.clone(), reference.clone()).expect("index entry should be valid");

    assert_eq!(entry.key(), &key);
    assert_eq!(entry.target(), &reference);

    // The entry owns no domain object, database-row structure, or storage
    // provider state. Its target remains an opaque, source-owned reference.
}

#[test]
fn phase2_object_reference_remains_source_owned_and_does_not_create_canonical_identity() {
    let first = phase2_object_reference("source-a", "object-7");
    let second = phase2_object_reference("source-b", "object-7");

    assert_eq!(first.object_reference(), "object-7");
    assert_eq!(second.object_reference(), "object-7");
    assert_ne!(first.source(), second.source());

    let index_id = phase2_index_id(0x7a);
    assert_eq!(index_id.as_bytes(), &[0x7a; 64]);

    // The same source-level reference text can occur under different source
    // ownership. ObjectReference therefore remains a source-owned reference,
    // not a replacement Object/Index identity.
    assert_ne!(first, second);
}

#[test]
fn phase2_similarity_entry_accepts_generic_representation_without_embedding_or_relationship_semantics()
 {
    let representation = KeyMaterial::Sequence(vec![
        KeyMaterial::Unsigned(10),
        KeyMaterial::Unsigned(20),
        KeyMaterial::Unsigned(30),
    ]);
    let target = phase2_object_reference("similarity-source", "object-9");

    let entry = SimilarityEntry::new(representation.clone(), target.clone())
        .expect("similarity entry should be valid");

    assert_eq!(entry.representation(), &representation);
    assert_eq!(entry.target(), &target);
    assert!(entry.metadata().is_none());

    // Phase 2 carries generic representation material. It does not require an
    // embedding model, generator, ANN algorithm, vector provider, or semantic
    // relationship/predicate model.
}

#[test]
fn phase2_relationship_family_remains_a_generic_index_family() {
    let identity = IndexDefinitionIdentity::new(
        phase2_definition_id("conformance.relationship"),
        phase2_namespace("conformance.relationships"),
        IndexFamily::Relationship,
    );

    let definition = IndexDefinition::new(
        identity,
        KeyDefinition::new(["field_a", "field_b"])
            .expect("relationship key definition must be valid"),
        phase2_target_reference_type(),
        Uniqueness::NonUnique,
        phase2_consistency(),
        None,
        None,
    )
    .expect("relationship definition should be valid");

    assert_eq!(definition.family(), IndexFamily::Relationship);
    assert_eq!(definition.key_definition().len(), 2);

    // IndexFamily::Relationship is only the logical index-family classification.
    // It does not introduce predicate semantics, a graph model, or KG storage.
}

#[test]
fn phase2_versions_remain_separate_from_one_another_and_from_core_contract_version() {
    let core_contract_version = Version::new(1, 0, 0);
    let source_version = SourceVersion::new("source-v4").expect("source version must be valid");
    let schema_version = SchemaVersion::new("schema-v2").expect("schema version must be valid");
    let index_version = phase2_index_version("index-v7");

    // Core contract version is still represented by Core's `Version` type and
    // used at the universal contract boundary.
    assert_eq!(core_contract_version, Version::new(1, 0, 0));

    assert_eq!(source_version.as_str(), "source-v4");
    assert_eq!(schema_version.as_str(), "schema-v2");
    assert_eq!(index_version.id().as_str(), "index-v7");

    assert_ne!(source_version.as_str(), schema_version.as_str());
    assert_ne!(schema_version.as_str(), index_version.id().as_str());

    // The four version concepts remain owned by their respective contracts:
    // Core contract compatibility, source state, schema compatibility, and
    // logical index state.
}

#[test]
fn phase2_query_contracts_are_reference_oriented_and_do_not_execute_queries() {
    let index_id = phase2_index_id(0x55);
    let query = QueryRequest::new(index_id, KeyMaterial::text("lookup"))
        .expect("query request should be valid");

    let reference = phase2_object_reference("source-q", "object-11");
    let hit = QueryHit::new(reference.clone())
        .with_metrics(Some(0.91), None)
        .expect("query hit metrics should be valid");
    let result = QueryResult::new(
        phase2_index_id(0x55),
        phase2_index_version("index-v1"),
        vec![hit],
    )
    .expect("query result should be valid");

    assert_eq!(query.query(), &KeyMaterial::text("lookup"));
    assert_eq!(result.hits().len(), 1);
    assert_eq!(result.hits()[0].reference(), &reference);
    assert_eq!(result.hits()[0].score(), Some(0.91));

    // QueryRequest and QueryResult are logical retrieval contracts. They do not
    // contain an executor, provider handle, or hydrated domain object.
}
