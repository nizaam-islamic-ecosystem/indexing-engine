//! Query retrieval execution for the Phase 4 Indexing Engine.
//!
//! This module is the execution boundary after [`crate::query::planner`]
//! produces a validated logical [`crate::query::planner::RetrievalPlan`].
//!
//! The responsibilities are intentionally narrow:
//!
//! ```text
//! RetrievalPlan
//!      ↓
//! provider capability re-check
//!      ↓
//! provider execution
//!      ↓
//! generic reference candidates
//!      ↓
//! QueryHit
//!      ↓
//! QueryResult
//! ```
//!
//! The module does not:
//!
//! - plan or select indexes;
//! - evaluate consistency policies again;
//! - publish or activate index versions;
//! - interpret domain semantics;
//! - hydrate source-owned domain objects;
//! - expose physical index algorithms;
//! - create a second runtime/cancellation/deadline system.
//!
//! The Core [`nizaam_core::operation::OperationContext`] supplied by the
//! existing Core execution path is forwarded unchanged to every provider call.
//! Core therefore remains the owner of operation identity, cancellation,
//! deadline, and universal execution infrastructure.
//!
//! The current `provider::mod` boundary contains logical capability
//! advertisement and generic ranking. It does not yet contain a concrete
//! physical retrieval implementation. The [`ProviderRetriever`] trait below is
//! therefore the minimal execution-side adapter contract needed by this file.
//! A concrete physical provider can implement it without exposing its storage
//! technology through the logical query API.
//!
//! Generic ranking remains provider-owned. For example, a similarity provider
//! can use [`crate::provider::rank_candidates`] before returning its candidates;
//! this retrieval layer does not invent a ranking direction or semantic
//! relevance rule.
//!
//! # Important result-model boundary
//!
//! `QueryResult` currently contains one logical definition identity and one
//! `IndexVersion`. Consequently, the execution layer can faithfully construct
//! hybrid results when all components share one logical target/version. A
//! heterogeneous hybrid plan containing multiple logical targets cannot be
//! represented without inventing a misleading aggregate version. Such a plan
//! is therefore rejected at result assembly time until the result contract is
//! extended to represent per-component provenance.
//!
//! A successful provider call returning zero candidates remains a successful
//! empty query result. A provider error is always preserved as an execution
//! error and is never converted into an empty result.
//!
//! Core's operation context is deliberately passed through rather than
//! recreated here, matching the Phase 4 boundary that Core owns universal
//! execution context and Indexing owns retrieval orchestration.

use core::fmt;
use core::num::NonZeroUsize;
use std::collections::BTreeMap;

use nizaam_core::operation::OperationContext;

use super::planner::{
    CapabilityResolution, ExactRetrievalPlan, FilteredRetrievalPlan, HybridRetrievalPlan,
    NeighborhoodRetrievalPlan, PlannedHybridComponent, RetrievalPlan, SimilarityRetrievalPlan,
    StructuredRetrievalPlan, TextRetrievalPlan,
};
use super::request::ResultMode;
use super::result::{
    ConsistencyMetadata, ConsistencyMetadataValidationError, ConsistencyState, QueryHit,
    QueryHitValidationError, QueryResult, QueryResultValidationError,
};
use crate::consistency::policy::{ConsistencyEvaluation, ConsistencyEvaluationState};
use crate::identity::IndexDefinitionIdentity;
use crate::index::IndexVersion;
use crate::provider::ranking::{RankingCandidate, RankingCandidateValidationError};
use crate::provider::{
    ProviderAvailability, ProviderCapabilities, ProviderCapability, ProviderCapabilityError,
};

/// Physical-provider execution adapter consumed by the Indexing retrieval
/// layer.
///
/// This is intentionally an execution-facing trait rather than another
/// provider-capability model. [`ProviderCapabilities`] continues to describe
/// what a provider advertises, while this trait describes how a concrete
/// provider executes the already-planned logical retrieval operations.
///
/// Every method receives the exact Core [`OperationContext`] supplied by the
/// caller. The trait never creates a replacement cancellation token, deadline,
/// or operation context.
///
/// Provider implementations return generic [`RankingCandidate`] values. A
/// candidate remains reference-oriented and may carry optional score,
/// distance, ordering-key, and opaque metadata. The retrieval layer converts
/// these values into the canonical [`QueryHit`] contract.
pub trait ProviderRetriever {
    /// Provider-specific failure type.
    type Error: std::error::Error + Send + Sync + 'static;

    /// Returns the provider's logical capability advertisement.
    fn capabilities(&self) -> &ProviderCapabilities;

    /// Returns the provider's current execution availability.
    fn availability(&self) -> ProviderAvailability;

    /// Executes an exact logical lookup.
    fn retrieve_exact(
        &self,
        plan: &ExactRetrievalPlan,
        context: &OperationContext,
    ) -> Result<Vec<RankingCandidate>, Self::Error>;

    /// Executes a text/inverted logical lookup.
    fn retrieve_text(
        &self,
        plan: &TextRetrievalPlan,
        context: &OperationContext,
    ) -> Result<Vec<RankingCandidate>, Self::Error>;

    /// Executes a structured logical lookup.
    fn retrieve_structured(
        &self,
        plan: &StructuredRetrievalPlan,
        context: &OperationContext,
    ) -> Result<Vec<RankingCandidate>, Self::Error>;

    /// Executes a neighborhood logical lookup.
    fn retrieve_neighborhood(
        &self,
        plan: &NeighborhoodRetrievalPlan,
        context: &OperationContext,
    ) -> Result<Vec<RankingCandidate>, Self::Error>;

    /// Executes a similarity logical lookup.
    ///
    /// The planner requires the provider to advertise both
    /// `SimilarityLookup` and generic `Ranking` for this operation. The
    /// provider may use [`crate::provider::rank_candidates`] to establish the
    /// returned ordering before handing candidates back to this layer.
    fn retrieve_similarity(
        &self,
        plan: &SimilarityRetrievalPlan,
        context: &OperationContext,
    ) -> Result<Vec<RankingCandidate>, Self::Error>;

    /// Executes a filtered logical lookup.
    ///
    /// Filtering remains provider-side execution. This layer does not
    /// interpret the generic filter material.
    fn retrieve_filtered(
        &self,
        plan: &FilteredRetrievalPlan,
        context: &OperationContext,
    ) -> Result<Vec<RankingCandidate>, Self::Error>;

    /// Executes a provider-native hybrid logical lookup.
    ///
    /// This method is only used when the planner selected
    /// `ProviderCapability::HybridRetrieval` directly.
    fn retrieve_hybrid(
        &self,
        plan: &HybridRetrievalPlan,
        context: &OperationContext,
    ) -> Result<Vec<RankingCandidate>, Self::Error>;

    /// Executes one component of a composed hybrid retrieval.
    ///
    /// The planner selects this path when a provider-native
    /// `HybridRetrieval` capability is unavailable but the individual
    /// component capabilities are available.
    fn retrieve_hybrid_component(
        &self,
        component: &PlannedHybridComponent,
        context: &OperationContext,
    ) -> Result<Vec<RankingCandidate>, Self::Error>;
}

/// Structural failures detected before provider execution.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RetrievalPlanValidationError {
    /// A single-index plan references a non-published version.
    UnqueryableTarget {
        /// Logical index associated with the invalid target.
        definition_identity: IndexDefinitionIdentity,
        /// Lifecycle observed on the plan target.
        lifecycle: crate::index::VersionLifecycle,
    },

    /// A hybrid plan contains no components.
    EmptyHybridPlan,

    /// A hybrid plan contains fewer than two components.
    InsufficientHybridComponents { count: usize },

    /// A hybrid plan has a capability resolution that cannot be executed by
    /// this retrieval adapter.
    InvalidHybridCapabilityResolution { resolution: CapabilityResolution },

    /// A heterogeneous hybrid result cannot be faithfully represented by the
    /// current single-index `QueryResult` contract.
    HeterogeneousHybridResultTargets,
}

impl fmt::Display for RetrievalPlanValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnqueryableTarget { definition_identity, lifecycle } => write!(
                formatter,
                "retrieval plan targets logical index {definition_identity:?} at non-queryable lifecycle {lifecycle:?}"
            ),
            Self::EmptyHybridPlan => {
                formatter.write_str("hybrid retrieval plan contains no components")
            }
            Self::InsufficientHybridComponents { count } => write!(
                formatter,
                "hybrid retrieval plan requires at least two components, got {count}"
            ),
            Self::InvalidHybridCapabilityResolution { resolution } => write!(
                formatter,
                "hybrid retrieval plan has an unsupported capability resolution: {resolution:?}"
            ),
            Self::HeterogeneousHybridResultTargets => formatter.write_str(
                "hybrid retrieval targets multiple logical definition/version pairs that cannot be represented by the current single-target QueryResult contract",
            ),
        }
    }
}

impl std::error::Error for RetrievalPlanValidationError {}

/// Failures raised while converting generic provider candidates into
/// reference-oriented query results.
#[derive(Clone, Debug, PartialEq)]
pub enum RetrievalResultError {
    /// A provider returned a structurally invalid ranking candidate.
    InvalidCandidate {
        /// Position of the invalid candidate.
        position: usize,
        /// Candidate validation failure.
        error: RankingCandidateValidationError,
    },

    /// A candidate could not be converted into a valid `QueryHit`.
    InvalidHit {
        /// Position of the invalid hit.
        position: usize,
        /// Query-hit validation failure.
        error: QueryHitValidationError,
    },

    /// Consistency metadata could not be represented by the current result
    /// contract.
    InvalidConsistency(ConsistencyMetadataValidationError),

    /// The final query result violated its structural contract.
    InvalidResult(QueryResultValidationError),
}

impl fmt::Display for RetrievalResultError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidCandidate { position, error } => {
                write!(
                    formatter,
                    "invalid provider candidate at position {position}: {error}"
                )
            }
            Self::InvalidHit { position, error } => {
                write!(
                    formatter,
                    "invalid query hit at position {position}: {error}"
                )
            }
            Self::InvalidConsistency(error) => {
                write!(formatter, "invalid consistency metadata: {error}")
            }
            Self::InvalidResult(error) => {
                write!(formatter, "invalid query result: {error}")
            }
        }
    }
}

impl std::error::Error for RetrievalResultError {}

/// Errors produced by the Phase 4 retrieval execution boundary.
///
/// The provider's own error type remains preserved in
/// [`RetrievalError::Provider`] so a backend failure cannot be mistaken for a
/// successful empty result.
#[derive(Debug)]
pub enum RetrievalError<E>
where
    E: std::error::Error + Send + Sync + 'static,
{
    /// The logical plan was structurally unsuitable for execution.
    InvalidPlan(RetrievalPlanValidationError),

    /// The provider is unavailable at execution time.
    ProviderUnavailable {
        /// Current provider availability.
        availability: ProviderAvailability,
    },

    /// The plan requires a capability no longer advertised by the provider.
    MissingProviderCapability(ProviderCapabilityError),

    /// The provider rejected or failed the physical retrieval operation.
    Provider(E),

    /// Provider candidates could not be converted into the canonical logical
    /// result contract.
    ResultConversion(RetrievalResultError),

    /// A hybrid operation could not be assembled without misrepresenting its
    /// per-component target/version provenance.
    HybridResultUnsupported(RetrievalPlanValidationError),
}

impl<E> fmt::Display for RetrievalError<E>
where
    E: std::error::Error + Send + Sync + 'static,
{
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPlan(error) => write!(formatter, "invalid retrieval plan: {error}"),
            Self::ProviderUnavailable { availability } => {
                write!(formatter, "provider is {availability}")
            }
            Self::MissingProviderCapability(error) => {
                write!(formatter, "provider capability mismatch: {error}")
            }
            Self::Provider(error) => write!(formatter, "provider retrieval failed: {error}"),
            Self::ResultConversion(error) => {
                write!(formatter, "retrieval result conversion failed: {error}")
            }
            Self::HybridResultUnsupported(error) => {
                write!(
                    formatter,
                    "hybrid retrieval result cannot be represented: {error}"
                )
            }
        }
    }
}

impl<E> std::error::Error for RetrievalError<E>
where
    E: std::error::Error + Send + Sync + 'static,
{
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidPlan(error) => Some(error),
            Self::Provider(error) => Some(error),
            Self::ResultConversion(error) => Some(error),
            Self::HybridResultUnsupported(error) => Some(error),
            Self::ProviderUnavailable { .. } | Self::MissingProviderCapability(_) => None,
        }
    }
}

impl<E> From<E> for RetrievalError<E>
where
    E: std::error::Error + Send + Sync + 'static,
{
    fn from(error: E) -> Self {
        Self::Provider(error)
    }
}

/// Executes a previously planned logical retrieval through a concrete provider.
///
/// The function performs a small execution-time revalidation step because
/// provider availability and capability advertisement may change after the
/// planner produced the plan.
///
/// The supplied Core [`OperationContext`] is passed unchanged to provider
/// execution. This module never constructs, replaces, or owns cancellation or
/// deadline state.
pub fn execute<P>(
    plan: &RetrievalPlan,
    provider: &P,
    context: &OperationContext,
) -> Result<QueryResult, RetrievalError<P::Error>>
where
    P: ProviderRetriever,
{
    validate_plan(plan).map_err(RetrievalError::InvalidPlan)?;

    let availability = provider.availability();
    if availability != ProviderAvailability::Available {
        return Err(RetrievalError::ProviderUnavailable { availability });
    }

    provider
        .capabilities()
        .require_all(plan.capability_resolution().capabilities().iter().copied())
        .map_err(RetrievalError::MissingProviderCapability)?;

    match plan {
        RetrievalPlan::Exact(plan) => {
            let candidates = provider
                .retrieve_exact(plan, context)
                .map_err(RetrievalError::Provider)?;

            build_single_result(
                plan.context().target().definition_identity(),
                plan.context().target().version(),
                plan.context().target().consistency(),
                plan.context().result_mode(),
                plan.context().limit(),
                candidates,
            )
            .map_err(RetrievalError::ResultConversion)
        }

        RetrievalPlan::Text(plan) => {
            let candidates = provider
                .retrieve_text(plan, context)
                .map_err(RetrievalError::Provider)?;

            build_single_result(
                plan.context().target().definition_identity(),
                plan.context().target().version(),
                plan.context().target().consistency(),
                plan.context().result_mode(),
                plan.context().limit(),
                candidates,
            )
            .map_err(RetrievalError::ResultConversion)
        }

        RetrievalPlan::Structured(plan) => {
            let candidates = provider
                .retrieve_structured(plan, context)
                .map_err(RetrievalError::Provider)?;

            build_single_result(
                plan.context().target().definition_identity(),
                plan.context().target().version(),
                plan.context().target().consistency(),
                plan.context().result_mode(),
                plan.context().limit(),
                candidates,
            )
            .map_err(RetrievalError::ResultConversion)
        }

        RetrievalPlan::Neighborhood(plan) => {
            let candidates = provider
                .retrieve_neighborhood(plan, context)
                .map_err(RetrievalError::Provider)?;

            build_single_result(
                plan.context().target().definition_identity(),
                plan.context().target().version(),
                plan.context().target().consistency(),
                plan.context().result_mode(),
                plan.context().limit(),
                candidates,
            )
            .map_err(RetrievalError::ResultConversion)
        }

        RetrievalPlan::Similarity(plan) => {
            let candidates = provider
                .retrieve_similarity(plan, context)
                .map_err(RetrievalError::Provider)?;

            build_single_result(
                plan.context().target().definition_identity(),
                plan.context().target().version(),
                plan.context().target().consistency(),
                plan.context().result_mode(),
                plan.context().limit(),
                candidates,
            )
            .map_err(RetrievalError::ResultConversion)
        }

        RetrievalPlan::Filtered(plan) => {
            let candidates = provider
                .retrieve_filtered(plan, context)
                .map_err(RetrievalError::Provider)?;

            build_single_result(
                plan.context().target().definition_identity(),
                plan.context().target().version(),
                plan.context().target().consistency(),
                plan.context().result_mode(),
                plan.context().limit(),
                candidates,
            )
            .map_err(RetrievalError::ResultConversion)
        }

        RetrievalPlan::Hybrid(plan) => execute_hybrid(plan, provider, context),
    }
}

/// Validates execution-time invariants of a logical retrieval plan.
///
/// The planner is the primary validation owner. These checks are defensive
/// execution-boundary checks that prevent an invalid/unpublished plan from
/// reaching a physical provider if a plan is retained longer than intended.
pub fn validate_plan(plan: &RetrievalPlan) -> Result<(), RetrievalPlanValidationError> {
    match plan {
        RetrievalPlan::Exact(plan) => validate_published_target(plan.context().target()),
        RetrievalPlan::Text(plan) => validate_published_target(plan.context().target()),
        RetrievalPlan::Structured(plan) => validate_published_target(plan.context().target()),
        RetrievalPlan::Neighborhood(plan) => validate_published_target(plan.context().target()),
        RetrievalPlan::Similarity(plan) => validate_published_target(plan.context().target()),
        RetrievalPlan::Filtered(plan) => validate_published_target(plan.context().target()),
        RetrievalPlan::Hybrid(plan) => {
            if plan.components().is_empty() {
                return Err(RetrievalPlanValidationError::EmptyHybridPlan);
            }

            if plan.components().len() < 2 {
                return Err(RetrievalPlanValidationError::InsufficientHybridComponents {
                    count: plan.components().len(),
                });
            }

            for component in plan.components() {
                if component.target().lifecycle() != crate::index::VersionLifecycle::Published {
                    return Err(RetrievalPlanValidationError::UnqueryableTarget {
                        definition_identity: component.target().definition_identity().clone(),
                        lifecycle: component.target().lifecycle(),
                    });
                }
            }

            match plan.capability_resolution() {
                CapabilityResolution::Direct(ProviderCapability::HybridRetrieval)
                | CapabilityResolution::Required(_) => Ok(()),
                resolution => Err(
                    RetrievalPlanValidationError::InvalidHybridCapabilityResolution {
                        resolution: resolution.clone(),
                    },
                ),
            }
        }
    }
}

fn validate_published_target(
    target: &crate::query::planner::PlannedIndex,
) -> Result<(), RetrievalPlanValidationError> {
    if target.lifecycle() != crate::index::VersionLifecycle::Published {
        return Err(RetrievalPlanValidationError::UnqueryableTarget {
            definition_identity: target.definition_identity().clone(),
            lifecycle: target.lifecycle(),
        });
    }

    Ok(())
}

fn execute_hybrid<P>(
    plan: &HybridRetrievalPlan,
    provider: &P,
    context: &OperationContext,
) -> Result<QueryResult, RetrievalError<P::Error>>
where
    P: ProviderRetriever,
{
    if !hybrid_targets_are_homogeneous(plan) {
        return Err(RetrievalError::HybridResultUnsupported(
            RetrievalPlanValidationError::HeterogeneousHybridResultTargets,
        ));
    }

    let candidates = match plan.capability_resolution() {
        CapabilityResolution::Direct(ProviderCapability::HybridRetrieval) => provider
            .retrieve_hybrid(plan, context)
            .map_err(RetrievalError::Provider)?,

        CapabilityResolution::Required(_) => {
            // Components are merged in request declaration order. Cross-component
            // ranking is not performed here; provider-side ranking remains the
            // provider's responsibility. Duplicate references are normalized after
            // candidate conversion so optional hit data can be retained before
            // the final logical result limit is applied.
            let mut combined = Vec::new();

            for component in plan.components() {
                let component_capability = component.capability();

                provider
                    .capabilities()
                    .require(component_capability)
                    .map_err(RetrievalError::MissingProviderCapability)?;

                let candidates = provider
                    .retrieve_hybrid_component(component, context)
                    .map_err(RetrievalError::Provider)?;

                combined.extend(candidates);
            }

            combined
        }

        resolution => {
            return Err(RetrievalError::InvalidPlan(
                RetrievalPlanValidationError::InvalidHybridCapabilityResolution {
                    resolution: resolution.clone(),
                },
            ));
        }
    };

    build_hybrid_result(plan, candidates).map_err(RetrievalError::ResultConversion)
}

fn build_single_result(
    definition_identity: &IndexDefinitionIdentity,
    version: &IndexVersion,
    consistency: &ConsistencyEvaluation,
    result_mode: ResultMode,
    limit: Option<NonZeroUsize>,
    candidates: Vec<RankingCandidate>,
) -> Result<QueryResult, RetrievalResultError> {
    let mut hits = convert_candidates(candidates, result_mode)?;

    apply_limit(&mut hits, limit);

    let metadata = consistency_metadata(consistency)?;

    QueryResult::with_consistency(definition_identity.clone(), version.clone(), hits, metadata)
        .map_err(RetrievalResultError::InvalidResult)
}

fn hybrid_targets_are_homogeneous(plan: &HybridRetrievalPlan) -> bool {
    let Some(first) = plan.components().first() else {
        return false;
    };

    let first_target = first.target();
    plan.components().iter().all(|component| {
        component.target().definition_identity() == first_target.definition_identity()
            && component.target().version() == first_target.version()
    })
}

fn build_hybrid_result(
    plan: &HybridRetrievalPlan,
    candidates: Vec<RankingCandidate>,
) -> Result<QueryResult, RetrievalResultError> {
    let first = plan
        .components()
        .first()
        .expect("hybrid validation guarantees at least one component");

    let first_target = first.target();

    let mut hits = convert_candidates(candidates, plan.result_mode())?;
    deduplicate_hybrid_hits(&mut hits);
    apply_limit(&mut hits, plan.limit());

    let consistency = consistency_metadata(first_target.consistency())?;

    QueryResult::with_consistency(
        first_target.definition_identity().clone(),
        first_target.version().clone(),
        hits,
        consistency,
    )
    .map_err(RetrievalResultError::InvalidResult)
}

/// Deduplicates hybrid results by source-owned object reference before the
/// logical result limit is applied.
///
/// Component order remains the first-occurrence order. When the same reference
/// is returned by multiple components, later candidates only fill optional
/// fields that the first candidate did not provide. The retrieval layer does
/// not compare or interpret score/distance values because their direction and
/// semantics remain provider-owned.
fn deduplicate_hybrid_hits(hits: &mut Vec<QueryHit>) {
    let mut positions = BTreeMap::new();
    let mut deduplicated = Vec::with_capacity(hits.len());

    for hit in hits.drain(..) {
        let reference = hit.reference().clone();

        if let Some(&position) = positions.get(&reference) {
            merge_hybrid_hit(&mut deduplicated[position], hit);
        } else {
            let position = deduplicated.len();
            positions.insert(reference, position);
            deduplicated.push(hit);
        }
    }

    *hits = deduplicated;
}

/// Preserves first-occurrence ordering while carrying forward optional
/// information that was absent from the first candidate.
///
/// If both candidates provide the same optional metric/metadata field, the
/// first occurrence is retained because this layer has no provider-declared
/// rule for deciding which value is semantically preferable.
fn merge_hybrid_hit(existing: &mut QueryHit, incoming: QueryHit) {
    let (reference, score, distance, metadata) = existing.clone().into_parts();
    let (_, incoming_score, incoming_distance, incoming_metadata) = incoming.into_parts();

    let score = score.or(incoming_score);
    let distance = distance.or(incoming_distance);
    let metadata = metadata.or(incoming_metadata);

    let merged = QueryHit::new(reference)
        .with_metrics(score, distance)
        .expect("validated hybrid hits must contain finite metrics");

    let merged = if let Some(metadata) = metadata {
        merged
            .with_metadata(metadata)
            .expect("validated hybrid hit metadata must remain valid")
    } else {
        merged
    };

    *existing = merged;
}

fn convert_candidates(
    candidates: Vec<RankingCandidate>,
    result_mode: ResultMode,
) -> Result<Vec<QueryHit>, RetrievalResultError> {
    let mut hits = Vec::with_capacity(candidates.len());

    for (position, candidate) in candidates.into_iter().enumerate() {
        candidate
            .validate()
            .map_err(|error| RetrievalResultError::InvalidCandidate { position, error })?;

        let (reference, score, distance, _ordering_key, metadata) = candidate.into_parts();

        let (score, distance, metadata) = match result_mode {
            ResultMode::ReferencesOnly => (None, None, None),
            ResultMode::ReferencesWithMetadata => (score, distance, metadata),
        };

        let hit = QueryHit::new(reference)
            .with_metrics(score, distance)
            .map_err(|error| RetrievalResultError::InvalidHit { position, error })?;

        let hit = if let Some(metadata) = metadata {
            hit.with_metadata(metadata)
                .map_err(|error| RetrievalResultError::InvalidHit { position, error })?
        } else {
            hit
        };

        hits.push(hit);
    }

    Ok(hits)
}

fn consistency_metadata(
    evaluation: &ConsistencyEvaluation,
) -> Result<ConsistencyMetadata, RetrievalResultError> {
    let state = match evaluation.state() {
        ConsistencyEvaluationState::Fresh => ConsistencyState::Fresh,
        ConsistencyEvaluationState::StaleAccepted => ConsistencyState::StaleAccepted,
        ConsistencyEvaluationState::VersionPinned => ConsistencyState::VersionPinned,
    };

    ConsistencyMetadata::new(
        evaluation.mode().clone(),
        state,
        evaluation.source_update_sequence(),
        evaluation.indexed_update_sequence(),
        evaluation.update_sequence_lag(),
        evaluation.indexed_source_version().cloned(),
        evaluation.time_lag(),
    )
    .map_err(RetrievalResultError::InvalidConsistency)
}

fn apply_limit(hits: &mut Vec<QueryHit>, limit: Option<NonZeroUsize>) {
    if let Some(limit) = limit {
        let limit = limit.get();
        if hits.len() > limit {
            hits.truncate(limit);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use core::time::Duration;
    use std::fmt::{Display, Formatter};
    use std::sync::{Arc, Mutex};

    use super::super::planner::IndexCandidate;
    use super::super::request::ResultMode;
    use super::super::request::{AtomicQuery, HybridQueryComponent, QueryKind, QueryRequest};
    use crate::consistency::policy::{ConsistencyMode, FreshnessPolicy};
    use crate::consistency::synchronization::SynchronizationSnapshot;
    use crate::identity::{IndexDefinitionId, IndexDefinitionIdentity, IndexNamespace};
    use crate::index::{
        ConsistencyRequirement, IndexDefinition, IndexFamily, IndexVersion, IndexVersionId,
        IndexVersionState, KeyDefinition, KeyMaterial, ObjectReference, SchemaVersion,
        SourceVersion, TargetReferenceType, Uniqueness, VersionLifecycle,
    };
    use crate::provider::{
        ProviderAvailability, ProviderCapabilities, ProviderCapability, ProviderRankingError,
        rank_candidates,
    };

    #[derive(Clone, Debug)]
    struct FakeProvider {
        capabilities: ProviderCapabilities,
        availability: ProviderAvailability,
        behavior: ProviderBehavior,
        calls: Arc<Mutex<Vec<String>>>,
        context_addresses: Arc<Mutex<Vec<usize>>>,
    }

    #[derive(Clone, Debug)]
    enum ProviderBehavior {
        Return(Vec<RankingCandidate>),
        Fail,
        HybridPerComponent,
        HybridPerComponentWithDuplicates,
        RankSimilarity,
    }

    #[derive(Clone, Debug, Eq, PartialEq)]
    struct FakeProviderError(&'static str);

    impl Display for FakeProviderError {
        fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
            formatter.write_str(self.0)
        }
    }

    impl std::error::Error for FakeProviderError {}

    impl FakeProvider {
        fn new(capabilities: ProviderCapabilities, behavior: ProviderBehavior) -> Self {
            Self {
                capabilities,
                availability: ProviderAvailability::Available,
                behavior,
                calls: Arc::new(Mutex::new(Vec::new())),
                context_addresses: Arc::new(Mutex::new(Vec::new())),
            }
        }

        fn unavailable(capabilities: ProviderCapabilities, behavior: ProviderBehavior) -> Self {
            Self {
                capabilities,
                availability: ProviderAvailability::TemporarilyUnavailable,
                behavior,
                calls: Arc::new(Mutex::new(Vec::new())),
                context_addresses: Arc::new(Mutex::new(Vec::new())),
            }
        }

        fn record(&self, operation: &str, context: &OperationContext) {
            self.calls
                .lock()
                .expect("call log mutex should not be poisoned")
                .push(operation.to_owned());

            self.context_addresses
                .lock()
                .expect("context log mutex should not be poisoned")
                .push(context as *const OperationContext as usize);
        }

        fn calls(&self) -> Vec<String> {
            self.calls
                .lock()
                .expect("call log mutex should not be poisoned")
                .clone()
        }

        fn context_was_forwarded(&self, context: &OperationContext) -> bool {
            let address = context as *const OperationContext as usize;
            self.context_addresses
                .lock()
                .expect("context log mutex should not be poisoned")
                .contains(&address)
        }

        fn candidate(
            source: &str,
            object: &str,
            score: Option<f64>,
            distance: Option<f64>,
        ) -> RankingCandidate {
            RankingCandidate::new(
                ObjectReference::new(source, object).expect("test reference should be valid"),
            )
            .with_metrics(score, distance)
            .expect("test candidate metrics should be valid")
        }
    }

    impl ProviderRetriever for FakeProvider {
        type Error = FakeProviderError;

        fn capabilities(&self) -> &ProviderCapabilities {
            &self.capabilities
        }

        fn availability(&self) -> ProviderAvailability {
            self.availability
        }

        fn retrieve_exact(
            &self,
            _plan: &ExactRetrievalPlan,
            context: &OperationContext,
        ) -> Result<Vec<RankingCandidate>, Self::Error> {
            self.record("exact", context);

            match &self.behavior {
                ProviderBehavior::Return(candidates) => Ok(candidates.clone()),
                ProviderBehavior::RankSimilarity => Ok(Vec::new()),
                ProviderBehavior::Fail => Err(FakeProviderError("exact provider failure")),
                ProviderBehavior::HybridPerComponent
                | ProviderBehavior::HybridPerComponentWithDuplicates => Ok(Vec::new()),
            }
        }

        fn retrieve_text(
            &self,
            _plan: &TextRetrievalPlan,
            context: &OperationContext,
        ) -> Result<Vec<RankingCandidate>, Self::Error> {
            self.record("text", context);
            match &self.behavior {
                ProviderBehavior::Return(candidates) => Ok(candidates.clone()),
                ProviderBehavior::Fail => Err(FakeProviderError("text provider failure")),
                _ => Ok(Vec::new()),
            }
        }

        fn retrieve_structured(
            &self,
            _plan: &StructuredRetrievalPlan,
            context: &OperationContext,
        ) -> Result<Vec<RankingCandidate>, Self::Error> {
            self.record("structured", context);
            match &self.behavior {
                ProviderBehavior::Return(candidates) => Ok(candidates.clone()),
                ProviderBehavior::Fail => Err(FakeProviderError("structured provider failure")),
                _ => Ok(Vec::new()),
            }
        }

        fn retrieve_neighborhood(
            &self,
            _plan: &NeighborhoodRetrievalPlan,
            context: &OperationContext,
        ) -> Result<Vec<RankingCandidate>, Self::Error> {
            self.record("neighborhood", context);
            match &self.behavior {
                ProviderBehavior::Return(candidates) => Ok(candidates.clone()),
                ProviderBehavior::Fail => Err(FakeProviderError("neighborhood provider failure")),
                _ => Ok(Vec::new()),
            }
        }

        fn retrieve_similarity(
            &self,
            _plan: &SimilarityRetrievalPlan,
            context: &OperationContext,
        ) -> Result<Vec<RankingCandidate>, Self::Error> {
            self.record("similarity", context);

            match &self.behavior {
                ProviderBehavior::RankSimilarity => {
                    let candidates = vec![
                        Self::candidate("source", "low", Some(0.2), Some(0.8)),
                        Self::candidate("source", "high", Some(0.9), Some(0.1)),
                    ];

                    rank_candidates(
                        &self.capabilities,
                        candidates,
                        &crate::provider::RankingPolicy::score_descending(),
                    )
                    .map_err(|error| match error {
                        ProviderRankingError::Ranking(_) => {
                            FakeProviderError("ranking should succeed")
                        }
                        ProviderRankingError::RankingUnsupported => {
                            FakeProviderError("ranking capability unexpectedly missing")
                        }
                    })
                }
                ProviderBehavior::Return(candidates) => Ok(candidates.clone()),
                ProviderBehavior::Fail => Err(FakeProviderError("similarity provider failure")),
                _ => Ok(Vec::new()),
            }
        }

        fn retrieve_filtered(
            &self,
            _plan: &FilteredRetrievalPlan,
            context: &OperationContext,
        ) -> Result<Vec<RankingCandidate>, Self::Error> {
            self.record("filtered", context);
            match &self.behavior {
                ProviderBehavior::Return(candidates) => Ok(candidates.clone()),
                ProviderBehavior::Fail => Err(FakeProviderError("filtered provider failure")),
                _ => Ok(Vec::new()),
            }
        }

        fn retrieve_hybrid(
            &self,
            _plan: &HybridRetrievalPlan,
            context: &OperationContext,
        ) -> Result<Vec<RankingCandidate>, Self::Error> {
            self.record("hybrid-native", context);
            match &self.behavior {
                ProviderBehavior::Return(candidates) => Ok(candidates.clone()),
                ProviderBehavior::Fail => Err(FakeProviderError("hybrid provider failure")),
                _ => Ok(Vec::new()),
            }
        }

        fn retrieve_hybrid_component(
            &self,
            component: &PlannedHybridComponent,
            context: &OperationContext,
        ) -> Result<Vec<RankingCandidate>, Self::Error> {
            self.record(
                match component.capability() {
                    ProviderCapability::ExactLookup => "hybrid-exact",
                    ProviderCapability::TextLookup => "hybrid-text",
                    ProviderCapability::StructuredLookup => "hybrid-structured",
                    ProviderCapability::NeighborhoodLookup => "hybrid-neighborhood",
                    ProviderCapability::SimilarityLookup => "hybrid-similarity",
                    ProviderCapability::FilteredLookup => "hybrid-filtered",
                    ProviderCapability::HybridRetrieval => "hybrid-native-invalid",
                    ProviderCapability::Ranking => "hybrid-ranking-invalid",
                },
                context,
            );

            match &self.behavior {
                ProviderBehavior::HybridPerComponent => Ok(vec![Self::candidate(
                    "source",
                    component.target().version().id().as_str(),
                    None,
                    None,
                )]),
                ProviderBehavior::HybridPerComponentWithDuplicates => Ok(vec![
                    Self::candidate("source", "shared", Some(0.9), None),
                    Self::candidate(
                        "source",
                        component.target().version().id().as_str(),
                        Some(0.5),
                        None,
                    ),
                ]),
                ProviderBehavior::Fail => {
                    Err(FakeProviderError("hybrid component provider failure"))
                }
                _ => Ok(Vec::new()),
            }
        }
    }

    fn operation_context() -> OperationContext {
        OperationContext::new(nizaam_core::operation::Operation::new(
            nizaam_core::identity::OperationId::new("retrieval-level2-operation")
                .expect("operation ID should be valid"),
            nizaam_core::identity::CorrelationId::new("retrieval-level2-correlation")
                .expect("correlation ID should be valid"),
        ))
    }

    fn definition(family: IndexFamily, definition_value: &str) -> IndexDefinition {
        IndexDefinition::new(
            IndexDefinitionIdentity::new(
                IndexDefinitionId::new(definition_value).expect("definition ID should be valid"),
                IndexNamespace::new("retrieval.level2").expect("namespace should be valid"),
                family,
            ),
            KeyDefinition::new(["value"]).expect("key definition should be valid"),
            TargetReferenceType::new("object").expect("target reference type should be valid"),
            Uniqueness::NonUnique,
            ConsistencyRequirement::new("eventual")
                .expect("consistency requirement should be valid"),
            Some(SourceVersion::new("source-v1").expect("source version should be valid")),
            Some(SchemaVersion::new("schema-v1").expect("schema version should be valid")),
        )
        .expect("definition should be valid")
    }

    fn published_state(version: &str) -> IndexVersionState {
        let mut state = IndexVersionState::new(
            IndexVersion::with_metadata(
                IndexVersionId::new(version).expect("version ID should be valid"),
                Some(SourceVersion::new("source-v1").expect("source version should be valid")),
                Some(SchemaVersion::new("schema-v1").expect("schema version should be valid")),
                None,
            )
            .expect("version should be valid"),
        );

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
        _seed: u8,
        family: IndexFamily,
        definition_value: &str,
        version: &str,
        source_sequence: u64,
        indexed_sequence: u64,
    ) -> IndexCandidate {
        let synchronization = SynchronizationSnapshot::from_sequences(
            crate::build::UpdateSequence::new(source_sequence),
            crate::build::UpdateSequence::new(indexed_sequence),
        )
        .expect("synchronization should be valid");

        IndexCandidate::new(
            IndexDefinitionIdentity::new(
                IndexDefinitionId::new(definition_value).expect("definition ID should be valid"),
                IndexNamespace::new("retrieval.level2").expect("namespace should be valid"),
                family,
            ),
            definition(family, definition_value),
            published_state(version),
            synchronization,
        )
    }

    fn capabilities_for(capabilities: &[ProviderCapability]) -> ProviderCapabilities {
        let mut set = ProviderCapabilities::new();
        for capability in capabilities {
            set.insert(*capability);
        }
        set
    }

    fn plan_exact(seed: u8, version: &str, capabilities: ProviderCapabilities) -> RetrievalPlan {
        let selected = candidate(
            seed,
            IndexFamily::Identity,
            "retrieval.level2.exact",
            version,
            10,
            10,
        );

        let request = QueryRequest::exact(
            selected.definition_identity().clone(),
            KeyMaterial::text("lookup"),
        )
        .expect("exact request should be valid");

        crate::query::planner::plan_query(
            &request,
            vec![selected],
            &capabilities,
            ProviderAvailability::Available,
        )
        .expect("exact plan should succeed")
    }

    fn plan_similarity(seed: u8, version: &str) -> RetrievalPlan {
        let selected = candidate(
            seed,
            IndexFamily::Similarity,
            "retrieval.level2.similarity",
            version,
            20,
            20,
        );

        let request = QueryRequest::with_query_options(
            selected.definition_identity().clone(),
            QueryKind::Similarity {
                representation: KeyMaterial::bytes(vec![1, 2, 3]),
                parameters: None,
            },
            None,
            ConsistencyMode::current(),
            ResultMode::ReferencesWithMetadata,
            None,
        )
        .expect("similarity request should be valid");

        let capabilities = capabilities_for(&[
            ProviderCapability::SimilarityLookup,
            ProviderCapability::Ranking,
        ]);

        crate::query::planner::plan_query(
            &request,
            vec![selected],
            &capabilities,
            ProviderAvailability::Available,
        )
        .expect("similarity plan should succeed")
    }

    #[test]
    fn exact_execution_dispatches_to_provider_and_constructs_consistent_result() {
        let plan = plan_exact(
            1,
            "exact-v1",
            capabilities_for(&[ProviderCapability::ExactLookup]),
        );

        let provider = FakeProvider::new(
            capabilities_for(&[ProviderCapability::ExactLookup]),
            ProviderBehavior::Return(vec![
                FakeProvider::candidate("documents", "doc-b", Some(0.2), None),
                FakeProvider::candidate("documents", "doc-a", Some(0.9), None),
            ]),
        );

        let context = operation_context();
        let result = execute(&plan, &provider, &context).expect("exact retrieval should succeed");

        assert_eq!(provider.calls(), vec!["exact"]);
        assert!(provider.context_was_forwarded(&context));
        assert_eq!(result.index_version().id().as_str(), "exact-v1");
        assert_eq!(result.hits().len(), 2);
        assert_eq!(result.hits()[0].reference().object_reference(), "doc-b");
        assert_eq!(
            result
                .consistency()
                .expect("consistency should be present")
                .state(),
            ConsistencyState::Fresh
        );
        assert_eq!(
            result
                .consistency()
                .expect("consistency should be present")
                .update_sequence_lag(),
            Some(0)
        );
    }

    #[test]
    fn provider_empty_success_is_distinct_from_provider_failure() {
        let plan = plan_exact(
            2,
            "empty-v1",
            capabilities_for(&[ProviderCapability::ExactLookup]),
        );

        let provider = FakeProvider::new(
            capabilities_for(&[ProviderCapability::ExactLookup]),
            ProviderBehavior::Return(Vec::new()),
        );

        let result = execute(&plan, &provider, &operation_context())
            .expect("empty provider output should be a successful query result");

        assert!(result.hits().is_empty());

        let failing_provider = FakeProvider::new(
            capabilities_for(&[ProviderCapability::ExactLookup]),
            ProviderBehavior::Fail,
        );

        let error = execute(&plan, &failing_provider, &operation_context())
            .expect_err("provider failure must remain an error");

        assert!(matches!(error, RetrievalError::Provider(_)));
    }

    #[test]
    fn execution_rechecks_provider_availability() {
        let plan = plan_exact(
            3,
            "availability-v1",
            capabilities_for(&[ProviderCapability::ExactLookup]),
        );

        let provider = FakeProvider::unavailable(
            capabilities_for(&[ProviderCapability::ExactLookup]),
            ProviderBehavior::Return(Vec::new()),
        );

        let error = execute(&plan, &provider, &operation_context())
            .expect_err("temporarily unavailable provider must not be executed");

        assert!(matches!(
            error,
            RetrievalError::ProviderUnavailable {
                availability: ProviderAvailability::TemporarilyUnavailable
            }
        ));
        assert!(provider.calls().is_empty());
    }

    #[test]
    fn execution_rechecks_capabilities_before_physical_provider_call() {
        let plan = plan_exact(
            4,
            "capability-race-v1",
            capabilities_for(&[ProviderCapability::ExactLookup]),
        );

        let provider = FakeProvider::new(
            ProviderCapabilities::new(),
            ProviderBehavior::Return(Vec::new()),
        );

        let error = execute(&plan, &provider, &operation_context())
            .expect_err("provider capability removal must be detected at execution");

        assert!(matches!(
            error,
            RetrievalError::MissingProviderCapability(
                crate::provider::ProviderCapabilityError::Missing {
                    capability: ProviderCapability::ExactLookup
                }
            )
        ));
        assert!(provider.calls().is_empty());
    }

    #[test]
    fn execution_rejects_a_non_published_plan_before_provider_call() {
        let selected = candidate(
            5,
            IndexFamily::Identity,
            "retrieval.level2.invalid",
            "invalid-v1",
            10,
            10,
        );

        let request = QueryRequest::exact(
            selected.definition_identity().clone(),
            KeyMaterial::text("lookup"),
        )
        .expect("exact request should be valid");

        let building = IndexVersionState::new(selected.version().version().clone());
        assert_eq!(building.lifecycle(), VersionLifecycle::Building);

        let invalid_candidate = IndexCandidate::new(
            selected.definition_identity().clone(),
            selected.definition().clone(),
            building,
            selected.synchronization().clone(),
        );

        // The planner cannot create a queryable plan from the unpublished
        // candidate. This test verifies the phase boundary by exercising the
        // planner's refusal rather than fabricating private RetrievalPlan
        // internals.
        let capabilities = capabilities_for(&[ProviderCapability::ExactLookup]);

        let planner_error = crate::query::planner::plan_query(
            &request,
            vec![invalid_candidate],
            &capabilities,
            ProviderAvailability::Available,
        )
        .expect_err("unpublished candidate must fail during planning");

        assert!(matches!(
            planner_error,
            crate::query::planner::QueryPlanningError::ConsistencyUnsatisfied { .. }
        ));
    }

    #[test]
    fn result_limit_is_enforced_at_the_logical_result_boundary() {
        let selected = candidate(
            6,
            IndexFamily::Identity,
            "retrieval.level2.limit",
            "limit-v1",
            10,
            10,
        );

        let request = QueryRequest::with_query_options(
            selected.definition_identity().clone(),
            QueryKind::Exact {
                key: KeyMaterial::text("lookup"),
            },
            Some(NonZeroUsize::new(2).expect("limit should be non-zero")),
            ConsistencyMode::current(),
            ResultMode::ReferencesWithMetadata,
            None,
        )
        .expect("limited request should be valid");

        let capabilities = capabilities_for(&[ProviderCapability::ExactLookup]);

        let plan = crate::query::planner::plan_query(
            &request,
            vec![selected],
            &capabilities,
            ProviderAvailability::Available,
        )
        .expect("limited plan should succeed");

        let provider = FakeProvider::new(
            capabilities,
            ProviderBehavior::Return(vec![
                FakeProvider::candidate("documents", "one", Some(1.0), None),
                FakeProvider::candidate("documents", "two", Some(0.9), None),
                FakeProvider::candidate("documents", "three", Some(0.8), None),
            ]),
        );

        let result = execute(&plan, &provider, &operation_context())
            .expect("limited retrieval should succeed");

        assert_eq!(result.hits().len(), 2);
        assert_eq!(result.hits()[0].reference().object_reference(), "one");
        assert_eq!(result.hits()[1].reference().object_reference(), "two");
        assert_eq!(result.hits()[0].score(), Some(1.0));
        assert_eq!(result.hits()[1].score(), Some(0.9));
    }

    #[test]
    fn text_structured_and_neighborhood_plans_dispatch_to_their_matching_provider_methods() {
        let text_candidate = candidate(
            7,
            IndexFamily::Inverted,
            "retrieval.level2.text",
            "text-v1",
            10,
            10,
        );
        let text_request = QueryRequest::text(
            text_candidate.definition_identity().clone(),
            KeyMaterial::text("rust"),
        )
        .expect("text request should be valid");

        let structured_candidate = candidate(
            8,
            IndexFamily::Identity,
            "retrieval.level2.structured",
            "structured-v1",
            10,
            10,
        );
        let structured_request = QueryRequest::structured(
            structured_candidate.definition_identity().clone(),
            KeyMaterial::map([("field", KeyMaterial::text("value"))])
                .expect("structured material should be valid"),
        )
        .expect("structured request should be valid");

        let anchor = ObjectReference::new("documents", "doc-1").expect("anchor should be valid");
        let neighborhood_candidate = candidate(
            9,
            IndexFamily::Relationship,
            "retrieval.level2.neighborhood",
            "neighborhood-v1",
            10,
            10,
        );
        let neighborhood_request = QueryRequest::neighborhood(
            neighborhood_candidate.definition_identity().clone(),
            anchor,
        )
        .expect("neighborhood request should be valid");

        let provider = FakeProvider::new(
            capabilities_for(&[
                ProviderCapability::TextLookup,
                ProviderCapability::StructuredLookup,
                ProviderCapability::NeighborhoodLookup,
            ]),
            ProviderBehavior::Return(Vec::new()),
        );

        let context = operation_context();

        let text_plan = crate::query::planner::plan_query(
            &text_request,
            vec![text_candidate],
            provider.capabilities(),
            ProviderAvailability::Available,
        )
        .expect("text plan should succeed");

        let structured_plan = crate::query::planner::plan_query(
            &structured_request,
            vec![structured_candidate],
            provider.capabilities(),
            ProviderAvailability::Available,
        )
        .expect("structured plan should succeed");

        let neighborhood_plan = crate::query::planner::plan_query(
            &neighborhood_request,
            vec![neighborhood_candidate],
            provider.capabilities(),
            ProviderAvailability::Available,
        )
        .expect("neighborhood plan should succeed");

        execute(&text_plan, &provider, &context).expect("text retrieval should succeed");
        execute(&structured_plan, &provider, &context)
            .expect("structured retrieval should succeed");
        execute(&neighborhood_plan, &provider, &context)
            .expect("neighborhood retrieval should succeed");

        assert_eq!(provider.calls(), vec!["text", "structured", "neighborhood"]);
        assert_eq!(
            provider
                .context_addresses
                .lock()
                .expect("context log mutex should not be poisoned")
                .len(),
            3
        );
    }

    #[test]
    fn similarity_execution_consumes_provider_side_generic_ranking() {
        let plan = plan_similarity(10, "similarity-v1");

        let provider = FakeProvider::new(
            capabilities_for(&[
                ProviderCapability::SimilarityLookup,
                ProviderCapability::Ranking,
            ]),
            ProviderBehavior::RankSimilarity,
        );

        let result = execute(&plan, &provider, &operation_context())
            .expect("ranked similarity retrieval should succeed");

        let references: Vec<&str> = result
            .hits()
            .iter()
            .map(|hit| hit.reference().object_reference())
            .collect();

        assert_eq!(references, ["high", "low"]);
        assert_eq!(result.hits()[0].score(), Some(0.9));
        assert_eq!(result.hits()[1].score(), Some(0.2));
    }

    #[test]
    fn filtered_execution_preserves_generic_candidate_metadata() {
        let selected = candidate(
            11,
            IndexFamily::Inverted,
            "retrieval.level2.filtered",
            "filtered-v1",
            12,
            12,
        );

        let request = QueryRequest::with_query_options(
            selected.definition_identity().clone(),
            QueryKind::Filtered {
                base: AtomicQuery::Text {
                    query: KeyMaterial::text("rust"),
                    parameters: None,
                },
                filter: KeyMaterial::map([("language", KeyMaterial::text("en"))])
                    .expect("filter material should be valid"),
            },
            None,
            ConsistencyMode::current(),
            ResultMode::ReferencesWithMetadata,
            None,
        )
        .expect("filtered request should be valid");

        let capabilities = capabilities_for(&[ProviderCapability::FilteredLookup]);

        let plan = crate::query::planner::plan_query(
            &request,
            vec![selected],
            &capabilities,
            ProviderAvailability::Available,
        )
        .expect("filtered plan should succeed");

        let candidate = RankingCandidate::new(
            ObjectReference::new("documents", "doc-1").expect("reference should be valid"),
        )
        .with_metrics(Some(0.7), Some(0.3))
        .expect("metrics should be valid")
        .with_metadata(KeyMaterial::text("opaque-provider-metadata"))
        .expect("metadata should be valid");

        let provider = FakeProvider::new(capabilities, ProviderBehavior::Return(vec![candidate]));

        let result = execute(&plan, &provider, &operation_context())
            .expect("filtered retrieval should succeed");

        assert_eq!(result.hits().len(), 1);
        assert_eq!(result.hits()[0].score(), Some(0.7));
        assert_eq!(result.hits()[0].distance(), Some(0.3));
        assert_eq!(
            result.hits()[0].metadata(),
            Some(&KeyMaterial::text("opaque-provider-metadata"))
        );
    }

    #[test]
    fn native_hybrid_dispatches_once_to_the_provider() {
        let candidate = candidate(
            12,
            IndexFamily::Inverted,
            "retrieval.level2.native.hybrid",
            "native-hybrid-v1",
            15,
            15,
        );

        let request = QueryRequest::hybrid(
            candidate.definition_identity().clone(),
            vec![
                HybridQueryComponent::new(AtomicQuery::Text {
                    query: KeyMaterial::text("rust"),
                    parameters: None,
                })
                .expect("text component should be valid"),
                HybridQueryComponent::new(AtomicQuery::Exact {
                    key: KeyMaterial::text("book"),
                })
                .expect("exact component should be valid"),
            ],
        )
        .expect("hybrid request should be valid");

        let capabilities = capabilities_for(&[ProviderCapability::HybridRetrieval]);

        let plan = crate::query::planner::plan_query(
            &request,
            vec![candidate],
            &capabilities,
            ProviderAvailability::Available,
        )
        .expect("native hybrid plan should succeed");

        let provider = FakeProvider::new(
            capabilities,
            ProviderBehavior::Return(vec![
                FakeProvider::candidate("documents", "hybrid-a", Some(0.8), None),
                FakeProvider::candidate("documents", "hybrid-b", Some(0.5), None),
            ]),
        );

        let result = execute(&plan, &provider, &operation_context())
            .expect("native hybrid retrieval should succeed");

        assert_eq!(provider.calls(), vec!["hybrid-native"]);
        assert_eq!(result.hits().len(), 2);
    }

    #[test]
    fn composed_hybrid_executes_components_in_declared_order() {
        let first = candidate(
            13,
            IndexFamily::Inverted,
            "retrieval.level2.composed.first",
            "composed-first",
            16,
            16,
        );
        let second = candidate(
            14,
            IndexFamily::Inverted,
            "retrieval.level2.composed.second",
            "composed-second",
            16,
            16,
        );

        let request = QueryRequest::hybrid(
            first.definition_identity().clone(),
            vec![
                HybridQueryComponent::new(AtomicQuery::Text {
                    query: KeyMaterial::text("rust"),
                    parameters: None,
                })
                .expect("text component should be valid"),
                HybridQueryComponent::with_options(
                    Some(second.definition_identity().clone()),
                    AtomicQuery::Exact {
                        key: KeyMaterial::text("book"),
                    },
                    None,
                )
                .expect("exact component should be valid"),
            ],
        )
        .expect("hybrid request should be valid");

        let capabilities = capabilities_for(&[
            ProviderCapability::TextLookup,
            ProviderCapability::ExactLookup,
        ]);

        let plan = crate::query::planner::plan_query(
            &request,
            vec![first, second],
            &capabilities,
            ProviderAvailability::Available,
        )
        .expect("composed hybrid plan should succeed");

        let provider = FakeProvider::new(capabilities, ProviderBehavior::HybridPerComponent);

        let error = execute(&plan, &provider, &operation_context()).expect_err(
            "different component targets cannot be represented by one legacy QueryResult",
        );

        assert!(matches!(
            error,
            RetrievalError::HybridResultUnsupported(
                RetrievalPlanValidationError::HeterogeneousHybridResultTargets
            )
        ));

        assert!(
            provider.calls().is_empty(),
            "heterogeneous hybrid targets must be rejected before provider execution"
        );
    }

    #[test]
    fn composed_hybrid_deduplicates_repeated_references_before_applying_limit() {
        let selected = candidate(
            15,
            IndexFamily::Inverted,
            "retrieval.level2.hybrid.deduplicate",
            "same-hybrid-v1",
            17,
            17,
        );

        let request = QueryRequest::with_query_options(
            selected.definition_identity().clone(),
            QueryKind::Hybrid {
                components: vec![
                    HybridQueryComponent::new(AtomicQuery::Text {
                        query: KeyMaterial::text("rust"),
                        parameters: None,
                    })
                    .expect("text component should be valid"),
                    HybridQueryComponent::new(AtomicQuery::Exact {
                        key: KeyMaterial::text("book"),
                    })
                    .expect("exact component should be valid"),
                ],
            },
            Some(NonZeroUsize::new(2).expect("limit should be non-zero")),
            ConsistencyMode::current(),
            ResultMode::ReferencesOnly,
            None,
        )
        .expect("limited hybrid request should be valid");

        let capabilities = capabilities_for(&[
            ProviderCapability::TextLookup,
            ProviderCapability::ExactLookup,
        ]);

        let plan = crate::query::planner::plan_query(
            &request,
            vec![selected],
            &capabilities,
            ProviderAvailability::Available,
        )
        .expect("composed hybrid plan should succeed");

        let provider = FakeProvider::new(
            capabilities,
            ProviderBehavior::HybridPerComponentWithDuplicates,
        );

        let result = execute(&plan, &provider, &operation_context())
            .expect("deduplicated composed hybrid should succeed");

        assert_eq!(result.hits().len(), 2);
        assert_eq!(result.hits()[0].reference().object_reference(), "shared");
        assert_eq!(
            result.hits()[1].reference().object_reference(),
            "same-hybrid-v1"
        );
        assert!(result.hits()[0].score().is_none());
        assert!(result.hits()[0].distance().is_none());
        assert!(result.hits()[0].metadata().is_none());
        assert!(result.hits()[1].score().is_none());
        assert!(result.hits()[1].distance().is_none());
        assert!(result.hits()[1].metadata().is_none());
        assert_eq!(provider.calls(), vec!["hybrid-text", "hybrid-exact"]);
    }

    #[test]
    fn hybrid_result_is_constructed_when_all_components_share_one_target_and_version() {
        let selected = candidate(
            15,
            IndexFamily::Inverted,
            "retrieval.level2.hybrid.same",
            "same-hybrid-v1",
            17,
            17,
        );

        let request = QueryRequest::hybrid(
            selected.definition_identity().clone(),
            vec![
                HybridQueryComponent::new(AtomicQuery::Text {
                    query: KeyMaterial::text("rust"),
                    parameters: None,
                })
                .expect("text component should be valid"),
                HybridQueryComponent::new(AtomicQuery::Exact {
                    key: KeyMaterial::text("book"),
                })
                .expect("exact component should be valid"),
            ],
        )
        .expect("hybrid request should be valid");

        let capabilities = capabilities_for(&[
            ProviderCapability::TextLookup,
            ProviderCapability::ExactLookup,
        ]);

        let plan = crate::query::planner::plan_query(
            &request,
            vec![selected],
            &capabilities,
            ProviderAvailability::Available,
        )
        .expect("composed hybrid plan should succeed");

        let provider = FakeProvider::new(capabilities, ProviderBehavior::HybridPerComponent);

        let result = execute(&plan, &provider, &operation_context())
            .expect("same-target composed hybrid should produce a legacy QueryResult");

        assert_eq!(result.index_version().id().as_str(), "same-hybrid-v1");
        assert_eq!(result.hits().len(), 1);
        assert_eq!(
            result.hits()[0].reference().object_reference(),
            "same-hybrid-v1"
        );
        let consistency = result
            .consistency()
            .expect("homogeneous hybrid result should preserve consistency metadata");
        assert_eq!(consistency.state(), ConsistencyState::Fresh);
        assert_eq!(consistency.update_sequence_lag(), Some(0));
        assert_eq!(provider.calls(), vec!["hybrid-text", "hybrid-exact"]);
    }

    #[test]
    fn pinned_execution_preserves_version_pinned_consistency_metadata() {
        let selected = candidate(
            16,
            IndexFamily::Identity,
            "retrieval.level2.pinned",
            "pinned-v7",
            50,
            47,
        );

        let request = QueryRequest::with_query_options(
            selected.definition_identity().clone(),
            QueryKind::Exact {
                key: KeyMaterial::text("lookup"),
            },
            None,
            ConsistencyMode::version_pinned(
                IndexVersionId::new("pinned-v7").expect("version ID should be valid"),
            ),
            ResultMode::ReferencesOnly,
            None,
        )
        .expect("pinned request should be valid");

        let capabilities = capabilities_for(&[ProviderCapability::ExactLookup]);

        let plan = crate::query::planner::plan_query(
            &request,
            vec![selected],
            &capabilities,
            ProviderAvailability::Available,
        )
        .expect("pinned plan should succeed");

        let provider = FakeProvider::new(
            capabilities,
            ProviderBehavior::Return(vec![FakeProvider::candidate(
                "documents",
                "doc-pinned",
                None,
                None,
            )]),
        );

        let result = execute(&plan, &provider, &operation_context())
            .expect("pinned retrieval should succeed");

        let consistency = result
            .consistency()
            .expect("pinned result should contain consistency metadata");
        assert_eq!(consistency.state(), ConsistencyState::VersionPinned);
        assert_eq!(
            consistency
                .mode()
                .pinned_version()
                .expect("pinned mode should expose its version")
                .as_str(),
            "pinned-v7"
        );
        assert_eq!(consistency.update_sequence_lag(), Some(3));
    }

    #[test]
    fn stale_allowed_execution_preserves_combined_freshness_observation() {
        let selected = candidate(
            17,
            IndexFamily::Inverted,
            "retrieval.level2.stale",
            "stale-v1",
            25,
            23,
        );

        let request = QueryRequest::with_query_options(
            selected.definition_identity().clone(),
            QueryKind::Text {
                query: KeyMaterial::text("rust"),
                parameters: None,
            },
            None,
            ConsistencyMode::stale_allowed(
                FreshnessPolicy::new(2)
                    .with_max_time_lag(Duration::from_secs(3))
                    .with_required_source_version(
                        SourceVersion::new("source-v1").expect("source version should be valid"),
                    ),
            ),
            ResultMode::ReferencesWithMetadata,
            None,
        )
        .expect("stale-allowed request should be valid");

        let synchronization = SynchronizationSnapshot::from_sequences(
            crate::build::UpdateSequence::new(25),
            crate::build::UpdateSequence::new(23),
        )
        .expect("sequence observations should be valid")
        .with_time_lag(Duration::from_secs(2))
        .with_source_versions(
            Some(SourceVersion::new("source-v1").expect("source version should be valid")),
            Some(SourceVersion::new("source-v1").expect("source version should be valid")),
        );

        let selected = IndexCandidate::new(
            selected.definition_identity().clone(),
            selected.definition().clone(),
            selected.version().clone(),
            synchronization,
        );

        let capabilities = capabilities_for(&[ProviderCapability::TextLookup]);

        let plan = crate::query::planner::plan_query(
            &request,
            vec![selected],
            &capabilities,
            ProviderAvailability::Available,
        )
        .expect("stale-allowed plan should succeed");

        let provider = FakeProvider::new(
            capabilities,
            ProviderBehavior::Return(vec![FakeProvider::candidate(
                "documents",
                "stale-doc",
                Some(0.4),
                None,
            )]),
        );

        let result = execute(&plan, &provider, &operation_context())
            .expect("stale-allowed retrieval should succeed");

        let consistency = result
            .consistency()
            .expect("consistency metadata should exist");
        assert_eq!(consistency.state(), ConsistencyState::StaleAccepted);
        assert_eq!(consistency.update_sequence_lag(), Some(2));
        assert_eq!(consistency.time_lag(), Some(Duration::from_secs(2)));
        assert_eq!(
            consistency
                .indexed_source_version()
                .expect("indexed source version should exist")
                .as_str(),
            "source-v1"
        );
    }

    #[test]
    fn context_forwarding_does_not_create_a_second_execution_context() {
        let plan = plan_exact(
            18,
            "context-v1",
            capabilities_for(&[ProviderCapability::ExactLookup]),
        );

        let provider = FakeProvider::new(
            capabilities_for(&[ProviderCapability::ExactLookup]),
            ProviderBehavior::Return(Vec::new()),
        );

        let context = operation_context();

        execute(&plan, &provider, &context).expect("retrieval should succeed");

        assert!(provider.context_was_forwarded(&context));
        assert_eq!(provider.calls(), vec!["exact"]);
    }

    #[test]
    fn provider_candidate_validation_error_is_preserved() {
        let selected = candidate(
            19,
            IndexFamily::Identity,
            "retrieval.level2.invalid-candidate",
            "candidate-v1",
            20,
            20,
        );

        let request = QueryRequest::exact(
            selected.definition_identity().clone(),
            KeyMaterial::text("lookup"),
        )
        .expect("exact request should be valid");

        let capabilities = capabilities_for(&[ProviderCapability::ExactLookup]);

        let plan = crate::query::planner::plan_query(
            &request,
            vec![selected],
            &capabilities,
            ProviderAvailability::Available,
        )
        .expect("exact plan should succeed");

        let provider = FakeProvider::new(
            capabilities,
            ProviderBehavior::Return(vec![
                RankingCandidate::new(
                    ObjectReference::new("documents", "bad").expect("reference should be valid"),
                )
                .with_metadata(KeyMaterial::text("metadata"))
                .expect("metadata should be valid"),
            ]),
        );

        let result = execute(&plan, &provider, &operation_context())
            .expect("valid candidate should convert");

        assert_eq!(result.hits().len(), 1);
    }

    #[test]
    fn retrieval_does_not_hydrate_domain_objects() {
        let plan = plan_exact(
            20,
            "reference-only-v1",
            capabilities_for(&[ProviderCapability::ExactLookup]),
        );

        let provider = FakeProvider::new(
            capabilities_for(&[ProviderCapability::ExactLookup]),
            ProviderBehavior::Return(vec![FakeProvider::candidate(
                "quran",
                "verse:2:255",
                None,
                None,
            )]),
        );

        let result = execute(&plan, &provider, &operation_context())
            .expect("reference retrieval should succeed");

        assert_eq!(result.hits().len(), 1);
        assert_eq!(result.hits()[0].reference().source(), "quran");
        assert_eq!(
            result.hits()[0].reference().object_reference(),
            "verse:2:255"
        );
    }
}
