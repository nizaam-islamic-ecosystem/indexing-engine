//! Index-specific lifecycle state and guarded lifecycle transitions.
//!
//! Phase 5 deliberately separates the lifecycle of an individual Indexing
//! index from the Core engine lifecycle and from operational conditions such
//! as integrity, synchronization, availability, resource pressure, and
//! failure classification.
//!
//! The lifecycle represented here is:
//!
//! ```text
//! Creating
//!     ↓
//! Building
//!     ↓
//! Validating
//!     ↓
//! Ready
//!     ↓
//! Active
//!     ↓
//! Maintaining
//!     ↓
//! Active
//!     ↓
//! Retiring
//!     ↓
//! Retired
//! ```
//!
//! Guarded alternate paths to `Retiring` are intentionally allowed from the
//! non-terminal construction/operation states where an index can be safely
//! withdrawn before becoming active. No transition is permitted to bypass the
//! lifecycle's validation/activation boundary, and `Retired` is terminal.
//!
//! This module does **not** decide:
//! - whether an index is corrupt or stale;
//! - whether an index is currently available for queries;
//! - whether an operation should be retried;
//! - whether an index should be rebuilt;
//! - how much capacity is available; or
//! - whether the Core engine itself is ready or serving.
//!
//! Those concerns belong to the Phase 5 integrity, recovery, capacity, and
//! Core runtime/health boundaries respectively.

use crate::identity::IndexDefinitionIdentity;
use core::fmt;
use std::error::Error;

/// Lifecycle state of one logical Indexing index.
///
/// The lifecycle is intentionally independent from the Core engine lifecycle.
/// Operational dimensions such as `Stale`, `Unavailable`, or `Corrupt` are
/// not lifecycle states and must be represented by their owning subsystems.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum IndexLifecycleState {
    /// The logical index identity exists and creation has begun.
    Creating,

    /// Index contents are being constructed or populated.
    Building,

    /// The constructed index is undergoing logical validation.
    Validating,

    /// Validation completed successfully and the index is eligible for
    /// activation.
    Ready,

    /// The index is the active logical index state for its owning lifecycle
    /// boundary.
    Active,

    /// The active index is undergoing controlled maintenance.
    Maintaining,

    /// The index is being withdrawn from service and will not become active
    /// again through this lifecycle instance.
    Retiring,

    /// The index lifecycle has ended. This state is terminal.
    Retired,
}

impl IndexLifecycleState {
    /// Returns whether the state is terminal.
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Retired)
    }

    /// Returns whether the state is the active operational lifecycle state.
    ///
    /// This does not imply that the index is healthy, synchronized, or
    /// queryable. Those are separate operational dimensions.
    #[must_use]
    pub const fn is_active(self) -> bool {
        matches!(self, Self::Active)
    }

    /// Returns whether the lifecycle state permits the next transition.
    ///
    /// Repeating the same state is a valid no-op. All other transitions are
    /// explicitly enumerated so that future lifecycle changes cannot silently
    /// widen the transition graph.
    #[must_use]
    pub const fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            // Idempotent no-op transitions.
            (Self::Creating, Self::Creating)
                | (Self::Building, Self::Building)
                | (Self::Validating, Self::Validating)
                | (Self::Ready, Self::Ready)
                | (Self::Active, Self::Active)
                | (Self::Maintaining, Self::Maintaining)
                | (Self::Retiring, Self::Retiring)
                | (Self::Retired, Self::Retired)
                // Guarded forward lifecycle transitions.
                | (Self::Creating, Self::Building)
                | (Self::Building, Self::Validating)
                | (Self::Validating, Self::Ready)
                | (Self::Ready, Self::Active)
                | (Self::Active, Self::Maintaining)
                | (Self::Maintaining, Self::Active)
                // Controlled retirement may begin before activation completes.
                | (Self::Creating, Self::Retiring)
                | (Self::Building, Self::Retiring)
                | (Self::Validating, Self::Retiring)
                | (Self::Ready, Self::Retiring)
                | (Self::Active, Self::Retiring)
                | (Self::Maintaining, Self::Retiring)
                // Retirement is terminal.
                | (Self::Retiring, Self::Retired)
        )
    }

    /// Returns all lifecycle states in their logical progression order.
    #[must_use]
    pub const fn all() -> [Self; 8] {
        [
            Self::Creating,
            Self::Building,
            Self::Validating,
            Self::Ready,
            Self::Active,
            Self::Maintaining,
            Self::Retiring,
            Self::Retired,
        ]
    }
}

/// One logical Indexing index together with its Phase 5 lifecycle state.
///
/// This value owns only lifecycle state. It does not own an active-version
/// registry, index contents, provider state, capacity accounting, integrity
/// state, recovery policy, or Core engine lifecycle.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IndexLifecycle {
    definition_identity: IndexDefinitionIdentity,
    state: IndexLifecycleState,
}

impl IndexLifecycle {
    /// Creates a lifecycle value for an index in the initial `Creating` state.
    #[must_use]
    pub fn new(definition_identity: IndexDefinitionIdentity) -> Self {
        Self {
            definition_identity,
            state: IndexLifecycleState::Creating,
        }
    }

    /// Returns the logical index definition identity owned by this lifecycle value.
    #[must_use]
    pub fn definition_identity(&self) -> &IndexDefinitionIdentity {
        &self.definition_identity
    }

    /// Returns the current index lifecycle state.
    #[must_use]
    pub const fn state(&self) -> IndexLifecycleState {
        self.state
    }

    /// Returns whether the current lifecycle state is terminal.
    #[must_use]
    pub const fn is_terminal(&self) -> bool {
        self.state.is_terminal()
    }

    /// Returns whether the current lifecycle state is `Active`.
    ///
    /// This does not imply integrity, synchronization, availability, or
    /// queryability.
    #[must_use]
    pub const fn is_active(&self) -> bool {
        self.state.is_active()
    }

    /// Advances the index through one explicitly permitted lifecycle
    /// transition.
    pub fn transition_to(
        &mut self,
        next: IndexLifecycleState,
    ) -> Result<(), IndexLifecycleTransitionError> {
        let current = self.state;
        if !current.can_transition_to(next) {
            return Err(IndexLifecycleTransitionError::new(
                self.definition_identity.clone(),
                current,
                next,
            ));
        }

        self.state = next;
        Ok(())
    }

    /// Advances from `Creating` to `Building`.
    pub fn mark_building(&mut self) -> Result<(), IndexLifecycleTransitionError> {
        self.transition_to(IndexLifecycleState::Building)
    }

    /// Advances from `Building` to `Validating`.
    pub fn mark_validating(&mut self) -> Result<(), IndexLifecycleTransitionError> {
        self.transition_to(IndexLifecycleState::Validating)
    }

    /// Advances from `Validating` to `Ready`.
    pub fn mark_ready(&mut self) -> Result<(), IndexLifecycleTransitionError> {
        self.transition_to(IndexLifecycleState::Ready)
    }

    /// Advances from `Ready` to `Active`.
    pub fn mark_active(&mut self) -> Result<(), IndexLifecycleTransitionError> {
        self.transition_to(IndexLifecycleState::Active)
    }

    /// Advances from `Active` to `Maintaining`.
    pub fn mark_maintaining(&mut self) -> Result<(), IndexLifecycleTransitionError> {
        self.transition_to(IndexLifecycleState::Maintaining)
    }

    /// Advances the index to `Retiring` when the current lifecycle state
    /// permits controlled retirement.
    ///
    /// The underlying transition guard remains authoritative, so this method
    /// also returns an error when called from an ineligible lifecycle state.
    pub fn mark_retiring(&mut self) -> Result<(), IndexLifecycleTransitionError> {
        self.transition_to(IndexLifecycleState::Retiring)
    }

    /// Advances from `Retiring` to the terminal `Retired` state.
    pub fn mark_retired(&mut self) -> Result<(), IndexLifecycleTransitionError> {
        self.transition_to(IndexLifecycleState::Retired)
    }

    /// Consumes the lifecycle value and returns `(definition_identity, state)`.
    #[must_use]
    pub fn into_parts(self) -> (IndexDefinitionIdentity, IndexLifecycleState) {
        (self.definition_identity, self.state)
    }
}

/// Error returned when an index lifecycle transition is not permitted.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IndexLifecycleTransitionError {
    definition_identity: IndexDefinitionIdentity,
    from: IndexLifecycleState,
    to: IndexLifecycleState,
}

impl IndexLifecycleTransitionError {
    /// Creates a lifecycle transition error.
    #[must_use]
    fn new(
        definition_identity: IndexDefinitionIdentity,
        from: IndexLifecycleState,
        to: IndexLifecycleState,
    ) -> Self {
        Self {
            definition_identity,
            from,
            to,
        }
    }

    /// Returns the affected logical index definition identity.
    #[must_use]
    pub fn definition_identity(&self) -> &IndexDefinitionIdentity {
        &self.definition_identity
    }

    /// Returns the lifecycle state from which the transition was attempted.
    #[must_use]
    pub const fn from(&self) -> IndexLifecycleState {
        self.from
    }

    /// Returns the requested destination lifecycle state.
    #[must_use]
    pub const fn to(&self) -> IndexLifecycleState {
        self.to
    }
}

impl fmt::Display for IndexLifecycleTransitionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "invalid lifecycle transition for index definition {:?}: {:?} → {:?}",
            self.definition_identity, self.from, self.to
        )
    }
}

impl Error for IndexLifecycleTransitionError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn definition_identity(byte: u8) -> IndexDefinitionIdentity {
        let definition_id =
            crate::identity::IndexDefinitionId::new(format!("lifecycle-definition-{byte}"))
                .expect("test definition ID must be valid");
        let namespace = crate::identity::IndexNamespace::new(format!("lifecycle.namespace.{byte}"))
            .expect("test namespace must be valid");

        IndexDefinitionIdentity::new(
            definition_id,
            namespace,
            crate::index::IndexFamily::Identity,
        )
    }

    #[test]
    fn new_index_starts_in_creating() {
        let lifecycle = IndexLifecycle::new(definition_identity(0x11));

        assert_eq!(lifecycle.definition_identity(), &definition_identity(0x11));
        assert_eq!(lifecycle.state(), IndexLifecycleState::Creating);
        assert!(!lifecycle.is_active());
        assert!(!lifecycle.is_terminal());
    }

    #[test]
    fn happy_path_reaches_retired_through_all_primary_states() {
        let mut lifecycle = IndexLifecycle::new(definition_identity(0x22));

        lifecycle
            .mark_building()
            .expect("Creating → Building should work");
        lifecycle
            .mark_validating()
            .expect("Building → Validating should work");
        lifecycle
            .mark_ready()
            .expect("Validating → Ready should work");
        lifecycle.mark_active().expect("Ready → Active should work");
        assert!(lifecycle.is_active());

        lifecycle
            .mark_maintaining()
            .expect("Active → Maintaining should work");
        assert!(!lifecycle.is_active());

        lifecycle
            .transition_to(IndexLifecycleState::Active)
            .expect("Maintaining → Active should work");
        assert!(lifecycle.is_active());

        lifecycle
            .mark_retiring()
            .expect("Active → Retiring should work");
        lifecycle
            .mark_retired()
            .expect("Retiring → Retired should work");

        assert_eq!(lifecycle.state(), IndexLifecycleState::Retired);
        assert!(lifecycle.is_terminal());
    }

    #[test]
    fn controlled_retirement_is_available_before_activation() {
        fn advance_to(
            lifecycle: &mut IndexLifecycle,
            target: IndexLifecycleState,
        ) -> Result<(), IndexLifecycleTransitionError> {
            match target {
                IndexLifecycleState::Creating => Ok(()),
                IndexLifecycleState::Building => lifecycle.mark_building(),
                IndexLifecycleState::Validating => {
                    lifecycle.mark_building()?;
                    lifecycle.mark_validating()
                }
                IndexLifecycleState::Ready => {
                    lifecycle.mark_building()?;
                    lifecycle.mark_validating()?;
                    lifecycle.mark_ready()
                }
                IndexLifecycleState::Active => {
                    lifecycle.mark_building()?;
                    lifecycle.mark_validating()?;
                    lifecycle.mark_ready()?;
                    lifecycle.mark_active()
                }
                IndexLifecycleState::Maintaining => {
                    lifecycle.mark_building()?;
                    lifecycle.mark_validating()?;
                    lifecycle.mark_ready()?;
                    lifecycle.mark_active()?;
                    lifecycle.mark_maintaining()
                }
                IndexLifecycleState::Retiring | IndexLifecycleState::Retired => unreachable!(),
            }
        }

        for state in [
            IndexLifecycleState::Creating,
            IndexLifecycleState::Building,
            IndexLifecycleState::Validating,
            IndexLifecycleState::Ready,
        ] {
            let mut lifecycle = IndexLifecycle::new(definition_identity(0x33));

            advance_to(&mut lifecycle, state)
                .unwrap_or_else(|error| panic!("cannot reach {state:?}: {error}"));

            lifecycle
                .mark_retiring()
                .unwrap_or_else(|error| panic!("{state:?} → Retiring should work: {error}"));
            assert_eq!(lifecycle.state(), IndexLifecycleState::Retiring);

            lifecycle
                .mark_retired()
                .expect("Retiring → Retired should work");
        }
    }

    #[test]
    fn maintaining_can_return_to_active() {
        let mut lifecycle = IndexLifecycle::new(definition_identity(0x44));
        lifecycle
            .mark_building()
            .expect("Creating → Building should work");
        lifecycle
            .mark_validating()
            .expect("Building → Validating should work");
        lifecycle
            .mark_ready()
            .expect("Validating → Ready should work");
        lifecycle.mark_active().expect("Ready → Active should work");
        lifecycle
            .mark_maintaining()
            .expect("Active → Maintaining should work");
        lifecycle
            .mark_active()
            .expect("Maintaining → Active should work");

        assert_eq!(lifecycle.state(), IndexLifecycleState::Active);
    }

    #[test]
    fn same_state_transition_is_a_no_op() {
        let mut lifecycle = IndexLifecycle::new(definition_identity(0x55));

        for state in IndexLifecycleState::all() {
            lifecycle
                .transition_to(state)
                .unwrap_or_else(|error| panic!("cannot reach {state:?}: {error}"));
            lifecycle.transition_to(state).unwrap_or_else(|error| {
                panic!("same-state transition for {state:?} failed: {error}")
            });
            assert_eq!(lifecycle.state(), state);
        }
    }

    #[test]
    fn retired_is_terminal() {
        let mut lifecycle = IndexLifecycle::new(definition_identity(0x66));
        lifecycle
            .transition_to(IndexLifecycleState::Retiring)
            .expect("Creating → Retiring should work");
        lifecycle
            .transition_to(IndexLifecycleState::Retired)
            .expect("Retiring → Retired should work");

        for next in IndexLifecycleState::all() {
            if next == IndexLifecycleState::Retired {
                assert!(lifecycle.transition_to(next).is_ok());
            } else {
                let error = lifecycle
                    .transition_to(next)
                    .expect_err("Retired must not transition to another state");
                assert_eq!(error.from(), IndexLifecycleState::Retired);
                assert_eq!(error.to(), next);
            }
        }
    }

    #[test]
    fn invalid_transitions_are_rejected() {
        let invalid = [
            (
                IndexLifecycleState::Creating,
                IndexLifecycleState::Validating,
            ),
            (IndexLifecycleState::Creating, IndexLifecycleState::Ready),
            (IndexLifecycleState::Creating, IndexLifecycleState::Active),
            (
                IndexLifecycleState::Creating,
                IndexLifecycleState::Maintaining,
            ),
            (IndexLifecycleState::Creating, IndexLifecycleState::Retired),
            (IndexLifecycleState::Building, IndexLifecycleState::Ready),
            (IndexLifecycleState::Building, IndexLifecycleState::Active),
            (
                IndexLifecycleState::Building,
                IndexLifecycleState::Maintaining,
            ),
            (IndexLifecycleState::Building, IndexLifecycleState::Retired),
            (
                IndexLifecycleState::Validating,
                IndexLifecycleState::Building,
            ),
            (IndexLifecycleState::Validating, IndexLifecycleState::Active),
            (
                IndexLifecycleState::Validating,
                IndexLifecycleState::Maintaining,
            ),
            (
                IndexLifecycleState::Validating,
                IndexLifecycleState::Retired,
            ),
            (IndexLifecycleState::Ready, IndexLifecycleState::Building),
            (IndexLifecycleState::Ready, IndexLifecycleState::Validating),
            (IndexLifecycleState::Ready, IndexLifecycleState::Maintaining),
            (IndexLifecycleState::Ready, IndexLifecycleState::Retired),
            (IndexLifecycleState::Active, IndexLifecycleState::Building),
            (IndexLifecycleState::Active, IndexLifecycleState::Validating),
            (IndexLifecycleState::Active, IndexLifecycleState::Ready),
            (IndexLifecycleState::Active, IndexLifecycleState::Retired),
            (
                IndexLifecycleState::Maintaining,
                IndexLifecycleState::Creating,
            ),
            (
                IndexLifecycleState::Maintaining,
                IndexLifecycleState::Building,
            ),
            (
                IndexLifecycleState::Maintaining,
                IndexLifecycleState::Validating,
            ),
            (IndexLifecycleState::Maintaining, IndexLifecycleState::Ready),
            (
                IndexLifecycleState::Maintaining,
                IndexLifecycleState::Retired,
            ),
            (IndexLifecycleState::Retiring, IndexLifecycleState::Creating),
            (IndexLifecycleState::Retiring, IndexLifecycleState::Building),
            (
                IndexLifecycleState::Retiring,
                IndexLifecycleState::Validating,
            ),
            (IndexLifecycleState::Retiring, IndexLifecycleState::Ready),
            (IndexLifecycleState::Retiring, IndexLifecycleState::Active),
            (
                IndexLifecycleState::Retiring,
                IndexLifecycleState::Maintaining,
            ),
        ];

        for (from, to) in invalid {
            assert!(
                !from.can_transition_to(to),
                "{from:?} → {to:?} must be rejected"
            );
        }
    }

    #[test]
    fn full_transition_matrix_matches_the_guarded_graph() {
        let allowed = [
            (IndexLifecycleState::Creating, IndexLifecycleState::Building),
            (
                IndexLifecycleState::Building,
                IndexLifecycleState::Validating,
            ),
            (IndexLifecycleState::Validating, IndexLifecycleState::Ready),
            (IndexLifecycleState::Ready, IndexLifecycleState::Active),
            (
                IndexLifecycleState::Active,
                IndexLifecycleState::Maintaining,
            ),
            (
                IndexLifecycleState::Maintaining,
                IndexLifecycleState::Active,
            ),
            (IndexLifecycleState::Creating, IndexLifecycleState::Retiring),
            (IndexLifecycleState::Building, IndexLifecycleState::Retiring),
            (
                IndexLifecycleState::Validating,
                IndexLifecycleState::Retiring,
            ),
            (IndexLifecycleState::Ready, IndexLifecycleState::Retiring),
            (IndexLifecycleState::Active, IndexLifecycleState::Retiring),
            (
                IndexLifecycleState::Maintaining,
                IndexLifecycleState::Retiring,
            ),
            (IndexLifecycleState::Retiring, IndexLifecycleState::Retired),
        ];

        for from in IndexLifecycleState::all() {
            for to in IndexLifecycleState::all() {
                let expected = from == to || allowed.contains(&(from, to));
                assert_eq!(
                    from.can_transition_to(to),
                    expected,
                    "unexpected transition guard for {from:?} → {to:?}"
                );
            }
        }
    }

    #[test]
    fn transition_errors_preserve_identity_and_states() {
        let id = definition_identity(0x77);
        let mut lifecycle = IndexLifecycle::new(id.clone());

        let error = lifecycle
            .transition_to(IndexLifecycleState::Active)
            .expect_err("Creating must not bypass the lifecycle boundary");

        assert_eq!(error.definition_identity(), &id);
        assert_eq!(error.from(), IndexLifecycleState::Creating);
        assert_eq!(error.to(), IndexLifecycleState::Active);
        assert!(error.to_string().contains("invalid lifecycle transition"));
        assert_eq!(lifecycle.state(), IndexLifecycleState::Creating);
    }

    #[test]
    fn into_parts_preserves_identity_and_state() {
        let id = definition_identity(0x88);
        let mut lifecycle = IndexLifecycle::new(id.clone());
        lifecycle
            .mark_building()
            .expect("Creating → Building should work");
        lifecycle
            .mark_validating()
            .expect("Building → Validating should work");

        let (returned_identity, state) = lifecycle.into_parts();

        assert_eq!(returned_identity, id);
        assert_eq!(state, IndexLifecycleState::Validating);
    }
}
