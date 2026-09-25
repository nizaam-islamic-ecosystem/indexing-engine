//! Canonical logical index definitions.
//!
//! `IndexDefinition` is the canonical logical contract produced by Indexing
//! after a source-owned requirement has been validated and normalized.
//!
//! This module deliberately contains no physical provider configuration,
//! storage configuration, query execution, build/rebuild workflow, publication
//! workflow, or domain semantics.

use super::IndexFamily;
use super::key::{KeyDefinition, KeyMaterial};
use crate::identity::{IndexDefinitionId, IndexDefinitionIdentity, IndexNamespace};
use crate::index::version::{SchemaVersion, SourceVersion};
use core::fmt;

/// Identifies the logical type of object referenced by an index entry.
///
/// The value is intentionally opaque to Indexing. The source/domain owner
/// defines the meaning of the reference type.
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct TargetReferenceType(String);

impl TargetReferenceType {
    /// Constructs a target-reference type.
    pub fn new(value: impl Into<String>) -> Result<Self, TargetReferenceTypeValidationError> {
        let value = value.into();
        validate_text_value(&value).map_err(TargetReferenceTypeValidationError::InvalidValue)?;
        Ok(Self(value))
    }

    /// Returns the target-reference type as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Returns the number of bytes in the target-reference type.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns whether the value is empty.
    ///
    /// This is unreachable for successfully constructed values.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl AsRef<str> for TargetReferenceType {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl TryFrom<&str> for TargetReferenceType {
    type Error = TargetReferenceTypeValidationError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl TryFrom<String> for TargetReferenceType {
    type Error = TargetReferenceTypeValidationError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl fmt::Debug for TargetReferenceType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("TargetReferenceType")
            .field(&self.0)
            .finish()
    }
}

impl fmt::Display for TargetReferenceType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// Logical uniqueness requirement for an index.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum Uniqueness {
    /// Each logical key may identify at most one target within the index.
    Unique,

    /// A logical key may identify multiple targets within the index.
    NonUnique,
}

impl fmt::Display for Uniqueness {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unique => formatter.write_str("unique"),
            Self::NonUnique => formatter.write_str("non-unique"),
        }
    }
}

/// Logical consistency requirement associated with an index definition.
///
/// The value remains opaque because Phase 2 does not define a consistency
/// protocol or physical update mechanism. Source/consumer contracts may
/// define the value while Indexing preserves it as logical metadata.
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct ConsistencyRequirement(String);

impl ConsistencyRequirement {
    /// Constructs a consistency requirement.
    pub fn new(value: impl Into<String>) -> Result<Self, ConsistencyRequirementValidationError> {
        let value = value.into();
        validate_text_value(&value).map_err(ConsistencyRequirementValidationError::InvalidValue)?;
        Ok(Self(value))
    }

    /// Returns the consistency requirement as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Returns the number of bytes in the consistency requirement.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns whether the value is empty.
    ///
    /// This is unreachable for successfully constructed values.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl AsRef<str> for ConsistencyRequirement {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl TryFrom<&str> for ConsistencyRequirement {
    type Error = ConsistencyRequirementValidationError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl TryFrom<String> for ConsistencyRequirement {
    type Error = ConsistencyRequirementValidationError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl fmt::Debug for ConsistencyRequirement {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("ConsistencyRequirement")
            .field(&self.0)
            .finish()
    }
}

impl fmt::Display for ConsistencyRequirement {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// Canonical logical definition of one indexing space.
///
/// This is a descriptive contract. It does not identify or contain a
/// physical provider, storage resource, partition, shard, database table,
/// query plan, or executable build operation.
///
/// `IndexDefinitionIdentity` supplies the Phase 1 identity components:
/// definition ID, namespace, and family.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct IndexDefinition {
    identity: IndexDefinitionIdentity,
    key_definition: KeyDefinition,
    target_reference_type: TargetReferenceType,
    uniqueness: Uniqueness,
    consistency_requirement: ConsistencyRequirement,
    source_version: Option<SourceVersion>,
    schema_version: Option<SchemaVersion>,
    lifecycle_metadata: Option<KeyMaterial>,
}

impl IndexDefinition {
    /// Constructs a canonical logical index definition.
    ///
    /// All structural inputs are validated before the definition is returned.
    pub fn new(
        identity: IndexDefinitionIdentity,
        key_definition: KeyDefinition,
        target_reference_type: TargetReferenceType,
        uniqueness: Uniqueness,
        consistency_requirement: ConsistencyRequirement,
        source_version: Option<SourceVersion>,
        schema_version: Option<SchemaVersion>,
    ) -> Result<Self, IndexDefinitionValidationError> {
        Self::with_metadata(
            identity,
            key_definition,
            target_reference_type,
            uniqueness,
            consistency_requirement,
            source_version,
            schema_version,
            None,
        )
    }

    /// Constructs a canonical logical index definition with generic logical
    /// lifecycle metadata.
    ///
    /// The metadata is descriptive only. This method does not create or
    /// transition lifecycle state.
    #[allow(clippy::too_many_arguments)]
    pub fn with_metadata(
        identity: IndexDefinitionIdentity,
        key_definition: KeyDefinition,
        target_reference_type: TargetReferenceType,
        uniqueness: Uniqueness,
        consistency_requirement: ConsistencyRequirement,
        source_version: Option<SourceVersion>,
        schema_version: Option<SchemaVersion>,
        lifecycle_metadata: Option<KeyMaterial>,
    ) -> Result<Self, IndexDefinitionValidationError> {
        key_definition
            .validate()
            .map_err(IndexDefinitionValidationError::InvalidKeyDefinition)?;

        if let Some(metadata) = &lifecycle_metadata {
            metadata
                .validate()
                .map_err(IndexDefinitionValidationError::InvalidLifecycleMetadata)?;
        }

        Ok(Self {
            identity,
            key_definition,
            target_reference_type,
            uniqueness,
            consistency_requirement,
            source_version,
            schema_version,
            lifecycle_metadata,
        })
    }

    /// Returns the complete Phase 1 definition identity.
    #[must_use]
    pub fn identity(&self) -> &IndexDefinitionIdentity {
        &self.identity
    }

    /// Returns the logical definition identifier.
    #[must_use]
    pub fn definition_id(&self) -> &IndexDefinitionId {
        self.identity.definition_id()
    }

    /// Returns the logical namespace.
    #[must_use]
    pub fn namespace(&self) -> &IndexNamespace {
        self.identity.namespace()
    }

    /// Returns the indexing family.
    #[must_use]
    pub fn family(&self) -> IndexFamily {
        self.identity.family()
    }

    /// Returns the generic logical key definition.
    #[must_use]
    pub fn key_definition(&self) -> &KeyDefinition {
        &self.key_definition
    }

    /// Returns the logical target-reference type.
    #[must_use]
    pub fn target_reference_type(&self) -> &TargetReferenceType {
        &self.target_reference_type
    }

    /// Returns the logical uniqueness requirement.
    #[must_use]
    pub fn uniqueness(&self) -> Uniqueness {
        self.uniqueness
    }

    /// Returns the logical consistency requirement.
    #[must_use]
    pub fn consistency_requirement(&self) -> &ConsistencyRequirement {
        &self.consistency_requirement
    }

    /// Returns the associated source-state version, if supplied.
    #[must_use]
    pub fn source_version(&self) -> Option<&SourceVersion> {
        self.source_version.as_ref()
    }

    /// Returns the associated schema version, if supplied.
    #[must_use]
    pub fn schema_version(&self) -> Option<&SchemaVersion> {
        self.schema_version.as_ref()
    }

    /// Returns optional generic lifecycle metadata.
    ///
    /// This is metadata only and is not a lifecycle state machine.
    #[must_use]
    pub fn lifecycle_metadata(&self) -> Option<&KeyMaterial> {
        self.lifecycle_metadata.as_ref()
    }

    /// Validates the logical definition.
    ///
    /// Validation is structural/logical only. It does not validate or select a
    /// physical provider and does not determine whether the definition can be
    /// built, queried, published, or stored.
    pub fn validate(&self) -> Result<(), IndexDefinitionValidationError> {
        self.key_definition
            .validate()
            .map_err(IndexDefinitionValidationError::InvalidKeyDefinition)?;

        if let Some(metadata) = &self.lifecycle_metadata {
            metadata
                .validate()
                .map_err(IndexDefinitionValidationError::InvalidLifecycleMetadata)?;
        }

        Ok(())
    }

    /// Consumes the definition and returns its logical components.
    #[must_use]
    pub fn into_parts(self) -> IndexDefinitionParts {
        (
            self.identity,
            self.key_definition,
            self.target_reference_type,
            self.uniqueness,
            self.consistency_requirement,
            self.source_version,
            self.schema_version,
            self.lifecycle_metadata,
        )
    }
}

/// The logical components returned when an [`IndexDefinition`] is consumed.
pub type IndexDefinitionParts = (
    IndexDefinitionIdentity,
    KeyDefinition,
    TargetReferenceType,
    Uniqueness,
    ConsistencyRequirement,
    Option<SourceVersion>,
    Option<SchemaVersion>,
    Option<KeyMaterial>,
);

/// Validation failures for [`TargetReferenceType`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TargetReferenceTypeValidationError {
    /// The target-reference type is structurally invalid.
    InvalidValue(TextValueValidationError),
}

impl fmt::Display for TargetReferenceTypeValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidValue(error) => {
                write!(formatter, "invalid target reference type: {error}")
            }
        }
    }
}

impl std::error::Error for TargetReferenceTypeValidationError {}

/// Validation failures for [`ConsistencyRequirement`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ConsistencyRequirementValidationError {
    /// The consistency requirement is structurally invalid.
    InvalidValue(TextValueValidationError),
}

impl fmt::Display for ConsistencyRequirementValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidValue(error) => {
                write!(formatter, "invalid consistency requirement: {error}")
            }
        }
    }
}

impl std::error::Error for ConsistencyRequirementValidationError {}

/// Structural validation failures for opaque logical text values.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TextValueValidationError {
    /// The value contains no bytes.
    Empty,

    /// The value contains a Unicode control character.
    ControlCharacter {
        /// Byte position of the control character.
        index: usize,
    },
}

impl fmt::Display for TextValueValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("value must not be empty"),
            Self::ControlCharacter { index } => {
                write!(
                    formatter,
                    "value contains a control character at byte index {index}"
                )
            }
        }
    }
}

fn validate_text_value(value: &str) -> Result<(), TextValueValidationError> {
    if value.is_empty() {
        return Err(TextValueValidationError::Empty);
    }

    for (index, character) in value.char_indices() {
        if character.is_control() {
            return Err(TextValueValidationError::ControlCharacter { index });
        }
    }

    Ok(())
}

/// Validation failures for [`IndexDefinition`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum IndexDefinitionValidationError {
    /// The generic key definition is invalid.
    InvalidKeyDefinition(super::key::KeyDefinitionValidationError),

    /// The generic logical lifecycle metadata is invalid.
    InvalidLifecycleMetadata(super::key::KeyMaterialValidationError),
}

impl fmt::Display for IndexDefinitionValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidKeyDefinition(error) => {
                write!(
                    formatter,
                    "invalid index definition key definition: {error}"
                )
            }
            Self::InvalidLifecycleMetadata(error) => {
                write!(
                    formatter,
                    "invalid index definition lifecycle metadata: {error}"
                )
            }
        }
    }
}

impl std::error::Error for IndexDefinitionValidationError {}

#[cfg(test)]
mod tests {
    use super::super::key::KeyDefinitionValidationError;
    use super::*;
    use crate::identity::{IndexDefinitionId, IndexNamespace};

    fn definition_id(value: &str) -> IndexDefinitionId {
        IndexDefinitionId::new(value).expect("test definition ID should be valid")
    }

    fn namespace(value: &str) -> IndexNamespace {
        IndexNamespace::new(value).expect("test namespace should be valid")
    }

    fn identity() -> IndexDefinitionIdentity {
        IndexDefinitionIdentity::new(
            definition_id("lexical.index"),
            namespace("source.lexical"),
            IndexFamily::Inverted,
        )
    }

    fn key_definition() -> KeyDefinition {
        KeyDefinition::new(["term"]).expect("test key definition should be valid")
    }

    fn target_reference_type() -> TargetReferenceType {
        TargetReferenceType::new("source.object")
            .expect("test target reference type should be valid")
    }

    fn consistency() -> ConsistencyRequirement {
        ConsistencyRequirement::new("logical-consistent")
            .expect("test consistency requirement should be valid")
    }

    #[test]
    fn constructs_a_canonical_logical_definition() {
        let definition = IndexDefinition::new(
            identity(),
            key_definition(),
            target_reference_type(),
            Uniqueness::NonUnique,
            consistency(),
            Some(SourceVersion::new("source-v3").expect("valid source version")),
            Some(SchemaVersion::new("schema-v2").expect("valid schema version")),
        )
        .expect("definition should be valid");

        assert_eq!(definition.definition_id().as_str(), "lexical.index");
        assert_eq!(definition.namespace().as_str(), "source.lexical");
        assert_eq!(definition.family(), IndexFamily::Inverted);
        assert_eq!(definition.uniqueness(), Uniqueness::NonUnique);
        assert_eq!(definition.target_reference_type().as_str(), "source.object");
        assert_eq!(
            definition.consistency_requirement().as_str(),
            "logical-consistent"
        );
        assert_eq!(
            definition
                .source_version()
                .expect("source version")
                .as_str(),
            "source-v3"
        );
        assert_eq!(
            definition
                .schema_version()
                .expect("schema version")
                .as_str(),
            "schema-v2"
        );
    }

    #[test]
    fn preserves_phase_one_definition_identity() {
        let identity = identity();
        let definition = IndexDefinition::new(
            identity.clone(),
            key_definition(),
            target_reference_type(),
            Uniqueness::Unique,
            consistency(),
            None,
            None,
        )
        .expect("definition should be valid");

        assert_eq!(definition.identity(), &identity);
        assert_eq!(definition.definition_id(), identity.definition_id());
        assert_eq!(definition.namespace(), identity.namespace());
        assert_eq!(definition.family(), identity.family());
    }

    #[test]
    fn keeps_index_definition_id_separate_from_index_id() {
        let definition = IndexDefinition::new(
            identity(),
            key_definition(),
            target_reference_type(),
            Uniqueness::Unique,
            consistency(),
            None,
            None,
        )
        .expect("definition should be valid");

        // The definition contains the Phase 1 `IndexDefinitionIdentity`, not
        // an `IndexId`. A concrete index resource remains a separate concept.
        assert_eq!(definition.definition_id().as_str(), "lexical.index");
    }

    #[test]
    fn accepts_unicode_in_opaque_logical_values() {
        let target = TargetReferenceType::new("مصدر.كائن")
            .expect("Unicode should remain valid in opaque logical text");

        let consistency = ConsistencyRequirement::new("متسق")
            .expect("Unicode should remain valid in opaque logical text");

        assert_eq!(target.as_str(), "مصدر.كائن");
        assert_eq!(consistency.as_str(), "متسق");
    }

    #[test]
    fn rejects_empty_target_reference_type() {
        let error =
            TargetReferenceType::new("").expect_err("empty target reference type must be rejected");

        assert_eq!(
            error,
            TargetReferenceTypeValidationError::InvalidValue(TextValueValidationError::Empty)
        );
    }

    #[test]
    fn rejects_control_characters_in_logical_text() {
        let target_error = TargetReferenceType::new("source\nobject")
            .expect_err("control characters must be rejected");

        let consistency_error = ConsistencyRequirement::new("logical\tconsistent")
            .expect_err("control characters must be rejected");

        assert_eq!(
            target_error,
            TargetReferenceTypeValidationError::InvalidValue(
                TextValueValidationError::ControlCharacter { index: 6 }
            )
        );
        assert_eq!(
            consistency_error,
            ConsistencyRequirementValidationError::InvalidValue(
                TextValueValidationError::ControlCharacter { index: 7 }
            )
        );
    }

    #[test]
    fn rejects_invalid_key_definition_before_index_definition_construction() {
        let error = KeyDefinition::new(["valid", "valid"])
            .expect_err("duplicate key fields should be rejected");

        assert_eq!(
            error,
            KeyDefinitionValidationError::DuplicateField("valid".to_owned())
        );
    }

    #[test]
    fn rejects_invalid_lifecycle_metadata() {
        let result = IndexDefinition::with_metadata(
            identity(),
            key_definition(),
            target_reference_type(),
            Uniqueness::NonUnique,
            consistency(),
            None,
            None,
            Some(KeyMaterial::text("invalid\nmetadata")),
        );

        assert!(matches!(
            result,
            Err(IndexDefinitionValidationError::InvalidLifecycleMetadata(_))
        ));
    }

    #[test]
    fn preserves_generic_lifecycle_metadata_without_interpreting_it() {
        let metadata = KeyMaterial::map([
            ("generation-note", KeyMaterial::text("candidate")),
            ("revision", KeyMaterial::Unsigned(2)),
        ])
        .expect("test metadata should be valid");

        let definition = IndexDefinition::with_metadata(
            identity(),
            key_definition(),
            target_reference_type(),
            Uniqueness::NonUnique,
            consistency(),
            None,
            None,
            Some(metadata.clone()),
        )
        .expect("definition should be valid");

        assert_eq!(definition.lifecycle_metadata(), Some(&metadata));
    }

    #[test]
    fn validation_is_repeatable() {
        let definition = IndexDefinition::with_metadata(
            identity(),
            key_definition(),
            target_reference_type(),
            Uniqueness::Unique,
            consistency(),
            Some(SourceVersion::new("source-v1").expect("valid source")),
            Some(SchemaVersion::new("schema-v1").expect("valid schema")),
            Some(KeyMaterial::Unsigned(1)),
        )
        .expect("definition should be valid");

        definition.validate().expect("definition should validate");
        definition
            .validate()
            .expect("validation should be deterministic");
    }

    #[test]
    fn into_parts_preserves_all_logical_components() {
        let identity = identity();
        let key = key_definition();
        let target = target_reference_type();
        let source = SourceVersion::new("source-v1").expect("valid source");
        let schema = SchemaVersion::new("schema-v1").expect("valid schema");
        let consistency = consistency();
        let metadata = Some(KeyMaterial::Unsigned(7));

        let definition = IndexDefinition::with_metadata(
            identity.clone(),
            key.clone(),
            target.clone(),
            Uniqueness::Unique,
            consistency.clone(),
            Some(source.clone()),
            Some(schema.clone()),
            metadata.clone(),
        )
        .expect("definition should be valid");

        let (
            returned_identity,
            returned_key,
            returned_target,
            returned_uniqueness,
            returned_consistency,
            returned_source,
            returned_schema,
            returned_metadata,
        ) = definition.into_parts();

        assert_eq!(returned_identity, identity);
        assert_eq!(returned_key, key);
        assert_eq!(returned_target, target);
        assert_eq!(returned_uniqueness, Uniqueness::Unique);
        assert_eq!(returned_consistency, consistency);
        assert_eq!(returned_source, Some(source));
        assert_eq!(returned_schema, Some(schema));
        assert_eq!(returned_metadata, metadata);
    }

    #[test]
    fn definition_is_descriptive_not_executable() {
        let definition = IndexDefinition::new(
            identity(),
            key_definition(),
            target_reference_type(),
            Uniqueness::NonUnique,
            consistency(),
            None,
            None,
        )
        .expect("definition should be valid");

        // The public API exposes logical description only. No provider,
        // storage, partition, shard, query-plan, or build operation is owned
        // by this type.
        assert_eq!(definition.family(), IndexFamily::Inverted);
    }

    #[test]
    fn relationship_family_does_not_gain_predicate_semantics() {
        let relationship_identity = IndexDefinitionIdentity::new(
            definition_id("relationship.index"),
            namespace("source.relationship"),
            IndexFamily::Relationship,
        );

        let definition = IndexDefinition::new(
            relationship_identity,
            key_definition(),
            target_reference_type(),
            Uniqueness::NonUnique,
            consistency(),
            None,
            None,
        )
        .expect("relationship definition should be valid");

        // `Relationship` remains only an indexing family. No predicate or
        // semantic relationship model is introduced by this definition.
        assert_eq!(definition.family(), IndexFamily::Relationship);
    }
}
