#[test]
fn fresh_observer_package_probe_does_not_reenter_height_one_after_300s_without_checkpoint_receipt()
{
    let _nonce_lock = lock_checkpoint_probe_nonce();
    let _probe_nonce = CheckpointProbeNonceGuard::install();
    let world_id = "world-live-package-probe-300s-height-one-reentry";
    let dir_a = temp_dir("live-package-probe-300s-height-one-reentry-a");
    let dir_b = temp_dir("live-package-probe-300s-height-one-reentry-b");
    let (_, public_key_a) = deterministic_keypair_hex(207);
    let (_, public_key_c) = deterministic_keypair_hex(208);
    let pos_config = signed_pos_config_with_signer_seeds(
        vec![
            PosValidator {
                validator_id: "node-a".to_string(),
                stake: 50,
            },
            PosValidator {
                validator_id: "node-c".to_string(),
                stake: 50,
            },
        ],
        &[("node-a", 207), ("node-c", 208)],
    );
    let replication_config_a = signed_replication_config(dir_a.clone(), 207)
        .with_remote_writer_allowlist(vec![deterministic_keypair_hex(209).1])
        .expect("allowlist a")
        .with_fetch_requester_allowlist(vec![deterministic_keypair_hex(209).1])
        .expect("fetch allowlist a");
    let replication_config_b = signed_replication_config(dir_b.clone(), 209)
        .with_remote_writer_allowlist(vec![public_key_a.clone(), public_key_c.clone()])
        .expect("allowlist b")
        .with_fetch_requester_allowlist(vec![public_key_a.clone(), public_key_c.clone()])
        .expect("fetch allowlist b");
    let config_a = NodeConfig::new("node-a", world_id, NodeRole::Sequencer)
        .expect("config a")
        .with_pos_config(pos_config.clone())
        .expect("pos config a")
        .with_replication(replication_config_a);
    let config_b = NodeConfig::new("node-b", world_id, NodeRole::Observer)
        .expect("config b")
        .with_pos_config(pos_config)
        .expect("pos config b")
        .with_require_execution_on_commit(true)
        .with_replication(replication_config_b.clone());
    let mut replication_a =
        ReplicationRuntime::new(config_a.replication.as_ref().expect("repl a"), "node-a")
            .expect("runtime a");
    let height_one = replication_a
        .build_local_commit_message(
            "node-a",
            world_id,
            20_100,
            &committed_decision(1),
            Some("peer-exec-block-1"),
            Some("peer-exec-state-1"),
        )
        .expect("build delayed height-one candidate")
        .expect("candidate");
    let checkpoint_height = 43_340;
    let head = Arc::new(Mutex::new(super::replication::FetchHeadResponse {
        found: true,
        head: Some(super::replication::ReplicationHeadSummary {
            world_id: world_id.to_string(),
            height: 1,
            block_hash: "block-1".to_string(),
            state_root: "peer-exec-state-1".to_string(),
            timestamp_ms: 20_100,
        }),
    }));
    let network: Arc<
        dyn oasis7_proto::distributed_net::DistributedNetwork<WorldError> + Send + Sync,
    > = Arc::new(PeerHeadCheckpointNetwork {
        inner: Arc::new(TestInMemoryNetwork::default()),
        fetch_protocols: Arc::new(Mutex::new(Vec::new())),
        head: Arc::clone(&head),
        checkpoint_fetch_available: Arc::new(AtomicBool::new(false)),
        checkpoint_fetch_not_found: Arc::new(AtomicBool::new(true)),
        connected_peer_ids: vec!["node-a".to_string(), "node-c".to_string()],
    });
    network
        .register_handler(
            REPLICATION_FETCH_COMMIT_PROTOCOL,
            Box::new(|_| {
                serde_json::to_vec(&super::replication::FetchCommitResponse {
                    found: false,
                    message: None,
                lineage_envelope: None,
                })
                .map_err(|err| WorldError::DistributedValidationFailed {
                    reason: format!("encode absent checkpoint response failed: {err}"),
                })
            }),
        )
        .expect("register absent checkpoint response");
    let handle_b = NodeReplicationNetworkHandle::new(Arc::clone(&network));
    let mut endpoint_b =
        ReplicationNetworkEndpoint::new(&handle_b, world_id, true, &config_b.network_policy)
            .expect("fresh observer endpoint");
    let mut replication_b =
        ReplicationRuntime::new(config_b.replication.as_ref().expect("repl b"), "node-b")
            .expect("fresh observer runtime");
    let mut engine_b = PosNodeEngine::new(&config_b).expect("fresh observer engine");
    let high_head = |node_id: &str, public_key_hex: String| PeerCommittedHead {
        height: checkpoint_height,
        block_hash: format!("block-{checkpoint_height}"),
        committed_at_ms: 20_164,
        observed_at_ms: 20_200,
        execution_block_hash: Some(format!("exec-block-{checkpoint_height}")),
        execution_state_root: Some(format!("exec-state-{checkpoint_height}")),
        action_root: empty_action_root(),
        public_key_hex: Some(public_key_hex),
        signature_hex: Some(format!("signed-{node_id}-{checkpoint_height}")),
    };
    engine_b.peer_heads.insert(
        "node-a".to_string(),
        high_head("node-a", deterministic_keypair_hex(207).1),
    );
    engine_b.peer_heads.insert(
        "node-c".to_string(),
        high_head("node-c", deterministic_keypair_hex(208).1),
    );
    assert_eq!(engine_b.peer_heads.len(), 2);
    let mut execution_hook = PackageProbeHeightOneFailureHook {
        incremental_commits: Vec::new(),
        rollback_heights: Vec::new(),
    };
    macro_rules! package_tick {
        ($now_ms:expr) => {
            engine_b.tick(
                "node-b",
                world_id,
                $now_ms,
                None,
                Some(&mut replication_b),
                Some(&mut endpoint_b),
                None,
                Vec::new(),
                Some(&mut execution_hook),
            )
        };
    }
    package_tick!(20_300).expect("initial probe must hold height zero");
    package_tick!(320_300).expect("300s probe must hold height zero");
    assert_eq!(engine_b.committed_height, 0);
    assert!(!dir_b
        .join("checkpoint-verification")
        .join(format!("{checkpoint_height}.json"))
        .exists());
    endpoint_b
        .publish_replication(&height_one)
        .expect("publish delayed candidate");
    let result = package_tick!(320_301);
    assert!(result.is_ok(), "height-one candidate must remain deferred: result={result:?} incremental={:?} rollback={:?}", execution_hook.incremental_commits, execution_hook.rollback_heights);
    assert_eq!(
        result
            .expect("height-zero snapshot")
            .consensus_snapshot
            .committed_height,
        0
    );
    assert!(execution_hook.incremental_commits.is_empty());
    assert!(execution_hook.rollback_heights.is_empty());
    endpoint_b
        .publish_replication(&height_one)
        .expect("republish delayed candidate");
    let result = package_tick!(320_302);
    assert!(
        result.is_ok(),
        "reentry candidate must remain deferred: result={result:?} incremental={:?} rollback={:?}",
        execution_hook.incremental_commits,
        execution_hook.rollback_heights
    );
    assert_eq!(
        result
            .expect("height-zero reentry snapshot")
            .consensus_snapshot
            .committed_height,
        0
    );
    assert!(execution_hook.incremental_commits.is_empty());
    assert!(execution_hook.rollback_heights.is_empty());
    let _ = fs::remove_dir_all(&dir_a);
    let _ = fs::remove_dir_all(&dir_b);
}

#[test]
fn testnet_249_low_world_connected_transport_without_high_evidence_completes_confirmation() {
    // Authority correction: this fixture has only an unsigned low FetchHead
    // and transport connectivity. It intentionally supplies no signed high
    // peer-head/commit evidence, so connected quorum must not be treated as a
    // high-head checkpoint authority source; legitimate low-head confirmation
    // must complete instead of holding forever.
    let _nonce_lock = lock_checkpoint_probe_nonce();
    let _probe_nonce = CheckpointProbeNonceGuard::install();
    let world_id = "world-testnet-249-connected-high-heads-without-cache";
    let dir_b = temp_dir("testnet-249-connected-high-heads-b");
    let (_, public_key_a) = deterministic_keypair_hex(249);
    let (_, public_key_b) = deterministic_keypair_hex(250);
    let (_, public_key_c) = deterministic_keypair_hex(251);
    let pos_config = signed_pos_config_with_signer_seeds(
        vec![
            PosValidator { validator_id: "node-a".to_string(), stake: 50 },
            PosValidator { validator_id: "node-c".to_string(), stake: 50 },
        ],
        &[("node-a", 249), ("node-c", 251)],
    );
    let replication_config_b = signed_replication_config(dir_b.clone(), 250)
        .with_remote_writer_allowlist(vec![public_key_a, public_key_c])
        .expect("allowlist b")
        .with_fetch_requester_allowlist(vec![public_key_b.clone()])
        .expect("fetch allowlist b");
    let config_b = NodeConfig::new("node-b", world_id, NodeRole::Observer)
        .expect("config b")
        .with_pos_config(pos_config.clone())
        .expect("pos config b")
        .with_require_execution_on_commit(true)
        .with_replication(replication_config_b.clone());
    let head = Arc::new(Mutex::new(super::replication::FetchHeadResponse {
        found: true,
        head: Some(super::replication::ReplicationHeadSummary {
            world_id: world_id.to_string(),
            height: 1,
            block_hash: "block-1".to_string(),
            state_root: "peer-exec-state-1".to_string(),
            timestamp_ms: 249_100,
        }),
    }));
    let network: Arc<dyn oasis7_proto::distributed_net::DistributedNetwork<WorldError> + Send + Sync> =
        Arc::new(PeerHeadCheckpointNetwork {
            inner: Arc::new(TestInMemoryNetwork::default()),
            fetch_protocols: Arc::new(Mutex::new(Vec::new())),
            head,
            checkpoint_fetch_available: Arc::new(AtomicBool::new(false)),
            checkpoint_fetch_not_found: Arc::new(AtomicBool::new(true)),
            connected_peer_ids: vec!["node-a".to_string(), "node-c".to_string()],
        });
    network
        .register_handler(REPLICATION_FETCH_COMMIT_PROTOCOL, Box::new(|_| {
            serde_json::to_vec(&super::replication::FetchCommitResponse {
                found: false,
                message: None,
            lineage_envelope: None,
            })
            .map_err(|err| WorldError::DistributedValidationFailed {
                reason: format!("encode absent checkpoint response failed: {err}"),
            })
        }))
        .expect("register absent checkpoint response");
    let handle_b = NodeReplicationNetworkHandle::new(Arc::clone(&network));
    let mut endpoint_b = ReplicationNetworkEndpoint::new(
        &handle_b, world_id, true, &config_b.network_policy,
    )
    .expect("fresh observer endpoint");
    assert_eq!(endpoint_b.connected_peer_ids(), vec!["node-a", "node-c"]);
    let mut replication_b = ReplicationRuntime::new(
        config_b.replication.as_ref().expect("repl b"), "node-b",
    )
    .expect("fresh observer runtime");
    let mut engine_b = PosNodeEngine::new(&config_b).expect("fresh observer engine");
    assert!(engine_b.peer_heads.is_empty());
    let mut execution_hook = PackageProbeHeightOneFailureHook {
        incremental_commits: Vec::new(),
        rollback_heights: Vec::new(),
    };
    macro_rules! package_tick {
        ($now_ms:expr) => {
            engine_b.tick(
                "node-b", world_id, $now_ms, None, Some(&mut replication_b),
                Some(&mut endpoint_b), None, Vec::new(), Some(&mut execution_hook),
            )
        };
    }
    package_tick!(249_300).expect("initial stale world head must hold height zero");
    package_tick!(549_300).expect("300s unavailable checkpoint probe must hold height zero");
    assert_eq!(engine_b.committed_height, 0);
    assert!(execution_hook.incremental_commits.is_empty());
    assert!(execution_hook.rollback_heights.is_empty());
    let result = package_tick!(549_301);
    assert!(
        result.is_ok(),
        "legitimate low-head confirmation must complete: result={result:?} incremental={:?} rollback={:?}",
        execution_hook.incremental_commits,
        execution_hook.rollback_heights
    );
    let snapshot = result.expect("height-zero post-merge snapshot");
    assert_eq!(snapshot.consensus_snapshot.committed_height, 0);
    assert!(!engine_b.fresh_observer_checkpoint_bootstrap_retry_pending);
    assert!(engine_b.fresh_observer_checkpoint_low_head_confirmation.is_none());
    assert!(execution_hook.incremental_commits.is_empty());
    assert!(execution_hook.rollback_heights.is_empty());
    assert_eq!(engine_b.committed_height, 0);
    assert_eq!(engine_b.replication_persisted_height, 0);
    let _ = fs::remove_dir_all(&dir_b);
}

fn low_world_connected_quorum_fixture(
    connected_peer_ids: Vec<String>,
    world_id: &str,
    bind_transport_peers: bool,
) -> (
    NodeConfig,
    ReplicationRuntime,
    ReplicationNetworkEndpoint,
    PosNodeEngine,
    Vec<std::path::PathBuf>,
) {
    let dir_a = temp_dir("low-world-connected-quorum-a");
    let dir_b = temp_dir("low-world-connected-quorum-b");
    let dir_c = temp_dir("low-world-connected-quorum-c");
    let (_, public_key_a) = deterministic_keypair_hex(252);
    let (_, public_key_b) = deterministic_keypair_hex(253);
    let (_, public_key_c) = deterministic_keypair_hex(254);
    let mut pos_config = signed_pos_config_with_signer_seeds(
        vec![
            PosValidator { validator_id: "node-a".to_string(), stake: 50 },
            PosValidator { validator_id: "node-c".to_string(), stake: 50 },
        ],
        &[("node-a", 252), ("node-c", 254)],
    );
    if bind_transport_peers {
        pos_config = pos_config
            .with_validator_player_ids(
                [
                    ("node-a".to_string(), "12D3KooWvalidatorA".to_string()),
                    ("node-c".to_string(), "12D3KooWvalidatorC".to_string()),
                ]
                .into_iter()
                .collect(),
            )
            .expect("transport peer binding");
    }
    let replication_config = signed_replication_config(dir_b.clone(), 253)
        .with_remote_writer_allowlist(vec![public_key_a, public_key_c])
        .expect("allowlist")
        .with_fetch_requester_allowlist(vec![public_key_b])
        .expect("fetch allowlist");
    let config = NodeConfig::new("node-b", world_id, NodeRole::Observer)
        .expect("observer config")
        .with_pos_config(pos_config)
        .expect("pos config")
        .with_require_execution_on_commit(true)
        .with_replication(replication_config);
    let head = Arc::new(Mutex::new(super::replication::FetchHeadResponse {
        found: true,
        head: Some(super::replication::ReplicationHeadSummary {
            world_id: world_id.to_string(),
            height: 1,
            block_hash: "block-1".to_string(),
            state_root: "state-1".to_string(),
            timestamp_ms: 10_100,
        }),
    }));
    let network: Arc<dyn oasis7_proto::distributed_net::DistributedNetwork<WorldError> + Send + Sync> =
        Arc::new(PeerHeadCheckpointNetwork {
            inner: Arc::new(TestInMemoryNetwork::default()),
            fetch_protocols: Arc::new(Mutex::new(Vec::new())),
            head,
            checkpoint_fetch_available: Arc::new(AtomicBool::new(false)),
            checkpoint_fetch_not_found: Arc::new(AtomicBool::new(true)),
            connected_peer_ids,
        });
    let handle = NodeReplicationNetworkHandle::new(Arc::clone(&network));
    let endpoint = ReplicationNetworkEndpoint::new(
        &handle, world_id, true, &config.network_policy,
    )
    .expect("observer endpoint");
    let replication = ReplicationRuntime::new(
        config.replication.as_ref().expect("replication"), "node-b",
    )
    .expect("replication runtime");
    let engine = PosNodeEngine::new(&config).expect("observer engine");
    (config, replication, endpoint, engine, vec![dir_a, dir_b, dir_c])
}

#[test]
fn unbound_validator_id_shaped_transport_peers_do_not_activate_checkpoint_quorum_hold() {
    let world_id = "world-unbound-validator-id-shaped-transport-peer";
    let (_config, _replication, endpoint, mut engine, dirs) =
        low_world_connected_quorum_fixture(
            vec!["12D3KooWvalidatorA".to_string(), "12D3KooWvalidatorC".to_string()],
            world_id,
            false,
        );
    let hold = engine.hold_fresh_observer_for_connected_validator_quorum(&endpoint, 1);
    assert!(
        !hold,
        "transport IDs that merely resemble validator IDs must not activate high-head authority"
    );
    assert!(!engine.fresh_observer_checkpoint_bootstrap_retry_pending);
    for dir in dirs {
        let _ = fs::remove_dir_all(dir);
    }
}

#[test]
fn low_world_with_bound_transport_quorum_completes_confirmation_without_high_evidence() {
    let _nonce_lock = lock_checkpoint_probe_nonce();
    let _probe_nonce = CheckpointProbeNonceGuard::install();
    let world_id = "world-low-head-connected-quorum-confirmation";
    let (_config, mut replication, mut endpoint, mut engine, dirs) =
        low_world_connected_quorum_fixture(
            vec!["12D3KooWvalidatorA".to_string(), "12D3KooWvalidatorC".to_string()],
            world_id,
            true,
        );
    let mut execution_hook = PackageProbeHeightOneFailureHook {
        incremental_commits: Vec::new(),
        rollback_heights: Vec::new(),
    };
    engine
        .tick(
            "node-b", world_id, 10_300, None, Some(&mut replication),
            Some(&mut endpoint), None, Vec::new(), Some(&mut execution_hook),
        )
        .expect("first legitimate low-head confirmation poll");
    engine
        .tick(
            "node-b", world_id, 10_301, None, Some(&mut replication),
            Some(&mut endpoint), None, Vec::new(), Some(&mut execution_hook),
        )
        .expect("second legitimate low-head confirmation poll");
    assert!(
        !engine.fresh_observer_checkpoint_bootstrap_retry_pending,
        "low world without high-head evidence must complete low-head confirmation"
    );
    assert!(engine.fresh_observer_checkpoint_low_head_confirmation.is_none());
    for dir in dirs {
        let _ = fs::remove_dir_all(dir);
    }
}

fn testnet_251_signed_high_heads_scenario(heads_before_preflight: bool) {
    let _nonce_lock = lock_checkpoint_probe_nonce();
    let _probe_nonce = CheckpointProbeNonceGuard::install();
    let world_id = "world-testnet-251-signed-heads-no-closure";
    let dir_a = temp_dir("testnet-251-signed-heads-a");
    let dir_b = temp_dir("testnet-251-signed-heads-b");
    let (_, public_key_a) = deterministic_keypair_hex(251);
    let (_, public_key_b) = deterministic_keypair_hex(253);
    let pos_config = signed_pos_config_with_signer_seeds(
        vec![
            PosValidator {
                validator_id: "node-a".to_string(),
                stake: 50,
            },
            PosValidator {
                validator_id: "node-c".to_string(),
                stake: 50,
            },
        ],
        &[("node-a", 251), ("node-c", 252)],
    );
    let replication_config_a = signed_replication_config(dir_a.clone(), 251)
        .with_remote_writer_allowlist(vec![public_key_b.clone()])
        .expect("allowlist a")
        .with_fetch_requester_allowlist(vec![public_key_b.clone()])
        .expect("fetch allowlist a");
    let replication_config_b = signed_replication_config(dir_b.clone(), 253)
        .with_remote_writer_allowlist(vec![public_key_a.clone()])
        .expect("allowlist b")
        .with_fetch_requester_allowlist(vec![public_key_a.clone()])
        .expect("fetch allowlist b");
    let config_a = NodeConfig::new("node-a", world_id, NodeRole::Sequencer)
        .expect("config a")
        .with_pos_config(pos_config.clone())
        .expect("pos config a")
        .with_replication(replication_config_a.clone());
    let config_b = NodeConfig::new("node-b", world_id, NodeRole::Observer)
        .expect("config b")
        .with_pos_config(pos_config)
        .expect("pos config b")
        .with_require_execution_on_commit(true)
        .with_replication(replication_config_b.clone());
    let mut replication_a =
        ReplicationRuntime::new(config_a.replication.as_ref().expect("replication a"), "node-a")
            .expect("runtime a");
    let height_one = replication_a
        .build_local_commit_message(
            "node-a",
            world_id,
            251_100,
            &committed_decision(1),
            Some("peer-exec-block-1"),
            Some("peer-exec-state-1"),
        )
        .expect("build height-one candidate")
        .expect("height-one candidate");
    let high_height = 64;
    let network: Arc<
        dyn oasis7_proto::distributed_net::DistributedNetwork<WorldError> + Send + Sync,
    > = Arc::new(PeerHeadCheckpointNetwork {
        inner: Arc::new(TestInMemoryNetwork::default()),
        fetch_protocols: Arc::new(Mutex::new(Vec::new())),
        head: Arc::new(Mutex::new(super::replication::FetchHeadResponse {
            found: true,
            head: Some(super::replication::ReplicationHeadSummary {
                world_id: world_id.to_string(),
                height: 1,
                block_hash: "block-1".to_string(),
                state_root: "peer-exec-state-1".to_string(),
                timestamp_ms: 251_100,
            }),
        })),
        checkpoint_fetch_available: Arc::new(AtomicBool::new(false)),
        checkpoint_fetch_not_found: Arc::new(AtomicBool::new(true)),
        connected_peer_ids: vec!["node-a".to_string(), "node-c".to_string()],
    });
    let handle = NodeReplicationNetworkHandle::new(Arc::clone(&network));
    let mut replication_endpoint =
        ReplicationNetworkEndpoint::new(&handle, world_id, true, &config_b.network_policy)
            .expect("observer replication endpoint");
    let mut consensus_endpoint =
        ConsensusNetworkEndpoint::new(&handle, world_id, true, &config_b.network_policy)
            .expect("observer consensus endpoint");
    let publish_endpoint = ConsensusNetworkEndpoint::new(
        &NodeReplicationNetworkHandle::new(Arc::clone(&network)),
        world_id,
        false,
        &config_a.network_policy,
    )
    .expect("validator consensus endpoint");
    let publish_signed_heads = || {
        for (node_id, seed) in [("node-a", 251_u8), ("node-c", 252_u8)] {
            let (private_hex, public_hex) = deterministic_keypair_hex(seed);
            let signing_key = SigningKey::from_bytes(
                &hex::decode(private_hex)
                    .expect("decode validator key")
                    .try_into()
                    .expect("validator key length"),
            );
            let signer = ConsensusMessageSigner::new(signing_key, public_hex)
                .expect("validator commit signer");
            let mut commit = GossipCommitMessage {
                version: 1,
                world_id: world_id.to_string(),
                node_id: node_id.to_string(),
                player_id: node_id.to_string(),
                height: high_height,
                slot: high_height,
                epoch: 0,
                block_hash: "block-64".to_string(),
                action_root: empty_action_root(),
                actions: Vec::new(),
                committed_at_ms: 251_164,
                execution_block_hash: Some("exec-block-64".to_string()),
                execution_state_root: Some("exec-state-64".to_string()),
                public_key_hex: None,
                signature_hex: None,
            };
            sign_commit_message(&mut commit, &signer).expect("sign validator commit");
            publish_endpoint
                .publish_commit(&commit)
                .expect("publish signed validator head");
        }
    };
    if heads_before_preflight {
        publish_signed_heads();
    }
    let mut replication_b =
        ReplicationRuntime::new(config_b.replication.as_ref().expect("replication b"), "node-b")
            .expect("runtime b");
    let mut engine_b = PosNodeEngine::new(&config_b).expect("observer engine");
    let mut execution_hook = PackageProbeHeightOneFailureHook {
        incremental_commits: Vec::new(),
        rollback_heights: Vec::new(),
    };
    let preflight = engine_b.tick(
        "node-b",
        world_id,
        251_300,
        None,
        Some(&mut replication_b),
        Some(&mut replication_endpoint),
        Some(&mut consensus_endpoint),
        Vec::new(),
        Some(&mut execution_hook),
    );
    assert!(preflight.is_ok(), "initial checkpoint preflight must not execute height one: {preflight:?}");
    assert_eq!(engine_b.peer_heads.len(), if heads_before_preflight { 2 } else { 0 });
    assert_eq!(engine_b.committed_height, 0);
    assert!(execution_hook.incremental_commits.is_empty());
    assert!(execution_hook.rollback_heights.is_empty());
    if !heads_before_preflight {
        publish_signed_heads();
        let authority_tick = engine_b.tick(
            "node-b",
            world_id,
            251_301,
            None,
            Some(&mut replication_b),
            Some(&mut replication_endpoint),
            Some(&mut consensus_endpoint),
            Vec::new(),
            Some(&mut execution_hook),
        );
        assert!(authority_tick.is_ok(), "late signed heads must be ingested without height-one execution: {authority_tick:?}");
        assert_eq!(engine_b.peer_heads.len(), 2);
        assert!(engine_b.peer_heads.values().all(|head| head.height == high_height));
        assert_eq!(engine_b.committed_height, 0);
        assert!(execution_hook.incremental_commits.is_empty());
        assert!(execution_hook.rollback_heights.is_empty());
    }
    let unavailable_retry = engine_b.tick(
        "node-b",
        world_id,
        551_300,
        None,
        Some(&mut replication_b),
        Some(&mut replication_endpoint),
        Some(&mut consensus_endpoint),
        Vec::new(),
        Some(&mut execution_hook),
    );
    assert!(
        unavailable_retry.is_ok(),
        "unavailable checkpoint closure must remain fail-closed through the probe window: {unavailable_retry:?}"
    );
    replication_endpoint
        .publish_replication(&height_one)
        .expect("publish height-one candidate after signed heads");
    let result = engine_b.tick(
        "node-b",
        world_id,
        551_301,
        None,
        Some(&mut replication_b),
        Some(&mut replication_endpoint),
        Some(&mut consensus_endpoint),
        Vec::new(),
        Some(&mut execution_hook),
    );
    assert!(
        result.is_ok(),
        "signed high authority without checkpoint closure must remain fail-closed: result={result:?} incremental={:?} rollback={:?}",
        execution_hook.incremental_commits,
        execution_hook.rollback_heights
    );
    let snapshot = result.expect("height-zero fail-closed snapshot");
    assert_eq!(snapshot.consensus_snapshot.committed_height, 0);
    assert_eq!(engine_b.committed_height, 0);
    assert_eq!(engine_b.replication_persisted_height, 0);
    assert!(execution_hook.incremental_commits.is_empty());
    assert!(execution_hook.rollback_heights.is_empty());
    let _ = fs::remove_dir_all(&dir_a);
    let _ = fs::remove_dir_all(&dir_b);
}

#[test]
fn testnet_251_signed_high_heads_hold_height_zero_without_checkpoint_closure() {
    testnet_251_signed_high_heads_scenario(true);
}

#[test]
fn testnet_251_late_signed_high_heads_hold_height_zero_without_checkpoint_closure() {
    testnet_251_signed_high_heads_scenario(false);
}
