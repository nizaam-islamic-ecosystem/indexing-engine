//! Phase 5 integration tests for the typed Indexing event boundary.
//!
//! `IndexEvent` is Indexing-owned typed content carried by Core's existing
//! `UniversalRequest`; `IndexEventResponse` is Indexing-owned result content
//! carried by Core's existing response boundary.

use nizaam_indexing::{
    ConsistencyRequirement, IndexEvent, IndexEventResponse, IndexEventValidationError, IndexFamily,
    IndexId, IndexNamespace, IndexRequirement, KeyDefinition, KeyMaterial, ObjectReference,
    TargetReferenceType, Uniqueness,
};

use nizaam_core::contracts::{
    ContractDescriptor, ContractMetadata, EncodedPayload, Interaction, MessageEnvelope,
    Participants, PayloadDescriptor, UniversalRequest, Version,
};
use nizaam_core::identity::{
    CapabilityId, ContractId, CorrelationId, EngineId, EngineInstanceId, MessageId, OperationId,
};
use nizaam_core::operation::{Operation, OperationContext};

fn request(
    sender: EngineId,
    sender_instance: Option<EngineInstanceId>,
    interaction: Interaction,
    payload: &[u8],
) -> UniversalRequest {
    let capability = CapabilityId::new("nizaam.indexing.event.integration")
        .expect("capability id should be valid");
    let contract =
        ContractId::new("nizaam.indexing.event.contract").expect("contract id should be valid");
    let version = Version::new(1, 0, 0);

    let payload_descriptor = PayloadDescriptor::new("application/octet-stream", version.clone())
        .expect("payload descriptor should be valid");

    let descriptor = ContractDescriptor::new(
        contract,
        capability,
        version,
        interaction,
        payload_descriptor.clone(),
    );

    let mut participants = Participants::new(
        sender,
        EngineId::new("nizaam.indexing.integration").expect("target engine id should be valid"),
    );

    if let Some(instance) = sender_instance {
        participants = participants.with_sender_instance(instance);
    }

    let metadata = ContractMetadata::new(descriptor, participants);

    let operation = Operation::new(
        OperationId::new("event.integration.operation").expect("operation id should be valid"),
        CorrelationId::new("event.integration.correlation")
            .expect("correlation id should be valid"),
    );

    let envelope = MessageEnvelope::new(
        MessageId::new("event.integration.message").expect("message id should be valid"),
        OperationContext::new(operation),
        metadata,
        EncodedPayload::new(payload_descriptor, payload.to_vec()),
    );

    UniversalRequest::new(envelope)
}

fn requirement() -> IndexRequirement {
    IndexRequirement::new(
        IndexNamespace::new("logical").expect("index namespace should be valid"),
        IndexFamily::Inverted,
        KeyDefinition::new(["term"]).expect("key definition should be valid"),
        TargetReferenceType::new("source.object").expect("target reference type should be valid"),
        Uniqueness::NonUnique,
        ConsistencyRequirement::new("logical").expect("consistency requirement should be valid"),
        None,
        None,
    )
    .expect("index requirement should be valid")
}

fn object_reference() -> ObjectReference {
    ObjectReference::new("nizaam.source.integration", "object:1")
        .expect("object reference should be valid")
}

fn logical_event(sender_instance: Option<EngineInstanceId>) -> IndexEvent {
    IndexEvent::new(
        request(
            EngineId::new("nizaam.source.integration").expect("source engine id should be valid"),
            sender_instance,
            Interaction::Request,
            b"source-owned-payload",
        ),
        requirement(),
        object_reference(),
        KeyMaterial::text("term"),
    )
    .expect("logical IndexEvent should be valid")
}

fn index_id(seed: u8) -> IndexId {
    IndexId::from_bytes([seed; nizaam_indexing::INDEX_ID_BYTE_LEN])
}

#[test]
fn request_is_wrapped_as_a_typed_index_event_without_rebuilding_core_identity() {
    let sender =
        EngineId::new("nizaam.source.integration").expect("source engine id should be valid");
    let instance = EngineInstanceId::new("nizaam.source.integration.instance")
        .expect("source instance id should be valid");

    let request = request(
        sender.clone(),
        Some(instance.clone()),
        Interaction::Request,
        b"source-owned-payload",
    );
    let event_id = request.event_id().clone();
    let message_id = request.message_id().clone();
    let operation_context = request.universal_event().envelope.operation_context.clone();

    let event = IndexEvent::new(
        request,
        requirement(),
        object_reference(),
        KeyMaterial::text("term"),
    )
    .expect("IndexEvent should be valid");

    assert_eq!(event.source_engine_id(), &sender);
    assert_eq!(event.source_engine_instance_id(), Some(&instance));
    assert_eq!(event.event_id(), &event_id);
    assert_eq!(event.message_id(), &message_id);
    assert_eq!(event.operation_context(), &operation_context);
    assert_eq!(event.source_payload(), b"source-owned-payload");
}

#[test]
fn optional_source_engine_instance_is_preserved_without_being_required() {
    let event = logical_event(None);

    let expected =
        EngineId::new("nizaam.source.integration").expect("source engine id should be valid");
    assert!(event.source_engine_instance_id().is_none());
    assert_eq!(event.source_engine_id(), &expected);
}

#[test]
fn indexing_owned_logical_contract_fields_are_preserved() {
    let event = logical_event(None);

    assert_eq!(event.requirement().family(), IndexFamily::Inverted);
    assert_eq!(
        event.object_reference().source(),
        "nizaam.source.integration"
    );
    assert_eq!(event.object_reference().object_reference(), "object:1");
    assert_eq!(event.key_material(), &KeyMaterial::text("term"));
}

#[test]
fn source_payload_remains_opaque_source_owned_data() {
    let event = logical_event(None);

    assert_eq!(event.source_payload(), b"source-owned-payload");

    // The event contract exposes the payload as bytes only. It does not
    // reinterpret the bytes as a physical provider/storage instruction.
}

#[test]
fn event_validation_accepts_a_valid_request_contract() {
    let event = logical_event(None);

    event
        .validate()
        .expect("valid IndexEvent should pass validation");
}

#[test]
fn response_interaction_cannot_be_accepted_as_an_index_event() {
    let request = request(
        EngineId::new("nizaam.source.integration").expect("source engine id should be valid"),
        None,
        Interaction::Response,
        b"",
    );

    let error = IndexEvent::new(
        request,
        requirement(),
        object_reference(),
        KeyMaterial::Null,
    )
    .expect_err("Core response interaction must not become an IndexEvent");

    assert_eq!(
        error,
        IndexEventValidationError::InvalidInteraction(Interaction::Response)
    );
}

#[test]
fn consuming_index_event_returns_the_original_universal_request() {
    let sender =
        EngineId::new("nizaam.source.integration").expect("source engine id should be valid");
    let request = request(sender, None, Interaction::Request, b"source-owned-payload");

    let expected_event_id = request.event_id().clone();
    let expected_message_id = request.message_id().clone();
    let expected_operation = request.universal_event().envelope.operation_context.clone();
    let expected_payload = request.universal_event().envelope.payload.bytes().to_vec();

    let event = IndexEvent::new(
        request,
        requirement(),
        object_reference(),
        KeyMaterial::Null,
    )
    .expect("event should be valid");

    let returned = event.into_universal_request();

    assert_eq!(returned.event_id(), &expected_event_id);
    assert_eq!(returned.message_id(), &expected_message_id);
    assert_eq!(
        returned.universal_event().envelope.operation_context,
        expected_operation
    );
    assert_eq!(
        returned.universal_event().envelope.payload.bytes(),
        expected_payload.as_slice()
    );
}

#[test]
fn response_primary_success_value_is_the_logical_index_id() {
    let id = index_id(0x11);
    let response = IndexEventResponse::new(id);

    assert_eq!(response.index_id(), &id);
    assert!(response.version().is_none());
    assert!(response.technical_result().is_none());
}

#[test]
fn response_can_carry_optional_logical_version_and_technical_result() {
    let id = index_id(0x22);
    let version = nizaam_indexing::IndexVersionId::new("v1").expect("version id should be valid");

    let response = IndexEventResponse::with_version(id, version.clone())
        .with_technical_result("logical index assignment completed");

    assert_eq!(response.index_id(), &id);
    assert_eq!(response.version(), Some(&version));
    assert_eq!(
        response.technical_result(),
        Some("logical index assignment completed")
    );
}

#[test]
fn response_consumption_returns_the_logical_index_id_only() {
    let id = index_id(0x33);
    let response = IndexEventResponse::new(id);

    assert_eq!(response.into_index_id(), id);
}

#[test]
fn response_content_does_not_duplicate_core_transport_or_correlation_metadata() {
    let response = IndexEventResponse::new(index_id(0x44));

    // The response value itself contains only Indexing-owned result content.
    // Event/message identity, operation/correlation, transport, cancellation,
    // deadline, and retry semantics remain outside this type.
    assert!(response.version().is_none());
    assert!(response.technical_result().is_none());
}

#[test]
fn event_and_response_use_different_protocol_roles() {
    let event = logical_event(None);
    let response = IndexEventResponse::new(index_id(0x55));

    assert_eq!(
        event
            .universal_event()
            .envelope
            .metadata
            .descriptor
            .interaction,
        Interaction::Request
    );
    assert_eq!(response.index_id(), &index_id(0x55));
}
