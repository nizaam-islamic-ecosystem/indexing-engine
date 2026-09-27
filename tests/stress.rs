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
    IndexVersionState, KeyDefinition, KeyMaterial, ObjectReference, SchemaVersion, SourceVersion,
    TargetReferenceType, Uniqueness, VersionLifecycle,
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

fn ready_state(candidate: &nizaam_indexing::build::BuildCandidate) -> IndexVersionState {
    let mut state = IndexVersionState::new(candidate.version().clone());
    state
        .transition_to(VersionLifecycle::Validating)
        .expect("candidate state should enter validation");
    state
        .mark_ready()
        .expect("candidate state should become ready");
    state
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
            let mut candidate = updater
                .create_candidate(
                    &base,
                    version_id(&format!("index-v{seed}-candidate")),
                    None,
                    None,
                )
                .expect("candidate creation should succeed");
            let mut journal = UpdateJournal::new();

            for update_number in 0..24usize {
                let (updated, _) = updater
                    .apply_and_record(
                        &candidate,
                        IndexMutation::Insert(entry(
                            format!("term-{update_number}"),
                            format!("doc:{seed}:{update_number}"),
                        )),
                        &mut journal,
                    )
                    .expect("logical update should succeed");
                candidate = updated;
            }

            (
                candidate.len(),
                journal.current_sequence(),
                journal.len(),
                candidate
                    .entries()
                    .iter()
                    .any(|value| value.key() == &KeyMaterial::text("term-23")),
            )
        }));
    }

    for handle in handles {
        let (candidate_len, sequence, journal_len, contains_last_update) =
            handle.join().expect("worker should complete");
        assert_eq!(candidate_len, 25);
        assert_eq!(sequence, UpdateSequence::new(24));
        assert_eq!(journal_len, 24);
        assert!(contains_last_update);
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

        let candidate_state = ready_state(&candidate);
        let prepared = publisher
            .prepare(
                candidate,
                candidate_state,
                active.definition(),
                Some(active.version().id().clone()),
                Some(active.version().id().clone()),
            )
            .expect("candidate should prepare against the current active version");

        let publication = publisher
            .publish(prepared, Some(active), None)
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
                    vec![entry("base", "doc:base"), entry("before", "doc:before")],
                ),
                Some(version_id("index-v1")),
                Some(version_id("index-v1")),
            )
            .with_snapshot_sequence(UpdateSequence::new(1)),
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
    assert_eq!(result.candidate().len(), 34);
}

// -----------------------------------------------------------------------------
// Phase 4 query-pressure coverage
// -----------------------------------------------------------------------------

use std::fmt::{Display, Formatter};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use nizaam_indexing::consistency::{ConsistencyMode, FreshnessPolicy, SynchronizationSnapshot};
use nizaam_indexing::provider::{
    ProviderAvailability, ProviderCapabilities, ProviderCapability, RankingCandidate,
};
use nizaam_indexing::query::{
    ExactRetrievalPlan, FilteredRetrievalPlan, HybridRetrievalPlan, IndexCandidate,
    NeighborhoodRetrievalPlan, PlannedHybridComponent, ProviderRetriever, QueryKind, QueryRequest,
    ResultMode, SimilarityRetrievalPlan, StructuredRetrievalPlan, TextRetrievalPlan, execute,
    plan_query,
};

fn phase4_stress_index_candidate(
    seed: u8,
    version: &str,
    source_sequence: u64,
    indexed_sequence: u64,
    lifecycle: VersionLifecycle,
) -> IndexCandidate {
    let definition = definition();
    let version = nizaam_indexing::index::IndexVersion::with_metadata(
        version_id(version),
        Some(source_version("source-v1")),
        Some(schema_version("schema-v1")),
        None,
    )
    .expect("phase4 stress version should be valid");
    let mut state = IndexVersionState::new(version);
    if lifecycle != VersionLifecycle::Building {
        state
            .transition_to(VersionLifecycle::Validating)
            .expect("version should enter validation");
        state.mark_ready().expect("version should become ready");
    }
    if lifecycle == VersionLifecycle::Published {
        state
            .mark_published()
            .expect("version should become published");
    }

    let synchronization = SynchronizationSnapshot::from_sequences(
        UpdateSequence::new(source_sequence),
        UpdateSequence::new(indexed_sequence),
    )
    .expect("phase4 synchronization should be valid");

    IndexCandidate::new(index_id(seed), definition, state, synchronization)
}

#[derive(Clone, Debug)]
struct StressQueryProvider {
    capabilities: ProviderCapabilities,
    calls: Arc<AtomicUsize>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct StressQueryProviderError;

impl Display for StressQueryProviderError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("stress query provider failure")
    }
}

impl std::error::Error for StressQueryProviderError {}

impl StressQueryProvider {
    fn new(capabilities: ProviderCapabilities) -> Self {
        Self {
            capabilities,
            calls: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn exact_result(&self) -> Result<Vec<RankingCandidate>, StressQueryProviderError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        std::thread::sleep(Duration::from_millis(2));
        Ok(vec![RankingCandidate::new(
            ObjectReference::new("documents", "stress:object")
                .expect("stress reference should be valid"),
        )])
    }
}

impl ProviderRetriever for StressQueryProvider {
    type Error = StressQueryProviderError;

    fn capabilities(&self) -> &ProviderCapabilities {
        &self.capabilities
    }

    fn availability(&self) -> ProviderAvailability {
        ProviderAvailability::Available
    }

    fn retrieve_exact(
        &self,
        _plan: &ExactRetrievalPlan,
        _context: &nizaam_core::operation::OperationContext,
    ) -> Result<Vec<RankingCandidate>, Self::Error> {
        self.exact_result()
    }

    fn retrieve_text(
        &self,
        _plan: &TextRetrievalPlan,
        _context: &nizaam_core::operation::OperationContext,
    ) -> Result<Vec<RankingCandidate>, Self::Error> {
        self.exact_result()
    }

    fn retrieve_structured(
        &self,
        _plan: &StructuredRetrievalPlan,
        _context: &nizaam_core::operation::OperationContext,
    ) -> Result<Vec<RankingCandidate>, Self::Error> {
        self.exact_result()
    }

    fn retrieve_neighborhood(
        &self,
        _plan: &NeighborhoodRetrievalPlan,
        _context: &nizaam_core::operation::OperationContext,
    ) -> Result<Vec<RankingCandidate>, Self::Error> {
        self.exact_result()
    }

    fn retrieve_similarity(
        &self,
        _plan: &SimilarityRetrievalPlan,
        _context: &nizaam_core::operation::OperationContext,
    ) -> Result<Vec<RankingCandidate>, Self::Error> {
        self.exact_result()
    }

    fn retrieve_filtered(
        &self,
        _plan: &FilteredRetrievalPlan,
        _context: &nizaam_core::operation::OperationContext,
    ) -> Result<Vec<RankingCandidate>, Self::Error> {
        self.exact_result()
    }

    fn retrieve_hybrid(
        &self,
        _plan: &HybridRetrievalPlan,
        _context: &nizaam_core::operation::OperationContext,
    ) -> Result<Vec<RankingCandidate>, Self::Error> {
        self.exact_result()
    }

    fn retrieve_hybrid_component(
        &self,
        _component: &PlannedHybridComponent,
        _context: &nizaam_core::operation::OperationContext,
    ) -> Result<Vec<RankingCandidate>, Self::Error> {
        self.exact_result()
    }
}

fn stress_operation_context(seed: usize) -> nizaam_core::operation::OperationContext {
    nizaam_core::operation::OperationContext::new(nizaam_core::operation::Operation::new(
        nizaam_core::identity::OperationId::new(format!("phase4-stress-operation-{seed}"))
            .expect("operation ID should be valid"),
        nizaam_core::identity::CorrelationId::new(format!("phase4-stress-correlation-{seed}"))
            .expect("correlation ID should be valid"),
    ))
}

#[test]
fn phase4_concurrent_query_planning_remains_stateless_and_deterministic() {
    let candidate =
        phase4_stress_index_candidate(0xa1, "published-v1", 100, 100, VersionLifecycle::Published);
    let index_id = *candidate.index_id();
    let request = Arc::new(
        QueryRequest::exact(index_id, KeyMaterial::text("parallel"))
            .expect("parallel request should be valid"),
    );
    let candidate = Arc::new(candidate);
    let handles: Vec<_> = (0..16usize)
        .map(|_| {
            let request = Arc::clone(&request);
            let candidate = Arc::clone(&candidate);
            std::thread::spawn(move || {
                let plan = plan_query(
                    &request,
                    vec![(*candidate).clone()],
                    &ProviderCapabilities::with(ProviderCapability::ExactLookup),
                    ProviderAvailability::Available,
                )
                .expect("concurrent logical planning should succeed");
                assert_eq!(
                    plan.target().expect("target should exist").index_id(),
                    &index_id
                );
                assert_eq!(
                    plan.target()
                        .expect("target should exist")
                        .version()
                        .id()
                        .as_str(),
                    "published-v1"
                );
            })
        })
        .collect();

    for handle in handles {
        handle.join().expect("query-planning worker should finish");
    }
}

#[test]
fn phase4_current_query_remains_on_the_published_version_during_rebuild_pressure() {
    let published =
        phase4_stress_index_candidate(0xa2, "published-v1", 120, 120, VersionLifecycle::Published);
    let rebuilding =
        phase4_stress_index_candidate(0xa2, "candidate-v2", 120, 120, VersionLifecycle::Ready);
    let request = QueryRequest::with_spec(
        *published.index_id(),
        None,
        None,
        None,
        QueryKind::Exact {
            key: KeyMaterial::text("during-rebuild"),
        },
        None,
        ConsistencyMode::Current,
        ResultMode::ReferencesOnly,
        None,
    )
    .expect("query should be valid");

    let plan = plan_query(
        &request,
        vec![published, rebuilding],
        &ProviderCapabilities::with(ProviderCapability::ExactLookup),
        ProviderAvailability::Available,
    )
    .expect("published current state should remain queryable");

    assert_eq!(
        plan.target()
            .expect("target should exist")
            .version()
            .id()
            .as_str(),
        "published-v1"
    );
    assert_eq!(
        plan.target().expect("target should exist").lifecycle(),
        VersionLifecycle::Published
    );
}

#[test]
fn phase4_stale_allowed_pressure_chooses_the_lowest_acceptable_update_sequence_lag() {
    let request_candidate =
        phase4_stress_index_candidate(0xa3, "published-v0", 200, 200, VersionLifecycle::Published);
    let request = QueryRequest::with_spec(
        *request_candidate.index_id(),
        None,
        None,
        None,
        QueryKind::Text {
            query: KeyMaterial::text("bounded-stale"),
            parameters: None,
        },
        None,
        ConsistencyMode::StaleAllowed(FreshnessPolicy::new(5)),
        ResultMode::ReferencesOnly,
        None,
    )
    .expect("stale request should be valid");

    let candidates: Vec<_> = (0..32u8)
        .map(|seed| {
            phase4_stress_index_candidate(
                0xa3,
                &format!("published-v{seed}"),
                200,
                if seed == 0 {
                    200
                } else {
                    200 - (u64::from(((seed - 1) % 5) + 1))
                },
                VersionLifecycle::Published,
            )
        })
        .collect();

    let plan = plan_query(
        &request,
        candidates,
        &ProviderCapabilities::with(ProviderCapability::TextLookup),
        ProviderAvailability::Available,
    )
    .expect("bounded stale candidates should contain an acceptable version");

    assert_eq!(
        plan.target()
            .expect("target should exist")
            .version()
            .id()
            .as_str(),
        "published-v0"
    );
    assert_eq!(
        plan.target()
            .expect("target should exist")
            .consistency()
            .update_sequence_lag(),
        Some(0)
    );
}

#[test]
fn phase4_slow_provider_retrieval_remains_bounded_and_completes_under_concurrent_pressure() {
    let candidate =
        phase4_stress_index_candidate(0xa4, "published-v1", 300, 300, VersionLifecycle::Published);
    let index_id = *candidate.index_id();
    let provider = Arc::new(StressQueryProvider::new(ProviderCapabilities::with(
        ProviderCapability::ExactLookup,
    )));
    let mut handles = Vec::new();

    for worker in 0..8usize {
        let provider = Arc::clone(&provider);
        let candidate = candidate.clone();
        handles.push(std::thread::spawn(move || {
            let request =
                QueryRequest::exact(index_id, KeyMaterial::text(format!("worker-{worker}")))
                    .expect("stress query should be valid");
            let plan = plan_query(
                &request,
                vec![candidate],
                provider.capabilities(),
                provider.availability(),
            )
            .expect("stress query should plan");
            let result = execute(&plan, provider.as_ref(), &stress_operation_context(worker))
                .expect("slow provider should eventually return a result");
            assert_eq!(result.hits().len(), 1);
        }));
    }

    for handle in handles {
        handle.join().expect("retrieval worker should finish");
    }

    assert_eq!(provider.calls.load(Ordering::SeqCst), 8);
}
