//! Error boundary for the Nizaam Indexing Engine.
//!
//! Phase 0 does not introduce a second error taxonomy. Shared technical error
//! occurrences are represented by the Core `ErrorEvent`, while the underlying
//! `GlobalError` remains the structured error payload carried by that event.
//!
//! Indexing-specific error categories should be added here only when a real
//! Indexing-owned responsibility requires one in a later implementation phase.
//!
//! This keeps the dependency direction explicit:
//!
//! ```text
//! Indexing
//!     ↓
//! nizaam-core::error::ErrorEvent
//!     ↓
//! nizaam-core::error::GlobalError
//! ```
//!
//! The Error System therefore remains owned by Core rather than being replaced
//! by a competing Indexing-local error system.

use nizaam_core::error::ErrorEvent;

/// Result type used by Indexing Engine boundaries.
///
/// The error occurrence contract remains owned by `nizaam-core::error::ErrorEvent`.
/// `ErrorEvent` carries the Core `GlobalError` payload together with its
/// universal-event occurrence metadata.
pub type IndexingResult<T> = std::result::Result<T, ErrorEvent>;

#[cfg(test)]
mod tests {
    use super::IndexingResult;
    use nizaam_core::contracts::Version;
    use nizaam_core::error::{
        ErrorClass, ErrorCode, ErrorContext, ErrorEvent, ErrorOwner, GlobalError, Severity,
    };
    use nizaam_core::identity::{CorrelationId, OperationId};
    use nizaam_core::operation::{Operation, OperationContext};
    use nizaam_core::status::Retryability;

    fn operation_context() -> OperationContext {
        OperationContext::new(Operation::new(
            OperationId::new("indexing-error-test-operation")
                .expect("test operation id must be valid"),
            CorrelationId::new("indexing-error-test-correlation")
                .expect("test correlation id must be valid"),
        ))
    }

    fn core_error() -> GlobalError {
        GlobalError {
            code: ErrorCode::new("CORE.TEST.001").expect("test error code must be valid"),
            owner: ErrorOwner::new("CORE").expect("test error owner must be valid"),
            version: Version::new(1, 0, 0),
            class: ErrorClass::Contract,
            severity: Severity::Error,
            retryability: Retryability::NonRetryable,
            message: "test error".to_owned(),
            details: Vec::new(),
            solution_reference: None,
            context: ErrorContext::new(operation_context()),
            cause: None,
        }
    }

    #[test]
    fn indexing_result_uses_core_error_event() {
        let result: IndexingResult<()> = Err(ErrorEvent::new(core_error()));

        match result {
            Ok(()) => panic!("expected the test result to contain a Core error event"),
            Err(event) => {
                assert_eq!(event.error().code.as_str(), "CORE.TEST.001");
                assert_eq!(event.error().owner.as_str(), "CORE");
                assert_eq!(event.event_type(), "error");
            }
        }
    }
}
