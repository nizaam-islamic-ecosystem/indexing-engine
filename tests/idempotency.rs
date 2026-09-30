//! Level 3 integration tests for duplicate and idempotent Index Assignment Operations.
//!
//! These tests exercise duplicate admission through the real
//! `IndexingEngine::handle_index_event` boundary. They intentionally use
//! isolated operation roots and the existing Core capability registration
//! helpers rather than introducing a second idempotency implementation.

mod common;

use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Barrier},
    thread,
};

use common::{
    capability_definition, counting_echo_handler, engine_registry, engine_with_operation_root,
    index_event, invocation_count, invocation_counter, register_engine, remove_test_operation_root,
    test_capability_id, test_capacity_accounting, test_engine_id, test_engine_instance_id,
    test_index_definition, test_index_event, test_index_event_capacity_request,
    test_index_requirement, test_object_reference,
};
use nizaam_core::capability::{CapabilityOutcome, arc_handler};
use nizaam_indexing::IndexEvent;
use nizaam_indexing::engine::runtime::{IndexEventHandlingError, IndexingEngine};
use nizaam_indexing::identity::IndexId;
use nizaam_indexing::index::KeyMaterial;

fn prepare_serving_engine(
    operation_root: PathBuf,
    handler: Arc<dyn nizaam_core::capability::CapabilityHandler>,
) -> IndexingEngine {
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

fn execute_event(
    engine: &IndexingEngine,
    event: &IndexEvent,
) -> Result<nizaam_indexing::IndexEventResponse, IndexEventHandlingError> {
    let definition = test_index_definition();
    let capacity = test_capacity_accounting();

    engine.handle_index_event(
        event,
        &definition,
        &capacity,
        test_index_event_capacity_request(),
    )
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

fn journal_path(root: &Path, index_id: &IndexId) -> PathBuf {
    root.join(format!(
        "index-assignment-{}.jsonl",
        hex_encode(index_id.as_bytes())
    ))
}

#[test]
fn completed_operation_is_rejected_as_duplicate() {
    let root = common::test_operation_root();
    let counter = invocation_counter();
    let engine = prepare_serving_engine(root.clone(), counting_echo_handler(counter.clone()));
    let event = test_index_event("idempotency-completed");

    execute_event(&engine, &event).expect("first execution should complete");

    let error = execute_event(&engine, &event)
        .expect_err("the completed operation should be rejected as a duplicate");

    assert!(matches!(
        error,
        IndexEventHandlingError::OperationAlreadyCompleted { .. }
    ));
    assert_eq!(invocation_count(&counter), 1);

    engine.shutdown().expect("engine shutdown should succeed");
    remove_test_operation_root(&root);
}

#[test]
fn concurrent_duplicate_operation_is_rejected_or_reported_in_progress() {
    let root = common::test_operation_root();
    let counter = invocation_counter();
    let entered = Arc::new(Barrier::new(2));
    let release = Arc::new(Barrier::new(2));

    let entered_handler = entered.clone();
    let release_handler = release.clone();
    let counter_handler = counter.clone();

    let handler = arc_handler(move |_context, invocation| {
        counter_handler.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        entered_handler.wait();
        release_handler.wait();

        Ok(CapabilityOutcome::new(invocation.payload_bytes().to_vec()))
    });

    let engine = Arc::new(prepare_serving_engine(root.clone(), handler));
    let event = test_index_event("idempotency-concurrent-duplicate");
    let first_engine = engine.clone();
    let first_event = event.clone();

    let first = thread::spawn(move || execute_event(&first_engine, &first_event));

    entered.wait();

    let second = execute_event(&engine, &event)
        .expect_err("the second submission must not execute the same operation");

    assert!(matches!(
        second,
        IndexEventHandlingError::OperationInProgress { .. }
    ));
    assert_eq!(invocation_count(&counter), 1);

    release.wait();

    first
        .join()
        .expect("first execution thread should not panic")
        .expect("first execution should complete successfully");
    assert_eq!(invocation_count(&counter), 1);

    engine.shutdown().expect("engine shutdown should succeed");
    remove_test_operation_root(&root);
}

#[test]
fn same_event_different_operation_identity_is_handled_as_conflict() {
    let root = common::test_operation_root();
    let counter = invocation_counter();
    let engine = prepare_serving_engine(root.clone(), counting_echo_handler(counter.clone()));
    let event = test_index_event("idempotency-conflicting-operation");
    let definition = test_index_definition();
    let index_id = IndexId::generate(
        definition.namespace(),
        definition.definition_id(),
        definition.family(),
        event.key_material(),
    )
    .expect("test IndexId generation should succeed");
    let journal = journal_path(&root, &index_id);

    fs::create_dir_all(&root).expect("operation root should be creatable");
    let conflicting_operation = format!(
        "{{\"event_id\":\"{}\",\"operation_id\":\"different-operation\",\"status\":\"started\"}}\n",
        event.event_id().as_str()
    );
    fs::write(&journal, conflicting_operation)
        .expect("conflicting journal fixture should be writable");

    let error = execute_event(&engine, &event)
        .expect_err("same EventId with a different OperationId should conflict");

    assert!(matches!(
        error,
        IndexEventHandlingError::OperationConflict { .. }
    ));
    assert_eq!(invocation_count(&counter), 0);

    engine.shutdown().expect("engine shutdown should succeed");
    remove_test_operation_root(&root);
}

#[test]
fn independent_operations_are_not_treated_as_duplicates() {
    let root = common::test_operation_root();
    let counter = invocation_counter();
    let engine = prepare_serving_engine(root.clone(), counting_echo_handler(counter.clone()));

    let first = test_index_event("idempotency-independent-first");
    let second = test_index_event("idempotency-independent-second");

    execute_event(&engine, &first).expect("first independent operation should complete");
    execute_event(&engine, &second).expect("second independent operation should complete");

    assert_eq!(invocation_count(&counter), 2);

    engine.shutdown().expect("engine shutdown should succeed");
    remove_test_operation_root(&root);
}

#[test]
fn distinct_event_occurrences_with_same_index_id_are_processed_as_independent_operations() {
    let root = common::test_operation_root();
    let counter = invocation_counter();
    let engine = prepare_serving_engine(root.clone(), counting_echo_handler(counter.clone()));

    let first = index_event(
        "idempotency-same-index-id-first",
        common::engine_id("nizaam.indexing.test.source"),
        Some(common::engine_instance_id(
            "nizaam.indexing.test.source.instance",
        )),
        test_engine_id(),
        Some(test_engine_instance_id()),
        "semantic",
        test_index_requirement(),
        test_object_reference("object:same-index-id:first"),
        KeyMaterial::text("same-logical-key"),
        b"payload:first",
    );
    let second = index_event(
        "idempotency-same-index-id-second",
        common::engine_id("nizaam.indexing.test.source"),
        Some(common::engine_instance_id(
            "nizaam.indexing.test.source.instance",
        )),
        test_engine_id(),
        Some(test_engine_instance_id()),
        "semantic",
        test_index_requirement(),
        test_object_reference("object:same-index-id:second"),
        KeyMaterial::text("same-logical-key"),
        b"payload:second",
    );

    let definition = test_index_definition();
    let first_index_id = IndexId::generate(
        definition.namespace(),
        definition.definition_id(),
        definition.family(),
        first.key_material(),
    )
    .expect("first IndexId generation should succeed");
    let second_index_id = IndexId::generate(
        definition.namespace(),
        definition.definition_id(),
        definition.family(),
        second.key_material(),
    )
    .expect("second IndexId generation should succeed");

    assert_eq!(first_index_id, second_index_id);
    assert_ne!(first.event_id(), second.event_id());
    assert_ne!(
        first.operation_context().operation.id,
        second.operation_context().operation.id
    );

    execute_event(&engine, &first).expect("first operation should complete");
    execute_event(&engine, &second)
        .expect("different event and operation identities should remain independent");

    assert_eq!(invocation_count(&counter), 2);

    engine.shutdown().expect("engine shutdown should succeed");
    remove_test_operation_root(&root);
}

#[test]
fn repeated_submission_after_completion_does_not_append_another_execution() {
    let root = common::test_operation_root();
    let counter = invocation_counter();
    let engine = prepare_serving_engine(root.clone(), counting_echo_handler(counter.clone()));
    let event = test_index_event("idempotency-single-completion");

    let response = execute_event(&engine, &event).expect("first execution should complete");
    let definition = test_index_definition();
    let index_id = IndexId::generate(
        definition.namespace(),
        definition.definition_id(),
        definition.family(),
        event.key_material(),
    )
    .expect("test IndexId generation should succeed");
    let journal = journal_path(&root, &index_id);
    let before = common::read_test_file(&journal);

    let duplicate =
        execute_event(&engine, &event).expect_err("completed operation should remain rejected");
    assert!(matches!(
        duplicate,
        IndexEventHandlingError::OperationAlreadyCompleted { .. }
    ));
    assert_eq!(invocation_count(&counter), 1);

    let after = common::read_test_file(&journal);
    assert_eq!(after, before);
    assert_eq!(response.event(), &event);

    engine.shutdown().expect("engine shutdown should succeed");
    remove_test_operation_root(&root);
}
