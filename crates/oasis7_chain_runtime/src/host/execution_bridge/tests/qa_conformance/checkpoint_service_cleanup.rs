//! Close the child only after its canonical service response has completed.
use super::*;
use oasis7_viewer_runtime::viewer::ViewerRuntimeLiveServer;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError};

const ARMED: &str = "checkpoint-submit-response-armed";
const HELD: &str = "checkpoint-submit-response-held";
const RELEASE: &str = "checkpoint-submit-response-release";
const FINISHED: &str = "checkpoint-submit-response-finished";

pub(super) fn arm(root: &Path, enabled: bool) {
    if enabled {
        fs::write(root.join(ARMED), b"1").unwrap();
    }
}

pub(super) fn hold_response(gate: &http_fixture::WorldGate, path: &str) -> Option<PathBuf> {
    if path != SUBMIT_PATH {
        return None;
    }
    let root = gate.root.lock().unwrap().clone()?;
    if !root.join(ARMED).exists() {
        return None;
    }
    match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(root.join(HELD))
    {
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => return None,
        Err(_) => panic!("checkpoint response gate setup failed"),
    }
    // The request has already been fully read and recorded. No real response
    // byte is written until Session::close and service cleanup have started.
    assert!(wait_for(&root.join(RELEASE), Duration::from_secs(1)));
    Some(root)
}

pub(super) fn response_finished(root: Option<PathBuf>, result: &Result<bool, String>) {
    if let Some(root) = root
        && matches!(result, Ok(true))
    {
        fs::write(root.join(FINISHED), b"1").unwrap();
    }
}

fn wait_for(path: &Path, budget: Duration) -> bool {
    let deadline = Instant::now() + budget;
    while !path.exists() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(1));
    }
    path.exists()
}

pub(super) fn wait_until_held(root: &Path) {
    if root.join(ARMED).exists() {
        assert!(wait_for(&root.join(HELD), Duration::from_secs(1)));
    }
}

pub(super) fn join(
    shared: Arc<Mutex<ViewerRuntimeLiveServer>>,
    root: &Path,
) -> Result<(), &'static str> {
    let cleanup = cleanup_deadline::CleanupDeadline::start("checkpoint_service_workers");
    if !root.join(ARMED).exists() {
        let result = ViewerRuntimeLiveServer::test_join_agent_service_workers(shared, None);
        cleanup.complete();
        return result;
    }
    let (started, start) = mpsc::channel();
    let (completed, completion) = mpsc::channel();
    let worker = thread::spawn(move || {
        let result =
            ViewerRuntimeLiveServer::test_join_agent_service_workers(shared, Some(started));
        completed.send(result).unwrap();
    });
    start.recv_timeout(Duration::from_secs(1)).unwrap();
    assert!(wait_for(&root.join(HELD), Duration::from_millis(100)));
    // This bounded negative observation follows the actual ownership transfer,
    // not a guessed sleep before cleanup. The live HTTP response remains held.
    assert_eq!(
        completion.recv_timeout(Duration::from_millis(25)),
        Err(RecvTimeoutError::Timeout),
        "cleanup returned before the held canonical Submit response"
    );
    // Release does not depend on pump completion: it may be waiting for a
    // publication behind this very transport on the same capacity-one lane.
    fs::write(root.join(RELEASE), b"1").unwrap();
    let result = completion.recv_timeout(Duration::from_secs(2)).unwrap();
    let joined = worker.join();
    let response_finished = wait_for(&root.join(FINISHED), Duration::from_millis(100));
    cleanup.complete();
    assert!(joined.is_ok(), "service cleanup controller must join");
    assert!(response_finished, "original dispatcher response must succeed");
    if result.is_ok() {
        println!("PRE2_CHECKPOINT_SERVICE_RESPONSE_JOIN_PASSED");
    }
    result
}
