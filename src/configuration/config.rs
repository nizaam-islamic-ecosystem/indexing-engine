//! Operational configuration for the Nizaam Indexing Engine.
//!
//! This module defines the Indexing-owned configuration values that Phase 5
//! needs for bounded operational behavior. It deliberately does **not** own
//! configuration loading, parsing, validation of raw external sources,
//! snapshot generation, snapshot activation, or runtime update coordination.
//! Those responsibilities remain with `nizaam-core`'s configuration system.
//!
//! The intended lifecycle is:
//!
//! ```text
//! Core configuration source
//!          ↓
//! Core parsing / validation / resolution
//!          ↓
//! IndexingConfiguration candidate
//!          ↓
//! Core immutable configuration snapshot
//!          ↓
//! activation / hot update
//! ```
//!
//! A configuration value is therefore immutable after construction. A runtime
//! update creates a new `IndexingConfiguration` value and lets the Core
//! configuration machinery activate the corresponding new snapshot.
//!
//! Phase 5 intentionally keeps this module independent from the capacity,
//! lifecycle, integrity, recovery, and provider implementations. Those
//! modules consume the validated configuration; they do not own how a Core
//! snapshot is loaded or activated.

use core::fmt;
use core::num::NonZeroUsize;
use std::error::Error;

/// Operational configuration for one Indexing Engine instance.
///
/// The values represent logical safety bounds rather than physical machine
/// resources. No field describes a database, storage engine, physical index
/// algorithm, or other provider-specific implementation choice.
///
/// The configuration is immutable after construction. To change an
/// operational setting, construct a new value and publish it through the
/// Core configuration snapshot/update mechanism.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IndexingConfiguration {
    max_concurrent_queries: NonZeroUsize,
    max_concurrent_builds: NonZeroUsize,
    max_concurrent_maintenance: NonZeroUsize,
    max_concurrent_recoveries: NonZeroUsize,
    max_pending_capacity_waiters: NonZeroUsize,
    max_batch_size: NonZeroUsize,
    max_capacity_units: NonZeroUsize,
    max_operation_capacity_units: NonZeroUsize,
}

impl IndexingConfiguration {
    /// Constructs a validated operational configuration.
    ///
    /// No implicit production defaults are supplied. This is intentional:
    /// Phase 5 requires bounded behavior but does not freeze arbitrary
    /// numerical limits in the logical architecture.
    #[expect(
        clippy::too_many_arguments,
        reason = "Each argument is an independent logical safety bound in the configuration contract."
    )]
    pub fn new(
        max_concurrent_queries: usize,
        max_concurrent_builds: usize,
        max_concurrent_maintenance: usize,
        max_concurrent_recoveries: usize,
        max_pending_capacity_waiters: usize,
        max_batch_size: usize,
        max_capacity_units: usize,
        max_operation_capacity_units: usize,
    ) -> Result<Self, ConfigurationValidationError> {
        let max_concurrent_queries = require_non_zero(
            max_concurrent_queries,
            ConfigurationField::MaxConcurrentQueries,
        )?;
        let max_concurrent_builds = require_non_zero(
            max_concurrent_builds,
            ConfigurationField::MaxConcurrentBuilds,
        )?;
        let max_concurrent_maintenance = require_non_zero(
            max_concurrent_maintenance,
            ConfigurationField::MaxConcurrentMaintenance,
        )?;
        let max_concurrent_recoveries = require_non_zero(
            max_concurrent_recoveries,
            ConfigurationField::MaxConcurrentRecoveries,
        )?;
        let max_pending_capacity_waiters = require_non_zero(
            max_pending_capacity_waiters,
            ConfigurationField::MaxPendingCapacityWaiters,
        )?;
        let max_batch_size = require_non_zero(max_batch_size, ConfigurationField::MaxBatchSize)?;
        let max_capacity_units =
            require_non_zero(max_capacity_units, ConfigurationField::MaxCapacityUnits)?;
        let max_operation_capacity_units = require_non_zero(
            max_operation_capacity_units,
            ConfigurationField::MaxOperationCapacityUnits,
        )?;

        if max_operation_capacity_units > max_capacity_units {
            return Err(
                ConfigurationValidationError::OperationCapacityExceedsBudget {
                    max_operation_capacity_units: max_operation_capacity_units.get(),
                    max_capacity_units: max_capacity_units.get(),
                },
            );
        }

        Ok(Self {
            max_concurrent_queries,
            max_concurrent_builds,
            max_concurrent_maintenance,
            max_concurrent_recoveries,
            max_pending_capacity_waiters,
            max_batch_size,
            max_capacity_units,
            max_operation_capacity_units,
        })
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
    /// The bound is intentionally separate from the concurrency limits so a
    /// future capacity layer can implement bounded waiting without creating an
    /// unbounded local queue.
    #[must_use]
    pub const fn max_pending_capacity_waiters(self) -> NonZeroUsize {
        self.max_pending_capacity_waiters
    }

    /// Returns the maximum number of logical mutations accepted in one batch.
    #[must_use]
    pub const fn max_batch_size(self) -> NonZeroUsize {
        self.max_batch_size
    }

    /// Returns the total logical capacity-unit budget available to concurrent
    /// Indexing operations.
    #[must_use]
    pub const fn max_capacity_units(self) -> NonZeroUsize {
        self.max_capacity_units
    }

    /// Returns the maximum logical capacity-unit cost one operation may claim.
    #[must_use]
    pub const fn max_operation_capacity_units(self) -> NonZeroUsize {
        self.max_operation_capacity_units
    }
}

/// Identifies a configuration field that must contain a positive value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConfigurationField {
    /// Maximum concurrent query count.
    MaxConcurrentQueries,

    /// Maximum concurrent build count.
    MaxConcurrentBuilds,

    /// Maximum concurrent maintenance count.
    MaxConcurrentMaintenance,

    /// Maximum concurrent recovery count.
    MaxConcurrentRecoveries,

    /// Maximum number of operations allowed to wait for capacity.
    MaxPendingCapacityWaiters,

    /// Maximum logical mutations in one batch.
    MaxBatchSize,

    /// Total logical capacity-unit budget.
    MaxCapacityUnits,

    /// Maximum logical capacity-unit cost for one operation.
    MaxOperationCapacityUnits,
}

impl ConfigurationField {
    /// Returns the stable field name used in diagnostics.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::MaxConcurrentQueries => "max_concurrent_queries",
            Self::MaxConcurrentBuilds => "max_concurrent_builds",
            Self::MaxConcurrentMaintenance => "max_concurrent_maintenance",
            Self::MaxConcurrentRecoveries => "max_concurrent_recoveries",
            Self::MaxPendingCapacityWaiters => "max_pending_capacity_waiters",
            Self::MaxBatchSize => "max_batch_size",
            Self::MaxCapacityUnits => "max_capacity_units",
            Self::MaxOperationCapacityUnits => "max_operation_capacity_units",
        }
    }
}

/// Validation failure for an [`IndexingConfiguration`] value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConfigurationValidationError {
    /// A required positive configuration value was supplied as zero.
    ZeroValue {
        /// The field that contained the invalid value.
        field: ConfigurationField,
    },

    /// One operation could claim more logical capacity than the total budget.
    OperationCapacityExceedsBudget {
        /// The invalid per-operation capacity-unit limit.
        max_operation_capacity_units: usize,

        /// The configured total capacity-unit budget.
        max_capacity_units: usize,
    },
}

impl fmt::Display for ConfigurationField {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl fmt::Display for ConfigurationValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroValue { field } => {
                write!(formatter, "{field} must be greater than zero")
            }
            Self::OperationCapacityExceedsBudget {
                max_operation_capacity_units,
                max_capacity_units,
            } => write!(
                formatter,
                "max operation capacity units ({max_operation_capacity_units}) must not exceed max capacity units ({max_capacity_units})"
            ),
        }
    }
}

impl Error for ConfigurationValidationError {}

fn require_non_zero(
    value: usize,
    field: ConfigurationField,
) -> Result<NonZeroUsize, ConfigurationValidationError> {
    NonZeroUsize::new(value).ok_or(ConfigurationValidationError::ZeroValue { field })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_configuration() -> IndexingConfiguration {
        IndexingConfiguration::new(8, 2, 2, 1, 16, 1_024, 128, 32)
            .expect("test configuration should be valid")
    }

    #[test]
    fn constructs_valid_configuration() {
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
    fn every_positive_limit_rejects_zero() {
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
                })
            );
        }
    }

    #[test]
    fn operation_capacity_cannot_exceed_total_budget() {
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
    fn configuration_is_value_like_and_can_be_replaced_atomically() {
        let original = valid_configuration();
        let replacement = IndexingConfiguration::new(16, 4, 2, 2, 32, 2_048, 256, 64)
            .expect("replacement configuration should be valid");

        assert_ne!(original, replacement);
        assert_eq!(original.max_concurrent_queries().get(), 8);
        assert_eq!(replacement.max_concurrent_queries().get(), 16);
    }

    #[test]
    fn configuration_field_names_are_stable() {
        assert_eq!(
            ConfigurationField::MaxConcurrentQueries.as_str(),
            "max_concurrent_queries"
        );
        assert_eq!(
            ConfigurationField::MaxConcurrentBuilds.as_str(),
            "max_concurrent_builds"
        );
        assert_eq!(
            ConfigurationField::MaxConcurrentMaintenance.as_str(),
            "max_concurrent_maintenance"
        );
        assert_eq!(
            ConfigurationField::MaxConcurrentRecoveries.as_str(),
            "max_concurrent_recoveries"
        );
        assert_eq!(
            ConfigurationField::MaxPendingCapacityWaiters.as_str(),
            "max_pending_capacity_waiters"
        );
        assert_eq!(ConfigurationField::MaxBatchSize.as_str(), "max_batch_size");
        assert_eq!(
            ConfigurationField::MaxCapacityUnits.as_str(),
            "max_capacity_units"
        );
        assert_eq!(
            ConfigurationField::MaxOperationCapacityUnits.as_str(),
            "max_operation_capacity_units"
        );
    }
}
