//! LIVE-2 RED fixtures for World-owned cognition MVCC validation.
//!
//! Unlike the compatibility fixtures in `agent_cognition_mvcc`, these inputs
//! are built from the actual World head, manifest, capability root, and
//! AgentIntentV2 state.  The target validator must compare those values in the
//! documented fixed order and leave the world untouched on every rejection.

use super::super::*;
use oasis7_agent_api::world_action::GeoPos;
use crate::agent_state::AgentState;
use crate::cognition::world_state_binding_digest_v1;
use crate::cognition_recovery::cognition_digest_v1;
use crate::util::hash_json;
use crate::world::derive_agent_chat_request_digest;
use serde_json::{Value, json};

const WORLD_ID: &str = "world-live-mvcc";
const AGENT_ID: &str = "agent-live-mvcc";
const BRANCH_ID: &str = "main";
const INTENT_ID: &str = "intent-live-mvcc";
const NOW: u64 = 17;
const FINALITY_BLOCK_HASH: &str =
    "blake3:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const REQUEST_DIGEST: &str =
    "blake3:1111111111111111111111111111111111111111111111111111111111111111";
const OBSERVATION_DIGEST: &str =
    "blake3:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd";
const CONTEXT_DIGEST: &str =
    "blake3:3333333333333333333333333333333333333333333333333333333333333333";
const INTENT_MESSAGE: &str = "live cognition request";

fn live_world() -> World {
    live_world_at(NOW, 4)
}

fn live_world_at(time: u64, reorg_epoch: u64) -> World {
    let mut state = WorldState {
        time,
        ..WorldState::default()
    };
    state.agents.insert(
        AGENT_ID.to_string(),
        AgentCell::new(AgentState::new(AGENT_ID, GeoPos::new(10, 20, 0)), time),
    );
    let authority = AgentIntentAuthorityContext {
        intent_tick: Some(time),
        world_id: Some(WORLD_ID.to_string()),
        reorg_epoch: Some(reorg_epoch),
        authority_scope: Some("agent:live-mvcc".to_string()),
        replaces_intent_id: None,
    };
    let intent_request_digest = derive_agent_chat_request_digest(
        "player-live-mvcc",
        AGENT_ID,
        1,
        INTENT_MESSAGE,
        &authority,
    );
    let intent = AgentIntentV2 {
        schema_version: AGENT_INTENT_V2_SCHEMA_VERSION,
        agent_id: AGENT_ID.to_string(),
        intent_id: INTENT_ID.to_string(),
        kind: "agent_chat".to_string(),
        summary: "Agent guidance accepted; the Agent will evaluate its next world action."
            .to_string(),
        target_id: None,
        effect_intent_id: None,
        intent_tick: Some(time),
        world_id: Some(WORLD_ID.to_string()),
        reorg_epoch: Some(reorg_epoch),
        authority_scope: Some("agent:live-mvcc".to_string()),
        status: "accepted".to_string(),
        source: "player".to_string(),
        logical_time: time,
        event_seq: 3,
        updated_at: time,
        receipt_ref: None,
        reason_code: None,
        reason_summary: None,
        replaced_by: None,
        actor_id: "player-live-mvcc".to_string(),
        request_digest: intent_request_digest,
    };
    state.agents.get_mut(AGENT_ID).expect("live agent").intent = Some(intent.clone());
    state
        .agent_intent_ledger
        .insert(INTENT_ID.to_string(), intent);
    World::new_with_state(state)
}

fn precondition_for_tick(expected_tick: u64) -> Value {
    precondition_for("world.logical_tick", expected_tick)
}

fn precondition_for(path: &str, expected: u64) -> Value {
    precondition_for_operator(path, "eq", expected)
}

fn precondition_for_operator(path: &str, operator: &str, expected: u64) -> Value {
    json!({
        "schema_version": 1,
        "subject": {"kind": "world", "id": WORLD_ID},
        "path_or_rule": path,
        "operator": operator,
        "expected_value_bytes": serde_cbor::to_vec(&expected).expect("encode precondition"),
        "missing_behavior": "fail"
    })
}

fn envelope_value(world: &World, precondition: Option<Value>) -> Value {
    envelope_value_with_reorg(world, precondition, 4)
}

fn envelope_value_with_reorg(
    world: &World,
    precondition: Option<Value>,
    reorg_epoch: u64,
) -> Value {
    let state_root = world.current_state_root_hash().expect("live state root");
    let manifest_hash = world.current_manifest_hash().expect("live manifest hash");
    let capability_root = world.capability_authorization_root().to_string();
    let authority_context_hash =
        hash_json(&world.capability_revocation_state()).expect("live authority context hash");
    let base_world_hash = world_state_binding_digest_v1(
        WORLD_ID,
        BRANCH_ID,
        4,
        Some(FINALITY_BLOCK_HASH),
        "verified",
        world.state().time,
        &state_root,
        reorg_epoch,
        &manifest_hash,
    );
    let runtime_manifest_hash = cognition_digest_v1("oasis7.runtime.manifest.v1", &manifest_hash);
    let mut value = json!({
        "schema_version": AGENT_DECISION_ENVELOPE_V1_SCHEMA,
        "world_id": WORLD_ID,
        "agent_id": AGENT_ID,
        "branch_id": BRANCH_ID,
        "finality_epoch": 4,
        "finality_block_hash": FINALITY_BLOCK_HASH,
        "finality_status": "verified",
        "agent_session_id": "session.live-mvcc",
        "agent_turn_id": "turn.live-mvcc",
        "decision_request_id": "request.live-mvcc",
        "retry_seq": 0,
        "base_tick": world.state().time,
        "base_world_hash": base_world_hash,
        "reorg_epoch": reorg_epoch,
        "runtime_manifest_hash": runtime_manifest_hash,
        "capability_snapshot_hash": capability_root,
        "authority_context_hash": authority_context_hash,
        "observation_digest": OBSERVATION_DIGEST,
        "context_digest": CONTEXT_DIGEST,
        "issued_at_tick": world.state().time,
        "valid_until_tick": world.state().time + 5,
        "preconditions": precondition.into_iter().collect::<Vec<_>>(),
        "decision_kind": "wait",
        "action": {"type": "wait"},
        // request_digest is an opaque Agent-owned input.  The remaining
        // identity fields are filled from the production canonical helpers.
        "request_digest": REQUEST_DIGEST,
        "decision_digest": "",
        "envelope_digest": "",
        "provider_invocation_key": "",
        "envelope_idempotency_key": "",
        "origin_intent_ref": {
            "intent_id": INTENT_ID,
            "request_digest": world
                .state()
                .agent_intent_ledger
                .get(INTENT_ID)
                .expect("live intent")
                .request_digest
        },
        "source": "provider"
    });
    let mut envelope: AgentDecisionEnvelopeV1 =
        serde_json::from_value(value.take()).expect("decode derivation seed");
    envelope.provider_invocation_key = envelope.derive_provider_invocation_key();
    envelope.decision_digest = envelope.derive_decision_digest();
    envelope.envelope_digest = envelope.derive_envelope_digest();
    envelope.envelope_idempotency_key = envelope.derive_envelope_idempotency_key();
    serde_json::to_value(envelope).expect("encode derived envelope")
}

fn envelope(world: &World) -> AgentDecisionEnvelopeV1 {
    serde_json::from_value(envelope_value(world, Some(precondition_for_tick(NOW))))
        .expect("decode live envelope")
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

#[test]
fn fresh_live_world_envelope_is_accepted_against_real_authoritative_state() {
    let world = live_world();
    let candidate = envelope(&world);
    MvccValidator::validate(&world, &candidate)
        .expect("fresh candidate should match the real World head");
}

#[test]
fn unsigned_world_tick_and_reorg_preconditions_accept_values_above_i64_max() {
    let high_tick = i64::MAX as u64 + 1;
    let world = live_world_at(high_tick, 4);
    let candidate: AgentDecisionEnvelopeV1 = serde_json::from_value(envelope_value(
        &world,
        Some(precondition_for_tick(high_tick)),
    ))
    .expect("decode high tick envelope");
    MvccValidator::validate(&world, &candidate)
        .expect("u64 logical tick equality must not downcast through i64");
    let candidate: AgentDecisionEnvelopeV1 = serde_json::from_value(envelope_value(
        &world,
        Some(precondition_for_operator(
            "world.logical_tick",
            "gte",
            i64::MAX as u64,
        )),
    ))
    .expect("decode high tick ordering envelope");
    MvccValidator::validate(&world, &candidate)
        .expect("u64 logical tick ordering must remain unsigned");

    let high_reorg = i64::MAX as u64 + 1;
    let world = live_world_at(NOW, high_reorg);
    let candidate: AgentDecisionEnvelopeV1 = serde_json::from_value(envelope_value_with_reorg(
        &world,
        Some(precondition_for("world.reorg_epoch", high_reorg)),
        high_reorg,
    ))
    .expect("decode high reorg envelope");
    MvccValidator::validate(&world, &candidate)
        .expect("u64 reorg epoch equality must not downcast through i64");
    let candidate: AgentDecisionEnvelopeV1 = serde_json::from_value(envelope_value_with_reorg(
        &world,
        Some(precondition_for_operator(
            "world.reorg_epoch",
            "gt",
            i64::MAX as u64,
        )),
        high_reorg,
    ))
    .expect("decode high reorg ordering envelope");
    MvccValidator::validate(&world, &candidate)
        .expect("u64 reorg epoch ordering must remain unsigned");
}

#[test]
fn fixed_order_uses_real_world_head_before_capability_authority_and_preconditions() {
    let world = live_world();
    let before = fingerprint(&world);
    let mut candidate = envelope_value(&world, Some(precondition_for_tick(NOW + 1)));
    candidate["base_tick"] = json!(NOW + 1);
    candidate["base_world_hash"] = json!("blake3:valid-but-wrong-parent");
    candidate["capability_snapshot_hash"] = json!("blake3:valid-but-wrong-capability");
    candidate["authority_context_hash"] = json!("blake3:valid-but-wrong-authority");
    let candidate: AgentDecisionEnvelopeV1 =
        serde_json::from_value(candidate).expect("decode multi-conflict envelope");

    let error = MvccValidator::validate(&world, &candidate)
        .expect_err("stale real parent must fail before later checks");
    assert_eq!(error.code(), "stale_base");
    assert_eq!(fingerprint(&world), before);
}

#[test]
fn preconditions_are_evaluated_against_the_same_committed_world_snapshot() {
    let world = live_world();
    let candidate: AgentDecisionEnvelopeV1 =
        serde_json::from_value(envelope_value(&world, Some(precondition_for_tick(NOW + 1))))
            .expect("decode mismatched precondition");

    let error = MvccValidator::validate(&world, &candidate)
        .expect_err("wrong live tick precondition must fail closed");
    assert_eq!(error.code(), "precondition_failed");
}

#[test]
fn world_tick_and_state_root_change_invalidates_the_old_envelope_without_effect() {
    let mut world = live_world();
    let candidate = envelope(&world);
    let before = fingerprint(&world);
    world.step().expect("advance the real runtime world");

    let error = MvccValidator::validate(&world, &candidate)
        .expect_err("envelope bound to the prior live head must be stale");
    assert_eq!(error.code(), "stale_base");
    assert_ne!(
        fingerprint(&world),
        before,
        "the world tick itself advanced"
    );
}

#[test]
fn changed_reorg_epoch_invalidates_uncommitted_turn_before_first_effect() {
    let world = live_world();
    let mut candidate = envelope_value(&world, Some(precondition_for_tick(NOW)));
    candidate["reorg_epoch"] = json!(5);
    let candidate: AgentDecisionEnvelopeV1 =
        serde_json::from_value(candidate).expect("decode reorg-invalidated envelope");
    let before = fingerprint(&world);

    let error = MvccValidator::validate(&world, &candidate)
        .expect_err("reorg change must invalidate the uncommitted turn");
    assert_eq!(error.code(), "reorg_invalidated");
    assert_eq!(fingerprint(&world), before);
}

#[test]
fn agent_intent_binding_mismatch_is_rejected_without_world_effect() {
    let world = live_world();
    let mut candidate = envelope_value(&world, Some(precondition_for_tick(NOW)));
    candidate["origin_intent_ref"]["request_digest"] = json!("blake3:forged-intent");
    let candidate: AgentDecisionEnvelopeV1 =
        serde_json::from_value(candidate).expect("decode forged intent envelope");
    let before = fingerprint(&world);

    let error = MvccValidator::validate(&world, &candidate)
        .expect_err("AgentIntentV2 request digest mismatch must fail closed");
    assert_eq!(error.code(), "intent_conflict");
    assert_eq!(fingerprint(&world), before);
}
