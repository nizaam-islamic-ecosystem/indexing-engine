//! Level 3 integration tests for filesystem-backed recovery discovery and dispatch.
//!
//! These tests exercise the public recovery-execution boundary against durable
//! JSONL operation journals. They verify discovery and handler dispatch only.
//! They do not introduce a second retry scheduler, deserialize IndexEvent
//! snapshots, or mark operations completed.

mod common;

use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
};

use nizaam_indexing::engine::runtime::IndexEventHandlingError;
use nizaam_indexing::recovery::execution::{
    PersistedRecoveryOperation, RecoveryAction, RecoveryExecutionError, RecoveryExecutor,
    RecoveryHandler, RecoveryRequest, scan_incomplete_operations,
};
use nizaam_indexing::recovery::{ClassifiedFailure, FailureClass};

static NEXT_ROOT_ID: AtomicUsize = AtomicUsize::new(0);

fn test_root(name: &str) -> PathBuf {
    let sequence = NEXT_ROOT_ID.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "nizaam-indexing-recovery-execution-{name}-{}-{sequence}",
        std::process::id()
    ))
}

fn create_root(name: &str) -> PathBuf {
    let root = test_root(name);
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("test recovery root should be created");
    root
}

fn cleanup_root(root: &Path) {
    let _ = fs::remove_dir_all(root);
}

fn write_started_journal(
    root: &Path,
    index_id: &str,
    event_id: &str,
    operation_id: &str,
    event_snapshot: &str,
) -> PathBuf {
    let journal = root.join(format!("index-assignment-{index_id}.jsonl"));
    fs::write(
        &journal,
        format!(
            concat!(
                "{{\"status\":\"started\",\"event_id\":\"{}\",",
                "\"operation_id\":\"{}\",",
                "\"event_snapshot\":\"{}\"}}\n"
            ),
            event_id, operation_id, event_snapshot,
        ),
    )
    .expect("started journal should be written");
    journal
}

#[derive(Default)]
struct PersistedRecordingHandler {
    operations: Vec<(String, String, String, PathBuf, Option<PathBuf>)>,
}

impl RecoveryHandler for PersistedRecordingHandler {
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

    fn restore_availability(&mut self, _: &RecoveryRequest) -> Result<(), RecoveryExecutionError> {
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

    fn preserve_safe_state(&mut self, _: &RecoveryRequest) -> Result<(), RecoveryExecutionError> {
        Ok(())
    }

    fn surface_query_failure(&mut self, _: &RecoveryRequest) -> Result<(), RecoveryExecutionError> {
        Ok(())
    }

    fn recover_persisted_operation(
        &mut self,
        operation: &PersistedRecoveryOperation,
    ) -> Result<(), RecoveryExecutionError> {
        self.operations.push((
            operation.index_id_hex().to_owned(),
            operation.event_id().to_owned(),
            operation.operation_id().to_owned(),
            operation.event_snapshot().to_path_buf(),
            operation.response_snapshot().map(Path::to_path_buf),
        ));
        Ok(())
    }
}

#[derive(Default)]
struct DefaultRecoveryHandler;

impl RecoveryHandler for DefaultRecoveryHandler {
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

    fn restore_availability(&mut self, _: &RecoveryRequest) -> Result<(), RecoveryExecutionError> {
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

    fn preserve_safe_state(&mut self, _: &RecoveryRequest) -> Result<(), RecoveryExecutionError> {
        Ok(())
    }

    fn surface_query_failure(&mut self, _: &RecoveryRequest) -> Result<(), RecoveryExecutionError> {
        Ok(())
    }
}

#[test]
fn missing_recovery_root_is_treated_as_no_incomplete_operations() {
    let root = test_root("missing-root");

    let operations =
        scan_incomplete_operations(&root).expect("a missing recovery root should be empty");

    assert!(operations.is_empty());
}

#[test]
fn scanner_returns_the_started_operation_identity_and_snapshot_paths() {
    let root = create_root("single-started");
    let event_snapshot = "events/received-event-1.snapshot";

    write_started_journal(&root, "0011", "event-1", "operation-1", event_snapshot);

    let operations =
        scan_incomplete_operations(&root).expect("started journal should be discoverable");

    assert_eq!(operations.len(), 1);

    let operation = &operations[0];
    assert_eq!(operation.index_id_hex(), "0011");
    assert_eq!(operation.event_id(), "event-1");
    assert_eq!(operation.operation_id(), "operation-1");
    assert_eq!(operation.event_snapshot(), Path::new(event_snapshot));
    assert_eq!(operation.response_snapshot(), None);

    cleanup_root(&root);
}

#[test]
fn scanner_ignores_completed_operations_and_non_journal_files() {
    let root = create_root("completed-and-noise");

    fs::write(
        root.join("index-assignment-0022.jsonl"),
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
    .expect("completed journal should be written");

    fs::write(
        root.join("not-a-journal.txt"),
        "started operation must not be discovered from this file\n",
    )
    .expect("noise file should be written");

    let operations = scan_incomplete_operations(&root).expect("recovery scan should succeed");

    assert!(operations.is_empty());

    cleanup_root(&root);
}

#[test]
fn scanner_ignores_unrecognized_journal_records_but_discovers_valid_started_state() {
    let root = create_root("journal-noise");

    fs::write(
        root.join("index-assignment-0033.jsonl"),
        concat!(
            "this is not JSON\n",
            "{\"status\":\"unknown\",\"event_id\":\"ignored\",",
            "\"operation_id\":\"ignored\",\"event_snapshot\":\"ignored\"}\n",
            "{\"status\":\"started\",\"event_id\":\"event-3\",",
            "\"operation_id\":\"operation-3\",",
            "\"event_snapshot\":\"events/received-event-3.snapshot\"}\n"
        ),
    )
    .expect("journal with noise should be written");

    let operations =
        scan_incomplete_operations(&root).expect("recognized started record should be found");

    assert_eq!(operations.len(), 1);
    assert_eq!(operations[0].operation_id(), "operation-3");

    cleanup_root(&root);
}

#[test]
fn scanner_surfaces_a_missing_event_snapshot_as_a_recovery_error() {
    let root = create_root("missing-snapshot");

    fs::write(
        root.join("index-assignment-0044.jsonl"),
        "{\"status\":\"started\",\"event_id\":\"event-4\",\"operation_id\":\"operation-4\"}\n",
    )
    .expect("journal should be written");

    let error =
        scan_incomplete_operations(&root).expect_err("missing event snapshot must be rejected");

    assert_eq!(error.action(), RecoveryAction::Rebuild);
    assert!(
        error.message().contains("has no event snapshot path"),
        "unexpected recovery error: {}",
        error.message()
    );

    cleanup_root(&root);
}

#[test]
fn scanner_surfaces_a_response_snapshot_recorded_for_an_incomplete_operation() {
    let root = create_root("response-snapshot");

    fs::write(
        root.join("index-assignment-0055.jsonl"),
        concat!(
            "{\"status\":\"started\",\"event_id\":\"event-5\",",
            "\"operation_id\":\"operation-5\",",
            "\"event_snapshot\":\"events/received-event-5.snapshot\",",
            "\"response_snapshot\":\"responses/response-event-5.snapshot\"}\n"
        ),
    )
    .expect("journal should be written");

    let operations =
        scan_incomplete_operations(&root).expect("incomplete operation should be discoverable");

    assert_eq!(operations.len(), 1);
    assert_eq!(
        operations[0].response_snapshot(),
        Some(Path::new("responses/response-event-5.snapshot"))
    );

    cleanup_root(&root);
}

#[test]
fn executor_dispatches_each_discovered_incomplete_operation_to_the_handler() {
    let root = create_root("executor-dispatch");

    write_started_journal(
        &root,
        "0066",
        "event-6",
        "operation-6",
        "events/received-event-6.snapshot",
    );

    let mut handler = PersistedRecordingHandler::default();

    let operations = RecoveryExecutor::new()
        .recover_incomplete_operations(&root, &mut handler)
        .expect("incomplete operation dispatch should succeed");

    assert_eq!(operations.len(), 1);
    assert_eq!(handler.operations.len(), 1);
    assert_eq!(
        handler.operations[0],
        (
            "0066".to_owned(),
            "event-6".to_owned(),
            "operation-6".to_owned(),
            PathBuf::from("events/received-event-6.snapshot"),
            None,
        )
    );

    cleanup_root(&root);
}

#[test]
fn executor_returns_handler_error_without_claiming_completion_or_retrying() {
    struct FailingPersistedHandler;

    impl RecoveryHandler for FailingPersistedHandler {
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
            _: &PersistedRecoveryOperation,
        ) -> Result<(), RecoveryExecutionError> {
            Err(RecoveryExecutionError::new(
                RecoveryAction::Rebuild,
                "simulated persistent recovery failure",
            ))
        }
    }

    let root = create_root("executor-handler-error");

    let journal = write_started_journal(
        &root,
        "0077",
        "event-7",
        "operation-7",
        "events/received-event-7.snapshot",
    );
    let before = fs::read_to_string(&journal).expect("journal should be readable");

    let mut handler = FailingPersistedHandler;

    let error = RecoveryExecutor::new()
        .recover_incomplete_operations(&root, &mut handler)
        .expect_err("handler failure should be propagated");

    assert_eq!(error.action(), RecoveryAction::Rebuild);
    assert_eq!(error.message(), "simulated persistent recovery failure");

    let after = fs::read_to_string(&journal).expect("journal should remain readable");
    assert_eq!(after, before);

    cleanup_root(&root);
}

#[test]
fn default_persisted_recovery_handler_does_not_implicitly_retry() {
    let root = create_root("default-handler");

    write_started_journal(
        &root,
        "0088",
        "event-8",
        "operation-8",
        "events/received-event-8.snapshot",
    );

    let mut handler = DefaultRecoveryHandler;

    let error = RecoveryExecutor::new()
        .recover_incomplete_operations(&root, &mut handler)
        .expect_err("default handler must not perform persistent recovery");

    assert_eq!(error.action(), RecoveryAction::Rebuild);
    assert_eq!(
        error.message(),
        "persistent recovery is not implemented by this RecoveryHandler"
    );

    cleanup_root(&root);
}

#[test]
fn recovery_discovery_does_not_mutate_the_journal() {
    let root = create_root("read-only-scan");

    let journal = write_started_journal(
        &root,
        "0099",
        "event-9",
        "operation-9",
        "events/received-event-9.snapshot",
    );
    let before = fs::read_to_string(&journal).expect("journal should be readable");

    let operations =
        scan_incomplete_operations(&root).expect("read-only recovery discovery should succeed");

    assert_eq!(operations.len(), 1);

    let after = fs::read_to_string(&journal).expect("journal should remain readable");
    assert_eq!(after, before);

    cleanup_root(&root);
}

#[test]
fn recovery_discovers_an_unfinished_assignment_written_by_index_event_execution() {
    let root = common::test_operation_root();
    let engine = common::engine_with_operation_root(
        common::test_engine_id(),
        common::test_engine_instance_id(),
        root.clone(),
    );
    let registry = common::engine_registry();

    engine.start().expect("engine startup should succeed");
    engine
        .begin_registration()
        .expect("engine should enter registration");
    common::register_engine(&engine, &registry).expect("engine registration should succeed");

    let capability =
        common::capability_definition(&common::test_engine_id(), &common::test_capability_id());
    engine
        .register_capability(
            capability,
            common::failing_handler("recovery discovery test failure"),
        )
        .expect("failing test capability should register");
    engine
        .register_phase0_capability()
        .expect("Phase 0 capability should register");
    engine.mark_ready().expect("engine should become ready");
    engine.serve().expect("engine should enter serving state");

    let event = common::test_index_event("recovery-runtime-written");
    let error = engine
        .handle_index_event(
            &event,
            &common::test_index_definition(),
            &common::test_capacity_accounting(),
            common::test_index_event_capacity_request(),
        )
        .expect_err("the failing handler should leave an unfinished assignment");

    assert!(matches!(error, IndexEventHandlingError::Capability(_)));

    let operations =
        scan_incomplete_operations(&root).expect("runtime-written started journal should scan");

    assert_eq!(operations.len(), 1);
    assert_eq!(operations[0].event_id(), event.event_id().as_str());
    assert_eq!(
        operations[0].operation_id(),
        event.operation_context().operation.id.as_str(),
    );
    assert!(
        operations[0].event_snapshot().is_file(),
        "recovery must discover the event snapshot actually written by IndexEvent execution",
    );

    engine.shutdown().expect("engine shutdown should succeed");
    common::remove_test_operation_root(&root);
}

#[test]
fn recovery_execution_does_not_change_the_selected_failure_action() {
    let request = RecoveryRequest::new(ClassifiedFailure::new(FailureClass::CorruptIndex));

    assert_eq!(request.action(), RecoveryAction::Rebuild);
}
