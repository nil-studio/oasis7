//! Real server/node process restart using the same compiled artifact and durable CAS.
use super::*;
use std::process::{Child, Command, Stdio};

struct ServerProcess(Child);
impl Drop for ServerProcess {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn start_server(root: &Path, endpoint: &str) -> ServerProcess {
    let log = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(root.join("server-process.log"))
        .unwrap();
    let child=Command::new(std::env::current_exe().unwrap()).args(["--ignored","--exact","execution_bridge_real_tests::real_execution_bridge::tests::qa_conformance::process_restart::server_process_entry","--nocapture"])
        .env("PRE2_SERVER_ROOT",root).env("PRE2_SERVER_ENDPOINT",endpoint).stdout(Stdio::from(log.try_clone().unwrap())).stderr(Stdio::from(log)).spawn().unwrap();
    println!(
        "server_process_pid={} server_process_log={}",
        child.id(),
        root.join("server-process.log").display()
    );
    ServerProcess(child)
}
fn ready(client: &RemoteWorldServiceClient, server: &mut ServerProcess) {
    let deadline = Instant::now() + Duration::from_secs(15);
    while client.describe().is_err() {
        assert!(
            server.0.try_wait().unwrap().is_none(),
            "server process exited before readiness"
        );
        assert!(
            Instant::now() < deadline,
            "server process readiness timed out"
        );
        thread::sleep(Duration::from_millis(20));
    }
}
#[test]
fn real_tcp_full_node_server_process_restart_recovers_exact_receipt() {
    full_node_restart(false);
}
#[test]
fn real_tcp_full_node_server_restart_restores_stale_same_height_cache() {
    full_node_restart(true);
}
fn copy_cache_tree(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_cache_tree(&entry.path(), &target);
        } else {
            assert!(entry.file_type().unwrap().is_file());
            fs::copy(entry.path(), target).unwrap();
        }
    }
}
fn full_node_restart(stale_cache: bool) {
    use oasis7_viewer_runtime::viewer::{
        CollectDataCommand, CollectDataRequest, sign_collect_data_auth_proof,
    };
    let mut fixture = Fixture::with_controlled_commits(true);
    let root = fixture.root.clone();
    let mut config = fixture.client.config().clone();
    let owner = fixture.owner.clone();
    fixture.preserve_root = true;
    let _scenario_permit = fixture.scenario_permit.clone();
    drop(fixture);
    let old_cache = root.join("older-distfs-cache-generation");
    if stale_cache {
        copy_cache_tree(&root.join("world/.distfs-state"), &old_cache);
    }
    let reserve = TcpListener::bind("127.0.0.1:0").unwrap();
    config.endpoint = format!("http://{}", reserve.local_addr().unwrap());
    drop(reserve);
    let client = RemoteWorldServiceClient::new(config).unwrap();
    let mut server = start_server(&root, &client.config().endpoint);
    ready(&client, &mut server);
    let public = sign_read_request("owner", (), &owner)
        .unwrap()
        .subject_public_key;
    let mut command = CollectDataCommand::Submit {
        request: CollectDataRequest {
            electricity_cost: 7,
            data_amount: 11,
            player_id: "owner-a".into(),
            public_key: Some(public.clone()),
            auth: None,
        },
    };
    let proof = sign_collect_data_auth_proof(&command, 12, &public, &owner).unwrap();
    let CollectDataCommand::Submit { request } = &mut command else {
        unreachable!()
    };
    request.auth = Some(proof);
    let payload = WorldServicePayloadV1::GameplayJson(serde_json::to_vec(&command).unwrap());
    let original = SubmitIntentRequest {
        contract_version: 1,
        correlation: derive_correlation(client.config().expected_world.clone(), &payload).unwrap(),
        deadline_unix_ms: None,
        signed_payload: payload,
    };
    client.submit(original.clone()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(15);
    let committed = loop {
        let response = client
            .lookup(
                LookupIntentRequest {
                    contract_version: 1,
                    key: original.correlation.key.clone(),
                },
                original.signed_payload.clone(),
            )
            .unwrap();
        if matches!(response.outcome, IntentOutcome::Committed { .. }) {
            break response;
        }
        assert!(
            Instant::now() < deadline,
            "live server did not commit signed gameplay"
        );
        thread::sleep(Duration::from_millis(20));
    };
    let IntentOutcome::Committed {
        commit: original_commit,
        ..
    } = &committed.outcome
    else {
        unreachable!()
    };
    let old_view = client
        .read_view(ReadWorldViewRequest {
            observation_nonce: None,
            formal_anchor: None,
            contract_version: 1,
            world: client.config().expected_world.clone(),
            scope_id: client.config().scope_id.clone(),
            min_commit: Some(original_commit.as_ref().clone()),
            fixed_commit: None,
            deadline_unix_ms: None,
        })
        .unwrap();
    drop(server);
    if stale_cache {
        // Reinstall only the older mutable cache, preserving real receipt/CAS/index/JSON.
        fs::remove_dir_all(root.join("world/.distfs-state")).unwrap();
        copy_cache_tree(&old_cache, &root.join("world/.distfs-state"));
    }
    assert!(
        client.describe().is_err(),
        "killed process remained available"
    );
    let reserve = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut switched_config = client.config().clone();
    switched_config.endpoint = format!("http://{}", reserve.local_addr().unwrap());
    drop(reserve);
    assert_ne!(switched_config.endpoint, client.config().endpoint);
    let switched = RemoteWorldServiceClient::new(switched_config).unwrap();
    assert_eq!(
        switched.config().expected_world,
        client.config().expected_world
    );
    assert_eq!(
        switched.config().trusted_service_public_key,
        client.config().trusted_service_public_key
    );
    assert_eq!(switched.config().scope_id, client.config().scope_id);
    let mut restarted = start_server(&root, &switched.config().endpoint);
    ready(&switched, &mut restarted);
    let recovered = switched
        .lookup(
            LookupIntentRequest {
                contract_version: 1,
                key: original.correlation.key.clone(),
            },
            original.signed_payload.clone(),
        )
        .unwrap();
    assert_eq!(
        serde_json::to_value(&recovered).unwrap(),
        serde_json::to_value(&committed).unwrap(),
        "restarted process changed original receipt"
    );
    let IntentOutcome::Committed { commit, .. } = recovered.outcome else {
        unreachable!()
    };
    let new_view = switched
        .read_view(ReadWorldViewRequest {
            observation_nonce: None,
            formal_anchor: None,
            contract_version: 1,
            world: switched.config().expected_world.clone(),
            scope_id: switched.config().scope_id.clone(),
            min_commit: Some(*commit),
            fixed_commit: None,
            deadline_unix_ms: None,
        })
        .unwrap();
    println!(
        "PRE2_SAME_ARTIFACT_ENDPOINT_SWITCH_PASSED old_endpoint={} new_endpoint={} world={} original_key_digest={} old_commit={} new_commit={}",
        client.config().endpoint,
        switched.config().endpoint,
        serde_json::to_string(&client.config().expected_world).unwrap(),
        correlation::key_digest(&original.correlation.key).unwrap(),
        serde_json::to_string(&old_view.version().commit).unwrap(),
        serde_json::to_string(&new_view.version().commit).unwrap()
    );
    println!(
        "PRE2_FULL_NODE_SERVER_PROCESS_RESTART_PASSED application_artifact_blake3={}",
        blake3::hash(&fs::read(std::env::current_exe().unwrap()).unwrap())
    );
    drop(restarted);
    if stale_cache {
        assert!(
            fs::read_to_string(root.join("server-process.log"))
                .unwrap()
                .contains(
                    "pre2_verified_same_height_service_cache_restored height=4 journal_len=15"
                )
        );
        println!("PRE2_STALE_INDEXED_CACHE_SAME_HEIGHT_CAS_RESTORE_PASSED");
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
#[ignore = "same-artifact server process entrypoint; parent supplies durable fixture roots"]
fn server_process_entry() {
    let root = std::path::PathBuf::from(std::env::var("PRE2_SERVER_ROOT").unwrap());
    let endpoint = std::env::var("PRE2_SERVER_ENDPOINT").unwrap();
    let _lock =
        crate::world_writer_lock::acquire_live_world_writer_lock(&root.join("world")).unwrap();
    let bootstrap = super::super::super::local_bootstrap::derive_service_execution_bootstrap(
        &root.join("world"),
        &root.join("records"),
        &root.join("store"),
        "w1",
    )
    .unwrap();
    let driver = NodeRuntimeExecutionDriver::new_with_local_bootstrap(
        root.join("state.json"),
        root.join("world"),
        root.join("records"),
        root.join("store"),
        &oasis7_proto::storage_profile::StorageProfileConfig::default(),
        bootstrap.clone(),
    )
    .unwrap();
    let config = NodeConfig::new("node-a", "w1", NodeRole::Sequencer)
        .unwrap()
        .with_tick_interval(Duration::from_millis(500))
        .unwrap();
    let mut node = NodeRuntime::new(config)
        .with_execution_hook(driver)
        .with_local_execution_bootstrap(bootstrap);
    node.start().unwrap();
    let node = Arc::new(Mutex::new(node));
    let private_key_hex = hex::encode([9u8; 32]);
    let signer = crate::feedback_submit_api::FeedbackSubmitSigner {
        public_key_hex: sign_read_request("service", (), &private_key_hex)
            .unwrap()
            .subject_public_key,
        private_key_hex,
    };
    let listener = TcpListener::bind(endpoint.strip_prefix("http://").unwrap()).unwrap();
    loop {
        let (mut stream, _) = listener.accept().unwrap();
        http_fixture::configure_accepted_stream(&stream);
        let bytes = read_request(&mut stream);
        if bytes.is_empty() {
            continue;
        }
        let first = std::str::from_utf8(&bytes).unwrap().lines().next().unwrap();
        let mut parts = first.split_whitespace();
        let method = parts.next().unwrap();
        let path = parts.next().unwrap();
        let _ = crate::world_service_api::maybe_handle(
            &mut stream,
            &bytes,
            &node,
            method,
            path,
            "w1",
            &root.join("world"),
            &root.join("records"),
            &root.join("store"),
            &signer,
        );
    }
}

#[test]
fn real_tcp_service_bootstrap_rejects_tampered_record_root() {
    let fixture = Fixture::with_controlled_commits(true);
    let derive = || {
        super::super::super::local_bootstrap::derive_service_execution_bootstrap(
            &fixture.root.join("world"),
            &fixture.root.join("records"),
            &fixture.root.join("store"),
            "w1",
        )
    };
    let bootstrap = derive().unwrap();
    let record_path = execution_bridge_record_path(&fixture.root.join("records"), bootstrap.height);
    let mut record = load_execution_bridge_record(&record_path).unwrap();
    record.execution_state_root = "0".repeat(64);
    fs::write(record_path, serde_json::to_vec(&record).unwrap()).unwrap();
    assert!(
        derive().is_err(),
        "tampered durable record cannot become service bootstrap"
    );
    assert!(
        fixture.client.describe().is_err(),
        "tampered generation cannot publish a view"
    );
}
#[test]
fn real_tcp_service_bootstrap_rejects_missing_cas_material() {
    let fixture = Fixture::with_controlled_commits(true);
    let derive = || {
        super::super::super::local_bootstrap::derive_service_execution_bootstrap(
            &fixture.root.join("world"),
            &fixture.root.join("records"),
            &fixture.root.join("store"),
            "w1",
        )
    };
    derive().unwrap();
    fs::rename(
        fixture.root.join("store"),
        fixture.root.join("unavailable-store"),
    )
    .unwrap();
    assert!(
        derive().is_err(),
        "missing CAS must not fall back to mutable world cache"
    );
    assert!(
        fixture.client.describe().is_err(),
        "missing CAS must fail committed view closed"
    );
}

// Startup must authenticate the actual admitted setup predecessor, not invent record h2.
#[test]
fn real_tcp_restart_rejects_tampered_setup_boundary() {
    reject_invalid_setup_boundary(true);
}
#[test]
fn real_tcp_restart_rejects_missing_setup_boundary() {
    reject_invalid_setup_boundary(false);
}
fn reject_invalid_setup_boundary(tamper: bool) {
    let mut fixture = Fixture::with_controlled_commits(true);
    let root = fixture.root.clone();
    let baseline = super::super::super::local_bootstrap::derive_service_execution_bootstrap(
        &root.join("world"),
        &root.join("records"),
        &root.join("store"),
        "w1",
    )
    .unwrap();
    fixture.preserve_root = true;
    let _scenario_permit = fixture.scenario_permit.clone();
    drop(fixture);
    let path = root.join("records/local-execution-bootstrap.json");
    assert!(
        path.is_file(),
        "actual admitted setup boundary missing before tamper"
    );
    if tamper {
        let mut boundary: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        boundary["execution_state_root"] = serde_json::json!("0".repeat(64));
        fs::write(&path, serde_json::to_vec(&boundary).unwrap()).unwrap();
    } else {
        fs::rename(&path, path.with_extension("hidden")).unwrap();
    }
    let result = NodeRuntimeExecutionDriver::new_with_local_bootstrap(
        root.join("state.json"),
        root.join("world"),
        root.join("records"),
        root.join("store"),
        &oasis7_proto::storage_profile::StorageProfileConfig::default(),
        baseline,
    );
    assert!(
        result.is_err(),
        "invalid setup predecessor admitted during restart"
    );
    println!(
        "setup_boundary_restart_rejected tampered={tamper} error={}",
        result.err().unwrap()
    );
    fs::remove_dir_all(root).unwrap();
}
