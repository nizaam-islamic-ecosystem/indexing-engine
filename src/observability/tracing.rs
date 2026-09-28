//! Indexing tracing integration with the Nizaam Core observability system.
//!
//! Phase 6 does not implement a second tracing framework for the Indexing
//! Engine. Core remains authoritative for trace identifiers, span
//! identifiers, trace context, span relationships, span attributes, span
//! events, timing, validation, and completed spans.
//!
//! This module provides only the Indexing-facing facade over Core's existing
//! tracing primitives.
//!
//! The intended boundary is:
//!
//! ```text
//! Indexing operation
//!       │
//!       ▼
//! IndexingTracer
//!       │
//!       ├── Core Span::root
//!       │
//!       └── Core Span::child
//!                │
//!                ▼
//!          TraceContext
//!                │
//!                ▼
//!          CompletedSpan
//! ```
//!
//! No Indexing-specific span storage, registry, exporter, subscriber,
//! background worker, or tracing protocol is introduced here.
//!
//! Trace names should describe actual Indexing execution. Operation-specific
//! span hierarchies are therefore created by the operation integration rather
//! than being hard-coded into this module.

use nizaam_core::observability::{Span, SpanEvent, SpanId, TraceContext, TraceError, TraceId};

/// Stateless Indexing-facing facade over Core tracing.
///
/// `IndexingTracer` owns no tracing state. Every span, trace identifier,
/// relationship, event, attribute, and completed span remains a Core-owned
/// value.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct IndexingTracer;

impl IndexingTracer {
    /// Creates a stateless Indexing tracing facade.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Starts a root span using Core's tracing implementation.
    ///
    /// Core remains responsible for validating the trace identifier, span
    /// identifier, and span name.
    pub fn root_span(
        &self,
        trace_id: TraceId,
        span_id: SpanId,
        name: impl Into<String>,
    ) -> Result<Span, TraceError> {
        Span::root(trace_id, span_id, name)
    }

    /// Starts a child span using Core's tracing implementation.
    ///
    /// The child inherits the parent's trace identifier and records the
    /// parent's span identifier as its parent span.
    pub fn child_span(
        &self,
        parent: &Span,
        span_id: SpanId,
        name: impl Into<String>,
    ) -> Result<Span, TraceError> {
        Span::child(parent, span_id, name)
    }

    /// Returns the Core trace context associated with a span.
    ///
    /// This is a convenience boundary only; context construction and the
    /// underlying identifiers remain Core-owned.
    #[must_use]
    pub fn context(&self, span: &Span) -> TraceContext {
        span.context()
    }

    /// Creates a Core span event.
    ///
    /// Event validation remains entirely owned by Core.
    pub fn event(
        &self,
        name: impl Into<String>,
        timestamp: std::time::SystemTime,
    ) -> Result<SpanEvent, TraceError> {
        SpanEvent::new(name, timestamp)
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, SystemTime};

    use super::*;

    fn trace_id() -> TraceId {
        TraceId::new("indexing-trace-1").unwrap()
    }

    fn span_id(value: &str) -> SpanId {
        SpanId::new(value).unwrap()
    }

    #[test]
    fn creates_a_stateless_tracing_facade() {
        let tracer = IndexingTracer::new();

        assert_eq!(tracer, IndexingTracer);
    }

    #[test]
    fn creates_a_root_span_through_core() {
        let tracer = IndexingTracer::new();

        let span = tracer
            .root_span(trace_id(), span_id("root"), "index.query")
            .unwrap();

        assert_eq!(span.trace_id().as_str(), "indexing-trace-1");
        assert_eq!(span.span_id().as_str(), "root");
        assert_eq!(span.name(), "index.query");
        assert!(span.parent_span_id().is_none());
    }

    #[test]
    fn creates_child_spans_through_core() {
        let tracer = IndexingTracer::new();

        let root = tracer
            .root_span(trace_id(), span_id("root"), "index.query")
            .unwrap();

        let child = tracer
            .child_span(&root, span_id("consistency"), "consistency.check")
            .unwrap();

        assert_eq!(child.trace_id(), root.trace_id());
        assert_ne!(child.span_id(), root.span_id());
        assert_eq!(child.parent_span_id(), Some(root.span_id()));
        assert_eq!(child.name(), "consistency.check");
    }

    #[test]
    fn sibling_spans_preserve_the_same_trace_and_parent() {
        let tracer = IndexingTracer::new();

        let root = tracer
            .root_span(trace_id(), span_id("root"), "index.rebuild")
            .unwrap();

        let build = tracer
            .child_span(&root, span_id("build"), "candidate.build")
            .unwrap();

        let validation = tracer
            .child_span(&root, span_id("validation"), "integrity.validate")
            .unwrap();

        assert_eq!(build.trace_id(), validation.trace_id());
        assert_ne!(build.span_id(), validation.span_id());
        assert_eq!(build.parent_span_id(), Some(root.span_id()));
        assert_eq!(validation.parent_span_id(), Some(root.span_id()));
    }

    #[test]
    fn context_is_delegated_to_the_core_span() {
        let tracer = IndexingTracer::new();

        let span = tracer
            .root_span(trace_id(), span_id("root"), "index.query")
            .unwrap();

        let context = tracer.context(&span);

        assert_eq!(context.trace_id(), span.trace_id());
        assert_eq!(context.span_id(), span.span_id());
    }

    #[test]
    fn creates_events_through_core() {
        let tracer = IndexingTracer::new();
        let timestamp = SystemTime::UNIX_EPOCH;

        let event = tracer
            .event("provider.retrieve.started", timestamp)
            .unwrap();

        assert_eq!(event.name(), "provider.retrieve.started");
        assert_eq!(event.timestamp(), timestamp);
        assert!(event.attributes().is_empty());
    }

    #[test]
    fn core_validates_span_names() {
        let tracer = IndexingTracer::new();

        let result = tracer.root_span(trace_id(), span_id("root"), " ");

        assert!(matches!(result, Err(TraceError::InvalidSpanName)));
    }

    #[test]
    fn core_validates_span_identifiers() {
        assert_eq!(SpanId::new(" "), Err(TraceError::InvalidSpanId));
    }

    #[test]
    fn core_rejects_child_span_reusing_parent_id() {
        let tracer = IndexingTracer::new();

        let root = tracer
            .root_span(trace_id(), span_id("root"), "index.query")
            .unwrap();

        let result = tracer.child_span(&root, span_id("root"), "provider.retrieve");

        assert!(matches!(result, Err(TraceError::SpanIdMatchesParent)));
    }

    #[test]
    fn span_attributes_and_events_remain_core_owned() {
        let tracer = IndexingTracer::new();

        let mut span = tracer
            .root_span(trace_id(), span_id("root"), "index.query")
            .unwrap();

        span.set_attribute("component", "query").unwrap();

        let event = tracer
            .event("provider.retrieve.started", SystemTime::UNIX_EPOCH)
            .unwrap();

        span.add_event(event).unwrap();

        assert_eq!(span.attributes().get("component"), Some("query"));
        assert_eq!(span.events().len(), 1);
    }

    #[test]
    fn completed_span_is_produced_by_core() {
        let tracer = IndexingTracer::new();

        let span = tracer
            .root_span(trace_id(), span_id("root"), "index.rebuild")
            .unwrap();

        let completed = span.finish();

        assert_eq!(completed.trace_id().as_str(), "indexing-trace-1");
        assert_eq!(completed.span_id().as_str(), "root");
        assert_eq!(completed.name(), "index.rebuild");
        assert!(completed.duration() >= Duration::ZERO);
        assert!(completed.ended_at() >= completed.started_at());
    }

    #[test]
    fn tracer_has_no_operation_specific_state() {
        let first = IndexingTracer::new();
        let second = IndexingTracer::new();

        assert_eq!(first, second);
    }
}
