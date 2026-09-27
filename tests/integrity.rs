//! Phase 5 integration tests for logical Indexing integrity validation.
//!
//! The tests exercise the public validator and its interaction with the
//! already-established Phase 2/3 logical index contracts.

use nizaam_indexing::{
    ConsistencyRequirement, IndexDefinition, IndexDefinitionId, IndexDefinitionIdentity,
    IndexEntry, IndexFamily, IndexVersion, IndexVersionId, IndexVersionState,
    IntegrityValidationError, IntegrityValidator, KeyDefinition, KeyMaterial, ObjectReference,
    SchemaVersion, SourceVersion, TargetReferenceType, Uniqueness, VersionLifecycle,
    VersioningError, validate_definition, validate_entry, validate_publication_preconditions,
    validate_reference, validate_version, validate_version_compatibility,
};

fn definition(
    target_type: &str,
    source_version: Option<&str>,
    schema_version: Option<&str>,
) -> IndexDefinition {
    IndexDefinition::new(
        IndexDefinitionIdentity::new(
            IndexDefinitionId::new("tests.integrity").expect("definition id should be valid"),
            nizaam_indexing::IndexNamespace::new("integrity").expect("namespace should be valid"),
            IndexFamily::Inverted,
        ),
        KeyDefinition::new(["term"]).expect("key definition should be valid"),
        TargetReferenceType::new(target_type).expect("target reference type should be valid"),
        Uniqueness::NonUnique,
        ConsistencyRequirement::new("logical").expect("consistency requirement should be valid"),
        source_version
            .map(|value| SourceVersion::new(value).expect("source version should be valid")),
        schema_version
            .map(|value| SchemaVersion::new(value).expect("schema version should be valid")),
    )
    .expect("definition should be valid")
}

fn version(
    value: &str,
    source_version: Option<&str>,
    schema_version: Option<&str>,
) -> IndexVersion {
    IndexVersion::with_metadata(
        IndexVersionId::new(value).expect("version id should be valid"),
        source_version
            .map(|value| SourceVersion::new(value).expect("source version should be valid")),
        schema_version
            .map(|value| SchemaVersion::new(value).expect("schema version should be valid")),
        None,
    )
    .expect("version should be valid")
}

fn entry(object: &str) -> IndexEntry {
    IndexEntry::new(
        KeyMaterial::text("term"),
        ObjectReference::new("tests.source", object).expect("object reference should be valid"),
    )
    .expect("entry should be valid")
}

fn ready_state(version: &IndexVersion) -> IndexVersionState {
    let mut state = IndexVersionState::new(version.clone());
    state
        .transition_to(VersionLifecycle::Validating)
        .expect("candidate version should transition to Validating");
    state
        .mark_ready()
        .expect("candidate version should transition to Ready");
    state
}

#[test]
fn valid_definition_version_entry_and_reference_pass_integrity_validation() {
    let definition = definition("source.object", Some("source-v1"), Some("schema-v1"));
    let candidate = version("v1", Some("source-v1"), Some("schema-v1"));
    let entry = entry("object:1");
    let validator = IntegrityValidator::new();

    validator
        .validate_definition(&definition)
        .expect("valid definition should pass");
    validator
        .validate_version(&candidate)
        .expect("valid version should pass");
    validator
        .validate_entry(&entry)
        .expect("valid entry should pass");
    validator
        .validate_reference(entry.target())
        .expect("valid object reference should pass");
}

#[test]
fn complete_index_validation_checks_definition_version_compatibility_and_entries() {
    let definition = definition("source.object", Some("source-v1"), Some("schema-v1"));
    let candidate = version("v1", Some("source-v1"), Some("schema-v1"));
    let entries = [entry("object:1"), entry("object:2")];

    IntegrityValidator::new()
        .validate_index(&definition, &candidate, entries.iter())
        .expect("complete logical index should satisfy integrity validation");
}

#[test]
fn entry_batch_validation_accepts_multiple_structurally_valid_entries() {
    let entries = [entry("object:1"), entry("object:2"), entry("object:3")];

    IntegrityValidator::new()
        .validate_entries(entries.iter())
        .expect("all valid entries should pass batch validation");
}

#[test]
fn matching_reference_type_contracts_are_accepted() {
    let expected =
        TargetReferenceType::new("source.object").expect("reference type should be valid");
    let actual = TargetReferenceType::new("source.object").expect("reference type should be valid");

    IntegrityValidator::new()
        .validate_reference_type(&expected, &actual)
        .expect("equal reference-type contracts should match");
}

#[test]
fn mismatched_reference_type_contracts_are_rejected() {
    let expected =
        TargetReferenceType::new("source.object").expect("reference type should be valid");
    let actual =
        TargetReferenceType::new("source.relationship").expect("reference type should be valid");

    let error = IntegrityValidator::new()
        .validate_reference_type(&expected, &actual)
        .expect_err("different reference types must be rejected");

    assert!(matches!(
        error,
        IntegrityValidationError::ReferenceTypeMismatch {
            expected: ref expected_error,
            actual: ref actual_error,
        } if expected_error == &expected && actual_error == &actual
    ));
}

#[test]
fn source_version_mismatch_is_rejected_at_integrity_boundary() {
    let definition = definition("source.object", Some("source-v2"), None);
    let candidate = version("candidate-v1", Some("source-v1"), None);

    let error = validate_version_compatibility(&candidate, &definition)
        .expect_err("source-version mismatch must be rejected");

    assert!(matches!(
        error,
        IntegrityValidationError::VersionCompatibility(
            VersioningError::SourceVersionMismatch { .. }
        )
    ));
}

#[test]
fn schema_version_mismatch_is_rejected_at_integrity_boundary() {
    let definition = definition("source.object", None, Some("schema-v2"));
    let candidate = version("candidate-v1", None, Some("schema-v1"));

    let error = validate_version_compatibility(&candidate, &definition)
        .expect_err("schema-version mismatch must be rejected");

    assert!(matches!(
        error,
        IntegrityValidationError::VersionCompatibility(
            VersioningError::SchemaVersionMismatch { .. }
        )
    ));
}

#[test]
fn publication_requires_candidate_ready_state() {
    let definition = definition("source.object", None, None);
    let candidate = version("candidate-v1", None, None);
    let state = IndexVersionState::new(candidate.clone());

    let error = validate_publication_preconditions(&candidate, &state, &definition, None, None)
        .expect_err("non-ready candidate must not be publication eligible");

    assert!(matches!(
        error,
        IntegrityValidationError::CandidateNotReady {
            lifecycle: VersionLifecycle::Building,
            ..
        }
    ));
}

#[test]
fn stale_candidate_lineage_is_rejected_before_publication() {
    let definition = definition("source.object", None, None);
    let candidate = version("candidate-v3", None, None);
    let base = IndexVersionId::new("active-v1").expect("base version should be valid");
    let active = IndexVersionId::new("active-v2").expect("active version should be valid");

    let error = validate_publication_preconditions(
        &candidate,
        &ready_state(&candidate),
        &definition,
        Some(&base),
        Some(&active),
    )
    .expect_err("stale candidate lineage must be rejected");

    assert!(matches!(
        error,
        IntegrityValidationError::VersionCompatibility(VersioningError::StaleCandidate { .. })
    ));
}

#[test]
fn ready_candidate_with_valid_initial_lineage_is_publication_eligible() {
    let definition = definition("source.object", None, None);
    let candidate = version("candidate-v1", None, None);

    validate_publication_preconditions(
        &candidate,
        &ready_state(&candidate),
        &definition,
        None,
        None,
    )
    .expect("initial ready candidate should satisfy publication preconditions");
}

#[test]
fn free_function_helpers_match_the_validator_boundary() {
    let definition = definition("source.object", Some("source-v1"), Some("schema-v1"));
    let candidate = version("candidate-v1", Some("source-v1"), Some("schema-v1"));
    let entry = entry("object:1");

    validate_definition(&definition).expect("free definition helper should pass");
    validate_version(&candidate).expect("free version helper should pass");
    validate_entry(&entry).expect("free entry helper should pass");
    validate_reference(entry.target()).expect("free reference helper should pass");
    validate_version_compatibility(&candidate, &definition)
        .expect("free compatibility helper should pass");

    let validator = IntegrityValidator::new();
    validator
        .validate_definition(&definition)
        .expect("validator definition should pass");
    validator
        .validate_version(&candidate)
        .expect("validator version should pass");
    validator
        .validate_entry(&entry)
        .expect("validator entry should pass");
    validator
        .validate_reference(entry.target())
        .expect("validator reference should pass");
    validator
        .validate_version_compatibility(&candidate, &definition)
        .expect("validator compatibility should pass");
}

#[test]
fn integrity_validation_does_not_require_source_engine_lookup() {
    let reference = ObjectReference::new("external.source", "opaque-object:42")
        .expect("opaque source reference should be valid");

    validate_reference(&reference)
        .expect("structural reference validation must not require source access");
}

#[test]
fn candidate_state_identity_must_match_candidate_version() {
    let definition = definition("source.object", None, None);
    let candidate = version("candidate-v1", None, None);
    let different_version = version("candidate-v2", None, None);
    let state = ready_state(&different_version);

    let error = validate_publication_preconditions(&candidate, &state, &definition, None, None)
        .expect_err("candidate and candidate-state version identities must match");

    assert!(matches!(
        error,
        IntegrityValidationError::CandidateStateMismatch { .. }
    ));
}
