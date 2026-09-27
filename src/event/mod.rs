//! Phase 5 event boundary for the Indexing Engine.
//!
//! This module is intentionally a thin composition facade over the two typed
//! event-boundary contracts:
//!
//! ```text
//! Core UniversalRequest
//!        |
//!        v
//!   IndexEvent
//!        |
//!        v
//!   Indexing capability
//!        |
//!        v
//! IndexEventResponse
//!        |
//!        v
//! Core UniversalResponse
//! ```
//!
//! [`IndexEvent`] owns only Indexing-specific logical event content while Core
//! remains the owner of universal request/event identity, operation/correlation
//! context, transport metadata, and the request boundary. [`IndexEventResponse`]
//! similarly contains only Indexing-owned result content; the surrounding Core
//! capability layer remains responsible for the `UniversalResponse` envelope,
//! response interaction, identity, transport, retry, cancellation, and deadline
//! semantics.
//!
//! No second event protocol, response protocol, correlation mechanism, retry
//! mechanism, or physical-provider instruction boundary is introduced here.

pub mod index_event;
pub mod index_event_response;

pub use index_event::{IndexEvent, IndexEventResult, IndexEventValidationError};

pub use index_event_response::IndexEventResponse;

#[cfg(test)]
mod tests {
    use super::*;
    use nizaam_core::contracts::{
        ContractDescriptor, ContractMetadata, EncodedPayload, Interaction, MessageEnvelope,
        Participants, PayloadDescriptor, UniversalRequest, Version,
    };
    use nizaam_core::identity::{CapabilityId, ContractId, CorrelationId, EngineId, MessageId};
    use nizaam_core::operation::{Operation, OperationContext};

    use crate::identity::{INDEX_ID_BYTE_LEN, IndexNamespace};
    use crate::index::{
        ConsistencyRequirement, IndexFamily, KeyDefinition, KeyMaterial, ObjectReference,
        TargetReferenceType, Uniqueness,
    };
    use crate::requirement::IndexRequirement;

    fn request() -> UniversalRequest {
        let capability = CapabilityId::new("nizaam.indexing.event.mod.test")
            .expect("capability id must be valid");
        let contract = ContractId::new("nizaam.indexing.event.mod.contract")
            .expect("contract id must be valid");
        let version = Version::new(1, 0, 0);
        let payload_descriptor =
            PayloadDescriptor::new("application/octet-stream", version.clone())
                .expect("payload descriptor must be valid");
        let descriptor = ContractDescriptor::new(
            contract,
            capability,
            version,
            Interaction::Request,
            payload_descriptor.clone(),
        );
        let participants = Participants::new(
            EngineId::new("nizaam.source.test").expect("source id must be valid"),
            EngineId::new("nizaam.indexing.test").expect("target id must be valid"),
        );
        let metadata = ContractMetadata::new(descriptor, participants);
        let operation = Operation::new(
            nizaam_core::identity::OperationId::new("index-event.mod.operation")
                .expect("operation id must be valid"),
            CorrelationId::new("index-event.mod.correlation")
                .expect("correlation id must be valid"),
        );
        let envelope = MessageEnvelope::new(
            MessageId::new("index-event.mod.message").expect("message id must be valid"),
            OperationContext::new(operation),
            metadata,
            EncodedPayload::new(payload_descriptor, b"source-owned-payload".to_vec()),
        );

        UniversalRequest::new(envelope)
    }

    fn requirement() -> IndexRequirement {
        IndexRequirement::new(
            IndexNamespace::new("logical").expect("namespace must be valid"),
            IndexFamily::Inverted,
            KeyDefinition::new(["term"]).expect("key definition must be valid"),
            TargetReferenceType::new("source.object").expect("target type must be valid"),
            Uniqueness::NonUnique,
            ConsistencyRequirement::new("logical").expect("consistency must be valid"),
            None,
            None,
        )
        .expect("requirement must be valid")
    }

    fn event() -> IndexEvent {
        IndexEvent::new(
            request(),
            requirement(),
            ObjectReference::new("nizaam.source.test", "object:1")
                .expect("object reference must be valid"),
            KeyMaterial::text("term"),
        )
        .expect("IndexEvent must be valid")
    }

    fn index_id() -> crate::identity::IndexId {
        crate::identity::IndexId::from_bytes([0x22; INDEX_ID_BYTE_LEN])
    }

    #[test]
    fn public_boundary_exposes_both_typed_contracts() {
        let event = event();
        let index_id = index_id();
        let response = IndexEventResponse::new(index_id);

        assert_eq!(event.requirement().family(), IndexFamily::Inverted);
        assert_eq!(response.index_id(), &index_id);
    }

    #[test]
    fn event_boundary_preserves_core_request_identity_and_context() {
        let event = event();
        let request = event.request();

        assert_eq!(event.event_id(), request.event_id());
        assert_eq!(event.message_id(), request.message_id());
        assert_eq!(
            event.operation_context(),
            &request.universal_event().envelope.operation_context
        );
        assert_eq!(event.source_payload(), b"source-owned-payload");
    }

    #[test]
    fn response_boundary_contains_indexing_result_content_only() {
        let index_id = index_id();
        let response = IndexEventResponse::new(index_id)
            .with_technical_result("logical index assignment completed");

        assert_eq!(response.index_id(), &index_id);
        assert_eq!(
            response.technical_result(),
            Some("logical index assignment completed")
        );
        assert!(response.version().is_none());
    }

    #[test]
    fn event_and_response_do_not_duplicate_core_protocol_ownership() {
        let event = event();
        let response = IndexEventResponse::new(index_id());

        // Core identities and operation/correlation context are read from the
        // UniversalRequest. The typed response contains no corresponding Core
        // identity fields; those belong to UniversalResponse.
        assert_eq!(event.event_id(), event.request().event_id());
        assert_eq!(event.message_id(), event.request().message_id());
        assert!(response.version().is_none());
        assert!(response.technical_result().is_none());
    }

    #[test]
    fn response_is_result_content_not_an_event_interaction() {
        let response = IndexEventResponse::new(index_id());

        // No Core Interaction is embedded in IndexEventResponse. The response
        // interaction is established only when the engine later constructs the
        // Core UniversalResponse envelope.
        let returned_index_id = response.into_index_id();
        assert_eq!(returned_index_id, index_id());
    }
}
