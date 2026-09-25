//! Level 3 integration tests for the Phase 1 and Phase 2 identity boundaries.
//!
//! These tests exercise the public identity API as a downstream consumer would.
//! Phase 1 identity behavior remains covered, while Phase 2 adds regression
//! checks for `ObjectReference`, `IndexVersion`, source/schema version metadata,
//! and separation from Core contract-version identity.
//!
//! The tests intentionally verify identity boundaries only. They do not test
//! physical storage, provider selection, query execution, or domain-object
//! semantics.

use nizaam_core::contracts::Version as CoreContractVersion;
use nizaam_indexing::identity::{
    IndexDefinitionId, IndexDefinitionIdentity, IndexId, IndexIdGenerationVersion, IndexNamespace,
    NamespaceRegistry,
};
use nizaam_indexing::index::{
    IndexFamily, IndexVersion, IndexVersionId, KeyMaterial, ObjectReference, SchemaVersion,
    SourceVersion,
};

fn index_id(seed: u8) -> IndexId {
    IndexId::from_bytes([seed; 64])
}

fn namespace(value: &str) -> IndexNamespace {
    IndexNamespace::new(value).expect("test namespace must satisfy Phase 1 grammar")
}

fn definition_id(value: &str) -> IndexDefinitionId {
    IndexDefinitionId::new(value).expect("test definition ID must be valid")
}

fn object_reference(source: &str, reference: &str) -> ObjectReference {
    ObjectReference::new(source, reference).expect("test object reference must be valid")
}

fn index_version(value: &str) -> IndexVersion {
    let id = IndexVersionId::new(value).expect("test index version ID must be valid");
    IndexVersion::new(id)
}

fn source_version(value: &str) -> SourceVersion {
    SourceVersion::new(value).expect("test source version must be valid")
}

fn schema_version(value: &str) -> SchemaVersion {
    SchemaVersion::new(value).expect("test schema version must be valid")
}

fn generated_index_id(
    namespace: &IndexNamespace,
    definition: &IndexDefinitionId,
    key_material: &KeyMaterial,
) -> IndexId {
    IndexId::generate(namespace, definition, key_material)
        .expect("test key material must produce a valid index ID")
}

#[test]
fn index_id_is_independent_from_namespace_identity() {
    let index = index_id(0x11);
    let namespace = namespace("core.identity");

    let definition = IndexDefinitionIdentity::new(
        definition_id("identity-index"),
        namespace.clone(),
        IndexFamily::Identity,
    );

    assert_eq!(definition.namespace(), &namespace);
    assert_ne!(index.as_bytes(), [0u8; 64].as_ref());
}

#[test]
fn index_id_is_independent_from_definition_identity() {
    let index = index_id(0x22);
    let definition = IndexDefinitionIdentity::new(
        definition_id("definition-a"),
        namespace("core.identity"),
        IndexFamily::Identity,
    );

    assert_eq!(index.as_bytes(), &[0x22; 64]);
    assert_eq!(definition.definition_id().as_str(), "definition-a");
}

#[test]
fn index_id_is_distinct_from_index_definition_id() {
    let index = index_id(0x23);
    let definition_id = definition_id("definition-b");

    assert_eq!(index.as_bytes(), &[0x23; 64]);
    assert_eq!(definition_id.as_str(), "definition-b");

    fn accepts_index_id(_: &IndexId) {}
    fn accepts_definition_id(_: &IndexDefinitionId) {}

    accepts_index_id(&index);
    accepts_definition_id(&definition_id);
}

#[test]
fn generated_index_id_is_deterministic_for_identical_logical_input() {
    let namespace = namespace("quran.text");
    let definition = definition_id("verse-term");
    let key = KeyMaterial::text("lemma");

    let first = generated_index_id(&namespace, &definition, &key);
    let second = generated_index_id(&namespace, &definition, &key);

    assert_eq!(first, second);
    assert_eq!(first.as_bytes().len(), 64);
}

#[test]
fn generated_index_id_changes_when_namespace_definition_or_key_material_changes() {
    let name_space = namespace("quran.text");
    let alternate_namespace = namespace("quran.word");
    let definition = definition_id("verse-term");
    let alternate_definition = definition_id("verse-lemma");
    let key = KeyMaterial::text("lemma");
    let alternate_key = KeyMaterial::text("root");

    let base = generated_index_id(&name_space, &definition, &key);
    let namespace_changed = generated_index_id(&alternate_namespace, &definition, &key);
    let definition_changed = generated_index_id(&name_space, &alternate_definition, &key);
    let key_changed = generated_index_id(&name_space, &definition, &alternate_key);

    assert_ne!(base, namespace_changed);
    assert_ne!(base, definition_changed);
    assert_ne!(base, key_changed);
}

#[test]
fn equivalent_map_key_material_uses_one_canonical_index_id() {
    let namespace = namespace("quran.text");
    let definition = definition_id("verse-term");

    let first = KeyMaterial::map([
        ("source", KeyMaterial::text("quran")),
        ("verse", KeyMaterial::Unsigned(1)),
    ])
    .expect("first key material must be valid");
    let second = KeyMaterial::map([
        ("verse", KeyMaterial::Unsigned(1)),
        ("source", KeyMaterial::text("quran")),
    ])
    .expect("second key material must be valid");

    assert_eq!(first.canonical_bytes(), second.canonical_bytes());
    assert_eq!(
        generated_index_id(&namespace, &definition, &first),
        generated_index_id(&namespace, &definition, &second)
    );
}

#[test]
fn index_id_generation_version_is_separate_from_index_version() {
    let generation_version = IndexIdGenerationVersion::CURRENT;
    let index_version = index_version("index-v1");

    assert_eq!(generation_version.value(), 1);
    assert_eq!(IndexId::generation_version(), generation_version);
    assert_eq!(index_version.id().as_str(), "index-v1");
}

#[test]
fn multiple_namespaces_can_coexist() {
    let mut registry = NamespaceRegistry::new();

    let core = namespace("core.identity");
    let search = namespace("search.documents");
    let relations = namespace("graph.relations");

    registry.register(core.clone()).unwrap();
    registry.register(search.clone()).unwrap();
    registry.register(relations.clone()).unwrap();

    assert_eq!(registry.len(), 3);
    assert!(registry.contains(&core));
    assert!(registry.contains(&search));
    assert!(registry.contains(&relations));

    assert_eq!(registry.snapshot(), vec![core, relations, search]);
}

#[test]
fn duplicate_namespace_registration_fails() {
    let mut registry = NamespaceRegistry::new();
    let namespace = namespace("core.identity");

    registry.register(namespace.clone()).unwrap();

    assert!(registry.register(namespace.clone()).is_err());
    assert_eq!(registry.len(), 1);
    assert!(registry.contains(&namespace));
}

#[test]
fn namespace_unregister_removes_registered_namespace() {
    let mut registry = NamespaceRegistry::new();
    let namespace = namespace("core.identity");

    registry.register(namespace.clone()).unwrap();
    assert!(registry.contains(&namespace));

    assert!(registry.unregister(&namespace).is_ok());
    assert!(!registry.contains(&namespace));
    assert!(registry.is_empty());
}

#[test]
fn unregistered_namespace_is_absent() {
    let registry = NamespaceRegistry::new();
    let namespace = namespace("core.identity");

    assert!(!registry.contains(&namespace));
    assert!(registry.snapshot().is_empty());
    assert_eq!(registry.len(), 0);
}

#[test]
fn one_index_identity_can_be_associated_with_multiple_logical_spaces() {
    let index = index_id(0x33);

    let first_namespace = namespace("core.identity");
    let second_namespace = namespace("search.documents");

    let first_definition = IndexDefinitionIdentity::new(
        definition_id("identity-primary"),
        first_namespace.clone(),
        IndexFamily::Identity,
    );
    let second_definition = IndexDefinitionIdentity::new(
        definition_id("identity-search"),
        second_namespace.clone(),
        IndexFamily::Identity,
    );

    assert_ne!(first_definition.namespace(), second_definition.namespace());
    assert_ne!(
        first_definition.definition_id(),
        second_definition.definition_id()
    );

    assert_eq!(index.as_bytes(), &[0x33; 64]);
}

#[test]
fn definition_identity_is_separate_from_concrete_index_identity() {
    let index = index_id(0x44);
    let definition = IndexDefinitionIdentity::new(
        definition_id("identity-definition"),
        namespace("core.identity"),
        IndexFamily::Identity,
    );

    assert_eq!(definition.family(), IndexFamily::Identity);
    assert_eq!(definition.namespace().as_str(), "core.identity");
    assert_eq!(definition.definition_id().as_str(), "identity-definition");
    assert_eq!(index.as_bytes(), &[0x44; 64]);
}

#[test]
fn identity_components_compose_without_collapsing_into_one_identity_type() {
    let mut registry = NamespaceRegistry::new();
    let namespace = namespace("core.identity");

    registry.register(namespace.clone()).unwrap();

    let definition_id = definition_id("identity-index");
    let definition = IndexDefinitionIdentity::new(
        definition_id.clone(),
        namespace.clone(),
        IndexFamily::Identity,
    );
    let index = index_id(0x55);

    assert!(registry.contains(&namespace));
    assert_eq!(definition.definition_id(), &definition_id);
    assert_eq!(definition.namespace(), &namespace);
    assert_eq!(definition.family(), IndexFamily::Identity);
    assert_eq!(index.as_bytes(), &[0x55; 64]);
}

#[test]
fn index_namespace_is_distinct_from_object_reference() {
    let namespace = namespace("search.documents");
    let reference = object_reference("documents", "document:42");

    assert_eq!(namespace.as_str(), "search.documents");
    assert_eq!(reference.source(), "documents");
    assert_eq!(reference.object_reference(), "document:42");

    fn accepts_namespace(_: &IndexNamespace) {}
    fn accepts_object_reference(_: &ObjectReference) {}

    accepts_namespace(&namespace);
    accepts_object_reference(&reference);
}

#[test]
fn different_object_reference_sources_remain_distinct() {
    let first = object_reference("quran", "verse:1:1");
    let second = object_reference("hadith", "muslim:1");

    assert_ne!(first, second);
    assert_ne!(first.source(), second.source());
    assert_ne!(first.object_reference(), second.object_reference());
}

#[test]
fn index_version_has_its_own_identity_type() {
    let version = index_version("index-v1");

    assert_eq!(version.id().as_str(), "index-v1");

    fn accepts_index_version(_: &IndexVersion) {}
    accepts_index_version(&version);
}

#[test]
fn index_version_is_distinct_from_source_and_schema_versions() {
    let index_version = index_version("index-v2");
    let source_version = source_version("source-v2");
    let schema_version = schema_version("schema-v2");

    assert_eq!(index_version.id().as_str(), "index-v2");
    assert_eq!(source_version.as_str(), "source-v2");
    assert_eq!(schema_version.as_str(), "schema-v2");

    fn accepts_index_version(_: &IndexVersion) {}
    fn accepts_source_version(_: &SourceVersion) {}
    fn accepts_schema_version(_: &SchemaVersion) {}

    accepts_index_version(&index_version);
    accepts_source_version(&source_version);
    accepts_schema_version(&schema_version);
}

#[test]
fn index_version_is_distinct_from_core_contract_version() {
    let index_version = index_version("1.0.0");
    let contract_version = CoreContractVersion::new(1, 0, 0);

    assert_eq!(index_version.id().as_str(), "1.0.0");
    assert_eq!(contract_version.to_string(), "1.0.0");

    fn accepts_index_version(_: &IndexVersion) {}
    fn accepts_core_contract_version(_: &CoreContractVersion) {}

    accepts_index_version(&index_version);
    accepts_core_contract_version(&contract_version);
}

#[test]
fn object_reference_does_not_become_a_new_index_identity() {
    let index = index_id(0x66);
    let reference = object_reference("quran", "verse:2:255");

    assert_eq!(index.as_bytes(), &[0x66; 64]);
    assert_eq!(reference.source(), "quran");
    assert_eq!(reference.object_reference(), "verse:2:255");

    fn accepts_object_reference(_: &ObjectReference) {}
    accepts_object_reference(&reference);
}

#[test]
fn logical_identity_dimensions_can_coexist_without_being_interchangeable() {
    let namespace = namespace("search.documents");
    let definition = IndexDefinitionIdentity::new(
        definition_id("documents.v1"),
        namespace.clone(),
        IndexFamily::Inverted,
    );
    let index = index_id(0x77);
    let version = index_version("documents-v1");
    let reference = object_reference("documents", "document:77");

    assert_eq!(namespace.as_str(), "search.documents");
    assert_eq!(definition.namespace(), &namespace);
    assert_eq!(definition.family(), IndexFamily::Inverted);
    assert_eq!(index.as_bytes(), &[0x77; 64]);
    assert_eq!(version.id().as_str(), "documents-v1");
    assert_eq!(reference.source(), "documents");
    assert_eq!(reference.object_reference(), "document:77");
}
