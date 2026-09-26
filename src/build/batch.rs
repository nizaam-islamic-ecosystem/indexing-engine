//! Bounded logical batch execution for Phase 3.
//!
//! This module owns the grouping and transactional execution boundary for
//! multiple [`IndexMutation`](super::update::IndexMutation) values.
//!
//! The intended model is:
//!
//! ```text
//! logical mutation stream
//!        ↓
//! bounded chunk
//!        ↓
//! validate + apply as one logical transaction
//!        ↓
//! commit chunk state
//!        ↓
//! next bounded chunk
//! ```
//!
//! Each bounded chunk is transactional at the logical Indexing layer. A
//! successful earlier chunk remains part of the returned candidate when a
//! later chunk fails, while the failing chunk itself is not partially
//! committed.
//!
//! This module deliberately does not define an arbitrary production chunk
//! size. The caller supplies the bound. It also does not implement physical
//! provider transactions, persistence, scheduling, publication, rebuild
//! replay, Core runtime behavior, or domain semantics.

use super::builder::BuildCandidate;
use super::update::{
    CandidateUpdater, IndexMutation, UpdateError, UpdateJournal, UpdateJournalError,
};
use crate::error::IndexingResult;
use core::fmt;
use core::num::NonZeroUsize;
use nizaam_core::contracts::Version as CoreVersion;
use nizaam_core::error::{
    ErrorClass, ErrorCode, ErrorContext, ErrorEvent, ErrorOwner, GlobalError, Severity,
};
use nizaam_core::status::Retryability;
use std::error::Error;

/// Configuration for bounded batch execution.
///
/// The chunk size is supplied by the caller rather than frozen by the
/// Indexing logical contract. A chunk size of zero is rejected because every
/// successful execution step must make progress through the input stream.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BatchOptions {
    chunk_size: NonZeroUsize,
}

impl BatchOptions {
    /// Constructs bounded batch options from a positive chunk size.
    pub fn new(chunk_size: usize) -> Result<Self, BatchOptionsError> {
        let chunk_size = NonZeroUsize::new(chunk_size).ok_or(BatchOptionsError::ZeroChunkSize)?;

        Ok(Self { chunk_size })
    }

    /// Returns the maximum number of logical mutations processed in one
    /// transactional chunk.
    #[must_use]
    pub const fn chunk_size(self) -> NonZeroUsize {
        self.chunk_size
    }
}

/// Configuration failures for bounded batch execution.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BatchOptionsError {
    /// A zero-sized chunk cannot make progress through the input stream.
    ZeroChunkSize,
}

impl fmt::Display for BatchOptionsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroChunkSize => {
                formatter.write_str("batch chunk size must be greater than zero")
            }
        }
    }
}

impl Error for BatchOptionsError {}

/// State committed by all successfully completed chunks of a logical batch.
///
/// This value is returned on success and is also preserved inside
/// [`BatchError::ChunkFailed`] when a later chunk fails. The latter is
/// important because Phase 3 defines transactional semantics per bounded
/// chunk rather than treating an arbitrarily large batch as one transaction.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BatchResult {
    candidate: BuildCandidate,
    journal: UpdateJournal,
    completed_chunks: usize,
    applied_mutations: usize,
}

impl BatchResult {
    /// Returns the candidate containing all successfully committed chunks.
    #[must_use]
    pub fn candidate(&self) -> &BuildCandidate {
        &self.candidate
    }

    /// Returns the update journal containing only successfully committed
    /// mutations.
    #[must_use]
    pub fn journal(&self) -> &UpdateJournal {
        &self.journal
    }

    /// Returns the number of fully committed chunks.
    #[must_use]
    pub fn completed_chunks(&self) -> usize {
        self.completed_chunks
    }

    /// Returns the number of mutations committed across completed chunks.
    #[must_use]
    pub fn applied_mutations(&self) -> usize {
        self.applied_mutations
    }

    /// Consumes the batch result and returns its committed logical state.
    #[must_use]
    pub fn into_parts(self) -> (BuildCandidate, UpdateJournal, usize, usize) {
        (
            self.candidate,
            self.journal,
            self.completed_chunks,
            self.applied_mutations,
        )
    }

    fn new(candidate: BuildCandidate, journal: UpdateJournal) -> Self {
        Self {
            candidate,
            journal,
            completed_chunks: 0,
            applied_mutations: 0,
        }
    }

    fn record_committed_chunk(
        &mut self,
        mutation_count: usize,
        candidate: BuildCandidate,
        journal: UpdateJournal,
    ) {
        self.candidate = candidate;
        self.journal = journal;
        self.completed_chunks += 1;
        self.applied_mutations += mutation_count;
    }
}

/// Failure while executing a bounded transactional chunk.
#[derive(Debug, Eq, PartialEq)]
pub enum BatchChunkError {
    /// The candidate mutation itself failed.
    Update(UpdateError),

    /// The logical update journal could not record the chunk.
    Journal(UpdateJournalError),
}

impl fmt::Display for BatchChunkError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Update(error) => write!(formatter, "batch chunk update failed: {error}"),
            Self::Journal(error) => write!(formatter, "batch chunk journal failed: {error}"),
        }
    }
}

impl Error for BatchChunkError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Update(error) => Some(error),
            Self::Journal(error) => Some(error),
        }
    }
}

/// Failure raised by bounded batch execution.
///
/// When a chunk fails, `committed` contains the state after all earlier
/// successful chunks. The failing chunk is excluded from that state, so no
/// partial mutation from the failing chunk becomes committed.
// This error intentionally carries the committed prefix so callers can
// continue from the last successful transactional chunk. The payload is large
// by design; boxing would change the public error shape and is unnecessary for
// this logical contract.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Eq, PartialEq)]
pub enum BatchError {
    /// The supplied batch options are invalid.
    InvalidOptions(BatchOptionsError),

    /// One bounded transactional chunk failed.
    ChunkFailed {
        /// Zero-based chunk number of the failing chunk.
        chunk_index: usize,

        /// Logical reason the chunk failed.
        error: BatchChunkError,

        /// State committed by all chunks before the failing chunk.
        committed: BatchResult,
    },
}

impl fmt::Display for BatchError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidOptions(error) => {
                write!(formatter, "invalid batch options: {error}")
            }
            Self::ChunkFailed {
                chunk_index, error, ..
            } => write!(formatter, "batch chunk {chunk_index} failed: {error}"),
        }
    }
}

impl Error for BatchError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidOptions(error) => Some(error),
            Self::ChunkFailed { error, .. } => Some(error),
        }
    }
}

impl BatchOptionsError {
    /// Converts invalid batch configuration into the shared Core error
    /// contract.
    #[must_use]
    pub fn into_global_error(self, context: ErrorContext) -> GlobalError {
        GlobalError {
            code: ErrorCode::new("INDEXING.BATCH.001")
                .expect("Indexing error code is statically valid"),
            owner: ErrorOwner::new("INDEXING").expect("Indexing error owner is statically valid"),
            version: CoreVersion::new(1, 0, 0),
            class: ErrorClass::Validation,
            severity: Severity::Error,
            retryability: Retryability::NonRetryable,
            message: self.to_string(),
            details: Vec::new(),
            solution_reference: None,
            context,
            cause: None,
        }
    }
}

impl BatchChunkError {
    /// Converts a bounded-chunk failure into the shared Core error contract.
    #[must_use]
    pub fn into_global_error(self, context: ErrorContext) -> GlobalError {
        match self {
            Self::Update(error) => error.into_global_error(context),
            Self::Journal(error) => error.into_global_error(context),
        }
    }
}

impl BatchError {
    /// Converts a bounded-batch failure into the shared Core error contract.
    #[must_use]
    pub fn into_global_error(self, context: ErrorContext) -> GlobalError {
        match self {
            Self::InvalidOptions(error) => error.into_global_error(context),
            Self::ChunkFailed {
                chunk_index,
                error,
                committed,
            } => {
                let mut global = error.into_global_error(context);

                for (key, value) in [
                    ("batch_chunk", chunk_index.to_string()),
                    ("completed_chunks", committed.completed_chunks().to_string()),
                    (
                        "applied_mutations",
                        committed.applied_mutations().to_string(),
                    ),
                ] {
                    if let Some(detail) = nizaam_core::error::DiagnosticDetail::new(key, value) {
                        global = global.with_detail(detail);
                    }
                }

                global
            }
        }
    }
}

impl BatchExecutor {
    /// Core-error result adapter for bounded batch execution.
    #[allow(clippy::result_large_err)]
    pub fn execute_result<I>(
        &self,
        candidate: &BuildCandidate,
        journal: &UpdateJournal,
        mutations: I,
        options: BatchOptions,
        context: ErrorContext,
    ) -> IndexingResult<BatchResult>
    where
        I: IntoIterator<Item = IndexMutation>,
    {
        self.execute(candidate, journal, mutations, options)
            .map_err(|error| ErrorEvent::new(error.into_global_error(context)))
    }

    /// Core-error result adapter for execution of one bounded chunk.
    #[allow(clippy::result_large_err)]
    pub fn execute_one_chunk_result<I>(
        &self,
        candidate: &BuildCandidate,
        journal: &UpdateJournal,
        mutations: I,
        options: BatchOptions,
        context: ErrorContext,
    ) -> IndexingResult<BatchResult>
    where
        I: IntoIterator<Item = IndexMutation>,
    {
        self.execute_one_chunk(candidate, journal, mutations, options)
            .map_err(|error| ErrorEvent::new(error.into_global_error(context)))
    }
}

/// Stateless bounded batch executor for logical candidate updates.
///
/// The executor operates functionally: the supplied candidate and journal are
/// never mutated in place. Successful execution returns new committed state.
/// If a later chunk fails, the error carries the state committed by earlier
/// chunks while leaving the original inputs untouched.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BatchExecutor {
    updater: CandidateUpdater,
}

impl BatchExecutor {
    /// Creates a stateless bounded batch executor.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            updater: CandidateUpdater::new(),
        }
    }

    /// Applies a logical mutation stream in bounded transactional chunks.
    ///
    /// Each chunk is collected only up to the caller-supplied bound, then
    /// applied atomically to the current candidate. The update journal is
    /// cloned for the chunk as well; the candidate and journal are committed
    /// together only after the complete chunk has succeeded.
    ///
    /// This preserves:
    ///
    /// ```text
    /// successful chunk
    ///     → committed
    ///
    /// failing chunk
    ///     → no partial commit
    ///
    /// later input
    ///     → not processed after failure
    /// ```
    ///
    /// No publication occurs here. The returned candidate remains an
    /// unpublished logical candidate for the later lifecycle/publication
    /// layer.
    #[allow(clippy::result_large_err)]
    pub fn execute<I>(
        &self,
        candidate: &BuildCandidate,
        journal: &UpdateJournal,
        mutations: I,
        options: BatchOptions,
    ) -> Result<BatchResult, BatchError>
    where
        I: IntoIterator<Item = IndexMutation>,
    {
        let chunk_size = options.chunk_size().get();
        let mut input = mutations.into_iter();
        let mut committed = BatchResult::new(candidate.clone(), journal.clone());
        let mut chunk_index = 0usize;

        loop {
            let chunk: Vec<IndexMutation> = input.by_ref().take(chunk_size).collect();

            if chunk.is_empty() {
                break;
            }

            let candidate_before_chunk = committed.candidate.clone();
            let journal_before_chunk = committed.journal.clone();

            let updated_candidate = self
                .updater
                .apply_mutations(&candidate_before_chunk, chunk.iter().cloned())
                .map_err(|error| BatchError::ChunkFailed {
                    chunk_index,
                    error: BatchChunkError::Update(error),
                    committed: committed.clone(),
                })?;

            let mut updated_journal = journal_before_chunk.clone();

            for mutation in chunk.iter().cloned() {
                if let Err(error) = updated_journal.append(mutation) {
                    return Err(BatchError::ChunkFailed {
                        chunk_index,
                        error: BatchChunkError::Journal(error),
                        committed,
                    });
                }
            }

            committed.record_committed_chunk(chunk.len(), updated_candidate, updated_journal);

            chunk_index += 1;
        }

        Ok(committed)
    }

    /// Applies a single bounded chunk without consuming more than the caller
    /// supplied chunk size from the mutation stream.
    ///
    /// This convenience method uses the same per-chunk transactional
    /// semantics as [`Self::execute`], but is useful when a higher-level
    /// caller already performs its own bounded grouping.
    #[allow(clippy::result_large_err)]
    pub fn execute_one_chunk<I>(
        &self,
        candidate: &BuildCandidate,
        journal: &UpdateJournal,
        mutations: I,
        options: BatchOptions,
    ) -> Result<BatchResult, BatchError>
    where
        I: IntoIterator<Item = IndexMutation>,
    {
        let chunk_size = options.chunk_size().get();
        let chunk: Vec<IndexMutation> = mutations.into_iter().take(chunk_size).collect();

        if chunk.is_empty() {
            return Ok(BatchResult::new(candidate.clone(), journal.clone()));
        }

        let updated_candidate = self
            .updater
            .apply_mutations(candidate, chunk.iter().cloned())
            .map_err(|error| BatchError::ChunkFailed {
                chunk_index: 0,
                error: BatchChunkError::Update(error),
                committed: BatchResult::new(candidate.clone(), journal.clone()),
            })?;

        let mut updated_journal = journal.clone();

        for mutation in chunk.iter().cloned() {
            if let Err(error) = updated_journal.append(mutation) {
                return Err(BatchError::ChunkFailed {
                    chunk_index: 0,
                    error: BatchChunkError::Journal(error),
                    committed: BatchResult::new(candidate.clone(), journal.clone()),
                });
            }
        }

        let mut result = BatchResult::new(updated_candidate, updated_journal);
        result.record_committed_chunk(
            chunk.len(),
            result.candidate.clone(),
            result.journal.clone(),
        );

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::{IndexDefinitionId, IndexDefinitionIdentity, IndexId, IndexNamespace};
    use crate::index::{
        ConsistencyRequirement, IndexDefinition, IndexEntry, IndexFamily, IndexVersion,
        IndexVersionId, KeyDefinition, KeyMaterial, ObjectReference, TargetReferenceType,
        Uniqueness,
    };

    fn index_id(seed: u8) -> IndexId {
        IndexId::from_bytes([seed; 64])
    }

    fn version_id(value: &str) -> IndexVersionId {
        IndexVersionId::new(value).expect("test version ID should be valid")
    }

    fn definition(uniqueness: Uniqueness) -> IndexDefinition {
        let identity = IndexDefinitionIdentity::new(
            IndexDefinitionId::new("documents.v1").expect("definition ID should be valid"),
            IndexNamespace::new("search.documents").expect("namespace should be valid"),
            IndexFamily::Inverted,
        );

        IndexDefinition::new(
            identity,
            KeyDefinition::new(["term"]).expect("key definition should be valid"),
            TargetReferenceType::new("source.document")
                .expect("target reference type should be valid"),
            uniqueness,
            ConsistencyRequirement::new("logical-v1")
                .expect("consistency requirement should be valid"),
            Some(crate::index::SourceVersion::new("source-v1").expect("source should be valid")),
            Some(crate::index::SchemaVersion::new("schema-v1").expect("schema should be valid")),
        )
        .expect("definition should be valid")
    }

    fn candidate(uniqueness: Uniqueness, entries: Vec<IndexEntry>) -> BuildCandidate {
        BuildCandidate::from_parts(
            index_id(0x11),
            definition(uniqueness),
            IndexVersion::with_metadata(
                version_id("index-v1"),
                Some(
                    crate::index::SourceVersion::new("source-v1").expect("source should be valid"),
                ),
                Some(
                    crate::index::SchemaVersion::new("schema-v1").expect("schema should be valid"),
                ),
                None,
            )
            .expect("version should be valid"),
            entries,
        )
    }

    fn entry(key: &str, reference: &str) -> IndexEntry {
        IndexEntry::new(
            KeyMaterial::text(key),
            ObjectReference::new("documents", reference).expect("object reference should be valid"),
        )
        .expect("entry should be valid")
    }

    fn target(reference: &str) -> ObjectReference {
        ObjectReference::new("documents", reference).expect("object reference should be valid")
    }

    fn insert(key: &str, reference: &str) -> IndexMutation {
        IndexMutation::Insert(entry(key, reference))
    }

    fn delete(reference: &str) -> IndexMutation {
        IndexMutation::Delete(super::super::update::DeleteSelector::Target(target(
            reference,
        )))
    }

    fn options(size: usize) -> BatchOptions {
        BatchOptions::new(size).expect("test batch options should be valid")
    }

    #[test]
    fn rejects_zero_chunk_size() {
        let error = BatchOptions::new(0).expect_err("zero chunk size must be rejected");

        assert_eq!(error, BatchOptionsError::ZeroChunkSize);
    }

    #[test]
    fn preserves_the_caller_supplied_chunk_bound() {
        let options = BatchOptions::new(7).expect("positive chunk size should be valid");
        assert_eq!(options.chunk_size().get(), 7);
    }

    #[test]
    fn empty_batch_preserves_candidate_and_journal() {
        let original = candidate(Uniqueness::NonUnique, vec![entry("a", "doc:1")]);
        let journal = UpdateJournal::new();

        let result = BatchExecutor::new()
            .execute(&original, &journal, Vec::<IndexMutation>::new(), options(2))
            .expect("empty batch should succeed");

        assert_eq!(result.candidate(), &original);
        assert_eq!(result.journal(), &journal);
        assert_eq!(result.completed_chunks(), 0);
        assert_eq!(result.applied_mutations(), 0);
    }

    #[test]
    fn one_chunk_commits_all_mutations_transactionally() {
        let original = candidate(Uniqueness::NonUnique, vec![entry("a", "doc:1")]);
        let journal = UpdateJournal::new();

        let result = BatchExecutor::new()
            .execute(
                &original,
                &journal,
                vec![insert("b", "doc:2"), insert("c", "doc:3")],
                options(2),
            )
            .expect("single successful chunk should commit");

        assert_eq!(result.candidate().len(), 3);
        assert_eq!(result.journal().len(), 2);
        assert_eq!(result.completed_chunks(), 1);
        assert_eq!(result.applied_mutations(), 2);
        assert_eq!(original.len(), 1);
        assert!(journal.is_empty());
    }

    #[test]
    fn multiple_chunks_commit_independently() {
        let original = candidate(Uniqueness::NonUnique, Vec::new());
        let journal = UpdateJournal::new();

        let result = BatchExecutor::new()
            .execute(
                &original,
                &journal,
                vec![
                    insert("a", "doc:1"),
                    insert("b", "doc:2"),
                    insert("c", "doc:3"),
                    insert("d", "doc:4"),
                    insert("e", "doc:5"),
                ],
                options(2),
            )
            .expect("all bounded chunks should commit");

        assert_eq!(result.completed_chunks(), 3);
        assert_eq!(result.applied_mutations(), 5);
        assert_eq!(result.candidate().len(), 5);
        assert_eq!(result.journal().len(), 5);
    }

    #[test]
    fn failing_chunk_does_not_partially_commit() {
        let original = candidate(Uniqueness::Unique, vec![entry("existing", "doc:1")]);
        let journal = UpdateJournal::new();

        let error = BatchExecutor::new()
            .execute(
                &original,
                &journal,
                vec![
                    insert("first", "doc:2"),
                    insert("second", "doc:3"),
                    insert("conflict", "doc:4"),
                    insert("conflict", "doc:5"),
                ],
                options(2),
            )
            .expect_err("second chunk should fail because it conflicts internally");

        match error {
            BatchError::ChunkFailed {
                chunk_index,
                error: BatchChunkError::Update(update_error),
                committed,
            } => {
                assert_eq!(chunk_index, 1);
                assert!(matches!(
                    update_error,
                    UpdateError::UniqueKeyConflict { .. }
                ));
                assert_eq!(committed.completed_chunks(), 1);
                assert_eq!(committed.applied_mutations(), 2);
                assert_eq!(committed.candidate().len(), 3);
                assert_eq!(committed.journal().len(), 2);

                // The failing chunk did not leak either mutation.
                assert!(
                    committed
                        .candidate()
                        .entries()
                        .iter()
                        .all(|value| value.target().object_reference() != "doc:4")
                );
                assert!(
                    committed
                        .candidate()
                        .entries()
                        .iter()
                        .all(|value| value.target().object_reference() != "doc:5")
                );
            }
            other => panic!("unexpected batch error: {other:?}"),
        }

        assert_eq!(original.len(), 1);
        assert!(journal.is_empty());
    }

    #[test]
    fn failing_chunk_stops_processing_later_input() {
        let original = candidate(Uniqueness::Unique, vec![entry("existing", "doc:1")]);
        let journal = UpdateJournal::new();

        let error = BatchExecutor::new()
            .execute(
                &original,
                &journal,
                vec![
                    insert("ok", "doc:2"),
                    insert("existing", "doc:3"),
                    insert("later", "doc:4"),
                ],
                options(2),
            )
            .expect_err("the first chunk should fail");

        match error {
            BatchError::ChunkFailed {
                chunk_index,
                committed,
                ..
            } => {
                assert_eq!(chunk_index, 0);
                assert_eq!(committed.completed_chunks(), 0);
                assert_eq!(committed.applied_mutations(), 0);
                assert_eq!(committed.candidate(), &original);
                assert!(committed.journal().is_empty());
            }
            other => panic!("unexpected batch error: {other:?}"),
        }
    }

    #[test]
    fn journal_sequence_follows_committed_mutation_order() {
        let original = candidate(Uniqueness::NonUnique, Vec::new());
        let mut journal = UpdateJournal::new();
        journal
            .append(insert("prior", "doc:0"))
            .expect("prior journal append should succeed");

        let result = BatchExecutor::new()
            .execute(
                &original,
                &journal,
                vec![insert("a", "doc:1"), insert("b", "doc:2")],
                options(2),
            )
            .expect("batch should succeed");

        let sequences: Vec<_> = result
            .journal()
            .records()
            .iter()
            .map(|record| record.sequence().value())
            .collect();

        assert_eq!(sequences, [1, 2, 3]);
    }

    #[test]
    fn delete_mutations_are_supported_inside_batches() {
        let original = candidate(
            Uniqueness::NonUnique,
            vec![entry("a", "doc:1"), entry("b", "doc:2")],
        );
        let journal = UpdateJournal::new();

        let result = BatchExecutor::new()
            .execute(&original, &journal, vec![delete("doc:1")], options(1))
            .expect("delete batch should succeed");

        assert_eq!(result.candidate().len(), 1);
        assert_eq!(
            result.candidate().entries()[0].target().object_reference(),
            "doc:2"
        );
        assert_eq!(result.journal().len(), 1);
    }

    #[test]
    fn execute_one_chunk_uses_the_same_transactional_boundary() {
        let original = candidate(Uniqueness::NonUnique, Vec::new());
        let journal = UpdateJournal::new();

        let result = BatchExecutor::new()
            .execute_one_chunk(
                &original,
                &journal,
                vec![insert("a", "doc:1"), insert("b", "doc:2")],
                options(2),
            )
            .expect("one bounded chunk should succeed");

        assert_eq!(result.completed_chunks(), 1);
        assert_eq!(result.applied_mutations(), 2);
        assert_eq!(result.candidate().len(), 2);
        assert_eq!(result.journal().len(), 2);
    }

    #[test]
    fn execute_one_chunk_does_not_apply_input_beyond_the_bound() {
        let original = candidate(Uniqueness::NonUnique, Vec::new());
        let journal = UpdateJournal::new();

        let result = BatchExecutor::new()
            .execute_one_chunk(
                &original,
                &journal,
                vec![
                    insert("a", "doc:1"),
                    insert("b", "doc:2"),
                    insert("c", "doc:3"),
                ],
                options(2),
            )
            .expect("bounded chunk should succeed");

        assert_eq!(result.candidate().len(), 2);
        assert_eq!(result.journal().len(), 2);
    }

    #[test]
    fn batch_result_round_trip_preserves_committed_state() {
        let original = candidate(Uniqueness::NonUnique, Vec::new());
        let journal = UpdateJournal::new();

        let result = BatchExecutor::new()
            .execute(&original, &journal, vec![insert("a", "doc:1")], options(1))
            .expect("batch should succeed");

        let (candidate, journal, chunks, mutations) = result.into_parts();

        assert_eq!(candidate.len(), 1);
        assert_eq!(journal.len(), 1);
        assert_eq!(chunks, 1);
        assert_eq!(mutations, 1);
    }

    #[test]
    fn batch_error_display_identifies_the_failing_chunk() {
        let original = candidate(Uniqueness::Unique, vec![entry("a", "doc:1")]);

        let error = BatchExecutor::new()
            .execute(
                &original,
                &UpdateJournal::new(),
                vec![insert("a", "doc:2")],
                options(1),
            )
            .expect_err("unique conflict should fail");

        assert!(error.to_string().contains("batch chunk 0 failed"));
    }
}
