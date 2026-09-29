//! Typed Indexing response contract for the Phase 5 event boundary.
//!
//! The response preserves the accepted [`IndexEvent`] and adds the
//! [`IndexAssignedId`] produced by the Indexing Engine. This makes the
//! successful result explicit without mutating Core's `UniversalRequest` or
//! reinterpreting the source-owned payload.
//!
//! The surrounding Core capability/request infrastructure remains responsible
//! for the `UniversalResponse` envelope, response interaction, message/event
//! identity, operation/correlation context, transport, retry, cancellation,
//! and deadline semantics.

use core::fmt;

use crate::event::IndexEvent;
use crate::identity::IndexAssignedId;
use crate::index::IndexVersionId;

/// Typed result carried by an Indexing capability response.
///
/// The response contains the same logical event accepted by Indexing plus the
/// identity assigned to the referenced source object. Core identities and
/// transport semantics remain outside this value.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IndexEventResponse {
    event: IndexEvent,
    assigned_id: IndexAssignedId,
    version: Option<IndexVersionId>,
    technical_result: Option<String>,
}

impl IndexEventResponse {
    /// Creates a successful response containing the accepted event and its
    /// Indexing-assigned entity identity.
    #[must_use]
    pub fn new(event: IndexEvent, assigned_id: IndexAssignedId) -> Self {
        Self {
            event,
            assigned_id,
            version: None,
            technical_result: None,
        }
    }

    /// Creates a response with optional logical version metadata.
    #[must_use]
    pub fn with_version(
        event: IndexEvent,
        assigned_id: IndexAssignedId,
        version: IndexVersionId,
    ) -> Self {
        Self {
            event,
            assigned_id,
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

    /// Returns the accepted Indexing event unchanged.
    #[must_use]
    pub fn event(&self) -> &IndexEvent {
        &self.event
    }

    /// Returns the identity assigned to the referenced source object.
    #[must_use]
    pub fn assigned_id(&self) -> &IndexAssignedId {
        &self.assigned_id
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

    /// Consumes the response and returns the event together with its assigned
    /// entity identity.
    #[must_use]
    pub fn into_parts(self) -> (IndexEvent, IndexAssignedId) {
        (self.event, self.assigned_id)
    }

    /// Consumes the response and returns the accepted event.
    #[must_use]
    pub fn into_event(self) -> IndexEvent {
        self.event
    }

    /// Consumes the response and returns only the assigned entity identity.
    #[must_use]
    pub fn into_assigned_id(self) -> IndexAssignedId {
        self.assigned_id
    }
}

impl fmt::Display for IndexEventResponse {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "IndexEventResponse(assigned_id={})",
            self.assigned_id
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::EntityType;
    use crate::identity::{INDEX_ASSIGNED_ID_BYTE_LEN, IndexNamespace};
    use crate::index::{
        ConsistencyRequirement, IndexFamily, KeyDefinition, KeyMaterial, ObjectReference,
        TargetReferenceType, Uniqueness,
    };
    use crate::requirement::IndexRequirement;
    use nizaam_core::contracts::{
        ContractDescriptor, ContractMetadata, EncodedPayload, Interaction, MessageEnvelope,
        Participants, PayloadDescriptor, UniversalRequest, Version,
    };
    use nizaam_core::identity::{CapabilityId, ContractId, CorrelationId, EngineId, MessageId};
    use nizaam_core::operation::{Operation, OperationContext};

    fn event() -> IndexEvent {
        let capability =
            CapabilityId::new("nizaam.indexing.event.response.test").expect("capability id");
        let contract =
            ContractId::new("nizaam.indexing.event.response.contract").expect("contract id");
        let version = Version::new(1, 0, 0);
        let payload_descriptor =
            PayloadDescriptor::new("application/octet-stream", version.clone())
                .expect("payload descriptor");
        let descriptor = ContractDescriptor::new(
            contract,
            capability,
            version,
            Interaction::Request,
            payload_descriptor.clone(),
        );
        let participants = Participants::new(
            EngineId::new("nizaam.source.test").expect("source id"),
            EngineId::new("nizaam.indexing.test").expect("target id"),
        );
        let metadata = ContractMetadata::new(descriptor, participants);
        let operation = Operation::new(
            nizaam_core::identity::OperationId::new("index-event.response.operation")
                .expect("operation id"),
            CorrelationId::new("index-event.response.correlation").expect("correlation id"),
        );
        let envelope = MessageEnvelope::new(
            MessageId::new("index-event.response.message").expect("message id"),
            OperationContext::new(operation),
            metadata,
            EncodedPayload::new(payload_descriptor, b"source-owned-payload".to_vec()),
        );
        let request = UniversalRequest::new(envelope);

        let requirement = IndexRequirement::new(
            IndexNamespace::new("logical").expect("namespace"),
            IndexFamily::Inverted,
            KeyDefinition::new(["term"]).expect("key definition"),
            TargetReferenceType::new("source.object").expect("target type"),
            Uniqueness::NonUnique,
            ConsistencyRequirement::new("logical").expect("consistency"),
            None,
            None,
        )
        .expect("requirement");

        IndexEvent::new(
            request,
            EntityType::new("word").expect("entity type"),
            requirement,
            ObjectReference::new("nizaam.source.test", "object:1").expect("object reference"),
            KeyMaterial::text("term"),
        )
        .expect("IndexEvent must be valid")
    }

    fn assigned_id() -> IndexAssignedId {
        IndexAssignedId::from_parts(
            crate::index::TargetReferenceType::new("source.object").expect("target type"),
            [0x11; INDEX_ASSIGNED_ID_BYTE_LEN],
        )
    }

    #[test]
    fn primary_success_result_preserves_event_and_returns_assigned_id() {
        let event = event();
        let assigned_id = assigned_id();
        let response = IndexEventResponse::new(event.clone(), assigned_id.clone());

        assert_eq!(response.event(), &event);
        assert_eq!(response.assigned_id(), &assigned_id);
        assert!(response.version().is_none());
        assert!(response.technical_result().is_none());
    }

    #[test]
    fn logical_version_metadata_is_optional() {
        let event = event();
        let assigned_id = assigned_id();
        let version = IndexVersionId::new("v1").expect("test IndexVersionId must be valid");
        let response = IndexEventResponse::with_version(event, assigned_id, version.clone());

        assert_eq!(response.version(), Some(&version));
    }

    #[test]
    fn technical_result_information_is_optional_and_indexing_owned() {
        let response = IndexEventResponse::new(event(), assigned_id())
            .with_technical_result("logical entity assignment completed");

        assert_eq!(
            response.technical_result(),
            Some("logical entity assignment completed")
        );
    }

    #[test]
    fn response_preserves_event_and_all_typed_content() {
        let event = event();
        let assigned_id = assigned_id();
        let version = IndexVersionId::new("v1").expect("test IndexVersionId must be valid");
        let response =
            IndexEventResponse::with_version(event.clone(), assigned_id.clone(), version.clone())
                .with_technical_result("updated");

        assert_eq!(response.event(), &event);
        assert_eq!(response.assigned_id(), &assigned_id);
        assert_eq!(response.version(), Some(&version));
        assert_eq!(response.technical_result(), Some("updated"));
        assert_eq!(
            response.to_string(),
            format!("IndexEventResponse(assigned_id={})", assigned_id)
        );
    }

    #[test]
    fn response_consumption_returns_event_and_assigned_id() {
        let event = event();
        let assigned_id = assigned_id();
        let response = IndexEventResponse::new(event.clone(), assigned_id.clone());

        let (returned_event, returned_id) = response.into_parts();

        assert_eq!(returned_event, event);
        assert_eq!(returned_id, assigned_id);
    }

    #[test]
    fn response_does_not_create_a_second_core_protocol_identity() {
        let event = event();
        let response = IndexEventResponse::new(event.clone(), assigned_id());

        assert_eq!(response.event().event_id(), event.event_id());
        assert_eq!(response.event().message_id(), event.message_id());
        assert_eq!(
            response.event().operation_context(),
            event.operation_context()
        );
    }
}
