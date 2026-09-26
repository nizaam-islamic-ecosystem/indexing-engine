//! Public build orchestration boundary for Phase 3.
//!
//! This module composes the five logical build subsystems introduced by Phase 3:
//!
//! - [`builder`] constructs isolated logical candidates from source snapshots.
//! - [`update`] applies incremental logical mutations and records their journal
//!   sequence.
//! - [`batch`] executes bounded transactional mutation chunks.
//! - [`rebuild`] reconstructs an isolated candidate and replays concurrent
//!   logical updates to a consistency point.
//! - [`publication`] validates and prepares the explicit `READY → ACTIVE`
//!   publication boundary.
//!
//! The module root re-exports the public contracts of those child modules so
//! callers can use a single `build` boundary without depending on the internal
//! file layout. The child modules remain public for explicit submodule access.
//!
//! This boundary owns no additional build state, lifecycle registry,
//! synchronization primitive, provider implementation, storage, query
//! execution, or runtime. Active-version ownership remains with the later
//! Indexing lifecycle coordination layer, while Core remains responsible for
//! execution context, cancellation, deadlines, and capability/runtime
//! mechanisms.
//!
//! The module-level tests intentionally exercise interactions across the child
//! modules rather than duplicating their file-local unit tests. They therefore
//! serve as Phase 3 Level 2 tests for the public build boundary.

pub mod batch;
pub mod builder;
pub mod publication;
pub mod rebuild;
pub mod update;

pub use batch::{
    BatchChunkError, BatchError, BatchExecutor, BatchOptions, BatchOptionsError, BatchResult,
};
pub use builder::{BuildCandidate, BuildError, BuildInput, BuildSnapshot, IndexBuilder};
pub use publication::{IndexPublisher, PublicationCandidate, PublicationError, PublicationResult};
pub use rebuild::{IndexRebuilder, RebuildError, RebuildInput, RebuildProgress, RebuildResult};
pub use update::{
    CandidateUpdater, DeleteSelector, IndexMutation, UpdateError, UpdateJournal,
    UpdateJournalError, UpdateRecord, UpdateSequence,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::{IndexDefinitionId, IndexDefinitionIdentity, IndexId, IndexNamespace};
    use crate::index::{
        ConsistencyRequirement, IndexDefinition, IndexEntry, IndexFamily, IndexVersion,
        IndexVersionId, KeyDefinition, KeyMaterial, ObjectReference, SchemaVersion, SourceVersion,
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

    fn definition(uniqueness: Uniqueness) -> IndexDefinition {
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
            uniqueness,
            ConsistencyRequirement::new("logical-v1")
                .expect("test consistency requirement should be valid"),
            Some(source_version("source-v1")),
            Some(schema_version("schema-v1")),
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

    fn build_candidate(index: IndexId, version: &str, entries: Vec<IndexEntry>) -> BuildCandidate {
        IndexBuilder::new()
            .build(BuildInput::new(
                index,
                definition(Uniqueness::NonUnique),
                version_id(version),
                BuildSnapshot::with_versions(
                    Some(source_version("source-v1")),
                    Some(schema_version("schema-v1")),
                    entries,
                ),
            ))
            .expect("test candidate should build")
    }

    #[test]
    fn build_boundary_exports_are_usable_across_builder_and_update_modules() {
        let base = build_candidate(index_id(0x11), "index-v1", vec![entry("alpha", "doc:1")]);
        let mut journal = UpdateJournal::new();

        let applied: Result<(BuildCandidate, UpdateSequence), UpdateError> =
            CandidateUpdater::new().apply_and_record(
                &base,
                IndexMutation::Insert(entry("beta", "doc:2")),
                &mut journal,
            );

        let (updated, sequence) = applied.expect("cross-module update should succeed");
        let logical_version = IndexVersion::new(version_id("index-v1"));

        assert_eq!(logical_version.id(), base.version().id());
        assert_eq!(sequence, UpdateSequence::new(1));
        assert_eq!(journal.current_sequence(), sequence);
        assert_eq!(journal.records()[0].sequence(), sequence);
        assert_eq!(updated.len(), 2);
        assert_eq!(base.len(), 1);
    }

    #[test]
    fn build_boundary_composes_builder_update_and_bounded_batch_execution() {
        let base = build_candidate(index_id(0x22), "index-v1", vec![entry("alpha", "doc:1")]);
        let updater = CandidateUpdater::new();
        let mut journal = UpdateJournal::new();

        let (updated, first_sequence) = updater
            .apply_and_record(
                &base,
                IndexMutation::Insert(entry("beta", "doc:2")),
                &mut journal,
            )
            .expect("first logical update should succeed");

        let batch = BatchExecutor::new()
            .execute(
                &updated,
                &journal,
                vec![
                    IndexMutation::Insert(entry("gamma", "doc:3")),
                    IndexMutation::Insert(entry("delta", "doc:4")),
                    IndexMutation::Insert(entry("epsilon", "doc:5")),
                ],
                BatchOptions::new(2).expect("test chunk size should be valid"),
            )
            .expect("bounded batch should succeed");

        assert_eq!(first_sequence, UpdateSequence::new(1));
        assert_eq!(batch.completed_chunks(), 2);
        assert_eq!(batch.applied_mutations(), 3);
        assert_eq!(batch.candidate().len(), 5);
        assert_eq!(batch.journal().current_sequence(), UpdateSequence::new(4));
        assert_eq!(batch.journal().len(), 4);
        assert_eq!(base.len(), 1);
        assert_eq!(journal.len(), 1);
    }

    #[test]
    fn build_boundary_composes_isolated_rebuild_with_journal_replay() {
        let mut journal = UpdateJournal::new();

        journal
            .append(IndexMutation::Insert(entry("before", "doc:before")))
            .expect("pre-captured journal update should succeed");

        let input = RebuildInput::new(
            index_id(0x33),
            definition(Uniqueness::NonUnique),
            version_id("index-v2"),
            BuildSnapshot::with_versions(
                Some(source_version("source-v1")),
                Some(schema_version("schema-v1")),
                vec![entry("alpha", "doc:1")],
            ),
            Some(version_id("index-v1")),
            Some(version_id("index-v1")),
        );

        let rebuilder = IndexRebuilder::new();
        let progress = rebuilder
            .start(input, &journal)
            .expect("rebuild should start from the observed active lineage");

        assert_eq!(progress.captured_sequence(), UpdateSequence::new(1));
        assert_eq!(progress.replayed_through(), UpdateSequence::new(1));
        assert_eq!(progress.replayed_updates(), 0);
        assert_eq!(progress.candidate().len(), 1);

        journal
            .append(IndexMutation::Insert(entry("after", "doc:after")))
            .expect("post-capture journal update should succeed");

        assert!(!progress.is_caught_up(&journal));

        let progress = rebuilder
            .replay(&progress, &journal)
            .expect("newer journal updates should replay onto the isolated candidate");

        let result: Result<RebuildResult, RebuildError> = rebuilder.finish(&progress, &journal);

        let result = result.expect("rebuild should finish at the journal consistency point");

        assert_eq!(result.captured_sequence(), UpdateSequence::new(1));
        assert_eq!(result.replayed_through(), UpdateSequence::new(2));
        assert_eq!(result.replayed_updates(), 1);
        assert_eq!(result.candidate().len(), 2);
        assert_eq!(result.candidate().version().id().as_str(), "index-v2");
    }

    #[test]
    fn build_boundary_composes_rebuild_result_with_explicit_publication() {
        let rebuilder = IndexRebuilder::new();
        let rebuilt = rebuilder
            .rebuild(
                RebuildInput::new(
                    index_id(0x44),
                    definition(Uniqueness::NonUnique),
                    version_id("index-v2"),
                    BuildSnapshot::with_versions(
                        Some(source_version("source-v1")),
                        Some(schema_version("schema-v1")),
                        vec![entry("alpha", "doc:1"), entry("beta", "doc:2")],
                    ),
                    Some(version_id("index-v1")),
                    Some(version_id("index-v1")),
                ),
                &UpdateJournal::new(),
            )
            .expect("stable rebuild should produce an unpublished candidate");

        let previous_active =
            build_candidate(index_id(0x44), "index-v1", vec![entry("alpha", "doc:1")]);

        let publisher = IndexPublisher::new();
        let prepared: PublicationCandidate = publisher
            .prepare(
                rebuilt.candidate().clone(),
                rebuilt.candidate().definition(),
                Some(version_id("index-v1")),
                Some(version_id("index-v1")),
            )
            .expect("rebuilt candidate should pass publication preparation");

        let result: Result<PublicationResult, PublicationError> =
            publisher.publish(prepared, Some(previous_active.clone()));

        let result = result.expect("publication should succeed at the explicit boundary");

        assert_eq!(result.active().version().id().as_str(), "index-v2");
        assert_eq!(result.previous_active(), Some(&previous_active));
    }
}
