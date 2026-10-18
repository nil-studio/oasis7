use super::super::*;
use ed25519_dalek::SigningKey;
use serde_json::json;
use std::collections::BTreeMap;

#[test]
fn effect_pipeline_signs_receipt() {
    let mut world = World::new();
    world.add_capability(CapabilityGrant::allow_all("cap_all"));
    world.set_policy(PolicySet::allow_all());
    world.set_receipt_signer(ReceiptSigner::hmac_sha256(b"secret"));

    let intent_id = world
        .emit_effect(
            "http.request",
            json!({"url": "https://example.com"}),
            "cap_all",
            EffectOrigin::System,
        )
        .unwrap();

    let intent = world.take_next_effect().unwrap();
    assert_eq!(intent.intent_id, intent_id);

    let receipt = EffectReceipt {
        birth_era: None,
        world: None,
        intent_payload_digest: None,
        intent_id: intent_id.clone(),
        status: "ok".to_string(),
        payload: json!({"status": 200}),
        cost_cents: Some(5),
        signature: None,
    };

    world.ingest_receipt(receipt).unwrap();

    let event = world.journal().events.last().unwrap();
    match &event.body {
        WorldEventBody::ReceiptAppended(receipt) => {
            assert!(receipt.signature.is_some());
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

#[test]
fn policy_denies_effect() {
    let mut world = World::new();
    world.add_capability(CapabilityGrant::allow_all("cap_all"));
    world.set_policy(PolicySet {
        rules: vec![PolicyRule {
            when: PolicyWhen {
                effect_kind: Some("http.request".to_string()),
                origin_kind: None,
                cap_name: None,
            },
            decision: PolicyDecision::Deny {
                reason: "blocked".to_string(),
            },
        }],
    });

    let err = world
        .emit_effect(
            "http.request",
            json!({"url": "https://example.com"}),
            "cap_all",
            EffectOrigin::System,
        )
        .unwrap_err();

    assert!(matches!(err, WorldError::PolicyDenied { .. }));

    let event = world.journal().events.last().unwrap();
    match &event.body {
        WorldEventBody::PolicyDecisionRecorded(record) => {
            assert!(matches!(record.decision, PolicyDecision::Deny { .. }));
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

#[test]
fn pending_effects_are_bounded_and_track_evictions() {
    let mut world = World::new().with_runtime_memory_limits(WorldRuntimeMemoryLimits {
        max_pending_effects: 1,
        ..WorldRuntimeMemoryLimits::default()
    });
    world.add_capability(CapabilityGrant::allow_all("cap_all"));
    world.set_policy(PolicySet::allow_all());

    let first_intent = world
        .emit_effect(
            "http.request",
            json!({"url": "https://example.com/a"}),
            "cap_all",
            EffectOrigin::System,
        )
        .expect("emit first");
    let second_intent = world
        .emit_effect(
            "http.request",
            json!({"url": "https://example.com/b"}),
            "cap_all",
            EffectOrigin::System,
        )
        .expect("emit second");

    assert_eq!(world.pending_effects_len(), 1);
    assert_eq!(
        world.runtime_backpressure_stats().pending_effects_evicted,
        1
    );
    let next = world.take_next_effect().expect("one pending effect");
    assert_ne!(next.intent_id, first_intent);
    assert_eq!(next.intent_id, second_intent);
}

#[test]
fn inflight_effect_dispatch_respects_capacity_limit() {
    let mut world = World::new().with_runtime_memory_limits(WorldRuntimeMemoryLimits {
        max_inflight_effects: 1,
        ..WorldRuntimeMemoryLimits::default()
    });
    world.add_capability(CapabilityGrant::allow_all("cap_all"));
    world.set_policy(PolicySet::allow_all());

    let first_intent = world
        .emit_effect(
            "http.request",
            json!({"url": "https://example.com/a"}),
            "cap_all",
            EffectOrigin::System,
        )
        .expect("emit first");
    let second_intent = world
        .emit_effect(
            "http.request",
            json!({"url": "https://example.com/b"}),
            "cap_all",
            EffectOrigin::System,
        )
        .expect("emit second");

    let first = world.take_next_effect().expect("first effect");
    assert_eq!(first.intent_id, first_intent);
    assert!(
        world.take_next_effect().is_none(),
        "dispatch should be blocked when inflight reaches limit"
    );
    assert_eq!(
        world
            .runtime_backpressure_stats()
            .inflight_effect_dispatch_blocked,
        1
    );

    world
        .ingest_receipt(EffectReceipt {
            birth_era: None,
            world: None,
            intent_payload_digest: None,
            intent_id: first_intent.clone(),
            status: "ok".to_string(),
            payload: json!({"status": 200}),
            cost_cents: Some(1),
            signature: None,
        })
        .expect("receipt");
    let second = world
        .take_next_effect()
        .expect("second effect after receipt");
    assert_eq!(second.intent_id, second_intent);
}

#[test]
fn effect_pipeline_signs_receipt_with_ed25519_anchor() {
    let mut world = World::new();
    world.add_capability(CapabilityGrant::allow_all("cap_all"));
    world.set_policy(PolicySet::allow_all());

    let signer_node_id = "receipt.signer.test.node.1";
    let signer_private_key_hex = deterministic_private_key_hex("receipt-ed25519-signer-1");
    let signer_public_key_hex = deterministic_public_key_hex("receipt-ed25519-signer-1");
    world
        .bind_node_identity(signer_node_id, signer_public_key_hex.as_str())
        .unwrap();
    world.set_receipt_signer(
        ReceiptSigner::ed25519(signer_node_id, signer_private_key_hex.as_str()).unwrap(),
    );

    let intent_id = world
        .emit_effect(
            "http.request",
            json!({"url": "https://example.com"}),
            "cap_all",
            EffectOrigin::System,
        )
        .unwrap();
    let _intent = world.take_next_effect().unwrap();
    world
        .ingest_receipt(EffectReceipt {
            birth_era: None,
            world: None,
            intent_payload_digest: None,
            intent_id,
            status: "ok".to_string(),
            payload: json!({"status": 200}),
            cost_cents: Some(5),
            signature: None,
        })
        .unwrap();

    let event = world.journal().events.last().unwrap();
    match &event.body {
        WorldEventBody::ReceiptAppended(receipt) => {
            let signature = receipt.signature.as_ref().expect("signed receipt");
            assert_eq!(signature.algorithm, SignatureAlgorithm::Ed25519);
            assert_eq!(signature.signer_node_id.as_deref(), Some(signer_node_id));
            assert_eq!(signature.participant_signatures.len(), 0);
            assert!(signature.consensus_height.is_some());
            assert_eq!(signature.receipts_root.as_ref().map(String::len), Some(64));
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

#[test]
fn ingest_receipt_rejects_ed25519_anchor_mismatch() {
    let mut world = World::new();
    world.add_capability(CapabilityGrant::allow_all("cap_all"));
    world.set_policy(PolicySet::allow_all());

    let signer_node_id = "receipt.signer.test.node.1";
    let signer_private_key_hex = deterministic_private_key_hex("receipt-ed25519-signer-anchor");
    let signer_public_key_hex = deterministic_public_key_hex("receipt-ed25519-signer-anchor");
    world
        .bind_node_identity(signer_node_id, signer_public_key_hex.as_str())
        .unwrap();
    let signer = ReceiptSigner::ed25519(signer_node_id, signer_private_key_hex.as_str()).unwrap();
    world.set_receipt_signer(signer.clone());

    let intent_id = world
        .emit_effect(
            "http.request",
            json!({"url": "https://example.com"}),
            "cap_all",
            EffectOrigin::System,
        )
        .unwrap();
    let _intent = world.take_next_effect().unwrap();

    let mut receipt = EffectReceipt {
        birth_era: None,
        world: None,
        intent_payload_digest: None,
        intent_id,
        status: "ok".to_string(),
        payload: json!({"status": 200}),
        cost_cents: Some(5),
        signature: None,
    };
    let invalid_root = "0".repeat(64);
    receipt.signature = Some(signer.sign(&receipt, 999, invalid_root.as_str()).unwrap());

    let err = world.ingest_receipt(receipt).unwrap_err();
    assert!(matches!(err, WorldError::ReceiptSignatureInvalid { .. }));
}

#[test]
fn effect_pipeline_signs_receipt_with_threshold_ed25519() {
    let mut world = World::new();
    world.add_capability(CapabilityGrant::allow_all("cap_all"));
    world.set_policy(PolicySet::allow_all());

    let mut signer_private_keys = BTreeMap::new();
    for (node_id, seed_label) in [
        ("receipt.threshold.node.1", "receipt-threshold-signer-1"),
        ("receipt.threshold.node.2", "receipt-threshold-signer-2"),
    ] {
        let private_key_hex = deterministic_private_key_hex(seed_label);
        let public_key_hex = deterministic_public_key_hex(seed_label);
        world
            .bind_node_identity(node_id, public_key_hex.as_str())
            .unwrap();
        signer_private_keys.insert(node_id.to_string(), private_key_hex);
    }
    world.set_receipt_signer(ReceiptSigner::threshold_ed25519(2, signer_private_keys).unwrap());

    let intent_id = world
        .emit_effect(
            "http.request",
            json!({"url": "https://example.com"}),
            "cap_all",
            EffectOrigin::System,
        )
        .unwrap();
    let _intent = world.take_next_effect().unwrap();
    world
        .ingest_receipt(EffectReceipt {
            birth_era: None,
            world: None,
            intent_payload_digest: None,
            intent_id,
            status: "ok".to_string(),
            payload: json!({"status": 200}),
            cost_cents: Some(5),
            signature: None,
        })
        .unwrap();

    let event = world.journal().events.last().unwrap();
    match &event.body {
        WorldEventBody::ReceiptAppended(receipt) => {
            let signature = receipt.signature.as_ref().expect("signed receipt");
            assert_eq!(signature.algorithm, SignatureAlgorithm::ThresholdEd25519);
            assert_eq!(signature.threshold, Some(2));
            assert_eq!(signature.participants.len(), 2);
            assert_eq!(signature.participant_signatures.len(), 2);
            assert!(signature.consensus_height.is_some());
            assert_eq!(signature.receipts_root.as_ref().map(String::len), Some(64));
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

fn deterministic_private_key_hex(seed_label: &str) -> String {
    crate::util::sha256_hex(seed_label.as_bytes())
}

fn deterministic_public_key_hex(seed_label: &str) -> String {
    let private_key_hex = deterministic_private_key_hex(seed_label);
    let private_key_bytes = hex::decode(private_key_hex).expect("decode private key hex");
    let private_key: [u8; 32] = private_key_bytes
        .as_slice()
        .try_into()
        .expect("private key length");
    let signing_key = SigningKey::from_bytes(&private_key);
    hex::encode(signing_key.verifying_key().to_bytes())
}
