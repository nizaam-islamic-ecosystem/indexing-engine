//! Phase 5 integration tests for Indexing capacity limits and accounting.
//!
//! These tests exercise the public capacity boundary without introducing a
//! scheduler, queue, retry loop, or independent waiting mechanism.

use nizaam_indexing::{
    CapacityAccounting, CapacityAdmissionError, CapacityLimits, CapacityOperation, CapacityRequest,
    CapacityUsageSnapshot, CapacityWaiterError, IndexingConfiguration,
};

fn configuration() -> IndexingConfiguration {
    IndexingConfiguration::new(
        2, // queries
        1, // builds
        1, // maintenance
        1, // recoveries
        2, // pending waiters
        4, // batch size
        8, // total logical units
        4, // per-operation logical units
    )
    .expect("test configuration should be valid")
}

fn accounting() -> CapacityAccounting {
    CapacityAccounting::from_configuration(configuration())
}

#[test]
fn configuration_maps_to_all_capacity_limits() {
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
fn all_capacity_operation_classes_have_distinct_admission_counters() {
    let accounting = accounting();

    let query = accounting
        .try_acquire(CapacityRequest::new(CapacityOperation::Query, 1))
        .expect("query admission should succeed");
    let build = accounting
        .try_acquire(CapacityRequest::new(CapacityOperation::Build, 1))
        .expect("build admission should succeed");
    let maintenance = accounting
        .try_acquire(CapacityRequest::new(CapacityOperation::Maintenance, 1))
        .expect("maintenance admission should succeed");
    let recovery = accounting
        .try_acquire(CapacityRequest::new(CapacityOperation::Recovery, 1))
        .expect("recovery admission should succeed");

    let usage = accounting.usage();

    assert_eq!(query.operation(), CapacityOperation::Query);
    assert_eq!(build.operation(), CapacityOperation::Build);
    assert_eq!(maintenance.operation(), CapacityOperation::Maintenance);
    assert_eq!(recovery.operation(), CapacityOperation::Recovery);

    assert_eq!(usage.active_queries(), 1);
    assert_eq!(usage.active_builds(), 1);
    assert_eq!(usage.active_maintenance(), 1);
    assert_eq!(usage.active_recoveries(), 1);
    assert_eq!(usage.active_operations(), 4);
    assert_eq!(usage.consumed_capacity_units(), 4);
}

#[test]
fn concurrency_limit_rejects_the_next_operation_without_mutating_usage() {
    let accounting = accounting();

    let _first = accounting
        .try_acquire(CapacityRequest::new(CapacityOperation::Build, 1))
        .expect("first build should be admitted");

    let result = accounting.try_acquire(CapacityRequest::new(CapacityOperation::Build, 1));

    assert!(matches!(
        result,
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
fn per_operation_capacity_limit_is_checked_before_reservation() {
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
fn aggregate_capacity_limit_is_enforced_against_current_usage() {
    let accounting = accounting();

    let _first = accounting
        .try_acquire(CapacityRequest::new(CapacityOperation::Query, 4))
        .expect("first four-unit query should be admitted");

    let _second = accounting
        .try_acquire(CapacityRequest::new(CapacityOperation::Build, 4))
        .expect("admission that exactly fills the total budget should be admitted");

    let result = accounting.try_acquire(CapacityRequest::new(CapacityOperation::Query, 1));

    assert!(matches!(
        result,
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
fn batch_size_limit_is_enforced_without_reserving_capacity() {
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
fn successful_admission_is_released_when_lease_is_dropped() {
    let accounting = accounting();

    {
        let lease = accounting
            .try_acquire(CapacityRequest::new(CapacityOperation::Query, 3))
            .expect("query admission should succeed");

        assert_eq!(lease.capacity_units(), 3);
        assert_eq!(accounting.usage().active_queries(), 1);
        assert_eq!(accounting.usage().consumed_capacity_units(), 3);
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
fn zero_logical_units_still_count_as_an_active_operation() {
    let accounting = accounting();

    let _lease = accounting
        .try_acquire(CapacityRequest::new(CapacityOperation::Recovery, 0))
        .expect("zero logical units should be within the configured budget");

    assert_eq!(accounting.usage().active_recoveries(), 1);
    assert_eq!(accounting.usage().consumed_capacity_units(), 0);
    assert_eq!(accounting.usage().active_operations(), 1);
}

#[test]
fn waiter_registration_is_bounded_and_does_not_store_pending_operations() {
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
fn waiter_guard_release_is_idempotent() {
    let accounting = accounting();

    let mut waiter = accounting
        .try_register_waiter()
        .expect("waiter should be registered");

    waiter.release();
    waiter.release();

    assert_eq!(accounting.usage().pending_capacity_waiters(), 0);
}

#[test]
fn cloned_accounting_handles_share_one_local_accounting_state() {
    let first = accounting();
    let second = first.clone();

    let _lease = second
        .try_acquire(CapacityRequest::new(CapacityOperation::Query, 2))
        .expect("admission through the clone should succeed");

    assert_eq!(first.usage().active_queries(), 1);
    assert_eq!(first.usage().consumed_capacity_units(), 2);
}

#[test]
fn failed_admission_is_transactional_across_all_checked_bounds() {
    let accounting = accounting();

    let _existing = accounting
        .try_acquire(CapacityRequest::new(CapacityOperation::Query, 4))
        .expect("existing admission should succeed");

    let before = accounting.usage();

    let result = accounting.try_acquire(CapacityRequest::with_batch_size(
        CapacityOperation::Query,
        5,
        4,
    ));

    assert!(matches!(
        result,
        Err(CapacityAdmissionError::OperationCapacityExceeded {
            requested: 5,
            limit: 4,
        })
    ));
    assert_eq!(accounting.usage(), before);
}

#[test]
fn capacity_limits_expose_the_hybrid_model_without_physical_resource_semantics() {
    let limits = CapacityLimits::from_configuration(configuration());

    assert_eq!(limits.max_concurrent(CapacityOperation::Query).get(), 2);
    assert_eq!(limits.max_concurrent(CapacityOperation::Build).get(), 1);
    assert_eq!(
        limits.max_concurrent(CapacityOperation::Maintenance).get(),
        1
    );
    assert_eq!(limits.max_concurrent(CapacityOperation::Recovery).get(), 1);

    assert!(limits.allows_batch_size(4));
    assert!(!limits.allows_batch_size(5));
    assert!(limits.allows_operation_capacity(4));
    assert!(!limits.allows_operation_capacity(5));
    assert!(limits.allows_total_capacity(8));
    assert!(!limits.allows_total_capacity(9));
}
