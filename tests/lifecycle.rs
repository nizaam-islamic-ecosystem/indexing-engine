//! Phase 5 integration tests for the logical Indexing index lifecycle.
//!
//! These tests exercise the public Indexing API from the perspective of a
//! downstream consumer. They intentionally keep index lifecycle state
//! separate from the Core-backed engine runtime lifecycle.

use nizaam_indexing::{
    IndexDefinitionId, IndexDefinitionIdentity, IndexFamily, IndexLifecycle, IndexLifecycleState,
    IndexNamespace,
};

fn definition_identity(seed: u8) -> IndexDefinitionIdentity {
    IndexDefinitionIdentity::new(
        IndexDefinitionId::new(format!("phase5.lifecycle.{seed}"))
            .expect("test definition ID should be valid"),
        IndexNamespace::new(format!("phase5.lifecycle.{seed}"))
            .expect("test namespace should be valid"),
        IndexFamily::Identity,
    )
}

fn active_lifecycle(seed: u8) -> IndexLifecycle {
    let mut lifecycle = IndexLifecycle::new(definition_identity(seed));

    lifecycle
        .mark_building()
        .expect("Creating -> Building should be valid");
    lifecycle
        .mark_validating()
        .expect("Building -> Validating should be valid");
    lifecycle
        .mark_ready()
        .expect("Validating -> Ready should be valid");
    lifecycle
        .mark_active()
        .expect("Ready -> Active should be valid");

    lifecycle
}

#[test]
fn new_index_has_creating_state_and_preserves_identity() {
    let id = definition_identity(0x11);
    let lifecycle = IndexLifecycle::new(id.clone());

    assert_eq!(lifecycle.definition_identity(), &id);
    assert_eq!(lifecycle.state(), IndexLifecycleState::Creating);
    assert!(!lifecycle.is_active());
    assert!(!lifecycle.is_terminal());
}

#[test]
fn valid_lifecycle_path_reaches_active_and_retired() {
    let mut lifecycle = active_lifecycle(0x22);

    assert_eq!(lifecycle.state(), IndexLifecycleState::Active);
    assert!(lifecycle.is_active());
    assert!(!lifecycle.is_terminal());

    lifecycle
        .mark_maintaining()
        .expect("Active -> Maintaining should be valid");
    assert_eq!(lifecycle.state(), IndexLifecycleState::Maintaining);
    assert!(!lifecycle.is_active());

    lifecycle
        .mark_active()
        .expect("Maintaining -> Active should be valid");
    assert_eq!(lifecycle.state(), IndexLifecycleState::Active);

    lifecycle
        .mark_retiring()
        .expect("Active -> Retiring should be valid");
    lifecycle
        .mark_retired()
        .expect("Retiring -> Retired should be valid");

    assert_eq!(lifecycle.state(), IndexLifecycleState::Retired);
    assert!(lifecycle.is_terminal());
}

#[test]
fn controlled_retirement_is_allowed_before_activation() {
    let states = [
        IndexLifecycleState::Creating,
        IndexLifecycleState::Building,
        IndexLifecycleState::Validating,
        IndexLifecycleState::Ready,
    ];

    for (seed, target) in states.into_iter().enumerate() {
        let mut lifecycle = IndexLifecycle::new(definition_identity(0x30 + seed as u8));

        match target {
            IndexLifecycleState::Creating => {}
            IndexLifecycleState::Building => lifecycle
                .mark_building()
                .expect("Creating -> Building should be valid"),
            IndexLifecycleState::Validating => {
                lifecycle
                    .mark_building()
                    .expect("Creating -> Building should be valid");
                lifecycle
                    .mark_validating()
                    .expect("Building -> Validating should be valid");
            }
            IndexLifecycleState::Ready => {
                lifecycle
                    .mark_building()
                    .expect("Creating -> Building should be valid");
                lifecycle
                    .mark_validating()
                    .expect("Building -> Validating should be valid");
                lifecycle
                    .mark_ready()
                    .expect("Validating -> Ready should be valid");
            }
            _ => unreachable!("only pre-activation states are tested here"),
        }

        assert_eq!(lifecycle.state(), target);

        lifecycle
            .mark_retiring()
            .expect("pre-activation retirement should be valid");
        lifecycle
            .mark_retired()
            .expect("Retiring -> Retired should be valid");

        assert!(lifecycle.is_terminal());
    }
}

#[test]
fn invalid_transition_is_rejected_without_mutating_state() {
    let id = definition_identity(0x44);
    let mut lifecycle = IndexLifecycle::new(id.clone());

    let error = lifecycle
        .transition_to(IndexLifecycleState::Active)
        .expect_err("Creating -> Active must not bypass validation and readiness");

    assert_eq!(error.definition_identity(), &id);
    assert_eq!(error.from(), IndexLifecycleState::Creating);
    assert_eq!(error.to(), IndexLifecycleState::Active);
    assert_eq!(lifecycle.state(), IndexLifecycleState::Creating);
}

#[test]
fn retired_state_is_terminal() {
    let mut lifecycle = IndexLifecycle::new(definition_identity(0x55));

    lifecycle
        .mark_retiring()
        .expect("Creating -> Retiring should be valid");
    lifecycle
        .mark_retired()
        .expect("Retiring -> Retired should be valid");

    for next in IndexLifecycleState::all() {
        if next == IndexLifecycleState::Retired {
            lifecycle
                .transition_to(next)
                .expect("Retired -> Retired should remain a valid no-op");
        } else {
            let error = lifecycle
                .transition_to(next)
                .expect_err("Retired must not transition to another lifecycle state");

            assert_eq!(error.from(), IndexLifecycleState::Retired);
            assert_eq!(error.to(), next);
            assert_eq!(lifecycle.state(), IndexLifecycleState::Retired);
        }
    }
}

#[test]
fn same_state_transitions_are_idempotent() {
    let mut lifecycle = IndexLifecycle::new(definition_identity(0x66));

    lifecycle
        .transition_to(IndexLifecycleState::Creating)
        .expect("Creating -> Creating should be a no-op");

    lifecycle
        .mark_building()
        .expect("Creating -> Building should be valid");
    lifecycle
        .transition_to(IndexLifecycleState::Building)
        .expect("Building -> Building should be a no-op");

    lifecycle
        .mark_validating()
        .expect("Building -> Validating should be valid");
    lifecycle
        .transition_to(IndexLifecycleState::Validating)
        .expect("Validating -> Validating should be a no-op");

    lifecycle
        .mark_ready()
        .expect("Validating -> Ready should be valid");
    lifecycle
        .transition_to(IndexLifecycleState::Ready)
        .expect("Ready -> Ready should be a no-op");

    lifecycle
        .mark_active()
        .expect("Ready -> Active should be valid");
    lifecycle
        .transition_to(IndexLifecycleState::Active)
        .expect("Active -> Active should be a no-op");

    lifecycle
        .mark_retiring()
        .expect("Active -> Retiring should be valid");
    lifecycle
        .transition_to(IndexLifecycleState::Retiring)
        .expect("Retiring -> Retiring should be a no-op");

    lifecycle
        .mark_retired()
        .expect("Retiring -> Retired should be valid");
    lifecycle
        .transition_to(IndexLifecycleState::Retired)
        .expect("Retired -> Retired should be a no-op");
}

#[test]
fn independent_indexes_keep_independent_lifecycle_state() {
    let mut first = active_lifecycle(0x71);
    let second = IndexLifecycle::new(definition_identity(0x72));

    first
        .mark_maintaining()
        .expect("Active -> Maintaining should be valid");

    assert_eq!(first.state(), IndexLifecycleState::Maintaining);
    assert_eq!(second.state(), IndexLifecycleState::Creating);
    assert_ne!(first.definition_identity(), second.definition_identity());
}

#[test]
fn lifecycle_transitions_do_not_change_core_runtime_state() {
    let runtime = nizaam_indexing::IndexingRuntime::new(
        nizaam_core::identity::EngineId::new("nizaam.indexing.lifecycle.test")
            .expect("engine id should be valid"),
        nizaam_core::identity::EngineInstanceId::new("nizaam.indexing.lifecycle.test.instance")
            .expect("engine instance id should be valid"),
    );

    runtime
        .start()
        .expect("Core-backed runtime should reach Capabilities");

    let runtime_state_before = runtime.state();

    let mut lifecycle = active_lifecycle(0x81);
    lifecycle
        .mark_maintaining()
        .expect("Active -> Maintaining should be valid");
    lifecycle
        .mark_active()
        .expect("Maintaining -> Active should be valid");

    assert_eq!(runtime.state(), runtime_state_before);
}
