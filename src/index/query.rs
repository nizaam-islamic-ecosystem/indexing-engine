//! Legacy Phase 2 query adapter for the Indexing Engine.
//!
//! Phase 4 moved the canonical logical query request/result contracts to
//! [`crate::query`]. This module preserves the `crate::index::query` module
//! path and the legacy exact-query construction shape while delegating all
//! query behavior to the canonical Phase 4 request/result types.
//!
//! The logical target is represented by [`IndexDefinitionIdentity`], matching
//! the canonical query contract. `IndexId` is intentionally not used here:
//! it identifies an Index Assignment Operation, not a logical query target.
//!
//! The legacy request remains a thin adapter over the canonical Phase 4
//! request. There is no second query implementation. The adapter continues to
//! expose `query()` and `into_parts()` for legacy callers while also exposing
//! the canonical request through `Deref`/conversion.
//!
//! New Phase 4 query construction, planning, consistency, and retrieval types
//! belong to [`crate::query`].

use core::num::NonZeroUsize;
use core::ops::Deref;

use crate::identity::IndexDefinitionIdentity;
use crate::index::KeyMaterial;
use crate::query::QueryKind;

pub use crate::query::{
    MetricKind, QueryHit, QueryHitValidationError, QueryRequestValidationError, QueryResult,
    QueryResultValidationError,
};

/// Backward-compatible wrapper around the canonical Phase 4 [`crate::query::QueryRequest`].
///
/// The legacy constructors create only exact requests, which makes the original
/// `query() -> &KeyMaterial` and four-element `into_parts()` contract
/// well-defined. Phase 4 callers should construct [`crate::query::QueryRequest`]
/// directly when they need non-exact query kinds.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QueryRequest {
    inner: crate::query::QueryRequest,
}

impl QueryRequest {
    /// Constructs the original exact logical query request.
    pub fn new(
        definition_identity: IndexDefinitionIdentity,
        query: KeyMaterial,
    ) -> Result<Self, QueryRequestValidationError> {
        Ok(Self {
            inner: crate::query::QueryRequest::new(definition_identity, query)?,
        })
    }

    /// Constructs the original exact logical request with optional limit and metadata.
    pub fn with_options(
        definition_identity: IndexDefinitionIdentity,
        query: KeyMaterial,
        limit: Option<NonZeroUsize>,
        metadata: Option<KeyMaterial>,
    ) -> Result<Self, QueryRequestValidationError> {
        Ok(Self {
            inner: crate::query::QueryRequest::with_options(
                definition_identity,
                query,
                limit,
                metadata,
            )?,
        })
    }

    /// Returns the logical index definition identity.
    #[must_use]
    pub fn definition_identity(&self) -> &IndexDefinitionIdentity {
        self.inner.definition_identity()
    }

    /// Returns the original generic query material.
    ///
    /// Legacy `QueryRequest` values are always exact requests. Use
    /// `query_kind()` on the canonical request for Phase 4 query variants.
    #[must_use]
    pub fn query(&self) -> &KeyMaterial {
        self.inner
            .query_material()
            .expect("legacy QueryRequest must contain exact query material")
    }

    /// Returns the optional logical result-count requirement.
    #[must_use]
    pub fn limit(&self) -> Option<NonZeroUsize> {
        self.inner.limit()
    }

    /// Returns caller-supplied generic metadata.
    #[must_use]
    pub fn metadata(&self) -> Option<&KeyMaterial> {
        self.inner.metadata()
    }

    /// Validates the wrapped canonical request.
    pub fn validate(&self) -> Result<(), QueryRequestValidationError> {
        self.inner.validate()
    }

    /// Borrows the canonical Phase 4 request.
    #[must_use]
    pub fn as_canonical(&self) -> &crate::query::QueryRequest {
        &self.inner
    }

    /// Converts the legacy request into the canonical Phase 4 request.
    #[must_use]
    pub fn into_canonical(self) -> crate::query::QueryRequest {
        self.inner
    }

    /// Consumes the request using the original Phase 2 four-element tuple.
    ///
    /// This compatibility method intentionally preserves the old public
    /// signature. The richer Phase 4 request decomposition is available through
    /// `crate::query::QueryRequest::into_parts()`.
    #[must_use]
    pub fn into_parts(
        self,
    ) -> (
        IndexDefinitionIdentity,
        KeyMaterial,
        Option<NonZeroUsize>,
        Option<KeyMaterial>,
    ) {
        let (
            definition_identity,
            _namespace,
            _definition_id,
            _family,
            query,
            limit,
            _consistency,
            _result_mode,
            metadata,
        ) = self.inner.into_parts();

        let query = match query {
            QueryKind::Exact { key } => key,
            _ => unreachable!("legacy QueryRequest can only contain an exact query"),
        };

        (definition_identity, query, limit, metadata)
    }
}

impl Deref for QueryRequest {
    type Target = crate::query::QueryRequest;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl From<QueryRequest> for crate::query::QueryRequest {
    fn from(request: QueryRequest) -> Self {
        request.inner
    }
}

impl AsRef<crate::query::QueryRequest> for QueryRequest {
    fn as_ref(&self) -> &crate::query::QueryRequest {
        &self.inner
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::{IndexDefinitionId, IndexDefinitionIdentity, IndexNamespace};
    use crate::index::{
        IndexFamily, IndexVersion, IndexVersionId, KeyMaterial, ObjectReference, SourceVersion,
    };
    use core::num::NonZeroUsize;

    fn definition_identity(byte: u8) -> IndexDefinitionIdentity {
        let definition_id = IndexDefinitionId::new(format!("legacy-query-definition-{byte}"))
            .expect("test definition ID must be valid");
        let namespace = IndexNamespace::new(format!("legacy.query.namespace.{byte}"))
            .expect("test namespace must be valid");

        IndexDefinitionIdentity::new(definition_id, namespace, IndexFamily::Identity)
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
        let request = QueryRequest::new(definition_identity(0x11), KeyMaterial::text("bismillah"))
            .expect("request should be valid");

        assert_eq!(request.definition_identity(), &definition_identity(0x11));
        assert_eq!(request.query(), &KeyMaterial::text("bismillah"));
        assert!(request.limit().is_none());
        assert!(request.metadata().is_none());
    }

    #[test]
    fn query_request_supports_logical_limit_and_generic_metadata() {
        let request = QueryRequest::with_options(
            definition_identity(0x22),
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

        let result = QueryRequest::new(definition_identity(0x33), invalid);

        assert!(matches!(
            result,
            Err(QueryRequestValidationError::InvalidQueryMaterial(_))
        ));
    }

    #[test]
    fn query_request_validation_is_repeatable() {
        let request = QueryRequest::new(definition_identity(0x44), KeyMaterial::Unsigned(42))
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

        let result = QueryResult::new(definition_identity(0x55), version(1), vec![hit])
            .expect("result should be valid");

        assert_eq!(result.definition_identity(), &definition_identity(0x55));
        assert_eq!(result.index_version().id().as_str(), "index-version-1");
        assert_eq!(result.hits().len(), 1);
        assert_eq!(result.hits()[0].reference().object_reference(), "verse:1:1");
        assert!(result.continuation().is_none());
        assert!(result.consistency().is_none());
    }

    #[test]
    fn query_result_supports_opaque_continuation() {
        let result = QueryResult::with_continuation(
            definition_identity(0x66),
            version(2),
            vec![QueryHit::new(reference())],
            Some(vec![1, 2, 3, 4]),
        )
        .expect("result should be valid");

        assert_eq!(result.continuation(), Some(&[1, 2, 3, 4][..]));
        assert!(result.consistency().is_none());
    }

    #[test]
    fn query_result_is_empty_hit_list_capable() {
        let result = QueryResult::new(definition_identity(0x77), version(3), Vec::new())
            .expect("empty result should be valid");

        assert!(result.hits().is_empty());
    }

    #[test]
    fn query_result_validation_checks_index_version() {
        let result = QueryResult::new(
            definition_identity(0x88),
            version(4),
            vec![QueryHit::new(reference())],
        )
        .expect("result should be valid");

        assert_eq!(result.validate(), Ok(()));
    }

    #[test]
    fn source_version_remains_distinct_from_index_version() {
        let source_version =
            SourceVersion::new("source-v1").expect("source version should be valid");

        let index_version = IndexVersion::new(
            IndexVersionId::new("index-v1").expect("index version should be valid"),
        );

        assert_eq!(source_version.as_ref(), "source-v1");
        assert_eq!(index_version.id().as_str(), "index-v1");
    }

    #[test]
    fn query_contract_contains_no_domain_object_type() {
        let result = QueryResult::new(
            definition_identity(0x99),
            version(5),
            vec![QueryHit::new(reference())],
        )
        .expect("result should be valid");

        // The only object-bearing value exposed by a hit is ObjectReference.
        assert_eq!(result.hits()[0].reference().source(), "quran");
    }

    #[test]
    fn into_parts_preserves_query_request_contents() {
        let request = QueryRequest::with_options(
            definition_identity(0xaa),
            KeyMaterial::text("query"),
            NonZeroUsize::new(5),
            Some(KeyMaterial::Bool(true)),
        )
        .expect("request should be valid");

        let (returned_definition_identity, query, limit, metadata) = request.into_parts();

        assert_eq!(returned_definition_identity, definition_identity(0xaa));
        assert_eq!(query, KeyMaterial::text("query"));
        assert_eq!(limit, NonZeroUsize::new(5));
        assert_eq!(metadata, Some(KeyMaterial::Bool(true)));
    }

    #[test]
    fn legacy_request_can_be_borrowed_or_converted_as_canonical_phase4_request() {
        let request = QueryRequest::new(definition_identity(0xbc), KeyMaterial::text("bridge"))
            .expect("legacy request should be valid");

        assert!(matches!(
            request.as_canonical().query_kind(),
            crate::query::QueryKind::Exact {
                key: KeyMaterial::Text(_)
            }
        ));

        let canonical: crate::query::QueryRequest = request.into();
        assert!(matches!(
            canonical.query_kind(),
            crate::query::QueryKind::Exact { .. }
        ));
    }

    #[test]
    fn legacy_paths_resolve_to_canonical_phase4_types() {
        let request = QueryRequest::new(definition_identity(0xbb), KeyMaterial::text("canonical"))
            .expect("request should be valid");

        let result = QueryResult::new(definition_identity(0xbb), version(6), Vec::new())
            .expect("result should be valid");

        let _: crate::query::QueryRequest = request.into();
        let _: crate::query::QueryResult = result;
    }
}
