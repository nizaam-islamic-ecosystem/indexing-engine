//! Public configuration boundary for Phase 5 operational Indexing settings.
//!
//! This module assembles the Indexing-owned configuration model and exposes it
//! through a single public boundary. It does not implement configuration
//! loading, parsing, snapshot activation, or runtime update coordination.
//! Those responsibilities remain with `nizaam-core`'s configuration
//! infrastructure.
//!
//! The architectural separation is:
//!
//! ```text
//! Core configuration infrastructure
//!     ↓
//! validated IndexingConfiguration
//!     ↓
//! Core immutable configuration snapshot
//!     ↓
//! activation / hot update
//! ```
//!
//! `config` owns the typed Indexing operational values. Other Phase 5 modules
//! such as capacity and recovery consume those values; they do not become
//! configuration managers themselves.

/// Indexing-owned operational configuration values and validation.
pub mod config;

// -----------------------------------------------------------------------------
// Public configuration surface
// -----------------------------------------------------------------------------

pub use config::{ConfigurationField, ConfigurationValidationError, IndexingConfiguration};

#[cfg(test)]
mod tests {
    use super::*;

    fn configuration() -> IndexingConfiguration {
        IndexingConfiguration::new(8, 2, 2, 1, 16, 1_024, 128, 32)
            .expect("test configuration should be valid")
    }

    #[test]
    fn configuration_module_exposes_the_canonical_configuration_model() {
        let value = configuration();

        assert_eq!(value.max_concurrent_queries().get(), 8);
        assert_eq!(value.max_concurrent_builds().get(), 2);
        assert_eq!(value.max_concurrent_maintenance().get(), 2);
        assert_eq!(value.max_concurrent_recoveries().get(), 1);
        assert_eq!(value.max_pending_capacity_waiters().get(), 16);
        assert_eq!(value.max_batch_size().get(), 1_024);
        assert_eq!(value.max_capacity_units().get(), 128);
        assert_eq!(value.max_operation_capacity_units().get(), 32);
    }

    #[test]
    fn configuration_module_preserves_validation_boundary() {
        let error = IndexingConfiguration::new(8, 2, 2, 1, 16, 1_024, 32, 33)
            .expect_err("per-operation capacity must not exceed the total budget");

        assert_eq!(
            error,
            ConfigurationValidationError::OperationCapacityExceedsBudget {
                max_operation_capacity_units: 33,
                max_capacity_units: 32,
            }
        );
    }

    #[test]
    fn configuration_module_exposes_stable_diagnostic_field_names() {
        assert_eq!(
            ConfigurationField::MaxConcurrentQueries.as_str(),
            "max_concurrent_queries"
        );
        assert_eq!(ConfigurationField::MaxBatchSize.as_str(), "max_batch_size");
        assert_eq!(
            ConfigurationField::MaxOperationCapacityUnits.as_str(),
            "max_operation_capacity_units"
        );
    }
}
