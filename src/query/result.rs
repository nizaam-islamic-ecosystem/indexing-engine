//! Canonical reference-oriented query-result contracts for Phase 4.
//!
//! This module defines **what Indexing returns** after a logical query has
//! been executed. Results remain provider-neutral and source-reference-only:
//! they identify matching source-owned objects and the logical index/version
//! from which those matches were produced, without hydrating or interpreting
//! the source objects.
//!
//! The module deliberately does not implement:
//!
//! - query planning or index selection;
//! - provider selection or physical retrieval;
//! - domain-object hydration;
//! - domain-specific relevance/ranking;
//! - Core runtime/capability dispatch;
//! - consistency evaluation.
//!
//! Phase 2 originally exposed `QueryHit` and `QueryResult` from
//! `index/query.rs`. Phase 4 promotes them to this canonical result boundary.
//! The old module can re-export these types for compatibility without keeping a
//! second implementation.

use core::fmt;

use crate::consistency::policy::ConsistencyMode;
use crate::identity::IndexDefinitionIdentity;
use crate::index::{
    IndexVersion, IndexVersionValidationError, KeyMaterial, KeyMaterialValidationError,
    ObjectReference,
};

/// The consistency state represented by a successful [`QueryResult`].
///
/// This is result metadata, not a policy evaluator. The consistency subsystem
/// decides whether a version satisfies the caller's requested policy; this enum
/// records the resulting logical state alongside the returned references.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ConsistencyState {
    /// The selected queryable version satisfied the applicable freshness
    /// requirement. This may be used by a `Current` query or by a
    /// `StaleAllowed` query whose selected version is still fresh.
    Fresh,

    /// The selected queryable version was older than the current source state
    /// but was explicitly accepted by the requested stale-allowed policy.
    StaleAccepted,

    /// The result was produced from the explicitly requested pinned version.
    /// The synchronization facts in the same metadata value describe whether
    /// that pinned version was also current or behind the source.
    VersionPinned,
}

/// Query-time consistency metadata attached to a result.
///
/// This value is intentionally factual rather than prescriptive. It records
/// the declared consistency mode and the synchronization information known at
/// result construction time. The consistency subsystem remains responsible for
/// deciding whether these facts satisfy the request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConsistencyMetadata {
    mode: ConsistencyMode,
    state: ConsistencyState,
    source_update_sequence: Option<u64>,
    indexed_update_sequence: Option<u64>,
    update_sequence_lag: Option<u64>,
    indexed_source_version: Option<crate::index::SourceVersion>,
    time_lag: Option<std::time::Duration>,
}

/// Owned components returned by [`ConsistencyMetadata::into_parts`].
pub type ConsistencyMetadataParts = (
    ConsistencyMode,
    ConsistencyState,
    Option<u64>,
    Option<u64>,
    Option<u64>,
    Option<crate::index::SourceVersion>,
    Option<std::time::Duration>,
);

impl ConsistencyMetadata {
    /// Constructs consistency metadata for a successfully evaluated query.
    pub fn new(
        mode: ConsistencyMode,
        state: ConsistencyState,
        source_update_sequence: Option<u64>,
        indexed_update_sequence: Option<u64>,
        update_sequence_lag: Option<u64>,
        indexed_source_version: Option<crate::index::SourceVersion>,
        time_lag: Option<std::time::Duration>,
    ) -> Result<Self, ConsistencyMetadataValidationError> {
        let metadata = Self {
            mode,
            state,
            source_update_sequence,
            indexed_update_sequence,
            update_sequence_lag,
            indexed_source_version,
            time_lag,
        };

        metadata.validate()?;
        Ok(metadata)
    }

    /// Returns the declared consistency mode for the query.
    #[must_use]
    pub fn mode(&self) -> &ConsistencyMode {
        &self.mode
    }

    /// Returns the logical consistency state represented by the result.
    #[must_use]
    pub const fn state(&self) -> ConsistencyState {
        self.state
    }

    /// Returns the source-side update sequence observed during consistency
    /// evaluation, when available.
    #[must_use]
    pub const fn source_update_sequence(&self) -> Option<u64> {
        self.source_update_sequence
    }

    /// Returns the indexed-side update sequence represented by the selected
    /// queryable version, when available.
    #[must_use]
    pub const fn indexed_update_sequence(&self) -> Option<u64> {
        self.indexed_update_sequence
    }

    /// Returns the source-to-index sequence lag, when it was computed.
    #[must_use]
    pub const fn update_sequence_lag(&self) -> Option<u64> {
        self.update_sequence_lag
    }

    /// Returns the source version associated with the selected index state,
    /// when available.
    #[must_use]
    pub fn indexed_source_version(&self) -> Option<&crate::index::SourceVersion> {
        self.indexed_source_version.as_ref()
    }

    /// Returns the observed source/index time lag, when available.
    #[must_use]
    pub const fn time_lag(&self) -> Option<std::time::Duration> {
        self.time_lag
    }

    /// Validates the consistency metadata's internal relationships.
    pub fn validate(&self) -> Result<(), ConsistencyMetadataValidationError> {
        match (self.source_update_sequence, self.indexed_update_sequence) {
            (Some(source), Some(indexed)) => {
                if indexed > source {
                    return Err(ConsistencyMetadataValidationError::IndexedSequenceAhead {
                        source,
                        indexed,
                    });
                }

                let expected_lag = source - indexed;
                if let Some(lag) = self.update_sequence_lag
                    && lag != expected_lag
                {
                    return Err(
                        ConsistencyMetadataValidationError::UpdateSequenceLagMismatch {
                            source,
                            indexed,
                            reported: lag,
                            expected: expected_lag,
                        },
                    );
                }
            }
            (None, None) => {
                if self.update_sequence_lag.is_some() {
                    return Err(ConsistencyMetadataValidationError::LagWithoutSequences);
                }
            }
            (Some(_), None) | (None, Some(_)) => {
                if self.update_sequence_lag.is_some() {
                    return Err(ConsistencyMetadataValidationError::LagWithoutSequences);
                }
            }
        }

        match (&self.mode, self.state) {
            (_, ConsistencyState::Fresh) => Ok(()),
            (ConsistencyMode::VersionPinned(_), ConsistencyState::VersionPinned) => Ok(()),
            (ConsistencyMode::StaleAllowed(_), ConsistencyState::StaleAccepted) => Ok(()),
            _ => Err(ConsistencyMetadataValidationError::StateModeMismatch),
        }
    }

    /// Consumes the metadata and returns its logical components.
    #[must_use]
    pub fn into_parts(self) -> ConsistencyMetadataParts {
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

/// Structural failures for [`ConsistencyMetadata`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ConsistencyMetadataValidationError {
    /// The selected indexed sequence cannot be ahead of the source sequence.
    IndexedSequenceAhead { source: u64, indexed: u64 },

    /// A reported sequence lag does not match the supplied source/index
    /// sequence values.
    UpdateSequenceLagMismatch {
        source: u64,
        indexed: u64,
        reported: u64,
        expected: u64,
    },

    /// A lag was supplied without both sequence endpoints.
    LagWithoutSequences,

    /// The result's consistency state is inconsistent with its declared mode.
    StateModeMismatch,
}

impl fmt::Display for ConsistencyMetadataValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::IndexedSequenceAhead { source, indexed } => write!(
                formatter,
                "indexed update sequence {indexed} is ahead of source sequence {source}"
            ),
            Self::UpdateSequenceLagMismatch {
                source,
                indexed,
                reported,
                expected,
            } => write!(
                formatter,
                "reported update-sequence lag {reported} does not match source {source} and indexed {indexed}; expected {expected}"
            ),
            Self::LagWithoutSequences => formatter
                .write_str("update-sequence lag requires both source and indexed sequences"),
            Self::StateModeMismatch => formatter
                .write_str("consistency state does not match the declared consistency mode"),
        }
    }
}

impl std::error::Error for ConsistencyMetadataValidationError {}

/// A single reference-oriented logical retrieval record.
///
/// A hit never owns or hydrates the source object. `reference` remains an
/// opaque reference to an object owned by another engine.
///
/// `score` and `distance` are optional generic retrieval metadata. The result
/// contract does not define how a provider computes them or what they mean for
/// a source domain.
#[derive(Clone, Debug, PartialEq)]
pub struct QueryHit {
    reference: ObjectReference,
    score: Option<f64>,
    distance: Option<f64>,
    metadata: Option<KeyMaterial>,
}

impl QueryHit {
    /// Constructs a reference-only retrieval hit.
    pub fn new(reference: ObjectReference) -> Self {
        Self {
            reference,
            score: None,
            distance: None,
            metadata: None,
        }
    }

    /// Sets optional generic score and distance metadata.
    ///
    /// Non-finite values are rejected because they cannot represent stable
    /// logical retrieval metadata.
    pub fn with_metrics(
        mut self,
        score: Option<f64>,
        distance: Option<f64>,
    ) -> Result<Self, QueryHitValidationError> {
        validate_metric(score, MetricKind::Score)?;
        validate_metric(distance, MetricKind::Distance)?;

        self.score = score;
        self.distance = distance;
        Ok(self)
    }

    /// Sets optional generic retrieval metadata.
    pub fn with_metadata(mut self, metadata: KeyMaterial) -> Result<Self, QueryHitValidationError> {
        metadata
            .validate()
            .map_err(QueryHitValidationError::InvalidMetadata)?;
        self.metadata = Some(metadata);
        Ok(self)
    }

    /// Returns the source-owned object reference.
    #[must_use]
    pub fn reference(&self) -> &ObjectReference {
        &self.reference
    }

    /// Returns an optional generic score.
    #[must_use]
    pub fn score(&self) -> Option<f64> {
        self.score
    }

    /// Returns an optional generic distance.
    #[must_use]
    pub fn distance(&self) -> Option<f64> {
        self.distance
    }

    /// Returns optional generic retrieval metadata.
    #[must_use]
    pub fn metadata(&self) -> Option<&KeyMaterial> {
        self.metadata.as_ref()
    }

    /// Validates the complete hit contract.
    pub fn validate(&self) -> Result<(), QueryHitValidationError> {
        validate_metric(self.score, MetricKind::Score)?;
        validate_metric(self.distance, MetricKind::Distance)?;

        if let Some(metadata) = &self.metadata {
            metadata
                .validate()
                .map_err(QueryHitValidationError::InvalidMetadata)?;
        }

        Ok(())
    }

    /// Consumes the hit and returns its logical components.
    #[must_use]
    pub fn into_parts(
        self,
    ) -> (
        ObjectReference,
        Option<f64>,
        Option<f64>,
        Option<KeyMaterial>,
    ) {
        (self.reference, self.score, self.distance, self.metadata)
    }
}

/// Generic logical results returned by Indexing.
///
/// Results remain reference-oriented. The result records the logical index and
/// the exact `IndexVersion` used for retrieval, plus optional consistency
/// metadata describing the query-time synchronization state.
///
/// The `continuation` field is opaque transport-neutral metadata for a later
/// retrieval layer. This type does not implement pagination algorithms.
#[derive(Clone, Debug, PartialEq)]
pub struct QueryResult {
    definition_identity: IndexDefinitionIdentity,
    index_version: IndexVersion,
    hits: Vec<QueryHit>,
    continuation: Option<Vec<u8>>,
    consistency: Option<ConsistencyMetadata>,
}

impl QueryResult {
    /// Constructs a result without continuation or consistency metadata.
    ///
    /// This constructor preserves the Phase 2 API. Phase 4 query execution
    /// should use a consistency-aware constructor once the consistency policy
    /// has been evaluated.
    pub fn new(
        definition_identity: IndexDefinitionIdentity,
        index_version: IndexVersion,
        hits: Vec<QueryHit>,
    ) -> Result<Self, QueryResultValidationError> {
        Self::with_all(definition_identity, index_version, hits, None, None)
    }

    /// Constructs a result with opaque continuation information.
    pub fn with_continuation(
        definition_identity: IndexDefinitionIdentity,
        index_version: IndexVersion,
        hits: Vec<QueryHit>,
        continuation: Option<Vec<u8>>,
    ) -> Result<Self, QueryResultValidationError> {
        Self::with_all(definition_identity, index_version, hits, continuation, None)
    }

    /// Constructs a result with query-time consistency metadata.
    pub fn with_consistency(
        definition_identity: IndexDefinitionIdentity,
        index_version: IndexVersion,
        hits: Vec<QueryHit>,
        consistency: ConsistencyMetadata,
    ) -> Result<Self, QueryResultValidationError> {
        Self::with_all(
            definition_identity,
            index_version,
            hits,
            None,
            Some(consistency),
        )
    }

    /// Constructs the complete result representation.
    pub fn with_continuation_and_consistency(
        definition_identity: IndexDefinitionIdentity,
        index_version: IndexVersion,
        hits: Vec<QueryHit>,
        continuation: Option<Vec<u8>>,
        consistency: ConsistencyMetadata,
    ) -> Result<Self, QueryResultValidationError> {
        Self::with_all(
            definition_identity,
            index_version,
            hits,
            continuation,
            Some(consistency),
        )
    }

    fn with_all(
        definition_identity: IndexDefinitionIdentity,
        index_version: IndexVersion,
        hits: Vec<QueryHit>,
        continuation: Option<Vec<u8>>,
        consistency: Option<ConsistencyMetadata>,
    ) -> Result<Self, QueryResultValidationError> {
        let result = Self {
            definition_identity,
            index_version,
            hits,
            continuation,
            consistency,
        };

        result.validate()?;
        Ok(result)
    }

    /// Returns the concrete logical definition identity queried.
    #[must_use]
    pub fn definition_identity(&self) -> &IndexDefinitionIdentity {
        &self.definition_identity
    }

    /// Returns the exact logical index version associated with this result.
    #[must_use]
    pub fn index_version(&self) -> &IndexVersion {
        &self.index_version
    }

    /// Returns the reference-oriented retrieval hits.
    #[must_use]
    pub fn hits(&self) -> &[QueryHit] {
        &self.hits
    }

    /// Returns opaque continuation information, if supplied.
    #[must_use]
    pub fn continuation(&self) -> Option<&[u8]> {
        self.continuation.as_deref()
    }

    /// Returns query-time consistency metadata, if supplied.
    #[must_use]
    pub fn consistency(&self) -> Option<&ConsistencyMetadata> {
        self.consistency.as_ref()
    }

    /// Validates the complete logical result contract.
    ///
    /// Validation remains structural. It does not decide whether the selected
    /// version is active/current, whether a provider is capable of the query,
    /// or whether the caller's consistency requirement is semantically
    /// satisfiable.
    pub fn validate(&self) -> Result<(), QueryResultValidationError> {
        self.index_version
            .validate()
            .map_err(QueryResultValidationError::InvalidIndexVersion)?;

        for (position, hit) in self.hits.iter().enumerate() {
            hit.validate()
                .map_err(|error| QueryResultValidationError::InvalidHit { position, error })?;
        }

        if let Some(consistency) = &self.consistency {
            consistency
                .validate()
                .map_err(QueryResultValidationError::InvalidConsistency)?;
        }

        Ok(())
    }

    /// Consumes the result and returns the Phase 2-compatible logical parts.
    ///
    /// Consistency metadata is intentionally omitted from this compatibility
    /// tuple. Use [`QueryResult::into_parts_with_consistency`] when the Phase 4
    /// metadata is required.
    #[must_use]
    pub fn into_parts(
        self,
    ) -> (
        IndexDefinitionIdentity,
        IndexVersion,
        Vec<QueryHit>,
        Option<Vec<u8>>,
    ) {
        (
            self.definition_identity,
            self.index_version,
            self.hits,
            self.continuation,
        )
    }

    /// Consumes the result and returns all Phase 4 logical components.
    #[must_use]
    pub fn into_parts_with_consistency(
        self,
    ) -> (
        IndexDefinitionIdentity,
        IndexVersion,
        Vec<QueryHit>,
        Option<Vec<u8>>,
        Option<ConsistencyMetadata>,
    ) {
        (
            self.definition_identity,
            self.index_version,
            self.hits,
            self.continuation,
            self.consistency,
        )
    }
}

/// Structural failures for [`QueryHit`].
#[derive(Clone, Debug, PartialEq)]
pub enum QueryHitValidationError {
    /// A score or distance is not finite.
    NonFiniteMetric { kind: MetricKind, value: f64 },

    /// Per-hit generic metadata is structurally invalid.
    InvalidMetadata(KeyMaterialValidationError),
}

impl fmt::Display for QueryHitValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonFiniteMetric { kind, value } => {
                write!(formatter, "{kind} must be finite, got {value}")
            }
            Self::InvalidMetadata(error) => {
                write!(formatter, "invalid retrieval metadata: {error}")
            }
        }
    }
}

impl std::error::Error for QueryHitValidationError {}

/// Names the two optional numeric retrieval metrics supported by [`QueryHit`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MetricKind {
    /// Generic provider/result score.
    Score,

    /// Generic provider/result distance.
    Distance,
}

impl fmt::Display for MetricKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Score => formatter.write_str("score"),
            Self::Distance => formatter.write_str("distance"),
        }
    }
}

/// Structural failures for [`QueryResult`].
#[derive(Clone, Debug, PartialEq)]
pub enum QueryResultValidationError {
    /// The logical index version is structurally invalid.
    InvalidIndexVersion(IndexVersionValidationError),

    /// A returned hit is structurally invalid.
    InvalidHit {
        /// Zero-based position of the invalid hit.
        position: usize,
        /// Hit validation failure.
        error: QueryHitValidationError,
    },

    /// Query-time consistency metadata is structurally invalid.
    InvalidConsistency(ConsistencyMetadataValidationError),
}

impl fmt::Display for QueryResultValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidIndexVersion(error) => {
                write!(formatter, "invalid index version: {error}")
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
        }
    }
}

impl std::error::Error for QueryResultValidationError {}

fn validate_metric(value: Option<f64>, kind: MetricKind) -> Result<(), QueryHitValidationError> {
    if let Some(value) = value
        && !value.is_finite()
    {
        return Err(QueryHitValidationError::NonFiniteMetric { kind, value });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::{IndexDefinitionId, IndexDefinitionIdentity, IndexNamespace};
    use crate::index::IndexFamily;
    use crate::index::{IndexVersionId, SourceVersion};

    fn definition_identity(byte: u8) -> IndexDefinitionIdentity {
        IndexDefinitionIdentity::new(
            IndexDefinitionId::new(format!("query.result.definition.{byte}"))
                .expect("definition ID should be valid"),
            IndexNamespace::new(format!("query.result.namespace.{byte}"))
                .expect("namespace should be valid"),
            IndexFamily::Inverted,
        )
    }

    fn version(byte: u8) -> IndexVersion {
        let id = IndexVersionId::new(format!("index-version-{byte}"))
            .expect("test version ID must be valid");
        IndexVersion::new(id)
    }

    fn reference() -> ObjectReference {
        ObjectReference::new("quran", "verse:1:1").expect("test reference must be valid")
    }

    #[test]
    fn query_hit_is_reference_only() {
        let hit = QueryHit::new(reference())
            .with_metrics(Some(0.95), None)
            .expect("metrics should be valid");

        assert_eq!(hit.reference().source(), "quran");
        assert_eq!(hit.reference().object_reference(), "verse:1:1");
        assert_eq!(hit.score(), Some(0.95));
        assert!(hit.distance().is_none());
    }

    #[test]
    fn query_hit_rejects_non_finite_metrics() {
        let result = QueryHit::new(reference()).with_metrics(Some(f64::NAN), None);

        assert!(matches!(
            result,
            Err(QueryHitValidationError::NonFiniteMetric {
                kind: MetricKind::Score,
                ..
            })
        ));

        let result = QueryHit::new(reference()).with_metrics(None, Some(f64::INFINITY));

        assert!(matches!(
            result,
            Err(QueryHitValidationError::NonFiniteMetric {
                kind: MetricKind::Distance,
                ..
            })
        ));
    }

    #[test]
    fn query_result_preserves_index_version_and_hits() {
        let result = QueryResult::new(
            definition_identity(0x11),
            version(1),
            vec![QueryHit::new(reference())],
        )
        .expect("result should be valid");

        assert_eq!(result.definition_identity(), &definition_identity(0x11));
        assert_eq!(result.index_version().id().as_str(), "index-version-1");
        assert_eq!(result.hits().len(), 1);
        assert_eq!(result.hits()[0].reference().object_reference(), "verse:1:1");
        assert!(result.continuation().is_none());
        assert!(result.consistency().is_none());
    }

    #[test]
    fn query_result_supports_opaque_continuation() {
        let result = QueryResult::with_continuation(
            definition_identity(0x22),
            version(2),
            vec![QueryHit::new(reference())],
            Some(vec![1, 2, 3, 4]),
        )
        .expect("result should be valid");

        assert_eq!(result.continuation(), Some(&[1, 2, 3, 4][..]));
    }

    #[test]
    fn query_result_allows_an_empty_successful_hit_list() {
        let result = QueryResult::new(definition_identity(0x33), version(3), Vec::new())
            .expect("empty result should be valid");

        assert!(result.hits().is_empty());
    }

    #[test]
    fn consistency_metadata_records_current_state() {
        let mode = ConsistencyMode::Current;
        let metadata = ConsistencyMetadata::new(
            mode,
            ConsistencyState::Fresh,
            Some(100),
            Some(100),
            Some(0),
            Some(SourceVersion::new("source-v100").expect("source version should be valid")),
            None,
        )
        .expect("consistency metadata should be valid");

        let result = QueryResult::with_consistency(
            definition_identity(0x44),
            version(4),
            vec![QueryHit::new(reference())],
            metadata,
        )
        .expect("result should be valid");

        let consistency = result
            .consistency()
            .expect("consistency metadata should exist");
        assert_eq!(consistency.state(), ConsistencyState::Fresh);
        assert_eq!(consistency.source_update_sequence(), Some(100));
        assert_eq!(consistency.indexed_update_sequence(), Some(100));
        assert_eq!(consistency.update_sequence_lag(), Some(0));
        assert_eq!(
            consistency
                .indexed_source_version()
                .expect("indexed source version should exist")
                .as_ref(),
            "source-v100"
        );
    }

    #[test]
    fn version_pinned_metadata_records_pinned_state() {
        let metadata = ConsistencyMetadata::new(
            ConsistencyMode::VersionPinned(
                IndexVersionId::new("index-version-7").expect("version ID should be valid"),
            ),
            ConsistencyState::VersionPinned,
            Some(50),
            Some(50),
            Some(0),
            None,
            None,
        )
        .expect("consistency metadata should be valid");

        assert_eq!(metadata.state(), ConsistencyState::VersionPinned);
        assert!(matches!(metadata.mode(), ConsistencyMode::VersionPinned(_)));
    }

    #[test]
    fn consistency_metadata_rejects_sequence_regression() {
        let result = ConsistencyMetadata::new(
            ConsistencyMode::Current,
            ConsistencyState::Fresh,
            Some(10),
            Some(11),
            None,
            None,
            None,
        );

        assert!(matches!(
            result,
            Err(ConsistencyMetadataValidationError::IndexedSequenceAhead {
                source: 10,
                indexed: 11
            })
        ));
    }

    #[test]
    fn consistency_metadata_rejects_incorrect_lag() {
        let result = ConsistencyMetadata::new(
            ConsistencyMode::Current,
            ConsistencyState::Fresh,
            Some(10),
            Some(7),
            Some(2),
            None,
            None,
        );

        assert!(matches!(
            result,
            Err(
                ConsistencyMetadataValidationError::UpdateSequenceLagMismatch {
                    source: 10,
                    indexed: 7,
                    reported: 2,
                    expected: 3
                }
            )
        ));
    }

    #[test]
    fn consistency_metadata_rejects_mode_state_mismatch() {
        let result = ConsistencyMetadata::new(
            ConsistencyMode::Current,
            ConsistencyState::StaleAccepted,
            None,
            None,
            None,
            None,
            None,
        );

        assert!(matches!(
            result,
            Err(ConsistencyMetadataValidationError::StateModeMismatch)
        ));
    }

    #[test]
    fn result_validation_rejects_invalid_hit() {
        let hit = QueryHit {
            reference: reference(),
            score: Some(f64::NAN),
            distance: None,
            metadata: None,
        };

        let result = QueryResult {
            definition_identity: definition_identity(0x55),
            index_version: version(5),
            hits: vec![hit],
            continuation: None,
            consistency: None,
        };

        assert!(matches!(
            result.validate(),
            Err(QueryResultValidationError::InvalidHit {
                position: 0,
                error: QueryHitValidationError::NonFiniteMetric {
                    kind: MetricKind::Score,
                    ..
                }
            })
        ));
    }

    #[test]
    fn into_parts_with_consistency_preserves_all_components() {
        let metadata = ConsistencyMetadata::new(
            ConsistencyMode::Current,
            ConsistencyState::Fresh,
            Some(9),
            Some(9),
            Some(0),
            None,
            None,
        )
        .expect("metadata should be valid");

        let result = QueryResult::with_continuation_and_consistency(
            definition_identity(0x66),
            version(6),
            vec![QueryHit::new(reference())],
            Some(vec![8, 7, 6]),
            metadata.clone(),
        )
        .expect("result should be valid");

        let (definition_identity, version, hits, continuation, consistency) =
            result.into_parts_with_consistency();

        assert_eq!(
            definition_identity.definition_id().as_str(),
            "query.result.definition.102"
        );
        assert_eq!(version.id().as_str(), "index-version-6");
        assert_eq!(hits.len(), 1);
        assert_eq!(continuation, Some(vec![8, 7, 6]));
        assert_eq!(consistency, Some(metadata));
    }

    #[test]
    fn legacy_into_parts_keeps_phase_2_shape() {
        let result = QueryResult::new(
            definition_identity(0x77),
            version(7),
            vec![QueryHit::new(reference())],
        )
        .expect("result should be valid");

        let (definition_identity, version, hits, continuation) = result.into_parts();

        assert_eq!(
            definition_identity.definition_id().as_str(),
            "query.result.definition.119"
        );
        assert_eq!(version.id().as_str(), "index-version-7");
        assert_eq!(hits.len(), 1);
        assert!(continuation.is_none());
    }
}
