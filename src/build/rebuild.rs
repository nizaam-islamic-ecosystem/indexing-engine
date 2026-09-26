//! Major isolated rebuild orchestration for Phase 3.
//!
//! This module coordinates the logical rebuild workflow:
//!
//! ```text
//! active version
//!      │
//!      ├────────────── remains untouched
//!      │
//!      ▼
//! source snapshot
//!      ↓
//! capture update sequence N
//!      ↓
//! build isolated candidate
//!      ↓
//! replay journal records > N
//!      ↓
//! reach a consistency point
//!      ↓
//! validate
//!      ↓
//! ready for later publication
//! ```
//!
//! The rebuild boundary is intentionally separated from publication. A
//! successful rebuild only produces a validated, unpublished logical
//! candidate. A failed rebuild cannot mutate or replace the active version
//! because active-version state is not owned by this module.
//!
//! Concurrent-update coordination remains provider/synchronization-neutral.
//! The staged `start` → `replay` → `finish` API lets a higher-level lifecycle
//! owner provide fresh journal snapshots between replay rounds without this
//! module freezing a particular synchronization primitive.
//!
//! This module deliberately does not implement:
//! - Core runtime or capability dispatch;
//! - a global scheduler or Control Plane;
//! - physical storage or provider selection;
//! - query planning or retrieval;
//! - publication or active-version switching;
//! - domain mutation or semantic interpretation;
//! - an Indexing-specific cancellation/deadline system.

use super::builder::{BuildCandidate, BuildError, BuildInput, BuildSnapshot, IndexBuilder};
use super::update::{CandidateUpdater, UpdateError, UpdateJournal, UpdateSequence};
use crate::consistency::versioning::{VersioningError, validate_active_version_compatibility};
use crate::error::IndexingResult;
use crate::identity::IndexId;
use crate::index::{IndexDefinition, IndexVersionId};
use core::fmt;
use nizaam_core::contracts::Version as CoreVersion;
use nizaam_core::error::{
    ErrorClass, ErrorCode, ErrorContext, ErrorEvent, ErrorOwner, GlobalError, Severity,
};
use nizaam_core::status::Retryability;
use std::error::Error;

/// Logical input to an isolated major rebuild.
///
/// `snapshot` is the source-owned state supplied to Indexing for this build.
/// The rebuild workflow does not query or mutate the source engine.
///
/// `base_version` and `active_version` carry the lineage context needed to
/// ensure that the rebuild candidate is derived from the active version that
/// the caller observed when construction began. They are deliberately kept
/// outside [`IndexVersion`], which remains the Phase 2 logical version value.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RebuildInput {
    index_id: IndexId,
    definition: IndexDefinition,
    candidate_version_id: IndexVersionId,
    snapshot: BuildSnapshot,
    base_version: Option<IndexVersionId>,
    active_version: Option<IndexVersionId>,
}

impl RebuildInput {
    /// Creates a rebuild input from a logical index, definition, candidate
    /// version identity, captured source snapshot, and observed lineage.
    #[must_use]
    pub fn new(
        index_id: IndexId,
        definition: IndexDefinition,
        candidate_version_id: IndexVersionId,
        snapshot: BuildSnapshot,
        base_version: Option<IndexVersionId>,
        active_version: Option<IndexVersionId>,
    ) -> Self {
        Self {
            index_id,
            definition,
            candidate_version_id,
            snapshot,
            base_version,
            active_version,
        }
    }

    /// Returns the logical index identity targeted by the rebuild.
    #[must_use]
    pub fn index_id(&self) -> &IndexId {
        &self.index_id
    }

    /// Returns the logical definition governing the rebuild candidate.
    #[must_use]
    pub fn definition(&self) -> &IndexDefinition {
        &self.definition
    }

    /// Returns the logical version identity to construct.
    #[must_use]
    pub fn candidate_version_id(&self) -> &IndexVersionId {
        &self.candidate_version_id
    }

    /// Returns the source snapshot captured for this rebuild.
    #[must_use]
    pub fn snapshot(&self) -> &BuildSnapshot {
        &self.snapshot
    }

    /// Returns the version the caller observed as the candidate's base.
    #[must_use]
    pub fn base_version(&self) -> Option<&IndexVersionId> {
        self.base_version.as_ref()
    }

    /// Returns the version the caller observed as active when rebuilding.
    #[must_use]
    pub fn active_version(&self) -> Option<&IndexVersionId> {
        self.active_version.as_ref()
    }

    /// Consumes the input and returns all logical components.
    #[must_use]
    pub fn into_parts(
        self,
    ) -> (
        IndexId,
        IndexDefinition,
        IndexVersionId,
        BuildSnapshot,
        Option<IndexVersionId>,
        Option<IndexVersionId>,
    ) {
        (
            self.index_id,
            self.definition,
            self.candidate_version_id,
            self.snapshot,
            self.base_version,
            self.active_version,
        )
    }
}

/// Intermediate logical rebuild state.
///
/// This is not a lifecycle-state enum. It is an immutable-progress value that
/// records which portion of the update journal has already been replayed onto
/// the isolated candidate. The active version remains external to this value.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RebuildProgress {
    candidate: BuildCandidate,
    base_version: Option<IndexVersionId>,
    captured_sequence: UpdateSequence,
    replayed_through: UpdateSequence,
    replayed_updates: usize,
}

impl RebuildProgress {
    /// Returns the isolated candidate currently under rebuild.
    #[must_use]
    pub fn candidate(&self) -> &BuildCandidate {
        &self.candidate
    }

    /// Returns the version observed as the candidate's base when rebuild
    /// started.
    #[must_use]
    pub fn base_version(&self) -> Option<&IndexVersionId> {
        self.base_version.as_ref()
    }

    /// Returns the journal sequence captured when rebuild construction began.
    #[must_use]
    pub const fn captured_sequence(&self) -> UpdateSequence {
        self.captured_sequence
    }

    /// Returns the latest journal sequence replayed into the candidate.
    #[must_use]
    pub const fn replayed_through(&self) -> UpdateSequence {
        self.replayed_through
    }

    /// Returns the number of logical update records replayed so far.
    #[must_use]
    pub const fn replayed_updates(&self) -> usize {
        self.replayed_updates
    }

    /// Returns whether the progress has caught up to the supplied journal
    /// snapshot.
    ///
    /// A lower observed sequence is not considered caught up. That situation
    /// normally means the caller supplied a different or older journal view.
    #[must_use]
    pub fn is_caught_up(&self, journal: &UpdateJournal) -> bool {
        journal.current_sequence() == self.replayed_through
    }

    /// Consumes the progress and returns its isolated candidate and replay
    /// metadata without performing publication.
    #[must_use]
    pub fn into_parts(
        self,
    ) -> (
        BuildCandidate,
        Option<IndexVersionId>,
        UpdateSequence,
        UpdateSequence,
        usize,
    ) {
        (
            self.candidate,
            self.base_version,
            self.captured_sequence,
            self.replayed_through,
            self.replayed_updates,
        )
    }
}

/// Successful completion of a logical rebuild.
///
/// The candidate has reached a journal consistency point and passed logical
/// candidate validation. It is still unpublished. Later publication logic is
/// responsible for re-checking current lineage and performing the READY →
/// ACTIVE transition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RebuildResult {
    candidate: BuildCandidate,
    base_version: Option<IndexVersionId>,
    captured_sequence: UpdateSequence,
    replayed_through: UpdateSequence,
    replayed_updates: usize,
}

impl RebuildResult {
    /// Returns the validated, unpublished rebuild candidate.
    #[must_use]
    pub fn candidate(&self) -> &BuildCandidate {
        &self.candidate
    }

    /// Returns the candidate's observed rebuild base version.
    #[must_use]
    pub fn base_version(&self) -> Option<&IndexVersionId> {
        self.base_version.as_ref()
    }

    /// Returns the journal sequence captured at rebuild start.
    #[must_use]
    pub const fn captured_sequence(&self) -> UpdateSequence {
        self.captured_sequence
    }

    /// Returns the latest journal sequence replayed into the candidate.
    #[must_use]
    pub const fn replayed_through(&self) -> UpdateSequence {
        self.replayed_through
    }

    /// Returns the total number of journal records replayed during rebuild.
    #[must_use]
    pub const fn replayed_updates(&self) -> usize {
        self.replayed_updates
    }

    /// Consumes the result and returns the isolated candidate plus rebuild
    /// metadata.
    #[must_use]
    pub fn into_parts(
        self,
    ) -> (
        BuildCandidate,
        Option<IndexVersionId>,
        UpdateSequence,
        UpdateSequence,
        usize,
    ) {
        (
            self.candidate,
            self.base_version,
            self.captured_sequence,
            self.replayed_through,
            self.replayed_updates,
        )
    }
}

/// Errors produced by the logical rebuild workflow.
#[derive(Debug, Eq, PartialEq)]
pub enum RebuildError {
    /// Candidate construction or logical validation failed.
    Build(BuildError),

    /// Candidate lineage or other version-level compatibility failed.
    Versioning(VersioningError),

    /// A replayed logical mutation could not be applied to the candidate.
    Update(UpdateError),

    /// A caller supplied a journal whose sequence is older than the rebuild
    /// progress already obtained.
    JournalSequenceRegressed {
        /// Sequence through which the candidate has already replayed.
        replayed_through: UpdateSequence,

        /// Sequence reported by the newly supplied journal snapshot.
        observed: UpdateSequence,
    },

    /// The candidate was asked to finish while the supplied journal had newer
    /// updates that had not yet been replayed.
    NotCaughtUp {
        /// Sequence through which the candidate has replayed.
        replayed_through: UpdateSequence,

        /// Latest sequence visible in the supplied journal snapshot.
        journal_sequence: UpdateSequence,
    },
}

impl fmt::Display for RebuildError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Build(error) => {
                write!(formatter, "rebuild candidate construction failed: {error}")
            }
            Self::Versioning(error) => {
                write!(
                    formatter,
                    "rebuild candidate versioning validation failed: {error}"
                )
            }
            Self::Update(error) => write!(formatter, "rebuild update replay failed: {error}"),
            Self::JournalSequenceRegressed {
                replayed_through,
                observed,
            } => write!(
                formatter,
                "rebuild journal sequence regressed from {replayed_through} to {observed}"
            ),
            Self::NotCaughtUp {
                replayed_through,
                journal_sequence,
            } => write!(
                formatter,
                "rebuild is not caught up: replayed through {replayed_through}, journal is at {journal_sequence}"
            ),
        }
    }
}

impl Error for RebuildError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Build(error) => Some(error),
            Self::Versioning(error) => Some(error),
            Self::Update(error) => Some(error),
            Self::JournalSequenceRegressed { .. } | Self::NotCaughtUp { .. } => None,
        }
    }
}

impl RebuildError {
    /// Converts the typed rebuild failure into the shared Core error
    /// contract using the caller-supplied execution context.
    #[must_use]
    pub fn into_global_error(self, context: ErrorContext) -> GlobalError {
        let (code, class, message, details) = match self {
            Self::Build(error) => (
                "INDEXING.REBUILD.001",
                ErrorClass::Execution,
                format!("rebuild candidate construction failed: {error}"),
                Vec::new(),
            ),
            Self::Versioning(error) => (
                "INDEXING.REBUILD.002",
                ErrorClass::Contract,
                format!("rebuild candidate versioning validation failed: {error}"),
                Vec::new(),
            ),
            Self::Update(error) => (
                "INDEXING.REBUILD.003",
                ErrorClass::Execution,
                format!("rebuild update replay failed: {error}"),
                Vec::new(),
            ),
            Self::JournalSequenceRegressed {
                replayed_through,
                observed,
            } => (
                "INDEXING.REBUILD.004",
                ErrorClass::Contract,
                format!("rebuild journal sequence regressed from {replayed_through} to {observed}"),
                vec![
                    ("replayed_through", replayed_through.to_string()),
                    ("observed_sequence", observed.to_string()),
                ],
            ),
            Self::NotCaughtUp {
                replayed_through,
                journal_sequence,
            } => (
                "INDEXING.REBUILD.005",
                ErrorClass::Contract,
                format!(
                    "rebuild is not caught up: replayed through {replayed_through}, journal is at {journal_sequence}"
                ),
                vec![
                    ("replayed_through", replayed_through.to_string()),
                    ("journal_sequence", journal_sequence.to_string()),
                ],
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

impl IndexRebuilder {
    /// Core-error result adapter for starting an isolated rebuild.
    #[allow(clippy::result_large_err)]
    pub fn start_result(
        &self,
        input: RebuildInput,
        journal: &UpdateJournal,
        context: ErrorContext,
    ) -> IndexingResult<RebuildProgress> {
        self.start(input, journal)
            .map_err(|error| ErrorEvent::new(error.into_global_error(context)))
    }

    /// Core-error result adapter for replaying a journal snapshot.
    #[allow(clippy::result_large_err)]
    pub fn replay_result(
        &self,
        progress: &RebuildProgress,
        journal: &UpdateJournal,
        context: ErrorContext,
    ) -> IndexingResult<RebuildProgress> {
        self.replay(progress, journal)
            .map_err(|error| ErrorEvent::new(error.into_global_error(context)))
    }

    /// Core-error result adapter for completing an isolated rebuild.
    #[allow(clippy::result_large_err)]
    pub fn finish_result(
        &self,
        progress: RebuildProgress,
        journal: &UpdateJournal,
        context: ErrorContext,
    ) -> IndexingResult<RebuildResult> {
        self.finish(&progress, journal)
            .map_err(|error| ErrorEvent::new(error.into_global_error(context)))
    }

    /// Core-error result adapter for the complete rebuild workflow.
    #[allow(clippy::result_large_err)]
    pub fn rebuild_result(
        &self,
        input: RebuildInput,
        journal: &UpdateJournal,
        context: ErrorContext,
    ) -> IndexingResult<RebuildResult> {
        self.rebuild(input, journal)
            .map_err(|error| ErrorEvent::new(error.into_global_error(context)))
    }
}

/// Stateless Indexing-local major rebuild orchestrator.
///
/// The orchestrator creates isolated candidates and replays logical update
/// records onto them. It has no active-version storage and therefore cannot
/// corrupt the currently published candidate by construction.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct IndexRebuilder {
    builder: IndexBuilder,
    updater: CandidateUpdater,
}

impl IndexRebuilder {
    /// Creates a stateless logical rebuild orchestrator.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            builder: IndexBuilder::new(),
            updater: CandidateUpdater::new(),
        }
    }

    /// Starts an isolated rebuild.
    ///
    /// The supplied [`BuildSnapshot`] is the source state used for candidate
    /// construction. After the snapshot is supplied, the current journal
    /// sequence is captured as the replay boundary `N`.
    ///
    /// No update records at or below `N` are replayed because they are treated
    /// as part of the captured build boundary. Records strictly greater than
    /// `N` are replayed later.
    pub fn start(
        &self,
        input: RebuildInput,
        journal: &UpdateJournal,
    ) -> Result<RebuildProgress, RebuildError> {
        let base_version = input.base_version.clone();
        let active_version = input.active_version.clone();

        // The source snapshot is already supplied by the caller. Capture the
        // journal boundary before construction so mutations accepted during
        // the build remain eligible for replay.
        let sequence = journal.current_sequence();

        let build_input = BuildInput::new(
            input.index_id,
            input.definition,
            input.candidate_version_id,
            input.snapshot,
        );

        let candidate = self
            .builder
            .build(build_input)
            .map_err(RebuildError::Build)?;

        validate_active_version_compatibility(
            candidate.version(),
            base_version.as_ref(),
            active_version.as_ref(),
        )
        .map_err(RebuildError::Versioning)?;

        Ok(RebuildProgress {
            candidate,
            base_version,
            captured_sequence: sequence,
            replayed_through: sequence,
            replayed_updates: 0,
        })
    }

    /// Replays all journal records newer than the progress boundary.
    ///
    /// The supplied progress value is treated as immutable input and a new
    /// progress value is returned. This means a failed replay leaves the prior
    /// isolated candidate/progress value unchanged for the caller.
    ///
    /// A higher-level owner can call this method again with a fresh journal
    /// snapshot when updates continue to arrive during rebuild or replay. The
    /// synchronization mechanism is intentionally outside this module.
    pub fn replay(
        &self,
        progress: &RebuildProgress,
        journal: &UpdateJournal,
    ) -> Result<RebuildProgress, RebuildError> {
        let observed_sequence = journal.current_sequence();

        if observed_sequence < progress.replayed_through {
            return Err(RebuildError::JournalSequenceRegressed {
                replayed_through: progress.replayed_through,
                observed: observed_sequence,
            });
        }

        let mut candidate = progress.candidate.clone();
        let mut replayed_through = progress.replayed_through;
        let mut replayed_updates = progress.replayed_updates;

        for record in journal.records_after(progress.replayed_through) {
            let sequence = record.sequence();

            if sequence <= replayed_through {
                return Err(RebuildError::JournalSequenceRegressed {
                    replayed_through,
                    observed: sequence,
                });
            }

            candidate = self
                .updater
                .apply(&candidate, record.mutation().clone())
                .map_err(RebuildError::Update)?;

            replayed_through = sequence;
            replayed_updates += 1;
        }

        Ok(RebuildProgress {
            candidate,
            base_version: progress.base_version.clone(),
            captured_sequence: progress.captured_sequence,
            replayed_through,
            replayed_updates,
        })
    }

    /// Returns whether the rebuild progress has caught up to the supplied
    /// journal snapshot.
    #[must_use]
    pub fn is_caught_up(&self, progress: &RebuildProgress, journal: &UpdateJournal) -> bool {
        progress.is_caught_up(journal)
    }

    /// Validates a rebuild progress value only when it has reached the current
    /// journal sequence.
    ///
    /// This is the logical consistency point before later publication. If the
    /// journal has advanced, the caller must obtain the newer snapshot and
    /// replay again rather than treating the candidate as ready.
    pub fn finish(
        &self,
        progress: &RebuildProgress,
        journal: &UpdateJournal,
    ) -> Result<RebuildResult, RebuildError> {
        let observed_sequence = journal.current_sequence();

        if observed_sequence < progress.replayed_through {
            return Err(RebuildError::JournalSequenceRegressed {
                replayed_through: progress.replayed_through,
                observed: observed_sequence,
            });
        }

        if observed_sequence != progress.replayed_through {
            return Err(RebuildError::NotCaughtUp {
                replayed_through: progress.replayed_through,
                journal_sequence: observed_sequence,
            });
        }

        IndexBuilder::validate(&progress.candidate).map_err(RebuildError::Build)?;

        Ok(RebuildResult {
            candidate: progress.candidate.clone(),
            base_version: progress.base_version.clone(),
            captured_sequence: progress.captured_sequence,
            replayed_through: progress.replayed_through,
            replayed_updates: progress.replayed_updates,
        })
    }

    /// Runs a complete rebuild against a stable journal snapshot.
    ///
    /// For a rebuild that must coordinate with updates arriving during
    /// construction or replay, prefer the staged [`Self::start`],
    /// [`Self::replay`], and [`Self::finish`] methods so the caller can provide
    /// fresh journal snapshots between replay rounds.
    pub fn rebuild(
        &self,
        input: RebuildInput,
        journal: &UpdateJournal,
    ) -> Result<RebuildResult, RebuildError> {
        let progress = self.start(input, journal)?;
        let progress = self.replay(&progress, journal)?;
        self.finish(&progress, journal)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build::update::IndexMutation;
    use crate::identity::{IndexDefinitionId, IndexDefinitionIdentity, IndexNamespace};
    use crate::index::{
        ConsistencyRequirement, IndexEntry, IndexFamily, KeyDefinition, KeyMaterial,
        ObjectReference, SchemaVersion, SourceVersion, TargetReferenceType, Uniqueness,
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
        IndexDefinition::new(
            IndexDefinitionIdentity::new(
                IndexDefinitionId::new("documents.v1").expect("test definition ID should be valid"),
                IndexNamespace::new("search.documents").expect("test namespace should be valid"),
                IndexFamily::Inverted,
            ),
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
        base: Option<&str>,
        active: Option<&str>,
        snapshot_entries: Vec<IndexEntry>,
    ) -> RebuildInput {
        RebuildInput::new(
            index_id(0x55),
            definition(Some("source-v1"), Some("schema-v1")),
            version_id("index-v2"),
            BuildSnapshot::with_versions(
                Some(source_version("source-v1")),
                Some(schema_version("schema-v1")),
                snapshot_entries,
            ),
            base.map(version_id),
            active.map(version_id),
        )
    }

    fn insert(key: &str, reference: &str) -> IndexMutation {
        IndexMutation::Insert(entry(key, reference))
    }

    #[test]
    fn start_builds_isolated_candidate_and_captures_sequence() {
        let mut journal = UpdateJournal::new();
        journal
            .append(insert("old", "doc:old"))
            .expect("append should succeed");

        let rebuilder = IndexRebuilder::new();
        let progress = rebuilder
            .start(
                input(
                    Some("index-v1"),
                    Some("index-v1"),
                    vec![entry("alpha", "doc:1")],
                ),
                &journal,
            )
            .expect("rebuild should start");

        assert_eq!(progress.captured_sequence().value(), 1);
        assert_eq!(progress.replayed_through().value(), 1);
        assert_eq!(progress.replayed_updates(), 0);
        assert_eq!(progress.candidate().len(), 1);
        assert_eq!(
            progress.base_version().map(IndexVersionId::as_str),
            Some("index-v1")
        );
    }

    #[test]
    fn start_rejects_stale_base_lineage() {
        let journal = UpdateJournal::new();
        let rebuilder = IndexRebuilder::new();

        let error = rebuilder
            .start(
                input(
                    Some("index-v1"),
                    Some("index-v2"),
                    vec![entry("alpha", "doc:1")],
                ),
                &journal,
            )
            .expect_err("stale rebuild lineage must fail");

        assert!(matches!(
            error,
            RebuildError::Versioning(VersioningError::StaleCandidate { .. })
        ));
    }

    #[test]
    fn start_rejects_rebuild_that_targets_active_version() {
        let journal = UpdateJournal::new();
        let input = RebuildInput::new(
            index_id(0x55),
            definition(Some("source-v1"), Some("schema-v1")),
            version_id("index-v1"),
            BuildSnapshot::with_versions(
                Some(source_version("source-v1")),
                Some(schema_version("schema-v1")),
                vec![entry("alpha", "doc:1")],
            ),
            Some(version_id("index-v1")),
            Some(version_id("index-v1")),
        );

        let error = IndexRebuilder::new()
            .start(input, &journal)
            .expect_err("rebuild must create a different candidate version");

        assert!(matches!(
            error,
            RebuildError::Versioning(VersioningError::AlreadyActive { .. })
        ));
    }

    #[test]
    fn start_allows_initial_rebuild_without_active_version() {
        let journal = UpdateJournal::new();
        let input = RebuildInput::new(
            index_id(0x66),
            definition(Some("source-v1"), Some("schema-v1")),
            version_id("index-v1"),
            BuildSnapshot::with_versions(
                Some(source_version("source-v1")),
                Some(schema_version("schema-v1")),
                vec![entry("alpha", "doc:1")],
            ),
            None,
            None,
        );

        let progress = IndexRebuilder::new()
            .start(input, &journal)
            .expect("initial rebuild should be allowed");

        assert_eq!(progress.captured_sequence(), UpdateSequence::INITIAL);
    }

    #[test]
    fn replay_applies_only_records_after_captured_boundary() {
        let mut journal = UpdateJournal::new();
        journal
            .append(insert("before", "doc:before"))
            .expect("append should succeed");

        let rebuilder = IndexRebuilder::new();
        let progress = rebuilder
            .start(
                input(
                    Some("index-v1"),
                    Some("index-v1"),
                    vec![entry("alpha", "doc:1")],
                ),
                &journal,
            )
            .expect("rebuild should start");

        journal
            .append(insert("after", "doc:after"))
            .expect("append should succeed");
        journal
            .append(insert("later", "doc:later"))
            .expect("append should succeed");

        let replayed = rebuilder
            .replay(&progress, &journal)
            .expect("updates after the boundary should replay");

        assert_eq!(replayed.replayed_through().value(), 3);
        assert_eq!(replayed.replayed_updates(), 2);
        assert!(
            replayed
                .candidate()
                .entries()
                .iter()
                .any(|value| value.key() == &KeyMaterial::text("after"))
        );
        assert!(
            replayed
                .candidate()
                .entries()
                .iter()
                .any(|value| value.key() == &KeyMaterial::text("later"))
        );
        assert!(
            replayed
                .candidate()
                .entries()
                .iter()
                .all(|value| value.key() != &KeyMaterial::text("before"))
        );
    }

    #[test]
    fn replay_does_not_append_replayed_mutations_to_the_journal() {
        let mut journal = UpdateJournal::new();
        journal
            .append(insert("after", "doc:after"))
            .expect("append should succeed");
        let journal_before = journal.clone();

        let rebuilder = IndexRebuilder::new();
        let progress = rebuilder
            .start(
                input(None, None, vec![entry("alpha", "doc:1")]),
                &UpdateJournal::new(),
            )
            .expect("initial rebuild should start");

        let replayed = rebuilder
            .replay(&progress, &journal)
            .expect("journal mutation should replay");

        assert_eq!(journal, journal_before);
        assert_eq!(replayed.replayed_through(), journal.current_sequence());
    }

    #[test]
    fn replay_can_advance_through_multiple_concurrent_update_snapshots() {
        let initial_journal = UpdateJournal::new();
        let rebuilder = IndexRebuilder::new();
        let progress = rebuilder
            .start(
                input(None, None, vec![entry("alpha", "doc:1")]),
                &initial_journal,
            )
            .expect("rebuild should start");

        let mut journal_round_one = initial_journal.clone();
        journal_round_one
            .append(insert("beta", "doc:2"))
            .expect("append should succeed");
        let progress = rebuilder
            .replay(&progress, &journal_round_one)
            .expect("first concurrent update should replay");
        assert_eq!(progress.replayed_through().value(), 1);

        let mut journal_round_two = journal_round_one.clone();
        journal_round_two
            .append(insert("gamma", "doc:3"))
            .expect("append should succeed");
        let progress = rebuilder
            .replay(&progress, &journal_round_two)
            .expect("second concurrent update should replay");

        assert_eq!(progress.replayed_through().value(), 2);
        assert_eq!(progress.replayed_updates(), 2);
        assert!(
            progress
                .candidate()
                .entries()
                .iter()
                .any(|value| value.key() == &KeyMaterial::text("beta"))
        );
        assert!(
            progress
                .candidate()
                .entries()
                .iter()
                .any(|value| value.key() == &KeyMaterial::text("gamma"))
        );
    }

    #[test]
    fn replay_failure_preserves_previous_progress_value() {
        let mut journal = UpdateJournal::new();
        journal
            .append(insert("duplicate", "doc:1"))
            .expect("append should succeed");

        let rebuilder = IndexRebuilder::new();
        let progress = rebuilder
            .start(
                input(None, None, vec![entry("duplicate", "doc:1")]),
                &UpdateJournal::new(),
            )
            .expect("initial rebuild should start");
        let original = progress.clone();

        let error = rebuilder
            .replay(&progress, &journal)
            .expect_err("replaying a duplicate entry must fail");

        assert!(matches!(
            error,
            RebuildError::Update(UpdateError::DuplicateEntry(_))
        ));
        assert_eq!(progress, original);
    }

    #[test]
    fn replay_rejects_an_older_journal_snapshot() {
        let mut journal = UpdateJournal::new();
        journal
            .append(insert("beta", "doc:2"))
            .expect("append should succeed");

        let rebuilder = IndexRebuilder::new();
        let progress = rebuilder
            .start(input(None, None, vec![entry("alpha", "doc:1")]), &journal)
            .expect("rebuild should start");

        let older_journal = UpdateJournal::new();
        let error = rebuilder
            .replay(&progress, &older_journal)
            .expect_err("older journal must be rejected");

        assert_eq!(
            error,
            RebuildError::JournalSequenceRegressed {
                replayed_through: UpdateSequence::new(1),
                observed: UpdateSequence::INITIAL,
            }
        );
    }

    #[test]
    fn finish_requires_a_consistency_point_before_ready() {
        let journal = UpdateJournal::new();
        let rebuilder = IndexRebuilder::new();
        let progress = rebuilder
            .start(input(None, None, vec![entry("alpha", "doc:1")]), &journal)
            .expect("rebuild should start");

        let mut advanced_journal = journal.clone();
        advanced_journal
            .append(insert("beta", "doc:2"))
            .expect("append should succeed");

        let error = rebuilder
            .finish(&progress, &advanced_journal)
            .expect_err("candidate must replay the new update before ready");

        assert_eq!(
            error,
            RebuildError::NotCaughtUp {
                replayed_through: UpdateSequence::INITIAL,
                journal_sequence: UpdateSequence::new(1),
            }
        );
    }

    #[test]
    fn finish_validates_and_returns_unpublished_rebuild_result() {
        let journal = UpdateJournal::new();
        let rebuilder = IndexRebuilder::new();
        let progress = rebuilder
            .start(
                input(
                    Some("index-v1"),
                    Some("index-v1"),
                    vec![entry("alpha", "doc:1")],
                ),
                &journal,
            )
            .expect("rebuild should start");

        let result = rebuilder
            .finish(&progress, &journal)
            .expect("caught-up valid candidate should finish");

        assert_eq!(result.captured_sequence(), UpdateSequence::INITIAL);
        assert_eq!(result.replayed_through(), UpdateSequence::INITIAL);
        assert_eq!(result.replayed_updates(), 0);
        assert_eq!(
            result.base_version().map(IndexVersionId::as_str),
            Some("index-v1")
        );
        assert_eq!(result.candidate().version().id().as_str(), "index-v2");
    }

    #[test]
    fn rebuild_convenience_method_completes_against_stable_journal() {
        let mut journal = UpdateJournal::new();
        journal
            .append(insert("beta", "doc:2"))
            .expect("append should succeed");

        let result = IndexRebuilder::new()
            .rebuild(input(None, None, vec![entry("alpha", "doc:1")]), &journal)
            .expect("stable-journal rebuild should complete");

        assert_eq!(result.replayed_updates(), 0);
        assert_eq!(result.replayed_through(), journal.current_sequence());
    }
}
