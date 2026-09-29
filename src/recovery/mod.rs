//! Phase 5 recovery boundary for the Indexing Engine.
//!
//! This module is intentionally a thin composition facade over the two recovery
//! concerns:
//!
//! ```text
//! failure.rs
//!     -> classify Indexing failures
//!     -> expose Core retryability hint
//!             |
//!             v
//! execution.rs
//!     -> select deterministic recovery action
//!     -> execute one local recovery operation
//!             |
//!             v
//! Phase 3 validate -> publish
//! ```
//!
//! The recovery module does not create a second retry system, scheduler,
//! runtime, active-version registry, or publication mechanism. Core remains the
//! owner of retry policy and execution infrastructure, while Phase 3 remains
//! the owner of candidate construction, validation, and publication.

pub mod execution;
pub mod failure;

pub use failure::{ClassifiedFailure, FailureClass, FailureClassifier, classify, retryability};

pub use execution::{
    RecoveryAction, RecoveryExecutionError, RecoveryExecutor, RecoveryHandler, RecoveryOutcome,
    RecoveryRequest, action_for, execute_recovery,
};
