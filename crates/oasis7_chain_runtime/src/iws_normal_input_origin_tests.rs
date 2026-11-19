//! Real Node-issued fresh provenance. A separately gated child covers restart
//! at a persisted local boundary; neither claims Formal GEN or an applied tick.
use super::*;
use oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedWorkV1;
use oasis7_node::{
    NodeConfig, NodeExecutionCommitContext, NodeExecutionCommitOutcome, NodeExecutionCommitResult,
    NodeExecutionHook, NodeLocalExecutionContinuation, NodeRole, NodeRuntime,
};
use std::{
    sync::{Arc, Mutex, mpsc},
    time::Duration,
};

const FIXTURE_STEPS: u64 = 4096;
const FIXTURE_BYTES: u64 = 16 * 1024;

struct CaptureState {
    memory: MemoryBudget,
    work: FormalBoundedWorkV1,
    // Independent expected DATA, never a constructor for a verified capability.
    original: Option<NodeLocalExecutionContinuation>,
    origin: Option<PaidNodeInputOriginV1>,
    capture_error: Option<FormalError>,
}

struct CaptureHook {
    state: Arc<Mutex<CaptureState>>,
    first_capture: Option<mpsc::SyncSender<()>>,
}

impl NodeExecutionHook for CaptureHook {
    fn guarded_local_execution(&self) -> bool {
        true
    }

    fn prepare_verified_local_continuation(
        &mut self,
        continuation: &VerifiedNodeLocalContinuationV1<'_>,
    ) -> Result<(), String> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| "capture fixture state poisoned".to_string())?;
        // Deferred may call prepare again. Keep the original decision and work;
        // neither a retry nor a new callback may recapture or refill them.
        if let Some(original) = state.original.as_ref() {
            if original != continuation.original() {
                return Err("capture fixture original continuation changed".into());
            }
            return if state.origin.is_some() {
                Ok(())
            } else {
                Err("capture fixture previous capture failed".into())
            };
        }
        state.original = Some(continuation.original().clone());
        let captured = {
            let mut stop = || false;
            let mut ctx = FormalBoundedContextV1::new(&state.memory, &state.work, &mut stop);
            PaidNodeInputOriginV1::fresh(continuation, &mut ctx)
        };
        let outcome = match captured {
            Ok(origin) => {
                state.origin = Some(origin);
                Ok(())
            }
            Err(error) => {
                let reason = error.to_string();
                state.capture_error = Some(error);
                Err(reason)
            }
        };
        drop(state);
        if let Some(sender) = self.first_capture.take() {
            let _ = sender.send(());
        }
        outcome
    }

    fn pending_local_continuation(
        &mut self,
    ) -> Result<Option<NodeLocalExecutionContinuation>, String> {
        self.state
            .lock()
            .map(|state| state.original.clone())
            .map_err(|_| "capture fixture state poisoned".to_string())
    }

    fn on_commit_outcome_with_expected(
        &mut self,
        context: NodeExecutionCommitContext,
        expected_execution_block_hash: Option<&str>,
        expected_execution_state_root: Option<&str>,
    ) -> Result<NodeExecutionCommitOutcome, String> {
        let state = self
            .state
            .lock()
            .map_err(|_| "capture fixture state poisoned".to_string())?;
        let original = state
            .original
            .as_ref()
            .ok_or_else(|| "capture fixture original is missing".to_string())?;
        if state.origin.is_none()
            || original.context != context
            || expected_execution_block_hash.is_some()
            || expected_execution_state_root.is_some()
        {
            return Err("capture fixture execution context changed".into());
        }
        Ok(NodeExecutionCommitOutcome::Deferred)
    }

    fn on_commit(
        &mut self,
        _context: NodeExecutionCommitContext,
    ) -> Result<NodeExecutionCommitResult, String> {
        Err("capture fixture requires typed Deferred outcome".into())
    }

    fn complete_local_continuation(
        &mut self,
        _context: &NodeExecutionCommitContext,
    ) -> Result<(), String> {
        Err("capture fixture must never complete a deferred decision".into())
    }
}

fn capture_with_real_node(memory: MemoryBudget, work: FormalBoundedWorkV1) -> CaptureState {
    let state = Arc::new(Mutex::new(CaptureState {
        memory,
        work,
        original: None,
        origin: None,
        capture_error: None,
    }));
    let (sender, receiver) = mpsc::sync_channel(1);
    // Default is one real local validator, without gossip or replication.
    // No bootstrap or fabricated InitialActivation is used for fresh capture.
    let config = NodeConfig::new(
        "normal-origin-node",
        "normal-origin-world",
        NodeRole::Sequencer,
    )
    .unwrap()
    .with_tick_interval(Duration::from_millis(5))
    .unwrap();
    let mut node = NodeRuntime::new(config).with_execution_hook(CaptureHook {
        state: Arc::clone(&state),
        first_capture: Some(sender),
    });
    node.start().expect("start real Node");
    let observed = receiver.recv_timeout(Duration::from_secs(5));
    // Join before checking the deadline, including on the failing-test path.
    let stopped = node.stop();
    let snapshot = node.snapshot();
    drop(node);
    stopped.expect("stop real Node");
    assert!(
        observed.is_ok(),
        "actual fresh callback absent: {snapshot:?}"
    );
    let state = Arc::try_unwrap(state)
        .ok()
        .expect("stopped Node released the capture hook")
        .into_inner()
        .unwrap_or_else(|_| panic!("capture fixture state poisoned"));
    assert!(!snapshot.running);
    assert_eq!(snapshot.consensus.last_execution_height, 0);
    if state.capture_error.is_none() {
        assert!(
            snapshot.last_error.is_none(),
            "fresh fixture failed: {snapshot:?}"
        );
        assert_eq!(snapshot.consensus.committed_height, 0);
    }
    state
}

fn captured() -> CaptureState {
    let state = capture_with_real_node(
        MemoryBudget::new(FIXTURE_BYTES as usize, 0).unwrap(),
        FormalBoundedWorkV1::new(FIXTURE_STEPS, FIXTURE_BYTES).unwrap(),
    );
    assert!(state.capture_error.is_none());
    assert!(state.origin.is_some());
    state
}

fn assert_invalid(error: FormalError, expected: &str) {
    assert!(matches!(error, FormalError::Invalid(message) if message == expected));
}

fn validate(
    origin: &PaidNodeInputOriginV1,
    memory: &MemoryBudget,
    work: &FormalBoundedWorkV1,
    cancelled: bool,
) -> Result<(), FormalError> {
    let mut stop = || cancelled;
    origin.validate_context(&mut FormalBoundedContextV1::new(memory, work, &mut stop))
}

#[test]
fn fresh_hook_preserves_node_originals_and_continues_the_original_meter() {
    let mut state = captured();
    let origin = state.origin.take().unwrap();
    let original = state.original.as_ref().unwrap();
    let input = serde_cbor::to_vec(&original.context).unwrap();
    let continuation = serde_cbor::to_vec(original).unwrap();
    assert_eq!(origin.input_bytes(), input);
    assert_eq!(origin.continuation_bytes(), continuation);
    assert!(!origin.is_node_restored());
    assert_eq!(
        state.work.remaining_read_bytes(),
        FIXTURE_BYTES - 2 * (input.len() + continuation.len()) as u64
    );
    let steps = state.work.remaining_steps();
    let bytes = state.work.remaining_read_bytes();
    assert!(steps < FIXTURE_STEPS);
    assert!(state.memory.used() > 0);
    validate(&origin, &state.memory, &state.work, false).unwrap();
    validate(&origin, &state.memory, &state.work, false).unwrap();
    assert_eq!(state.work.remaining_steps(), steps - 2);
    assert_eq!(state.work.remaining_read_bytes(), bytes);
    drop(origin);
    assert_eq!(state.memory.used(), 0);
}

#[test]
fn foreign_core_and_pool_do_not_replace_origin_or_release_original_charges() {
    let mut state = captured();
    let origin = state.origin.take().unwrap();
    let steps = state.work.remaining_steps();
    let bytes = state.work.remaining_read_bytes();
    let reserved = state.memory.used();
    let foreign_core = FormalBoundedWorkV1::new(steps, bytes).unwrap();
    assert_invalid(
        validate(&origin, &state.memory, &foreign_core, false).unwrap_err(),
        "normal Node origin pool/Core",
    );
    assert_eq!(foreign_core.remaining_steps(), steps - 1);
    assert_eq!(state.work.remaining_steps(), steps);
    let foreign_pool = MemoryBudget::new(FIXTURE_BYTES as usize, 0).unwrap();
    assert_invalid(
        validate(&origin, &foreign_pool, &state.work, false).unwrap_err(),
        "normal Node origin pool/Core",
    );
    assert_eq!(state.work.remaining_steps(), steps - 1);
    assert_eq!(state.memory.used(), reserved);
    assert_eq!(foreign_pool.used(), 0);
    validate(&origin, &state.memory, &state.work, false).unwrap();
    assert_eq!(state.work.remaining_steps(), steps - 2);
    assert_eq!(state.work.remaining_read_bytes(), bytes);
    assert_eq!(state.memory.used(), reserved);
    drop(origin);
    assert_eq!(state.memory.used(), 0);
}

#[test]
fn cancellation_precedes_foreign_identity_without_charging_or_refilling_work() {
    let mut state = captured();
    let origin = state.origin.take().unwrap();
    let steps = state.work.remaining_steps();
    let bytes = state.work.remaining_read_bytes();
    let reserved = state.memory.used();
    let foreign_core = FormalBoundedWorkV1::new(steps, bytes).unwrap();
    let foreign_pool = MemoryBudget::new(FIXTURE_BYTES as usize, 0).unwrap();
    assert_invalid(
        validate(&origin, &foreign_pool, &foreign_core, true).unwrap_err(),
        "bounded archive read cancelled",
    );
    assert_eq!(foreign_core.remaining_steps(), steps);
    assert_eq!(foreign_core.remaining_read_bytes(), bytes);
    assert_eq!(state.work.remaining_steps(), steps);
    assert_eq!(state.work.remaining_read_bytes(), bytes);
    assert_eq!(state.memory.used(), reserved);
    validate(&origin, &state.memory, &state.work, false).unwrap();
    assert_eq!(state.work.remaining_steps(), steps - 1);
    drop(origin);
    assert_eq!(state.memory.used(), 0);
}

#[test]
fn fresh_capture_admission_failure_mints_no_origin_and_keeps_the_spent_step() {
    let state = capture_with_real_node(
        MemoryBudget::new(0, 0).unwrap(),
        FormalBoundedWorkV1::new(FIXTURE_STEPS, FIXTURE_BYTES).unwrap(),
    );
    assert!(state.origin.is_none());
    assert_invalid(state.capture_error.unwrap(), "normal Node origin admission");
    assert_eq!(state.work.remaining_steps(), FIXTURE_STEPS - 1);
    assert_eq!(state.work.remaining_read_bytes(), FIXTURE_BYTES);
    assert_eq!(state.memory.used(), 0);
}

#[test]
fn fresh_capture_step_exhaustion_precedes_memory_admission() {
    let state = capture_with_real_node(
        MemoryBudget::new(0, 0).unwrap(),
        FormalBoundedWorkV1::new(0, FIXTURE_BYTES).unwrap(),
    );
    assert!(state.origin.is_none());
    assert_invalid(state.capture_error.unwrap(), "bounded archive read steps");
    assert_eq!(state.work.remaining_steps(), 0);
    assert_eq!(state.work.remaining_read_bytes(), FIXTURE_BYTES);
    assert_eq!(state.memory.used(), 0);
}

#[cfg(feature = "node-libp2p")]
#[path = "iws_normal_input_origin_restored_tests.rs"]
mod restored_tests;
