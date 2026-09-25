//! Level 3 integration tests for the Phase 2 Indexing data model.
//!
//! These tests exercise the public logical index contracts from outside the
//! crate. They verify that definitions, entries, references, similarity data,
//! versions, queries, and results compose without introducing physical
//! storage/provider behavior or domain-semantic ownership.
//!
//! The tests intentionally remain provider-neutral and reference-oriented.

use nizaam_indexing::identity::{
    IndexDefinitionId, IndexDefinitionIdentity, IndexId, IndexNamespace,
};
use nizaam_indexing::index::{
    ConsistencyRequirement, IndexDefinition, IndexEntry, IndexFamily, IndexVersion, IndexVersionId,
    KeyDefinition, KeyMaterial, ObjectReference, QueryHit, QueryRequest, QueryResult,
    SchemaVersion, SimilarityEntry, SourceVersion, TargetReferenceType, Uniqueness,
};
use std::num::NonZeroUsize;

fn index_id(seed: u8) -> IndexId {
    IndexId::from_bytes([seed; 64])
}

fn namespace(value: &str) -> IndexNamespace {
    IndexNamespace::new(value).expect("test namespace must be valid")
}

fn definition_id(value: &str) -> IndexDefinitionId {
    IndexDefinitionId::new(value).expect("test definition ID must be valid")
}

fn key_definition(fields: &[&str]) -> KeyDefinition {
    KeyDefinition::new(fields.iter().copied()).expect("test key definition must be valid")
}

fn target_reference_type(value: &str) -> TargetReferenceType {
    TargetReferenceType::new(value).expect("test target-reference type must be valid")
}

fn consistency(value: &str) -> ConsistencyRequirement {
    ConsistencyRequirement::new(value).expect("test consistency requirement must be valid")
}

fn source_version(value: &str) -> SourceVersion {
    SourceVersion::new(value).expect("test source version must be valid")
}

fn schema_version(value: &str) -> SchemaVersion {
    SchemaVersion::new(value).expect("test schema version must be valid")
}

fn object_reference(source: &str, reference: &str) -> ObjectReference {
    ObjectReference::new(source, reference).expect("test object reference must be valid")
}

fn index_version(value: &str) -> IndexVersion {
    IndexVersion::new(IndexVersionId::new(value).expect("test index version ID must be valid"))
}

fn definition() -> IndexDefinition {
    let identity = IndexDefinitionIdentity::new(
        definition_id("documents.v1"),
        namespace("search.documents"),
        IndexFamily::Inverted,
    );

    IndexDefinition::new(
        identity,
        key_definition(&["term"]),
        target_reference_type("source.document"),
        Uniqueness::NonUnique,
        consistency("logical-v1"),
        Some(source_version("source-v3")),
        Some(schema_version("schema-v2")),
    )
    .expect("test index definition must be valid")
}

#[test]
fn all_phase_two_index_family_values_remain_available() {
    let families = [
        IndexFamily::Identity,
        IndexFamily::Inverted,
        IndexFamily::Relationship,
        IndexFamily::Similarity,
    ];

    assert_eq!(families.len(), 4);

    for (position, family) in families.iter().enumerate() {
        for (other_position, other_family) in families.iter().enumerate() {
            if position == other_position {
                assert_eq!(family, other_family);
            } else {
                assert_ne!(family, other_family);
            }
        }
    }
}

#[test]
fn index_definition_composes_phase_one_identity_types() {
    let definition = definition();

    assert_eq!(definition.definition_id().as_str(), "documents.v1");
    assert_eq!(definition.namespace().as_str(), "search.documents");
    assert_eq!(definition.family(), IndexFamily::Inverted);
    assert_eq!(definition.key_definition().fields()[0].name(), "term");
}

#[test]
fn index_definition_preserves_target_uniqueness_consistency_and_versions() {
    let definition = definition();

    assert_eq!(
        definition.target_reference_type().as_str(),
        "source.document"
    );
    assert_eq!(definition.uniqueness(), Uniqueness::NonUnique);
    assert_eq!(definition.consistency_requirement().as_str(), "logical-v1");
    assert_eq!(
        definition
            .source_version()
            .expect("source version")
            .as_str(),
        "source-v3"
    );
    assert_eq!(
        definition
            .schema_version()
            .expect("schema version")
            .as_str(),
        "schema-v2"
    );
    definition.validate().expect("definition should validate");
}

#[test]
fn index_definition_identity_does_not_become_index_identity() {
    let definition = definition();
    let index = index_id(0x11);

    assert_eq!(definition.definition_id().as_str(), "documents.v1");
    assert_eq!(index.as_bytes(), &[0x11; 64]);

    fn accepts_index_id(_: &IndexId) {}
    fn accepts_definition_id(_: &IndexDefinitionId) {}

    accepts_index_id(&index);
    accepts_definition_id(definition.definition_id());
}

#[test]
fn index_entry_accepts_generic_key_material_and_object_reference() {
    let key = KeyMaterial::text("bismillah");
    let reference = object_reference("quran", "verse:1:1");

    let entry = IndexEntry::new(key.clone(), reference.clone())
        .expect("logical index entry should be valid");

    assert_eq!(entry.key(), &key);
    assert_eq!(entry.target(), &reference);
    entry.validate().expect("entry should validate");
}

#[test]
fn index_entry_can_preserve_structured_key_material_without_interpreting_it() {
    let key = KeyMaterial::map([
        ("source", KeyMaterial::text("documents")),
        (
            "term",
            KeyMaterial::Sequence(vec![
                KeyMaterial::text("quran"),
                KeyMaterial::text("search"),
            ]),
        ),
    ])
    .expect("structured key material should be valid");

    let entry = IndexEntry::new(key.clone(), object_reference("documents", "document:1"))
        .expect("structured index entry should be valid");

    assert_eq!(entry.key(), &key);
    assert!(matches!(entry.key(), KeyMaterial::Map(_)));
}

#[test]
fn object_reference_remains_an_opaque_source_owned_target() {
    let reference = object_reference("documents", "document:1");

    assert_eq!(reference.source(), "documents");
    assert_eq!(reference.object_reference(), "document:1");

    fn accepts_reference(_: &ObjectReference) {}
    accepts_reference(&reference);
}

#[test]
fn similarity_entry_remains_generic_and_reference_oriented() {
    let representation = KeyMaterial::bytes(vec![1, 2, 3, 4]);
    let target = object_reference("documents", "document:2");
    let metadata = KeyMaterial::text("logical-similarity-metadata");

    let entry = SimilarityEntry::with_metadata(
        representation.clone(),
        target.clone(),
        Some(metadata.clone()),
    )
    .expect("similarity entry should be valid");

    assert_eq!(entry.representation(), &representation);
    assert_eq!(entry.target(), &target);
    assert_eq!(entry.metadata(), Some(&metadata));
    entry.validate().expect("similarity entry should validate");
}

#[test]
fn similarity_family_does_not_create_relationship_semantics() {
    let representation = KeyMaterial::text("embedding-like-opaque-value");
    let target = object_reference("documents", "document:3");

    let entry =
        SimilarityEntry::new(representation, target).expect("similarity entry should be valid");

    assert_eq!(entry.target().source(), "documents");
    assert_eq!(entry.target().object_reference(), "document:3");

    // The similarity contract exposes representation + target + generic
    // metadata only. It does not expose a predicate, relation, or ontology.
}

#[test]
fn relationship_family_does_not_create_semantic_predicate_ownership() {
    let identity = IndexDefinitionIdentity::new(
        definition_id("relationships.v1"),
        namespace("graph.relationships"),
        IndexFamily::Relationship,
    );

    let definition = IndexDefinition::new(
        identity,
        key_definition(&["source", "predicate", "target"]),
        target_reference_type("source.relationship"),
        Uniqueness::NonUnique,
        consistency("logical"),
        None,
        None,
    )
    .expect("relationship-family definition should be valid");

    assert_eq!(definition.family(), IndexFamily::Relationship);
    assert_eq!(definition.key_definition().fields()[1].name(), "predicate");
    // "predicate" is only a source-defined key field here; Indexing assigns no
    // semantic meaning to its value.
}

#[test]
fn index_version_composes_with_logical_definition_metadata() {
    let definition = definition();
    let version = IndexVersion::with_metadata(
        IndexVersionId::new("documents.index-v1").expect("valid index version ID"),
        definition.source_version().cloned(),
        definition.schema_version().cloned(),
        Some(KeyMaterial::text("candidate")),
    )
    .expect("index version should be valid");

    assert_eq!(version.id().as_str(), "documents.index-v1");
    assert_eq!(
        version.source_version().expect("source version").as_str(),
        "source-v3"
    );
    assert_eq!(
        version.schema_version().expect("schema version").as_str(),
        "schema-v2"
    );
    assert_eq!(version.metadata(), Some(&KeyMaterial::text("candidate")));
}

#[test]
fn logical_query_request_remains_provider_neutral() {
    let request = QueryRequest::with_options(
        index_id(0x22),
        KeyMaterial::map([("term", KeyMaterial::text("bismillah"))])
            .expect("query key material should be valid"),
        NonZeroUsize::new(10),
        Some(KeyMaterial::text("caller-metadata")),
    )
    .expect("logical query request should be valid");

    assert_eq!(request.index_id().as_bytes(), &[0x22; 64]);
    assert_eq!(request.limit(), NonZeroUsize::new(10));
    assert!(matches!(request.query(), KeyMaterial::Map(_)));
    assert_eq!(
        request.metadata(),
        Some(&KeyMaterial::text("caller-metadata"))
    );
    request.validate().expect("request should validate");
}

#[test]
fn query_result_is_reference_only_and_carries_index_version() {
    let version = index_version("documents.index-v2");
    let reference = object_reference("documents", "document:42");
    let hit = QueryHit::new(reference.clone())
        .with_metrics(Some(0.95), Some(0.10))
        .expect("logical retrieval metrics should be valid");

    let result =
        QueryResult::with_continuation(index_id(0x33), version, vec![hit], Some(vec![1, 2, 3]))
            .expect("query result should be valid");

    assert_eq!(result.index_id().as_bytes(), &[0x33; 64]);
    assert_eq!(result.index_version().id().as_str(), "documents.index-v2");
    assert_eq!(result.hits().len(), 1);
    assert_eq!(result.hits()[0].reference(), &reference);
    assert_eq!(result.hits()[0].score(), Some(0.95));
    assert_eq!(result.hits()[0].distance(), Some(0.10));
    assert_eq!(result.continuation(), Some(&[1, 2, 3][..]));
    result.validate().expect("query result should validate");
}

#[test]
fn query_result_does_not_hydrate_domain_objects() {
    let result = QueryResult::new(
        index_id(0x44),
        index_version("documents.index-v3"),
        vec![QueryHit::new(object_reference("documents", "document:99"))],
    )
    .expect("query result should be valid");

    assert_eq!(
        result.hits()[0].reference().object_reference(),
        "document:99"
    );

    fn accepts_reference(_: &ObjectReference) {}
    accepts_reference(result.hits()[0].reference());
}

#[test]
fn index_entry_and_similarity_entry_are_distinct_logical_contracts() {
    let reference = object_reference("documents", "document:7");

    let index_entry = IndexEntry::new(KeyMaterial::text("term"), reference.clone())
        .expect("index entry should be valid");

    let similarity_entry = SimilarityEntry::new(KeyMaterial::bytes(vec![7, 8, 9]), reference)
        .expect("similarity entry should be valid");

    assert_eq!(index_entry.target().source(), "documents");
    assert_eq!(similarity_entry.target().source(), "documents");
    assert_ne!(index_entry.key(), similarity_entry.representation());
}

#[test]
fn domain_and_provider_specific_types_are_not_required_by_the_phase_two_model() {
    let definition = definition();
    let entry = IndexEntry::new(
        KeyMaterial::text("term"),
        object_reference("documents", "document:8"),
    )
    .expect("entry should be valid");
    let version = index_version("documents.index-v4");
    let request = QueryRequest::new(index_id(0x55), KeyMaterial::text("term"))
        .expect("request should be valid");

    assert_eq!(definition.family(), IndexFamily::Inverted);
    assert_eq!(entry.target().source(), "documents");
    assert_eq!(version.id().as_str(), "documents.index-v4");
    assert_eq!(request.query(), &KeyMaterial::text("term"));

    // This composition uses only logical Indexing contracts. No database,
    // storage-provider, partition, shard, embedding-provider, or domain
    // object type participates in the public model.
}
