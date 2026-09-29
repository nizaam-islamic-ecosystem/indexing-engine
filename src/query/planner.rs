//! Logical query planning for the Phase 4 Indexing Engine.
//!
//! This module is the boundary between a validated logical [`QueryRequest`]
//! and a provider-ready **logical** [`RetrievalPlan`]. It resolves a request
//! against caller-supplied logical index/version candidates, verifies the
//! requested consistency policy, and negotiates the logical provider
//! capabilities required by the query.
//!
//! The planner deliberately does not:
//!
//! - choose a physical database or storage backend;
//! - expose a physical index algorithm or execution plan;
//! - perform retrieval;
//! - calculate ranking values;
//! - interpret domain semantics or relationship predicates;
//! - own version publication or an active-version registry;
//! - use Core routing as a local query planner;
//! - create a second cancellation/deadline/runtime system.
//!
//! The intended dependency direction is:
//!
//! ```text
//! QueryRequest
//!      ↓
//! logical candidate selection
//!      ↓
//! consistency evaluation
//!      ↓
//! provider capability negotiation
//!      ↓
//! RetrievalPlan
//!      ↓
//! query/retrieval.rs
//! ```
//!
//! `IndexCandidate` is the planner input boundary. The publication/lifecycle
//! owner supplies the queryable version candidates and the synchronization
//! observations; this module consumes those facts without creating another
//! publication or active-version mechanism.

use core::fmt;
use core::num::NonZeroUsize;

use super::request::{AtomicQuery, HybridQueryComponent, QueryKind, QueryRequest, ResultMode};
use crate::consistency::policy::{ConsistencyEvaluation, ConsistencyMode, ConsistencyPolicyError};
use crate::consistency::synchronization::SynchronizationSnapshot;
use crate::identity::{IndexDefinitionId, IndexDefinitionIdentity, IndexNamespace};
use crate::index::{
    IndexDefinition, IndexFamily, IndexVersion, IndexVersionState, KeyMaterial, ObjectReference,
    VersionLifecycle,
};
use crate::provider::{ProviderAvailability, ProviderCapabilities, ProviderCapability};

/// A logical index/version candidate supplied to the query planner.
///
/// The candidate associates a logical [`IndexDefinitionIdentity`] with one [`IndexDefinition`],
/// one [`IndexVersionState`], and the query-time synchronization observations
/// needed by the consistency subsystem.
///
/// The planner never treats the candidate as a physical provider handle. The
/// physical storage implementation remains behind the shared provider
/// abstraction.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IndexCandidate {
    definition_identity: IndexDefinitionIdentity,
    definition: IndexDefinition,
    version: IndexVersionState,
    synchronization: SynchronizationSnapshot,
}

impl IndexCandidate {
    /// Creates a planner candidate from already-constructed logical contracts.
    #[must_use]
    pub fn new(
        definition_identity: IndexDefinitionIdentity,
        definition: IndexDefinition,
        version: IndexVersionState,
        synchronization: SynchronizationSnapshot,
    ) -> Self {
        Self {
            definition_identity,
            definition,
            version,
            synchronization,
        }
    }

    /// Returns the logical index identity associated with this candidate.
    #[must_use]
    pub const fn definition_identity(&self) -> &IndexDefinitionIdentity {
        &self.definition_identity
    }

    /// Returns the logical index definition.
    #[must_use]
    pub fn definition(&self) -> &IndexDefinition {
        &self.definition
    }

    /// Returns the logical version/lifecycle state.
    #[must_use]
    pub fn version(&self) -> &IndexVersionState {
        &self.version
    }

    /// Returns the query-time synchronization observations.
    #[must_use]
    pub fn synchronization(&self) -> &SynchronizationSnapshot {
        &self.synchronization
    }

    /// Validates all logical structures carried by the candidate.
    pub fn validate(&self) -> Result<(), CandidateValidationError> {
        self.definition
            .validate()
            .map_err(CandidateValidationError::InvalidDefinition)?;

        self.version
            .version()
            .validate()
            .map_err(CandidateValidationError::InvalidVersion)?;

        let definition_identity = IndexDefinitionIdentity::new(
            self.definition.definition_id().clone(),
            self.definition.namespace().clone(),
            self.definition.family(),
        );
        if self.definition_identity != definition_identity {
            return Err(CandidateValidationError::DefinitionIdentityMismatch {
                supplied: self.definition_identity.clone(),
                definition: definition_identity,
            });
        }

        self.synchronization
            .validate()
            .map_err(CandidateValidationError::InvalidSynchronization)?;

        Ok(())
    }

    /// Consumes the candidate and returns its logical components.
    #[must_use]
    pub fn into_parts(
        self,
    ) -> (
        IndexDefinitionIdentity,
        IndexDefinition,
        IndexVersionState,
        SynchronizationSnapshot,
    ) {
        (
            self.definition_identity,
            self.definition,
            self.version,
            self.synchronization,
        )
    }
}

/// Structural validation failures for [`IndexCandidate`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CandidateValidationError {
    /// The logical index definition is invalid.
    InvalidDefinition(crate::index::IndexDefinitionValidationError),

    /// The logical index version is invalid.
    InvalidVersion(crate::index::IndexVersionValidationError),

    /// The supplied definition identity does not match the identity components
    /// carried by the candidate definition.
    DefinitionIdentityMismatch {
        supplied: IndexDefinitionIdentity,
        definition: IndexDefinitionIdentity,
    },

    /// The synchronization observation is invalid.
    InvalidSynchronization(crate::consistency::synchronization::SynchronizationError),
}

impl fmt::Display for CandidateValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidDefinition(error) => {
                write!(formatter, "invalid index definition: {error}")
            }
            Self::InvalidVersion(error) => write!(formatter, "invalid index version: {error}"),
            Self::DefinitionIdentityMismatch {
                supplied,
                definition,
            } => write!(
                formatter,
                "candidate definition identity {supplied:?} does not match definition identity {definition:?}"
            ),
            Self::InvalidSynchronization(error) => {
                write!(formatter, "invalid synchronization observation: {error}")
            }
        }
    }
}

impl std::error::Error for CandidateValidationError {}

/// A logical provider-capability requirement derived from one query shape.
///
/// `alternatives` represents capability sets that can satisfy the same logical
/// request. Most query kinds have one direct requirement. A hybrid request has
/// two alternatives:
///
/// 1. a provider-native [`ProviderCapability::HybridRetrieval`] capability;
/// 2. the set of capabilities needed to execute its individual components as
///    a composed logical retrieval.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QueryCapabilityRequirement {
    alternatives: Vec<Vec<ProviderCapability>>,
}

impl QueryCapabilityRequirement {
    /// Creates a capability requirement from explicit alternatives.
    ///
    /// Empty alternatives are rejected because such a requirement could never
    /// describe an executable query. Individual alternatives must also contain
    /// at least one capability.
    pub fn new(
        alternatives: Vec<Vec<ProviderCapability>>,
    ) -> Result<Self, QueryCapabilityRequirementError> {
        if alternatives.is_empty() {
            return Err(QueryCapabilityRequirementError::NoAlternatives);
        }

        if alternatives.iter().any(Vec::is_empty) {
            return Err(QueryCapabilityRequirementError::EmptyAlternative);
        }

        Ok(Self { alternatives })
    }

    /// Derives the logical provider capabilities required by a query kind.
    pub fn for_query(query: &QueryKind) -> Result<Self, QueryCapabilityRequirementError> {
        match query {
            QueryKind::Exact { .. } => Self::new(vec![vec![ProviderCapability::ExactLookup]]),
            QueryKind::Text { .. } => Self::new(vec![vec![ProviderCapability::TextLookup]]),
            QueryKind::Structured { .. } => {
                Self::new(vec![vec![ProviderCapability::StructuredLookup]])
            }
            QueryKind::Neighborhood { .. } => {
                Self::new(vec![vec![ProviderCapability::NeighborhoodLookup]])
            }
            QueryKind::Similarity { .. } => Self::new(vec![vec![
                ProviderCapability::SimilarityLookup,
                ProviderCapability::Ranking,
            ]]),
            QueryKind::Filtered { base, .. } => {
                let mut required = vec![ProviderCapability::FilteredLookup];
                if atomic_query_requires_ranking(base) {
                    required.push(ProviderCapability::Ranking);
                }
                Self::new(vec![required])
            }
            QueryKind::Hybrid { components } => {
                let mut composed = Vec::with_capacity(components.len() + 1);

                for component in components {
                    composed.push(capability_for_hybrid_component(component));
                }

                if components.iter().any(hybrid_component_requires_ranking) {
                    composed.push(ProviderCapability::Ranking);
                }

                Self::new(vec![vec![ProviderCapability::HybridRetrieval], composed])
            }
        }
    }

    /// Returns all logical capability alternatives in declaration order.
    #[must_use]
    pub fn alternatives(&self) -> &[Vec<ProviderCapability>] {
        &self.alternatives
    }

    /// Returns whether at least one capability alternative is advertised.
    #[must_use]
    pub fn is_satisfied_by(&self, capabilities: &ProviderCapabilities) -> bool {
        self.alternatives
            .iter()
            .any(|alternative| capabilities.supports_all(alternative.iter().copied()))
    }

    /// Resolves the first supported logical capability alternative.
    ///
    /// The first supported alternative is preferred because alternatives are
    /// declared from the most direct execution boundary to the fallback
    /// composition boundary. This ordering is about execution structure, not
    /// domain relevance or physical-provider preference.
    pub fn resolve(&self, capabilities: &ProviderCapabilities) -> Option<CapabilityResolution> {
        self.alternatives.iter().find_map(|alternative| {
            if !capabilities.supports_all(alternative.iter().copied()) {
                return None;
            }

            if alternative.len() == 1 && alternative[0] == ProviderCapability::HybridRetrieval {
                Some(CapabilityResolution::Direct(
                    ProviderCapability::HybridRetrieval,
                ))
            } else if alternative.len() == 1 {
                Some(CapabilityResolution::Direct(alternative[0]))
            } else {
                Some(CapabilityResolution::Required(alternative.clone()))
            }
        })
    }
}

/// Failures while constructing a [`QueryCapabilityRequirement`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QueryCapabilityRequirementError {
    /// No executable capability alternative was supplied.
    NoAlternatives,

    /// One capability alternative was empty.
    EmptyAlternative,
}

impl fmt::Display for QueryCapabilityRequirementError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoAlternatives => {
                formatter.write_str("query capability requirement has no alternatives")
            }
            Self::EmptyAlternative => {
                formatter.write_str("query capability requirement contains an empty alternative")
            }
        }
    }
}

impl std::error::Error for QueryCapabilityRequirementError {}

/// Logical capability resolution selected by the planner.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CapabilityResolution {
    /// One provider capability can execute the logical operation directly.
    Direct(ProviderCapability),

    /// The logical operation requires multiple advertised provider
    /// capabilities. Multiple requirements do not imply multiple physical
    /// providers or physical operators; they are only logical capability
    /// prerequisites.
    Required(Vec<ProviderCapability>),
}

impl CapabilityResolution {
    /// Returns all capabilities selected for this plan.
    #[must_use]
    pub fn capabilities(&self) -> &[ProviderCapability] {
        match self {
            Self::Direct(capability) => core::slice::from_ref(capability),
            Self::Required(capabilities) => capabilities,
        }
    }
}

/// One resolved logical index/version target inside a retrieval plan.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlannedIndex {
    definition_identity: IndexDefinitionIdentity,
    definition: IndexDefinition,
    version: IndexVersion,
    lifecycle: VersionLifecycle,
    consistency: ConsistencyEvaluation,
}

impl PlannedIndex {
    fn from_candidate(candidate: &IndexCandidate, consistency: ConsistencyEvaluation) -> Self {
        Self {
            definition_identity: candidate.definition_identity().clone(),
            definition: candidate.definition.clone(),
            version: candidate.version.version().clone(),
            lifecycle: candidate.version.lifecycle(),
            consistency,
        }
    }

    /// Returns the logical index identity selected for retrieval.
    #[must_use]
    pub const fn definition_identity(&self) -> &IndexDefinitionIdentity {
        &self.definition_identity
    }

    /// Returns the selected logical index definition.
    #[must_use]
    pub fn definition(&self) -> &IndexDefinition {
        &self.definition
    }

    /// Returns the selected logical index version.
    #[must_use]
    pub fn version(&self) -> &IndexVersion {
        &self.version
    }

    /// Returns the lifecycle state observed during planning.
    #[must_use]
    pub const fn lifecycle(&self) -> VersionLifecycle {
        self.lifecycle
    }

    /// Returns the successful query-time consistency evaluation.
    #[must_use]
    pub fn consistency(&self) -> &ConsistencyEvaluation {
        &self.consistency
    }
}

/// Common logical plan metadata for a single-index retrieval operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetrievalPlanContext {
    target: PlannedIndex,
    limit: Option<NonZeroUsize>,
    result_mode: ResultMode,
    metadata: Option<KeyMaterial>,
    capability_resolution: CapabilityResolution,
}

impl RetrievalPlanContext {
    fn new(
        target: PlannedIndex,
        limit: Option<NonZeroUsize>,
        result_mode: ResultMode,
        metadata: Option<KeyMaterial>,
        capability_resolution: CapabilityResolution,
    ) -> Self {
        Self {
            target,
            limit,
            result_mode,
            metadata,
            capability_resolution,
        }
    }

    /// Returns the selected index/version target.
    #[must_use]
    pub fn target(&self) -> &PlannedIndex {
        &self.target
    }

    /// Returns the logical result-count limit.
    #[must_use]
    pub const fn limit(&self) -> Option<NonZeroUsize> {
        self.limit
    }

    /// Returns the requested result representation.
    #[must_use]
    pub const fn result_mode(&self) -> ResultMode {
        self.result_mode
    }

    /// Returns request-local opaque metadata.
    #[must_use]
    pub fn metadata(&self) -> Option<&KeyMaterial> {
        self.metadata.as_ref()
    }

    /// Returns the logical provider capability resolution selected for the
    /// operation.
    #[must_use]
    pub fn capability_resolution(&self) -> &CapabilityResolution {
        &self.capability_resolution
    }
}

/// Logical plan for exact retrieval.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExactRetrievalPlan {
    context: RetrievalPlanContext,
    key: KeyMaterial,
}

impl ExactRetrievalPlan {
    /// Returns common plan metadata.
    #[must_use]
    pub fn context(&self) -> &RetrievalPlanContext {
        &self.context
    }

    /// Returns the exact logical key material.
    #[must_use]
    pub fn key(&self) -> &KeyMaterial {
        &self.key
    }
}

/// Logical plan for text/inverted retrieval.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TextRetrievalPlan {
    context: RetrievalPlanContext,
    query: KeyMaterial,
    parameters: Option<KeyMaterial>,
}

impl TextRetrievalPlan {
    /// Returns common plan metadata.
    #[must_use]
    pub fn context(&self) -> &RetrievalPlanContext {
        &self.context
    }

    /// Returns logical text query material.
    #[must_use]
    pub fn query(&self) -> &KeyMaterial {
        &self.query
    }

    /// Returns optional logical text parameters.
    #[must_use]
    pub fn parameters(&self) -> Option<&KeyMaterial> {
        self.parameters.as_ref()
    }
}

/// Logical plan for structured retrieval.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StructuredRetrievalPlan {
    context: RetrievalPlanContext,
    fields: KeyMaterial,
}

impl StructuredRetrievalPlan {
    /// Returns common plan metadata.
    #[must_use]
    pub fn context(&self) -> &RetrievalPlanContext {
        &self.context
    }

    /// Returns structured logical query fields.
    #[must_use]
    pub fn fields(&self) -> &KeyMaterial {
        &self.fields
    }
}

/// Logical plan for neighborhood retrieval.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NeighborhoodRetrievalPlan {
    context: RetrievalPlanContext,
    anchor: ObjectReference,
    key: Option<KeyMaterial>,
}

impl NeighborhoodRetrievalPlan {
    /// Returns common plan metadata.
    #[must_use]
    pub fn context(&self) -> &RetrievalPlanContext {
        &self.context
    }

    /// Returns the opaque source-owned neighborhood anchor.
    #[must_use]
    pub fn anchor(&self) -> &ObjectReference {
        &self.anchor
    }

    /// Returns optional logical key restrictions.
    #[must_use]
    pub fn key(&self) -> Option<&KeyMaterial> {
        self.key.as_ref()
    }
}

/// Logical plan for similarity retrieval.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SimilarityRetrievalPlan {
    context: RetrievalPlanContext,
    representation: KeyMaterial,
    parameters: Option<KeyMaterial>,
}

impl SimilarityRetrievalPlan {
    /// Returns common plan metadata.
    #[must_use]
    pub fn context(&self) -> &RetrievalPlanContext {
        &self.context
    }

    /// Returns the generic logical similarity representation.
    #[must_use]
    pub fn representation(&self) -> &KeyMaterial {
        &self.representation
    }

    /// Returns optional logical similarity parameters.
    #[must_use]
    pub fn parameters(&self) -> Option<&KeyMaterial> {
        self.parameters.as_ref()
    }
}

/// Logical plan for filtered retrieval.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FilteredRetrievalPlan {
    context: RetrievalPlanContext,
    base: AtomicQuery,
    filter: KeyMaterial,
}

impl FilteredRetrievalPlan {
    /// Returns common plan metadata.
    #[must_use]
    pub fn context(&self) -> &RetrievalPlanContext {
        &self.context
    }

    /// Returns the base atomic logical query.
    #[must_use]
    pub fn base(&self) -> &AtomicQuery {
        &self.base
    }

    /// Returns the generic logical filter material.
    #[must_use]
    pub fn filter(&self) -> &KeyMaterial {
        &self.filter
    }
}

/// One resolved component of a hybrid logical retrieval plan.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlannedHybridComponent {
    target: PlannedIndex,
    query: AtomicQuery,
    filter: Option<KeyMaterial>,
    capability: ProviderCapability,
}

impl PlannedHybridComponent {
    fn new(
        target: PlannedIndex,
        query: AtomicQuery,
        filter: Option<KeyMaterial>,
        capability: ProviderCapability,
    ) -> Self {
        Self {
            target,
            query,
            filter,
            capability,
        }
    }

    /// Returns the selected logical index/version target.
    #[must_use]
    pub fn target(&self) -> &PlannedIndex {
        &self.target
    }

    /// Returns the atomic query component.
    #[must_use]
    pub fn query(&self) -> &AtomicQuery {
        &self.query
    }

    /// Returns the optional component filter.
    #[must_use]
    pub fn filter(&self) -> Option<&KeyMaterial> {
        self.filter.as_ref()
    }

    /// Returns the logical provider capability assigned to this component when
    /// hybrid execution is composed instead of handled by one provider-native
    /// hybrid capability.
    #[must_use]
    pub const fn capability(&self) -> ProviderCapability {
        self.capability
    }
}

/// Logical plan for hybrid retrieval.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HybridRetrievalPlan {
    components: Vec<PlannedHybridComponent>,
    limit: Option<NonZeroUsize>,
    result_mode: ResultMode,
    metadata: Option<KeyMaterial>,
    capability_resolution: CapabilityResolution,
}

impl HybridRetrievalPlan {
    /// Returns all resolved hybrid components in request order.
    #[must_use]
    pub fn components(&self) -> &[PlannedHybridComponent] {
        &self.components
    }

    /// Returns the logical result-count limit.
    #[must_use]
    pub const fn limit(&self) -> Option<NonZeroUsize> {
        self.limit
    }

    /// Returns the requested result representation.
    #[must_use]
    pub const fn result_mode(&self) -> ResultMode {
        self.result_mode
    }

    /// Returns request-local opaque metadata.
    #[must_use]
    pub fn metadata(&self) -> Option<&KeyMaterial> {
        self.metadata.as_ref()
    }

    /// Returns how the provider capabilities satisfy the hybrid operation.
    #[must_use]
    pub fn capability_resolution(&self) -> &CapabilityResolution {
        &self.capability_resolution
    }
}

/// Complete logical retrieval plan produced by the planner.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RetrievalPlan {
    /// Exact logical lookup.
    Exact(ExactRetrievalPlan),

    /// Text/inverted logical lookup.
    Text(TextRetrievalPlan),

    /// Structured logical lookup.
    Structured(StructuredRetrievalPlan),

    /// Neighborhood logical lookup.
    Neighborhood(NeighborhoodRetrievalPlan),

    /// Similarity logical lookup.
    Similarity(SimilarityRetrievalPlan),

    /// Filtered logical lookup.
    Filtered(FilteredRetrievalPlan),

    /// Hybrid logical lookup.
    Hybrid(HybridRetrievalPlan),
}

impl RetrievalPlan {
    /// Returns the primary selected target for single-index plans.
    ///
    /// Hybrid plans may contain multiple logical targets and therefore return
    /// `None` here; inspect [`HybridRetrievalPlan::components`] instead.
    #[must_use]
    pub fn target(&self) -> Option<&PlannedIndex> {
        match self {
            Self::Exact(plan) => Some(plan.context().target()),
            Self::Text(plan) => Some(plan.context().target()),
            Self::Structured(plan) => Some(plan.context().target()),
            Self::Neighborhood(plan) => Some(plan.context().target()),
            Self::Similarity(plan) => Some(plan.context().target()),
            Self::Filtered(plan) => Some(plan.context().target()),
            Self::Hybrid(_) => None,
        }
    }

    /// Returns the consistency mode represented by a single-index plan.
    ///
    /// Hybrid plans can contain multiple evaluations, so use their component
    /// targets to inspect each consistency decision.
    #[must_use]
    pub fn consistency(&self) -> Option<&ConsistencyEvaluation> {
        self.target().map(PlannedIndex::consistency)
    }

    /// Returns the selected provider capability resolution for a single-index
    /// plan. Hybrid plans expose it directly through their variant.
    #[must_use]
    pub fn capability_resolution(&self) -> &CapabilityResolution {
        match self {
            Self::Exact(plan) => plan.context().capability_resolution(),
            Self::Text(plan) => plan.context().capability_resolution(),
            Self::Structured(plan) => plan.context().capability_resolution(),
            Self::Neighborhood(plan) => plan.context().capability_resolution(),
            Self::Similarity(plan) => plan.context().capability_resolution(),
            Self::Filtered(plan) => plan.context().capability_resolution(),
            Self::Hybrid(plan) => plan.capability_resolution(),
        }
    }

    /// Returns the logical query limit, when present.
    #[must_use]
    pub fn limit(&self) -> Option<NonZeroUsize> {
        match self {
            Self::Exact(plan) => plan.context().limit(),
            Self::Text(plan) => plan.context().limit(),
            Self::Structured(plan) => plan.context().limit(),
            Self::Neighborhood(plan) => plan.context().limit(),
            Self::Similarity(plan) => plan.context().limit(),
            Self::Filtered(plan) => plan.context().limit(),
            Self::Hybrid(plan) => plan.limit(),
        }
    }

    /// Returns the requested logical result representation.
    #[must_use]
    pub fn result_mode(&self) -> ResultMode {
        match self {
            Self::Exact(plan) => plan.context().result_mode(),
            Self::Text(plan) => plan.context().result_mode(),
            Self::Structured(plan) => plan.context().result_mode(),
            Self::Neighborhood(plan) => plan.context().result_mode(),
            Self::Similarity(plan) => plan.context().result_mode(),
            Self::Filtered(plan) => plan.context().result_mode(),
            Self::Hybrid(plan) => plan.result_mode(),
        }
    }
}

/// Logical query-planning failures.
#[derive(Clone, Debug, PartialEq)]
pub enum QueryPlanningError {
    /// The request failed its structural validation.
    InvalidRequest(crate::query::request::QueryRequestValidationError),

    /// A planner candidate failed structural validation.
    InvalidCandidate {
        /// Position in the supplied candidate collection.
        position: usize,
        /// Candidate validation failure.
        error: CandidateValidationError,
    },

    /// No candidate exists for the requested logical index identifier.
    NoDefinitionCandidate {
        definition_identity: IndexDefinitionIdentity,
    },

    /// Candidates exist for the requested index, but the optional logical
    /// namespace/definition/family selectors do not match any of them.
    SelectionHintsMismatch {
        definition_identity: IndexDefinitionIdentity,
        namespace: Option<IndexNamespace>,
        definition_id: Option<IndexDefinitionId>,
        family: Option<IndexFamily>,
    },

    /// Provider execution is unavailable at the time of planning.
    ProviderUnavailable { availability: ProviderAvailability },

    /// No advertised provider capability alternative can satisfy the request.
    MissingProviderCapabilities {
        requirement: QueryCapabilityRequirement,
    },

    /// Consistency evaluation failed for every otherwise-eligible candidate.
    ConsistencyUnsatisfied {
        definition_identity: IndexDefinitionIdentity,
        last_error: ConsistencyPolicyError,
    },

    /// More than one candidate represented the exact same pinned logical
    /// version for one target index.
    AmbiguousPinnedVersion {
        definition_identity: IndexDefinitionIdentity,
        version_id: crate::index::IndexVersionId,
    },

    /// More than one queryable candidate satisfied `Current`. Because the
    /// planner does not own an active-version registry, it cannot safely infer
    /// which zero-lag published candidate is the active one.
    AmbiguousCurrentVersion {
        definition_identity: IndexDefinitionIdentity,
        candidate_count: usize,
    },

    /// The planner could not construct a capability requirement from the
    /// logical query shape. This is an internal contract error because a
    /// structurally valid `QueryKind` should always map to a non-empty
    /// capability requirement.
    CapabilityRequirement(QueryCapabilityRequirementError),

    /// A hybrid component could not be mapped to a supplied logical target.
    InvalidHybridComponent {
        position: usize,
        source: Box<QueryPlanningError>,
    },

    /// A hybrid request contained a component target that does not exist.
    HybridTargetMissing {
        position: usize,
        definition_identity: IndexDefinitionIdentity,
    },
}

impl fmt::Display for QueryPlanningError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRequest(error) => write!(formatter, "invalid query request: {error}"),
            Self::InvalidCandidate { position, error } => {
                write!(
                    formatter,
                    "invalid planner candidate at position {position}: {error}"
                )
            }
            Self::NoDefinitionCandidate {
                definition_identity,
            } => {
                write!(
                    formatter,
                    "no planner candidate exists for logical index definition {definition_identity:?}"
                )
            }
            Self::SelectionHintsMismatch {
                definition_identity,
                namespace,
                definition_id,
                family,
            } => write!(
                formatter,
                "logical selection hints do not match index definition {definition_identity:?}: namespace={namespace:?}, definition_id={definition_id:?}, family={family:?}"
            ),
            Self::ProviderUnavailable { availability } => {
                write!(formatter, "provider is {availability}")
            }
            Self::MissingProviderCapabilities { requirement } => {
                write!(
                    formatter,
                    "provider capabilities cannot satisfy {requirement:?}"
                )
            }
            Self::ConsistencyUnsatisfied {
                definition_identity,
                last_error,
            } => {
                write!(
                    formatter,
                    "consistency requirement cannot be satisfied for index definition {definition_identity:?}: {last_error}"
                )
            }
            Self::AmbiguousPinnedVersion {
                definition_identity,
                version_id,
            } => write!(
                formatter,
                "multiple candidates expose the pinned version {version_id:?} for index definition {definition_identity:?}"
            ),
            Self::AmbiguousCurrentVersion {
                definition_identity,
                candidate_count,
            } => write!(
                formatter,
                "multiple queryable candidates ({candidate_count}) satisfy current consistency for index definition {definition_identity:?}; the planner does not own active-version state"
            ),
            Self::CapabilityRequirement(error) => {
                write!(formatter, "invalid query capability requirement: {error}")
            }
            Self::InvalidHybridComponent { position, source } => {
                write!(
                    formatter,
                    "invalid hybrid component at position {position}: {source}"
                )
            }
            Self::HybridTargetMissing {
                position,
                definition_identity,
            } => {
                write!(
                    formatter,
                    "hybrid component {position} targets missing index definition {definition_identity:?}"
                )
            }
        }
    }
}

impl std::error::Error for QueryPlanningError {}

impl From<QueryCapabilityRequirementError> for QueryPlanningError {
    fn from(error: QueryCapabilityRequirementError) -> Self {
        Self::CapabilityRequirement(error)
    }
}

/// Plans a logical Indexing query against the supplied candidate versions and
/// provider capability advertisement.
///
/// This is the primary Phase 4 planner entry point. Candidate versions are
/// supplied by the caller because Phase 3 deliberately does not own a global
/// active-version registry. The planner only chooses among the candidates
/// visible through this boundary.
pub fn plan_query<I>(
    request: &QueryRequest,
    candidates: I,
    capabilities: &ProviderCapabilities,
    availability: ProviderAvailability,
) -> Result<RetrievalPlan, QueryPlanningError>
where
    I: IntoIterator<Item = IndexCandidate>,
{
    request
        .validate()
        .map_err(QueryPlanningError::InvalidRequest)?;

    if availability != ProviderAvailability::Available {
        return Err(QueryPlanningError::ProviderUnavailable { availability });
    }

    let candidates: Vec<IndexCandidate> = candidates.into_iter().collect();
    for (position, candidate) in candidates.iter().enumerate() {
        candidate
            .validate()
            .map_err(|error| QueryPlanningError::InvalidCandidate { position, error })?;
    }

    let requirement = QueryCapabilityRequirement::for_query(request.query_kind())
        .map_err(QueryPlanningError::from)?;

    let resolution = requirement.resolve(capabilities).ok_or_else(|| {
        QueryPlanningError::MissingProviderCapabilities {
            requirement: requirement.clone(),
        }
    })?;

    match request.query_kind() {
        QueryKind::Hybrid { components } => {
            plan_hybrid(request, components, &candidates, resolution)
        }
        query => {
            let target = select_candidate(
                request.definition_identity(),
                request.namespace(),
                request.definition_id(),
                request.family(),
                request.consistency(),
                &candidates,
            )?;

            build_single_plan(request, query, target, resolution)
        }
    }
}

fn build_single_plan(
    request: &QueryRequest,
    query: &QueryKind,
    candidate: &IndexCandidate,
    capability_resolution: CapabilityResolution,
) -> Result<RetrievalPlan, QueryPlanningError> {
    let consistency = evaluate_candidate(candidate, request.consistency()).map_err(|error| {
        QueryPlanningError::ConsistencyUnsatisfied {
            definition_identity: candidate.definition_identity().clone(),
            last_error: error,
        }
    })?;

    let target = PlannedIndex::from_candidate(candidate, consistency);
    let context = RetrievalPlanContext::new(
        target,
        request.limit(),
        request.result_mode(),
        request.metadata().cloned(),
        capability_resolution,
    );

    let plan = match query {
        QueryKind::Exact { key } => RetrievalPlan::Exact(ExactRetrievalPlan {
            context,
            key: key.clone(),
        }),
        QueryKind::Text { query, parameters } => RetrievalPlan::Text(TextRetrievalPlan {
            context,
            query: query.clone(),
            parameters: parameters.clone(),
        }),
        QueryKind::Structured { fields } => RetrievalPlan::Structured(StructuredRetrievalPlan {
            context,
            fields: fields.clone(),
        }),
        QueryKind::Neighborhood { anchor, key } => {
            RetrievalPlan::Neighborhood(NeighborhoodRetrievalPlan {
                context,
                anchor: anchor.clone(),
                key: key.clone(),
            })
        }
        QueryKind::Similarity {
            representation,
            parameters,
        } => RetrievalPlan::Similarity(SimilarityRetrievalPlan {
            context,
            representation: representation.clone(),
            parameters: parameters.clone(),
        }),
        QueryKind::Filtered { base, filter } => RetrievalPlan::Filtered(FilteredRetrievalPlan {
            context,
            base: base.clone(),
            filter: filter.clone(),
        }),
        QueryKind::Hybrid { .. } => unreachable!("hybrid queries are planned by plan_hybrid"),
    };

    Ok(plan)
}

fn plan_hybrid(
    request: &QueryRequest,
    components: &[HybridQueryComponent],
    candidates: &[IndexCandidate],
    capability_resolution: CapabilityResolution,
) -> Result<RetrievalPlan, QueryPlanningError> {
    let mut planned = Vec::with_capacity(components.len());

    for (position, component) in components.iter().enumerate() {
        let target_definition_identity = component
            .target_definition_identity()
            .cloned()
            .unwrap_or_else(|| request.definition_identity().clone());

        let candidate = find_candidate_for_hybrid_component(
            position,
            target_definition_identity,
            request,
            component.target_definition_identity().is_none(),
            candidates,
        )?;

        let consistency =
            evaluate_candidate(candidate, request.consistency()).map_err(|error| {
                QueryPlanningError::InvalidHybridComponent {
                    position,
                    source: Box::new(QueryPlanningError::ConsistencyUnsatisfied {
                        definition_identity: candidate.definition_identity().clone(),
                        last_error: error,
                    }),
                }
            })?;

        let target = PlannedIndex::from_candidate(candidate, consistency);
        let capability = capability_for_hybrid_component(component);

        planned.push(PlannedHybridComponent::new(
            target,
            component.query().clone(),
            component.filter().cloned(),
            capability,
        ));
    }

    Ok(RetrievalPlan::Hybrid(HybridRetrievalPlan {
        components: planned,
        limit: request.limit(),
        result_mode: request.result_mode(),
        metadata: request.metadata().cloned(),
        capability_resolution,
    }))
}

fn find_candidate_for_hybrid_component<'a>(
    position: usize,
    target_definition_identity: IndexDefinitionIdentity,
    request: &QueryRequest,
    uses_parent_target: bool,
    candidates: &'a [IndexCandidate],
) -> Result<&'a IndexCandidate, QueryPlanningError> {
    let target_candidates: Vec<&IndexCandidate> = candidates
        .iter()
        .filter(|candidate| candidate.definition_identity() == &target_definition_identity)
        .collect();

    if target_candidates.is_empty() {
        return Err(QueryPlanningError::HybridTargetMissing {
            position,
            definition_identity: target_definition_identity,
        });
    }

    let filtered: Vec<&IndexCandidate> = if uses_parent_target {
        target_candidates
            .into_iter()
            .filter(|candidate| selector_matches(candidate, request))
            .collect()
    } else {
        target_candidates
    };

    if filtered.is_empty() {
        return Err(QueryPlanningError::InvalidHybridComponent {
            position,
            source: Box::new(QueryPlanningError::SelectionHintsMismatch {
                definition_identity: target_definition_identity,
                namespace: request.namespace().cloned(),
                definition_id: request.definition_id().cloned(),
                family: request.family(),
            }),
        });
    }

    select_from_matching_candidates(&filtered, request.consistency())
}

fn select_candidate<'a>(
    definition_identity: &IndexDefinitionIdentity,
    namespace: Option<&IndexNamespace>,
    definition_id: Option<&IndexDefinitionId>,
    family: Option<IndexFamily>,
    consistency: &ConsistencyMode,
    candidates: &'a [IndexCandidate],
) -> Result<&'a IndexCandidate, QueryPlanningError> {
    let matching_index: Vec<&IndexCandidate> = candidates
        .iter()
        .filter(|candidate| candidate.definition_identity() == definition_identity)
        .collect();

    if matching_index.is_empty() {
        return Err(QueryPlanningError::NoDefinitionCandidate {
            definition_identity: definition_identity.clone(),
        });
    }

    let matching_hints: Vec<&IndexCandidate> = matching_index
        .into_iter()
        .filter(|candidate| selector_matches_parts(candidate, namespace, definition_id, family))
        .collect();

    if matching_hints.is_empty() {
        return Err(QueryPlanningError::SelectionHintsMismatch {
            definition_identity: definition_identity.clone(),
            namespace: namespace.cloned(),
            definition_id: definition_id.cloned(),
            family,
        });
    }

    select_from_matching_candidates(&matching_hints, consistency)
}

fn select_from_matching_candidates<'a>(
    candidates: &[&'a IndexCandidate],
    consistency: &ConsistencyMode,
) -> Result<&'a IndexCandidate, QueryPlanningError> {
    let mut accepted = Vec::new();
    let mut last_error = None;

    for candidate in candidates {
        match evaluate_candidate(candidate, consistency) {
            Ok(evaluation) => accepted.push((*candidate, evaluation)),
            Err(error) => last_error = Some(error),
        }
    }

    if accepted.is_empty() {
        let first = candidates
            .first()
            .expect("candidate selection requires a non-empty candidate slice");
        return Err(QueryPlanningError::ConsistencyUnsatisfied {
            definition_identity: first.definition_identity().clone(),
            last_error: last_error.expect("rejected candidates must produce a consistency error"),
        });
    }

    if let ConsistencyMode::VersionPinned(requested) = consistency {
        if accepted.len() > 1 {
            return Err(QueryPlanningError::AmbiguousPinnedVersion {
                definition_identity: accepted[0].0.definition_identity().clone(),
                version_id: requested.clone(),
            });
        }

        return Ok(accepted[0].0);
    }

    if matches!(consistency, ConsistencyMode::Current) {
        if accepted.len() > 1 {
            return Err(QueryPlanningError::AmbiguousCurrentVersion {
                definition_identity: accepted[0].0.definition_identity().clone(),
                candidate_count: accepted.len(),
            });
        }

        return Ok(accepted[0].0);
    }

    // StaleAllowed may legitimately have multiple published candidates. The
    // planner prefers the smallest observed update-sequence
    // lag because that is the mandatory freshness dimension in Phase 4. When
    // that value is equal, the opaque version identifier provides only a
    // deterministic tie-break; it is not interpreted as semantic versioning.
    accepted.sort_by(|left, right| {
        left.1
            .update_sequence_lag()
            .cmp(&right.1.update_sequence_lag())
            .then_with(|| left.0.version().id().cmp(right.0.version().id()))
    });

    Ok(accepted[0].0)
}

fn evaluate_candidate(
    candidate: &IndexCandidate,
    consistency: &ConsistencyMode,
) -> Result<ConsistencyEvaluation, ConsistencyPolicyError> {
    candidate.synchronization().evaluate_with_policy(
        consistency,
        candidate.version().version(),
        candidate.version().lifecycle(),
    )
}

fn selector_matches(candidate: &IndexCandidate, request: &QueryRequest) -> bool {
    selector_matches_parts(
        candidate,
        request.namespace(),
        request.definition_id(),
        request.family(),
    )
}

fn selector_matches_parts(
    candidate: &IndexCandidate,
    namespace: Option<&IndexNamespace>,
    definition_id: Option<&IndexDefinitionId>,
    family: Option<IndexFamily>,
) -> bool {
    if let Some(expected) = namespace
        && candidate.definition().namespace() != expected
    {
        return false;
    }

    if let Some(expected) = definition_id
        && candidate.definition().definition_id() != expected
    {
        return false;
    }

    if let Some(expected) = family
        && candidate.definition().family() != expected
    {
        return false;
    }

    true
}

fn capability_for_hybrid_component(component: &HybridQueryComponent) -> ProviderCapability {
    if component.filter().is_some() {
        ProviderCapability::FilteredLookup
    } else {
        capability_for_atomic_query(component.query())
    }
}

fn hybrid_component_requires_ranking(component: &HybridQueryComponent) -> bool {
    atomic_query_requires_ranking(component.query())
}

fn atomic_query_requires_ranking(query: &AtomicQuery) -> bool {
    matches!(query, AtomicQuery::Similarity { .. })
}

fn capability_for_atomic_query(query: &AtomicQuery) -> ProviderCapability {
    match query {
        AtomicQuery::Exact { .. } => ProviderCapability::ExactLookup,
        AtomicQuery::Text { .. } => ProviderCapability::TextLookup,
        AtomicQuery::Structured { .. } => ProviderCapability::StructuredLookup,
        AtomicQuery::Neighborhood { .. } => ProviderCapability::NeighborhoodLookup,
        AtomicQuery::Similarity { .. } => ProviderCapability::SimilarityLookup,
    }
}

impl fmt::Display for QueryCapabilityRequirement {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_list().entries(&self.alternatives).finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use core::time::Duration;

    use crate::consistency::policy::FreshnessPolicy;
    use crate::identity::{IndexDefinitionIdentity, IndexNamespace};
    use crate::index::{
        ConsistencyRequirement, IndexVersionId, KeyDefinition, SchemaVersion, SourceVersion,
        TargetReferenceType, Uniqueness,
    };
    use crate::provider::{RankingCandidate, RankingPolicy, rank_candidates};

    fn namespace(value: &str) -> IndexNamespace {
        IndexNamespace::new(value).expect("test namespace should be valid")
    }

    fn definition_id(value: &str) -> IndexDefinitionId {
        IndexDefinitionId::new(value).expect("test definition ID should be valid")
    }

    fn definition_identity(_seed: u8, definition: &IndexDefinition) -> IndexDefinitionIdentity {
        IndexDefinitionIdentity::new(
            definition.definition_id().clone(),
            definition.namespace().clone(),
            definition.family(),
        )
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
            KeyDefinition::new(["field"]).expect("test key definition should be valid"),
            TargetReferenceType::new("object").expect("test target type should be valid"),
            Uniqueness::NonUnique,
            ConsistencyRequirement::new("eventual")
                .expect("test consistency requirement should be valid"),
            Some(SourceVersion::new("source-v1").expect("source version should be valid")),
            Some(SchemaVersion::new("schema-v1").expect("schema version should be valid")),
        )
        .expect("test definition should be valid")
    }

    fn version(value: &str) -> IndexVersionState {
        IndexVersionState::new(
            IndexVersion::with_metadata(
                IndexVersionId::new(value).expect("test version ID should be valid"),
                Some(SourceVersion::new("source-v1").expect("source version should be valid")),
                Some(SchemaVersion::new("schema-v1").expect("schema version should be valid")),
                None,
            )
            .expect("test version should be valid"),
        )
    }

    fn published_version(value: &str) -> IndexVersionState {
        let mut state = version(value);
        state
            .transition_to(VersionLifecycle::Validating)
            .expect("test version should enter validation");
        state
            .mark_ready()
            .expect("test version should become ready");
        state
            .mark_published()
            .expect("test version should become published");
        state
    }

    fn candidate(
        seed: u8,
        definition_value: &str,
        family: IndexFamily,
        version_value: &str,
        source_sequence: u64,
        indexed_sequence: u64,
    ) -> IndexCandidate {
        let definition = definition("planner.level2", definition_value, family);
        let id = definition_identity(seed, &definition);
        let sync = SynchronizationSnapshot::from_sequences(
            crate::build::UpdateSequence::new(source_sequence),
            crate::build::UpdateSequence::new(indexed_sequence),
        )
        .expect("test synchronization should be valid");

        IndexCandidate::new(id, definition, published_version(version_value), sync)
    }

    fn available_capabilities() -> ProviderCapabilities {
        ProviderCapabilities::new()
            .with_capability(ProviderCapability::ExactLookup)
            .with_capability(ProviderCapability::TextLookup)
            .with_capability(ProviderCapability::StructuredLookup)
            .with_capability(ProviderCapability::NeighborhoodLookup)
            .with_capability(ProviderCapability::SimilarityLookup)
            .with_capability(ProviderCapability::FilteredLookup)
            .with_capability(ProviderCapability::HybridRetrieval)
            .with_capability(ProviderCapability::Ranking)
    }

    #[test]
    fn exact_query_resolves_logical_candidate_and_builds_exact_plan() {
        let selected = candidate(
            1,
            "exact-definition",
            IndexFamily::Identity,
            "published-v1",
            10,
            10,
        );
        let request = QueryRequest::exact(
            selected.definition_identity().clone(),
            KeyMaterial::text("lookup"),
        )
        .expect("exact request should be valid");

        let plan = plan_query(
            &request,
            vec![selected],
            &available_capabilities(),
            ProviderAvailability::Available,
        )
        .expect("exact query should be planned");

        match plan {
            RetrievalPlan::Exact(plan) => {
                assert_eq!(plan.key(), &KeyMaterial::text("lookup"));
                assert_eq!(
                    plan.context().target().lifecycle(),
                    VersionLifecycle::Published
                );
                assert_eq!(
                    plan.context().target().consistency().update_sequence_lag(),
                    Some(0)
                );
                assert_eq!(
                    plan.context().capability_resolution(),
                    &CapabilityResolution::Direct(ProviderCapability::ExactLookup)
                );
            }
            other => panic!("expected exact plan, got {other:?}"),
        }
    }

    #[test]
    fn logical_selector_hints_are_enforced_against_the_resolved_definition() {
        let selected = candidate(
            2,
            "selector-definition",
            IndexFamily::Inverted,
            "published-v2",
            20,
            20,
        );
        let request = QueryRequest::with_spec(
            selected.definition_identity().clone(),
            Some(namespace("planner.level2")),
            Some(definition_id("selector-definition")),
            Some(IndexFamily::Inverted),
            QueryKind::Text {
                query: KeyMaterial::text("rust"),
                parameters: None,
            },
            None,
            ConsistencyMode::current(),
            ResultMode::ReferencesOnly,
            None,
        )
        .expect("selector request should be valid");

        let plan = plan_query(
            &request,
            vec![selected.clone()],
            &available_capabilities(),
            ProviderAvailability::Available,
        )
        .expect("matching selectors should plan successfully");

        assert_eq!(
            plan.target()
                .expect("single-index plan should have target")
                .definition(),
            selected.definition()
        );

        let wrong = QueryRequest::with_spec(
            selected.definition_identity().clone(),
            Some(namespace("planner.level2")),
            Some(definition_id("different-definition")),
            Some(IndexFamily::Inverted),
            QueryKind::Text {
                query: KeyMaterial::text("rust"),
                parameters: None,
            },
            None,
            ConsistencyMode::current(),
            ResultMode::ReferencesOnly,
            None,
        )
        .expect("wrong selector value is structurally valid");

        let error = plan_query(
            &wrong,
            vec![selected],
            &available_capabilities(),
            ProviderAvailability::Available,
        )
        .expect_err("mismatching definition selector must be rejected");

        assert!(matches!(
            error,
            QueryPlanningError::SelectionHintsMismatch { .. }
        ));
    }

    #[test]
    fn current_consistency_selects_the_zero_lag_published_candidate() {
        let definition = definition("planner.level2", "current-selection", IndexFamily::Inverted);
        let shared_definition_identity = definition_identity(3, &definition);
        let stale_sync = SynchronizationSnapshot::from_sequences(
            crate::build::UpdateSequence::new(30),
            crate::build::UpdateSequence::new(28),
        )
        .expect("stale synchronization should be valid");
        let current_sync = SynchronizationSnapshot::from_sequences(
            crate::build::UpdateSequence::new(30),
            crate::build::UpdateSequence::new(30),
        )
        .expect("current synchronization should be valid");
        let stale = IndexCandidate::new(
            shared_definition_identity.clone(),
            definition.clone(),
            published_version("published-v1"),
            stale_sync,
        );
        let current = IndexCandidate::new(
            shared_definition_identity.clone(),
            definition,
            published_version("published-v2"),
            current_sync,
        );
        let request = QueryRequest::with_query(
            shared_definition_identity.clone(),
            QueryKind::Text {
                query: KeyMaterial::text("current"),
                parameters: None,
            },
        )
        .expect("current query should be valid");

        let plan = plan_query(
            &request,
            vec![stale, current.clone()],
            &available_capabilities(),
            ProviderAvailability::Available,
        )
        .expect("current query should select the acceptable candidate");

        assert_eq!(
            plan.target()
                .expect("single-index plan should have target")
                .version()
                .id(),
            current.version().id()
        );
        assert_eq!(
            plan.consistency()
                .expect("single-index plan should have consistency")
                .update_sequence_lag(),
            Some(0)
        );
    }

    #[test]
    fn current_consistency_rejects_ambiguous_zero_lag_candidates_without_active_registry() {
        let definition = definition("planner.level2", "current-ambiguous", IndexFamily::Inverted);
        let shared_definition_identity = definition_identity(4, &definition);

        let first = IndexCandidate::new(
            shared_definition_identity.clone(),
            definition.clone(),
            published_version("published-v1"),
            SynchronizationSnapshot::from_sequences(
                crate::build::UpdateSequence::new(40),
                crate::build::UpdateSequence::new(40),
            )
            .expect("first synchronization should be valid"),
        );

        let second = IndexCandidate::new(
            shared_definition_identity.clone(),
            definition,
            published_version("published-v2"),
            SynchronizationSnapshot::from_sequences(
                crate::build::UpdateSequence::new(40),
                crate::build::UpdateSequence::new(40),
            )
            .expect("second synchronization should be valid"),
        );

        let request = QueryRequest::with_query(
            shared_definition_identity.clone(),
            QueryKind::Text {
                query: KeyMaterial::text("ambiguous-current"),
                parameters: None,
            },
        )
        .expect("current query should be valid");

        let error = plan_query(
            &request,
            vec![first, second],
            &available_capabilities(),
            ProviderAvailability::Available,
        )
        .expect_err("Current must not guess between multiple zero-lag published candidates");

        assert!(matches!(
            error,
            QueryPlanningError::AmbiguousCurrentVersion {
                candidate_count: 2,
                ..
            }
        ));
    }

    #[test]
    fn current_consistency_rejects_positive_lag_instead_of_rewriting_the_query() {
        let selected = candidate(
            5,
            "lagged-v1",
            IndexFamily::Inverted,
            "published-v1",
            40,
            38,
        );
        let request = QueryRequest::text(
            selected.definition_identity().clone(),
            KeyMaterial::text("lagged"),
        )
        .expect("text request should be valid");

        let error = plan_query(
            &request,
            vec![selected],
            &available_capabilities(),
            ProviderAvailability::Available,
        )
        .expect_err("Current must reject a positive sequence lag");

        assert!(matches!(
            error,
            QueryPlanningError::ConsistencyUnsatisfied {
                last_error: ConsistencyPolicyError::CurrentStateNotFresh { lag: 2 },
                ..
            }
        ));
    }

    #[test]
    fn pinned_consistency_selects_exact_version_without_fallback() {
        let pinned = candidate(6, "pinned-v7", IndexFamily::Inverted, "v7", 50, 48);
        let other = candidate(7, "other-v8", IndexFamily::Inverted, "v8", 50, 50);
        let request = QueryRequest::with_query_options(
            pinned.definition_identity().clone(),
            QueryKind::Text {
                query: KeyMaterial::text("pinned"),
                parameters: None,
            },
            None,
            ConsistencyMode::version_pinned(
                IndexVersionId::new("v7").expect("pinned version ID should be valid"),
            ),
            ResultMode::ReferencesOnly,
            None,
        )
        .expect("pinned request should be valid");

        let plan = plan_query(
            &request,
            vec![pinned.clone(), other],
            &available_capabilities(),
            ProviderAvailability::Available,
        )
        .expect("exact pinned version should be selected");

        assert_eq!(
            plan.target()
                .expect("single-index plan should have target")
                .version()
                .id(),
            pinned.version().id()
        );
        assert_eq!(
            plan.consistency()
                .expect("single-index plan should have consistency")
                .state(),
            crate::consistency::policy::ConsistencyEvaluationState::VersionPinned
        );
    }

    #[test]
    fn pinned_consistency_does_not_fallback_to_another_version() {
        let selected = candidate(8, "available-v8", IndexFamily::Inverted, "v8", 60, 60);
        let request = QueryRequest::with_query_options(
            selected.definition_identity().clone(),
            QueryKind::Text {
                query: KeyMaterial::text("pinned"),
                parameters: None,
            },
            None,
            ConsistencyMode::version_pinned(
                IndexVersionId::new("missing-v7").expect("pinned version ID should be valid"),
            ),
            ResultMode::ReferencesOnly,
            None,
        )
        .expect("pinned request should be valid");

        let error = plan_query(
            &request,
            vec![selected],
            &available_capabilities(),
            ProviderAvailability::Available,
        )
        .expect_err("a missing pinned version must not fall back");

        assert!(matches!(
            error,
            QueryPlanningError::ConsistencyUnsatisfied {
                last_error: ConsistencyPolicyError::PinnedVersionMismatch { .. },
                ..
            }
        ));
    }

    #[test]
    fn stale_allowed_requires_all_declared_freshness_constraints() {
        let selected = {
            let definition =
                definition("planner.level2", "stale-definition", IndexFamily::Inverted);
            let sync = SynchronizationSnapshot::from_sequences(
                crate::build::UpdateSequence::new(70),
                crate::build::UpdateSequence::new(68),
            )
            .expect("synchronization should be valid")
            .with_time_lag(Duration::from_secs(2));
            IndexCandidate::new(
                definition_identity(9, &definition),
                definition,
                published_version("stale-v1"),
                sync,
            )
        };

        let request = QueryRequest::with_query_options(
            selected.definition_identity().clone(),
            QueryKind::Text {
                query: KeyMaterial::text("stale"),
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
            ResultMode::ReferencesOnly,
            None,
        )
        .expect("stale query should be valid");

        let plan = plan_query(
            &request,
            vec![selected],
            &available_capabilities(),
            ProviderAvailability::Available,
        )
        .expect("all stale constraints should pass");

        assert_eq!(
            plan.consistency()
                .expect("single-index plan should have consistency")
                .state(),
            crate::consistency::policy::ConsistencyEvaluationState::StaleAccepted
        );
        assert_eq!(
            plan.consistency()
                .expect("single-index plan should have consistency")
                .time_lag(),
            Some(Duration::from_secs(2))
        );
    }

    #[test]
    fn unpublished_candidate_never_becomes_a_retrieval_plan() {
        let definition = definition(
            "planner.level2",
            "candidate-definition",
            IndexFamily::Inverted,
        );
        let id = definition_identity(10, &definition);
        let sync = SynchronizationSnapshot::from_sequences(
            crate::build::UpdateSequence::new(80),
            crate::build::UpdateSequence::new(80),
        )
        .expect("synchronization should be valid");
        let candidate = IndexCandidate::new(id, definition, version("ready-v1"), sync);
        let mut ready = candidate.version.clone();
        ready
            .transition_to(VersionLifecycle::Validating)
            .expect("candidate should enter validation");
        ready.mark_ready().expect("candidate should become ready");

        let candidate = IndexCandidate::new(
            candidate.definition_identity().clone(),
            candidate.definition.clone(),
            ready,
            candidate.synchronization.clone(),
        );
        let request = QueryRequest::exact(
            candidate.definition_identity().to_owned(),
            KeyMaterial::text("candidate"),
        )
        .expect("exact request should be valid");

        let error = plan_query(
            &request,
            vec![candidate],
            &available_capabilities(),
            ProviderAvailability::Available,
        )
        .expect_err("READY candidate must never become queryable");

        assert!(matches!(
            error,
            QueryPlanningError::ConsistencyUnsatisfied {
                last_error: ConsistencyPolicyError::UnqueryableVersion {
                    lifecycle: VersionLifecycle::Ready,
                    ..
                },
                ..
            }
        ));
    }

    #[test]
    fn similarity_planning_requires_similarity_and_generic_ranking_capabilities() {
        let selected = candidate(
            11,
            "similarity-definition",
            IndexFamily::Similarity,
            "published-v1",
            90,
            90,
        );
        let request = QueryRequest::similarity(
            selected.definition_identity().clone(),
            KeyMaterial::bytes([1_u8, 2_u8, 3_u8]),
        )
        .expect("similarity request should be valid");

        let capabilities = ProviderCapabilities::with(ProviderCapability::SimilarityLookup);
        let error = plan_query(
            &request,
            vec![selected.clone()],
            &capabilities,
            ProviderAvailability::Available,
        )
        .expect_err("similarity must require generic ranking support");

        assert!(matches!(
            error,
            QueryPlanningError::MissingProviderCapabilities { .. }
        ));

        let capabilities = capabilities.with_capability(ProviderCapability::Ranking);
        let plan = plan_query(
            &request,
            vec![selected],
            &capabilities,
            ProviderAvailability::Available,
        )
        .expect("similarity should plan when both capabilities are present");

        assert_eq!(
            plan.capability_resolution().capabilities(),
            &[
                ProviderCapability::SimilarityLookup,
                ProviderCapability::Ranking
            ]
        );
    }

    #[test]
    fn filtered_similarity_requires_generic_ranking_alongside_filter_capability() {
        let selected = candidate(
            25,
            "filtered-similarity",
            IndexFamily::Similarity,
            "v1",
            95,
            95,
        );
        let request = QueryRequest::filtered(
            selected.definition_identity().clone(),
            AtomicQuery::Similarity {
                representation: KeyMaterial::bytes([4_u8, 5_u8]),
                parameters: None,
            },
            KeyMaterial::text("published"),
        )
        .expect("filtered similarity request should be valid");

        let capabilities = ProviderCapabilities::with(ProviderCapability::FilteredLookup);
        let error = plan_query(
            &request,
            vec![selected.clone()],
            &capabilities,
            ProviderAvailability::Available,
        )
        .expect_err("filtered similarity requires ranking support");

        assert!(matches!(
            error,
            QueryPlanningError::MissingProviderCapabilities { .. }
        ));

        let capabilities = capabilities.with_capability(ProviderCapability::Ranking);
        let plan = plan_query(
            &request,
            vec![selected],
            &capabilities,
            ProviderAvailability::Available,
        )
        .expect("filtered similarity should plan with ranking support");

        assert_eq!(
            plan.capability_resolution().capabilities(),
            &[
                ProviderCapability::FilteredLookup,
                ProviderCapability::Ranking
            ]
        );
    }

    #[test]
    fn hybrid_prefers_provider_native_hybrid_capability_when_available() {
        let first = candidate(12, "hybrid-a", IndexFamily::Inverted, "a-v1", 100, 100);
        let second = candidate(13, "hybrid-b", IndexFamily::Similarity, "b-v1", 100, 100);

        let first_component = HybridQueryComponent::new(AtomicQuery::Text {
            query: KeyMaterial::text("rust"),
            parameters: None,
        })
        .expect("text component should be valid");
        let second_component = HybridQueryComponent::new(AtomicQuery::Similarity {
            representation: KeyMaterial::bytes([5_u8, 6_u8]),
            parameters: None,
        })
        .expect("similarity component should be valid");

        let request = QueryRequest::hybrid(
            first.definition_identity().clone(),
            vec![first_component, second_component],
        )
        .expect("hybrid request should be valid");

        let plan = plan_query(
            &request,
            vec![first, second],
            &available_capabilities(),
            ProviderAvailability::Available,
        )
        .expect("native hybrid capability should plan successfully");

        match plan {
            RetrievalPlan::Hybrid(plan) => {
                assert_eq!(plan.components().len(), 2);
                assert_eq!(
                    plan.capability_resolution(),
                    &CapabilityResolution::Direct(ProviderCapability::HybridRetrieval)
                );
            }
            other => panic!("expected hybrid plan, got {other:?}"),
        }
    }

    #[test]
    fn hybrid_can_compose_atomic_provider_capabilities_when_native_hybrid_is_absent() {
        let first = candidate(
            14,
            "hybrid-compose-a",
            IndexFamily::Inverted,
            "a-v1",
            110,
            110,
        );
        let second = candidate(
            15,
            "hybrid-compose-b",
            IndexFamily::Inverted,
            "b-v1",
            110,
            110,
        );

        let first_component = HybridQueryComponent::new(AtomicQuery::Text {
            query: KeyMaterial::text("rust"),
            parameters: None,
        })
        .expect("text component should be valid");
        let second_component = HybridQueryComponent::new(AtomicQuery::Exact {
            key: KeyMaterial::text("book"),
        })
        .expect("exact component should be valid");

        let request = QueryRequest::hybrid(
            first.definition_identity().clone(),
            vec![first_component, second_component],
        )
        .expect("hybrid request should be valid");

        let capabilities = ProviderCapabilities::new()
            .with_capability(ProviderCapability::TextLookup)
            .with_capability(ProviderCapability::ExactLookup);

        let plan = plan_query(
            &request,
            vec![first, second],
            &capabilities,
            ProviderAvailability::Available,
        )
        .expect("logical hybrid composition should be supported");

        match plan {
            RetrievalPlan::Hybrid(plan) => {
                assert_eq!(
                    plan.components()[0].capability(),
                    ProviderCapability::TextLookup
                );
                assert_eq!(
                    plan.components()[1].capability(),
                    ProviderCapability::ExactLookup
                );
                assert_eq!(
                    plan.capability_resolution().capabilities(),
                    &[
                        ProviderCapability::TextLookup,
                        ProviderCapability::ExactLookup
                    ]
                );
            }
            other => panic!("expected hybrid plan, got {other:?}"),
        }
    }

    #[test]
    fn hybrid_component_filters_require_filtered_lookup_capability() {
        let selected = candidate(16, "hybrid-filter", IndexFamily::Inverted, "v1", 120, 120);
        let component = HybridQueryComponent::with_options(
            None,
            AtomicQuery::Text {
                query: KeyMaterial::text("rust"),
                parameters: None,
            },
            Some(
                KeyMaterial::map([("language", KeyMaterial::text("en"))])
                    .expect("filter should be valid"),
            ),
        )
        .expect("filtered hybrid component should be valid");
        let request = QueryRequest::hybrid(selected.definition_identity().clone(), vec![component])
            .expect_err("one component is invalid by the request contract");
        assert!(matches!(
            request,
            crate::query::request::QueryRequestValidationError::InvalidQuery(
                crate::query::request::QueryKindValidationError::InsufficientHybridComponents {
                    count: 1
                }
            )
        ));

        let first = candidate(
            17,
            "hybrid-filter-a",
            IndexFamily::Inverted,
            "a-v1",
            120,
            120,
        );
        let second = candidate(
            18,
            "hybrid-filter-b",
            IndexFamily::Inverted,
            "b-v1",
            120,
            120,
        );
        let first_component = HybridQueryComponent::new(AtomicQuery::Text {
            query: KeyMaterial::text("rust"),
            parameters: None,
        })
        .expect("first component should be valid");
        let filtered_component = HybridQueryComponent::with_options(
            Some(second.definition_identity().clone()),
            AtomicQuery::Exact {
                key: KeyMaterial::text("book"),
            },
            Some(KeyMaterial::text("published")),
        )
        .expect("filtered component should be valid");

        let request = QueryRequest::hybrid(
            first.definition_identity().clone(),
            vec![first_component, filtered_component],
        )
        .expect("two-component hybrid request should be valid");

        let capabilities = ProviderCapabilities::with(ProviderCapability::TextLookup)
            .with_capability(ProviderCapability::FilteredLookup);

        let plan = plan_query(
            &request,
            vec![first, second],
            &capabilities,
            ProviderAvailability::Available,
        )
        .expect("composed hybrid with filtering should plan");

        match plan {
            RetrievalPlan::Hybrid(plan) => {
                assert_eq!(
                    plan.components()[0].capability(),
                    ProviderCapability::TextLookup
                );
                assert_eq!(
                    plan.components()[1].capability(),
                    ProviderCapability::FilteredLookup
                );
            }
            other => panic!("expected hybrid plan, got {other:?}"),
        }
    }

    #[test]
    fn provider_unavailability_is_distinguished_from_missing_capability() {
        let selected = candidate(19, "availability", IndexFamily::Identity, "v1", 130, 130);
        let request = QueryRequest::exact(
            selected.definition_identity().clone(),
            KeyMaterial::text("x"),
        )
        .expect("exact request should be valid");

        let unavailable = plan_query(
            &request,
            vec![selected.clone()],
            &available_capabilities(),
            ProviderAvailability::TemporarilyUnavailable,
        )
        .expect_err("temporary provider unavailability should be explicit");
        assert!(matches!(
            unavailable,
            QueryPlanningError::ProviderUnavailable {
                availability: ProviderAvailability::TemporarilyUnavailable
            }
        ));

        let missing_capability = plan_query(
            &request,
            vec![selected],
            &ProviderCapabilities::new(),
            ProviderAvailability::Available,
        )
        .expect_err("missing exact capability should remain distinct");
        assert!(matches!(
            missing_capability,
            QueryPlanningError::MissingProviderCapabilities { .. }
        ));
    }

    #[test]
    fn stale_allowed_selects_the_lowest_observed_update_lag_without_using_domain_semantics() {
        let definition = definition("planner.level2", "stale-selection", IndexFamily::Inverted);
        let shared_definition_identity = definition_identity(20, &definition);
        let one_lag = IndexCandidate::new(
            shared_definition_identity.clone(),
            definition.clone(),
            published_version("z-version"),
            SynchronizationSnapshot::from_sequences(
                crate::build::UpdateSequence::new(150),
                crate::build::UpdateSequence::new(149),
            )
            .expect("one-lag synchronization should be valid"),
        );
        let two_lag = IndexCandidate::new(
            shared_definition_identity.clone(),
            definition,
            published_version("a-version"),
            SynchronizationSnapshot::from_sequences(
                crate::build::UpdateSequence::new(150),
                crate::build::UpdateSequence::new(148),
            )
            .expect("two-lag synchronization should be valid"),
        );

        let request = QueryRequest::with_query_options(
            shared_definition_identity.clone(),
            QueryKind::Text {
                query: KeyMaterial::text("meaning-free"),
                parameters: None,
            },
            None,
            ConsistencyMode::stale_allowed(FreshnessPolicy::new(2)),
            ResultMode::ReferencesOnly,
            None,
        )
        .expect("stale request should be valid");

        let plan = plan_query(
            &request,
            vec![one_lag.clone(), two_lag],
            &available_capabilities(),
            ProviderAvailability::Available,
        )
        .expect("one-lag candidate should satisfy stale policy");

        assert_eq!(
            plan.target()
                .expect("single-index plan should have target")
                .version()
                .id(),
            one_lag.version().id()
        );
        assert_eq!(
            plan.consistency()
                .expect("single-index plan should have consistency")
                .update_sequence_lag(),
            Some(1)
        );
    }

    #[test]
    fn duplicate_pinned_candidates_are_not_silently_chosen() {
        let first = candidate(22, "duplicate-a", IndexFamily::Inverted, "v7", 160, 160);
        let duplicate = IndexCandidate::new(
            first.definition_identity().clone(),
            first.definition().clone(),
            first.version().clone(),
            first.synchronization().clone(),
        );
        let request = QueryRequest::with_query_options(
            first.definition_identity().clone(),
            QueryKind::Text {
                query: KeyMaterial::text("duplicate"),
                parameters: None,
            },
            None,
            ConsistencyMode::version_pinned(
                IndexVersionId::new("v7").expect("version should be valid"),
            ),
            ResultMode::ReferencesOnly,
            None,
        )
        .expect("request should be valid");

        let error = plan_query(
            &request,
            vec![first, duplicate],
            &available_capabilities(),
            ProviderAvailability::Available,
        )
        .expect_err("duplicate exact pinned candidates must be rejected");

        assert!(matches!(
            error,
            QueryPlanningError::AmbiguousPinnedVersion { .. }
        ));
    }

    #[test]
    fn plan_preserves_limit_result_mode_and_request_metadata_without_reinterpreting_them() {
        let selected = candidate(24, "metadata-plan", IndexFamily::Inverted, "v1", 180, 180);
        let request = QueryRequest::with_query_options(
            selected.definition_identity().clone(),
            QueryKind::Text {
                query: KeyMaterial::text("opaque-query"),
                parameters: Some(KeyMaterial::Unsigned(7)),
            },
            Some(NonZeroUsize::new(5).expect("positive limit should be valid")),
            ConsistencyMode::current(),
            ResultMode::ReferencesWithMetadata,
            Some(KeyMaterial::text("opaque-request-metadata")),
        )
        .expect("request should be valid");

        let plan = plan_query(
            &request,
            vec![selected],
            &available_capabilities(),
            ProviderAvailability::Available,
        )
        .expect("query should be planned");

        match plan {
            RetrievalPlan::Text(plan) => {
                assert_eq!(
                    plan.context().limit(),
                    Some(NonZeroUsize::new(5).expect("positive limit should be valid"))
                );
                assert_eq!(
                    plan.context().result_mode(),
                    ResultMode::ReferencesWithMetadata
                );
                assert_eq!(
                    plan.context().metadata(),
                    Some(&KeyMaterial::text("opaque-request-metadata"))
                );
                assert_eq!(plan.query(), &KeyMaterial::text("opaque-query"));
                assert_eq!(plan.parameters(), Some(&KeyMaterial::Unsigned(7)));
            }
            other => panic!("expected text plan, got {other:?}"),
        }
    }

    #[test]
    fn ranking_module_remains_a_separate_provider_mechanism_from_plan_construction() {
        let selected = candidate(
            23,
            "ranking-separation",
            IndexFamily::Similarity,
            "v1",
            170,
            170,
        );
        let request = QueryRequest::similarity(
            selected.definition_identity().clone(),
            KeyMaterial::bytes([9_u8, 8_u8]),
        )
        .expect("similarity request should be valid");

        let plan = plan_query(
            &request,
            vec![selected],
            &available_capabilities(),
            ProviderAvailability::Available,
        )
        .expect("similarity plan should be valid");

        let ranked = rank_candidates(
            &ProviderCapabilities::with(ProviderCapability::Ranking),
            vec![
                RankingCandidate::new(
                    ObjectReference::new("source", "b").expect("reference should be valid"),
                )
                .with_score(0.2)
                .expect("score should be valid"),
                RankingCandidate::new(
                    ObjectReference::new("source", "a").expect("reference should be valid"),
                )
                .with_score(0.9)
                .expect("score should be valid"),
            ],
            &RankingPolicy::score_descending(),
        )
        .expect("generic provider ranking should succeed");

        assert!(matches!(plan, RetrievalPlan::Similarity(_)));
        assert_eq!(ranked[0].reference().object_reference(), "a");
    }
}
