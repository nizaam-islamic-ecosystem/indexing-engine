//! Generic provider-side ranking support for Phase 4 query retrieval.
//!
//! This module owns reusable ranking mechanics at the shared provider
//! abstraction boundary. It can order provider-facing retrieval candidates by
//! generic scores, distances, or explicit ordering keys while carrying opaque
//! generic metadata alongside each candidate.
//!
//! The module deliberately does not decide:
//!
//! - domain relevance;
//! - semantic authority;
//! - relationship meaning;
//! - domain-specific ranking policy;
//! - source-object hydration;
//! - physical index algorithms;
//! - database/storage operators;
//! - Core runtime, capability dispatch, cancellation, or deadline behavior.
//!
//! `ObjectReference`, `KeyMaterial`, and the logical ranking data are reused
//! from the existing Indexing contracts. A later retrieval layer can convert
//! ranked candidates into the canonical reference-oriented `QueryHit` result
//! model without introducing a second ranking abstraction.

use core::cmp::Ordering;
use core::fmt;

use crate::index::{KeyMaterial, KeyMaterialValidationError, ObjectReference};

/// The generic value on which provider-side ranking is performed.
///
/// `OrderingKey` is intentionally a logical [`KeyMaterial`] value rather than
/// a physical storage key. This keeps ranking independent of provider
/// implementation details.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RankingCriterion {
    /// Rank by an optional provider-supplied score.
    Score,

    /// Rank by an optional provider-supplied distance.
    Distance,

    /// Rank by an explicit comparable logical ordering key.
    OrderingKey,
}

impl fmt::Display for RankingCriterion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Score => formatter.write_str("score"),
            Self::Distance => formatter.write_str("distance"),
            Self::OrderingKey => formatter.write_str("ordering key"),
        }
    }
}

/// Ordering direction for a selected generic ranking criterion.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum RankingDirection {
    /// Lower values are ordered first.
    Ascending,

    /// Higher values are ordered first for ordered criteria.
    Descending,
}

/// Generic provider-side ranking policy.
///
/// The policy selects one generic criterion and one ordering direction. It does
/// not assign semantic meaning to the selected value.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct RankingPolicy {
    criterion: RankingCriterion,
    direction: RankingDirection,
}

impl RankingPolicy {
    /// Creates a ranking policy from a generic criterion and direction.
    #[must_use]
    pub const fn new(criterion: RankingCriterion, direction: RankingDirection) -> Self {
        Self {
            criterion,
            direction,
        }
    }

    /// Creates descending score ranking.
    #[must_use]
    pub const fn score_descending() -> Self {
        Self::new(RankingCriterion::Score, RankingDirection::Descending)
    }

    /// Creates ascending score ranking.
    #[must_use]
    pub const fn score_ascending() -> Self {
        Self::new(RankingCriterion::Score, RankingDirection::Ascending)
    }

    /// Creates ascending distance ranking.
    #[must_use]
    pub const fn distance_ascending() -> Self {
        Self::new(RankingCriterion::Distance, RankingDirection::Ascending)
    }

    /// Creates descending distance ranking.
    #[must_use]
    pub const fn distance_descending() -> Self {
        Self::new(RankingCriterion::Distance, RankingDirection::Descending)
    }

    /// Creates ascending ordering-key ranking.
    #[must_use]
    pub const fn ordering_key_ascending() -> Self {
        Self::new(RankingCriterion::OrderingKey, RankingDirection::Ascending)
    }

    /// Creates descending ordering-key ranking.
    #[must_use]
    pub const fn ordering_key_descending() -> Self {
        Self::new(RankingCriterion::OrderingKey, RankingDirection::Descending)
    }

    /// Returns the selected generic ranking criterion.
    #[must_use]
    pub const fn criterion(&self) -> RankingCriterion {
        self.criterion
    }

    /// Returns the selected ordering direction.
    #[must_use]
    pub const fn direction(&self) -> RankingDirection {
        self.direction
    }
}

/// One provider-facing reference candidate that can participate in generic
/// ranking.
///
/// The candidate carries optional ranking data without interpreting it. The
/// referenced object remains source-owned and opaque to Indexing.
#[derive(Clone, Debug, PartialEq)]
pub struct RankingCandidate {
    reference: ObjectReference,
    score: Option<f64>,
    distance: Option<f64>,
    ordering_key: Option<KeyMaterial>,
    metadata: Option<KeyMaterial>,
}

impl RankingCandidate {
    /// Creates a reference-only ranking candidate.
    #[must_use]
    pub fn new(reference: ObjectReference) -> Self {
        Self {
            reference,
            score: None,
            distance: None,
            ordering_key: None,
            metadata: None,
        }
    }

    /// Sets an optional score and distance pair.
    ///
    /// Non-finite numeric values are rejected because they cannot participate
    /// in deterministic numeric ordering.
    pub fn with_metrics(
        mut self,
        score: Option<f64>,
        distance: Option<f64>,
    ) -> Result<Self, RankingCandidateValidationError> {
        validate_score(score)?;
        validate_distance(distance)?;

        self.score = score;
        self.distance = distance;
        Ok(self)
    }

    /// Sets an individual score.
    pub fn with_score(mut self, score: f64) -> Result<Self, RankingCandidateValidationError> {
        validate_score(Some(score))?;
        self.score = Some(score);
        Ok(self)
    }

    /// Sets an individual distance.
    pub fn with_distance(mut self, distance: f64) -> Result<Self, RankingCandidateValidationError> {
        validate_distance(Some(distance))?;
        self.distance = Some(distance);
        Ok(self)
    }

    /// Sets the generic comparable ordering key.
    pub fn with_ordering_key(
        mut self,
        ordering_key: KeyMaterial,
    ) -> Result<Self, RankingCandidateValidationError> {
        ordering_key
            .validate()
            .map_err(RankingCandidateValidationError::InvalidOrderingKey)?;
        self.ordering_key = Some(ordering_key);
        Ok(self)
    }

    /// Sets opaque generic ranking metadata.
    ///
    /// Metadata is carried through ranking but is never interpreted as a
    /// semantic relevance signal by this module.
    pub fn with_metadata(
        mut self,
        metadata: KeyMaterial,
    ) -> Result<Self, RankingCandidateValidationError> {
        metadata
            .validate()
            .map_err(RankingCandidateValidationError::InvalidMetadata)?;
        self.metadata = Some(metadata);
        Ok(self)
    }

    /// Returns the source-owned object reference.
    #[must_use]
    pub fn reference(&self) -> &ObjectReference {
        &self.reference
    }

    /// Returns the optional generic score.
    #[must_use]
    pub fn score(&self) -> Option<f64> {
        self.score
    }

    /// Returns the optional generic distance.
    #[must_use]
    pub fn distance(&self) -> Option<f64> {
        self.distance
    }

    /// Returns the optional comparable logical ordering key.
    #[must_use]
    pub fn ordering_key(&self) -> Option<&KeyMaterial> {
        self.ordering_key.as_ref()
    }

    /// Returns opaque generic ranking metadata.
    #[must_use]
    pub fn metadata(&self) -> Option<&KeyMaterial> {
        self.metadata.as_ref()
    }

    /// Validates all optional ranking data carried by the candidate.
    pub fn validate(&self) -> Result<(), RankingCandidateValidationError> {
        validate_score(self.score)?;
        validate_distance(self.distance)?;

        if let Some(ordering_key) = &self.ordering_key {
            ordering_key
                .validate()
                .map_err(RankingCandidateValidationError::InvalidOrderingKey)?;
        }

        if let Some(metadata) = &self.metadata {
            metadata
                .validate()
                .map_err(RankingCandidateValidationError::InvalidMetadata)?;
        }

        Ok(())
    }

    /// Consumes the candidate and returns its logical components.
    #[must_use]
    pub fn into_parts(
        self,
    ) -> (
        ObjectReference,
        Option<f64>,
        Option<f64>,
        Option<KeyMaterial>,
        Option<KeyMaterial>,
    ) {
        (
            self.reference,
            self.score,
            self.distance,
            self.ordering_key,
            self.metadata,
        )
    }

    fn has_criterion(&self, criterion: RankingCriterion) -> bool {
        match criterion {
            RankingCriterion::Score => self.score.is_some(),
            RankingCriterion::Distance => self.distance.is_some(),
            RankingCriterion::OrderingKey => self.ordering_key.is_some(),
        }
    }
}

/// Structural failures in a [`RankingCandidate`].
#[derive(Clone, Debug, PartialEq)]
pub enum RankingCandidateValidationError {
    /// The candidate contains a non-finite score.
    NonFiniteScore { value: f64 },

    /// The candidate contains a non-finite distance.
    NonFiniteDistance { value: f64 },

    /// The candidate contains invalid logical ordering-key material.
    InvalidOrderingKey(KeyMaterialValidationError),

    /// The candidate contains invalid generic ranking metadata.
    InvalidMetadata(KeyMaterialValidationError),
}

impl fmt::Display for RankingCandidateValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonFiniteScore { value } => {
                write!(formatter, "ranking score must be finite, got {value}")
            }
            Self::NonFiniteDistance { value } => {
                write!(formatter, "ranking distance must be finite, got {value}")
            }
            Self::InvalidOrderingKey(error) => {
                write!(formatter, "invalid ranking ordering key: {error}")
            }
            Self::InvalidMetadata(error) => {
                write!(formatter, "invalid ranking metadata: {error}")
            }
        }
    }
}

impl std::error::Error for RankingCandidateValidationError {}

/// Failures returned by the generic ranking operation.
#[derive(Clone, Debug, PartialEq)]
pub enum RankingError {
    /// A candidate contains structurally invalid ranking data.
    InvalidCandidate {
        /// Position of the invalid candidate in the supplied collection.
        position: usize,
        /// Candidate validation failure.
        error: RankingCandidateValidationError,
    },

    /// A candidate does not contain the criterion selected by the ranking
    /// policy. Ranking never silently drops such a candidate.
    MissingCriterion {
        /// Position of the candidate in the supplied collection.
        position: usize,
        /// Criterion required by the ranking policy.
        criterion: RankingCriterion,
        /// Opaque source-owned reference of the affected candidate.
        reference: ObjectReference,
    },
}

impl fmt::Display for RankingError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidCandidate { position, error } => {
                write!(
                    formatter,
                    "invalid ranking candidate at position {position}: {error}"
                )
            }
            Self::MissingCriterion {
                position,
                criterion,
                reference,
            } => write!(
                formatter,
                "ranking candidate at position {position} is missing {criterion} for reference {}:{}",
                reference.source(),
                reference.object_reference()
            ),
        }
    }
}

impl std::error::Error for RankingError {}

/// Ranks provider-facing candidates using one generic criterion.
///
/// All supplied candidates are retained. A missing selected criterion is an
/// explicit error rather than an implicit drop, default value, or conversion
/// to an empty result. Equal primary values are deterministically ordered by
/// the opaque [`ObjectReference`] only as a generic tie-breaker.
pub fn rank<I>(candidates: I, policy: &RankingPolicy) -> Result<Vec<RankingCandidate>, RankingError>
where
    I: IntoIterator<Item = RankingCandidate>,
{
    let mut ranked: Vec<RankingCandidate> = candidates.into_iter().collect();

    for (position, candidate) in ranked.iter().enumerate() {
        candidate
            .validate()
            .map_err(|error| RankingError::InvalidCandidate { position, error })?;

        if !candidate.has_criterion(policy.criterion()) {
            return Err(RankingError::MissingCriterion {
                position,
                criterion: policy.criterion(),
                reference: candidate.reference().clone(),
            });
        }
    }

    ranked.sort_by(|left, right| {
        let mut ordering = compare_candidates(left, right, policy.criterion());

        if policy.direction() == RankingDirection::Descending {
            ordering = ordering.reverse();
        }

        ordering.then_with(|| left.reference().cmp(right.reference()))
    });

    Ok(ranked)
}

fn compare_candidates(
    left: &RankingCandidate,
    right: &RankingCandidate,
    criterion: RankingCriterion,
) -> Ordering {
    match criterion {
        RankingCriterion::Score => left
            .score
            .expect("score criterion was validated before ranking")
            .partial_cmp(
                &right
                    .score
                    .expect("score criterion was validated before ranking"),
            )
            .expect("finite ranking scores must be comparable"),

        RankingCriterion::Distance => left
            .distance
            .expect("distance criterion was validated before ranking")
            .partial_cmp(
                &right
                    .distance
                    .expect("distance criterion was validated before ranking"),
            )
            .expect("finite ranking distances must be comparable"),

        RankingCriterion::OrderingKey => left
            .ordering_key
            .as_ref()
            .expect("ordering-key criterion was validated before ranking")
            .cmp(
                right
                    .ordering_key
                    .as_ref()
                    .expect("ordering-key criterion was validated before ranking"),
            ),
    }
}

fn validate_score(value: Option<f64>) -> Result<(), RankingCandidateValidationError> {
    if let Some(value) = value
        && !value.is_finite()
    {
        return Err(RankingCandidateValidationError::NonFiniteScore { value });
    }

    Ok(())
}

fn validate_distance(value: Option<f64>) -> Result<(), RankingCandidateValidationError> {
    if let Some(value) = value
        && !value.is_finite()
    {
        return Err(RankingCandidateValidationError::NonFiniteDistance { value });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reference(source: &str, value: &str) -> ObjectReference {
        ObjectReference::new(source, value).expect("test reference should be valid")
    }

    fn candidate(source: &str, value: &str, score: f64) -> RankingCandidate {
        RankingCandidate::new(reference(source, value))
            .with_score(score)
            .expect("finite test score should be valid")
    }

    #[test]
    fn score_descending_rank_reorders_candidates_without_dropping_references() {
        let ranked = rank(
            vec![
                candidate("source", "object-c", 0.2),
                candidate("source", "object-a", 0.9),
                candidate("source", "object-b", 0.5),
            ],
            &RankingPolicy::score_descending(),
        )
        .expect("score ranking should succeed");

        let references: Vec<&str> = ranked
            .iter()
            .map(|candidate| candidate.reference().object_reference())
            .collect();

        assert_eq!(references, ["object-a", "object-b", "object-c"]);
        assert_eq!(ranked.len(), 3);
    }

    #[test]
    fn distance_ascending_uses_lower_distance_first() {
        let ranked = rank(
            vec![
                RankingCandidate::new(reference("source", "object-c"))
                    .with_distance(0.8)
                    .expect("finite distance should be valid"),
                RankingCandidate::new(reference("source", "object-a"))
                    .with_distance(0.1)
                    .expect("finite distance should be valid"),
                RankingCandidate::new(reference("source", "object-b"))
                    .with_distance(0.4)
                    .expect("finite distance should be valid"),
            ],
            &RankingPolicy::distance_ascending(),
        )
        .expect("distance ranking should succeed");

        let distances: Vec<f64> = ranked
            .iter()
            .map(|candidate| candidate.distance().expect("ranked candidate has distance"))
            .collect();

        assert_eq!(distances, [0.1, 0.4, 0.8]);
    }

    #[test]
    fn logical_ordering_key_supports_generic_non_numeric_ranking() {
        let ranked = rank(
            vec![
                RankingCandidate::new(reference("source", "object-c"))
                    .with_ordering_key(KeyMaterial::Unsigned(30))
                    .expect("logical ordering key should be valid"),
                RankingCandidate::new(reference("source", "object-a"))
                    .with_ordering_key(KeyMaterial::Unsigned(10))
                    .expect("logical ordering key should be valid"),
                RankingCandidate::new(reference("source", "object-b"))
                    .with_ordering_key(KeyMaterial::Unsigned(20))
                    .expect("logical ordering key should be valid"),
            ],
            &RankingPolicy::ordering_key_ascending(),
        )
        .expect("ordering-key ranking should succeed");

        let keys: Vec<&KeyMaterial> = ranked
            .iter()
            .map(|candidate| candidate.ordering_key().expect("ranked candidate has key"))
            .collect();

        assert_eq!(keys[0], &KeyMaterial::Unsigned(10));
        assert_eq!(keys[1], &KeyMaterial::Unsigned(20));
        assert_eq!(keys[2], &KeyMaterial::Unsigned(30));
    }

    #[test]
    fn equal_primary_values_use_reference_as_a_deterministic_generic_tie_breaker() {
        let ranked = rank(
            vec![
                candidate("source", "object-b", 1.0),
                candidate("source", "object-c", 1.0),
                candidate("source", "object-a", 1.0),
            ],
            &RankingPolicy::score_descending(),
        )
        .expect("tie-ranked candidates should still succeed");

        let references: Vec<&str> = ranked
            .iter()
            .map(|candidate| candidate.reference().object_reference())
            .collect();

        assert_eq!(references, ["object-a", "object-b", "object-c"]);
    }

    #[test]
    fn missing_selected_criterion_is_an_explicit_error() {
        let candidates = vec![
            candidate("source", "object-a", 1.0),
            RankingCandidate::new(reference("source", "object-b")),
        ];

        let error = rank(candidates, &RankingPolicy::score_descending())
            .expect_err("missing score must not be silently ignored");

        assert!(matches!(
            error,
            RankingError::MissingCriterion {
                position: 1,
                criterion: RankingCriterion::Score,
                ..
            }
        ));
    }

    #[test]
    fn invalid_numeric_metrics_are_rejected_before_ranking() {
        let score_error = RankingCandidate::new(reference("source", "score-nan"))
            .with_score(f64::NAN)
            .expect_err("NaN score must be rejected");

        assert!(matches!(
            score_error,
            RankingCandidateValidationError::NonFiniteScore { .. }
        ));

        let distance_error = RankingCandidate::new(reference("source", "distance-inf"))
            .with_distance(f64::INFINITY)
            .expect_err("infinite distance must be rejected");

        assert!(matches!(
            distance_error,
            RankingCandidateValidationError::NonFiniteDistance { .. }
        ));
    }

    #[test]
    fn generic_metadata_is_preserved_but_does_not_change_score_ordering() {
        let first = candidate("source", "object-a", 0.8)
            .with_metadata(
                KeyMaterial::map([("label", KeyMaterial::text("domain-a"))])
                    .expect("metadata should be valid"),
            )
            .expect("metadata should be attached");

        let second = candidate("source", "object-b", 0.2)
            .with_metadata(
                KeyMaterial::map([("label", KeyMaterial::text("domain-z"))])
                    .expect("metadata should be valid"),
            )
            .expect("metadata should be attached");

        let ranked = rank(vec![second, first], &RankingPolicy::score_descending())
            .expect("score ranking should ignore metadata semantics");

        assert_eq!(
            ranked[0].metadata(),
            Some(
                &KeyMaterial::map([("label", KeyMaterial::text("domain-a"))])
                    .expect("metadata should be valid"),
            )
        );
        assert_eq!(ranked[0].reference().object_reference(), "object-a");
        assert_eq!(ranked[1].reference().object_reference(), "object-b");
    }

    #[test]
    fn candidate_validation_is_independent_of_a_selected_ranking_policy() {
        let candidate = RankingCandidate::new(reference("source", "object-a"))
            .with_metadata(KeyMaterial::text("opaque-metadata"))
            .expect("metadata should be valid");

        candidate
            .validate()
            .expect("a candidate may be structurally valid before a ranking criterion is chosen");

        let error = rank(vec![candidate], &RankingPolicy::score_descending())
            .expect_err("ranking must still require the selected score");

        assert!(matches!(
            error,
            RankingError::MissingCriterion {
                criterion: RankingCriterion::Score,
                ..
            }
        ));
    }

    #[test]
    fn ranking_preserves_reference_metrics_and_internal_ordering_key() {
        let original = RankingCandidate::new(reference("source", "object-a"))
            .with_metrics(Some(0.75), Some(0.25))
            .expect("finite metrics should be valid")
            .with_ordering_key(KeyMaterial::Text("rank-key".to_owned()))
            .expect("ordering key should be valid")
            .with_metadata(KeyMaterial::Unsigned(42))
            .expect("metadata should be valid");

        let parts = original.clone().into_parts();
        assert_eq!(parts.1, Some(0.75));
        assert_eq!(parts.2, Some(0.25));
        assert_eq!(parts.3, Some(KeyMaterial::Text("rank-key".to_owned())));
        assert_eq!(parts.4, Some(KeyMaterial::Unsigned(42)));

        let ranked = rank(vec![original], &RankingPolicy::score_descending())
            .expect("a fully populated candidate should rank successfully");

        assert_eq!(ranked[0].score(), Some(0.75));
        assert_eq!(ranked[0].distance(), Some(0.25));
        assert_eq!(
            ranked[0].ordering_key(),
            Some(&KeyMaterial::Text("rank-key".to_owned()))
        );
        assert_eq!(ranked[0].reference(), &reference("source", "object-a"));
    }

    #[test]
    fn ranking_remains_independent_of_source_semantics() {
        let ranked = rank(
            vec![
                candidate("source-b", "opaque-2", 0.4),
                candidate("source-a", "opaque-1", 0.4),
            ],
            &RankingPolicy::score_descending(),
        )
        .expect("generic score ranking should not require source semantics");

        assert_eq!(ranked.len(), 2);
        assert_eq!(ranked[0].score(), Some(0.4));
        assert_eq!(ranked[1].score(), Some(0.4));
        assert_eq!(ranked[0].reference().source(), "source-a");
        assert_eq!(ranked[1].reference().source(), "source-b");
    }
}
