//! Real signed profile-2 parent enables formal birth; no synthetic scope token.
use super::*;
use crate::{CapabilityGrant, EffectOrigin, ReceiptSigner, RuntimeCommittedTickContext};
use crate::formal_read_fixtures as fixtures;
use oasis7_distfs::controlled_authority::formal::business_index::BusinessIndexNodeReader;
use oasis7_distfs::controlled_authority::formal::*;
use oasis7_distfs::controlled_authority::replicated_protocol::ArtifactRole;
use std::{collections::BTreeMap, sync::Arc};
struct Empty;
impl BusinessIndexNodeReader for Empty {
    fn read_node(&self, _: &str) -> Result<Option<Vec<u8>>, String> {
        Ok(None)
    }
}
fn formal_world() -> World {
    let (mut world, mut trust, anchor, proof) = fixtures::actual_world_initial_fixture();
    world.initialize_empty_formal_history().unwrap();
    world
        .capabilities
        .insert("cap".into(), CapabilityGrant::allow_all("cap"));
    world.set_policy(crate::PolicySet {
        rules: vec![crate::PolicyRule {
            when: crate::PolicyWhen {
                effect_kind: Some("http.request".into()),
                origin_kind: None,
                cap_name: Some("cap".into()),
            },
            decision: crate::PolicyDecision::Allow,
        }],
    });
    world.next_intent_id = u64::MAX;
    world.next_intent_id_era = 7;
    let bytes = serde_cbor::to_vec(&world.snapshot()).unwrap();
    let root = crate::blake3_hex(&bytes);
    let original = &proof.decision.record;
    let mut activation = SignedInitialActivationV1 {
        signature_hex: String::new(),
        body: InitialActivationBodyV1 {
            schema_version: 1,
            authority: anchor.authority().definition().clone(),
            recovery_policy: anchor.authority().policy().clone(),
            initial_state_root: root.clone(),
            runtime_manifest_digest: original.runtime_manifest_digest.clone(),
            governing_manifest_digest: original.governing_manifest_digest.clone(),
            execution_policy_digest: original.execution_policy_digest.clone(),
            artifact_roots: original.artifact_roots.clone(),
        },
    };
    activation.body.artifact_roots.remove(&ArtifactRole::Input);
    activation.body.authority.profile_version = 2;
    activation.body.initial_state_root = root.clone();
    activation
        .body
        .artifact_roots
        .insert(ArtifactRole::Snapshot, root.clone());
    activation.signature_hex = fixtures::sign(
        initial_activation_signing_bytes(&activation.body).unwrap(),
        9,
    );
    trust.initial_state_root = root.clone();
    let authority = verify_initial_activation(&activation, &trust).unwrap();
    let mut record = proof.decision.record.clone();
    record.original_authority_id = authority.authority_id().unwrap();
    record.activation_ref = authority.activation_ref().into();
    record.before_state_root = root.clone();
    record.after_state_root = root.clone();
    record.artifact_roots = activation.body.artifact_roots.clone();
    record.payload_digest =
        crate::blake3_hex(&initial_activation_input_bytes(&activation).unwrap());
    record
        .artifact_roots
        .insert(ArtifactRole::Input, record.payload_digest.clone());
    let proof = fixtures::qualified_proof(record, None, &authority, None, None, 4, 5, 6);
    let anchor = VerifiedFormalReadAnchorV1::from_initial(&activation, &trust, &proof).unwrap();
    let parent =
        crate::VerifiedFormalRetainedHistoryParentV1::verify(&anchor, &bytes).unwrap();
    world
        .attach_verified_formal_history(parent, Arc::new(Empty))
        .unwrap();
    let context = RuntimeCommittedTickContext {
        height: 1,
        slot: 0,
        epoch: 0,
        node_block_hash: "a".repeat(64),
        action_root: "fixture".into(),
        authority_node_id: world.tick_consensus_authority_source().into(),
        committed_at_unix_ms: 0,
    };
    world.begin_formal_execution_tick(&context).unwrap();
    world
}
#[test]
fn formal_effect_birth_rollover_keeps_same_payload_distinct_and_receipts_scoped() {
    let mut world = formal_world();
    let params =
        serde_json::json!({"target_id":"receiver", "receiver_contract_digest":"b".repeat(64)});
    world
        .emit_effect("http.request", params.clone(), "cap", EffectOrigin::System)
        .unwrap();
    world
        .emit_effect("http.request", params, "cap", EffectOrigin::System)
        .unwrap();
    let first = world.take_next_effect().unwrap();
    let second = world.take_next_effect().unwrap();
    assert_eq!(first.birth_era, Some(7));
    assert_eq!(second.birth_era, Some(8));
    assert_eq!(first.intent_id, format!("intent-{}", u64::MAX));
    assert_eq!(second.intent_id, "intent-1");
    let mut other_era = second.clone();
    other_era.birth_era = Some(7);
    world.pending_effects.push_back(other_era.clone());
    world.take_next_effect().unwrap();
    assert_eq!(world.inflight_effects.len(), 3);
    assert_ne!(other_era.identity_key(), second.identity_key());
    assert_ne!(
        other_era.payload_digest().unwrap(),
        second.payload_digest().unwrap()
    );
    let correct = world
        .receipt_for_effect(&second, "ok".into(), serde_json::json!({}), None)
        .unwrap();
    let key = world.effect_receipt_key(&correct).unwrap();
    assert_eq!(key, second.identity_key());
    let mut wrong = correct.clone();
    wrong.birth_era = None;
    wrong.world = None;
    wrong.intent_payload_digest = None;
    assert!(world.effect_receipt_key(&wrong).is_err());
    wrong = correct.clone();
    wrong.birth_era = Some(6);
    assert!(world.effect_receipt_key(&wrong).is_err());
    wrong = correct.clone();
    wrong.world.as_mut().unwrap().chain_id.push('x');
    assert!(world.effect_receipt_key(&wrong).is_err());
    wrong = correct.clone();
    wrong.intent_payload_digest = Some("c".repeat(64));
    assert!(world.effect_receipt_key(&wrong).is_err());
    let signer = ReceiptSigner::hmac_sha256(b"secret");
    let signature = signer.sign(&correct, 1, &"d".repeat(64)).unwrap();
    signer
        .verify(&correct, &signature, &BTreeMap::new(), 1, &"d".repeat(64))
        .unwrap();
    for wrong in [
        wrong,
        {
            let mut x = correct.clone();
            x.birth_era = Some(7);
            x
        },
        {
            let mut x = correct.clone();
            x.world.as_mut().unwrap().world_id.push('x');
            x
        },
    ] {
        assert!(
            signer
                .verify(&wrong, &signature, &BTreeMap::new(), 1, &"d".repeat(64))
                .is_err()
        );
    }
    let before = serde_cbor::to_vec(&world.snapshot()).unwrap();
    let mut legacy = correct.clone();
    legacy.birth_era = None;
    legacy.world = None;
    legacy.intent_payload_digest = None;
    assert!(world.ingest_receipt(legacy).is_err());
    assert_eq!(before, serde_cbor::to_vec(&world.snapshot()).unwrap());
    world.set_receipt_signer(signer);
    let receipt_event_id = world.ingest_receipt(correct).unwrap();
    let mut completed = crate::AgentIntentV2 {
        schema_version: 2,
        agent_id: "agent".into(),
        intent_id: "agent-request".into(),
        kind: "chat_instruction".into(),
        summary: "complete".into(),
        target_id: None,
        effect_intent_id: Some(second.intent_id.clone()),
        intent_tick: Some(0),
        world_id: Some("world".into()),
        reorg_epoch: Some(0),
        authority_scope: Some("scope".into()),
        status: "completed".into(),
        source: "player_agent_chat".into(),
        logical_time: 0,
        event_seq: receipt_event_id + 1,
        updated_at: 1,
        receipt_ref: Some(format!("world-event:{receipt_event_id}")),
        reason_code: None,
        reason_summary: None,
        replaced_by: None,
        actor_id: "actor".into(),
        request_digest: "f".repeat(64),
    };
    let event = crate::DomainEvent::AgentIntentTransitioned {
        intent: completed.clone(),
    };
    assert!(
        world
            .validate_agent_intent_receipt_reference(&event, Some(receipt_event_id + 1))
            .is_err(),
        "old raw AgentIntent reference must not bind a new-era receipt"
    );
    completed.effect_intent_id = Some(other_era.identity_key());
    let event = crate::DomainEvent::AgentIntentTransitioned {
        intent: completed.clone(),
    };
    assert!(
        world
            .validate_agent_intent_receipt_reference(&event, Some(receipt_event_id + 1))
            .is_err(),
        "same raw id with wrong era must not complete"
    );
    completed.effect_intent_id = Some(second.identity_key());
    let event = crate::DomainEvent::AgentIntentTransitioned { intent: completed };
    assert_eq!(
        world
            .validate_agent_intent_receipt_reference(&event, Some(receipt_event_id + 1))
            .unwrap(),
        Some(receipt_event_id)
    );
    assert!(!world.inflight_effects.contains_key(&second.identity_key()));
    assert!(
        world
            .inflight_effects
            .contains_key(&other_era.identity_key())
    );
    let snapshot = world.snapshot();
    let restored = World::from_snapshot(snapshot.clone(), world.journal().clone()).unwrap();
    assert_eq!(
        serde_cbor::to_vec(&snapshot).unwrap(),
        serde_cbor::to_vec(&restored.snapshot()).unwrap()
    );
}
#[test]
fn legacy_effect_none_roundtrips_original_cbor_and_never_guesses_current_era() {
    #[derive(serde::Serialize)]
    struct OldIntent<'a> {
        intent_id: &'a str,
        kind: &'a str,
        params: serde_json::Value,
        cap_ref: &'a str,
        origin: EffectOrigin,
    }
    #[derive(serde::Serialize)]
    struct OldReceipt<'a> {
        intent_id: &'a str,
        status: &'a str,
        payload: serde_json::Value,
        cost_cents: Option<u64>,
        signature: Option<crate::ReceiptSignature>,
    }
    let bytes = serde_cbor::to_vec(&OldIntent {
        intent_id: "intent-1",
        kind: "http.request",
        params: serde_json::json!({}),
        cap_ref: "cap",
        origin: EffectOrigin::System,
    })
    .unwrap();
    let intent: EffectIntent = serde_cbor::from_slice(&bytes).unwrap();
    assert_eq!(intent.birth_era, None);
    assert_eq!(intent.identity_key(), "intent-1");
    assert_eq!(bytes, serde_cbor::to_vec(&intent).unwrap());
    let bytes = serde_cbor::to_vec(&OldReceipt {
        intent_id: "intent-1",
        status: "ok",
        payload: serde_json::json!({}),
        cost_cents: None,
        signature: None,
    })
    .unwrap();
    let receipt: EffectReceipt = serde_cbor::from_slice(&bytes).unwrap();
    assert_eq!(bytes, serde_cbor::to_vec(&receipt).unwrap());
    let mut world = World::new();
    world.next_intent_id_era = 9;
    assert_eq!(world.effect_birth_era_for(9), None);
    world.pending_effects.push_back(intent);
    assert_eq!(world.effect_receipt_key(&receipt).unwrap(), "intent-1");
    let signer = ReceiptSigner::hmac_sha256(b"legacy");
    let signature = signer.sign(&receipt, 1, &"e".repeat(64)).unwrap();
    signer
        .verify(&receipt, &signature, &BTreeMap::new(), 1, &"e".repeat(64))
        .unwrap();
}

#[test]
fn formal_effect_birth_linked_queue_survives_lowered_limit_and_replay_admission() {
    let mut world = formal_world();
    for _ in 0..2 {
        world
            .emit_effect(
                "http.request",
                serde_json::json!({}),
                "cap",
                EffectOrigin::System,
            )
            .unwrap();
    }
    let originals = world.pending_effects.clone();
    for intent in &originals {
        world.capability_effect_receipt_links.insert(
            intent.identity_key(),
            crate::CapabilityEffectReceiptLink {
                authorization_receipt_id: "original-debit".into(),
            },
        );
    }
    world.runtime_memory_limits.max_pending_effects = 1;
    world.enforce_pending_effect_limit();
    assert_eq!(
        world.pending_effects, originals,
        "qualified debit obligations cannot be evicted"
    );
    let mut extra = originals[0].clone();
    extra.intent_id = "intent-99".into();
    assert!(world.push_pending_effect_bounded(extra).is_err());
    assert_eq!(world.pending_effects, originals);
}
