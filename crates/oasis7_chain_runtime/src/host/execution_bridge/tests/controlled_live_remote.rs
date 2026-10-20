use super::*;
use crate::controlled_live_config::GuardedRemoteEndpoints;
use oasis7_distfs::controlled_authority::replicated_protocol::transport::EndpointServer;
use std::sync::{Arc, atomic::AtomicBool};

struct Service {
    address: std::net::SocketAddr,
    stop: Arc<AtomicBool>,
    worker: Option<std::thread::JoinHandle<()>>,
}

// Freezing and committing require multiple sequential durable RPCs, each with
// a 5s transport timeout. The old 10s single-process fixture deadline does not
// cover that protocol budget. Keep the aggregate wait finite and diagnostic.
fn wait_for_durable_protocol(mut predicate: impl FnMut() -> bool) {
    let started = std::time::Instant::now();
    while started.elapsed() < Duration::from_secs(60) {
        if predicate() {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        predicate(),
        "durable protocol did not finish within its bounded RPC budget"
    );
}

#[test]
#[cfg(all(unix, feature = "wasmtime"))]
fn frozen_inputs_replay_after_interruption_without_reselecting_original_actions() {
    assert_frozen_replay("before_execution");
}
#[test]
#[cfg(all(unix, feature = "wasmtime"))]
fn executed_but_unsaved_candidate_replays_identical_closed_record() {
    assert_frozen_replay("before_candidate_save");
}
#[cfg(all(unix, feature = "wasmtime"))]
fn assert_frozen_replay(boundary: &str) {
    let fixture = Fixture::new();
    let mut config: GuardedConfiguration =
        serde_json::from_slice(&fs::read(&fixture.config).unwrap()).unwrap();
    config.schema_version = 2;
    fs::write(&fixture.config, serde_json::to_vec(&config).unwrap()).unwrap();
    let request = fixture.request();
    let helper = format!(
        "{}::frozen_crash_helper",
        module_path!().split_once("::").unwrap().1
    );
    let exit = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", &helper, "--ignored", "--nocapture"])
        .env("O7_GUARDED_FROZEN_CONFIG", &fixture.config)
        .env("O7_GUARDED_FROZEN_BOUNDARY", boundary)
        .status()
        .unwrap();
    use std::os::unix::process::ExitStatusExt;
    assert_eq!(exit.signal(), Some(libc::SIGKILL));
    let original: PendingTransaction =
        capture::decode_generic(&fs::read(pending_path(&fixture.records)).unwrap()).unwrap();
    assert!(!history::evidence_path(&fixture.records, 2).exists());
    // Retain an interrupted candidate directory as diagnostic data. Replay
    // executes into a new private directory from the original frozen world.
    let interrupted_path = fixture.records.join("controlled-candidate-42");
    if !interrupted_path.exists() {
        fs::create_dir(&interrupted_path).unwrap();
    }
    fs::write(interrupted_path.join("partial"), b"unfinished execution").unwrap();
    let mut driver = fixture.driver();
    assert_eq!(
        driver.pending_local_continuation().unwrap().unwrap(),
        original.continuation
    );
    let mut restarted = fixture.node(driver);
    restarted.start().unwrap();
    let mut last_error = None;
    wait_for_durable_protocol(|| {
        let snapshot = restarted.snapshot();
        if snapshot.last_error != last_error {
            eprintln!("frozen replay diagnostic: {:?}", snapshot.last_error);
            last_error = snapshot.last_error;
        }
        snapshot.consensus.committed_height == 42
    });
    restarted.stop().unwrap();
    drop(restarted);
    assert_eq!(
        fs::read(interrupted_path.join("partial")).unwrap(),
        b"unfinished execution"
    );
    let completed: PendingTransaction =
        capture::decode_generic(&fs::read(pending_path(&fixture.records)).unwrap()).unwrap();
    assert_eq!(completed.continuation, original.continuation);
    assert_eq!(
        completed.frozen_certificate_ref,
        original.frozen_certificate_ref
    );
    let evidence: DurabilityEvidence =
        serde_json::from_slice(&fs::read(history::evidence_path(&fixture.records, 2)).unwrap())
            .unwrap();
    let certificate = evidence.frozen_certificate.as_ref().unwrap();
    assert_eq!(
        certificate.input.body.continuation_bytes,
        to_cbor(&original.continuation).unwrap()
    );
    assert_eq!(
        certificate.input.body.action_bytes,
        to_cbor(&original.continuation.context.committed_actions).unwrap()
    );
    assert_eq!(evidence.proposal.body.schema_version, 2);
    if boundary == "before_candidate_save" {
        let original_digest =
            fs::read_to_string(fixture.records.join("test-interrupted-candidate-digest")).unwrap();
        assert_eq!(
            oasis7_runtime::blake3_hex(&to_cbor(&evidence.proposal.body.record).unwrap()),
            original_digest,
            "replay must preserve the complete deterministic record, not only its state root"
        );
    }
    let authority = GuardedAuthority::load(&fixture.config).unwrap();
    let published = history::load_head(&authority, &fixture.records, None).unwrap();
    let digest = correlation::key_digest(&request.correlation.key).unwrap();
    assert!(
        published
            .snapshot
            .capability_revocation_state
            .world_service_results
            .contains_key(&digest)
    );
}
impl Service {
    fn start(endpoint: FileEndpoint) -> Self {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = stop.clone();
        let worker = std::thread::spawn(move || {
            let mut service = EndpointServer::new(endpoint, Duration::from_secs(5)).unwrap();
            while !worker_stop.load(Ordering::Acquire) {
                if service.serve_one(&listener).is_err() {
                    std::thread::sleep(Duration::from_millis(2));
                }
            }
        });
        Self {
            address,
            stop,
            worker: Some(worker),
        }
    }
}
impl Drop for Service {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        self.worker.take().unwrap().join().unwrap();
    }
}

#[test]
#[cfg(all(unix, feature = "wasmtime"))]
fn remote_guarded_execution_does_not_read_endpoint_keys_and_reopens_original_result() {
    let fixture = Fixture::new();
    let mut config: GuardedConfiguration =
        serde_json::from_slice(&fs::read(&fixture.config).unwrap()).unwrap();
    let open = |path: &Path, role, key_file: &Path| {
        FileEndpoint::open(
            path,
            fixture.trust.clone(),
            role,
            crate::controlled_live_config::read_existing_signing_key(key_file).unwrap(),
            &fixture.initial,
        )
        .unwrap()
    };
    let primary = Service::start(open(
        &config.primary_directory,
        EndpointRole::Primary,
        &config.primary_key_file,
    ));
    let replica = Service::start(open(
        &config.replica_directory,
        EndpointRole::Replica,
        &config.replica_key_file,
    ));
    fs::remove_file(&config.primary_key_file).unwrap();
    fs::remove_file(&config.replica_key_file).unwrap();
    config.schema_version = 2;
    config.remote_endpoints = Some(GuardedRemoteEndpoints {
        primary: primary.address,
        replica: replica.address,
        timeout_ms: 5000,
    });
    config.primary_directory = PathBuf::new();
    config.replica_directory = PathBuf::new();
    config.primary_key_file = PathBuf::new();
    config.replica_key_file = PathBuf::new();
    fs::write(&fixture.config, serde_json::to_vec(&config).unwrap()).unwrap();
    let original_binding = GuardedAuthority::load(&fixture.config)
        .unwrap()
        .configuration_digest;
    let request = fixture.request();
    let mut node = fixture.node(fixture.driver());
    node.submit_consensus_action_payload(
        42,
        correlation::encode_consensus_intent(&request).unwrap(),
    )
    .unwrap();
    node.start().unwrap();
    let mut last_error = None;
    wait_for_durable_protocol(|| {
        let snapshot = node.snapshot();
        if snapshot.last_error != last_error {
            eprintln!("remote frozen diagnostic: {:?}", snapshot.last_error);
            last_error = snapshot.last_error;
        }
        snapshot.consensus.committed_height == 42
    });
    node.stop().unwrap();
    drop(node);
    let original_bytes = fs::read(history::evidence_path(&fixture.records, 2)).unwrap();
    let original: DurabilityEvidence = serde_json::from_slice(&original_bytes).unwrap();
    let authority = GuardedAuthority::load(&fixture.config).unwrap();
    let published = history::load_head(&authority, &fixture.records, None).unwrap();
    assert_eq!(
        published.head,
        verify_evidence(&original, &fixture.trust, &fixture.initial).unwrap()
    );
    // The physical key location and ingress port do not alter execution identity.
    let moved_writer = fixture.root.join("moved-writer.key");
    fs::rename(&config.writer_key_file, &moved_writer).unwrap();
    config.writer_key_file = moved_writer;
    config.status_bind = "127.0.0.1:19100".parse().unwrap();
    fs::write(&fixture.config, serde_json::to_vec(&config).unwrap()).unwrap();
    assert_eq!(
        GuardedAuthority::load(&fixture.config)
            .unwrap()
            .configuration_digest,
        original_binding
    );
    let mut resumed = fixture.driver();
    let continuation = resumed.pending_local_continuation().unwrap().unwrap();
    let result = resumed
        .on_commit_outcome_with_expected(continuation.context, None, None)
        .unwrap();
    assert!(matches!(result, NodeExecutionCommitOutcome::Applied(_)));
    assert_eq!(
        fs::read(history::evidence_path(&fixture.records, 2)).unwrap(),
        original_bytes
    );
    drop(resumed);
    // Actual execution scheduling remains bound, unlike deployment locations.
    config.node_tick_ms += 1;
    fs::write(&fixture.config, serde_json::to_vec(&config).unwrap()).unwrap();
    assert_ne!(
        GuardedAuthority::load(&fixture.config)
            .unwrap()
            .configuration_digest,
        original_binding
    );
    assert!(
        GuardedExecutionDriver::new(
            GuardedAuthority::load(&fixture.config).unwrap(),
            fixture.records.clone()
        )
        .is_err()
    );
    drop((primary, replica));
}

#[test]
#[cfg(all(unix, feature = "wasmtime"))]
fn remote_guarded_configuration_rejects_ignored_local_paths_and_unsafe_transport() {
    let fixture = Fixture::new();
    let mut value: serde_json::Value =
        serde_json::from_slice(&fs::read(&fixture.config).unwrap()).unwrap();
    value["schema_version"] = 2.into();
    value["remote_endpoints"] =
        serde_json::json!({"primary":"127.0.0.1:1", "replica":"127.0.0.1:2", "timeout_ms":1000});
    fs::write(&fixture.config, serde_json::to_vec(&value).unwrap()).unwrap();
    assert!(GuardedAuthority::load(&fixture.config).is_err());
    for field in [
        "primary_directory",
        "replica_directory",
        "primary_key_file",
        "replica_key_file",
    ] {
        value.as_object_mut().unwrap().remove(field);
    }
    value["remote_endpoints"]["replica"] = "192.0.2.1:2".into();
    fs::write(&fixture.config, serde_json::to_vec(&value).unwrap()).unwrap();
    assert!(GuardedAuthority::load(&fixture.config).is_err());
    value["remote_endpoints"]["replica"] = "127.0.0.1:2".into();
    value["writer_key_file"] = "".into();
    fs::write(&fixture.config, serde_json::to_vec(&value).unwrap()).unwrap();
    assert!(GuardedAuthority::load(&fixture.config).is_err());
}

#[test]
#[ignore = "isolated actual NodeRuntime InputFrozen SIGKILL helper"]
#[cfg(all(unix, feature = "wasmtime"))]
fn frozen_crash_helper() {
    let config = PathBuf::from(std::env::var_os("O7_GUARDED_FROZEN_CONFIG").unwrap());
    let authority = GuardedAuthority::load(&config).unwrap();
    let fixture = Fixture {
        root: config.parent().unwrap().to_path_buf(),
        records: authority.config.records_directory.clone(),
        replica: authority.config.replica_directory.clone(),
        trust: authority.policy.trust.clone(),
        initial: authority.anchor.activation().qualified_head().clone(),
        config,
    };
    let mut driver = fixture.driver();
    match std::env::var("O7_GUARDED_FROZEN_BOUNDARY")
        .unwrap()
        .as_str()
    {
        "before_execution" => driver.kill_before_execution = true,
        "before_candidate_save" => driver.kill_before_candidate_save = true,
        _ => panic!("invalid frozen crash boundary"),
    }
    let request = fixture.request();
    let mut node = fixture.node(driver);
    node.submit_consensus_action_payload(
        42,
        correlation::encode_consensus_intent(&request).unwrap(),
    )
    .unwrap();
    node.start().unwrap();
    std::thread::sleep(Duration::from_secs(8));
    panic!(
        "actual runtime did not reach InputFrozen crash boundary: {:?}",
        node.snapshot().last_error
    );
}
