use super::*;
use crate::status_payload;

#[test]
#[expect(
    clippy::field_reassign_with_default,
    reason = "Test fixture construction intentionally starts from canonical defaults before overriding scenario-specific fields."
)]
fn mainnet_validator_relay_policy_requires_governed_redundancy_and_surfaces_slashing_boundary() {
    let runtime_sha256 = current_test_binary_sha256();
    let (dir, manifest_path) =
        write_test_network_tier_manifest_for_tier(runtime_sha256.as_str(), "mainnet");
    let loaded = LoadedNetworkTierManifest::load(manifest_path.as_path()).expect("load manifest");
    let mut consensus = NodeConsensusSnapshot::default();
    consensus.committed_height = 10;
    consensus.network_committed_height = 10;
    consensus.replication_persisted_height = 10;
    consensus.known_peer_heads = 2;
    consensus.validator_stakes = BTreeMap::from([
        ("validator-b".to_string(), 40),
        ("validator-c".to_string(), 34),
    ]);
    consensus.required_stake = 67;
    consensus.total_stake = 100;
    add_test_stake_proof_chain(&mut consensus);
    consensus.peer_heads = vec![
        peer_head("node-b", Some("validator-b"), 10, i64::MAX),
        peer_head("node-c", Some("validator-c"), 10, i64::MAX),
    ];
    let snapshot = NodeSnapshot {
        node_id: "validator-a".to_string(),
        player_id: "player-a".to_string(),
        world_id: "mainnet-a".to_string(),
        role: NodeRole::Sequencer,
        replication_enabled: true,
        running: true,
        tick_count: 1,
        last_tick_unix_ms: Some(1_700_000_000_000),
        consensus,
        consensus_progress_observer_error: None,
        last_error: None,
    };
    let mut p2p = public_p2p_status();
    p2p.detected_reachability = None;
    p2p.autonat_status = "unknown".to_string();
    p2p.public_port_reachability = "unknown".to_string();
    p2p.observed_public_addr = None;
    p2p.confirmed_external_direct_addrs = Vec::new();
    p2p.relay_available = true;
    let policy = status_payload::readiness_policy(&snapshot, Some(&loaded));
    let network_head =
        status_payload::build_network_head_status(&snapshot, 1_700_000_000_000, Some(&loaded));
    let status = status_payload::build_chain_node_observability_status(
        &snapshot,
        &minimal_storage_metrics(),
        &minimal_reward_metrics(),
        &replication_with_active_peers(1),
        &network_head,
        &p2p,
        &policy,
        None,
        1_700_000_000_000,
    );

    assert_eq!(policy.relay_policy, "public_direct_or_governed_relay");
    assert_eq!(policy.slashing_policy, "evidence_only_readiness_gate");
    assert!(!policy.slashing_enforced);
    assert!(!status.reachability_policy_ok);
    assert!(
        status
            .alerts
            .iter()
            .any(|alert| alert.code == "p2p_reachability_degraded")
    );
    assert!(
        status
            .alerts
            .iter()
            .any(|alert| alert.code == "mainnet_slashing_evidence_only")
    );

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn mainnet_sync_lag_stalls_after_policy_window() {
    let runtime_sha256 = current_test_binary_sha256();
    let (dir, manifest_path) =
        write_test_network_tier_manifest_for_tier(runtime_sha256.as_str(), "mainnet");
    let loaded = LoadedNetworkTierManifest::load(manifest_path.as_path()).expect("load manifest");
    let mut consensus = NodeConsensusSnapshot::default();
    consensus.committed_height = 10;
    consensus.network_committed_height = 12;
    consensus.replication_persisted_height = 10;
    consensus.last_committed_at_ms = Some(1_700_000_000_000);
    consensus.known_peer_heads = 2;
    consensus.validator_stakes = BTreeMap::from([
        ("validator-b".to_string(), 40),
        ("validator-c".to_string(), 34),
    ]);
    consensus.required_stake = 67;
    consensus.total_stake = 100;
    add_test_stake_proof_chain(&mut consensus);
    consensus.peer_heads = vec![
        peer_head("node-b", Some("validator-b"), 12, i64::MAX),
        peer_head("node-c", Some("validator-c"), 12, i64::MAX),
    ];
    let snapshot = NodeSnapshot {
        node_id: "validator-a".to_string(),
        player_id: "player-a".to_string(),
        world_id: "mainnet-a".to_string(),
        role: NodeRole::Sequencer,
        replication_enabled: true,
        running: true,
        tick_count: 1,
        last_tick_unix_ms: Some(1_700_000_020_000),
        consensus,
        consensus_progress_observer_error: None,
        last_error: None,
    };
    let policy = status_payload::readiness_policy(&snapshot, Some(&loaded));
    let network_head =
        status_payload::build_network_head_status(&snapshot, 1_700_000_020_000, Some(&loaded));
    let status = status_payload::build_chain_node_observability_status(
        &snapshot,
        &minimal_storage_metrics(),
        &minimal_reward_metrics(),
        &replication_with_active_peers(2),
        &network_head,
        &public_p2p_status(),
        &policy,
        None,
        1_700_000_020_000,
    );

    assert!(!status.ready);
    assert!(
        status
            .alerts
            .iter()
            .any(|alert| alert.code == "consensus_sync_stalled")
    );
    let sync = status_payload::build_sync_status(
        &network_head,
        status.network_height_lag,
        &policy,
        &snapshot,
        1_700_000_020_000,
    );
    assert_eq!(sync.status, "stalled");

    let _ = fs::remove_dir_all(dir);
}
