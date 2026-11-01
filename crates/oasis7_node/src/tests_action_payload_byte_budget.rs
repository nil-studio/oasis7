use super::*;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, Instant};

// RED contract: one configured byte budget must cover the public submit buffer
// and engine pending map together; the existing count/per-action limits remain
// independent safety checks.
struct ByteBudgetExecutionHook;

fn accepted_action_payload() -> Vec<u8> {
    let payload = serde_cbor::to_vec(&serde_json::json!({"aaaaaaa": "bbbbbb"}))
        .expect("encode accepted legacy runtime action payload");
    assert_eq!(
        payload.len(),
        16,
        "fixture must stay within the 16-byte limit"
    );
    payload
}

impl NodeExecutionHook for ByteBudgetExecutionHook {
    fn on_commit(
        &mut self,
        context: NodeExecutionCommitContext,
    ) -> Result<NodeExecutionCommitResult, String> {
        Ok(NodeExecutionCommitResult {
            execution_height: context.height,
            execution_block_hash: format!("execution-block-{}", context.height),
            execution_state_root: format!("execution-state-{}", context.height),
        })
    }
}

#[test]
fn runtime_public_action_queue_enforces_aggregate_payload_byte_budget() {
    const QUEUE_BYTE_BUDGET: usize = 24;
    const ACTION_BYTES: usize = 16;
    let config = NodeConfig::new(
        "node-public-byte-budget",
        "world-public-byte-budget",
        NodeRole::Observer,
    )
    .expect("config")
    .with_max_pending_consensus_actions(8)
    .expect("pending action count")
    .with_max_consensus_action_payload_bytes(ACTION_BYTES)
    .expect("per-action payload limit")
    .with_max_pending_consensus_action_queue_bytes(QUEUE_BYTE_BUDGET)
    .expect("aggregate public+engine byte budget");
    let runtime = NodeRuntime::new(config);
    runtime
        .submit_consensus_action_payload(1, accepted_action_payload())
        .expect("first action should fit aggregate budget");
    let err = runtime
        .submit_consensus_action_payload(2, accepted_action_payload())
        .expect_err("projected public queue bytes must be rejected before exceeding budget");
    assert!(
        err.to_string().contains("byte budget"),
        "unexpected aggregate queue rejection: {err}"
    );
    assert_eq!(
        runtime
            .snapshot()
            .consensus
            .pending_consensus_actions
            .submit_buffer_payload_bytes,
        ACTION_BYTES,
        "rejected action must not be retained in the public queue"
    );
}

#[test]
fn pos_engine_action_queue_enforces_aggregate_payload_byte_budget() {
    const QUEUE_BYTE_BUDGET: usize = 24;
    const ACTION_BYTES: usize = 16;
    let config = NodeConfig::new(
        "node-engine-byte-budget",
        "world-engine-byte-budget",
        NodeRole::Observer,
    )
    .expect("config")
    .with_max_engine_pending_consensus_actions(8)
    .expect("engine action count")
    .with_max_consensus_action_payload_bytes(ACTION_BYTES)
    .expect("per-action payload limit")
    .with_max_pending_consensus_action_queue_bytes(QUEUE_BYTE_BUDGET)
    .expect("aggregate public+engine byte budget");
    let mut engine = PosNodeEngine::new(&config).expect("engine");
    let incoming = (1..=2)
        .map(|action_id| {
            NodeConsensusAction::from_payload(
                action_id,
                config.player_id.clone(),
                vec![action_id as u8; ACTION_BYTES],
            )
            .expect("action")
        })
        .collect();
    let err = engine
        .tick(
            &config.node_id,
            &config.world_id,
            1_000,
            None,
            None,
            None,
            None,
            incoming,
            None,
        )
        .expect_err("projected engine queue bytes must be rejected before exceeding budget");
    assert!(
        err.to_string().contains("byte budget"),
        "unexpected aggregate engine queue rejection: {err}"
    );
}

#[test]
fn runtime_retained_committed_batches_enforce_aggregate_payload_byte_budget() {
    // The retained window must reject/drop before projected raw action payload
    // bytes exceed this budget, while preserving the existing count window.
    const RETAINED_BYTE_BUDGET: usize = 24;
    const ACTION_BYTES: usize = 16;
    let config = NodeConfig::new(
        "node-retained-byte-budget",
        "world-retained-byte-budget",
        NodeRole::Sequencer,
    )
    .expect("config")
    .with_tick_interval(Duration::from_millis(10))
    .expect("tick interval")
    .with_max_engine_pending_consensus_actions(1)
    .expect("engine action count")
    .with_max_consensus_action_payload_bytes(ACTION_BYTES)
    .expect("per-action payload limit")
    .with_max_committed_action_batches(8)
    .expect("retained batch count")
    .with_max_committed_action_batch_bytes(RETAINED_BYTE_BUDGET)
    .expect("aggregate retained batch byte budget");
    let mut runtime = NodeRuntime::new(config).with_execution_hook(ByteBudgetExecutionHook);
    for action_id in 1..=4 {
        runtime
            .submit_consensus_action_payload(action_id, accepted_action_payload())
            .expect("submit action");
    }

    runtime.start().expect("start");
    let deadline = Instant::now() + Duration::from_secs(2);
    while runtime.snapshot().consensus.committed_height < 4 && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }
    runtime.stop().expect("stop");

    let batches = runtime.drain_committed_action_batches();
    let retained_payload_bytes: usize = batches
        .iter()
        .flat_map(|batch| batch.actions.iter())
        .map(|action| action.payload_cbor.len())
        .sum();
    assert!(
        retained_payload_bytes <= RETAINED_BYTE_BUDGET,
        "retained committed payload bytes exceeded budget: bytes={retained_payload_bytes} budget={RETAINED_BYTE_BUDGET}"
    );
}

#[test]
fn pending_proposal_handoff_keeps_aggregate_reservation_until_payload_drop() {
    const ACTION_BYTES: usize = 16;
    const QUEUE_BYTE_BUDGET: usize = ACTION_BYTES;
    let config = NodeConfig::new(
        "node-pending-handoff-byte-budget",
        "world-pending-handoff-byte-budget",
        NodeRole::Observer,
    )
    .expect("config")
    .with_max_engine_pending_consensus_actions(8)
    .expect("engine action count")
    .with_max_consensus_action_payload_bytes(ACTION_BYTES)
    .expect("per-action payload limit")
    .with_max_pending_consensus_action_queue_bytes(QUEUE_BYTE_BUDGET)
    .expect("aggregate byte budget");
    let mut engine = PosNodeEngine::new(&config).expect("engine");
    let action =
        NodeConsensusAction::from_payload(1, config.player_id.clone(), accepted_action_payload())
            .expect("action");
    merge_pending_consensus_actions_with_budget(
        &mut engine.pending_consensus_actions,
        vec![action.clone()],
        engine.max_pending_consensus_actions,
        engine.pending_consensus_action_queue_bytes.as_ref(),
        engine.max_pending_consensus_action_queue_bytes,
        false,
    )
    .expect("initial action reservation");

    let committed_actions = engine
        .drain_proposable_consensus_actions(1, None)
        .expect("proposal handoff");
    assert_eq!(committed_actions, vec![action.clone()]);
    engine.pending = Some(PendingProposal {
        height: 1,
        slot: 0,
        epoch: 0,
        opened_at_ms: 1,
        proposer_id: config.node_id.clone(),
        block_hash: "pending-byte-budget-block".to_string(),
        action_root: compute_consensus_action_root(&committed_actions).expect("action root"),
        committed_actions,
        attestations: BTreeMap::new(),
        approved_stake: 0,
        rejected_stake: 0,
        status: PosConsensusStatus::Pending,
    });

    let next =
        NodeConsensusAction::from_payload(2, config.player_id.clone(), accepted_action_payload())
            .expect("next action");
    let err = merge_pending_consensus_actions_with_budget(
        &mut engine.pending_consensus_actions,
        vec![next],
        engine.max_pending_consensus_actions,
        engine.pending_consensus_action_queue_bytes.as_ref(),
        engine.max_pending_consensus_action_queue_bytes,
        false,
    )
    .expect_err("pending proposal payload must remain charged until dropped");
    assert!(
        err.to_string().contains("byte budget"),
        "unexpected handoff rejection: {err}"
    );
    assert_eq!(
        engine
            .pending_consensus_action_queue_bytes
            .load(Ordering::Acquire),
        ACTION_BYTES,
        "proposal handoff must retain the aggregate reservation"
    );
}

#[test]
fn pending_merge_conflict_rolls_back_map_and_reservation_atomically() {
    const ACTION_BYTES: usize = 16;
    let config = NodeConfig::new(
        "node-atomic-merge-byte-budget",
        "world-atomic-merge-byte-budget",
        NodeRole::Observer,
    )
    .expect("config")
    .with_max_engine_pending_consensus_actions(8)
    .expect("engine action count")
    .with_max_consensus_action_payload_bytes(ACTION_BYTES)
    .expect("per-action payload limit");
    let mut pending = BTreeMap::new();
    let existing =
        NodeConsensusAction::from_payload(1, config.player_id.clone(), vec![0x11; ACTION_BYTES])
            .expect("existing action");
    pending.insert(existing.action_id, existing.clone());
    let queue_bytes = AtomicUsize::new(ACTION_BYTES);
    let new_action =
        NodeConsensusAction::from_payload(2, config.player_id.clone(), vec![0x22; ACTION_BYTES])
            .expect("new action");
    let conflicting =
        NodeConsensusAction::from_payload(1, config.player_id, vec![0x33; ACTION_BYTES])
            .expect("conflicting action");

    let err = merge_pending_consensus_actions_with_budget(
        &mut pending,
        vec![new_action, conflicting],
        8,
        &queue_bytes,
        ACTION_BYTES * 3,
        false,
    )
    .expect_err("conflicting merge must fail");
    assert!(
        err.to_string().contains("conflicting consensus action"),
        "unexpected conflict error: {err}"
    );
    assert_eq!(
        pending,
        BTreeMap::from([(existing.action_id, existing)]),
        "failed merge must not leave a partially inserted action"
    );
    assert_eq!(
        queue_bytes.load(Ordering::Acquire),
        ACTION_BYTES,
        "failed merge must restore the pre-merge reservation"
    );
}

#[test]
fn incoming_reserved_validation_failure_releases_reserved_bytes() {
    const ACTION_BYTES: usize = 16;
    let config = NodeConfig::new(
        "node-validation-reservation-byte-budget",
        "world-validation-reservation-byte-budget",
        NodeRole::Observer,
    )
    .expect("config");
    let mut malformed =
        NodeConsensusAction::from_payload(1, config.player_id, vec![0x44; ACTION_BYTES])
            .expect("action");
    malformed.payload_hash = "tampered-payload-hash".to_string();
    let mut pending = BTreeMap::new();
    let queue_bytes = AtomicUsize::new(ACTION_BYTES);

    let err = merge_pending_consensus_actions_with_budget(
        &mut pending,
        vec![malformed],
        8,
        &queue_bytes,
        ACTION_BYTES * 2,
        true,
    )
    .expect_err("malformed pre-reserved action must fail validation");
    assert!(
        err.to_string().contains("payload hash mismatch"),
        "unexpected validation error: {err}"
    );
    assert_eq!(
        queue_bytes.load(Ordering::Acquire),
        0,
        "validation failure must release incoming_already_reserved bytes"
    );
    assert!(
        pending.is_empty(),
        "invalid action must not enter pending map"
    );
}

#[test]
fn incoming_reserved_byte_budget_failure_releases_reserved_bytes() {
    const ACTION_BYTES: usize = 16;
    let config = NodeConfig::new(
        "node-byte-budget-reservation-failure",
        "world-byte-budget-reservation-failure",
        NodeRole::Observer,
    )
    .expect("config");
    let action = NodeConsensusAction::from_payload(1, config.player_id, vec![0x55; ACTION_BYTES])
        .expect("action");
    let mut pending = BTreeMap::new();
    let queue_bytes = AtomicUsize::new(ACTION_BYTES);

    let err = merge_pending_consensus_actions_with_budget(
        &mut pending,
        vec![action],
        8,
        &queue_bytes,
        ACTION_BYTES / 2,
        true,
    )
    .expect_err("invalid pre-reserved byte budget must fail closed");
    assert!(
        err.to_string().contains("reservation invalid"),
        "unexpected reservation error: {err}"
    );
    assert_eq!(
        queue_bytes.load(Ordering::Acquire),
        0,
        "byte-budget failure must release incoming_already_reserved bytes"
    );
    assert!(
        pending.is_empty(),
        "failed reservation must not mutate pending map"
    );
}

fn proposal_budget_config(world_id: &str, queue_bytes: usize) -> NodeConfig {
    NodeConfig::new("node-a", world_id, NodeRole::Observer)
        .expect("config")
        .with_pos_validators(vec![
            PosValidator {
                validator_id: "node-a".to_string(),
                stake: 60,
            },
            PosValidator {
                validator_id: "node-b".to_string(),
                stake: 40,
            },
        ])
        .expect("validators")
        .with_max_consensus_action_payload_bytes(16)
        .expect("per-action payload limit")
        .with_max_pending_consensus_action_queue_bytes(queue_bytes)
        .expect("aggregate byte budget")
}

fn proposal_action(
    config: &NodeConfig,
    action_id: u64,
    payload_bytes: usize,
) -> NodeConsensusAction {
    NodeConsensusAction::from_payload(
        action_id,
        config.player_id.clone(),
        vec![action_id as u8; payload_bytes],
    )
    .expect("proposal action")
}

fn gossip_proposal(
    config: &NodeConfig,
    proposer_id: &str,
    height: u64,
    block_hash: &str,
    actions: Vec<NodeConsensusAction>,
) -> GossipProposalMessage {
    GossipProposalMessage {
        version: 1,
        world_id: config.world_id.clone(),
        node_id: proposer_id.to_string(),
        player_id: proposer_id.to_string(),
        proposer_id: proposer_id.to_string(),
        height,
        slot: 0,
        epoch: 0,
        block_hash: block_hash.to_string(),
        action_root: compute_consensus_action_root(&actions).expect("proposal action root"),
        actions,
        proposed_at_ms: 1_000,
        public_key_hex: None,
        signature_hex: None,
    }
}

fn pending_proposal(
    _config: &NodeConfig,
    proposer_id: &str,
    height: u64,
    block_hash: &str,
    actions: Vec<NodeConsensusAction>,
) -> PendingProposal {
    PendingProposal {
        height,
        slot: 0,
        epoch: 0,
        opened_at_ms: 1_000,
        proposer_id: proposer_id.to_string(),
        block_hash: block_hash.to_string(),
        action_root: compute_consensus_action_root(&actions).expect("pending action root"),
        committed_actions: actions,
        attestations: BTreeMap::new(),
        approved_stake: 0,
        rejected_stake: 0,
        status: PosConsensusStatus::Pending,
    }
}

#[test]
fn inbound_proposal_budget_failure_rejects_without_pending_or_reservation_mutation() {
    let config = proposal_budget_config("world-inbound-proposal-budget", 16);
    let mut engine = PosNodeEngine::new(&config).expect("engine");
    let message = gossip_proposal(
        &config,
        "node-a",
        1,
        "inbound-over-budget",
        vec![
            proposal_action(&config, 1, 16),
            proposal_action(&config, 2, 16),
        ],
    );

    let _ = engine.ingest_proposal_message(&config.world_id, &message, 0);

    assert!(
        engine.pending.is_none(),
        "over-budget inbound proposal must not become pending"
    );
    assert_eq!(engine.next_height, 1, "rejection must not advance height");
    assert_eq!(
        engine
            .pending_consensus_action_queue_bytes
            .load(Ordering::Acquire),
        0,
        "rejection must not reserve inbound proposal bytes"
    );
}

#[test]
fn higher_inbound_proposal_transfers_exact_pending_reservation() {
    let config = proposal_budget_config("world-higher-proposal-transfer", 32);
    let mut engine = PosNodeEngine::new(&config).expect("engine");
    let old_actions = vec![proposal_action(&config, 1, 16)];
    reserve_action_payload_bytes(
        &engine.pending_consensus_action_queue_bytes,
        engine.max_pending_consensus_action_queue_bytes,
        16,
    )
    .expect("old pending reservation");
    engine.pending = Some(pending_proposal(
        &config,
        "node-a",
        1,
        "old-pending",
        old_actions,
    ));
    let replacement = gossip_proposal(
        &config,
        "node-b",
        2,
        "higher-pending",
        vec![
            proposal_action(&config, 2, 16),
            proposal_action(&config, 3, 16),
        ],
    );

    let _ = engine.ingest_proposal_message(&config.world_id, &replacement, 0);

    assert_eq!(
        engine
            .pending
            .as_ref()
            .map(|pending| pending.block_hash.as_str()),
        Some("higher-pending"),
        "valid higher proposal should replace the old proposal"
    );
    assert_eq!(
        engine
            .pending_consensus_action_queue_bytes
            .load(Ordering::Acquire),
        32,
        "replacement must release old bytes and reserve the new proposal exactly"
    );
}

#[test]
fn over_budget_higher_proposal_preserves_old_pending_and_reservation() {
    let config = proposal_budget_config("world-higher-proposal-reject", 32);
    let mut engine = PosNodeEngine::new(&config).expect("engine");
    let old_actions = vec![proposal_action(&config, 1, 16)];
    reserve_action_payload_bytes(
        &engine.pending_consensus_action_queue_bytes,
        engine.max_pending_consensus_action_queue_bytes,
        16,
    )
    .expect("old pending reservation");
    engine.pending = Some(pending_proposal(
        &config,
        "node-a",
        1,
        "old-pending",
        old_actions,
    ));
    let replacement = gossip_proposal(
        &config,
        "node-b",
        2,
        "over-budget-higher-pending",
        vec![
            proposal_action(&config, 2, 16),
            proposal_action(&config, 3, 16),
            proposal_action(&config, 4, 16),
        ],
    );

    let _ = engine.ingest_proposal_message(&config.world_id, &replacement, 0);

    assert_eq!(
        engine
            .pending
            .as_ref()
            .map(|pending| pending.block_hash.as_str()),
        Some("old-pending"),
        "failed replacement must preserve the old proposal"
    );
    assert_eq!(
        engine
            .pending_consensus_action_queue_bytes
            .load(Ordering::Acquire),
        16,
        "failed replacement must preserve the old reservation"
    );
}

#[test]
fn committed_decision_cannot_release_unowned_pending_bytes() {
    let config = proposal_budget_config("world-commit-owned-reservation", 64);
    let mut engine = PosNodeEngine::new(&config).expect("engine");
    let pending_actions = vec![proposal_action(&config, 1, 8)];
    engine.pending = Some(pending_proposal(
        &config,
        "node-a",
        1,
        "pending-unowned",
        pending_actions.clone(),
    ));
    // Simulate unrelated public/engine ownership. The pending proposal has no
    // reservation in this fixture; commit must not subtract these bytes.
    engine
        .pending_consensus_action_queue_bytes
        .store(16, Ordering::Release);
    let decision = PosDecision {
        height: 1,
        slot: 0,
        epoch: 0,
        proposer_id: "node-a".to_string(),
        status: PosConsensusStatus::Committed,
        block_hash: "pending-unowned".to_string(),
        action_root: compute_consensus_action_root(&pending_actions).expect("action root"),
        committed_actions: pending_actions,
        approved_stake: 100,
        rejected_stake: 0,
        required_stake: 67,
        total_stake: 100,
    };

    engine.apply_decision(&decision).expect("commit decision");
    engine
        .apply_decision(&decision)
        .expect("repeated commit decision is idempotent");

    assert_eq!(
        engine
            .pending_consensus_action_queue_bytes
            .load(Ordering::Acquire),
        16,
        "commit must release only an owned pending reservation"
    );
}

#[test]
fn synced_replication_clear_releases_owned_pending_reservation() {
    let config = proposal_budget_config("world-sync-clear-reservation", 32);
    let mut engine = PosNodeEngine::new(&config).expect("engine");
    let actions = vec![proposal_action(&config, 1, 16)];
    reserve_action_payload_bytes(
        &engine.pending_consensus_action_queue_bytes,
        engine.max_pending_consensus_action_queue_bytes,
        16,
    )
    .expect("pending reservation");
    engine.pending = Some(pending_proposal(
        &config,
        "node-a",
        1,
        "sync-clear-pending",
        actions,
    ));

    engine
        .record_synced_replication_height(1, "sync-boundary".to_string(), 2_000)
        .expect("record synced height");

    assert!(engine.pending.is_none(), "sync must clear pending proposal");
    assert_eq!(
        engine
            .pending_consensus_action_queue_bytes
            .load(Ordering::Acquire),
        0,
        "sync clear must release the owned reservation"
    );
}

#[test]
fn rollback_clear_releases_owned_pending_reservation() {
    let config = proposal_budget_config("world-rollback-clear-reservation", 32);
    let mut engine = PosNodeEngine::new(&config).expect("engine");
    let actions = vec![proposal_action(&config, 1, 16)];
    reserve_action_payload_bytes(
        &engine.pending_consensus_action_queue_bytes,
        engine.max_pending_consensus_action_queue_bytes,
        16,
    )
    .expect("pending reservation");
    engine.pending = Some(pending_proposal(
        &config,
        "node-a",
        2,
        "rollback-clear-pending",
        actions,
    ));

    engine
        .rollback_to_replicated_commit_boundary(
            1,
            "rollback-boundary".to_string(),
            2_000,
            None,
            None,
        )
        .expect("rollback boundary");

    assert!(
        engine.pending.is_none(),
        "rollback must clear pending proposal"
    );
    assert_eq!(
        engine
            .pending_consensus_action_queue_bytes
            .load(Ordering::Acquire),
        0,
        "rollback clear must release the owned reservation"
    );
}

#[test]
fn restore_state_clear_releases_owned_pending_reservation() {
    let config = proposal_budget_config("world-restore-clear-reservation", 32);
    let mut engine = PosNodeEngine::new(&config).expect("engine");
    let actions = vec![proposal_action(&config, 1, 16)];
    reserve_action_payload_bytes(
        &engine.pending_consensus_action_queue_bytes,
        engine.max_pending_consensus_action_queue_bytes,
        16,
    )
    .expect("pending reservation");
    engine.pending = Some(pending_proposal(
        &config,
        "node-a",
        1,
        "restore-clear-pending",
        actions,
    ));
    let snapshot = engine.export_state_snapshot();

    engine
        .restore_state_snapshot(snapshot, None)
        .expect("restore state snapshot");

    assert!(
        engine.pending.is_none(),
        "restore must clear pending proposal"
    );
    assert_eq!(
        engine
            .pending_consensus_action_queue_bytes
            .load(Ordering::Acquire),
        0,
        "restore clear must release the owned reservation"
    );
}
