//! Level 3 integration tests for Phase 1 identity primitives.
//!
//! These tests verify the public identity API as a consumer would use it.
//! They intentionally remain limited to the Phase 1 identity boundaries:
//! `IndexId`, `IndexNamespace`, `NamespaceRegistry`, `IndexDefinitionId`,
//! `IndexDefinitionIdentity`, and `IndexFamily`.

use nizaam_indexing::identity::{
    IndexDefinitionId, IndexDefinitionIdentity, IndexId, IndexNamespace, NamespaceRegistry,
};
use nizaam_indexing::index::IndexFamily;

fn index_id(seed: u8) -> IndexId {
    IndexId::from_bytes([seed; 64])
}

fn namespace(value: &str) -> IndexNamespace {
    IndexNamespace::new(value).expect("test namespace must satisfy Phase 1 grammar")
}

fn definition_id(value: &str) -> IndexDefinitionId {
    IndexDefinitionId::new(value).expect("test definition ID must be valid")
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

    // `IndexId` and `IndexDefinitionIdentity` are separate public identity
    // concepts. The integration test intentionally keeps them as separate
    // values rather than introducing any conversion or coupling.
    assert_eq!(index.as_bytes(), &[0x22; 64]);
    assert_eq!(definition.definition_id().as_str(), "definition-a");
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
fn one_identity_can_be_associated_with_multiple_logical_spaces() {
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

    // The concrete index identity remains its own Phase 1 concept and does
    // not become a namespace or definition identity.
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

    // No Phase 1 conversion or equality relationship is introduced between
    // `IndexId` and `IndexDefinitionIdentity`.
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
