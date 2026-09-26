//! Controlled publication of validated Indexing candidates for Phase 3.
//!
//! Publication owns the logical `READY → ACTIVE` boundary without introducing
//! physical provider state or a second runtime. The operation is functional:
//! all validation is completed before the returned value identifies the new
//! active candidate, while the previous active candidate remains available to
//! the caller until that returned transition is accepted by the outer
//! lifecycle owner.
//!
//! Conceptually:
//! ```text
//! validated candidate
//!        ↓
//! publication eligibility
//!        ↓
//! lineage re-check
//!        ↓
//! active candidate
//! ```
//!
//! A candidate is prepared for publication only after the version-level rules
//! and structural candidate validation succeed. Preparation is represented as
//! a type rather than as another lifecycle enum; the canonical lifecycle state
//! model remains owned by the later `index/version.rs` integration.
//!
//! Publication deliberately does not implement:
//! - physical provider/storage publication;
//! - persistence or database transactions;
//! - query consistency or retrieval;
//! - security/authorization;
//! - a global scheduler or Control Plane;
//! - an Indexing-specific cancellation/deadline mechanism.

use super::builder::BuildCandidate;
use crate::consistency::versioning::{VersioningError, validate_publication_eligibility};
use crate::error::IndexingResult;
use crate::identity::IndexId;
use crate::index::{IndexDefinition, IndexVersionId};
use core::fmt;
use nizaam_core::contracts::Version as CoreVersion;
use nizaam_core::error::{
    ErrorClass, ErrorCode, ErrorContext, ErrorEvent, ErrorOwner, GlobalError, Severity,
};
use nizaam_core::status::Retryability;
use std::error::Error;

/// Candidate prepared for the `READY → ACTIVE` publication boundary.
///
/// Construction is deliberately tied to successful publication-eligibility
/// validation. Once a value of this type exists, it contains an owned logical
/// candidate and the lineage observation against which it was validated.
///
/// The value still has no physical provider handle and is not itself the global
/// active-version registry. The outer lifecycle owner performs the actual state
/// replacement by accepting [`PublicationResult`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublicationCandidate {
    candidate: BuildCandidate,
    base_version: Option<IndexVersionId>,
    validated_against_active: Option<IndexVersionId>,
}

impl PublicationCandidate {
    /// Returns the candidate that passed publication preparation.
    #[must_use]
    pub fn candidate(&self) -> &BuildCandidate {
        &self.candidate
    }

    /// Returns the candidate base version used for lineage validation.
    #[must_use]
    pub fn base_version(&self) -> Option<&IndexVersionId> {
        self.base_version.as_ref()
    }

    /// Returns the active version observed when this candidate was prepared.
    #[must_use]
    pub fn validated_against_active(&self) -> Option<&IndexVersionId> {
        self.validated_against_active.as_ref()
    }

    /// Consumes the prepared value and returns its logical parts.
    #[must_use]
    pub fn into_parts(
        self,
    ) -> (
        BuildCandidate,
        Option<IndexVersionId>,
        Option<IndexVersionId>,
    ) {
        (
            self.candidate,
            self.base_version,
            self.validated_against_active,
        )
    }

    fn new(
        candidate: BuildCandidate,
        base_version: Option<IndexVersionId>,
        validated_against_active: Option<IndexVersionId>,
    ) -> Self {
        Self {
            candidate,
            base_version,
            validated_against_active,
        }
    }
}

/// Result of a successful logical publication transition.
///
/// The returned `active` candidate is the new logical active version. The
/// previous active candidate is preserved in the result so an outer lifecycle
/// owner can retain historical/rollback information without this module
/// mutating or discarding it during validation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublicationResult {
    active: BuildCandidate,
    previous_active: Option<BuildCandidate>,
}

impl PublicationResult {
    /// Returns the newly active candidate.
    #[must_use]
    pub fn active(&self) -> &BuildCandidate {
        &self.active
    }

    /// Returns the previously active candidate, when one existed.
    #[must_use]
    pub fn previous_active(&self) -> Option<&BuildCandidate> {
        self.previous_active.as_ref()
    }

    /// Consumes the publication result and returns `(active, previous_active)`.
    #[must_use]
    pub fn into_parts(self) -> (BuildCandidate, Option<BuildCandidate>) {
        (self.active, self.previous_active)
    }
}

/// Errors produced by the logical publication boundary.
#[derive(Debug, Eq, PartialEq)]
pub enum PublicationError {
    /// Candidate validation or version-level publication eligibility failed.
    Versioning(VersioningError),

    /// The candidate and currently active candidate belong to different
    /// logical index resources.
    IndexMismatch {
        candidate_index: IndexId,
        active_index: IndexId,
    },

    /// The active version changed after preparation and before publication.
    ///
    /// The caller must re-prepare/re-validate the candidate against the new
    /// active lineage rather than allowing an older prepared candidate to
    /// overwrite the newer active candidate.
    ActiveVersionChanged {
        observed: Option<IndexVersionId>,
        current: Option<IndexVersionId>,
    },
}

impl fmt::Display for PublicationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Versioning(error) => {
                write!(
                    formatter,
                    "publication versioning validation failed: {error}"
                )
            }
            Self::IndexMismatch {
                candidate_index,
                active_index,
            } => write!(
                formatter,
                "publication index mismatch: candidate index {candidate_index:?} differs from active index {active_index:?}"
            ),
            Self::ActiveVersionChanged { observed, current } => write!(
                formatter,
                "active version changed after publication preparation: observed {observed:?}, current {current:?}"
            ),
        }
    }
}

impl Error for PublicationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Versioning(error) => Some(error),
            Self::IndexMismatch { .. } | Self::ActiveVersionChanged { .. } => None,
        }
    }
}

impl PublicationError {
    /// Converts the typed publication failure into the shared Core error
    /// contract using the caller-supplied execution context.
    #[must_use]
    pub fn into_global_error(self, context: ErrorContext) -> GlobalError {
        let (code, class, message, details) = match self {
            Self::Versioning(error) => (
                "INDEXING.PUBLICATION.001",
                ErrorClass::Contract,
                format!("publication versioning validation failed: {error}"),
                Vec::new(),
            ),
            Self::IndexMismatch {
                candidate_index,
                active_index,
            } => (
                "INDEXING.PUBLICATION.002",
                ErrorClass::Contract,
                format!(
                    "publication index mismatch: candidate index {candidate_index:?} differs from active index {active_index:?}"
                ),
                vec![
                    ("candidate_index", format!("{candidate_index:?}")),
                    ("active_index", format!("{active_index:?}")),
                ],
            ),
            Self::ActiveVersionChanged { observed, current } => (
                "INDEXING.PUBLICATION.003",
                ErrorClass::Contract,
                format!(
                    "active version changed after publication preparation: observed {observed:?}, current {current:?}"
                ),
                vec![
                    ("observed_active_version", format!("{observed:?}")),
                    ("current_active_version", format!("{current:?}")),
                ],
            ),
        };

        let mut global = GlobalError {
            code: ErrorCode::new(code).expect("Indexing error code is statically valid"),
            owner: ErrorOwner::new("INDEXING").expect("Indexing error owner is statically valid"),
            version: CoreVersion::new(1, 0, 0),
            class,
            severity: Severity::Error,
            retryability: Retryability::NonRetryable,
            message,
            details: Vec::new(),
            solution_reference: None,
            context,
            cause: None,
        };

        for (key, value) in details {
            if let Some(detail) = nizaam_core::error::DiagnosticDetail::new(key, value) {
                global = global.with_detail(detail);
            }
        }

        global
    }
}

/// Stateless authority for logical candidate publication.
///
/// The publisher does not own the active registry. Instead it performs the
/// complete validation necessary for a safe logical transition and returns a
/// functional result that the lifecycle owner can commit as one state change.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct IndexPublisher;

impl IndexPublisher {
    /// Creates a stateless logical publisher.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Prepares a candidate for publication against the active version
    /// observed by the caller.
    ///
    /// This performs the full Phase 3 version-level publication-eligibility
    /// check. The returned [`PublicationCandidate`] represents the READY side
    /// of the publication boundary without introducing a duplicate lifecycle
    /// state enum here.
    #[allow(clippy::result_large_err)]
    pub fn prepare(
        &self,
        candidate: BuildCandidate,
        definition: &IndexDefinition,
        base_version: Option<IndexVersionId>,
        active_version: Option<IndexVersionId>,
    ) -> Result<PublicationCandidate, PublicationError> {
        validate_publication_eligibility(
            candidate.version(),
            definition,
            base_version.as_ref(),
            active_version.as_ref(),
        )
        .map_err(PublicationError::Versioning)?;

        Ok(PublicationCandidate::new(
            candidate,
            base_version,
            active_version,
        ))
    }

    /// Publishes a prepared candidate against the current active candidate.
    ///
    /// The current active candidate is checked again at this boundary. This is
    /// what prevents a stale prepared candidate from replacing a newer active
    /// version when multiple candidates exist concurrently.
    ///
    /// No caller-visible state is replaced until all checks have succeeded.
    /// On failure the supplied current active candidate remains untouched.
    #[allow(clippy::result_large_err)]
    pub fn publish(
        &self,
        prepared: PublicationCandidate,
        current_active: Option<BuildCandidate>,
    ) -> Result<PublicationResult, PublicationError> {
        let current_active_version = current_active
            .as_ref()
            .map(|candidate| candidate.version().id().clone());

        if prepared.validated_against_active().cloned() != current_active_version {
            return Err(PublicationError::ActiveVersionChanged {
                observed: prepared.validated_against_active().cloned(),
                current: current_active_version,
            });
        }

        if let Some(active) = current_active.as_ref() {
            if prepared.candidate().index_id() != active.index_id() {
                return Err(PublicationError::IndexMismatch {
                    candidate_index: *prepared.candidate().index_id(),
                    active_index: *active.index_id(),
                });
            }

            validate_publication_eligibility(
                prepared.candidate().version(),
                prepared.candidate().definition(),
                prepared.base_version(),
                Some(active.version().id()),
            )
            .map_err(PublicationError::Versioning)?;
        } else {
            validate_publication_eligibility(
                prepared.candidate().version(),
                prepared.candidate().definition(),
                prepared.base_version(),
                None,
            )
            .map_err(PublicationError::Versioning)?;
        }

        let (active, _base_version, _validated_against_active) = prepared.into_parts();
        Ok(PublicationResult {
            active,
            previous_active: current_active,
        })
    }

    /// Core-error result adapter for publication preparation.
    #[allow(clippy::result_large_err)]
    pub fn prepare_result(
        &self,
        candidate: BuildCandidate,
        definition: &IndexDefinition,
        base_version: Option<IndexVersionId>,
        active_version: Option<IndexVersionId>,
        context: ErrorContext,
    ) -> IndexingResult<PublicationCandidate> {
        self.prepare(candidate, definition, base_version, active_version)
            .map_err(|error| ErrorEvent::new(error.into_global_error(context)))
    }

    /// Core-error result adapter for the publication transition.
    #[allow(clippy::result_large_err)]
    pub fn publish_result(
        &self,
        prepared: PublicationCandidate,
        current_active: Option<BuildCandidate>,
        context: ErrorContext,
    ) -> IndexingResult<PublicationResult> {
        self.publish(prepared, current_active)
            .map_err(|error| ErrorEvent::new(error.into_global_error(context)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build::builder::{BuildInput, BuildSnapshot, IndexBuilder};
    use crate::identity::{IndexDefinitionId, IndexDefinitionIdentity, IndexNamespace};
    use crate::index::{
        ConsistencyRequirement, IndexFamily, KeyDefinition, TargetReferenceType, Uniqueness,
    };
    use nizaam_core::identity::{CorrelationId, OperationId};
    use nizaam_core::operation::{Operation, OperationContext};

    fn index_id() -> IndexId {
        IndexId::from_bytes([0x11; 64])
    }

    fn version_id(value: &str) -> IndexVersionId {
        IndexVersionId::new(value).expect("test version ID should be valid")
    }

    fn definition() -> IndexDefinition {
        IndexDefinition::new(
            IndexDefinitionIdentity::new(
                IndexDefinitionId::new("test.publication").expect("definition ID should be valid"),
                IndexNamespace::new("test").expect("namespace should be valid"),
                IndexFamily::Inverted,
            ),
            KeyDefinition::new(["text"]).expect("key definition should be valid"),
            TargetReferenceType::new("object").expect("target reference type should be valid"),
            Uniqueness::NonUnique,
            ConsistencyRequirement::new("eventual")
                .expect("consistency requirement should be valid"),
            None,
            None,
        )
        .expect("definition should be valid")
    }

    fn candidate(index_id: IndexId, version: &str) -> BuildCandidate {
        let input = BuildInput::new(
            index_id,
            definition(),
            version_id(version),
            BuildSnapshot::new(std::iter::empty()),
        );
        IndexBuilder::new()
            .build(input)
            .expect("test candidate should build")
    }

    fn context() -> ErrorContext {
        ErrorContext::new(OperationContext::new(Operation::new(
            OperationId::new("publication-test-operation").expect("test operation should be valid"),
            CorrelationId::new("publication-test-correlation")
                .expect("test correlation should be valid"),
        )))
    }

    #[test]
    fn prepare_valid_initial_candidate() {
        let publisher = IndexPublisher::new();
        let candidate = candidate(index_id(), "v1");

        let prepared = publisher
            .prepare(candidate.clone(), &definition(), None, None)
            .expect("initial candidate should be publishable");

        assert_eq!(prepared.candidate(), &candidate);
        assert!(prepared.base_version().is_none());
        assert!(prepared.validated_against_active().is_none());
    }

    #[test]
    fn prepare_rejects_stale_candidate() {
        let publisher = IndexPublisher::new();
        let candidate = candidate(index_id(), "v2");

        let error = publisher
            .prepare(
                candidate,
                &definition(),
                Some(version_id("v1")),
                Some(version_id("v3")),
            )
            .expect_err("candidate based on v1 must not prepare against v3");

        assert!(matches!(
            error,
            PublicationError::Versioning(VersioningError::StaleCandidate { .. })
        ));
    }

    #[test]
    fn publish_preserves_previous_active_candidate() {
        let publisher = IndexPublisher::new();
        let previous = candidate(index_id(), "v1");
        let next = candidate(index_id(), "v2");
        let prepared = publisher
            .prepare(
                next.clone(),
                &definition(),
                Some(version_id("v1")),
                Some(version_id("v1")),
            )
            .expect("v2 should prepare from v1");

        let result = publisher
            .publish(prepared, Some(previous.clone()))
            .expect("publication should succeed");

        assert_eq!(result.active(), &next);
        assert_eq!(result.previous_active(), Some(&previous));
    }

    #[test]
    fn publish_rejects_changed_active_version() {
        let publisher = IndexPublisher::new();
        let prepared = publisher
            .prepare(
                candidate(index_id(), "v3"),
                &definition(),
                Some(version_id("v1")),
                Some(version_id("v1")),
            )
            .expect("candidate should prepare against v1");
        let current = candidate(index_id(), "v2");

        let error = publisher
            .publish(prepared, Some(current))
            .expect_err("older prepared observation must not overwrite v2");

        assert_eq!(
            error,
            PublicationError::ActiveVersionChanged {
                observed: Some(version_id("v1")),
                current: Some(version_id("v2")),
            }
        );
    }

    #[test]
    fn publish_rejects_different_index_resource() {
        let publisher = IndexPublisher::new();
        let first = index_id();
        let second = IndexId::from_bytes([0x22; 64]);
        let prepared = publisher
            .prepare(
                candidate(first, "v2"),
                &definition(),
                Some(version_id("v1")),
                Some(version_id("v1")),
            )
            .expect("candidate should prepare");
        let active = candidate(second, "v1");

        let error = publisher
            .publish(prepared, Some(active))
            .expect_err("different logical indexes must not publish together");

        assert_eq!(
            error,
            PublicationError::IndexMismatch {
                candidate_index: first,
                active_index: second,
            }
        );
    }

    #[test]
    fn successful_publication_result_is_core_adaptable() {
        let publisher = IndexPublisher::new();
        let prepared = publisher
            .prepare(candidate(index_id(), "v1"), &definition(), None, None)
            .expect("initial candidate should prepare");

        let result = publisher.publish_result(prepared, None, context());
        assert!(result.is_ok());
    }
}
