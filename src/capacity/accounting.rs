//! Runtime capacity accounting for the Nizaam Indexing Engine.
//!
//! This module answers the question:
//!
//! ```text
//! What is currently being consumed?
//! ```
//!
//! It is deliberately separate from [`super::limits`], which answers:
//!
//! ```text
//! What is allowed?
//! ```
//!
//! The accounting model is the approved Phase 5 hybrid model:
//!
//! ```text
//! hard counts
//!     → active queries
//!     → active builds
//!     → active maintenance operations
//!     → active recovery operations
//!
//! logical units
//!     → total units currently reserved
//!     → per-operation unit ceiling enforced during admission
//! ```
//!
//! The accounting layer also keeps a bounded count of operations that have
//! registered as waiting for capacity. It does **not** implement a scheduler,
//! an unbounded queue, retry policy, or independent cancellation/deadline
//! machinery. Higher layers may combine this bounded accounting with Core's
//! [`OperationContext`](nizaam_core::operation::OperationContext), cancellation,
//! and deadline infrastructure.
//!
//! Admission is intentionally transactional: concurrency and logical-unit
//! checks are evaluated while holding the accounting lock and the counters are
//! changed only when every requested bound passes. Failed admission therefore
//! leaves the accounting state unchanged.

use super::limits::{CapacityLimits, CapacityOperation};
use core::fmt;
use std::error::Error;
use std::sync::{Arc, Mutex, MutexGuard};

/// A logical request for capacity admission.
///
/// `capacity_units` is an Indexing-owned logical estimate. It does not
/// represent bytes, CPU time, database pages, memory, or any provider-specific
/// physical resource quantity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CapacityRequest {
    operation: CapacityOperation,
    capacity_units: usize,
    batch_size: Option<usize>,
}

impl CapacityRequest {
    /// Creates a capacity request without a batch-size constraint.
    #[must_use]
    pub const fn new(operation: CapacityOperation, capacity_units: usize) -> Self {
        Self {
            operation,
            capacity_units,
            batch_size: None,
        }
    }

    /// Creates a capacity request with an associated logical batch size.
    #[must_use]
    pub const fn with_batch_size(
        operation: CapacityOperation,
        capacity_units: usize,
        batch_size: usize,
    ) -> Self {
        Self {
            operation,
            capacity_units,
            batch_size: Some(batch_size),
        }
    }

    /// Returns the workload class being admitted.
    #[must_use]
    pub const fn operation(self) -> CapacityOperation {
        self.operation
    }

    /// Returns the logical capacity-unit cost requested by the operation.
    #[must_use]
    pub const fn capacity_units(self) -> usize {
        self.capacity_units
    }

    /// Returns the optional logical batch size associated with this request.
    #[must_use]
    pub const fn batch_size(self) -> Option<usize> {
        self.batch_size
    }
}

/// A point-in-time view of the capacity currently accounted as consumed.
///
/// The snapshot is a value copy and therefore does not keep the accounting
/// lock alive after it has been obtained.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CapacityUsageSnapshot {
    active_queries: usize,
    active_builds: usize,
    active_maintenance: usize,
    active_recoveries: usize,
    consumed_capacity_units: usize,
    pending_capacity_waiters: usize,
}

impl CapacityUsageSnapshot {
    /// Returns the number of currently admitted queries.
    #[must_use]
    pub const fn active_queries(self) -> usize {
        self.active_queries
    }

    /// Returns the number of currently admitted builds.
    #[must_use]
    pub const fn active_builds(self) -> usize {
        self.active_builds
    }

    /// Returns the number of currently admitted maintenance operations.
    #[must_use]
    pub const fn active_maintenance(self) -> usize {
        self.active_maintenance
    }

    /// Returns the number of currently admitted recovery operations.
    #[must_use]
    pub const fn active_recoveries(self) -> usize {
        self.active_recoveries
    }

    /// Returns the logical capacity units currently reserved by admitted
    /// operations.
    #[must_use]
    pub const fn consumed_capacity_units(self) -> usize {
        self.consumed_capacity_units
    }

    /// Returns the number of operations currently registered as waiting for
    /// capacity.
    #[must_use]
    pub const fn pending_capacity_waiters(self) -> usize {
        self.pending_capacity_waiters
    }

    /// Returns the total number of currently admitted operations.
    #[must_use]
    pub const fn active_operations(self) -> usize {
        self.active_queries + self.active_builds + self.active_maintenance + self.active_recoveries
    }
}

/// Current capacity accounting for one Indexing Engine instance.
///
/// The accounting state is local to this value. It is not a global scheduler,
/// resource manager, or process-wide quota registry. Cloning the accounting
/// value shares the same local counters, which is useful when multiple local
/// components participate in admission for the same hosted Indexing instance.
#[derive(Clone, Debug)]
pub struct CapacityAccounting {
    limits: CapacityLimits,
    usage: Arc<Mutex<CapacityUsageSnapshot>>,
}

impl CapacityAccounting {
    /// Creates empty accounting governed by the supplied logical limits.
    #[must_use]
    pub fn new(limits: CapacityLimits) -> Self {
        Self {
            limits,
            usage: Arc::new(Mutex::new(CapacityUsageSnapshot::default())),
        }
    }

    /// Creates empty accounting directly from a validated configuration.
    #[must_use]
    pub fn from_configuration(configuration: crate::configuration::IndexingConfiguration) -> Self {
        Self::new(CapacityLimits::from_configuration(configuration))
    }

    /// Returns the immutable limits used for admission decisions.
    #[must_use]
    pub const fn limits(&self) -> CapacityLimits {
        self.limits
    }

    /// Returns a value snapshot of the current usage.
    #[must_use]
    pub fn usage(&self) -> CapacityUsageSnapshot {
        *self.lock_usage()
    }

    /// Attempts to admit one operation and its requested logical capacity.
    ///
    /// Admission succeeds only when all requested limits can be satisfied at
    /// the same time:
    ///
    /// 1. the batch size is within its configured bound, when supplied;
    /// 2. the operation's logical-unit request fits its per-operation ceiling;
    /// 3. another operation can fit the workload-class concurrency bound; and
    /// 4. the requested units fit the remaining aggregate budget.
    ///
    /// On success, the returned [`CapacityLease`] owns the reservation. Dropping
    /// the lease releases the same counts and logical units automatically.
    pub fn try_acquire(
        &self,
        request: CapacityRequest,
    ) -> Result<CapacityLease, CapacityAdmissionError> {
        if let Some(batch_size) = request.batch_size
            && !self.limits.allows_batch_size(batch_size)
        {
            return Err(CapacityAdmissionError::BatchSizeExceeded {
                requested: batch_size,
                limit: self.limits.max_batch_size().get(),
            });
        }

        if !self
            .limits
            .allows_operation_capacity(request.capacity_units)
        {
            return Err(CapacityAdmissionError::OperationCapacityExceeded {
                requested: request.capacity_units,
                limit: self.limits.max_operation_capacity_units().get(),
            });
        }

        let mut usage = self.lock_usage();
        let current_count = current_count(*usage, request.operation);
        let concurrency_limit = self.limits.max_concurrent(request.operation).get();

        if current_count >= concurrency_limit {
            return Err(CapacityAdmissionError::ConcurrencyLimitReached {
                operation: request.operation,
                current: current_count,
                limit: concurrency_limit,
            });
        }

        let remaining_units = self
            .limits
            .max_capacity_units()
            .get()
            .saturating_sub(usage.consumed_capacity_units);

        if request.capacity_units > remaining_units {
            return Err(CapacityAdmissionError::TotalCapacityExceeded {
                requested: request.capacity_units,
                consumed: usage.consumed_capacity_units,
                limit: self.limits.max_capacity_units().get(),
            });
        }

        increment_count(&mut usage, request.operation);
        usage.consumed_capacity_units += request.capacity_units;

        Ok(CapacityLease {
            operation: request.operation,
            capacity_units: request.capacity_units,
            usage: Arc::clone(&self.usage),
            released: false,
        })
    }

    /// Attempts to register one operation as waiting for capacity.
    ///
    /// This method only accounts for the bounded waiter count. It does not
    /// block, sleep, enqueue work, or decide how the caller should wait. Those
    /// concerns remain outside this accounting layer.
    pub fn try_register_waiter(&self) -> Result<CapacityWaiterGuard, CapacityWaiterError> {
        let mut usage = self.lock_usage();
        let current = usage.pending_capacity_waiters;
        let limit = self.limits.max_pending_capacity_waiters().get();

        if current >= limit {
            return Err(CapacityWaiterError::LimitReached { current, limit });
        }

        usage.pending_capacity_waiters += 1;

        Ok(CapacityWaiterGuard {
            usage: Arc::clone(&self.usage),
            released: false,
        })
    }

    fn lock_usage(&self) -> MutexGuard<'_, CapacityUsageSnapshot> {
        match self.usage.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }
}

/// An admitted capacity reservation.
///
/// The lease is RAII-based: dropping it releases the exact operation count and
/// logical-unit reservation that were admitted together. This keeps accounting
/// balanced even when execution exits early because of an error or cancellation
/// at a higher layer.
pub struct CapacityLease {
    operation: CapacityOperation,
    capacity_units: usize,
    usage: Arc<Mutex<CapacityUsageSnapshot>>,
    released: bool,
}

impl CapacityLease {
    /// Returns the workload class represented by this reservation.
    #[must_use]
    pub const fn operation(&self) -> CapacityOperation {
        self.operation
    }

    /// Returns the logical capacity units reserved by this lease.
    #[must_use]
    pub const fn capacity_units(&self) -> usize {
        self.capacity_units
    }

    /// Releases the reservation explicitly.
    ///
    /// The operation is idempotent. Dropping an already released lease is a
    /// no-op.
    pub fn release(&mut self) {
        if self.released {
            return;
        }

        let mut usage = match self.usage.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };

        decrement_count(&mut usage, self.operation);
        usage.consumed_capacity_units = usage
            .consumed_capacity_units
            .saturating_sub(self.capacity_units);
        self.released = true;
    }
}

impl Drop for CapacityLease {
    fn drop(&mut self) {
        self.release();
    }
}

impl fmt::Debug for CapacityLease {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CapacityLease")
            .field("operation", &self.operation)
            .field("capacity_units", &self.capacity_units)
            .field("released", &self.released)
            .finish()
    }
}

/// A bounded registration representing one operation waiting for capacity.
///
/// The guard only tracks the waiter's existence. It does not store the pending
/// operation and therefore cannot become an unbounded local queue.
pub struct CapacityWaiterGuard {
    usage: Arc<Mutex<CapacityUsageSnapshot>>,
    released: bool,
}

impl CapacityWaiterGuard {
    /// Releases the waiter registration explicitly.
    ///
    /// The operation is idempotent. Dropping an already released guard is a
    /// no-op.
    pub fn release(&mut self) {
        if self.released {
            return;
        }

        let mut usage = match self.usage.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };

        usage.pending_capacity_waiters = usage.pending_capacity_waiters.saturating_sub(1);
        self.released = true;
    }
}

impl Drop for CapacityWaiterGuard {
    fn drop(&mut self) {
        self.release();
    }
}

impl fmt::Debug for CapacityWaiterGuard {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CapacityWaiterGuard")
            .field("released", &self.released)
            .finish()
    }
}

/// Reason an operation was rejected by the current capacity accounting state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CapacityAdmissionError {
    /// The request's logical batch exceeds the configured batch bound.
    BatchSizeExceeded { requested: usize, limit: usize },

    /// The operation's logical capacity-unit request exceeds its per-operation
    /// ceiling.
    OperationCapacityExceeded { requested: usize, limit: usize },

    /// The workload class has reached its concurrent-admission limit.
    ConcurrencyLimitReached {
        operation: CapacityOperation,
        current: usize,
        limit: usize,
    },

    /// The operation would exceed the aggregate logical capacity budget.
    TotalCapacityExceeded {
        requested: usize,
        consumed: usize,
        limit: usize,
    },
}

impl fmt::Display for CapacityAdmissionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BatchSizeExceeded { requested, limit } => write!(
                formatter,
                "capacity admission rejected batch size {requested}; limit is {limit}"
            ),
            Self::OperationCapacityExceeded { requested, limit } => write!(
                formatter,
                "capacity admission rejected operation capacity {requested}; per-operation limit is {limit}"
            ),
            Self::ConcurrencyLimitReached {
                operation,
                current,
                limit,
            } => write!(
                formatter,
                "capacity admission rejected {operation:?}; current concurrency {current} reached limit {limit}"
            ),
            Self::TotalCapacityExceeded {
                requested,
                consumed,
                limit,
            } => write!(
                formatter,
                "capacity admission rejected {requested} units with {consumed} already consumed; total limit is {limit}"
            ),
        }
    }
}

impl Error for CapacityAdmissionError {}

/// Reason a capacity waiter could not be registered.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CapacityWaiterError {
    /// The bounded waiter registration limit has been reached.
    LimitReached { current: usize, limit: usize },
}

impl fmt::Display for CapacityWaiterError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LimitReached { current, limit } => write!(
                formatter,
                "capacity waiter limit reached: current {current}, limit {limit}"
            ),
        }
    }
}

impl Error for CapacityWaiterError {}

fn current_count(usage: CapacityUsageSnapshot, operation: CapacityOperation) -> usize {
    match operation {
        CapacityOperation::Query => usage.active_queries,
        CapacityOperation::Build => usage.active_builds,
        CapacityOperation::Maintenance => usage.active_maintenance,
        CapacityOperation::Recovery => usage.active_recoveries,
    }
}

fn increment_count(usage: &mut CapacityUsageSnapshot, operation: CapacityOperation) {
    match operation {
        CapacityOperation::Query => usage.active_queries += 1,
        CapacityOperation::Build => usage.active_builds += 1,
        CapacityOperation::Maintenance => usage.active_maintenance += 1,
        CapacityOperation::Recovery => usage.active_recoveries += 1,
    }
}

fn decrement_count(usage: &mut CapacityUsageSnapshot, operation: CapacityOperation) {
    match operation {
        CapacityOperation::Query => usage.active_queries = usage.active_queries.saturating_sub(1),
        CapacityOperation::Build => usage.active_builds = usage.active_builds.saturating_sub(1),
        CapacityOperation::Maintenance => {
            usage.active_maintenance = usage.active_maintenance.saturating_sub(1)
        }
        CapacityOperation::Recovery => {
            usage.active_recoveries = usage.active_recoveries.saturating_sub(1)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::configuration::IndexingConfiguration;

    fn accounting() -> CapacityAccounting {
        CapacityAccounting::from_configuration(
            IndexingConfiguration::new(2, 1, 1, 1, 2, 4, 8, 4)
                .expect("test configuration should be valid"),
        )
    }

    #[test]
    fn new_accounting_starts_empty() {
        assert_eq!(accounting().usage(), CapacityUsageSnapshot::default());
    }

    #[test]
    fn successful_admission_updates_hard_counts_and_logical_units() {
        let accounting = accounting();

        let lease = accounting
            .try_acquire(CapacityRequest::new(CapacityOperation::Query, 3))
            .expect("query admission should succeed");

        assert_eq!(lease.operation(), CapacityOperation::Query);
        assert_eq!(lease.capacity_units(), 3);
        assert_eq!(accounting.usage().active_queries(), 1);
        assert_eq!(accounting.usage().consumed_capacity_units(), 3);
        assert_eq!(accounting.usage().active_operations(), 1);
    }

    #[test]
    fn lease_drop_releases_exact_reservation() {
        let accounting = accounting();

        {
            let _lease = accounting
                .try_acquire(CapacityRequest::new(CapacityOperation::Build, 4))
                .expect("build admission should succeed");

            assert_eq!(accounting.usage().active_builds(), 1);
            assert_eq!(accounting.usage().consumed_capacity_units(), 4);
        }

        assert_eq!(accounting.usage(), CapacityUsageSnapshot::default());
    }

    #[test]
    fn explicit_lease_release_is_idempotent() {
        let accounting = accounting();
        let mut lease = accounting
            .try_acquire(CapacityRequest::new(CapacityOperation::Maintenance, 2))
            .expect("maintenance admission should succeed");

        lease.release();
        lease.release();

        assert_eq!(accounting.usage(), CapacityUsageSnapshot::default());
    }

    #[test]
    fn concurrency_limit_is_enforced_without_mutating_usage_on_failure() {
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

    #[test]
    fn per_operation_capacity_is_enforced_before_accounting_changes() {
        let accounting = accounting();

        let result = accounting.try_acquire(CapacityRequest::new(CapacityOperation::Query, 5));

        assert!(matches!(
            result,
            Err(CapacityAdmissionError::OperationCapacityExceeded {
                requested: 5,
                limit: 4,
            })
        ));
        assert_eq!(accounting.usage(), CapacityUsageSnapshot::default());
    }

    #[test]
    fn total_capacity_budget_is_enforced_against_current_consumption() {
        let accounting = accounting();
        let _first = accounting
            .try_acquire(CapacityRequest::new(CapacityOperation::Query, 4))
            .expect("first admission should succeed");

        let _second = accounting
            .try_acquire(CapacityRequest::new(CapacityOperation::Build, 4))
            .expect("admission that exactly fills the total budget should succeed");

        let third = accounting.try_acquire(CapacityRequest::new(CapacityOperation::Query, 1));

        assert!(matches!(
            third,
            Err(CapacityAdmissionError::TotalCapacityExceeded {
                requested: 1,
                consumed: 8,
                limit: 8,
            })
        ));
        assert_eq!(accounting.usage().active_queries(), 1);
        assert_eq!(accounting.usage().active_builds(), 1);
        assert_eq!(accounting.usage().consumed_capacity_units(), 8);
    }

    #[test]
    fn batch_size_limit_is_enforced_as_part_of_admission() {
        let accounting = accounting();

        let result = accounting.try_acquire(CapacityRequest::with_batch_size(
            CapacityOperation::Build,
            1,
            5,
        ));

        assert!(matches!(
            result,
            Err(CapacityAdmissionError::BatchSizeExceeded {
                requested: 5,
                limit: 4,
            })
        ));
        assert_eq!(accounting.usage(), CapacityUsageSnapshot::default());
    }

    #[test]
    fn zero_capacity_units_are_accounted_without_special_cases() {
        let accounting = accounting();
        let _lease = accounting
            .try_acquire(CapacityRequest::new(CapacityOperation::Recovery, 0))
            .expect("zero logical units are within the configured limit");

        assert_eq!(accounting.usage().active_recoveries(), 1);
        assert_eq!(accounting.usage().consumed_capacity_units(), 0);
    }

    #[test]
    fn waiter_registration_is_bounded_and_released_by_guard_drop() {
        let accounting = accounting();
        let first = accounting
            .try_register_waiter()
            .expect("first waiter should be registered");
        let second = accounting
            .try_register_waiter()
            .expect("second waiter should be registered");

        assert_eq!(accounting.usage().pending_capacity_waiters(), 2);

        let third = accounting.try_register_waiter();
        assert!(matches!(
            third,
            Err(CapacityWaiterError::LimitReached {
                current: 2,
                limit: 2,
            })
        ));
        assert_eq!(accounting.usage().pending_capacity_waiters(), 2);

        drop(first);
        assert_eq!(accounting.usage().pending_capacity_waiters(), 1);

        drop(second);
        assert_eq!(accounting.usage().pending_capacity_waiters(), 0);
    }

    #[test]
    fn waiter_explicit_release_is_idempotent() {
        let accounting = accounting();
        let mut waiter = accounting
            .try_register_waiter()
            .expect("waiter should be registered");

        waiter.release();
        waiter.release();

        assert_eq!(accounting.usage().pending_capacity_waiters(), 0);
    }

    #[test]
    fn cloned_accounting_handles_share_the_same_local_state() {
        let first = accounting();
        let second = first.clone();
        let _lease = second
            .try_acquire(CapacityRequest::new(CapacityOperation::Query, 2))
            .expect("admission through clone should succeed");

        assert_eq!(first.usage().active_queries(), 1);
        assert_eq!(first.usage().consumed_capacity_units(), 2);
    }
}
