//! Observability integration boundary for the Nizaam Indexing Engine.
//!
//! Phase 6 reuses the observability mechanisms provided by `nizaam_core`
//! rather than introducing a second Indexing-specific observability
//! framework.
//!
//! Core remains authoritative for:
//!
//! - structured logging,
//! - metric definitions and recording,
//! - tracing and span lifecycle,
//! - diagnostics and diagnostic classification,
//! - validation and bounded observability data,
//! - dispatch, buffering, storage, and other observability infrastructure.
//!
//! The Indexing layer contributes only thin adapters that expose the Core
//! mechanisms at Indexing operation boundaries.
//!
//! ```text
//!                     Indexing Engine
//!                           │
//!                           ▼
//!                  observability facade
//!                           │
//!          ┌────────────────┼────────────────┐
//!          │                │                │
//!          ▼                ▼                ▼
//!   IndexingLogger   IndexingMetrics   IndexingTracer
//!          │                │                │
//!          └────────────────┼────────────────┘
//!                           │
//!                           ▼
//!                    Core observability
//!                           │
//!                    IndexingDiagnostics
//!                           │
//!                           ▼
//!                       Nizaam Core
//! ```
//!
//! No Indexing-specific logging system, metrics registry, tracing system,
//! diagnostic store, exporter, subscriber, background worker, or universal
//! observability error framework is introduced here.

pub mod diagnostics;
pub mod logging;
pub mod metrics;
pub mod tracing;

pub use diagnostics::IndexingDiagnostics;
pub use logging::IndexingLogger;
pub use metrics::IndexingMetrics;
pub use tracing::IndexingTracer;

#[cfg(test)]
mod tests {
    use std::sync::{Arc, mpsc};
    use std::time::{Duration, SystemTime};

    use nizaam_core::events::EventName;
    use nizaam_core::identity::{CorrelationId, EngineId, OperationId};
    use nizaam_core::logging::{
        DispatchOutcome, LogContext, LogEvent, LogEventType, LogLevel, LogScope, LogSink,
        LoggingSystem,
    };
    use nizaam_core::observability::{
        DiagnosticCondition, DiagnosticKind, MetricDimensions, MetricKind, MetricName,
    };
    use nizaam_core::operation::{Operation, OperationContext};

    use super::{IndexingDiagnostics, IndexingLogger, IndexingMetrics, IndexingTracer};

    // -------------------------------------------------------------------------
    // Level 2: public facade / export surface
    // -------------------------------------------------------------------------

    #[test]
    fn exposes_all_phase_6_observability_adapters() {
        let diagnostics = IndexingDiagnostics::new();
        let metrics = IndexingMetrics::new();
        let tracer = IndexingTracer::new();

        assert_eq!(diagnostics, IndexingDiagnostics::new());
        assert_eq!(tracer, IndexingTracer::new());

        assert!(metrics.snapshot().unwrap().is_empty());

        let _ = diagnostics;
        let _ = tracer;
    }

    // -------------------------------------------------------------------------
    // Level 2: logging facade integration
    // -------------------------------------------------------------------------

    struct RecordingSink(mpsc::Sender<LogEvent>);

    impl LogSink for RecordingSink {
        fn publish(&self, event: &LogEvent) {
            let _ = self.0.send(event.clone());
        }
    }

    fn operation_context() -> OperationContext {
        OperationContext::new(Operation::new(
            OperationId::new("observability-level2-operation").unwrap(),
            CorrelationId::new("observability-level2-correlation").unwrap(),
        ))
    }

    fn logging_event(engine_id: EngineId) -> LogEvent {
        LogEvent::new(
            EventName::new("index.build.started").unwrap(),
            LogLevel::Info,
            nizaam_core::logging::LogSource::Engine(engine_id.clone()),
            LogScope::Local,
            "indexing-engine",
            LogContext::new(operation_context()).from_engine(engine_id),
            "index build started",
            LogEventType::Diagnostic,
        )
        .unwrap()
    }

    #[test]
    fn logging_facade_reaches_core_logging_system() {
        let system = LoggingSystem::new(8).unwrap();
        let (sender, receiver) = mpsc::channel();

        system.subscribe(Arc::new(RecordingSink(sender)));

        let engine_id = EngineId::new("indexing-engine").unwrap();

        let instance = system.instance(
            LogScope::Local,
            nizaam_core::logging::LogSource::Engine(engine_id.clone()),
        );

        let logger = IndexingLogger::new(&instance);
        let event = logging_event(engine_id);

        assert_eq!(logger.emit(event.clone()).unwrap(), DispatchOutcome::Queued);

        let received = receiver
            .recv_timeout(Duration::from_secs(1))
            .expect("Core logging system should receive the event");

        assert_eq!(received, event);

        system.shutdown().unwrap();
    }

    // -------------------------------------------------------------------------
    // Level 2: metrics facade integration
    // -------------------------------------------------------------------------

    #[test]
    fn metrics_facade_reaches_core_metric_recorder() {
        let metrics = IndexingMetrics::new();

        let descriptor = nizaam_core::observability::MetricDescriptor::new(
            MetricName::new("index.observability.level2").unwrap(),
            MetricKind::Counter,
        );

        metrics
            .record_counter(&descriptor, MetricDimensions::new(), 3)
            .unwrap();

        let snapshots = metrics.snapshot().unwrap();

        assert_eq!(snapshots.len(), 1);
        assert_eq!(
            snapshots[0].descriptor().name().as_str(),
            "index.observability.level2"
        );
        assert_eq!(
            snapshots[0].value(),
            &nizaam_core::observability::MetricValue::Counter(3)
        );
    }

    // -------------------------------------------------------------------------
    // Level 2: tracing facade integration
    // -------------------------------------------------------------------------

    #[test]
    fn tracing_facade_composes_core_trace_and_span_context() {
        let tracer = IndexingTracer::new();

        let trace_id =
            nizaam_core::observability::TraceId::new("observability-level2-trace").unwrap();

        let root_span_id = nizaam_core::observability::SpanId::new("observability-root").unwrap();

        let child_span_id = nizaam_core::observability::SpanId::new("observability-child").unwrap();

        let root = tracer
            .root_span(trace_id, root_span_id, "index.operation")
            .unwrap();

        let child = tracer
            .child_span(&root, child_span_id, "index.operation.step")
            .unwrap();

        let context = tracer.context(&child);

        assert_eq!(context.trace_id(), child.trace_id());
        assert_eq!(context.span_id(), child.span_id());
        assert_eq!(child.parent_span_id(), Some(root.span_id()));

        let event = tracer
            .event("index.operation.step.started", SystemTime::UNIX_EPOCH)
            .unwrap();

        assert_eq!(event.name(), "index.operation.step.started");
        assert_eq!(event.timestamp(), SystemTime::UNIX_EPOCH);
    }

    // -------------------------------------------------------------------------
    // Level 2: diagnostics facade integration
    // -------------------------------------------------------------------------

    #[test]
    fn diagnostics_facade_composes_core_diagnostic_details() {
        let diagnostics = IndexingDiagnostics::new();

        let diagnostic = diagnostics
            .create(
                DiagnosticKind::Operational,
                DiagnosticCondition::Degraded,
                "index requires attention",
            )
            .unwrap();

        let diagnostic = diagnostics
            .with_operation_id(diagnostic, "observability-level2-operation")
            .unwrap();

        let diagnostic = diagnostics
            .with_attempt_id(diagnostic, "observability-level2-attempt")
            .unwrap();

        let diagnostic = diagnostics
            .with_provider(diagnostic, "index-provider")
            .unwrap();

        assert_eq!(
            diagnostic.details().get("operation_id"),
            Some("observability-level2-operation")
        );

        assert_eq!(
            diagnostic.details().get("attempt_id"),
            Some("observability-level2-attempt")
        );

        assert_eq!(diagnostic.details().get("provider"), Some("index-provider"));

        assert_eq!(diagnostic.kind(), DiagnosticKind::Operational);

        assert_eq!(diagnostic.condition(), DiagnosticCondition::Degraded);
    }

    // -------------------------------------------------------------------------
    // Level 2: cross-adapter composition
    // -------------------------------------------------------------------------

    #[test]
    fn observability_adapters_remain_independent_core_backed_values() {
        let diagnostics = IndexingDiagnostics::new();
        let metrics = IndexingMetrics::new();
        let tracer = IndexingTracer::new();

        let diagnostic = diagnostics
            .create(
                DiagnosticKind::Operational,
                DiagnosticCondition::Degraded,
                "index operation observed",
            )
            .unwrap();

        let descriptor = nizaam_core::observability::MetricDescriptor::new(
            MetricName::new("index.observability.operations").unwrap(),
            MetricKind::Counter,
        );

        metrics
            .record_counter(&descriptor, MetricDimensions::new(), 1)
            .unwrap();

        let trace_id =
            nizaam_core::observability::TraceId::new("observability-composition-trace").unwrap();

        let span_id =
            nizaam_core::observability::SpanId::new("observability-composition-span").unwrap();

        let span = tracer
            .root_span(trace_id, span_id, "index.operation")
            .unwrap();

        assert_eq!(diagnostic.kind(), DiagnosticKind::Operational);

        assert_eq!(metrics.snapshot().unwrap().len(), 1);

        assert_eq!(tracer.context(&span).trace_id(), span.trace_id());
    }
}
