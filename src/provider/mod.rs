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

pub mod capabilities;
pub mod ranking;

pub use capabilities::{
    ProviderAvailability, ProviderCapabilities, ProviderCapability, ProviderCapabilityError,
    ProviderRankingError, rank_candidates,
};

pub use ranking::{
    RankingCandidate, RankingCandidateValidationError, RankingCriterion, RankingDirection,
    RankingError, RankingPolicy, rank,
};

#[cfg(test)]
mod phase_four_module_tests {
    use super::*;

    #[test]
    fn provider_capability_types_are_publicly_reachable() {
        let capabilities = ProviderCapabilities::with(ProviderCapability::ExactLookup);

        assert!(capabilities.supports(ProviderCapability::ExactLookup));
        assert_eq!(ProviderAvailability::Available.to_string(), "available");
    }

    #[test]
    fn provider_ranking_boundary_is_publicly_reachable() {
        let capabilities = ProviderCapabilities::with(ProviderCapability::Ranking);

        assert!(capabilities.supports(ProviderCapability::Ranking));
        let _: Option<ProviderRankingError> = None;
        let _: Option<RankingPolicy> = None;
    }
}
