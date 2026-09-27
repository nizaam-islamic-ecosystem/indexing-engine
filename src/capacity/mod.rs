//! Public capacity boundary for the Nizaam Indexing Engine.
//!
//! Phase 5 deliberately separates two different questions:
//!
//! ```text
//! limits.rs
//!     → What is allowed?
//!
//! accounting.rs
//!     → What is currently being consumed?
//! ```
//!
//! [`limits`] owns the validated logical workload bounds derived from the
//! Indexing configuration. [`accounting`] owns local runtime consumption and
//! bounded waiter registration. Keeping these concerns separate prevents the
//! capacity layer from becoming a scheduler, queue manager, retry system, or
//! physical resource manager.
//!
//! The public surface below intentionally re-exports the canonical capacity
//! contracts so callers can use `crate::capacity::*` without depending on the
//! internal child-module layout.
//!
//! The capacity boundary is Indexing-local. Core remains responsible for the
//! engine runtime, request admission, cancellation, deadlines, retry
//! infrastructure, and configuration snapshot/update machinery.

/// Allowed logical capacity bounds for one hosted Indexing instance.
pub mod limits;

/// Current local capacity consumption and reservation accounting.
pub mod accounting;

// -----------------------------------------------------------------------------
// Canonical limit surface
// -----------------------------------------------------------------------------

pub use limits::{CapacityLimits, CapacityOperation};

// -----------------------------------------------------------------------------
// Canonical accounting surface
// -----------------------------------------------------------------------------

pub use accounting::{
    CapacityAccounting, CapacityAdmissionError, CapacityLease, CapacityRequest,
    CapacityUsageSnapshot, CapacityWaiterError, CapacityWaiterGuard,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::configuration::IndexingConfiguration;

    fn configuration() -> IndexingConfiguration {
        IndexingConfiguration::new(2, 1, 1, 1, 2, 4, 8, 4)
            .expect("test configuration should be valid")
    }

    fn accounting() -> CapacityAccounting {
        CapacityAccounting::from_configuration(configuration())
    }

    #[test]
    fn capacity_module_exposes_the_canonical_limit_surface() {
        let limits = CapacityLimits::from_configuration(configuration());

        assert_eq!(limits.max_concurrent(CapacityOperation::Query).get(), 2);
        assert_eq!(limits.max_concurrent(CapacityOperation::Build).get(), 1);
        assert_eq!(limits.max_batch_size().get(), 4);
        assert_eq!(limits.max_capacity_units().get(), 8);
        assert_eq!(limits.max_operation_capacity_units().get(), 4);
    }

    #[test]
    fn capacity_module_exposes_the_canonical_accounting_surface() {
        let accounting = accounting();
        let lease = accounting
            .try_acquire(CapacityRequest::new(CapacityOperation::Query, 2))
            .expect("query admission should succeed");

        assert_eq!(lease.operation(), CapacityOperation::Query);
        assert_eq!(lease.capacity_units(), 2);
        assert_eq!(accounting.usage().active_queries(), 1);
        assert_eq!(accounting.usage().consumed_capacity_units(), 2);
    }

    #[test]
    fn capacity_module_preserves_bounded_waiter_accounting() {
        let accounting = accounting();
        let first = accounting
            .try_register_waiter()
            .expect("first waiter should be accepted");
        let second = accounting
            .try_register_waiter()
            .expect("second waiter should be accepted");

        assert_eq!(accounting.usage().pending_capacity_waiters(), 2);

        let third = accounting.try_register_waiter();
        assert!(matches!(
            third,
            Err(CapacityWaiterError::LimitReached {
                current: 2,
                limit: 2,
            })
        ));

        drop(first);
        drop(second);

        assert_eq!(accounting.usage().pending_capacity_waiters(), 0);
    }

    #[test]
    fn capacity_module_keeps_failed_admission_transactional() {
        let accounting = accounting();
        let _first = accounting
            .try_acquire(CapacityRequest::new(CapacityOperation::Build, 1))
            .expect("first build should succeed");

        let second = accounting.try_acquire(CapacityRequest::new(CapacityOperation::Build, 1));

        assert!(matches!(
            second,
            Err(CapacityAdmissionError::ConcurrencyLimitReached {
                operation: CapacityOperation::Build,
                current: 1,
                limit: 1,
            })
        ));
        assert_eq!(accounting.usage().active_builds(), 1);
        assert_eq!(accounting.usage().consumed_capacity_units(), 1);
    }
}
