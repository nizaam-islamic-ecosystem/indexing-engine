//! Public lifecycle boundary for Phase 5 index lifecycle management.
//!
//! The lifecycle module owns the lifecycle of an individual logical Indexing
//! index. It intentionally remains separate from the Core engine lifecycle and
//! from the operational dimensions introduced by the rest of Phase 5, such as
//! integrity, synchronization, availability, capacity pressure, failure
//! classification, and recovery.
//!
//! The child [`state`] module contains the actual lifecycle state machine and
//! guarded transition rules. This module is only the composition and public
//! export boundary; it does not implement another lifecycle system or keep a
//! global registry of index states.
//!
//! The architectural separation is:
//!
//! ```text
//! Core
//!     → engine lifecycle / runtime
//!
//! Indexing lifecycle
//!     → lifecycle of an individual logical index
//!
//! Other Phase 5 modules
//!     → capacity / integrity / failure / recovery / configuration
//! ```
//!
//! Operational conditions such as `Stale`, `Unavailable`, `Corrupt`, and
//! `Failed` are deliberately not lifecycle states here. They belong to their
//! respective operational boundaries.

/// Index-specific lifecycle state and guarded transition implementation.
pub mod state;

// -----------------------------------------------------------------------------
// Public lifecycle surface
// -----------------------------------------------------------------------------

pub use state::{IndexLifecycle, IndexLifecycleState, IndexLifecycleTransitionError};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::{INDEX_ID_BYTE_LEN, IndexId};

    fn index_id(seed: u8) -> IndexId {
        IndexId::from_bytes([seed; INDEX_ID_BYTE_LEN])
    }

    #[test]
    fn lifecycle_module_exposes_the_canonical_state_machine() {
        let lifecycle = IndexLifecycle::new(index_id(0x11));

        assert_eq!(lifecycle.index_id(), index_id(0x11));
        assert_eq!(lifecycle.state(), IndexLifecycleState::Creating);
        assert!(IndexLifecycleState::Creating.can_transition_to(IndexLifecycleState::Building));
        assert!(!IndexLifecycleState::Creating.can_transition_to(IndexLifecycleState::Active));
    }

    #[test]
    fn lifecycle_module_preserves_guarded_transition_boundary() {
        let mut lifecycle = IndexLifecycle::new(index_id(0x22));

        lifecycle
            .transition_to(IndexLifecycleState::Building)
            .expect("Creating → Building should be valid");
        lifecycle
            .transition_to(IndexLifecycleState::Validating)
            .expect("Building → Validating should be valid");
        lifecycle
            .transition_to(IndexLifecycleState::Ready)
            .expect("Validating → Ready should be valid");

        let error = lifecycle
            .transition_to(IndexLifecycleState::Retired)
            .expect_err("Ready must not bypass Retiring");

        assert_eq!(error.index_id(), index_id(0x22));
        assert_eq!(error.from(), IndexLifecycleState::Ready);
        assert_eq!(error.to(), IndexLifecycleState::Retired);
        assert_eq!(lifecycle.state(), IndexLifecycleState::Ready);
    }

    #[test]
    fn lifecycle_module_keeps_retired_terminal() {
        let mut lifecycle = IndexLifecycle::new(index_id(0x33));

        lifecycle
            .transition_to(IndexLifecycleState::Retiring)
            .expect("Creating → Retiring should be valid");
        lifecycle
            .transition_to(IndexLifecycleState::Retired)
            .expect("Retiring → Retired should be valid");

        assert!(lifecycle.is_terminal());

        let error = lifecycle
            .transition_to(IndexLifecycleState::Building)
            .expect_err("Retired must remain terminal");

        assert_eq!(error.from(), IndexLifecycleState::Retired);
        assert_eq!(error.to(), IndexLifecycleState::Building);
        assert_eq!(lifecycle.state(), IndexLifecycleState::Retired);
    }
}
