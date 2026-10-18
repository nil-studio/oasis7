//! RED contract for the Phase 0 typed transition kernel.
//!
//! These tests deliberately use an isolated kernel state rather than `World`.
//! The production implementation must provide the same staged/commit semantics
//! before any world mutation surface is migrated to it.

use std::collections::BTreeMap;

use super::super::{ExecutionTransaction, TransitionBaseHead, TransitionKernelState};

fn base_head(version: u64) -> TransitionBaseHead {
    TransitionBaseHead::from_state_root(format!("kernel-state-head-{version}"))
}

fn canonical_state() -> TransitionKernelState {
    TransitionKernelState {
        scalar: 7,
        entries: BTreeMap::from([("ore".to_string(), 11), ("water".to_string(), 5)]),
    }
}

#[test]
fn staged_scalar_and_map_mutations_are_invisible_until_commit() {
    let mut canonical = canonical_state();
    let before = canonical.clone();
    let head = base_head(3);
    let mut transaction = ExecutionTransaction::begin(&canonical, head.clone());

    transaction.buffer_mut().set_scalar(19);
    transaction.buffer_mut().map_insert("ore".to_string(), 23);
    transaction.buffer_mut().map_insert("fuel".to_string(), 2);

    assert_eq!(canonical, before, "staging mutated canonical state");
    assert_eq!(transaction.view().scalar, 19);
    assert_eq!(transaction.view().entries.get("ore"), Some(&23));
    assert_eq!(transaction.view().entries.get("fuel"), Some(&2));

    let prepared = transaction
        .prepare()
        .expect("valid typed delta must prepare");
    prepared
        .commit_into(&mut canonical, &head)
        .expect("prepared commit must install atomically");

    assert_eq!(canonical.scalar, 19);
    assert_eq!(canonical.entries.get("ore"), Some(&23));
    assert_eq!(canonical.entries.get("fuel"), Some(&2));
}

#[test]
fn abort_discards_staged_scalar_and_map_mutations() {
    let canonical = canonical_state();
    let before = canonical.clone();
    let mut transaction = ExecutionTransaction::begin(&canonical, base_head(4));

    transaction.buffer_mut().set_scalar(41);
    transaction.buffer_mut().map_insert("ore".to_string(), 99);
    transaction.abort();

    assert_eq!(canonical, before, "abort published a staged mutation");
}

#[test]
fn nested_savepoint_rollback_removes_child_mutations_but_keeps_root_mutations() {
    let canonical = canonical_state();
    let mut transaction = ExecutionTransaction::begin(&canonical, base_head(5));

    transaction.buffer_mut().set_scalar(13);
    transaction
        .buffer_mut()
        .map_insert("root-only".to_string(), 17);
    let child_savepoint = transaction.savepoint();

    transaction.buffer_mut().set_scalar(29);
    transaction
        .buffer_mut()
        .map_insert("child-only".to_string(), 31);
    transaction
        .rollback_to(child_savepoint)
        .expect("valid child savepoint must roll back");

    assert_eq!(transaction.view().scalar, 13);
    assert_eq!(transaction.view().entries.get("root-only"), Some(&17));
    assert_eq!(transaction.view().entries.get("child-only"), None);
}

#[test]
fn prepared_commit_rejects_a_mismatched_base_head_without_publishing() {
    let mut canonical = canonical_state();
    let before = canonical.clone();
    let expected_head = base_head(6);
    let mut transaction = ExecutionTransaction::begin(&canonical, expected_head.clone());
    transaction.buffer_mut().set_scalar(53);
    let prepared = transaction
        .prepare()
        .expect("valid typed delta must prepare");

    let error = prepared
        .commit_into(&mut canonical, &base_head(7))
        .expect_err("stale base head must fail closed");

    assert!(error.is_base_head_mismatch());
    assert_eq!(canonical, before, "stale commit changed canonical state");
}
