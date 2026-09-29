//! Core runtime integration for the Nizaam Indexing Engine.
//!
//! Phase 0 intentionally does not implement a second runtime system. This
//! module is a thin Indexing-facing boundary over `nizaam_core::runtime::EngineRuntime`.
//! Core remains authoritative for lifecycle validity, request admission,
//! cancellation, deadlines, execution context, capability dispatch, and
//! shutdown coordination.
//!
//! The runtime lifecycle established here is:
//!
//! ```text
//! Created -> Starting -> Configuring -> Dependencies -> Capabilities -> Registering -> Ready -> Serving -> Draining -> Stopped
//! ```
//!
//! Engine registration and capability registration remain coordinated by the
//! surrounding engine composition layer. This module only exposes the runtime
//! boundary needed to make that composition explicit.

use nizaam_core::capability::{CapabilityDispatchResult, CapabilityInvocation};
use nizaam_core::error::InvalidTransition;
use nizaam_core::identity::{EngineId, EngineInstanceId};
use nizaam_core::operation::OperationContext;
use nizaam_core::runtime::{EngineContext, EngineRuntime, LifecycleState, RequestAdmissionError};

use super::capability::CapabilitySet;
use super::registration::IndexingRegistration;
use crate::capacity::{CapacityAccounting, CapacityAdmissionError, CapacityRequest};
use crate::event::{IndexEvent, IndexEventResponse, IndexEventValidationError};
use crate::identity::IndexDefinitionId;
use crate::index::IndexDefinition;
use nizaam_core::capability::{
    CapabilityDefinition, CapabilityError, CapabilityHandler, CapabilityOutcome,
    RegistryError as CapabilityRegistryError,
};
use nizaam_core::contracts::{
    EncodedPayload, Interaction, MessageEnvelope, Participants, UniversalRequest, UniversalResponse,
};
use nizaam_core::control_plane::registry::{
    EngineRegistry, RegistryError as ControlPlaneRegistryError,
};
use nizaam_core::identity::MessageId;
use nizaam_core::status::Status;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

/// Result returned by the Indexing runtime after lifecycle admission.
///
/// A request that is not admitted fails with the Core
/// [`RequestAdmissionError`]. Once admitted, capability lookup, cancellation,
/// deadline checks, and handler execution remain represented by Core's
/// [`CapabilityDispatchResult`].
pub type RuntimeDispatchResult = Result<CapabilityDispatchResult, RequestAdmissionError>;

/// Indexing-facing runtime adapter over the Core [`EngineRuntime`].
///
/// This type deliberately contains only the Core runtime. It does not define
/// its own lifecycle state machine, cancellation token, deadline mechanism, or
/// shutdown coordinator.
#[derive(Debug)]
pub struct IndexingRuntime {
    core: EngineRuntime,
}

impl IndexingRuntime {
    /// Creates a new Indexing runtime in the Core `Created` state.
    ///
    /// The logical `EngineId` and concrete `EngineInstanceId` are supplied to
    /// Core at construction and become the authoritative runtime identities.
    #[must_use]
    pub fn new(engine_id: EngineId, engine_instance_id: EngineInstanceId) -> Self {
        Self {
            core: EngineRuntime::new(engine_id, engine_instance_id),
        }
    }

    /// Returns the logical engine identity owned by the Core runtime.
    #[must_use]
    pub fn engine_id(&self) -> &EngineId {
        self.core.engine_id()
    }

    /// Returns the concrete engine-instance identity owned by the Core runtime.
    #[must_use]
    pub fn engine_instance_id(&self) -> &EngineInstanceId {
        self.core.instance_id()
    }

    /// Returns the current Core lifecycle state.
    #[must_use]
    pub fn state(&self) -> LifecycleState {
        self.core.state()
    }

    /// Applies one lifecycle transition through the Core runtime.
    ///
    /// Core remains responsible for validating the legal lifecycle graph and
    /// for preventing invalid or terminal transitions.
    pub(crate) fn transition(&self, next: LifecycleState) -> Result<(), InvalidTransition> {
        self.core.transition(next)
    }

    /// Performs the non-registration portion of Phase 0 startup.
    ///
    /// This advances the runtime through the Core-managed initialization states
    /// up to `Capabilities`:
    ///
    /// ```text
    /// Created
    ///   -> Starting
    ///   -> Configuring
    ///   -> Dependencies
    ///   -> Capabilities
    /// ```
    ///
    /// Engine registration is intentionally not hidden inside this method.
    /// The engine composition layer must keep registration distinct and must
    /// establish `Registering` before the runtime can become `Ready`.
    pub fn start(&self) -> Result<(), InvalidTransition> {
        for next in [
            LifecycleState::Starting,
            LifecycleState::Configuring,
            LifecycleState::Dependencies,
            LifecycleState::Capabilities,
        ] {
            self.transition(next)?;
        }

        Ok(())
    }

    /// Enters the Core `Registering` lifecycle state.
    ///
    /// Actual engine registration is owned by `registration.rs`. Keeping the
    /// transition separate ensures registration is not confused with startup
    /// mechanics or capability registration.
    pub(crate) fn begin_registration(&self) -> Result<(), InvalidTransition> {
        self.transition(LifecycleState::Registering)
    }

    /// Marks the runtime `Ready` after required Phase 0 registration work has
    /// completed successfully.
    ///
    /// This method is crate-visible so the engine composition layer remains the
    /// authority that sequences registration before readiness.
    pub(crate) fn mark_ready(&self) -> Result<(), InvalidTransition> {
        self.transition(LifecycleState::Ready)
    }

    /// Explicitly transitions a ready runtime into `Serving`.
    ///
    /// `Ready` and `Serving` remain separate. Normal request admission is not
    /// enabled merely by reaching `Ready`.
    pub(crate) fn serve(&self) -> Result<(), InvalidTransition> {
        self.transition(LifecycleState::Serving)
    }

    /// Explicitly begins draining through the Core lifecycle.
    ///
    /// After this transition, new normal requests are rejected by
    /// [`Self::admit_request`]. Work that already passed admission continues to
    /// use its existing Core execution context.
    pub fn drain(&self) -> Result<(), InvalidTransition> {
        self.transition(LifecycleState::Draining)
    }

    /// Applies Core request admission for one normal request.
    ///
    /// Core is authoritative here: only `Serving` admits new normal work.
    /// Admission is deliberately separate from capability dispatch so a
    /// rejected request never reaches capability resolution or a handler.
    pub fn admit_request(&self) -> Result<(), RequestAdmissionError> {
        self.core.admit_request()
    }

    /// Creates a Core [`EngineContext`] from an existing [`OperationContext`].
    ///
    /// No Indexing-specific operation identity, cancellation token, deadline,
    /// security container, or provenance container is created. Callers may
    /// continue enriching the returned Core context with the Core-provided
    /// `with_deadline`, `with_security`, `with_provenance`, or child-context
    /// APIs before capability execution.
    #[must_use]
    pub fn context(&self, operation: OperationContext) -> EngineContext {
        EngineContext::new(operation)
    }

    /// Admits and then dispatches one capability invocation.
    ///
    /// The ordering is intentionally:
    ///
    /// ```text
    /// runtime admission
    ///       -> Core capability dispatch
    ///       -> cancellation/deadline checks
    ///       -> capability resolution
    ///       -> handler invocation
    /// ```
    ///
    /// The runtime performs only the local admission boundary. Core remains
    /// responsible for the capability execution rules represented by
    /// [`CapabilityDispatchResult`].
    pub fn dispatch(
        &self,
        capabilities: &CapabilitySet,
        context: &EngineContext,
        invocation: &CapabilityInvocation,
    ) -> RuntimeDispatchResult {
        self.admit_request()?;
        Ok(capabilities.dispatch(context, invocation))
    }

    /// Gracefully shuts down through the Core runtime.
    ///
    /// Core owns the shutdown coordination, including transition into
    /// `Draining`, runtime-owned cleanup, and the terminal `Stopped` state.
    /// The boolean result is passed through unchanged because the Core runtime
    /// distinguishes normal external completion from shutdown initiated by a
    /// runtime-owned background task.
    pub fn shutdown(&self) -> Result<bool, InvalidTransition> {
        self.core.shutdown()
    }

    /// Returns the Core runtime shutdown cancellation token.
    #[must_use]
    pub fn shutdown_token(&self) -> &nizaam_core::runtime::CancellationToken {
        self.core.shutdown_token()
    }

    /// Returns the Core runtime owned background-task manager.
    ///
    /// This is exposed only as a direct Core integration surface. Phase 0 does
    /// not define an Indexing-specific task manager.
    #[must_use]
    pub fn background_tasks(&self) -> &nizaam_core::runtime::BackgroundTasks {
        self.core.background_tasks()
    }
}

// The engine facade is implemented here so `engine/mod.rs` remains a pure
// module/export boundary plus Level 2 tests. The facade still composes the
// Core-backed registration, runtime, and capability adapters defined by this
// engine module.
pub struct IndexingEngine {
    registration: IndexingRegistration,
    runtime: IndexingRuntime,
    capabilities: CapabilitySet,
    operation_root: PathBuf,
    engine_registered: AtomicBool,
    phase0_capability_registered: AtomicBool,
}

/// Thin facade-level composition error for setup operations.
///
/// Core lifecycle and registry failures are preserved directly. The ownership
/// mismatch is a local composition invariant of the Indexing Engine boundary,
/// where a public capability registration request must belong to this engine.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EngineSetupError {
    Lifecycle(InvalidTransition),
    Registry(ControlPlaneRegistryError),
    CapabilityRegistry(CapabilityRegistryError),
    CapabilityOwnerMismatch {
        capability_id: nizaam_core::identity::CapabilityId,
        expected_engine: EngineId,
        actual_engine: EngineId,
    },
}

impl std::fmt::Display for EngineSetupError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Lifecycle(error) => error.fmt(formatter),
            Self::Registry(error) => error.fmt(formatter),
            Self::CapabilityRegistry(error) => error.fmt(formatter),
            Self::CapabilityOwnerMismatch {
                capability_id,
                expected_engine,
                actual_engine,
            } => write!(
                formatter,
                "capability {} is owned by engine {}, expected {}",
                capability_id.as_str(),
                actual_engine.as_str(),
                expected_engine.as_str(),
            ),
        }
    }
}

impl std::error::Error for EngineSetupError {}

impl From<InvalidTransition> for EngineSetupError {
    fn from(error: InvalidTransition) -> Self {
        Self::Lifecycle(error)
    }
}

impl From<ControlPlaneRegistryError> for EngineSetupError {
    fn from(error: ControlPlaneRegistryError) -> Self {
        Self::Registry(error)
    }
}

impl From<CapabilityRegistryError> for EngineSetupError {
    fn from(error: CapabilityRegistryError) -> Self {
        Self::CapabilityRegistry(error)
    }
}

/// Error produced while handling a request at the Indexing Engine boundary.
///
/// Core remains authoritative for request admission. The additional target
/// errors here describe a local-engine routing invariant that is not part of
/// Core's lifecycle admission error contract.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RequestHandlingError {
    Admission(RequestAdmissionError),
    TargetEngineMismatch {
        expected: EngineId,
        actual: EngineId,
    },
    TargetInstanceMismatch {
        expected: EngineInstanceId,
        actual: EngineInstanceId,
    },
}

impl std::fmt::Display for RequestHandlingError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Admission(error) => error.fmt(formatter),
            Self::TargetEngineMismatch { expected, actual } => write!(
                formatter,
                "request target engine {} does not match local engine {}",
                actual.as_str(),
                expected.as_str(),
            ),
            Self::TargetInstanceMismatch { expected, actual } => write!(
                formatter,
                "request target instance {} does not match local instance {}",
                actual.as_str(),
                expected.as_str(),
            ),
        }
    }
}

impl std::error::Error for RequestHandlingError {}

impl From<RequestAdmissionError> for RequestHandlingError {
    fn from(error: RequestAdmissionError) -> Self {
        Self::Admission(error)
    }
}

/// Error produced while admitting a typed Indexing event through the engine
/// boundary.
///
/// Core remains authoritative for lifecycle admission and capability errors.
/// Indexing contributes only the local event-validation, target-routing, and
/// bounded-capacity failures required before Core dispatch.
#[derive(Debug, Eq, PartialEq)]
pub enum IndexEventHandlingError {
    Admission(RequestAdmissionError),
    TargetEngineMismatch {
        expected: EngineId,
        actual: EngineId,
    },
    TargetInstanceMismatch {
        expected: EngineInstanceId,
        actual: EngineInstanceId,
    },
    EventValidation(IndexEventValidationError),
    Capacity(CapacityAdmissionError),
    Capability(CapabilityError),
    DefinitionRequirementMismatch {
        definition_id: IndexDefinitionId,
    },
    IndexIdGeneration(crate::identity::IndexIdGenerationError),
    OperationInProgress {
        index_id: crate::identity::IndexId,
    },
    OperationAlreadyCompleted {
        index_id: crate::identity::IndexId,
    },
    OperationConflict {
        index_id: crate::identity::IndexId,
    },
    Persistence(String),
}

impl std::fmt::Display for IndexEventHandlingError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Admission(error) => error.fmt(formatter),
            Self::TargetEngineMismatch { expected, actual } => write!(
                formatter,
                "IndexEvent target engine {} does not match local engine {}",
                actual.as_str(),
                expected.as_str(),
            ),
            Self::TargetInstanceMismatch { expected, actual } => write!(
                formatter,
                "IndexEvent target instance {} does not match local instance {}",
                actual.as_str(),
                expected.as_str(),
            ),
            Self::EventValidation(error) => error.fmt(formatter),
            Self::Capacity(error) => error.fmt(formatter),
            Self::Capability(error) => error.fmt(formatter),
            Self::DefinitionRequirementMismatch { definition_id } => write!(
                formatter,
                "IndexDefinition {} does not match the IndexEvent requirement",
                definition_id.as_str(),
            ),
            Self::IndexIdGeneration(error) => error.fmt(formatter),
            Self::OperationInProgress { index_id } => write!(
                formatter,
                "Index Assignment Operation {} is already in progress",
                hex_encode(index_id.as_bytes()),
            ),
            Self::OperationAlreadyCompleted { index_id } => write!(
                formatter,
                "Index Assignment Operation {} is already completed",
                hex_encode(index_id.as_bytes()),
            ),
            Self::OperationConflict { index_id } => write!(
                formatter,
                "Index Assignment Operation {} conflicts with an existing event or operation identity",
                hex_encode(index_id.as_bytes()),
            ),
            Self::Persistence(error) => {
                write!(
                    formatter,
                    "Index Assignment Operation persistence failed: {error}"
                )
            }
        }
    }
}

impl std::error::Error for IndexEventHandlingError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Admission(error) => Some(error),
            Self::EventValidation(error) => Some(error),
            Self::Capacity(error) => Some(error),
            Self::Capability(error) => Some(error),
            Self::DefinitionRequirementMismatch { .. }
            | Self::IndexIdGeneration(_)
            | Self::OperationInProgress { .. }
            | Self::OperationAlreadyCompleted { .. }
            | Self::OperationConflict { .. }
            | Self::TargetEngineMismatch { .. }
            | Self::TargetInstanceMismatch { .. }
            | Self::Persistence(_) => None,
        }
    }
}

impl From<RequestAdmissionError> for IndexEventHandlingError {
    fn from(error: RequestAdmissionError) -> Self {
        Self::Admission(error)
    }
}

impl From<IndexEventValidationError> for IndexEventHandlingError {
    fn from(error: IndexEventValidationError) -> Self {
        Self::EventValidation(error)
    }
}

impl From<CapacityAdmissionError> for IndexEventHandlingError {
    fn from(error: CapacityAdmissionError) -> Self {
        Self::Capacity(error)
    }
}

/// Result of handling one admitted UniversalRequest.
///
/// Core's admission error remains preserved inside [`RequestHandlingError`],
/// while local request-target validation has its own boundary error. Successful
/// capability execution yields a Core `UniversalResponse`.
pub type UniversalRequestResult =
    Result<Result<UniversalResponse, CapabilityError>, RequestHandlingError>;

/// Filesystem location used for Indexing Assignment Operation records.
///
/// The operation journal is intentionally outside Core. Core owns transport,
/// lifecycle, admission, cancellation, deadlines, and capability execution;
/// Indexing owns the durable record of its typed assignment operation.
fn indexing_operation_directory(root_directory: &Path) -> Result<PathBuf, IndexEventHandlingError> {
    if root_directory.as_os_str().is_empty() {
        return Err(IndexEventHandlingError::Persistence(
            "Indexing operation root must not be empty".to_owned(),
        ));
    }

    Ok(root_directory.to_path_buf())
}

/// Returns the durable directory for one deterministic Index Assignment
/// Operation.
///
/// `IndexId` is the operation identity. Individual received events may belong
/// to the same deterministic operation, so event-specific snapshots use the
/// Core `EventId` as a second-level identity.
fn index_assignment_operation_directory(
    root: &Path,
    index_id: &crate::identity::IndexId,
) -> PathBuf {
    root.join(format!(
        "index-assignment-{}",
        hex_encode(index_id.as_bytes())
    ))
}

/// Writes one durable snapshot using a temporary file followed by rename.
///
/// The temporary file prevents a partially written snapshot from being exposed
/// under the final filename. `sync_all` makes the snapshot durable before the
/// rename is reported successful.
fn write_durable_snapshot(path: &Path, contents: &str) -> Result<(), IndexEventHandlingError> {
    let parent = path.parent().ok_or_else(|| {
        IndexEventHandlingError::Persistence(format!(
            "snapshot path has no parent: {}",
            path.display()
        ))
    })?;

    std::fs::create_dir_all(parent).map_err(|error| {
        IndexEventHandlingError::Persistence(format!(
            "could not create {}: {error}",
            parent.display()
        ))
    })?;

    let temporary_path = path.with_extension("tmp");
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&temporary_path)
        .map_err(|error| {
            IndexEventHandlingError::Persistence(format!(
                "could not create temporary snapshot {}: {error}",
                temporary_path.display()
            ))
        })?;

    file.write_all(contents.as_bytes()).map_err(|error| {
        IndexEventHandlingError::Persistence(format!(
            "could not write temporary snapshot {}: {error}",
            temporary_path.display()
        ))
    })?;

    file.sync_all().map_err(|error| {
        IndexEventHandlingError::Persistence(format!(
            "could not durably flush temporary snapshot {}: {error}",
            temporary_path.display()
        ))
    })?;

    std::fs::rename(&temporary_path, path).map_err(|error| {
        IndexEventHandlingError::Persistence(format!(
            "could not publish snapshot {}: {error}",
            path.display()
        ))
    })?;

    Ok(())
}

/// Saves the exact received `IndexEvent` as a durable recovery snapshot.
///
/// The runtime clones the typed event before execution. The snapshot keeps the
/// complete `Debug` representation as well as the important machine-readable
/// identity/reference fields and the opaque source payload bytes. The payload
/// is never interpreted by Indexing.
fn persist_received_index_event(
    operation_directory: &Path,
    event: &IndexEvent,
) -> Result<PathBuf, IndexEventHandlingError> {
    let event_id = hex_encode(event.event_id().as_str().as_bytes());
    let snapshot_directory = operation_directory.join("events");
    let path = snapshot_directory.join(format!("received-{event_id}.snapshot"));

    let operation_id = event.operation_context().operation.id.as_str();
    let snapshot = format!(
        "kind=IndexEvent\n\
         event_id={}\n\
         message_id={}\n\
         operation_id={}\n\
         entity_type={}\n\
         source_engine={}\n\
         source={}\n\
         object_reference={}\n\
         target_reference_type={}\n\
         namespace={}\n\
         family={}\n\
         key_material={}\n\
         source_payload={}\n\
         debug={:#?}\n",
        json_escape(event.event_id().as_str()),
        json_escape(event.message_id().as_str()),
        json_escape(operation_id),
        json_escape(event.entity_type().as_str()),
        json_escape(event.source_engine_id().as_str()),
        json_escape(event.object_reference().source()),
        json_escape(event.object_reference().object_reference()),
        json_escape(event.requirement().target_reference_type().as_str()),
        json_escape(event.requirement().namespace().as_str()),
        json_escape(&format!("{:?}", event.requirement().family())),
        hex_encode(&event.key_material().canonical_bytes()),
        hex_encode(event.source_payload()),
        event,
    );

    write_durable_snapshot(&path, &snapshot)?;
    Ok(path)
}

/// Saves the prepared `IndexEventResponse` as a durable recovery snapshot.
///
/// The response is cloned by the runtime before persistence. This means the
/// filesystem contains the actual typed response produced by the execution
/// path, rather than a separately reconstructed approximation.
fn persist_index_event_response(
    operation_directory: &Path,
    response: &IndexEventResponse,
) -> Result<PathBuf, IndexEventHandlingError> {
    let event_id = hex_encode(response.event().event_id().as_str().as_bytes());
    let snapshot_directory = operation_directory.join("responses");
    let path = snapshot_directory.join(format!("response-{event_id}.snapshot"));

    let snapshot = format!(
        "kind=IndexEventResponse\n\
         event_id={}\n\
         assigned_id={}\n\
         version={}\n\
         technical_result={}\n\
         debug={:#?}\n",
        json_escape(response.event().event_id().as_str()),
        json_escape(&response.assigned_id().to_string()),
        response
            .version()
            .map(|value| json_escape(value.as_str()))
            .unwrap_or_default(),
        response
            .technical_result()
            .map(json_escape)
            .unwrap_or_default(),
        response,
    );

    write_durable_snapshot(&path, &snapshot)?;
    Ok(path)
}

/// Appends one durable Index Assignment Operation lifecycle record.
///
/// The journal is append-only. A `started` record is written after the
/// received event snapshot has been durably stored and before assignment
/// execution. A `completed` record is written only after the actual
/// `IndexEventResponse` snapshot has been durably stored.
#[allow(clippy::too_many_arguments)]
fn persist_index_assignment_operation(
    directory: &Path,
    index_id: &crate::identity::IndexId,
    definition_id: &IndexDefinitionId,
    event: &IndexEvent,
    assigned_id: &crate::identity::IndexAssignedId,
    status: &str,
    event_snapshot: Option<&Path>,
    response_snapshot: Option<&Path>,
) -> Result<(), IndexEventHandlingError> {
    std::fs::create_dir_all(directory).map_err(|error| {
        IndexEventHandlingError::Persistence(format!(
            "could not create {}: {error}",
            directory.display()
        ))
    })?;

    let file_name = format!("index-assignment-{}.jsonl", hex_encode(index_id.as_bytes()));
    let path = directory.join(file_name);

    let operation_id = event.operation_context().operation.id.as_str();

    let event_snapshot = event_snapshot
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_default();
    let response_snapshot = response_snapshot
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_default();

    let record = format!(
        "{{\"index_id\":\"{}\",\"definition_id\":\"{}\",\"entity_type\":\"{}\",\"assigned_id\":\"{}\",\"target_reference_type\":\"{}\",\"source\":\"{}\",\"object_reference\":\"{}\",\"namespace\":\"{}\",\"family\":\"{}\",\"key_material\":\"{}\",\"event_id\":\"{}\",\"message_id\":\"{}\",\"operation_id\":\"{}\",\"event_snapshot\":\"{}\",\"response_snapshot\":\"{}\",\"status\":\"{}\"}}\n",
        hex_encode(index_id.as_bytes()),
        json_escape(definition_id.as_str()),
        json_escape(event.entity_type().as_str()),
        json_escape(&assigned_id.to_string()),
        json_escape(event.requirement().target_reference_type().as_str()),
        json_escape(event.object_reference().source()),
        json_escape(event.object_reference().object_reference()),
        json_escape(event.requirement().namespace().as_str()),
        json_escape(&format!("{:?}", event.requirement().family())),
        hex_encode(&event.key_material().canonical_bytes()),
        json_escape(event.event_id().as_str()),
        json_escape(event.message_id().as_str()),
        json_escape(operation_id),
        json_escape(&event_snapshot),
        json_escape(&response_snapshot),
        json_escape(status),
    );

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|error| {
            IndexEventHandlingError::Persistence(format!(
                "could not open {}: {error}",
                path.display()
            ))
        })?;

    file.write_all(record.as_bytes()).map_err(|error| {
        IndexEventHandlingError::Persistence(format!(
            "could not append {}: {error}",
            path.display()
        ))
    })?;

    file.sync_all().map_err(|error| {
        IndexEventHandlingError::Persistence(format!(
            "could not durably flush {}: {error}",
            path.display()
        ))
    })?;

    Ok(())
}

fn json_escape(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());

    for character in value.chars() {
        match character {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            character if character.is_control() => {
                use std::fmt::Write as _;
                let _ = write!(escaped, "\\u{:04x}", character as u32);
            }
            character => escaped.push(character),
        }
    }

    escaped
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ExistingOperationState {
    Missing,
    Started,
    Completed,
    Conflict,
}

/// Reads the durable operation journal for one deterministic `IndexId` and
/// classifies the relationship between the stored execution occurrence and the
/// received event.
///
/// The journal deliberately remains a simple append-only JSONL evidence format.
/// This helper only reads the stable identity/status fields needed for duplicate
/// admission; it does not attempt to deserialize the typed IndexEvent.
fn existing_operation_state(
    journal_path: &Path,
    event_id: &str,
    operation_id: &str,
) -> Result<ExistingOperationState, IndexEventHandlingError> {
    if !journal_path.exists() {
        return Ok(ExistingOperationState::Missing);
    }

    let contents = fs::read_to_string(journal_path).map_err(|error| {
        IndexEventHandlingError::Persistence(format!(
            "could not read {}: {error}",
            journal_path.display()
        ))
    })?;

    let mut saw_matching_occurrence = false;
    let mut saw_completed = false;
    let mut saw_started = false;

    for (line_number, line) in contents.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }

        let stored_event_id = json_string_field(line, "event_id").ok_or_else(|| {
            IndexEventHandlingError::Persistence(format!(
                "malformed journal record in {} at line {}: missing or invalid event_id",
                journal_path.display(),
                line_number + 1
            ))
        })?;
        let stored_operation_id = json_string_field(line, "operation_id").ok_or_else(|| {
            IndexEventHandlingError::Persistence(format!(
                "malformed journal record in {} at line {}: missing or invalid operation_id",
                journal_path.display(),
                line_number + 1
            ))
        })?;
        let status = json_string_field(line, "status").ok_or_else(|| {
            IndexEventHandlingError::Persistence(format!(
                "malformed journal record in {} at line {}: missing or invalid status",
                journal_path.display(),
                line_number + 1
            ))
        })?;

        if stored_event_id != event_id {
            continue;
        }

        if stored_operation_id != operation_id {
            return Ok(ExistingOperationState::Conflict);
        }

        saw_matching_occurrence = true;

        match status.as_str() {
            INDEX_ASSIGNMENT_STATUS_COMPLETED => saw_completed = true,
            INDEX_ASSIGNMENT_STATUS_STARTED => saw_started = true,
            _ => {
                return Err(IndexEventHandlingError::Persistence(format!(
                    "malformed journal record in {} at line {}: unsupported status {}",
                    journal_path.display(),
                    line_number + 1,
                    status
                )));
            }
        }
    }

    if saw_completed {
        Ok(ExistingOperationState::Completed)
    } else if saw_started {
        Ok(ExistingOperationState::Started)
    } else if saw_matching_occurrence {
        Ok(ExistingOperationState::Conflict)
    } else {
        Ok(ExistingOperationState::Missing)
    }
}

pub(crate) fn json_string_field(record: &str, field: &str) -> Option<String> {
    let prefix = format!("\"{field}\":\"");
    let start = record.find(&prefix)? + prefix.len();
    let mut characters = record[start..].chars().peekable();
    let mut output = String::new();

    while let Some(character) = characters.next() {
        match character {
            '"' => return Some(output),
            '\\' => {
                let escaped = characters.next()?;
                match escaped {
                    '"' => output.push('"'),
                    '\\' => output.push('\\'),
                    '/' => output.push('/'),
                    'b' => output.push('\u{0008}'),
                    'f' => output.push('\u{000c}'),
                    'n' => output.push('\n'),
                    'r' => output.push('\r'),
                    't' => output.push('\t'),
                    'u' => {
                        let mut hex = String::with_capacity(4);
                        for _ in 0..4 {
                            hex.push(characters.next()?);
                        }

                        let code = u16::from_str_radix(&hex, 16).ok()?;

                        if (0xD800..=0xDBFF).contains(&code) {
                            let mut lookahead = characters.clone();
                            if lookahead.next() == Some('\\') && lookahead.next() == Some('u') {
                                let mut low_hex = String::with_capacity(4);
                                for _ in 0..4 {
                                    low_hex.push(lookahead.next()?);
                                }
                                let low = u16::from_str_radix(&low_hex, 16).ok()?;
                                if (0xDC00..=0xDFFF).contains(&low) {
                                    let high = u32::from(code - 0xD800);
                                    let low = u32::from(low - 0xDC00);
                                    let scalar = 0x1_0000 + ((high << 10) | low);
                                    output.push(char::from_u32(scalar)?);
                                    characters = lookahead;
                                    continue;
                                }
                            }
                            return None;
                        }

                        output.push(char::from_u32(u32::from(code))?);
                    }
                    _ => return None,
                }
            }
            other => output.push(other),
        }
    }

    None
}

/// Creates an exclusive per-operation marker. The marker closes the race where
/// two concurrent requests observe the same `IndexId` before either writes its
/// `started` journal record.
///
/// The marker is intentionally ephemeral: a normal return removes it through
/// `OperationLock::drop`. If the process crashes, the marker remains and the
/// operation is conservatively treated as in progress until recovery/retry
/// reconciliation clears it.
struct OperationLock {
    path: PathBuf,
}

impl OperationLock {
    fn acquire(operation_directory: &Path, event_id: &str) -> Result<Self, bool> {
        let lock_directory = operation_directory.join("locks");
        if fs::create_dir_all(&lock_directory).is_err() {
            return Err(true);
        }

        let path = lock_directory.join(format!("event-{}.lock", hex_encode(event_id.as_bytes())));

        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(_) => Ok(Self { path }),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Err(false),
            Err(_) => Err(true),
        }
    }
}

impl Drop for OperationLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

const INDEX_ASSIGNMENT_STATUS_STARTED: &str = "started";
const INDEX_ASSIGNMENT_STATUS_COMPLETED: &str = "completed";

impl IndexingEngine {
    /// Creates a new Indexing Engine facade in the Core `Created` state.
    ///
    /// The default operation root is relative to the process working directory.
    /// Applications that need a specific durable location should use
    /// [`Self::new_with_operation_root`].
    #[must_use]
    pub fn new(engine_id: EngineId, engine_instance_id: EngineInstanceId) -> Self {
        Self::new_with_operation_root(
            engine_id,
            engine_instance_id,
            PathBuf::from(".nizaam").join("indexing"),
        )
    }

    /// Creates a new Indexing Engine facade with an explicit durable operation
    /// root.
    #[must_use]
    pub fn new_with_operation_root(
        engine_id: EngineId,
        engine_instance_id: EngineInstanceId,
        operation_root: impl Into<PathBuf>,
    ) -> Self {
        Self {
            registration: IndexingRegistration::new(engine_id.clone(), engine_instance_id.clone()),
            runtime: IndexingRuntime::new(engine_id, engine_instance_id),
            capabilities: CapabilitySet::new(),
            operation_root: operation_root.into(),
            engine_registered: AtomicBool::new(false),
            phase0_capability_registered: AtomicBool::new(false),
        }
    }

    /// Returns the configured root used for durable Index Assignment Operation
    /// records.
    #[must_use]
    pub fn operation_root(&self) -> &Path {
        &self.operation_root
    }

    /// Returns the logical engine identity shared by registration and runtime.
    #[must_use]
    pub fn engine_id(&self) -> &EngineId {
        self.registration.engine_id()
    }

    /// Returns the concrete engine-instance identity shared by registration
    /// and runtime.
    #[must_use]
    pub fn engine_instance_id(&self) -> &EngineInstanceId {
        self.registration.engine_instance_id()
    }

    /// Returns the declarative Indexing registration boundary.
    #[must_use]
    pub fn registration(&self) -> &IndexingRegistration {
        &self.registration
    }

    /// Returns the Indexing runtime adapter.
    #[must_use]
    pub fn runtime(&self) -> &IndexingRuntime {
        &self.runtime
    }

    /// Returns the Indexing capability integration boundary.
    #[must_use]
    pub fn capabilities(&self) -> &CapabilitySet {
        &self.capabilities
    }

    /// Advances the runtime through the non-registration startup states.
    pub fn start(&self) -> Result<(), InvalidTransition> {
        self.runtime.start()
    }

    /// Enters the Core `Registering` lifecycle state.
    ///
    /// Engine registration remains separate from the lifecycle transition and
    /// from capability registration, while the facade preserves the underlying
    /// Core lifecycle and registry error types in [`EngineSetupError`].
    pub fn begin_registration(&self) -> Result<(), InvalidTransition> {
        self.runtime.begin_registration()
    }

    /// Registers this engine instance through the Core Control Plane registry.
    ///
    /// Registration is only valid during the Core `Registering` lifecycle state.
    /// The returned error is a thin composition of the existing Core lifecycle
    /// and Core registry errors. No new failure category is introduced.
    pub fn register_engine(&self, registry: &EngineRegistry) -> Result<(), EngineSetupError> {
        self.require_state(LifecycleState::Registering)?;

        self.registration
            .register(registry)
            .map_err(EngineSetupError::Registry)?;

        self.engine_registered.store(true, Ordering::Release);

        Ok(())
    }

    /// Registers one capability for this Indexing Engine.
    ///
    /// Capability registration is accepted only while the Core runtime is in
    /// `Registering`, and the definition must name this engine as its owner.
    /// The underlying registry and registration errors remain Core-owned.
    pub fn register_capability(
        &self,
        definition: CapabilityDefinition,
        handler: Arc<dyn CapabilityHandler>,
    ) -> Result<(), EngineSetupError> {
        self.require_state(LifecycleState::Registering)?;

        if definition.owning_engine() != self.engine_id() {
            return Err(EngineSetupError::CapabilityOwnerMismatch {
                capability_id: definition.capability_id().clone(),
                expected_engine: self.engine_id().clone(),
                actual_engine: definition.owning_engine().clone(),
            });
        }

        self.capabilities
            .register(definition, handler)
            .map_err(EngineSetupError::CapabilityRegistry)
    }

    /// Registers the private Phase 0 bootstrap capability under this engine's
    /// logical `EngineId`.
    ///
    /// Capability registration remains separate from engine registration, but
    /// both are required before the engine may become `Ready`.
    pub fn register_phase0_capability(
        &self,
    ) -> Result<nizaam_core::identity::CapabilityId, EngineSetupError> {
        self.require_state(LifecycleState::Registering)?;

        let capability_id = self
            .capabilities
            .register_phase0_capability(self.engine_id())
            .map_err(EngineSetupError::CapabilityRegistry)?;

        self.phase0_capability_registered
            .store(true, Ordering::Release);

        Ok(capability_id)
    }

    /// Marks the engine ready only after required Phase 0 registration state
    /// has been established successfully.
    pub fn mark_ready(&self) -> Result<(), InvalidTransition> {
        if !self.engine_registered.load(Ordering::Acquire)
            || !self.phase0_capability_registered.load(Ordering::Acquire)
        {
            return Err(InvalidTransition::new(
                format!("{:?}", self.runtime.state()),
                format!("{:?}", LifecycleState::Ready),
            ));
        }

        self.runtime.mark_ready()
    }

    /// Explicitly enters `Serving` after the engine has reached `Ready`.
    pub fn serve(&self) -> Result<(), InvalidTransition> {
        self.runtime.serve()
    }

    /// Handles one universal request through the complete Phase 0 public
    /// request boundary.
    ///
    /// The facade preserves the Core request contract and constructs the
    /// corresponding `EngineContext` from the request's `OperationContext`.
    /// Core remains authoritative for lifecycle admission, capability
    /// resolution, cancellation, deadlines, and handler invocation.
    pub fn handle_request(&self, request: &UniversalRequest) -> UniversalRequestResult {
        self.runtime.admit_request()?;

        let envelope = &request.universal_event().envelope;
        let participants = &envelope.metadata.participants;

        if participants.target != self.engine_id().clone() {
            return Err(RequestHandlingError::TargetEngineMismatch {
                expected: self.engine_id().clone(),
                actual: participants.target.clone(),
            });
        }

        if let Some(target_instance) = participants.target_instance.as_ref()
            && target_instance != self.engine_instance_id()
        {
            return Err(RequestHandlingError::TargetInstanceMismatch {
                expected: self.engine_instance_id().clone(),
                actual: target_instance.clone(),
            });
        }

        let descriptor = &envelope.metadata.descriptor;

        let context = self.runtime.context(envelope.operation_context.clone());
        let invocation = CapabilityInvocation::new(
            descriptor.capability_id.clone(),
            descriptor.contract_id.clone(),
            envelope.payload.bytes().to_vec(),
        );

        match self.capabilities.dispatch(&context, &invocation) {
            CapabilityDispatchResult::Outcome(outcome) => {
                Ok(Ok(self.response_from_outcome(request, outcome)))
            }
            CapabilityDispatchResult::Error(error) => Ok(Err(error)),
        }
    }

    /// Executes one typed Index Assignment Operation.
    ///
    /// The canonical [`IndexDefinition`] is supplied explicitly because an
    /// [`IndexRequirement`] intentionally does not contain definition identity.
    /// The runtime verifies that the source requirement matches that canonical
    /// definition before using its `IndexDefinitionId` for assignment identity.
    /// This preserves the existing normalization boundary instead of deriving a
    /// definition ID from Core transport metadata.
    ///
    /// The execution order is:
    ///
    /// ```text
    /// Core admission
    ///     -> target validation
    ///     -> IndexEvent validation
    ///     -> requirement/definition binding validation
    ///     -> capacity admission
    ///     -> clone received IndexEvent
    ///     -> IndexId generation
    ///     -> IndexAssignedId generation
    ///     -> duplicate/idempotency state check
    ///     -> exclusive operation claim
    ///     -> persist received IndexEvent snapshot
    ///     -> persist `started` operation record
    ///     -> Core capability dispatch / assignment execution
    ///     -> construct IndexEventResponse
    ///     -> clone and persist IndexEventResponse snapshot
    ///     -> persist `completed` operation record
    /// ```
    ///
    /// `EntityType` is consumed as source-supplied classification metadata and
    /// is persisted with the assignment. It is never inferred from the opaque
    /// Core payload.
    pub fn handle_index_event(
        &self,
        event: &IndexEvent,
        definition: &IndexDefinition,
        capacity: &CapacityAccounting,
        capacity_request: CapacityRequest,
    ) -> Result<IndexEventResponse, IndexEventHandlingError> {
        self.runtime.admit_request()?;

        let envelope = &event.universal_event().envelope;
        let participants = &envelope.metadata.participants;

        if participants.target != self.engine_id().clone() {
            return Err(IndexEventHandlingError::TargetEngineMismatch {
                expected: self.engine_id().clone(),
                actual: participants.target.clone(),
            });
        }

        if let Some(target_instance) = participants.target_instance.as_ref()
            && target_instance != self.engine_instance_id()
        {
            return Err(IndexEventHandlingError::TargetInstanceMismatch {
                expected: self.engine_instance_id().clone(),
                actual: target_instance.clone(),
            });
        }

        event.validate()?;
        self.validate_definition_matches_requirement(event, definition)?;

        let _capacity_lease = capacity.try_acquire(capacity_request)?;

        // The runtime takes its own immutable copy immediately after the event
        // has passed admission, routing, validation, and capacity checks. This
        // copy is the source snapshot for backup/recovery/retry.
        let received_event = event.clone();

        // Generate the Indexing-owned identities that define this assignment
        // before execution. IndexId identifies the assignment operation;
        // IndexAssignedId identifies the referenced source object.
        let index_id = crate::identity::IndexId::generate(
            definition.namespace(),
            definition.definition_id(),
            definition.family(),
            received_event.key_material(),
        )
        .map_err(IndexEventHandlingError::IndexIdGeneration)?;

        let assigned_id = crate::identity::IndexAssignedId::generate(
            received_event.requirement().target_reference_type(),
            received_event.object_reference(),
        );

        let root_directory = indexing_operation_directory(self.operation_root())?;
        let operation_directory = index_assignment_operation_directory(&root_directory, &index_id);
        let journal_path = root_directory.join(format!(
            "index-assignment-{}.jsonl",
            hex_encode(index_id.as_bytes())
        ));

        match existing_operation_state(
            &journal_path,
            received_event.event_id().as_str(),
            received_event.operation_context().operation.id.as_str(),
        )? {
            ExistingOperationState::Completed => {
                return Err(IndexEventHandlingError::OperationAlreadyCompleted { index_id });
            }
            ExistingOperationState::Conflict => {
                return Err(IndexEventHandlingError::OperationConflict { index_id });
            }
            ExistingOperationState::Missing => {}
            ExistingOperationState::Started => {
                return Err(IndexEventHandlingError::OperationInProgress { index_id });
            }
        }

        let _operation_lock = match OperationLock::acquire(
            &operation_directory,
            received_event.event_id().as_str(),
        ) {
            Ok(lock) => lock,
            Err(false) => {
                return Err(IndexEventHandlingError::OperationInProgress { index_id });
            }
            Err(true) => {
                return Err(IndexEventHandlingError::Persistence(format!(
                    "could not acquire operation lock in {}",
                    operation_directory.display()
                )));
            }
        };

        // Re-check after acquiring the exclusive operation marker. Another
        // concurrent request may have completed the operation between the
        // first journal inspection and lock acquisition.
        match existing_operation_state(
            &journal_path,
            received_event.event_id().as_str(),
            received_event.operation_context().operation.id.as_str(),
        )? {
            ExistingOperationState::Completed => {
                return Err(IndexEventHandlingError::OperationAlreadyCompleted { index_id });
            }
            ExistingOperationState::Conflict => {
                return Err(IndexEventHandlingError::OperationConflict { index_id });
            }
            ExistingOperationState::Missing => {}
            ExistingOperationState::Started => {
                return Err(IndexEventHandlingError::OperationInProgress { index_id });
            }
        }

        // Persist the received event before executing the assignment. A crash
        // after this point leaves the exact received event available for later
        // recovery/retry instead of losing the input entirely.
        let event_snapshot = persist_received_index_event(&operation_directory, &received_event)?;

        persist_index_assignment_operation(
            &root_directory,
            &index_id,
            definition.definition_id(),
            &received_event,
            &assigned_id,
            INDEX_ASSIGNMENT_STATUS_STARTED,
            Some(&event_snapshot),
            None,
        )?;

        let context = self
            .runtime
            .context(received_event.operation_context().clone());

        // Core performs capability dispatch. The Indexing runtime owns the
        // typed assignment identity and durable operation lifecycle around it.
        match self
            .capabilities
            .dispatch_index_event(&context, &received_event)
        {
            CapabilityDispatchResult::Outcome(_) => {}
            CapabilityDispatchResult::Error(error) => {
                return Err(IndexEventHandlingError::Capability(error));
            }
        }

        // Construct the real typed response from the exact received-event copy
        // and the identity produced for this assignment.
        let response = IndexEventResponse::new(received_event.clone(), assigned_id);

        // Persist the exact response object produced by the execution path.
        // Completion is not recorded until this snapshot is durable.
        let response_copy = response.clone();
        let response_snapshot = persist_index_event_response(&operation_directory, &response_copy)?;

        persist_index_assignment_operation(
            &root_directory,
            &index_id,
            definition.definition_id(),
            &received_event,
            response.assigned_id(),
            INDEX_ASSIGNMENT_STATUS_COMPLETED,
            Some(&event_snapshot),
            Some(&response_snapshot),
        )?;

        Ok(response)
    }

    fn validate_definition_matches_requirement(
        &self,
        event: &IndexEvent,
        definition: &IndexDefinition,
    ) -> Result<(), IndexEventHandlingError> {
        let requirement = event.requirement();

        let matches = definition.namespace() == requirement.namespace()
            && definition.family() == requirement.family()
            && definition.key_definition() == requirement.key_definition()
            && definition.target_reference_type() == requirement.target_reference_type()
            && definition.uniqueness() == requirement.uniqueness()
            && definition.consistency_requirement() == requirement.consistency_requirement()
            && definition.source_version() == requirement.source_version()
            && definition.schema_version() == requirement.schema_version();

        if matches {
            Ok(())
        } else {
            Err(IndexEventHandlingError::DefinitionRequirementMismatch {
                definition_id: definition.definition_id().clone(),
            })
        }
    }

    fn response_from_outcome(
        &self,
        request: &UniversalRequest,
        outcome: CapabilityOutcome,
    ) -> UniversalResponse {
        let request_envelope = &request.universal_event().envelope;
        let request_participants = &request_envelope.metadata.participants;
        let mut metadata = request_envelope.metadata.clone();
        metadata.descriptor.interaction = Interaction::Response;

        let mut participants = Participants::new(
            self.engine_id().clone(),
            request_participants.sender.clone(),
        )
        .with_sender_instance(self.engine_instance_id().clone());

        if let Some(instance) = request_participants.sender_instance.clone() {
            participants = participants.with_target_instance(instance);
        }

        metadata.participants = participants;

        let payload_descriptor = metadata.descriptor.payload.clone();
        let response_envelope = MessageEnvelope::new(
            MessageId::generate(),
            request_envelope.operation_context.clone(),
            metadata,
            EncodedPayload::new(payload_descriptor, outcome.into_bytes()),
        );

        UniversalResponse::new(response_envelope, Status::Success)
    }

    fn require_state(&self, expected: LifecycleState) -> Result<(), EngineSetupError> {
        let current = self.runtime.state();
        if current == expected {
            Ok(())
        } else {
            Err(EngineSetupError::Lifecycle(InvalidTransition::new(
                format!("{current:?}"),
                format!("{expected:?}"),
            )))
        }
    }

    /// Explicitly enters `Draining` through the Core runtime.
    pub fn drain(&self) -> Result<(), InvalidTransition> {
        self.runtime.drain()
    }

    /// Gracefully shuts down through the Core runtime.
    pub fn shutdown(&self) -> Result<bool, InvalidTransition> {
        self.runtime.shutdown()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use nizaam_core::capability::{
        CapabilityDefinition, CapabilityError, CapabilityOutcome, arc_handler,
    };
    use nizaam_core::contracts::Version;
    use nizaam_core::identity::{CapabilityId, ContractId, CorrelationId, OperationId};
    use nizaam_core::operation::{Deadline, Operation, OperationContext};

    fn engine_id() -> EngineId {
        EngineId::new("nizaam.indexing.test").expect("test engine id must be valid")
    }

    fn instance_id() -> EngineInstanceId {
        EngineInstanceId::new("nizaam.indexing.test.instance")
            .expect("test engine instance id must be valid")
    }

    fn runtime() -> IndexingRuntime {
        IndexingRuntime::new(engine_id(), instance_id())
    }

    fn operation_context(name: &str) -> OperationContext {
        OperationContext::new(Operation::new(
            OperationId::new(format!("{name}.operation")).expect("test operation id must be valid"),
            CorrelationId::new(format!("{name}.correlation"))
                .expect("test correlation id must be valid"),
        ))
    }

    fn serving_runtime() -> IndexingRuntime {
        let runtime = runtime();
        runtime.start().unwrap();
        runtime.begin_registration().unwrap();
        runtime.mark_ready().unwrap();
        runtime.serve().unwrap();
        runtime
    }

    fn phase0_capability(
        runtime: &IndexingRuntime,
        invocation_count: Arc<AtomicUsize>,
    ) -> (CapabilitySet, CapabilityInvocation) {
        let capabilities = CapabilitySet::new();
        let capability_id =
            CapabilityId::new("nizaam.indexing.runtime.phase0").expect("valid test capability id");
        let contract_id = ContractId::new("nizaam.indexing.runtime.phase0.contract")
            .expect("valid test contract id");

        let definition = CapabilityDefinition::new(
            capability_id.clone(),
            runtime.engine_id().clone(),
            "Phase 0 runtime test capability",
        )
        .expect("test capability definition must be valid")
        .with_version(Version::new(1, 0, 0));

        let handler_count = Arc::clone(&invocation_count);
        let handler = arc_handler(move |_context, invocation| {
            handler_count.fetch_add(1, Ordering::SeqCst);
            Ok(CapabilityOutcome::new(invocation.payload_bytes().to_vec()))
        });

        capabilities
            .register(definition, handler)
            .expect("test capability registration must succeed");

        let invocation = CapabilityInvocation::new(capability_id, contract_id, b"phase0".to_vec());

        (capabilities, invocation)
    }

    #[test]
    fn construction_delegates_identity_to_the_core_runtime() {
        let runtime = runtime();

        assert_eq!(runtime.engine_id(), &engine_id());
        assert_eq!(runtime.engine_instance_id(), &instance_id());
        assert_eq!(runtime.state(), LifecycleState::Created);
    }

    #[test]
    fn start_preserves_the_core_startup_sequence() {
        let runtime = runtime();

        runtime.start().unwrap();

        assert_eq!(runtime.state(), LifecycleState::Capabilities);
    }

    #[test]
    fn readiness_and_serving_remain_separate() {
        let runtime = runtime();

        runtime.start().unwrap();
        runtime.begin_registration().unwrap();
        runtime.mark_ready().unwrap();

        assert_eq!(runtime.state(), LifecycleState::Ready);
        assert_eq!(
            runtime.admit_request(),
            Err(RequestAdmissionError::NotServing(LifecycleState::Ready))
        );

        runtime.serve().unwrap();
        assert_eq!(runtime.state(), LifecycleState::Serving);
        assert!(runtime.admit_request().is_ok());
    }

    #[test]
    fn non_serving_dispatch_is_rejected_before_capability_execution() {
        let runtime = runtime();
        let invocation_count = Arc::new(AtomicUsize::new(0));
        let (capabilities, invocation) = phase0_capability(&runtime, Arc::clone(&invocation_count));
        let context = runtime.context(operation_context("not-serving"));

        let result = runtime.dispatch(&capabilities, &context, &invocation);

        assert!(matches!(
            result,
            Err(RequestAdmissionError::NotServing(LifecycleState::Created))
        ));
        assert_eq!(invocation_count.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn serving_dispatch_reaches_core_capability_handler() {
        let runtime = serving_runtime();
        let invocation_count = Arc::new(AtomicUsize::new(0));
        let (capabilities, invocation) = phase0_capability(&runtime, Arc::clone(&invocation_count));
        let context = runtime.context(operation_context("serving"));

        let result = runtime
            .dispatch(&capabilities, &context, &invocation)
            .expect("serving runtime should admit the request");

        match result {
            CapabilityDispatchResult::Outcome(outcome) => {
                assert_eq!(outcome.as_bytes(), b"phase0");
            }
            CapabilityDispatchResult::Error(error) => {
                panic!("unexpected Core capability error: {error:?}");
            }
        }

        assert_eq!(invocation_count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn draining_rejects_new_dispatch_without_invoking_the_handler() {
        let runtime = serving_runtime();
        let invocation_count = Arc::new(AtomicUsize::new(0));
        let (capabilities, invocation) = phase0_capability(&runtime, Arc::clone(&invocation_count));
        let context = runtime.context(operation_context("draining"));

        runtime.drain().unwrap();

        let result = runtime.dispatch(&capabilities, &context, &invocation);

        assert!(matches!(
            result,
            Err(RequestAdmissionError::NotServing(LifecycleState::Draining))
        ));
        assert_eq!(invocation_count.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn context_preserves_the_supplied_operation_context() {
        let runtime = runtime();
        let operation = operation_context("context");
        let context = runtime.context(operation.clone());

        assert_eq!(context.operation(), &operation);
        assert!(context.deadline().is_none());
    }

    #[test]
    fn cancelled_context_is_rejected_by_core_dispatch_before_handler_execution() {
        let runtime = serving_runtime();
        let invocation_count = Arc::new(AtomicUsize::new(0));
        let (capabilities, invocation) = phase0_capability(&runtime, Arc::clone(&invocation_count));
        let context = runtime.context(operation_context("cancelled"));
        context.cancellation().cancel();

        let result = runtime
            .dispatch(&capabilities, &context, &invocation)
            .expect("serving runtime admission should succeed");

        assert!(matches!(
            result,
            CapabilityDispatchResult::Error(CapabilityError::Cancelled)
        ));
        assert_eq!(invocation_count.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn expired_context_is_rejected_by_core_dispatch_before_handler_execution() {
        let runtime = serving_runtime();
        let invocation_count = Arc::new(AtomicUsize::new(0));
        let (capabilities, invocation) = phase0_capability(&runtime, Arc::clone(&invocation_count));
        let context = runtime
            .context(operation_context("expired"))
            .with_deadline(Deadline::from_now(std::time::Duration::ZERO).unwrap());

        let result = runtime
            .dispatch(&capabilities, &context, &invocation)
            .expect("serving runtime admission should succeed");

        assert!(matches!(
            result,
            CapabilityDispatchResult::Error(CapabilityError::DeadlineExpired)
        ));
        assert_eq!(invocation_count.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn shutdown_delegates_to_core_and_reaches_stopped() {
        let runtime = serving_runtime();

        let completed = runtime.shutdown().expect("Core shutdown should succeed");

        assert!(completed);
        assert_eq!(runtime.state(), LifecycleState::Stopped);
        assert!(runtime.shutdown_token().is_cancelled());
    }

    #[test]
    fn stopped_runtime_remains_terminal_and_shutdown_is_idempotent() {
        let runtime = serving_runtime();

        runtime.shutdown().unwrap();
        runtime.shutdown().unwrap();

        assert_eq!(runtime.state(), LifecycleState::Stopped);
        assert!(runtime.transition(LifecycleState::Serving).is_err());
    }

    #[test]
    fn runtime_does_not_define_a_second_capability_error_boundary() {
        let runtime = serving_runtime();
        let invocation_count = Arc::new(AtomicUsize::new(0));
        let capabilities = CapabilitySet::new();
        let capability_id =
            CapabilityId::new("nizaam.indexing.runtime.failure").expect("valid capability id");
        let definition = CapabilityDefinition::new(
            capability_id.clone(),
            runtime.engine_id().clone(),
            "Phase 0 capability failure test",
        )
        .expect("valid definition");

        let count = Arc::clone(&invocation_count);
        capabilities
            .register(
                definition,
                arc_handler(move |_context, _invocation| {
                    count.fetch_add(1, Ordering::SeqCst);
                    Err(CapabilityError::HandlerFailed(
                        "phase 0 test failure".to_owned(),
                    ))
                }),
            )
            .unwrap();

        let invocation = CapabilityInvocation::new(
            capability_id,
            ContractId::new("nizaam.indexing.runtime.failure.contract").unwrap(),
            b"failure".to_vec(),
        );
        let context = runtime.context(operation_context("capability-error"));

        let dispatch_result = runtime
            .dispatch(&capabilities, &context, &invocation)
            .unwrap();

        assert!(matches!(
            dispatch_result,
            CapabilityDispatchResult::Error(CapabilityError::HandlerFailed(_))
        ));
        assert_eq!(invocation_count.load(Ordering::SeqCst), 1);
    }
    #[test]
    fn operation_journal_classifies_started_and_completed_occurrences() {
        let directory = std::env::temp_dir().join(format!(
            "nizaam-indexing-runtime-state-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock should be after UNIX epoch")
                .as_nanos()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let journal = directory.join("operation.jsonl");

        std::fs::write(
            &journal,
            "{\"event_id\":\"event-1\",\"operation_id\":\"operation-1\",\"status\":\"started\"}\n",
        )
        .unwrap();

        assert_eq!(
            existing_operation_state(&journal, "event-1", "operation-1").unwrap(),
            ExistingOperationState::Started
        );

        std::fs::OpenOptions::new()
            .append(true)
            .open(&journal)
            .unwrap()
            .write_all(
                b"{\"event_id\":\"event-1\",\"operation_id\":\"operation-1\",\"status\":\"completed\"}\n",
            )
            .unwrap();

        assert_eq!(
            existing_operation_state(&journal, "event-1", "operation-1").unwrap(),
            ExistingOperationState::Completed
        );

        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn operation_journal_rejects_a_different_event_or_operation_identity() {
        let directory = std::env::temp_dir().join(format!(
            "nizaam-indexing-runtime-conflict-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock should be after UNIX epoch")
                .as_nanos()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let journal = directory.join("operation.jsonl");

        std::fs::write(
            &journal,
            "{\"event_id\":\"event-1\",\"operation_id\":\"operation-1\",\"status\":\"started\"}\n",
        )
        .unwrap();

        assert_eq!(
            existing_operation_state(&journal, "event-2", "operation-1").unwrap(),
            ExistingOperationState::Missing
        );
        assert_eq!(
            existing_operation_state(&journal, "event-1", "operation-2").unwrap(),
            ExistingOperationState::Conflict
        );

        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn operation_journal_treats_different_events_as_independent_occurrences() {
        let directory = std::env::temp_dir().join(format!(
            "nizaam-indexing-runtime-independent-event-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock should be after UNIX epoch")
                .as_nanos()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let journal = directory.join("operation.jsonl");

        std::fs::write(
            &journal,
            concat!(
                "{\"event_id\":\"event-1\",\"operation_id\":\"operation-1\",\"status\":\"started\"}\n",
                "{\"event_id\":\"event-2\",\"operation_id\":\"operation-2\",\"status\":\"started\"}\n"
            ),
        )
        .unwrap();

        assert_eq!(
            existing_operation_state(&journal, "event-2", "operation-2").unwrap(),
            ExistingOperationState::Started
        );
        assert_eq!(
            existing_operation_state(&journal, "event-3", "operation-3").unwrap(),
            ExistingOperationState::Missing
        );

        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn malformed_journal_record_blocks_duplicate_admission() {
        let directory = std::env::temp_dir().join(format!(
            "nizaam-indexing-runtime-malformed-journal-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock should be after UNIX epoch")
                .as_nanos()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let journal = directory.join("operation.jsonl");

        std::fs::write(&journal, "{\"event_id\":\"event-1\"\n").unwrap();

        assert!(matches!(
            existing_operation_state(&journal, "event-1", "operation-1"),
            Err(IndexEventHandlingError::Persistence(_))
        ));

        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn json_string_field_round_trips_json_unicode_escapes() {
        assert_eq!(
            json_string_field(r#"{"value":"hello \u00e9 \u4f60\u597d"}"#, "value"),
            Some("hello é 你好".to_owned())
        );
    }

    #[test]
    fn operation_lock_is_scoped_per_event() {
        let directory = std::env::temp_dir().join(format!(
            "nizaam-indexing-runtime-event-lock-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock should be after UNIX epoch")
                .as_nanos()
        ));
        std::fs::create_dir_all(&directory).unwrap();

        let first = OperationLock::acquire(&directory, "event-1")
            .expect("first event should claim its lock");
        let second = OperationLock::acquire(&directory, "event-2")
            .expect("second event should claim its own lock");
        assert!(OperationLock::acquire(&directory, "event-1").is_err());

        drop(second);
        drop(first);
        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn operation_lock_is_exclusive_and_released_after_scope() {
        let directory = std::env::temp_dir().join(format!(
            "nizaam-indexing-runtime-lock-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock should be after UNIX epoch")
                .as_nanos()
        ));
        std::fs::create_dir_all(&directory).unwrap();

        let first = OperationLock::acquire(&directory, "event-1")
            .expect("first operation should claim the lock");
        assert!(OperationLock::acquire(&directory, "event-1").is_err());
        drop(first);

        assert!(OperationLock::acquire(&directory, "event-1").is_ok());

        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn assignment_persistence_helpers_encode_binary_identity_stably() {
        assert_eq!(hex_encode(&[0x00, 0xab, 0xff]), "00abff");
        assert_eq!(json_escape("a\"b\\c\n"), "a\\\"b\\\\c\\n");
    }
}
