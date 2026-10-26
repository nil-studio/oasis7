//! Real Node/driver recovery across a multi-height V2 history.
//!
//! The TCP signer backend is an in-memory test double. The execution driver
//! itself uses its normal durable ledger and OS writer lock.

use super::*;
use oasis7_runtime::World;

const MIN_REPLAY_HEIGHTS: u64 = 2;
const MAX_HISTORY_PAGE: usize = 64;

fn signer_service(
    binding: &BftSignerBindingV2,
    provider_key: &SigningKey,
    validator_key: &SigningKey,
    shared_snapshot: Arc<Mutex<BftSignerBackendSnapshotV2>>,
) -> Arc<BftSignerServiceV2> {
    Arc::new(
        BftSignerServiceV2::open(
            binding.clone(),
            Box::new(NodeTestSignerKey(provider_key.clone())),
            vec![Box::new(NodeTestSignerKey(validator_key.clone()))],
            Box::new(NodeTestSignerBackend {
                binding_digest: binding.digest().expect("signer binding digest"),
                snapshot: shared_snapshot,
            }),
        )
        .expect("open V2 signer service over its persisted snapshot"),
    )
}

fn signer_binding(
    config: &NodeBftExecutionConfigV2,
    local_key: &SigningKey,
    requester_key: &SigningKey,
) -> BftSignerBindingV2 {
    BftSignerBindingV2 {
        schema_version: 2,
        scope: config.scope.clone(),
        genesis_authority_epoch: config.authority_epoch,
        genesis_validators: config.initial_validators.clone(),
        genesis_state_root: config.genesis_state_root.clone(),
        genesis_manifest_hash: config.genesis_manifest_hash.clone(),
        schedule_policy: config.schedule_policy.clone(),
        local_validator_id: "v0".into(),
        provisioned_signing_keys: vec![BftSignerKeyIdentityV2 {
            validator_id: "v0".into(),
            public_key_hex: hex::encode(local_key.verifying_key().to_bytes()),
        }],
        timeouts: oasis7_consensus::tendermint_v1::BftTimeoutsV1 {
            propose_ms: 200,
            prevote_ms: 200,
            precommit_ms: 200,
            round_delta_ms: 20,
        },
        operator_instance_id: "driver-v2-cold-replay-instance".into(),
        requester_auth_public_key: hex::encode(requester_key.verifying_key().to_bytes()),
    }
}

fn remote_provider(
    address: SocketAddr,
    binding: &BftSignerBindingV2,
    requester_key: SigningKey,
    provider_key: &SigningKey,
) -> Arc<RemoteBftSignerProviderV2> {
    Arc::new(
        RemoteBftSignerProviderV2::new(
            address,
            Duration::from_secs(3),
            binding.clone(),
            requester_key,
            BftSignerTrustV2 {
                schema_version: 2,
                provider_public_key: hex::encode(provider_key.verifying_key().to_bytes()),
                binding_digest: binding.digest().expect("signer binding digest"),
            },
        )
        .expect("construct remote V2 provider"),
    )
}

fn canonical_test_root(prefix: &str) -> std::path::PathBuf {
    let allocated = super::super::temp_dir(prefix);
    let basename = allocated
        .file_name()
        .expect("test root has a basename")
        .to_string_lossy();
    std::env::temp_dir()
        .canonicalize()
        .expect("canonical temporary directory")
        .join(basename.as_ref())
}

fn wait_for_height(
    runtime: &NodeRuntime,
    target: u64,
    signer_server: &mut Option<thread::JoinHandle<Result<(), String>>>,
) -> oasis7_node::NodeBftStatusV2 {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let status = runtime.bft_v2_status().expect("selected V2 runtime status");
        if status.committed_height >= target {
            return status;
        }
        if !status.running {
            let signer_result = if signer_server
                .as_ref()
                .is_some_and(thread::JoinHandle::is_finished)
            {
                signer_server.take().map(|server| server.join())
            } else {
                None
            };
            panic!("Node stopped before height {target}: {status:?}; signer={signer_result:?}");
        }
        assert!(
            Instant::now() < deadline,
            "Node did not continue from replayed history to height {target}: {status:?}"
        );
        thread::sleep(Duration::from_millis(10));
    }
}

fn execution_record_path(driver_root: &std::path::Path, height: u64) -> std::path::PathBuf {
    driver_root
        .join("records/bft-execution-v2")
        .join(format!("height-{height:020}.cbor"))
}

fn node_runtime(
    driver_root: &std::path::Path,
    replication_root: std::path::PathBuf,
    world: World,
    scope: &BftScopeV2,
    binding: BftSignerBindingV2,
    provider: Arc<RemoteBftSignerProviderV2>,
) -> NodeRuntime {
    let driver = super::super::driver_bft::driver_for_world(driver_root, world);
    let node_config = NodeConfig::new("v0", scope.world_id.clone(), NodeRole::Sequencer)
        .expect("create single-validator Node")
        .with_tick_interval(Duration::from_millis(100))
        .expect("configure bounded Node polling interval")
        .with_replication(
            NodeReplicationConfig::new(replication_root)
                .expect("configure durable Node replication path"),
        );
    NodeRuntime::new(node_config)
        .with_replication_network_consensus_enabled(false)
        .with_execution_hook(driver)
        .with_tendermint_v2(binding, provider)
        .expect("select the real driver and remote signer for V2")
}

#[test]
fn real_node_cold_replays_mult_height_driver_and_signer_history_then_continues() {
    let root = canonical_test_root("bft-v2-node-cold-multiheight-replay");
    std::fs::create_dir_all(&root).expect("create isolated cold-restart fixture");

    let keys = vec![SigningKey::from_bytes(&[111; 32])];
    let scope = super::v2_scope();
    let (config, genesis_world): (NodeBftExecutionConfigV2, World) =
        super::v2_genesis_config(&scope, &keys);
    let requester_key = SigningKey::from_bytes(&[112; 32]);
    let provider_key = SigningKey::from_bytes(&[113; 32]);
    let binding = signer_binding(&config, &keys[0], &requester_key);
    let signer_state = BftSignerStateV2::genesis(&binding).expect("signer genesis state");
    let shared_snapshot = Arc::new(Mutex::new(BftSignerBackendSnapshotV2 {
        state: signer_state,
        lease_generation: 1,
        lease_token_digest: String::new(),
        protected_time_floor_ms: 100,
    }));

    let driver_root = root.join("driver");
    let seed_service = signer_service(
        &binding,
        &provider_key,
        &keys[0],
        Arc::clone(&shared_snapshot),
    );
    let (seed_address, seed_server) = super::spawn_node_test_signer(seed_service.clone());
    let mut seed_server = Some(seed_server);
    let seed_provider =
        remote_provider(seed_address, &binding, requester_key.clone(), &provider_key);
    let mut seed_runtime = node_runtime(
        driver_root.as_path(),
        root.join("seed-replication"),
        genesis_world.clone(),
        &scope,
        binding.clone(),
        seed_provider.clone(),
    );
    seed_runtime
        .start()
        .expect("start first real Node and acquire the TCP signer");
    wait_for_height(&seed_runtime, MIN_REPLAY_HEIGHTS, &mut seed_server);
    seed_runtime.stop().expect("stop first Node cleanly");
    let seeded_status = seed_runtime
        .bft_v2_status()
        .expect("read stopped first Node status");
    let seeded_certificates = seed_runtime
        .raw_bft_v2_commit_history_page(0, MAX_HISTORY_PAGE)
        .expect("read the first Node's actual signed history");
    let seeded_height = seeded_status.committed_height;
    assert!(seeded_height >= MIN_REPLAY_HEIGHTS);
    assert_eq!(seeded_certificates.len() as u64, seeded_height);
    assert_eq!(
        seeded_certificates
            .iter()
            .map(|certificate| certificate.proposal.block.height)
            .collect::<Vec<_>>(),
        (1..=seeded_height).collect::<Vec<_>>(),
        "the first real Node must produce one contiguous QC per height"
    );
    drop(seed_runtime);
    drop(seed_provider);
    seed_server
        .take()
        .expect("first signer server join handle")
        .join()
        .expect("join first signer server after Node shutdown")
        .expect("server releases first Node lease cleanly");
    let after_seed = shared_snapshot
        .lock()
        .expect("read signer state after first Node shutdown")
        .clone();
    assert!(after_seed.lease_token_digest.is_empty());
    assert_eq!(after_seed.state.history, seeded_certificates);
    drop(seed_service);

    let before_restart = (1..=seeded_height)
        .map(|height| {
            std::fs::read(execution_record_path(driver_root.as_path(), height))
                .expect("read exact pre-restart execution record")
        })
        .collect::<Vec<_>>();

    // Reopen the signer service over the same backend snapshot to model a
    // provider process restart, then use a new real TCP connection for Node.
    let restarted_service = signer_service(
        &binding,
        &provider_key,
        &keys[0],
        Arc::clone(&shared_snapshot),
    );
    let (node_signer_address, node_signer_server) =
        super::spawn_node_test_signer(restarted_service.clone());
    let mut node_signer_server = Some(node_signer_server);
    let node_provider =
        remote_provider(node_signer_address, &binding, requester_key, &provider_key);
    let mut runtime = node_runtime(
        driver_root.as_path(),
        root.join("node-replication"),
        genesis_world,
        &scope,
        binding.clone(),
        node_provider,
    );
    runtime
        .start()
        .expect("cold startup verifies and replays raw history from genesis");

    let replayed = runtime
        .raw_bft_v2_commit_history_page(0, MAX_HISTORY_PAGE)
        .expect("read actual replayed QC history");
    assert!(replayed.len() >= seeded_height as usize);
    assert_eq!(
        &replayed[..seeded_height as usize],
        seeded_certificates.as_slice(),
        "Node replay must preserve the exact signed QC prefix"
    );

    let continued_height = seeded_height + 1;
    wait_for_height(&runtime, continued_height, &mut node_signer_server);
    runtime.stop().expect("stop continued Node cleanly");
    let continued = runtime
        .bft_v2_status()
        .expect("read stopped continued Node status");
    let final_history = runtime
        .raw_bft_v2_commit_history_page(0, MAX_HISTORY_PAGE)
        .expect("read final stopped history after forward progress");
    assert!(continued.committed_height >= continued_height);
    assert_eq!(final_history.len() as u64, continued.committed_height);
    assert_eq!(
        &final_history[..seeded_height as usize],
        seeded_certificates.as_slice(),
        "new commits must extend, not rewrite or duplicate, the replayed prefix"
    );

    node_signer_server
        .take()
        .expect("restarted signer server join handle")
        .join()
        .expect("join restarted signer server")
        .expect("server releases Node lease cleanly");
    drop(runtime);
    drop(restarted_service);

    for (index, height) in (1..=seeded_height).enumerate() {
        assert_eq!(
            std::fs::read(execution_record_path(driver_root.as_path(), height))
                .expect("read replayed execution record"),
            before_restart[index],
            "replaying a prior QC must not rewrite its already installed World record"
        );
    }
    for height in seeded_height + 1..=continued.committed_height {
        assert!(execution_record_path(driver_root.as_path(), height).is_file());
    }
    let signer_after_restart = shared_snapshot
        .lock()
        .expect("read shared signer backend after restart")
        .clone();
    assert!(signer_after_restart.lease_token_digest.is_empty());
    assert_eq!(signer_after_restart.state.history, final_history);

    let _ = std::fs::remove_dir_all(root);
}
