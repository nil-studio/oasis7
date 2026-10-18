//! P0.2 RED fixtures for the AgentDecisionEnvelope/MVCC seam.
//!
//! These tests target the wished-for runtime API from issue #3602.  They are
//! intentionally separate from the P0.1 wire tests: the Runtime owns the
//! authoritative parent-head check, validation order, and no-effect boundary.

use super::super::*;
use serde_json::{Value, json};

use crate::{
    AgentCognitionMailbox, AgentDecisionEnvelopeV1, MvccValidator, PreconditionV1,
};

const WORLD_ID: &str = "world-1";
const AGENT_ID: &str = "agent-1";
const BRANCH_ID: &str = "main";
const STATE_ROOT: &str = "blake3:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const FINALITY_BLOCK: &str =
    "blake3:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const MANIFEST_HASH: &str =
    "blake3:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";
const REQUEST_DIGEST: &str =
    "blake3:4444444444444444444444444444444444444444444444444444444444444444";
const ENVELOPE_DIGEST: &str =
    "blake3:5555555555555555555555555555555555555555555555555555555555555555";
const INVOCATION_KEY: &str =
    "blake3:6666666666666666666666666666666666666666666666666666666666666666";
const IDEMPOTENCY_KEY: &str =
    "blake3:7777777777777777777777777777777777777777777777777777777777777777";

fn envelope_fixture() -> Value {
    json!({
        "schema_version": "agent-decision-envelope.v1",
        "world_id": WORLD_ID,
        "agent_id": AGENT_ID,
        "branch_id": BRANCH_ID,
        "finality_epoch": 7,
        "finality_block_hash": FINALITY_BLOCK,
        "finality_status": "verified",
        "agent_session_id": "session.agent-1.v1",
        "agent_turn_id": "turn.agent-1.7",
        "decision_request_id": "request.agent-1.7",
        "retry_seq": 0,
        "base_tick": 42,
        "base_world_hash": STATE_ROOT,
        "reorg_epoch": 3,
        "runtime_manifest_hash": MANIFEST_HASH,
        "capability_snapshot_hash": "blake3:eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee",
        "authority_context_hash": "blake3:ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
        "observation_digest": "blake3:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
        "context_digest": "blake3:3333333333333333333333333333333333333333333333333333333333333333",
        "issued_at_tick": 42,
        "valid_until_tick": 50,
        "preconditions": [],
        "decision_kind": "move_agent",
        "action": {
            "type": "move_agent",
            "data": {"agent_id": AGENT_ID, "to": {"x_cm": 1, "y_cm": 0, "z_cm": 0}}
        },
        "request_digest": REQUEST_DIGEST,
        "decision_digest": "blake3:8888888888888888888888888888888888888888888888888888888888888888",
        "envelope_digest": ENVELOPE_DIGEST,
        "provider_invocation_key": INVOCATION_KEY,
        "envelope_idempotency_key": IDEMPOTENCY_KEY,
        "origin_intent_ref": null,
        "source": "provider"
    })
}

fn envelope() -> AgentDecisionEnvelopeV1 {
    serde_json::from_value(envelope_fixture()).expect("decode AgentDecisionEnvelopeV1 fixture")
}

#[derive(Debug, Clone, PartialEq)]
struct WorldFingerprint {
    snapshot: Snapshot,
    journal: Journal,
    state_root: String,
}

fn fingerprint(world: &World) -> WorldFingerprint {
    WorldFingerprint {
        snapshot: world.snapshot(),
        journal: world.journal().clone(),
        state_root: world.current_state_root_hash().expect("state root"),
    }
}

fn assert_unchanged(world: &World, before: &WorldFingerprint) {
    assert_eq!(
        &fingerprint(world),
        before,
        "rejected envelope changed world truth"
    );
}

fn validation_code(world: &World, value: Value) -> String {
    let candidate: AgentDecisionEnvelopeV1 =
        serde_json::from_value(value).expect("decode envelope candidate");
    MvccValidator::validate(world, &candidate)
        .expect_err("invalid candidate must be rejected")
        .code()
        .to_string()
}

fn assert_rejected_without_effect(mut candidate: Value, expected_code: &str) {
    let world = World::new();
    let before = fingerprint(&world);
    assert_eq!(validation_code(&world, candidate.take()), expected_code);
    assert_unchanged(&world, &before);
}

#[test]
fn mvcc_validation_order_is_schema_then_identity_then_idempotency_then_head() {
    let world = World::new();
    let mut candidate = envelope_fixture();
    candidate["schema_version"] = json!("agent-decision-envelope.v999");
    candidate["world_id"] = json!("other-world");
    candidate["envelope_idempotency_key"] = json!("same-key-different-digest");
    candidate["base_tick"] = json!(u64::MAX);
    candidate["capability_snapshot_hash"] = json!("blake3:stale-capability");
    candidate["preconditions"] = json!([{"path_or_rule": "unknown.path", "operator": "eq"}]);

    assert_eq!(
        validation_code(&world, candidate),
        "unsupported_schema_version"
    );
}

#[test]
fn stale_head_finality_reorg_and_capability_rejections_have_no_effect() {
    let mut stale_tick = envelope_fixture();
    stale_tick["base_tick"] = json!(u64::MAX);
    assert_rejected_without_effect(stale_tick, "stale_base");

    let mut stale_hash = envelope_fixture();
    stale_hash["base_world_hash"] = json!("blake3:stale-parent");
    assert_rejected_without_effect(stale_hash, "stale_base");

    let mut stale_finality = envelope_fixture();
    stale_finality["finality_epoch"] = json!(999);
    assert_rejected_without_effect(stale_finality, "reorg_invalidated");

    let mut stale_reorg = envelope_fixture();
    stale_reorg["reorg_epoch"] = json!(999);
    assert_rejected_without_effect(stale_reorg, "reorg_invalidated");

    let mut stale_capability = envelope_fixture();
    stale_capability["capability_snapshot_hash"] = json!("blake3:stale-capability");
    assert_rejected_without_effect(stale_capability, "stale_capability_snapshot");
}

#[test]
fn precondition_registry_accepts_supported_paths_and_declared_operators() {
    let numeric_paths = [
        "world.logical_tick",
        "world.reorg_epoch",
        "agent.resource.data",
    ];
    for path in numeric_paths {
        for operator in ["eq", "neq", "lt", "lte", "gt", "gte"] {
            let condition: PreconditionV1 = serde_json::from_value(json!({
                "schema_version": 1,
                "subject": {"kind": "agent", "id": AGENT_ID},
                "path_or_rule": path,
                "operator": operator,
                "expected_value_bytes": [1],
                "missing_behavior": "fail"
            }))
            .expect("decode supported numeric precondition");
            MvccValidator::validate_precondition_shape(&condition)
                .expect("supported numeric precondition shape");
        }
    }

    for path in [
        "world.state_root",
        "world.runtime_manifest_hash",
        "agent.status",
        "agent.position",
        "agent.inventory_digest",
        "agent.capability_snapshot_hash",
        "intent.status",
    ] {
        for operator in ["eq", "neq"] {
            let condition: PreconditionV1 = serde_json::from_value(json!({
                "schema_version": 1,
                "subject": {"kind": "world", "id": WORLD_ID},
                "path_or_rule": path,
                "operator": operator,
                "expected_value_bytes": [1],
                "missing_behavior": "fail"
            }))
            .expect("decode supported equality precondition");
            MvccValidator::validate_precondition_shape(&condition)
                .expect("supported equality precondition shape");
        }
    }
}

#[test]
fn runtime_identity_and_resource_registry_reject_noncanonical_or_unknown_values() {
    let mut decomposed = envelope_fixture();
    decomposed["agent_id"] = json!("Cafe\u{301}");
    assert_rejected_without_effect(decomposed, "invalid_identity");

    let mut whitespace_subject = envelope_fixture();
    whitespace_subject["preconditions"] = json!([{
        "schema_version": 1,
        "subject": {"kind": "world", "id": "   "},
        "path_or_rule": "world.logical_tick",
        "operator": "eq",
        "expected_value_bytes": [1],
        "missing_behavior": "fail"
    }]);
    assert_rejected_without_effect(whitespace_subject, "precondition_failed");

    let unknown_resource: PreconditionV1 = serde_json::from_value(json!({
        "schema_version": 1,
        "subject": {"kind": "agent", "id": AGENT_ID},
        "path_or_rule": "agent.resource.oxygen",
        "operator": "eq",
        "expected_value_bytes": [1],
        "missing_behavior": "fail"
    }))
    .expect("decode unknown resource precondition");
    assert!(
        MvccValidator::validate_precondition_shape(&unknown_resource).is_err(),
        "unknown resource paths must not pass generic shape validation"
    );
}

#[test]
fn unknown_type_malformed_duplicate_and_bounded_preconditions_fail_closed_without_effect() {
    let mut unknown_path = envelope_fixture();
    unknown_path["preconditions"] = json!([{
        "schema_version": 1,
        "subject": {"kind": "world", "id": WORLD_ID},
        "path_or_rule": "world.unknown",
        "operator": "eq",
        "expected_value_bytes": [1],
        "missing_behavior": "fail"
    }]);
    assert_rejected_without_effect(unknown_path, "precondition_failed");

    let mut unknown_operator = envelope_fixture();
    unknown_operator["preconditions"] = json!([{
        "schema_version": 1,
        "subject": {"kind": "world", "id": WORLD_ID},
        "path_or_rule": "world.logical_tick",
        "operator": "contains",
        "expected_value_bytes": [1],
        "missing_behavior": "fail"
    }]);
    assert_rejected_without_effect(unknown_operator, "precondition_failed");

    let mut malformed = envelope_fixture();
    malformed["preconditions"] = json!([{
        "schema_version": 1,
        "subject": {"kind": "world", "id": WORLD_ID},
        "path_or_rule": "world.logical_tick",
        "operator": "eq",
        "expected_value_bytes": [255],
        "missing_behavior": "fail"
    }]);
    assert_rejected_without_effect(malformed, "precondition_failed");

    let duplicate = json!({
        "schema_version": 1,
        "subject": {"kind": "world", "id": WORLD_ID},
        "path_or_rule": "world.logical_tick",
        "operator": "eq",
        "expected_value_bytes": [1],
        "missing_behavior": "fail"
    });
    let mut duplicate_envelope = envelope_fixture();
    duplicate_envelope["preconditions"] = json!([duplicate.clone(), duplicate]);
    assert_rejected_without_effect(duplicate_envelope, "precondition_failed");

    let mut over_limit = envelope_fixture();
    over_limit["preconditions"] = json!(
        (0..33)
            .map(|_| json!({
                "schema_version": 1,
                "subject": {"kind": "world", "id": WORLD_ID},
                "path_or_rule": "world.logical_tick",
                "operator": "eq",
                "expected_value_bytes": [1],
                "missing_behavior": "fail"
            }))
            .collect::<Vec<_>>()
    );
    assert_rejected_without_effect(over_limit, "precondition_failed");
}

#[test]
fn idempotency_same_key_same_digest_replays_and_collision_has_no_effect() {
    let mut world = World::new();
    let mut validator = MvccValidator::default();
    let first = envelope();
    let first_result = validator
        .submit(&mut world, first.clone())
        .expect("first envelope disposition");
    let before_collision = fingerprint(&world);
    let replay = validator
        .submit(&mut world, first)
        .expect("same digest is idempotent replay");
    assert_eq!(replay, first_result);

    let mut collision_value = envelope_fixture();
    collision_value["envelope_digest"] = json!("blake3:collision");
    let collision: AgentDecisionEnvelopeV1 =
        serde_json::from_value(collision_value).expect("decode collision envelope");
    let error = validator
        .submit(&mut world, collision)
        .expect_err("same key/different digest must fail closed");
    assert_eq!(error.code(), "idempotency_conflict");
    assert_unchanged(&world, &before_collision);
}

#[test]
fn bounded_mailbox_enqueue_is_nonblocking_and_enforces_per_agent_single_flight() {
    let mut mailbox = AgentCognitionMailbox::with_capacity(1, 1);
    mailbox
        .try_enqueue(envelope())
        .expect("first request enters bounded mailbox");

    let mut second_session = envelope_fixture();
    second_session["agent_session_id"] = json!("session.agent-1.v2");
    second_session["agent_turn_id"] = json!("turn.agent-1.8");
    second_session["decision_request_id"] = json!("request.agent-1.8");
    let busy: AgentDecisionEnvelopeV1 =
        serde_json::from_value(second_session).expect("decode reentrant request");
    let error = mailbox
        .try_enqueue(busy)
        .expect_err("same Agent cannot hold two active turns");
    assert_eq!(error.code(), "agent_busy");

    let mut other_agent = envelope_fixture();
    other_agent["agent_id"] = json!("agent-2");
    other_agent["agent_session_id"] = json!("session.agent-2.v1");
    other_agent["agent_turn_id"] = json!("turn.agent-2.1");
    other_agent["decision_request_id"] = json!("request.agent-2.1");
    mailbox
        .try_enqueue(serde_json::from_value(other_agent).expect("decode other-agent request"))
        .expect_err("capacity is bounded and enqueue must return immediately");
}

#[test]
fn provider_delay_cannot_block_world_step_or_create_an_effect_before_validation() {
    let world = World::new();
    let before = fingerprint(&world);
    let mut mailbox = AgentCognitionMailbox::with_capacity(1, 1);
    let pending = mailbox
        .try_enqueue(envelope())
        .expect("enqueue is a non-blocking operation");
    assert_eq!(pending.disposition(), "pending");
    assert_eq!(pending.provider_invocation_count(), 0);
    assert_unchanged(&world, &before);
}
