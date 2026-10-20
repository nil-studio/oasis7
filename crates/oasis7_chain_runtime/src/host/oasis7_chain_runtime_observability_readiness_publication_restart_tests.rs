use super::*;

use std::sync::{Condvar, Mutex, mpsc};
use std::time::{Duration, Instant};

use oasis7_node::{NodeConfig, NodeRuntime};

use crate::publication_lifecycle::{PublicationLifecycleObserver, load_snapshot};

fn wait_for_runtime_snapshot(
    runtime: &NodeRuntime,
    predicate: impl Fn(&NodeSnapshot) -> bool,
) -> NodeSnapshot {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let snapshot = runtime.snapshot();
        if predicate(&snapshot) {
            return snapshot;
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for runtime snapshot: {snapshot:?}"
        );
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn runtime_restart_recovers_publication_after_busy_predecessor_without_new_snapshot() {
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after epoch")
        .as_millis() as i64;
    let (root, records_dir, manifest) = prepare_world(
        "runtime-restart-busy-publication",
        &[(800, now_ms - 2_000), (801, now_ms - 1_000)],
    );
    let execution_world_dir = root.join("execution-world");
    let predecessor = lifecycle_snapshot(800, now_ms - 2_000, 800, now_ms - 2_000, now_ms - 100);
    let successor = lifecycle_snapshot(801, now_ms - 1_000, 801, now_ms - 1_000, now_ms - 50);

    let (entered_tx, entered_rx) = mpsc::channel();
    let release = Arc::new((Mutex::new(false), Condvar::new()));
    let hook_release = Arc::clone(&release);
    let observer = PublicationLifecycleObserver::new(
        predecessor.node_id.clone(),
        predecessor.player_id.clone(),
        predecessor.world_id.clone(),
        predecessor.role,
        predecessor.replication_enabled,
        Some(manifest.clone()),
        execution_world_dir.clone(),
        records_dir.clone(),
    )
    .with_before_native_replace_hook_for_test(Arc::new(move || {
        entered_tx
            .send(())
            .expect("report predecessor staged commit entry");
        let (released_lock, released_signal) = &*hook_release;
        let released = released_lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        drop(
            released_signal
                .wait_while(released, |released| !*released)
                .unwrap_or_else(|poisoned| poisoned.into_inner()),
        );
    }));
    let config = NodeConfig::new(
        predecessor.node_id.clone(),
        predecessor.world_id.clone(),
        NodeRole::Sequencer,
    )
    .expect("runtime config")
    .with_tick_interval(Duration::from_secs(60))
    .expect("slow deterministic runtime tick")
    .with_replication_root(root.join("replication"))
    .expect("runtime replication root");
    let mut runtime = NodeRuntime::new(config).with_consensus_progress_observer(observer);
    runtime.start().expect("start predecessor runtime");
    runtime
        .submit_consensus_progress_for_test(predecessor.consensus.clone(), now_ms - 100)
        .expect("submit predecessor progress");
    entered_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("predecessor must block after acquiring durable commit authority");
    assert!(
        !execution_world_dir.join(STATE_FILE).exists(),
        "blocked staged commit became durable before native replace"
    );

    let (stopped_tx, stopped_rx) = mpsc::channel();
    thread::spawn(move || {
        let result = runtime.stop();
        let _ = stopped_tx.send((runtime, result));
    });
    let (mut runtime, stop_result) = stopped_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("runtime stop must remain bounded while predecessor commit is blocked");
    stop_result.expect("stop predecessor runtime");
    runtime.start().expect("restart successor runtime");
    runtime
        .submit_consensus_progress_for_test(successor.consensus.clone(), now_ms)
        .expect("submit successor progress exactly once");

    let busy = wait_for_runtime_snapshot(&runtime, |snapshot| {
        snapshot
            .consensus_progress_observer_error
            .as_ref()
            .and_then(|error| error.code.as_deref())
            == Some("observer_lifecycle_commit_busy")
    });
    let blocked_status = construct_status(StatusMethod::Get, busy, &manifest, &root, &records_dir);
    assert!(
        is_critical_not_ready(&blocked_status),
        "{}",
        outcome_diagnostic(&blocked_status)
    );

    let (released_lock, released_signal) = &*release;
    *released_lock
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = true;
    released_signal.notify_all();

    let recovered = wait_for_runtime_snapshot(&runtime, |snapshot| {
        snapshot.consensus_progress_observer_error.is_none()
            && load_snapshot(execution_world_dir.as_path())
                .ok()
                .flatten()
                .and_then(|state| state.catch_up)
                .is_some_and(|binding| binding.height == 801)
    });
    let durable = load_snapshot(execution_world_dir.as_path())
        .expect("read durable lifecycle state")
        .expect("successor lifecycle state must exist");
    assert!(durable.episode.is_none());
    assert_eq!(
        durable.catch_up.as_ref().map(|binding| binding.height),
        Some(801)
    );
    let recovered_status = construct_status(
        StatusMethod::Head,
        recovered,
        &manifest,
        &root,
        &records_dir,
    );
    assert!(
        recovered_status.observability.ready && recovered_status.readiness.ready,
        "{}",
        outcome_diagnostic(&recovered_status)
    );
    assert!(
        recovered_status
            .observability
            .alerts
            .iter()
            .all(|alert| alert.code != "consensus_progress_observer_error"),
        "{}",
        outcome_diagnostic(&recovered_status)
    );

    runtime.stop().expect("stop successor runtime");
    fs::remove_dir_all(root).expect("remove runtime restart fixture");
}

#[test]
fn runtime_restart_rejects_precheck_predecessor_from_reused_generation() {
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after epoch")
        .as_millis() as i64;
    let (root, records_dir, manifest) =
        prepare_world("runtime-restart-generation-fence", &[(900, now_ms - 1_000)]);
    let execution_world_dir = root.join("execution-world");
    let predecessor = lifecycle_snapshot(900, now_ms - 1_000, 900, now_ms - 1_000, now_ms - 50);
    let (entered_tx, entered_rx) = mpsc::channel();
    let (result_tx, result_rx) = mpsc::channel();
    let release = Arc::new((Mutex::new(false), Condvar::new()));
    let hook_release = Arc::clone(&release);
    let observer = PublicationLifecycleObserver::new(
        predecessor.node_id.clone(),
        predecessor.player_id.clone(),
        predecessor.world_id.clone(),
        predecessor.role,
        predecessor.replication_enabled,
        Some(manifest),
        execution_world_dir.clone(),
        records_dir,
    )
    .with_before_final_generation_check_hook_for_test(Arc::new(move || {
        entered_tx
            .send(())
            .expect("report predecessor precheck entry");
        let (released_lock, released_signal) = &*hook_release;
        let released = released_lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        drop(
            released_signal
                .wait_while(released, |released| !*released)
                .unwrap_or_else(|poisoned| poisoned.into_inner()),
        );
    }))
    .with_publication_result_hook_for_test(Arc::new(move |error| {
        result_tx.send(error).expect("report predecessor result");
    }));
    let config = NodeConfig::new(
        predecessor.node_id.clone(),
        predecessor.world_id.clone(),
        NodeRole::Sequencer,
    )
    .expect("runtime config")
    .with_tick_interval(Duration::from_secs(60))
    .expect("slow deterministic runtime tick")
    .with_replication_root(root.join("replication"))
    .expect("runtime replication root");
    let mut runtime = NodeRuntime::new(config).with_consensus_progress_observer(observer);
    runtime.start().expect("start predecessor runtime");
    runtime
        .submit_consensus_progress_for_test(predecessor.consensus, now_ms)
        .expect("submit predecessor progress");
    entered_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("predecessor must pause before its final generation check");

    let (stopped_tx, stopped_rx) = mpsc::channel();
    thread::spawn(move || {
        let result = runtime.stop();
        let _ = stopped_tx.send((runtime, result));
    });
    let (mut runtime, stop_result) = stopped_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("runtime stop must remain bounded before final generation check");
    stop_result.expect("stop predecessor runtime");
    runtime.start().expect("restart successor runtime");

    let (released_lock, released_signal) = &*release;
    *released_lock
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = true;
    released_signal.notify_all();
    assert_eq!(
        result_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("predecessor must finish after release")
            .as_deref(),
        Some("observer_lifecycle_revoked")
    );
    assert!(
        !execution_world_dir.join(STATE_FILE).exists(),
        "precheck predecessor published after restart"
    );

    runtime.stop().expect("stop successor runtime");
    fs::remove_dir_all(root).expect("remove generation fence fixture");
}
