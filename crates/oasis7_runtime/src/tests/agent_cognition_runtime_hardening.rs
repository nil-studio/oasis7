//! RED fixtures for the World-owned cognition authority/remediation seam.

use super::super::*;
use crate::cognition::world_state_binding_digest_v1;
use crate::cognition_recovery::cognition_digest_v1;
use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub(super) const WORLD_ID: &str = "world-runtime-hardening";
pub(super) const AGENT_ID: &str = "agent-runtime-hardening";

pub(super) fn digest(seed: u8) -> String {
    let hex = b"0123456789abcdef"[(seed % 16) as usize] as char;
    format!("blake3:{}", hex.to_string().repeat(64))
}

pub(super) fn envelope(world: &World) -> AgentDecisionEnvelopeV1 {
    let request_digest = digest(0);
    let mut envelope = AgentDecisionEnvelopeV1 {
        schema_version: AGENT_DECISION_ENVELOPE_V1_SCHEMA.to_string(),
        world_id: WORLD_ID.to_string(),
        agent_id: AGENT_ID.to_string(),
        branch_id: "main".to_string(),
        finality_epoch: 0,
        finality_block_hash: None,
        finality_status: "pending".to_string(),
        agent_session_id: "session.runtime-hardening".to_string(),
        agent_turn_id: "turn.runtime-hardening".to_string(),
        decision_request_id: "request.runtime-hardening".to_string(),
        retry_seq: 1,
        base_tick: world.state().time,
        base_world_hash: world.current_state_root_hash().expect("world root"),
        reorg_epoch: 0,
        runtime_manifest_hash: world.current_manifest_hash().expect("manifest hash"),
        capability_snapshot_hash: digest(1),
        authority_context_hash: digest(2),
        observation_digest: digest(3),
        context_digest: digest(4),
        issued_at_tick: world.state().time,
        valid_until_tick: world.state().time + 10,
        preconditions: Vec::new(),
        decision_kind: "wait".to_string(),
        action: json!({
            "type": "RegisterAgent",
            "data": {
                "agent_id": AGENT_ID,
                "pos": {"x_cm": 0, "y_cm": 0, "z_cm": 0}
            }
        }),
        request_digest,
        decision_digest: String::new(),
        envelope_digest: String::new(),
        provider_invocation_key: String::new(),
        envelope_idempotency_key: String::new(),
        origin_intent_ref: None,
        source: "runtime-hardening-test".to_string(),
    };
    envelope.decision_digest = envelope.derive_decision_digest();
    envelope.envelope_digest = envelope.derive_envelope_digest();
    envelope.provider_invocation_key = envelope.derive_provider_invocation_key();
    envelope.envelope_idempotency_key = envelope.derive_envelope_idempotency_key();
    if let Ok(binding) = world.current_cognition_runtime_binding() {
        envelope.finality_epoch = binding.finality_epoch;
        envelope.finality_block_hash = binding.finality_block_hash.map(|hash| hash.to_string());
        envelope.finality_status = binding.finality_status;
        envelope.base_tick = binding.base_tick;
        envelope.base_world_hash = binding.base_world_hash.to_string();
        envelope.reorg_epoch = binding.reorg_epoch;
        envelope.runtime_manifest_hash = binding.runtime_manifest_hash.to_string();
    }
    envelope.decision_digest = envelope.derive_decision_digest();
    envelope.envelope_digest = envelope.derive_envelope_digest();
    envelope.provider_invocation_key = envelope.derive_provider_invocation_key();
    envelope.envelope_idempotency_key = envelope.derive_envelope_idempotency_key();
    envelope
}

pub(super) fn response_artifact_for_envelope(envelope: &AgentDecisionEnvelopeV1) -> Value {
    let mut artifact = RuntimeCognitionResponseArtifactV1 {
        schema_version: 1,
        context_discriminator: RuntimeCognitionResponseArtifactV1::CONTEXT_DISCRIMINATOR
            .to_string(),
        context_version: RuntimeCognitionResponseArtifactV1::CONTEXT_VERSION,
        agent_session_id: envelope.agent_session_id.clone(),
        agent_turn_id: envelope.agent_turn_id.clone(),
        decision_request_id: envelope.decision_request_id.clone(),
        retry_seq: envelope.retry_seq,
        transport_attempt: 1,
        request_digest: envelope.request_digest.clone(),
        response_digest: digest(17),
        artifact_digest: String::new(),
    };
    artifact.refresh_artifact_digest();
    let mut value = serde_json::to_value(artifact).expect("response artifact");
    let outer_base_world_hash = if envelope.base_world_hash.starts_with("blake3:") {
        envelope.base_world_hash.clone()
    } else {
        world_state_binding_digest_v1(
            &envelope.world_id,
            &envelope.branch_id,
            envelope.finality_epoch,
            envelope.finality_block_hash.as_deref(),
            &envelope.finality_status,
            envelope.base_tick,
            &envelope.base_world_hash,
            envelope.reorg_epoch,
            &envelope.runtime_manifest_hash,
        )
    };
    let outer_manifest_hash = if envelope.runtime_manifest_hash.starts_with("blake3:") {
        envelope.runtime_manifest_hash.clone()
    } else {
        cognition_digest_v1(
            "oasis7.runtime.manifest.v1",
            &envelope.runtime_manifest_hash,
        )
    };
    value["outer_lineage"] = json!({
        "agent_id": envelope.agent_id,
        "world_id": envelope.world_id,
        "branch_id": envelope.branch_id,
        "finality_epoch": envelope.finality_epoch,
        "finality_block_hash": envelope.finality_block_hash,
        "finality_status": envelope.finality_status,
        "base_tick": envelope.base_tick,
        "base_world_hash": outer_base_world_hash,
        "reorg_epoch": envelope.reorg_epoch,
        "runtime_manifest_hash": outer_manifest_hash,
    });
    value
}

pub(super) fn continuous_commit_request(world: &World) -> RuntimeCognitionCommitRequestV1 {
    let binding = world
        .current_cognition_runtime_binding()
        .expect("current runtime binding");
    RuntimeCognitionCommitRequestV1 {
        agent_id: AGENT_ID.to_string(),
        agent_session_id: "session.runtime-continuous".to_string(),
        agent_turn_id: "turn.runtime-continuous".to_string(),
        decision_request_id: "request.runtime-continuous".to_string(),
        retry_seq: 1,
        transport_attempt: 1,
        request_digest: digest(12),
        observation_digest: digest(13),
        context_digest: digest(14),
        capability_snapshot_hash: digest(15),
        authority_context_hash: digest(16),
        captured_base_binding: RuntimeCognitionBaseBindingV1 {
            world_id: binding.world_id,
            branch_id: binding.branch_id,
            finality_epoch: binding.finality_epoch,
            finality_block_hash: binding.finality_block_hash.map(|hash| hash.to_string()),
            finality_status: binding.finality_status,
            base_tick: binding.base_tick,
            base_world_hash: binding.base_world_hash.to_string(),
            reorg_epoch: binding.reorg_epoch,
            runtime_manifest_hash: binding.runtime_manifest_hash.to_string(),
        },
    }
}

pub(super) fn continuous_response_artifact(
    request: &RuntimeCognitionCommitRequestV1,
) -> RuntimeCognitionResponseArtifactV1 {
    let mut artifact = RuntimeCognitionResponseArtifactV1 {
        schema_version: 1,
        context_discriminator: RuntimeCognitionResponseArtifactV1::CONTEXT_DISCRIMINATOR
            .to_string(),
        context_version: RuntimeCognitionResponseArtifactV1::CONTEXT_VERSION,
        agent_session_id: request.agent_session_id.clone(),
        agent_turn_id: request.agent_turn_id.clone(),
        decision_request_id: request.decision_request_id.clone(),
        retry_seq: request.retry_seq,
        transport_attempt: request.transport_attempt,
        request_digest: request.request_digest.clone(),
        response_digest: digest(17),
        artifact_digest: String::new(),
    };
    artifact.refresh_artifact_digest();
    artifact
}

pub(super) fn policy() -> SchedulerPolicyV1 {
    serde_json::from_value(json!({
        "schema_version": "scheduler-policy.v1",
        "max_total_wakes_per_tick": 8,
        "max_wakes_per_agent_per_tick": 1,
        "aging_after_ticks": 2,
        "max_starvation_ticks": 4,
        "initial_priority": 0,
        "comparator": "deadline_due_desc,next_wake_tick_asc,effective_priority_desc,starvation_deadline_tick_asc,cursor_distance_asc,agent_id_asc,continuation_id_asc,wake_seq_asc",
        "service_order": "stable_round_robin"
    }))
    .expect("policy")
}

pub(super) fn proposal(world: &World) -> CognitionContinuationProposalV1 {
    let mut proposal = CognitionContinuationProposalV1 {
        schema_version: 1,
        continuation_proposal_id: "proposal.runtime-hardening".to_string(),
        world_id: WORLD_ID.to_string(),
        branch_id: "main".to_string(),
        finality_epoch: 0,
        finality_block_hash: None,
        finality_status: "pending".to_string(),
        reorg_epoch: 0,
        runtime_manifest_hash: world.current_manifest_hash().expect("manifest hash"),
        agent_id: AGENT_ID.to_string(),
        agent_session_id: "session.runtime-hardening".to_string(),
        agent_turn_id: "turn.runtime-hardening".to_string(),
        decision_request_id: "request.runtime-hardening".to_string(),
        origin_turn_id: "turn.runtime-hardening".to_string(),
        origin_request_digest: digest(5),
        action_or_plan_kind: "wait".to_string(),
        action_or_envelope_digest: None,
        remaining_budget: ContinuationBudgetV1 {
            unit: "steps".to_string(),
            value: 2,
        },
        baseline_observation_digest: digest(6),
        goal_digest: digest(7),
        policy_digest: digest(8),
        policy_revision: 1,
        precondition_summary: "ready".to_string(),
        precondition_digest: digest(9),
        wake_conditions: vec![WakeConditionV1 {
            schema_version: "wake-condition.v1".to_string(),
            kind: "at_or_after_tick".to_string(),
            logical_tick: Some(world.state().time + 1),
            event_digest: None,
            receipt_id: None,
            subject: None,
            path_or_rule: None,
            operator: None,
            expected_value_bytes: None,
        }],
        next_wake_tick: Some(world.state().time + 1),
        valid_until_tick: Some(world.state().time + 10),
        source: "runtime-hardening-test".to_string(),
        proposal_digest: String::new(),
    };
    proposal.proposal_digest = proposal.proposal_digest();
    proposal
}

pub(super) fn bind_test_turn(world: &mut World) {
    world
        .bind_cognition_runtime(WORLD_ID, "main", 0, None, "pending", 0)
        .expect("bind test Runtime authority");
    world
        .start_cognition_turn(
            AGENT_ID,
            "session.runtime-hardening",
            "turn.runtime-hardening",
            "request.runtime-hardening",
            &digest(5),
        )
        .expect("register test cognition turn");
}

pub(super) fn test_continuation_wake(
    world: &World,
    wake_id: &str,
    continuation_id: &str,
) -> SchedulerWakeV1 {
    serde_json::from_value(json!({
        "schema_version": "scheduler-wake.v1",
        "wake_id": wake_id,
        "continuation_id": continuation_id,
        "world_id": WORLD_ID,
        "branch_id": "main",
        "finality_epoch": 0,
        "finality_block_hash": null,
        "finality_status": "pending",
        "reorg_epoch": 0,
        "runtime_manifest_hash": world.current_manifest_hash().expect("manifest"),
        "agent_id": AGENT_ID,
        "agent_session_id": "session.consumer",
        "agent_turn_id": "turn.consumer",
        "decision_request_id": "request.consumer",
        "next_wake_tick": 0,
        "eligible_since_tick": 0,
        "starvation_deadline_tick": 4,
        "initial_priority": 0,
        "wake_seq": 1,
        "retry_seq": 0,
        "status": "pending",
        "pending_reason": "capacity_available"
    }))
    .expect("wake")
}

pub(super) fn test_continuation(wake: &SchedulerWakeV1) -> AgentContinuation {
    let mut continuation: AgentContinuation = serde_json::from_value(json!({
        "schema_version": "agent-continuation.v1",
        "continuation_id": wake.continuation_id,
        "wake_id": wake.wake_id,
        "world_id": wake.world_id,
        "branch_id": wake.branch_id,
        "finality_epoch": wake.finality_epoch,
        "finality_block_hash": null,
        "finality_status": wake.finality_status,
        "reorg_epoch": wake.reorg_epoch,
        "runtime_manifest_hash": wake.runtime_manifest_hash,
        "agent_id": wake.agent_id,
        "agent_session_id": wake.agent_session_id,
        "agent_turn_id": wake.agent_turn_id,
        "decision_request_id": wake.decision_request_id,
        "origin_turn_id": wake.agent_turn_id,
        "origin_request_digest": digest(23),
        "continuation_proposal_id": "proposal.consumer",
        "proposal_digest": digest(24),
        "action_or_envelope_digest": null,
        "wake_conditions": [{
            "schema_version": "wake-condition.v1",
            "kind": "at_or_after_tick",
            "logical_tick": 0
        }],
        "next_wake_tick": 0,
        "remaining_budget": {"unit": "steps", "value": 2},
        "valid_until_tick": 10,
        "precondition_digest": digest(25),
        "wake_seq": wake.wake_seq,
        "logical_tick": 0,
        "status": "scheduled",
        "terminal_disposition": null
    }))
    .expect("continuation");
    continuation.refresh_status_digest();
    continuation
}

pub(super) fn temp_dir(label: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    std::env::temp_dir().join(format!("oasis7-runtime-hardening-{label}-{nonce}"))
}

pub(super) fn read_snapshot(dir: &Path) -> Value {
    serde_json::from_slice(&fs::read(dir.join("snapshot.json")).expect("snapshot"))
        .expect("decode snapshot")
}

pub(super) fn write_snapshot(dir: &Path, snapshot: &Value) {
    fs::write(
        dir.join("snapshot.json"),
        serde_json::to_vec_pretty(snapshot).expect("encode snapshot"),
    )
    .expect("write snapshot");
}

#[test]
fn typed_runtime_readback_returns_only_validated_continuation_and_wake_identity() {
    let mut world = World::new()
        .try_with_cognition_scheduler(policy(), 1)
        .expect("scheduler");
    bind_test_turn(&mut world);
    let wake = test_continuation_wake(&world, "wake.readback", "continuation.readback");
    let continuation = test_continuation(&wake);
    world
        .install_cognition_continuation_for_test(continuation.clone())
        .expect("continuation projection");
    world
        .enqueue_cognition_wake_for_test(wake.clone())
        .expect("wake projection");

    let active = world
        .active_cognition_continuations()
        .expect("typed active continuation readback");
    assert_eq!(active, vec![continuation.clone()]);
    let readback = world
        .cognition_wake_readback(&wake.wake_id)
        .expect("typed wake readback")
        .expect("wake present");
    let mut expected = wake;
    expected.request_digest = continuation.origin_request_digest;
    assert_eq!(readback, expected);
}

#[test]
fn world_commit_prepare_finalize_is_bound_and_receipt_is_world_derived() {
    let mut world = World::new();
    let envelope = envelope(&world);
    let prepared = world
        .prepare_cognition_envelope(
            envelope.clone(),
            Some(response_artifact_for_envelope(&envelope)),
        )
        .expect("prepare through World");
    assert_eq!(prepared.status, "prepared");
    assert_eq!(prepared.parent_world_hash, envelope.base_world_hash);
    let committed = world
        .finalize_cognition_commit(&prepared.commit_id)
        .expect("finalize through World");
    assert_eq!(committed.status, "committed");
    let lineage = world
        .read_runtime_receipt_lineage(&committed.receipt_id)
        .expect("read World-derived receipt lineage");
    assert_eq!(lineage.agent_id, envelope.agent_id);
    assert_eq!(lineage.request_digest, envelope.request_digest);
    assert!(world.verify_runtime_receipt_lineage(&lineage).is_ok());
}

#[test]
fn continuous_commit_api_derives_world_authority_and_reads_back_lineage() {
    let mut world = World::new();
    world
        .bind_cognition_runtime(WORLD_ID, "main", 0, None, "pending", 0)
        .expect("bind World authority");
    let runtime_binding = world
        .current_cognition_runtime_binding()
        .expect("derive canonical World binding");
    runtime_binding.validate().expect("binding validates");
    assert_eq!(runtime_binding.base_tick, world.state().time);
    assert!(runtime_binding.base_world_hash.is_canonical_blake3());
    assert!(runtime_binding.runtime_manifest_hash.is_canonical_blake3());
    assert_ne!(
        runtime_binding.base_world_hash.as_str(),
        world.current_state_root_hash().expect("raw state root")
    );
    let request = continuous_commit_request(&world);
    request
        .validate()
        .expect("continuous request fixture validates");
    let artifact = continuous_response_artifact(&request);
    let action: Action = serde_json::from_value(json!({
        "type": "RegisterAgent",
        "data": {
            "agent_id": AGENT_ID,
            "pos": {"x_cm": 0, "y_cm": 0, "z_cm": 0}
        }
    }))
    .expect("runtime action");

    let (committed, lineage) = world
        .commit_cognition_action(request.clone(), action, artifact)
        .expect("World-owned continuous commit");

    assert_eq!(committed.world_id, WORLD_ID);
    assert_eq!(committed.agent_id, request.agent_id);
    assert_eq!(committed.action_id, "action:1");
    assert!(
        world
            .journal()
            .events
            .iter()
            .any(|event| event.caused_by == Some(CausedBy::Action(1)))
    );
    assert_eq!(world.state().time, 1);
    assert_eq!(lineage.receipt_id, committed.receipt_id);
    assert_eq!(lineage.request_digest, request.request_digest);
    let outer = &world.cognition()["responses"][0]["response_artifact"]["outer_lineage"];
    assert_eq!(outer["world_id"], WORLD_ID);
    assert_eq!(outer["agent_id"], AGENT_ID);
    assert_eq!(outer["base_tick"], request.captured_base_binding.base_tick);
    assert_eq!(
        outer["base_world_hash"],
        request.captured_base_binding.base_world_hash
    );
    world
        .verify_runtime_receipt_lineage(&lineage)
        .expect("readback lineage remains World-bound");
    world
        .replay_cognition_response(&committed.envelope_digest)
        .expect("typed response artifact replays only with its marker binding");
}

#[test]
fn continuous_commit_api_rejects_response_outer_lineage_mismatch() {
    let mut world = World::new();
    world
        .bind_cognition_runtime(WORLD_ID, "main", 0, None, "pending", 0)
        .expect("bind World authority");
    let request = continuous_commit_request(&world);
    let mut artifact = continuous_response_artifact(&request);
    artifact.transport_attempt = artifact.transport_attempt.saturating_add(1);
    artifact.refresh_artifact_digest();
    let cognition_before = world.cognition().clone();
    let action: Action = serde_json::from_value(json!({
        "type": "RegisterAgent",
        "data": {
            "agent_id": AGENT_ID,
            "pos": {"x_cm": 0, "y_cm": 0, "z_cm": 0}
        }
    }))
    .expect("runtime action");

    assert!(
        world
            .commit_cognition_action(request, action, artifact)
            .is_err()
    );
    assert_eq!(world.state().time, 0);
    assert_eq!(world.cognition(), &cognition_before);
}

#[test]
fn continuous_commit_rejects_a_stale_captured_world_head() {
    let mut world = World::new();
    world
        .bind_cognition_runtime(WORLD_ID, "main", 0, None, "pending", 0)
        .expect("bind World authority");
    let request = continuous_commit_request(&world);
    world.step().expect("advance World head");
    let world_journal_before = world.journal().clone();
    let action: Action = serde_json::from_value(json!({
        "type": "RegisterAgent",
        "data": {
            "agent_id": AGENT_ID,
            "pos": {"x_cm": 0, "y_cm": 0, "z_cm": 0}
        }
    }))
    .expect("runtime action");
    let artifact = continuous_response_artifact(&request);
    let state_root_before = world.current_state_root_hash().expect("current root");
    let cognition_before = world.cognition().clone();

    let first = world
        .commit_cognition_action(request.clone(), action.clone(), artifact.clone())
        .expect_err("a request captured before the World head moved must be rejected");
    assert!(matches!(
        first,
        WorldError::DistributedValidationFailed { ref reason } if reason == "stale_base"
    ));
    assert_eq!(
        world.current_state_root_hash().expect("root unchanged"),
        state_root_before
    );
    assert_eq!(world.journal(), &world_journal_before);
    assert_eq!(
        world.cognition()["commit_records"],
        cognition_before["commit_records"]
    );
    assert!(
        world.cognition()["receipt_registry"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let events = world.cognition()["cognition_journal"]["events"]
        .as_array()
        .expect("terminal events");
    assert_eq!(events.len(), 2);
    assert_eq!(events[0]["kind"], "DecisionRejected");
    assert_eq!(events[1]["kind"], "CognitionTurnCompleted");
    assert_eq!(events[1]["status"], "rejected");

    let cognition_after_first = world.cognition().clone();
    world.step().expect("advance again before stale replay");
    let replay = world
        .commit_cognition_action(request, action, artifact)
        .expect_err("same stale identity must replay stale_base");
    assert!(matches!(
        replay,
        WorldError::DistributedValidationFailed { ref reason } if reason == "stale_base"
    ));
    assert_eq!(world.cognition(), &cognition_after_first);
}

#[test]
fn cognition_commit_error_classifier_is_typed_and_preserves_stable_code() {
    let stale = WorldError::DistributedValidationFailed {
        reason: "stale_base".to_string(),
    };
    assert_eq!(
        classify_cognition_commit_error(&stale),
        Some(CognitionCommitRejectReasonV1::StaleBase)
    );
    assert_eq!(
        CognitionCommitRejectReasonV1::StaleBase.code(),
        "stale_base"
    );

    let legacy_stale = WorldError::DistributedValidationFailed {
        reason: "cognition validation failed: cognition_base_binding_stale".to_string(),
    };
    assert_eq!(
        classify_cognition_commit_error(&legacy_stale),
        Some(CognitionCommitRejectReasonV1::StaleBase)
    );

    let unrelated = WorldError::DistributedValidationFailed {
        reason: "cognition validation failed: response_digest_mismatch".to_string(),
    };
    assert_eq!(classify_cognition_commit_error(&unrelated), None);
}

#[test]
fn stale_delayed_response_is_durable_rejection_and_idempotent_replay() {
    let mut world = World::new();
    world
        .bind_cognition_runtime(WORLD_ID, "main", 0, None, "pending", 0)
        .expect("bind World authority");
    let stale = envelope(&world);
    world
        .step()
        .expect("advance World head before delayed response");

    let state_root_before = world.current_state_root_hash().expect("current root");
    let journal_before = world.journal().clone();
    let first = world
        .prepare_cognition_envelope(stale.clone(), Some(response_artifact_for_envelope(&stale)))
        .expect_err("delayed response must be rejected as stale_base");
    assert!(matches!(
        first,
        WorldError::DistributedValidationFailed { ref reason } if reason == "stale_base"
    ));
    assert_eq!(
        world.current_state_root_hash().expect("root unchanged"),
        state_root_before
    );
    assert_eq!(
        world.journal(),
        &journal_before,
        "stale response submitted no world action"
    );
    assert!(
        world.cognition()["commit_records"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(
        world.cognition()["receipt_registry"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(world.pending_effects_len(), 0);

    let events = world.cognition()["cognition_journal"]["events"]
        .as_array()
        .expect("durable cognition events");
    assert_eq!(events.len(), 2);
    assert_eq!(events[0]["kind"], "DecisionRejected");
    assert_eq!(events[1]["kind"], "CognitionTurnCompleted");
    for event in events {
        for (field, value) in [
            ("agent_id", json!(stale.agent_id)),
            ("agent_session_id", json!(stale.agent_session_id)),
            ("agent_turn_id", json!(stale.agent_turn_id)),
            ("decision_request_id", json!(stale.decision_request_id)),
            ("request_digest", json!(stale.request_digest)),
            ("envelope_digest", json!(stale.envelope_digest)),
            (
                "envelope_idempotency_key",
                json!(stale.envelope_idempotency_key),
            ),
        ] {
            assert_eq!(event[field], value, "terminal event lost {field}");
        }
    }
    assert_eq!(events[0]["reject_reason"], "stale_base");
    assert_eq!(events[1]["status"], "rejected");

    let dir = temp_dir("stale-terminal");
    world
        .save_to_dir(&dir)
        .expect("persist stale terminal projection");
    let mut restored = World::load_from_dir(&dir).expect("restore stale terminal projection");
    assert_eq!(restored.cognition(), world.cognition());
    let cognition_after_first = restored.cognition().clone();
    restored.step().expect("advance again before stale replay");
    let replay = restored
        .prepare_cognition_envelope(stale.clone(), Some(response_artifact_for_envelope(&stale)))
        .expect_err("same stale identity remains stale_base");
    assert!(matches!(
        replay,
        WorldError::DistributedValidationFailed { ref reason } if reason == "stale_base"
    ));
    assert_eq!(restored.cognition(), &cognition_after_first);
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn scheduler_persists_selected_wake_identity_and_releases_it_on_cancel() {
    let mut world = World::new()
        .try_with_cognition_scheduler(policy(), 1)
        .expect("validated scheduler");
    bind_test_turn(&mut world);
    let admitted = world
        .admit_cognition_continuation(proposal(&world))
        .expect("admit continuation");
    world.step().expect("advance to wake tick");
    let selected = world
        .select_ready_cognition_wakes(world.state().time)
        .expect("select ready wake");
    if selected.is_empty() {
        // The production World tick may already own the scheduler lease.
        assert_eq!(
            world.cognition_scheduler_snapshot()["in_flight"][admitted.wake_id.as_str()]["wake_id"],
            admitted.wake_id
        );
    } else {
        assert_eq!(selected.len(), 1);
    }
    let before = world.cognition_scheduler_snapshot();
    assert_eq!(
        before["in_flight"][admitted.wake_id.as_str()]["wake_id"],
        admitted.wake_id
    );

    let dir = temp_dir("in-flight-wake");
    world.save_to_dir(&dir).expect("save selected wake");
    let mut restored = World::load_from_dir(&dir).expect("restore selected wake");
    assert_eq!(
        restored.cognition_scheduler_snapshot()["in_flight"][admitted.wake_id.as_str()],
        before["in_flight"][admitted.wake_id.as_str()]
    );
    let recovered = restored
        .recover_cognition_scheduler(restored.state().time)
        .expect("recover selected wake without dropping its identity");
    assert_eq!(recovered.len(), 1);
    assert_eq!(recovered[0].wake_id, admitted.wake_id);
    assert_eq!(
        restored.cognition_scheduler_snapshot()["active"][0]["wake_id"],
        admitted.wake_id
    );
    restored
        .transition_cognition_continuation(
            admitted.continuation_id.as_str(),
            ContinuationStatusV1::Cancelled,
            restored.state().time,
        )
        .expect("cancel selected continuation");
    assert!(
        restored.cognition_scheduler_snapshot()["in_flight"]
            .get(admitted.wake_id.as_str())
            .is_none(),
        "terminal cancellation must release the exact in-flight wake"
    );
    let kinds: Vec<&str> = restored.cognition()["cognition_journal"]["events"]
        .as_array()
        .expect("journal events")
        .iter()
        .filter_map(|event| event["kind"].as_str())
        .collect();
    assert!(kinds.contains(&"SchedulerWakeSelected"));
    assert!(kinds.contains(&"ContinuationCancelled"));
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn event_receipt_and_state_only_continuations_admit_without_a_forced_tick() {
    let mut world = World::new()
        .try_with_cognition_scheduler(policy(), 8)
        .expect("validated scheduler");
    bind_test_turn(&mut world);
    let cases = [
        (
            "event",
            WakeConditionV1 {
                schema_version: "wake-condition.v1".to_string(),
                kind: "world_event_committed".to_string(),
                logical_tick: None,
                event_digest: Some(digest(21)),
                receipt_id: None,
                subject: None,
                path_or_rule: None,
                operator: None,
                expected_value_bytes: None,
            },
        ),
        (
            "receipt",
            WakeConditionV1 {
                schema_version: "wake-condition.v1".to_string(),
                kind: "receipt_linked".to_string(),
                logical_tick: None,
                event_digest: None,
                receipt_id: Some(digest(22)),
                subject: None,
                path_or_rule: None,
                operator: None,
                expected_value_bytes: None,
            },
        ),
        (
            "state",
            WakeConditionV1 {
                schema_version: "wake-condition.v1".to_string(),
                kind: "state_predicate".to_string(),
                logical_tick: None,
                event_digest: None,
                receipt_id: None,
                subject: Some(PreconditionSubjectV1 {
                    kind: "world".to_string(),
                    id: WORLD_ID.to_string(),
                }),
                path_or_rule: Some("world.logical_tick".to_string()),
                operator: Some("eq".to_string()),
                expected_value_bytes: Some(vec![0]),
            },
        ),
    ];
    let mut state_continuation_id = String::new();
    for (index, (suffix, condition)) in cases.into_iter().enumerate() {
        let turn_id = format!("turn.runtime-hardening-{suffix}");
        let request_id = format!("request.runtime-hardening-{suffix}");
        let request_digest = digest(30 + index as u8);
        world
            .start_cognition_turn(
                AGENT_ID,
                "session.runtime-hardening",
                &turn_id,
                &request_id,
                &request_digest,
            )
            .expect("register untimed cognition turn");
        let mut candidate = proposal(&world);
        candidate.continuation_proposal_id = format!("proposal.untimed-{suffix}");
        candidate.agent_turn_id = turn_id.clone();
        candidate.origin_turn_id = turn_id;
        candidate.decision_request_id = request_id;
        candidate.origin_request_digest = request_digest;
        candidate.wake_conditions = vec![condition];
        candidate.next_wake_tick = None;
        candidate.proposal_digest = candidate.proposal_digest();
        let admitted = world
            .admit_cognition_continuation(candidate)
            .unwrap_or_else(|error| panic!("{suffix} untimed continuation admission: {error:?}"));
        assert!(admitted.next_wake_tick.is_none());
        if suffix == "state" {
            state_continuation_id = admitted.continuation_id;
        }
    }
    let selected = world
        .select_ready_cognition_wakes(world.state().time)
        .expect("state predicate wakes evaluate without a forced tick");
    assert!(
        selected
            .iter()
            .any(|wake| wake.continuation_id == state_continuation_id)
    );
}

#[test]
fn cognition_finalize_linearizes_the_world_action_before_publishing_receipt() {
    let mut world = World::new();
    let parent_root = world.current_state_root_hash().expect("parent root");
    let envelope = envelope(&world);
    let prepared = world
        .prepare_cognition_envelope(
            envelope.clone(),
            Some(response_artifact_for_envelope(&envelope)),
        )
        .expect("prepare");
    let committed = world
        .finalize_cognition_commit(&prepared.commit_id)
        .expect("finalize");

    assert_eq!(committed.status, "committed");
    assert_ne!(committed.staged_state_root, parent_root);
    assert_eq!(world.state().time, 1);
    assert!(world.state().agents.contains_key(AGENT_ID));
}

#[test]
fn restore_rejects_tampered_canonical_response_artifact() {
    let mut world = World::new();
    let envelope = envelope(&world);
    let prepared = world
        .prepare_cognition_envelope(
            envelope.clone(),
            Some(response_artifact_for_envelope(&envelope)),
        )
        .expect("prepare");
    world
        .finalize_cognition_commit(&prepared.commit_id)
        .expect("finalize");
    let dir = temp_dir("tampered-response");
    world.save_to_dir(&dir).expect("save");
    let mut snapshot = read_snapshot(&dir);
    snapshot["cognition"]["responses"][0]["response_artifact"]["decision"] = json!("tampered");
    write_snapshot(&dir, &snapshot);
    fs::remove_dir_all(dir.join(".distfs-state")).expect("force JSON restore");
    assert!(
        World::load_from_dir(&dir).is_err(),
        "tampered response must fail closed"
    );
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn continuation_admission_recomputes_digest_binds_authority_and_enqueues_wake() {
    let mut world = World::new()
        .try_with_cognition_scheduler(policy(), 2)
        .expect("validated scheduler");
    bind_test_turn(&mut world);
    let proposal = proposal(&world);
    let admitted = world
        .admit_cognition_continuation(proposal.clone())
        .expect("admit proposal");
    assert_eq!(
        admitted.continuation_proposal_id,
        proposal.continuation_proposal_id
    );
    assert!(
        world.cognition()["scheduler_state"]["active"]
            .as_array()
            .expect("active wakes")
            .iter()
            .any(|wake| wake["wake_id"] == admitted.wake_id)
    );

    let mut forged = proposal;
    forged.proposal_digest = digest(10);
    assert!(world.admit_cognition_continuation(forged).is_err());
}

#[test]
fn scheduler_constructor_is_fallible_and_foreign_wake_is_rejected() {
    let invalid = SchedulerPolicyV1 {
        max_total_wakes_per_tick: 0,
        ..policy()
    };
    assert!(
        World::new()
            .try_with_cognition_scheduler(invalid, 1)
            .is_err()
    );

    let mut world = World::new()
        .try_with_cognition_scheduler(policy(), 1)
        .expect("scheduler");
    world
        .bind_cognition_runtime(WORLD_ID, "main", 0, None, "pending", 0)
        .expect("bind scheduler identity");
    let wake: SchedulerWakeV1 = serde_json::from_value(json!({
        "schema_version": "scheduler-wake.v1",
        "wake_id": "wake-foreign",
        "continuation_id": "continuation-foreign",
        "world_id": "other-world",
        "branch_id": "main",
        "finality_epoch": 0,
        "finality_block_hash": "",
        "finality_status": "pending",
        "reorg_epoch": 0,
        "runtime_manifest_hash": digest(11),
        "agent_id": AGENT_ID,
        "agent_session_id": "session.runtime-hardening",
        "agent_turn_id": "turn.runtime-hardening",
        "decision_request_id": "request.runtime-hardening",
        "next_wake_tick": 1,
        "eligible_since_tick": 0,
        "starvation_deadline_tick": 4,
        "initial_priority": 0,
        "wake_seq": 1,
        "retry_seq": 0,
        "status": "pending",
        "pending_reason": "capacity_available"
    }))
    .expect("wake");
    assert!(world.enqueue_cognition_wake(wake).is_err());
}

#[test]
fn response_replay_rejects_missing_or_mismatched_journal_head() {
    let mut world = World::new();
    let envelope = envelope(&world);
    let prepared = world
        .prepare_cognition_envelope(
            envelope.clone(),
            Some(response_artifact_for_envelope(&envelope)),
        )
        .expect("prepare");
    world
        .finalize_cognition_commit(&prepared.commit_id)
        .expect("finalize");
    let mut projection = world.cognition().clone();
    projection["responses"][0]["journal_head"] = json!("blake3:bad");
    let dir = temp_dir("tampered-journal-head");
    world.save_to_dir(&dir).expect("save");
    let mut snapshot = read_snapshot(&dir);
    snapshot["cognition"] = projection;
    write_snapshot(&dir, &snapshot);
    fs::remove_dir_all(dir.join(".distfs-state")).expect("force JSON restore");
    assert!(World::load_from_dir(&dir).is_err());
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn generic_prepare_rejects_an_unbound_response_artifact() {
    let mut world = World::new();
    let envelope = envelope(&world);
    let cognition_before = world.cognition().clone();
    let error = world
        .prepare_cognition_envelope(envelope, Some(json!({"decision": "wait"})))
        .expect_err("generic response artifacts cannot cross the typed boundary");
    assert!(matches!(
        error,
        WorldError::DistributedValidationFailed { ref reason }
            if reason.contains("response_artifact_identity_invalid")
    ));
    assert_eq!(world.cognition(), &cognition_before);
}

#[test]
fn restore_rejects_consistently_tampered_commit_digest() {
    let mut world = World::new();
    let envelope = envelope(&world);
    let prepared = world
        .prepare_cognition_envelope(
            envelope.clone(),
            Some(response_artifact_for_envelope(&envelope)),
        )
        .expect("prepare");
    let committed = world
        .finalize_cognition_commit(&prepared.commit_id)
        .expect("finalize");
    let dir = temp_dir("tampered-commit-digest");
    world.save_to_dir(&dir).expect("save");
    let mut snapshot = read_snapshot(&dir);
    let forged_digest = "blake3:forged-commit-receipt-digest";
    snapshot["cognition"]["commit_records"][0]["receipt_digest"] = json!(forged_digest);
    snapshot["cognition"]["receipt_registry"][0]["receipt_digest"] = json!(forged_digest);
    snapshot["cognition"]["receipt_lineage_registry"][0]["receipt_digest"] = json!(forged_digest);
    write_snapshot(&dir, &snapshot);
    fs::remove_dir_all(dir.join(".distfs-state")).expect("force JSON restore");
    assert!(
        World::load_from_dir(&dir).is_err(),
        "consistent receipt tampering must fail closed"
    );
    let _ = fs::remove_dir_all(dir);
    let _ = committed;
}

#[test]
fn restore_rejects_noncanonical_response_digest() {
    let mut world = World::new();
    let envelope = envelope(&world);
    let prepared = world
        .prepare_cognition_envelope(
            envelope.clone(),
            Some(response_artifact_for_envelope(&envelope)),
        )
        .expect("prepare");
    world
        .finalize_cognition_commit(&prepared.commit_id)
        .expect("finalize");
    let dir = temp_dir("noncanonical-response-digest");
    world.save_to_dir(&dir).expect("save");
    let mut snapshot = read_snapshot(&dir);
    snapshot["cognition"]["responses"][0]["response_digest"] = json!("forged");
    write_snapshot(&dir, &snapshot);
    fs::remove_dir_all(dir.join(".distfs-state")).expect("force JSON restore");
    assert!(
        World::load_from_dir(&dir).is_err(),
        "noncanonical response digest must fail closed"
    );
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn recovery_reconciles_every_committed_marker_not_only_the_latest() {
    let mut world = World::new();
    let first = envelope(&world);
    let first_commit = world
        .prepare_cognition_envelope(first.clone(), Some(response_artifact_for_envelope(&first)))
        .expect("first prepare");
    let first_commit = world
        .finalize_cognition_commit(&first_commit.commit_id)
        .expect("first finalize");

    let mut second = envelope(&world);
    second.agent_turn_id = "turn.runtime-hardening-2".to_string();
    second.decision_request_id = "request.runtime-hardening-2".to_string();
    second.request_digest = digest(10);
    second.retry_seq = 2;
    second.decision_digest = second.derive_decision_digest();
    second.envelope_digest = second.derive_envelope_digest();
    second.provider_invocation_key = second.derive_provider_invocation_key();
    second.envelope_idempotency_key = second.derive_envelope_idempotency_key();
    let second_commit = world
        .prepare_cognition_envelope(
            second.clone(),
            Some(response_artifact_for_envelope(&second)),
        )
        .expect("second prepare");
    let second_commit = world
        .finalize_cognition_commit(&second_commit.commit_id)
        .expect("second finalize");

    let dir = temp_dir("all-committed-markers");
    world.save_to_dir(&dir).expect("save");
    let mut snapshot = read_snapshot(&dir);
    snapshot["cognition"]["receipt_registry"] = json!([{
        "receipt_id": second_commit.receipt_id,
        "receipt_digest": second_commit.receipt_digest
    }]);
    snapshot["cognition"]["idempotency_index"]
        .as_object_mut()
        .expect("idempotency object")
        .remove(&first.envelope_idempotency_key);
    snapshot["cognition"]["receipt_lineage_registry"] = json!([{
        "schema_version": "runtime-receipt-lineage.v1",
        "status": "committed",
        "receipt_id": second_commit.receipt_id,
        "receipt_digest": second_commit.receipt_digest,
        "envelope_digest": second_commit.envelope_digest,
        "action_id": second_commit.action_id,
        "agent_id": second.agent_id,
        "agent_session_id": second.agent_session_id,
        "agent_turn_id": second.agent_turn_id,
        "decision_request_id": second.decision_request_id,
        "request_digest": second.request_digest,
        "feedback_id": second_commit.feedback_id
    }]);
    // The journal is an append-only authority and must remain a contiguous
    // prefix. Remove only the derived receipt/idempotency projections so
    // recovery exercises every committed marker without manufacturing a
    // middle-event gap that must fail closed.
    write_snapshot(&dir, &snapshot);
    fs::remove_dir_all(dir.join(".distfs-state")).expect("force JSON restore");

    let restored = World::load_from_dir(&dir).expect("restore all committed markers");
    assert!(
        restored.cognition()["receipt_registry"]
            .as_array()
            .expect("receipt registry")
            .iter()
            .any(|receipt| receipt["receipt_id"] == first_commit.receipt_id),
        "recovery must repair an earlier committed receipt too"
    );
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn exact_committed_replay_returns_original_receipt_without_terminal_delta() {
    let mut world = World::new();
    world
        .bind_cognition_runtime(WORLD_ID, "main", 0, None, "pending", 0)
        .expect("bind World authority");
    let request = continuous_commit_request(&world);
    let action: Action = serde_json::from_value(json!({
        "type": "RegisterAgent",
        "data": {"agent_id": AGENT_ID, "pos": {"x_cm": 0, "y_cm": 0, "z_cm": 0}}
    }))
    .expect("runtime action");
    let artifact = continuous_response_artifact(&request);
    let (committed, lineage) = world
        .commit_cognition_action(request.clone(), action.clone(), artifact.clone())
        .expect("first commit");
    let cognition = world.cognition().clone();
    let (replayed, replayed_lineage) = world
        .commit_cognition_action(request, action, artifact)
        .expect("same committed request must replay");
    assert_eq!(replayed, committed);
    assert_eq!(replayed_lineage, lineage);
    assert_eq!(world.cognition(), &cognition);
}

#[test]
fn committed_cognition_turn_is_closed_after_receipt_link() {
    let mut world = World::new();
    let envelope = envelope(&world);
    let prepared = world
        .prepare_cognition_envelope(
            envelope.clone(),
            Some(response_artifact_for_envelope(&envelope)),
        )
        .expect("prepare");
    world
        .finalize_cognition_commit(&prepared.commit_id)
        .expect("finalize");
    let events = world.cognition()["cognition_journal"]["events"]
        .as_array()
        .expect("cognition events");
    let link = events
        .iter()
        .position(|event| event["kind"] == "WorldReceiptLinked")
        .expect("receipt link");
    assert_eq!(events[link + 1]["kind"], "CognitionTurnCompleted");
    assert_eq!(events[link + 1]["status"], "committed");
}
