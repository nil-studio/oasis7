use super::*;
use crate::viewer::runtime_live::ViewerRuntimeLiveServerConfig;
use crate::world_service::client::WorldServiceClientConfig;
use oasis7_client_api::world_service::WorldIdentity;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::Duration;

fn server() -> Arc<Mutex<ViewerRuntimeLiveServer>> {
    Arc::new(Mutex::new(
        ViewerRuntimeLiveServer::new(ViewerRuntimeLiveServerConfig::new(
            oasis7_simulator::WorldScenario::Minimal,
        ))
        .unwrap(),
    ))
}

#[test]
fn cleanup_joins_actual_pump_holding_last_temporary_owner() {
    let shared = server();
    let weak = Arc::downgrade(&shared);
    let diagnostics = shared.lock().unwrap().agent_service_io_diagnostics.clone();
    let (entered, entry) = mpsc::channel();
    let (release, released) = mpsc::channel();
    *diagnostics.iteration_gate.lock().unwrap() = Some(IterationGate {
        entered,
        release: released,
    });
    // No Session or hosted work is installed. The configured service starts
    // the actual pump and executor, then the gate holds its first upgraded Arc.
    shared.lock().unwrap().config.world_service = Some(WorldServiceClientConfig {
        formal_read_trust: None,
        endpoint: "http://127.0.0.1:1".into(),
        trusted_service_public_key: hex::encode([9; 32]),
        expected_world: WorldIdentity {
            world_id: "fixture".into(),
            genesis_digest: "fixture-genesis".into(),
        },
        scope_id: "public".into(),
        read_private_key_hex: hex::encode([7; 32]),
        timeout: Duration::from_secs(2),
        max_response_bytes: 4096,
    });
    ViewerRuntimeLiveServer::start_agent_service_pump(&shared).unwrap();
    entry.recv_timeout(Duration::from_secs(1)).unwrap();
    let (started, start) = mpsc::channel();
    let (completed, completion) = mpsc::channel();
    let cleanup = thread::spawn(move || {
        completed
            .send(ViewerRuntimeLiveServer::test_join_agent_service_workers(
                shared,
                Some(started),
            ))
            .unwrap();
    });
    start.recv_timeout(Duration::from_secs(1)).unwrap();
    assert!(
        weak.upgrade().is_some(),
        "only the pump owns the server now"
    );
    assert_eq!(
        completion.recv_timeout(Duration::from_millis(25)),
        Err(RecvTimeoutError::Timeout)
    );
    release.send(()).unwrap();
    assert_eq!(
        completion.recv_timeout(Duration::from_secs(2)).unwrap(),
        Ok(())
    );
    cleanup.join().unwrap();
    assert!(
        weak.upgrade().is_none(),
        "the next Weak upgrade cannot restart dispatch"
    );
    assert!(diagnostics.workers.lock().unwrap().is_empty());
}

#[test]
fn cleanup_joins_executor_even_after_pump_panics() {
    let shared = server();
    let diagnostics = shared.lock().unwrap().agent_service_io_diagnostics.clone();
    let (entered, entry) = mpsc::channel();
    let (release, released) = mpsc::channel();
    let executor = thread::spawn(move || {
        entered.send(()).unwrap();
        released.recv_timeout(Duration::from_secs(2)).unwrap();
    });
    let pump = thread::spawn(|| panic!("controlled pump failure"));
    diagnostics
        .workers
        .lock()
        .unwrap()
        .push(Workers { pump, executor });
    entry.recv_timeout(Duration::from_secs(1)).unwrap();
    let (started, start) = mpsc::channel();
    let (completed, completion) = mpsc::channel();
    let cleanup = thread::spawn(move || {
        completed
            .send(ViewerRuntimeLiveServer::test_join_agent_service_workers(
                shared,
                Some(started),
            ))
            .unwrap();
    });
    start.recv_timeout(Duration::from_secs(1)).unwrap();
    assert_eq!(
        completion.recv_timeout(Duration::from_millis(25)),
        Err(RecvTimeoutError::Timeout),
        "a failed pump must not detach its still-running executor"
    );
    release.send(()).unwrap();
    assert_eq!(
        completion.recv_timeout(Duration::from_secs(2)).unwrap(),
        Err("canonical service worker cleanup failed")
    );
    cleanup.join().unwrap();
    assert!(diagnostics.workers.lock().unwrap().is_empty());
}
