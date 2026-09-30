//! Level 3 integration tests for durable Indexing Assignment Operation state.
//!
//! These tests exercise the filesystem side effects of the real
//! `IndexingEngine::handle_index_event` path. They intentionally use isolated
//! operation roots so the tests never touch the application's default
//! persistence location.

mod common;

use std::path::{Path, PathBuf};

use common::{
    capability_definition, counting_echo_handler, engine_registry, engine_with_operation_root,
    invocation_counter, register_engine, remove_test_operation_root, test_capability_id,
    test_capacity_accounting, test_engine_id, test_engine_instance_id, test_index_definition,
    test_index_event, test_index_event_capacity_request,
};
use nizaam_core::runtime::LifecycleState;
use nizaam_indexing::IndexEvent;
use nizaam_indexing::identity::IndexId;

fn prepare_serving_engine(
    operation_root: PathBuf,
) -> nizaam_indexing::engine::runtime::IndexingEngine {
    let engine =
        engine_with_operation_root(test_engine_id(), test_engine_instance_id(), operation_root);
    let registry = engine_registry();
    let counter = invocation_counter();

    engine.start().expect("engine startup should succeed");
    engine
        .begin_registration()
        .expect("engine should enter registration");
    register_engine(&engine, &registry).expect("engine registration should succeed");

    let capability = capability_definition(&test_engine_id(), &test_capability_id());
    engine
        .register_capability(capability, counting_echo_handler(counter))
        .expect("test capability registration should succeed");
    engine
        .register_phase0_capability()
        .expect("Phase 0 capability registration should succeed");
    engine.mark_ready().expect("engine should become ready");
    engine.serve().expect("engine should enter serving state");

    assert_eq!(engine.runtime().state(), LifecycleState::Serving);
    engine
}

fn execute_successfully(
    operation_root: &Path,
    name: &str,
) -> (
    nizaam_indexing::engine::runtime::IndexingEngine,
    IndexEvent,
    nizaam_indexing::IndexEventResponse,
    IndexId,
) {
    let engine = prepare_serving_engine(operation_root.to_path_buf());
    let event = test_index_event(name);
    let definition = test_index_definition();
    let index_id = IndexId::generate(
        definition.namespace(),
        definition.definition_id(),
        definition.family(),
        event.key_material(),
    )
    .expect("test IndexId generation should succeed");

    let response = engine
        .handle_index_event(
            &event,
            &definition,
            &test_capacity_accounting(),
            test_index_event_capacity_request(),
        )
        .expect("valid IndexEvent should be processed successfully");

    (engine, event, response, index_id)
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

fn operation_directory(root: &Path, index_id: &IndexId) -> PathBuf {
    root.join(format!(
        "index-assignment-{}",
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

fn journal_path(root: &Path, index_id: &IndexId) -> PathBuf {
    root.join(format!(
        "index-assignment-{}.jsonl",
        hex_encode(index_id.as_bytes())
    ))
}

#[test]
fn received_index_event_snapshot_contains_the_exact_durable_event_fields() {
    let root = common::test_operation_root();
    let (engine, event, _response, index_id) =
        execute_successfully(&root, "persistence-event-snapshot");

    let snapshot_path = event_snapshot_path(&root, &index_id, &event);
    assert!(snapshot_path.is_file());

    let snapshot = common::read_test_file(&snapshot_path);
    let operation_id = event.operation_context().operation.id.as_str();
    let expected_key = hex_encode(&event.key_material().canonical_bytes());
    let expected_payload = hex_encode(event.source_payload());

    assert!(snapshot.starts_with("kind=IndexEvent\n"));
    assert!(snapshot.contains(&format!("event_id={}\n", event.event_id().as_str())));
    assert!(snapshot.contains(&format!("message_id={}\n", event.message_id().as_str())));
    assert!(snapshot.contains(&format!("operation_id={operation_id}\n")));
    assert!(snapshot.contains(&format!("entity_type={}\n", event.entity_type().as_str())));
    assert!(snapshot.contains(&format!(
        "source_engine={}\n",
        event.source_engine_id().as_str()
    )));
    assert!(snapshot.contains(&format!("source={}\n", event.object_reference().source())));
    assert!(snapshot.contains(&format!(
        "object_reference={}\n",
        event.object_reference().object_reference()
    )));
    assert!(snapshot.contains(&format!(
        "target_reference_type={}\n",
        event.requirement().target_reference_type().as_str()
    )));
    assert!(snapshot.contains(&format!(
        "namespace={}\n",
        event.requirement().namespace().as_str()
    )));
    assert!(snapshot.contains(&format!("family={:?}\n", event.requirement().family())));
    assert!(snapshot.contains(&format!("key_material={expected_key}\n")));
    assert!(snapshot.contains(&format!("source_payload={expected_payload}\n")));
    assert!(snapshot.contains("debug=IndexEvent"));

    engine.shutdown().expect("engine shutdown should succeed");
    remove_test_operation_root(&root);
}

#[test]
fn response_snapshot_matches_the_returned_index_event_response() {
    let root = common::test_operation_root();
    let (engine, event, response, index_id) =
        execute_successfully(&root, "persistence-response-snapshot");

    let snapshot_path = response_snapshot_path(&root, &index_id, &event);
    assert!(snapshot_path.is_file());

    let snapshot = common::read_test_file(&snapshot_path);
    assert!(snapshot.starts_with("kind=IndexEventResponse\n"));
    assert!(snapshot.contains(&format!(
        "event_id={}\n",
        response.event().event_id().as_str()
    )));
    assert!(snapshot.contains(&format!("assigned_id={}\n", response.assigned_id())));
    assert!(snapshot.contains("version=\n"));
    assert!(snapshot.contains("technical_result=\n"));
    assert!(snapshot.contains("debug=IndexEventResponse"));

    assert_eq!(response.event(), &event);
    assert!(response.version().is_none());
    assert!(response.technical_result().is_none());

    engine.shutdown().expect("engine shutdown should succeed");
    remove_test_operation_root(&root);
}

#[test]
fn assignment_journal_contains_started_then_completed_records_with_correct_snapshots() {
    let root = common::test_operation_root();
    let (engine, event, response, index_id) =
        execute_successfully(&root, "persistence-journal-order");

    let journal = common::read_test_file(&journal_path(&root, &index_id));
    let records: Vec<&str> = journal.lines().collect();
    assert_eq!(
        records.len(),
        2,
        "one started and one completed record expected"
    );

    assert!(records[0].contains("\"status\":\"started\""));
    assert!(records[1].contains("\"status\":\"completed\""));
    assert!(records[0].contains("\"response_snapshot\":\"\""));
    assert!(records[1].contains("\"response_snapshot\":\""));

    let event_snapshot = event_snapshot_path(&root, &index_id, &event);
    let response_snapshot = response_snapshot_path(&root, &index_id, &event);
    let event_snapshot_string = event_snapshot.to_string_lossy();
    let response_snapshot_string = response_snapshot.to_string_lossy();

    assert!(records[0].contains(&format!(
        "\"event_snapshot\":\"{}\"",
        event_snapshot_string.replace('\\', "\\\\")
    )));
    assert!(records[1].contains(&format!(
        "\"event_snapshot\":\"{}\"",
        event_snapshot_string.replace('\\', "\\\\")
    )));
    assert!(records[1].contains(&format!(
        "\"response_snapshot\":\"{}\"",
        response_snapshot_string.replace('\\', "\\\\")
    )));

    assert!(records[0].contains(&format!("\"event_id\":\"{}\"", event.event_id().as_str())));
    assert!(records[1].contains(&format!(
        "\"event_id\":\"{}\"",
        response.event().event_id().as_str()
    )));
    assert!(records[1].contains(&format!("\"assigned_id\":\"{}\"", response.assigned_id())));

    engine.shutdown().expect("engine shutdown should succeed");
    remove_test_operation_root(&root);
}

#[test]
fn assignment_persistence_uses_the_expected_index_id_operation_directory_and_journal() {
    let root = common::test_operation_root();
    let (engine, event, _response, index_id) =
        execute_successfully(&root, "persistence-operation-identity");

    let directory = operation_directory(&root, &index_id);
    let journal = journal_path(&root, &index_id);

    assert!(directory.is_dir());
    assert!(journal.is_file());
    assert!(event_snapshot_path(&root, &index_id, &event).is_file());
    assert!(response_snapshot_path(&root, &index_id, &event).is_file());
    assert!(directory.join("locks").is_dir());

    let files = common::test_files_recursive(&root);
    assert!(files.contains(&journal));
    assert!(files.contains(&event_snapshot_path(&root, &index_id, &event)));
    assert!(files.contains(&response_snapshot_path(&root, &index_id, &event)));

    engine.shutdown().expect("engine shutdown should succeed");
    remove_test_operation_root(&root);
}
