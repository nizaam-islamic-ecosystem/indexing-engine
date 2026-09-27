//! Version-level consistency rules for Phase 3 index construction and publication.
//!
//! This module owns the logical relationship between:
//! - a candidate [`IndexVersion`];
//! - the source/schema versions declared by an [`IndexDefinition`]; and
//! - the active version from which a candidate was derived.
//!
//! It deliberately does **not** own candidate lifecycle state, query
//! consistency, provider/storage behavior, or the READY → ACTIVE transition.
//! Those responsibilities remain in the later Phase 3 lifecycle/build modules.

use crate::error::IndexingResult;
use crate::index::{IndexDefinition, IndexVersion, IndexVersionId, SchemaVersion, SourceVersion};
use core::fmt;
use nizaam_core::contracts::Version as CoreVersion;
use nizaam_core::error::{
    ErrorClass, ErrorCode, ErrorContext, ErrorEvent, ErrorOwner, GlobalError, Severity,
};
use nizaam_core::status::Retryability;

/// Errors produced by Phase 3 version-consistency checks.
///
/// These errors describe logical version relationships only. They do not
/// prescribe retry policy, storage behavior, query behavior, or provider
/// implementation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum VersioningError {
    /// The candidate does not contain the source version required by its
    /// logical definition.
    MissingCandidateSourceVersion {
        /// Candidate logical version identity.
        candidate: IndexVersionId,

        /// Source version required by the definition.
        expected: SourceVersion,
    },

    /// The candidate source version differs from the definition's required
    /// source version.
    SourceVersionMismatch {
        /// Candidate logical version identity.
        candidate: IndexVersionId,

        /// Source version required by the definition.
        expected: SourceVersion,

        /// Source version carried by the candidate.
        actual: SourceVersion,
    },

    /// The candidate does not contain the schema version required by its
    /// logical definition.
    MissingCandidateSchemaVersion {
        /// Candidate logical version identity.
        candidate: IndexVersionId,

        /// Schema version required by the definition.
        expected: SchemaVersion,
    },

    /// The candidate schema version differs from the definition's required
    /// schema version.
    SchemaVersionMismatch {
        /// Candidate logical version identity.
        candidate: IndexVersionId,

        /// Schema version required by the definition.
        expected: SchemaVersion,

        /// Schema version carried by the candidate.
        actual: SchemaVersion,
    },

    /// The candidate's base lineage no longer matches the currently active
    /// version.
    ///
    /// `None` is meaningful: an initial candidate with no active predecessor
    /// must also have no base version. Once an active version exists, a
    /// candidate must explicitly identify that active version as its base.
    StaleCandidate {
        /// Candidate logical version identity.
        candidate: IndexVersionId,

        /// Version from which the candidate was derived, if any.
        base_version: Option<IndexVersionId>,

        /// Version currently active, if any.
        active_version: Option<IndexVersionId>,
    },

    /// The candidate version is already the active version.
    AlreadyActive {
        /// Candidate logical version identity.
        candidate: IndexVersionId,
    },

    /// The candidate's underlying logical version metadata failed its
    /// structural validation.
    InvalidCandidate(crate::index::IndexVersionValidationError),

    /// The index definition failed its structural validation before version
    /// compatibility could be evaluated.
    InvalidDefinition(crate::index::IndexDefinitionValidationError),
}

impl fmt::Display for VersioningError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingCandidateSourceVersion {
                candidate,
                expected,
            } => write!(
                formatter,
                "candidate {candidate} is missing required source version {expected}"
            ),
            Self::SourceVersionMismatch {
                candidate,
                expected,
                actual,
            } => write!(
                formatter,
                "candidate {candidate} has source version {actual}, expected {expected}"
            ),
            Self::MissingCandidateSchemaVersion {
                candidate,
                expected,
            } => write!(
                formatter,
                "candidate {candidate} is missing required schema version {expected}"
            ),
            Self::SchemaVersionMismatch {
                candidate,
                expected,
                actual,
            } => write!(
                formatter,
                "candidate {candidate} has schema version {actual}, expected {expected}"
            ),
            Self::StaleCandidate {
                candidate,
                base_version,
                active_version,
            } => write!(
                formatter,
                "candidate {candidate} is stale: base version {base_version:?} does not \
                 match active version {active_version:?}"
            ),
            Self::AlreadyActive { candidate } => {
                write!(
                    formatter,
                    "candidate {candidate} is already the active version"
                )
            }
            Self::InvalidCandidate(error) => {
                write!(formatter, "invalid candidate index version: {error}")
            }
            Self::InvalidDefinition(error) => {
                write!(formatter, "invalid index definition: {error}")
            }
        }
    }
}

impl std::error::Error for VersioningError {}

/// Validates a candidate against the source-version requirement of an
/// [`IndexDefinition`].
///
/// The comparison is intentionally opaque. Indexing does not interpret source
/// version strings or assume that one value is newer than another.
///
/// If the definition does not specify a source version, the candidate's
/// source version is not restricted by this check.
pub fn validate_candidate_source_compatibility(
    candidate: &IndexVersion,
    expected_source_version: Option<&SourceVersion>,
) -> Result<(), VersioningError> {
    candidate
        .validate()
        .map_err(VersioningError::InvalidCandidate)?;

    let Some(expected) = expected_source_version else {
        return Ok(());
    };

    match candidate.source_version() {
        Some(actual) if actual == expected => Ok(()),
        Some(actual) => Err(VersioningError::SourceVersionMismatch {
            candidate: candidate.id().clone(),
            expected: expected.clone(),
            actual: actual.clone(),
        }),
        None => Err(VersioningError::MissingCandidateSourceVersion {
            candidate: candidate.id().clone(),
            expected: expected.clone(),
        }),
    }
}

/// Validates a candidate against the schema-version requirement of an
/// [`IndexDefinition`].
///
/// The comparison is intentionally opaque. Schema-version semantics remain
/// owned by the source/contract owner; Indexing only requires exact equality
/// when the definition declares a schema version.
pub fn validate_candidate_schema_compatibility(
    candidate: &IndexVersion,
    expected_schema_version: Option<&SchemaVersion>,
) -> Result<(), VersioningError> {
    candidate
        .validate()
        .map_err(VersioningError::InvalidCandidate)?;

    let Some(expected) = expected_schema_version else {
        return Ok(());
    };

    match candidate.schema_version() {
        Some(actual) if actual == expected => Ok(()),
        Some(actual) => Err(VersioningError::SchemaVersionMismatch {
            candidate: candidate.id().clone(),
            expected: expected.clone(),
            actual: actual.clone(),
        }),
        None => Err(VersioningError::MissingCandidateSchemaVersion {
            candidate: candidate.id().clone(),
            expected: expected.clone(),
        }),
    }
}

/// Validates all version metadata that the supplied [`IndexDefinition`] places
/// on a candidate.
///
/// This is the common source/schema compatibility boundary used by later
/// construction and publication workflows.
pub fn validate_candidate_compatibility(
    candidate: &IndexVersion,
    definition: &IndexDefinition,
) -> Result<(), VersioningError> {
    definition
        .validate()
        .map_err(VersioningError::InvalidDefinition)?;

    validate_candidate_source_compatibility(candidate, definition.source_version())?;
    validate_candidate_schema_compatibility(candidate, definition.schema_version())?;

    Ok(())
}

/// Returns whether a candidate's declared base is stale relative to the
/// currently active version.
///
/// The rule is deliberately exact:
///
/// ```text
/// candidate base == active version
///     → not stale
///
/// candidate base != active version
///     → stale
/// ```
///
/// `None` is part of the comparison. Therefore an initial candidate is valid
/// only while no active version exists.
#[must_use]
pub fn is_candidate_stale(
    base_version: Option<&IndexVersionId>,
    active_version: Option<&IndexVersionId>,
) -> bool {
    base_version != active_version
}

/// Validates that the candidate is still lineaged from the currently active
/// version.
///
/// This is the key publication-safety rule for the Phase 3 multiple-candidate
/// model. A candidate derived from an older active version cannot later replace
/// a newer active version merely because it became ready first/last.
pub fn validate_candidate_lineage(
    candidate: &IndexVersion,
    base_version: Option<&IndexVersionId>,
    active_version: Option<&IndexVersionId>,
) -> Result<(), VersioningError> {
    candidate
        .validate()
        .map_err(VersioningError::InvalidCandidate)?;

    if is_candidate_stale(base_version, active_version) {
        return Err(VersioningError::StaleCandidate {
            candidate: candidate.id().clone(),
            base_version: base_version.cloned(),
            active_version: active_version.cloned(),
        });
    }

    Ok(())
}

/// Validates candidate compatibility with the active-version publication
/// boundary.
///
/// This is intentionally version-level logic only. It does not inspect or
/// enforce the candidate lifecycle state; the publication/build layers own
/// lifecycle transitions.
///
/// A candidate that already is the active version is rejected as a publication
/// target, while an otherwise matching lineage is accepted.
pub fn validate_active_version_compatibility(
    candidate: &IndexVersion,
    base_version: Option<&IndexVersionId>,
    active_version: Option<&IndexVersionId>,
) -> Result<(), VersioningError> {
    validate_candidate_lineage(candidate, base_version, active_version)?;

    if active_version == Some(candidate.id()) {
        return Err(VersioningError::AlreadyActive {
            candidate: candidate.id().clone(),
        });
    }

    Ok(())
}

/// Performs every version-level check that must succeed before a candidate can
/// be considered eligible for publication.
///
/// This function does **not**:
/// - decide whether the candidate is in the READY lifecycle state;
/// - perform the READY → ACTIVE transition;
/// - persist or switch physical provider state;
/// - implement query consistency;
/// - introduce retry policy.
///
/// Those concerns belong to their owning Phase 3/4 mechanisms.
pub fn validate_publication_eligibility(
    candidate: &IndexVersion,
    definition: &IndexDefinition,
    base_version: Option<&IndexVersionId>,
    active_version: Option<&IndexVersionId>,
) -> Result<(), VersioningError> {
    validate_candidate_compatibility(candidate, definition)?;
    validate_active_version_compatibility(candidate, base_version, active_version)?;

    Ok(())
}

impl VersioningError {
    /// Converts the logical versioning failure into the shared Core error
    /// contract using the caller-supplied execution context.
    #[must_use]
    pub fn into_global_error(self, context: ErrorContext) -> GlobalError {
        let (code, class, message, details) = match self {
            Self::MissingCandidateSourceVersion {
                candidate,
                expected,
            } => (
                "INDEXING.VERSIONING.001",
                ErrorClass::Contract,
                format!("candidate {candidate} is missing required source version {expected}"),
                vec![
                    ("candidate_version", candidate.to_string()),
                    ("expected_source_version", expected.to_string()),
                ],
            ),
            Self::SourceVersionMismatch {
                candidate,
                expected,
                actual,
            } => (
                "INDEXING.VERSIONING.002",
                ErrorClass::Contract,
                format!("candidate {candidate} has source version {actual}, expected {expected}"),
                vec![
                    ("candidate_version", candidate.to_string()),
                    ("expected_source_version", expected.to_string()),
                    ("actual_source_version", actual.to_string()),
                ],
            ),
            Self::MissingCandidateSchemaVersion {
                candidate,
                expected,
            } => (
                "INDEXING.VERSIONING.003",
                ErrorClass::Contract,
                format!("candidate {candidate} is missing required schema version {expected}"),
                vec![
                    ("candidate_version", candidate.to_string()),
                    ("expected_schema_version", expected.to_string()),
                ],
            ),
            Self::SchemaVersionMismatch {
                candidate,
                expected,
                actual,
            } => (
                "INDEXING.VERSIONING.004",
                ErrorClass::Contract,
                format!("candidate {candidate} has schema version {actual}, expected {expected}"),
                vec![
                    ("candidate_version", candidate.to_string()),
                    ("expected_schema_version", expected.to_string()),
                    ("actual_schema_version", actual.to_string()),
                ],
            ),
            Self::StaleCandidate {
                candidate,
                base_version,
                active_version,
            } => (
                "INDEXING.VERSIONING.005",
                ErrorClass::Contract,
                format!(
                    "candidate {candidate} is stale: base version {base_version:?} does not \
                     match active version {active_version:?}"
                ),
                vec![
                    ("candidate_version", candidate.to_string()),
                    ("base_version", format!("{base_version:?}")),
                    ("active_version", format!("{active_version:?}")),
                ],
            ),
            Self::AlreadyActive { candidate } => (
                "INDEXING.VERSIONING.006",
                ErrorClass::Contract,
                format!("candidate {candidate} is already the active version"),
                vec![("candidate_version", candidate.to_string())],
            ),
            Self::InvalidCandidate(error) => (
                "INDEXING.VERSIONING.007",
                ErrorClass::Validation,
                format!("invalid candidate index version: {error}"),
                Vec::new(),
            ),
            Self::InvalidDefinition(error) => (
                "INDEXING.VERSIONING.008",
                ErrorClass::Validation,
                format!("invalid index definition: {error}"),
                Vec::new(),
            ),
        };

        let mut global = GlobalError {
            code: ErrorCode::new(code).expect("Indexing error code is statically valid"),
            owner: ErrorOwner::new("INDEXING").expect("Indexing error owner is statically valid"),
            version: CoreVersion::new(1, 0, 0),
            class,
            severity: Severity::Error,
            retryability: Retryability::NonRetryable,
            message,
            details: Vec::new(),
            solution_reference: None,
            context,
            cause: None,
        };

        for (key, value) in details {
            if let Some(detail) = nizaam_core::error::DiagnosticDetail::new(key, value) {
                global = global.with_detail(detail);
            }
        }

        global
    }
}

/// Core-error result adapter for source-version compatibility validation.
#[allow(clippy::result_large_err)]
pub fn validate_candidate_source_compatibility_result(
    candidate: &IndexVersion,
    expected_source_version: Option<&SourceVersion>,
    context: ErrorContext,
) -> IndexingResult<()> {
    validate_candidate_source_compatibility(candidate, expected_source_version)
        .map_err(|error| ErrorEvent::new(error.into_global_error(context)))
}

/// Core-error result adapter for schema-version compatibility validation.
#[allow(clippy::result_large_err)]
pub fn validate_candidate_schema_compatibility_result(
    candidate: &IndexVersion,
    expected_schema_version: Option<&SchemaVersion>,
    context: ErrorContext,
) -> IndexingResult<()> {
    validate_candidate_schema_compatibility(candidate, expected_schema_version)
        .map_err(|error| ErrorEvent::new(error.into_global_error(context)))
}

/// Core-error result adapter for full candidate compatibility validation.
#[allow(clippy::result_large_err)]
pub fn validate_candidate_compatibility_result(
    candidate: &IndexVersion,
    definition: &IndexDefinition,
    context: ErrorContext,
) -> IndexingResult<()> {
    validate_candidate_compatibility(candidate, definition)
        .map_err(|error| ErrorEvent::new(error.into_global_error(context)))
}

/// Core-error result adapter for candidate-lineage validation.
#[allow(clippy::result_large_err)]
pub fn validate_candidate_lineage_result(
    candidate: &IndexVersion,
    base_version: Option<&IndexVersionId>,
    active_version: Option<&IndexVersionId>,
    context: ErrorContext,
) -> IndexingResult<()> {
    validate_candidate_lineage(candidate, base_version, active_version)
        .map_err(|error| ErrorEvent::new(error.into_global_error(context)))
}

/// Core-error result adapter for active-version compatibility validation.
#[allow(clippy::result_large_err)]
pub fn validate_active_version_compatibility_result(
    candidate: &IndexVersion,
    base_version: Option<&IndexVersionId>,
    active_version: Option<&IndexVersionId>,
    context: ErrorContext,
) -> IndexingResult<()> {
    validate_active_version_compatibility(candidate, base_version, active_version)
        .map_err(|error| ErrorEvent::new(error.into_global_error(context)))
}

/// Core-error result adapter for publication eligibility validation.
#[allow(clippy::result_large_err)]
pub fn validate_publication_eligibility_result(
    candidate: &IndexVersion,
    definition: &IndexDefinition,
    base_version: Option<&IndexVersionId>,
    active_version: Option<&IndexVersionId>,
    context: ErrorContext,
) -> IndexingResult<()> {
    validate_publication_eligibility(candidate, definition, base_version, active_version)
        .map_err(|error| ErrorEvent::new(error.into_global_error(context)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::{IndexDefinitionId, IndexDefinitionIdentity, IndexNamespace};
    use crate::index::{
        ConsistencyRequirement, IndexDefinition, IndexFamily, IndexVersionId, KeyDefinition,
        TargetReferenceType, Uniqueness,
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

    fn definition() -> IndexDefinition {
        IndexDefinition::new(
            IndexDefinitionIdentity::new(
                IndexDefinitionId::new("test.definition").expect("definition ID should be valid"),
                IndexNamespace::new("test").expect("namespace should be valid"),
                IndexFamily::Inverted,
            ),
            KeyDefinition::new(["text"]).expect("key definition should be valid"),
            TargetReferenceType::new("object").expect("target type should be valid"),
            Uniqueness::NonUnique,
            ConsistencyRequirement::new("exact").expect("consistency requirement should be valid"),
            Some(source_version("source-v1")),
            Some(schema_version("schema-v1")),
        )
        .expect("definition should be valid")
    }

    fn candidate(value: &str, source: Option<&str>, schema: Option<&str>) -> IndexVersion {
        IndexVersion::with_metadata(
            version_id(value),
            source.map(source_version),
            schema.map(schema_version),
            None,
        )
        .expect("candidate version should be valid")
    }

    #[test]
    fn accepts_matching_source_and_schema_versions() {
        let candidate = candidate("candidate-v1", Some("source-v1"), Some("schema-v1"));
        let definition = definition();

        validate_candidate_compatibility(&candidate, &definition)
            .expect("matching source and schema versions should be compatible");
    }

    #[test]
    fn rejects_missing_candidate_source_version() {
        let candidate = candidate("candidate-v1", None, Some("schema-v1"));
        let expected = source_version("source-v1");

        let error = validate_candidate_source_compatibility(&candidate, Some(&expected))
            .expect_err("missing source version must be rejected");

        assert_eq!(
            error,
            VersioningError::MissingCandidateSourceVersion {
                candidate: version_id("candidate-v1"),
                expected,
            }
        );
    }

    #[test]
    fn rejects_source_version_mismatch() {
        let candidate = candidate("candidate-v1", Some("source-v2"), Some("schema-v1"));
        let expected = source_version("source-v1");

        let error = validate_candidate_source_compatibility(&candidate, Some(&expected))
            .expect_err("source mismatch must be rejected");

        assert_eq!(
            error,
            VersioningError::SourceVersionMismatch {
                candidate: version_id("candidate-v1"),
                expected,
                actual: source_version("source-v2"),
            }
        );
    }

    #[test]
    fn rejects_missing_candidate_schema_version() {
        let candidate = candidate("candidate-v1", Some("source-v1"), None);
        let expected = schema_version("schema-v1");

        let error = validate_candidate_schema_compatibility(&candidate, Some(&expected))
            .expect_err("missing schema version must be rejected");

        assert_eq!(
            error,
            VersioningError::MissingCandidateSchemaVersion {
                candidate: version_id("candidate-v1"),
                expected,
            }
        );
    }

    #[test]
    fn rejects_schema_version_mismatch() {
        let candidate = candidate("candidate-v1", Some("source-v1"), Some("schema-v2"));
        let expected = schema_version("schema-v1");

        let error = validate_candidate_schema_compatibility(&candidate, Some(&expected))
            .expect_err("schema mismatch must be rejected");

        assert_eq!(
            error,
            VersioningError::SchemaVersionMismatch {
                candidate: version_id("candidate-v1"),
                expected,
                actual: schema_version("schema-v2"),
            }
        );
    }

    #[test]
    fn allows_unconstrained_source_or_schema_versions() {
        let candidate = candidate("candidate-v1", Some("source-v9"), Some("schema-v9"));

        validate_candidate_source_compatibility(&candidate, None)
            .expect("missing source constraint should not reject a valid candidate");
        validate_candidate_schema_compatibility(&candidate, None)
            .expect("missing schema constraint should not reject a valid candidate");
    }

    #[test]
    fn candidate_is_not_stale_when_base_matches_active() {
        let active = version_id("active-v5");

        assert!(!is_candidate_stale(Some(&active), Some(&active)));
    }

    #[test]
    fn candidate_is_stale_when_newer_active_version_replaced_its_base() {
        let base = version_id("active-v5");
        let active = version_id("active-v6");

        assert!(is_candidate_stale(Some(&base), Some(&active)));

        let candidate = candidate("candidate-v7", Some("source-v1"), Some("schema-v1"));

        let error = validate_candidate_lineage(&candidate, Some(&base), Some(&active))
            .expect_err("candidate based on an older active version must be stale");

        assert_eq!(
            error,
            VersioningError::StaleCandidate {
                candidate: version_id("candidate-v7"),
                base_version: Some(base),
                active_version: Some(active),
            }
        );
    }

    #[test]
    fn initial_candidate_without_predecessor_is_valid_before_first_publication() {
        let candidate = candidate("candidate-v1", Some("source-v1"), Some("schema-v1"));

        validate_candidate_lineage(&candidate, None, None)
            .expect("initial candidate should have valid empty lineage");
    }

    #[test]
    fn unanchored_candidate_is_rejected_when_an_active_version_exists() {
        let active = version_id("active-v1");
        let candidate = candidate("candidate-v2", Some("source-v2"), Some("schema-v2"));

        let error = validate_candidate_lineage(&candidate, None, Some(&active))
            .expect_err("candidate without a base cannot safely replace an active version");

        assert_eq!(
            error,
            VersioningError::StaleCandidate {
                candidate: version_id("candidate-v2"),
                base_version: None,
                active_version: Some(active),
            }
        );
    }

    #[test]
    fn matching_lineage_allows_active_version_compatibility() {
        let active = version_id("active-v1");
        let candidate = candidate("candidate-v2", Some("source-v1"), Some("schema-v1"));

        validate_active_version_compatibility(&candidate, Some(&active), Some(&active))
            .expect("candidate derived from the current active version should be compatible");
    }

    #[test]
    fn already_active_version_is_not_a_publication_target() {
        let active = version_id("active-v2");
        let candidate = candidate("active-v2", Some("source-v2"), Some("schema-v2"));

        let error = validate_active_version_compatibility(&candidate, Some(&active), Some(&active))
            .expect_err("the active version itself is not a candidate publication target");

        assert_eq!(
            error,
            VersioningError::AlreadyActive {
                candidate: version_id("active-v2"),
            }
        );
    }

    #[test]
    fn publication_eligibility_combines_definition_and_lineage_checks() {
        let active = version_id("active-v1");
        let candidate = candidate("candidate-v2", Some("source-v1"), Some("schema-v1"));
        let definition = definition();

        validate_publication_eligibility(&candidate, &definition, Some(&active), Some(&active))
            .expect("compatible candidate with current lineage should be publication-eligible");
    }

    #[test]
    fn publication_eligibility_rejects_stale_candidate_even_when_metadata_matches() {
        let base = version_id("active-v1");
        let active = version_id("active-v2");
        let candidate = candidate("candidate-v3", Some("source-v1"), Some("schema-v1"));
        let definition = definition();

        let error =
            validate_publication_eligibility(&candidate, &definition, Some(&base), Some(&active))
                .expect_err("stale candidate must not be publication-eligible");

        assert!(matches!(error, VersioningError::StaleCandidate { .. }));
    }

    #[test]
    fn error_display_preserves_the_versioning_failure_meaning() {
        let error = VersioningError::AlreadyActive {
            candidate: version_id("active-v2"),
        };

        assert_eq!(
            error.to_string(),
            "candidate active-v2 is already the active version"
        );
    }

    #[test]
    fn version_values_are_compared_opaquely() {
        let newer_named_value = source_version("source-newer");
        let older_named_value = source_version("source-older");
        let candidate = candidate("candidate-v1", Some("source-newer"), Some("schema-v1"));

        validate_candidate_source_compatibility(&candidate, Some(&newer_named_value))
            .expect("exact source-version equality is the only compatibility rule");

        assert!(
            validate_candidate_source_compatibility(&candidate, Some(&older_named_value)).is_err()
        );
    }
}
