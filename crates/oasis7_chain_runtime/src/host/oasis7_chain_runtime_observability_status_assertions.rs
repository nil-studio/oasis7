use super::*;

pub(super) fn assert_storage_status_metrics(
    payload: &super::super::status_payload::ChainStatusResponse,
) {
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
}

pub(super) fn assert_network_path_status(
    payload: &super::super::status_payload::ChainStatusResponse,
) {
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
}

pub(super) fn assert_runtime_and_node_health_status(
    payload: &super::super::status_payload::ChainStatusResponse,
) {
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
}

pub(super) fn assert_consensus_queue_and_alert_status(
    payload: &super::super::status_payload::ChainStatusResponse,
) {
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
