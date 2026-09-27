//! Indexing-owned failure classification for Phase 5 recovery.
//!
//! This module defines the logical failure taxonomy owned by the Indexing
//! Engine. It deliberately does not execute recovery, schedule retries, or
//! replace Core's retry machinery.
//!
//! The boundary is:
//!
//! ```text
//! Indexing failure
//!       |
//!       v
//! FailureClass
//!       |
//!       +----> Core Retryability adapter
//!       |
//!       v
//! RecoveryExecutor / Core retry infrastructure
//! ```
//!
//! The Indexing failure class remains distinct from Core retry policy. A
//! failure may be classified here while the actual retry decision remains
//! subject to Core's retry policy, attempt accounting, cancellation, deadline,
//! resource, idempotency, and other safety gates.

use core::fmt;
use nizaam_core::status::Retryability;

/// Indexing-specific operational failure classes.
///
/// These categories describe the logical condition observed by the Indexing
/// Engine. They are intentionally separate from lifecycle states and from
/// Core's retry policy.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum FailureClass {
    /// The logical index state violates an Indexing contract.
    InvalidIndex,

    /// The index is valid but no longer corresponds to the required source or
    /// active-version lineage.
    StaleIndex,

    /// The index cannot currently serve its required operation.
    UnavailableIndex,

    /// The index state or contents are believed to be damaged or internally
    /// inconsistent.
    CorruptIndex,

    /// The source engine or source-owned data could not provide the required
    /// information.
    SourceDataFailure,

    /// Indexing could not proceed because a bounded operational resource was
    /// exhausted.
    ResourceExhaustion,

    /// A provider or provider-facing operation failed.
    ProviderFailure,

    /// A logical query or retrieval operation failed after entering the query
    /// boundary.
    QueryFailure,
}

impl FailureClass {
    /// Returns all Indexing failure classes in their canonical declaration
    /// order.
    pub const ALL: [Self; 8] = [
        Self::InvalidIndex,
        Self::StaleIndex,
        Self::UnavailableIndex,
        Self::CorruptIndex,
        Self::SourceDataFailure,
        Self::ResourceExhaustion,
        Self::ProviderFailure,
        Self::QueryFailure,
    ];

    /// Returns the stable logical name of the failure class.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidIndex => "invalid_index",
            Self::StaleIndex => "stale_index",
            Self::UnavailableIndex => "unavailable_index",
            Self::CorruptIndex => "corrupt_index",
            Self::SourceDataFailure => "source_data_failure",
            Self::ResourceExhaustion => "resource_exhaustion",
            Self::ProviderFailure => "provider_failure",
            Self::QueryFailure => "query_failure",
        }
    }

    /// Returns the conservative Core retryability classification for this
    /// Indexing failure class.
    ///
    /// This is only an adapter into Core's shared retryability primitive. It
    /// does not authorize another attempt. Core retry policy and its safety
    /// gates remain authoritative for actual retry admission.
    pub const fn retryability(self) -> Retryability {
        match self {
            // These failures normally require a logical correction, rebuild,
            // source intervention, or isolation rather than repeating the
            // same operation unchanged.
            Self::InvalidIndex
            | Self::StaleIndex
            | Self::CorruptIndex
            | Self::SourceDataFailure
            | Self::ResourceExhaustion => Retryability::NonRetryable,

            // These failures may be transient. Core still decides whether a
            // concrete retry is admitted.
            Self::UnavailableIndex | Self::ProviderFailure | Self::QueryFailure => {
                Retryability::Retryable
            }
        }
    }

    /// Returns whether the class carries a retryable hint into Core.
    ///
    /// `true` is only a classification hint. It must not be interpreted as a
    /// completed retry decision.
    pub const fn may_retry(self) -> bool {
        matches!(self.retryability(), Retryability::Retryable)
    }
}

impl fmt::Display for FailureClass {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// A classified Indexing failure.
///
/// The payload intentionally contains only the Indexing-owned class. Detailed
/// technical error occurrences, operation context, and Core error events stay
/// in the Core error boundary rather than being duplicated here.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ClassifiedFailure {
    class: FailureClass,
}

impl ClassifiedFailure {
    /// Creates a classified failure from an Indexing failure class.
    pub const fn new(class: FailureClass) -> Self {
        Self { class }
    }

    /// Returns the Indexing failure class.
    pub const fn class(self) -> FailureClass {
        self.class
    }

    /// Returns the conservative Core retryability hint for this failure.
    pub const fn retryability(self) -> Retryability {
        self.class.retryability()
    }

    /// Returns whether the failure carries a retryable hint into Core.
    pub const fn may_retry(self) -> bool {
        self.class.may_retry()
    }
}

impl From<FailureClass> for ClassifiedFailure {
    fn from(class: FailureClass) -> Self {
        Self::new(class)
    }
}

impl fmt::Display for ClassifiedFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.class.fmt(formatter)
    }
}

/// Classifies an Indexing-owned failure without executing recovery.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FailureClassifier;

impl FailureClassifier {
    /// Creates the stateless classifier.
    pub const fn new() -> Self {
        Self
    }

    /// Classifies the supplied Indexing failure class.
    pub const fn classify(&self, class: FailureClass) -> ClassifiedFailure {
        ClassifiedFailure::new(class)
    }

    /// Adapts an Indexing failure class directly to Core retryability.
    pub const fn retryability(&self, class: FailureClass) -> Retryability {
        class.retryability()
    }
}

/// Classifies a failure using the module's stateless classifier.
pub const fn classify(class: FailureClass) -> ClassifiedFailure {
    ClassifiedFailure::new(class)
}

/// Returns the Core retryability hint for an Indexing failure class.
pub const fn retryability(class: FailureClass) -> Retryability {
    class.retryability()
}

#[cfg(test)]
mod tests {
    use super::{ClassifiedFailure, FailureClass, FailureClassifier, classify, retryability};
    use nizaam_core::status::Retryability;

    #[test]
    fn all_required_failure_classes_are_distinct() {
        assert_eq!(FailureClass::ALL.len(), 8);

        for (index, class) in FailureClass::ALL.iter().enumerate() {
            assert!(
                FailureClass::ALL[..index]
                    .iter()
                    .all(|previous| previous != class)
            );
        }

        assert_eq!(FailureClass::InvalidIndex.as_str(), "invalid_index");
        assert_eq!(FailureClass::StaleIndex.as_str(), "stale_index");
        assert_eq!(FailureClass::UnavailableIndex.as_str(), "unavailable_index");
        assert_eq!(FailureClass::CorruptIndex.as_str(), "corrupt_index");
        assert_eq!(
            FailureClass::SourceDataFailure.as_str(),
            "source_data_failure"
        );
        assert_eq!(
            FailureClass::ResourceExhaustion.as_str(),
            "resource_exhaustion"
        );
        assert_eq!(FailureClass::ProviderFailure.as_str(), "provider_failure");
        assert_eq!(FailureClass::QueryFailure.as_str(), "query_failure");
    }

    #[test]
    fn classifier_preserves_indexing_failure_class() {
        let classifier = FailureClassifier::new();
        let classified = classifier.classify(FailureClass::CorruptIndex);

        assert_eq!(classified.class(), FailureClass::CorruptIndex);
        assert_eq!(
            classified,
            ClassifiedFailure::new(FailureClass::CorruptIndex)
        );
        assert_eq!(classified.to_string(), "corrupt_index");
    }

    #[test]
    fn retryable_hint_is_conservative_and_core_owned() {
        assert_eq!(
            retryability(FailureClass::InvalidIndex),
            Retryability::NonRetryable
        );
        assert_eq!(
            retryability(FailureClass::StaleIndex),
            Retryability::NonRetryable
        );
        assert_eq!(
            retryability(FailureClass::CorruptIndex),
            Retryability::NonRetryable
        );
        assert_eq!(
            retryability(FailureClass::SourceDataFailure),
            Retryability::NonRetryable
        );
        assert_eq!(
            retryability(FailureClass::ResourceExhaustion),
            Retryability::NonRetryable
        );

        assert_eq!(
            retryability(FailureClass::UnavailableIndex),
            Retryability::Retryable
        );
        assert_eq!(
            retryability(FailureClass::ProviderFailure),
            Retryability::Retryable
        );
        assert_eq!(
            retryability(FailureClass::QueryFailure),
            Retryability::Retryable
        );
    }

    #[test]
    fn classifier_and_free_functions_have_identical_semantics() {
        let classifier = FailureClassifier::new();

        for class in FailureClass::ALL {
            assert_eq!(classifier.classify(class), classify(class));
            assert_eq!(classifier.retryability(class), retryability(class));
        }
    }

    #[test]
    fn retryable_hint_does_not_change_failure_class() {
        let classified = classify(FailureClass::ProviderFailure);

        assert_eq!(classified.class(), FailureClass::ProviderFailure);
        assert!(classified.may_retry());
        assert_eq!(classified.retryability(), Retryability::Retryable);
    }
}
