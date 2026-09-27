//! Logical integrity validation for the Indexing Engine.
//!
//! Phase 5 makes the trust boundary explicit:
//!
//! ```text
//! external/source input
//!        ↓
//! logical integrity validation
//!        ↓
//! trusted logical index state
//! ```
//!
//! This module validates only contracts already owned by Indexing. It does not
//! inspect physical provider state, query a source engine, hydrate domain
//! objects, or repair storage.
//!
//! The validator covers the logical layers required by Phase 5:
//!
//! ```text
//! Index metadata
//!      ↓
//! Index definition
//!      ↓
//! Index version
//!      ↓
//! Index entries
//!      ↓
//! Object references
//!      ↓
//! Version compatibility
//!      ↓
//! Publication preconditions
//! ```
//!
//! In particular, [`ObjectReference`] remains opaque. Indexing can validate
//! its structural contract, and it can compare separately supplied logical
//! reference-type contracts, but it must not infer domain reference semantics
//! or call a source engine to establish object existence.

use crate::consistency::versioning::{self, VersioningError};
use crate::index::{
    IndexDefinition, IndexEntry, IndexEntryValidationError, IndexVersion, IndexVersionState,
    ObjectReference, ObjectReferenceValidationError, TargetReferenceType,
};
use core::fmt;
use std::collections::BTreeSet;
use std::error::Error;

/// Result type returned by the logical integrity validator.
pub type IntegrityResult<T> = Result<T, IntegrityValidationError>;

/// Logical integrity validation failures.
///
/// These errors remain local to the validation boundary. They do not classify
/// operational failures such as provider outages, resource exhaustion, or
/// source-data availability. Those belong to the Phase 5 recovery boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum IntegrityValidationError {
    /// The logical index definition is structurally invalid.
    InvalidDefinition(crate::index::IndexDefinitionValidationError),

    /// The logical index version or its metadata is structurally invalid.
    InvalidVersion(crate::index::IndexVersionValidationError),

    /// A logical index entry is structurally invalid.
    InvalidEntry {
        /// Zero-based position of the invalid entry when validating a batch.
        position: usize,
        /// Entry-level validation failure.
        error: IndexEntryValidationError,
    },

    /// A unique index contains more than one entry with the same logical key.
    UniqueKeyConflict {
        /// Zero-based position of the conflicting entry.
        position: usize,
        /// Logical key that conflicts with an earlier entry.
        key: crate::index::KeyMaterial,
    },

    /// A logical object reference is structurally invalid.
    InvalidReference(ObjectReferenceValidationError),

    /// The expected and supplied logical reference-type contracts disagree.
    ReferenceTypeMismatch {
        /// The reference type required by the receiving contract.
        expected: TargetReferenceType,
        /// The reference type supplied by the candidate/input contract.
        actual: TargetReferenceType,
    },

    /// A candidate version is incompatible with its index definition or
    /// active-version lineage.
    VersionCompatibility(VersioningError),

    /// The candidate lifecycle state is not eligible for publication.
    CandidateNotReady {
        /// Candidate version identity.
        candidate: crate::index::IndexVersionId,
        /// Lifecycle state observed at validation time.
        lifecycle: crate::index::VersionLifecycle,
    },

    /// The lifecycle wrapper does not describe the candidate being validated.
    CandidateStateMismatch {
        /// Candidate version identity.
        candidate: crate::index::IndexVersionId,
        /// Version identity carried by the lifecycle state.
        state: crate::index::IndexVersionId,
    },
}

impl fmt::Display for IntegrityValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidDefinition(error) => {
                write!(formatter, "invalid index definition: {error}")
            }
            Self::InvalidVersion(error) => {
                write!(formatter, "invalid index version: {error}")
            }
            Self::InvalidEntry { position, error } => {
                write!(
                    formatter,
                    "invalid index entry at position {position}: {error}"
                )
            }
            Self::UniqueKeyConflict { position, key } => write!(
                formatter,
                "unique index contains a conflicting key at position {position}: {key:?}"
            ),
            Self::InvalidReference(error) => {
                write!(formatter, "invalid object reference: {error}")
            }
            Self::ReferenceTypeMismatch { expected, actual } => write!(
                formatter,
                "object reference type mismatch: expected {expected}, got {actual}"
            ),
            Self::VersionCompatibility(error) => {
                write!(
                    formatter,
                    "index version compatibility validation failed: {error}"
                )
            }
            Self::CandidateNotReady {
                candidate,
                lifecycle,
            } => write!(
                formatter,
                "candidate {candidate} is not ready for publication; current lifecycle is {lifecycle:?}"
            ),
            Self::CandidateStateMismatch { candidate, state } => write!(
                formatter,
                "candidate state mismatch: candidate {candidate} does not match lifecycle state {state}"
            ),
        }
    }
}

impl Error for IntegrityValidationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidDefinition(error) => Some(error),
            Self::InvalidVersion(error) => Some(error),
            Self::InvalidEntry { error, .. } => Some(error),
            Self::UniqueKeyConflict { .. } => None,
            Self::InvalidReference(error) => Some(error),
            Self::VersionCompatibility(error) => Some(error),
            Self::ReferenceTypeMismatch { .. }
            | Self::CandidateNotReady { .. }
            | Self::CandidateStateMismatch { .. } => None,
        }
    }
}

/// Stateless logical integrity validator.
///
/// The validator carries no runtime state, provider handle, source callback,
/// cache, scheduler, or recovery mechanism. Keeping it stateless makes the
/// validation boundary deterministic and prevents it from becoming another
/// execution subsystem.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct IntegrityValidator;

impl IntegrityValidator {
    /// Creates a stateless integrity validator.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Validates an [`IndexDefinition`] and all metadata owned by it.
    pub fn validate_definition(&self, definition: &IndexDefinition) -> IntegrityResult<()> {
        definition
            .validate()
            .map_err(IntegrityValidationError::InvalidDefinition)
    }

    /// Validates an [`IndexVersion`] and its logical metadata.
    pub fn validate_version(&self, version: &IndexVersion) -> IntegrityResult<()> {
        version
            .validate()
            .map_err(IntegrityValidationError::InvalidVersion)
    }

    /// Validates an [`IndexEntry`] without contacting its source owner.
    pub fn validate_entry(&self, entry: &IndexEntry) -> IntegrityResult<()> {
        entry
            .validate()
            .map_err(|error| IntegrityValidationError::InvalidEntry { position: 0, error })?;

        self.validate_reference(entry.target())
    }

    /// Validates a batch of logical index entries.
    ///
    /// Every entry is validated independently. The method performs no source
    /// lookup and does not allocate an unbounded auxiliary structure.
    pub fn validate_entries<'a, I>(&self, entries: I) -> IntegrityResult<()>
    where
        I: IntoIterator<Item = &'a IndexEntry>,
    {
        for (position, entry) in entries.into_iter().enumerate() {
            entry
                .validate()
                .map_err(|error| IntegrityValidationError::InvalidEntry { position, error })?;

            self.validate_reference(entry.target())?;
        }

        Ok(())
    }

    /// Validates an [`ObjectReference`] structurally.
    ///
    /// `ObjectReference` already enforces its invariants at construction time.
    /// Reconstructing it here from its public components deliberately keeps
    /// validation deterministic while avoiding any source-engine callback.
    pub fn validate_reference(&self, reference: &ObjectReference) -> IntegrityResult<()> {
        ObjectReference::new(reference.source(), reference.object_reference())
            .map(|_| ())
            .map_err(IntegrityValidationError::InvalidReference)
    }

    /// Validates a separately supplied logical reference-type contract.
    ///
    /// `ObjectReference` intentionally does not encode a domain type, so this
    /// method compares two already-declared logical contracts instead of
    /// guessing a type from the opaque reference string.
    pub fn validate_reference_type(
        &self,
        expected: &TargetReferenceType,
        actual: &TargetReferenceType,
    ) -> IntegrityResult<()> {
        if expected == actual {
            Ok(())
        } else {
            Err(IntegrityValidationError::ReferenceTypeMismatch {
                expected: expected.clone(),
                actual: actual.clone(),
            })
        }
    }

    /// Validates the complete logical index state represented by a definition,
    /// version, and entry set.
    ///
    /// The check is intentionally contract-level. It does not select a
    /// provider, inspect storage, or determine whether referenced source
    /// objects currently exist.
    pub fn validate_index<'a, I>(
        &self,
        definition: &IndexDefinition,
        version: &IndexVersion,
        entries: I,
    ) -> IntegrityResult<()>
    where
        I: IntoIterator<Item = &'a IndexEntry>,
    {
        self.validate_definition(definition)?;
        self.validate_version(version)?;
        versioning::validate_candidate_compatibility(version, definition)
            .map_err(IntegrityValidationError::VersionCompatibility)?;

        // Materialize only references so the shared validation path can be
        // reused before the uniqueness pass; no index-entry payloads are copied.
        let entries: Vec<&IndexEntry> = entries.into_iter().collect();
        self.validate_entries(entries.iter().copied())?;

        if definition.uniqueness() == crate::index::Uniqueness::Unique {
            let mut seen_keys = BTreeSet::new();

            for (position, entry) in entries.iter().enumerate() {
                if !seen_keys.insert(entry.key()) {
                    return Err(IntegrityValidationError::UniqueKeyConflict {
                        position,
                        key: entry.key().clone(),
                    });
                }
            }
        }

        Ok(())
    }

    /// Validates source/schema version compatibility against an index
    /// definition. Active-version lineage is intentionally excluded because
    /// that is a publication precondition rather than a property of the
    /// candidate's source/schema compatibility.
    pub fn validate_version_compatibility(
        &self,
        candidate: &IndexVersion,
        definition: &IndexDefinition,
    ) -> IntegrityResult<()> {
        self.validate_definition(definition)?;
        self.validate_version(candidate)?;

        versioning::validate_candidate_compatibility(candidate, definition)
            .map_err(IntegrityValidationError::VersionCompatibility)
    }

    /// Validates the logical preconditions that must hold immediately before
    /// publication.
    ///
    /// Lifecycle ownership remains with the Phase 3 state model. This method
    /// only checks that the candidate has reached `Ready` and that its logical
    /// version/lineage compatibility still permits publication. It does not
    /// mutate state and does not perform the publication itself.
    pub fn validate_publication_preconditions(
        &self,
        candidate: &IndexVersion,
        candidate_state: &IndexVersionState,
        definition: &IndexDefinition,
        base_version: Option<&crate::index::IndexVersionId>,
        active_version: Option<&crate::index::IndexVersionId>,
    ) -> IntegrityResult<()> {
        self.validate_definition(definition)?;

        if candidate_state.version() != candidate {
            return Err(IntegrityValidationError::CandidateStateMismatch {
                candidate: candidate.id().clone(),
                state: candidate_state.id().clone(),
            });
        }

        if candidate_state.lifecycle() != crate::index::VersionLifecycle::Ready {
            return Err(IntegrityValidationError::CandidateNotReady {
                candidate: candidate.id().clone(),
                lifecycle: candidate_state.lifecycle(),
            });
        }

        self.validate_version(candidate)?;
        versioning::validate_publication_eligibility(
            candidate,
            definition,
            base_version,
            active_version,
        )
        .map_err(IntegrityValidationError::VersionCompatibility)
    }
}

/// Compatibility helper for callers that prefer free functions over a
/// validator value.
pub fn validate_definition(definition: &IndexDefinition) -> IntegrityResult<()> {
    IntegrityValidator::new().validate_definition(definition)
}

/// Validates one logical index version.
pub fn validate_version(version: &IndexVersion) -> IntegrityResult<()> {
    IntegrityValidator::new().validate_version(version)
}

/// Validates one logical index entry.
pub fn validate_entry(entry: &IndexEntry) -> IntegrityResult<()> {
    IntegrityValidator::new().validate_entry(entry)
}

/// Validates one opaque source-owned object reference structurally.
pub fn validate_reference(reference: &ObjectReference) -> IntegrityResult<()> {
    IntegrityValidator::new().validate_reference(reference)
}

/// Validates source/schema version compatibility without source access.
pub fn validate_version_compatibility(
    candidate: &IndexVersion,
    definition: &IndexDefinition,
) -> IntegrityResult<()> {
    IntegrityValidator::new().validate_version_compatibility(candidate, definition)
}

/// Validates publication preconditions without performing publication.
pub fn validate_publication_preconditions(
    candidate: &IndexVersion,
    candidate_state: &IndexVersionState,
    definition: &IndexDefinition,
    base_version: Option<&crate::index::IndexVersionId>,
    active_version: Option<&crate::index::IndexVersionId>,
) -> IntegrityResult<()> {
    IntegrityValidator::new().validate_publication_preconditions(
        candidate,
        candidate_state,
        definition,
        base_version,
        active_version,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::{IndexDefinitionId, IndexDefinitionIdentity, IndexNamespace};
    use crate::index::{
        ConsistencyRequirement, IndexFamily, IndexVersionId, KeyDefinition, KeyMaterial,
        SchemaVersion, SourceVersion, Uniqueness, VersionLifecycle,
    };

    fn definition(
        target_type: &str,
        source_version: Option<&str>,
        schema_version: Option<&str>,
    ) -> IndexDefinition {
        IndexDefinition::new(
            IndexDefinitionIdentity::new(
                IndexDefinitionId::new("integrity.test").expect("definition id should be valid"),
                IndexNamespace::new("integrity").expect("namespace should be valid"),
                IndexFamily::Inverted,
            ),
            KeyDefinition::new(["term"]).expect("key definition should be valid"),
            TargetReferenceType::new(target_type).expect("target type should be valid"),
            Uniqueness::NonUnique,
            ConsistencyRequirement::new("logical")
                .expect("consistency requirement should be valid"),
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

    fn entry() -> IndexEntry {
        IndexEntry::new(
            KeyMaterial::text("term"),
            ObjectReference::new("source", "object:1").expect("object reference should be valid"),
        )
        .expect("entry should be valid")
    }

    #[test]
    fn validates_complete_logical_index_state() {
        let definition = definition("source.object", Some("source-v1"), Some("schema-v1"));
        let version = version("v1", Some("source-v1"), Some("schema-v1"));

        IntegrityValidator::new()
            .validate_index(&definition, &version, [&entry()])
            .expect("complete logical index should be valid");
    }

    #[test]
    fn rejects_duplicate_keys_for_unique_index() {
        let base = definition("source.object", Some("source-v1"), Some("schema-v1"));
        let definition = IndexDefinition::with_metadata(
            base.identity().clone(),
            base.key_definition().clone(),
            base.target_reference_type().clone(),
            Uniqueness::Unique,
            base.consistency_requirement().clone(),
            base.source_version().cloned(),
            base.schema_version().cloned(),
            base.lifecycle_metadata().cloned(),
        )
        .expect("unique definition should be valid");

        let version = version("v1", Some("source-v1"), Some("schema-v1"));
        let first = entry();
        let second = IndexEntry::new(
            KeyMaterial::text("term"),
            ObjectReference::new("source", "object:2")
                .expect("second object reference should be valid"),
        )
        .expect("second entry should be valid");

        let error = IntegrityValidator::new()
            .validate_index(&definition, &version, [&first, &second])
            .expect_err("duplicate logical keys must be rejected for a unique index");

        assert!(matches!(
            error,
            IntegrityValidationError::UniqueKeyConflict { position: 1, .. }
        ));
    }

    #[test]
    fn validates_structural_reference_without_source_lookup() {
        let reference =
            ObjectReference::new("source", "object:1").expect("reference should be valid");

        validate_reference(&reference).expect("reference should remain structurally valid");
    }

    #[test]
    fn compares_explicit_reference_type_contracts() {
        let expected = TargetReferenceType::new("source.object")
            .expect("expected reference type should be valid");
        let actual = TargetReferenceType::new("source.relationship")
            .expect("actual reference type should be valid");

        let error = IntegrityValidator::new()
            .validate_reference_type(&expected, &actual)
            .expect_err("different declared reference types must be rejected");

        assert!(matches!(
            error,
            IntegrityValidationError::ReferenceTypeMismatch { .. }
        ));
    }

    #[test]
    fn rejects_source_version_mismatch() {
        let definition = definition("source.object", Some("source-v2"), None);
        let candidate = version("v1", Some("source-v1"), None);

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
    fn rejects_schema_version_mismatch() {
        let definition = definition("source.object", None, Some("schema-v2"));
        let candidate = version("v1", None, Some("schema-v1"));

        let error = validate_version_compatibility(&candidate, &definition)
            .expect_err("schema-version mismatch must be rejected");

        assert!(matches!(
            error,
            IntegrityValidationError::VersionCompatibility(
                VersioningError::SchemaVersionMismatch { .. }
            )
        ));
    }

    fn ready_state(version: &IndexVersion) -> IndexVersionState {
        let mut state = IndexVersionState::new(version.clone());
        state
            .transition_to(VersionLifecycle::Validating)
            .expect("candidate should transition to validating");
        state
            .mark_ready()
            .expect("candidate should transition to ready");
        state
    }

    #[test]
    fn rejects_stale_publication_lineage() {
        let definition = definition("source.object", None, None);
        let candidate = version("candidate-v3", None, None);
        let base = IndexVersionId::new("active-v1").expect("base version should be valid");
        let current = IndexVersionId::new("active-v2").expect("active version should be valid");

        let error = validate_publication_preconditions(
            &candidate,
            &ready_state(&candidate),
            &definition,
            Some(&base),
            Some(&current),
        )
        .expect_err("stale candidate lineage must be rejected");

        assert!(matches!(
            error,
            IntegrityValidationError::VersionCompatibility(VersioningError::StaleCandidate { .. })
        ));
    }

    #[test]
    fn publication_requires_ready_candidate() {
        let definition = definition("source.object", None, None);
        let candidate = version("candidate-v1", None, None);
        let state = IndexVersionState::new(candidate.clone());

        let error = validate_publication_preconditions(&candidate, &state, &definition, None, None)
            .expect_err("building candidate must not be publication eligible");

        assert!(matches!(
            error,
            IntegrityValidationError::CandidateNotReady {
                lifecycle: VersionLifecycle::Building,
                ..
            }
        ));
    }

    #[test]
    fn publication_accepts_ready_candidate_with_valid_lineage() {
        let definition = definition("source.object", None, None);
        let candidate = version("candidate-v1", None, None);
        let mut state = IndexVersionState::new(candidate.clone());
        state
            .transition_to(VersionLifecycle::Validating)
            .expect("candidate should transition to validating");
        state
            .mark_ready()
            .expect("candidate should transition to ready");

        validate_publication_preconditions(&candidate, &state, &definition, None, None)
            .expect("ready initial candidate should satisfy publication preconditions");
    }

    #[test]
    fn version_metadata_is_validated_before_compatibility() {
        let definition = definition("source.object", None, None);
        let candidate = version("candidate-v1", None, None);

        validate_definition(&definition).expect("definition metadata should validate");
        validate_version(&candidate).expect("version metadata should validate");
    }
}
