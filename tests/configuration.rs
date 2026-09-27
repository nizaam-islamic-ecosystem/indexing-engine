//! Phase 5 integration tests for Indexing operational configuration.
//!
//! `IndexingConfiguration` is an immutable validated value. Core owns the
//! configuration snapshot/update lifecycle; these tests therefore cover the
//! Indexing-owned value contract and its capacity projection.

use nizaam_indexing::{
    CapacityLimits, ConfigurationField, ConfigurationValidationError, IndexingConfiguration,
};

fn valid_configuration() -> IndexingConfiguration {
    IndexingConfiguration::new(
        8,     // max concurrent queries
        2,     // max concurrent builds
        2,     // max concurrent maintenance
        1,     // max concurrent recoveries
        16,    // max pending waiters
        1_024, // max batch size
        128,   // total capacity units
        32,    // per-operation capacity units
    )
    .expect("test configuration should be valid")
}

#[test]
fn valid_configuration_preserves_all_operational_bounds() {
    let configuration = valid_configuration();

    assert_eq!(configuration.max_concurrent_queries().get(), 8);
    assert_eq!(configuration.max_concurrent_builds().get(), 2);
    assert_eq!(configuration.max_concurrent_maintenance().get(), 2);
    assert_eq!(configuration.max_concurrent_recoveries().get(), 1);
    assert_eq!(configuration.max_pending_capacity_waiters().get(), 16);
    assert_eq!(configuration.max_batch_size().get(), 1_024);
    assert_eq!(configuration.max_capacity_units().get(), 128);
    assert_eq!(configuration.max_operation_capacity_units().get(), 32);
}

#[test]
fn every_required_positive_bound_rejects_zero() {
    let cases = [
        (
            ConfigurationField::MaxConcurrentQueries,
            (0, 2, 2, 1, 16, 1_024, 128, 32),
        ),
        (
            ConfigurationField::MaxConcurrentBuilds,
            (8, 0, 2, 1, 16, 1_024, 128, 32),
        ),
        (
            ConfigurationField::MaxConcurrentMaintenance,
            (8, 2, 0, 1, 16, 1_024, 128, 32),
        ),
        (
            ConfigurationField::MaxConcurrentRecoveries,
            (8, 2, 2, 0, 16, 1_024, 128, 32),
        ),
        (
            ConfigurationField::MaxPendingCapacityWaiters,
            (8, 2, 2, 1, 0, 1_024, 128, 32),
        ),
        (
            ConfigurationField::MaxBatchSize,
            (8, 2, 2, 1, 16, 0, 128, 32),
        ),
        (
            ConfigurationField::MaxCapacityUnits,
            (8, 2, 2, 1, 16, 1_024, 0, 0),
        ),
        (
            ConfigurationField::MaxOperationCapacityUnits,
            (8, 2, 2, 1, 16, 1_024, 128, 0),
        ),
    ];

    for (
        expected_field,
        (
            queries,
            builds,
            maintenance,
            recoveries,
            waiters,
            batch_size,
            capacity_units,
            operation_units,
        ),
    ) in cases
    {
        let result = IndexingConfiguration::new(
            queries,
            builds,
            maintenance,
            recoveries,
            waiters,
            batch_size,
            capacity_units,
            operation_units,
        );

        assert_eq!(
            result,
            Err(ConfigurationValidationError::ZeroValue {
                field: expected_field,
            }),
            "zero value for {expected_field} should be rejected"
        );
    }
}

#[test]
fn operation_capacity_cannot_exceed_total_capacity_budget() {
    let result = IndexingConfiguration::new(8, 2, 2, 1, 16, 1_024, 32, 33);

    assert_eq!(
        result,
        Err(
            ConfigurationValidationError::OperationCapacityExceedsBudget {
                max_operation_capacity_units: 33,
                max_capacity_units: 32,
            }
        )
    );
}

#[test]
fn operation_capacity_equal_to_total_budget_is_valid() {
    let result = IndexingConfiguration::new(8, 2, 2, 1, 16, 1_024, 32, 32);

    assert!(result.is_ok());
}

#[test]
fn configuration_is_value_like_and_replacement_is_non_mutating() {
    let original = valid_configuration();
    let replacement = IndexingConfiguration::new(16, 4, 2, 2, 32, 2_048, 256, 64)
        .expect("replacement configuration should be valid");

    assert_ne!(original, replacement);

    assert_eq!(original.max_concurrent_queries().get(), 8);
    assert_eq!(replacement.max_concurrent_queries().get(), 16);

    // The original value remains unchanged after creating a replacement.
    assert_eq!(original.max_capacity_units().get(), 128);
    assert_eq!(replacement.max_capacity_units().get(), 256);
}

#[test]
fn configuration_field_names_are_stable() {
    let expected = [
        (
            ConfigurationField::MaxConcurrentQueries,
            "max_concurrent_queries",
        ),
        (
            ConfigurationField::MaxConcurrentBuilds,
            "max_concurrent_builds",
        ),
        (
            ConfigurationField::MaxConcurrentMaintenance,
            "max_concurrent_maintenance",
        ),
        (
            ConfigurationField::MaxConcurrentRecoveries,
            "max_concurrent_recoveries",
        ),
        (
            ConfigurationField::MaxPendingCapacityWaiters,
            "max_pending_capacity_waiters",
        ),
        (ConfigurationField::MaxBatchSize, "max_batch_size"),
        (ConfigurationField::MaxCapacityUnits, "max_capacity_units"),
        (
            ConfigurationField::MaxOperationCapacityUnits,
            "max_operation_capacity_units",
        ),
    ];

    for (field, name) in expected {
        assert_eq!(field.as_str(), name);
        assert_eq!(field.to_string(), name);
    }
}

#[test]
fn configuration_is_copyable_without_changing_the_original_value() {
    let original = valid_configuration();
    let copied = original;

    assert_eq!(original, copied);
    assert_eq!(copied.max_concurrent_builds().get(), 2);
    assert_eq!(copied.max_operation_capacity_units().get(), 32);
}

#[test]
fn capacity_limits_are_a_lossless_projection_of_configuration() {
    let configuration = valid_configuration();
    let limits = CapacityLimits::from(configuration);

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
