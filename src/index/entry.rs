//! Generic logical index entries.
//!
//! `IndexEntry` represents one logical association between generic key
//! material and a source-owned [`ObjectReference`].
//!
//! This module intentionally does not implement:
//! - physical storage,
//! - database rows,
//! - lookup,
//! - index construction,
//! - provider selection,
//! - physical execution,
//! - semantic interpretation,
//! - domain-object hydration.
//!
//! The entry is a logical contract consumed by later indexing phases.

use super::key::KeyMaterial;
use super::reference::ObjectReference;

/// One generic logical indexed association between key material and a
/// source-owned target reference.
///
/// The source/domain engine owns the referenced object. `IndexEntry` owns only
/// the logical association used by Indexing.
///
/// `IndexEntry` is deliberately not a physical database row and contains no
/// provider or storage information.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct IndexEntry {
    key: KeyMaterial,
    target: ObjectReference,
}

impl IndexEntry {
    /// Creates a logical index entry.
    ///
    /// The key material is recursively validated. The target reference is
    /// already validated by [`ObjectReference`] construction.
    pub fn new(
        key: KeyMaterial,
        target: ObjectReference,
    ) -> Result<Self, IndexEntryValidationError> {
        key.validate()
            .map_err(IndexEntryValidationError::InvalidKey)?;

        Ok(Self { key, target })
    }

    /// Returns the logical key material.
    #[must_use]
    pub fn key(&self) -> &KeyMaterial {
        &self.key
    }

    /// Returns the source-owned target reference.
    #[must_use]
    pub fn target(&self) -> &ObjectReference {
        &self.target
    }

    /// Consumes the entry and returns its key material and target reference.
    #[must_use]
    pub fn into_parts(self) -> (KeyMaterial, ObjectReference) {
        (self.key, self.target)
    }

    /// Validates the logical entry.
    ///
    /// This performs only logical value validation. It does not contact a
    /// source engine, storage provider, or physical index.
    pub fn validate(&self) -> Result<(), IndexEntryValidationError> {
        self.key
            .validate()
            .map_err(IndexEntryValidationError::InvalidKey)?;

        Ok(())
    }
}

/// Validation failures for [`IndexEntry`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum IndexEntryValidationError {
    /// The key material is invalid.
    InvalidKey(super::key::KeyMaterialValidationError),
}

impl core::fmt::Display for IndexEntryValidationError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::InvalidKey(error) => {
                write!(formatter, "invalid index-entry key material: {error}")
            }
        }
    }
}

impl std::error::Error for IndexEntryValidationError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn target() -> ObjectReference {
        ObjectReference::new("quran", "verse:2:255").expect("test target reference should be valid")
    }

    #[test]
    fn constructs_a_valid_entry() {
        let key = KeyMaterial::text("ayah");
        let entry =
            IndexEntry::new(key.clone(), target()).expect("valid entry should be constructed");

        assert_eq!(entry.key(), &key);
        assert_eq!(entry.target().source(), "quran");
        assert_eq!(entry.target().object_reference(), "verse:2:255");
    }

    #[test]
    fn preserves_key_material_without_interpreting_its_domain_meaning() {
        let key = KeyMaterial::map([
            ("source", KeyMaterial::text("object-x")),
            ("predicate", KeyMaterial::text("opaque-predicate")),
            ("target", KeyMaterial::text("object-y")),
        ])
        .expect("test key material should be valid");

        let entry =
            IndexEntry::new(key.clone(), target()).expect("valid entry should be constructed");

        assert_eq!(entry.key(), &key);
    }

    #[test]
    fn accepts_structured_key_material() {
        let key = KeyMaterial::Sequence(vec![
            KeyMaterial::text("chapter"),
            KeyMaterial::Unsigned(2),
            KeyMaterial::Unsigned(255),
        ]);

        let entry = IndexEntry::new(key.clone(), target())
            .expect("structured key material should be accepted");

        assert_eq!(entry.key(), &key);
    }

    #[test]
    fn rejects_invalid_key_material() {
        let key = KeyMaterial::text("invalid\nvalue");

        let error =
            IndexEntry::new(key, target()).expect_err("invalid key material must be rejected");

        assert_eq!(
            error,
            IndexEntryValidationError::InvalidKey(
                super::super::key::KeyMaterialValidationError::TextControlCharacter { index: 7 }
            )
        );
    }

    #[test]
    fn validation_is_repeatable_and_has_no_external_dependencies() {
        let entry = IndexEntry::new(KeyMaterial::text("logical-key"), target())
            .expect("valid entry should be constructed");

        entry.validate().expect("entry should remain valid");
        entry
            .validate()
            .expect("validation should be deterministic");
    }

    #[test]
    fn equality_depends_on_key_and_target() {
        let first = IndexEntry::new(KeyMaterial::text("same-key"), target())
            .expect("valid entry should be constructed");

        let same = IndexEntry::new(KeyMaterial::text("same-key"), target())
            .expect("valid entry should be constructed");

        let different_key = IndexEntry::new(KeyMaterial::text("different-key"), target())
            .expect("valid entry should be constructed");

        let different_target = IndexEntry::new(
            KeyMaterial::text("same-key"),
            ObjectReference::new("hadith", "record:1")
                .expect("test target reference should be valid"),
        )
        .expect("valid entry should be constructed");

        assert_eq!(first, same);
        assert_ne!(first, different_key);
        assert_ne!(first, different_target);
    }

    #[test]
    fn into_parts_preserves_the_logical_association() {
        let key = KeyMaterial::text("logical-key");
        let target = target();

        let entry = IndexEntry::new(key.clone(), target.clone())
            .expect("valid entry should be constructed");

        let (returned_key, returned_target) = entry.into_parts();

        assert_eq!(returned_key, key);
        assert_eq!(returned_target, target);
    }

    #[test]
    fn source_owned_reference_remains_a_reference() {
        let entry = IndexEntry::new(KeyMaterial::text("logical-key"), target())
            .expect("valid entry should be constructed");

        // The entry exposes the reference only; there is no domain-object
        // loading or hydration operation on IndexEntry.
        assert_eq!(entry.target().source(), "quran");
    }

    #[test]
    fn entry_has_no_physical_storage_identity() {
        let entry = IndexEntry::new(KeyMaterial::text("logical-key"), target())
            .expect("valid entry should be constructed");

        assert_eq!(entry.key(), &KeyMaterial::text("logical-key"));
        assert_eq!(entry.target().object_reference(), "verse:2:255");
    }
}
