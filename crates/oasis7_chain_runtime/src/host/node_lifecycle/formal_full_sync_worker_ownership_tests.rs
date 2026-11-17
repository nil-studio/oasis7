//! Regression coverage for the exact worker launcher used by the supervisor.
use super::*;

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let dir = std::env::temp_dir().canonicalize().unwrap().join(format!(
            "oasis7-full-sync-owner-{}",
            io::random_id().unwrap()
        ));
        io::private_dir(&dir).unwrap();
        drop(io::private_file(&dir.join("process.lock")).unwrap());
        Self(dir)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn detached_full_sync_worker_keeps_owner_until_exit_or_panic() {
    for panic_after_release in [false, true] {
        let fixture = Fixture::new();
        let ownership = Arc::new(lock(&fixture.0).unwrap());
        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let worker = spawn_owned_worker(Arc::clone(&ownership), move || {
            entered_tx.send(()).unwrap();
            release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            assert!(!panic_after_release, "intentional worker unwind");
        })
        .unwrap();
        entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        // Equivalent to stop removing the supervisor's reference. The worker
        // has not exited, so the next incarnation must still be rejected.
        drop(ownership);
        assert!(lock(&fixture.0).is_err());
        release_tx.send(()).unwrap();
        assert_eq!(worker.join().is_err(), panic_after_release);
        let next_owner = lock(&fixture.0).unwrap();
        drop(next_owner);
    }
}

#[test]
fn full_sync_worker_exit_does_not_unlock_live_supervisor() {
    let fixture = Fixture::new();
    let ownership = Arc::new(lock(&fixture.0).unwrap());
    let worker = spawn_owned_worker(Arc::clone(&ownership), || ()).unwrap();
    worker.join().unwrap();
    // A worker failure/exit notification is not permission to steal the still
    // live supervisor's ownership while it persists terminal state.
    assert!(lock(&fixture.0).is_err());
    drop(ownership);
    let next_owner = lock(&fixture.0).unwrap();
    drop(next_owner);
}

#[test]
fn initialization_failure_stops_worker_before_releasing_last_owner() {
    let fixture = Fixture::new();
    let ownership = Arc::new(lock(&fixture.0).unwrap());
    let flag = Arc::new(AtomicBool::new(false));
    let stop_guard = WorkerStop {
        flag: Arc::clone(&flag),
        status_gate: Arc::new(Mutex::new(())),
    };
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let worker = spawn_owned_worker(Arc::clone(&ownership), move || {
        entered_tx.send(()).unwrap();
        release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(flag.load(Ordering::SeqCst));
    })
    .unwrap();
    entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    // Model an error after spawn (e.g. signal registration): stack cleanup
    // signals stop, but does not release the still-executing worker's fence.
    drop(stop_guard);
    drop(ownership);
    assert!(lock(&fixture.0).is_err());
    release_tx.send(()).unwrap();
    worker.join().unwrap();
    let next_owner = lock(&fixture.0).unwrap();
    drop(next_owner);
}

#[test]
fn full_sync_owner_rejects_wrong_directory_and_replaced_lock() {
    let first = Fixture::new();
    let second = Fixture::new();
    let ownership = lock(&first.0).unwrap();
    ownership.require_directory(&first.0).unwrap();
    assert!(ownership.require_directory(&second.0).is_err());
    // A pathname alone is insufficient: it may now name another lock inode.
    fs::rename(first.0.join("process.lock"), first.0.join("old.lock")).unwrap();
    drop(io::private_file(&first.0.join("process.lock")).unwrap());
    assert!(ownership.require_directory(&first.0).is_err());
    // The original lock is still held, not explicitly unlocked by validation.
    assert!(
        io::private_file(&first.0.join("old.lock"))
            .unwrap()
            .try_lock()
            .is_err()
    );
    drop(ownership);
    io::private_file(&first.0.join("old.lock"))
        .unwrap()
        .try_lock()
        .unwrap();
}

#[test]
fn full_sync_owner_rejects_replaced_directory() {
    let fixture = Fixture::new();
    let ownership = lock(&fixture.0).unwrap();
    let moved = fixture.0.with_extension("moved");
    fs::rename(&fixture.0, &moved).unwrap();
    io::private_dir(&fixture.0).unwrap();
    drop(io::private_file(&fixture.0.join("process.lock")).unwrap());
    assert!(ownership.require_directory(&fixture.0).is_err());
    assert!(ownership.require_directory(&moved).is_err());
    assert!(lock(&moved).is_err());
    drop(ownership);
    drop(lock(&moved).unwrap());
    fs::remove_dir_all(moved).unwrap();
}

#[test]
fn stop_is_visible_before_status_gate_is_released() {
    let flag = Arc::new(AtomicBool::new(false));
    let gate = Arc::new(Mutex::new(()));
    let held = gate.lock().unwrap();
    let stop = WorkerStop {
        flag: Arc::clone(&flag),
        status_gate: Arc::clone(&gate),
    };
    let (done_tx, done_rx) = mpsc::channel();
    let stopper = std::thread::spawn(move || {
        stop.stop();
        done_tx.send(()).unwrap();
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    while !flag.load(Ordering::SeqCst) && Instant::now() < deadline {
        std::thread::yield_now();
    }
    let visible_while_locked = flag.load(Ordering::SeqCst);
    let returned_while_locked = done_rx.try_recv().is_ok();
    // Release even on a failed assertion, so the regression never strands a
    // thread behind the gate it intentionally holds.
    drop(held);
    done_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    stopper.join().unwrap();
    assert!(visible_while_locked, "stop was hidden by the status gate");
    assert!(!returned_while_locked, "stop returned before the status write");
}
