//! Logical namespace identity and in-memory namespace registration.
//!
//! Phase 1 keeps namespaces at the logical Indexing layer. A namespace is an
//! opaque caller-supplied identifier that separates logical indexing spaces.
//! It is not a domain model, physical partition, database, shard, or storage
//! provider identifier.
//!
//! The Phase 1 namespace grammar is intentionally strict:
//!
//! - ASCII lowercase letters (`a-z`)
//! - ASCII digits (`0-9`)
//! - separators (`.`, `-`, `_`)
//! - non-empty
//! - maximum length of 128 bytes
//! - starts and ends with an alphanumeric character
//! - no whitespace or control characters
//! - no Unicode characters
//! - no consecutive separators
//!
//! The registry in this module is intentionally in-memory. It provides logical
//! registration, duplicate detection, unregistration, membership checks, and
//! deterministic snapshots. It does not provide persistence, distributed
//! coordination, physical partitioning, or storage management.

use core::fmt;
use std::collections::BTreeSet;
use std::error::Error;

const MAX_NAMESPACE_LEN: usize = 128;

/// Maximum number of bytes allowed in an [`IndexNamespace`].
pub const MAX_NAMESPACE_BYTES: usize = MAX_NAMESPACE_LEN;

/// A validated logical Indexing namespace.
///
/// Namespace values are intentionally opaque to Indexing. Names such as
/// `quran`, `arabic`, or `kg` may be supplied by callers, but this type does
/// not assign domain meaning to them.
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct IndexNamespace(String);

impl IndexNamespace {
    /// Constructs a namespace from a string after applying the Phase 1 grammar.
    pub fn new(value: impl Into<String>) -> Result<Self, NamespaceValidationError> {
        let value = value.into();
        validate_namespace(&value)?;
        Ok(Self(value))
    }

    /// Returns the namespace as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consumes the namespace and returns its owned string.
    #[must_use]
    pub fn into_string(self) -> String {
        self.0
    }

    /// Returns the number of bytes in the validated namespace.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns whether the namespace contains no bytes.
    ///
    /// This is unreachable for successfully constructed values, but the
    /// accessor is useful for generic collection-style handling.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl TryFrom<&str> for IndexNamespace {
    type Error = NamespaceValidationError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl TryFrom<String> for IndexNamespace {
    type Error = NamespaceValidationError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl AsRef<str> for IndexNamespace {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Debug for IndexNamespace {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("IndexNamespace")
            .field(&self.0)
            .finish()
    }
}

impl fmt::Display for IndexNamespace {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// Validation failures for the Phase 1 namespace grammar.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NamespaceValidationError {
    /// The namespace contains no characters.
    Empty,

    /// The namespace exceeds the 128-byte limit.
    TooLong {
        /// The supplied byte length.
        length: usize,
        /// The maximum permitted byte length.
        maximum: usize,
    },

    /// The first character is not an ASCII lowercase letter or digit.
    InvalidStart {
        /// The invalid first byte.
        byte: u8,
    },

    /// The final character is not an ASCII lowercase letter or digit.
    InvalidEnd {
        /// The invalid final byte.
        byte: u8,
    },

    /// A non-ASCII byte was supplied.
    NonAscii {
        /// The invalid byte.
        byte: u8,
        /// Byte position within the input.
        index: usize,
    },

    /// A character is outside the permitted namespace grammar.
    InvalidCharacter {
        /// The invalid byte.
        byte: u8,
        /// Byte position within the input.
        index: usize,
    },

    /// Two namespace separators occur consecutively.
    ConsecutiveSeparators {
        /// Byte position of the second separator.
        index: usize,
    },
}

impl fmt::Display for NamespaceValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("namespace must not be empty"),
            Self::TooLong { length, maximum } => {
                write!(
                    formatter,
                    "namespace length {length} exceeds maximum {maximum} bytes"
                )
            }
            Self::InvalidStart { byte } => {
                write!(formatter, "namespace starts with invalid byte 0x{byte:02X}")
            }
            Self::InvalidEnd { byte } => {
                write!(formatter, "namespace ends with invalid byte 0x{byte:02X}")
            }
            Self::NonAscii { byte, index } => {
                write!(
                    formatter,
                    "namespace contains non-ASCII byte 0x{byte:02X} at index {index}"
                )
            }
            Self::InvalidCharacter { byte, index } => {
                write!(
                    formatter,
                    "namespace contains invalid byte 0x{byte:02X} at index {index}"
                )
            }
            Self::ConsecutiveSeparators { index } => {
                write!(
                    formatter,
                    "namespace contains consecutive separators at index {index}"
                )
            }
        }
    }
}

impl Error for NamespaceValidationError {}

/// Errors produced by [`NamespaceRegistry`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NamespaceRegistryError {
    /// The namespace was already registered.
    AlreadyRegistered(IndexNamespace),

    /// The namespace is not currently registered.
    NotRegistered(IndexNamespace),
}

impl fmt::Display for NamespaceRegistryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AlreadyRegistered(namespace) => {
                write!(formatter, "namespace `{namespace}` is already registered")
            }
            Self::NotRegistered(namespace) => {
                write!(formatter, "namespace `{namespace}` is not registered")
            }
        }
    }
}

impl Error for NamespaceRegistryError {}

/// In-memory registry of logical Indexing namespaces.
///
/// The registry intentionally models logical registration only. It does not
/// imply a database catalog, persistence layer, distributed registry, physical
/// partition, shard, or storage provider.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct NamespaceRegistry {
    namespaces: BTreeSet<IndexNamespace>,
}

impl NamespaceRegistry {
    /// Creates an empty namespace registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a logical namespace.
    ///
    /// Registration is deterministic: attempting to register a namespace that
    /// is already present returns [`NamespaceRegistryError::AlreadyRegistered`].
    pub fn register(&mut self, namespace: IndexNamespace) -> Result<(), NamespaceRegistryError> {
        if !self.namespaces.insert(namespace.clone()) {
            return Err(NamespaceRegistryError::AlreadyRegistered(namespace));
        }

        Ok(())
    }

    /// Returns whether the namespace is currently registered.
    #[must_use]
    pub fn contains(&self, namespace: &IndexNamespace) -> bool {
        self.namespaces.contains(namespace)
    }

    /// Removes a registered logical namespace.
    ///
    /// This operates only on the in-memory logical registry. It does not imply
    /// physical deletion of an index or storage resource.
    pub fn unregister(
        &mut self,
        namespace: &IndexNamespace,
    ) -> Result<IndexNamespace, NamespaceRegistryError> {
        if !self.namespaces.remove(namespace) {
            return Err(NamespaceRegistryError::NotRegistered(namespace.clone()));
        }

        Ok(namespace.clone())
    }

    /// Returns a deterministic snapshot of all currently registered namespaces.
    #[must_use]
    pub fn snapshot(&self) -> Vec<IndexNamespace> {
        self.namespaces.iter().cloned().collect()
    }

    /// Returns the number of registered namespaces.
    #[must_use]
    pub fn len(&self) -> usize {
        self.namespaces.len()
    }

    /// Returns whether no namespaces are registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.namespaces.is_empty()
    }
}

fn validate_namespace(value: &str) -> Result<(), NamespaceValidationError> {
    if value.is_empty() {
        return Err(NamespaceValidationError::Empty);
    }

    let length = value.len();

    if length > MAX_NAMESPACE_LEN {
        return Err(NamespaceValidationError::TooLong {
            length,
            maximum: MAX_NAMESPACE_LEN,
        });
    }

    let bytes = value.as_bytes();

    for (index, byte) in bytes.iter().copied().enumerate() {
        if byte >= 0x80 {
            return Err(NamespaceValidationError::NonAscii { byte, index });
        }
    }

    if !is_ascii_alphanumeric(bytes[0]) {
        return Err(NamespaceValidationError::InvalidStart { byte: bytes[0] });
    }

    if !is_ascii_alphanumeric(bytes[bytes.len() - 1]) {
        return Err(NamespaceValidationError::InvalidEnd {
            byte: bytes[bytes.len() - 1],
        });
    }

    let mut previous_was_separator = false;

    for (index, byte) in bytes.iter().copied().enumerate() {
        if is_separator(byte) {
            if previous_was_separator {
                return Err(NamespaceValidationError::ConsecutiveSeparators { index });
            }

            previous_was_separator = true;
            continue;
        }

        if !is_ascii_alphanumeric(byte) {
            return Err(NamespaceValidationError::InvalidCharacter { byte, index });
        }

        previous_was_separator = false;
    }

    Ok(())
}

fn is_ascii_alphanumeric(byte: u8) -> bool {
    byte.is_ascii_lowercase() || byte.is_ascii_digit()
}

fn is_separator(byte: u8) -> bool {
    matches!(byte, b'.' | b'-' | b'_')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_namespace(value: &str) -> IndexNamespace {
        IndexNamespace::new(value).expect("test namespace must be valid")
    }

    #[test]
    fn accepts_valid_namespace_values() {
        for value in [
            "quran",
            "arabic",
            "kg",
            "lexical",
            "quran.text",
            "quran-word",
            "source_01",
            "kg.relationship",
        ] {
            assert!(
                IndexNamespace::new(value).is_ok(),
                "expected `{value}` to be valid"
            );
        }
    }

    #[test]
    fn rejects_empty_namespace() {
        assert_eq!(
            IndexNamespace::new(""),
            Err(NamespaceValidationError::Empty)
        );
    }

    #[test]
    fn rejects_uppercase_letters() {
        assert!(matches!(
            IndexNamespace::new("Quran"),
            Err(NamespaceValidationError::InvalidStart { .. })
        ));

        assert!(matches!(
            IndexNamespace::new("quranText"),
            Err(NamespaceValidationError::InvalidCharacter { .. })
        ));
    }

    #[test]
    fn rejects_whitespace() {
        assert!(matches!(
            IndexNamespace::new("quran text"),
            Err(NamespaceValidationError::InvalidCharacter { .. })
        ));
    }

    #[test]
    fn rejects_control_characters() {
        assert!(matches!(
            IndexNamespace::new("quran\ntext"),
            Err(NamespaceValidationError::InvalidCharacter { .. })
        ));
    }

    #[test]
    fn rejects_unicode() {
        assert!(matches!(
            IndexNamespace::new("قuran"),
            Err(NamespaceValidationError::NonAscii { .. })
        ));
    }

    #[test]
    fn rejects_leading_and_trailing_separators() {
        for value in [".quran", "-quran", "_quran", "quran.", "quran-", "quran_"] {
            assert!(matches!(
                IndexNamespace::new(value),
                Err(NamespaceValidationError::InvalidStart { .. }
                    | NamespaceValidationError::InvalidEnd { .. })
            ));
        }
    }

    #[test]
    fn rejects_consecutive_separators_of_any_kind() {
        for value in [
            "quran..text",
            "quran--text",
            "quran__text",
            "quran.-text",
            "quran-.text",
            "quran._text",
            "quran_.text",
        ] {
            assert!(matches!(
                IndexNamespace::new(value),
                Err(NamespaceValidationError::ConsecutiveSeparators { .. })
            ));
        }
    }

    #[test]
    fn rejects_unsupported_characters() {
        for value in ["quran/text", "quran:text", "quran@text", "quran$text"] {
            assert!(matches!(
                IndexNamespace::new(value),
                Err(NamespaceValidationError::InvalidCharacter { .. })
            ));
        }
    }

    #[test]
    fn enforces_the_128_byte_limit() {
        let valid = "a".repeat(MAX_NAMESPACE_BYTES);
        let invalid = "a".repeat(MAX_NAMESPACE_BYTES + 1);

        assert!(IndexNamespace::new(valid).is_ok());
        assert_eq!(
            IndexNamespace::new(invalid),
            Err(NamespaceValidationError::TooLong {
                length: MAX_NAMESPACE_BYTES + 1,
                maximum: MAX_NAMESPACE_BYTES,
            })
        );
    }

    #[test]
    fn exposes_stable_string_accessors() {
        let namespace = valid_namespace("kg.relationship");

        assert_eq!(namespace.as_str(), "kg.relationship");
        assert_eq!(namespace.len(), "kg.relationship".len());
        assert!(!namespace.is_empty());
        assert_eq!(namespace.clone().into_string(), "kg.relationship");
    }

    #[test]
    fn display_and_debug_identify_the_namespace_value() {
        let namespace = valid_namespace("kg.relationship");

        assert_eq!(namespace.to_string(), "kg.relationship");
        assert!(format!("{namespace:?}").starts_with("IndexNamespace("));
    }

    #[test]
    fn equality_and_ordering_are_value_based() {
        let first = valid_namespace("a.namespace");
        let same = valid_namespace("a.namespace");
        let second = valid_namespace("b.namespace");

        assert_eq!(first, same);
        assert_ne!(first, second);
        assert!(first < second);
    }

    #[test]
    fn hash_is_consistent_for_equal_namespaces() {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let first = valid_namespace("kg");
        let second = valid_namespace("kg");

        let mut first_hasher = DefaultHasher::new();
        first.hash(&mut first_hasher);

        let mut second_hasher = DefaultHasher::new();
        second.hash(&mut second_hasher);

        assert_eq!(first_hasher.finish(), second_hasher.finish());
    }

    #[test]
    fn try_from_and_as_ref_match_the_primary_api() {
        let namespace = IndexNamespace::try_from("lexical").expect("namespace must be valid");
        let namespace_ref: &str = namespace.as_ref();

        assert_eq!(namespace_ref, "lexical");
    }

    #[test]
    fn registry_starts_empty() {
        let registry = NamespaceRegistry::new();

        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);
        assert!(registry.snapshot().is_empty());
    }

    #[test]
    fn registry_registers_and_contains_namespace() {
        let mut registry = NamespaceRegistry::new();
        let namespace = valid_namespace("kg");

        registry
            .register(namespace.clone())
            .expect("first registration should succeed");

        assert!(registry.contains(&namespace));
        assert_eq!(registry.len(), 1);
        assert_eq!(registry.snapshot(), vec![namespace]);
    }

    #[test]
    fn registry_rejects_duplicate_registration_deterministically() {
        let mut registry = NamespaceRegistry::new();
        let namespace = valid_namespace("kg");

        registry
            .register(namespace.clone())
            .expect("first registration should succeed");

        assert_eq!(
            registry.register(namespace.clone()),
            Err(NamespaceRegistryError::AlreadyRegistered(namespace))
        );
        assert_eq!(registry.len(), 1);
    }

    #[test]
    fn registry_unregisters_an_existing_namespace() {
        let mut registry = NamespaceRegistry::new();
        let namespace = valid_namespace("kg");

        registry
            .register(namespace.clone())
            .expect("registration should succeed");

        assert_eq!(registry.unregister(&namespace), Ok(namespace.clone()));
        assert!(!registry.contains(&namespace));
        assert!(registry.is_empty());
    }

    #[test]
    fn registry_rejects_unregistering_a_missing_namespace() {
        let mut registry = NamespaceRegistry::new();
        let namespace = valid_namespace("kg");

        assert_eq!(
            registry.unregister(&namespace),
            Err(NamespaceRegistryError::NotRegistered(namespace))
        );
    }

    #[test]
    fn registry_snapshot_is_deterministic() {
        let mut registry = NamespaceRegistry::new();

        let namespaces = [
            valid_namespace("zeta"),
            valid_namespace("alpha"),
            valid_namespace("middle"),
        ];

        for namespace in namespaces {
            registry
                .register(namespace)
                .expect("registration should succeed");
        }

        assert_eq!(
            registry.snapshot(),
            vec![
                valid_namespace("alpha"),
                valid_namespace("middle"),
                valid_namespace("zeta"),
            ]
        );
    }

    #[test]
    fn registry_supports_multiple_independent_logical_spaces() {
        let mut registry = NamespaceRegistry::new();

        let first = valid_namespace("quran.text");
        let second = valid_namespace("kg.relationship");
        let third = valid_namespace("source_01");

        for namespace in [first.clone(), second.clone(), third.clone()] {
            registry
                .register(namespace)
                .expect("registration should succeed");
        }

        assert_eq!(registry.len(), 3);
        assert!(registry.contains(&first));
        assert!(registry.contains(&second));
        assert!(registry.contains(&third));
    }

    #[test]
    fn cloned_registry_is_independent() {
        let mut registry = NamespaceRegistry::new();
        let namespace = valid_namespace("kg");

        registry
            .register(namespace.clone())
            .expect("registration should succeed");

        let mut cloned = registry.clone();

        cloned
            .unregister(&namespace)
            .expect("unregister should succeed");

        assert!(registry.contains(&namespace));
        assert!(!cloned.contains(&namespace));
    }

    #[test]
    fn namespace_registry_does_not_assign_domain_meaning() {
        let mut registry = NamespaceRegistry::new();
        let namespaces = [
            valid_namespace("quran"),
            valid_namespace("arabic"),
            valid_namespace("kg"),
        ];

        for namespace in namespaces {
            registry
                .register(namespace)
                .expect("registration should succeed");
        }

        assert_eq!(registry.len(), 3);
        assert_eq!(
            registry
                .snapshot()
                .iter()
                .map(IndexNamespace::as_str)
                .collect::<Vec<_>>(),
            vec!["arabic", "kg", "quran"]
        );
    }
}
