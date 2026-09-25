//! Identity for a logical Indexing definition.
//!
//! Phase 1 establishes only the identity boundary of an index definition.
//! The complete `IndexDefinition` data contract is deferred to Phase 2.
//!
//! `IndexDefinitionIdentity` combines:
//!
//! - an `IndexDefinitionId` identifying the logical definition,
//! - an `IndexNamespace` identifying the logical indexing space, and
//! - an `IndexFamily` identifying the broad indexing family.
//!
//! This module does not define key material, reference types, uniqueness,
//! consistency requirements, versions, lifecycle state, storage, or physical
//! indexing behavior.

use core::fmt;
use std::error::Error;

use crate::identity::IndexNamespace;
use crate::index::IndexFamily;

/// A validated identifier for a logical Indexing definition.
///
/// The representation is intentionally a string newtype. Phase 1 does not
/// assign domain meaning to the identifier and does not constrain it to a
/// physical format or a particular serialization.
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct IndexDefinitionId(String);

impl IndexDefinitionId {
    /// Constructs an `IndexDefinitionId`.
    ///
    /// Phase 1 validation requires a non-empty value containing no control
    /// characters. The remaining representation is intentionally opaque so
    /// that later definition and transport decisions are not frozen here.
    pub fn new(value: impl Into<String>) -> Result<Self, IndexDefinitionIdValidationError> {
        let value = value.into();
        validate_definition_id(&value)?;
        Ok(Self(value))
    }

    /// Returns the identifier as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consumes the identifier and returns its owned string.
    #[must_use]
    pub fn into_string(self) -> String {
        self.0
    }

    /// Returns the number of bytes in the identifier.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns whether the identifier contains no bytes.
    ///
    /// This is unreachable for successfully constructed values.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl TryFrom<&str> for IndexDefinitionId {
    type Error = IndexDefinitionIdValidationError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl TryFrom<String> for IndexDefinitionId {
    type Error = IndexDefinitionIdValidationError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl AsRef<str> for IndexDefinitionId {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Debug for IndexDefinitionId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("IndexDefinitionId")
            .field(&self.0)
            .finish()
    }
}

impl fmt::Display for IndexDefinitionId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// Validation failures for [`IndexDefinitionId`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IndexDefinitionIdValidationError {
    /// The identifier contains no characters.
    Empty,

    /// The identifier contains a Unicode control character.
    ControlCharacter {
        /// Byte position of the control character.
        index: usize,
    },
}

impl fmt::Display for IndexDefinitionIdValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("index definition ID must not be empty"),
            Self::ControlCharacter { index } => {
                write!(
                    formatter,
                    "index definition ID contains a control character at byte index {index}"
                )
            }
        }
    }
}

impl Error for IndexDefinitionIdValidationError {}

fn validate_definition_id(value: &str) -> Result<(), IndexDefinitionIdValidationError> {
    if value.is_empty() {
        return Err(IndexDefinitionIdValidationError::Empty);
    }

    for (index, character) in value.char_indices() {
        if character.is_control() {
            return Err(IndexDefinitionIdValidationError::ControlCharacter { index });
        }
    }

    Ok(())
}

/// Identity of a logical Indexing definition within a logical indexing space.
///
/// This is intentionally not the complete Phase 2 `IndexDefinition`. It only
/// identifies the definition together with the namespace and indexing family
/// required by the Phase 1 logical index-space model.
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct IndexDefinitionIdentity {
    definition_id: IndexDefinitionId,
    namespace: IndexNamespace,
    family: IndexFamily,
}

impl IndexDefinitionIdentity {
    /// Constructs a definition identity from its Phase 1 identity components.
    #[must_use]
    pub fn new(
        definition_id: IndexDefinitionId,
        namespace: IndexNamespace,
        family: IndexFamily,
    ) -> Self {
        Self {
            definition_id,
            namespace,
            family,
        }
    }

    /// Returns the logical definition identifier.
    #[must_use]
    pub fn definition_id(&self) -> &IndexDefinitionId {
        &self.definition_id
    }

    /// Returns the logical namespace associated with the definition.
    #[must_use]
    pub fn namespace(&self) -> &IndexNamespace {
        &self.namespace
    }

    /// Returns the broad indexing family associated with the definition.
    #[must_use]
    pub fn family(&self) -> IndexFamily {
        self.family
    }

    /// Consumes the identity and returns its three Phase 1 components.
    #[must_use]
    pub fn into_parts(self) -> (IndexDefinitionId, IndexNamespace, IndexFamily) {
        (self.definition_id, self.namespace, self.family)
    }
}

impl fmt::Debug for IndexDefinitionIdentity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("IndexDefinitionIdentity")
            .field("definition_id", &self.definition_id)
            .field("namespace", &self.namespace)
            .field("family", &self.family)
            .finish()
    }
}

impl fmt::Display for IndexDefinitionIdentity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("IndexDefinitionIdentity")
            .field("definition_id", &self.definition_id)
            .field("namespace", &self.namespace)
            .field("family", &self.family)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn definition_id(value: &str) -> IndexDefinitionId {
        IndexDefinitionId::new(value).expect("test definition ID must be valid")
    }

    fn namespace(value: &str) -> IndexNamespace {
        IndexNamespace::new(value).expect("test namespace must be valid")
    }

    #[test]
    fn accepts_a_non_empty_definition_id() {
        let id =
            IndexDefinitionId::new("lexical.definition.v1").expect("definition ID should be valid");

        assert_eq!(id.as_str(), "lexical.definition.v1");
        assert_eq!(id.len(), "lexical.definition.v1".len());
        assert!(!id.is_empty());
    }

    #[test]
    fn rejects_an_empty_definition_id() {
        assert_eq!(
            IndexDefinitionId::new(""),
            Err(IndexDefinitionIdValidationError::Empty)
        );
    }

    #[test]
    fn rejects_control_characters() {
        let result = IndexDefinitionId::new("definition\nv1");

        assert!(matches!(
            result,
            Err(IndexDefinitionIdValidationError::ControlCharacter { .. })
        ));
    }

    #[test]
    fn permits_opaque_unicode_definition_ids() {
        let id = IndexDefinitionId::new("تعريف-١")
            .expect("Unicode is not prohibited by the Phase 1 definition-ID contract");

        assert_eq!(id.as_str(), "تعريف-١");
    }

    #[test]
    fn string_conversions_match_primary_api() {
        let id = IndexDefinitionId::try_from(String::from("definition-1"))
            .expect("definition ID should be valid");

        let reference: &str = id.as_ref();

        assert_eq!(reference, "definition-1");
        assert_eq!(id.clone().into_string(), "definition-1");
    }

    #[test]
    fn equality_and_ordering_are_value_based() {
        let first = definition_id("definition-a");
        let same = definition_id("definition-a");
        let second = definition_id("definition-b");

        assert_eq!(first, same);
        assert_ne!(first, second);
        assert!(first < second);
    }

    #[test]
    fn hash_is_consistent_for_equal_definition_ids() {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let first = definition_id("definition-a");
        let second = definition_id("definition-a");

        let mut first_hasher = DefaultHasher::new();
        first.hash(&mut first_hasher);

        let mut second_hasher = DefaultHasher::new();
        second.hash(&mut second_hasher);

        assert_eq!(first_hasher.finish(), second_hasher.finish());
    }

    #[test]
    fn display_and_debug_identify_the_definition_id() {
        let id = definition_id("definition-a");

        assert_eq!(id.to_string(), "definition-a");
        assert!(format!("{id:?}").starts_with("IndexDefinitionId("));
    }

    #[test]
    fn definition_identity_preserves_all_phase_one_components() {
        let id = definition_id("definition-a");
        let namespace = namespace("kg.relationship");
        let identity =
            IndexDefinitionIdentity::new(id.clone(), namespace.clone(), IndexFamily::Relationship);

        assert_eq!(identity.definition_id(), &id);
        assert_eq!(identity.namespace(), &namespace);
        assert_eq!(identity.family(), IndexFamily::Relationship);

        let (returned_id, returned_namespace, returned_family) = identity.into_parts();

        assert_eq!(returned_id, id);
        assert_eq!(returned_namespace, namespace);
        assert_eq!(returned_family, IndexFamily::Relationship);
    }

    #[test]
    fn definition_id_identity_is_equal_only_when_all_components_match() {
        let definition = definition_id("definition-a");
        let lexical_namespace = namespace("lexical");

        let first = IndexDefinitionIdentity::new(
            definition.clone(),
            lexical_namespace.clone(),
            IndexFamily::Inverted,
        );
        let same = IndexDefinitionIdentity::new(
            definition.clone(),
            lexical_namespace.clone(),
            IndexFamily::Inverted,
        );
        let different_definition = IndexDefinitionIdentity::new(
            definition_id("definition-b"),
            lexical_namespace.clone(),
            IndexFamily::Inverted,
        );
        let different_namespace = IndexDefinitionIdentity::new(
            definition.clone(),
            namespace("kg"),
            IndexFamily::Inverted,
        );
        let different_family =
            IndexDefinitionIdentity::new(definition, lexical_namespace, IndexFamily::Identity);

        assert_eq!(first, same);
        assert_ne!(first, different_definition);
        assert_ne!(first, different_namespace);
        assert_ne!(first, different_family);
    }

    #[test]
    fn definition_identity_keeps_namespace_and_family_distinct() {
        let definition = definition_id("definition-a");

        let inverted = IndexDefinitionIdentity::new(
            definition.clone(),
            namespace("lexical"),
            IndexFamily::Inverted,
        );
        let relationship = IndexDefinitionIdentity::new(
            definition.clone(),
            namespace("lexical"),
            IndexFamily::Relationship,
        );
        let other_namespace =
            IndexDefinitionIdentity::new(definition, namespace("kg"), IndexFamily::Inverted);

        assert_ne!(inverted, relationship);
        assert_ne!(inverted, other_namespace);
        assert_eq!(inverted.family(), IndexFamily::Inverted);
        assert_eq!(relationship.family(), IndexFamily::Relationship);
    }

    #[test]
    fn definition_identity_debug_contains_its_components() {
        let identity = IndexDefinitionIdentity::new(
            definition_id("definition-a"),
            namespace("lexical"),
            IndexFamily::Inverted,
        );

        let debug = format!("{identity:?}");

        assert!(debug.contains("IndexDefinitionIdentity"));
        assert!(debug.contains("definition-a"));
        assert!(debug.contains("lexical"));
        assert!(debug.contains("Inverted"));
    }
}
