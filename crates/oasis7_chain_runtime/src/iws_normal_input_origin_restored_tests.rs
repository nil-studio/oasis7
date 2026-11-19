//! Same-process Node restart at a real persisted positive-height boundary.
//! This is local engineering provenance, not Formal GEN or cold Host recovery.
use super::*;
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "oasis7-normal-origin-restore-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[derive(Default)]
struct RestoredCapture {
    calls: usize,
    origin: Option<PaidNodeInputOriginV1>,
    error: Option<String>,
}

struct RestoringHook {
    fresh: CaptureHook,
    restored: Arc<Mutex<RestoredCapture>>,
    first_restore: Option<mpsc::SyncSender<()>>,
}
impl NodeExecutionHook for RestoringHook {
    fn guarded_local_execution(&self) -> bool {
        self.fresh.guarded_local_execution()
    }

    fn prepare_verified_local_continuation(
        &mut self,
        continuation: &VerifiedNodeLocalContinuationV1<'_>,
    ) -> Result<(), String> {
        self.fresh.prepare_verified_local_continuation(continuation)
    }

    fn pending_local_continuation(
        &mut self,
    ) -> Result<Option<NodeLocalExecutionContinuation>, String> {
        self.fresh.pending_local_continuation()
    }

    fn restored_verified_local_continuation(
        &mut self,
        continuation: &VerifiedNodeRestoredContinuationV1<'_>,
    ) -> Result<(), String> {
        let mut restored = self
            .restored
            .lock()
            .map_err(|_| "restored capture state poisoned".to_string())?;
        restored.calls += 1;
        // A second issuance is a fixture failure, never a budget reset or
        // silent replacement of the already captured owner.
        let outcome = if restored.calls != 1 {
            Err("unexpected repeated restored callback".to_string())
        } else {
            let state = self
                .fresh
                .state
                .lock()
                .map_err(|_| "capture fixture state poisoned".to_string())?;
            if state.original.as_ref() != Some(continuation.original()) {
                Err("Node restored different original data".to_string())
            } else {
                let mut stop = || false;
                let mut ctx = FormalBoundedContextV1::new(&state.memory, &state.work, &mut stop);
                match PaidNodeInputOriginV1::restored(continuation, &mut ctx) {
                    Ok(origin) => {
                        restored.origin = Some(origin);
                        Ok(())
                    }
                    Err(error) => Err(error.to_string()),
                }
            }
        };
        restored.error = outcome.as_ref().err().cloned();
        drop(restored);
        // Node issues restored provenance synchronously inside start().
        // Capacity one is required: a zero-capacity channel would deadlock.
        if let Some(sender) = self.first_restore.take() {
            let _ = sender.send(());
        }
        outcome
    }

    fn on_commit_outcome_with_expected(
        &mut self,
        context: NodeExecutionCommitContext,
        expected_execution_block_hash: Option<&str>,
        expected_execution_state_root: Option<&str>,
    ) -> Result<NodeExecutionCommitOutcome, String> {
        self.fresh.on_commit_outcome_with_expected(
            context,
            expected_execution_block_hash,
            expected_execution_state_root,
        )
    }

    fn on_commit(
        &mut self,
        context: NodeExecutionCommitContext,
    ) -> Result<NodeExecutionCommitResult, String> {
        self.fresh.on_commit(context)
    }

    fn complete_local_continuation(
        &mut self,
        context: &NodeExecutionCommitContext,
    ) -> Result<(), String> {
        self.fresh.complete_local_continuation(context)
    }
}

struct Fixture {
    // Drop Node before deleting only this fixture's dedicated directory.
    node: NodeRuntime,
    _directory: Directory,
    state: Arc<Mutex<CaptureState>>,
    restored: Arc<Mutex<RestoredCapture>>,
    fresh_receiver: mpsc::Receiver<()>,
    restored_receiver: mpsc::Receiver<()>,
}
impl Fixture {
    fn new() -> Self {
        let directory = Directory::new();
        let world_dir = directory.0.join("world");
        let records_dir = directory.0.join("records");
        let mut world = oasis7::runtime_host::world::new();
        world.step().expect("execute the real local setup tick");
        assert_eq!(world.state().time, 1);
        world
            .save_to_dir(&world_dir)
            .expect("persist the real world");
        let bootstrap = crate::execution_bridge_real_tests::real_execution_bridge::
            derive_local_execution_bootstrap(
                &world_dir,
                &records_dir,
                "normal-origin-world",
                1,
                "normal-origin-local-h1",
                &oasis7_runtime::ReleaseSecurityPolicy::default(),
            )
            .expect("derive the actual persisted positive-height boundary");
        assert_eq!(bootstrap.height, 1);
        drop(world);
        let state = Arc::new(Mutex::new(CaptureState {
            memory: MemoryBudget::new(FIXTURE_BYTES as usize, 0).unwrap(),
            work: FormalBoundedWorkV1::new(FIXTURE_STEPS, FIXTURE_BYTES).unwrap(),
            original: None,
            origin: None,
            capture_error: None,
        }));
        let restored = Arc::new(Mutex::new(RestoredCapture::default()));
        let (fresh_sender, fresh_receiver) = mpsc::sync_channel(1);
        let (restored_sender, restored_receiver) = mpsc::sync_channel(1);
        let config = NodeConfig::new(
            "normal-origin-node",
            "normal-origin-world",
            NodeRole::Sequencer,
        )
        .unwrap()
        .with_tick_interval(Duration::from_millis(5))
        .unwrap();
        let node = NodeRuntime::new(config)
            .with_local_execution_bootstrap(bootstrap)
            .with_execution_hook(RestoringHook {
                fresh: CaptureHook {
                    state: Arc::clone(&state),
                    first_capture: Some(fresh_sender),
                },
                restored: Arc::clone(&restored),
                first_restore: Some(restored_sender),
            });
        Self {
            node,
            _directory: directory,
            state,
            restored,
            fresh_receiver,
            restored_receiver,
        }
    }

    fn capture_fresh_and_stop(&mut self) {
        self.node.start().expect("start at the real h1 boundary");
        let observed = self.fresh_receiver.recv_timeout(Duration::from_secs(5));
        let stopped = self.node.stop();
        stopped.expect("join first Node worker before assertions");
        assert!(observed.is_ok(), "{:?}", self.node.snapshot());
        self.assert_deferred_boundary();
        let state = self.state.lock().unwrap();
        assert!(state.capture_error.is_none());
        assert!(state.origin.is_some());
        assert_eq!(state.original.as_ref().unwrap().context.height, 2);
    }

    fn assert_deferred_boundary(&self) {
        let snapshot = self.node.snapshot();
        assert!(!snapshot.running);
        assert!(snapshot.last_error.is_none(), "{snapshot:?}");
        assert_eq!(snapshot.consensus.committed_height, 1);
        assert_eq!(snapshot.consensus.last_execution_height, 1);
    }
}

#[test]
fn actual_node_restart_restores_exact_origin_and_continues_original_work() {
    let mut fixture = Fixture::new();
    fixture.capture_fresh_and_stop();
    let (identity, steps, bytes, paid) = {
        let state = fixture.state.lock().unwrap();
        (
            state.work.identity(),
            state.work.remaining_steps(),
            state.work.remaining_read_bytes(),
            state.memory.used(),
        )
    };
    fixture
        .node
        .start()
        .expect("restore the original pending decision");
    let observed = fixture
        .restored_receiver
        .recv_timeout(Duration::from_secs(5));
    let stopped = fixture.node.stop();
    stopped.expect("join restored Node worker before assertions");
    assert!(observed.is_ok(), "{:?}", fixture.node.snapshot());
    fixture.assert_deferred_boundary();

    let mut state = fixture.state.lock().unwrap();
    let fresh = state.origin.take().unwrap();
    let mut captured = fixture.restored.lock().unwrap();
    assert_eq!(captured.calls, 1);
    assert!(captured.error.is_none());
    let restored = captured.origin.take().unwrap();
    let original = state.original.as_ref().unwrap();
    let expected_input = serde_cbor::to_vec(&original.context).unwrap();
    let expected_continuation = serde_cbor::to_vec(original).unwrap();
    assert_eq!(fresh.input_bytes(), expected_input);
    assert_eq!(restored.input_bytes(), expected_input);
    assert_eq!(fresh.continuation_bytes(), expected_continuation);
    assert_eq!(restored.continuation_bytes(), expected_continuation);
    assert!(!fresh.is_node_restored());
    assert!(restored.is_node_restored());
    assert!(restored.uses_budget(&state.memory));
    assert_eq!(state.work.identity(), identity);
    assert_eq!(
        state.work.remaining_steps(),
        steps - (FIXTURE_STEPS - steps)
    );
    assert_eq!(
        state.work.remaining_read_bytes(),
        bytes - 2 * (expected_input.len() + expected_continuation.len()) as u64
    );
    assert!(state.memory.used() > paid);
    drop(restored);
    assert_eq!(state.memory.used(), paid);
    drop(fresh);
    assert_eq!(state.memory.used(), 0);
}

#[test]
fn changed_pending_slot_is_rejected_before_restored_provenance_is_issued() {
    let mut fixture = Fixture::new();
    fixture.capture_fresh_and_stop();
    let (identity, steps, bytes, paid) = {
        let mut state = fixture.state.lock().unwrap();
        // Corrupt only returned pending DATA, not the paid owner or the Core.
        let pending = state.original.as_mut().unwrap();
        pending.next_slot = pending.next_slot.checked_add(1).unwrap();
        (
            state.work.identity(),
            state.work.remaining_steps(),
            state.work.remaining_read_bytes(),
            state.memory.used(),
        )
    };
    let error = fixture
        .node
        .start()
        .expect_err("invalid saved slot must fail");
    assert!(
        error
            .to_string()
            .contains("guarded continuation next slot is not the committed slot successor"),
        "{error}"
    );
    // Failure is synchronous, before a worker exists. No successful stop is
    // required and no restored capability may have been minted.
    assert!(!fixture.node.snapshot().running);
    assert!(matches!(
        fixture.restored_receiver.try_recv(),
        Err(mpsc::TryRecvError::Empty)
    ));
    let captured = fixture.restored.lock().unwrap();
    assert_eq!(captured.calls, 0);
    assert!(captured.origin.is_none());
    assert!(captured.error.is_none());
    let mut state = fixture.state.lock().unwrap();
    assert_eq!(state.work.identity(), identity);
    assert_eq!(state.work.remaining_steps(), steps);
    assert_eq!(state.work.remaining_read_bytes(), bytes);
    assert_eq!(state.memory.used(), paid);
    let fresh = state.origin.take().unwrap();
    assert!(!fresh.is_node_restored());
    drop(fresh);
    assert_eq!(state.memory.used(), 0);
}
