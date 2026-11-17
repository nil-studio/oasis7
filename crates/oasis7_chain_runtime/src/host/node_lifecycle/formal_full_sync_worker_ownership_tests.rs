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
