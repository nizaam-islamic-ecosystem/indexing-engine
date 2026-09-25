//! Foundational index-family definitions.
//!
//! This module defines the logical families of indexes supported by the
//! indexing subsystem. It intentionally does not implement index storage,
//! construction, query execution, retrieval algorithms, serialization, or
//! semantic relationship logic.
//!
//! Phase 1 families:
//! - [`IndexFamily::Identity`]
//! - [`IndexFamily::Inverted`]
//! - [`IndexFamily::Relationship`]
//! - [`IndexFamily::Similarity`]

use core::fmt;

/// Identifies the logical family of an index.
///
/// `IndexFamily` describes the kind of indexing/retrieval structure an index
/// belongs to. It does not identify a concrete implementation, storage
/// provider, semantic mapping, or retrieval algorithm.
///
/// In particular:
/// - `Relationship` is a relationship-oriented indexing family, not a set of
///   semantic predicates.
/// - `Similarity` does not select an embedding model, vector database, or
///   similarity algorithm.
///
/// Serialization is intentionally not implemented here because the Phase 1
/// plan leaves serialization technology deferred.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum IndexFamily {
    /// Indexes objects by their identity.
    Identity,

    /// Indexes terms or other discrete searchable values for inverted
    /// retrieval.
    Inverted,

    /// Indexes relationship-oriented data.
    ///
    /// This variant does not define or encode semantic predicates such as
    /// `CAUSES`, `PART_OF`, `BEFORE`, or `RELATED_TO`.
    Relationship,

    /// Indexes data for similarity-oriented retrieval.
    ///
    /// This variant does not select an embedding model, vector database,
    /// HNSW/IVF implementation, FAISS, or another concrete similarity
    /// algorithm.
    Similarity,
}

impl fmt::Display for IndexFamily {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::Identity => "identity",
            Self::Inverted => "inverted",
            Self::Relationship => "relationship",
            Self::Similarity => "similarity",
        };

        formatter.write_str(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_phase_one_families_exist() {
        let families = [
            IndexFamily::Identity,
            IndexFamily::Inverted,
            IndexFamily::Relationship,
            IndexFamily::Similarity,
        ];

        assert_eq!(families.len(), 4);
    }

    #[test]
    fn all_phase_one_families_are_distinct() {
        assert_ne!(IndexFamily::Identity, IndexFamily::Inverted);
        assert_ne!(IndexFamily::Identity, IndexFamily::Relationship);
        assert_ne!(IndexFamily::Identity, IndexFamily::Similarity);
        assert_ne!(IndexFamily::Inverted, IndexFamily::Relationship);
        assert_ne!(IndexFamily::Inverted, IndexFamily::Similarity);
        assert_ne!(IndexFamily::Relationship, IndexFamily::Similarity);
    }

    #[test]
    fn equality_is_reflexive_and_clone_preserves_identity() {
        let family = IndexFamily::Relationship;
        let cloned = family;

        assert_eq!(family, family);
        assert_eq!(family, cloned);
    }

    #[test]
    fn ordering_is_deterministic() {
        let mut families = [
            IndexFamily::Similarity,
            IndexFamily::Relationship,
            IndexFamily::Identity,
            IndexFamily::Inverted,
        ];

        families.sort();

        assert_eq!(
            families,
            [
                IndexFamily::Identity,
                IndexFamily::Inverted,
                IndexFamily::Relationship,
                IndexFamily::Similarity,
            ]
        );
    }

    #[test]
    fn hashing_is_consistent_for_equal_values() {
        use core::hash::{Hash, Hasher};
        use std::collections::hash_map::DefaultHasher;

        fn hash_of(value: IndexFamily) -> u64 {
            let mut hasher = DefaultHasher::new();
            value.hash(&mut hasher);
            hasher.finish()
        }

        assert_eq!(
            hash_of(IndexFamily::Identity),
            hash_of(IndexFamily::Identity)
        );
        assert_eq!(
            hash_of(IndexFamily::Inverted),
            hash_of(IndexFamily::Inverted)
        );
        assert_eq!(
            hash_of(IndexFamily::Relationship),
            hash_of(IndexFamily::Relationship)
        );
        assert_eq!(
            hash_of(IndexFamily::Similarity),
            hash_of(IndexFamily::Similarity)
        );
    }

    #[test]
    fn display_is_stable() {
        assert_eq!(IndexFamily::Identity.to_string(), "identity");
        assert_eq!(IndexFamily::Inverted.to_string(), "inverted");
        assert_eq!(IndexFamily::Relationship.to_string(), "relationship");
        assert_eq!(IndexFamily::Similarity.to_string(), "similarity");
    }

    #[test]
    fn debug_is_available_for_all_families() {
        assert_eq!(format!("{:?}", IndexFamily::Identity), "Identity");
        assert_eq!(format!("{:?}", IndexFamily::Inverted), "Inverted");
        assert_eq!(format!("{:?}", IndexFamily::Relationship), "Relationship");
        assert_eq!(format!("{:?}", IndexFamily::Similarity), "Similarity");
    }

    #[test]
    fn copy_is_available() {
        let original = IndexFamily::Similarity;
        let copied = original;

        assert_eq!(original, copied);
    }

    #[test]
    fn relationship_family_remains_a_generic_family() {
        let family = IndexFamily::Relationship;

        assert_eq!(family.to_string(), "relationship");
    }

    #[test]
    fn similarity_family_remains_a_generic_family() {
        let family = IndexFamily::Similarity;

        assert_eq!(family.to_string(), "similarity");
    }
}
