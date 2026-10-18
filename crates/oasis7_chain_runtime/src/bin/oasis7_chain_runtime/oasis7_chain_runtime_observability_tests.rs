use super::*;
use oasis7_node::{
    Libp2pReachabilitySnapshot, LiveAutoNatStatus, LivePublicPortReachability, LiveTransportKind,
    LiveTransportTransition, LiveTransportTransitionCounters, NodeAutoNatStatus,
    NodeConsensusSnapshot, NodeFinalityLatencySnapshot, NodeHolePunchViability, NodeNetworkPolicy,
    NodePeerCommittedHead, NodePendingConsensusActionsSnapshot, NodePendingProposalSnapshot,
    NodePublicPortReachability, NodeReachabilityAutoDetection, NodeRole, NodeSnapshot,
    NodeUserMode, PosConsensusStatus,
};
use oasis7_proto::distributed_dht::{PeerDeploymentMode, PeerNodeRole, PeerReachabilityClass};
use oasis7_proto::storage_profile::{StorageProfile, StorageProfileConfig};
use oasis7_runtime::ReleaseSecurityPolicy;
use std::collections::BTreeMap;
use std::path::Path;

pub(super) fn sample_observability_p2p_status() -> super::status_payload::ChainP2pStatus {
    super::status_payload::ChainP2pStatus {
        requested_user_mode: "auto_join".to_string(),
        recommended_user_mode: "private_safe".to_string(),
        effective_user_mode: "private_safe".to_string(),
        applied_effective_user_mode: Some("private_safe".to_string()),
        requires_explicit_public_entry_confirmation: false,
        detected_reachability: Some("public".to_string()),
        hole_punch_viability: "viable".to_string(),
        autonat_status: "public".to_string(),
        public_port_reachability: "reachable".to_string(),
        observed_public_addr: Some("/ip4/203.0.113.10/tcp/4001".to_string()),
        confirmed_external_direct_addrs: vec!["/ip4/203.0.113.10/tcp/4001".to_string()],
        active_transport_kind: Some("direct".to_string()),
        active_transport_kind_since_unix_ms: Some(1_700_000_000_000),
        active_direct_path_count: 1,
        active_hole_punch_path_count: 0,
        active_relay_path_count: 0,
        transport_transition_count: 0,
        transport_transitions: super::status_payload::ChainP2pTransportTransitionCounters::default(
        ),
        last_transport_transition: None,
        relay_available: false,
        probe_stable: true,
        deployment_mode: "private".to_string(),
        node_role_claim: "validator_core".to_string(),
        rationale: Vec::new(),
    }
}

pub(super) fn sample_observability_storage_metrics()
-> super::storage_metrics::StorageMetricsSnapshot {
    super::storage_metrics::StorageMetricsSnapshot {
        storage_profile: "dev_local".to_string(),
        effective_budget: StorageProfileConfig::from(StorageProfile::DevLocal),
        bytes_by_dir: BTreeMap::new(),
        blob_counts: BTreeMap::new(),
        ref_count: 0,
        pin_count: 0,
        retained_heights: Vec::new(),
        checkpoint_count: 0,
        replay_summary: super::storage_metrics::StorageReplaySummary::default(),
        orphan_blob_count: 0,
        last_gc_at_ms: None,
        last_gc_result: "not_available".to_string(),
        last_gc_error: None,
        degraded_reason: None,
    }
}

pub(super) fn sample_observability_reward_runtime_metrics()
-> super::reward_runtime_worker::RewardRuntimeMetricsSnapshot {
    super::reward_runtime_worker::RewardRuntimeMetricsSnapshot {
        enabled: false,
        metrics_available: true,
        report_dir: String::new(),
        report_count: 0,
        latest_epoch_index: 0,
        latest_report_observed_at_unix_ms: 0,
        latest_total_distributed_points: 0,
        latest_minted_record_count: 0,
        cumulative_minted_record_count: 0,
        distfs_total_checks: 0,
        distfs_failed_checks: 0,
        distfs_failure_ratio: 0.0,
        settlement_apply_attempts_total: 0,
        settlement_apply_failures_total: 0,
        settlement_apply_failure_ratio: 0.0,
        invariant_ok: true,
        last_error: None,
    }
}

pub(super) fn sample_wasm_status() -> super::wasm_status::ChainWasmStatus {
    super::wasm_status::ChainWasmStatus {
        metrics_available: true,
        observed_since_unix_ms: Some(1_700_000_000_000),
        degraded_reason: None,
        build: super::wasm_status::ChainWasmBuildStatus {
            metrics_available: true,
            observed_since_unix_ms: Some(1_700_000_000_000),
            degraded_reason: None,
            total_build_wall_ms: Some(120),
            cargo_build_ms: Some(80),
            canonicalize_ms: Some(10),
            hash_ms: Some(5),
            receipt_write_ms: Some(3),
            metadata_write_ms: Some(2),
            wasm_size_bytes: Some(4096),
        },
        executor: oasis7_wasm_executor::WasmExecutorMetricsSnapshot {
            observed_since_unix_ms: 1_700_000_000_000,
            metrics_available: true,
            degraded_reason: None,
            calls_total: 4,
            memory_cache_hits: 2,
            disk_cache_hits: 1,
            compile_misses: 1,
            failure_by_code: BTreeMap::from([("trap".to_string(), 1)]),
            compile_ms_total: 90,
            deserialize_ms_total: 15,
            instantiate_ms_total: 20,
            entrypoint_call_ms_total: 45,
            decode_ms_total: 10,
            call_wall_ms_buckets: BTreeMap::from([("le_0010_ms".to_string(), 4)]),
            module_hotspots: vec![oasis7_wasm_executor::WasmExecutorModuleHotspot {
                module_id: "m.metrics".to_string(),
                calls_total: 4,
                wall_ms_total: 45,
                failure_count: 1,
                share_ppm: 1_000_000,
            }],
        },
        router: oasis7_wasm_router::WasmRouterMetricsSnapshot {
            observed_since_unix_ms: 1_700_000_000_000,
            metrics_available: true,
            degraded_reason: None,
            prepare_calls_total: 2,
            prepare_ms_total: 6,
            match_calls_total: 8,
            match_ms_total: 12,
            parse_fallbacks: 3,
            prepared_hits: 5,
            regex_compile_ms_total: 1,
            prepare_ms_buckets: BTreeMap::from([("le_0005_ms".to_string(), 2)]),
            match_ms_buckets: BTreeMap::from([("le_0005_ms".to_string(), 8)]),
            ..oasis7_wasm_router::WasmRouterMetricsSnapshot::empty()
        },
    }
}

fn sample_transfer_metrics() -> super::transfer_submit_api::ChainTransferMetricsStatus {
    super::transfer_submit_api::ChainTransferMetricsStatus {
        tracked_records: 6,
        accepted_count: 1,
        pending_count: 1,
        confirmed_count: 3,
        failed_count: 0,
        timeout_count: 1,
        inflight_count: 2,
        oldest_inflight_age_ms: Some(900),
        recent_confirmation_latency:
            super::transfer_submit_api::ChainTransferLatencySummaryStatus {
                sample_count: 3,
                avg_latency_ms: Some(640),
                max_latency_ms: Some(1_100),
                p50_latency_ms: Some(500),
                p95_latency_ms: Some(1_100),
            },
    }
}

#[expect(
    clippy::field_reassign_with_default,
    reason = "Test fixture construction intentionally starts from canonical defaults before overriding scenario-specific fields."
)]
fn assert_chain_status_payload_consensus_health_metrics() {
    let mut consensus = NodeConsensusSnapshot::default();
    consensus.committed_height = 5;
    consensus.network_committed_height = 7;
    consensus.known_peer_heads = 1;
    consensus.peer_heads = vec![NodePeerCommittedHead {
        node_id: "node-b".to_string(),
        validator_id: None,
        height: 7,
        block_hash: "peer-block-7".to_string(),
        committed_at_ms: 1_700_000_000_000,
        observed_at_ms: i64::MAX,
        execution_block_hash: Some("peer-execution-7".to_string()),
        execution_state_root: Some("peer-state-7".to_string()),
    }];
    consensus.last_committed_at_ms = Some(1_700_000_000_000);
    consensus.inbound_rejected_proposal_future_slot = 3;
    consensus.inbound_rejected_proposal_stale_slot = 1;
    consensus.inbound_rejected_attestation_future_slot = 2;
    consensus.inbound_rejected_attestation_stale_slot = 4;
    consensus.inbound_rejected_attestation_epoch_mismatch = 5;
    consensus.last_inbound_timing_reject_reason =
        Some("attestation target_epoch mismatch".to_string());
    consensus.pending_proposal = Some(NodePendingProposalSnapshot {
        height: 6,
        slot: 9,
        epoch: 1,
        proposer_id: "node-b".to_string(),
        opened_at_ms: 1_699_999_999_500,
        action_count: 2,
        action_payload_bytes: 384,
        attestation_count: 1,
        approved_stake: 34,
        rejected_stake: 0,
        required_stake: 67,
        total_stake: 100,
        approval_progress_bps: 5_074,
        rejection_progress_bps: 0,
        remaining_approval_stake: 33,
        status: PosConsensusStatus::Pending,
    });
    consensus.pending_consensus_actions = NodePendingConsensusActionsSnapshot {
        queued_action_count: 7,
        queued_payload_bytes: 1_024,
        reserved_requeue_action_count: 2,
        reserved_requeue_payload_bytes: 300,
        available_capacity: 11,
        max_capacity: 20,
        submit_buffer_action_count: 3,
        submit_buffer_payload_bytes: 480,
        submit_buffer_max_capacity: 64,
    };
    consensus.recent_finality_latency = NodeFinalityLatencySnapshot {
        sample_count: 4,
        avg_latency_ms: Some(780),
        max_latency_ms: Some(1_500),
        p50_latency_ms: Some(700),
        p95_latency_ms: Some(1_500),
    };
    let snapshot = NodeSnapshot {
        node_id: "node-a".to_string(),
        player_id: "player-a".to_string(),
        world_id: "live-a".to_string(),
        role: NodeRole::Storage,
        replication_enabled: false,
        running: true,
        tick_count: 42,
        last_tick_unix_ms: Some(1_700_000_000_000),
        consensus,
        consensus_progress_observer_error: None,
        last_error: None,
    };
    let reward_runtime = super::reward_runtime_worker::RewardRuntimeMetricsSnapshot {
        enabled: true,
        metrics_available: true,
        report_dir: "/tmp/reports".to_string(),
        report_count: 2,
        latest_epoch_index: 1,
        latest_report_observed_at_unix_ms: 1_700_000_000_000,
        latest_total_distributed_points: 10,
        latest_minted_record_count: 1,
        cumulative_minted_record_count: 1,
        distfs_total_checks: 0,
        distfs_failed_checks: 0,
        distfs_failure_ratio: 0.0,
        settlement_apply_attempts_total: 0,
        settlement_apply_failures_total: 0,
        settlement_apply_failure_ratio: 0.0,
        invariant_ok: true,
        last_error: None,
    };
    let storage = super::storage_metrics::StorageMetricsSnapshot {
        storage_profile: "dev_local".to_string(),
        effective_budget: StorageProfileConfig::from(StorageProfile::DevLocal),
        bytes_by_dir: BTreeMap::from([("runtime_root".to_string(), 128)]),
        blob_counts: BTreeMap::from([("execution_store_blobs".to_string(), 2)]),
        ref_count: 5,
        pin_count: 3,
        retained_heights: vec![1, 2],
        checkpoint_count: 1,
        replay_summary: super::storage_metrics::StorageReplaySummary {
            retained_height_count: 2,
            earliest_retained_height: Some(1),
            latest_retained_height: Some(2),
            earliest_checkpoint_height: Some(2),
            latest_checkpoint_height: Some(2),
            mode: "checkpoint_plus_log".to_string(),
        },
        orphan_blob_count: 0,
        last_gc_at_ms: Some(1_700_000_000_000),
        last_gc_result: "failed".to_string(),
        last_gc_error: Some("gc failed".to_string()),
        degraded_reason: Some("storage degraded".to_string()),
    };
    let replication = super::ChainReplicationDebugStatus {
        local_peer_id: "peer-local".to_string(),
        connected_peers: vec!["peer-a".to_string()],
        peer_healths: vec![
            super::ChainPeerHealthStatus {
                peer_id: "peer-a".to_string(),
                status: "active".to_string(),
                issues: Vec::new(),
                discovery_sources: vec!["bootstrap".to_string()],
                active_path_kind: Some("direct".to_string()),
                source_operator: None,
                source_asn: None,
            },
            super::ChainPeerHealthStatus {
                peer_id: "peer-b".to_string(),
                status: "suspect".to_string(),
                issues: vec!["missing_peer_record".to_string()],
                discovery_sources: vec!["dht".to_string()],
                active_path_kind: None,
                source_operator: None,
                source_asn: None,
            },
        ],
        registered_protocols: vec!["/oasis7/fetch-commit/1".to_string()],
        protocol_retry_cooldown_peers: BTreeMap::new(),
        transport_retry_cooldown_peers: vec!["peer-a".to_string()],
        request_peer_scores: BTreeMap::new(),
        connection_events: Vec::new(),
        recent_errors: vec!["request failed: Timeout".to_string()],
    };

    let payload = build_chain_status_payload(
        snapshot,
        Path::new("/tmp/execution-world"),
        None,
        None,
        &NodeNetworkPolicy::recommend_for_user_mode(
            NodeRole::Storage,
            NodeUserMode::AutoJoin,
            NodeReachabilityAutoDetection {
                observed_reachability: Some(PeerReachabilityClass::Public),
                hole_punch_viability: NodeHolePunchViability::Viable,
                relay_available: true,
                probe_stable: true,
                autonat_status: NodeAutoNatStatus::Public,
                public_port_reachability: NodePublicPortReachability::Reachable,
            },
            false,
        )
        .expect("recommendation"),
        Some("private_safe".to_string()),
        NodeNetworkPolicy {
            deployment_mode: PeerDeploymentMode::Private,
            node_role_claim: PeerNodeRole::FullStorage,
        },
        &Libp2pReachabilitySnapshot {
            active_transport_kind: Some(LiveTransportKind::RelayReserved),
            active_transport_kind_since_unix_ms: Some(1_700_000_000_000),
            last_transport_transition: Some(LiveTransportTransition {
                from_kind: Some(LiveTransportKind::Direct),
                to_kind: Some(LiveTransportKind::RelayReserved),
                at_unix_ms: 1_700_000_000_100,
            }),
            transport_transition_counters: LiveTransportTransitionCounters {
                direct_to_relay_reserved: 1,
                selected_kind_change_count: 1,
                ..LiveTransportTransitionCounters::default()
            },
            active_direct_path_count: 0,
            active_hole_punch_path_count: 0,
            active_relay_path_count: 1,
            autonat_status: LiveAutoNatStatus::Public,
            public_port_reachability: LivePublicPortReachability::Reachable,
            observed_public_addr: Some("/dns4/public.example/tcp/4001".to_string()),
            confirmed_external_direct_addrs: vec!["/dns4/public.example/tcp/4001".to_string()],
            ..Libp2pReachabilitySnapshot::default()
        },
        NodeReachabilityAutoDetection {
            observed_reachability: Some(PeerReachabilityClass::Public),
            hole_punch_viability: NodeHolePunchViability::Viable,
            relay_available: true,
            probe_stable: true,
            autonat_status: NodeAutoNatStatus::Public,
            public_port_reachability: NodePublicPortReachability::Reachable,
        },
        ReleaseSecurityPolicy::default(),
        reward_runtime,
        storage.clone(),
        sample_wasm_status(),
        None,
        super::ChainTrafficStatus {
            udp_gossip: None,
            libp2p_replication: oasis7_node::Libp2pTrafficMetricsSnapshot::default(),
        },
        sample_transfer_metrics(),
        replication,
    );

    assert_eq!(payload.storage.storage_profile, "dev_local");
    assert_eq!(payload.storage.ref_count, 5);
    assert_eq!(payload.storage.pin_count, 3);
    assert_eq!(payload.storage.checkpoint_count, 1);
    assert_eq!(
        payload.storage.effective_budget.profile,
        StorageProfile::DevLocal
    );
    assert_eq!(payload.storage.replay_summary.mode, "checkpoint_plus_log");
    assert_eq!(
        payload.storage.replay_summary.latest_retained_height,
        Some(2)
    );
    assert_eq!(payload.storage.last_gc_result, "failed");
    assert_eq!(payload.storage.last_gc_error.as_deref(), Some("gc failed"));
    assert_eq!(
        payload.storage.degraded_reason.as_deref(),
        Some("storage degraded")
    );
    assert_eq!(payload.p2p.requested_user_mode, "auto_join");
    assert_eq!(payload.p2p.recommended_user_mode, "public_entry");
    assert_eq!(payload.p2p.effective_user_mode, "private_safe");
    assert_eq!(
        payload.p2p.applied_effective_user_mode.as_deref(),
        Some("private_safe")
    );
    assert!(payload.p2p.requires_explicit_public_entry_confirmation);
    assert_eq!(payload.p2p.detected_reachability.as_deref(), Some("public"));
    assert_eq!(payload.p2p.autonat_status, "public");
    assert_eq!(payload.p2p.public_port_reachability, "reachable");
    assert_eq!(
        payload.p2p.observed_public_addr.as_deref(),
        Some("/dns4/public.example/tcp/4001")
    );
    assert_eq!(
        payload.p2p.confirmed_external_direct_addrs,
        vec!["/dns4/public.example/tcp/4001".to_string()]
    );
    assert_eq!(
        payload.p2p.active_transport_kind.as_deref(),
        Some("relay_reserved")
    );
    assert_eq!(payload.p2p.active_relay_path_count, 1);
    assert_eq!(
        payload
            .observability
            .path_observability
            .selected_path_kind
            .as_deref(),
        Some("relay_reserved")
    );
    assert_eq!(
        payload
            .observability
            .path_observability
            .transitions
            .direct_to_relay_reserved,
        1
    );
    assert_eq!(
        payload.release_security_policy,
        ReleaseSecurityPolicy::default()
    );
    assert_eq!(
        payload.traffic.libp2p_replication.scope,
        "application_payload_with_substream_wire_bytes"
    );
    assert!(
        payload
            .traffic
            .libp2p_replication
            .excludes_transport_overhead
    );
    assert_eq!(
        payload.traffic.libp2p_replication.wire_totals.inbound.bytes,
        0
    );
    assert_eq!(
        payload.traffic.libp2p_replication.control_plane.wire_scope,
        "substream_wire_bytes_minus_application_payload"
    );
    assert!(
        payload
            .traffic
            .libp2p_replication
            .control_plane
            .excludes_transport_overhead
    );
    assert!(payload.wasm.metrics_available);
    assert_eq!(payload.wasm.build.total_build_wall_ms, Some(120));
    assert_eq!(payload.wasm.executor.memory_cache_hits, 2);
    assert_eq!(
        payload.wasm.executor.module_hotspots[0].module_id,
        "m.metrics"
    );
    assert_eq!(payload.wasm.executor.module_hotspots[0].calls_total, 4);
    assert_eq!(payload.wasm.router.prepared_hits, 5);
    assert!(payload.traffic.udp_gossip.is_none());
    assert_eq!(payload.observability.status, "warn");
    assert!(!payload.observability.ready);
    assert_eq!(payload.observability.connected_peer_count, 1);
    assert_eq!(payload.observability.active_peer_count, 1);
    assert_eq!(payload.observability.suspect_peer_count, 1);
    assert_eq!(payload.observability.peer_with_issues_count, 1);
    assert_eq!(payload.observability.known_peer_heads, 1);
    assert!(payload.observability.network_head_available);
    assert_eq!(payload.observability.network_height_lag, 2);
    assert_eq!(payload.consensus.network_head.source, "peer_quorum");
    assert_eq!(payload.consensus.network_head.decision, "ready");
    assert_eq!(payload.consensus.network_head.height, Some(7));
    assert_eq!(payload.observability.recent_replication_error_count, 1);
    assert!(payload.observability.storage_degraded);
    assert!(!payload.observability.reward_runtime_degraded);
    assert_eq!(
        payload.consensus.last_committed_at_ms,
        Some(1_700_000_000_000)
    );
    assert_eq!(
        payload.consensus.last_commit_age_ms,
        Some(
            payload
                .observed_at_unix_ms
                .saturating_sub(1_700_000_000_000)
        )
    );
    let pending_proposal = payload
        .consensus
        .pending_proposal
        .as_ref()
        .expect("pending proposal");
    assert_eq!(pending_proposal.height, 6);
    assert_eq!(pending_proposal.proposer_id, "node-b");
    assert_eq!(pending_proposal.opened_at_ms, 1_699_999_999_500);
    assert_eq!(
        pending_proposal.age_ms,
        payload
            .observed_at_unix_ms
            .saturating_sub(1_699_999_999_500)
    );
    assert_eq!(pending_proposal.action_count, 2);
    assert_eq!(pending_proposal.action_payload_bytes, 384);
    assert_eq!(pending_proposal.attestation_count, 1);
    assert_eq!(pending_proposal.required_stake, 67);
    assert_eq!(pending_proposal.total_stake, 100);
    assert_eq!(pending_proposal.approval_progress_bps, 5_074);
    assert_eq!(pending_proposal.remaining_approval_stake, 33);
    assert_eq!(pending_proposal.status, "pending");
    assert_eq!(payload.consensus.recent_finality_latency.sample_count, 4);
    assert_eq!(
        payload.consensus.recent_finality_latency.avg_latency_ms,
        Some(780)
    );
    assert_eq!(
        payload.consensus.recent_finality_latency.p95_latency_ms,
        Some(1_500)
    );
    assert_eq!(
        payload
            .consensus
            .pending_consensus_actions
            .queued_action_count,
        7
    );
    assert_eq!(
        payload
            .consensus
            .pending_consensus_actions
            .queued_payload_bytes,
        1_024
    );
    assert_eq!(
        payload
            .consensus
            .pending_consensus_actions
            .reserved_requeue_action_count,
        2
    );
    assert_eq!(
        payload
            .consensus
            .pending_consensus_actions
            .reserved_requeue_payload_bytes,
        300
    );
    assert_eq!(
        payload
            .consensus
            .pending_consensus_actions
            .available_capacity,
        11
    );
    assert_eq!(payload.consensus.pending_consensus_actions.max_capacity, 20);
    assert_eq!(
        payload
            .consensus
            .pending_consensus_actions
            .submit_buffer_action_count,
        3
    );
    assert_eq!(
        payload
            .consensus
            .pending_consensus_actions
            .submit_buffer_payload_bytes,
        480
    );
    assert_eq!(
        payload
            .consensus
            .pending_consensus_actions
            .submit_buffer_max_capacity,
        64
    );
    assert_eq!(payload.transactions.tracked_records, 6);
    assert_eq!(payload.transactions.inflight_count, 2);
    assert_eq!(payload.transactions.oldest_inflight_age_ms, Some(900));
    assert_eq!(
        payload
            .transactions
            .recent_confirmation_latency
            .sample_count,
        3
    );
    assert_eq!(
        payload
            .transactions
            .recent_confirmation_latency
            .p95_latency_ms,
        Some(1_100)
    );
    assert_eq!(
        payload
            .consensus
            .inbound_timing_rejections
            .proposal_future_slot,
        3
    );
    assert_eq!(
        payload
            .consensus
            .inbound_timing_rejections
            .attestation_epoch_mismatch,
        5
    );
    assert_eq!(
        payload
            .consensus
            .inbound_timing_rejections
            .last_reason
            .as_deref(),
        Some("attestation target_epoch mismatch")
    );
    assert!(
        payload
            .observability
            .alerts
            .iter()
            .any(|alert| alert.code == "consensus_network_lag")
    );
    assert!(
        payload
            .observability
            .alerts
            .iter()
            .any(|alert| alert.code == "replication_peer_health_degraded")
    );
    assert!(
        payload
            .observability
            .alerts
            .iter()
            .any(|alert| alert.code == "replication_recent_errors")
    );
    assert!(
        payload
            .observability
            .alerts
            .iter()
            .any(|alert| alert.code == "storage_degraded")
    );
}

#[test]
#[expect(
    clippy::field_reassign_with_default,
    reason = "Test fixture construction intentionally starts from canonical defaults before overriding scenario-specific fields."
)]
fn build_chain_status_payload_marks_local_chain_ahead_of_network_head() {
    let mut consensus = NodeConsensusSnapshot::default();
    consensus.committed_height = 1_233;
    consensus.network_committed_height = 143;
    consensus.replication_persisted_height = 1_233;
    consensus.known_peer_heads = 1;
    consensus.peer_heads = vec![NodePeerCommittedHead {
        node_id: "clean-rebuilt-validator".to_string(),
        validator_id: None,
        height: 143,
        block_hash: "clean-network-block-143".to_string(),
        committed_at_ms: 1_700_000_000_000,
        observed_at_ms: 1_700_000_000_000,
        execution_block_hash: Some("clean-network-execution-143".to_string()),
        execution_state_root: Some("clean-network-state-143".to_string()),
    }];
    let snapshot = NodeSnapshot {
        node_id: "observer-with-old-state".to_string(),
        player_id: "player-with-old-state".to_string(),
        world_id: "world-with-old-state".to_string(),
        role: NodeRole::Observer,
        replication_enabled: true,
        running: true,
        tick_count: 1,
        last_tick_unix_ms: None,
        consensus,
        consensus_progress_observer_error: None,
        last_error: None,
    };
    let network_head =
        super::status_payload::build_network_head_status(&snapshot, 1_700_000_000_000, None);
    assert_eq!(network_head.height, Some(143));
    let policy = super::status_payload::readiness_policy(&snapshot, None);
    let observability = super::status_payload::build_chain_node_observability_status(
        &snapshot,
        &sample_observability_storage_metrics(),
        &sample_observability_reward_runtime_metrics(),
        &super::ChainReplicationDebugStatus::default(),
        &network_head,
        &sample_observability_p2p_status(),
        &policy,
        None,
        1_700_000_000_000,
    );

    assert_eq!(observability.status, "critical");
    assert!(
        observability
            .alerts
            .iter()
            .any(|alert| alert.code == "local_chain_ahead_of_network_head"),
        "expected local-chain-ahead diagnostic alert, got {:?}",
        observability.alerts
    );
}

#[test]
#[expect(
    clippy::field_reassign_with_default,
    reason = "Test fixture construction intentionally starts from canonical defaults before overriding scenario-specific fields."
)]
fn build_chain_status_payload_marks_peer_head_unavailable_not_ready() {
    let mut consensus = NodeConsensusSnapshot::default();
    consensus.committed_height = 10;
    consensus.network_committed_height = 10;
    consensus.replication_persisted_height = 10;
    consensus.known_peer_heads = 0;
    let snapshot = NodeSnapshot {
        node_id: "observer-without-peer-heads".to_string(),
        player_id: "player-without-peer-heads".to_string(),
        world_id: "world-without-peer-heads".to_string(),
        role: NodeRole::Observer,
        replication_enabled: true,
        running: true,
        tick_count: 1,
        last_tick_unix_ms: None,
        consensus,
        consensus_progress_observer_error: None,
        last_error: None,
    };
    let network_head =
        super::status_payload::build_network_head_status(&snapshot, 1_700_000_000_000, None);
    let policy = super::status_payload::readiness_policy(&snapshot, None);
    let observability = super::status_payload::build_chain_node_observability_status(
        &snapshot,
        &super::storage_metrics::StorageMetricsSnapshot {
            storage_profile: "dev_local".to_string(),
            effective_budget: StorageProfileConfig::from(StorageProfile::DevLocal),
            bytes_by_dir: BTreeMap::new(),
            blob_counts: BTreeMap::new(),
            ref_count: 0,
            pin_count: 0,
            retained_heights: Vec::new(),
            checkpoint_count: 0,
            replay_summary: super::storage_metrics::StorageReplaySummary::default(),
            orphan_blob_count: 0,
            last_gc_at_ms: None,
            last_gc_result: "not_available".to_string(),
            last_gc_error: None,
            degraded_reason: None,
        },
        &super::reward_runtime_worker::RewardRuntimeMetricsSnapshot {
            enabled: false,
            metrics_available: true,
            report_dir: String::new(),
            report_count: 0,
            latest_epoch_index: 0,
            latest_report_observed_at_unix_ms: 0,
            latest_total_distributed_points: 0,
            latest_minted_record_count: 0,
            cumulative_minted_record_count: 0,
            distfs_total_checks: 0,
            distfs_failed_checks: 0,
            distfs_failure_ratio: 0.0,
            settlement_apply_attempts_total: 0,
            settlement_apply_failures_total: 0,
            settlement_apply_failure_ratio: 0.0,
            invariant_ok: true,
            last_error: None,
        },
        &super::ChainReplicationDebugStatus {
            local_peer_id: "peer-local".to_string(),
            connected_peers: vec!["peer-a".to_string(), "peer-b".to_string()],
            peer_healths: vec![
                super::ChainPeerHealthStatus {
                    peer_id: "peer-a".to_string(),
                    status: "active".to_string(),
                    issues: Vec::new(),
                    discovery_sources: vec!["static_bootstrap".to_string()],
                    active_path_kind: Some("direct".to_string()),
                    source_operator: None,
                    source_asn: None,
                },
                super::ChainPeerHealthStatus {
                    peer_id: "peer-b".to_string(),
                    status: "active".to_string(),
                    issues: Vec::new(),
                    discovery_sources: vec!["static_bootstrap".to_string()],
                    active_path_kind: Some("direct".to_string()),
                    source_operator: None,
                    source_asn: None,
                },
            ],
            registered_protocols: vec![
                "/aw/node/replication/fetch-blob/1.0.0".to_string(),
                "/aw/node/replication/fetch-commit/1.0.0".to_string(),
            ],
            protocol_retry_cooldown_peers: BTreeMap::new(),
            transport_retry_cooldown_peers: Vec::new(),
            request_peer_scores: BTreeMap::new(),
            connection_events: Vec::new(),
            recent_errors: Vec::new(),
        },
        &network_head,
        &sample_observability_p2p_status(),
        &policy,
        None,
        1_700_000_000_000,
    );

    assert_eq!(observability.status, "warn");
    assert!(!observability.ready);
    assert_eq!(observability.known_peer_heads, 0);
    assert!(!observability.network_head_available);
    assert_eq!(observability.network_height_lag, 0);
    assert!(
        observability
            .alerts
            .iter()
            .any(|alert| alert.code == "consensus_peer_head_unavailable")
    );
}

#[test]
#[expect(
    clippy::field_reassign_with_default,
    reason = "Test fixture construction intentionally starts from canonical defaults before overriding scenario-specific fields."
)]
fn build_chain_status_payload_marks_stale_peer_heads_not_ready() {
    let mut consensus = NodeConsensusSnapshot::default();
    consensus.committed_height = 10;
    consensus.network_committed_height = 10;
    consensus.replication_persisted_height = 10;
    consensus.known_peer_heads = 1;
    consensus.peer_heads = vec![NodePeerCommittedHead {
        node_id: "stale-peer".to_string(),
        validator_id: None,
        height: 10,
        block_hash: "stale-block".to_string(),
        committed_at_ms: 1_000,
        observed_at_ms: 1_000,
        execution_block_hash: Some("stale-execution".to_string()),
        execution_state_root: Some("stale-state".to_string()),
    }];
    let snapshot = NodeSnapshot {
        node_id: "observer-stale-peer-heads".to_string(),
        player_id: "player-stale-peer-heads".to_string(),
        world_id: "world-stale-peer-heads".to_string(),
        role: NodeRole::Observer,
        replication_enabled: true,
        running: true,
        tick_count: 1,
        last_tick_unix_ms: None,
        consensus,
        consensus_progress_observer_error: None,
        last_error: None,
    };
    let network_head = super::status_payload::build_network_head_status(&snapshot, 20_000, None);
    assert_eq!(network_head.source, "unknown");
    assert_eq!(network_head.decision, "degraded");
    assert_eq!(network_head.fresh_peer_count, 0);
    assert_eq!(network_head.stale_peer_count, 1);
}

#[test]
#[expect(
    clippy::field_reassign_with_default,
    reason = "Test fixture construction intentionally starts from canonical defaults before overriding scenario-specific fields."
)]
fn build_chain_status_payload_marks_peer_head_hash_conflict_critical() {
    let mut consensus = NodeConsensusSnapshot::default();
    consensus.committed_height = 10;
    consensus.network_committed_height = 10;
    consensus.replication_persisted_height = 10;
    consensus.known_peer_heads = 2;
    consensus.peer_heads = vec![
        NodePeerCommittedHead {
            node_id: "peer-a".to_string(),
            validator_id: None,
            height: 10,
            block_hash: "block-a".to_string(),
            committed_at_ms: 1_000,
            observed_at_ms: 10_000,
            execution_block_hash: Some("execution-a".to_string()),
            execution_state_root: Some("state-a".to_string()),
        },
        NodePeerCommittedHead {
            node_id: "peer-b".to_string(),
            validator_id: None,
            height: 10,
            block_hash: "block-b".to_string(),
            committed_at_ms: 1_000,
            observed_at_ms: 10_000,
            execution_block_hash: Some("execution-b".to_string()),
            execution_state_root: Some("state-b".to_string()),
        },
    ];
    let snapshot = NodeSnapshot {
        node_id: "observer-conflicting-peer-heads".to_string(),
        player_id: "player-conflicting-peer-heads".to_string(),
        world_id: "world-conflicting-peer-heads".to_string(),
        role: NodeRole::Observer,
        replication_enabled: true,
        running: true,
        tick_count: 1,
        last_tick_unix_ms: None,
        consensus,
        consensus_progress_observer_error: None,
        last_error: None,
    };
    let network_head = super::status_payload::build_network_head_status(&snapshot, 10_100, None);
    assert_eq!(network_head.source, "peer_conflict");
    assert_eq!(network_head.decision, "critical");
    assert_eq!(network_head.conflicting_peer_count, 2);
}

#[test]
#[expect(
    clippy::field_reassign_with_default,
    reason = "Test fixture construction intentionally starts from canonical defaults before overriding scenario-specific fields."
)]
fn build_chain_status_payload_marks_validator_unknown_reachability_not_ready() {
    let mut consensus = NodeConsensusSnapshot::default();
    consensus.committed_height = 10;
    consensus.network_committed_height = 10;
    consensus.replication_persisted_height = 10;
    consensus.known_peer_heads = 1;
    consensus.peer_heads = vec![NodePeerCommittedHead {
        node_id: "peer-a".to_string(),
        validator_id: None,
        height: 10,
        block_hash: "block-a".to_string(),
        committed_at_ms: 1_000,
        observed_at_ms: 10_000,
        execution_block_hash: Some("execution-a".to_string()),
        execution_state_root: Some("state-a".to_string()),
    }];
    let snapshot = NodeSnapshot {
        node_id: "validator-unknown-reachability".to_string(),
        player_id: "player-validator-unknown-reachability".to_string(),
        world_id: "world-validator-unknown-reachability".to_string(),
        role: NodeRole::Sequencer,
        replication_enabled: true,
        running: true,
        tick_count: 1,
        last_tick_unix_ms: None,
        consensus,
        consensus_progress_observer_error: None,
        last_error: None,
    };
    let network_head = super::status_payload::build_network_head_status(&snapshot, 10_100, None);
    let mut p2p = sample_observability_p2p_status();
    p2p.detected_reachability = None;
    p2p.autonat_status = "unknown".to_string();
    p2p.public_port_reachability = "unknown".to_string();
    p2p.observed_public_addr = None;
    p2p.confirmed_external_direct_addrs = Vec::new();
    p2p.relay_available = false;
    let policy = super::status_payload::readiness_policy(&snapshot, None);
    let status = super::status_payload::build_chain_node_observability_status(
        &snapshot,
        &super::storage_metrics::StorageMetricsSnapshot {
            storage_profile: "dev_local".to_string(),
            effective_budget: StorageProfileConfig::from(StorageProfile::DevLocal),
            bytes_by_dir: BTreeMap::new(),
            blob_counts: BTreeMap::new(),
            ref_count: 0,
            pin_count: 0,
            retained_heights: Vec::new(),
            checkpoint_count: 0,
            replay_summary: super::storage_metrics::StorageReplaySummary::default(),
            orphan_blob_count: 0,
            last_gc_at_ms: None,
            last_gc_result: "not_available".to_string(),
            last_gc_error: None,
            degraded_reason: None,
        },
        &super::reward_runtime_worker::RewardRuntimeMetricsSnapshot {
            enabled: false,
            metrics_available: true,
            report_dir: String::new(),
            report_count: 0,
            latest_epoch_index: 0,
            latest_report_observed_at_unix_ms: 0,
            latest_total_distributed_points: 0,
            latest_minted_record_count: 0,
            cumulative_minted_record_count: 0,
            distfs_total_checks: 0,
            distfs_failed_checks: 0,
            distfs_failure_ratio: 0.0,
            settlement_apply_attempts_total: 0,
            settlement_apply_failures_total: 0,
            settlement_apply_failure_ratio: 0.0,
            invariant_ok: true,
            last_error: None,
        },
        &super::ChainReplicationDebugStatus {
            local_peer_id: "peer-local".to_string(),
            connected_peers: vec!["peer-a".to_string()],
            peer_healths: vec![super::ChainPeerHealthStatus {
                peer_id: "peer-a".to_string(),
                status: "active".to_string(),
                issues: Vec::new(),
                discovery_sources: vec!["static_bootstrap".to_string()],
                active_path_kind: Some("direct".to_string()),
                source_operator: None,
                source_asn: None,
            }],
            registered_protocols: Vec::new(),
            protocol_retry_cooldown_peers: BTreeMap::new(),
            transport_retry_cooldown_peers: Vec::new(),
            request_peer_scores: BTreeMap::new(),
            connection_events: Vec::new(),
            recent_errors: Vec::new(),
        },
        &network_head,
        &p2p,
        &policy,
        None,
        10_100,
    );

    assert_eq!(status.status, "warn");
    assert!(!status.ready);
    assert!(!status.reachability_policy_ok);
    assert!(
        status
            .alerts
            .iter()
            .any(|alert| alert.code == "p2p_reachability_degraded")
    );
}

#[test]
fn build_chain_status_payload_includes_storage_metrics() {
    assert_chain_status_payload_consensus_health_metrics();
}

#[test]
fn build_chain_status_payload_surfaces_consensus_health_metrics() {
    assert_chain_status_payload_consensus_health_metrics();
}

#[test]
#[expect(
    clippy::field_reassign_with_default,
    reason = "Test fixture construction intentionally starts from canonical defaults before overriding scenario-specific fields."
)]
fn build_chain_status_payload_clamps_future_ages_to_zero() {
    let mut consensus = NodeConsensusSnapshot::default();
    consensus.last_committed_at_ms = Some(i64::MAX);
    consensus.pending_proposal = Some(NodePendingProposalSnapshot {
        height: 1,
        slot: 1,
        epoch: 0,
        proposer_id: "node-z".to_string(),
        opened_at_ms: i64::MAX,
        action_count: 0,
        action_payload_bytes: 0,
        attestation_count: 0,
        approved_stake: 0,
        rejected_stake: 0,
        required_stake: 1,
        total_stake: 1,
        approval_progress_bps: 0,
        rejection_progress_bps: 0,
        remaining_approval_stake: 1,
        status: PosConsensusStatus::Pending,
    });
    let snapshot = NodeSnapshot {
        node_id: "node-future".to_string(),
        player_id: "player-future".to_string(),
        world_id: "world-future".to_string(),
        role: NodeRole::Observer,
        replication_enabled: false,
        running: true,
        tick_count: 1,
        last_tick_unix_ms: None,
        consensus,
        consensus_progress_observer_error: None,
        last_error: None,
    };
    let recommendation = NodeNetworkPolicy::recommend_for_user_mode(
        NodeRole::Observer,
        NodeUserMode::PrivateSafe,
        NodeReachabilityAutoDetection::default(),
        false,
    )
    .expect("recommendation");
    let reward_runtime = super::reward_runtime_worker::RewardRuntimeMetricsSnapshot {
        enabled: true,
        metrics_available: true,
        report_dir: "/tmp/reports".to_string(),
        report_count: 0,
        latest_epoch_index: 0,
        latest_report_observed_at_unix_ms: 0,
        latest_total_distributed_points: 0,
        latest_minted_record_count: 0,
        cumulative_minted_record_count: 0,
        distfs_total_checks: 0,
        distfs_failed_checks: 0,
        distfs_failure_ratio: 0.0,
        settlement_apply_attempts_total: 0,
        settlement_apply_failures_total: 0,
        settlement_apply_failure_ratio: 0.0,
        invariant_ok: true,
        last_error: None,
    };
    let storage = super::storage_metrics::StorageMetricsSnapshot {
        storage_profile: "dev_local".to_string(),
        effective_budget: StorageProfileConfig::from(StorageProfile::DevLocal),
        bytes_by_dir: BTreeMap::new(),
        blob_counts: BTreeMap::new(),
        ref_count: 0,
        pin_count: 0,
        retained_heights: Vec::new(),
        checkpoint_count: 0,
        replay_summary: super::storage_metrics::StorageReplaySummary::default(),
        orphan_blob_count: 0,
        last_gc_at_ms: None,
        last_gc_result: "not_available".to_string(),
        last_gc_error: None,
        degraded_reason: None,
    };

    let payload = build_chain_status_payload(
        snapshot,
        Path::new("/tmp/execution-world"),
        None,
        None,
        &recommendation,
        None,
        NodeNetworkPolicy {
            deployment_mode: PeerDeploymentMode::Private,
            node_role_claim: PeerNodeRole::ObserverLight,
        },
        &Libp2pReachabilitySnapshot::default(),
        NodeReachabilityAutoDetection::default(),
        ReleaseSecurityPolicy::default(),
        reward_runtime,
        storage,
        sample_wasm_status(),
        None,
        super::ChainTrafficStatus {
            udp_gossip: None,
            libp2p_replication: oasis7_node::Libp2pTrafficMetricsSnapshot::default(),
        },
        super::transfer_submit_api::ChainTransferMetricsStatus {
            tracked_records: 0,
            accepted_count: 0,
            pending_count: 0,
            confirmed_count: 0,
            failed_count: 0,
            timeout_count: 0,
            inflight_count: 0,
            oldest_inflight_age_ms: None,
            recent_confirmation_latency:
                super::transfer_submit_api::ChainTransferLatencySummaryStatus {
                    sample_count: 0,
                    avg_latency_ms: None,
                    max_latency_ms: None,
                    p50_latency_ms: None,
                    p95_latency_ms: None,
                },
        },
        super::ChainReplicationDebugStatus::default(),
    );

    assert_eq!(payload.consensus.last_commit_age_ms, Some(0));
    assert_eq!(
        payload
            .consensus
            .pending_proposal
            .as_ref()
            .expect("pending proposal")
            .age_ms,
        0
    );
}
