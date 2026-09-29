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
use super::rebuild::RebuildResult;
use super::update::{UpdateJournal, UpdateSequence};
use crate::consistency::versioning::{VersioningError, validate_publication_eligibility};
use crate::error::IndexingResult;
use crate::identity::IndexDefinitionIdentity;
use crate::index::{IndexDefinition, IndexVersionId, IndexVersionState, VersionLifecycle};
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
    state: IndexVersionState,
    replayed_through: Option<UpdateSequence>,
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

    /// Returns the full lifecycle state that was validated before preparation.
    #[must_use]
    pub fn state(&self) -> &IndexVersionState {
        &self.state
    }

    /// Returns the lifecycle state that was validated before preparation.
    #[must_use]
    pub const fn validated_lifecycle(&self) -> VersionLifecycle {
        self.state.lifecycle()
    }

    /// Returns the rebuild journal sequence that was replayed before preparation,
    /// when this publication came from a completed rebuild.
    #[must_use]
    pub const fn replayed_through(&self) -> Option<UpdateSequence> {
        self.replayed_through
    }

    /// Consumes the prepared value and returns its logical parts.
    #[must_use]
    pub fn into_parts(
        self,
    ) -> (
        BuildCandidate,
        Option<IndexVersionId>,
        Option<IndexVersionId>,
        IndexVersionState,
    ) {
        (
            self.candidate,
            self.base_version,
            self.validated_against_active,
            self.state,
        )
    }

    fn new(
        candidate: BuildCandidate,
        base_version: Option<IndexVersionId>,
        validated_against_active: Option<IndexVersionId>,
        state: IndexVersionState,
        replayed_through: Option<UpdateSequence>,
    ) -> Self {
        Self {
            candidate,
            base_version,
            validated_against_active,
            state,
            replayed_through,
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
    state: IndexVersionState,
    previous_active: Option<BuildCandidate>,
}

impl PublicationResult {
    /// Returns the newly active candidate.
    #[must_use]
    pub fn active(&self) -> &BuildCandidate {
        &self.active
    }

    /// Returns the lifecycle state produced by the successful publication
    /// transition.
    #[must_use]
    pub fn state(&self) -> &IndexVersionState {
        &self.state
    }

    /// Returns the previously active candidate, when one existed.
    #[must_use]
    pub fn previous_active(&self) -> Option<&BuildCandidate> {
        self.previous_active.as_ref()
    }

    /// Consumes the publication result and returns
    /// `(active, state, previous_active)`.
    #[must_use]
    pub fn into_parts(self) -> (BuildCandidate, IndexVersionState, Option<BuildCandidate>) {
        (self.active, self.state, self.previous_active)
    }
}

/// Errors produced by the logical publication boundary.
#[derive(Debug, Eq, PartialEq)]
pub enum PublicationError {
    /// Candidate validation or version-level publication eligibility failed.
    Versioning(VersioningError),

    /// The candidate and currently active candidate belong to different
    /// logical index resources.
    DefinitionIdentityMismatch {
        candidate_identity: IndexDefinitionIdentity,
        active_identity: IndexDefinitionIdentity,
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

    /// The supplied lifecycle state was not `Ready`.
    CandidateNotReady {
        candidate: IndexVersionId,
        lifecycle: VersionLifecycle,
    },

    /// The lifecycle state belongs to a different logical version than the
    /// candidate being prepared.
    CandidateStateMismatch {
        candidate: IndexVersionId,
        state: IndexVersionId,
    },

    /// A rebuild-derived publication requires the current journal to verify
    /// that no updates were accepted after the rebuild consistency point.
    JournalUnavailable { replayed_through: UpdateSequence },

    /// The current journal sequence differs from the rebuild consistency point
    /// carried through publication preparation.
    JournalSequenceChanged {
        replayed_through: UpdateSequence,
        current: UpdateSequence,
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
            Self::DefinitionIdentityMismatch {
                candidate_identity,
                active_identity,
            } => write!(
                formatter,
                "publication index mismatch: candidate index {candidate_identity:?} differs from active index {active_identity:?}"
            ),
            Self::ActiveVersionChanged { observed, current } => write!(
                formatter,
                "active version changed after publication preparation: observed {observed:?}, current {current:?}"
            ),
            Self::CandidateNotReady {
                candidate,
                lifecycle,
            } => write!(
                formatter,
                "candidate {candidate:?} is not ready for publication: lifecycle is {lifecycle:?}"
            ),
            Self::CandidateStateMismatch { candidate, state } => write!(
                formatter,
                "candidate lifecycle state belongs to {state:?}, not candidate {candidate:?}"
            ),
            Self::JournalUnavailable { replayed_through } => write!(
                formatter,
                "rebuild publication requires a current journal at sequence {replayed_through}"
            ),
            Self::JournalSequenceChanged {
                replayed_through,
                current,
            } => write!(
                formatter,
                "rebuild publication journal sequence changed: expected {replayed_through}, current {current}"
            ),
        }
    }
}

impl Error for PublicationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Versioning(error) => Some(error),
            Self::DefinitionIdentityMismatch { .. }
            | Self::ActiveVersionChanged { .. }
            | Self::CandidateNotReady { .. }
            | Self::CandidateStateMismatch { .. }
            | Self::JournalUnavailable { .. }
            | Self::JournalSequenceChanged { .. } => None,
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
            Self::DefinitionIdentityMismatch {
                candidate_identity,
                active_identity,
            } => (
                "INDEXING.PUBLICATION.002",
                ErrorClass::Contract,
                format!(
                    "publication index mismatch: candidate index {candidate_identity:?} differs from active index {active_identity:?}"
                ),
                vec![
                    ("candidate_identity", format!("{candidate_identity:?}")),
                    ("active_identity", format!("{active_identity:?}")),
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
            Self::CandidateNotReady {
                candidate,
                lifecycle,
            } => (
                "INDEXING.PUBLICATION.004",
                ErrorClass::Contract,
                format!(
                    "candidate {candidate:?} is not ready for publication: lifecycle is {lifecycle:?}"
                ),
                vec![
                    ("candidate_version", candidate.to_string()),
                    ("candidate_lifecycle", format!("{lifecycle:?}")),
                ],
            ),
            Self::CandidateStateMismatch { candidate, state } => (
                "INDEXING.PUBLICATION.005",
                ErrorClass::Contract,
                format!(
                    "candidate lifecycle state belongs to {state:?}, not candidate {candidate:?}"
                ),
                vec![
                    ("candidate_version", candidate.to_string()),
                    ("state_version", state.to_string()),
                ],
            ),
            Self::JournalUnavailable { replayed_through } => (
                "INDEXING.PUBLICATION.006",
                ErrorClass::Contract,
                format!(
                    "rebuild publication requires a current journal at sequence {replayed_through}"
                ),
                vec![("replayed_through", replayed_through.to_string())],
            ),
            Self::JournalSequenceChanged {
                replayed_through,
                current,
            } => (
                "INDEXING.PUBLICATION.007",
                ErrorClass::Contract,
                format!(
                    "rebuild publication journal sequence changed: expected {replayed_through}, current {current}"
                ),
                vec![
                    ("replayed_through", replayed_through.to_string()),
                    ("current_journal_sequence", current.to_string()),
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
    /// The supplied [`IndexVersionState`] must belong to the same candidate
    /// version and must already be in the `Ready` lifecycle state. Publication
    /// therefore cannot bypass the candidate's explicit Building →
    /// Validating → Ready transition.
    ///
    /// This then performs the full Phase 3 version-level
    /// publication-eligibility check. The returned [`PublicationCandidate`]
    /// represents the READY side of the publication boundary.
    #[allow(clippy::result_large_err)]
    pub fn prepare(
        &self,
        candidate: BuildCandidate,
        candidate_state: IndexVersionState,
        definition: &IndexDefinition,
        base_version: Option<IndexVersionId>,
        active_version: Option<IndexVersionId>,
    ) -> Result<PublicationCandidate, PublicationError> {
        self.prepare_internal(
            candidate,
            candidate_state,
            definition,
            base_version,
            active_version,
            None,
        )
    }

    /// Prepares a completed rebuild for publication while carrying its replay
    /// consistency boundary into the publication candidate.
    ///
    /// The rebuild result supplies both the candidate and the exact journal
    /// sequence through which it has been replayed. The current journal is
    /// rechecked by [`Self::publish`] immediately before the active transition.
    #[allow(clippy::result_large_err)]
    pub fn prepare_rebuild(
        &self,
        rebuild: RebuildResult,
        candidate_state: IndexVersionState,
        definition: &IndexDefinition,
        active_version: Option<IndexVersionId>,
    ) -> Result<PublicationCandidate, PublicationError> {
        let (candidate, base_version, _captured_sequence, replayed_through, _replayed_updates) =
            rebuild.into_parts();

        self.prepare_internal(
            candidate,
            candidate_state,
            definition,
            base_version,
            active_version,
            Some(replayed_through),
        )
    }

    #[allow(clippy::result_large_err)]
    fn prepare_internal(
        &self,
        candidate: BuildCandidate,
        candidate_state: IndexVersionState,
        definition: &IndexDefinition,
        base_version: Option<IndexVersionId>,
        active_version: Option<IndexVersionId>,
        replayed_through: Option<UpdateSequence>,
    ) -> Result<PublicationCandidate, PublicationError> {
        if candidate_state.version() != candidate.version() {
            return Err(PublicationError::CandidateStateMismatch {
                candidate: candidate.version().id().clone(),
                state: candidate_state.id().clone(),
            });
        }

        if candidate_state.lifecycle() != VersionLifecycle::Ready {
            return Err(PublicationError::CandidateNotReady {
                candidate: candidate.version().id().clone(),
                lifecycle: candidate_state.lifecycle(),
            });
        }

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
            candidate_state,
            replayed_through,
        ))
    }

    /// Publishes a prepared candidate against the current active candidate and,
    /// for rebuild-derived candidates, the current logical update journal.
    ///
    /// Both observations are checked in this final publication boundary. A
    /// rebuild-derived candidate cannot publish without a current journal, and
    /// the journal sequence must still equal the rebuild's replay boundary.
    /// The outer lifecycle owner must accept the returned state under the
    /// synchronization boundary that owns active-version and journal state.
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
        current_journal: Option<&UpdateJournal>,
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
            if prepared.candidate().definition_identity() != active.definition_identity() {
                return Err(PublicationError::DefinitionIdentityMismatch {
                    candidate_identity: prepared.candidate().definition_identity().clone(),
                    active_identity: active.definition_identity().clone(),
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

        if let Some(expected_sequence) = prepared.replayed_through() {
            let journal = current_journal.ok_or(PublicationError::JournalUnavailable {
                replayed_through: expected_sequence,
            })?;
            let current_sequence = journal.current_sequence();

            if current_sequence != expected_sequence {
                return Err(PublicationError::JournalSequenceChanged {
                    replayed_through: expected_sequence,
                    current: current_sequence,
                });
            }
        }

        let (active, _base_version, _validated_against_active, mut state) = prepared.into_parts();

        state
            .mark_published()
            .expect("prepared state is Ready, and Ready → Published is valid");

        Ok(PublicationResult {
            active,
            state,
            previous_active: current_active,
        })
    }

    /// Core-error result adapter for publication preparation.
    #[allow(clippy::result_large_err)]
    pub fn prepare_result(
        &self,
        candidate: BuildCandidate,
        candidate_state: IndexVersionState,
        definition: &IndexDefinition,
        base_version: Option<IndexVersionId>,
        active_version: Option<IndexVersionId>,
        context: ErrorContext,
    ) -> IndexingResult<PublicationCandidate> {
        self.prepare(
            candidate,
            candidate_state,
            definition,
            base_version,
            active_version,
        )
        .map_err(|error| ErrorEvent::new(error.into_global_error(context)))
    }

    /// Core-error result adapter for preparing a completed rebuild for
    /// publication.
    #[allow(clippy::result_large_err)]
    pub fn prepare_rebuild_result(
        &self,
        rebuild: RebuildResult,
        candidate_state: IndexVersionState,
        definition: &IndexDefinition,
        active_version: Option<IndexVersionId>,
        context: ErrorContext,
    ) -> IndexingResult<PublicationCandidate> {
        self.prepare_rebuild(rebuild, candidate_state, definition, active_version)
            .map_err(|error| ErrorEvent::new(error.into_global_error(context)))
    }

    /// Core-error result adapter for the publication transition.
    #[allow(clippy::result_large_err)]
    pub fn publish_result(
        &self,
        prepared: PublicationCandidate,
        current_active: Option<BuildCandidate>,
        current_journal: Option<&UpdateJournal>,
        context: ErrorContext,
    ) -> IndexingResult<PublicationResult> {
        self.publish(prepared, current_active, current_journal)
            .map_err(|error| ErrorEvent::new(error.into_global_error(context)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build::builder::{BuildInput, BuildSnapshot, IndexBuilder};
    use crate::build::rebuild::{IndexRebuilder, RebuildInput};
    use crate::build::update::UpdateSequence;
    use crate::identity::{IndexDefinitionId, IndexDefinitionIdentity, IndexNamespace};
    use crate::index::{
        ConsistencyRequirement, IndexEntry, IndexFamily, IndexVersionState, KeyDefinition,
        KeyMaterial, ObjectReference, TargetReferenceType, Uniqueness, VersionLifecycle,
    };
    use nizaam_core::identity::{CorrelationId, OperationId};
    use nizaam_core::operation::{Operation, OperationContext};

    fn definition_identity() -> IndexDefinitionIdentity {
        IndexDefinitionIdentity::new(
            IndexDefinitionId::new("test.publication").expect("definition ID should be valid"),
            IndexNamespace::new("test").expect("namespace should be valid"),
            IndexFamily::Inverted,
        )
    }

    fn alternate_definition_identity() -> IndexDefinitionIdentity {
        IndexDefinitionIdentity::new(
            IndexDefinitionId::new("test.publication.other")
                .expect("definition ID should be valid"),
            IndexNamespace::new("test").expect("namespace should be valid"),
            IndexFamily::Inverted,
        )
    }

    fn version_id(value: &str) -> IndexVersionId {
        IndexVersionId::new(value).expect("test version ID should be valid")
    }

    fn definition_for(identity: IndexDefinitionIdentity) -> IndexDefinition {
        IndexDefinition::new(
            identity,
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

    fn definition() -> IndexDefinition {
        definition_for(definition_identity())
    }

    fn test_candidate(
        definition_identity: IndexDefinitionIdentity,
        version: &str,
    ) -> BuildCandidate {
        let input = BuildInput::new(
            definition_identity.clone(),
            definition_for(definition_identity),
            version_id(version),
            BuildSnapshot::new(std::iter::empty()),
        );
        IndexBuilder::new()
            .build(input)
            .expect("test candidate should build")
    }

    fn ready_state(candidate: &BuildCandidate) -> IndexVersionState {
        let mut state = IndexVersionState::new(candidate.version().clone());
        state
            .transition_to(VersionLifecycle::Validating)
            .expect("candidate state should enter validation");
        state
            .mark_ready()
            .expect("candidate state should become ready");
        state
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
        let candidate = test_candidate(definition_identity(), "v1");

        let prepared = publisher
            .prepare(
                candidate.clone(),
                ready_state(&candidate),
                &definition(),
                None,
                None,
            )
            .expect("initial candidate should be publishable");

        assert_eq!(prepared.candidate(), &candidate);
        assert!(prepared.base_version().is_none());
        assert!(prepared.validated_against_active().is_none());
    }

    #[test]
    fn prepare_rejects_candidate_that_is_not_ready() {
        let publisher = IndexPublisher::new();
        let candidate = test_candidate(definition_identity(), "v1");
        let state = IndexVersionState::new(candidate.version().clone());

        let error = publisher
            .prepare(candidate, state, &definition(), None, None)
            .expect_err("building candidates must not cross the publication boundary");

        assert!(matches!(
            error,
            PublicationError::CandidateNotReady {
                lifecycle: VersionLifecycle::Building,
                ..
            }
        ));
    }

    #[test]
    fn prepare_rejects_state_for_a_different_version() {
        let publisher = IndexPublisher::new();
        let candidate = test_candidate(definition_identity(), "v2");
        let other_candidate = test_candidate(definition_identity(), "v1");
        let state = ready_state(&other_candidate);

        let error = publisher
            .prepare(
                candidate,
                state,
                &definition(),
                Some(version_id("v1")),
                Some(version_id("v1")),
            )
            .expect_err("lifecycle state must belong to the candidate version");

        assert!(matches!(
            error,
            PublicationError::CandidateStateMismatch { .. }
        ));
    }

    #[test]
    fn prepare_rejects_stale_candidate() {
        let publisher = IndexPublisher::new();
        let candidate = test_candidate(definition_identity(), "v2");

        let candidate_state = ready_state(&candidate);
        let error = publisher
            .prepare(
                candidate,
                candidate_state,
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
        let previous = test_candidate(definition_identity(), "v1");
        let next = test_candidate(definition_identity(), "v2");
        let next_state = ready_state(&next);
        let prepared = publisher
            .prepare(
                next.clone(),
                next_state,
                &definition(),
                Some(version_id("v1")),
                Some(version_id("v1")),
            )
            .expect("v2 should prepare from v1");

        let result = publisher
            .publish(prepared, Some(previous.clone()), None)
            .expect("publication should succeed");

        assert_eq!(result.active(), &next);
        assert_eq!(result.state().lifecycle(), VersionLifecycle::Published);
        assert_eq!(result.previous_active(), Some(&previous));
    }

    #[test]
    fn publish_rejects_changed_active_version() {
        let publisher = IndexPublisher::new();
        let candidate = test_candidate(definition_identity(), "v3");
        let candidate_state = ready_state(&candidate);
        let prepared = publisher
            .prepare(
                candidate,
                candidate_state,
                &definition(),
                Some(version_id("v1")),
                Some(version_id("v1")),
            )
            .expect("candidate should prepare against v1");
        let current_active = test_candidate(definition_identity(), "v2");

        let error = publisher
            .publish(prepared, Some(current_active), None)
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
    fn publish_rejects_different_definition_identity() {
        let publisher = IndexPublisher::new();
        let first = definition_identity();
        let second = alternate_definition_identity();
        let candidate = test_candidate(first.clone(), "v2");
        let candidate_state = ready_state(&candidate);
        let prepared = publisher
            .prepare(
                candidate,
                candidate_state,
                &definition(),
                Some(version_id("v1")),
                Some(version_id("v1")),
            )
            .expect("candidate should prepare");
        let active_candidate = test_candidate(second.clone(), "v1");

        let error = publisher
            .publish(prepared, Some(active_candidate), None)
            .expect_err("different logical definition identities must not publish together");

        assert_eq!(
            error,
            PublicationError::DefinitionIdentityMismatch {
                candidate_identity: first,
                active_identity: second,
            }
        );
    }

    #[test]
    fn rebuild_publication_rejects_a_journal_advance_after_rebuild_completion() {
        let mut journal = UpdateJournal::new();
        let rebuilder = IndexRebuilder::new();
        let rebuilt = rebuilder
            .rebuild(
                RebuildInput::new(
                    definition_identity(),
                    definition(),
                    version_id("v2"),
                    BuildSnapshot::new(std::iter::empty()),
                    Some(version_id("v1")),
                    Some(version_id("v1")),
                ),
                &journal,
            )
            .expect("rebuild should finish at sequence zero");

        let rebuilt_candidate = rebuilt.candidate().clone();
        let prepared = IndexPublisher::new()
            .prepare_rebuild(
                rebuilt,
                ready_state(&rebuilt_candidate),
                &definition(),
                Some(version_id("v1")),
            )
            .expect("completed rebuild should prepare for publication");

        journal
            .append(super::super::update::IndexMutation::Insert(
                IndexEntry::new(
                    KeyMaterial::text("late"),
                    ObjectReference::new("object", "late").expect("reference should be valid"),
                )
                .expect("entry should be valid"),
            ))
            .expect("journal advance should succeed");

        let current_active = test_candidate(definition_identity(), "v1");
        let error = IndexPublisher::new()
            .publish(prepared, Some(current_active), Some(&journal))
            .expect_err("publication must reject a journal advance after rebuild");
        assert!(matches!(
            error,
            PublicationError::JournalSequenceChanged {
                replayed_through,
                current,
            } if replayed_through == UpdateSequence::INITIAL
                && current == UpdateSequence::new(1)
        ));
    }

    #[test]
    fn successful_publication_result_is_core_adaptable() {
        let publisher = IndexPublisher::new();
        let candidate = test_candidate(definition_identity(), "v1");
        let candidate_state = ready_state(&candidate);
        let prepared = publisher
            .prepare(candidate, candidate_state, &definition(), None, None)
            .expect("initial candidate should prepare");

        let result = publisher.publish_result(prepared, None, None, context());
        assert!(result.is_ok());
    }
}
