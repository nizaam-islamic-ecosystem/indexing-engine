//! Generic logical similarity-index entries.
//!
//! `SimilarityEntry` represents one logical association between a
//! source-supplied similarity representation and a source-owned
//! [`ObjectReference`].
//!
//! Phase 2 deliberately keeps the representation generic. A source or
//! another appropriate engine may supply representation/key material, while
//! Indexing owns the logical indexing contract around it.
//!
//! This module intentionally does not implement or freeze:
//! - embedding generation,
//! - embedding models,
//! - vector providers or vector databases,
//! - HNSW, IVF, FAISS, or other physical algorithms,
//! - similarity/distance algorithms,
//! - physical storage,
//! - provider selection,
//! - domain semantics,
//! - domain-object hydration.
//!
//! The similarity representation is carried as generic [`KeyMaterial`] rather
//! than as a concrete vector or embedding type.

use super::key::KeyMaterial;
use super::reference::ObjectReference;

/// One generic logical similarity-index association.
///
/// The representation is opaque logical material supplied by a source or
/// another appropriate engine. `SimilarityEntry` does not interpret that
/// material, generate it, or assign it a similarity algorithm.
///
/// The target remains a source-owned [`ObjectReference`]. Indexing therefore
/// stores a logical reference to the object rather than the object itself.
///
/// Optional metadata is also represented generically through [`KeyMaterial`].
/// No metadata schema is imposed by Phase 2.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct SimilarityEntry {
    representation: KeyMaterial,
    target: ObjectReference,
    metadata: Option<KeyMaterial>,
}

impl SimilarityEntry {
    /// Creates a similarity entry without additional logical metadata.
    ///
    /// The representation and metadata are validated structurally. No
    /// embedding generation, provider lookup, or similarity computation is
    /// performed.
    pub fn new(
        representation: KeyMaterial,
        target: ObjectReference,
    ) -> Result<Self, SimilarityEntryValidationError> {
        Self::with_metadata(representation, target, None)
    }

    /// Creates a similarity entry with optional generic logical metadata.
    ///
    /// The metadata remains opaque to Indexing. Sources may use structured
    /// [`KeyMaterial::Map`] values when they need named metadata fields.
    pub fn with_metadata(
        representation: KeyMaterial,
        target: ObjectReference,
        metadata: Option<KeyMaterial>,
    ) -> Result<Self, SimilarityEntryValidationError> {
        representation
            .validate()
            .map_err(SimilarityEntryValidationError::InvalidRepresentation)?;

        if let Some(metadata) = &metadata {
            metadata
                .validate()
                .map_err(SimilarityEntryValidationError::InvalidMetadata)?;
        }

        Ok(Self {
            representation,
            target,
            metadata,
        })
    }

    /// Returns the generic similarity representation/key material.
    #[must_use]
    pub fn representation(&self) -> &KeyMaterial {
        &self.representation
    }

    /// Returns the source-owned target reference.
    #[must_use]
    pub fn target(&self) -> &ObjectReference {
        &self.target
    }

    /// Returns optional generic logical similarity metadata.
    #[must_use]
    pub fn metadata(&self) -> Option<&KeyMaterial> {
        self.metadata.as_ref()
    }

    /// Consumes the entry and returns its logical parts.
    #[must_use]
    pub fn into_parts(self) -> (KeyMaterial, ObjectReference, Option<KeyMaterial>) {
        (self.representation, self.target, self.metadata)
    }

    /// Validates the complete logical similarity entry.
    ///
    /// Validation is purely structural and deterministic. It does not
    /// establish similarity, compute a distance, contact a provider, or
    /// resolve the target object.
    pub fn validate(&self) -> Result<(), SimilarityEntryValidationError> {
        self.representation
            .validate()
            .map_err(SimilarityEntryValidationError::InvalidRepresentation)?;

        if let Some(metadata) = &self.metadata {
            metadata
                .validate()
                .map_err(SimilarityEntryValidationError::InvalidMetadata)?;
        }

        Ok(())
    }
}

/// Validation failures for [`SimilarityEntry`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SimilarityEntryValidationError {
    /// The similarity representation/key material is invalid.
    InvalidRepresentation(super::key::KeyMaterialValidationError),

    /// The logical similarity metadata is invalid.
    InvalidMetadata(super::key::KeyMaterialValidationError),
}

impl core::fmt::Display for SimilarityEntryValidationError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::InvalidRepresentation(error) => {
                write!(formatter, "invalid similarity representation: {error}")
            }
            Self::InvalidMetadata(error) => {
                write!(formatter, "invalid similarity metadata: {error}")
            }
        }
    }
}

impl std::error::Error for SimilarityEntryValidationError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn target() -> ObjectReference {
        ObjectReference::new("quran", "verse:2:255").expect("test target reference should be valid")
    }

    #[test]
    fn constructs_a_similarity_entry_from_generic_representation() {
        let representation = KeyMaterial::Sequence(vec![
            KeyMaterial::Integer(10),
            KeyMaterial::Integer(20),
            KeyMaterial::Integer(30),
        ]);

        let entry = SimilarityEntry::new(representation.clone(), target())
            .expect("valid similarity entry should be constructed");

        assert_eq!(entry.representation(), &representation);
        assert_eq!(entry.target().source(), "quran");
        assert_eq!(entry.metadata(), None);
    }

    #[test]
    fn accepts_opaque_representation_without_interpreting_it() {
        let representation = KeyMaterial::Bytes(vec![0x10, 0x20, 0x30, 0x40]);

        let entry = SimilarityEntry::new(representation.clone(), target())
            .expect("opaque representation should be accepted");

        assert_eq!(entry.representation(), &representation);
    }

    #[test]
    fn accepts_structured_representation() {
        let representation = KeyMaterial::map([
            ("feature-a", KeyMaterial::Unsigned(10)),
            ("feature-b", KeyMaterial::Unsigned(20)),
        ])
        .expect("structured representation should be valid");

        let entry = SimilarityEntry::new(representation.clone(), target())
            .expect("structured representation should be accepted");

        assert_eq!(entry.representation(), &representation);
    }

    #[test]
    fn accepts_generic_logical_metadata() {
        let representation = KeyMaterial::bytes(vec![1, 2, 3]);
        let metadata = KeyMaterial::map([
            ("source", KeyMaterial::text("semantic-engine")),
            ("revision", KeyMaterial::Unsigned(7)),
        ])
        .expect("metadata should be valid");

        let entry =
            SimilarityEntry::with_metadata(representation, target(), Some(metadata.clone()))
                .expect("valid metadata should be accepted");

        assert_eq!(entry.metadata(), Some(&metadata));
    }

    #[test]
    fn rejects_invalid_representation() {
        let representation = KeyMaterial::text("invalid\nrepresentation");

        let error = SimilarityEntry::new(representation, target())
            .expect_err("invalid representation must be rejected");

        assert_eq!(
            error,
            SimilarityEntryValidationError::InvalidRepresentation(
                super::super::key::KeyMaterialValidationError::TextControlCharacter { index: 7 }
            )
        );
    }

    #[test]
    fn rejects_invalid_metadata() {
        let metadata = KeyMaterial::text("invalid\nmetadata");

        let error = SimilarityEntry::with_metadata(
            KeyMaterial::bytes(vec![1, 2, 3]),
            target(),
            Some(metadata),
        )
        .expect_err("invalid metadata must be rejected");

        assert_eq!(
            error,
            SimilarityEntryValidationError::InvalidMetadata(
                super::super::key::KeyMaterialValidationError::TextControlCharacter { index: 7 }
            )
        );
    }

    #[test]
    fn validation_is_repeatable_and_has_no_external_dependencies() {
        let entry = SimilarityEntry::new(KeyMaterial::text("logical-representation"), target())
            .expect("valid similarity entry should be constructed");

        entry.validate().expect("entry should remain valid");
        entry
            .validate()
            .expect("validation should be deterministic");
    }

    #[test]
    fn equality_depends_on_representation_target_and_metadata() {
        let first = SimilarityEntry::with_metadata(
            KeyMaterial::text("same"),
            target(),
            Some(KeyMaterial::Unsigned(1)),
        )
        .expect("valid entry should be constructed");

        let same = SimilarityEntry::with_metadata(
            KeyMaterial::text("same"),
            target(),
            Some(KeyMaterial::Unsigned(1)),
        )
        .expect("valid entry should be constructed");

        let different_representation = SimilarityEntry::with_metadata(
            KeyMaterial::text("different"),
            target(),
            Some(KeyMaterial::Unsigned(1)),
        )
        .expect("valid entry should be constructed");

        let different_target = SimilarityEntry::with_metadata(
            KeyMaterial::text("same"),
            ObjectReference::new("hadith", "record:1")
                .expect("test target reference should be valid"),
            Some(KeyMaterial::Unsigned(1)),
        )
        .expect("valid entry should be constructed");

        let different_metadata = SimilarityEntry::with_metadata(
            KeyMaterial::text("same"),
            target(),
            Some(KeyMaterial::Unsigned(2)),
        )
        .expect("valid entry should be constructed");

        assert_eq!(first, same);
        assert_ne!(first, different_representation);
        assert_ne!(first, different_target);
        assert_ne!(first, different_metadata);
    }

    #[test]
    fn into_parts_preserves_all_logical_components() {
        let representation = KeyMaterial::text("logical-representation");
        let target = target();
        let metadata = Some(KeyMaterial::Unsigned(42));

        let entry = SimilarityEntry::with_metadata(
            representation.clone(),
            target.clone(),
            metadata.clone(),
        )
        .expect("valid similarity entry should be constructed");

        let (returned_representation, returned_target, returned_metadata) = entry.into_parts();

        assert_eq!(returned_representation, representation);
        assert_eq!(returned_target, target);
        assert_eq!(returned_metadata, metadata);
    }

    #[test]
    fn remains_reference_oriented() {
        let entry =
            SimilarityEntry::new(KeyMaterial::bytes(vec![0xde, 0xad, 0xbe, 0xef]), target())
                .expect("valid similarity entry should be constructed");

        // SimilarityEntry exposes only a source-owned reference. It has no
        // domain-object loading or hydration operation.
        assert_eq!(entry.target().source(), "quran");
        assert_eq!(entry.target().object_reference(), "verse:2:255");
    }

    #[test]
    fn does_not_define_similarity_semantics() {
        let first = SimilarityEntry::new(KeyMaterial::bytes(vec![1, 2, 3]), target())
            .expect("valid similarity entry should be constructed");

        let second = SimilarityEntry::new(
            KeyMaterial::bytes(vec![1, 2, 3]),
            ObjectReference::new("quran", "verse:2:256")
                .expect("test target reference should be valid"),
        )
        .expect("valid similarity entry should be constructed");

        // Equality compares logical representation and target. The entry
        // does not calculate distance, similarity, ranking, or relevance.
        assert_ne!(first, second);
    }
}
