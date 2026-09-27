//! Capacity limits for the Nizaam Indexing Engine.
//!
//! This module owns the *allowed* workload bounds used by Phase 5 capacity
//! handling. It does not account for current consumption, waiters, queues, or
//! execution state; those concerns belong to [`super::accounting`].
//!
//! The capacity model is intentionally hybrid:
//!
//! ```text
//! hard-count limits
//!     → concurrent queries/builds/maintenance/recovery
//!
//! logical-unit limits
//!     → total operation capacity + per-operation capacity
//! ```
//!
//! A separate batch-size limit protects bounded logical mutation batches, and
//! a separate waiter limit provides the upper bound needed by the later
//! bounded-throttling layer. No limit describes CPU, RAM, disk, database
//! connections, storage pages, or any other physical resource.
//!
//! Numerical values are supplied by [`IndexingConfiguration`]. This module does
//! not invent production defaults. A new configuration snapshot therefore
//! produces a new [`CapacityLimits`] value, which keeps hot configuration
//! replacement value-oriented and does not create another configuration
//! manager inside Indexing.

use crate::configuration::IndexingConfiguration;
use core::num::NonZeroUsize;

/// Logical class of Indexing work for which concurrent capacity is bounded.
///
/// These categories correspond to the distinct operational pressures that
/// Phase 5 needs to protect. They are workload classifications only; they do
/// not introduce a scheduler or execution mechanism.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum CapacityOperation {
    /// Read/retrieval workload.
    Query,

    /// Index construction/update/rebuild workload.
    Build,

    /// Non-build index maintenance workload.
    Maintenance,

    /// Local failure-recovery workload.
    Recovery,
}

impl CapacityOperation {
    /// Returns all supported bounded workload classes in stable order.
    #[must_use]
    pub const fn all() -> [Self; 4] {
        [Self::Query, Self::Build, Self::Maintenance, Self::Recovery]
    }
}

/// Validated logical capacity limits for one Indexing Engine instance.
///
/// `CapacityLimits` describes what the Indexing Engine is allowed to admit; it
/// does not describe what is currently consumed. Current consumption belongs
/// to the accounting layer.
///
/// All values come from a validated [`IndexingConfiguration`]. The structure
/// is immutable and copyable so a caller can safely replace the complete
/// capacity view when Core activates a new configuration snapshot.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CapacityLimits {
    max_concurrent_queries: NonZeroUsize,
    max_concurrent_builds: NonZeroUsize,
    max_concurrent_maintenance: NonZeroUsize,
    max_concurrent_recoveries: NonZeroUsize,
    max_pending_capacity_waiters: NonZeroUsize,
    max_batch_size: NonZeroUsize,
    max_capacity_units: NonZeroUsize,
    max_operation_capacity_units: NonZeroUsize,
}

impl CapacityLimits {
    /// Creates the capacity-limit view represented by a validated Indexing
    /// configuration.
    ///
    /// The configuration is already responsible for rejecting invalid or zero
    /// values, so this conversion does not duplicate configuration validation
    /// or introduce another source of truth.
    #[must_use]
    pub const fn from_configuration(configuration: IndexingConfiguration) -> Self {
        Self {
            max_concurrent_queries: configuration.max_concurrent_queries(),
            max_concurrent_builds: configuration.max_concurrent_builds(),
            max_concurrent_maintenance: configuration.max_concurrent_maintenance(),
            max_concurrent_recoveries: configuration.max_concurrent_recoveries(),
            max_pending_capacity_waiters: configuration.max_pending_capacity_waiters(),
            max_batch_size: configuration.max_batch_size(),
            max_capacity_units: configuration.max_capacity_units(),
            max_operation_capacity_units: configuration.max_operation_capacity_units(),
        }
    }

    /// Returns the maximum concurrent admission count for the supplied work
    /// class.
    #[must_use]
    pub const fn max_concurrent(self, operation: CapacityOperation) -> NonZeroUsize {
        match operation {
            CapacityOperation::Query => self.max_concurrent_queries,
            CapacityOperation::Build => self.max_concurrent_builds,
            CapacityOperation::Maintenance => self.max_concurrent_maintenance,
            CapacityOperation::Recovery => self.max_concurrent_recoveries,
        }
    }

    /// Returns the maximum number of concurrently admitted queries.
    #[must_use]
    pub const fn max_concurrent_queries(self) -> NonZeroUsize {
        self.max_concurrent_queries
    }

    /// Returns the maximum number of concurrently admitted builds.
    #[must_use]
    pub const fn max_concurrent_builds(self) -> NonZeroUsize {
        self.max_concurrent_builds
    }

    /// Returns the maximum number of concurrently admitted maintenance
    /// operations.
    #[must_use]
    pub const fn max_concurrent_maintenance(self) -> NonZeroUsize {
        self.max_concurrent_maintenance
    }

    /// Returns the maximum number of concurrently admitted recovery
    /// operations.
    #[must_use]
    pub const fn max_concurrent_recoveries(self) -> NonZeroUsize {
        self.max_concurrent_recoveries
    }

    /// Returns the maximum number of operations that may wait for capacity.
    ///
    /// This is a bound for the later bounded-throttling mechanism. It is not a
    /// request queue and does not imply that Indexing owns a scheduler.
    #[must_use]
    pub const fn max_pending_capacity_waiters(self) -> NonZeroUsize {
        self.max_pending_capacity_waiters
    }

    /// Returns the maximum number of logical mutations accepted in one batch.
    ///
    /// This applies to logical mutation batching and is intentionally distinct
    /// from query result limits.
    #[must_use]
    pub const fn max_batch_size(self) -> NonZeroUsize {
        self.max_batch_size
    }

    /// Returns the total logical capacity-unit budget for concurrent Indexing
    /// operations.
    #[must_use]
    pub const fn max_capacity_units(self) -> NonZeroUsize {
        self.max_capacity_units
    }

    /// Returns the largest logical capacity-unit cost that one operation may
    /// claim.
    #[must_use]
    pub const fn max_operation_capacity_units(self) -> NonZeroUsize {
        self.max_operation_capacity_units
    }

    /// Returns whether the supplied batch size is within the configured bound.
    #[must_use]
    pub const fn allows_batch_size(self, batch_size: usize) -> bool {
        batch_size <= self.max_batch_size.get()
    }

    /// Returns whether the supplied concurrent count fits the limit for the
    /// selected workload class.
    #[must_use]
    pub const fn allows_concurrent(
        self,
        operation: CapacityOperation,
        current_count: usize,
    ) -> bool {
        current_count < self.max_concurrent(operation).get()
    }

    /// Returns whether the supplied logical capacity-unit request can fit as a
    /// single operation.
    ///
    /// This checks only the per-operation ceiling. The accounting layer must
    /// separately check the remaining total budget before admission.
    #[must_use]
    pub const fn allows_operation_capacity(self, requested_units: usize) -> bool {
        requested_units <= self.max_operation_capacity_units.get()
    }

    /// Returns whether a requested logical capacity-unit total fits within the
    /// configured aggregate budget.
    #[must_use]
    pub const fn allows_total_capacity(self, requested_units: usize) -> bool {
        requested_units <= self.max_capacity_units.get()
    }
}

impl From<IndexingConfiguration> for CapacityLimits {
    fn from(configuration: IndexingConfiguration) -> Self {
        Self::from_configuration(configuration)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn configuration() -> IndexingConfiguration {
        IndexingConfiguration::new(8, 2, 3, 1, 16, 1_024, 128, 32)
            .expect("test configuration should be valid")
    }

    fn limits() -> CapacityLimits {
        CapacityLimits::from_configuration(configuration())
    }

    #[test]
    fn configuration_is_converted_without_losing_any_limit() {
        let configuration = configuration();
        let limits = CapacityLimits::from_configuration(configuration);

        assert_eq!(
            limits.max_concurrent_queries(),
            configuration.max_concurrent_queries()
        );
        assert_eq!(
            limits.max_concurrent_builds(),
            configuration.max_concurrent_builds()
        );
        assert_eq!(
            limits.max_concurrent_maintenance(),
            configuration.max_concurrent_maintenance()
        );
        assert_eq!(
            limits.max_concurrent_recoveries(),
            configuration.max_concurrent_recoveries()
        );
        assert_eq!(
            limits.max_pending_capacity_waiters(),
            configuration.max_pending_capacity_waiters()
        );
        assert_eq!(limits.max_batch_size(), configuration.max_batch_size());
        assert_eq!(
            limits.max_capacity_units(),
            configuration.max_capacity_units()
        );
        assert_eq!(
            limits.max_operation_capacity_units(),
            configuration.max_operation_capacity_units()
        );
    }

    #[test]
    fn each_workload_class_uses_its_own_concurrency_limit() {
        let limits = limits();

        assert_eq!(limits.max_concurrent(CapacityOperation::Query).get(), 8);
        assert_eq!(limits.max_concurrent(CapacityOperation::Build).get(), 2);
        assert_eq!(
            limits.max_concurrent(CapacityOperation::Maintenance).get(),
            3
        );
        assert_eq!(limits.max_concurrent(CapacityOperation::Recovery).get(), 1);
    }

    #[test]
    fn all_returns_every_supported_workload_class() {
        assert_eq!(
            CapacityOperation::all(),
            [
                CapacityOperation::Query,
                CapacityOperation::Build,
                CapacityOperation::Maintenance,
                CapacityOperation::Recovery,
            ]
        );
    }

    #[test]
    fn concurrent_limit_is_strictly_bounded() {
        let limits = limits();

        assert!(limits.allows_concurrent(CapacityOperation::Query, 0));
        assert!(limits.allows_concurrent(CapacityOperation::Query, 7));
        assert!(!limits.allows_concurrent(CapacityOperation::Query, 8));

        assert!(limits.allows_concurrent(CapacityOperation::Build, 1));
        assert!(!limits.allows_concurrent(CapacityOperation::Build, 2));
    }

    #[test]
    fn batch_size_is_bounded_without_inventing_defaults() {
        let limits = limits();

        assert!(limits.allows_batch_size(1));
        assert!(limits.allows_batch_size(1_024));
        assert!(!limits.allows_batch_size(1_025));
    }

    #[test]
    fn operation_capacity_has_its_own_ceiling() {
        let limits = limits();

        assert!(limits.allows_operation_capacity(0));
        assert!(limits.allows_operation_capacity(32));
        assert!(!limits.allows_operation_capacity(33));

        assert!(limits.allows_total_capacity(128));
        assert!(!limits.allows_total_capacity(129));
    }

    #[test]
    fn from_trait_matches_explicit_conversion() {
        let configuration = configuration();

        assert_eq!(
            CapacityLimits::from(configuration),
            CapacityLimits::from_configuration(configuration)
        );
    }
}
