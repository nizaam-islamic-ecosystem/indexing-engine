//! Indexing-local candidate construction orchestration for Phase 3.
//!
//! This module coordinates the logical construction boundary:
//!
//! ```text
//! IndexDefinition
//!       ↓
//! source snapshot
//!       ↓
//! candidate IndexVersion
//!       ↓
//! populated logical entries
//!       ↓
//! validation
//!       ↓
//! unpublished BuildCandidate
//! ```
//!
//! The module deliberately does not implement:
//! - Core runtime or capability dispatch;
//! - a global scheduler or Control Plane;
//! - physical storage or provider selection;
//! - incremental update semantics;
//! - bounded batch execution;
//! - rebuild/update journals;
//! - publication or active-version switching;
//! - query planning or retrieval;
//! - domain-object hydration or semantic interpretation.
//!
//! `BuildCandidate` is a logical, unpublished construction result. It does
//! not model lifecycle state such as READY or ACTIVE. Those lifecycle
//! semantics belong to the later Phase 3 coordination/publication layers.

use crate::consistency::versioning::{VersioningError, validate_candidate_compatibility};
use crate::error::IndexingResult;
use crate::identity::IndexId;
use crate::index::{
    IndexDefinition, IndexEntry, IndexEntryValidationError, IndexVersion, IndexVersionId,
    IndexVersionValidationError, SchemaVersion, SourceVersion,
};
use core::fmt;
use nizaam_core::contracts::Version as CoreVersion;
use nizaam_core::error::{
    ErrorClass, ErrorCode, ErrorContext, ErrorEvent, ErrorOwner, GlobalError, Severity,
};
use nizaam_core::status::Retryability;
use std::error::Error;

/// A caller-provided logical source snapshot used to populate a candidate.
///
/// The snapshot is already outside the source engine's ownership boundary:
/// Indexing receives the source state and does not query or mutate the source
/// while constructing the candidate.
///
/// `source_version` and `schema_version` identify the state represented by the
/// supplied entries. They remain optional because the Phase 2 logical version
/// contract allows them to be absent when the definition does not require
/// them.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BuildSnapshot {
    source_version: Option<SourceVersion>,
    schema_version: Option<SchemaVersion>,
    entries: Vec<IndexEntry>,
}

impl BuildSnapshot {
    /// Creates a source snapshot without explicit source/schema version
    /// metadata.
    ///
    /// The supplied iterator is materialized into the logical construction
    /// input. Phase 3 bounded batch behavior is owned by `batch.rs`; this
    /// constructor does not silently define a batch-size policy.
    pub fn new<I>(entries: I) -> Self
    where
        I: IntoIterator<Item = IndexEntry>,
    {
        Self::with_versions(None, None, entries)
    }

    /// Creates a source snapshot with optional source/schema version metadata.
    pub fn with_versions<I>(
        source_version: Option<SourceVersion>,
        schema_version: Option<SchemaVersion>,
        entries: I,
    ) -> Self
    where
        I: IntoIterator<Item = IndexEntry>,
    {
        Self {
            source_version,
            schema_version,
            entries: entries.into_iter().collect(),
        }
    }

    /// Returns the source-state version represented by this snapshot.
    #[must_use]
    pub fn source_version(&self) -> Option<&SourceVersion> {
        self.source_version.as_ref()
    }

    /// Returns the schema version represented by this snapshot.
    #[must_use]
    pub fn schema_version(&self) -> Option<&SchemaVersion> {
        self.schema_version.as_ref()
    }

    /// Returns the logical entries supplied by the source snapshot.
    #[must_use]
    pub fn entries(&self) -> &[IndexEntry] {
        &self.entries
    }

    /// Returns the number of logical entries in the snapshot.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns whether the snapshot contains no logical entries.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Consumes the snapshot and returns all logical components.
    #[must_use]
    pub fn into_parts(
        self,
    ) -> (
        Option<SourceVersion>,
        Option<SchemaVersion>,
        Vec<IndexEntry>,
    ) {
        (self.source_version, self.schema_version, self.entries)
    }
}

/// Input to the Indexing-local candidate construction workflow.
///
/// `index_id` identifies the concrete logical index resource. It is supplied
/// by the caller because `builder.rs` constructs an index version; it does not
/// redefine the Phase 1/2 `IndexId` generation contract.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BuildInput {
    index_id: IndexId,
    definition: IndexDefinition,
    candidate_version_id: IndexVersionId,
    snapshot: BuildSnapshot,
}

impl BuildInput {
    /// Creates a logical build input from an index identity, definition,
    /// candidate version identity, and source snapshot.
    #[must_use]
    pub fn new(
        index_id: IndexId,
        definition: IndexDefinition,
        candidate_version_id: IndexVersionId,
        snapshot: BuildSnapshot,
    ) -> Self {
        Self {
            index_id,
            definition,
            candidate_version_id,
            snapshot,
        }
    }

    /// Returns the logical index identity targeted by construction.
    #[must_use]
    pub fn index_id(&self) -> &IndexId {
        &self.index_id
    }

    /// Returns the logical definition governing the candidate.
    #[must_use]
    pub fn definition(&self) -> &IndexDefinition {
        &self.definition
    }

    /// Returns the logical version identity to construct.
    #[must_use]
    pub fn candidate_version_id(&self) -> &IndexVersionId {
        &self.candidate_version_id
    }

    /// Returns the source snapshot used as construction input.
    #[must_use]
    pub fn snapshot(&self) -> &BuildSnapshot {
        &self.snapshot
    }

    /// Consumes the input and returns all logical components.
    #[must_use]
    pub fn into_parts(self) -> (IndexId, IndexDefinition, IndexVersionId, BuildSnapshot) {
        (
            self.index_id,
            self.definition,
            self.candidate_version_id,
            self.snapshot,
        )
    }
}

/// Logical result of candidate construction.
///
/// The candidate contains the definition, version metadata, and populated
/// logical `IndexEntry` values necessary for later Phase 3 lifecycle steps.
/// It is intentionally not an active index and does not contain lifecycle
/// state, provider handles, storage resources, or publication authority.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BuildCandidate {
    index_id: IndexId,
    definition: IndexDefinition,
    version: IndexVersion,
    entries: Vec<IndexEntry>,
}

impl BuildCandidate {
    /// Returns the logical index identity represented by the candidate.
    #[must_use]
    pub fn index_id(&self) -> &IndexId {
        &self.index_id
    }

    /// Returns the logical index definition governing the candidate.
    #[must_use]
    pub fn definition(&self) -> &IndexDefinition {
        &self.definition
    }

    /// Returns the candidate's logical index version metadata.
    #[must_use]
    pub fn version(&self) -> &IndexVersion {
        &self.version
    }

    /// Returns the populated logical index entries.
    #[must_use]
    pub fn entries(&self) -> &[IndexEntry] {
        &self.entries
    }

    /// Returns the number of populated logical entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns whether the candidate contains no logical entries.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Consumes the candidate and returns all logical construction components.
    #[must_use]
    pub fn into_parts(self) -> (IndexId, IndexDefinition, IndexVersion, Vec<IndexEntry>) {
        (self.index_id, self.definition, self.version, self.entries)
    }

    /// Reconstructs a candidate from already-owned logical components inside
    /// the crate.
    ///
    /// Later update/rebuild modules may use this boundary after applying their
    /// own logical mutation rules. Validation remains explicit and must be
    /// performed before publication.
    pub(crate) fn from_parts(
        index_id: IndexId,
        definition: IndexDefinition,
        version: IndexVersion,
        entries: Vec<IndexEntry>,
    ) -> Self {
        Self {
            index_id,
            definition,
            version,
            entries,
        }
    }
}

/// Failures produced while constructing or validating a logical candidate.
#[derive(Debug, Eq, PartialEq)]
pub enum BuildError {
    /// A populated logical entry failed validation.
    InvalidEntry {
        /// Zero-based position of the failing entry in the candidate.
        position: usize,

        /// The entry-level validation failure.
        error: IndexEntryValidationError,
    },

    /// Candidate version metadata could not be constructed.
    InvalidVersion(IndexVersionValidationError),

    /// Candidate/definition/source/schema compatibility failed.
    Versioning(VersioningError),
}

impl fmt::Display for BuildError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidEntry { position, error } => {
                write!(
                    formatter,
                    "invalid candidate entry at position {position}: {error}"
                )
            }
            Self::InvalidVersion(error) => {
                write!(formatter, "invalid candidate index version: {error}")
            }
            Self::Versioning(error) => {
                write!(formatter, "candidate versioning validation failed: {error}")
            }
        }
    }
}

impl Error for BuildError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidEntry { error, .. } => Some(error),
            Self::InvalidVersion(error) => Some(error),
            Self::Versioning(error) => Some(error),
        }
    }
}

impl BuildError {
    /// Converts the typed construction failure into the shared Core error
    /// contract using the caller-supplied execution context.
    #[must_use]
    pub fn into_global_error(self, context: ErrorContext) -> GlobalError {
        let (code, message, position) = match self {
            Self::InvalidEntry { position, error } => (
                "INDEXING.BUILD.001",
                format!("invalid candidate entry at position {position}: {error}"),
                Some(position),
            ),
            Self::InvalidVersion(error) => (
                "INDEXING.BUILD.002",
                format!("invalid candidate index version: {error}"),
                None,
            ),
            Self::Versioning(error) => (
                "INDEXING.BUILD.003",
                format!("candidate versioning validation failed: {error}"),
                None,
            ),
        };

        let mut global = GlobalError {
            code: ErrorCode::new(code).expect("Indexing error code is statically valid"),
            owner: ErrorOwner::new("INDEXING").expect("Indexing error owner is statically valid"),
            version: CoreVersion::new(1, 0, 0),
            class: ErrorClass::Validation,
            severity: Severity::Error,
            retryability: Retryability::NonRetryable,
            message,
            details: Vec::new(),
            solution_reference: None,
            context,
            cause: None,
        };

        if let Some(position) = position
            && let Some(detail) =
                nizaam_core::error::DiagnosticDetail::new("entry_position", position.to_string())
        {
            global = global.with_detail(detail);
        }

        global
    }
}

impl IndexBuilder {
    /// Core-error result adapter for candidate construction.
    #[allow(clippy::result_large_err)]
    pub fn build_result(
        &self,
        input: BuildInput,
        context: ErrorContext,
    ) -> IndexingResult<BuildCandidate> {
        self.build(input)
            .map_err(|error| ErrorEvent::new(error.into_global_error(context)))
    }

    /// Core-error result adapter for candidate validation.
    #[allow(clippy::result_large_err)]
    pub fn validate_result(
        candidate: &BuildCandidate,
        context: ErrorContext,
    ) -> IndexingResult<()> {
        Self::validate(candidate).map_err(|error| ErrorEvent::new(error.into_global_error(context)))
    }
}

/// Stateless orchestrator for Indexing-local logical candidate construction.
///
/// The builder owns no global state. Active-version tracking, concurrent
/// candidate coordination, rebuild journals, update sequencing, and
/// publication belong to later Phase 3 modules.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct IndexBuilder;

impl IndexBuilder {
    /// Creates a stateless index builder.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Constructs and validates an unpublished logical candidate.
    ///
    /// The operation performs these logical stages:
    ///
    /// ```text
    /// BuildInput
    ///     ↓
    /// candidate IndexVersion
    ///     ↓
    /// populate from source snapshot
    ///     ↓
    /// validate entries + definition/version compatibility
    ///     ↓
    /// BuildCandidate
    /// ```
    ///
    /// It does not publish the result or alter any active version.
    pub fn build(&self, input: BuildInput) -> Result<BuildCandidate, BuildError> {
        let (index_id, definition, candidate_version_id, snapshot) = input.into_parts();
        let (source_version, schema_version, entries) = snapshot.into_parts();

        let version =
            IndexVersion::with_metadata(candidate_version_id, source_version, schema_version, None)
                .map_err(BuildError::InvalidVersion)?;

        let candidate = BuildCandidate::from_parts(index_id, definition, version, entries);

        Self::validate(&candidate)?;

        Ok(candidate)
    }

    /// Validates a logical candidate without changing it or publishing it.
    ///
    /// This method is deliberately reusable by later Phase 3 layers after a
    /// candidate has been transformed by update or rebuild operations.
    pub fn validate(candidate: &BuildCandidate) -> Result<(), BuildError> {
        validate_candidate_compatibility(&candidate.version, &candidate.definition)
            .map_err(BuildError::Versioning)?;

        for (position, entry) in candidate.entries.iter().enumerate() {
            entry
                .validate()
                .map_err(|error| BuildError::InvalidEntry { position, error })?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::{IndexDefinitionId, IndexDefinitionIdentity, IndexNamespace};
    use crate::index::{
        ConsistencyRequirement, IndexFamily, KeyDefinition, KeyMaterial, ObjectReference,
        TargetReferenceType, Uniqueness,
    };

    fn index_id(seed: u8) -> IndexId {
        IndexId::from_bytes([seed; 64])
    }

    fn version_id(value: &str) -> IndexVersionId {
        IndexVersionId::new(value).expect("test version ID should be valid")
    }

    fn source_version(value: &str) -> SourceVersion {
        SourceVersion::new(value).expect("test source version should be valid")
    }

    fn schema_version(value: &str) -> SchemaVersion {
        SchemaVersion::new(value).expect("test schema version should be valid")
    }

    fn definition(source: Option<&str>, schema: Option<&str>) -> IndexDefinition {
        let identity = IndexDefinitionIdentity::new(
            IndexDefinitionId::new("documents.v1").expect("test definition ID should be valid"),
            IndexNamespace::new("search.documents").expect("test namespace should be valid"),
            IndexFamily::Inverted,
        );

        IndexDefinition::new(
            identity,
            KeyDefinition::new(["term"]).expect("test key definition should be valid"),
            TargetReferenceType::new("source.document")
                .expect("test target reference type should be valid"),
            Uniqueness::NonUnique,
            ConsistencyRequirement::new("logical-v1")
                .expect("test consistency requirement should be valid"),
            source.map(source_version),
            schema.map(schema_version),
        )
        .expect("test definition should be valid")
    }

    fn entry(key: &str, reference: &str) -> IndexEntry {
        IndexEntry::new(
            KeyMaterial::text(key),
            ObjectReference::new("documents", reference)
                .expect("test object reference should be valid"),
        )
        .expect("test entry should be valid")
    }

    fn input(
        index: IndexId,
        version: &str,
        definition: IndexDefinition,
        snapshot: BuildSnapshot,
    ) -> BuildInput {
        BuildInput::new(index, definition, version_id(version), snapshot)
    }

    #[test]
    fn builds_and_validates_a_candidate_from_a_snapshot() {
        let snapshot = BuildSnapshot::with_versions(
            Some(source_version("source-v3")),
            Some(schema_version("schema-v2")),
            vec![entry("alpha", "doc:1"), entry("beta", "doc:2")],
        );

        let builder = IndexBuilder::new();
        let candidate = builder
            .build(input(
                index_id(0x11),
                "index-v4",
                definition(Some("source-v3"), Some("schema-v2")),
                snapshot,
            ))
            .expect("valid logical candidate should build");

        assert_eq!(candidate.index_id(), &index_id(0x11));
        assert_eq!(candidate.version().id().as_str(), "index-v4");
        assert_eq!(candidate.len(), 2);
        assert_eq!(candidate.entries()[0].key(), &KeyMaterial::text("alpha"));
        assert_eq!(candidate.entries()[1].target().object_reference(), "doc:2");
    }

    #[test]
    fn candidate_preserves_source_and_schema_versions_from_the_snapshot() {
        let snapshot = BuildSnapshot::with_versions(
            Some(source_version("source-v8")),
            Some(schema_version("schema-v5")),
            Vec::<IndexEntry>::new(),
        );

        let candidate = IndexBuilder::new()
            .build(input(
                index_id(0x22),
                "index-v9",
                definition(Some("source-v8"), Some("schema-v5")),
                snapshot,
            ))
            .expect("valid candidate should build");

        assert_eq!(
            candidate
                .version()
                .source_version()
                .expect("source version should exist")
                .as_str(),
            "source-v8"
        );
        assert_eq!(
            candidate
                .version()
                .schema_version()
                .expect("schema version should exist")
                .as_str(),
            "schema-v5"
        );
    }

    #[test]
    fn rejects_missing_source_version_when_definition_requires_one() {
        let snapshot = BuildSnapshot::with_versions(
            None,
            Some(schema_version("schema-v2")),
            Vec::<IndexEntry>::new(),
        );

        let error = IndexBuilder::new()
            .build(input(
                index_id(0x33),
                "index-v1",
                definition(Some("source-v3"), Some("schema-v2")),
                snapshot,
            ))
            .expect_err("missing required source version must fail construction");

        assert!(matches!(
            error,
            BuildError::Versioning(VersioningError::MissingCandidateSourceVersion { .. })
        ));
    }

    #[test]
    fn rejects_source_version_mismatch() {
        let snapshot = BuildSnapshot::with_versions(
            Some(source_version("source-v4")),
            Some(schema_version("schema-v2")),
            Vec::<IndexEntry>::new(),
        );

        let error = IndexBuilder::new()
            .build(input(
                index_id(0x44),
                "index-v1",
                definition(Some("source-v3"), Some("schema-v2")),
                snapshot,
            ))
            .expect_err("source version mismatch must fail construction");

        assert!(matches!(
            error,
            BuildError::Versioning(VersioningError::SourceVersionMismatch { .. })
        ));
    }

    #[test]
    fn rejects_schema_version_mismatch() {
        let snapshot = BuildSnapshot::with_versions(
            Some(source_version("source-v3")),
            Some(schema_version("schema-v4")),
            Vec::<IndexEntry>::new(),
        );

        let error = IndexBuilder::new()
            .build(input(
                index_id(0x55),
                "index-v1",
                definition(Some("source-v3"), Some("schema-v2")),
                snapshot,
            ))
            .expect_err("schema version mismatch must fail construction");

        assert!(matches!(
            error,
            BuildError::Versioning(VersioningError::SchemaVersionMismatch { .. })
        ));
    }

    #[test]
    fn allows_an_empty_snapshot() {
        let snapshot = BuildSnapshot::new(Vec::<IndexEntry>::new());
        assert!(snapshot.is_empty());

        let candidate = IndexBuilder::new()
            .build(input(
                index_id(0x66),
                "index-empty",
                definition(None, None),
                snapshot,
            ))
            .expect("an empty logical snapshot is still a valid candidate");

        assert!(candidate.is_empty());
    }

    #[test]
    fn preserves_snapshot_entry_order() {
        let snapshot = BuildSnapshot::new(vec![
            entry("first", "doc:1"),
            entry("second", "doc:2"),
            entry("third", "doc:3"),
        ]);

        let candidate = IndexBuilder::new()
            .build(input(
                index_id(0x77),
                "index-order",
                definition(None, None),
                snapshot,
            ))
            .expect("candidate should build");

        let references: Vec<&str> = candidate
            .entries()
            .iter()
            .map(|value| value.target().object_reference())
            .collect();

        assert_eq!(references, ["doc:1", "doc:2", "doc:3"]);
    }

    #[test]
    fn build_does_not_regenerate_or_replace_the_supplied_index_identity() {
        let supplied = index_id(0x88);
        let snapshot = BuildSnapshot::new(Vec::<IndexEntry>::new());

        let candidate = IndexBuilder::new()
            .build(input(
                supplied,
                "index-v1",
                definition(None, None),
                snapshot,
            ))
            .expect("candidate should build");

        assert_eq!(candidate.index_id(), &supplied);
    }

    #[test]
    fn unconstrained_source_and_schema_versions_allow_snapshot_values() {
        let snapshot = BuildSnapshot::with_versions(
            Some(source_version("source-any")),
            Some(schema_version("schema-any")),
            Vec::<IndexEntry>::new(),
        );

        let candidate = IndexBuilder::new()
            .build(input(
                index_id(0x99),
                "index-v1",
                definition(None, None),
                snapshot,
            ))
            .expect("unconstrained definition should accept snapshot versions");

        assert_eq!(candidate.len(), 0);
    }

    #[test]
    fn validation_is_repeatable_and_has_no_publication_effect() {
        let candidate = IndexBuilder::new()
            .build(input(
                index_id(0xAA),
                "index-v1",
                definition(Some("source-v1"), Some("schema-v1")),
                BuildSnapshot::with_versions(
                    Some(source_version("source-v1")),
                    Some(schema_version("schema-v1")),
                    vec![entry("value", "doc:1")],
                ),
            ))
            .expect("candidate should build");

        IndexBuilder::validate(&candidate).expect("validation should succeed");
        IndexBuilder::validate(&candidate).expect("validation should be repeatable");

        // The builder exposes no active-version transition. The result remains
        // an explicit BuildCandidate for later lifecycle/publication handling.
        assert_eq!(candidate.version().id().as_str(), "index-v1");
    }

    #[test]
    fn candidate_round_trip_preserves_all_components() {
        let index = index_id(0xBB);
        let definition = definition(None, None);
        let version = IndexVersion::new(version_id("index-v2"));
        let entries = vec![entry("value", "doc:2")];

        let candidate =
            BuildCandidate::from_parts(index, definition.clone(), version.clone(), entries.clone());

        let (returned_index, returned_definition, returned_version, returned_entries) =
            candidate.into_parts();

        assert_eq!(returned_index, index);
        assert_eq!(returned_definition, definition);
        assert_eq!(returned_version, version);
        assert_eq!(returned_entries, entries);
    }
}
