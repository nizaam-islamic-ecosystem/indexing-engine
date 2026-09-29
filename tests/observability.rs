//! Phase 6 observability integration tests for the Nizaam Indexing Engine.
//!
//! These tests verify that the Indexing observability adapters delegate to
//! the existing Core logging, metrics, tracing, and diagnostics mechanisms.
//!
//! They intentionally do not create an Indexing observability backend,
//! exporter, registry, queue, subscriber, error framework, or persistent
//! event bus.
//!
//! The tests also verify the architectural separation required by Phase 6:
//!
//! ```text
//! metric != log
//! trace  != log
//! diagnostic != error replacement
//! event != persistent event bus
//! ```

use std::sync::{Arc, mpsc};
use std::time::{Duration, SystemTime};

use nizaam_core::{
    contracts::{
        ContractDescriptor, ContractMetadata, EncodedPayload, Interaction, MessageEnvelope,
        Participants, PayloadDescriptor, UniversalRequest,
    },
    events::EventName,
    identity::{CapabilityId, ContractId, CorrelationId, EngineId, MessageId, OperationId},
    logging::{
        DispatchOutcome, LogContext, LogEvent, LogEventType, LogLevel, LogScope, LogSink,
        LogSource, LoggingSystem,
    },
    observability::{
        DiagnosticCondition, DiagnosticKind, DiagnosticSubject, MetricDescriptor, MetricDimensions,
        MetricKind, MetricValue,
    },
    operation::{Operation, OperationContext},
    prelude::Version,
};

use nizaam_indexing::{
    event::{EntityType, IndexEvent},
    identity::IndexNamespace,
    index::{
        ConsistencyRequirement, IndexFamily, KeyDefinition, KeyMaterial, ObjectReference,
        TargetReferenceType, Uniqueness,
    },
    observability::{IndexingDiagnostics, IndexingLogger, IndexingMetrics, IndexingTracer},
    requirement::IndexRequirement,
};

fn operation_context(name: &str) -> OperationContext {
    OperationContext::new(Operation::new(
        OperationId::new(format!("{name}.operation")).expect("operation id must be valid"),
        CorrelationId::new(format!("{name}.correlation")).expect("correlation id must be valid"),
    ))
}

fn engine_id(value: &str) -> EngineId {
    EngineId::new(value).expect("engine id must be valid")
}

fn logging_event() -> LogEvent {
    let engine = engine_id("indexing.observability");

    LogEvent::new(
        EventName::new("index.build.started").expect("event name must be valid"),
        LogLevel::Info,
        LogSource::Engine(engine.clone()),
        LogScope::Local,
        "indexing-engine",
        LogContext::new(operation_context("logging")).from_engine(engine),
        "index build started",
        LogEventType::Diagnostic,
    )
    .expect("Core logging event must be valid")
}

fn metric_descriptor(name: &str, kind: MetricKind) -> MetricDescriptor {
    MetricDescriptor::new(
        nizaam_core::observability::MetricName::new(name).expect("metric name must be valid"),
        kind,
    )
}

fn universal_request() -> UniversalRequest {
    let payload_descriptor =
        PayloadDescriptor::new("application/octet-stream", Version::new(1, 0, 0))
            .expect("payload descriptor must be valid");

    let descriptor = ContractDescriptor::new(
        ContractId::new("indexing.observability.event").expect("contract id must be valid"),
        CapabilityId::new("index.event").expect("capability id must be valid"),
        Version::new(1, 0, 0),
        Interaction::Request,
        payload_descriptor.clone(),
    );

    let metadata = ContractMetadata::new(
        descriptor,
        Participants::new(
            engine_id("indexing.observability.sender"),
            engine_id("indexing.observability.receiver"),
        ),
    );

    UniversalRequest::new(MessageEnvelope::new(
        MessageId::new("indexing-observability-message").expect("message id must be valid"),
        operation_context("event"),
        metadata,
        EncodedPayload::new(payload_descriptor, b"event-payload".to_vec()),
    ))
}

fn index_requirement() -> IndexRequirement {
    IndexRequirement::new(
        IndexNamespace::new("logical").expect("namespace must be valid"),
        IndexFamily::Inverted,
        KeyDefinition::new(["term"]).expect("key definition must be valid"),
        TargetReferenceType::new("source.object").expect("target reference type must be valid"),
        Uniqueness::NonUnique,
        ConsistencyRequirement::new("logical").expect("consistency requirement must be valid"),
        None,
        None,
    )
    .expect("index requirement must be valid")
}

struct RecordingSink(mpsc::Sender<LogEvent>);

impl LogSink for RecordingSink {
    fn publish(&self, event: &LogEvent) {
        let _ = self.0.send(event.clone());
    }
}

#[test]
fn logging_delegates_to_core_and_preserves_the_complete_core_event() {
    let system = LoggingSystem::new(4).expect("Core logging system must be valid");
    let (sender, receiver) = mpsc::channel();

    system.subscribe(Arc::new(RecordingSink(sender)));

    let instance = system.instance(
        LogScope::Local,
        LogSource::Engine(engine_id("indexing.observability")),
    );
    let logger = IndexingLogger::new(&instance);
    let event = logging_event();

    assert_eq!(
        logger.emit(event.clone()).expect("logging must succeed"),
        DispatchOutcome::Queued
    );

    let received = receiver
        .recv_timeout(Duration::from_secs(1))
        .expect("Core logging system must deliver the event");

    assert_eq!(received, event);
    assert_eq!(logger.scope(), LogScope::Local);
    assert_eq!(
        logger.source(),
        &LogSource::Engine(engine_id("indexing.observability"))
    );

    system
        .shutdown()
        .expect("Core logging system must shut down");
}

#[test]
fn logging_keeps_correlation_inside_core_log_context() {
    let event = logging_event();

    assert_eq!(
        event.context.operation.operation.id.as_str(),
        "logging.operation"
    );
    assert_eq!(
        event.context.operation.operation.correlation_id.as_str(),
        "logging.correlation"
    );
}

#[test]
fn metrics_delegate_to_core_without_creating_a_second_metric_store() {
    let metrics = IndexingMetrics::new();
    let descriptor = metric_descriptor("index.build.total", MetricKind::Counter);

    metrics
        .record_counter(&descriptor, MetricDimensions::new(), 1)
        .expect("counter recording must succeed");
    metrics
        .record_counter(&descriptor, MetricDimensions::new(), 2)
        .expect("counter recording must succeed");

    let snapshot = metrics.snapshot().expect("metric snapshot must succeed");

    assert_eq!(snapshot.len(), 1);
    assert_eq!(snapshot[0].value(), &MetricValue::Counter(3));
}

#[test]
fn metrics_preserve_bounded_core_dimensions() {
    let metrics = IndexingMetrics::new();
    let descriptor = metric_descriptor("index.query.total", MetricKind::Counter);
    let mut dimensions = MetricDimensions::new();

    let mut accepted = 0;

    loop {
        let key = format!("dimension-{accepted}");

        match dimensions.insert(key, "bounded") {
            Ok(()) => accepted += 1,
            Err(nizaam_core::observability::MetricError::DimensionLimitExceeded) => break,
            Err(error) => panic!("unexpected Core dimension validation error: {error:?}"),
        }
    }

    assert!(accepted > 0);

    metrics
        .record_counter(&descriptor, dimensions.clone(), 1)
        .expect("bounded metric dimensions must be accepted");

    assert_eq!(
        metrics.snapshot().expect("snapshot must succeed")[0]
            .dimensions()
            .len(),
        accepted
    );

    assert_eq!(
        dimensions.insert("overflow", "rejected"),
        Err(nizaam_core::observability::MetricError::DimensionLimitExceeded)
    );
}

#[test]
fn tracing_preserves_core_trace_and_span_relationships() {
    let tracer = IndexingTracer::new();

    let trace_id = nizaam_core::observability::TraceId::new("indexing-observability-trace")
        .expect("trace id must be valid");
    let root_id = nizaam_core::observability::SpanId::new("root").expect("span id must be valid");
    let child_id = nizaam_core::observability::SpanId::new("query").expect("span id must be valid");

    let root = tracer
        .root_span(trace_id, root_id, "index.query")
        .expect("root span must be valid");

    let child = tracer
        .child_span(&root, child_id, "provider.retrieve")
        .expect("child span must be valid");

    assert_eq!(child.trace_id(), root.trace_id());
    assert_eq!(child.parent_span_id(), Some(root.span_id()));

    let context = tracer.context(&child);

    assert_eq!(context.trace_id(), child.trace_id());
    assert_eq!(context.span_id(), child.span_id());
}

#[test]
fn tracing_events_are_core_span_events_not_logs_or_metrics() {
    let tracer = IndexingTracer::new();
    let event = tracer
        .event("provider.retrieve.started", SystemTime::UNIX_EPOCH)
        .expect("Core span event must be valid");

    assert_eq!(event.name(), "provider.retrieve.started");
    assert_eq!(event.timestamp(), SystemTime::UNIX_EPOCH);
    assert!(event.attributes().is_empty());
}

#[test]
fn diagnostics_are_core_observations_and_not_indexing_errors() {
    let diagnostics = IndexingDiagnostics::new();

    let diagnostic = diagnostics
        .create(
            DiagnosticKind::Operational,
            DiagnosticCondition::Degraded,
            "index requires attention",
        )
        .expect("Core diagnostic must be valid");

    let diagnostic = diagnostics
        .with_failure_class(
            diagnostic,
            nizaam_indexing::recovery::FailureClass::ProviderFailure,
        )
        .expect("failure classification detail must be accepted");

    let diagnostic = diagnostics.with_subject(diagnostic, DiagnosticSubject::Runtime);

    assert_eq!(diagnostic.kind(), DiagnosticKind::Operational);
    assert_eq!(diagnostic.condition(), DiagnosticCondition::Degraded);
    assert_eq!(diagnostic.subject(), Some(&DiagnosticSubject::Runtime));
    assert_eq!(
        diagnostic.details().get("failure_class"),
        Some("provider_failure")
    );
}

#[test]
fn diagnostics_can_carry_indexing_context_without_replacing_core_error_semantics() {
    let diagnostics = IndexingDiagnostics::new();

    let diagnostic = diagnostics
        .create(
            DiagnosticKind::Operational,
            DiagnosticCondition::Degraded,
            "provider failure observed",
        )
        .expect("Core diagnostic must be valid");

    let diagnostic = diagnostics
        .with_provider(diagnostic, "logical-provider")
        .expect("provider detail must be accepted");

    let diagnostic = diagnostics
        .with_consistency_state(diagnostic, "current")
        .expect("consistency detail must be accepted");

    let diagnostic = diagnostics
        .with_lifecycle_state(diagnostic, "published")
        .expect("lifecycle detail must be accepted");

    assert_eq!(
        diagnostic.details().get("provider"),
        Some("logical-provider")
    );
    assert_eq!(
        diagnostic.details().get("consistency_state"),
        Some("current")
    );
    assert_eq!(
        diagnostic.details().get("lifecycle_state"),
        Some("published")
    );
}

#[test]
fn typed_index_event_remains_core_universal_request_content() {
    let event = IndexEvent::new(
        universal_request(),
        EntityType::new("semantic").expect("entity type must be valid"),
        index_requirement(),
        ObjectReference::new("nizaam.test.sender", "object:1")
            .expect("object reference must be valid"),
        KeyMaterial::Null,
    )
    .expect("IndexEvent must be valid");

    assert_eq!(
        event.operation_context().operation.id.as_str(),
        "event.operation"
    );
    assert_eq!(
        event.operation_context().operation.correlation_id.as_str(),
        "event.correlation"
    );

    assert_eq!(
        event.object_reference(),
        &ObjectReference::new("nizaam.test.sender", "object:1")
            .expect("object reference must be valid")
    );
}

#[test]
fn observability_adapters_remain_independent_core_backed_values() {
    let logger_system = LoggingSystem::new(1).expect("Core logging system must be valid");
    let logger_instance = logger_system.instance(
        LogScope::Local,
        LogSource::Engine(engine_id("indexing.observability")),
    );

    let logger = IndexingLogger::new(&logger_instance);
    let metrics = IndexingMetrics::new();
    let tracer = IndexingTracer::new();
    let diagnostics = IndexingDiagnostics::new();

    assert_eq!(logger.scope(), LogScope::Local);
    assert!(
        metrics
            .snapshot()
            .expect("snapshot must succeed")
            .is_empty()
    );
    assert_eq!(tracer, IndexingTracer::new());

    let diagnostic = diagnostics
        .create(
            DiagnosticKind::Operational,
            DiagnosticCondition::Degraded,
            "independent diagnostic",
        )
        .expect("diagnostic must be valid");

    assert!(diagnostic.details().is_empty());

    logger_system
        .shutdown()
        .expect("Core logging system must shut down");
}
