//! Typed Indexing event contract built on Core's universal request/event model.
//!
//! Phase 5 keeps event ownership deliberately split:
//!
//! ```text
//! Core UniversalRequest
//!     └── UniversalEvent / envelope
//!         ├── EventId
//!         ├── MessageId
//!         ├── OperationContext
//!         ├── Participants
//!         └── payload
//!                 │
//!                 ▼
//!          IndexEvent
//!         ├── EntityType          <- supplied by the source engine
//!         ├── IndexRequirement
//!         ├── ObjectReference
//!         └── KeyMaterial
//! ```
//!
//! `IndexEvent` therefore adds only the logical information required by the
//! Indexing Engine. The source engine supplies `EntityType`; Indexing does not
//! infer domain/entity semantics from the opaque Core payload. Core remains the
//! owner of occurrence identity, engine identity transport metadata,
//! operation/correlation context, cancellation/deadline context, and the
//! UniversalRequest boundary.
//!
//! The event contains no physical provider instruction. There is deliberately
//! no field for a B-tree, HNSW, FAISS, Lucene, GIN, database, table,
//! partition, shard, or other provider/storage choice. The source-owned
//! payload remains opaque to Indexing and is carried by Core unchanged.

use core::fmt;
use std::error::Error;

use nizaam_core::contracts::{Interaction, UniversalEvent, UniversalRequest};
use nizaam_core::identity::{EngineId, EngineInstanceId, EventId, MessageId};
use nizaam_core::operation::OperationContext;

use crate::index::{
    KeyMaterial, KeyMaterialValidationError, ObjectReference, ObjectReferenceValidationError,
};
use crate::requirement::{IndexRequirement, IndexRequirementValidationError};

/// Source-supplied classification of the entity represented by an [`IndexEvent`].
///
/// This value is supplied by the engine that owns the source entity. The
/// Indexing Engine treats it as opaque classification metadata and does not
/// infer it from `source_payload()`.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct EntityType(String);

impl EntityType {
    /// Creates a validated entity type.
    pub fn new(value: impl Into<String>) -> Result<Self, EntityTypeValidationError> {
        let value = value.into();

        if value.is_empty() {
            return Err(EntityTypeValidationError::Empty);
        }

        if let Some((index, _)) = value
            .char_indices()
            .find(|(_, character)| character.is_control())
        {
            return Err(EntityTypeValidationError::ControlCharacter { index });
        }

        Ok(Self(value))
    }

    /// Returns the source-supplied entity type.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Validation failures for [`EntityType`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EntityTypeValidationError {
    /// The source engine supplied an empty entity type.
    Empty,

    /// The entity type contains a Unicode control character.
    ControlCharacter { index: usize },
}

impl fmt::Display for EntityTypeValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(formatter, "entity type cannot be empty"),
            Self::ControlCharacter { index } => {
                write!(
                    formatter,
                    "entity type contains a control character at byte index {index}"
                )
            }
        }
    }
}

impl Error for EntityTypeValidationError {}

/// Result type used by the typed [`IndexEvent`] contract.
pub type IndexEventResult<T> = Result<T, IndexEventValidationError>;

/// Validation failures for an [`IndexEvent`].
#[derive(Debug, Eq, PartialEq)]
pub enum IndexEventValidationError {
    /// The source-supplied entity type is structurally invalid.
    InvalidEntityType(EntityTypeValidationError),

    /// The Indexing-owned requirement is not a valid logical contract.
    InvalidRequirement(IndexRequirementValidationError),

    /// The source-owned object reference is structurally invalid.
    InvalidObjectReference(ObjectReferenceValidationError),

    /// The generic logical key material is invalid.
    InvalidKeyMaterial(KeyMaterialValidationError),

    /// The wrapped Core request is not a request interaction.
    InvalidInteraction(Interaction),
}

impl fmt::Display for IndexEventValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidEntityType(error) => {
                write!(formatter, "invalid EntityType in IndexEvent: {error}")
            }
            Self::InvalidRequirement(error) => {
                write!(formatter, "invalid IndexRequirement in IndexEvent: {error}")
            }
            Self::InvalidObjectReference(error) => {
                write!(formatter, "invalid ObjectReference in IndexEvent: {error}")
            }
            Self::InvalidKeyMaterial(error) => {
                write!(formatter, "invalid KeyMaterial in IndexEvent: {error}")
            }
            Self::InvalidInteraction(interaction) => write!(
                formatter,
                "IndexEvent requires a Core request interaction, found {interaction:?}"
            ),
        }
    }
}

impl Error for IndexEventValidationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidEntityType(error) => Some(error),
            Self::InvalidRequirement(error) => Some(error),
            Self::InvalidObjectReference(error) => Some(error),
            Self::InvalidKeyMaterial(error) => Some(error),
            Self::InvalidInteraction(_) => None,
        }
    }
}

impl From<IndexRequirementValidationError> for IndexEventValidationError {
    fn from(error: IndexRequirementValidationError) -> Self {
        Self::InvalidRequirement(error)
    }
}

impl From<ObjectReferenceValidationError> for IndexEventValidationError {
    fn from(error: ObjectReferenceValidationError) -> Self {
        Self::InvalidObjectReference(error)
    }
}

impl From<KeyMaterialValidationError> for IndexEventValidationError {
    fn from(error: KeyMaterialValidationError) -> Self {
        Self::InvalidKeyMaterial(error)
    }
}

/// Typed logical Indexing event carried by Core's [`UniversalRequest`].
///
/// The Core request is stored directly instead of copying its event/message
/// identity, participants, operation context, or payload. This guarantees
/// that the Indexing event cannot accidentally become a second transport or
/// correlation protocol.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IndexEvent {
    request: UniversalRequest,
    entity_type: EntityType,
    requirement: IndexRequirement,
    object_reference: ObjectReference,
    key_material: KeyMaterial,
}

impl IndexEvent {
    /// Constructs a typed Indexing event around an existing Core request.
    ///
    /// The Core request must already be a request interaction. Indexing-owned
    /// logical values are validated at this boundary. Core occurrence and
    /// transport metadata are intentionally not reconstructed here.
    pub fn new(
        request: UniversalRequest,
        entity_type: EntityType,
        requirement: IndexRequirement,
        object_reference: ObjectReference,
        key_material: KeyMaterial,
    ) -> IndexEventResult<Self> {
        let event = Self {
            request,
            entity_type,
            requirement,
            object_reference,
            key_material,
        };

        event.validate()?;
        Ok(event)
    }

    /// Validates the complete typed event contract.
    pub fn validate(&self) -> IndexEventResult<()> {
        let event = self.request.universal_event();
        let interaction = event.envelope.metadata.descriptor.interaction;

        if interaction != Interaction::Request {
            return Err(IndexEventValidationError::InvalidInteraction(interaction));
        }

        EntityType::new(self.entity_type.as_str())
            .map(|_| ())
            .map_err(IndexEventValidationError::InvalidEntityType)?;

        self.requirement.validate()?;
        ObjectReference::new(
            self.object_reference.source(),
            self.object_reference.object_reference(),
        )
        .map(|_| ())
        .map_err(IndexEventValidationError::InvalidObjectReference)?;
        self.key_material
            .validate()
            .map_err(IndexEventValidationError::InvalidKeyMaterial)?;

        Ok(())
    }

    /// Returns the underlying Core [`UniversalRequest`].
    #[must_use]
    pub fn request(&self) -> &UniversalRequest {
        &self.request
    }

    /// Returns the underlying Core [`UniversalEvent`].
    #[must_use]
    pub fn universal_event(&self) -> &UniversalEvent {
        self.request.universal_event()
    }

    /// Consumes the typed event and returns its Core request unchanged.
    ///
    /// No new event, message, operation, correlation, or transport identity
    /// is created by this conversion.
    #[must_use]
    pub fn into_universal_request(self) -> UniversalRequest {
        self.request
    }

    /// Returns the source engine identity from Core participants.
    #[must_use]
    pub fn source_engine_id(&self) -> &EngineId {
        &self.universal_event().envelope.metadata.participants.sender
    }

    /// Returns the source engine-instance identity when Core supplied one.
    #[must_use]
    pub fn source_engine_instance_id(&self) -> Option<&EngineInstanceId> {
        self.universal_event()
            .envelope
            .metadata
            .participants
            .sender_instance
            .as_ref()
    }

    /// Returns Core's operation/correlation context unchanged.
    #[must_use]
    pub fn operation_context(&self) -> &OperationContext {
        &self.universal_event().envelope.operation_context
    }

    /// Returns Core's event occurrence identity.
    #[must_use]
    pub fn event_id(&self) -> &EventId {
        self.request.event_id()
    }

    /// Returns Core's message identity.
    #[must_use]
    pub fn message_id(&self) -> &MessageId {
        self.request.message_id()
    }

    /// Returns the source-supplied entity type.
    ///
    /// Indexing uses this value as classification metadata. It does not infer
    /// entity semantics from the opaque source payload.
    #[must_use]
    pub fn entity_type(&self) -> &EntityType {
        &self.entity_type
    }

    /// Returns the Indexing-owned logical requirement.
    #[must_use]
    pub fn requirement(&self) -> &IndexRequirement {
        &self.requirement
    }

    /// Returns the source-owned logical object reference.
    #[must_use]
    pub fn object_reference(&self) -> &ObjectReference {
        &self.object_reference
    }

    /// Returns the generic logical key material.
    #[must_use]
    pub fn key_material(&self) -> &KeyMaterial {
        &self.key_material
    }

    /// Returns the source-owned payload carried by Core's universal event.
    ///
    /// Indexing does not interpret this payload as a domain object or physical
    /// provider instruction. The source remains the owner of its semantics.
    #[must_use]
    pub fn source_payload(&self) -> &[u8] {
        self.universal_event().envelope.payload.bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nizaam_core::contracts::{
        ContractDescriptor, ContractMetadata, EncodedPayload, MessageEnvelope, Participants,
        PayloadDescriptor, Version,
    };
    use nizaam_core::identity::{CapabilityId, ContractId, CorrelationId, MessageId, OperationId};
    use nizaam_core::operation::Operation;

    fn request(sender: EngineId, sender_instance: Option<EngineInstanceId>) -> UniversalRequest {
        let capability = CapabilityId::new("nizaam.indexing.event.test")
            .expect("test capability id must be valid");
        let contract =
            ContractId::new("nizaam.indexing.event.index").expect("test contract id must be valid");
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

        let mut participants = Participants::new(
            sender,
            EngineId::new("nizaam.indexing.test").expect("target id must be valid"),
        );
        if let Some(instance) = sender_instance {
            participants = participants.with_sender_instance(instance);
        }

        let metadata = ContractMetadata::new(descriptor.clone(), participants);
        let operation = Operation::new(
            OperationId::new("index-event.operation").expect("operation id must be valid"),
            CorrelationId::new("index-event.correlation").expect("correlation id must be valid"),
        );
        let envelope = MessageEnvelope::new(
            MessageId::new("index-event.message").expect("message id must be valid"),
            OperationContext::new(operation),
            metadata,
            EncodedPayload::new(payload_descriptor, b"source-owned-payload".to_vec()),
        );

        UniversalRequest::new(envelope)
    }

    fn requirement() -> IndexRequirement {
        use crate::identity::IndexNamespace;
        use crate::index::{
            ConsistencyRequirement, IndexFamily, KeyDefinition, TargetReferenceType, Uniqueness,
        };

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

    fn object_reference() -> ObjectReference {
        ObjectReference::new("nizaam.source.test", "object:1")
            .expect("object reference must be valid")
    }

    #[test]
    fn constructs_typed_event_without_duplicating_core_identity() {
        let source = EngineId::new("nizaam.source.test").expect("source id must be valid");
        let instance = EngineInstanceId::new("nizaam.source.test.instance")
            .expect("source instance id must be valid");
        let request = request(source.clone(), Some(instance.clone()));
        let event_id = request.event_id().clone();
        let message_id = request.message_id().clone();
        let operation = request.universal_event().envelope.operation_context.clone();

        let event = IndexEvent::new(
            request,
            EntityType::new("word").expect("entity type must be valid"),
            requirement(),
            object_reference(),
            KeyMaterial::text("term"),
        )
        .expect("typed event must be valid");

        assert_eq!(event.source_engine_id(), &source);
        assert_eq!(event.source_engine_instance_id(), Some(&instance));
        assert_eq!(event.event_id(), &event_id);
        assert_eq!(event.message_id(), &message_id);
        assert_eq!(event.operation_context(), &operation);
        assert_eq!(event.source_payload(), b"source-owned-payload");
    }

    #[test]
    fn preserves_logical_indexing_contract_fields() {
        let event = IndexEvent::new(
            request(
                EngineId::new("nizaam.source.test").expect("source id must be valid"),
                None,
            ),
            EntityType::new("semantic").expect("entity type must be valid"),
            requirement(),
            object_reference(),
            KeyMaterial::text("term"),
        )
        .expect("typed event must be valid");

        assert_eq!(
            event.requirement().family(),
            crate::index::IndexFamily::Inverted
        );
        assert_eq!(event.object_reference().object_reference(), "object:1");
        assert_eq!(event.key_material(), &KeyMaterial::text("term"));
    }

    #[test]
    fn source_instance_identity_is_optional() {
        let event = IndexEvent::new(
            request(
                EngineId::new("nizaam.source.test").expect("source id must be valid"),
                None,
            ),
            EntityType::new("relationship").expect("entity type must be valid"),
            requirement(),
            object_reference(),
            KeyMaterial::Null,
        )
        .expect("typed event must be valid");

        assert!(event.source_engine_instance_id().is_none());
    }

    #[test]
    fn rejects_non_request_core_interaction() {
        let capability = CapabilityId::new("nizaam.indexing.event.test")
            .expect("test capability id must be valid");
        let contract =
            ContractId::new("nizaam.indexing.event.index").expect("test contract id must be valid");
        let version = Version::new(1, 0, 0);
        let payload_descriptor =
            PayloadDescriptor::new("application/octet-stream", version.clone())
                .expect("payload descriptor must be valid");
        let descriptor = ContractDescriptor::new(
            contract,
            capability,
            version,
            Interaction::Response,
            payload_descriptor.clone(),
        );
        let metadata = ContractMetadata::new(
            descriptor.clone(),
            Participants::new(
                EngineId::new("nizaam.source.test").expect("source id must be valid"),
                EngineId::new("nizaam.indexing.test").expect("target id must be valid"),
            ),
        );
        let envelope = MessageEnvelope::new(
            MessageId::new("index-event.response-message").expect("message id must be valid"),
            OperationContext::new(Operation::new(
                OperationId::new("index-event.response-operation")
                    .expect("operation id must be valid"),
                CorrelationId::new("index-event.response-correlation")
                    .expect("correlation id must be valid"),
            )),
            metadata,
            EncodedPayload::new(payload_descriptor, Vec::new()),
        );
        let request = UniversalRequest::new(envelope);

        let error = IndexEvent::new(
            request,
            EntityType::new("word").expect("entity type must be valid"),
            requirement(),
            object_reference(),
            KeyMaterial::Null,
        )
        .expect_err("response interaction must not be accepted as an IndexEvent");

        assert_eq!(
            error,
            IndexEventValidationError::InvalidInteraction(Interaction::Response)
        );
    }

    #[test]
    fn rejects_invalid_object_reference_then_accepts_valid_index_event() {
        let request = request(
            EngineId::new("nizaam.source.test").expect("source id must be valid"),
            None,
        );
        let invalid_reference = ObjectReference::new("nizaam.source.test", "\u{0000}")
            .expect_err("control character should be rejected");
        assert!(matches!(
            invalid_reference,
            ObjectReferenceValidationError::ObjectReferenceControlCharacter { .. }
        ));

        let event = IndexEvent::new(
            request,
            EntityType::new("word").expect("entity type must be valid"),
            requirement(),
            ObjectReference::new("nizaam.source.test", "object:1")
                .expect("object reference must be valid"),
            KeyMaterial::Null,
        )
        .expect("valid logical event should be accepted");

        assert!(event.validate().is_ok());
    }

    #[test]
    fn source_supplied_entity_type_is_preserved_without_payload_inference() {
        let event = IndexEvent::new(
            request(
                EngineId::new("nizaam.source.test").expect("source id must be valid"),
                None,
            ),
            EntityType::new("semantic").expect("entity type must be valid"),
            requirement(),
            object_reference(),
            KeyMaterial::Null,
        )
        .expect("logical event must be valid");

        assert_eq!(event.entity_type().as_str(), "semantic");
        assert_eq!(event.source_payload(), b"source-owned-payload");
    }

    #[test]
    fn rejects_empty_source_supplied_entity_type() {
        let error = EntityType::new("").expect_err("empty entity type must be rejected");

        assert_eq!(error, EntityTypeValidationError::Empty);
    }

    #[test]
    fn physical_provider_instructions_are_not_part_of_the_typed_contract() {
        // There is intentionally no provider/storage instruction field on
        // IndexEvent. Physical choices such as B-tree, HNSW, FAISS, Lucene,
        // GIN, database, table, or partition therefore cannot become part of
        // the Indexing-owned logical event contract. The source payload stays
        // opaque and source-owned rather than being reinterpreted as provider
        // configuration by this type.
        let event = IndexEvent::new(
            request(
                EngineId::new("nizaam.source.test").expect("source id must be valid"),
                None,
            ),
            EntityType::new("word").expect("entity type must be valid"),
            requirement(),
            object_reference(),
            KeyMaterial::Null,
        )
        .expect("logical event must remain valid");

        assert_eq!(event.source_payload(), b"source-owned-payload");
    }
}
