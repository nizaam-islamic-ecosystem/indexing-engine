//! Phase 4 query-layer integration tests.
//!
//! This suite exercises the public logical query boundary together with the
//! Indexing-local planner. It intentionally stays provider-neutral: the tests
//! verify logical query construction, candidate selection, capability
//! negotiation, and logical plan construction without depending on a physical
//! database, storage engine, or provider implementation.
//!
//! Provider execution/failure mechanics belong to the retrieval and fault
//! injection test surfaces. Consistency-specific edge cases belong primarily
//! to `tests/consistency.rs`.

use std::num::NonZeroUsize;

use nizaam_indexing::INDEX_ID_BYTE_LEN;
use nizaam_indexing::consistency::ConsistencyMode;
use nizaam_indexing::consistency::SynchronizationSnapshot;
use nizaam_indexing::identity::{
    IndexDefinitionId, IndexDefinitionIdentity, IndexId, IndexNamespace,
};
use nizaam_indexing::index::{
    ConsistencyRequirement, IndexDefinition, IndexFamily, IndexVersion, IndexVersionId,
    IndexVersionState, KeyDefinition, KeyMaterial, ObjectReference, SchemaVersion, SourceVersion,
    TargetReferenceType, Uniqueness, VersionLifecycle,
};
use nizaam_indexing::provider::{ProviderAvailability, ProviderCapabilities, ProviderCapability};
use nizaam_indexing::query::{
    AtomicQuery, CapabilityResolution, HybridQueryComponent, IndexCandidate, QueryKind,
    QueryRequest, ResultMode, RetrievalPlan, plan_query,
};

fn namespace(value: &str) -> IndexNamespace {
    IndexNamespace::new(value).expect("test namespace should be valid")
}

fn definition_id(value: &str) -> IndexDefinitionId {
    IndexDefinitionId::new(value).expect("test definition ID should be valid")
}

fn index_id(seed: u8, definition: &IndexDefinition) -> IndexId {
    IndexId::generate(
        definition.namespace(),
        definition.definition_id(),
        definition.family(),
        &KeyMaterial::Unsigned(u128::from(seed)),
    )
    .expect("test index ID should be generated")
}

fn definition(
    namespace_value: &str,
    definition_value: &str,
    family: IndexFamily,
) -> IndexDefinition {
    IndexDefinition::new(
        IndexDefinitionIdentity::new(
            definition_id(definition_value),
            namespace(namespace_value),
            family,
        ),
        KeyDefinition::new(["value"]).expect("test key definition should be valid"),
        TargetReferenceType::new("object").expect("test target reference type should be valid"),
        Uniqueness::NonUnique,
        ConsistencyRequirement::new("eventual")
            .expect("test consistency requirement should be valid"),
        Some(SourceVersion::new("source-v1").expect("source version should be valid")),
        Some(SchemaVersion::new("schema-v1").expect("schema version should be valid")),
    )
    .expect("test definition should be valid")
}

fn published_version(version_value: &str) -> IndexVersionState {
    let version = IndexVersion::with_metadata(
        IndexVersionId::new(version_value).expect("test version ID should be valid"),
        Some(SourceVersion::new("source-v1").expect("source version should be valid")),
        Some(SchemaVersion::new("schema-v1").expect("schema version should be valid")),
        None,
    )
    .expect("test index version should be valid");

    let mut state = IndexVersionState::new(version);

    state
        .transition_to(VersionLifecycle::Validating)
        .expect("version should enter validation");
    state.mark_ready().expect("version should become ready");
    state
        .mark_published()
        .expect("version should become published");

    state
}

fn candidate(
    seed: u8,
    family: IndexFamily,
    definition_value: &str,
    version_value: &str,
    source_sequence: u64,
    indexed_sequence: u64,
) -> IndexCandidate {
    let definition = definition("tests.query", definition_value, family);
    let id = index_id(seed, &definition);

    let synchronization = SynchronizationSnapshot::from_sequences(
        nizaam_indexing::build::UpdateSequence::new(source_sequence),
        nizaam_indexing::build::UpdateSequence::new(indexed_sequence),
    )
    .expect("test synchronization should be valid");

    IndexCandidate::new(
        id,
        definition,
        published_version(version_value),
        synchronization,
    )
}

fn capabilities(capabilities: &[ProviderCapability]) -> ProviderCapabilities {
    let mut result = ProviderCapabilities::new();

    for capability in capabilities {
        result.insert(*capability);
    }

    result
}

fn hybrid_component(query: AtomicQuery) -> HybridQueryComponent {
    HybridQueryComponent::new(query).expect("test hybrid component should be valid")
}

#[test]
fn logical_request_constructors_cover_all_phase4_query_kinds() {
    let exact = QueryRequest::exact(
        IndexId::from_bytes([1; INDEX_ID_BYTE_LEN]),
        KeyMaterial::text("exact"),
    )
    .expect("exact request should be valid");
    let text = QueryRequest::text(
        IndexId::from_bytes([2; INDEX_ID_BYTE_LEN]),
        KeyMaterial::text("text"),
    )
    .expect("text request should be valid");
    let structured = QueryRequest::structured(
        IndexId::from_bytes([3; INDEX_ID_BYTE_LEN]),
        KeyMaterial::text("fields"),
    )
    .expect("structured request should be valid");
    let neighborhood = QueryRequest::neighborhood(
        IndexId::from_bytes([4; INDEX_ID_BYTE_LEN]),
        ObjectReference::new("quran", "verse:1:1").expect("test anchor should be valid"),
    )
    .expect("neighborhood request should be valid");
    let similarity = QueryRequest::similarity(
        IndexId::from_bytes([5; INDEX_ID_BYTE_LEN]),
        KeyMaterial::bytes(vec![1, 2, 3]),
    )
    .expect("similarity request should be valid");
    let filtered = QueryRequest::filtered(
        IndexId::from_bytes([6; INDEX_ID_BYTE_LEN]),
        AtomicQuery::Text {
            query: KeyMaterial::text("base"),
            parameters: None,
        },
        KeyMaterial::text("filter"),
    )
    .expect("filtered request should be valid");

    let hybrid = QueryRequest::hybrid(
        IndexId::from_bytes([7; INDEX_ID_BYTE_LEN]),
        vec![
            hybrid_component(AtomicQuery::Text {
                query: KeyMaterial::text("one"),
                parameters: None,
            }),
            hybrid_component(AtomicQuery::Exact {
                key: KeyMaterial::text("two"),
            }),
        ],
    )
    .expect("hybrid request should be valid");

    assert!(matches!(exact.query_kind(), QueryKind::Exact { .. }));
    assert!(matches!(text.query_kind(), QueryKind::Text { .. }));
    assert!(matches!(
        structured.query_kind(),
        QueryKind::Structured { .. }
    ));
    assert!(matches!(
        neighborhood.query_kind(),
        QueryKind::Neighborhood { .. }
    ));
    assert!(matches!(
        similarity.query_kind(),
        QueryKind::Similarity { .. }
    ));
    assert!(matches!(filtered.query_kind(), QueryKind::Filtered { .. }));
    assert!(matches!(hybrid.query_kind(), QueryKind::Hybrid { .. }));
}

#[test]
fn logical_request_preserves_selection_hints_limit_consistency_and_result_mode() {
    let selected = candidate(
        10,
        IndexFamily::Inverted,
        "request-contract",
        "published-v1",
        20,
        20,
    );

    let request = QueryRequest::with_spec(
        *selected.index_id(),
        Some(namespace("tests.query")),
        Some(definition_id("request-contract")),
        Some(IndexFamily::Inverted),
        QueryKind::Text {
            query: KeyMaterial::text("mercy"),
            parameters: Some(KeyMaterial::Unsigned(2)),
        },
        NonZeroUsize::new(25),
        ConsistencyMode::VersionPinned(
            IndexVersionId::new("published-v1").expect("test version ID should be valid"),
        ),
        ResultMode::ReferencesWithMetadata,
        Some(KeyMaterial::text("opaque-metadata")),
    )
    .expect("request should be valid");

    assert_eq!(request.index_id(), selected.index_id());
    assert_eq!(request.namespace(), Some(&namespace("tests.query")));
    assert_eq!(
        request.definition_id(),
        Some(&definition_id("request-contract"))
    );
    assert_eq!(request.family(), Some(IndexFamily::Inverted));
    assert_eq!(request.limit().map(NonZeroUsize::get), Some(25));
    assert!(matches!(
        request.consistency(),
        ConsistencyMode::VersionPinned(_)
    ));
    assert_eq!(request.result_mode(), ResultMode::ReferencesWithMetadata);
    assert_eq!(
        request.metadata(),
        Some(&KeyMaterial::text("opaque-metadata"))
    );
}

#[test]
fn exact_query_builds_an_exact_logical_plan() {
    let selected = candidate(
        11,
        IndexFamily::Identity,
        "exact-plan",
        "published-v1",
        30,
        30,
    );
    let request = QueryRequest::exact(*selected.index_id(), KeyMaterial::text("lookup"))
        .expect("exact request should be valid");

    let plan = plan_query(
        &request,
        vec![selected],
        &capabilities(&[ProviderCapability::ExactLookup]),
        ProviderAvailability::Available,
    )
    .expect("exact query should plan");

    match plan {
        RetrievalPlan::Exact(plan) => {
            assert_eq!(plan.key(), &KeyMaterial::text("lookup"));
            assert_eq!(
                plan.context().capability_resolution(),
                &CapabilityResolution::Direct(ProviderCapability::ExactLookup)
            );
            assert_eq!(plan.context().result_mode(), ResultMode::ReferencesOnly);
        }
        other => panic!("expected exact plan, got {other:?}"),
    }
}

#[test]
fn text_query_builds_a_text_logical_plan() {
    let selected = candidate(
        12,
        IndexFamily::Inverted,
        "text-plan",
        "published-v1",
        40,
        40,
    );
    let request = QueryRequest::with_query(
        *selected.index_id(),
        QueryKind::Text {
            query: KeyMaterial::text("rust"),
            parameters: Some(KeyMaterial::Unsigned(7)),
        },
    )
    .expect("text request should be valid");

    let plan = plan_query(
        &request,
        vec![selected],
        &capabilities(&[ProviderCapability::TextLookup]),
        ProviderAvailability::Available,
    )
    .expect("text query should plan");

    match plan {
        RetrievalPlan::Text(plan) => {
            assert_eq!(plan.query(), &KeyMaterial::text("rust"));
            assert_eq!(plan.parameters(), Some(&KeyMaterial::Unsigned(7)));
            assert_eq!(
                plan.context().capability_resolution(),
                &CapabilityResolution::Direct(ProviderCapability::TextLookup)
            );
        }
        other => panic!("expected text plan, got {other:?}"),
    }
}

#[test]
fn structured_query_builds_a_structured_logical_plan() {
    let selected = candidate(
        13,
        IndexFamily::Relationship,
        "structured-plan",
        "published-v1",
        50,
        50,
    );
    let fields = KeyMaterial::map([
        ("source", KeyMaterial::text("object-a")),
        ("predicate", KeyMaterial::text("opaque-predicate")),
    ])
    .expect("structured test fields should be valid");

    let request = QueryRequest::structured(*selected.index_id(), fields.clone())
        .expect("structured request should be valid");

    let plan = plan_query(
        &request,
        vec![selected],
        &capabilities(&[ProviderCapability::StructuredLookup]),
        ProviderAvailability::Available,
    )
    .expect("structured query should plan");

    match plan {
        RetrievalPlan::Structured(plan) => {
            assert_eq!(plan.fields(), &fields);
            assert_eq!(
                plan.context().capability_resolution(),
                &CapabilityResolution::Direct(ProviderCapability::StructuredLookup)
            );
        }
        other => panic!("expected structured plan, got {other:?}"),
    }
}

#[test]
fn neighborhood_query_preserves_opaque_anchor_and_builds_the_expected_plan() {
    let selected = candidate(
        14,
        IndexFamily::Relationship,
        "neighborhood-plan",
        "published-v1",
        60,
        60,
    );
    let anchor = ObjectReference::new("quran", "verse:2:255").expect("test anchor should be valid");

    let request = QueryRequest::neighborhood(*selected.index_id(), anchor.clone())
        .expect("neighborhood request should be valid");

    let plan = plan_query(
        &request,
        vec![selected],
        &capabilities(&[ProviderCapability::NeighborhoodLookup]),
        ProviderAvailability::Available,
    )
    .expect("neighborhood query should plan");

    match plan {
        RetrievalPlan::Neighborhood(plan) => {
            assert_eq!(plan.anchor(), &anchor);
            assert!(plan.key().is_none());
            assert_eq!(
                plan.context().capability_resolution(),
                &CapabilityResolution::Direct(ProviderCapability::NeighborhoodLookup)
            );
        }
        other => panic!("expected neighborhood plan, got {other:?}"),
    }
}

#[test]
fn similarity_query_requires_similarity_and_generic_ranking_capabilities() {
    let selected = candidate(
        15,
        IndexFamily::Similarity,
        "similarity-plan",
        "published-v1",
        70,
        70,
    );
    let request = QueryRequest::similarity(*selected.index_id(), KeyMaterial::bytes(vec![1, 2, 3]))
        .expect("similarity request should be valid");

    let missing_ranking = capabilities(&[ProviderCapability::SimilarityLookup]);
    let error = plan_query(
        &request,
        vec![selected.clone()],
        &missing_ranking,
        ProviderAvailability::Available,
    )
    .expect_err("similarity planning must require generic ranking capability");

    assert!(matches!(
        error,
        nizaam_indexing::query::QueryPlanningError::MissingProviderCapabilities { .. }
    ));

    let complete = capabilities(&[
        ProviderCapability::SimilarityLookup,
        ProviderCapability::Ranking,
    ]);

    let plan = plan_query(
        &request,
        vec![selected],
        &complete,
        ProviderAvailability::Available,
    )
    .expect("similarity query should plan when both capabilities are advertised");

    assert!(matches!(plan, RetrievalPlan::Similarity(_)));
    assert_eq!(
        plan.capability_resolution().capabilities(),
        &[
            ProviderCapability::SimilarityLookup,
            ProviderCapability::Ranking
        ]
    );
}

#[test]
fn filtered_query_requires_filtered_lookup_and_preserves_the_atomic_base_query() {
    let selected = candidate(
        16,
        IndexFamily::Inverted,
        "filtered-plan",
        "published-v1",
        80,
        80,
    );
    let base = AtomicQuery::Text {
        query: KeyMaterial::text("rust"),
        parameters: None,
    };
    let filter = KeyMaterial::map([
        ("language", KeyMaterial::text("en")),
        ("published", KeyMaterial::Bool(true)),
    ])
    .expect("test filter should be valid");

    let request = QueryRequest::filtered(*selected.index_id(), base.clone(), filter.clone())
        .expect("filtered request should be valid");

    let plan = plan_query(
        &request,
        vec![selected],
        &capabilities(&[ProviderCapability::FilteredLookup]),
        ProviderAvailability::Available,
    )
    .expect("filtered query should plan");

    match plan {
        RetrievalPlan::Filtered(plan) => {
            assert_eq!(plan.base(), &base);
            assert_eq!(plan.filter(), &filter);
            assert_eq!(
                plan.context().capability_resolution(),
                &CapabilityResolution::Direct(ProviderCapability::FilteredLookup)
            );
        }
        other => panic!("expected filtered plan, got {other:?}"),
    }
}

#[test]
fn native_hybrid_query_uses_the_provider_native_hybrid_capability() {
    let selected = candidate(
        17,
        IndexFamily::Inverted,
        "hybrid-native-plan",
        "published-v1",
        90,
        90,
    );

    let request = QueryRequest::hybrid(
        *selected.index_id(),
        vec![
            hybrid_component(AtomicQuery::Text {
                query: KeyMaterial::text("rust"),
                parameters: None,
            }),
            hybrid_component(AtomicQuery::Exact {
                key: KeyMaterial::text("book"),
            }),
        ],
    )
    .expect("hybrid request should be valid");

    let plan = plan_query(
        &request,
        vec![selected],
        &capabilities(&[ProviderCapability::HybridRetrieval]),
        ProviderAvailability::Available,
    )
    .expect("native hybrid query should plan");

    match plan {
        RetrievalPlan::Hybrid(plan) => {
            assert_eq!(plan.components().len(), 2);
            assert!(matches!(
                plan.capability_resolution(),
                CapabilityResolution::Direct(ProviderCapability::HybridRetrieval)
            ));
            assert_eq!(
                plan.components()[0].query(),
                &AtomicQuery::Text {
                    query: KeyMaterial::text("rust"),
                    parameters: None,
                }
            );
            assert_eq!(
                plan.components()[1].query(),
                &AtomicQuery::Exact {
                    key: KeyMaterial::text("book"),
                }
            );
        }
        other => panic!("expected hybrid plan, got {other:?}"),
    }
}

#[test]
fn composed_hybrid_query_uses_individual_component_capabilities() {
    let selected = candidate(
        18,
        IndexFamily::Inverted,
        "hybrid-composed-plan",
        "published-v1",
        100,
        100,
    );

    let request = QueryRequest::hybrid(
        *selected.index_id(),
        vec![
            hybrid_component(AtomicQuery::Text {
                query: KeyMaterial::text("rust"),
                parameters: None,
            }),
            hybrid_component(AtomicQuery::Exact {
                key: KeyMaterial::text("book"),
            }),
        ],
    )
    .expect("hybrid request should be valid");

    let plan = plan_query(
        &request,
        vec![selected],
        &capabilities(&[
            ProviderCapability::TextLookup,
            ProviderCapability::ExactLookup,
        ]),
        ProviderAvailability::Available,
    )
    .expect("composed hybrid query should plan");

    match plan {
        RetrievalPlan::Hybrid(plan) => {
            assert_eq!(
                plan.capability_resolution().capabilities(),
                &[
                    ProviderCapability::TextLookup,
                    ProviderCapability::ExactLookup
                ]
            );
            assert_eq!(
                plan.components()[0].capability(),
                ProviderCapability::TextLookup
            );
            assert_eq!(
                plan.components()[1].capability(),
                ProviderCapability::ExactLookup
            );
        }
        other => panic!("expected composed hybrid plan, got {other:?}"),
    }
}

#[test]
fn selection_hints_are_enforced_by_the_logical_planner() {
    let selected = candidate(
        19,
        IndexFamily::Inverted,
        "selection-hints",
        "published-v1",
        110,
        110,
    );

    let request = QueryRequest::with_spec(
        *selected.index_id(),
        Some(namespace("tests.query")),
        Some(definition_id("selection-hints")),
        Some(IndexFamily::Inverted),
        QueryKind::Text {
            query: KeyMaterial::text("term"),
            parameters: None,
        },
        None,
        ConsistencyMode::Current,
        ResultMode::ReferencesOnly,
        None,
    )
    .expect("matching selection hints should be valid");

    let plan = plan_query(
        &request,
        vec![selected.clone()],
        &capabilities(&[ProviderCapability::TextLookup]),
        ProviderAvailability::Available,
    )
    .expect("matching selection hints should plan");

    assert_eq!(
        plan.target()
            .expect("single-index query should have a target")
            .definition(),
        selected.definition()
    );

    let wrong = QueryRequest::with_spec(
        *selected.index_id(),
        Some(namespace("tests.query")),
        Some(definition_id("different-definition")),
        Some(IndexFamily::Inverted),
        QueryKind::Text {
            query: KeyMaterial::text("term"),
            parameters: None,
        },
        None,
        ConsistencyMode::Current,
        ResultMode::ReferencesOnly,
        None,
    )
    .expect("mismatching definition hint is still structurally valid");

    let error = plan_query(
        &wrong,
        vec![selected],
        &capabilities(&[ProviderCapability::TextLookup]),
        ProviderAvailability::Available,
    )
    .expect_err("mismatching selection hints must be rejected");

    assert!(matches!(
        error,
        nizaam_indexing::query::QueryPlanningError::SelectionHintsMismatch { .. }
    ));
}

#[test]
fn current_query_accepts_a_zero_lag_published_candidate_and_rejects_positive_lag() {
    let current = candidate(
        20,
        IndexFamily::Inverted,
        "current-query",
        "published-current",
        120,
        120,
    );
    let request = QueryRequest::text(*current.index_id(), KeyMaterial::text("current"))
        .expect("text request should be valid");

    let plan = plan_query(
        &request,
        vec![current.clone()],
        &capabilities(&[ProviderCapability::TextLookup]),
        ProviderAvailability::Available,
    )
    .expect("zero-lag Current query should plan");

    assert_eq!(
        plan.target()
            .expect("single-index query should have a target")
            .version()
            .id(),
        current.version().version().id()
    );
    assert_eq!(
        plan.consistency()
            .expect("single-index query should have a consistency evaluation")
            .update_sequence_lag(),
        Some(0)
    );

    let lagged = candidate(
        21,
        IndexFamily::Inverted,
        "current-query-lagged",
        "published-lagged",
        120,
        119,
    );
    let lagged_request = QueryRequest::text(*lagged.index_id(), KeyMaterial::text("lagged"))
        .expect("lagged query should still be structurally valid");

    let error = plan_query(
        &lagged_request,
        vec![lagged],
        &capabilities(&[ProviderCapability::TextLookup]),
        ProviderAvailability::Available,
    )
    .expect_err("Current must reject positive update-sequence lag");

    assert!(matches!(
        error,
        nizaam_indexing::query::QueryPlanningError::ConsistencyUnsatisfied { .. }
    ));
}

#[test]
fn version_pinned_query_selects_the_exact_requested_published_version() {
    let selected = candidate(
        22,
        IndexFamily::Inverted,
        "pinned-query",
        "published-v7",
        130,
        130,
    );
    let request = QueryRequest::with_query_options(
        *selected.index_id(),
        QueryKind::Text {
            query: KeyMaterial::text("pinned"),
            parameters: None,
        },
        None,
        ConsistencyMode::VersionPinned(
            IndexVersionId::new("published-v7").expect("test version ID should be valid"),
        ),
        ResultMode::ReferencesOnly,
        None,
    )
    .expect("pinned request should be valid");

    let plan = plan_query(
        &request,
        vec![selected.clone()],
        &capabilities(&[ProviderCapability::TextLookup]),
        ProviderAvailability::Available,
    )
    .expect("matching pinned version should plan");

    assert_eq!(
        plan.target()
            .expect("single-index query should have a target")
            .version()
            .id()
            .as_str(),
        "published-v7"
    );
    assert!(matches!(
        plan.consistency()
            .expect("pinned query should have a consistency evaluation")
            .state(),
        nizaam_indexing::consistency::ConsistencyEvaluationState::VersionPinned
    ));
}

#[test]
fn unavailable_provider_is_rejected_before_logical_query_plan_creation() {
    let selected = candidate(
        23,
        IndexFamily::Identity,
        "unavailable-provider",
        "published-v1",
        140,
        140,
    );
    let request = QueryRequest::exact(*selected.index_id(), KeyMaterial::text("term"))
        .expect("exact request should be valid");

    let error = plan_query(
        &request,
        vec![selected],
        &capabilities(&[ProviderCapability::ExactLookup]),
        ProviderAvailability::TemporarilyUnavailable,
    )
    .expect_err("an unavailable provider must not produce a retrieval plan");

    assert!(matches!(
        error,
        nizaam_indexing::query::QueryPlanningError::ProviderUnavailable {
            availability: ProviderAvailability::TemporarilyUnavailable
        }
    ));
}

#[test]
fn unsupported_logical_query_capability_is_not_silently_rewritten() {
    let selected = candidate(
        24,
        IndexFamily::Similarity,
        "unsupported-capability",
        "published-v1",
        150,
        150,
    );
    let request = QueryRequest::similarity(*selected.index_id(), KeyMaterial::bytes(vec![9, 8, 7]))
        .expect("similarity request should be valid");

    let error = plan_query(
        &request,
        vec![selected],
        &capabilities(&[ProviderCapability::ExactLookup]),
        ProviderAvailability::Available,
    )
    .expect_err("an unsupported query must remain an error");

    assert!(matches!(
        error,
        nizaam_indexing::query::QueryPlanningError::MissingProviderCapabilities { .. }
    ));
}
