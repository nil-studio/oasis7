#![expect(
    clippy::field_reassign_with_default,
    reason = "Test fixtures intentionally override canonical defaults for scenario fields."
)]
#![expect(
    clippy::enum_variant_names,
    reason = "Readiness scenario variants intentionally share a descriptive state suffix."
)]
#![expect(
    clippy::type_complexity,
    reason = "Boundary mutation tables keep each named snapshot mutation next to its scenario label."
)]

use oasis7::network_tier_manifest::{
    LoadedNetworkTierManifest, NETWORK_TIER_MANIFEST_SCHEMA_V1, NetworkTierClaimsPolicy,
    NetworkTierEndpointPolicy, NetworkTierManifest, NetworkTierPromotionPolicy,
    NetworkTierRuntimeRefs, NetworkTierTokenPolicy, NetworkTierValidatorPolicy,
};
use oasis7_node::{
    Libp2pReachabilitySnapshot, LiveAutoNatStatus, LivePublicPortReachability, LiveTransportKind,
    NodeAutoNatStatus, NodeConsensusSnapshot, NodeHolePunchViability, NodeNetworkPolicy,
    NodePeerCommittedHead, NodePublicPortReachability, NodeReachabilityAutoDetection, NodeRole,
    NodeSnapshot, NodeUserMode, NodeValidatorStakeProofSnapshot,
};
use oasis7_proto::distributed_dht::{PeerDeploymentMode, PeerNodeRole, PeerReachabilityClass};
use oasis7_runtime::ReleaseSecurityPolicy;
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn build_chain_status_payload_allows_genesis_self_head_cold_start() {
    let mut consensus = NodeConsensusSnapshot::default();
    consensus.committed_height = 0;
    consensus.network_committed_height = 0;
    consensus.replication_persisted_height = 0;
    consensus.known_peer_heads = 0;
    consensus.peer_heads = Vec::new();
    let snapshot = NodeSnapshot {
        node_id: "validator-genesis".to_string(),
        player_id: "player-genesis".to_string(),
        world_id: "world-public-testnet-genesis".to_string(),
        role: NodeRole::Sequencer,
        replication_enabled: true,
        running: true,
        tick_count: 1,
        last_tick_unix_ms: Some(1_700_000_000_000),
        consensus,
        consensus_progress_observer_error: None,
        last_error: None,
    };
    let observed_at_ms = 1_700_000_000_000;
    let network_head =
        super::status_payload::build_network_head_status(&snapshot, observed_at_ms, None);
    let policy = super::status_payload::readiness_policy(&snapshot, None);
    let observability = super::status_payload::build_chain_node_observability_status(
        &snapshot,
        &super::observability_tests::sample_observability_storage_metrics(),
        &super::observability_tests::sample_observability_reward_runtime_metrics(),
        &super::ChainReplicationDebugStatus {
            local_peer_id: "validator-genesis".to_string(),
            connected_peers: vec!["validator-peer".to_string()],
            peer_healths: vec![super::ChainPeerHealthStatus {
                peer_id: "validator-peer".to_string(),
                status: "active".to_string(),
                issues: Vec::new(),
                discovery_sources: vec!["static_bootstrap".to_string()],
                active_path_kind: Some("direct".to_string()),
                source_operator: Some("validator".to_string()),
                source_asn: None,
            }],
            registered_protocols: vec![
                "/aw/node/replication/fetch-blob/1.0.0".to_string(),
                "/aw/node/replication/fetch-commit/1.0.0".to_string(),
            ],
            protocol_retry_cooldown_peers: BTreeMap::new(),
            transport_retry_cooldown_peers: Vec::new(),
            request_peer_scores: BTreeMap::from([("validator-peer".to_string(), 100)]),
            connection_events: Vec::new(),
            recent_errors: vec![
                "libp2p incoming connection error: LocalPeerId { endpoint: Listener { local_addr: /ip4/172.26.53.91/tcp/6831, send_back_addr: /ip4/39.104.204.172/tcp/34306 } }".to_string(),
            ],
        },
        &network_head,
        &super::observability_tests::sample_observability_p2p_status(),
        &policy,
        None,
        observed_at_ms,
    );
    let readiness = super::status_payload::build_readiness_status(&observability, policy);

    assert_eq!(network_head.source, "self_only");
    assert_eq!(network_head.required_peer_count, 0);
    assert!(observability.network_head_available);
    assert!(observability.ready);
    assert_eq!(readiness.status, "ready");
    assert!(
        readiness.failed_gates.is_empty(),
        "{:?}",
        readiness.failed_gates
    );
    assert!(
        observability
            .alerts
            .iter()
            .all(|alert| alert.code != "consensus_peer_head_unavailable")
    );
    assert!(
        observability
            .alerts
            .iter()
            .all(|alert| alert.code != "replication_recent_errors")
    );
}

#[test]
fn build_chain_status_payload_allows_clean_genesis_cold_start() {
    let mut consensus = NodeConsensusSnapshot::default();
    consensus.committed_height = 0;
    consensus.network_committed_height = 0;
    consensus.replication_persisted_height = 0;
    consensus.known_peer_heads = 0;
    consensus.peer_heads = Vec::new();
    let snapshot = NodeSnapshot {
        node_id: "validator-a".to_string(),
        player_id: "player-validator-a".to_string(),
        world_id: "world-public-testnet".to_string(),
        role: NodeRole::Sequencer,
        replication_enabled: true,
        running: true,
        tick_count: 1,
        last_tick_unix_ms: None,
        consensus,
        consensus_progress_observer_error: None,
        last_error: None,
    };
    let observed_at_ms = 10_100;
    let network_head =
        super::status_payload::build_network_head_status(&snapshot, observed_at_ms, None);
    let policy = super::status_payload::readiness_policy(&snapshot, None);
    let observability = super::status_payload::build_chain_node_observability_status(
        &snapshot,
        &super::observability_tests::sample_observability_storage_metrics(),
        &super::observability_tests::sample_observability_reward_runtime_metrics(),
        &super::ChainReplicationDebugStatus {
            local_peer_id: "validator-a".to_string(),
            connected_peers: vec!["validator-b".to_string()],
            peer_healths: vec![super::ChainPeerHealthStatus {
                peer_id: "validator-b".to_string(),
                status: "active".to_string(),
                issues: Vec::new(),
                discovery_sources: vec!["static_bootstrap".to_string()],
                active_path_kind: Some("direct".to_string()),
                source_operator: Some("validator".to_string()),
                source_asn: None,
            }],
            registered_protocols: vec![
                "/aw/node/replication/fetch-blob/1.0.0".to_string(),
                "/aw/node/replication/fetch-commit/1.0.0".to_string(),
                "/aw/node/replication/fetch-commit/head/1.0.0".to_string(),
            ],
            protocol_retry_cooldown_peers: BTreeMap::new(),
            transport_retry_cooldown_peers: Vec::new(),
            request_peer_scores: BTreeMap::from([("validator-b".to_string(), 100)]),
            connection_events: Vec::new(),
            recent_errors: vec![
                "libp2p incoming connection error: LocalPeerId { endpoint: Listener { local_addr: /ip4/172.26.53.91/tcp/6831, send_back_addr: /ip4/39.104.204.172/tcp/34306 } }".to_string(),
                "libp2p connection established peer=validator-b direction=outbound addr=/ip4/39.104.205.67/tcp/6832/p2p/validator-b".to_string(),
            ],
        },
        &network_head,
        &super::observability_tests::sample_observability_p2p_status(),
        &policy,
        None,
        observed_at_ms,
    );
    let readiness = super::status_payload::build_readiness_status(&observability, policy);

    assert_eq!(network_head.required_peer_count, 0);
    assert_eq!(network_head.source, "self_only");
    assert_eq!(network_head.decision, "ready");
    assert!(observability.network_head_available);
    assert!(observability.ready);
    assert!(
        observability
            .alerts
            .iter()
            .all(|alert| alert.code != "consensus_peer_head_unavailable")
    );
    assert!(
        observability
            .alerts
            .iter()
            .all(|alert| alert.code != "replication_recent_errors")
    );
    assert_eq!(readiness.status, "ready");
}

#[test]
fn build_chain_status_payload_blocks_isolated_genesis_validator() {
    let mut consensus = NodeConsensusSnapshot::default();
    consensus.committed_height = 0;
    consensus.network_committed_height = 0;
    consensus.replication_persisted_height = 0;
    consensus.known_peer_heads = 0;
    consensus.peer_heads = Vec::new();
    let snapshot = NodeSnapshot {
        node_id: "validator-a".to_string(),
        player_id: "player-validator-a".to_string(),
        world_id: "world-public-testnet".to_string(),
        role: NodeRole::Sequencer,
        replication_enabled: true,
        running: true,
        tick_count: 1,
        last_tick_unix_ms: None,
        consensus,
        consensus_progress_observer_error: None,
        last_error: None,
    };
    let observed_at_ms = 10_100;
    let network_head =
        super::status_payload::build_network_head_status(&snapshot, observed_at_ms, None);
    let policy = super::status_payload::readiness_policy(&snapshot, None);
    let observability = super::status_payload::build_chain_node_observability_status(
        &snapshot,
        &super::observability_tests::sample_observability_storage_metrics(),
        &super::observability_tests::sample_observability_reward_runtime_metrics(),
        &super::ChainReplicationDebugStatus {
            local_peer_id: "validator-a".to_string(),
            connected_peers: Vec::new(),
            peer_healths: Vec::new(),
            registered_protocols: vec![
                "/aw/node/replication/fetch-blob/1.0.0".to_string(),
                "/aw/node/replication/fetch-commit/1.0.0".to_string(),
                "/aw/node/replication/fetch-commit/head/1.0.0".to_string(),
            ],
            protocol_retry_cooldown_peers: BTreeMap::new(),
            transport_retry_cooldown_peers: Vec::new(),
            request_peer_scores: BTreeMap::new(),
            connection_events: Vec::new(),
            recent_errors: Vec::new(),
        },
        &network_head,
        &super::observability_tests::sample_observability_p2p_status(),
        &policy,
        None,
        observed_at_ms,
    );
    let readiness = super::status_payload::build_readiness_status(&observability, policy);

    assert_eq!(network_head.source, "self_only");
    assert_eq!(network_head.decision, "ready");
    assert!(!observability.network_head_available);
    assert!(!observability.ready);
    assert_eq!(readiness.status, "not_ready");
    assert!(
        readiness
            .failed_gates
            .iter()
            .any(|gate| gate == "replication_no_connected_peers"),
        "{:?}",
        readiness.failed_gates
    );
}

#[test]
fn build_chain_status_payload_tolerates_noisy_external_peer_with_healthy_validator_path() {
    let mut consensus = NodeConsensusSnapshot::default();
    consensus.committed_height = 10;
    consensus.network_committed_height = 10;
    consensus.replication_persisted_height = 10;
    consensus.known_peer_heads = 1;
    consensus.last_committed_at_ms = Some(10_000);
    consensus.peer_heads = vec![NodePeerCommittedHead {
        node_id: "validator-b".to_string(),
        validator_id: None,
        height: 10,
        block_hash: "block-a".to_string(),
        committed_at_ms: 10_000,
        observed_at_ms: 10_050,
        execution_block_hash: Some("execution-a".to_string()),
        execution_state_root: Some("state-a".to_string()),
    }];
    let snapshot = NodeSnapshot {
        node_id: "validator-a".to_string(),
        player_id: "player-validator-a".to_string(),
        world_id: "world-public-testnet".to_string(),
        role: NodeRole::Sequencer,
        replication_enabled: true,
        running: true,
        tick_count: 1,
        last_tick_unix_ms: None,
        consensus,
        consensus_progress_observer_error: None,
        last_error: None,
    };
    let observed_at_ms = 10_100;
    let network_head =
        super::status_payload::build_network_head_status(&snapshot, observed_at_ms, None);
    let policy = super::status_payload::readiness_policy(&snapshot, None);
    let observability = super::status_payload::build_chain_node_observability_status(
        &snapshot,
        &super::observability_tests::sample_observability_storage_metrics(),
        &super::observability_tests::sample_observability_reward_runtime_metrics(),
        &super::ChainReplicationDebugStatus {
            local_peer_id: "validator-a".to_string(),
            connected_peers: vec!["validator-b".to_string(), "observer-noisy".to_string()],
            peer_healths: vec![
                super::ChainPeerHealthStatus {
                    peer_id: "validator-b".to_string(),
                    status: "active".to_string(),
                    issues: Vec::new(),
                    discovery_sources: vec!["static_bootstrap".to_string()],
                    active_path_kind: Some("direct".to_string()),
                    source_operator: Some("validator".to_string()),
                    source_asn: None,
                },
                super::ChainPeerHealthStatus {
                    peer_id: "observer-noisy".to_string(),
                    status: "active".to_string(),
                    issues: Vec::new(),
                    discovery_sources: vec!["dht".to_string()],
                    active_path_kind: Some("direct".to_string()),
                    source_operator: Some("observer".to_string()),
                    source_asn: None,
                },
            ],
            registered_protocols: vec![
                "/aw/node/replication/fetch-blob/1.0.0".to_string(),
                "/aw/node/replication/fetch-commit/1.0.0".to_string(),
            ],
            protocol_retry_cooldown_peers: BTreeMap::new(),
            transport_retry_cooldown_peers: Vec::new(),
            request_peer_scores: BTreeMap::from([
                ("validator-b".to_string(), 100),
                ("observer-noisy".to_string(), 0),
            ]),
            connection_events: Vec::new(),
            recent_errors: vec![
                "libp2p inbound failure from PeerId(\"observer-noisy\"): Timeout".to_string(),
                "libp2p inbound failure from PeerId(\"observer-noisy\"): Timeout".to_string(),
                "libp2p inbound failure from PeerId(\"observer-noisy\"): Timeout".to_string(),
                "libp2p inbound failure from PeerId(\"observer-noisy\"): Timeout".to_string(),
                "libp2p incoming connection error: Transport(Other(Custom { kind: Other, error: ProtocolError(InvalidMessage) }))".to_string(),
            ],
        },
        &network_head,
        &super::observability_tests::sample_observability_p2p_status(),
        &policy,
        None,
        observed_at_ms,
    );
    let readiness = super::status_payload::build_readiness_status(&observability, policy);

    assert!(observability.ready);
    assert!(observability.transport_stable);
    assert_eq!(observability.transport_stability_score, 100);
    assert_eq!(observability.recent_replication_error_count, 5);
    assert!(
        observability
            .alerts
            .iter()
            .all(|alert| alert.code != "replication_recent_errors")
    );
    assert!(
        observability
            .alerts
            .iter()
            .all(|alert| alert.code != "replication_transport_unstable")
    );
    assert_eq!(readiness.status, "ready");
    assert!(readiness.ready);
    assert!(!readiness.failed_gates.iter().any(
        |gate| gate == "replication_recent_errors" || gate == "replication_transport_unstable"
    ));
}

#[test]
fn readiness_failed_gates_excludes_warning_when_critical_alert_blocks() {
    let mut consensus = NodeConsensusSnapshot::default();
    consensus.committed_height = 0;
    consensus.network_committed_height = 0;
    consensus.replication_persisted_height = 0;
    let snapshot = NodeSnapshot {
        node_id: "validator-genesis".to_string(),
        player_id: "player-genesis".to_string(),
        world_id: "world-public-testnet-genesis".to_string(),
        role: NodeRole::Sequencer,
        replication_enabled: false,
        running: true,
        tick_count: 1,
        last_tick_unix_ms: Some(1_700_000_000_000),
        consensus,
        consensus_progress_observer_error: None,
        last_error: Some("critical runtime failure".to_string()),
    };
    let observed_at_ms = 1_700_000_000_000;
    let network_head =
        super::status_payload::build_network_head_status(&snapshot, observed_at_ms, None);
    let policy = super::status_payload::readiness_policy(&snapshot, None);
    let mut storage_metrics = super::observability_tests::sample_observability_storage_metrics();
    storage_metrics.degraded_reason = Some("warning-only storage pressure".to_string());
    let observability = super::status_payload::build_chain_node_observability_status(
        &snapshot,
        &storage_metrics,
        &super::observability_tests::sample_observability_reward_runtime_metrics(),
        &super::ChainReplicationDebugStatus {
            local_peer_id: "validator-genesis".to_string(),
            connected_peers: Vec::new(),
            peer_healths: Vec::new(),
            registered_protocols: Vec::new(),
            protocol_retry_cooldown_peers: BTreeMap::new(),
            transport_retry_cooldown_peers: Vec::new(),
            request_peer_scores: BTreeMap::new(),
            connection_events: Vec::new(),
            recent_errors: Vec::new(),
        },
        &network_head,
        &super::observability_tests::sample_observability_p2p_status(),
        &policy,
        None,
        observed_at_ms,
    );
    let readiness = super::status_payload::build_readiness_status(&observability, policy);

    assert!(
        observability
            .alerts
            .iter()
            .any(|alert| alert.severity == "warn" && alert.code == "storage_degraded")
    );
    assert!(
        observability
            .alerts
            .iter()
            .any(|alert| alert.severity == "critical" && alert.code == "runtime_last_error")
    );
    assert_eq!(readiness.status, "not_ready");
    assert_eq!(
        readiness.failed_gates,
        vec!["runtime_last_error", "storage_degraded"]
    );
}

#[path = "oasis7_chain_runtime_observability_readiness_publication_support.rs"]
mod publication_support;
use publication_support::*;

#[test]
fn public_testnet_sequencer_publication_grace_requires_complete_one_block_parent_quorum() {
    #[derive(Clone, Copy)]
    enum Expected {
        PublicationWarningReady,
        CriticalNotReady,
        PublishedReady,
    }
    fn check_case(
        failures: &mut Vec<String>,
        label: &str,
        snapshot: &NodeSnapshot,
        manifest: Option<&LoadedNetworkTierManifest>,
        expected: Expected,
    ) {
        let (network_head, observability, readiness) = publication_test_status(snapshot, manifest);
        let has_publication_warning = observability.alerts.iter().any(|alert| {
            alert.severity == "warn" && alert.code == "sequencer_head_publication_pending"
        });
        let has_divergence_alert = observability
            .alerts
            .iter()
            .any(|alert| alert.code == "local_chain_ahead_of_network_head");
        let outcome_matches = match expected {
            Expected::PublicationWarningReady => {
                network_head.decision == "ready"
                    && network_head.height == Some(snapshot.consensus.committed_height - 1)
                    && observability.status == "warn"
                    && observability.ready
                    && readiness.status == "ready"
                    && readiness.ready
                    && readiness.failed_gates.is_empty()
                    && has_publication_warning
                    && !has_divergence_alert
            }
            Expected::CriticalNotReady => {
                observability.status == "critical"
                    && !observability.ready
                    && readiness.status == "not_ready"
                    && !readiness.ready
                    && !has_publication_warning
            }
            Expected::PublishedReady => {
                network_head.decision == "ready"
                    && network_head.height == Some(snapshot.consensus.committed_height)
                    && observability.ready
                    && readiness.status == "ready"
                    && readiness.ready
                    && !has_publication_warning
                    && !has_divergence_alert
            }
        };
        if !outcome_matches {
            failures.push(format!(
                "{label}: network_head={{source:{},decision:{},height:{:?},fresh:{},required:{},conflicting:{},stake_met:{}}} observability={{status:{},ready:{},alerts:{:?}}} readiness={{status:{},ready:{},failed_gates:{:?}}}",
                network_head.source,
                network_head.decision,
                network_head.height,
                network_head.fresh_peer_count,
                network_head.required_peer_count,
                network_head.conflicting_peer_count,
                network_head.stake_quorum_met,
                observability.status,
                observability.ready,
                observability
                    .alerts
                    .iter()
                    .map(|alert| format!("{}:{}", alert.severity, alert.code))
                    .collect::<Vec<_>>(),
                readiness.status,
                readiness.ready,
                readiness.failed_gates,
            ));
        }
    }
    let public_testnet = publication_test_manifest("public_testnet", 2);
    let mainnet = publication_test_manifest("mainnet", 2);
    let mut failures = Vec::new();
    let eligible_at_boundary = publication_test_snapshot(NodeRole::Sequencer, 30_000);
    check_case(
        &mut failures,
        "public_testnet exact one-block lag at immutable commit age 30000ms",
        &eligible_at_boundary,
        Some(&public_testnet),
        Expected::PublicationWarningReady,
    );
    let mut expired = eligible_at_boundary.clone();
    expired.consensus.last_committed_at_ms = Some(PUBLICATION_TEST_OBSERVED_AT_MS - 30_001);
    check_case(
        &mut failures,
        "publication grace expired at 30001ms",
        &expired,
        Some(&public_testnet),
        Expected::CriticalNotReady,
    );
    let mut lead_two = eligible_at_boundary.clone();
    lead_two.consensus.peer_heads[0].height = PUBLICATION_TEST_HEIGHT - 2;
    check_case(
        &mut failures,
        "lead greater than one",
        &lead_two,
        Some(&public_testnet),
        Expected::CriticalNotReady,
    );
    for (label, role) in [
        ("observer role", NodeRole::Observer),
        ("storage role", NodeRole::Storage),
    ] {
        let role_snapshot = publication_test_snapshot(role, 1_000);
        check_case(
            &mut failures,
            label,
            &role_snapshot,
            Some(&public_testnet),
            Expected::CriticalNotReady,
        );
    }
    check_case(
        &mut failures,
        "mainnet tier",
        &publication_test_snapshot(NodeRole::Sequencer, 1_000),
        Some(&mainnet),
        Expected::CriticalNotReady,
    );
    check_case(
        &mut failures,
        "unspecified tier",
        &publication_test_snapshot(NodeRole::Sequencer, 1_000),
        None,
        Expected::CriticalNotReady,
    );
    let boundary_mutations: [(&str, fn(&mut NodeSnapshot)); 8] = [
        ("network committed boundary incomplete", |snapshot| {
            snapshot.consensus.network_committed_height -= 1;
        }),
        ("replication persisted boundary incomplete", |snapshot| {
            snapshot.consensus.replication_persisted_height -= 1;
        }),
        ("execution boundary incomplete", |snapshot| {
            snapshot.consensus.last_execution_height -= 1;
        }),
        ("local block binding missing", |snapshot| {
            snapshot.consensus.last_block_hash = None;
        }),
        ("local execution block binding missing", |snapshot| {
            snapshot.consensus.last_execution_block_hash = None;
        }),
        ("local execution state binding missing", |snapshot| {
            snapshot.consensus.last_execution_state_root = None;
        }),
        ("peer execution block binding missing", |snapshot| {
            snapshot.consensus.peer_heads[0].execution_block_hash = None;
        }),
        ("peer execution state binding missing", |snapshot| {
            snapshot.consensus.peer_heads[0].execution_state_root = None;
        }),
    ];
    for (label, mutate) in boundary_mutations {
        let mut snapshot = publication_test_snapshot(NodeRole::Sequencer, 1_000);
        mutate(&mut snapshot);
        check_case(
            &mut failures,
            label,
            &snapshot,
            Some(&public_testnet),
            Expected::CriticalNotReady,
        );
    }
    let mut cross_height = publication_test_snapshot(NodeRole::Sequencer, 1_000);
    cross_height
        .consensus
        .peer_heads
        .push(NodePeerCommittedHead {
            node_id: "validator-c".to_string(),
            validator_id: Some("validator-c".to_string()),
            height: PUBLICATION_TEST_HEIGHT - 2,
            block_hash: "competing-block-h99".to_string(),
            committed_at_ms: PUBLICATION_TEST_OBSERVED_AT_MS - 2_000,
            observed_at_ms: PUBLICATION_TEST_OBSERVED_AT_MS - 100,
            execution_block_hash: Some("competing-execution-h99".to_string()),
            execution_state_root: Some("competing-state-h99".to_string()),
        });
    cross_height.consensus.known_peer_heads = 2;
    check_case(
        &mut failures,
        "fresh cross-height competing bucket outside H-1",
        &cross_height,
        Some(&public_testnet),
        Expected::CriticalNotReady,
    );
    let mut stale = publication_test_snapshot(NodeRole::Sequencer, 1_000);
    stale.consensus.peer_heads[0].observed_at_ms = PUBLICATION_TEST_OBSERVED_AT_MS - 30_001;
    check_case(
        &mut failures,
        "stale quorum",
        &stale,
        Some(&public_testnet),
        Expected::CriticalNotReady,
    );
    let count_quorum_manifest = publication_test_manifest("public_testnet", 3);
    check_case(
        &mut failures,
        "missing count quorum",
        &publication_test_snapshot(NodeRole::Sequencer, 1_000),
        Some(&count_quorum_manifest),
        Expected::CriticalNotReady,
    );
    let mut conflicting = publication_test_snapshot(NodeRole::Sequencer, 1_000);
    let mut conflicting_head = conflicting.consensus.peer_heads[0].clone();
    conflicting_head.node_id = "validator-c".to_string();
    conflicting_head.validator_id = Some("validator-c".to_string());
    conflicting_head.block_hash = "conflicting-block-h100".to_string();
    conflicting.consensus.peer_heads.push(conflicting_head);
    conflicting.consensus.known_peer_heads = 2;
    check_case(
        &mut failures,
        "conflicting quorum",
        &conflicting,
        Some(&public_testnet),
        Expected::CriticalNotReady,
    );
    let mut stake_quorum_missing = publication_test_snapshot(NodeRole::Sequencer, 1_000);
    stake_quorum_missing.consensus.required_stake = 68;
    check_case(
        &mut failures,
        "missing stake quorum",
        &stake_quorum_missing,
        Some(&public_testnet),
        Expected::CriticalNotReady,
    );
    let mut unrelated_critical = publication_test_snapshot(NodeRole::Sequencer, 1_000);
    unrelated_critical.last_error = Some("independent critical failure".to_string());
    let (_, observability, readiness) =
        publication_test_status(&unrelated_critical, Some(&public_testnet));
    if observability.status != "critical"
        || observability.ready
        || readiness.ready
        || !observability
            .alerts
            .iter()
            .any(|alert| alert.code == "runtime_last_error" && alert.severity == "critical")
        || !observability.alerts.iter().any(|alert| {
            alert.code == "sequencer_head_publication_pending" && alert.severity == "warn"
        })
    {
        failures.push(format!(
            "unrelated critical alert still blocks publication warning: observability={{status:{},ready:{},alerts:{:?}}} readiness={{status:{},ready:{},failed_gates:{:?}}}",
            observability.status,
            observability.ready,
            observability
                .alerts
                .iter()
                .map(|alert| format!("{}:{}", alert.severity, alert.code))
                .collect::<Vec<_>>(),
            readiness.status,
            readiness.ready,
            readiness.failed_gates,
        ));
    }
    let mut published = publication_test_snapshot(NodeRole::Sequencer, 30_001);
    published.consensus.peer_heads[0].height = PUBLICATION_TEST_HEIGHT;
    published.consensus.peer_heads[0].block_hash = "block-h101".to_string();
    published.consensus.peer_heads[0].execution_block_hash = Some("execution-h101".to_string());
    published.consensus.peer_heads[0].execution_state_root = Some("state-h101".to_string());
    check_case(
        &mut failures,
        "publication warning clears when quorum reaches H",
        &published,
        Some(&public_testnet),
        Expected::PublishedReady,
    );
    assert!(
        failures.is_empty(),
        "sequencer publication readiness contract mismatches:\n{}",
        failures.join("\n")
    );
}
#[test]
fn public_testnet_sequencer_publication_grace_tracks_lag_episode_not_latest_commit() {
    #[derive(Clone, Copy)]
    enum Expected {
        WarningReady,
        CriticalNotReady,
        PublishedReady,
    }
    fn advance(
        snapshot: &mut NodeSnapshot,
        local_height: u64,
        local_committed_at_ms: i64,
        peer_height: u64,
        peer_committed_at_ms: i64,
        observed_at_ms: i64,
    ) {
        snapshot.consensus.latest_height = local_height;
        snapshot.consensus.committed_height = local_height;
        snapshot.consensus.network_committed_height = local_height;
        snapshot.consensus.replication_persisted_height = local_height;
        snapshot.consensus.last_execution_height = local_height;
        snapshot.consensus.last_committed_at_ms = Some(local_committed_at_ms);
        snapshot.consensus.last_block_hash = Some(format!("block-h{local_height}"));
        snapshot.consensus.last_execution_block_hash = Some(format!("execution-h{local_height}"));
        snapshot.consensus.last_execution_state_root = Some(format!("state-h{local_height}"));
        snapshot.consensus.peer_heads[0].height = peer_height;
        snapshot.consensus.peer_heads[0].block_hash = format!("block-h{peer_height}");
        snapshot.consensus.peer_heads[0].committed_at_ms = peer_committed_at_ms;
        snapshot.consensus.peer_heads[0].observed_at_ms = observed_at_ms;
        snapshot.consensus.peer_heads[0].execution_block_hash =
            Some(format!("execution-h{peer_height}"));
        snapshot.consensus.peer_heads[0].execution_state_root =
            Some(format!("state-h{peer_height}"));
    }
    fn reconcile(
        snapshot: &NodeSnapshot,
        manifest: &LoadedNetworkTierManifest,
        dir: &std::path::Path,
        records_dir: &std::path::Path,
        observed_at_ms: i64,
    ) {
        super::publication_lifecycle::reconcile(
            snapshot,
            Some(manifest),
            dir.join("execution-world").as_path(),
            records_dir,
            observed_at_ms,
        )
        .expect("publication lifecycle owner reconciles transition");
    }
    fn check(
        failures: &mut Vec<String>,
        label: &str,
        snapshot: &NodeSnapshot,
        manifest: &LoadedNetworkTierManifest,
        dir: &std::path::Path,
        records_dir: &std::path::Path,
        expected: Expected,
    ) {
        let payload = publication_test_full_payload_from_records(
            snapshot.clone(),
            manifest,
            dir,
            records_dir,
        );
        let has_warning = payload.observability.alerts.iter().any(|alert| {
            alert.code == "sequencer_head_publication_pending" && alert.severity == "warn"
        });
        let has_divergence = payload.observability.alerts.iter().any(|alert| {
            alert.code == "local_chain_ahead_of_network_head" && alert.severity == "critical"
        });
        let matches = match expected {
            Expected::WarningReady => {
                payload.observability.status == "warn"
                    && payload.observability.ready
                    && payload.readiness.status == "ready"
                    && payload.readiness.ready
                    && has_warning
                    && !has_divergence
            }
            Expected::CriticalNotReady => {
                payload.observability.status == "critical"
                    && !payload.observability.ready
                    && payload.readiness.status == "not_ready"
                    && !payload.readiness.ready
                    && !has_warning
                    && has_divergence
            }
            Expected::PublishedReady => {
                payload.observability.ready
                    && payload.readiness.status == "ready"
                    && payload.readiness.ready
                    && !has_warning
                    && !has_divergence
            }
        };
        if !matches {
            failures.push(format!(
                "{label}: local_height={} local_committed_at_ms={:?} peer_height={} peer_committed_at_ms={} observed_at_ms={} observability={{status:{},ready:{},alerts:{:?}}} readiness={{status:{},ready:{},failed_gates:{:?}}}",
                snapshot.consensus.committed_height,
                snapshot.consensus.last_committed_at_ms,
                snapshot.consensus.peer_heads[0].height,
                snapshot.consensus.peer_heads[0].committed_at_ms,
                payload.observed_at_unix_ms,
                payload.observability.status,
                payload.observability.ready,
                payload.observability
                    .alerts
                    .iter()
                    .map(|alert| format!("{}:{}", alert.severity, alert.code))
                    .collect::<Vec<_>>(),
                payload.readiness.status,
                payload.readiness.ready,
                payload.readiness.failed_gates,
            ));
        }
    }

    let manifest = publication_test_manifest("public_testnet", 2);
    let dir = publication_test_temp_dir("persistent-episode");
    let records_dir = dir.join("records");
    fs::create_dir_all(&records_dir).expect("create persistent publication records dir");
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after epoch")
        .as_millis() as i64;
    let episode_started_at_ms = now_ms - 30_001;
    let second_commit_at_ms = now_ms - 15_000;
    let third_commit_at_ms = now_ms;
    let mut snapshot = publication_test_snapshot(NodeRole::Sequencer, 0);
    let mut failures = Vec::new();

    for (height, timestamp_ms) in [
        (100, episode_started_at_ms - 1_000),
        (101, episode_started_at_ms),
        (102, second_commit_at_ms),
        (103, third_commit_at_ms),
    ] {
        publication_test_write_execution_record(
            &records_dir,
            &publication_test_execution_record_at(
                height,
                format!("block-h{height}").as_str(),
                format!("block-h{}", height - 1).as_str(),
                format!("execution-h{height}").as_str(),
                format!("state-h{height}").as_str(),
                timestamp_ms,
            ),
        );
    }
    advance(
        &mut snapshot,
        100,
        episode_started_at_ms - 1_000,
        100,
        episode_started_at_ms - 1_000,
        episode_started_at_ms - 500,
    );
    reconcile(
        &snapshot,
        &manifest,
        &dir,
        &records_dir,
        episode_started_at_ms - 500,
    );
    for (local_height, local_committed_at_ms, peer_height, peer_committed_at_ms) in [
        (
            101,
            episode_started_at_ms,
            100,
            episode_started_at_ms - 1_000,
        ),
        (102, second_commit_at_ms, 101, episode_started_at_ms),
        (103, third_commit_at_ms, 102, second_commit_at_ms),
    ] {
        advance(
            &mut snapshot,
            local_height,
            local_committed_at_ms,
            peer_height,
            peer_committed_at_ms,
            local_committed_at_ms,
        );
        reconcile(
            &snapshot,
            &manifest,
            &dir,
            &records_dir,
            local_committed_at_ms,
        );
    }
    advance(
        &mut snapshot,
        103,
        third_commit_at_ms,
        102,
        second_commit_at_ms,
        i64::MAX,
    );
    check(
        &mut failures,
        "persistent one-block lag exceeds original 30000ms grace despite fresh H103 commit",
        &snapshot,
        &manifest,
        &dir,
        &records_dir,
        Expected::CriticalNotReady,
    );

    let caught_up_at_ms = third_commit_at_ms + 1;
    advance(
        &mut snapshot,
        103,
        third_commit_at_ms,
        103,
        third_commit_at_ms,
        caught_up_at_ms,
    );
    reconcile(&snapshot, &manifest, &dir, &records_dir, caught_up_at_ms);
    check(
        &mut failures,
        "quorum catch-up resolves the first lag episode",
        &snapshot,
        &manifest,
        &dir,
        &records_dir,
        Expected::PublishedReady,
    );

    let new_episode_commit_at_ms = caught_up_at_ms + 1;
    publication_test_write_execution_record(
        &records_dir,
        &publication_test_execution_record_at(
            104,
            "block-h104",
            "block-h103",
            "execution-h104",
            "state-h104",
            new_episode_commit_at_ms,
        ),
    );
    advance(
        &mut snapshot,
        104,
        new_episode_commit_at_ms,
        103,
        third_commit_at_ms,
        new_episode_commit_at_ms,
    );
    reconcile(
        &snapshot,
        &manifest,
        &dir,
        &records_dir,
        new_episode_commit_at_ms,
    );
    check(
        &mut failures,
        "post-catch-up one-block lag starts a genuinely new grace episode",
        &snapshot,
        &manifest,
        &dir,
        &records_dir,
        Expected::WarningReady,
    );

    let retained_timestamps = (101..=104)
        .map(|height| {
            let bytes = fs::read(records_dir.join(format!("{height:020}.json")))
                .expect("read retained chronology record");
            serde_json::from_slice::<serde_json::Value>(&bytes).expect("parse retained record")
                ["timestamp_ms"]
                .as_i64()
                .expect("retained timestamp")
        })
        .collect::<Vec<_>>();
    if retained_timestamps
        != vec![
            episode_started_at_ms,
            second_commit_at_ms,
            third_commit_at_ms,
            new_episode_commit_at_ms,
        ]
        || retained_timestamps[2] - retained_timestamps[0] <= 30_000
    {
        failures.push(format!(
            "retained execution records did not preserve lag/new-episode chronology: {retained_timestamps:?}"
        ));
    }

    fs::remove_dir_all(dir).expect("remove persistent publication test dir");
    assert!(
        failures.is_empty(),
        "sequencer publication episode contract mismatches:\n{}",
        failures.join("\n")
    );
}
#[test]
fn public_testnet_sequencer_publication_warning_requires_retained_direct_parent_proof() {
    let manifest = publication_test_manifest("public_testnet", 2);
    let mut valid_snapshot = publication_test_snapshot(NodeRole::Sequencer, -1);
    valid_snapshot.consensus.last_committed_at_ms = Some(i64::MAX);
    valid_snapshot.consensus.peer_heads[0].committed_at_ms = i64::MAX - 1;
    valid_snapshot.consensus.peer_heads[0].observed_at_ms = i64::MAX - 1;
    let valid_parent = publication_test_execution_record_at(
        100,
        "block-h100",
        "block-h99",
        "execution-h100",
        "state-h100",
        i64::MAX,
    );
    let valid_local = publication_test_execution_record_at(
        101,
        "block-h101",
        "block-h100",
        "execution-h101",
        "state-h101",
        i64::MAX,
    );
    let valid = publication_test_full_payload(
        valid_snapshot.clone(),
        &manifest,
        valid_local.clone(),
        valid_parent.clone(),
    );
    let mut failures = Vec::new();
    let valid_warning = valid.observability.alerts.iter().find(|alert| {
        alert.code == "sequencer_head_publication_pending" && alert.severity == "warn"
    });
    if valid.observability.status != "warn"
        || !valid.observability.ready
        || !valid.readiness.ready
        || !valid.readiness.failed_gates.is_empty()
        || valid_warning.is_none()
        || valid
            .observability
            .alerts
            .iter()
            .any(|alert| alert.code == "local_chain_ahead_of_network_head")
    {
        failures.push(format!(
            "valid retained direct-parent proof was not warning-only ready: observability={{status:{},ready:{},alerts:{:?}}} readiness={{status:{},ready:{},failed_gates:{:?}}}",
            valid.observability.status,
            valid.observability.ready,
            valid
                .observability
                .alerts
                .iter()
                .map(|alert| format!("{}:{}", alert.severity, alert.code))
                .collect::<Vec<_>>(),
            valid.readiness.status,
            valid.readiness.ready,
            valid.readiness.failed_gates,
        ));
    }
    if let Some(warning) = valid_warning {
        for expected_detail in [
            "101",
            "100",
            "block-h101",
            "block-h100",
            "execution-h101",
            "execution-h100",
            "state-h101",
            "state-h100",
        ] {
            if !warning.summary.contains(expected_detail) {
                failures.push(format!(
                    "publication warning omitted binding detail `{expected_detail}`: {}",
                    warning.summary
                ));
            }
        }
    }
    let mut non_parent_local = valid_local.clone();
    non_parent_local["prev_node_block_hash"] =
        serde_json::Value::String("fork-block-h100".to_string());
    let non_parent = publication_test_full_payload(
        valid_snapshot.clone(),
        &manifest,
        non_parent_local,
        valid_parent.clone(),
    );
    if non_parent.observability.status != "critical"
        || non_parent.observability.ready
        || non_parent.readiness.ready
        || non_parent.observability.alerts.iter().any(|alert| {
            alert.code == "sequencer_head_publication_pending" && alert.severity == "warn"
        })
    {
        failures.push(format!(
            "non-parent H-1 was not critical: observability={{status:{},ready:{},alerts:{:?}}}",
            non_parent.observability.status,
            non_parent.observability.ready,
            non_parent
                .observability
                .alerts
                .iter()
                .map(|alert| format!("{}:{}", alert.severity, alert.code))
                .collect::<Vec<_>>(),
        ));
    }
    let mismatched_parent = publication_test_execution_record(
        100,
        "block-h100",
        "block-h99",
        "fork-execution-h100",
        "fork-state-h100",
    );
    let retained_binding_mismatch = publication_test_full_payload(
        valid_snapshot.clone(),
        &manifest,
        valid_local.clone(),
        mismatched_parent,
    );
    if retained_binding_mismatch.observability.status != "critical"
        || retained_binding_mismatch.observability.ready
        || retained_binding_mismatch.readiness.ready
        || retained_binding_mismatch
            .observability
            .alerts
            .iter()
            .any(|alert| {
                alert.code == "sequencer_head_publication_pending" && alert.severity == "warn"
            })
    {
        failures.push(format!(
            "retained H-1 execution/state mismatch was not critical: observability={{status:{},ready:{},alerts:{:?}}}",
            retained_binding_mismatch.observability.status,
            retained_binding_mismatch.observability.ready,
            retained_binding_mismatch
                .observability
                .alerts
                .iter()
                .map(|alert| format!("{}:{}", alert.severity, alert.code))
                .collect::<Vec<_>>(),
        ));
    }
    let mut mismatched_local = valid_local;
    mismatched_local["execution_block_hash"] =
        serde_json::Value::String("execution-from-another-block".to_string());
    let local_binding_mismatch =
        publication_test_full_payload(valid_snapshot, &manifest, mismatched_local, valid_parent);
    if local_binding_mismatch.observability.status != "critical"
        || local_binding_mismatch.observability.ready
        || local_binding_mismatch.readiness.ready
        || local_binding_mismatch
            .observability
            .alerts
            .iter()
            .any(|alert| {
                alert.code == "sequencer_head_publication_pending" && alert.severity == "warn"
            })
    {
        failures.push(format!(
            "local H execution binding mismatch was not critical: observability={{status:{},ready:{},alerts:{:?}}}",
            local_binding_mismatch.observability.status,
            local_binding_mismatch.observability.ready,
            local_binding_mismatch
                .observability
                .alerts
                .iter()
                .map(|alert| format!("{}:{}", alert.severity, alert.code))
                .collect::<Vec<_>>(),
        ));
    }
    assert!(
        failures.is_empty(),
        "sequencer publication ancestry/binding contract mismatches:\n{}",
        failures.join("\n")
    );
}
