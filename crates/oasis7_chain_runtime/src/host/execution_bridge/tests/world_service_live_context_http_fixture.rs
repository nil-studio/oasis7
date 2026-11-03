//! Explicit two-process Native/World test bridge; ready data is not a deployed grant.
use super::*;
use oasis7_client_api::world_service::{entry_executor::*, task_context::*};
use std::{
    io::Write,
    net::TcpListener,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
    time::{Duration, Instant},
};

fn request(
    world: &WorldIdentity,
    fence: &GameTaskFenceV1,
    get: &SignedReadRequest<TaskContextGetV1>,
    nonce: &str,
) -> EntryExecutorControlRequestV1 {
    let access = sign_read_request(
        VIEW_JSON_PATH,
        ReadWorldViewRequest {
            contract_version: 1,
            observation_nonce: Some(nonce.into()),
            formal_anchor: None,
            world: world.clone(),
            scope_id: format!("agent:{}", fence.agent_id),
            min_commit: None,
            fixed_commit: None,
            deadline_unix_ms: None,
        },
        &hex::encode([73; 32]),
    )
    .unwrap();
    EntryExecutorControlRequestV1 {
        access: EntryExecutorAccessV1 {
            read: access,
            fence: fence.clone(),
        },
        operation: EntryExecutorOperationV1::ReadLiveContext(TaskContextLiveReadV1 {
            original: get.clone(),
            observation_nonce: nonce.into(),
        }),
    }
}
struct StopOnDrop<'a>(&'a AtomicBool);
impl Drop for StopOnDrop<'_> {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}
fn write_new(path: &std::path::Path, value: &impl serde::Serialize) {
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .unwrap();
    serde_json::to_writer(&mut file, value).unwrap();
    file.write_all(b"\n").unwrap();
    file.sync_all().unwrap();
}
#[test]
#[ignore = "explicit loopback Native-to-World live context fixture"]
fn serve_live_context_world_http_fixture() {
    let path = |name| {
        std::path::PathBuf::from(std::env::var_os(name).expect("explicit isolated fixture path"))
    };
    let ready = path("OASIS7_CUSTODY_WORLD_HTTP_READY");
    let stop_file = path("OASIS7_CUSTODY_WORLD_HTTP_STOP");
    let advance = path("OASIS7_LIVE_CONTEXT_ADVANCE");
    let next = path("OASIS7_LIVE_CONTEXT_NEXT");
    let stats_request = path("OASIS7_LIVE_CONTEXT_STATS_REQUEST");
    let stats = path("OASIS7_LIVE_CONTEXT_STATS");
    let mut f = active_context_read_fixture_with_setup(
        true,
        super::super::participant_http::install_live_context_fixture_identity,
    );
    let original_get = serde_json::to_vec(&f.get).unwrap();
    let read_world = f.world.clone();
    let read_fence = f.fence.clone();
    let read_get = f.get.clone();
    let first = request(&read_world, &read_fence, &read_get, &"a1".repeat(32));
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let stop = AtomicBool::new(false);
    let counts = Arc::new([AtomicUsize::new(0), AtomicUsize::new(0)]);
    let root = f.dir.canonicalize().unwrap();
    let runtime = &f.runtime;
    let signer = &f.signer;
    let store = &f.store;
    std::thread::scope(|scope| {
        let observed = counts.clone();
        let server_stop = &stop;
        let server_root = &root;
        let server = scope.spawn(move || {
            super::super::participant_http::serve_live_context_fixture(
                listener,
                runtime,
                server_root,
                signer,
                store,
                server_stop,
                observed,
            )
        });
        let _stop_guard = StopOnDrop(&stop);
        write_new(
            &ready,
            &serde_json::json!({
                "label":"real-native-authority-context-public-test-originals-not-deployment",
                "endpoint":endpoint, "world":{"world_id":read_world.world_id,"genesis_digest":read_world.genesis_digest,
                    "service_public_key":signer.public_key_hex},
                "identity_namespace":{"service_id":"model-contract","environment_id":"test","instance_epoch":"4"},
                "fixture_issuer_public_key":sign_read_request("participant-issuer-selfcheck", (), &hex::encode([95;32])).unwrap().subject_public_key,
                "authority_evidence":"fixture-preconfigured-world-policy-not-native-signup-or-custody-source",
                "live_context_read":true,"request":first,
            }),
        );
        let deadline = Instant::now() + Duration::from_secs(180);
        while !stop_file.exists() && Instant::now() < deadline {
            if advance.exists() && !next.exists() {
                let height = f.driver.state.last_applied_committed_height + 1;
                f.driver
                    .on_commit(NodeExecutionCommitContext {
                        world_id: "w1".into(),
                        node_id: "node-a".into(),
                        proposer_id: "node-a".into(),
                        height,
                        slot: height,
                        epoch: 0,
                        node_block_hash: format!("live-context-advance-{height}"),
                        action_root: compute_consensus_action_root(&[]).unwrap(),
                        committed_actions: vec![],
                        committed_at_unix_ms: height as i64 * 1000,
                    })
                    .unwrap();
                assert!(
                    serde_json::to_vec(&f.get).unwrap() == original_get,
                    "original Get must remain byte-identical"
                );
                write_new(
                    &next,
                    &request(&read_world, &read_fence, &read_get, &"b2".repeat(32)),
                );
            }
            if stats_request.exists() && !stats.exists() {
                write_new(
                    &stats,
                    &serde_json::json!({"view":counts[0].load(Ordering::SeqCst),"live_read":counts[1].load(Ordering::SeqCst)}),
                );
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        stop.store(true, Ordering::SeqCst);
        server.join().unwrap();
    });
    std::fs::remove_dir_all(f.dir).unwrap();
}
