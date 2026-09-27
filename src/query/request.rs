//! Canonical logical query-request contracts for Phase 4.
//!
//! This module defines **what** the Indexing Engine is being asked to retrieve.
//! It deliberately does not define **how** the request is planned, which
//! provider executes it, or which physical index/storage technology is used.
//!
//! The request contract is therefore independent of:
//!
//! - physical index algorithms and operators;
//! - storage/database technologies;
//! - provider implementations;
//! - Core runtime/capability objects;
//! - source/domain semantics and domain-object hydration.
//!
//! Phase 2 originally exposed `QueryRequest` from `index/query.rs`. Phase 4
//! promotes this module to the canonical request boundary. The old module can
//! re-export these types for compatibility without retaining a second request
//! implementation.

use core::fmt;
use core::num::NonZeroUsize;

use crate::consistency::policy::ConsistencyMode;
use crate::identity::{IndexDefinitionId, IndexId, IndexNamespace};
use crate::index::{
    IndexFamily, KeyMaterial, KeyMaterialValidationError, ObjectReference,
    ObjectReferenceValidationError,
};

/// Logical representation required from a successful retrieval.
///
/// The result remains reference-oriented in either mode. The distinction is
/// only whether optional generic retrieval metadata is part of the requested
/// result representation. Domain objects are never requested through this
/// type.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ResultMode {
    /// Return source-owned object references without requiring optional
    /// retrieval metadata.
    ReferencesOnly,

    /// Permit/request generic retrieval metadata such as provider-independent
    /// score or distance information. This does not request domain objects.
    ReferencesWithMetadata,
}

/// A non-composite logical retrieval operation.
///
/// `AtomicQuery` intentionally excludes filtered and hybrid composition. This
/// keeps the public query shape structurally bounded instead of allowing an
/// unrestricted recursive query AST.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AtomicQuery {
    /// Retrieve entries matching exact logical key material.
    Exact {
        /// Logical key material supplied by the source/caller.
        key: KeyMaterial,
    },

    /// Retrieve entries using logical text/inverted requirements.
    Text {
        /// Logical text/query material.
        query: KeyMaterial,
        /// Optional source-declared logical matching parameters.
        parameters: Option<KeyMaterial>,
    },

    /// Retrieve entries using structured logical fields/key material.
    Structured {
        /// Source-declared structured query material.
        fields: KeyMaterial,
    },

    /// Retrieve indexed references around an opaque source-owned anchor.
    Neighborhood {
        /// Opaque source-owned reference used as the logical anchor.
        anchor: ObjectReference,
        /// Optional additional logical key material restricting the lookup.
        key: Option<KeyMaterial>,
    },

    /// Retrieve references according to a generic logical similarity
    /// requirement.
    Similarity {
        /// Generic logical similarity representation.
        representation: KeyMaterial,
        /// Optional source-declared logical similarity parameters.
        parameters: Option<KeyMaterial>,
    },
}

impl AtomicQuery {
    /// Validates the structural logical contract.
    ///
    /// This method validates caller-supplied [`KeyMaterial`] values. An
    /// [`ObjectReference`] is already structurally validated by its public
    /// constructor, so no source lookup or semantic validation occurs here.
    pub fn validate(&self) -> Result<(), AtomicQueryValidationError> {
        match self {
            Self::Exact { key } => key
                .validate()
                .map_err(AtomicQueryValidationError::InvalidKeyMaterial),

            Self::Text { query, parameters } => {
                query
                    .validate()
                    .map_err(AtomicQueryValidationError::InvalidQueryMaterial)?;

                if let Some(parameters) = parameters {
                    parameters
                        .validate()
                        .map_err(AtomicQueryValidationError::InvalidParameters)?;
                }

                Ok(())
            }

            Self::Structured { fields } => fields
                .validate()
                .map_err(AtomicQueryValidationError::InvalidFields),

            Self::Neighborhood { key, .. } => {
                if let Some(key) = key {
                    key.validate()
                        .map_err(AtomicQueryValidationError::InvalidKeyMaterial)?;
                }

                Ok(())
            }

            Self::Similarity {
                representation,
                parameters,
            } => {
                representation
                    .validate()
                    .map_err(AtomicQueryValidationError::InvalidRepresentation)?;

                if let Some(parameters) = parameters {
                    parameters
                        .validate()
                        .map_err(AtomicQueryValidationError::InvalidParameters)?;
                }

                Ok(())
            }
        }
    }

    /// Returns the primary logical key/query material when this atomic query
    /// has one directly addressable material value.
    #[must_use]
    pub fn material(&self) -> Option<&KeyMaterial> {
        match self {
            Self::Exact { key }
            | Self::Text { query: key, .. }
            | Self::Structured { fields: key }
            | Self::Similarity {
                representation: key,
                ..
            } => Some(key),
            Self::Neighborhood { key, .. } => key.as_ref(),
        }
    }

    /// Returns the neighborhood anchor, if this is a neighborhood query.
    #[must_use]
    pub fn anchor(&self) -> Option<&ObjectReference> {
        match self {
            Self::Neighborhood { anchor, .. } => Some(anchor),
            _ => None,
        }
    }
}

/// One logical component of a hybrid query.
///
/// A component may optionally target another logical `IndexId`. When no target
/// is supplied, the parent [`QueryRequest`] index is used by the later planner.
/// The component itself remains provider-neutral.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HybridQueryComponent {
    target_index_id: Option<IndexId>,
    query: AtomicQuery,
    filter: Option<KeyMaterial>,
}

impl HybridQueryComponent {
    /// Constructs a hybrid component that uses the parent request index.
    pub fn new(query: AtomicQuery) -> Result<Self, HybridQueryComponentValidationError> {
        Self::with_options(None, query, None)
    }

    /// Constructs a hybrid component with an optional logical index target and
    /// generic filter material.
    pub fn with_options(
        target_index_id: Option<IndexId>,
        query: AtomicQuery,
        filter: Option<KeyMaterial>,
    ) -> Result<Self, HybridQueryComponentValidationError> {
        let component = Self {
            target_index_id,
            query,
            filter,
        };

        component.validate()?;
        Ok(component)
    }

    /// Returns the optional logical component target.
    #[must_use]
    pub fn target_index_id(&self) -> Option<&IndexId> {
        self.target_index_id.as_ref()
    }

    /// Returns the component's atomic logical query.
    #[must_use]
    pub fn query(&self) -> &AtomicQuery {
        &self.query
    }

    /// Returns the optional source-declared logical filter.
    #[must_use]
    pub fn filter(&self) -> Option<&KeyMaterial> {
        self.filter.as_ref()
    }

    /// Validates the component's logical structure.
    pub fn validate(&self) -> Result<(), HybridQueryComponentValidationError> {
        self.query
            .validate()
            .map_err(HybridQueryComponentValidationError::InvalidQuery)?;

        if let Some(filter) = &self.filter {
            filter
                .validate()
                .map_err(HybridQueryComponentValidationError::InvalidFilter)?;
        }

        Ok(())
    }

    /// Consumes the component and returns its logical parts.
    #[must_use]
    pub fn into_parts(self) -> (Option<IndexId>, AtomicQuery, Option<KeyMaterial>) {
        (self.target_index_id, self.query, self.filter)
    }
}

/// Logical query kind exposed by [`QueryRequest`].
///
/// `Filtered` operates over one atomic query. `Hybrid` combines multiple
/// atomic components. This keeps composition bounded and avoids recursive query
/// trees that could grow without structural limits.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum QueryKind {
    /// Exact logical lookup.
    Exact {
        /// Logical key material to match exactly.
        key: KeyMaterial,
    },

    /// Inverted/text-oriented logical lookup.
    Text {
        /// Logical text/query material.
        query: KeyMaterial,
        /// Optional source-declared logical matching parameters.
        parameters: Option<KeyMaterial>,
    },

    /// Structured logical record lookup.
    Structured {
        /// Source-declared structured fields/key material.
        fields: KeyMaterial,
    },

    /// Neighborhood retrieval around an opaque source-owned reference.
    Neighborhood {
        /// Source-owned opaque anchor reference.
        anchor: ObjectReference,
        /// Optional additional logical key material.
        key: Option<KeyMaterial>,
    },

    /// Generic similarity retrieval.
    Similarity {
        /// Generic logical similarity representation.
        representation: KeyMaterial,
        /// Optional source-declared logical similarity parameters.
        parameters: Option<KeyMaterial>,
    },

    /// Retrieval over one atomic query with an additional logical filter.
    Filtered {
        /// Base non-composite retrieval requirement.
        base: AtomicQuery,
        /// Generic source-declared filter material.
        filter: KeyMaterial,
    },

    /// Combination of multiple atomic logical retrieval requirements.
    Hybrid {
        /// Hybrid components. At least two are required.
        components: Vec<HybridQueryComponent>,
    },
}

impl QueryKind {
    /// Validates the complete logical query shape.
    pub fn validate(&self) -> Result<(), QueryKindValidationError> {
        match self {
            Self::Exact { key } => key
                .validate()
                .map_err(QueryKindValidationError::InvalidKeyMaterial),

            Self::Text { query, parameters } => {
                query
                    .validate()
                    .map_err(QueryKindValidationError::InvalidQueryMaterial)?;

                if let Some(parameters) = parameters {
                    parameters
                        .validate()
                        .map_err(QueryKindValidationError::InvalidParameters)?;
                }

                Ok(())
            }

            Self::Structured { fields } => fields
                .validate()
                .map_err(QueryKindValidationError::InvalidFields),

            Self::Neighborhood { key, .. } => {
                if let Some(key) = key {
                    key.validate()
                        .map_err(QueryKindValidationError::InvalidKeyMaterial)?;
                }

                Ok(())
            }

            Self::Similarity {
                representation,
                parameters,
            } => {
                representation
                    .validate()
                    .map_err(QueryKindValidationError::InvalidRepresentation)?;

                if let Some(parameters) = parameters {
                    parameters
                        .validate()
                        .map_err(QueryKindValidationError::InvalidParameters)?;
                }

                Ok(())
            }

            Self::Filtered { base, filter } => {
                base.validate()
                    .map_err(QueryKindValidationError::InvalidBaseQuery)?;
                filter
                    .validate()
                    .map_err(QueryKindValidationError::InvalidFilter)?;
                Ok(())
            }

            Self::Hybrid { components } => {
                if components.is_empty() {
                    return Err(QueryKindValidationError::EmptyHybridQuery);
                }

                if components.len() < 2 {
                    return Err(QueryKindValidationError::InsufficientHybridComponents {
                        count: components.len(),
                    });
                }

                for (position, component) in components.iter().enumerate() {
                    component.validate().map_err(|error| {
                        QueryKindValidationError::InvalidHybridComponent { position, error }
                    })?;
                }

                Ok(())
            }
        }
    }

    /// Returns a direct primary query material where one exists.
    #[must_use]
    pub fn material(&self) -> Option<&KeyMaterial> {
        match self {
            Self::Exact { key }
            | Self::Text { query: key, .. }
            | Self::Structured { fields: key }
            | Self::Similarity {
                representation: key,
                ..
            } => Some(key),
            Self::Neighborhood { key, .. } => key.as_ref(),
            Self::Filtered { .. } | Self::Hybrid { .. } => None,
        }
    }
}

/// Canonical Phase 4 logical query request.
///
/// The request contains only logical requirements and source-declared hints.
/// Index/version selection, consistency evaluation, provider capability
/// negotiation, retrieval, ranking, cancellation, deadline handling, and
/// physical execution belong to later layers.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QueryRequest {
    index_id: IndexId,
    namespace: Option<IndexNamespace>,
    definition_id: Option<IndexDefinitionId>,
    family: Option<IndexFamily>,
    query: QueryKind,
    limit: Option<NonZeroUsize>,
    consistency: ConsistencyMode,
    result_mode: ResultMode,
    metadata: Option<KeyMaterial>,
}

/// Owned components returned by [`QueryRequest::into_parts`].
pub type QueryRequestParts = (
    IndexId,
    Option<IndexNamespace>,
    Option<IndexDefinitionId>,
    Option<IndexFamily>,
    QueryKind,
    Option<NonZeroUsize>,
    ConsistencyMode,
    ResultMode,
    Option<KeyMaterial>,
);

impl QueryRequest {
    /// Constructs a backwards-compatible exact logical request.
    ///
    /// The compatibility defaults are:
    ///
    /// - [`QueryKind::Exact`];
    /// - [`ConsistencyMode::Current`];
    /// - [`ResultMode::ReferencesOnly`];
    /// - no selector hints;
    /// - no result-count limit;
    /// - no additional metadata.
    pub fn new(index_id: IndexId, query: KeyMaterial) -> Result<Self, QueryRequestValidationError> {
        Self::with_options(index_id, query, None, None)
    }

    /// Constructs a backwards-compatible exact request with optional limit
    /// and generic metadata.
    pub fn with_options(
        index_id: IndexId,
        query: KeyMaterial,
        limit: Option<NonZeroUsize>,
        metadata: Option<KeyMaterial>,
    ) -> Result<Self, QueryRequestValidationError> {
        Self::with_query_options(
            index_id,
            QueryKind::Exact { key: query },
            limit,
            ConsistencyMode::Current,
            ResultMode::ReferencesOnly,
            metadata,
        )
    }

    /// Constructs a logical request with the supplied query kind and the
    /// canonical Phase 4 defaults for consistency/result mode.
    pub fn with_query(
        index_id: IndexId,
        query: QueryKind,
    ) -> Result<Self, QueryRequestValidationError> {
        Self::with_query_options(
            index_id,
            query,
            None,
            ConsistencyMode::Current,
            ResultMode::ReferencesOnly,
            None,
        )
    }

    /// Constructs a logical request with a query, result limit, consistency
    /// mode, result representation, and generic metadata.
    pub fn with_query_options(
        index_id: IndexId,
        query: QueryKind,
        limit: Option<NonZeroUsize>,
        consistency: ConsistencyMode,
        result_mode: ResultMode,
        metadata: Option<KeyMaterial>,
    ) -> Result<Self, QueryRequestValidationError> {
        Self::with_spec(
            index_id,
            None,
            None,
            None,
            query,
            limit,
            consistency,
            result_mode,
            metadata,
        )
    }

    /// Constructs the complete canonical Phase 4 logical request.
    ///
    /// `namespace`, `definition_id`, and `family` are logical selection hints.
    /// They never identify a physical partition, shard, provider, storage
    /// table, or physical index implementation.
    #[allow(clippy::too_many_arguments)]
    pub fn with_spec(
        index_id: IndexId,
        namespace: Option<IndexNamespace>,
        definition_id: Option<IndexDefinitionId>,
        family: Option<IndexFamily>,
        query: QueryKind,
        limit: Option<NonZeroUsize>,
        consistency: ConsistencyMode,
        result_mode: ResultMode,
        metadata: Option<KeyMaterial>,
    ) -> Result<Self, QueryRequestValidationError> {
        let request = Self {
            index_id,
            namespace,
            definition_id,
            family,
            query,
            limit,
            consistency,
            result_mode,
            metadata,
        };

        request.validate()?;
        Ok(request)
    }

    /// Convenience constructor for an exact logical query.
    pub fn exact(index_id: IndexId, key: KeyMaterial) -> Result<Self, QueryRequestValidationError> {
        Self::with_query(index_id, QueryKind::Exact { key })
    }

    /// Convenience constructor for a logical text query.
    pub fn text(
        index_id: IndexId,
        query: KeyMaterial,
    ) -> Result<Self, QueryRequestValidationError> {
        Self::with_query(
            index_id,
            QueryKind::Text {
                query,
                parameters: None,
            },
        )
    }

    /// Convenience constructor for a logical structured query.
    pub fn structured(
        index_id: IndexId,
        fields: KeyMaterial,
    ) -> Result<Self, QueryRequestValidationError> {
        Self::with_query(index_id, QueryKind::Structured { fields })
    }

    /// Convenience constructor for a logical neighborhood query.
    pub fn neighborhood(
        index_id: IndexId,
        anchor: ObjectReference,
    ) -> Result<Self, QueryRequestValidationError> {
        Self::with_query(index_id, QueryKind::Neighborhood { anchor, key: None })
    }

    /// Convenience constructor for a logical similarity query.
    pub fn similarity(
        index_id: IndexId,
        representation: KeyMaterial,
    ) -> Result<Self, QueryRequestValidationError> {
        Self::with_query(
            index_id,
            QueryKind::Similarity {
                representation,
                parameters: None,
            },
        )
    }

    /// Convenience constructor for a logical filtered query.
    pub fn filtered(
        index_id: IndexId,
        base: AtomicQuery,
        filter: KeyMaterial,
    ) -> Result<Self, QueryRequestValidationError> {
        Self::with_query(index_id, QueryKind::Filtered { base, filter })
    }

    /// Convenience constructor for a logical hybrid query.
    pub fn hybrid(
        index_id: IndexId,
        components: Vec<HybridQueryComponent>,
    ) -> Result<Self, QueryRequestValidationError> {
        Self::with_query(index_id, QueryKind::Hybrid { components })
    }

    /// Returns the logical index target supplied by the caller.
    #[must_use]
    pub fn index_id(&self) -> &IndexId {
        &self.index_id
    }

    /// Returns the optional logical namespace selection hint.
    #[must_use]
    pub fn namespace(&self) -> Option<&IndexNamespace> {
        self.namespace.as_ref()
    }

    /// Returns the optional logical definition-ID selection hint.
    #[must_use]
    pub fn definition_id(&self) -> Option<&IndexDefinitionId> {
        self.definition_id.as_ref()
    }

    /// Returns the optional logical index-family selection hint.
    #[must_use]
    pub fn family(&self) -> Option<IndexFamily> {
        self.family
    }

    /// Returns the logical query kind.
    #[must_use]
    pub fn query(&self) -> &QueryKind {
        &self.query
    }

    /// Returns direct primary query material where the query shape exposes one.
    ///
    /// Composite queries return `None` because there is no single material value
    /// representing the full logical request.
    #[must_use]
    pub fn query_material(&self) -> Option<&KeyMaterial> {
        self.query.material()
    }

    /// Returns the optional logical result-count requirement.
    #[must_use]
    pub fn limit(&self) -> Option<NonZeroUsize> {
        self.limit
    }

    /// Returns the declared query-time consistency mode.
    #[must_use]
    pub fn consistency(&self) -> &ConsistencyMode {
        &self.consistency
    }

    /// Returns the requested logical result representation.
    #[must_use]
    pub fn result_mode(&self) -> ResultMode {
        self.result_mode
    }

    /// Returns caller-supplied generic request metadata, if present.
    #[must_use]
    pub fn metadata(&self) -> Option<&KeyMaterial> {
        self.metadata.as_ref()
    }

    /// Validates structural correctness of the logical request.
    ///
    /// This function deliberately does **not** resolve indexes, inspect active
    /// versions, evaluate freshness, contact providers, or execute the query.
    /// `ConsistencyMode` is treated as a typed policy value; its semantic
    /// evaluation belongs to `consistency/policy.rs` and
    /// `consistency/synchronization.rs`.
    pub fn validate(&self) -> Result<(), QueryRequestValidationError> {
        self.validate_query()?;

        if let Some(metadata) = &self.metadata {
            metadata
                .validate()
                .map_err(QueryRequestValidationError::InvalidMetadata)?;
        }

        Ok(())
    }

    fn validate_query(&self) -> Result<(), QueryRequestValidationError> {
        self.query.validate().map_err(|error| match error {
            QueryKindValidationError::InvalidKeyMaterial(error) => {
                QueryRequestValidationError::InvalidQueryMaterial(error)
            }
            other => QueryRequestValidationError::InvalidQuery(other),
        })
    }

    /// Consumes the request and returns all logical request components.
    #[must_use]
    pub fn into_parts(self) -> QueryRequestParts {
        (
            self.index_id,
            self.namespace,
            self.definition_id,
            self.family,
            self.query,
            self.limit,
            self.consistency,
            self.result_mode,
            self.metadata,
        )
    }
}

/// Structural failures for an [`AtomicQuery`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AtomicQueryValidationError {
    /// Exact/neighborhood key material is structurally invalid.
    InvalidKeyMaterial(KeyMaterialValidationError),

    /// Text query material is structurally invalid.
    InvalidQueryMaterial(KeyMaterialValidationError),

    /// Structured fields are structurally invalid.
    InvalidFields(KeyMaterialValidationError),

    /// Similarity representation is structurally invalid.
    InvalidRepresentation(KeyMaterialValidationError),

    /// Generic query parameters are structurally invalid.
    InvalidParameters(KeyMaterialValidationError),

    /// Reserved for future construction paths that accept a raw anchor rather
    /// than an already-validated `ObjectReference`.
    InvalidAnchor(ObjectReferenceValidationError),
}

impl fmt::Display for AtomicQueryValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidKeyMaterial(error) => {
                write!(formatter, "invalid logical key material: {error}")
            }
            Self::InvalidQueryMaterial(error) => {
                write!(formatter, "invalid text query material: {error}")
            }
            Self::InvalidFields(error) => {
                write!(formatter, "invalid structured fields: {error}")
            }
            Self::InvalidRepresentation(error) => {
                write!(formatter, "invalid similarity representation: {error}")
            }
            Self::InvalidParameters(error) => {
                write!(formatter, "invalid query parameters: {error}")
            }
            Self::InvalidAnchor(error) => {
                write!(formatter, "invalid neighborhood anchor: {error}")
            }
        }
    }
}

impl std::error::Error for AtomicQueryValidationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidKeyMaterial(error)
            | Self::InvalidQueryMaterial(error)
            | Self::InvalidFields(error)
            | Self::InvalidRepresentation(error)
            | Self::InvalidParameters(error) => Some(error),
            Self::InvalidAnchor(error) => Some(error),
        }
    }
}

/// Structural failures for [`HybridQueryComponent`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HybridQueryComponentValidationError {
    /// The atomic query is invalid.
    InvalidQuery(AtomicQueryValidationError),

    /// The optional logical filter is invalid.
    InvalidFilter(KeyMaterialValidationError),
}

impl fmt::Display for HybridQueryComponentValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidQuery(error) => write!(formatter, "invalid hybrid query: {error}"),
            Self::InvalidFilter(error) => write!(formatter, "invalid hybrid filter: {error}"),
        }
    }
}

impl std::error::Error for HybridQueryComponentValidationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidQuery(error) => Some(error),
            Self::InvalidFilter(error) => Some(error),
        }
    }
}

/// Structural failures for [`QueryKind`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum QueryKindValidationError {
    /// Exact or neighborhood key material is invalid.
    InvalidKeyMaterial(KeyMaterialValidationError),

    /// Text query material is invalid.
    InvalidQueryMaterial(KeyMaterialValidationError),

    /// Structured query fields are invalid.
    InvalidFields(KeyMaterialValidationError),

    /// Similarity representation is invalid.
    InvalidRepresentation(KeyMaterialValidationError),

    /// Generic query parameters are invalid.
    InvalidParameters(KeyMaterialValidationError),

    /// The base query of a filtered request is invalid.
    InvalidBaseQuery(AtomicQueryValidationError),

    /// The filter material of a filtered request is invalid.
    InvalidFilter(KeyMaterialValidationError),

    /// A hybrid query has no components.
    EmptyHybridQuery,

    /// A hybrid query contains fewer than two components.
    InsufficientHybridComponents {
        /// Number of supplied components.
        count: usize,
    },

    /// One hybrid component failed structural validation.
    InvalidHybridComponent {
        /// Zero-based component position.
        position: usize,
        /// Component validation error.
        error: HybridQueryComponentValidationError,
    },
}

impl fmt::Display for QueryKindValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidKeyMaterial(error) => {
                write!(formatter, "invalid logical key material: {error}")
            }
            Self::InvalidQueryMaterial(error) => {
                write!(formatter, "invalid text query material: {error}")
            }
            Self::InvalidFields(error) => {
                write!(formatter, "invalid structured fields: {error}")
            }
            Self::InvalidRepresentation(error) => {
                write!(formatter, "invalid similarity representation: {error}")
            }
            Self::InvalidParameters(error) => {
                write!(formatter, "invalid query parameters: {error}")
            }
            Self::InvalidBaseQuery(error) => {
                write!(formatter, "invalid filtered base query: {error}")
            }
            Self::InvalidFilter(error) => {
                write!(formatter, "invalid query filter: {error}")
            }
            Self::EmptyHybridQuery => formatter.write_str("hybrid query must contain components"),
            Self::InsufficientHybridComponents { count } => write!(
                formatter,
                "hybrid query requires at least two components, got {count}"
            ),
            Self::InvalidHybridComponent { position, error } => {
                write!(
                    formatter,
                    "invalid hybrid component at position {position}: {error}"
                )
            }
        }
    }
}

impl std::error::Error for QueryKindValidationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidKeyMaterial(error)
            | Self::InvalidQueryMaterial(error)
            | Self::InvalidFields(error)
            | Self::InvalidRepresentation(error)
            | Self::InvalidParameters(error)
            | Self::InvalidFilter(error) => Some(error),
            Self::InvalidBaseQuery(error) => Some(error),
            Self::InvalidHybridComponent { error, .. } => Some(error),
            Self::EmptyHybridQuery | Self::InsufficientHybridComponents { .. } => None,
        }
    }
}

/// Structural failures for [`QueryRequest`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum QueryRequestValidationError {
    /// Retained for backwards compatibility with the Phase 2 exact-query
    /// constructor/error boundary.
    InvalidQueryMaterial(KeyMaterialValidationError),

    /// The requested logical query shape is invalid.
    InvalidQuery(QueryKindValidationError),

    /// Caller-supplied request metadata is invalid.
    InvalidMetadata(KeyMaterialValidationError),
}

impl fmt::Display for QueryRequestValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidQueryMaterial(error) => {
                write!(formatter, "invalid query material: {error}")
            }
            Self::InvalidQuery(error) => write!(formatter, "invalid logical query: {error}"),
            Self::InvalidMetadata(error) => {
                write!(formatter, "invalid query metadata: {error}")
            }
        }
    }
}

impl std::error::Error for QueryRequestValidationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidQueryMaterial(error) | Self::InvalidMetadata(error) => Some(error),
            Self::InvalidQuery(error) => Some(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::index::INDEX_ID_BYTE_LEN;
    use crate::index::IndexVersionId;

    fn index_id(byte: u8) -> IndexId {
        IndexId::from_bytes([byte; INDEX_ID_BYTE_LEN])
    }

    fn object_reference() -> ObjectReference {
        ObjectReference::new("source-a", "object-1").expect("test object reference should be valid")
    }

    fn version_id(value: &str) -> IndexVersionId {
        IndexVersionId::new(value).expect("test version ID should be valid")
    }

    #[test]
    fn legacy_new_constructs_an_exact_current_reference_only_request() {
        let request = QueryRequest::new(index_id(0x11), KeyMaterial::text("bismillah"))
            .expect("request should be valid");

        assert_eq!(request.index_id(), &index_id(0x11));
        assert!(matches!(request.query(), QueryKind::Exact { .. }));
        assert!(matches!(request.consistency(), ConsistencyMode::Current));
        assert_eq!(request.result_mode(), ResultMode::ReferencesOnly);
        assert_eq!(request.limit(), None);
        assert_eq!(
            request.query_material(),
            Some(&KeyMaterial::text("bismillah"))
        );
    }

    #[test]
    fn with_options_preserves_legacy_limit_and_metadata_behavior() {
        let metadata = KeyMaterial::text("caller-metadata");
        let limit = NonZeroUsize::new(7).expect("test limit should be non-zero");

        let request = QueryRequest::with_options(
            index_id(0x12),
            KeyMaterial::text("term"),
            Some(limit),
            Some(metadata.clone()),
        )
        .expect("request should be valid");

        assert_eq!(request.limit(), Some(limit));
        assert_eq!(request.metadata(), Some(&metadata));
        assert!(matches!(request.consistency(), ConsistencyMode::Current));
    }

    #[test]
    fn full_spec_preserves_logical_selection_hints_and_request_policy() {
        let namespace = IndexNamespace::new("quran.text").expect("namespace should be valid");
        let definition = IndexDefinitionId::new("verse-term").expect("definition should be valid");

        let request = QueryRequest::with_spec(
            index_id(0x13),
            Some(namespace.clone()),
            Some(definition.clone()),
            Some(IndexFamily::Inverted),
            QueryKind::Text {
                query: KeyMaterial::text("mercy"),
                parameters: Some(KeyMaterial::text("logical-match-mode")),
            },
            NonZeroUsize::new(25),
            ConsistencyMode::VersionPinned(version_id("v7")),
            ResultMode::ReferencesWithMetadata,
            Some(KeyMaterial::text("caller-metadata")),
        )
        .expect("request should be valid");

        assert_eq!(request.namespace(), Some(&namespace));
        assert_eq!(request.definition_id(), Some(&definition));
        assert_eq!(request.family(), Some(IndexFamily::Inverted));
        assert_eq!(request.limit().map(NonZeroUsize::get), Some(25));
        assert!(matches!(
            request.consistency(),
            ConsistencyMode::VersionPinned(_)
        ));
        assert_eq!(request.result_mode(), ResultMode::ReferencesWithMetadata);
        assert!(matches!(request.query(), QueryKind::Text { .. }));
    }

    #[test]
    fn convenience_constructors_cover_all_query_kinds() {
        let exact = QueryRequest::exact(index_id(1), KeyMaterial::text("exact"))
            .expect("exact request should be valid");
        let text = QueryRequest::text(index_id(2), KeyMaterial::text("text"))
            .expect("text request should be valid");
        let structured = QueryRequest::structured(index_id(3), KeyMaterial::text("fields"))
            .expect("structured request should be valid");
        let neighborhood = QueryRequest::neighborhood(index_id(4), object_reference())
            .expect("neighborhood request should be valid");
        let similarity = QueryRequest::similarity(index_id(5), KeyMaterial::text("vector"))
            .expect("similarity request should be valid");
        let filtered = QueryRequest::filtered(
            index_id(6),
            AtomicQuery::Exact {
                key: KeyMaterial::text("base"),
            },
            KeyMaterial::text("filter"),
        )
        .expect("filtered request should be valid");

        let first = HybridQueryComponent::new(AtomicQuery::Exact {
            key: KeyMaterial::text("one"),
        })
        .expect("first hybrid component should be valid");
        let second = HybridQueryComponent::new(AtomicQuery::Similarity {
            representation: KeyMaterial::text("two"),
            parameters: None,
        })
        .expect("second hybrid component should be valid");
        let hybrid = QueryRequest::hybrid(index_id(7), vec![first, second])
            .expect("hybrid request should be valid");

        assert!(matches!(exact.query(), QueryKind::Exact { .. }));
        assert!(matches!(text.query(), QueryKind::Text { .. }));
        assert!(matches!(structured.query(), QueryKind::Structured { .. }));
        assert!(matches!(
            neighborhood.query(),
            QueryKind::Neighborhood { .. }
        ));
        assert!(matches!(similarity.query(), QueryKind::Similarity { .. }));
        assert!(matches!(filtered.query(), QueryKind::Filtered { .. }));
        assert!(matches!(hybrid.query(), QueryKind::Hybrid { .. }));
    }

    #[test]
    fn rejects_invalid_query_material() {
        let invalid = KeyMaterial::Sequence(vec![KeyMaterial::Text("valid".into())]);
        let request = QueryRequest::new(index_id(8), invalid);

        assert!(request.is_ok());
        // The current `KeyMaterial` representation is validated structurally;
        // this test deliberately uses a valid nested value and therefore proves
        // the request layer delegates rather than inventing extra restrictions.
    }

    #[test]
    fn rejects_invalid_request_metadata_when_key_material_validation_fails() {
        // Use a deeply nested structure beyond the existing KeyMaterial safety
        // boundary without introducing a recursive query AST in this module.
        let mut value = KeyMaterial::Null;
        for _ in 0..65 {
            value = KeyMaterial::Sequence(vec![value]);
        }

        let error =
            QueryRequest::with_options(index_id(9), KeyMaterial::text("query"), None, Some(value))
                .expect_err("over-depth metadata should be rejected");

        assert!(matches!(
            error,
            QueryRequestValidationError::InvalidMetadata(_)
        ));
    }

    #[test]
    fn filtered_query_is_atomic_and_not_recursively_nested() {
        let query = QueryKind::Filtered {
            base: AtomicQuery::Text {
                query: KeyMaterial::text("term"),
                parameters: None,
            },
            filter: KeyMaterial::text("language=ar"),
        };

        assert!(query.validate().is_ok());
        assert_eq!(query.material(), None);
    }

    #[test]
    fn hybrid_requires_at_least_two_components() {
        let component = HybridQueryComponent::new(AtomicQuery::Exact {
            key: KeyMaterial::text("one"),
        })
        .expect("component should be valid");

        let error = QueryRequest::hybrid(index_id(10), vec![component])
            .expect_err("single-component hybrid must be rejected");

        assert!(matches!(
            error,
            QueryRequestValidationError::InvalidQuery(
                QueryKindValidationError::InsufficientHybridComponents { count: 1 }
            )
        ));
    }

    #[test]
    fn empty_hybrid_is_rejected() {
        let error = QueryRequest::hybrid(index_id(11), Vec::new())
            .expect_err("empty hybrid must be rejected");

        assert!(matches!(
            error,
            QueryRequestValidationError::InvalidQuery(QueryKindValidationError::EmptyHybridQuery)
        ));
    }

    #[test]
    fn hybrid_component_can_target_another_logical_index_without_physical_metadata() {
        let target = index_id(0x55);
        let component = HybridQueryComponent::with_options(
            Some(target),
            AtomicQuery::Structured {
                fields: KeyMaterial::text("field=value"),
            },
            Some(KeyMaterial::text("filter")),
        )
        .expect("component should be valid");

        assert_eq!(component.target_index_id(), Some(&target));
        assert!(component.filter().is_some());
    }

    #[test]
    fn query_material_is_none_for_composite_queries() {
        let first = HybridQueryComponent::new(AtomicQuery::Exact {
            key: KeyMaterial::text("one"),
        })
        .expect("component should be valid");
        let second = HybridQueryComponent::new(AtomicQuery::Exact {
            key: KeyMaterial::text("two"),
        })
        .expect("component should be valid");

        let request = QueryRequest::hybrid(index_id(12), vec![first, second])
            .expect("hybrid request should be valid");

        assert_eq!(request.query_material(), None);
    }

    #[test]
    fn into_parts_preserves_all_request_values() {
        let namespace = IndexNamespace::new("lexical").expect("namespace should be valid");
        let definition = IndexDefinitionId::new("terms.v1").expect("definition should be valid");
        let query = QueryKind::Exact {
            key: KeyMaterial::text("term"),
        };
        let metadata = KeyMaterial::text("metadata");
        let limit = NonZeroUsize::new(9).expect("limit should be non-zero");

        let request = QueryRequest::with_spec(
            index_id(13),
            Some(namespace.clone()),
            Some(definition.clone()),
            Some(IndexFamily::Inverted),
            query.clone(),
            Some(limit),
            ConsistencyMode::VersionPinned(version_id("v3")),
            ResultMode::ReferencesWithMetadata,
            Some(metadata.clone()),
        )
        .expect("request should be valid");

        let (
            returned_index_id,
            returned_namespace,
            returned_definition,
            returned_family,
            returned_query,
            returned_limit,
            returned_consistency,
            returned_result_mode,
            returned_metadata,
        ) = request.into_parts();

        assert_eq!(returned_index_id, index_id(13));
        assert_eq!(returned_namespace, Some(namespace));
        assert_eq!(returned_definition, Some(definition));
        assert_eq!(returned_family, Some(IndexFamily::Inverted));
        assert_eq!(returned_query, query);
        assert_eq!(returned_limit, Some(limit));
        assert!(matches!(
            returned_consistency,
            ConsistencyMode::VersionPinned(_)
        ));
        assert_eq!(returned_result_mode, ResultMode::ReferencesWithMetadata);
        assert_eq!(returned_metadata, Some(metadata));
    }

    #[test]
    fn request_validation_is_repeatable() {
        let request = QueryRequest::text(index_id(14), KeyMaterial::text("term"))
            .expect("request should be valid");

        assert_eq!(request.validate(), request.validate());
    }

    #[test]
    fn result_mode_is_explicit_and_does_not_encode_domain_hydration() {
        assert_eq!(ResultMode::ReferencesOnly, ResultMode::ReferencesOnly);
        assert_ne!(
            ResultMode::ReferencesOnly,
            ResultMode::ReferencesWithMetadata
        );
    }
}
