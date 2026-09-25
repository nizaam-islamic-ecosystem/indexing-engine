//! Repository-level integration tests for the Phase 2 `IndexRequirement` contract.
//!
//! These tests exercise the public consumer-facing requirement boundary rather
//! than implementation details. They verify that a source engine can express
//! logical indexing needs, that invalid logical inputs are rejected at their
//! owned contract boundaries, and that a valid requirement can be normalized
//! into the canonical logical `IndexDefinition` without introducing physical
//! provider configuration.

use nizaam_indexing::identity::{IndexDefinitionId, IndexNamespace};
use nizaam_indexing::index::{
    ConsistencyRequirement, IndexDefinition, IndexFamily, KeyDefinition, SchemaVersion,
    SourceVersion, TargetReferenceType, Uniqueness,
};
use nizaam_indexing::requirement::IndexRequirement;

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

fn requirement() -> IndexRequirement {
    IndexRequirement::new(
        namespace("source.lexical"),
        IndexFamily::Inverted,
        key_definition(&["term"]),
        target_reference_type("source.object"),
        Uniqueness::NonUnique,
        consistency("logical-v1"),
        Some(source_version("source-v3")),
        Some(schema_version("schema-v2")),
    )
    .expect("test requirement should be valid")
}

#[test]
fn source_engine_can_express_a_valid_logical_indexing_requirement() {
    let requirement = requirement();

    assert_eq!(requirement.namespace().as_str(), "source.lexical");
    assert_eq!(requirement.family(), IndexFamily::Inverted);
    assert_eq!(requirement.key_definition().len(), 1);
    assert_eq!(
        requirement.target_reference_type().as_str(),
        "source.object"
    );
    assert_eq!(requirement.uniqueness(), Uniqueness::NonUnique);
    assert_eq!(requirement.consistency_requirement().as_str(), "logical-v1");
    assert_eq!(
        requirement
            .source_version()
            .expect("source version")
            .as_str(),
        "source-v3"
    );
    assert_eq!(
        requirement
            .schema_version()
            .expect("schema version")
            .as_str(),
        "schema-v2"
    );
}

#[test]
fn requirement_validation_is_deterministic() {
    let requirement = requirement();

    assert_eq!(requirement.validate(), Ok(()));
    assert_eq!(requirement.validate(), Ok(()));
}

#[test]
fn invalid_logical_inputs_are_rejected_before_requirement_construction() {
    assert!(IndexNamespace::new("").is_err());
    assert!(IndexDefinitionId::new("").is_err());

    assert!(KeyDefinition::new(Vec::<String>::new()).is_err());
    assert!(KeyDefinition::new(["term", "term"]).is_err());

    assert!(TargetReferenceType::new("").is_err());
    assert!(TargetReferenceType::new("source\nobject").is_err());

    assert!(ConsistencyRequirement::new("").is_err());
    assert!(ConsistencyRequirement::new("logical\tv1").is_err());

    assert!(SourceVersion::new("").is_err());
    assert!(SchemaVersion::new("").is_err());
}

#[test]
fn all_index_families_can_be_expressed_without_semantic_specialization() {
    for family in [
        IndexFamily::Identity,
        IndexFamily::Inverted,
        IndexFamily::Relationship,
        IndexFamily::Similarity,
    ] {
        let requirement = IndexRequirement::new(
            namespace("source.generic"),
            family,
            key_definition(&["key"]),
            target_reference_type("source.object"),
            Uniqueness::NonUnique,
            consistency("logical"),
            None,
            None,
        )
        .expect("all generic index families should be expressible");

        assert_eq!(requirement.family(), family);
    }
}

#[test]
fn requirement_normalizes_into_a_canonical_logical_definition() {
    let requirement = requirement();

    let definition = requirement
        .normalize(definition_id("lexical.v1"))
        .expect("valid requirement should normalize");

    assert_eq!(definition.definition_id().as_str(), "lexical.v1");
    assert_eq!(definition.namespace(), requirement.namespace());
    assert_eq!(definition.family(), requirement.family());
    assert_eq!(definition.key_definition(), requirement.key_definition());
    assert_eq!(
        definition.target_reference_type(),
        requirement.target_reference_type()
    );
    assert_eq!(definition.uniqueness(), requirement.uniqueness());
    assert_eq!(
        definition.consistency_requirement(),
        requirement.consistency_requirement()
    );
}

#[test]
fn normalization_preserves_source_and_schema_version_compatibility_metadata() {
    let requirement = requirement();

    let definition = requirement
        .normalize(definition_id("lexical.compatibility"))
        .expect("valid requirement should normalize");

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
}

#[test]
fn requirement_remains_distinct_from_definition_identity() {
    let requirement = requirement();

    let first = requirement
        .normalize(definition_id("lexical.v1"))
        .expect("first normalization should succeed");
    let second = requirement
        .normalize(definition_id("lexical.v2"))
        .expect("second normalization should succeed");

    assert_eq!(first.namespace(), second.namespace());
    assert_eq!(first.family(), second.family());
    assert_ne!(first.definition_id(), second.definition_id());

    // The requirement itself does not carry an IndexDefinitionId. A definition
    // identifier is supplied only at the normalization boundary.
    assert_eq!(requirement.namespace().as_str(), "source.lexical");
}

#[test]
fn requirement_is_provider_neutral() {
    let requirement = requirement();

    // The public requirement can be created entirely from logical concepts.
    // No physical provider, storage engine, partition, shard, database table,
    // index algorithm, or embedding configuration is required.
    assert_eq!(requirement.family(), IndexFamily::Inverted);
    assert_eq!(requirement.key_definition().fields()[0].name(), "term");
    assert_eq!(
        requirement.target_reference_type().as_str(),
        "source.object"
    );
}

#[test]
fn relationship_family_does_not_create_semantic_predicate_ownership() {
    let requirement = IndexRequirement::new(
        namespace("source.relationships"),
        IndexFamily::Relationship,
        key_definition(&["source", "predicate", "target"]),
        target_reference_type("source.relationship"),
        Uniqueness::NonUnique,
        consistency("logical"),
        None,
        None,
    )
    .expect("relationship indexing requirement should be valid");

    assert_eq!(requirement.family(), IndexFamily::Relationship);

    // The requirement preserves generic key fields only. It does not expose
    // a predicate-specific API or assign meaning to the "predicate" field.
    assert_eq!(requirement.key_definition().fields()[1].name(), "predicate");
}

#[test]
fn into_parts_round_trips_the_source_requirement() {
    let requirement = requirement();

    let (
        namespace,
        family,
        key_definition,
        target_reference_type,
        uniqueness,
        consistency_requirement,
        source_version,
        schema_version,
    ) = requirement.into_parts();

    assert_eq!(namespace.as_str(), "source.lexical");
    assert_eq!(family, IndexFamily::Inverted);
    assert_eq!(key_definition.fields()[0].name(), "term");
    assert_eq!(target_reference_type.as_str(), "source.object");
    assert_eq!(uniqueness, Uniqueness::NonUnique);
    assert_eq!(consistency_requirement.as_str(), "logical-v1");
    assert_eq!(
        source_version.expect("source version").as_str(),
        "source-v3"
    );
    assert_eq!(
        schema_version.expect("schema version").as_str(),
        "schema-v2"
    );
}

#[test]
fn normalized_value_is_the_canonical_index_definition_type() {
    let requirement = requirement();

    let definition = requirement
        .normalize(definition_id("lexical.v1"))
        .expect("normalization should succeed");

    fn accepts_definition(_: &IndexDefinition) {}

    accepts_definition(&definition);
}
