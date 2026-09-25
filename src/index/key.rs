//! Generic logical key definitions and key material for the Indexing Engine.
//!
//! Phase 2 keeps key representation source-neutral and provider-neutral.
//! A key has no domain meaning inside Indexing; it is only logical material
//! that can participate in an index definition or index entry.
//!
//! This module intentionally does not define:
//! - physical index keys,
//! - database serialization,
//! - provider-specific key formats,
//! - embeddings,
//! - domain semantics,
//! - query execution.
//!
//! The canonical byte representation provided here is an Indexing-owned
//! deterministic representation for logical identity/canonicalization. It is
//! not a general-purpose serialization format.

use core::fmt;
use std::collections::BTreeMap;
use std::error::Error;

/// A logical definition of the fields that make up an index key.
///
/// Field names are source-defined labels. Indexing does not assign semantic
/// meaning to them.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct KeyDefinition {
    fields: Vec<KeyField>,
}

impl KeyDefinition {
    /// Creates a key definition from an ordered collection of field names.
    ///
    /// Field names must be non-empty and unique.
    pub fn new<I, S>(fields: I) -> Result<Self, KeyDefinitionValidationError>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let mut normalized = Vec::new();

        for field in fields {
            let field = field.into();

            validate_field_name(&field)?;

            if normalized
                .iter()
                .any(|existing: &KeyField| existing.name() == field)
            {
                return Err(KeyDefinitionValidationError::DuplicateField(field));
            }

            normalized.push(KeyField(field));
        }

        if normalized.is_empty() {
            return Err(KeyDefinitionValidationError::Empty);
        }

        Ok(Self { fields: normalized })
    }

    /// Returns the fields in their declared logical order.
    #[must_use]
    pub fn fields(&self) -> &[KeyField] {
        &self.fields
    }

    /// Returns the number of key fields.
    #[must_use]
    pub fn len(&self) -> usize {
        self.fields.len()
    }

    /// Returns whether the definition contains no fields.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.fields.is_empty()
    }

    /// Validates the complete logical key definition.
    ///
    /// Validation is purely structural and deterministic. It checks the same
    /// invariants enforced by [`KeyDefinition::new`]: at least one field,
    /// valid field names, and unique field names.
    pub fn validate(&self) -> Result<(), KeyDefinitionValidationError> {
        if self.fields.is_empty() {
            return Err(KeyDefinitionValidationError::Empty);
        }

        for (position, field) in self.fields.iter().enumerate() {
            validate_field_name(field.name())?;

            if self
                .fields
                .iter()
                .take(position)
                .any(|existing| existing.name() == field.name())
            {
                return Err(KeyDefinitionValidationError::DuplicateField(
                    field.name().to_owned(),
                ));
            }
        }

        Ok(())
    }

    /// Returns the deterministic canonical bytes of the key definition.
    ///
    /// This representation is an Indexing identity/canonicalization primitive,
    /// not a general-purpose serialization format.
    #[must_use]
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"NIZAAM-INDEX-KEY-DEFINITION\0");
        encode_u64(self.fields.len() as u64, &mut bytes);

        for field in &self.fields {
            encode_text(field.name(), &mut bytes);
        }

        bytes
    }
}

/// One source-defined logical field in a [`KeyDefinition`].
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct KeyField(String);

impl KeyField {
    /// Creates a validated key field.
    pub fn new(name: impl Into<String>) -> Result<Self, KeyDefinitionValidationError> {
        let name = name.into();
        validate_field_name(&name)?;
        Ok(Self(name))
    }

    /// Returns the source-defined field name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.0
    }
}

/// Generic logical key material.
///
/// This is deliberately a small, deterministic value model. It allows
/// sources to represent structured keys without forcing Indexing to
/// understand domain semantics or a physical storage format.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum KeyMaterial {
    /// No value.
    Null,

    /// Boolean value.
    Bool(bool),

    /// Signed integer value.
    Integer(i128),

    /// Unsigned integer value.
    Unsigned(u128),

    /// UTF-8 text.
    Text(String),

    /// Opaque bytes supplied by the source.
    Bytes(Vec<u8>),

    /// Ordered logical sequence.
    Sequence(Vec<KeyMaterial>),

    /// Deterministically ordered named fields.
    Map(BTreeMap<String, KeyMaterial>),
}

impl KeyMaterial {
    /// Constructs text key material.
    #[must_use]
    pub fn text(value: impl Into<String>) -> Self {
        Self::Text(value.into())
    }

    /// Constructs opaque byte key material.
    #[must_use]
    pub fn bytes(value: impl Into<Vec<u8>>) -> Self {
        Self::Bytes(value.into())
    }

    /// Constructs a named-field map.
    ///
    /// Field names must be non-empty and must not contain control
    /// characters. `BTreeMap` provides deterministic logical ordering.
    pub fn map<I, K>(fields: I) -> Result<Self, KeyMaterialValidationError>
    where
        I: IntoIterator<Item = (K, KeyMaterial)>,
        K: Into<String>,
    {
        let mut map = BTreeMap::new();

        for (name, value) in fields {
            let name = name.into();
            validate_material_field_name(&name)?;

            if map.insert(name.clone(), value).is_some() {
                return Err(KeyMaterialValidationError::DuplicateMapField(name));
            }
        }

        Ok(Self::Map(map))
    }

    /// Returns deterministic canonical bytes for this logical key material.
    ///
    /// The encoding is owned by the Indexing logical contract and is intended
    /// for deterministic identity generation/canonicalization. It is not
    /// declared to be a wire or persistence serialization format.
    #[must_use]
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        self.encode_canonical(&mut bytes);
        bytes
    }

    /// Validates the complete key-material tree.
    pub fn validate(&self) -> Result<(), KeyMaterialValidationError> {
        match self {
            Self::Null | Self::Bool(_) | Self::Integer(_) | Self::Unsigned(_) => Ok(()),

            Self::Text(value) => validate_text(value),

            Self::Bytes(_) => Ok(()),

            Self::Sequence(values) => {
                for value in values {
                    value.validate()?;
                }
                Ok(())
            }

            Self::Map(fields) => {
                for (name, value) in fields {
                    validate_material_field_name(name)?;
                    value.validate()?;
                }
                Ok(())
            }
        }
    }

    fn encode_canonical(&self, output: &mut Vec<u8>) {
        match self {
            Self::Null => output.push(0x00),

            Self::Bool(false) => output.push(0x01),
            Self::Bool(true) => output.push(0x02),

            Self::Integer(value) => {
                output.push(0x03);
                output.extend_from_slice(&value.to_be_bytes());
            }

            Self::Unsigned(value) => {
                output.push(0x04);
                output.extend_from_slice(&value.to_be_bytes());
            }

            Self::Text(value) => {
                output.push(0x05);
                encode_text(value, output);
            }

            Self::Bytes(value) => {
                output.push(0x06);
                encode_bytes(value, output);
            }

            Self::Sequence(values) => {
                output.push(0x07);
                encode_u64(values.len() as u64, output);

                for value in values {
                    value.encode_canonical(output);
                }
            }

            Self::Map(fields) => {
                output.push(0x08);
                encode_u64(fields.len() as u64, output);

                for (name, value) in fields {
                    encode_text(name, output);
                    value.encode_canonical(output);
                }
            }
        }
    }
}

/// Validation failures for [`KeyDefinition`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum KeyDefinitionValidationError {
    /// The definition contains no fields.
    Empty,

    /// A field name is empty.
    EmptyField,

    /// A field name contains a control character.
    ControlCharacter { field: String, index: usize },

    /// The same field occurs more than once.
    DuplicateField(String),
}

impl fmt::Display for KeyDefinitionValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("key definition must contain at least one field"),
            Self::EmptyField => formatter.write_str("key-definition field name must not be empty"),
            Self::ControlCharacter { field, index } => write!(
                formatter,
                "key-definition field {field:?} contains a control character at byte index {index}"
            ),
            Self::DuplicateField(field) => {
                write!(
                    formatter,
                    "key-definition contains duplicate field {field:?}"
                )
            }
        }
    }
}

impl Error for KeyDefinitionValidationError {}

/// Validation failures for [`KeyMaterial`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum KeyMaterialValidationError {
    /// A map field name is empty.
    EmptyMapField,

    /// A map field name contains a control character.
    ControlCharacter { field: String, index: usize },

    /// A map contains the same field more than once.
    DuplicateMapField(String),

    /// Text contains a control character.
    TextControlCharacter { index: usize },
}

impl fmt::Display for KeyMaterialValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyMapField => {
                formatter.write_str("key-material map field name must not be empty")
            }
            Self::ControlCharacter { field, index } => write!(
                formatter,
                "key-material map field {field:?} contains a control character at byte index {index}"
            ),
            Self::DuplicateMapField(field) => {
                write!(
                    formatter,
                    "key-material map contains duplicate field {field:?}"
                )
            }
            Self::TextControlCharacter { index } => write!(
                formatter,
                "key-material text contains a control character at byte index {index}"
            ),
        }
    }
}

impl Error for KeyMaterialValidationError {}

fn validate_field_name(value: &str) -> Result<(), KeyDefinitionValidationError> {
    if value.is_empty() {
        return Err(KeyDefinitionValidationError::EmptyField);
    }

    for (index, character) in value.char_indices() {
        if character.is_control() {
            return Err(KeyDefinitionValidationError::ControlCharacter {
                field: value.to_owned(),
                index,
            });
        }
    }

    Ok(())
}

fn validate_material_field_name(value: &str) -> Result<(), KeyMaterialValidationError> {
    if value.is_empty() {
        return Err(KeyMaterialValidationError::EmptyMapField);
    }

    for (index, character) in value.char_indices() {
        if character.is_control() {
            return Err(KeyMaterialValidationError::ControlCharacter {
                field: value.to_owned(),
                index,
            });
        }
    }

    Ok(())
}

fn validate_text(value: &str) -> Result<(), KeyMaterialValidationError> {
    for (index, character) in value.char_indices() {
        if character.is_control() {
            return Err(KeyMaterialValidationError::TextControlCharacter { index });
        }
    }

    Ok(())
}

fn encode_u64(value: u64, output: &mut Vec<u8>) {
    output.extend_from_slice(&value.to_be_bytes());
}

fn encode_text(value: &str, output: &mut Vec<u8>) {
    encode_bytes(value.as_bytes(), output);
}

fn encode_bytes(value: &[u8], output: &mut Vec<u8>) {
    encode_u64(value.len() as u64, output);
    output.extend_from_slice(value);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_definition_requires_at_least_one_field() {
        let error = KeyDefinition::new(Vec::<String>::new())
            .expect_err("empty key definition must be rejected");

        assert_eq!(error, KeyDefinitionValidationError::Empty);
    }

    #[test]
    fn key_definition_rejects_empty_field_names() {
        let error = KeyDefinition::new([""]).expect_err("empty field must be rejected");

        assert_eq!(error, KeyDefinitionValidationError::EmptyField);
    }

    #[test]
    fn key_definition_rejects_duplicate_fields() {
        let error =
            KeyDefinition::new(["source", "source"]).expect_err("duplicate field must be rejected");

        assert_eq!(
            error,
            KeyDefinitionValidationError::DuplicateField("source".to_owned())
        );
    }

    #[test]
    fn key_definition_validation_is_repeatable() {
        let definition = KeyDefinition::new(["source", "predicate"])
            .expect("valid key definition should be constructed");

        definition
            .validate()
            .expect("constructed key definition should validate");
        definition
            .validate()
            .expect("validation should remain deterministic");
    }

    #[test]
    fn key_definition_preserves_declared_field_order() {
        let definition =
            KeyDefinition::new(["source", "predicate", "target"]).expect("valid definition");

        let names: Vec<_> = definition.fields().iter().map(KeyField::name).collect();

        assert_eq!(names, ["source", "predicate", "target"]);
    }

    #[test]
    fn key_definition_canonical_bytes_are_deterministic() {
        let first = KeyDefinition::new(["source", "predicate"]).expect("valid definition");
        let second = KeyDefinition::new(["source", "predicate"]).expect("valid definition");

        assert_eq!(first.canonical_bytes(), second.canonical_bytes());
    }

    #[test]
    fn changing_key_definition_changes_canonical_bytes() {
        let first = KeyDefinition::new(["source", "predicate"]).expect("valid definition");
        let second = KeyDefinition::new(["source", "target"]).expect("valid definition");

        assert_ne!(first.canonical_bytes(), second.canonical_bytes());
    }

    #[test]
    fn key_material_supports_generic_scalar_values() {
        let values = [
            KeyMaterial::Null,
            KeyMaterial::Bool(true),
            KeyMaterial::Integer(-10),
            KeyMaterial::Unsigned(10),
            KeyMaterial::text("value"),
            KeyMaterial::bytes(vec![1, 2, 3]),
        ];

        for value in values {
            value.validate().expect("scalar key material must validate");
            assert!(!value.canonical_bytes().is_empty());
        }
    }

    #[test]
    fn key_material_supports_structured_values() {
        let material = KeyMaterial::map([
            ("source", KeyMaterial::text("x")),
            (
                "target",
                KeyMaterial::Sequence(vec![KeyMaterial::Unsigned(1), KeyMaterial::Unsigned(2)]),
            ),
        ])
        .expect("valid map");

        material
            .validate()
            .expect("structured material must validate");
        assert!(!material.canonical_bytes().is_empty());
    }

    #[test]
    fn map_material_has_deterministic_order() {
        let first = KeyMaterial::map([
            ("b", KeyMaterial::Unsigned(2)),
            ("a", KeyMaterial::Unsigned(1)),
        ])
        .expect("valid map");

        let second = KeyMaterial::map([
            ("a", KeyMaterial::Unsigned(1)),
            ("b", KeyMaterial::Unsigned(2)),
        ])
        .expect("valid map");

        assert_eq!(first, second);
        assert_eq!(first.canonical_bytes(), second.canonical_bytes());
    }

    #[test]
    fn sequence_order_is_semantically_preserved() {
        let first = KeyMaterial::Sequence(vec![KeyMaterial::Unsigned(1), KeyMaterial::Unsigned(2)]);
        let second =
            KeyMaterial::Sequence(vec![KeyMaterial::Unsigned(2), KeyMaterial::Unsigned(1)]);

        assert_ne!(first, second);
        assert_ne!(first.canonical_bytes(), second.canonical_bytes());
    }

    #[test]
    fn different_key_material_produces_different_canonical_bytes() {
        let first = KeyMaterial::text("one");
        let second = KeyMaterial::text("two");

        assert_ne!(first.canonical_bytes(), second.canonical_bytes());
    }

    #[test]
    fn key_material_rejects_control_characters_in_map_names() {
        let error = KeyMaterial::map([("source\n", KeyMaterial::text("value"))])
            .expect_err("control character must be rejected");

        assert_eq!(
            error,
            KeyMaterialValidationError::ControlCharacter {
                field: "source\n".to_owned(),
                index: 6,
            }
        );
    }

    #[test]
    fn key_material_rejects_control_characters_in_text() {
        let material = KeyMaterial::text("value\t");

        let error = material
            .validate()
            .expect_err("control character must be rejected");

        assert_eq!(
            error,
            KeyMaterialValidationError::TextControlCharacter { index: 5 }
        );
    }

    #[test]
    fn unicode_text_is_supported() {
        let material = KeyMaterial::text("كلمة");

        material.validate().expect("unicode text should be valid");
        assert!(!material.canonical_bytes().is_empty());
    }

    #[test]
    fn opaque_bytes_are_not_interpreted_as_text() {
        let material = KeyMaterial::bytes(vec![0, 255, 1, 254]);

        material.validate().expect("opaque bytes should be valid");
        assert!(!material.canonical_bytes().is_empty());
    }

    #[test]
    fn canonical_encoding_distinguishes_value_kinds() {
        let text = KeyMaterial::text("1");
        let unsigned = KeyMaterial::Unsigned(1);

        assert_ne!(text.canonical_bytes(), unsigned.canonical_bytes());
    }

    #[test]
    fn key_material_validation_is_recursive() {
        let material = KeyMaterial::Sequence(vec![KeyMaterial::Map(
            [("nested".to_owned(), KeyMaterial::text("bad\nvalue"))]
                .into_iter()
                .collect(),
        )]);

        let error = material
            .validate()
            .expect_err("nested invalid material must be rejected");

        assert_eq!(
            error,
            KeyMaterialValidationError::TextControlCharacter { index: 3 }
        );
    }
}
