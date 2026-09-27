//! Typed Indexing response contract for the Phase 5 event boundary.
//!
//! The response is intentionally an Indexing-owned result value, not a second
//! response protocol. Its primary success value is the logical [`IndexId`].
//! Optional logical version metadata and bounded Indexing technical-result
//! information may accompany that identifier.
//!
//! The surrounding Core capability/request infrastructure remains responsible
//! for the `UniversalResponse` envelope, response interaction, message/event
//! identity, operation/correlation context, transport, retry, cancellation,
//! and deadline semantics.

use core::fmt;

use crate::identity::IndexId;
use crate::index::IndexVersionId;

/// Typed result carried by an Indexing capability response.
///
/// This is the Indexing-owned content that the surrounding Core capability
/// layer places inside its existing [`nizaam_core::contracts::UniversalResponse`]
/// boundary. It does not create a second response envelope or correlation
/// mechanism.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IndexEventResponse {
    index_id: IndexId,
    version: Option<IndexVersionId>,
    technical_result: Option<String>,
}

impl IndexEventResponse {
    /// Creates a successful response for an assigned logical index.
    ///
    /// The returned identifier is an Indexing logical identifier. It is not a
    /// physical provider/storage identifier.
    #[must_use]
    pub fn new(index_id: IndexId) -> Self {
        Self {
            index_id,
            version: None,
            technical_result: None,
        }
    }

    /// Creates a response with optional logical version metadata.
    #[must_use]
    pub fn with_version(index_id: IndexId, version: IndexVersionId) -> Self {
        Self {
            index_id,
            version: Some(version),
            technical_result: None,
        }
    }

    /// Adds optional Indexing-owned technical result information.
    ///
    /// This value is intentionally opaque technical information. It must not
    /// be used to introduce transport instructions, provider-specific storage
    /// configuration, retry policy, or source-domain semantics.
    #[must_use]
    pub fn with_technical_result(mut self, technical_result: impl Into<String>) -> Self {
        self.technical_result = Some(technical_result.into());
        self
    }

    /// Returns the assigned logical [`IndexId`].
    #[must_use]
    pub fn index_id(&self) -> &IndexId {
        &self.index_id
    }

    /// Returns optional logical version metadata.
    #[must_use]
    pub fn version(&self) -> Option<&IndexVersionId> {
        self.version.as_ref()
    }

    /// Returns optional Indexing technical result information.
    #[must_use]
    pub fn technical_result(&self) -> Option<&str> {
        self.technical_result.as_deref()
    }

    /// Consumes the response and returns its assigned logical index id.
    #[must_use]
    pub fn into_index_id(self) -> IndexId {
        self.index_id
    }
}

impl fmt::Display for IndexEventResponse {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "IndexEventResponse(index_id={:?})",
            self.index_id
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn index_id() -> IndexId {
        IndexId::from_bytes([0x11; crate::identity::INDEX_ID_BYTE_LEN])
    }

    fn version_id() -> IndexVersionId {
        IndexVersionId::new("v1").expect("test IndexVersionId must be valid")
    }

    #[test]
    fn primary_success_result_is_the_logical_index_id() {
        let id = index_id();
        let response = IndexEventResponse::new(id);

        assert_eq!(response.index_id(), &id);
        assert!(response.version().is_none());
        assert!(response.technical_result().is_none());
    }

    #[test]
    fn logical_version_metadata_is_optional() {
        let id = index_id();
        let version = version_id();
        let response = IndexEventResponse::with_version(id, version.clone());

        assert_eq!(response.version(), Some(&version));
    }

    #[test]
    fn technical_result_information_is_optional_and_indexing_owned() {
        let response = IndexEventResponse::new(index_id())
            .with_technical_result("logical index assignment completed");

        assert_eq!(
            response.technical_result(),
            Some("logical index assignment completed")
        );
    }

    #[test]
    fn response_preserves_all_typed_content() {
        let id = index_id();
        let version = version_id();
        let response =
            IndexEventResponse::with_version(id, version.clone()).with_technical_result("updated");

        assert_eq!(response.index_id(), &id);
        assert_eq!(response.version(), Some(&version));
        assert_eq!(response.technical_result(), Some("updated"));
        assert_eq!(
            response.to_string(),
            format!("IndexEventResponse(index_id={:?})", id)
        );
    }

    #[test]
    fn response_consumption_returns_only_the_logical_index_id() {
        let id = index_id();
        let response = IndexEventResponse::new(id);

        assert_eq!(response.into_index_id(), id);
    }

    #[test]
    fn response_has_no_event_or_transport_identity() {
        // The response intentionally contains only Indexing-owned result
        // content. EventId, MessageId, OperationId, correlation, transport,
        // retry, cancellation, and deadline remain Core-owned concerns of the
        // surrounding UniversalResponse boundary.
        let response = IndexEventResponse::new(index_id());

        assert!(response.version().is_none());
        assert!(response.technical_result().is_none());
    }
}
