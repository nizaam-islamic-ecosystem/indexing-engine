//! Logical query and result contracts for the Indexing Engine.
//!
//! This module defines what a caller asks Indexing to retrieve and what
//! Indexing can return at the logical contract boundary.
//!
//! It deliberately does not implement query planning, provider selection,
//! physical lookup, retrieval execution, pagination algorithms, or
//! domain-object hydration.

use super::key::KeyMaterial;
use super::reference::ObjectReference;
use super::version::IndexVersion;
use crate::identity::IndexId;
use core::fmt;
use core::num::NonZeroUsize;

/// A provider-neutral logical request to retrieve information from an index.
///
/// The request identifies the logical index resource and supplies generic
/// query material. The optional limit expresses a logical result-count
/// requirement; it does not select a physical retrieval strategy.
///
/// Query execution, planning, provider selection, and physical lookup are
/// intentionally outside this contract.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QueryRequest {
    index_id: IndexId,
    query: KeyMaterial,
    limit: Option<NonZeroUsize>,
    metadata: Option<KeyMaterial>,
}

impl QueryRequest {
    /// Constructs a logical query request.
    pub fn new(index_id: IndexId, query: KeyMaterial) -> Result<Self, QueryRequestValidationError> {
        Self::with_options(index_id, query, None, None)
    }

    /// Constructs a logical query request with optional logical result
    /// constraints and caller-supplied generic metadata.
    pub fn with_options(
        index_id: IndexId,
        query: KeyMaterial,
        limit: Option<NonZeroUsize>,
        metadata: Option<KeyMaterial>,
    ) -> Result<Self, QueryRequestValidationError> {
        let request = Self {
            index_id,
            query,
            limit,
            metadata,
        };

        request.validate()?;
        Ok(request)
    }

    /// Returns the concrete logical index identity being queried.
    #[must_use]
    pub fn index_id(&self) -> &IndexId {
        &self.index_id
    }

    /// Returns the generic logical query material.
    #[must_use]
    pub fn query(&self) -> &KeyMaterial {
        &self.query
    }

    /// Returns the optional logical result-count requirement.
    #[must_use]
    pub fn limit(&self) -> Option<NonZeroUsize> {
        self.limit
    }

    /// Returns caller-supplied generic query metadata, if present.
    #[must_use]
    pub fn metadata(&self) -> Option<&KeyMaterial> {
        self.metadata.as_ref()
    }

    /// Validates the logical query contract.
    ///
    /// No physical provider or execution configuration is examined because
    /// none belongs to `QueryRequest`.
    pub fn validate(&self) -> Result<(), QueryRequestValidationError> {
        self.query
            .validate()
            .map_err(QueryRequestValidationError::InvalidQueryMaterial)?;

        if let Some(metadata) = &self.metadata {
            metadata
                .validate()
                .map_err(QueryRequestValidationError::InvalidMetadata)?;
        }

        Ok(())
    }

    /// Consumes the request and returns its logical components.
    #[must_use]
    pub fn into_parts(
        self,
    ) -> (
        IndexId,
        KeyMaterial,
        Option<NonZeroUsize>,
        Option<KeyMaterial>,
    ) {
        (self.index_id, self.query, self.limit, self.metadata)
    }
}

/// A single reference-oriented logical retrieval record.
///
/// A hit never owns or hydrates the source object. `reference` remains an
/// opaque reference to an object owned by another engine.
///
/// `score` and `distance` are optional generic retrieval metadata. A provider
/// or algorithm is free to omit them; this type does not define their
/// calculation or interpretation.
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

    /// Sets optional logical retrieval metadata.
    ///
    /// Non-finite numeric values are rejected because they cannot represent
    /// stable logical retrieval metadata.
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

    /// Validates the retrieval hit.
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
/// Results remain reference-oriented. The result contains source-owned
/// [`ObjectReference`] values rather than domain objects.
///
/// The result also records the index identity and logical index version from
/// which the result was produced. Continuation is opaque metadata for a later
/// retrieval layer; this Phase 2 contract does not implement continuation
/// algorithms or pagination state transitions.
#[derive(Clone, Debug, PartialEq)]
pub struct QueryResult {
    index_id: IndexId,
    index_version: IndexVersion,
    hits: Vec<QueryHit>,
    continuation: Option<Vec<u8>>,
}

impl QueryResult {
    /// Constructs a result with no continuation information.
    pub fn new(
        index_id: IndexId,
        index_version: IndexVersion,
        hits: Vec<QueryHit>,
    ) -> Result<Self, QueryResultValidationError> {
        Self::with_continuation(index_id, index_version, hits, None)
    }

    /// Constructs a result with optional opaque continuation information.
    pub fn with_continuation(
        index_id: IndexId,
        index_version: IndexVersion,
        hits: Vec<QueryHit>,
        continuation: Option<Vec<u8>>,
    ) -> Result<Self, QueryResultValidationError> {
        let result = Self {
            index_id,
            index_version,
            hits,
            continuation,
        };

        result.validate()?;
        Ok(result)
    }

    /// Returns the concrete logical index identity queried.
    #[must_use]
    pub fn index_id(&self) -> &IndexId {
        &self.index_id
    }

    /// Returns the logical index version associated with this result.
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

    /// Validates the complete logical result contract.
    pub fn validate(&self) -> Result<(), QueryResultValidationError> {
        self.index_version
            .validate()
            .map_err(QueryResultValidationError::InvalidIndexVersion)?;

        for (position, hit) in self.hits.iter().enumerate() {
            hit.validate()
                .map_err(|error| QueryResultValidationError::InvalidHit { position, error })?;
        }

        Ok(())
    }

    /// Consumes the result and returns its logical components.
    #[must_use]
    pub fn into_parts(self) -> (IndexId, IndexVersion, Vec<QueryHit>, Option<Vec<u8>>) {
        (
            self.index_id,
            self.index_version,
            self.hits,
            self.continuation,
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum QueryRequestValidationError {
    InvalidQueryMaterial(super::key::KeyMaterialValidationError),
    InvalidMetadata(super::key::KeyMaterialValidationError),
}

impl fmt::Display for QueryRequestValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidQueryMaterial(error) => {
                write!(formatter, "invalid query material: {error}")
            }
            Self::InvalidMetadata(error) => {
                write!(formatter, "invalid query metadata: {error}")
            }
        }
    }
}

impl std::error::Error for QueryRequestValidationError {}

#[derive(Clone, Debug, PartialEq)]
pub enum QueryHitValidationError {
    NonFiniteMetric { kind: MetricKind, value: f64 },
    InvalidMetadata(super::key::KeyMaterialValidationError),
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MetricKind {
    Score,
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

#[derive(Clone, Debug, PartialEq)]
pub enum QueryResultValidationError {
    InvalidIndexVersion(super::version::IndexVersionValidationError),
    InvalidHit {
        position: usize,
        error: QueryHitValidationError,
    },
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
    use crate::identity::index::INDEX_ID_BYTE_LEN;
    use crate::index::{IndexVersionId, SourceVersion};

    fn index_id(byte: u8) -> IndexId {
        IndexId::from_bytes([byte; INDEX_ID_BYTE_LEN])
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
    fn valid_query_request_is_provider_neutral() {
        let request = QueryRequest::new(index_id(0x11), KeyMaterial::text("bismillah"))
            .expect("request should be valid");

        assert_eq!(request.index_id().as_bytes(), &[0x11; INDEX_ID_BYTE_LEN]);
        assert_eq!(request.query(), &KeyMaterial::text("bismillah"));
        assert!(request.limit().is_none());
        assert!(request.metadata().is_none());
    }

    #[test]
    fn query_request_supports_logical_limit_and_generic_metadata() {
        let request = QueryRequest::with_options(
            index_id(0x22),
            KeyMaterial::text("term"),
            NonZeroUsize::new(10),
            Some(KeyMaterial::text("caller-metadata")),
        )
        .expect("request should be valid");

        assert_eq!(request.limit(), NonZeroUsize::new(10));
        assert_eq!(
            request.metadata(),
            Some(&KeyMaterial::text("caller-metadata"))
        );
    }

    #[test]
    fn query_request_rejects_invalid_query_material() {
        let invalid = KeyMaterial::Text("\u{0000}".to_owned());

        let result = QueryRequest::new(index_id(0x33), invalid);

        assert!(matches!(
            result,
            Err(QueryRequestValidationError::InvalidQueryMaterial(_))
        ));
    }

    #[test]
    fn query_request_validation_is_repeatable() {
        let request = QueryRequest::new(index_id(0x44), KeyMaterial::Unsigned(42))
            .expect("request should be valid");

        assert_eq!(request.validate(), Ok(()));
        assert_eq!(request.validate(), Ok(()));
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
    fn query_result_preserves_index_and_version() {
        let hit = QueryHit::new(reference());

        let result = QueryResult::new(index_id(0x55), version(1), vec![hit])
            .expect("result should be valid");

        assert_eq!(result.index_id().as_bytes(), &[0x55; INDEX_ID_BYTE_LEN]);
        assert_eq!(result.index_version().id().as_str(), "index-version-1");
        assert_eq!(result.hits().len(), 1);
        assert_eq!(result.hits()[0].reference().object_reference(), "verse:1:1");
        assert!(result.continuation().is_none());
    }

    #[test]
    fn query_result_supports_opaque_continuation() {
        let result = QueryResult::with_continuation(
            index_id(0x66),
            version(2),
            vec![QueryHit::new(reference())],
            Some(vec![1, 2, 3, 4]),
        )
        .expect("result should be valid");

        assert_eq!(result.continuation(), Some(&[1, 2, 3, 4][..]));
    }

    #[test]
    fn query_result_is_empty_hit_list_capable() {
        let result = QueryResult::new(index_id(0x77), version(3), Vec::new())
            .expect("empty result should be valid");

        assert!(result.hits().is_empty());
    }

    #[test]
    fn query_result_validation_checks_index_version() {
        let result = QueryResult::new(index_id(0x88), version(4), vec![QueryHit::new(reference())])
            .expect("result should be valid");

        assert_eq!(result.validate(), Ok(()));
    }

    #[test]
    fn source_version_remains_distinct_from_index_version() {
        let source_version = SourceVersion::new("source-v1").expect("source version must be valid");

        let index_version = IndexVersion::new(
            IndexVersionId::new("index-v1").expect("index version must be valid"),
        );

        assert_eq!(source_version.as_ref(), "source-v1");
        assert_eq!(index_version.id().as_str(), "index-v1");
    }

    #[test]
    fn query_contract_contains_no_domain_object_type() {
        let result = QueryResult::new(index_id(0x99), version(5), vec![QueryHit::new(reference())])
            .expect("result should be valid");

        // The only object-bearing value exposed by a hit is ObjectReference.
        assert_eq!(result.hits()[0].reference().source(), "quran");
    }

    #[test]
    fn into_parts_preserves_query_request() {
        let request = QueryRequest::with_options(
            index_id(0xaa),
            KeyMaterial::text("query"),
            NonZeroUsize::new(5),
            Some(KeyMaterial::Bool(true)),
        )
        .expect("request should be valid");

        let (index_id, query, limit, metadata) = request.into_parts();

        assert_eq!(index_id.as_bytes(), &[0xaa; INDEX_ID_BYTE_LEN]);
        assert_eq!(query, KeyMaterial::text("query"));
        assert_eq!(limit, NonZeroUsize::new(5));
        assert_eq!(metadata, Some(KeyMaterial::Bool(true)));
    }
}
