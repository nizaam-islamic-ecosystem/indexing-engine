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
//! UniversalRequest boundary. When an IndexEvent crosses the Control Plane,
//! the Indexing-owned fields are carried in an explicit versioned payload
//! boundary and reconstructed without creating replacement Core identities.
//!
//! The event contains no physical provider instruction. There is deliberately
//! no field for a B-tree, HNSW, FAISS, Lucene, GIN, database, table,
//! partition, shard, or other provider/storage choice. The source-owned
//! payload remains opaque to Indexing and is carried by Core unchanged.

use core::fmt;
use std::collections::BTreeMap;
use std::error::Error;

use nizaam_core::contracts::{Interaction, UniversalEvent, UniversalRequest};
use nizaam_core::identity::{EngineId, EngineInstanceId, EventId, MessageId};
use nizaam_core::operation::OperationContext;

use crate::identity::IndexNamespace;
use crate::index::{
    ConsistencyRequirement, IndexFamily, KeyDefinition, KeyMaterial, KeyMaterialValidationError,
    ObjectReference, ObjectReferenceValidationError, SchemaVersion, SourceVersion,
    TargetReferenceType, Uniqueness,
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

    /// A component of the transported Indexing requirement is structurally invalid.
    InvalidRequirementValue(IndexEventRequirementValueValidationError),

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
            Self::InvalidRequirementValue(error) => {
                write!(
                    formatter,
                    "invalid IndexRequirement value in IndexEvent: {error}"
                )
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
            Self::InvalidRequirementValue(error) => Some(error),
            Self::InvalidObjectReference(error) => Some(error),
            Self::InvalidKeyMaterial(error) => Some(error),
            Self::InvalidInteraction(_) => None,
        }
    }
}

/// Validation failures for individual IndexRequirement components decoded from
/// the Control Plane application payload. The original validation error is
/// preserved instead of being collapsed into a transport length error.
#[derive(Debug, Eq, PartialEq)]
pub enum IndexEventRequirementValueValidationError {
    Namespace(crate::identity::NamespaceValidationError),
    TargetReferenceType(crate::index::TargetReferenceTypeValidationError),
    ConsistencyRequirement(crate::index::ConsistencyRequirementValidationError),
    SourceVersion(crate::index::SourceVersionValidationError),
    SchemaVersion(crate::index::SchemaVersionValidationError),
}

impl fmt::Display for IndexEventRequirementValueValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Namespace(error) => write!(formatter, "invalid namespace: {error}"),
            Self::TargetReferenceType(error) => {
                write!(formatter, "invalid target reference type: {error}")
            }
            Self::ConsistencyRequirement(error) => {
                write!(formatter, "invalid consistency requirement: {error}")
            }
            Self::SourceVersion(error) => write!(formatter, "invalid source version: {error}"),
            Self::SchemaVersion(error) => write!(formatter, "invalid schema version: {error}"),
        }
    }
}

impl Error for IndexEventRequirementValueValidationError {}

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

    /// Encodes the Indexing-owned fields together with the source payload as
    /// the application payload of a Core [`UniversalRequest`].
    ///
    /// Core remains the owner of the surrounding envelope and its identities.
    /// This method only defines the Indexing-owned payload boundary required
    /// because `IndexEvent` contains fields that are intentionally not copied
    /// into Core's universal envelope. The returned bytes are not a second
    /// event envelope and do not contain a second EventId, MessageId,
    /// OperationId, or CorrelationId.
    pub fn control_plane_payload(&self) -> Result<Vec<u8>, IndexEventTransportError> {
        encode_transport_payload(self)
    }

    /// Creates the Core [`UniversalRequest`] whose application payload contains
    /// the Indexing-owned Control Plane payload representation.
    ///
    /// This method does not create a new Core identity and does not perform
    /// wire-level transport envelope. The returned request retains the existing
    /// envelope, `MessageId`, `EventId`, operation context, participants, and
    /// contract metadata. Core's Control Plane/transport layer is responsible
    /// for serializing that request on the wire.
    pub fn into_control_plane_request(self) -> Result<UniversalRequest, IndexEventTransportError> {
        let payload = encode_transport_payload(&self)?;
        let mut request = self.into_universal_request();
        let descriptor = request
            .universal_event()
            .envelope
            .payload
            .descriptor()
            .clone();
        request.event.envelope.payload =
            nizaam_core::contracts::EncodedPayload::new(descriptor, payload);
        Ok(request)
    }

    /// Reconstructs an [`IndexEvent`] from a Core request carrying the explicit
    /// Indexing transport payload produced by [`Self::control_plane_payload`].
    ///
    /// The supplied Core request is retained unchanged. In particular, this
    /// method never creates a replacement Core identity or operation context.
    pub fn from_control_plane_request(
        request: UniversalRequest,
    ) -> Result<Self, IndexEventTransportError> {
        let payload = request.universal_event().envelope.payload.bytes();
        let (entity_type, requirement, object_reference, key_material, source_payload) =
            decode_transport_payload(payload)?;

        let mut request = request;
        let descriptor = request
            .universal_event()
            .envelope
            .payload
            .descriptor()
            .clone();
        request.event.envelope.payload =
            nizaam_core::contracts::EncodedPayload::new(descriptor, source_payload);

        let event = Self {
            request,
            entity_type,
            requirement,
            object_reference,
            key_material,
        };

        event
            .validate()
            .map_err(IndexEventTransportError::InvalidEvent)?;
        Ok(event)
    }
}

/// Errors produced while encoding or decoding the explicit IndexEvent
/// Control Plane application-payload boundary. Core remains responsible for
/// serializing and transporting the enclosing `UniversalRequest`.
#[derive(Debug, Eq, PartialEq)]
pub enum IndexEventTransportError {
    /// The payload does not contain the Indexing transport magic/version.
    InvalidHeader,
    /// The payload ended before a complete value could be read.
    Truncated,
    /// A length/count would exceed the transport decoder's safety bounds.
    InvalidLength,
    /// A tagged value contains an unknown variant.
    InvalidTag(u8),
    /// A length-delimited value was not valid UTF-8.
    InvalidUtf8,
    /// The reconstructed IndexEvent failed its normal logical validation.
    InvalidEvent(IndexEventValidationError),
    /// A nested key-material value exceeded the supported nesting depth.
    KeyMaterialDepthExceeded,
}

impl fmt::Display for IndexEventTransportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidHeader => formatter.write_str("invalid IndexEvent transport header"),
            Self::Truncated => formatter.write_str("truncated IndexEvent transport payload"),
            Self::InvalidLength => formatter.write_str("invalid IndexEvent transport length"),
            Self::InvalidTag(tag) => write!(formatter, "invalid IndexEvent transport tag {tag}"),
            Self::InvalidUtf8 => formatter.write_str("IndexEvent transport value is not UTF-8"),
            Self::InvalidEvent(error) => {
                write!(formatter, "invalid transported IndexEvent: {error}")
            }
            Self::KeyMaterialDepthExceeded => {
                formatter.write_str("transported key material exceeds the maximum nesting depth")
            }
        }
    }
}

impl Error for IndexEventTransportError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidEvent(error) => Some(error),
            _ => None,
        }
    }
}

const TRANSPORT_MAGIC: &[u8; 4] = b"NIZE";
const TRANSPORT_VERSION: u8 = 1;

fn encode_transport_payload(event: &IndexEvent) -> Result<Vec<u8>, IndexEventTransportError> {
    event
        .validate()
        .map_err(IndexEventTransportError::InvalidEvent)?;

    let mut writer = TransportWriter::new();
    writer.bytes(TRANSPORT_MAGIC);
    writer.u8(TRANSPORT_VERSION);
    writer.string(event.entity_type().as_str())?;

    let requirement = event.requirement();
    writer.string(requirement.namespace().as_str())?;
    writer.u8(match requirement.family() {
        IndexFamily::Identity => 0,
        IndexFamily::Inverted => 1,
        IndexFamily::Relationship => 2,
        IndexFamily::Similarity => 3,
    });
    writer.u32(requirement.key_definition().fields().len())?;
    for field in requirement.key_definition().fields() {
        writer.string(field.name())?;
    }
    writer.string(requirement.target_reference_type().as_str())?;
    writer.u8(match requirement.uniqueness() {
        Uniqueness::Unique => 0,
        Uniqueness::NonUnique => 1,
    });
    writer.string(requirement.consistency_requirement().as_str())?;
    writer.optional_string(requirement.source_version().map(SourceVersion::as_str))?;
    writer.optional_string(requirement.schema_version().map(SchemaVersion::as_str))?;

    writer.string(event.object_reference().source())?;
    writer.string(event.object_reference().object_reference())?;
    writer.key_material(event.key_material(), 0)?;
    writer.bytes_value(event.source_payload())?;

    Ok(writer.finish())
}

fn decode_transport_payload(
    payload: &[u8],
) -> Result<
    (
        EntityType,
        IndexRequirement,
        ObjectReference,
        KeyMaterial,
        Vec<u8>,
    ),
    IndexEventTransportError,
> {
    let mut reader = TransportReader::new(payload);
    if reader.take(4)? != TRANSPORT_MAGIC {
        return Err(IndexEventTransportError::InvalidHeader);
    }
    if reader.u8()? != TRANSPORT_VERSION {
        return Err(IndexEventTransportError::InvalidHeader);
    }

    let entity_type = EntityType::new(reader.string()?).map_err(|error| {
        IndexEventTransportError::InvalidEvent(IndexEventValidationError::InvalidEntityType(error))
    })?;

    let namespace = IndexNamespace::new(reader.string()?).map_err(|error| {
        IndexEventTransportError::InvalidEvent(IndexEventValidationError::InvalidRequirementValue(
            IndexEventRequirementValueValidationError::Namespace(error),
        ))
    })?;

    let family = match reader.u8()? {
        0 => IndexFamily::Identity,
        1 => IndexFamily::Inverted,
        2 => IndexFamily::Relationship,
        3 => IndexFamily::Similarity,
        tag => return Err(IndexEventTransportError::InvalidTag(tag)),
    };

    let field_count = reader.count(4)?;
    let mut fields = Vec::with_capacity(field_count);
    for _ in 0..field_count {
        fields.push(reader.string()?);
    }
    let key_definition = KeyDefinition::new(fields).map_err(|error| {
        IndexEventTransportError::InvalidEvent(IndexEventValidationError::InvalidRequirement(
            IndexRequirementValidationError::InvalidKeyDefinition(error),
        ))
    })?;

    let target_reference_type = TargetReferenceType::new(reader.string()?).map_err(|error| {
        IndexEventTransportError::InvalidEvent(IndexEventValidationError::InvalidRequirementValue(
            IndexEventRequirementValueValidationError::TargetReferenceType(error),
        ))
    })?;

    let uniqueness = match reader.u8()? {
        0 => Uniqueness::Unique,
        1 => Uniqueness::NonUnique,
        tag => return Err(IndexEventTransportError::InvalidTag(tag)),
    };
    let consistency_requirement =
        ConsistencyRequirement::new(reader.string()?).map_err(|error| {
            IndexEventTransportError::InvalidEvent(
                IndexEventValidationError::InvalidRequirementValue(
                    IndexEventRequirementValueValidationError::ConsistencyRequirement(error),
                ),
            )
        })?;
    let source_version = reader
        .optional_string()?
        .map(SourceVersion::new)
        .transpose()
        .map_err(|error| {
            IndexEventTransportError::InvalidEvent(
                IndexEventValidationError::InvalidRequirementValue(
                    IndexEventRequirementValueValidationError::SourceVersion(error),
                ),
            )
        })?;
    let schema_version = reader
        .optional_string()?
        .map(SchemaVersion::new)
        .transpose()
        .map_err(|error| {
            IndexEventTransportError::InvalidEvent(
                IndexEventValidationError::InvalidRequirementValue(
                    IndexEventRequirementValueValidationError::SchemaVersion(error),
                ),
            )
        })?;

    let requirement = IndexRequirement::new(
        namespace,
        family,
        key_definition,
        target_reference_type,
        uniqueness,
        consistency_requirement,
        source_version,
        schema_version,
    )
    .map_err(IndexEventValidationError::InvalidRequirement)
    .map_err(IndexEventTransportError::InvalidEvent)?;

    let object_reference =
        ObjectReference::new(reader.string()?, reader.string()?).map_err(|error| {
            IndexEventTransportError::InvalidEvent(
                IndexEventValidationError::InvalidObjectReference(error),
            )
        })?;
    let key_material = reader.key_material(0)?;
    let source_payload = reader.bytes_value()?;

    if !reader.is_finished() {
        return Err(IndexEventTransportError::InvalidLength);
    }

    Ok((
        entity_type,
        requirement,
        object_reference,
        key_material,
        source_payload,
    ))
}

struct TransportWriter {
    bytes: Vec<u8>,
}

impl TransportWriter {
    fn new() -> Self {
        Self { bytes: Vec::new() }
    }

    fn finish(self) -> Vec<u8> {
        self.bytes
    }

    fn bytes(&mut self, value: &[u8]) {
        self.bytes.extend_from_slice(value);
    }

    fn u8(&mut self, value: u8) {
        self.bytes.push(value);
    }

    fn u32(&mut self, value: usize) -> Result<(), IndexEventTransportError> {
        let value = u32::try_from(value).map_err(|_| IndexEventTransportError::InvalidLength)?;
        self.bytes.extend_from_slice(&value.to_le_bytes());
        Ok(())
    }

    fn string(&mut self, value: &str) -> Result<(), IndexEventTransportError> {
        self.bytes_value(value.as_bytes())
    }

    fn optional_string(&mut self, value: Option<&str>) -> Result<(), IndexEventTransportError> {
        match value {
            Some(value) => {
                self.u8(1);
                self.string(value)?;
            }
            None => self.u8(0),
        }
        Ok(())
    }

    fn bytes_value(&mut self, value: &[u8]) -> Result<(), IndexEventTransportError> {
        self.u32(value.len())?;
        self.bytes.extend_from_slice(value);
        Ok(())
    }

    fn key_material(
        &mut self,
        value: &KeyMaterial,
        depth: usize,
    ) -> Result<(), IndexEventTransportError> {
        if depth > crate::index::key::KEY_MATERIAL_MAX_DEPTH {
            return Err(IndexEventTransportError::KeyMaterialDepthExceeded);
        }

        match value {
            KeyMaterial::Null => self.u8(0),
            KeyMaterial::Bool(value) => {
                self.u8(1);
                self.u8(u8::from(*value));
            }
            KeyMaterial::Integer(value) => {
                self.u8(2);
                self.bytes.extend_from_slice(&value.to_le_bytes());
            }
            KeyMaterial::Unsigned(value) => {
                self.u8(3);
                self.bytes.extend_from_slice(&value.to_le_bytes());
            }
            KeyMaterial::Text(value) => {
                self.u8(4);
                self.string(value)?;
            }
            KeyMaterial::Bytes(value) => {
                self.u8(5);
                self.bytes_value(value)?;
            }
            KeyMaterial::Sequence(values) => {
                self.u8(6);
                self.u32(values.len())?;
                for value in values {
                    self.key_material(value, depth + 1)?;
                }
            }
            KeyMaterial::Map(values) => {
                self.u8(7);
                self.u32(values.len())?;
                for (name, value) in values {
                    self.string(name)?;
                    self.key_material(value, depth + 1)?;
                }
            }
        }

        Ok(())
    }
}

struct TransportReader<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> TransportReader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8], IndexEventTransportError> {
        let end = self
            .offset
            .checked_add(length)
            .ok_or(IndexEventTransportError::InvalidLength)?;
        if end > self.bytes.len() {
            return Err(IndexEventTransportError::Truncated);
        }
        let value = &self.bytes[self.offset..end];
        self.offset = end;
        Ok(value)
    }

    fn u8(&mut self) -> Result<u8, IndexEventTransportError> {
        Ok(*self
            .take(1)?
            .first()
            .ok_or(IndexEventTransportError::Truncated)?)
    }

    fn u32(&mut self) -> Result<u32, IndexEventTransportError> {
        let bytes = self.take(4)?;
        Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    fn count(&mut self, minimum_bytes_per_item: usize) -> Result<usize, IndexEventTransportError> {
        let value = self.u32()? as usize;
        let remaining = self.bytes.len().saturating_sub(self.offset);
        if minimum_bytes_per_item == 0 || value > remaining / minimum_bytes_per_item {
            return Err(IndexEventTransportError::InvalidLength);
        }
        Ok(value)
    }

    fn string(&mut self) -> Result<String, IndexEventTransportError> {
        let bytes = self.bytes_value()?;
        String::from_utf8(bytes).map_err(|_| IndexEventTransportError::InvalidUtf8)
    }

    fn optional_string(&mut self) -> Result<Option<String>, IndexEventTransportError> {
        match self.u8()? {
            0 => Ok(None),
            1 => Ok(Some(self.string()?)),
            tag => Err(IndexEventTransportError::InvalidTag(tag)),
        }
    }

    fn bytes_value(&mut self) -> Result<Vec<u8>, IndexEventTransportError> {
        let length = self.u32()? as usize;
        if length > self.bytes.len().saturating_sub(self.offset) {
            return Err(IndexEventTransportError::InvalidLength);
        }
        Ok(self.take(length)?.to_vec())
    }

    fn key_material(&mut self, depth: usize) -> Result<KeyMaterial, IndexEventTransportError> {
        if depth > crate::index::key::KEY_MATERIAL_MAX_DEPTH {
            return Err(IndexEventTransportError::KeyMaterialDepthExceeded);
        }

        match self.u8()? {
            0 => Ok(KeyMaterial::Null),
            1 => Ok(KeyMaterial::Bool(match self.u8()? {
                0 => false,
                1 => true,
                tag => return Err(IndexEventTransportError::InvalidTag(tag)),
            })),
            2 => {
                let bytes = self.take(16)?;
                Ok(KeyMaterial::Integer(i128::from_le_bytes(
                    bytes.try_into().unwrap(),
                )))
            }
            3 => {
                let bytes = self.take(16)?;
                Ok(KeyMaterial::Unsigned(u128::from_le_bytes(
                    bytes.try_into().unwrap(),
                )))
            }
            4 => Ok(KeyMaterial::Text(self.string()?)),
            5 => Ok(KeyMaterial::Bytes(self.bytes_value()?)),
            6 => {
                let count = self.count(1)?;
                let mut values = Vec::with_capacity(count);
                for _ in 0..count {
                    values.push(self.key_material(depth + 1)?);
                }
                Ok(KeyMaterial::Sequence(values))
            }
            7 => {
                let count = self.count(5)?;
                let mut values = BTreeMap::new();
                for _ in 0..count {
                    let name = self.string()?;
                    let value = self.key_material(depth + 1)?;
                    if values.insert(name.clone(), value).is_some() {
                        return Err(IndexEventTransportError::InvalidEvent(
                            IndexEventValidationError::InvalidKeyMaterial(
                                KeyMaterialValidationError::DuplicateMapField(name),
                            ),
                        ));
                    }
                }
                KeyMaterial::map(values).map_err(|error| {
                    IndexEventTransportError::InvalidEvent(
                        IndexEventValidationError::InvalidKeyMaterial(error),
                    )
                })
            }
            tag => Err(IndexEventTransportError::InvalidTag(tag)),
        }
    }

    fn is_finished(&self) -> bool {
        self.offset == self.bytes.len()
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
    fn control_plane_transport_round_trip_preserves_core_identity_and_indexing_fields() {
        let source = EngineId::new("nizaam.source.test").expect("source id must be valid");
        let original = IndexEvent::new(
            request(source, None),
            EntityType::new("semantic").expect("entity type must be valid"),
            requirement(),
            object_reference(),
            KeyMaterial::Sequence(vec![KeyMaterial::text("term"), KeyMaterial::Unsigned(7)]),
        )
        .expect("event must be valid");

        let original_event_id = original.event_id().clone();
        let original_message_id = original.message_id().clone();
        let original_operation = original.operation_context().clone();
        let original_source_payload = original.source_payload().to_vec();

        let transport_request = original
            .clone()
            .into_control_plane_request()
            .expect("control plane request construction must succeed");
        let reconstructed = IndexEvent::from_control_plane_request(transport_request)
            .expect("transport reconstruction must succeed");

        assert_eq!(reconstructed.event_id(), &original_event_id);
        assert_eq!(reconstructed.message_id(), &original_message_id);
        assert_eq!(reconstructed.operation_context(), &original_operation);
        assert_eq!(reconstructed.entity_type(), original.entity_type());
        assert_eq!(reconstructed.requirement(), original.requirement());
        assert_eq!(
            reconstructed.object_reference(),
            original.object_reference()
        );
        assert_eq!(reconstructed.key_material(), original.key_material());
        assert_eq!(reconstructed.source_payload(), original_source_payload);
    }

    #[test]
    fn control_plane_transport_rejects_truncated_payload() {
        let event = IndexEvent::new(
            request(
                EngineId::new("nizaam.source.test").expect("source id must be valid"),
                None,
            ),
            EntityType::new("word").expect("entity type must be valid"),
            requirement(),
            object_reference(),
            KeyMaterial::text("term"),
        )
        .expect("event must be valid");

        let mut payload = event
            .control_plane_payload()
            .expect("transport encoding must succeed");
        payload.pop();

        let envelope = &event.universal_event().envelope;
        let transport_request = UniversalRequest::new(MessageEnvelope::new(
            envelope.message_id.clone(),
            envelope.operation_context.clone(),
            envelope.metadata.clone(),
            EncodedPayload::new(envelope.metadata.descriptor.payload.clone(), payload),
        ));

        assert!(matches!(
            IndexEvent::from_control_plane_request(transport_request),
            Err(IndexEventTransportError::Truncated | IndexEventTransportError::InvalidLength)
        ));
    }

    #[test]
    fn control_plane_transport_keeps_source_payload_opaque_to_indexing_metadata() {
        let event = IndexEvent::new(
            request(
                EngineId::new("nizaam.source.test").expect("source id must be valid"),
                None,
            ),
            EntityType::new("word").expect("entity type must be valid"),
            requirement(),
            object_reference(),
            KeyMaterial::text("term"),
        )
        .expect("event must be valid");

        let request = event
            .clone()
            .into_control_plane_request()
            .expect("control plane request construction must succeed");
        let reconstructed = IndexEvent::from_control_plane_request(request)
            .expect("transport reconstruction must succeed");

        assert_eq!(reconstructed.source_payload(), b"source-owned-payload");
        assert_eq!(reconstructed.entity_type().as_str(), "word");
        assert_eq!(reconstructed.message_id(), event.message_id());
        assert_eq!(reconstructed.event_id(), event.event_id());
    }

    #[test]
    fn control_plane_payload_is_application_data_only() {
        let event = IndexEvent::new(
            request(
                EngineId::new("nizaam.source.test").expect("source id must be valid"),
                None,
            ),
            EntityType::new("word").expect("entity type must be valid"),
            requirement(),
            object_reference(),
            KeyMaterial::text("term"),
        )
        .expect("event must be valid");

        let payload = event
            .control_plane_payload()
            .expect("application payload encoding must succeed");

        assert_eq!(&payload[..4], b"NIZE");
    }

    #[test]
    fn control_plane_request_round_trip_restores_the_original_core_payload() {
        let event = IndexEvent::new(
            request(
                EngineId::new("nizaam.source.test").expect("source id must be valid"),
                None,
            ),
            EntityType::new("word").expect("entity type must be valid"),
            requirement(),
            object_reference(),
            KeyMaterial::text("term"),
        )
        .expect("event must be valid");

        let message_id = event.message_id().clone();
        let event_id = event.event_id().clone();
        let request = event
            .clone()
            .into_control_plane_request()
            .expect("control plane request construction must succeed");

        assert_eq!(request.message_id(), &message_id);
        assert_eq!(request.event_id(), &event_id);
        assert_ne!(
            request.universal_event().envelope.payload.bytes(),
            event.source_payload()
        );

        let reconstructed = IndexEvent::from_control_plane_request(request)
            .expect("control plane request reconstruction must succeed");

        assert_eq!(reconstructed.event_id(), &event_id);
        assert_eq!(reconstructed.message_id(), &message_id);
        assert_eq!(reconstructed.source_payload(), event.source_payload());
        assert_eq!(reconstructed.entity_type(), event.entity_type());
        assert_eq!(reconstructed.requirement(), event.requirement());
        assert_eq!(reconstructed.object_reference(), event.object_reference());
        assert_eq!(reconstructed.key_material(), event.key_material());
    }

    #[test]
    fn transported_requirement_validation_errors_are_not_reported_as_lengths() {
        let event = IndexEvent::new(
            request(
                EngineId::new("nizaam.source.test").expect("source id must be valid"),
                None,
            ),
            EntityType::new("word").expect("entity type must be valid"),
            requirement(),
            object_reference(),
            KeyMaterial::text("term"),
        )
        .expect("event must be valid");

        let mut payload = event
            .control_plane_payload()
            .expect("application payload encoding must succeed");
        // Locate the namespace bytes and corrupt the first namespace character
        // without changing any application-payload structure. The decoder
        // must preserve the namespace validation error as an invalid event.
        let namespace_marker = b"logical";
        let position = payload
            .windows(namespace_marker.len())
            .position(|window| window == namespace_marker)
            .expect("encoded namespace must be present");
        payload[position] = b' ';

        let mut request = event.into_control_plane_request().expect("request");
        let descriptor = request
            .universal_event()
            .envelope
            .payload
            .descriptor()
            .clone();
        request.event.envelope.payload =
            nizaam_core::contracts::EncodedPayload::new(descriptor, payload);

        assert!(matches!(
            IndexEvent::from_control_plane_request(request),
            Err(IndexEventTransportError::InvalidEvent(
                IndexEventValidationError::InvalidRequirementValue(
                    IndexEventRequirementValueValidationError::Namespace(_)
                )
            ))
        ));
    }

    #[test]
    fn control_plane_payload_does_not_impose_an_indexing_item_count_limit() {
        let values = (0..4097).map(KeyMaterial::Unsigned).collect::<Vec<_>>();
        let event = IndexEvent::new(
            request(
                EngineId::new("nizaam.source.test").expect("source id must be valid"),
                None,
            ),
            EntityType::new("word").expect("entity type must be valid"),
            requirement(),
            object_reference(),
            KeyMaterial::Sequence(values),
        )
        .expect("event must be valid");

        let expected_key_material = event.key_material().canonical_bytes();
        let payload = event
            .control_plane_payload()
            .expect("application payload encoding must not impose an item-count limit");

        let request = event
            .into_control_plane_request()
            .expect("control plane request construction must succeed");
        let reconstructed = IndexEvent::from_control_plane_request(request)
            .expect("4097 logical key items must remain decodable");

        assert!(!payload.is_empty());
        assert_eq!(
            reconstructed.key_material().canonical_bytes(),
            expected_key_material
        );
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
