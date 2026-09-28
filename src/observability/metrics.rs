//! Indexing metrics integration with the Nizaam Core observability system.
//!
//! Phase 6 does not implement a second metrics framework for the Indexing
//! Engine. Core remains authoritative for metric definitions, metric kinds,
//! bounded dimensions, concurrent recording, snapshots, and metric errors.
//!
//! This module provides only the Indexing-facing adapter over Core's existing
//! [`nizaam_core::observability::MetricRecorder`].
//!
//! The intended boundary is:
//!
//! ```text
//! Indexing operation
//!       │
//!       ▼
//! IndexingMetrics
//!       │
//!       ▼
//! Core MetricRecorder
//!       │
//!       ├── Counter
//!       ├── Gauge
//!       └── Histogram
//! ```
//!
//! No Indexing-specific metric storage, exporter, registry, background
//! recorder, or unbounded dimension mechanism is introduced here.
//!
//! Metric names remain operation-level implementation details. This module
//! therefore does not predeclare Indexing operation metrics before those
//! operations are integrated.
//!
//! Metric dimensions must remain bounded and must not be used to store raw
//! request payloads, full query text, arbitrary object identifiers, or other
//! uncontrolled user-provided values.

use nizaam_core::observability::{
    MetricDescriptor, MetricDimensions, MetricError, MetricRecorder, MetricSnapshot,
};

/// Indexing-facing adapter over the Core metric recorder.
///
/// The actual metric storage, synchronization, validation, and snapshot
/// behavior remain entirely owned by Core.
#[derive(Clone, Default)]
pub struct IndexingMetrics {
    core: MetricRecorder,
}

impl IndexingMetrics {
    /// Creates an empty Indexing metrics adapter backed by a Core recorder.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Records a counter increment using the Core metric contract.
    ///
    /// Counter overflow, metric-kind mismatches, invalid dimensions, and
    /// other metric validation failures are returned directly from Core.
    pub fn record_counter(
        &self,
        descriptor: &MetricDescriptor,
        dimensions: MetricDimensions,
        amount: u64,
    ) -> Result<(), MetricError> {
        self.core.increment_counter(descriptor, dimensions, amount)
    }

    /// Sets a gauge value using the Core metric contract.
    ///
    /// Core remains responsible for rejecting non-finite values and
    /// incompatible metric kinds.
    pub fn set_gauge(
        &self,
        descriptor: &MetricDescriptor,
        dimensions: MetricDimensions,
        value: f64,
    ) -> Result<(), MetricError> {
        self.core.set_gauge(descriptor, dimensions, value)
    }

    /// Records one histogram observation using the Core metric contract.
    ///
    /// Core remains responsible for observation validation and aggregation.
    pub fn observe(
        &self,
        descriptor: &MetricDescriptor,
        dimensions: MetricDimensions,
        value: f64,
    ) -> Result<(), MetricError> {
        self.core.observe(descriptor, dimensions, value)
    }

    /// Returns a snapshot of all metric series currently recorded by Core.
    ///
    /// The snapshot is intended for inspection or later integration with an
    /// external observability/export mechanism. This module does not implement
    /// such an exporter.
    pub fn snapshot(&self) -> Result<Vec<MetricSnapshot>, MetricError> {
        self.core.snapshot()
    }

    /// Clears the currently recorded metric series.
    ///
    /// This delegates directly to the Core recorder and does not introduce a
    /// second Indexing-specific storage lifecycle.
    pub fn clear(&self) -> Result<(), MetricError> {
        self.core.clear()
    }
}

impl From<MetricRecorder> for IndexingMetrics {
    fn from(core: MetricRecorder) -> Self {
        Self { core }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nizaam_core::observability::MetricKind;

    fn descriptor(name: &str, kind: MetricKind) -> MetricDescriptor {
        MetricDescriptor::new(
            nizaam_core::observability::MetricName::new(name).unwrap(),
            kind,
        )
    }

    #[test]
    fn creates_empty_metrics_adapter() {
        let metrics = IndexingMetrics::new();

        assert!(metrics.snapshot().unwrap().is_empty());
    }

    #[test]
    fn delegates_counter_recording_to_core() {
        let metrics = IndexingMetrics::new();
        let descriptor = descriptor("index.build.total", MetricKind::Counter);

        metrics
            .record_counter(&descriptor, MetricDimensions::new(), 1)
            .unwrap();
        metrics
            .record_counter(&descriptor, MetricDimensions::new(), 2)
            .unwrap();

        let snapshot = metrics.snapshot().unwrap();

        assert_eq!(snapshot.len(), 1);
        assert_eq!(
            snapshot[0].descriptor().name().as_str(),
            "index.build.total"
        );
        assert_eq!(
            snapshot[0].value(),
            &nizaam_core::observability::MetricValue::Counter(3)
        );
    }

    #[test]
    fn delegates_gauge_recording_to_core() {
        let metrics = IndexingMetrics::new();
        let descriptor = descriptor("index.capacity.active", MetricKind::Gauge);

        metrics
            .set_gauge(&descriptor, MetricDimensions::new(), 4.0)
            .unwrap();

        assert_eq!(
            metrics.snapshot().unwrap()[0].value(),
            &nizaam_core::observability::MetricValue::Gauge(4.0)
        );
    }

    #[test]
    fn delegates_histogram_recording_to_core() {
        let metrics = IndexingMetrics::new();
        let descriptor = descriptor("index.build.latency", MetricKind::Histogram);

        metrics
            .observe(&descriptor, MetricDimensions::new(), 10.0)
            .unwrap();
        metrics
            .observe(&descriptor, MetricDimensions::new(), 20.0)
            .unwrap();

        assert_eq!(
            metrics.snapshot().unwrap()[0].value(),
            &nizaam_core::observability::MetricValue::Histogram {
                count: 2,
                sum: 30.0,
            }
        );
    }

    #[test]
    fn preserves_bounded_core_dimensions() {
        let metrics = IndexingMetrics::new();
        let descriptor = descriptor("index.query.total", MetricKind::Counter);
        let mut dimensions = MetricDimensions::new();

        let mut accepted = 0;

        loop {
            let key = format!("dimension-{accepted}");
            match dimensions.insert(key, "bounded") {
                Ok(()) => accepted += 1,
                Err(MetricError::DimensionLimitExceeded) => break,
                Err(error) => panic!("unexpected Core dimension validation error: {error:?}"),
            }
        }

        assert!(accepted > 0);

        metrics
            .record_counter(&descriptor, dimensions.clone(), 1)
            .unwrap();

        assert_eq!(metrics.snapshot().unwrap()[0].dimensions().len(), accepted);

        assert_eq!(
            dimensions.insert("overflow", "rejected"),
            Err(MetricError::DimensionLimitExceeded)
        );
    }

    #[test]
    fn preserves_core_kind_mismatch_errors() {
        let metrics = IndexingMetrics::new();

        let counter = descriptor("index.operations.total", MetricKind::Counter);
        let gauge = descriptor("index.operations.total", MetricKind::Gauge);

        metrics
            .record_counter(&counter, MetricDimensions::new(), 1)
            .unwrap();

        assert_eq!(
            metrics.set_gauge(&gauge, MetricDimensions::new(), 1.0),
            Err(MetricError::KindMismatch {
                expected: MetricKind::Gauge,
                actual: MetricKind::Counter,
            })
        );
    }

    #[test]
    fn preserves_core_non_finite_value_validation() {
        let metrics = IndexingMetrics::new();

        let gauge = descriptor("index.active", MetricKind::Gauge);
        let histogram = descriptor("index.query.latency", MetricKind::Histogram);

        assert_eq!(
            metrics.set_gauge(&gauge, MetricDimensions::new(), f64::NAN),
            Err(MetricError::NonFiniteValue)
        );

        assert_eq!(
            metrics.observe(&histogram, MetricDimensions::new(), f64::INFINITY,),
            Err(MetricError::NonFiniteValue)
        );
    }

    #[test]
    fn dimensions_create_separate_metric_series() {
        let metrics = IndexingMetrics::new();
        let descriptor = descriptor("index.query.total", MetricKind::Counter);

        let mut first = MetricDimensions::new();
        first.insert("operation", "query").unwrap();

        let mut second = MetricDimensions::new();
        second.insert("operation", "rebuild").unwrap();

        metrics.record_counter(&descriptor, first, 3).unwrap();
        metrics.record_counter(&descriptor, second, 5).unwrap();

        let snapshot = metrics.snapshot().unwrap();

        assert_eq!(snapshot.len(), 2);

        assert!(snapshot.iter().any(|item| {
            item.dimensions().get("operation") == Some("query")
                && item.value() == &nizaam_core::observability::MetricValue::Counter(3)
        }));

        assert!(snapshot.iter().any(|item| {
            item.dimensions().get("operation") == Some("rebuild")
                && item.value() == &nizaam_core::observability::MetricValue::Counter(5)
        }));
    }

    #[test]
    fn clear_delegates_to_core_recorder() {
        let metrics = IndexingMetrics::new();
        let descriptor = descriptor("index.recovery.total", MetricKind::Counter);

        metrics
            .record_counter(&descriptor, MetricDimensions::new(), 1)
            .unwrap();

        assert_eq!(metrics.snapshot().unwrap().len(), 1);

        metrics.clear().unwrap();

        assert!(metrics.snapshot().unwrap().is_empty());
    }

    #[test]
    fn can_wrap_an_existing_core_recorder() {
        let recorder = MetricRecorder::new();
        let metrics = IndexingMetrics::from(recorder);

        assert!(metrics.snapshot().unwrap().is_empty());
    }
}
