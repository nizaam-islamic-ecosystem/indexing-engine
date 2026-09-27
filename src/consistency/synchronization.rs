//! Query-time source/index synchronization observations for Phase 4.
//!
//! This module answers a deliberately narrow question:
//!
//! ```text
//! What synchronization facts are known about the source state and the
//! indexed state being considered for a query?
//! ```
//!
//! It does **not** decide whether those facts are acceptable for a query. That
//! decision belongs to [`crate::consistency::policy`]. The synchronization
//! layer therefore measures/records facts such as:
//!
//! - source and indexed [`UpdateSequence`] observations;
//! - the derived source/index sequence lag;
//! - an optional already-computed source/index time lag;
//! - optional source-version observations.
//!
//! The separation is important because the Phase 3 update/rebuild machinery
//! already owns mutation and replay behavior. Phase 4 only consumes its
//! synchronization primitives at query time; it does not become a second
//! update engine.

use core::fmt;
use core::time::Duration;

use crate::build::UpdateSequence;
use crate::consistency::policy::{ConsistencyEvaluation, ConsistencyMode, ConsistencyPolicyError};
use crate::index::{IndexVersion, SourceVersion, VersionLifecycle};

/// A query-time snapshot of source/index synchronization facts.
///
/// `source_update_sequence` is the source state observed for the query.
/// `indexed_update_sequence` is the update sequence known to be represented by
/// the indexed state being considered. The synchronization layer does not
/// discover either value itself; the owner of the source/index state supplies
/// them.
///
/// `time_lag` is an optional elapsed source/index lag computed by the caller.
/// This module deliberately does not invent a timestamp interpretation because
/// the Phase 0–3 codebase does not attach source/index timestamps to
/// `IndexVersion`.
///
/// `source_version` and `indexed_source_version` are opaque source-version
/// observations. Their meaning remains source-owned. This module only exposes
/// whether the two supplied values are equal when both are present.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SynchronizationSnapshot {
    source_update_sequence: Option<UpdateSequence>,
    indexed_update_sequence: Option<UpdateSequence>,
    time_lag: Option<Duration>,
    source_version: Option<SourceVersion>,
    indexed_source_version: Option<SourceVersion>,
}

/// Owned components returned by [`SynchronizationSnapshot::into_parts`].
pub type SynchronizationSnapshotParts = (
    Option<UpdateSequence>,
    Option<UpdateSequence>,
    Option<Duration>,
    Option<SourceVersion>,
    Option<SourceVersion>,
);

/// Owned components returned by [`SynchronizationEvaluation::into_parts`].
pub type SynchronizationEvaluationParts = (
    IndexSynchronizationState,
    SourceVersionSynchronizationState,
    Option<UpdateSequence>,
    Option<UpdateSequence>,
    Option<u64>,
    Option<Duration>,
    Option<SourceVersion>,
    Option<SourceVersion>,
);

impl SynchronizationSnapshot {
    /// Creates a synchronization snapshot from optional source/index update
    /// sequence observations.
    ///
    /// An indexed sequence greater than the source sequence is rejected because
    /// such an observation cannot represent a valid source/index lag boundary.
    pub fn new(
        source_update_sequence: Option<UpdateSequence>,
        indexed_update_sequence: Option<UpdateSequence>,
    ) -> Result<Self, SynchronizationError> {
        let snapshot = Self {
            source_update_sequence,
            indexed_update_sequence,
            time_lag: None,
            source_version: None,
            indexed_source_version: None,
        };

        snapshot.validate()?;
        Ok(snapshot)
    }

    /// Creates a synchronized observation using both concrete update
    /// sequences.
    pub fn from_sequences(
        source_update_sequence: UpdateSequence,
        indexed_update_sequence: UpdateSequence,
    ) -> Result<Self, SynchronizationError> {
        Self::new(Some(source_update_sequence), Some(indexed_update_sequence))
    }

    /// Adds an already-computed non-negative source/index time lag.
    #[must_use]
    pub const fn with_time_lag(mut self, time_lag: Duration) -> Self {
        self.time_lag = Some(time_lag);
        self
    }

    /// Adds the source version observed at query time.
    #[must_use]
    pub fn with_source_version(mut self, source_version: SourceVersion) -> Self {
        self.source_version = Some(source_version);
        self
    }

    /// Adds the source version represented by the indexed state.
    #[must_use]
    pub fn with_indexed_source_version(mut self, indexed_source_version: SourceVersion) -> Self {
        self.indexed_source_version = Some(indexed_source_version);
        self
    }

    /// Adds both source/index source-version observations.
    #[must_use]
    pub fn with_source_versions(
        mut self,
        source_version: Option<SourceVersion>,
        indexed_source_version: Option<SourceVersion>,
    ) -> Self {
        self.source_version = source_version;
        self.indexed_source_version = indexed_source_version;
        self
    }

    /// Validates the snapshot's synchronization invariants.
    ///
    /// Missing sequence observations are allowed because a pinned query may be
    /// evaluated without freshness semantics. The Phase 4 policy decides when
    /// a missing observation makes a particular consistency mode unsatisfied.
    pub fn validate(&self) -> Result<(), SynchronizationError> {
        if let (Some(source), Some(indexed)) =
            (self.source_update_sequence, self.indexed_update_sequence)
            && indexed.value() > source.value()
        {
            return Err(SynchronizationError::IndexedSequenceAhead { source, indexed });
        }

        Ok(())
    }

    /// Returns the source update sequence observed for the query.
    #[must_use]
    pub const fn source_update_sequence(&self) -> Option<UpdateSequence> {
        self.source_update_sequence
    }

    /// Returns the update sequence represented by the indexed state.
    #[must_use]
    pub const fn indexed_update_sequence(&self) -> Option<UpdateSequence> {
        self.indexed_update_sequence
    }

    /// Returns the numeric source/index sequence lag when both observations
    /// are available.
    #[must_use]
    pub const fn update_sequence_lag(&self) -> Option<u64> {
        match (self.source_update_sequence, self.indexed_update_sequence) {
            (Some(source), Some(indexed)) => Some(source.value() - indexed.value()),
            _ => None,
        }
    }

    /// Returns whether a numeric sequence lag can be established.
    #[must_use]
    pub const fn has_update_sequence_observation(&self) -> bool {
        self.source_update_sequence.is_some() && self.indexed_update_sequence.is_some()
    }

    /// Returns the optional observed source/index time lag.
    #[must_use]
    pub const fn time_lag(&self) -> Option<Duration> {
        self.time_lag
    }

    /// Returns the source version observed at query time, if available.
    #[must_use]
    pub fn source_version(&self) -> Option<&SourceVersion> {
        self.source_version.as_ref()
    }

    /// Returns the source version represented by the indexed state, if
    /// available.
    #[must_use]
    pub fn indexed_source_version(&self) -> Option<&SourceVersion> {
        self.indexed_source_version.as_ref()
    }

    /// Compares the optional source-version observations without interpreting
    /// their ordering or semantics.
    #[must_use]
    pub fn source_version_state(&self) -> SourceVersionSynchronizationState {
        match (&self.source_version, &self.indexed_source_version) {
            (Some(source), Some(indexed)) if source == indexed => {
                SourceVersionSynchronizationState::Matching
            }
            (Some(_), Some(_)) => SourceVersionSynchronizationState::Mismatched,
            _ => SourceVersionSynchronizationState::Unknown,
        }
    }

    /// Returns the coarse query-time synchronization state derived only from
    /// update-sequence observations.
    #[must_use]
    pub const fn state(&self) -> IndexSynchronizationState {
        match self.update_sequence_lag() {
            Some(0) => IndexSynchronizationState::Synchronized,
            Some(lag) => IndexSynchronizationState::Lagged { lag },
            None => IndexSynchronizationState::Unknown,
        }
    }

    /// Produces a factual synchronization evaluation snapshot.
    #[must_use]
    pub fn evaluate(&self) -> SynchronizationEvaluation {
        SynchronizationEvaluation::from_snapshot(self)
    }

    /// Applies a declared Phase 4 consistency policy using these synchronization
    /// observations.
    ///
    /// This is only a convenience bridge. The acceptance decision itself is
    /// still implemented by [`ConsistencyMode`]; this module supplies the
    /// observations and does not duplicate policy rules.
    ///
    /// A `Current` request is rejected when both source-version observations are
    /// present and differ. For the policy evaluator, the indexed source version
    /// comes from the snapshot when available and otherwise falls back to the
    /// source version recorded on the selected `IndexVersion`.
    pub fn evaluate_with_policy(
        &self,
        mode: &ConsistencyMode,
        version: &IndexVersion,
        lifecycle: VersionLifecycle,
    ) -> Result<ConsistencyEvaluation, ConsistencyPolicyError> {
        if mode.is_current()
            && matches!(
                self.source_version_state(),
                SourceVersionSynchronizationState::Mismatched
            )
        {
            let source = self
                .source_version
                .clone()
                .expect("mismatched source-version state requires a source version");
            let indexed = self
                .indexed_source_version
                .clone()
                .expect("mismatched source-version state requires an indexed source version");

            return Err(ConsistencyPolicyError::CurrentSourceVersionMismatch { source, indexed });
        }

        let indexed_source_version = self
            .indexed_source_version
            .as_ref()
            .or_else(|| version.source_version());

        mode.evaluate(
            version,
            lifecycle,
            self.source_update_sequence.map(UpdateSequence::value),
            self.indexed_update_sequence.map(UpdateSequence::value),
            indexed_source_version,
            self.time_lag,
        )
    }

    /// Consumes the snapshot into its logical observations.
    #[must_use]
    pub fn into_parts(self) -> SynchronizationSnapshotParts {
        (
            self.source_update_sequence,
            self.indexed_update_sequence,
            self.time_lag,
            self.source_version,
            self.indexed_source_version,
        )
    }
}

/// Coarse source/index synchronization state derived from update-sequence
/// observations.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum IndexSynchronizationState {
    /// Source and indexed state are at the same observed update sequence.
    Synchronized,

    /// The indexed state is behind the source by the supplied sequence lag.
    Lagged { lag: u64 },

    /// A sequence comparison cannot be established because one or both
    /// observations are unavailable.
    Unknown,
}

impl IndexSynchronizationState {
    /// Returns whether the state is synchronized at zero sequence lag.
    #[must_use]
    pub const fn is_synchronized(self) -> bool {
        matches!(self, Self::Synchronized)
    }

    /// Returns whether the state is known to be behind the observed source.
    #[must_use]
    pub const fn is_lagged(self) -> bool {
        matches!(self, Self::Lagged { .. })
    }

    /// Returns the known update-sequence lag, if available.
    #[must_use]
    pub const fn lag(self) -> Option<u64> {
        match self {
            Self::Synchronized => Some(0),
            Self::Lagged { lag } => Some(lag),
            Self::Unknown => None,
        }
    }
}

/// Factual state of optional source-version synchronization observations.
///
/// The values are intentionally equality-based. Indexing does not assume that
/// one opaque source-version string is newer than another.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SourceVersionSynchronizationState {
    /// Both observations exist and are exactly equal.
    Matching,

    /// Both observations exist but differ.
    Mismatched,

    /// One or both source-version observations are unavailable.
    Unknown,
}

impl SourceVersionSynchronizationState {
    /// Returns whether both source-version observations are present and equal.
    #[must_use]
    pub const fn is_matching(self) -> bool {
        matches!(self, Self::Matching)
    }

    /// Returns whether both observations are present but unequal.
    #[must_use]
    pub const fn is_mismatched(self) -> bool {
        matches!(self, Self::Mismatched)
    }
}

/// A copyable factual summary of query-time synchronization observations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SynchronizationEvaluation {
    state: IndexSynchronizationState,
    source_version_state: SourceVersionSynchronizationState,
    source_update_sequence: Option<UpdateSequence>,
    indexed_update_sequence: Option<UpdateSequence>,
    update_sequence_lag: Option<u64>,
    time_lag: Option<Duration>,
    source_version: Option<SourceVersion>,
    indexed_source_version: Option<SourceVersion>,
}

impl SynchronizationEvaluation {
    fn from_snapshot(snapshot: &SynchronizationSnapshot) -> Self {
        Self {
            state: snapshot.state(),
            source_version_state: snapshot.source_version_state(),
            source_update_sequence: snapshot.source_update_sequence,
            indexed_update_sequence: snapshot.indexed_update_sequence,
            update_sequence_lag: snapshot.update_sequence_lag(),
            time_lag: snapshot.time_lag,
            source_version: snapshot.source_version.clone(),
            indexed_source_version: snapshot.indexed_source_version.clone(),
        }
    }

    /// Returns the coarse sequence synchronization state.
    #[must_use]
    pub const fn state(&self) -> IndexSynchronizationState {
        self.state
    }

    /// Returns the source-version comparison state.
    #[must_use]
    pub const fn source_version_state(&self) -> SourceVersionSynchronizationState {
        self.source_version_state
    }

    /// Returns the source update sequence.
    #[must_use]
    pub const fn source_update_sequence(&self) -> Option<UpdateSequence> {
        self.source_update_sequence
    }

    /// Returns the indexed update sequence.
    #[must_use]
    pub const fn indexed_update_sequence(&self) -> Option<UpdateSequence> {
        self.indexed_update_sequence
    }

    /// Returns the calculated update-sequence lag.
    #[must_use]
    pub const fn update_sequence_lag(&self) -> Option<u64> {
        self.update_sequence_lag
    }

    /// Returns the optional observed time lag.
    #[must_use]
    pub const fn time_lag(&self) -> Option<Duration> {
        self.time_lag
    }

    /// Returns the source version observed at query time.
    #[must_use]
    pub fn source_version(&self) -> Option<&SourceVersion> {
        self.source_version.as_ref()
    }

    /// Returns the indexed source version represented by the selected state.
    #[must_use]
    pub fn indexed_source_version(&self) -> Option<&SourceVersion> {
        self.indexed_source_version.as_ref()
    }

    /// Returns the evaluation as the synchronization-state tuple.
    #[must_use]
    pub fn into_parts(self) -> SynchronizationEvaluationParts {
        (
            self.state,
            self.source_version_state,
            self.source_update_sequence,
            self.indexed_update_sequence,
            self.update_sequence_lag,
            self.time_lag,
            self.source_version,
            self.indexed_source_version,
        )
    }
}

/// Structural failures for synchronization observations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SynchronizationError {
    /// The indexed observation claims to be ahead of the source observation.
    IndexedSequenceAhead {
        source: UpdateSequence,
        indexed: UpdateSequence,
    },
}

impl fmt::Display for SynchronizationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::IndexedSequenceAhead { source, indexed } => write!(
                formatter,
                "indexed update sequence {indexed} is ahead of source sequence {source}"
            ),
        }
    }
}

impl std::error::Error for SynchronizationError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::consistency::policy::{ConsistencyMode, FreshnessPolicy};
    use crate::index::IndexVersionId;

    fn sequence(value: u64) -> UpdateSequence {
        UpdateSequence::new(value)
    }

    fn version(value: &str) -> IndexVersion {
        IndexVersion::new(IndexVersionId::new(value).expect("test version ID should be valid"))
    }

    fn source_version(value: &str) -> SourceVersion {
        SourceVersion::new(value).expect("test source version should be valid")
    }

    #[test]
    fn zero_lag_is_synchronized() {
        let snapshot = SynchronizationSnapshot::from_sequences(sequence(10), sequence(10))
            .expect("equal sequences should be valid");

        assert_eq!(snapshot.state(), IndexSynchronizationState::Synchronized);
        assert_eq!(snapshot.update_sequence_lag(), Some(0));
        assert!(snapshot.has_update_sequence_observation());
    }

    #[test]
    fn positive_lag_is_explicitly_reported() {
        let snapshot = SynchronizationSnapshot::from_sequences(sequence(12), sequence(9))
            .expect("indexed sequence behind source should be valid");

        assert_eq!(
            snapshot.state(),
            IndexSynchronizationState::Lagged { lag: 3 }
        );
        assert_eq!(snapshot.update_sequence_lag(), Some(3));
    }

    #[test]
    fn missing_sequence_observation_is_unknown_not_invalid() {
        let snapshot = SynchronizationSnapshot::new(Some(sequence(12)), None)
            .expect("partial observation should remain representable");

        assert_eq!(snapshot.state(), IndexSynchronizationState::Unknown);
        assert_eq!(snapshot.update_sequence_lag(), None);
        assert!(!snapshot.has_update_sequence_observation());
    }

    #[test]
    fn indexed_sequence_ahead_is_rejected() {
        let error = SynchronizationSnapshot::from_sequences(sequence(4), sequence(5))
            .expect_err("indexed state must not be ahead of source observation");

        assert_eq!(
            error,
            SynchronizationError::IndexedSequenceAhead {
                source: sequence(4),
                indexed: sequence(5),
            }
        );
    }

    #[test]
    fn source_version_state_is_equality_based() {
        let matching = SynchronizationSnapshot::new(None, None)
            .expect("empty sequence observations are valid")
            .with_source_versions(Some(source_version("v1")), Some(source_version("v1")));
        assert_eq!(
            matching.source_version_state(),
            SourceVersionSynchronizationState::Matching
        );

        let mismatched = SynchronizationSnapshot::new(None, None)
            .expect("empty sequence observations are valid")
            .with_source_versions(Some(source_version("v1")), Some(source_version("v2")));
        assert_eq!(
            mismatched.source_version_state(),
            SourceVersionSynchronizationState::Mismatched
        );

        let unknown = SynchronizationSnapshot::new(None, None)
            .expect("empty sequence observations are valid");
        assert_eq!(
            unknown.source_version_state(),
            SourceVersionSynchronizationState::Unknown
        );
    }

    #[test]
    fn time_lag_is_recorded_without_inventing_timestamp_semantics() {
        let snapshot = SynchronizationSnapshot::from_sequences(sequence(8), sequence(7))
            .expect("sequence observations should be valid")
            .with_time_lag(Duration::from_secs(3));

        assert_eq!(snapshot.time_lag(), Some(Duration::from_secs(3)));
    }

    #[test]
    fn evaluate_with_policy_falls_back_to_index_version_source_version() {
        let snapshot = SynchronizationSnapshot::from_sequences(sequence(10), sequence(8))
            .expect("lagged sequences should be valid");

        let version = IndexVersion::with_metadata(
            IndexVersionId::new("v1").expect("test version ID should be valid"),
            Some(source_version("source-v1")),
            None,
            None,
        )
        .expect("test version should be valid");

        let policy = ConsistencyMode::stale_allowed(
            FreshnessPolicy::new(2).with_required_source_version(source_version("source-v1")),
        );

        let evaluation = snapshot
            .evaluate_with_policy(&policy, &version, VersionLifecycle::Published)
            .expect("index version source metadata should satisfy the source-version requirement");

        assert_eq!(
            evaluation.indexed_source_version(),
            Some(&source_version("source-v1"))
        );
    }

    #[test]
    fn evaluate_with_policy_uses_snapshot_indexed_source_version_for_required_constraint() {
        let snapshot = SynchronizationSnapshot::from_sequences(sequence(10), sequence(8))
            .expect("lagged sequences should be valid")
            .with_source_versions(
                Some(source_version("source-v2")),
                Some(source_version("source-v2")),
            );

        let version = IndexVersion::with_metadata(
            IndexVersionId::new("v2").expect("test version ID should be valid"),
            Some(source_version("source-v1")),
            None,
            None,
        )
        .expect("test version should be valid");

        let mode = ConsistencyMode::stale_allowed(
            FreshnessPolicy::new(2).with_required_source_version(source_version("source-v2")),
        );

        let evaluation = snapshot
            .evaluate_with_policy(&mode, &version, VersionLifecycle::Published)
            .expect("snapshot indexed source version should satisfy the required version");

        assert_eq!(
            evaluation.indexed_source_version(),
            Some(&source_version("source-v2"))
        );
        assert_eq!(evaluation.update_sequence_lag(), Some(2));
    }

    #[test]
    fn evaluation_captures_all_factual_observations() {
        let snapshot = SynchronizationSnapshot::from_sequences(sequence(15), sequence(13))
            .expect("sequence observations should be valid")
            .with_time_lag(Duration::from_millis(250))
            .with_source_versions(
                Some(source_version("source-v2")),
                Some(source_version("source-v1")),
            );

        let evaluation = snapshot.evaluate();

        assert_eq!(
            evaluation.state(),
            IndexSynchronizationState::Lagged { lag: 2 }
        );
        assert_eq!(
            evaluation.source_version_state(),
            SourceVersionSynchronizationState::Mismatched
        );
        assert_eq!(evaluation.update_sequence_lag(), Some(2));
        assert_eq!(evaluation.time_lag(), Some(Duration::from_millis(250)));
        assert_eq!(
            evaluation.source_version().map(SourceVersion::as_str),
            Some("source-v2")
        );
        assert_eq!(
            evaluation
                .indexed_source_version()
                .map(SourceVersion::as_str),
            Some("source-v1")
        );
    }

    #[test]
    fn current_policy_can_consume_synchronized_snapshot() {
        let snapshot = SynchronizationSnapshot::from_sequences(sequence(20), sequence(20))
            .expect("equal sequences should be valid");

        let result = snapshot.evaluate_with_policy(
            &ConsistencyMode::Current,
            &version("v1"),
            VersionLifecycle::Published,
        );

        let evaluation = result.expect("zero-lag published state should satisfy Current");
        assert_eq!(evaluation.update_sequence_lag(), Some(0));
    }

    #[test]
    fn current_policy_rejects_mismatched_source_versions() {
        let snapshot = SynchronizationSnapshot::from_sequences(sequence(10), sequence(10))
            .expect("equal sequences should be valid")
            .with_source_versions(
                Some(source_version("source-v2")),
                Some(source_version("source-v1")),
            );

        let error = snapshot
            .evaluate_with_policy(
                &ConsistencyMode::Current,
                &version("v1"),
                VersionLifecycle::Published,
            )
            .expect_err("Current must reject conflicting source-version observations");

        assert_eq!(
            error,
            ConsistencyPolicyError::CurrentSourceVersionMismatch {
                source: source_version("source-v2"),
                indexed: source_version("source-v1"),
            }
        );
    }

    #[test]
    fn current_policy_rejects_lagged_snapshot() {
        let snapshot = SynchronizationSnapshot::from_sequences(sequence(20), sequence(19))
            .expect("indexed sequence behind source should be valid");

        let error = snapshot
            .evaluate_with_policy(
                &ConsistencyMode::Current,
                &version("v1"),
                VersionLifecycle::Published,
            )
            .expect_err("positive lag must not satisfy Current");

        assert!(matches!(
            error,
            ConsistencyPolicyError::CurrentStateNotFresh { lag: 1 }
        ));
    }

    #[test]
    fn stale_allowed_policy_consumes_sequence_and_time_constraints() {
        let snapshot = SynchronizationSnapshot::from_sequences(sequence(20), sequence(18))
            .expect("indexed sequence behind source should be valid")
            .with_time_lag(Duration::from_secs(2));

        let policy = FreshnessPolicy::new(2).with_max_time_lag(Duration::from_secs(3));

        let result = snapshot.evaluate_with_policy(
            &ConsistencyMode::StaleAllowed(policy),
            &version("v1"),
            VersionLifecycle::Published,
        );

        let evaluation = result.expect("all declared freshness constraints should pass");
        assert_eq!(evaluation.update_sequence_lag(), Some(2));
    }
}
