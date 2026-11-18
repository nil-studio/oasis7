use super::*;

pub(super) fn observer_config(pos_config: NodePosConfig, root_dir: PathBuf) -> NodeConfig {
    NodeConfig::new("node-b", "world-signed", NodeRole::Observer)
        .expect("config b")
        .with_tick_interval(Duration::from_millis(10))
        .expect("tick b")
        .with_pos_config(pos_config)
        .expect("pos config b")
        .with_allow_local_proposals(false)
        .with_replication(signed_replication_config(root_dir, 22))
}

fn replication_runtime(config: &NodeConfig) -> ReplicationRuntime {
    // Match NodeRuntime::start's validator defaults when driving the engine directly.
    let validator_signers = config
        .pos_config
        .validator_signer_public_keys
        .values()
        .cloned()
        .collect::<Vec<_>>();
    let effective = config
        .replication
        .as_ref()
        .expect("replication config")
        .clone()
        .with_default_remote_writer_allowlist(validator_signers.iter().cloned())
        .expect("default remote writer allowlist")
        .with_default_fetch_requester_allowlist(validator_signers)
        .expect("default fetch requester allowlist");
    ReplicationRuntime::new(&effective, &config.node_id).expect("replication runtime")
}

fn tick_gossip(
    engine: &mut PosNodeEngine,
    config: &NodeConfig,
    now_ms: i64,
    endpoint: &GossipEndpoint,
    replication: &mut ReplicationRuntime,
    execution_hook: Option<&mut dyn NodeExecutionHook>,
) {
    engine
        .tick(
            &config.node_id,
            &config.world_id,
            now_ms,
            Some(endpoint),
            Some(replication),
            None,
            None,
            Vec::new(),
            execution_hook,
        )
        .expect("signed gossip engine tick");
}

#[test]
fn signed_gossip_observer_first_schedule_follows_sequencer() {
    let dir_a = temp_dir("signed-follower-schedule-a");
    let dir_b = temp_dir("signed-follower-schedule-b");
    let mut pos_config = signed_pos_config_with_signer_seeds(
        vec![
            PosValidator {
                validator_id: "node-a".to_string(),
                stake: 60,
            },
            PosValidator {
                validator_id: "node-b".to_string(),
                stake: 40,
            },
        ],
        &[("node-a", 11), ("node-b", 22)],
    );
    const GENESIS_MS: i64 = 1_000;
    pos_config.slot_clock_genesis_unix_ms = Some(GENESIS_MS);
    let config_a = NodeConfig::new("node-a", "world-signed", NodeRole::Sequencer)
        .expect("config a")
        .with_pos_config(pos_config.clone())
        .expect("pos config a")
        .with_auto_attest_all_validators(true)
        .with_replication(signed_replication_config(dir_a.clone(), 11));
    let config_b = observer_config(pos_config, dir_b.clone());
    let mut engine_a = PosNodeEngine::new(&config_a).expect("engine a");
    let mut engine_b = PosNodeEngine::new(&config_b).expect("engine b");
    let mut replication_a = replication_runtime(&config_a);
    let mut replication_b = replication_runtime(&config_b);

    // Bind the actual endpoints once; keep both sockets owned throughout the test.
    let bind_addr = "127.0.0.1:0".parse().expect("loopback address");
    let endpoint_a =
        GossipEndpoint::bind(&gossip_config(bind_addr, Vec::new())).expect("endpoint a");
    let endpoint_b =
        GossipEndpoint::bind(&gossip_config(bind_addr, Vec::new())).expect("endpoint b");
    endpoint_a
        .remember_peer(endpoint_b.local_addr_for_test().expect("address b"))
        .expect("peer b");
    endpoint_b
        .remember_peer(endpoint_a.local_addr_for_test().expect("address a"))
        .expect("peer a");

    let observer_slot = (0..256)
        .find(|slot| engine_b.expected_proposer(*slot).as_deref() == Some("node-b"))
        .expect("observer proposer slot");
    let sequencer_slot = (observer_slot + 1..observer_slot + 256)
        .find(|slot| engine_a.expected_proposer(*slot).as_deref() == Some("node-a"))
        .expect("subsequent sequencer proposer slot");
    let observer_time = GENESIS_MS + i64::try_from(observer_slot).expect("observer slot fits i64");
    let sequencer_time =
        GENESIS_MS + i64::try_from(sequencer_slot).expect("sequencer slot fits i64");
    let calls = Arc::new(Mutex::new(Vec::new()));
    let mut execution_hook = RecordingExecutionHook::new(calls.clone());

    // Drive B -> A -> B -> A without runtime threads or wall-clock sleeps. With
    // the old fixture B proposes, A votes but awaits B's missing replication,
    // and B commits without publishing a replication original.
    tick_gossip(
        &mut engine_b,
        &config_b,
        observer_time,
        &endpoint_b,
        &mut replication_b,
        None,
    );
    let observer_proposed_first = engine_b.pending.is_some();
    tick_gossip(
        &mut engine_a,
        &config_a,
        sequencer_time,
        &endpoint_a,
        &mut replication_a,
        Some(&mut execution_hook),
    );
    tick_gossip(
        &mut engine_b,
        &config_b,
        sequencer_time,
        &endpoint_b,
        &mut replication_b,
        None,
    );
    tick_gossip(
        &mut engine_a,
        &config_a,
        sequencer_time,
        &endpoint_a,
        &mut replication_a,
        Some(&mut execution_hook),
    );

    let store_a = LocalCasStore::new(dir_a.join("store"));
    let store_b = LocalCasStore::new(dir_b.join("store"));
    let files_a = store_a.list_files().expect("files a");
    let files_b = store_b.list_files().expect("files b");
    assert!(
        !observer_proposed_first,
        "observer originated a proposal; a=(committed {}, network {}, persisted {}, files {}), b=(committed {}, network {}, persisted {}, files {})",
        engine_a.committed_height,
        engine_a.network_committed_height,
        engine_a.replication_persisted_height,
        files_a.len(),
        engine_b.committed_height,
        engine_b.network_committed_height,
        engine_b.replication_persisted_height,
        files_b.len(),
    );
    assert_eq!(engine_a.committed_height, 1);
    assert_eq!(engine_a.replication_persisted_height, 1);
    assert_eq!(engine_b.committed_height, 1);
    let original = replication_a
        .load_commit_message_by_height("world-signed", 1)
        .expect("load sequencer original")
        .expect("sequencer original");
    assert_eq!(original.node_id, "node-a");
    assert!(original.signature_hex.is_some());
    assert_eq!(
        replication_b
            .load_commit_message_by_height("world-signed", 1)
            .expect("load follower original")
            .expect("follower original"),
        original
    );
    let path = "consensus/commits/00000000000000000001.json";
    assert_eq!(
        store_a.read_file(path).expect("sequencer file"),
        original.payload
    );
    assert_eq!(
        store_b.read_file(path).expect("follower file"),
        original.payload
    );
    let calls = calls.lock().expect("execution calls");
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].proposer_id, "node-a");

    let _ = fs::remove_dir_all(&dir_a);
    let _ = fs::remove_dir_all(&dir_b);
}
