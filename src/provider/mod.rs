//! Shared provider abstraction boundary for the Indexing Engine.
//!
//! Phase 4 keeps provider concerns at one top-level Indexing boundary shared by
//! build, update, query, retrieval, and recovery workflows. This module
//! describes provider capabilities and exposes the generic provider-side
//! ranking mechanism implemented in [`ranking`].
//!
//! The module deliberately does not define a physical provider, storage
//! backend, database query language, or execution plan. A later provider
//! implementation can advertise these logical capabilities and perform the
//! corresponding physical work behind this boundary.
//!
//! The ownership boundary remains:
//!
//! ```text
//! Indexing
//!     -> logical query requirements and retrieval orchestration
//!
//! Provider abstraction
//!     -> reusable provider capabilities and generic ranking
//!
//! Physical provider
//!     -> physical indexes, storage, and retrieval mechanics
//! ```
//!
//! Generic ranking is intentionally separated into [`ranking`] so ranking
//! mechanics remain reusable without becoming domain-specific relevance logic.

pub mod ranking;

pub use ranking::{
    RankingCandidate, RankingCandidateValidationError, RankingCriterion, RankingDirection,
    RankingError, RankingPolicy, rank,
};

use core::fmt;
use std::collections::BTreeSet;

/// A logical retrieval or provider-side capability advertised by a provider.
///
/// These values describe **what kind of logical operation a provider can
/// satisfy**. They do not identify how the provider implements that operation.
/// In particular, they do not expose physical algorithms, storage engines,
/// database operators, or execution plans.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ProviderCapability {
    /// Exact logical lookup.
    ExactLookup,

    /// Text/inverted logical retrieval.
    TextLookup,

    /// Structured logical record retrieval.
    StructuredLookup,

    /// Neighborhood retrieval around an opaque source-owned reference.
    NeighborhoodLookup,

    /// Similarity-oriented logical retrieval.
    SimilarityLookup,

    /// Logical filtered retrieval.
    FilteredLookup,

    /// Provider-side execution of a hybrid logical retrieval request.
    HybridRetrieval,

    /// Generic provider-side ranking of retrieval candidates.
    Ranking,
}

impl fmt::Display for ProviderCapability {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::ExactLookup => "exact-lookup",
            Self::TextLookup => "text-lookup",
            Self::StructuredLookup => "structured-lookup",
            Self::NeighborhoodLookup => "neighborhood-lookup",
            Self::SimilarityLookup => "similarity-lookup",
            Self::FilteredLookup => "filtered-lookup",
            Self::HybridRetrieval => "hybrid-retrieval",
            Self::Ranking => "ranking",
        };

        formatter.write_str(value)
    }
}

/// A generic provider capability set.
///
/// The set is deliberately independent of a concrete provider handle or
/// physical implementation. It is suitable for capability advertisement and
/// logical planner-side negotiation.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ProviderCapabilities {
    capabilities: BTreeSet<ProviderCapability>,
}

impl ProviderCapabilities {
    /// Creates an empty capability set.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a capability set containing one capability.
    #[must_use]
    pub fn with(capability: ProviderCapability) -> Self {
        let mut capabilities = Self::new();
        capabilities.insert(capability);
        capabilities
    }

    /// Adds a capability while building a capability set through method chaining.
    #[must_use]
    pub fn with_capability(mut self, capability: ProviderCapability) -> Self {
        self.insert(capability);
        self
    }

    /// Adds a capability to this set.
    ///
    /// Returns `true` when the capability was not already present.
    pub fn insert(&mut self, capability: ProviderCapability) -> bool {
        self.capabilities.insert(capability)
    }

    /// Removes a capability from this set.
    ///
    /// Returns `true` when the capability was present.
    pub fn remove(&mut self, capability: ProviderCapability) -> bool {
        self.capabilities.remove(&capability)
    }

    /// Returns whether this provider advertises the supplied capability.
    #[must_use]
    pub fn supports(&self, capability: ProviderCapability) -> bool {
        self.capabilities.contains(&capability)
    }

    /// Requires one capability to be present.
    pub fn require(&self, capability: ProviderCapability) -> Result<(), ProviderCapabilityError> {
        if self.supports(capability) {
            Ok(())
        } else {
            Err(ProviderCapabilityError::Missing { capability })
        }
    }

    /// Returns whether all supplied capabilities are advertised.
    #[must_use]
    pub fn supports_all<I>(&self, capabilities: I) -> bool
    where
        I: IntoIterator<Item = ProviderCapability>,
    {
        capabilities
            .into_iter()
            .all(|capability| self.supports(capability))
    }

    /// Requires every supplied capability to be present.
    pub fn require_all<I>(&self, capabilities: I) -> Result<(), ProviderCapabilityError>
    where
        I: IntoIterator<Item = ProviderCapability>,
    {
        for capability in capabilities {
            self.require(capability)?;
        }

        Ok(())
    }

    /// Returns the number of advertised capabilities.
    #[must_use]
    pub fn len(&self) -> usize {
        self.capabilities.len()
    }

    /// Returns whether no capabilities are advertised.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.capabilities.is_empty()
    }

    /// Iterates over advertised capabilities in deterministic order.
    pub fn iter(&self) -> impl Iterator<Item = &ProviderCapability> {
        self.capabilities.iter()
    }
}

impl IntoIterator for ProviderCapabilities {
    type Item = ProviderCapability;
    type IntoIter = std::collections::btree_set::IntoIter<ProviderCapability>;

    fn into_iter(self) -> Self::IntoIter {
        self.capabilities.into_iter()
    }
}

/// Failure raised when a provider does not advertise a required capability.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderCapabilityError {
    /// The requested capability is not advertised by the provider.
    Missing { capability: ProviderCapability },
}

impl fmt::Display for ProviderCapabilityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Missing { capability } => {
                write!(
                    formatter,
                    "provider does not advertise capability {capability}"
                )
            }
        }
    }
}

impl std::error::Error for ProviderCapabilityError {}

/// Provider availability at the logical abstraction boundary.
///
/// This describes whether the provider may currently participate in execution;
/// it does not alter or reinterpret the advertised capability set.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ProviderAvailability {
    /// Provider is available for normal execution.
    Available,

    /// Provider exists and advertises capabilities, but execution is currently
    /// unavailable and may become available later.
    TemporarilyUnavailable,

    /// Provider is not currently available for execution.
    Unavailable,
}

impl fmt::Display for ProviderAvailability {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::Available => "available",
            Self::TemporarilyUnavailable => "temporarily-unavailable",
            Self::Unavailable => "unavailable",
        };

        formatter.write_str(value)
    }
}

/// Errors produced when the shared provider boundary invokes generic ranking.
#[derive(Clone, Debug, PartialEq)]
pub enum ProviderRankingError {
    /// The provider did not advertise generic ranking capability.
    RankingUnsupported,

    /// The generic ranking operation rejected the supplied candidates or
    /// ranking policy.
    Ranking(RankingError),
}

impl fmt::Display for ProviderRankingError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RankingUnsupported => {
                formatter.write_str("provider does not advertise generic ranking capability")
            }
            Self::Ranking(error) => write!(formatter, "provider ranking failed: {error}"),
        }
    }
}

impl std::error::Error for ProviderRankingError {}

impl From<RankingError> for ProviderRankingError {
    fn from(error: RankingError) -> Self {
        Self::Ranking(error)
    }
}

/// Invokes the shared generic ranking mechanism through an advertised
/// provider capability set.
///
/// This adapter is intentionally small: it verifies that the logical provider
/// boundary advertises [`ProviderCapability::Ranking`] and then delegates all
/// candidate validation and ordering to [`ranking::rank`]. It does not add
/// another ranking algorithm or interpret ranking values semantically.
pub fn rank_candidates<I>(
    capabilities: &ProviderCapabilities,
    candidates: I,
    policy: &RankingPolicy,
) -> Result<Vec<RankingCandidate>, ProviderRankingError>
where
    I: IntoIterator<Item = RankingCandidate>,
{
    capabilities
        .require(ProviderCapability::Ranking)
        .map_err(|_| ProviderRankingError::RankingUnsupported)?;

    ranking::rank(candidates, policy).map_err(ProviderRankingError::Ranking)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reference(source: &str, value: &str) -> crate::index::ObjectReference {
        crate::index::ObjectReference::new(source, value).expect("test reference should be valid")
    }

    fn candidate(source: &str, value: &str, score: f64) -> RankingCandidate {
        RankingCandidate::new(reference(source, value))
            .with_score(score)
            .expect("finite test score should be valid")
    }

    #[test]
    fn provider_module_exposes_all_phase4_logical_retrieval_capabilities_and_ranking() {
        let capabilities = ProviderCapabilities::new()
            .with_capability(ProviderCapability::ExactLookup)
            .with_capability(ProviderCapability::TextLookup)
            .with_capability(ProviderCapability::StructuredLookup)
            .with_capability(ProviderCapability::NeighborhoodLookup)
            .with_capability(ProviderCapability::SimilarityLookup)
            .with_capability(ProviderCapability::FilteredLookup)
            .with_capability(ProviderCapability::HybridRetrieval)
            .with_capability(ProviderCapability::Ranking);

        assert_eq!(capabilities.len(), 8);
        assert!(capabilities.supports_all([
            ProviderCapability::ExactLookup,
            ProviderCapability::TextLookup,
            ProviderCapability::StructuredLookup,
            ProviderCapability::NeighborhoodLookup,
            ProviderCapability::SimilarityLookup,
            ProviderCapability::FilteredLookup,
            ProviderCapability::HybridRetrieval,
            ProviderCapability::Ranking,
        ]));
    }

    #[test]
    fn capability_advertisement_is_deterministic_and_round_trippable() {
        let capabilities = ProviderCapabilities::new()
            .with_capability(ProviderCapability::SimilarityLookup)
            .with_capability(ProviderCapability::ExactLookup)
            .with_capability(ProviderCapability::Ranking)
            .with_capability(ProviderCapability::TextLookup);

        let listed: Vec<ProviderCapability> = capabilities.iter().copied().collect();

        assert_eq!(
            listed,
            vec![
                ProviderCapability::ExactLookup,
                ProviderCapability::TextLookup,
                ProviderCapability::SimilarityLookup,
                ProviderCapability::Ranking,
            ]
        );

        let round_trip: Vec<ProviderCapability> = capabilities.clone().into_iter().collect();
        assert_eq!(listed, round_trip);
    }

    #[test]
    fn missing_provider_capability_is_distinguished_from_empty_capability_set() {
        let capabilities = ProviderCapabilities::new();

        assert!(capabilities.is_empty());
        assert!(matches!(
            capabilities.require(ProviderCapability::SimilarityLookup),
            Err(ProviderCapabilityError::Missing {
                capability: ProviderCapability::SimilarityLookup
            })
        ));
    }

    #[test]
    fn capability_removal_affects_only_the_removed_capability() {
        let mut capabilities = ProviderCapabilities::with(ProviderCapability::Ranking);
        capabilities.insert(ProviderCapability::TextLookup);
        capabilities.insert(ProviderCapability::SimilarityLookup);

        assert!(capabilities.remove(ProviderCapability::TextLookup));
        assert!(!capabilities.supports(ProviderCapability::TextLookup));
        assert!(capabilities.supports(ProviderCapability::SimilarityLookup));
        assert!(capabilities.supports(ProviderCapability::Ranking));
        assert!(!capabilities.remove(ProviderCapability::TextLookup));
    }

    #[test]
    fn provider_boundary_uses_ranking_module_only_when_ranking_is_advertised() {
        let mut without_ranking = ProviderCapabilities::new();
        without_ranking.insert(ProviderCapability::ExactLookup);

        let unsupported = rank_candidates(
            &without_ranking,
            vec![candidate("source", "object-a", 0.9)],
            &RankingPolicy::score_descending(),
        )
        .expect_err("ranking must require explicit provider capability");

        assert!(matches!(
            unsupported,
            ProviderRankingError::RankingUnsupported
        ));

        let with_ranking = ProviderCapabilities::with(ProviderCapability::Ranking);
        let ranked = rank_candidates(
            &with_ranking,
            vec![
                candidate("source", "object-b", 0.2),
                candidate("source", "object-a", 0.8),
            ],
            &RankingPolicy::score_descending(),
        )
        .expect("advertised ranking capability should delegate to ranking module");

        assert_eq!(ranked.len(), 2);
        assert_eq!(ranked[0].reference().object_reference(), "object-a");
        assert_eq!(ranked[1].reference().object_reference(), "object-b");
    }

    #[test]
    fn provider_ranking_propagates_ranking_errors_without_converting_them_to_empty_results() {
        let capabilities = ProviderCapabilities::with(ProviderCapability::Ranking);
        let candidates = vec![
            candidate("source", "object-a", 0.9),
            RankingCandidate::new(reference("source", "object-b")),
        ];

        let error = rank_candidates(
            &capabilities,
            candidates,
            &RankingPolicy::score_descending(),
        )
        .expect_err("missing ranking criterion must remain an error");

        assert!(matches!(
            error,
            ProviderRankingError::Ranking(RankingError::MissingCriterion {
                position: 1,
                criterion: RankingCriterion::Score,
                ..
            })
        ));
    }

    #[test]
    fn provider_boundary_preserves_reference_oriented_ranking_metadata() {
        let candidate = RankingCandidate::new(reference("source", "opaque-object"))
            .with_metrics(Some(0.75), Some(0.25))
            .expect("finite metrics should be valid")
            .with_ordering_key(crate::index::KeyMaterial::Unsigned(42))
            .expect("ordering key should be valid")
            .with_metadata(crate::index::KeyMaterial::text("opaque-metadata"))
            .expect("metadata should be valid");

        let capabilities = ProviderCapabilities::with(ProviderCapability::Ranking);
        let ranked = rank_candidates(
            &capabilities,
            vec![candidate],
            &RankingPolicy::score_descending(),
        )
        .expect("fully populated generic candidate should rank successfully");

        assert_eq!(ranked.len(), 1);
        assert_eq!(ranked[0].reference().source(), "source");
        assert_eq!(ranked[0].reference().object_reference(), "opaque-object");
        assert_eq!(ranked[0].score(), Some(0.75));
        assert_eq!(ranked[0].distance(), Some(0.25));
        assert_eq!(
            ranked[0].ordering_key(),
            Some(&crate::index::KeyMaterial::Unsigned(42))
        );
        assert_eq!(
            ranked[0].metadata(),
            Some(&crate::index::KeyMaterial::text("opaque-metadata"))
        );
    }

    #[test]
    fn provider_capability_advertisement_does_not_encode_physical_algorithm_choice() {
        let capabilities = ProviderCapabilities::new()
            .with_capability(ProviderCapability::SimilarityLookup)
            .with_capability(ProviderCapability::Ranking);

        assert!(capabilities.supports(ProviderCapability::SimilarityLookup));
        assert!(capabilities.supports(ProviderCapability::Ranking));

        // The provider boundary exposes only logical capabilities. There is no
        // physical algorithm, database type, or execution-plan selector in the
        // advertised capability state.
        let displayed: Vec<String> = capabilities.iter().map(ToString::to_string).collect();

        assert_eq!(
            displayed,
            vec!["similarity-lookup".to_owned(), "ranking".to_owned()]
        );
    }

    #[test]
    fn provider_availability_is_independent_from_capability_advertisement() {
        let capabilities = ProviderCapabilities::with(ProviderCapability::Ranking);

        assert!(capabilities.supports(ProviderCapability::Ranking));
        assert_eq!(
            ProviderAvailability::TemporarilyUnavailable.to_string(),
            "temporarily-unavailable"
        );
        assert_eq!(ProviderAvailability::Available.to_string(), "available");
        assert_eq!(ProviderAvailability::Unavailable.to_string(), "unavailable");
    }

    #[test]
    fn provider_capability_display_is_stable_for_all_logical_capabilities() {
        let values = [
            (ProviderCapability::ExactLookup, "exact-lookup"),
            (ProviderCapability::TextLookup, "text-lookup"),
            (ProviderCapability::StructuredLookup, "structured-lookup"),
            (
                ProviderCapability::NeighborhoodLookup,
                "neighborhood-lookup",
            ),
            (ProviderCapability::SimilarityLookup, "similarity-lookup"),
            (ProviderCapability::FilteredLookup, "filtered-lookup"),
            (ProviderCapability::HybridRetrieval, "hybrid-retrieval"),
            (ProviderCapability::Ranking, "ranking"),
        ];

        for (capability, expected) in values {
            assert_eq!(capability.to_string(), expected);
        }
    }
}
