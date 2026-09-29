//! Immediate local recovery orchestration for Phase 5.
//!
//! Recovery is the operational layer between failure classification and the
//! existing Phase 3 construction/publication machinery:
//!
//! ```text
//! failure
//!    ↓
//! classify
//!    ↓
//! RecoveryExecutor
//!    ↓
//! recovery action
//!    ↓
//! Phase 3 build/update/rebuild path when required
//!    ↓
//! validate
//!    ↓
//! publish
//! ```
//!
//! This module deliberately does not own:
//! - Core retry policy, retry budgets, or retry scheduling;
//! - Core runtime or task scheduling;
//! - the active-version registry;
//! - physical provider mechanics;
//! - source-engine semantics;
//! - validation or publication implementation.
//!
//! The executor therefore selects and dispatches deterministic local recovery
//! actions. The supplied [`RecoveryHandler`] performs the subsystem-specific
//! operation. This keeps recovery immediate and composable without creating a
//! second scheduler or retry system.

use super::failure::{ClassifiedFailure, FailureClass};
use crate::index::IndexVersionId;
use core::fmt;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

use crate::engine::runtime::json_string_field;

/// A durable Index Assignment operation that was started but not completed.
///
/// This is the recovery module's filesystem-facing descriptor. It contains
/// identity and snapshot locations only; it does not deserialize an
/// `IndexEvent`, and it does not grant retry authority. A recovery handler may
/// use the event snapshot to reconstruct the typed event and must still route
/// any retry through the normal Core-approved execution path.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PersistedRecoveryOperation {
    index_id_hex: String,
    event_id: String,
    operation_id: String,
    event_snapshot: PathBuf,
    response_snapshot: Option<PathBuf>,
    operation_lock: PathBuf,
}

impl PersistedRecoveryOperation {
    fn new(
        index_id_hex: String,
        event_id: String,
        operation_id: String,
        event_snapshot: PathBuf,
        response_snapshot: Option<PathBuf>,
        operation_lock: PathBuf,
    ) -> Self {
        Self {
            index_id_hex,
            event_id,
            operation_id,
            event_snapshot,
            response_snapshot,
            operation_lock,
        }
    }

    /// Returns the persisted Index Assignment Operation ID as lowercase hex.
    #[must_use]
    pub fn index_id_hex(&self) -> &str {
        &self.index_id_hex
    }

    /// Returns the persisted Core EventId.
    #[must_use]
    pub fn event_id(&self) -> &str {
        &self.event_id
    }

    /// Returns the persisted Core OperationId.
    #[must_use]
    pub fn operation_id(&self) -> &str {
        &self.operation_id
    }

    /// Returns the durable IndexEvent snapshot path.
    #[must_use]
    pub fn event_snapshot(&self) -> &Path {
        &self.event_snapshot
    }

    /// Returns the durable response snapshot path, when one was found.
    #[must_use]
    pub fn response_snapshot(&self) -> Option<&Path> {
        self.response_snapshot.as_deref()
    }

    /// Returns the event-scoped operation lock path.
    #[must_use]
    pub fn operation_lock(&self) -> &Path {
        &self.operation_lock
    }
}

/// Scans the Indexing operation journal for operations that were started
/// without a matching completed record.
///
/// The scanner is deliberately read-only. It never marks an operation
/// completed and never retries an operation itself.
pub fn scan_incomplete_operations(
    root_directory: impl AsRef<Path>,
) -> Result<Vec<PersistedRecoveryOperation>, RecoveryExecutionError> {
    let root_directory = root_directory.as_ref();

    let entries = match fs::read_dir(root_directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => {
            return Err(RecoveryExecutionError::new(
                RecoveryAction::Rebuild,
                format!(
                    "could not scan recovery directory {}: {error}",
                    root_directory.display()
                ),
            ));
        }
    };

    let mut operations = Vec::new();

    for entry in entries {
        let entry = entry.map_err(|error| {
            RecoveryExecutionError::new(
                RecoveryAction::Rebuild,
                format!("could not read recovery directory entry: {error}"),
            )
        })?;

        let path = entry.path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("jsonl") {
            continue;
        }

        let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };

        let Some(index_id_hex) = file_name
            .strip_prefix("index-assignment-")
            .and_then(|name| name.strip_suffix(".jsonl"))
        else {
            continue;
        };

        if index_id_hex.is_empty() {
            continue;
        }

        if let Some(operation) = scan_journal_for_incomplete_operation(&path, index_id_hex)? {
            operations.push(operation);
        }
    }

    operations.sort_by(|left, right| {
        left.index_id_hex
            .cmp(&right.index_id_hex)
            .then_with(|| left.event_id.cmp(&right.event_id))
            .then_with(|| left.operation_id.cmp(&right.operation_id))
    });

    Ok(operations)
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct JournalOccurrence {
    event_id: String,
    operation_id: String,
    status: String,
    event_snapshot: Option<PathBuf>,
    response_snapshot: Option<PathBuf>,
}

fn scan_journal_for_incomplete_operation(
    journal_path: &Path,
    index_id_hex: &str,
) -> Result<Option<PersistedRecoveryOperation>, RecoveryExecutionError> {
    let contents = fs::read_to_string(journal_path).map_err(|error| {
        RecoveryExecutionError::new(
            RecoveryAction::Rebuild,
            format!(
                "could not read recovery journal {}: {error}",
                journal_path.display()
            ),
        )
    })?;

    let mut occurrences = Vec::<JournalOccurrence>::new();

    for line in contents.lines() {
        let Some(status) = json_string_field(line, "status") else {
            continue;
        };
        if status != "started" && status != "completed" {
            continue;
        }

        let Some(event_id) = json_string_field(line, "event_id") else {
            continue;
        };
        let Some(operation_id) = json_string_field(line, "operation_id") else {
            continue;
        };

        occurrences.push(JournalOccurrence {
            event_id,
            operation_id,
            status,
            event_snapshot: json_string_field(line, "event_snapshot").map(PathBuf::from),
            response_snapshot: json_string_field(line, "response_snapshot").map(PathBuf::from),
        });
    }

    // A journal is append-only. Pair each started occurrence with the first
    // later completed occurrence having the same event/operation identity.
    // If no such completion exists, the operation is recoverable.
    for (index, started) in occurrences
        .iter()
        .enumerate()
        .filter(|(_, occurrence)| occurrence.status == "started")
    {
        let completed = occurrences[index + 1..].iter().any(|occurrence| {
            occurrence.status == "completed"
                && occurrence.event_id == started.event_id
                && occurrence.operation_id == started.operation_id
        });

        if !completed {
            let event_snapshot = started.event_snapshot.clone().ok_or_else(|| {
                RecoveryExecutionError::new(
                    RecoveryAction::Rebuild,
                    format!(
                        "started operation {} has no event snapshot path",
                        started.operation_id
                    ),
                )
            })?;

            let response_snapshot = started
                .response_snapshot
                .clone()
                .filter(|path| !path.as_os_str().is_empty())
                .or_else(|| {
                    let derived = journal_path
                        .parent()
                        .unwrap_or_else(|| Path::new("."))
                        .join(format!("index-assignment-{index_id_hex}"))
                        .join("responses")
                        .join(format!(
                            "response-{}.snapshot",
                            hex_encode(started.event_id.as_bytes())
                        ));
                    derived.exists().then_some(derived)
                });

            let operation_lock = journal_path
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .join(format!("index-assignment-{index_id_hex}"))
                .join("locks")
                .join(format!(
                    "event-{}.lock",
                    hex_encode(started.event_id.as_bytes())
                ));

            return Ok(Some(PersistedRecoveryOperation::new(
                index_id_hex.to_owned(),
                started.event_id.clone(),
                started.operation_id.clone(),
                event_snapshot,
                response_snapshot,
                operation_lock,
            )));
        }
    }

    Ok(None)
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);

    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }

    output
}

/// Recovery action selected for an Indexing failure.
///
/// The action describes the next Indexing-owned recovery operation. It does
/// not itself perform the operation and it does not encode Core retry policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecoveryAction {
    /// Synchronize or incrementally update a stale index when that path is
    /// known to be safe.
    Synchronize,

    /// Isolate the affected state and construct a replacement through the
    /// Phase 3 rebuild path.
    Rebuild,

    /// Invalidate the unusable candidate and rebuild or restore it through the
    /// normal validated publication path.
    InvalidateAndRebuild,

    /// Restore availability without replacing the logical index when the
    /// surrounding lifecycle/provider boundary can safely do so.
    RestoreAvailability,

    /// Apply bounded capacity behavior. This means defer/throttle/reject at
    /// the local admission boundary, not queue indefinitely.
    Throttle,

    /// Surface the provider failure through Core's retry/reliability boundary.
    /// This action does not perform a retry itself.
    DelegateToCoreRetry,

    /// Preserve the last known-good state and surface the source-data failure.
    PreserveSafeState,

    /// Surface a query failure without changing index state.
    SurfaceQueryFailure,
}

impl RecoveryAction {
    /// Returns the stable logical name of the recovery action.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Synchronize => "synchronize",
            Self::Rebuild => "rebuild",
            Self::InvalidateAndRebuild => "invalidate_and_rebuild",
            Self::RestoreAvailability => "restore_availability",
            Self::Throttle => "throttle",
            Self::DelegateToCoreRetry => "delegate_to_core_retry",
            Self::PreserveSafeState => "preserve_safe_state",
            Self::SurfaceQueryFailure => "surface_query_failure",
        }
    }
}

impl fmt::Display for RecoveryAction {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Deterministically selects the initial recovery action for an Indexing
/// failure class.
///
/// The mapping follows the Phase 5 recovery scope. It is intentionally pure so
/// callers can inspect the selected action before invoking execution.
#[must_use]
pub const fn action_for(class: FailureClass) -> RecoveryAction {
    match class {
        FailureClass::StaleIndex => RecoveryAction::Synchronize,
        FailureClass::CorruptIndex => RecoveryAction::Rebuild,
        FailureClass::InvalidIndex => RecoveryAction::InvalidateAndRebuild,
        FailureClass::UnavailableIndex => RecoveryAction::RestoreAvailability,
        FailureClass::ResourceExhaustion => RecoveryAction::Throttle,
        FailureClass::ProviderFailure => RecoveryAction::DelegateToCoreRetry,
        FailureClass::SourceDataFailure => RecoveryAction::PreserveSafeState,
        FailureClass::QueryFailure => RecoveryAction::SurfaceQueryFailure,
    }
}

/// Input to one recovery execution.
///
/// `active_version` is observational lineage only. The executor never exposes
/// a mutation operation for it. A recovery implementation must construct a
/// candidate separately and return through validation/publication before that
/// candidate can replace the active version.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecoveryRequest {
    failure: ClassifiedFailure,
    active_version: Option<IndexVersionId>,
}

impl RecoveryRequest {
    /// Creates a recovery request for the supplied classified failure.
    #[must_use]
    pub const fn new(failure: ClassifiedFailure) -> Self {
        Self {
            failure,
            active_version: None,
        }
    }

    /// Associates the last known-good active version observed by the caller.
    ///
    /// This is carried as protection/lineage context only. Recovery never
    /// destroys or mutates this version.
    #[must_use]
    pub fn with_active_version(mut self, version: IndexVersionId) -> Self {
        self.active_version = Some(version);
        self
    }

    /// Returns the classified failure.
    #[must_use]
    pub const fn failure(&self) -> ClassifiedFailure {
        self.failure
    }

    /// Returns the observed active version, when supplied.
    #[must_use]
    pub fn active_version(&self) -> Option<&IndexVersionId> {
        self.active_version.as_ref()
    }

    /// Returns the deterministic initial action for this request.
    #[must_use]
    pub const fn action(&self) -> RecoveryAction {
        action_for(self.failure.class())
    }
}

/// Result of a successfully dispatched recovery operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecoveryOutcome {
    /// A synchronization/update operation was dispatched.
    Synchronized,

    /// An isolated rebuild operation was dispatched.
    Rebuilt,

    /// An invalid candidate was rejected and a rebuild was dispatched.
    InvalidatedAndRebuildRequested,

    /// Availability restoration was dispatched.
    AvailabilityRestored,

    /// Capacity handling was applied.
    Throttled,

    /// Provider failure was handed to Core's retry/reliability boundary.
    DelegatedToCoreRetry,

    /// The known-good state was deliberately preserved and the source failure
    /// was surfaced.
    SafeStatePreserved,

    /// The query failure was surfaced without index-state mutation.
    QueryFailureSurfaced,
}

impl RecoveryOutcome {
    /// Returns the action represented by this outcome.
    #[must_use]
    pub const fn action(self) -> RecoveryAction {
        match self {
            Self::Synchronized => RecoveryAction::Synchronize,
            Self::Rebuilt => RecoveryAction::Rebuild,
            Self::InvalidatedAndRebuildRequested => RecoveryAction::InvalidateAndRebuild,
            Self::AvailabilityRestored => RecoveryAction::RestoreAvailability,
            Self::Throttled => RecoveryAction::Throttle,
            Self::DelegatedToCoreRetry => RecoveryAction::DelegateToCoreRetry,
            Self::SafeStatePreserved => RecoveryAction::PreserveSafeState,
            Self::QueryFailureSurfaced => RecoveryAction::SurfaceQueryFailure,
        }
    }
}

/// Error returned by a recovery handler.
#[derive(Debug, Eq, PartialEq)]
pub struct RecoveryExecutionError {
    action: RecoveryAction,
    message: String,
}

impl RecoveryExecutionError {
    /// Creates a handler failure for the supplied recovery action.
    pub fn new(action: RecoveryAction, message: impl Into<String>) -> Self {
        Self {
            action,
            message: message.into(),
        }
    }

    /// Returns the action that failed.
    #[must_use]
    pub const fn action(&self) -> RecoveryAction {
        self.action
    }

    /// Returns the handler-provided failure message.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Normalizes a handler error to the recovery action selected by the executor.
    ///
    /// The handler may carry an action for its own local error context, but the
    /// executor owns the action actually selected for the current recovery
    /// request. Keeping this normalization in one place prevents individual
    /// dispatch branches from drifting apart.
    #[must_use]
    fn from_handler(action: RecoveryAction, error: Self) -> Self {
        Self::new(action, error.message)
    }
}

impl fmt::Display for RecoveryExecutionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "recovery action {} failed: {}",
            self.action, self.message
        )
    }
}

impl Error for RecoveryExecutionError {}

/// Immediate local recovery operation boundary.
///
/// Implementations perform the actual local operation using existing
/// Indexing/Phase 3 facilities. In particular, a rebuild handler should use
/// [`crate::build::rebuild::IndexRebuilder`] and return its unpublished result
/// to the normal validation/publication flow rather than modifying active
/// state directly.
pub trait RecoveryHandler {
    /// Synchronizes or incrementally updates a stale index.
    fn synchronize(&mut self, request: &RecoveryRequest) -> Result<(), RecoveryExecutionError>;

    /// Starts/executes an isolated rebuild for the affected index.
    fn rebuild(&mut self, request: &RecoveryRequest) -> Result<(), RecoveryExecutionError>;

    /// Invalidates the unusable candidate and starts its replacement rebuild.
    fn invalidate_and_rebuild(
        &mut self,
        request: &RecoveryRequest,
    ) -> Result<(), RecoveryExecutionError>;

    /// Restores availability without assuming that a rebuild is necessary.
    fn restore_availability(
        &mut self,
        request: &RecoveryRequest,
    ) -> Result<(), RecoveryExecutionError>;

    /// Applies bounded local capacity handling.
    fn throttle(&mut self, request: &RecoveryRequest) -> Result<(), RecoveryExecutionError>;

    /// Delegates provider retry handling to Core.
    fn delegate_to_core_retry(
        &mut self,
        request: &RecoveryRequest,
    ) -> Result<(), RecoveryExecutionError>;

    /// Preserves the last known-good state and surfaces the source failure.
    fn preserve_safe_state(
        &mut self,
        request: &RecoveryRequest,
    ) -> Result<(), RecoveryExecutionError>;

    /// Surfaces a query failure without mutating index state.
    fn surface_query_failure(
        &mut self,
        request: &RecoveryRequest,
    ) -> Result<(), RecoveryExecutionError>;

    /// Recovers one durable incomplete operation.
    ///
    /// The default implementation deliberately refuses filesystem recovery so
    /// existing handlers remain source-compatible. A concrete handler that
    /// supports crash recovery must reconstruct the typed `IndexEvent` from
    /// [`PersistedRecoveryOperation::event_snapshot`], reconcile any response
    /// snapshot, and invoke the normal Core-approved execution path. It must
    /// not implement an independent retry scheduler.
    fn recover_persisted_operation(
        &mut self,
        _operation: &PersistedRecoveryOperation,
    ) -> Result<(), RecoveryExecutionError> {
        Err(RecoveryExecutionError::new(
            RecoveryAction::Rebuild,
            "persistent recovery is not implemented by this RecoveryHandler",
        ))
    }
}

/// Stateless dispatcher for immediate Indexing-local recovery.
///
/// The executor owns dispatch semantics, not the underlying runtime. It calls
/// one handler operation immediately and returns only after that operation has
/// completed or failed.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RecoveryExecutor;

impl RecoveryExecutor {
    /// Creates the stateless recovery executor.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Returns the deterministic action for a classified failure.
    #[must_use]
    pub const fn action_for(&self, failure: FailureClass) -> RecoveryAction {
        action_for(failure)
    }

    /// Executes the selected local recovery operation immediately.
    ///
    /// No retry loop is performed here. Provider retry is explicitly delegated
    /// to Core, and every replacement-index path remains unpublished until the
    /// normal validate → publish boundary is completed by the caller.
    pub fn execute<H: RecoveryHandler>(
        &self,
        request: &RecoveryRequest,
        handler: &mut H,
    ) -> Result<RecoveryOutcome, RecoveryExecutionError> {
        match request.action() {
            RecoveryAction::Synchronize => {
                handler.synchronize(request).map_err(|error| {
                    RecoveryExecutionError::from_handler(RecoveryAction::Synchronize, error)
                })?;
                Ok(RecoveryOutcome::Synchronized)
            }
            RecoveryAction::Rebuild => {
                handler.rebuild(request).map_err(|error| {
                    RecoveryExecutionError::from_handler(RecoveryAction::Rebuild, error)
                })?;
                Ok(RecoveryOutcome::Rebuilt)
            }
            RecoveryAction::InvalidateAndRebuild => {
                handler.invalidate_and_rebuild(request).map_err(|error| {
                    RecoveryExecutionError::from_handler(
                        RecoveryAction::InvalidateAndRebuild,
                        error,
                    )
                })?;
                Ok(RecoveryOutcome::InvalidatedAndRebuildRequested)
            }
            RecoveryAction::RestoreAvailability => {
                handler.restore_availability(request).map_err(|error| {
                    RecoveryExecutionError::from_handler(RecoveryAction::RestoreAvailability, error)
                })?;
                Ok(RecoveryOutcome::AvailabilityRestored)
            }
            RecoveryAction::Throttle => {
                handler.throttle(request).map_err(|error| {
                    RecoveryExecutionError::from_handler(RecoveryAction::Throttle, error)
                })?;
                Ok(RecoveryOutcome::Throttled)
            }
            RecoveryAction::DelegateToCoreRetry => {
                handler.delegate_to_core_retry(request).map_err(|error| {
                    RecoveryExecutionError::from_handler(RecoveryAction::DelegateToCoreRetry, error)
                })?;
                Ok(RecoveryOutcome::DelegatedToCoreRetry)
            }
            RecoveryAction::PreserveSafeState => {
                handler.preserve_safe_state(request).map_err(|error| {
                    RecoveryExecutionError::from_handler(RecoveryAction::PreserveSafeState, error)
                })?;
                Ok(RecoveryOutcome::SafeStatePreserved)
            }
            RecoveryAction::SurfaceQueryFailure => {
                handler.surface_query_failure(request).map_err(|error| {
                    RecoveryExecutionError::from_handler(RecoveryAction::SurfaceQueryFailure, error)
                })?;
                Ok(RecoveryOutcome::QueryFailureSurfaced)
            }
        }
    }

    /// Scans durable Indexing journals and dispatches every incomplete
    /// operation to the supplied recovery handler.
    ///
    /// This method performs discovery only. It does not deserialize events,
    /// retry operations, or write completion records itself. Those operations
    /// remain the responsibility of the recovery handler and the normal Core
    /// runtime path.
    pub fn recover_incomplete_operations<H: RecoveryHandler>(
        &self,
        root_directory: impl AsRef<Path>,
        handler: &mut H,
    ) -> Result<Vec<PersistedRecoveryOperation>, RecoveryExecutionError> {
        let operations = scan_incomplete_operations(root_directory)?;

        for operation in &operations {
            if operation.operation_lock().exists() {
                match fs::remove_file(operation.operation_lock()) {
                    Ok(()) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => {
                        return Err(RecoveryExecutionError::new(
                            RecoveryAction::Rebuild,
                            format!(
                                "could not reconcile recovery lock {}: {error}",
                                operation.operation_lock().display()
                            ),
                        ));
                    }
                }
            }

            handler.recover_persisted_operation(operation)?;
        }

        Ok(operations)
    }
}

/// Selects and immediately executes recovery for a classified failure.
pub fn execute_recovery<H: RecoveryHandler>(
    failure: ClassifiedFailure,
    active_version: Option<IndexVersionId>,
    handler: &mut H,
) -> Result<RecoveryOutcome, RecoveryExecutionError> {
    let request = match active_version {
        Some(version) => RecoveryRequest::new(failure).with_active_version(version),
        None => RecoveryRequest::new(failure),
    };

    RecoveryExecutor::new().execute(&request, handler)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct RecordingHandler {
        actions: Vec<RecoveryAction>,
    }

    impl RecoveryHandler for RecordingHandler {
        fn synchronize(&mut self, _: &RecoveryRequest) -> Result<(), RecoveryExecutionError> {
            self.actions.push(RecoveryAction::Synchronize);
            Ok(())
        }

        fn rebuild(&mut self, _: &RecoveryRequest) -> Result<(), RecoveryExecutionError> {
            self.actions.push(RecoveryAction::Rebuild);
            Ok(())
        }

        fn invalidate_and_rebuild(
            &mut self,
            _: &RecoveryRequest,
        ) -> Result<(), RecoveryExecutionError> {
            self.actions.push(RecoveryAction::InvalidateAndRebuild);
            Ok(())
        }

        fn restore_availability(
            &mut self,
            _: &RecoveryRequest,
        ) -> Result<(), RecoveryExecutionError> {
            self.actions.push(RecoveryAction::RestoreAvailability);
            Ok(())
        }

        fn throttle(&mut self, _: &RecoveryRequest) -> Result<(), RecoveryExecutionError> {
            self.actions.push(RecoveryAction::Throttle);
            Ok(())
        }

        fn delegate_to_core_retry(
            &mut self,
            _: &RecoveryRequest,
        ) -> Result<(), RecoveryExecutionError> {
            self.actions.push(RecoveryAction::DelegateToCoreRetry);
            Ok(())
        }

        fn preserve_safe_state(
            &mut self,
            _: &RecoveryRequest,
        ) -> Result<(), RecoveryExecutionError> {
            self.actions.push(RecoveryAction::PreserveSafeState);
            Ok(())
        }

        fn surface_query_failure(
            &mut self,
            _: &RecoveryRequest,
        ) -> Result<(), RecoveryExecutionError> {
            self.actions.push(RecoveryAction::SurfaceQueryFailure);
            Ok(())
        }
    }

    #[test]
    fn phase_five_failure_mapping_is_deterministic() {
        assert_eq!(
            action_for(FailureClass::StaleIndex),
            RecoveryAction::Synchronize
        );
        assert_eq!(
            action_for(FailureClass::CorruptIndex),
            RecoveryAction::Rebuild
        );
        assert_eq!(
            action_for(FailureClass::InvalidIndex),
            RecoveryAction::InvalidateAndRebuild
        );
        assert_eq!(
            action_for(FailureClass::UnavailableIndex),
            RecoveryAction::RestoreAvailability
        );
        assert_eq!(
            action_for(FailureClass::ResourceExhaustion),
            RecoveryAction::Throttle
        );
        assert_eq!(
            action_for(FailureClass::ProviderFailure),
            RecoveryAction::DelegateToCoreRetry
        );
        assert_eq!(
            action_for(FailureClass::SourceDataFailure),
            RecoveryAction::PreserveSafeState
        );
        assert_eq!(
            action_for(FailureClass::QueryFailure),
            RecoveryAction::SurfaceQueryFailure
        );
    }

    #[test]
    fn executor_dispatches_immediately_without_retry_loop() {
        let executor = RecoveryExecutor::new();
        let mut handler = RecordingHandler::default();

        for class in FailureClass::ALL {
            let request = RecoveryRequest::new(ClassifiedFailure::new(class));
            let outcome = executor
                .execute(&request, &mut handler)
                .expect("recovery succeeds");
            assert_eq!(outcome.action(), action_for(class));
        }

        assert_eq!(handler.actions.len(), FailureClass::ALL.len());
    }

    #[test]
    fn active_version_is_observational_and_preserved() {
        let active = IndexVersionId::new("v7").expect("valid version");
        let request = RecoveryRequest::new(ClassifiedFailure::new(FailureClass::CorruptIndex))
            .with_active_version(active.clone());

        assert_eq!(request.active_version(), Some(&active));
        assert_eq!(request.action(), RecoveryAction::Rebuild);
    }

    #[test]
    fn source_failure_preserves_safe_state() {
        let mut handler = RecordingHandler::default();
        let outcome = execute_recovery(
            ClassifiedFailure::new(FailureClass::SourceDataFailure),
            None,
            &mut handler,
        )
        .expect("recovery dispatch succeeds");

        assert_eq!(outcome, RecoveryOutcome::SafeStatePreserved);
        assert_eq!(handler.actions, vec![RecoveryAction::PreserveSafeState]);
    }

    #[test]
    fn provider_failure_is_delegated_instead_of_retried_locally() {
        let mut handler = RecordingHandler::default();
        let outcome = execute_recovery(
            ClassifiedFailure::new(FailureClass::ProviderFailure),
            None,
            &mut handler,
        )
        .expect("delegation succeeds");

        assert_eq!(outcome, RecoveryOutcome::DelegatedToCoreRetry);
        assert_eq!(handler.actions, vec![RecoveryAction::DelegateToCoreRetry]);
    }

    #[test]
    fn handler_failure_is_normalized_to_the_selected_recovery_action() {
        let error = RecoveryExecutionError::new(
            RecoveryAction::Rebuild,
            "handler reported a local failure",
        );

        let normalized = RecoveryExecutionError::from_handler(RecoveryAction::Synchronize, error);

        assert_eq!(normalized.action(), RecoveryAction::Synchronize);
        assert_eq!(normalized.message(), "handler reported a local failure");
    }

    #[test]
    fn scanner_finds_started_operation_without_completion() {
        let root = std::env::temp_dir().join(format!(
            "nizaam-indexing-recovery-test-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("recovery test directory must be created");

        let journal = root.join("index-assignment-0011.jsonl");
        std::fs::write(
            &journal,
            concat!(
                "{\"status\":\"started\",\"event_id\":\"event-1\",",
                "\"operation_id\":\"operation-1\",",
                "\"event_snapshot\":\"events/received-event-1.snapshot\"}\n"
            ),
        )
        .expect("journal must be written");

        let operations = scan_incomplete_operations(&root).expect("journal scan must succeed");

        assert_eq!(operations.len(), 1);
        assert_eq!(operations[0].index_id_hex(), "0011");
        assert_eq!(operations[0].event_id(), "event-1");
        assert_eq!(operations[0].operation_id(), "operation-1");
        assert_eq!(
            operations[0].event_snapshot(),
            Path::new("events/received-event-1.snapshot")
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn scanner_derives_a_response_snapshot_saved_before_a_crash() {
        let root = std::env::temp_dir().join(format!(
            "nizaam-indexing-recovery-response-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock should be after UNIX epoch")
                .as_nanos()
        ));
        let operation_dir = root.join("index-assignment-abc");
        let response_dir = operation_dir.join("responses");
        fs::create_dir_all(&response_dir).unwrap();

        fs::write(
            root.join("index-assignment-abc.jsonl"),
            "{\"event_id\":\"event-1\",\"operation_id\":\"operation-1\",\"event_snapshot\":\"event.snapshot\",\"response_snapshot\":\"\",\"status\":\"started\"}\n",
        )
        .unwrap();
        let expected_response = response_dir.join("response-6576656e742d31.snapshot");
        fs::write(&expected_response, "response").unwrap();

        let operations = scan_incomplete_operations(&root).unwrap();
        assert_eq!(operations.len(), 1);
        assert_eq!(
            operations[0].response_snapshot(),
            Some(expected_response.as_path())
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn scanner_does_not_return_completed_operation() {
        let root = std::env::temp_dir().join(format!(
            "nizaam-indexing-recovery-complete-test-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("recovery test directory must be created");

        let journal = root.join("index-assignment-0022.jsonl");
        std::fs::write(
            &journal,
            concat!(
                "{\"status\":\"started\",\"event_id\":\"event-2\",",
                "\"operation_id\":\"operation-2\",",
                "\"event_snapshot\":\"events/received-event-2.snapshot\"}\n",
                "{\"status\":\"completed\",\"event_id\":\"event-2\",",
                "\"operation_id\":\"operation-2\",",
                "\"event_snapshot\":\"events/received-event-2.snapshot\",",
                "\"response_snapshot\":\"responses/response-event-2.snapshot\"}\n"
            ),
        )
        .expect("journal must be written");

        let operations = scan_incomplete_operations(&root).expect("journal scan must succeed");

        assert!(operations.is_empty());

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn executor_dispatches_incomplete_operations_to_handler() {
        struct PersistedHandler {
            operations: Vec<String>,
        }

        impl RecoveryHandler for PersistedHandler {
            fn synchronize(&mut self, _: &RecoveryRequest) -> Result<(), RecoveryExecutionError> {
                Ok(())
            }

            fn rebuild(&mut self, _: &RecoveryRequest) -> Result<(), RecoveryExecutionError> {
                Ok(())
            }

            fn invalidate_and_rebuild(
                &mut self,
                _: &RecoveryRequest,
            ) -> Result<(), RecoveryExecutionError> {
                Ok(())
            }

            fn restore_availability(
                &mut self,
                _: &RecoveryRequest,
            ) -> Result<(), RecoveryExecutionError> {
                Ok(())
            }

            fn throttle(&mut self, _: &RecoveryRequest) -> Result<(), RecoveryExecutionError> {
                Ok(())
            }

            fn delegate_to_core_retry(
                &mut self,
                _: &RecoveryRequest,
            ) -> Result<(), RecoveryExecutionError> {
                Ok(())
            }

            fn preserve_safe_state(
                &mut self,
                _: &RecoveryRequest,
            ) -> Result<(), RecoveryExecutionError> {
                Ok(())
            }

            fn surface_query_failure(
                &mut self,
                _: &RecoveryRequest,
            ) -> Result<(), RecoveryExecutionError> {
                Ok(())
            }

            fn recover_persisted_operation(
                &mut self,
                operation: &PersistedRecoveryOperation,
            ) -> Result<(), RecoveryExecutionError> {
                self.operations.push(operation.operation_id().to_owned());
                Ok(())
            }
        }

        let root = std::env::temp_dir().join(format!(
            "nizaam-indexing-recovery-dispatch-test-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("recovery test directory must be created");

        std::fs::write(
            root.join("index-assignment-0033.jsonl"),
            concat!(
                "{\"status\":\"started\",\"event_id\":\"event-3\",",
                "\"operation_id\":\"operation-3\",",
                "\"event_snapshot\":\"events/received-event-3.snapshot\"}\n"
            ),
        )
        .expect("journal must be written");

        let lock_path = root
            .join("index-assignment-0033")
            .join("locks")
            .join(format!("event-{}.lock", hex_encode(b"event-3")));
        std::fs::create_dir_all(lock_path.parent().expect("lock parent should exist"))
            .expect("lock directory must be created");
        std::fs::write(&lock_path, b"stale lock").expect("stale lock must be written");

        let mut handler = PersistedHandler {
            operations: Vec::new(),
        };
        let executor = RecoveryExecutor::new();
        let recovered = executor
            .recover_incomplete_operations(&root, &mut handler)
            .expect("persistent recovery dispatch must succeed");

        assert_eq!(recovered.len(), 1);
        assert_eq!(handler.operations, vec!["operation-3".to_owned()]);
        assert!(
            !lock_path.exists(),
            "recovery must reconcile the stale event lock"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn handler_failure_is_propagated_without_fallback_or_second_attempt() {
        struct FailingHandler;

        impl RecoveryHandler for FailingHandler {
            fn synchronize(&mut self, _: &RecoveryRequest) -> Result<(), RecoveryExecutionError> {
                Err(RecoveryExecutionError::new(
                    RecoveryAction::Rebuild,
                    "synchronization unavailable",
                ))
            }

            fn rebuild(&mut self, _: &RecoveryRequest) -> Result<(), RecoveryExecutionError> {
                Ok(())
            }

            fn invalidate_and_rebuild(
                &mut self,
                _: &RecoveryRequest,
            ) -> Result<(), RecoveryExecutionError> {
                Ok(())
            }

            fn restore_availability(
                &mut self,
                _: &RecoveryRequest,
            ) -> Result<(), RecoveryExecutionError> {
                Ok(())
            }

            fn throttle(&mut self, _: &RecoveryRequest) -> Result<(), RecoveryExecutionError> {
                Ok(())
            }

            fn delegate_to_core_retry(
                &mut self,
                _: &RecoveryRequest,
            ) -> Result<(), RecoveryExecutionError> {
                Ok(())
            }

            fn preserve_safe_state(
                &mut self,
                _: &RecoveryRequest,
            ) -> Result<(), RecoveryExecutionError> {
                Ok(())
            }

            fn surface_query_failure(
                &mut self,
                _: &RecoveryRequest,
            ) -> Result<(), RecoveryExecutionError> {
                Ok(())
            }
        }

        let mut handler = FailingHandler;
        let error = execute_recovery(
            ClassifiedFailure::new(FailureClass::StaleIndex),
            None,
            &mut handler,
        )
        .expect_err("handler failure must propagate");

        assert_eq!(error.action(), RecoveryAction::Synchronize);
        assert_eq!(error.message(), "synchronization unavailable");
    }
}
