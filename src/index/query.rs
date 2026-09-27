//! Backward-compatible query contract exports for the Indexing Engine.
//!
//! Phase 4 promotes the canonical logical query-request and query-result
//! contracts to [`crate::query`]. This legacy module remains at
//! `crate::index::query` only as a compatibility boundary for the Phase 2
//! public API.
//!
//! The query implementations themselves are owned by the canonical Phase 4
//! modules. This file therefore does not maintain a second implementation.
//! Existing logical request/result behavior is preserved by re-exporting the
//! canonical types and retaining the legacy contract test coverage below.
//!
//! New Phase 4 query kinds, consistency modes, planning contracts, and
//! retrieval execution contracts belong to [`crate::query`], not this legacy
//! compatibility module.

pub use crate::query::{
    MetricKind, QueryHit, QueryHitValidationError, QueryRequest, QueryRequestValidationError,
    QueryResult, QueryResultValidationError,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::index::INDEX_ID_BYTE_LEN;
    use crate::index::{IndexVersion, IndexVersionId, KeyMaterial, ObjectReference, SourceVersion};
    use core::num::NonZeroUsize;

    fn index_id(byte: u8) -> crate::identity::IndexId {
        crate::identity::IndexId::from_bytes([byte; INDEX_ID_BYTE_LEN])
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
        assert_eq!(
            request.query(),
            &crate::query::QueryKind::Exact {
                key: KeyMaterial::text("bismillah"),
            }
        );
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
        assert!(result.consistency().is_none());
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
        assert!(result.consistency().is_none());
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
        let result = QueryResult::new(index_id(0x99), version(5), vec![QueryHit::new(reference())])
            .expect("result should be valid");

        // The only object-bearing value exposed by a hit is ObjectReference.
        assert_eq!(result.hits()[0].reference().source(), "quran");
    }

    #[test]
    fn into_parts_preserves_query_request_contents() {
        let request = QueryRequest::with_options(
            index_id(0xaa),
            KeyMaterial::text("query"),
            NonZeroUsize::new(5),
            Some(KeyMaterial::Bool(true)),
        )
        .expect("request should be valid");

        // Phase 4 extends the request tuple with selector/query-policy fields.
        // The legacy test intent is preserved by checking the corresponding
        // canonical fields instead of discarding the richer Phase 4 contract.
        let (
            returned_index_id,
            namespace,
            definition_id,
            family,
            query,
            limit,
            consistency,
            result_mode,
            metadata,
        ) = request.into_parts();

        assert_eq!(returned_index_id.as_bytes(), &[0xaa; INDEX_ID_BYTE_LEN]);
        assert!(namespace.is_none());
        assert!(definition_id.is_none());
        assert!(family.is_none());
        assert_eq!(
            query,
            crate::query::QueryKind::Exact {
                key: KeyMaterial::text("query"),
            }
        );
        assert_eq!(limit, NonZeroUsize::new(5));
        assert_eq!(
            consistency,
            crate::consistency::policy::ConsistencyMode::Current
        );
        assert_eq!(result_mode, crate::query::ResultMode::ReferencesOnly);
        assert_eq!(metadata, Some(KeyMaterial::Bool(true)));
    }

    #[test]
    fn legacy_paths_resolve_to_canonical_phase4_types() {
        let request = QueryRequest::new(index_id(0xbb), KeyMaterial::text("canonical"))
            .expect("request should be valid");

        let result = QueryResult::new(index_id(0xbb), version(6), Vec::new())
            .expect("result should be valid");

        let _: crate::query::QueryRequest = request;
        let _: crate::query::QueryResult = result;
    }
}
