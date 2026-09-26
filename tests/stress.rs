//! Level 3 lifecycle-pressure tests for the Phase 3 logical build system.
//!
//! These are correctness stress tests, not benchmarks. They exercise many
//! independent candidates, repeated bounded chunks, replay pressure, and
//! concurrent stateless construction without freezing a synchronization
//! primitive or introducing provider/storage assumptions.

use std::thread;

use nizaam_indexing::build::{
    BatchExecutor, BatchOptions, BuildInput, BuildSnapshot, CandidateUpdater, IndexBuilder,
    IndexMutation, IndexRebuilder, RebuildInput, UpdateJournal, UpdateSequence,
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
            IndexDefinitionId::new("phase3.stress.documents")
                .expect("definition ID should be valid"),
            IndexNamespace::new("phase3.stress").expect("namespace should be valid"),
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

fn entry(key: impl Into<String>, reference: impl Into<String>) -> IndexEntry {
    IndexEntry::new(
        KeyMaterial::text(key),
        ObjectReference::new("documents", reference).expect("reference should be valid"),
    )
    .expect("entry should be valid")
}

fn build_candidate(
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
fn many_candidates_remain_independent_under_concurrent_stateless_construction() {
    let workers = 8usize;
    let candidates_per_worker = 8usize;
    let mut handles = Vec::with_capacity(workers);

    for worker in 0..workers {
        handles.push(thread::spawn(move || {
            let builder = IndexBuilder::new();
            let mut results = Vec::with_capacity(candidates_per_worker);

            for candidate_number in 0..candidates_per_worker {
                let seed = (worker * candidates_per_worker + candidate_number + 1) as u8;
                let version = format!("candidate-v{seed}");
                let candidate = builder
                    .build(BuildInput::new(
                        index_id(seed),
                        definition(),
                        version_id(&version),
                        BuildSnapshot::with_versions(
                            Some(source_version("source-v1")),
                            Some(schema_version("schema-v1")),
                            vec![entry(format!("term-{seed}"), format!("doc:{seed}"))],
                        ),
                    ))
                    .expect("concurrent candidate construction should succeed");

                results.push((*candidate.index_id(), candidate.version().id().clone()));
            }

            results
        }));
    }

    let mut all = Vec::new();
    for handle in handles {
        all.extend(handle.join().expect("worker should complete without panic"));
    }

    assert_eq!(all.len(), workers * candidates_per_worker);

    let distinct_index_ids: std::collections::BTreeSet<_> =
        all.into_iter().map(|(index, _)| index).collect();
    assert_eq!(distinct_index_ids.len(), workers * candidates_per_worker);
}

#[test]
fn concurrent_candidate_updates_do_not_cross_contaminate_journals() {
    let candidate_count = 16usize;
    let mut handles = Vec::with_capacity(candidate_count);

    for number in 0..candidate_count {
        handles.push(thread::spawn(move || {
            let seed = (number + 1) as u8;
            let base = build_candidate(
                seed,
                &format!("index-v{seed}"),
                vec![entry("base", format!("doc:{seed}"))],
            );
            let updater = CandidateUpdater::new();
            let candidate = updater
                .create_candidate(
                    &base,
                    version_id(&format!("index-v{seed}-candidate")),
                    None,
                    None,
                )
                .expect("candidate creation should succeed");
            let mut journal = UpdateJournal::new();

            for update_number in 0..24usize {
                updater
                    .apply_and_record(
                        &candidate,
                        IndexMutation::Insert(entry(
                            format!("term-{update_number}"),
                            format!("doc:{seed}:{update_number}"),
                        )),
                        &mut journal,
                    )
                    .expect("logical update should succeed");
            }

            (candidate.len(), journal.current_sequence(), journal.len())
        }));
    }

    for handle in handles {
        let (base_len, sequence, journal_len) = handle.join().expect("worker should complete");
        assert_eq!(base_len, 1);
        assert_eq!(sequence, UpdateSequence::new(24));
        assert_eq!(journal_len, 24);
    }
}

#[test]
fn replay_pressure_catches_up_all_post_snapshot_updates() {
    let mut journal = UpdateJournal::new();
    let rebuilder = IndexRebuilder::new();

    let progress = rebuilder
        .start(
            RebuildInput::new(
                index_id(0x77),
                definition(),
                version_id("index-v2"),
                BuildSnapshot::with_versions(
                    Some(source_version("source-v1")),
                    Some(schema_version("schema-v1")),
                    vec![entry("base", "doc:base")],
                ),
                Some(version_id("index-v1")),
                Some(version_id("index-v1")),
            ),
            &journal,
        )
        .expect("rebuild should start");

    for update_number in 0..64usize {
        journal
            .append(IndexMutation::Insert(entry(
                format!("term-{update_number}"),
                format!("doc:{update_number}"),
            )))
            .expect("journal append should succeed");
    }

    assert_eq!(journal.current_sequence(), UpdateSequence::new(64));

    let progress = rebuilder
        .replay(&progress, &journal)
        .expect("replay pressure should be handled deterministically");

    assert!(rebuilder.is_caught_up(&progress, &journal));
    assert_eq!(progress.replayed_updates(), 64);
    assert_eq!(progress.candidate().len(), 65);

    let result = rebuilder
        .finish(&progress, &journal)
        .expect("caught-up progress should finish");
    assert_eq!(result.replayed_through(), UpdateSequence::new(64));
}

#[test]
fn large_logical_batch_is_processed_as_many_bounded_transactional_chunks() {
    let base = build_candidate(0x88, "index-v1", vec![entry("base", "doc:base")]);
    let mutations: Vec<_> = (0..100usize)
        .map(|number| {
            IndexMutation::Insert(entry(format!("term-{number}"), format!("doc:{number}")))
        })
        .collect();

    let result = BatchExecutor::new()
        .execute(
            &base,
            &UpdateJournal::new(),
            mutations,
            BatchOptions::new(7).expect("chunk size should be valid"),
        )
        .expect("all bounded chunks should succeed");

    assert_eq!(result.applied_mutations(), 100);
    assert_eq!(result.completed_chunks(), 15);
    assert_eq!(result.candidate().len(), 101);
    assert_eq!(result.journal().len(), 100);
    assert_eq!(
        result.journal().current_sequence(),
        UpdateSequence::new(100)
    );
}

#[test]
fn repeated_publication_advances_only_through_explicit_boundaries() {
    let publisher = nizaam_indexing::build::IndexPublisher::new();
    let mut active = build_candidate(0xaa, "index-v1", vec![entry("base", "doc:base")]);

    for version_number in 2..=8usize {
        let version = format!("index-v{version_number}");
        let candidate = build_candidate(
            0xaa,
            &version,
            vec![
                entry("base", "doc:base"),
                entry(
                    format!("term-{version_number}"),
                    format!("doc:{version_number}"),
                ),
            ],
        );

        let prepared = publisher
            .prepare(
                candidate,
                active.definition(),
                Some(active.version().id().clone()),
                Some(active.version().id().clone()),
            )
            .expect("candidate should prepare against the current active version");

        let publication = publisher
            .publish(prepared, Some(active))
            .expect("explicit publication should succeed");

        let expected_previous = format!("index-v{}", version_number - 1);
        assert_eq!(
            publication
                .previous_active()
                .expect("previous active should exist")
                .version()
                .id()
                .as_str(),
            expected_previous.as_str(),
        );
        active = publication.active().clone();
        assert_eq!(active.version().id().as_str(), version);
    }

    assert_eq!(active.version().id().as_str(), "index-v8");
}

#[test]
fn rebuild_plus_updates_requires_replay_before_publication() {
    let mut journal = UpdateJournal::new();
    let rebuilder = IndexRebuilder::new();

    journal
        .append(IndexMutation::Insert(entry("before", "doc:before")))
        .expect("pre-capture update should succeed");

    let progress = rebuilder
        .start(
            RebuildInput::new(
                index_id(0x99),
                definition(),
                version_id("index-v2"),
                BuildSnapshot::with_versions(
                    Some(source_version("source-v1")),
                    Some(schema_version("schema-v1")),
                    vec![entry("base", "doc:base")],
                ),
                Some(version_id("index-v1")),
                Some(version_id("index-v1")),
            ),
            &journal,
        )
        .expect("rebuild should start");

    for update_number in 0..32usize {
        journal
            .append(IndexMutation::Insert(entry(
                format!("term-{update_number}"),
                format!("doc:{update_number}"),
            )))
            .expect("journal update should succeed");
    }

    assert!(!rebuilder.is_caught_up(&progress, &journal));

    let progress = rebuilder
        .replay(&progress, &journal)
        .expect("replay should consume all newer updates");
    let result = rebuilder
        .finish(&progress, &journal)
        .expect("rebuild should finish at the consistency point");

    assert_eq!(result.captured_sequence(), UpdateSequence::new(1));
    assert_eq!(result.replayed_through(), UpdateSequence::new(33));
    assert_eq!(result.replayed_updates(), 32);
    assert_eq!(result.candidate().len(), 33);
}
