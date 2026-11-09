//! Deterministic compensation/lifecycle interleaving, without a timing race.
use super::*;
use std::sync::mpsc;

fn completion_releases_root_before_waiting_for_compensation(joined: bool) {
    let gate = Arc::new(WorldGate::default());
    *gate.root.lock().unwrap() = Some(std::path::PathBuf::from("unused-lock-order-root"));
    // This is the selector side of the original compensation -> root order.
    let compensation = gate.compensation_settle.lock().unwrap();
    let (snapshot_tx, snapshot_rx) = mpsc::channel();
    let (continue_tx, continue_rx) = mpsc::channel();
    let completion_gate = gate.clone();
    let worker = thread::spawn(move || {
        let after_snapshot = || {
            snapshot_tx.send(()).unwrap();
            continue_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        };
        if joined {
            completion_gate.connection_worker_joined_after_snapshot(
                thread::current().id(),
                after_snapshot,
            );
        } else {
            completion_gate.connection_worker_finished_after_snapshot(after_snapshot);
        }
    });
    let reached_snapshot = snapshot_rx.recv_timeout(Duration::from_secs(2));
    // Never block here: the old if-let scope reliably reports WouldBlock.
    // Release both rendezvous and compensation before asserting, so even RED
    // joins its real completion worker instead of leaving a deadlocked thread.
    let root_available = gate.root.try_lock().is_ok();
    let _ = continue_tx.send(());
    drop(compensation);
    let joined_worker = worker.join();
    reached_snapshot.expect("actual lifecycle callback must reach its root snapshot");
    joined_worker.expect("actual lifecycle callback must return after compensation release");
    assert!(
        root_available,
        "root must be released before entering compensation state"
    );
}

#[test]
fn finished_callback_releases_root_before_compensation_lock() {
    completion_releases_root_before_waiting_for_compensation(false);
}

#[test]
fn joined_callback_releases_root_before_compensation_lock() {
    completion_releases_root_before_waiting_for_compensation(true);
}
