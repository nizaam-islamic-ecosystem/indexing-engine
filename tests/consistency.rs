//! Level 3 integration coverage for Phase 3 version consistency rules.

use nizaam_indexing::consistency::{
    VersioningError, is_candidate_stale, validate_active_version_compatibility,
    validate_candidate_compatibility, validate_candidate_lineage,
    validate_candidate_schema_compatibility, validate_candidate_source_compatibility,
    validate_publication_eligibility,
};
use nizaam_indexing::identity::{IndexDefinitionId, IndexDefinitionIdentity, IndexNamespace};
use nizaam_indexing::index::{
    ConsistencyRequirement, IndexDefinition, IndexFamily, IndexVersion, IndexVersionId,
    KeyDefinition, SchemaVersion, SourceVersion, TargetReferenceType, Uniqueness,
};

fn version_id(value: &str) -> IndexVersionId {
    IndexVersionId::new(value).expect("test version ID should be valid")
}

fn source_version(value: &str) -> SourceVersion {
    SourceVersion::new(value).expect("test source version should be valid")
}

fn schema_version(value: &str) -> SchemaVersion {
    SchemaVersion::new(value).expect("test schema version should be valid")
}

fn version(value: &str, source: Option<&str>, schema: Option<&str>) -> IndexVersion {
    IndexVersion::with_metadata(
        version_id(value),
        source.map(source_version),
        schema.map(schema_version),
        None,
    )
    .expect("test version should be valid")
}

fn definition(source: Option<&str>, schema: Option<&str>) -> IndexDefinition {
    IndexDefinition::new(
        IndexDefinitionIdentity::new(
            IndexDefinitionId::new("phase3.consistency.documents")
                .expect("definition ID should be valid"),
            IndexNamespace::new("phase3.consistency").expect("namespace should be valid"),
            IndexFamily::Inverted,
        ),
        KeyDefinition::new(["term"]).expect("key definition should be valid"),
        TargetReferenceType::new("documents.document")
            .expect("target reference type should be valid"),
        Uniqueness::NonUnique,
        ConsistencyRequirement::new("logical-v1").expect("consistency requirement should be valid"),
        source.map(source_version),
        schema.map(schema_version),
    )
    .expect("definition should be valid")
}

#[test]
fn source_version_mismatch_is_rejected_without_interpreting_version_order() {
    let candidate = version("candidate-v1", Some("source-v2"), Some("schema-v1"));
    let expected = source_version("source-v1");

    let error = validate_candidate_source_compatibility(&candidate, Some(&expected))
        .expect_err("different source versions must be rejected");

    assert!(matches!(
        error,
        VersioningError::SourceVersionMismatch { .. }
    ));
}

#[test]
fn schema_version_mismatch_is_rejected_exactly() {
    let candidate = version("candidate-v1", Some("source-v1"), Some("schema-v2"));
    let expected = schema_version("schema-v1");

    let error = validate_candidate_schema_compatibility(&candidate, Some(&expected))
        .expect_err("different schema versions must be rejected");

    assert!(matches!(
        error,
        VersioningError::SchemaVersionMismatch { .. }
    ));
}

#[test]
fn candidate_lineage_is_exactly_anchored_to_the_observed_active_version() {
    let candidate = version("candidate-v2", Some("source-v1"), Some("schema-v1"));
    let active = version_id("active-v1");

    validate_candidate_lineage(&candidate, Some(&active), Some(&active))
        .expect("matching base and active versions should be accepted");

    let newer_active = version_id("active-v2");
    let error = validate_candidate_lineage(&candidate, Some(&active), Some(&newer_active))
        .expect_err("a candidate based on an older active version must be stale");

    assert_eq!(
        error,
        VersioningError::StaleCandidate {
            candidate: version_id("candidate-v2"),
            base_version: Some(active),
            active_version: Some(newer_active),
        }
    );
}

#[test]
fn initial_candidate_requires_an_empty_lineage_before_first_publication() {
    let candidate = version("candidate-v1", None, None);

    assert!(!is_candidate_stale(None, None));
    validate_candidate_lineage(&candidate, None, None)
        .expect("an initial candidate should be valid without a predecessor");

    let active = version_id("active-v1");
    assert!(is_candidate_stale(None, Some(&active)));
    assert!(matches!(
        validate_candidate_lineage(&candidate, None, Some(&active)),
        Err(VersioningError::StaleCandidate { .. })
    ));
}

#[test]
fn multiple_candidates_can_be_valid_against_the_same_active_version_independently() {
    let active = version_id("active-v1");
    let candidate_a = version("candidate-v2", Some("source-v1"), Some("schema-v1"));
    let candidate_b = version("candidate-v3", Some("source-v1"), Some("schema-v1"));

    validate_active_version_compatibility(&candidate_a, Some(&active), Some(&active))
        .expect("first candidate should remain independently valid");
    validate_active_version_compatibility(&candidate_b, Some(&active), Some(&active))
        .expect("second candidate should remain independently valid");
}

#[test]
fn already_active_version_is_not_a_publication_target() {
    let active = version_id("active-v2");
    let candidate = version("active-v2", Some("source-v1"), Some("schema-v1"));

    let error = validate_active_version_compatibility(&candidate, Some(&active), Some(&active))
        .expect_err("the current active version cannot be republished as a candidate");

    assert_eq!(
        error,
        VersioningError::AlreadyActive {
            candidate: version_id("active-v2"),
        }
    );
}

#[test]
fn publication_eligibility_requires_both_metadata_and_lineage_to_match() {
    let active = version_id("active-v1");
    let candidate = version("candidate-v2", Some("source-v1"), Some("schema-v1"));
    let definition = definition(Some("source-v1"), Some("schema-v1"));

    validate_candidate_compatibility(&candidate, &definition)
        .expect("candidate metadata should satisfy the definition");
    validate_publication_eligibility(&candidate, &definition, Some(&active), Some(&active))
        .expect("matching metadata and lineage should be publication eligible");

    let newer_active = version_id("active-v2");
    assert!(matches!(
        validate_publication_eligibility(
            &candidate,
            &definition,
            Some(&active),
            Some(&newer_active),
        ),
        Err(VersioningError::StaleCandidate { .. })
    ));
}
