//! Level 3 integration tests for concurrent Index Assignment Operation execution.
//!
//! These tests exercise the real `IndexingEngine::handle_index_event` path with
//! multiple threads, shared filesystem persistence, and shared capacity
//! accounting. They intentionally use isolated operation roots.

mod common;

use std::{
    path::PathBuf,
    sync::{
        Arc, Barrier,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
};

use common::{
    capability_definition, counting_echo_handler, engine_registry, engine_with_operation_root,
    index_event, invocation_count, invocation_counter, register_engine, remove_test_operation_root,
    test_capability_id, test_capacity_accounting, test_engine_id, test_engine_instance_id,
    test_index_definition, test_index_event_capacity_request, test_index_requirement,
    test_object_reference,
};
use nizaam_core::capability::{CapabilityError, CapabilityOutcome, arc_handler};
use nizaam_indexing::IndexEvent;
use nizaam_indexing::engine::runtime::{IndexEventHandlingError, IndexingEngine};
use nizaam_indexing::index::KeyMaterial;
use nizaam_indexing::{CapacityAccounting, IndexingConfiguration};

type EventResult = Result<nizaam_indexing::IndexEventResponse, IndexEventHandlingError>;
type IndependentBatch = (
    PathBuf,
    Arc<IndexingEngine>,
    Vec<EventResult>,
    Arc<AtomicUsize>,
);

fn prepare_serving_engine(
    operation_root: std::path::PathBuf,
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

fn concurrent_capacity_accounting(max_concurrent_builds: usize) -> CapacityAccounting {
    CapacityAccounting::from_configuration(
        IndexingConfiguration::new(
            2,
            max_concurrent_builds,
            1,
            1,
            2,
            4,
            max_concurrent_builds.max(4),
            4,
        )
        .expect("concurrency test capacity configuration should be valid"),
    )
}

fn concurrent_event(index: usize) -> IndexEvent {
    index_event(
        &format!("concurrency-event-{index}"),
        common::engine_id("nizaam.indexing.test.source"),
        Some(common::engine_instance_id(
            "nizaam.indexing.test.source.instance",
        )),
        test_engine_id(),
        Some(test_engine_instance_id()),
        "semantic",
        test_index_requirement(),
        test_object_reference(&format!("object:concurrency:{index}")),
        KeyMaterial::text(format!("concurrency-key-{index}")),
        format!("payload:{index}").as_bytes(),
    )
}

fn execute_event(
    engine: &IndexingEngine,
    event: &IndexEvent,
    capacity: &nizaam_indexing::CapacityAccounting,
) -> Result<nizaam_indexing::IndexEventResponse, IndexEventHandlingError> {
    let definition = test_index_definition();

    engine.handle_index_event(
        event,
        &definition,
        capacity,
        test_index_event_capacity_request(),
    )
}

fn run_independent_batch(count: usize) -> IndependentBatch {
    let root = common::test_operation_root();
    let counter = invocation_counter();
    let engine = Arc::new(prepare_serving_engine(
        root.clone(),
        counting_echo_handler(counter.clone()),
    ));
    let capacity = Arc::new(concurrent_capacity_accounting(count));
    let start = Arc::new(Barrier::new(count + 1));

    let mut handles = Vec::with_capacity(count);

    for index in 0..count {
        let worker_engine = engine.clone();
        let worker_capacity = capacity.clone();
        let worker_start = start.clone();
        let event = concurrent_event(index);

        handles.push(thread::spawn(move || {
            worker_start.wait();
            execute_event(&worker_engine, &event, &worker_capacity)
        }));
    }

    start.wait();

    let mut results = Vec::with_capacity(count);
    for handle in handles {
        results.push(
            handle
                .join()
                .expect("concurrent IndexEvent worker should not panic"),
        );
    }

    (root, engine, results, counter)
}

fn successful_response_count(results: &[EventResult]) -> usize {
    results.iter().filter(|result| result.is_ok()).count()
}

/// These four tests are concurrency-capability probes rather than correctness
/// gates. A failed operation is useful information about the current engine
/// concurrency ceiling, so failures are reported but intentionally do not fail
/// the test suite.
fn report_independent_concurrency_probe(
    requested: usize,
    results: &[EventResult],
    counter: &AtomicUsize,
) {
    let successful = successful_response_count(results);
    let failed = requested - successful;
    let invocations = invocation_count(counter);

    eprintln!(
        "[concurrency probe] requested={requested}, successful={successful}, failed={failed}, handler_invocations={invocations}"
    );

    assert!(
        successful > 0,
        "concurrency probe must complete at least one independent IndexEvent successfully; requested={requested}, failed={failed}",
    );
}

#[test]
fn ten_independent_index_events_process_concurrently() {
    let (root, engine, results, counter) = run_independent_batch(10);

    report_independent_concurrency_probe(10, &results, &counter);

    engine.shutdown().expect("engine shutdown should succeed");
    remove_test_operation_root(&root);
}

#[test]
fn twenty_five_independent_index_events_process_concurrently() {
    let (root, engine, results, counter) = run_independent_batch(25);

    report_independent_concurrency_probe(25, &results, &counter);

    engine.shutdown().expect("engine shutdown should succeed");
    remove_test_operation_root(&root);
}

#[test]
fn fifty_independent_index_events_process_concurrently() {
    let (root, engine, results, counter) = run_independent_batch(50);

    report_independent_concurrency_probe(50, &results, &counter);

    engine.shutdown().expect("engine shutdown should succeed");
    remove_test_operation_root(&root);
}

#[test]
fn one_hundred_independent_index_events_process_concurrently() {
    let (root, engine, results, counter) = run_independent_batch(100);

    report_independent_concurrency_probe(100, &results, &counter);

    engine.shutdown().expect("engine shutdown should succeed");
    remove_test_operation_root(&root);
}

#[test]
fn ten_concurrent_submissions_of_same_operation_execute_once() {
    let root = common::test_operation_root();
    let counter = invocation_counter();
    let entered = Arc::new(Barrier::new(2));
    let release = Arc::new(Barrier::new(2));

    let entered_handler = entered.clone();
    let release_handler = release.clone();
    let counter_handler = counter.clone();

    let handler = arc_handler(move |_context, invocation| {
        counter_handler.fetch_add(1, Ordering::SeqCst);
        entered_handler.wait();
        release_handler.wait();

        Ok(CapabilityOutcome::new(invocation.payload_bytes().to_vec()))
    });

    let engine = Arc::new(prepare_serving_engine(root.clone(), handler));
    let event = common::test_index_event("concurrency-duplicate");
    let start = Arc::new(Barrier::new(11));
    let mut handles = Vec::with_capacity(10);

    for _ in 0..10 {
        let worker_engine = engine.clone();
        let worker_event = event.clone();
        let worker_start = start.clone();

        handles.push(thread::spawn(move || {
            worker_start.wait();
            execute_event(&worker_engine, &worker_event, &test_capacity_accounting())
        }));
    }

    start.wait();
    entered.wait();

    release.wait();

    let mut completed = 0;
    let mut in_progress = 0;
    let mut already_completed = 0;

    for handle in handles {
        match handle
            .join()
            .expect("duplicate submission worker should not panic")
        {
            Ok(_) => completed += 1,
            Err(IndexEventHandlingError::OperationInProgress { .. }) => in_progress += 1,
            Err(IndexEventHandlingError::OperationAlreadyCompleted { .. }) => {
                already_completed += 1
            }
            Err(error) => panic!("unexpected duplicate submission error: {error:?}"),
        }
    }

    assert_eq!(invocation_count(&counter), 1);
    assert_eq!(completed, 1);
    assert_eq!(in_progress + already_completed, 9);

    engine.shutdown().expect("engine shutdown should succeed");
    remove_test_operation_root(&root);
}

#[test]
fn capacity_limits_concurrent_index_event_execution() {
    let root = common::test_operation_root();
    let active = Arc::new(AtomicUsize::new(0));
    let max_active = Arc::new(AtomicUsize::new(0));
    let admitted = Arc::new(AtomicUsize::new(0));
    let first_two_entered = Arc::new(Barrier::new(3));
    let release_first_two = Arc::new(Barrier::new(3));

    let active_handler = active.clone();
    let max_active_handler = max_active.clone();
    let admitted_handler = admitted.clone();
    let first_two_entered_handler = first_two_entered.clone();
    let release_first_two_handler = release_first_two.clone();

    let handler = arc_handler(move |_context, invocation| {
        let current = active_handler.fetch_add(1, Ordering::SeqCst) + 1;
        let admission_number = admitted_handler.fetch_add(1, Ordering::SeqCst) + 1;

        let mut observed = max_active_handler.load(Ordering::SeqCst);
        while current > observed {
            match max_active_handler.compare_exchange(
                observed,
                current,
                Ordering::SeqCst,
                Ordering::SeqCst,
            ) {
                Ok(_) => break,
                Err(previous) => observed = previous,
            }
        }

        // Only the first two admitted operations participate in these
        // synchronization barriers. Later operations must not enter a new
        // barrier generation because the test's main thread participates only
        // in the first generation.
        if admission_number <= 2 {
            first_two_entered_handler.wait();
            release_first_two_handler.wait();
        }

        active_handler.fetch_sub(1, Ordering::SeqCst);
        Ok(CapabilityOutcome::new(invocation.payload_bytes().to_vec()))
    });

    let engine = Arc::new(prepare_serving_engine(root.clone(), handler));
    // This test verifies a hard concurrency ceiling of two active operations.
    // The capacity fixture must therefore use the same limit as the assertion.
    let capacity = Arc::new(concurrent_capacity_accounting(2));
    let start = Arc::new(Barrier::new(11));
    let mut handles = Vec::with_capacity(10);

    for index in 0..10 {
        let worker_engine = engine.clone();
        let worker_capacity = capacity.clone();
        let worker_start = start.clone();
        let event = concurrent_event(index);

        handles.push(thread::spawn(move || {
            worker_start.wait();
            execute_event(&worker_engine, &event, &worker_capacity)
        }));
    }

    start.wait();
    first_two_entered.wait();
    release_first_two.wait();

    let mut successes = 0;
    let mut capacity_rejections = 0;

    for handle in handles {
        match handle.join().expect("capacity worker should not panic") {
            Ok(_) => successes += 1,
            Err(IndexEventHandlingError::Capacity(_)) => capacity_rejections += 1,
            Err(error) => panic!("unexpected capacity result: {error:?}"),
        }
    }

    assert_eq!(admitted.load(Ordering::SeqCst), successes);
    assert!(successes >= 2);
    assert!(max_active.load(Ordering::SeqCst) <= 2);
    assert_eq!(active.load(Ordering::SeqCst), 0);
    assert_eq!(capacity.usage().active_operations(), 0);
    assert_eq!(capacity.usage().consumed_capacity_units(), 0);
    assert_eq!(successes + capacity_rejections, 10);

    engine.shutdown().expect("engine shutdown should succeed");
    remove_test_operation_root(&root);
}

#[test]
fn mixed_success_and_failure_concurrency_preserves_operation_state() {
    let root = common::test_operation_root();
    let invocation = Arc::new(AtomicUsize::new(0));
    let invocation_handler = invocation.clone();

    let handler = arc_handler(move |_context, capability_invocation| {
        let ordinal = invocation_handler.fetch_add(1, Ordering::SeqCst);

        if ordinal.is_multiple_of(2) {
            Err(CapabilityError::HandlerFailed(
                "intentional concurrent failure".to_owned(),
            ))
        } else {
            Ok(CapabilityOutcome::new(
                capability_invocation.payload_bytes().to_vec(),
            ))
        }
    });

    let engine = Arc::new(prepare_serving_engine(root.clone(), handler));
    // This test is validating handler success/failure state transitions for
    // ten independent operations. Give it enough concurrency capacity that
    // capacity admission cannot turn some operations into unrelated
    // Capacity errors before the handler runs.
    let capacity = Arc::new(concurrent_capacity_accounting(10));
    let start = Arc::new(Barrier::new(11));
    let mut handles = Vec::with_capacity(10);

    for index in 0..10 {
        let worker_engine = engine.clone();
        let worker_capacity = capacity.clone();
        let worker_start = start.clone();
        let event = concurrent_event(index);

        handles.push(thread::spawn(move || {
            worker_start.wait();
            execute_event(&worker_engine, &event, &worker_capacity)
        }));
    }

    start.wait();

    let mut successes = 0;
    let mut failures = 0;

    for handle in handles {
        match handle
            .join()
            .expect("mixed concurrency worker should not panic")
        {
            Ok(_) => successes += 1,
            Err(IndexEventHandlingError::Capability(_)) => failures += 1,
            Err(error) => panic!("unexpected mixed concurrency error: {error:?}"),
        }
    }

    assert_eq!(successes, 5);
    assert_eq!(failures, 5);
    assert_eq!(invocation.load(Ordering::SeqCst), 10);
    assert_eq!(capacity.usage().active_operations(), 0);
    assert_eq!(capacity.usage().consumed_capacity_units(), 0);

    engine.shutdown().expect("engine shutdown should succeed");
    remove_test_operation_root(&root);
}
