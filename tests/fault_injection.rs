//! Deterministic fault-injection coverage for Phase 3, Phase 4, Phase 5, and Phase 6.
//!
//! Faults are injected through real public logical APIs using invalid or
//! conflicting inputs. No provider/storage implementation is fabricated here,
//! because those boundaries are deliberately deferred.
//!
//! Phase 6 faults continue to use real Core boundaries: routing selection is
//! never treated as execution, and lifecycle admission remains authoritative
//! after a destination has been selected.

use nizaam_indexing::build::{
    BatchChunkError, BatchError, BatchExecutor, BatchOptions, BuildInput, BuildSnapshot,
    CandidateUpdater, IndexBuilder, IndexMutation, IndexPublisher, IndexRebuilder,
    PublicationError, RebuildError, RebuildInput, UpdateError, UpdateJournal,
};
use nizaam_indexing::identity::{IndexDefinitionId, IndexDefinitionIdentity, IndexNamespace};
use nizaam_indexing::index::{
    ConsistencyRequirement, IndexDefinition, IndexEntry, IndexFamily, IndexVersionId,
    IndexVersionState, KeyDefinition, KeyMaterial, ObjectReference, SchemaVersion, SourceVersion,
    TargetReferenceType, Uniqueness, VersionLifecycle,
};
use nizaam_indexing::{
    CapacityAccounting, CapacityAdmissionError, CapacityOperation, CapacityRequest,
    ConfigurationValidationError, FailureClass, IndexLifecycle, IndexLifecycleState,
    IndexingConfiguration, IntegrityValidationError, IntegrityValidator, RecoveryAction,
    RecoveryRequest, action_for, classify,
};

fn definition_identity(seed: u8) -> IndexDefinitionIdentity {
    IndexDefinitionIdentity::new(
        IndexDefinitionId::new(format!("phase3.faults.documents.{seed}"))
            .expect("definition ID should be valid"),
        IndexNamespace::new(format!("phase3.faults.{seed}")).expect("namespace should be valid"),
        IndexFamily::Inverted,
    )
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

fn definition(seed: u8) -> IndexDefinition {
    IndexDefinition::new(
        definition_identity(seed),
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

fn candidate(
    seed: u8,
    version: &str,
    entries: Vec<IndexEntry>,
) -> nizaam_indexing::build::BuildCandidate {
    IndexBuilder::new()
        .build(BuildInput::new(
            definition_identity(seed),
            definition(seed),
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
            definition_identity(0x11),
            definition(0x11),
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
                definition_identity(0x55),
                definition(0x55),
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

    let candidate_state = ready_state(&candidate);
    let error = publisher
        .prepare(
            candidate,
            candidate_state,
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
            v2.clone(),
            ready_state(&v2),
            v1.definition(),
            Some(version_id("index-v1")),
            Some(version_id("index-v1")),
        )
        .expect("v2 should prepare");

    let published_v3 = publisher
        .publish(
            publisher
                .prepare(
                    v3.clone(),
                    ready_state(&v3),
                    v1.definition(),
                    Some(version_id("index-v1")),
                    Some(version_id("index-v1")),
                )
                .expect("v3 should prepare"),
            Some(v1),
            None,
        )
        .expect("v3 should publish");

    let active_v3 = published_v3.active().clone();
    let error = publisher
        .publish(prepared_v2, Some(active_v3.clone()), None)
        .expect_err("older prepared candidate must be rejected");

    assert!(matches!(
        error,
        PublicationError::ActiveVersionChanged { .. }
    ));
    assert_eq!(active_v3.version().id().as_str(), "index-v3");
}

// -----------------------------------------------------------------------------
// Phase 4 fault-injection coverage
// -----------------------------------------------------------------------------

use std::fmt::{Display, Formatter};
use std::sync::{Arc, Mutex};

use nizaam_indexing::consistency::{ConsistencyMode, FreshnessPolicy, SynchronizationSnapshot};
use nizaam_indexing::provider::{
    ProviderAvailability, ProviderCapabilities, ProviderCapability, RankingCandidate,
};
use nizaam_indexing::query::{
    AtomicQuery, ExactRetrievalPlan, FilteredRetrievalPlan, HybridQueryComponent,
    HybridRetrievalPlan, IndexCandidate, NeighborhoodRetrievalPlan, PlannedHybridComponent,
    ProviderRetriever, QueryKind, QueryRequest, ResultMode, RetrievalError,
    SimilarityRetrievalPlan, StructuredRetrievalPlan, TextRetrievalPlan, execute, plan_query,
};

fn phase4_fault_candidate(
    definition_name: &str,
    version: &str,
    source_sequence: u64,
    indexed_sequence: u64,
    lifecycle: VersionLifecycle,
) -> IndexCandidate {
    let definition = IndexDefinition::new(
        IndexDefinitionIdentity::new(
            IndexDefinitionId::new(definition_name).expect("definition ID should be valid"),
            IndexNamespace::new("phase4.faults").expect("namespace should be valid"),
            IndexFamily::Identity,
        ),
        KeyDefinition::new(["value"]).expect("key definition should be valid"),
        TargetReferenceType::new("documents.document")
            .expect("target reference type should be valid"),
        Uniqueness::NonUnique,
        ConsistencyRequirement::new("logical-v1").expect("consistency requirement should be valid"),
        Some(source_version("source-v1")),
        Some(schema_version("schema-v1")),
    )
    .expect("fault-injection definition should be valid");

    let version = nizaam_indexing::index::IndexVersion::with_metadata(
        version_id(version),
        Some(source_version("source-v1")),
        Some(schema_version("schema-v1")),
        None,
    )
    .expect("version should be valid");
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
        nizaam_indexing::build::UpdateSequence::new(source_sequence),
        nizaam_indexing::build::UpdateSequence::new(indexed_sequence),
    )
    .expect("synchronization should be valid");

    IndexCandidate::new(
        IndexDefinitionIdentity::new(
            definition.definition_id().clone(),
            definition.namespace().clone(),
            definition.family(),
        ),
        definition,
        state,
        synchronization,
    )
}

fn fault_operation_context() -> nizaam_core::operation::OperationContext {
    nizaam_core::operation::OperationContext::new(nizaam_core::operation::Operation::new(
        nizaam_core::identity::OperationId::new("phase4-fault-operation")
            .expect("operation ID should be valid"),
        nizaam_core::identity::CorrelationId::new("phase4-fault-correlation")
            .expect("correlation ID should be valid"),
    ))
}

#[derive(Clone, Debug)]
struct FaultProvider {
    capabilities: ProviderCapabilities,
    availability: ProviderAvailability,
    fail: bool,
    calls: Arc<Mutex<usize>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct FaultProviderError(&'static str);

impl Display for FaultProviderError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.0)
    }
}

impl std::error::Error for FaultProviderError {}

impl FaultProvider {
    fn new(
        capabilities: ProviderCapabilities,
        availability: ProviderAvailability,
        fail: bool,
    ) -> Self {
        Self {
            capabilities,
            availability,
            fail,
            calls: Arc::new(Mutex::new(0)),
        }
    }

    fn result(&self) -> Result<Vec<RankingCandidate>, FaultProviderError> {
        *self
            .calls
            .lock()
            .expect("call counter mutex should be valid") += 1;
        if self.fail {
            return Err(FaultProviderError("injected provider failure"));
        }

        Ok(vec![RankingCandidate::new(
            ObjectReference::new("documents", "doc:1").expect("test reference should be valid"),
        )])
    }
}

impl ProviderRetriever for FaultProvider {
    type Error = FaultProviderError;

    fn capabilities(&self) -> &ProviderCapabilities {
        &self.capabilities
    }

    fn availability(&self) -> ProviderAvailability {
        self.availability
    }

    fn retrieve_exact(
        &self,
        _plan: &ExactRetrievalPlan,
        _context: &nizaam_core::operation::OperationContext,
    ) -> Result<Vec<RankingCandidate>, Self::Error> {
        self.result()
    }

    fn retrieve_text(
        &self,
        _plan: &TextRetrievalPlan,
        _context: &nizaam_core::operation::OperationContext,
    ) -> Result<Vec<RankingCandidate>, Self::Error> {
        self.result()
    }

    fn retrieve_structured(
        &self,
        _plan: &StructuredRetrievalPlan,
        _context: &nizaam_core::operation::OperationContext,
    ) -> Result<Vec<RankingCandidate>, Self::Error> {
        self.result()
    }

    fn retrieve_neighborhood(
        &self,
        _plan: &NeighborhoodRetrievalPlan,
        _context: &nizaam_core::operation::OperationContext,
    ) -> Result<Vec<RankingCandidate>, Self::Error> {
        self.result()
    }

    fn retrieve_similarity(
        &self,
        _plan: &SimilarityRetrievalPlan,
        _context: &nizaam_core::operation::OperationContext,
    ) -> Result<Vec<RankingCandidate>, Self::Error> {
        self.result()
    }

    fn retrieve_filtered(
        &self,
        _plan: &FilteredRetrievalPlan,
        _context: &nizaam_core::operation::OperationContext,
    ) -> Result<Vec<RankingCandidate>, Self::Error> {
        self.result()
    }

    fn retrieve_hybrid(
        &self,
        _plan: &HybridRetrievalPlan,
        _context: &nizaam_core::operation::OperationContext,
    ) -> Result<Vec<RankingCandidate>, Self::Error> {
        self.result()
    }

    fn retrieve_hybrid_component(
        &self,
        _component: &PlannedHybridComponent,
        _context: &nizaam_core::operation::OperationContext,
    ) -> Result<Vec<RankingCandidate>, Self::Error> {
        self.result()
    }
}

#[test]
fn phase4_provider_failure_is_preserved_and_is_not_converted_into_an_empty_result() {
    let candidate = phase4_fault_candidate(
        "phase4.faults.provider",
        "published-v1",
        10,
        10,
        VersionLifecycle::Published,
    );
    let request = QueryRequest::exact(
        candidate.definition_identity().clone(),
        KeyMaterial::text("term"),
    )
    .expect("request should be valid");
    let provider = FaultProvider::new(
        ProviderCapabilities::with(ProviderCapability::ExactLookup),
        ProviderAvailability::Available,
        true,
    );
    let plan = plan_query(
        &request,
        vec![candidate],
        provider.capabilities(),
        provider.availability(),
    )
    .expect("planning should succeed before the injected execution fault");

    let error = execute(&plan, &provider, &fault_operation_context())
        .expect_err("provider failure must propagate as an execution error");

    assert!(matches!(
        error,
        RetrievalError::Provider(FaultProviderError("injected provider failure"))
    ));
}

#[test]
fn phase4_execution_rechecks_provider_availability_and_rejects_unavailable_provider() {
    let candidate = phase4_fault_candidate(
        "phase4.faults.availability",
        "published-v1",
        11,
        11,
        VersionLifecycle::Published,
    );
    let request = QueryRequest::exact(
        candidate.definition_identity().clone(),
        KeyMaterial::text("term"),
    )
    .expect("request should be valid");
    let planning_capabilities = ProviderCapabilities::with(ProviderCapability::ExactLookup);
    let plan = plan_query(
        &request,
        vec![candidate],
        &planning_capabilities,
        ProviderAvailability::Available,
    )
    .expect("planning should succeed while provider is available");

    let provider = FaultProvider::new(
        planning_capabilities,
        ProviderAvailability::TemporarilyUnavailable,
        false,
    );
    let error = execute(&plan, &provider, &fault_operation_context())
        .expect_err("execution-time provider unavailability must be detected");

    assert!(matches!(
        error,
        RetrievalError::ProviderUnavailable {
            availability: ProviderAvailability::TemporarilyUnavailable
        }
    ));
}

#[test]
fn phase4_execution_rechecks_capability_and_rejects_missing_provider_support() {
    let candidate = phase4_fault_candidate(
        "phase4.faults.capability",
        "published-v1",
        12,
        12,
        VersionLifecycle::Published,
    );
    let request = QueryRequest::exact(
        candidate.definition_identity().clone(),
        KeyMaterial::text("term"),
    )
    .expect("request should be valid");
    let planning_capabilities = ProviderCapabilities::with(ProviderCapability::ExactLookup);
    let plan = plan_query(
        &request,
        vec![candidate],
        &planning_capabilities,
        ProviderAvailability::Available,
    )
    .expect("planning should succeed with advertised capability");

    let provider = FaultProvider::new(
        ProviderCapabilities::new(),
        ProviderAvailability::Available,
        false,
    );
    let error = execute(&plan, &provider, &fault_operation_context())
        .expect_err("execution must reject a capability that disappeared");

    assert!(matches!(
        error,
        RetrievalError::MissingProviderCapability(_)
    ));
}

#[test]
fn phase4_consistency_failure_stops_planning_before_provider_execution() {
    let candidate = phase4_fault_candidate(
        "phase4.faults.consistency",
        "published-v1",
        20,
        17,
        VersionLifecycle::Published,
    );
    let request = QueryRequest::with_spec(
        candidate.definition_identity().clone(),
        None,
        None,
        None,
        QueryKind::Exact {
            key: KeyMaterial::text("term"),
        },
        None,
        ConsistencyMode::Current,
        ResultMode::ReferencesOnly,
        None,
    )
    .expect("request should be valid");

    let error = plan_query(
        &request,
        vec![candidate],
        &ProviderCapabilities::with(ProviderCapability::ExactLookup),
        ProviderAvailability::Available,
    )
    .expect_err("positive freshness lag must fail Current planning");

    assert!(matches!(
        error,
        nizaam_indexing::query::QueryPlanningError::ConsistencyUnsatisfied { .. }
    ));
}

#[test]
fn phase4_pinned_version_mismatch_is_a_failure_and_does_not_substitute_another_version() {
    let candidate = phase4_fault_candidate(
        "phase4.faults.pinned",
        "published-v2",
        30,
        30,
        VersionLifecycle::Published,
    );
    let request = QueryRequest::with_spec(
        candidate.definition_identity().clone(),
        None,
        None,
        None,
        QueryKind::Exact {
            key: KeyMaterial::text("term"),
        },
        None,
        ConsistencyMode::VersionPinned(
            IndexVersionId::new("published-v1").expect("version ID should be valid"),
        ),
        ResultMode::ReferencesOnly,
        None,
    )
    .expect("request should be valid");

    let error = plan_query(
        &request,
        vec![candidate],
        &ProviderCapabilities::with(ProviderCapability::ExactLookup),
        ProviderAvailability::Available,
    )
    .expect_err("a different version must not satisfy a pinned request");

    assert!(matches!(
        error,
        nizaam_indexing::query::QueryPlanningError::ConsistencyUnsatisfied { .. }
    ));
}

#[test]
fn phase4_unpublished_candidate_cannot_be_reached_by_provider_execution() {
    let candidate = phase4_fault_candidate(
        "phase4.faults.unpublished",
        "candidate-v1",
        40,
        40,
        VersionLifecycle::Ready,
    );
    let request = QueryRequest::exact(
        candidate.definition_identity().clone(),
        KeyMaterial::text("term"),
    )
    .expect("request should be valid");

    let result = plan_query(
        &request,
        vec![candidate],
        &ProviderCapabilities::with(ProviderCapability::ExactLookup),
        ProviderAvailability::Available,
    );

    assert!(matches!(
        result,
        Err(nizaam_indexing::query::QueryPlanningError::ConsistencyUnsatisfied { .. })
    ));
}

#[test]
fn phase4_stale_allowed_rejects_lag_beyond_the_declared_freshness_bound() {
    let candidate = phase4_fault_candidate(
        "phase4.faults.stale",
        "published-v1",
        50,
        46,
        VersionLifecycle::Published,
    );
    let request = QueryRequest::with_spec(
        candidate.definition_identity().clone(),
        None,
        None,
        None,
        QueryKind::Text {
            query: KeyMaterial::text("term"),
            parameters: None,
        },
        None,
        ConsistencyMode::StaleAllowed(FreshnessPolicy::new(2)),
        ResultMode::ReferencesOnly,
        None,
    )
    .expect("request should be valid");

    let error = plan_query(
        &request,
        vec![candidate],
        &ProviderCapabilities::with(ProviderCapability::TextLookup),
        ProviderAvailability::Available,
    )
    .expect_err("lag beyond the declared policy must fail planning");

    assert!(matches!(
        error,
        nizaam_indexing::query::QueryPlanningError::ConsistencyUnsatisfied { .. }
    ));
}

#[test]
fn phase4_heterogeneous_hybrid_results_are_rejected_instead_of_returning_misleading_provenance() {
    let first = phase4_fault_candidate(
        "phase4.faults.hybrid.a",
        "published-a",
        60,
        60,
        VersionLifecycle::Published,
    );
    let second = phase4_fault_candidate(
        "phase4.faults.hybrid.b",
        "published-b",
        60,
        60,
        VersionLifecycle::Published,
    );
    let request = QueryRequest::hybrid(
        first.definition_identity().clone(),
        vec![
            HybridQueryComponent::with_options(
                Some(first.definition_identity().clone()),
                AtomicQuery::Exact {
                    key: KeyMaterial::text("a"),
                },
                None,
            )
            .expect("first component should be valid"),
            HybridQueryComponent::with_options(
                Some(second.definition_identity().clone()),
                AtomicQuery::Exact {
                    key: KeyMaterial::text("b"),
                },
                None,
            )
            .expect("second component should be valid"),
        ],
    )
    .expect("hybrid request should be valid");
    let provider = FaultProvider::new(
        ProviderCapabilities::with(ProviderCapability::HybridRetrieval),
        ProviderAvailability::Available,
        false,
    );
    let plan = plan_query(
        &request,
        vec![first, second],
        provider.capabilities(),
        provider.availability(),
    )
    .expect("heterogeneous hybrid plan can be formed before result assembly");

    let error = execute(&plan, &provider, &fault_operation_context())
        .expect_err("heterogeneous hybrid provenance must not be collapsed");

    assert!(matches!(
        error,
        RetrievalError::HybridResultUnsupported(
            nizaam_indexing::query::RetrievalPlanValidationError::HeterogeneousHybridResultTargets
        )
    ));
}

// -----------------------------------------------------------------------------
// Phase 5 deterministic fault-injection coverage
// -----------------------------------------------------------------------------

#[test]
fn phase5_integrity_fault_rejects_source_version_incompatibility() {
    let definition = definition(0);
    let incompatible = nizaam_indexing::index::IndexVersion::with_metadata(
        version_id("index-v9"),
        Some(source_version("source-v9")),
        Some(schema_version("schema-v1")),
        None,
    )
    .expect("incompatible version should still be structurally valid");

    let error = IntegrityValidator::new()
        .validate_version_compatibility(&incompatible, &definition)
        .expect_err("incompatible source version must be rejected");

    assert!(matches!(
        error,
        IntegrityValidationError::VersionCompatibility(_)
    ));
}

#[test]
fn phase5_capacity_fault_rejects_oversized_batch_without_consuming_capacity() {
    let configuration =
        IndexingConfiguration::new(1, 1, 1, 1, 1, 2, 4, 2).expect("configuration should be valid");
    let accounting = CapacityAccounting::from_configuration(configuration);

    let error = accounting
        .try_acquire(CapacityRequest::with_batch_size(
            CapacityOperation::Build,
            1,
            3,
        ))
        .expect_err("batch size above the configured bound must be rejected");

    assert!(matches!(
        error,
        CapacityAdmissionError::BatchSizeExceeded {
            requested: 3,
            limit: 2,
        }
    ));
    assert_eq!(accounting.usage().active_builds(), 0);
    assert_eq!(accounting.usage().consumed_capacity_units(), 0);
}

#[test]
fn phase5_lifecycle_fault_cannot_bypass_required_index_readiness() {
    let id = definition_identity(0xb1);
    let mut lifecycle = IndexLifecycle::new(id.clone());

    let error = lifecycle
        .transition_to(IndexLifecycleState::Active)
        .expect_err("Creating -> Active must be rejected");

    assert_eq!(error.definition_identity(), &id);
    assert_eq!(error.from(), IndexLifecycleState::Creating);
    assert_eq!(error.to(), IndexLifecycleState::Active);
    assert_eq!(lifecycle.state(), IndexLifecycleState::Creating);
}

#[test]
fn phase5_configuration_fault_is_rejected_before_capacity_limits_are_created() {
    let error = IndexingConfiguration::new(0, 1, 1, 1, 1, 2, 4, 2)
        .expect_err("zero query concurrency must be rejected");

    assert_eq!(
        error,
        ConfigurationValidationError::ZeroValue {
            field: nizaam_indexing::ConfigurationField::MaxConcurrentQueries,
        }
    );
}

#[test]
fn phase5_recovery_fault_preserves_active_lineage_and_selects_deterministic_action() {
    let active = version_id("active-v7");
    let request = RecoveryRequest::new(classify(FailureClass::CorruptIndex))
        .with_active_version(active.clone());

    assert_eq!(request.action(), RecoveryAction::Rebuild);
    assert_eq!(
        action_for(FailureClass::CorruptIndex),
        RecoveryAction::Rebuild
    );
    assert_eq!(request.active_version(), Some(&active));
}

// -----------------------------------------------------------------------------
// Phase 6 deterministic fault-injection coverage
// -----------------------------------------------------------------------------

#[test]
fn phase6_selected_destination_cannot_bypass_core_runtime_admission() {
    use nizaam_core::control_plane::{
        PolicyInput, RoutingCandidate, RoutingConstraints, RoutingPolicy,
    };
    use nizaam_indexing::engine::runtime::IndexingEngine;

    let engine = IndexingEngine::new(
        nizaam_core::identity::EngineId::new("nizaam.indexing.phase6.fault")
            .expect("engine id should be valid"),
        nizaam_core::identity::EngineInstanceId::new("nizaam.indexing.phase6.fault.instance")
            .expect("engine instance id should be valid"),
    )
    .expect("default IndexingEngine construction should resolve a platform home directory");

    engine.start().expect("engine should start");
    engine
        .begin_registration()
        .expect("engine should enter registration");

    let registry = nizaam_core::control_plane::registry::EngineRegistry::new();
    engine
        .register_engine(&registry)
        .expect("engine registration should succeed");

    engine
        .register_phase0_capability()
        .expect("phase0 capability should register");
    engine.mark_ready().expect("engine should become ready");
    engine.serve().expect("engine should become serving");

    // Core routing policy consumes destinations that have already passed the
    // Control Plane eligibility boundary. The current Indexing registration
    // boundary does not publish capability/contract advertisements into that
    // eligibility view, so this test must not fabricate such metadata.
    //
    // The fault under test is specifically the boundary after selection:
    // a routing selection cannot make a later Core runtime admission succeed.
    let candidate = RoutingCandidate::new(engine.engine_instance_id().clone());

    let selection = RoutingPolicy::deterministic()
        .evaluate(&PolicyInput::new(
            std::slice::from_ref(&candidate),
            &RoutingConstraints::new(),
        ))
        .expect("already-eligible destination must be selectable");

    assert_eq!(
        selection.into_instance_id(),
        engine.engine_instance_id().clone()
    );

    // Inject the lifecycle fault after successful selection. Routing has not
    // changed the runtime state and cannot make a draining engine serve new
    // requests.
    engine
        .drain()
        .expect("engine should enter draining state after selection");

    assert_eq!(
        engine.runtime().admit_request(),
        Err(nizaam_core::runtime::RequestAdmissionError::NotServing(
            nizaam_core::runtime::LifecycleState::Draining,
        ))
    );

    engine
        .shutdown()
        .expect("draining engine should shut down cleanly");
}
