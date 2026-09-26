//! Level 3 integration coverage for the Phase 3 logical build pipeline.
//!
//! These tests exercise the public crate boundary across construction, logical
//! updates, bounded batches, isolated rebuild/replay, candidate lifecycle, and
//! explicit publication. They intentionally do not introduce providers,
//! storage, query execution, scheduler behavior, or domain semantics.

use nizaam_indexing::build::{
    BatchError, BatchExecutor, BatchOptions, BuildCandidate, BuildInput, BuildSnapshot,
    CandidateUpdater, IndexBuilder, IndexMutation, IndexPublisher, IndexRebuilder, RebuildError,
    UpdateError, UpdateJournal, UpdateSequence,
};
use nizaam_indexing::identity::{
    IndexDefinitionId, IndexDefinitionIdentity, IndexId, IndexNamespace,
};
use nizaam_indexing::index::{
    ConsistencyRequirement, IndexDefinition, IndexEntry, IndexFamily, IndexVersion, IndexVersionId,
    KeyDefinition, KeyMaterial, ObjectReference, SchemaVersion, SourceVersion, TargetReferenceType,
    Uniqueness, VersionLifecycle,
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
    IndexDefinition::new(
        IndexDefinitionIdentity::new(
            IndexDefinitionId::new("phase3.build.documents")
                .expect("definition ID should be valid"),
            IndexNamespace::new("phase3.build").expect("namespace should be valid"),
            IndexFamily::Inverted,
        ),
        KeyDefinition::new(["term"]).expect("key definition should be valid"),
        TargetReferenceType::new("documents.document")
            .expect("target reference type should be valid"),
        uniqueness,
        ConsistencyRequirement::new("logical-v1").expect("consistency requirement should be valid"),
        Some(source_version("source-v1")),
        Some(schema_version("schema-v1")),
    )
    .expect("definition should be valid")
}

fn entry(key: &str, reference: &str) -> IndexEntry {
    IndexEntry::new(
        KeyMaterial::text(key),
        ObjectReference::new("documents", reference).expect("object reference should be valid"),
    )
    .expect("entry should be valid")
}

fn build_candidate(seed: u8, version: &str, entries: Vec<IndexEntry>) -> BuildCandidate {
    IndexBuilder::new()
        .build(BuildInput::new(
            index_id(seed),
            definition(Uniqueness::NonUnique),
            version_id(version),
            BuildSnapshot::with_versions(
                Some(source_version("source-v1")),
                Some(schema_version("schema-v1")),
                entries,
            ),
        ))
        .expect("candidate should build")
}

#[test]
fn build_pipeline_constructs_and_populates_an_unpublished_candidate() {
    let candidate = build_candidate(
        0x11,
        "index-v1",
        vec![entry("alpha", "doc:1"), entry("beta", "doc:2")],
    );

    assert_eq!(candidate.index_id(), &index_id(0x11));
    assert_eq!(candidate.version().id().as_str(), "index-v1");
    assert_eq!(candidate.len(), 2);
    assert_eq!(candidate.entries()[0].key(), &KeyMaterial::text("alpha"));
    assert_eq!(candidate.entries()[1].target().object_reference(), "doc:2");
}

#[test]
fn single_update_is_functional_and_does_not_publish_automatically() {
    let base = build_candidate(0x22, "index-v1", vec![entry("alpha", "doc:1")]);
    let updater = CandidateUpdater::new();
    let mut journal = UpdateJournal::new();

    let update_candidate = updater
        .create_candidate(&base, version_id("index-v2"), None, None)
        .expect("candidate creation should preserve the logical base state");

    let (updated, sequence) = updater
        .apply_and_record(
            &update_candidate,
            IndexMutation::Insert(entry("beta", "doc:2")),
            &mut journal,
        )
        .expect("logical update should succeed");

    assert_eq!(base.version().id().as_str(), "index-v1");
    assert_eq!(base.len(), 1);
    assert_eq!(update_candidate.version().id().as_str(), "index-v2");
    assert_eq!(updated.version().id().as_str(), "index-v2");
    assert_eq!(updated.len(), 2);
    assert_eq!(sequence, UpdateSequence::new(1));
    assert_eq!(journal.current_sequence(), UpdateSequence::new(1));
}

#[test]
fn delete_update_removes_only_the_selected_logical_target() {
    let base = build_candidate(
        0x2a,
        "index-v1",
        vec![entry("alpha", "doc:1"), entry("beta", "doc:2")],
    );
    let updated = CandidateUpdater::new()
        .apply(
            &base,
            IndexMutation::Delete(nizaam_indexing::build::DeleteSelector::target(
                ObjectReference::new("documents", "doc:1")
                    .expect("object reference should be valid"),
            )),
        )
        .expect("delete should succeed");

    assert_eq!(base.len(), 2);
    assert_eq!(updated.len(), 1);
    assert_eq!(updated.entries()[0].target().object_reference(), "doc:2");
}

#[test]
fn bounded_batch_commits_complete_chunks_and_preserves_transaction_boundaries() {
    let base = build_candidate(0x33, "index-v1", vec![entry("alpha", "doc:1")]);
    let journal = UpdateJournal::new();

    let result = BatchExecutor::new()
        .execute(
            &base,
            &journal,
            vec![
                IndexMutation::Insert(entry("beta", "doc:2")),
                IndexMutation::Insert(entry("gamma", "doc:3")),
                IndexMutation::Insert(entry("delta", "doc:4")),
                IndexMutation::Insert(entry("epsilon", "doc:5")),
                IndexMutation::Insert(entry("zeta", "doc:6")),
            ],
            BatchOptions::new(2).expect("chunk size should be valid"),
        )
        .expect("all bounded chunks should commit");

    assert_eq!(result.completed_chunks(), 3);
    assert_eq!(result.applied_mutations(), 5);
    assert_eq!(result.candidate().len(), 6);
    assert_eq!(result.journal().len(), 5);
    assert_eq!(result.journal().current_sequence(), UpdateSequence::new(5));

    // The original inputs are immutable from the executor's point of view.
    assert!(journal.is_empty());
    assert_eq!(base.len(), 1);
}

#[test]
fn failing_batch_chunk_returns_only_prior_commits() {
    let base = build_candidate(0x44, "index-v1", vec![entry("alpha", "doc:1")]);

    let result = BatchExecutor::new().execute(
        &base,
        &UpdateJournal::new(),
        vec![
            IndexMutation::Insert(entry("beta", "doc:2")),
            IndexMutation::Insert(entry("gamma", "doc:3")),
            IndexMutation::Insert(entry("gamma", "doc:3")),
        ],
        BatchOptions::new(2).expect("chunk size should be valid"),
    );

    let BatchError::ChunkFailed {
        chunk_index,
        error,
        committed,
    } = result.expect_err("second chunk must fail on the duplicate entry")
    else {
        panic!("expected a transactional chunk failure");
    };

    assert_eq!(chunk_index, 1);
    assert!(matches!(
        error,
        nizaam_indexing::build::BatchChunkError::Update(UpdateError::DuplicateEntry(_))
    ));
    assert_eq!(committed.completed_chunks(), 1);
    assert_eq!(committed.applied_mutations(), 2);
    assert_eq!(committed.candidate().len(), 3);
    assert_eq!(committed.journal().len(), 2);
    assert_eq!(
        committed.journal().current_sequence(),
        UpdateSequence::new(2)
    );
    assert_eq!(base.len(), 1);
}

#[test]
fn rebuild_captures_a_boundary_replays_newer_updates_and_stops_at_a_consistency_point() {
    let mut journal = UpdateJournal::new();
    journal
        .append(IndexMutation::Insert(entry("before", "doc:before")))
        .expect("pre-capture update should succeed");

    let rebuilder = IndexRebuilder::new();
    let progress = rebuilder
        .start(
            nizaam_indexing::build::RebuildInput::new(
                index_id(0x55),
                definition(Uniqueness::NonUnique),
                version_id("index-v2"),
                BuildSnapshot::with_versions(
                    Some(source_version("source-v1")),
                    Some(schema_version("schema-v1")),
                    vec![entry("alpha", "doc:1")],
                ),
                Some(version_id("index-v1")),
                Some(version_id("index-v1")),
            ),
            &journal,
        )
        .expect("rebuild should start from the observed active lineage");

    assert_eq!(progress.captured_sequence(), UpdateSequence::new(1));
    assert_eq!(progress.replayed_through(), UpdateSequence::new(1));
    assert_eq!(progress.replayed_updates(), 0);

    journal
        .append(IndexMutation::Insert(entry("after", "doc:after")))
        .expect("post-capture update should succeed");

    assert!(!rebuilder.is_caught_up(&progress, &journal));

    let progress = rebuilder
        .replay(&progress, &journal)
        .expect("newer updates should replay onto the isolated candidate");
    let result = rebuilder
        .finish(&progress, &journal)
        .expect("rebuild should finish once the journal boundary is caught up");

    assert_eq!(result.captured_sequence(), UpdateSequence::new(1));
    assert_eq!(result.replayed_through(), UpdateSequence::new(2));
    assert_eq!(result.replayed_updates(), 1);
    assert_eq!(result.candidate().len(), 2);
    assert_eq!(result.candidate().version().id().as_str(), "index-v2");
}

#[test]
fn rebuild_refuses_to_finish_while_the_journal_is_ahead() {
    let mut journal = UpdateJournal::new();
    let rebuilder = IndexRebuilder::new();

    let progress = rebuilder
        .start(
            nizaam_indexing::build::RebuildInput::new(
                index_id(0x66),
                definition(Uniqueness::NonUnique),
                version_id("index-v2"),
                BuildSnapshot::with_versions(
                    Some(source_version("source-v1")),
                    Some(schema_version("schema-v1")),
                    vec![entry("alpha", "doc:1")],
                ),
                Some(version_id("index-v1")),
                Some(version_id("index-v1")),
            ),
            &journal,
        )
        .expect("rebuild should start");

    journal
        .append(IndexMutation::Insert(entry("late", "doc:late")))
        .expect("late update should be recorded");

    let error = rebuilder
        .finish(&progress, &journal)
        .expect_err("finish must reject a candidate that has not replayed the newest update");

    assert!(matches!(error, RebuildError::NotCaughtUp { .. }));
}

#[test]
fn explicit_publication_is_the_only_boundary_that_switches_the_local_active_value() {
    let active = build_candidate(0x77, "index-v1", vec![entry("alpha", "doc:1")]);
    let candidate = build_candidate(
        0x77,
        "index-v2",
        vec![entry("alpha", "doc:1"), entry("beta", "doc:2")],
    );
    let publisher = IndexPublisher::new();

    let prepared = publisher
        .prepare(
            candidate,
            active.definition(),
            Some(version_id("index-v1")),
            Some(version_id("index-v1")),
        )
        .expect("candidate should be eligible against the observed active version");

    // Nothing in preparation changes the caller's active value.
    assert_eq!(active.version().id().as_str(), "index-v1");

    let publication = publisher
        .publish(prepared, Some(active.clone()))
        .expect("publication should cross the explicit boundary");

    assert_eq!(publication.active().version().id().as_str(), "index-v2");
    assert_eq!(
        publication
            .previous_active()
            .expect("previous active should be preserved")
            .version()
            .id()
            .as_str(),
        "index-v1"
    );
    assert_eq!(active.version().id().as_str(), "index-v1");
}

#[test]
fn older_prepared_candidate_cannot_overwrite_a_newer_published_candidate() {
    let v1 = build_candidate(0x88, "index-v1", vec![entry("alpha", "doc:1")]);
    let v2 = build_candidate(
        0x88,
        "index-v2",
        vec![entry("alpha", "doc:1"), entry("beta", "doc:2")],
    );
    let v3 = build_candidate(
        0x88,
        "index-v3",
        vec![entry("alpha", "doc:1"), entry("gamma", "doc:3")],
    );
    let publisher = IndexPublisher::new();

    let prepared_v2 = publisher
        .prepare(
            v2,
            v1.definition(),
            Some(version_id("index-v1")),
            Some(version_id("index-v1")),
        )
        .expect("v2 should prepare against v1");

    let prepared_v3 = publisher
        .prepare(
            v3,
            v1.definition(),
            Some(version_id("index-v1")),
            Some(version_id("index-v1")),
        )
        .expect("v3 should also prepare independently against v1");

    let published_v3 = publisher
        .publish(prepared_v3, Some(v1))
        .expect("v3 should publish first");

    let newer_active = published_v3.active().clone();
    let error = publisher
        .publish(prepared_v2, Some(newer_active.clone()))
        .expect_err("v2 must be rejected after v3 becomes active");

    assert!(matches!(
        error,
        nizaam_indexing::build::PublicationError::ActiveVersionChanged { .. }
    ));
    assert_eq!(newer_active.version().id().as_str(), "index-v3");
}

#[test]
fn version_lifecycle_remains_separate_from_build_and_publication_values() {
    let version = IndexVersion::new(version_id("index-v4"));
    let mut state = nizaam_indexing::index::IndexVersionState::new(version);

    assert_eq!(state.lifecycle(), VersionLifecycle::Building);
    state
        .transition_to(VersionLifecycle::Validating)
        .expect("building should transition to validating");
    state
        .mark_ready()
        .expect("validating should transition to ready");

    assert_eq!(state.lifecycle(), VersionLifecycle::Ready);
    assert!(!state.is_published());

    state
        .mark_published()
        .expect("ready should transition to published");
    assert!(state.is_published());
}
