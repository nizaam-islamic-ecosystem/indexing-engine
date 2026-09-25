//! Logical reference to a source-owned object.
//!
//! Phase 2 defines `ObjectReference` as a generic, source-neutral reference
//! carried by the Indexing Engine. The referenced object remains owned by its
//! source/domain engine.
//!
//! This module intentionally does not:
//! - load or hydrate domain objects,
//! - query source engines,
//! - define domain schemas or entity types,
//! - create a second canonical object-identity system,
//! - generate `IndexId` values,
//! - perform physical storage or provider operations.
//!
//! `ObjectReference` is therefore a logical value contract rather than a
//! domain-object or storage representation.

use core::fmt;
use std::error::Error;

/// A validated, source-owned reference to one logical object.
///
/// The source identifies the owner of the referenced object. The object
/// reference itself is opaque to the Indexing Engine: Indexing carries and
/// compares it but does not interpret its domain meaning.
///
/// This type is deliberately distinct from:
///
/// - [`crate::identity::IndexId`],
/// - [`crate::identity::IndexDefinitionId`], and
/// - the source/domain object's canonical identity.
///
/// `ObjectReference` does not load, hydrate, query, or own the referenced
/// object.
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct ObjectReference {
    source: String,
    object_reference: String,
}

impl ObjectReference {
    /// Constructs a validated object reference.
    ///
    /// Both the source and the opaque object-reference value must be
    /// non-empty and must not contain Unicode control characters.
    ///
    /// The validation is intentionally structural. It does not contact the
    /// source engine and therefore does not establish whether the referenced
    /// object actually exists.
    pub fn new(
        source: impl Into<String>,
        object_reference: impl Into<String>,
    ) -> Result<Self, ObjectReferenceValidationError> {
        let source = source.into();
        let object_reference = object_reference.into();

        validate_component(&source, ObjectReferenceComponent::Source)?;
        validate_component(&object_reference, ObjectReferenceComponent::ObjectReference)?;

        Ok(Self {
            source,
            object_reference,
        })
    }

    /// Returns the source/owner identifier.
    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Returns the opaque source-owned object reference.
    #[must_use]
    pub fn object_reference(&self) -> &str {
        &self.object_reference
    }

    /// Consumes the reference and returns its logical components.
    #[must_use]
    pub fn into_parts(self) -> (String, String) {
        (self.source, self.object_reference)
    }
}

impl fmt::Debug for ObjectReference {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ObjectReference")
            .field("source", &self.source)
            .field("object_reference", &self.object_reference)
            .finish()
    }
}

/// Validation failures for [`ObjectReference`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ObjectReferenceValidationError {
    /// The source/owner identifier is empty.
    EmptySource,

    /// The opaque object reference is empty.
    EmptyObjectReference,

    /// The source/owner identifier contains a Unicode control character.
    SourceControlCharacter {
        /// Byte position of the control character.
        index: usize,
    },

    /// The opaque object reference contains a Unicode control character.
    ObjectReferenceControlCharacter {
        /// Byte position of the control character.
        index: usize,
    },
}

impl fmt::Display for ObjectReferenceValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptySource => {
                formatter.write_str("object reference source/owner identifier must not be empty")
            }
            Self::EmptyObjectReference => {
                formatter.write_str("object reference value must not be empty")
            }
            Self::SourceControlCharacter { index } => {
                write!(
                    formatter,
                    "object reference source/owner identifier contains a control character \
                     at byte index {index}"
                )
            }
            Self::ObjectReferenceControlCharacter { index } => {
                write!(
                    formatter,
                    "object reference value contains a control character at byte index {index}"
                )
            }
        }
    }
}

impl Error for ObjectReferenceValidationError {}

#[derive(Clone, Copy)]
enum ObjectReferenceComponent {
    Source,
    ObjectReference,
}

fn validate_component(
    value: &str,
    component: ObjectReferenceComponent,
) -> Result<(), ObjectReferenceValidationError> {
    if value.is_empty() {
        return Err(match component {
            ObjectReferenceComponent::Source => ObjectReferenceValidationError::EmptySource,
            ObjectReferenceComponent::ObjectReference => {
                ObjectReferenceValidationError::EmptyObjectReference
            }
        });
    }

    for (index, character) in value.char_indices() {
        if character.is_control() {
            return Err(match component {
                ObjectReferenceComponent::Source => {
                    ObjectReferenceValidationError::SourceControlCharacter { index }
                }
                ObjectReferenceComponent::ObjectReference => {
                    ObjectReferenceValidationError::ObjectReferenceControlCharacter { index }
                }
            });
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reference(source: &str, object_reference: &str) -> ObjectReference {
        ObjectReference::new(source, object_reference)
            .expect("test object reference should be valid")
    }

    #[test]
    fn constructs_a_valid_object_reference() {
        let value = reference("quran", "verse:2:255");

        assert_eq!(value.source(), "quran");
        assert_eq!(value.object_reference(), "verse:2:255");
    }

    #[test]
    fn preserves_opaque_reference_contents_without_interpreting_them() {
        let value = reference("source-a", "opaque/value::123");

        assert_eq!(value.source(), "source-a");
        assert_eq!(value.object_reference(), "opaque/value::123");
    }

    #[test]
    fn rejects_an_empty_source() {
        let error =
            ObjectReference::new("", "object-1").expect_err("empty source must be rejected");

        assert_eq!(error, ObjectReferenceValidationError::EmptySource);
    }

    #[test]
    fn rejects_an_empty_object_reference() {
        let error = ObjectReference::new("source-a", "")
            .expect_err("empty object reference must be rejected");

        assert_eq!(error, ObjectReferenceValidationError::EmptyObjectReference);
    }

    #[test]
    fn rejects_a_control_character_in_the_source() {
        let error = ObjectReference::new("source\n", "object-1")
            .expect_err("source control character must be rejected");

        assert_eq!(
            error,
            ObjectReferenceValidationError::SourceControlCharacter { index: 6 }
        );
    }

    #[test]
    fn rejects_a_control_character_in_the_object_reference() {
        let error = ObjectReference::new("source-a", "object\t1")
            .expect_err("object-reference control character must be rejected");

        assert_eq!(
            error,
            ObjectReferenceValidationError::ObjectReferenceControlCharacter { index: 6 }
        );
    }

    #[test]
    fn equality_depends_on_the_complete_logical_reference() {
        let first = reference("source-a", "object-1");
        let second = reference("source-a", "object-1");
        let different_source = reference("source-b", "object-1");
        let different_object = reference("source-a", "object-2");

        assert_eq!(first, second);
        assert_ne!(first, different_source);
        assert_ne!(first, different_object);
    }

    #[test]
    fn ordering_is_deterministic() {
        let first = reference("source-a", "object-1");
        let second = reference("source-a", "object-2");

        assert!(first < second);
    }

    #[test]
    fn clone_preserves_the_reference() {
        let original = reference("source-a", "object-1");
        let cloned = original.clone();

        assert_eq!(original, cloned);
    }

    #[test]
    fn into_parts_preserves_both_components() {
        let value = reference("source-a", "object-1");

        let (source, object_reference) = value.into_parts();

        assert_eq!(source, "source-a");
        assert_eq!(object_reference, "object-1");
    }

    #[test]
    fn unicode_is_allowed_when_it_is_not_a_control_character() {
        let value = reference("مصدر", "كائن-١");

        assert_eq!(value.source(), "مصدر");
        assert_eq!(value.object_reference(), "كائن-١");
    }

    #[test]
    fn whitespace_is_not_rejected_by_the_reference_contract() {
        let value = reference("source a", "object b");

        assert_eq!(value.source(), "source a");
        assert_eq!(value.object_reference(), "object b");
    }

    #[test]
    fn debug_identifies_the_logical_reference_type() {
        let value = reference("source-a", "object-1");
        let debug = format!("{value:?}");

        assert!(debug.contains("ObjectReference"));
        assert!(debug.contains("source-a"));
        assert!(debug.contains("object-1"));
    }

    #[test]
    fn validation_does_not_perform_source_existence_checks() {
        // A structurally valid reference is accepted without requiring a
        // source engine, repository, database, or network connection.
        let value = reference("future-source", "object-that-may-not-exist");

        assert_eq!(value.source(), "future-source");
        assert_eq!(value.object_reference(), "object-that-may-not-exist");
    }
}
