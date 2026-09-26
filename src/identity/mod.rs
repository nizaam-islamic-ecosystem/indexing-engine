//! Public identity boundary for the Phase 1 + Phase 2 Indexing model.
//!
//! This module composes the identity implementation files:
//!
//! - [`index`] defines `IndexId` and the frozen Phase 2 generation scheme.
//! - [`namespace`] defines `IndexNamespace` and `NamespaceRegistry`.
//! - [`definition`] defines `IndexDefinitionId` and `IndexDefinitionIdentity`.
//!
//! The module-level boundary intentionally contains no second identity model.
//! Its tests exercise interactions between the identity types owned by the
//! child modules, complementing the file-local unit tests.

pub mod definition;
pub mod index;
pub mod namespace;

pub use definition::{
    IndexDefinitionId, IndexDefinitionIdValidationError, IndexDefinitionIdentity,
};
pub use index::{
    INDEX_ID_BIT_LEN, INDEX_ID_BYTE_LEN, IndexId, IndexIdGenerationError, IndexIdGenerationVersion,
};
pub use namespace::{
    IndexNamespace, MAX_NAMESPACE_BYTES, NamespaceRegistry, NamespaceRegistryError,
    NamespaceValidationError,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::{IndexFamily, KeyMaterial};

    fn index_id(seed: u8) -> IndexId {
        let bytes = core::array::from_fn(|offset| seed.wrapping_add(offset as u8));
        IndexId::from_bytes(bytes)
    }

    fn namespace(value: &str) -> IndexNamespace {
        IndexNamespace::new(value).expect("test namespace must be valid")
    }

    fn definition_id(value: &str) -> IndexDefinitionId {
        IndexDefinitionId::new(value).expect("test definition ID must be valid")
    }

    #[test]
    fn identity_boundary_reexports_all_phase_one_identity_types() {
        let index = index_id(1);
        let namespace = namespace("lexical");
        let definition_id = definition_id("lexical.v1");

        let definition = IndexDefinitionIdentity::new(
            definition_id.clone(),
            namespace.clone(),
            IndexFamily::Inverted,
        );

        assert_eq!(index.as_bytes().len(), INDEX_ID_BYTE_LEN);
        assert_eq!(INDEX_ID_BIT_LEN, 512);
        assert_eq!(namespace.as_str(), "lexical");
        assert_eq!(definition.definition_id(), &definition_id);
        assert_eq!(definition.namespace(), &namespace);
        assert_eq!(definition.family(), IndexFamily::Inverted);
    }

    #[test]
    fn identity_boundary_reexports_phase_two_index_id_generation_contract() {
        let namespace = namespace("quran.text");
        let definition = definition_id("verse-term");
        let key = KeyMaterial::text("lemma");

        let version = IndexIdGenerationVersion::CURRENT;
        assert_eq!(version.value(), 1);
        assert_eq!(IndexId::generation_version(), version);

        let generated =
            IndexId::generate(&namespace, &definition, IndexFamily::Inverted, &key)
            .expect("generation should succeed");

        assert_eq!(generated.as_bytes().len(), INDEX_ID_BYTE_LEN);
    }

    #[test]
    fn namespace_registry_and_definition_identity_compose_without_physical_semantics() {
        let mut registry = NamespaceRegistry::new();

        let lexical = namespace("lexical");
        let relationship = namespace("kg.relationship");

        registry
            .register(lexical.clone())
            .expect("lexical namespace registration should succeed");
        registry
            .register(relationship.clone())
            .expect("relationship namespace registration should succeed");

        let lexical_definition = IndexDefinitionIdentity::new(
            definition_id("lexical.v1"),
            lexical.clone(),
            IndexFamily::Inverted,
        );
        let relationship_definition = IndexDefinitionIdentity::new(
            definition_id("relationship.v1"),
            relationship.clone(),
            IndexFamily::Relationship,
        );

        assert!(registry.contains(&lexical));
        assert!(registry.contains(&relationship));
        assert_ne!(lexical_definition, relationship_definition);
        assert_eq!(registry.snapshot().len(), 2);
    }

    #[test]
    fn one_definition_can_use_different_logical_spaces_without_merging_namespace_identity() {
        let definition_id = definition_id("shared-definition");

        let lexical = IndexDefinitionIdentity::new(
            definition_id.clone(),
            namespace("lexical"),
            IndexFamily::Inverted,
        );
        let identity = IndexDefinitionIdentity::new(
            definition_id,
            namespace("identity"),
            IndexFamily::Identity,
        );

        assert_ne!(lexical, identity);
        assert_eq!(lexical.definition_id(), identity.definition_id());
        assert_ne!(lexical.namespace(), identity.namespace());
        assert_ne!(lexical.family(), identity.family());
    }

    #[test]
    fn index_id_remains_distinct_from_definition_identity() {
        let index = index_id(9);
        let definition = IndexDefinitionIdentity::new(
            definition_id("definition.v1"),
            namespace("lexical"),
            IndexFamily::Inverted,
        );

        let _ = index;

        // The two types intentionally have different roles and representations.
        // This test uses their separate access paths rather than introducing a
        // conversion between them.
        assert_eq!(IndexId::byte_len(), 64);
        assert_eq!(definition.family(), IndexFamily::Inverted);
    }

    #[test]
    fn namespace_registry_snapshot_preserves_logical_identity_values() {
        let mut registry = NamespaceRegistry::new();

        let first = namespace("alpha");
        let second = namespace("beta");

        registry
            .register(first.clone())
            .expect("first namespace registration should succeed");
        registry
            .register(second.clone())
            .expect("second namespace registration should succeed");

        assert_eq!(registry.snapshot(), vec![first, second]);
    }

    #[test]
    fn definition_identity_keeps_family_as_indexing_vocabulary() {
        let definition = IndexDefinitionIdentity::new(
            definition_id("relationship-index"),
            namespace("kg.relationship"),
            IndexFamily::Relationship,
        );

        assert_eq!(definition.family(), IndexFamily::Relationship);
        assert_eq!(definition.namespace().as_str(), "kg.relationship");
    }
}
