//! Level 3 integration tests for persistence-boundary failures in the real
//! `IndexingEngine::handle_index_event` path.
//!
//! Each test forces one concrete filesystem boundary to fail by using an
//! isolated temporary operation root and a path collision. No production
//! fault-injection hooks are introduced merely to make these tests possible.

mod common;

use std::{
    fs,
    path::{Path, PathBuf},
};

use common::{
    capability_definition, counting_echo_handler, engine_registry, engine_with_operation_root,
    failing_handler, invocation_count, invocation_counter, register_engine,
    remove_test_operation_root, test_capability_id, test_capacity_accounting, test_engine_id,
    test_engine_instance_id, test_index_definition, test_index_event,
    test_index_event_capacity_request,
};
use nizaam_core::capability::{CapabilityOutcome, arc_handler};
use nizaam_indexing::IndexEvent;
use nizaam_indexing::engine::runtime::IndexEventHandlingError;
use nizaam_indexing::identity::IndexId;

fn prepare_serving_engine(
    operation_root: PathBuf,
    handler: std::sync::Arc<dyn nizaam_core::capability::CapabilityHandler>,
) -> nizaam_indexing::engine::runtime::IndexingEngine {
    let engine =
        engine_with_operation_root(test_engine_id(), test_engine_instance_id(), operation_root);
    let registry = engine_registry();

    engine.start().expect("engine startup should succeed");
    engine
        .begin_registration()
        .expect("engine should enter registration");
    register_engine(&engine, &registry).expect("engine registration should succeed");

    let capability = capability_definition(&test_engine_id(), &test_capability_id());
    engine
        .register_capability(capability, handler)
        .expect("test capability registration should succeed");
    engine
        .register_phase0_capability()
        .expect("Phase 0 capability registration should succeed");
    engine.mark_ready().expect("engine should become ready");
    engine.serve().expect("engine should enter serving state");

    engine
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

fn index_id_for(event: &IndexEvent) -> IndexId {
    let definition = test_index_definition();
    IndexId::generate(
        definition.namespace(),
        definition.definition_id(),
        definition.family(),
        event.key_material(),
    )
    .expect("test IndexId generation should succeed")
}

fn operation_directory(root: &Path, index_id: &IndexId) -> PathBuf {
    root.join(format!(
        "index-assignment-{}",
        hex_encode(index_id.as_bytes())
    ))
}

fn journal_path(root: &Path, index_id: &IndexId) -> PathBuf {
    root.join(format!(
        "index-assignment-{}.jsonl",
        hex_encode(index_id.as_bytes())
    ))
}

fn event_snapshot_path(root: &Path, index_id: &IndexId, event: &IndexEvent) -> PathBuf {
    let event_id = hex_encode(event.event_id().as_str().as_bytes());
    operation_directory(root, index_id)
        .join("events")
        .join(format!("received-{event_id}.snapshot"))
}

fn response_snapshot_path(root: &Path, index_id: &IndexId, event: &IndexEvent) -> PathBuf {
    let event_id = hex_encode(event.event_id().as_str().as_bytes());
    operation_directory(root, index_id)
        .join("responses")
        .join(format!("response-{event_id}.snapshot"))
}

fn execute_event(
    engine: &nizaam_indexing::engine::runtime::IndexingEngine,
    event: &IndexEvent,
) -> Result<nizaam_indexing::IndexEventResponse, IndexEventHandlingError> {
    engine.handle_index_event(
        event,
        &test_index_definition(),
        &test_capacity_accounting(),
        test_index_event_capacity_request(),
    )
}

#[test]
fn event_snapshot_write_failure_prevents_capability_execution() {
    let root = common::test_operation_root();
    let counter = invocation_counter();
    let engine = prepare_serving_engine(root.clone(), counting_echo_handler(counter.clone()));
    let event = test_index_event("persistence-fault-event-snapshot");
    let index_id = index_id_for(&event);
    let events_path = operation_directory(&root, &index_id).join("events");

    fs::create_dir_all(operation_directory(&root, &index_id))
        .expect("operation directory should be creatable");
    fs::write(&events_path, b"not a directory")
        .expect("events path should be replaceable with a file");

    let error = execute_event(&engine, &event).expect_err("event snapshot persistence should fail");

    assert!(matches!(error, IndexEventHandlingError::Persistence(_)));
    assert_eq!(invocation_count(&counter), 0);
    assert!(!journal_path(&root, &index_id).exists());
    assert!(events_path.is_file());

    engine.shutdown().expect("engine shutdown should succeed");
    remove_test_operation_root(&root);
}

#[cfg(unix)]
#[test]
fn started_journal_failure_prevents_capability_execution_after_event_snapshot() {
    let root = common::test_operation_root();
    let counter = invocation_counter();
    let engine = prepare_serving_engine(root.clone(), counting_echo_handler(counter.clone()));
    let event = test_index_event("persistence-fault-started-journal");
    let index_id = index_id_for(&event);
    let operation_directory = operation_directory(&root, &index_id);
    let journal = journal_path(&root, &index_id);

    fs::create_dir_all(&operation_directory).expect("operation directory should be creatable");

    #[cfg(unix)]
    std::os::unix::fs::symlink("/proc/self/nonexistent", &journal)
        .expect("dangling journal symlink should be creatable");

    let error =
        execute_event(&engine, &event).expect_err("started journal persistence should fail");

    assert!(matches!(error, IndexEventHandlingError::Persistence(_)));
    assert_eq!(invocation_count(&counter), 0);
    assert!(event_snapshot_path(&root, &index_id, &event).is_file());

    #[cfg(unix)]
    assert!(journal.is_symlink());

    engine.shutdown().expect("engine shutdown should succeed");
    remove_test_operation_root(&root);
}

#[test]
fn capability_failure_leaves_received_event_and_started_record_without_completion() {
    let root = common::test_operation_root();
    let engine = prepare_serving_engine(
        root.clone(),
        failing_handler("intentional capability failure"),
    );
    let event = test_index_event("persistence-fault-capability");
    let index_id = index_id_for(&event);

    let error = execute_event(&engine, &event).expect_err("capability failure should be returned");

    assert!(matches!(error, IndexEventHandlingError::Capability(_)));

    let event_snapshot = event_snapshot_path(&root, &index_id, &event);
    let journal = journal_path(&root, &index_id);
    assert!(event_snapshot.is_file());
    assert!(journal.is_file());

    let journal_contents = common::read_test_file(&journal);
    assert_eq!(journal_contents.lines().count(), 1);
    assert!(journal_contents.contains("\"status\":\"started\""));
    assert!(!response_snapshot_path(&root, &index_id, &event).exists());
    assert!(!journal_contents.contains("\"status\":\"completed\""));

    engine.shutdown().expect("engine shutdown should succeed");
    remove_test_operation_root(&root);
}

#[test]
fn response_snapshot_failure_does_not_complete_the_operation() {
    let root = common::test_operation_root();
    let counter = invocation_counter();
    let engine = prepare_serving_engine(root.clone(), counting_echo_handler(counter.clone()));
    let event = test_index_event("persistence-fault-response-snapshot");
    let index_id = index_id_for(&event);
    let responses_path = operation_directory(&root, &index_id).join("responses");

    fs::create_dir_all(operation_directory(&root, &index_id))
        .expect("operation directory should be creatable");
    fs::write(&responses_path, b"not a directory")
        .expect("responses path should be replaceable with a file");

    let error =
        execute_event(&engine, &event).expect_err("response snapshot persistence should fail");

    assert!(matches!(error, IndexEventHandlingError::Persistence(_)));
    assert_eq!(invocation_count(&counter), 1);
    assert!(event_snapshot_path(&root, &index_id, &event).is_file());
    assert!(journal_path(&root, &index_id).is_file());
    assert!(responses_path.is_file());

    let journal_contents = common::read_test_file(&journal_path(&root, &index_id));
    assert_eq!(journal_contents.lines().count(), 1);
    assert!(journal_contents.contains("\"status\":\"started\""));
    assert!(!journal_contents.contains("\"status\":\"completed\""));

    engine.shutdown().expect("engine shutdown should succeed");
    remove_test_operation_root(&root);
}

#[test]
fn completed_journal_failure_preserves_response_snapshot_without_completion() {
    let root = common::test_operation_root();
    let event = test_index_event("persistence-fault-completed-journal");
    let index_id = index_id_for(&event);
    let journal = journal_path(&root, &index_id);
    let journal_for_handler = journal.clone();
    let handler = arc_handler(move |_context, invocation| {
        fs::remove_file(&journal_for_handler)
            .expect("started journal should exist before capability returns");
        fs::create_dir(&journal_for_handler)
            .expect("journal path should become an unusable directory");
        Ok(CapabilityOutcome::new(invocation.payload_bytes().to_vec()))
    });
    let engine = prepare_serving_engine(root.clone(), handler);

    let error =
        execute_event(&engine, &event).expect_err("completed journal persistence should fail");

    assert!(matches!(error, IndexEventHandlingError::Persistence(_)));
    assert!(event_snapshot_path(&root, &index_id, &event).is_file());
    assert!(response_snapshot_path(&root, &index_id, &event).is_file());
    assert!(journal.is_dir());

    engine.shutdown().expect("engine shutdown should succeed");
    remove_test_operation_root(&root);
}

#[test]
fn operation_lock_contention_prevents_execution_before_persistence() {
    let root = common::test_operation_root();
    let counter = invocation_counter();
    let engine = prepare_serving_engine(root.clone(), counting_echo_handler(counter.clone()));
    let event = test_index_event("persistence-fault-lock");
    let index_id = index_id_for(&event);
    let lock_directory = operation_directory(&root, &index_id).join("locks");
    let lock_path = lock_directory.join(format!(
        "event-{}.lock",
        hex_encode(event.event_id().as_str().as_bytes())
    ));

    fs::create_dir_all(&lock_directory).expect("lock directory should be creatable");
    fs::write(&lock_path, b"held by another execution")
        .expect("test lock marker should be creatable");

    let error = execute_event(&engine, &event)
        .expect_err("existing operation lock should reject execution");

    assert!(matches!(
        error,
        IndexEventHandlingError::OperationInProgress { .. }
    ));
    assert_eq!(invocation_count(&counter), 0);
    assert!(!event_snapshot_path(&root, &index_id, &event).exists());
    assert!(!journal_path(&root, &index_id).exists());

    engine.shutdown().expect("engine shutdown should succeed");
    remove_test_operation_root(&root);
}

#[test]
fn malformed_existing_journal_does_not_create_a_false_completed_operation() {
    let root = common::test_operation_root();
    let counter = invocation_counter();
    let engine = prepare_serving_engine(root.clone(), counting_echo_handler(counter.clone()));
    let event = test_index_event("persistence-fault-malformed-journal");
    let index_id = index_id_for(&event);
    let journal = journal_path(&root, &index_id);

    fs::create_dir_all(&root).expect("operation root should be creatable");
    fs::write(&journal, "this is not valid JSONL\n")
        .expect("malformed journal fixture should be writable");

    let error = execute_event(&engine, &event)
        .expect_err("malformed journal content should be rejected before execution");

    assert!(matches!(error, IndexEventHandlingError::Persistence(_)));
    assert_eq!(invocation_count(&counter), 0);
    assert!(!event_snapshot_path(&root, &index_id, &event).exists());
    assert_eq!(
        common::read_test_file(&journal),
        "this is not valid JSONL\n"
    );

    engine.shutdown().expect("engine shutdown should succeed");
    remove_test_operation_root(&root);
}
