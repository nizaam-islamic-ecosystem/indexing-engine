//! Stable logical identity for a concrete Indexing index.
//!
//! Phase 1 establishes the binary width and identity semantics of `IndexId`.
//! The 512-bit value is intentionally treated as opaque bytes. This module
//! does not select or implement the algorithm that generates those bytes.
//!
//! The distinction is important:
//!
//! - `IndexId` owns the identity value.
//! - Any future generation/hash mechanism produces that value.
//! - Rust's `Hash` implementation only makes `IndexId` usable in hashed
//!   collections; it is not the index-generation algorithm.
//!
//! The scope fixes the identity width at 512 bits / 64 bytes while deferring
//! the generation algorithm and human-readable encoding to a later phase.

use core::fmt;

/// Fixed width of an [`IndexId`] in bytes.
pub const INDEX_ID_BYTE_LEN: usize = 64;

/// Fixed width of an [`IndexId`] in bits.
pub const INDEX_ID_BIT_LEN: usize = INDEX_ID_BYTE_LEN * 8;

/// Stable identity of one concrete logical Indexing index.
///
/// The underlying value is exactly 64 bytes (512 bits). The bytes have no
/// Indexing-defined semantic structure in Phase 1. In particular, this type
/// does not know whether the value came from SHA-512, BLAKE3, or another
/// future-approved generation mechanism.
///
/// Human-readable encoding is intentionally not part of this phase.
#[derive(Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct IndexId([u8; INDEX_ID_BYTE_LEN]);

impl IndexId {
    /// Returns the fixed number of bytes in an `IndexId`.
    #[must_use]
    pub const fn byte_len() -> usize {
        INDEX_ID_BYTE_LEN
    }

    /// Returns the fixed number of bits in an `IndexId`.
    #[must_use]
    pub const fn bit_len() -> usize {
        INDEX_ID_BIT_LEN
    }

    /// Constructs an `IndexId` from its complete fixed-width binary value.
    ///
    /// The caller supplies the already-established identity bytes. No hash or
    /// generation algorithm is selected or executed here.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; INDEX_ID_BYTE_LEN]) -> Self {
        Self(bytes)
    }

    /// Returns the exact underlying 64-byte identity value.
    ///
    /// This accessor does not expose any semantic interpretation of the
    /// identity. It only permits efficient comparison, storage, or transport
    /// by later layers.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; INDEX_ID_BYTE_LEN] {
        &self.0
    }

    /// Consumes the identity and returns its exact underlying bytes.
    #[must_use]
    pub const fn into_bytes(self) -> [u8; INDEX_ID_BYTE_LEN] {
        self.0
    }
}

impl AsRef<[u8; INDEX_ID_BYTE_LEN]> for IndexId {
    fn as_ref(&self) -> &[u8; INDEX_ID_BYTE_LEN] {
        self.as_bytes()
    }
}

impl AsRef<[u8]> for IndexId {
    fn as_ref(&self) -> &[u8] {
        self.as_bytes()
    }
}

impl fmt::Debug for IndexId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_tuple("IndexId").field(&self.0).finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    fn sample_bytes(seed: u8) -> [u8; INDEX_ID_BYTE_LEN] {
        let mut bytes = [0_u8; INDEX_ID_BYTE_LEN];

        for (index, byte) in bytes.iter_mut().enumerate() {
            *byte = seed.wrapping_add(index as u8);
        }

        bytes
    }

    #[test]
    fn constants_define_the_required_512_bit_width() {
        assert_eq!(INDEX_ID_BYTE_LEN, 64);
        assert_eq!(INDEX_ID_BIT_LEN, 512);
        assert_eq!(IndexId::byte_len(), 64);
        assert_eq!(IndexId::bit_len(), 512);
    }

    #[test]
    fn from_bytes_preserves_the_complete_identity_value() {
        let bytes = sample_bytes(7);
        let id = IndexId::from_bytes(bytes);

        assert_eq!(id.as_bytes(), &bytes);
        assert_eq!(id.into_bytes(), bytes);
    }

    #[test]
    fn arbitrary_64_byte_values_are_valid_identity_values() {
        let mut bytes = [0_u8; INDEX_ID_BYTE_LEN];

        for (index, byte) in bytes.iter_mut().enumerate() {
            *byte = if index % 2 == 0 { 0x00 } else { 0xFF };
        }

        let id = IndexId::from_bytes(bytes);

        assert_eq!(id.as_bytes(), &bytes);
    }

    #[test]
    fn identical_values_are_equal() {
        let bytes = sample_bytes(11);

        let first = IndexId::from_bytes(bytes);
        let second = IndexId::from_bytes(bytes);

        assert_eq!(first, second);
    }

    #[test]
    fn different_values_are_not_equal() {
        let first = IndexId::from_bytes(sample_bytes(13));
        let second = IndexId::from_bytes(sample_bytes(14));

        assert_ne!(first, second);
    }

    #[test]
    fn ordering_is_determined_by_the_binary_identity_value() {
        let low = IndexId::from_bytes([0x00; INDEX_ID_BYTE_LEN]);
        let high = IndexId::from_bytes([0xFF; INDEX_ID_BYTE_LEN]);

        assert!(low < high);
        assert!(high > low);
    }

    #[test]
    fn hash_is_consistent_for_equal_identity_values() {
        let bytes = sample_bytes(21);
        let first = IndexId::from_bytes(bytes);
        let second = IndexId::from_bytes(bytes);

        let mut first_hasher = DefaultHasher::new();
        first.hash(&mut first_hasher);

        let mut second_hasher = DefaultHasher::new();
        second.hash(&mut second_hasher);

        assert_eq!(first_hasher.finish(), second_hasher.finish());
    }

    #[test]
    fn hash_is_compatible_with_binary_identity_difference() {
        let first = IndexId::from_bytes([0x00; INDEX_ID_BYTE_LEN]);
        let second = IndexId::from_bytes([0xFF; INDEX_ID_BYTE_LEN]);

        let mut first_hasher = DefaultHasher::new();
        first.hash(&mut first_hasher);

        let mut second_hasher = DefaultHasher::new();
        second.hash(&mut second_hasher);

        assert_ne!(first_hasher.finish(), second_hasher.finish());
    }

    #[test]
    fn clone_and_copy_preserve_identity() {
        let id = IndexId::from_bytes(sample_bytes(29));

        let cloned = id;
        let copied = IndexId::from_bytes(id.into_bytes());

        assert_eq!(id, cloned);
        assert_eq!(id, copied);
    }

    #[test]
    fn as_ref_exposes_the_same_binary_identity() {
        let bytes = sample_bytes(37);
        let id = IndexId::from_bytes(bytes);

        let fixed_width: &[u8; INDEX_ID_BYTE_LEN] = id.as_ref();
        let slice: &[u8] = id.as_ref();

        assert_eq!(fixed_width, &bytes);
        assert_eq!(slice, bytes.as_slice());
    }

    #[test]
    fn debug_output_identifies_the_type_without_defining_a_wire_encoding() {
        let id = IndexId::from_bytes([0xAB; INDEX_ID_BYTE_LEN]);
        let debug = format!("{id:?}");

        assert!(debug.starts_with("IndexId("));
        assert!(!debug.is_empty());
    }
}
