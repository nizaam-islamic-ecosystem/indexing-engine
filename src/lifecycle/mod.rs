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
