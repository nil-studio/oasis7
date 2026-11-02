//! Actual Node shared-inner-lock reachability, not formal commit/HTTP authority.
use super::*;
use crate::{
    NodeConfig, NodeError, NodeExecutionCommitContext, NodeExecutionCommitResult,
    NodeExecutionHook, NodeRole, NodeRuntime,
};
use std::sync::mpsc;
use std::time::Duration;
struct OwnerHook;
impl NodeExecutionHook for OwnerHook {
    fn guarded_local_execution(&self) -> bool {
        true
    }
    fn validate_cleanup_v2_action_payload(&self, q: &TaskCleanupSubmitV2) -> Result<(), String> {
        q.verify_owner_signature()
    }
    fn on_commit(
        &mut self,
        _: NodeExecutionCommitContext,
    ) -> Result<NodeExecutionCommitResult, String> {
        Err("admission-only component never commits".into())
    }
}
// Baseline bindings are the actual two APIs used by bffa's new caller. The
// authorized fix must bind these to closed-only bounded Node APIs, preserving
// each signed sample/assertion. Ordinary snapshot/submit behavior stays intact.
fn profile_status(runtime: &NodeRuntime) -> Result<(), NodeError> {
    runtime.try_closed_task_mutation_status().map(|_| ())
}
fn profile_submit(runtime: &NodeRuntime, id: u64, raw: Vec<u8>) -> Result<(), NodeError> {
    runtime.try_submit_closed_task_mutation_payload(id, raw)
}
fn runtime() -> NodeRuntime {
    NodeRuntime::new(NodeConfig::new("node", "world", NodeRole::Observer).unwrap())
        .with_execution_hook(OwnerHook)
}
#[test]
fn closed_mutation_internal_state_contention_is_bounded_before_state_release() {
    let runtime = runtime();
    profile_status(&runtime).unwrap();
    let state = runtime.state.clone();
    let held = state.lock().unwrap();
    let (tx, rx) = mpsc::sync_channel(1);
    let worker = std::thread::spawn(move || {
        tx.send(profile_status(&runtime)).unwrap();
        runtime
    });
    let before_release = rx.recv_timeout(Duration::from_millis(150));
    // Release and join before asserting even on the old blocking implementation.
    drop(held);
    let runtime = worker.join().unwrap();
    profile_status(&runtime).unwrap();
    assert!(
        matches!(before_release, Ok(Err(_))),
        "new closed profile must refuse held Node inner state without waiting"
    );
}
#[test]
fn closed_mutation_internal_pending_contention_is_bounded_before_pending_release() {
    let runtime = runtime();
    let original = close_catalog(request());
    profile_submit(&runtime, 301, original.clone()).unwrap();
    let pending = runtime.pending_consensus_actions.clone();
    let held = pending.lock().unwrap();
    let (tx, rx) = mpsc::sync_channel(1);
    let worker = std::thread::spawn(move || {
        tx.send(profile_submit(&runtime, 302, original)).unwrap();
        runtime
    });
    let before_release = rx.recv_timeout(Duration::from_millis(150));
    drop(held);
    let runtime = worker.join().unwrap();
    profile_submit(&runtime, 303, close_catalog(request())).unwrap();
    assert!(
        matches!(before_release, Ok(Err(_))),
        "new closed profile must refuse held inner pending queue before byte reservation or enqueue"
    );
}
