//! Shared integration-test construction and inspection helpers for `nizaam-indexing`.
//!
//! These helpers centralize repeated, neutral test fixtures. They do not
//! implement Indexing behavior, lifecycle orchestration, routing, storage,
//! planning, or domain semantics. Tests remain responsible for making the
//! behavior under test explicit at the call site.
//!
//! The helpers cover:
//! - Core engine/operation/capability identities and construction
//! - Core request/event construction
//! - common Indexing identity, definition, requirement, entry, and event values
//! - capability handlers and invocation counters
//! - temporary filesystem roots and durable-file inspection
//!
//! A helper should be added here only when it is genuinely reusable across
//! multiple repository-level tests and does not hide the behavior being tested.

use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use nizaam_core::capability::{
    CapabilityDefinition, CapabilityError, CapabilityHandler, CapabilityInvocation,
    CapabilityOutcome, arc_handler,
};
use nizaam_core::contracts::{
    ContractDescriptor, ContractMetadata, EncodedPayload, Interaction, MessageEnvelope,
    Participants, PayloadDescriptor, UniversalRequest, UniversalResponse, Version,
};
use nizaam_core::control_plane::registry::EngineRegistry;
use nizaam_core::identity::{
    CapabilityId, ContractId, CorrelationId, EngineId, EngineInstanceId, MessageId, OperationId,
};
use nizaam_core::operation::{Operation, OperationContext};
use nizaam_core::status::Status;

use nizaam_indexing::IndexingRegistration;
use nizaam_indexing::engine::runtime::{EngineSetupError, IndexingEngine};
use nizaam_indexing::event::{EntityType, IndexEvent};
use nizaam_indexing::identity::{IndexDefinitionId, IndexDefinitionIdentity, IndexNamespace};
use nizaam_indexing::index::{
    ConsistencyRequirement, IndexDefinition, IndexEntry, IndexFamily, IndexVersion, IndexVersionId,
    KeyDefinition, KeyMaterial, ObjectReference, SchemaVersion, SourceVersion, TargetReferenceType,
    Uniqueness,
};
use nizaam_indexing::requirement::IndexRequirement;
use nizaam_indexing::{
    CapacityAccounting, CapacityOperation, CapacityRequest, IndexingConfiguration,
};

/// Stable logical engine identity used by repository-level tests.
pub const TEST_ENGINE_ID: &str = "nizaam.indexing.test";

/// Stable concrete engine-instance identity used by repository-level tests.
pub const TEST_ENGINE_INSTANCE_ID: &str = "nizaam.indexing.test.instance";

/// Default test contract identity used for opaque capability invocations.
pub const TEST_CONTRACT_ID: &str = "nizaam.indexing.test.contract";

/// Default test capability identity for custom integration-test handlers.
pub const TEST_CAPABILITY_ID: &str = "nizaam.indexing.test.capability";

/// Default source engine identity used by the generic IndexEvent fixture.
pub const TEST_SOURCE_ENGINE_ID: &str = "nizaam.indexing.test.source";

/// Default target-reference source used by the generic object-reference fixture.
pub const TEST_OBJECT_SOURCE: &str = "nizaam.indexing.test.source";

static TEST_OPERATION_ROOT_COUNTER: AtomicUsize = AtomicUsize::new(0);

/// Creates a fresh, unique filesystem root for one integration test.
///
/// The returned directory is not created by this helper. This keeps creation
/// and failure behavior visible to tests that explicitly exercise persistence.
#[must_use]
pub fn test_operation_root() -> PathBuf {
    let sequence = TEST_OPERATION_ROOT_COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "nizaam-indexing-test-{}-{}",
        std::process::id(),
        sequence
    ))
}

/// Removes a temporary test operation root when a test has finished inspecting it.
pub fn remove_test_operation_root(root: &Path) {
    if root.exists() {
        fs::remove_dir_all(root).expect("test operation root should be removable");
    }
}

/// Reads a UTF-8 test artifact from disk.
#[must_use]
pub fn read_test_file(path: &Path) -> String {
    fs::read_to_string(path).expect("test artifact should be readable as UTF-8")
}

/// Returns all files below a test operation root in deterministic path order.
#[must_use]
pub fn test_files_recursive(root: &Path) -> Vec<PathBuf> {
    fn visit(directory: &Path, files: &mut Vec<PathBuf>) {
        let entries = fs::read_dir(directory)
            .expect("test operation directory should be readable")
            .map(|entry| entry.expect("test directory entry should be readable"))
            .collect::<Vec<_>>();

        for entry in entries {
            let path = entry.path();
            if path.is_dir() {
                visit(&path, files);
            } else {
                files.push(path);
            }
        }
    }

    if !root.exists() {
        return Vec::new();
    }

    let mut files = Vec::new();
    visit(root, &mut files);
    files.sort();
    files
}

/// Creates the standard logical Indexing engine identity for tests.
#[must_use]
pub fn test_engine_id() -> EngineId {
    EngineId::new(TEST_ENGINE_ID).expect("test engine id must be valid")
}

/// Creates the standard concrete Indexing engine-instance identity for tests.
#[must_use]
pub fn test_engine_instance_id() -> EngineInstanceId {
    EngineInstanceId::new(TEST_ENGINE_INSTANCE_ID).expect("test engine instance id must be valid")
}

/// Alias for the standard Indexing engine-instance identity.
///
/// Kept as a semantic test-fixture name for tests that refer to the concrete
/// Indexing instance as an "index instance".
#[must_use]
pub fn test_index_instance_id() -> EngineInstanceId {
    test_engine_instance_id()
}

/// Creates an explicitly named logical engine identity for tests.
#[must_use]
pub fn engine_id(value: &str) -> EngineId {
    EngineId::new(value).expect("test engine id must be valid")
}

/// Creates an explicitly named concrete engine-instance identity for tests.
#[must_use]
pub fn engine_instance_id(value: &str) -> EngineInstanceId {
    EngineInstanceId::new(value).expect("test engine instance id must be valid")
}

/// Creates a fresh Indexing Engine facade using the standard test identities.
#[must_use]
pub fn test_engine() -> IndexingEngine {
    engine_with_operation_root(
        test_engine_id(),
        test_engine_instance_id(),
        test_operation_root(),
    )
}

/// Creates a fresh Indexing Engine facade using explicit test identities.
#[must_use]
pub fn engine(engine_id: EngineId, instance_id: EngineInstanceId) -> IndexingEngine {
    engine_with_operation_root(engine_id, instance_id, test_operation_root())
}

/// Creates a fresh Indexing Engine facade with an explicitly supplied operation root.
#[must_use]
pub fn engine_with_operation_root(
    engine_id: EngineId,
    instance_id: EngineInstanceId,
    operation_root: PathBuf,
) -> IndexingEngine {
    IndexingEngine::new_with_operation_root(engine_id, instance_id, operation_root)
}

/// Creates an empty Core-owned engine registry for a test.
#[must_use]
pub fn engine_registry() -> EngineRegistry {
    EngineRegistry::new()
}

/// Returns the declarative registration carried by an Indexing Engine.
#[must_use]
pub fn registration_fixture(engine: &IndexingEngine) -> IndexingRegistration {
    engine.registration().clone()
}

/// Registers an Indexing Engine through the Core engine registry.
///
/// This helper deliberately performs only the registration call. Runtime
/// lifecycle sequencing remains visible in the test.
pub fn register_engine(
    engine: &IndexingEngine,
    registry: &EngineRegistry,
) -> Result<(), EngineSetupError> {
    engine.register_engine(registry)
}

/// Creates a logical operation with deterministic test identities.
#[must_use]
pub fn operation(name: &str) -> Operation {
    Operation::new(
        OperationId::new(format!("{name}.operation")).expect("test operation id must be valid"),
        CorrelationId::new(format!("{name}.correlation"))
            .expect("test correlation id must be valid"),
    )
}

/// Creates an OperationContext around a deterministic test operation.
#[must_use]
pub fn operation_context(name: &str) -> OperationContext {
    OperationContext::new(operation(name))
}

/// Creates the standard test contract identity.
#[must_use]
pub fn test_contract_id() -> ContractId {
    ContractId::new(TEST_CONTRACT_ID).expect("test contract id must be valid")
}

/// Creates an explicitly named test contract identity.
#[must_use]
pub fn contract_id(value: &str) -> ContractId {
    ContractId::new(value).expect("test contract id must be valid")
}

/// Creates the standard custom capability identity.
#[must_use]
pub fn test_capability_id() -> CapabilityId {
    CapabilityId::new(TEST_CAPABILITY_ID).expect("test capability id must be valid")
}

/// Creates an explicitly named test capability identity.
#[must_use]
pub fn capability_id(value: &str) -> CapabilityId {
    CapabilityId::new(value).expect("test capability id must be valid")
}

/// Creates a minimal Core capability definition owned by `engine_id`.
#[must_use]
pub fn capability_definition(
    engine_id: &EngineId,
    capability_id: &CapabilityId,
) -> CapabilityDefinition {
    CapabilityDefinition::new(
        capability_id.clone(),
        engine_id.clone(),
        "Nizaam Indexing integration-test capability",
    )
    .expect("test capability definition must be valid")
}

/// Creates a capability definition with an explicit name and Core version.
#[must_use]
pub fn versioned_capability_definition(
    engine_id: &EngineId,
    capability_id: &CapabilityId,
    name: &str,
    version: Version,
) -> CapabilityDefinition {
    CapabilityDefinition::new(capability_id.clone(), engine_id.clone(), name)
        .expect("test capability definition must be valid")
        .with_version(version)
}

/// Creates an opaque Core capability invocation using the standard test
/// contract identity.
#[must_use]
pub fn capability_invocation(capability_id: CapabilityId, payload: &[u8]) -> CapabilityInvocation {
    CapabilityInvocation::new(capability_id, test_contract_id(), payload.to_vec())
}

/// Creates an opaque Core capability invocation with an explicit contract.
#[must_use]
pub fn capability_invocation_with_contract(
    capability_id: CapabilityId,
    contract_id: ContractId,
    payload: &[u8],
) -> CapabilityInvocation {
    CapabilityInvocation::new(capability_id, contract_id, payload.to_vec())
}

/// Creates the standard echo test handler.
///
/// The handler performs no Indexing work. It only returns the opaque payload
/// so integration tests can prove the Core dispatch boundary was reached.
#[must_use]
pub fn echo_handler() -> Arc<dyn CapabilityHandler> {
    arc_handler(|_context, invocation| {
        Ok(CapabilityOutcome::new(invocation.payload_bytes().to_vec()))
    })
}

/// Creates a handler that always returns a Core capability failure.
#[must_use]
pub fn failing_handler(message: &str) -> Arc<dyn CapabilityHandler> {
    let message = message.to_owned();

    arc_handler(move |_context, _invocation| Err(CapabilityError::HandlerFailed(message.clone())))
}

/// Creates a handler that records invocation count and echoes the payload.
#[must_use]
pub fn counting_echo_handler(counter: Arc<AtomicUsize>) -> Arc<dyn CapabilityHandler> {
    arc_handler(move |_context, invocation| {
        counter.fetch_add(1, Ordering::SeqCst);
        Ok(CapabilityOutcome::new(invocation.payload_bytes().to_vec()))
    })
}

/// Creates an invocation counter suitable for [`counting_echo_handler`].
#[must_use]
pub fn invocation_counter() -> Arc<AtomicUsize> {
    Arc::new(AtomicUsize::new(0))
}

/// Reads an invocation counter created by [`invocation_counter`].
#[must_use]
pub fn invocation_count(counter: &AtomicUsize) -> usize {
    counter.load(Ordering::SeqCst)
}

/// Wraps an already-constructed Core message envelope as a UniversalRequest.
#[must_use]
pub fn universal_request(envelope: MessageEnvelope) -> UniversalRequest {
    UniversalRequest::new(envelope)
}

/// Wraps an already-constructed Core message envelope as a UniversalResponse.
#[must_use]
pub fn universal_response(envelope: MessageEnvelope, status: Status) -> UniversalResponse {
    UniversalResponse::new(envelope, status)
}

/// Creates an IndexDefinitionId from a test value.
#[must_use]
pub fn definition_id(value: &str) -> IndexDefinitionId {
    IndexDefinitionId::new(value).expect("test definition ID must be valid")
}

/// Creates an IndexNamespace from a test value.
#[must_use]
pub fn namespace(value: &str) -> IndexNamespace {
    IndexNamespace::new(value).expect("test namespace must be valid")
}

/// Creates a logical IndexDefinitionIdentity.
#[must_use]
pub fn definition_identity(
    definition_id_value: &str,
    namespace_value: &str,
    family: IndexFamily,
) -> IndexDefinitionIdentity {
    IndexDefinitionIdentity::new(
        definition_id(definition_id_value),
        namespace(namespace_value),
        family,
    )
}

/// Creates a KeyDefinition from string-like key names.
#[must_use]
pub fn key_definition<I, S>(keys: I) -> KeyDefinition
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    KeyDefinition::new(keys).expect("test key definition must be valid")
}

/// Creates a target-reference type.
#[must_use]
pub fn target_reference_type(value: &str) -> TargetReferenceType {
    TargetReferenceType::new(value).expect("test target reference type must be valid")
}

/// Creates a consistency requirement.
#[must_use]
pub fn consistency_requirement(value: &str) -> ConsistencyRequirement {
    ConsistencyRequirement::new(value).expect("test consistency requirement must be valid")
}

/// Creates a source-version value.
#[must_use]
pub fn source_version(value: &str) -> SourceVersion {
    SourceVersion::new(value).expect("test source version must be valid")
}

/// Creates a schema-version value.
#[must_use]
pub fn schema_version(value: &str) -> SchemaVersion {
    SchemaVersion::new(value).expect("test schema version must be valid")
}

/// Creates an IndexVersionId from a test value.
#[must_use]
pub fn version_id(value: &str) -> IndexVersionId {
    IndexVersionId::new(value).expect("test version ID must be valid")
}

/// Creates an IndexVersion with optional source and schema metadata.
#[must_use]
pub fn index_version(value: &str, source: Option<&str>, schema: Option<&str>) -> IndexVersion {
    IndexVersion::with_metadata(
        version_id(value),
        source.map(source_version),
        schema.map(schema_version),
        None,
    )
    .expect("test index version must be valid")
}

/// Creates an opaque source-owned object reference.
#[must_use]
pub fn object_reference(source: &str, reference: &str) -> ObjectReference {
    ObjectReference::new(source, reference).expect("test object reference must be valid")
}

/// Creates a standard test object reference.
#[must_use]
pub fn test_object_reference(reference: &str) -> ObjectReference {
    object_reference(TEST_OBJECT_SOURCE, reference)
}

/// Creates an IndexEntry from a text key and object reference.
#[must_use]
pub fn index_entry(key: &str, source: &str, reference: &str) -> IndexEntry {
    IndexEntry::new(KeyMaterial::text(key), object_reference(source, reference))
        .expect("test index entry must be valid")
}

/// Creates an IndexEntry using the standard test object source.
#[must_use]
pub fn test_index_entry(key: &str, reference: &str) -> IndexEntry {
    index_entry(key, TEST_OBJECT_SOURCE, reference)
}

/// Creates an IndexRequirement from its canonical logical fields.
///
/// The explicit arguments keep the logical requirement fields visible to
/// integration-test call sites; the argument count is intentional for this
/// test-fixture constructor.
#[allow(clippy::too_many_arguments)]
#[must_use]
pub fn index_requirement(
    namespace_value: &str,
    family: IndexFamily,
    keys: &[&str],
    target_reference: &str,
    uniqueness: Uniqueness,
    consistency: &str,
    source_version_value: Option<&str>,
    schema_version_value: Option<&str>,
) -> IndexRequirement {
    IndexRequirement::new(
        namespace(namespace_value),
        family,
        key_definition(keys.iter().copied()),
        target_reference_type(target_reference),
        uniqueness,
        consistency_requirement(consistency),
        source_version_value.map(source_version),
        schema_version_value.map(schema_version),
    )
    .expect("test index requirement must be valid")
}

/// Creates the standard logical IndexRequirement used by event tests.
#[must_use]
pub fn test_index_requirement() -> IndexRequirement {
    index_requirement(
        "nizaam.indexing.test.namespace",
        IndexFamily::Inverted,
        &["term"],
        "source.object",
        Uniqueness::NonUnique,
        "logical",
        None,
        None,
    )
}

/// Creates an IndexDefinition from its canonical logical fields.
#[must_use]
pub fn index_definition(
    definition_identity: IndexDefinitionIdentity,
    key_definition_value: KeyDefinition,
    target_reference: TargetReferenceType,
    uniqueness: Uniqueness,
    consistency: ConsistencyRequirement,
    source_version_value: Option<SourceVersion>,
    schema_version_value: Option<SchemaVersion>,
) -> IndexDefinition {
    IndexDefinition::new(
        definition_identity,
        key_definition_value,
        target_reference,
        uniqueness,
        consistency,
        source_version_value,
        schema_version_value,
    )
    .expect("test index definition must be valid")
}

/// Creates the standard inverted test IndexDefinition.
#[must_use]
pub fn test_index_definition() -> IndexDefinition {
    index_definition(
        definition_identity(
            "nizaam.indexing.test.definition",
            "nizaam.indexing.test.namespace",
            IndexFamily::Inverted,
        ),
        key_definition(["term"]),
        target_reference_type("source.object"),
        Uniqueness::NonUnique,
        consistency_requirement("logical"),
        None,
        None,
    )
}

/// Creates an opaque Core request suitable for wrapping in an IndexEvent.
///
/// The explicit arguments keep routing, capability, contract, and payload
/// fields visible to integration-test call sites.
#[allow(clippy::too_many_arguments)]
#[must_use]
pub fn index_event_request(
    name: &str,
    sender: EngineId,
    sender_instance: Option<EngineInstanceId>,
    target: EngineId,
    target_instance: Option<EngineInstanceId>,
    capability: CapabilityId,
    contract: ContractId,
    interaction: Interaction,
    payload: &[u8],
) -> UniversalRequest {
    let version = Version::new(1, 0, 0);
    let payload_descriptor = PayloadDescriptor::new("application/octet-stream", version.clone())
        .expect("payload descriptor must be valid");

    let descriptor = ContractDescriptor::new(
        contract,
        capability,
        version,
        interaction,
        payload_descriptor.clone(),
    );

    let mut participants = Participants::new(sender, target);

    if let Some(instance) = sender_instance {
        participants = participants.with_sender_instance(instance);
    }

    if let Some(instance) = target_instance {
        participants = participants.with_target_instance(instance);
    }

    let metadata = ContractMetadata::new(descriptor, participants);

    let operation = Operation::new(
        OperationId::new(format!("{name}.operation")).expect("test operation ID must be valid"),
        CorrelationId::new(format!("{name}.correlation"))
            .expect("test correlation ID must be valid"),
    );

    let envelope = MessageEnvelope::new(
        MessageId::new(format!("{name}.message")).expect("test message ID must be valid"),
        OperationContext::new(operation),
        metadata,
        EncodedPayload::new(payload_descriptor, payload.to_vec()),
    );

    UniversalRequest::new(envelope)
}

/// Creates a standard IndexEvent while allowing tests to vary the identities
/// and logical fields that are relevant to the scenario.
///
/// The explicit arguments intentionally expose the complete event fixture
/// boundary used by Level 3 tests.
#[allow(clippy::too_many_arguments)]
#[must_use]
pub fn index_event(
    name: &str,
    sender: EngineId,
    sender_instance: Option<EngineInstanceId>,
    target: EngineId,
    target_instance: Option<EngineInstanceId>,
    entity_type: &str,
    requirement: IndexRequirement,
    object_reference_value: ObjectReference,
    key_material: KeyMaterial,
    payload: &[u8],
) -> IndexEvent {
    IndexEvent::new(
        index_event_request(
            name,
            sender,
            sender_instance,
            target,
            target_instance,
            capability_id(TEST_CAPABILITY_ID),
            test_contract_id(),
            Interaction::Request,
            payload,
        ),
        EntityType::new(entity_type).expect("test entity type must be valid"),
        requirement,
        object_reference_value,
        key_material,
    )
    .expect("test IndexEvent must be valid")
}

/// Rebuilds an IndexEvent's Core envelope as a Control Plane request carrying
/// the explicit Indexing transport payload. Core message, operation, participant,
/// contract, and capability metadata are retained; only the transport payload
/// is replaced with the versioned IndexEvent payload boundary.
#[must_use]
pub fn control_plane_request_for_event(event: &IndexEvent) -> UniversalRequest {
    let payload = event
        .control_plane_payload()
        .expect("test IndexEvent transport payload must encode");

    let envelope = event.universal_event().envelope.clone();
    let descriptor = envelope.metadata.descriptor.payload.clone();

    UniversalRequest::new(MessageEnvelope::new(
        event.message_id().clone(),
        event.operation_context().clone(),
        envelope.metadata,
        EncodedPayload::new(descriptor, payload),
    ))
}

/// Creates the standard capacity accounting used by IndexEvent integration tests.
#[must_use]
pub fn test_capacity_accounting() -> CapacityAccounting {
    let configuration = IndexingConfiguration::new(
        2,  // queries
        2,  // builds
        2,  // maintenance
        2,  // recoveries
        8,  // pending waiters
        16, // batch size
        32, // total logical units
        8,  // per-operation logical units
    )
    .expect("test indexing configuration must be valid");

    CapacityAccounting::from_configuration(configuration)
}

/// Creates the standard single-unit Build capacity request for IndexEvent tests.
#[must_use]
pub fn test_index_event_capacity_request() -> CapacityRequest {
    CapacityRequest::new(CapacityOperation::Build, 1)
}

/// Creates the standard successful IndexEvent fixture.
#[must_use]
pub fn test_index_event(name: &str) -> IndexEvent {
    index_event(
        name,
        engine_id("nizaam.indexing.test.source"),
        Some(engine_instance_id("nizaam.indexing.test.source.instance")),
        test_engine_id(),
        Some(test_engine_instance_id()),
        "semantic",
        test_index_requirement(),
        test_object_reference("object:1"),
        KeyMaterial::text("term"),
        b"source-owned-payload",
    )
}
