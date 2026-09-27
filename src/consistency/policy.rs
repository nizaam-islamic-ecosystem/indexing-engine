//! Query-time consistency policy for Phase 4.
//!
//! This module defines the logical consistency requirement attached to a query
//! and evaluates whether a selected, queryable [`IndexVersion`] can satisfy
//! that requirement.
//!
//! The policy deliberately does not:
//!
//! - locate or maintain the active-version registry;
//! - read an update journal;
//! - compute source/index observations from external systems;
//! - perform query planning or provider retrieval;
//! - own Core runtime, cancellation, deadline, or capability dispatch.
//!
//! The synchronization layer supplies query-time observations and the planner
//! selects candidate versions. This module applies the declared consistency
//! contract to those observations.
//!
//! Phase 4 uses three logical modes:
//!
//! ```text
//! Current
//!     -> current queryable published state with zero update-sequence lag
//!
//! VersionPinned(V)
//!     -> exactly the requested queryable published IndexVersion V
//!
//! StaleAllowed(policy)
//!     -> a queryable published version is acceptable when all declared
//!        freshness constraints are satisfied
//! ```
//!
//! The combined freshness model uses update-sequence lag as its mandatory
//! default dimension. Optional time and source-version constraints are
//! conjunctive: when present, every applicable constraint must pass. No
//! constraint is silently weakened when its required observation is missing.

use core::fmt;
use core::time::Duration;

use crate::index::{IndexVersion, IndexVersionId, SourceVersion, VersionLifecycle};

/// Query-time consistency mode declared by a caller.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ConsistencyMode {
    /// Require the current queryable published version with zero
    /// update-sequence lag relative to the observed source state.
    Current,

    /// Require exactly the supplied published [`IndexVersionId`].
    ///
    /// A pinned version is not automatically replaced by another version when
    /// the requested one is unavailable or cannot be used.
    VersionPinned(IndexVersionId),

    /// Permit an older queryable published version when the supplied combined
    /// freshness policy accepts its observed synchronization state.
    StaleAllowed(FreshnessPolicy),
}

impl ConsistencyMode {
    /// Returns [`ConsistencyMode::Current`].
    #[must_use]
    pub const fn current() -> Self {
        Self::Current
    }

    /// Creates an exact-version consistency requirement.
    #[must_use]
    pub fn version_pinned(version_id: IndexVersionId) -> Self {
        Self::VersionPinned(version_id)
    }

    /// Creates a stale-allowed consistency requirement using the supplied
    /// freshness policy.
    #[must_use]
    pub fn stale_allowed(policy: FreshnessPolicy) -> Self {
        Self::StaleAllowed(policy)
    }

    /// Returns whether this mode requires the current state rather than an
    /// explicitly bounded stale state or an exact pinned version.
    #[must_use]
    pub const fn is_current(&self) -> bool {
        matches!(self, Self::Current)
    }

    /// Returns the pinned version identifier, if this is a pinned request.
    #[must_use]
    pub fn pinned_version(&self) -> Option<&IndexVersionId> {
        match self {
            Self::VersionPinned(version_id) => Some(version_id),
            Self::Current | Self::StaleAllowed(_) => None,
        }
    }

    /// Returns the stale-allowed freshness policy, if present.
    #[must_use]
    pub fn freshness_policy(&self) -> Option<&FreshnessPolicy> {
        match self {
            Self::StaleAllowed(policy) => Some(policy),
            Self::Current | Self::VersionPinned(_) => None,
        }
    }

    /// Returns whether a selected version must be published before it can
    /// satisfy this request.
    ///
    /// All Phase 4 modes return `true`: unpublished candidates are never a
    /// normal query result.
    #[must_use]
    pub const fn requires_published_version(&self) -> bool {
        true
    }

    /// Evaluates this policy against one candidate [`IndexVersion`].
    ///
    /// The caller supplies observations gathered outside this module:
    ///
    /// - `source_update_sequence` is the source sequence observed at query
    ///   time;
    /// - `indexed_update_sequence` is the sequence represented by the selected
    ///   indexed state;
    /// - `indexed_source_version` is the opaque source version represented by the
    ///   selected indexed state, when known;
    /// - `time_lag` is a non-negative elapsed source/index lag computed by the
    ///   synchronization layer.
    ///
    /// A `Current` evaluation does not invent a source-version comparison from
    /// this method alone: when both source and indexed source-version
    /// observations are available, the synchronization boundary rejects a
    /// mismatch before calling this evaluator.
    ///
    /// The candidate must already be known to the caller as the version it is
    /// considering. This method nevertheless enforces the Phase 3 publication
    /// boundary so an unpublished candidate can never satisfy a query policy.
    pub fn evaluate(
        &self,
        version: &IndexVersion,
        lifecycle: VersionLifecycle,
        source_update_sequence: Option<u64>,
        indexed_update_sequence: Option<u64>,
        indexed_source_version: Option<&SourceVersion>,
        time_lag: Option<Duration>,
    ) -> Result<ConsistencyEvaluation, ConsistencyPolicyError> {
        if lifecycle != VersionLifecycle::Published {
            return Err(ConsistencyPolicyError::UnqueryableVersion {
                version: version.id().clone(),
                lifecycle,
            });
        }

        let update_sequence_lag =
            calculate_update_sequence_lag(source_update_sequence, indexed_update_sequence)?;

        match self {
            Self::Current => {
                let lag = require_update_sequence_lag(update_sequence_lag)?;
                if lag != 0 {
                    return Err(ConsistencyPolicyError::CurrentStateNotFresh { lag });
                }

                Ok(ConsistencyEvaluation::new(
                    self.clone(),
                    ConsistencyEvaluationState::Fresh,
                    source_update_sequence,
                    indexed_update_sequence,
                    Some(lag),
                    indexed_source_version.cloned(),
                    time_lag,
                ))
            }

            Self::VersionPinned(requested_version) => {
                if version.id() != requested_version {
                    return Err(ConsistencyPolicyError::PinnedVersionMismatch {
                        requested: requested_version.clone(),
                        actual: version.id().clone(),
                    });
                }

                Ok(ConsistencyEvaluation::new(
                    self.clone(),
                    ConsistencyEvaluationState::VersionPinned,
                    source_update_sequence,
                    indexed_update_sequence,
                    update_sequence_lag,
                    indexed_source_version.cloned(),
                    time_lag,
                ))
            }

            Self::StaleAllowed(policy) => {
                let assessment = policy.evaluate(
                    source_update_sequence,
                    indexed_update_sequence,
                    update_sequence_lag,
                    time_lag,
                    indexed_source_version,
                )?;

                Ok(ConsistencyEvaluation::new(
                    self.clone(),
                    match assessment {
                        FreshnessEvaluation::Fresh => ConsistencyEvaluationState::Fresh,
                        FreshnessEvaluation::StaleAccepted => {
                            ConsistencyEvaluationState::StaleAccepted
                        }
                    },
                    source_update_sequence,
                    indexed_update_sequence,
                    update_sequence_lag,
                    indexed_source_version.cloned(),
                    time_lag,
                ))
            }
        }
    }
}

/// Combined freshness constraints used by [`ConsistencyMode::StaleAllowed`].
///
/// `max_update_sequence_lag` is intentionally mandatory. Update-sequence lag
/// is therefore the default freshness dimension chosen for Phase 4 rather than
/// an optional measurement that can disappear when an observation is missing.
///
/// `max_time_lag` and `required_source_version` are optional additional
/// constraints. When supplied, they are evaluated together with the sequence
/// constraint; all applicable constraints must pass.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct FreshnessPolicy {
    max_update_sequence_lag: u64,
    max_time_lag: Option<Duration>,
    required_source_version: Option<SourceVersion>,
}

impl FreshnessPolicy {
    /// Creates a combined freshness policy with an explicit maximum
    /// update-sequence lag.
    ///
    /// No arbitrary default tolerance is invented here. The caller must
    /// explicitly state how many source updates may be ahead of the selected
    /// indexed state.
    #[must_use]
    pub const fn new(max_update_sequence_lag: u64) -> Self {
        Self {
            max_update_sequence_lag,
            max_time_lag: None,
            required_source_version: None,
        }
    }

    /// Adds a maximum elapsed source/index time lag.
    #[must_use]
    pub const fn with_max_time_lag(mut self, max_time_lag: Duration) -> Self {
        self.max_time_lag = Some(max_time_lag);
        self
    }

    /// Adds a required source-version compatibility constraint.
    #[must_use]
    pub fn with_required_source_version(mut self, source_version: SourceVersion) -> Self {
        self.required_source_version = Some(source_version);
        self
    }

    /// Returns the maximum permitted update-sequence lag.
    #[must_use]
    pub const fn max_update_sequence_lag(&self) -> u64 {
        self.max_update_sequence_lag
    }

    /// Returns the optional maximum permitted time lag.
    #[must_use]
    pub const fn max_time_lag(&self) -> Option<Duration> {
        self.max_time_lag
    }

    /// Returns the optional required source version.
    #[must_use]
    pub fn required_source_version(&self) -> Option<&SourceVersion> {
        self.required_source_version.as_ref()
    }

    /// Evaluates all applicable freshness constraints.
    pub fn evaluate(
        &self,
        source_update_sequence: Option<u64>,
        indexed_update_sequence: Option<u64>,
        update_sequence_lag: Option<u64>,
        time_lag: Option<Duration>,
        indexed_source_version: Option<&SourceVersion>,
    ) -> Result<FreshnessEvaluation, FreshnessPolicyError> {
        let lag = match (
            source_update_sequence,
            indexed_update_sequence,
            update_sequence_lag,
        ) {
            (Some(_), Some(_), Some(lag)) => lag,
            _ => return Err(FreshnessPolicyError::MissingUpdateSequence),
        };

        if lag > self.max_update_sequence_lag {
            return Err(FreshnessPolicyError::UpdateSequenceLagExceeded {
                lag,
                max_lag: self.max_update_sequence_lag,
            });
        }

        if let Some(max_time_lag) = self.max_time_lag {
            let observed = time_lag.ok_or(FreshnessPolicyError::MissingTimeLag)?;
            if observed > max_time_lag {
                return Err(FreshnessPolicyError::TimeLagExceeded {
                    lag: observed,
                    max_lag: max_time_lag,
                });
            }
        }

        if let Some(required_source_version) = &self.required_source_version {
            match indexed_source_version {
                Some(actual) if actual == required_source_version => {}
                Some(actual) => {
                    return Err(FreshnessPolicyError::SourceVersionMismatch {
                        required: required_source_version.clone(),
                        actual: actual.clone(),
                    });
                }
                None => {
                    return Err(FreshnessPolicyError::MissingIndexedSourceVersion {
                        required: required_source_version.clone(),
                    });
                }
            }
        }

        if lag == 0 {
            Ok(FreshnessEvaluation::Fresh)
        } else {
            Ok(FreshnessEvaluation::StaleAccepted)
        }
    }
}

/// Result of evaluating a [`FreshnessPolicy`].
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum FreshnessEvaluation {
    /// The selected version is synchronized to the observed source sequence
    /// and satisfies every additional supplied freshness constraint.
    Fresh,

    /// The selected version is behind the observed source state but remains
    /// within every declared freshness bound.
    StaleAccepted,
}

/// Logical consistency state produced by policy evaluation.
///
/// This is deliberately separate from the result module's
/// `query::result::ConsistencyState`: this enum represents the decision made
/// by the policy, while the query-result type records the corresponding result
/// metadata.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ConsistencyEvaluationState {
    /// The selected version satisfied a fresh/current requirement.
    Fresh,

    /// The selected version was stale but explicitly allowed by policy.
    StaleAccepted,

    /// The exact version requested by a pinned query was selected.
    VersionPinned,
}

/// Successful query-time consistency evaluation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConsistencyEvaluation {
    mode: ConsistencyMode,
    state: ConsistencyEvaluationState,
    source_update_sequence: Option<u64>,
    indexed_update_sequence: Option<u64>,
    update_sequence_lag: Option<u64>,
    indexed_source_version: Option<SourceVersion>,
    time_lag: Option<Duration>,
}

/// Owned components returned by [`ConsistencyEvaluation::into_parts`].
pub type ConsistencyEvaluationParts = (
    ConsistencyMode,
    ConsistencyEvaluationState,
    Option<u64>,
    Option<u64>,
    Option<u64>,
    Option<SourceVersion>,
    Option<Duration>,
);

impl ConsistencyEvaluation {
    fn new(
        mode: ConsistencyMode,
        state: ConsistencyEvaluationState,
        source_update_sequence: Option<u64>,
        indexed_update_sequence: Option<u64>,
        update_sequence_lag: Option<u64>,
        indexed_source_version: Option<SourceVersion>,
        time_lag: Option<Duration>,
    ) -> Self {
        Self {
            mode,
            state,
            source_update_sequence,
            indexed_update_sequence,
            update_sequence_lag,
            indexed_source_version,
            time_lag,
        }
    }

    /// Returns the consistency mode that was evaluated.
    #[must_use]
    pub fn mode(&self) -> &ConsistencyMode {
        &self.mode
    }

    /// Returns the resulting policy state.
    #[must_use]
    pub const fn state(&self) -> ConsistencyEvaluationState {
        self.state
    }

    /// Returns the observed source update sequence, when available.
    #[must_use]
    pub const fn source_update_sequence(&self) -> Option<u64> {
        self.source_update_sequence
    }

    /// Returns the indexed update sequence represented by the selected state,
    /// when available.
    #[must_use]
    pub const fn indexed_update_sequence(&self) -> Option<u64> {
        self.indexed_update_sequence
    }

    /// Returns the calculated source/index update-sequence lag.
    #[must_use]
    pub const fn update_sequence_lag(&self) -> Option<u64> {
        self.update_sequence_lag
    }

    /// Returns the source version represented by the selected index state.
    #[must_use]
    pub fn indexed_source_version(&self) -> Option<&SourceVersion> {
        self.indexed_source_version.as_ref()
    }

    /// Returns the observed source/index time lag, when supplied.
    #[must_use]
    pub const fn time_lag(&self) -> Option<Duration> {
        self.time_lag
    }

    /// Consumes the evaluation into its logical components.
    #[must_use]
    pub fn into_parts(self) -> ConsistencyEvaluationParts {
        (
            self.mode,
            self.state,
            self.source_update_sequence,
            self.indexed_update_sequence,
            self.update_sequence_lag,
            self.indexed_source_version,
            self.time_lag,
        )
    }
}

/// Errors raised while applying query-time consistency requirements.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ConsistencyPolicyError {
    /// Queries may only observe a published Indexing version.
    UnqueryableVersion {
        version: IndexVersionId,
        lifecycle: VersionLifecycle,
    },

    /// A `Current` query could not establish an update-sequence observation.
    MissingUpdateSequence,

    /// The observed selected index is ahead of the observed source sequence.
    IndexedSequenceAhead { source: u64, indexed: u64 },

    /// A `Current` query observed a positive source/index sequence lag.
    CurrentStateNotFresh { lag: u64 },

    /// A `Current` query observed conflicting source-version observations.
    CurrentSourceVersionMismatch {
        source: SourceVersion,
        indexed: SourceVersion,
    },

    /// The selected version is not the version explicitly requested by a
    /// `VersionPinned` query.
    PinnedVersionMismatch {
        requested: IndexVersionId,
        actual: IndexVersionId,
    },

    /// A stale-allowed policy could not establish or satisfy its freshness
    /// requirements.
    Freshness(FreshnessPolicyError),
}

impl fmt::Display for ConsistencyPolicyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnqueryableVersion { version, lifecycle } => write!(
                formatter,
                "index version {version} is not queryable because its lifecycle is {lifecycle:?}"
            ),
            Self::MissingUpdateSequence => formatter
                .write_str("update-sequence observations are required for freshness evaluation"),
            Self::IndexedSequenceAhead { source, indexed } => write!(
                formatter,
                "indexed update sequence {indexed} is ahead of source sequence {source}"
            ),
            Self::CurrentStateNotFresh { lag } => write!(
                formatter,
                "current consistency requires zero update-sequence lag; observed {lag}"
            ),
            Self::CurrentSourceVersionMismatch { source, indexed } => write!(
                formatter,
                "current consistency requires matching source versions; source is {source:?}, indexed is {indexed:?}"
            ),
            Self::PinnedVersionMismatch { requested, actual } => write!(
                formatter,
                "pinned query requested index version {requested}, but selected version is {actual}"
            ),
            Self::Freshness(error) => {
                write!(formatter, "freshness policy rejected index state: {error}")
            }
        }
    }
}

impl std::error::Error for ConsistencyPolicyError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Freshness(error) => Some(error),
            _ => None,
        }
    }
}

/// Errors raised when evaluating the combined freshness policy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FreshnessPolicyError {
    /// Update-sequence freshness cannot be evaluated without both sequence
    /// observations.
    MissingUpdateSequence,

    /// The selected indexed state is ahead of the observed source state.
    IndexedSequenceAhead { source: u64, indexed: u64 },

    /// The selected version exceeds the permitted source/index sequence lag.
    UpdateSequenceLagExceeded { lag: u64, max_lag: u64 },

    /// A time bound was declared but synchronization did not provide a time
    /// lag observation.
    MissingTimeLag,

    /// The observed source/index time lag exceeds the declared maximum.
    TimeLagExceeded { lag: Duration, max_lag: Duration },

    /// A source-version compatibility constraint was declared but the selected
    /// indexed state has no associated source version.
    MissingIndexedSourceVersion { required: SourceVersion },

    /// The selected indexed source version differs from the required one.
    SourceVersionMismatch {
        required: SourceVersion,
        actual: SourceVersion,
    },
}

impl fmt::Display for FreshnessPolicyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingUpdateSequence => formatter.write_str(
                "update-sequence freshness requires both source and indexed observations",
            ),
            Self::IndexedSequenceAhead { source, indexed } => write!(
                formatter,
                "indexed update sequence {indexed} is ahead of source sequence {source}"
            ),
            Self::UpdateSequenceLagExceeded { lag, max_lag } => write!(
                formatter,
                "update-sequence lag {lag} exceeds the permitted maximum {max_lag}"
            ),
            Self::MissingTimeLag => formatter.write_str(
                "a maximum time lag was declared but no time-lag observation was supplied",
            ),
            Self::TimeLagExceeded { lag, max_lag } => write!(
                formatter,
                "time lag {lag:?} exceeds the permitted maximum {max_lag:?}"
            ),
            Self::MissingIndexedSourceVersion { required } => write!(
                formatter,
                "source version {required} is required but the selected indexed state has no source version"
            ),
            Self::SourceVersionMismatch { required, actual } => write!(
                formatter,
                "required source version {required} does not match indexed source version {actual}"
            ),
        }
    }
}

impl std::error::Error for FreshnessPolicyError {}

fn calculate_update_sequence_lag(
    source_update_sequence: Option<u64>,
    indexed_update_sequence: Option<u64>,
) -> Result<Option<u64>, ConsistencyPolicyError> {
    match (source_update_sequence, indexed_update_sequence) {
        (Some(source), Some(indexed)) if indexed > source => {
            Err(ConsistencyPolicyError::IndexedSequenceAhead { source, indexed })
        }
        (Some(source), Some(indexed)) => Ok(Some(source - indexed)),
        _ => Ok(None),
    }
}

fn require_update_sequence_lag(
    update_sequence_lag: Option<u64>,
) -> Result<u64, ConsistencyPolicyError> {
    update_sequence_lag.ok_or(ConsistencyPolicyError::MissingUpdateSequence)
}

impl From<FreshnessPolicyError> for ConsistencyPolicyError {
    fn from(error: FreshnessPolicyError) -> Self {
        Self::Freshness(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::{IndexVersion, IndexVersionId, SourceVersion};

    fn version(value: &str) -> IndexVersion {
        IndexVersion::new(IndexVersionId::new(value).expect("test version ID should be valid"))
    }

    fn source_version(value: &str) -> SourceVersion {
        SourceVersion::new(value).expect("test source version should be valid")
    }

    #[test]
    fn current_requires_published_version_and_zero_sequence_lag() {
        let candidate = version("v1");
        let not_published = ConsistencyMode::Current
            .evaluate(
                &candidate,
                VersionLifecycle::Ready,
                Some(5),
                Some(5),
                candidate.source_version(),
                None,
            )
            .expect_err("a ready candidate must not be queryable");

        assert!(matches!(
            not_published,
            ConsistencyPolicyError::UnqueryableVersion {
                lifecycle: VersionLifecycle::Ready,
                ..
            }
        ));

        let published = ConsistencyMode::Current
            .evaluate(
                &candidate,
                VersionLifecycle::Published,
                Some(5),
                Some(5),
                candidate.source_version(),
                None,
            )
            .expect("zero sequence lag should satisfy current consistency");

        assert_eq!(published.state(), ConsistencyEvaluationState::Fresh);
        assert_eq!(published.update_sequence_lag(), Some(0));
    }

    #[test]
    fn current_rejects_positive_sequence_lag_instead_of_downgrading() {
        let result = ConsistencyMode::Current.evaluate(
            &version("v1"),
            VersionLifecycle::Published,
            Some(7),
            Some(6),
            version("v1").source_version(),
            None,
        );

        assert!(matches!(
            result,
            Err(ConsistencyPolicyError::CurrentStateNotFresh { lag: 1 })
        ));
    }

    #[test]
    fn current_uses_the_supplied_indexed_source_version_in_result_metadata() {
        let indexed = source_version("source-indexed");
        let result = ConsistencyMode::Current
            .evaluate(
                &version("v1"),
                VersionLifecycle::Published,
                Some(7),
                Some(7),
                Some(&indexed),
                None,
            )
            .expect("zero lag with an explicit indexed source version should succeed");

        assert_eq!(result.indexed_source_version(), Some(&indexed));
    }

    #[test]
    fn current_rejects_missing_sequence_observation() {
        let result = ConsistencyMode::Current.evaluate(
            &version("v1"),
            VersionLifecycle::Published,
            Some(7),
            None,
            version("v1").source_version(),
            None,
        );

        assert!(matches!(
            result,
            Err(ConsistencyPolicyError::MissingUpdateSequence)
        ));
    }

    #[test]
    fn pinned_mode_requires_exact_published_version_without_freshness_downgrade() {
        let requested = IndexVersionId::new("v2").expect("test version ID should be valid");
        let mode = ConsistencyMode::VersionPinned(requested.clone());

        let mismatch = mode.evaluate(
            &version("v1"),
            VersionLifecycle::Published,
            Some(10),
            Some(7),
            version("v1").source_version(),
            None,
        );

        assert!(matches!(
            mismatch,
            Err(ConsistencyPolicyError::PinnedVersionMismatch { .. })
        ));

        let evaluation = mode
            .evaluate(
                &version("v2"),
                VersionLifecycle::Published,
                Some(10),
                Some(7),
                version("v2").source_version(),
                None,
            )
            .expect("exact pinned version should be accepted even when behind");

        assert_eq!(
            evaluation.state(),
            ConsistencyEvaluationState::VersionPinned
        );
        assert_eq!(evaluation.update_sequence_lag(), Some(3));
    }

    #[test]
    fn stale_allowed_accepts_zero_lag_as_fresh() {
        let mode = ConsistencyMode::StaleAllowed(FreshnessPolicy::new(3));

        let evaluation = mode
            .evaluate(
                &version("v1"),
                VersionLifecycle::Published,
                Some(9),
                Some(9),
                version("v1").source_version(),
                None,
            )
            .expect("zero lag should satisfy stale-allowed policy");

        assert_eq!(evaluation.state(), ConsistencyEvaluationState::Fresh);
    }

    #[test]
    fn stale_allowed_accepts_bounded_sequence_lag() {
        let mode = ConsistencyMode::StaleAllowed(FreshnessPolicy::new(3));

        let evaluation = mode
            .evaluate(
                &version("v1"),
                VersionLifecycle::Published,
                Some(10),
                Some(8),
                version("v1").source_version(),
                None,
            )
            .expect("lag within the declared bound should be accepted");

        assert_eq!(
            evaluation.state(),
            ConsistencyEvaluationState::StaleAccepted
        );
        assert_eq!(evaluation.update_sequence_lag(), Some(2));
    }

    #[test]
    fn stale_allowed_rejects_sequence_lag_above_bound() {
        let mode = ConsistencyMode::StaleAllowed(FreshnessPolicy::new(1));

        let result = mode.evaluate(
            &version("v1"),
            VersionLifecycle::Published,
            Some(10),
            Some(8),
            version("v1").source_version(),
            None,
        );

        assert!(matches!(
            result,
            Err(ConsistencyPolicyError::Freshness(
                FreshnessPolicyError::UpdateSequenceLagExceeded { lag: 2, max_lag: 1 }
            ))
        ));
    }

    #[test]
    fn combined_policy_requires_all_declared_constraints() {
        let required = source_version("source-v2");
        let policy = FreshnessPolicy::new(2)
            .with_max_time_lag(Duration::from_secs(30))
            .with_required_source_version(required.clone());
        let mode = ConsistencyMode::StaleAllowed(policy);

        let version = IndexVersion::with_metadata(
            IndexVersionId::new("v2").expect("test version ID should be valid"),
            Some(required.clone()),
            None,
            None,
        )
        .expect("test version should be valid");

        let evaluation = mode
            .evaluate(
                &version,
                VersionLifecycle::Published,
                Some(20),
                Some(19),
                version.source_version(),
                Some(Duration::from_secs(10)),
            )
            .expect("all combined freshness constraints should pass");

        assert_eq!(
            evaluation.state(),
            ConsistencyEvaluationState::StaleAccepted
        );
        assert_eq!(evaluation.update_sequence_lag(), Some(1));
        assert_eq!(evaluation.time_lag(), Some(Duration::from_secs(10)));
        assert_eq!(evaluation.indexed_source_version(), Some(&required));
    }

    #[test]
    fn combined_policy_rejects_missing_optional_constraint_observation() {
        let policy = FreshnessPolicy::new(2).with_max_time_lag(Duration::from_secs(30));
        let mode = ConsistencyMode::StaleAllowed(policy);

        let result = mode.evaluate(
            &version("v1"),
            VersionLifecycle::Published,
            Some(20),
            Some(19),
            version("v1").source_version(),
            None,
        );

        assert!(matches!(
            result,
            Err(ConsistencyPolicyError::Freshness(
                FreshnessPolicyError::MissingTimeLag
            ))
        ));
    }

    #[test]
    fn combined_policy_rejects_source_version_mismatch() {
        let policy =
            FreshnessPolicy::new(2).with_required_source_version(source_version("source-v2"));
        let mode = ConsistencyMode::StaleAllowed(policy);

        let indexed_version = IndexVersion::with_metadata(
            IndexVersionId::new("v1").expect("test version ID should be valid"),
            Some(source_version("source-v1")),
            None,
            None,
        )
        .expect("test version should be valid");

        let result = mode.evaluate(
            &indexed_version,
            VersionLifecycle::Published,
            Some(20),
            Some(19),
            indexed_version.source_version(),
            None,
        );

        assert!(matches!(
            result,
            Err(ConsistencyPolicyError::Freshness(
                FreshnessPolicyError::SourceVersionMismatch { .. }
            ))
        ));
    }

    #[test]
    fn indexed_sequence_ahead_of_source_is_invalid() {
        let mode = ConsistencyMode::StaleAllowed(FreshnessPolicy::new(3));

        let result = mode.evaluate(
            &version("v1"),
            VersionLifecycle::Published,
            Some(4),
            Some(5),
            version("v1").source_version(),
            None,
        );

        assert!(matches!(
            result,
            Err(ConsistencyPolicyError::IndexedSequenceAhead {
                source: 4,
                indexed: 5
            })
        ));
    }

    #[test]
    fn policy_constructors_and_accessors_preserve_the_declared_contract() {
        let pinned = IndexVersionId::new("v9").expect("test version ID should be valid");
        let required = source_version("source-v9");
        let stale = FreshnessPolicy::new(4)
            .with_max_time_lag(Duration::from_secs(60))
            .with_required_source_version(required.clone());

        let current = ConsistencyMode::current();
        assert!(current.is_current());
        assert!(current.pinned_version().is_none());
        assert!(current.freshness_policy().is_none());
        assert!(current.requires_published_version());

        let pinned_mode = ConsistencyMode::version_pinned(pinned.clone());
        assert_eq!(pinned_mode.pinned_version(), Some(&pinned));

        let stale_mode = ConsistencyMode::stale_allowed(stale.clone());
        assert_eq!(stale_mode.freshness_policy(), Some(&stale));
        assert!(!stale_mode.is_current());
        assert!(stale_mode.requires_published_version());
        assert_eq!(stale.max_update_sequence_lag(), 4);
        assert_eq!(stale.max_time_lag(), Some(Duration::from_secs(60)));
        assert_eq!(stale.required_source_version(), Some(&required));
    }
}
