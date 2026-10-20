use oasis7::geometry::GeoPos;
use oasis7_node::{
    NodeConsensusMisbehaviorEvidenceSnapshot, NodeConsensusSlashingIntentSnapshot,
    NodeConsensusSlashingReceiptSnapshot, NodeConsensusSnapshot, NodeRole, NodeSnapshot,
};
use oasis7_proto::storage_profile::{StorageProfile, StorageProfileConfig};
use oasis7_runtime::{Action, GovernanceIdentityStatus, World};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn sample_slashing_p2p_status() -> super::status_payload::ChainP2pStatus {
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

fn slashing_storage_metrics() -> super::storage_metrics::StorageMetricsSnapshot {
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

fn slashing_reward_metrics() -> super::reward_runtime_worker::RewardRuntimeMetricsSnapshot {
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

fn slashing_temp_dir(prefix: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("oasis7-chain-runtime-{prefix}-{unique}"))
}

fn register_slashing_agent(world: &mut World, agent_id: &str) {
    world.submit_action(Action::RegisterAgent {
        agent_id: agent_id.to_string(),
        pos: GeoPos::new(0, 0, 0),
    });
    world.step().expect("register observability test agent");
}

#[test]
#[expect(
    clippy::field_reassign_with_default,
    reason = "Test fixture construction intentionally starts from canonical defaults before overriding scenario-specific fields."
)]
fn build_chain_status_payload_marks_consensus_misbehavior_critical() {
    let mut consensus = NodeConsensusSnapshot::default();
    consensus.committed_height = 8;
    consensus.network_committed_height = 8;
    consensus.misbehavior_evidence = vec![NodeConsensusMisbehaviorEvidenceSnapshot {
        kind: "commit_equivocation".to_string(),
        evidence_hash: "evidence-hash".to_string(),
        validator_id: "node-a".to_string(),
        node_id: "node-a".to_string(),
        height: 8,
        observed_at_ms: 1_700_000_000_000,
        first_block_hash: "block-a".to_string(),
        second_block_hash: "block-b".to_string(),
        first_execution_block_hash: Some("exec-a".to_string()),
        second_execution_block_hash: Some("exec-b".to_string()),
        first_execution_state_root: Some("state-a".to_string()),
        second_execution_state_root: Some("state-b".to_string()),
        first_action_root: "action-a".to_string(),
        second_action_root: "action-b".to_string(),
        first_public_key_hex: None,
        second_public_key_hex: None,
        first_signature_hex: None,
        second_signature_hex: None,
        slashable_stake: 60,
        total_stake: 100,
        validator_stake_root: "stake-root".to_string(),
        quarantined: true,
    }];
    consensus.quarantined_validators = vec!["node-a".to_string()];
    consensus.slashing_intents = vec![NodeConsensusSlashingIntentSnapshot {
        intent_id: "intent-1".to_string(),
        evidence_hash: "evidence-hash".to_string(),
        kind: "commit_equivocation".to_string(),
        validator_id: "node-a".to_string(),
        target_agent_id: "node-a".to_string(),
        reason: "consensus commit equivocation".to_string(),
        slash_stake: 60,
        appeal_window_ticks: 32,
        validator_stake_root: "stake-root".to_string(),
        governance_method: "apply_identity_penalty".to_string(),
        status: "pending_governance_submission".to_string(),
        enforced: false,
    }];
    let snapshot = NodeSnapshot {
        node_id: "node-b".to_string(),
        player_id: "player-b".to_string(),
        world_id: "world-misbehavior".to_string(),
        role: NodeRole::Sequencer,
        replication_enabled: false,
        running: true,
        tick_count: 1,
        last_tick_unix_ms: Some(1_700_000_000_000),
        consensus,
        consensus_progress_observer_error: None,
        last_error: None,
    };
    let network_head =
        super::status_payload::build_network_head_status(&snapshot, 1_700_000_000_000, None);
    let policy = super::status_payload::readiness_policy(&snapshot, None);
    let storage = slashing_storage_metrics();
    let reward = slashing_reward_metrics();
    let status = super::status_payload::build_chain_node_observability_status(
        &snapshot,
        &storage,
        &reward,
        &super::ChainReplicationDebugStatus::default(),
        &network_head,
        &sample_slashing_p2p_status(),
        &policy,
        None,
        1_700_000_000_000,
    );

    assert_eq!(status.status, "critical");
    assert!(!status.ready);
    assert_eq!(status.misbehavior_evidence_count, 1);
    assert_eq!(status.slashing_intent_count, 1);
    assert_eq!(status.pending_slashing_intent_count, 1);
    assert_eq!(status.quarantined_validator_count, 1);
    assert_eq!(status.slashable_stake_total, 60);
    assert!(
        status
            .alerts
            .iter()
            .any(|alert| alert.code == "consensus_misbehavior_evidence_present")
    );
    assert!(
        status
            .alerts
            .iter()
            .any(|alert| alert.code == "consensus_validator_quarantined")
    );
    assert!(
        status
            .alerts
            .iter()
            .any(|alert| alert.code == "consensus_slashing_intent_pending")
    );

    let mut receipted_snapshot = snapshot.clone();
    receipted_snapshot.consensus.slashing_receipts = vec![NodeConsensusSlashingReceiptSnapshot {
        penalty_id: 42,
        intent_id: "intent-1".to_string(),
        evidence_hash: "evidence-hash".to_string(),
        validator_id: "node-a".to_string(),
        target_agent_id: "node-a".to_string(),
        slash_stake: 60,
        status: "applied".to_string(),
        evidence_chain_hash: "chain-hash".to_string(),
        appeal_deadline_tick: 32,
        applied: true,
    }];
    let receipted_network_head = super::status_payload::build_network_head_status(
        &receipted_snapshot,
        1_700_000_000_000,
        None,
    );
    let receipted_status = super::status_payload::build_chain_node_observability_status(
        &receipted_snapshot,
        &storage,
        &reward,
        &super::ChainReplicationDebugStatus::default(),
        &receipted_network_head,
        &sample_slashing_p2p_status(),
        &policy,
        None,
        1_700_000_000_000,
    );
    assert_eq!(receipted_status.slashing_receipt_count, 1);
    assert_eq!(receipted_status.applied_slashing_receipt_count, 1);
    assert_eq!(receipted_status.pending_slashing_intent_count, 0);
    assert!(
        !receipted_status
            .alerts
            .iter()
            .any(|alert| alert.code == "consensus_slashing_intent_pending")
    );
}

#[test]
fn status_server_attaches_governance_slashing_receipts_from_execution_world() {
    let world_dir = slashing_temp_dir("slashing-receipt-world");
    let mut world = oasis7::runtime_host::world::new();
    register_slashing_agent(&mut world, "node-a");
    world
        .set_governance_identity_profile("node-a", 100, 0, GovernanceIdentityStatus::Active)
        .expect("set identity profile");
    let penalty_id = world
        .apply_identity_penalty(
            "node-a",
            "evidence-hash",
            "consensus commit equivocation",
            60,
            32,
            "guardian-1",
            vec![
                "governance.local.finality.signer.1".to_string(),
                "governance.local.finality.signer.2".to_string(),
            ],
        )
        .expect("apply identity penalty");
    world.save_to_dir(&world_dir).expect("save execution world");

    let mut snapshot = NodeSnapshot {
        node_id: "node-b".to_string(),
        player_id: "player-b".to_string(),
        world_id: "world-misbehavior".to_string(),
        role: NodeRole::Sequencer,
        replication_enabled: false,
        running: true,
        tick_count: 1,
        last_tick_unix_ms: Some(1_700_000_000_000),
        consensus: NodeConsensusSnapshot {
            slashing_intents: vec![NodeConsensusSlashingIntentSnapshot {
                intent_id: "intent-1".to_string(),
                evidence_hash: "evidence-hash".to_string(),
                kind: "commit_equivocation".to_string(),
                validator_id: "node-a".to_string(),
                target_agent_id: "node-a".to_string(),
                reason: "consensus commit equivocation".to_string(),
                slash_stake: 60,
                appeal_window_ticks: 32,
                validator_stake_root: "stake-root".to_string(),
                governance_method: "apply_identity_penalty".to_string(),
                status: "pending_governance_submission".to_string(),
                enforced: false,
            }],
            ..NodeConsensusSnapshot::default()
        },
        consensus_progress_observer_error: None,
        last_error: None,
    };

    super::status_server_support::attach_governance_slashing_receipts(
        &mut snapshot,
        world_dir.as_path(),
    );

    assert_eq!(snapshot.consensus.slashing_receipts.len(), 1);
    let receipt = &snapshot.consensus.slashing_receipts[0];
    assert_eq!(receipt.penalty_id, penalty_id);
    assert_eq!(receipt.intent_id, "intent-1");
    assert_eq!(receipt.evidence_hash, "evidence-hash");
    assert_eq!(receipt.validator_id, "node-a");
    assert_eq!(receipt.target_agent_id, "node-a");
    assert_eq!(receipt.slash_stake, 60);
    assert_eq!(receipt.status, "applied");
    assert_eq!(receipt.appeal_deadline_tick, 33);
    assert!(receipt.applied);
    assert!(!receipt.evidence_chain_hash.is_empty());
}

#[test]
fn status_server_treats_accepted_slashing_appeal_as_resolved_receipt() {
    let world_dir = slashing_temp_dir("slashing-receipt-appeal-accepted-world");
    let mut world = oasis7::runtime_host::world::new();
    register_slashing_agent(&mut world, "node-a");
    world
        .set_governance_identity_profile("node-a", 100, 0, GovernanceIdentityStatus::Active)
        .expect("set identity profile");
    let penalty_id = world
        .apply_identity_penalty(
            "node-a",
            "evidence-hash",
            "consensus commit equivocation",
            60,
            32,
            "guardian-1",
            vec![
                "governance.local.finality.signer.1".to_string(),
                "governance.local.finality.signer.2".to_string(),
            ],
        )
        .expect("apply identity penalty");
    world
        .appeal_identity_penalty(penalty_id, "node-a", "counter evidence")
        .expect("appeal identity penalty");
    world
        .resolve_identity_penalty_appeal(penalty_id, "committee", true, "appeal accepted")
        .expect("resolve appeal");
    world.save_to_dir(&world_dir).expect("save execution world");

    let mut snapshot = NodeSnapshot {
        node_id: "node-b".to_string(),
        player_id: "player-b".to_string(),
        world_id: "world-misbehavior".to_string(),
        role: NodeRole::Sequencer,
        replication_enabled: false,
        running: true,
        tick_count: 1,
        last_tick_unix_ms: Some(1_700_000_000_000),
        consensus: NodeConsensusSnapshot {
            slashing_intents: vec![NodeConsensusSlashingIntentSnapshot {
                intent_id: "intent-1".to_string(),
                evidence_hash: "evidence-hash".to_string(),
                kind: "commit_equivocation".to_string(),
                validator_id: "node-a".to_string(),
                target_agent_id: "node-a".to_string(),
                reason: "consensus commit equivocation".to_string(),
                slash_stake: 60,
                appeal_window_ticks: 32,
                validator_stake_root: "stake-root".to_string(),
                governance_method: "apply_identity_penalty".to_string(),
                status: "pending_governance_submission".to_string(),
                enforced: false,
            }],
            ..NodeConsensusSnapshot::default()
        },
        consensus_progress_observer_error: None,
        last_error: None,
    };

    super::status_server_support::attach_governance_slashing_receipts(
        &mut snapshot,
        world_dir.as_path(),
    );

    assert_eq!(snapshot.consensus.slashing_receipts.len(), 1);
    let receipt = &snapshot.consensus.slashing_receipts[0];
    assert_eq!(receipt.status, "appeal_accepted");
    assert!(receipt.applied);
    assert_eq!(
        super::status_payload::pending_slashing_intent_count(&snapshot),
        0
    );
}
