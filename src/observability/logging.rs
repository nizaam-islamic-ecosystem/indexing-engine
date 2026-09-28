//! Indexing logging integration with the Nizaam Core observability system.
//!
//! Phase 6 does not implement a second logging framework for the Indexing
//! Engine. Core remains authoritative for structured logging, event
//! validation, dispatch, buffering, subscribers, sinks, logging scopes,
//! logging sources, and logging instances.
//!
//! This module provides only the Indexing-facing adapter over Core's existing
//! [`nizaam_core::observability::ObservabilityLogger`].
//!
//! The intended boundary is:
//!
//! ```text
//! Indexing operation
//!       │
//!       ▼
//! IndexingLogger
//!       │
//!       ▼
//! Core ObservabilityLogger
//!       │
//!       ▼
//! Core LoggingInstance
//!       │
//!       ▼
//! Core LoggingSystem
//!       │
//!       ▼
//! Structured log
//! ```
//!
//! No Indexing-specific logging queue, dispatcher, sink, subscriber system,
//! event model, or error system is introduced here.
//!
//! Indexing-specific log meaning remains in the operation that emits the
//! Core [`nizaam_core::logging::LogEvent`]. Sensitive request payloads,
//! credentials, and other security-sensitive values must not be placed into
//! log metadata automatically.

use nizaam_core::logging::{
    DispatchOutcome, InstanceError, LogEvent, LogScope, LogSource, LoggingInstance,
};
use nizaam_core::observability::ObservabilityLogger;

/// Indexing-facing handle over an existing Core logging instance.
///
/// `IndexingLogger` owns no logging infrastructure. It only delegates
/// structured event publication to the Core observability logging adapter.
///
/// The lifetime is tied to the underlying Core [`LoggingInstance`], so the
/// Indexing adapter cannot outlive the logging instance it uses.
#[derive(Clone, Copy)]
pub struct IndexingLogger<'a> {
    core: ObservabilityLogger<'a>,
}

impl<'a> IndexingLogger<'a> {
    /// Creates an Indexing logging adapter over an existing Core logging
    /// instance.
    ///
    /// The caller is responsible for supplying the correctly scoped Core
    /// logging instance. For an engine-local logger this should be the Core
    /// local instance associated with the Indexing engine.
    pub fn new(instance: &'a LoggingInstance) -> Self {
        Self {
            core: ObservabilityLogger::new(instance),
        }
    }

    /// Emits a validated Core structured log event.
    ///
    /// Event validation and dispatch remain entirely owned by Core.
    ///
    /// Logging is intentionally not part of the Indexing correctness model:
    /// callers may handle the returned dispatch failure according to their
    /// operational policy without introducing an Indexing-specific logging
    /// error system.
    pub fn emit(&self, event: LogEvent) -> Result<DispatchOutcome, InstanceError> {
        self.core.emit(event)
    }

    /// Returns the logging scope enforced by the underlying Core instance.
    #[must_use]
    pub fn scope(&self) -> LogScope {
        self.core.scope()
    }

    /// Returns the logging source enforced by the underlying Core instance.
    #[must_use]
    pub fn source(&self) -> &LogSource {
        self.core.source()
    }
}

impl<'a> From<&'a LoggingInstance> for IndexingLogger<'a> {
    fn from(instance: &'a LoggingInstance) -> Self {
        Self::new(instance)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, mpsc};
    use std::time::Duration;

    use nizaam_core::events::EventName;
    use nizaam_core::identity::{CorrelationId, EngineId, OperationId};
    use nizaam_core::logging::{
        DispatchOutcome, LogContext, LogEvent, LogEventType, LogLevel, LogScope, LogSink,
        LogSource, LoggingSystem,
    };
    use nizaam_core::operation::{Operation, OperationContext};

    use super::*;

    struct RecordingSink(mpsc::Sender<LogEvent>);

    impl LogSink for RecordingSink {
        fn publish(&self, event: &LogEvent) {
            let _ = self.0.send(event.clone());
        }
    }

    fn operation_context() -> OperationContext {
        OperationContext::new(Operation::new(
            OperationId::new("indexing-logging-operation").unwrap(),
            CorrelationId::new("indexing-logging-correlation").unwrap(),
        ))
    }

    fn local_event(engine_id: EngineId) -> LogEvent {
        LogEvent::new(
            EventName::new("index.build.started").unwrap(),
            LogLevel::Info,
            LogSource::Engine(engine_id.clone()),
            LogScope::Local,
            "indexing-engine",
            LogContext::new(operation_context()).from_engine(engine_id),
            "index build started",
            LogEventType::Diagnostic,
        )
        .unwrap()
    }

    #[test]
    fn delegates_event_publication_to_core_logging_system() {
        let system = LoggingSystem::new(8).unwrap();
        let (sender, receiver) = mpsc::channel();

        system.subscribe(Arc::new(RecordingSink(sender)));

        let engine_id = EngineId::new("indexing-engine").unwrap();
        let instance = system.instance(LogScope::Local, LogSource::Engine(engine_id.clone()));
        let logger = IndexingLogger::new(&instance);
        let event = local_event(engine_id);

        assert_eq!(logger.emit(event.clone()).unwrap(), DispatchOutcome::Queued);

        let received = receiver
            .recv_timeout(Duration::from_secs(1))
            .expect("Core logging system should deliver the queued event");

        assert_eq!(received, event);

        system.shutdown().unwrap();
    }

    #[test]
    fn preserves_core_scope_and_source() {
        let system = LoggingSystem::new(1).unwrap();
        let engine_id = EngineId::new("indexing-engine").unwrap();

        let instance = system.instance(LogScope::Local, LogSource::Engine(engine_id.clone()));
        let logger = IndexingLogger::new(&instance);

        assert_eq!(logger.scope(), LogScope::Local);
        assert_eq!(logger.source(), &LogSource::Engine(engine_id));

        system.shutdown().unwrap();
    }

    #[test]
    fn core_remains_responsible_for_event_validation() {
        let system = LoggingSystem::new(1).unwrap();
        let engine_id = EngineId::new("indexing-engine").unwrap();

        let instance = system.instance(LogScope::Local, LogSource::Engine(engine_id.clone()));
        let logger = IndexingLogger::new(&instance);

        let event = LogEvent::new(
            EventName::new("index.build.started").unwrap(),
            LogLevel::Info,
            LogSource::Engine(engine_id.clone()),
            LogScope::Local,
            "indexing-engine",
            LogContext::new(operation_context()).from_engine(engine_id),
            "index build started",
            LogEventType::Diagnostic,
        )
        .unwrap()
        .with_metadata(" ", "sensitive-value");

        assert_eq!(
            logger.emit(event),
            Err(InstanceError::InvalidEvent(
                nizaam_core::logging::LogValidationError::EmptyMetadataField
            ))
        );

        system.shutdown().unwrap();
    }

    #[test]
    fn closed_core_logging_instance_returns_core_dispatch_error() {
        let system = LoggingSystem::new(1).unwrap();
        let engine_id = EngineId::new("indexing-engine").unwrap();

        let instance = system.instance(LogScope::Local, LogSource::Engine(engine_id.clone()));
        let logger = IndexingLogger::new(&instance);

        system.shutdown().unwrap();

        assert_eq!(
            logger.emit(local_event(engine_id)),
            Err(InstanceError::Dispatch(
                nizaam_core::logging::DispatchError::Closed
            ))
        );
    }

    #[test]
    fn conversion_from_core_logging_instance_preserves_the_same_boundary() {
        let system = LoggingSystem::new(1).unwrap();
        let engine_id = EngineId::new("indexing-engine").unwrap();

        let instance = system.instance(LogScope::Local, LogSource::Engine(engine_id));
        let logger = IndexingLogger::from(&instance);

        assert_eq!(logger.scope(), instance.scope());
        assert_eq!(logger.source(), instance.source());

        system.shutdown().unwrap();
    }
}
