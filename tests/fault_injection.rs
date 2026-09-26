//! Level 3 deterministic fault-injection coverage for Phase 3.
//!
//! Faults are injected through real public logical APIs using invalid or
//! conflicting inputs. No provider/storage implementation is fabricated here,
//! because those boundaries are deliberately deferred.

use nizaam_indexing::build::{
    BatchChunkError, BatchError, BatchExecutor, BatchOptions, BuildInput, BuildSnapshot,
    CandidateUpdater, IndexBuilder, IndexMutation, IndexPublisher, IndexRebuilder,
    PublicationError, RebuildError, RebuildInput, UpdateError, UpdateJournal,
};
use nizaam_indexing::identity::{
    IndexDefinitionId, IndexDefinitionIdentity, IndexId, IndexNamespace,
};
use nizaam_indexing::index::{
    ConsistencyRequirement, IndexDefinition, IndexEntry, IndexFamily, IndexVersionId,
    KeyDefinition, KeyMaterial, ObjectReference, SchemaVersion, SourceVersion, TargetReferenceType,
    Uniqueness,
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

fn definition() -> IndexDefinition {
    IndexDefinition::new(
        IndexDefinitionIdentity::new(
            IndexDefinitionId::new("phase3.faults.documents")
                .expect("definition ID should be valid"),
            IndexNamespace::new("phase3.faults").expect("namespace should be valid"),
            IndexFamily::Inverted,
        ),
        KeyDefinition::new(["term"]).expect("key definition should be valid"),
        TargetReferenceType::new("documents.document")
            .expect("target reference type should be valid"),
        Uniqueness::NonUnique,
        ConsistencyRequirement::new("logical-v1").expect("consistency requirement should be valid"),
        Some(source_version("source-v1")),
        Some(schema_version("schema-v1")),
    )
    .expect("definition should be valid")
}

fn entry(key: &str, reference: &str) -> IndexEntry {
    IndexEntry::new(
        KeyMaterial::text(key),
        ObjectReference::new("documents", reference).expect("reference should be valid"),
    )
    .expect("entry should be valid")
}

fn candidate(
    seed: u8,
    version: &str,
    entries: Vec<IndexEntry>,
) -> nizaam_indexing::build::BuildCandidate {
    IndexBuilder::new()
        .build(BuildInput::new(
            index_id(seed),
            definition(),
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
fn build_fault_source_version_mismatch_does_not_produce_a_candidate() {
    let error = IndexBuilder::new()
        .build(BuildInput::new(
            index_id(0x11),
            definition(),
            version_id("index-v1"),
            BuildSnapshot::with_versions(
                Some(source_version("source-v2")),
                Some(schema_version("schema-v1")),
                vec![entry("alpha", "doc:1")],
            ),
        ))
        .expect_err("mismatched source state must fail construction");

    assert!(matches!(
        error,
        nizaam_indexing::build::BuildError::Versioning(
            nizaam_indexing::consistency::VersioningError::SourceVersionMismatch { .. }
        )
    ));
}

#[test]
fn populate_update_fault_cannot_mutate_the_supplied_candidate() {
    let base = candidate(0x22, "index-v1", vec![entry("alpha", "doc:1")]);
    let updater = CandidateUpdater::new();

    let error = updater
        .apply(
            &base,
            IndexMutation::Update {
                target: ObjectReference::new("documents", "doc:missing")
                    .expect("target reference should be valid"),
                replacement: entry("replacement", "doc:missing"),
            },
        )
        .expect_err("updating an absent target must fail");

    assert!(matches!(error, UpdateError::TargetNotFound(_)));
    assert_eq!(base.len(), 1);
    assert_eq!(base.entries()[0].target().object_reference(), "doc:1");
}

#[test]
fn batch_chunk_fault_preserves_previous_commits_and_excludes_the_failing_chunk() {
    let base = candidate(0x33, "index-v1", vec![entry("alpha", "doc:1")]);

    let error = BatchExecutor::new()
        .execute(
            &base,
            &UpdateJournal::new(),
            vec![
                IndexMutation::Insert(entry("beta", "doc:2")),
                IndexMutation::Insert(entry("gamma", "doc:3")),
                IndexMutation::Insert(entry("gamma", "doc:3")),
            ],
            BatchOptions::new(2).expect("chunk size should be valid"),
        )
        .expect_err("duplicate entry must fail its transaction chunk");

    let BatchError::ChunkFailed {
        chunk_index,
        error,
        committed,
    } = error
    else {
        panic!("expected a chunk failure");
    };

    assert_eq!(chunk_index, 1);
    assert!(matches!(
        error,
        BatchChunkError::Update(UpdateError::DuplicateEntry(_))
    ));
    assert_eq!(committed.candidate().len(), 3);
    assert_eq!(committed.journal().len(), 2);
}

#[test]
fn update_fault_does_not_append_a_journal_record() {
    let base = candidate(0x44, "index-v1", vec![entry("alpha", "doc:1")]);
    let updater = CandidateUpdater::new();
    let mut journal = UpdateJournal::new();

    let error = updater
        .apply_and_record(
            &base,
            IndexMutation::Update {
                target: ObjectReference::new("documents", "doc:missing")
                    .expect("target reference should be valid"),
                replacement: entry("replacement", "doc:missing"),
            },
            &mut journal,
        )
        .expect_err("invalid update must fail before journal append");

    assert!(matches!(error, UpdateError::TargetNotFound(_)));
    assert!(journal.is_empty());
}

#[test]
fn replay_fault_leaves_the_previous_rebuild_progress_untouched() {
    let mut journal = UpdateJournal::new();
    let rebuilder = IndexRebuilder::new();

    let progress = rebuilder
        .start(
            RebuildInput::new(
                index_id(0x55),
                definition(),
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
        .append(IndexMutation::Update {
            target: ObjectReference::new("documents", "doc:missing")
                .expect("target reference should be valid"),
            replacement: entry("replacement", "doc:missing"),
        })
        .expect("journal records logical mutations before replay validation");

    let error = rebuilder
        .replay(&progress, &journal)
        .expect_err("invalid replay mutation must fail");

    assert!(matches!(
        error,
        RebuildError::Update(UpdateError::TargetNotFound(_))
    ));
    assert_eq!(progress.replayed_through().value(), 0);
    assert_eq!(progress.replayed_updates(), 0);
    assert_eq!(progress.candidate().len(), 1);
}

#[test]
fn validation_fault_rejects_an_incompatible_publication_candidate() {
    let base = candidate(0x66, "index-v1", vec![entry("alpha", "doc:1")]);
    let candidate = base.clone();
    let publisher = IndexPublisher::new();

    let mismatched_definition = IndexDefinition::new(
        IndexDefinitionIdentity::new(
            IndexDefinitionId::new("phase3.faults.other-definition")
                .expect("definition ID should be valid"),
            IndexNamespace::new("phase3.faults").expect("namespace should be valid"),
            IndexFamily::Inverted,
        ),
        KeyDefinition::new(["term"]).expect("key definition should be valid"),
        TargetReferenceType::new("documents.document").expect("target type should be valid"),
        Uniqueness::NonUnique,
        ConsistencyRequirement::new("logical-v1").expect("consistency should be valid"),
        Some(source_version("source-v2")),
        Some(schema_version("schema-v1")),
    )
    .expect("definition should be valid");

    let error = publisher
        .prepare(
            candidate,
            &mismatched_definition,
            Some(version_id("index-v1")),
            Some(version_id("index-v1")),
        )
        .expect_err("publication validation must reject source incompatibility");

    assert!(matches!(error, PublicationError::Versioning(_)));
    assert_eq!(base.version().id().as_str(), "index-v1");
}

#[test]
fn publication_fault_preserves_the_newer_active_candidate() {
    let v1 = candidate(0x77, "index-v1", vec![entry("alpha", "doc:1")]);
    let v2 = candidate(
        0x77,
        "index-v2",
        vec![entry("alpha", "doc:1"), entry("beta", "doc:2")],
    );
    let v3 = candidate(
        0x77,
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
        .expect("v2 should prepare");

    let published_v3 = publisher
        .publish(
            publisher
                .prepare(
                    v3,
                    v1.definition(),
                    Some(version_id("index-v1")),
                    Some(version_id("index-v1")),
                )
                .expect("v3 should prepare"),
            Some(v1),
        )
        .expect("v3 should publish");

    let active_v3 = published_v3.active().clone();
    let error = publisher
        .publish(prepared_v2, Some(active_v3.clone()))
        .expect_err("older prepared candidate must be rejected");

    assert!(matches!(
        error,
        PublicationError::ActiveVersionChanged { .. }
    ));
    assert_eq!(active_v3.version().id().as_str(), "index-v3");
}
