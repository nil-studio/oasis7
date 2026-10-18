use super::*;
use ed25519_dalek::{Signer, SigningKey};

#[path = "persistence_recovery_compat_tests.rs"]
mod persistence_recovery_compat_tests;
#[path = "persistence_recovery_retention_tests.rs"]
mod persistence_recovery_retention_tests;

const ROLLBACK_NOW_MS: u64 = 1_720_000_000_000;

fn rollback_test_key(seed: u8) -> SigningKey {
    SigningKey::from_bytes(&[seed; 32])
}

fn rollback_authority_registry(
    on_call_key: &SigningKey,
    governance_key: &SigningKey,
) -> RollbackAuthorityRegistry {
    RollbackAuthorityRegistry::new([
        RollbackAuthorityRecord {
            authority_id: "on-call-alice".to_string(),
            role: RollbackAuthorityRole::OnCall,
            public_key_hex: hex::encode(on_call_key.verifying_key().to_bytes()),
            active: true,
        },
        RollbackAuthorityRecord {
            authority_id: "governance-bob".to_string(),
            role: RollbackAuthorityRole::Governance,
            public_key_hex: hex::encode(governance_key.verifying_key().to_bytes()),
            active: true,
        },
    ])
    .expect("valid rollback authority registry")
}

#[test]
fn snapshot_deserialization_rejects_rollback_registry_map_key_mismatch() {
    let on_call_key = rollback_test_key(7);
    let governance_key = rollback_test_key(9);
    let mut world = World::new();
    world
        .set_rollback_authority_registry(rollback_authority_registry(&on_call_key, &governance_key))
        .expect("configure rollback authorities");

    let mut snapshot = serde_json::to_value(world.snapshot()).expect("encode snapshot");
    let records = snapshot["rollback_authority_registry"]["records"]
        .as_object_mut()
        .expect("serialized registry records");
    let record = records
        .remove("on-call-alice")
        .expect("on-call registry record");
    records.insert("attacker-controlled-map-key".to_string(), record);

    let encoded = serde_json::to_string(&snapshot).expect("encode malformed snapshot");
    Snapshot::from_json(&encoded)
        .expect_err("registry map keys must exactly match normalized authority ids");
}

#[expect(
    clippy::too_many_arguments,
    reason = "Rollback authorization fixture keeps every signed identity field explicit."
)]
fn signed_rollback_authorization(
    snapshot: &Snapshot,
    target_journal_len: usize,
    expected_target_state_root: &str,
    target_batch_id: Option<&str>,
    ticket: &str,
    reason: &str,
    nonce: &str,
    on_call_key: &SigningKey,
    governance_key: &SigningKey,
) -> RollbackAuthorizationEnvelope {
    let intent = RollbackIntent {
        schema_version: 1,
        rollback_ticket: ticket.to_string(),
        snapshot_hash: util::hash_json(snapshot).expect("snapshot hash"),
        snapshot_journal_len: snapshot.journal_len,
        target_journal_len,
        target_journal_commitment: None,
        expected_target_state_root: expected_target_state_root.to_string(),
        target_batch_id: target_batch_id.map(str::to_string),
        reason: reason.to_string(),
        issued_at_ms: ROLLBACK_NOW_MS - 1_000,
        expires_at_ms: ROLLBACK_NOW_MS + 60_000,
        nonce: nonce.to_string(),
    };
    let payload = intent
        .canonical_signing_payload()
        .expect("canonical rollback signing payload");
    RollbackAuthorizationEnvelope {
        intent,
        signatures: vec![
            RollbackApprovalSignature {
                authority_id: "on-call-alice".to_string(),
                role: RollbackAuthorityRole::OnCall,
                signature_scheme: "ed25519".to_string(),
                signature_hex: hex::encode(on_call_key.sign(&payload).to_bytes()),
            },
            RollbackApprovalSignature {
                authority_id: "governance-bob".to_string(),
                role: RollbackAuthorityRole::Governance,
                signature_scheme: "ed25519".to_string(),
                signature_hex: hex::encode(governance_key.sign(&payload).to_bytes()),
            },
        ],
    }
}

#[test]
fn rollback_to_snapshot_resets_state() {
    let on_call_key = rollback_test_key(7);
    let governance_key = rollback_test_key(9);
    let mut world = World::new();
    world
        .set_rollback_authority_registry(rollback_authority_registry(&on_call_key, &governance_key))
        .expect("configure rollback authorities");
    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        pos: pos(0, 0),
    });
    world.step().unwrap();
    let snapshot = world.snapshot();
    let snapshot_state_root = world
        .current_state_root_hash()
        .expect("snapshot state root");

    world.submit_action(Action::MoveAgent {
        agent_id: "agent-1".to_string(),
        to: pos(9, 9),
    });
    world.step().unwrap();
    assert_eq!(
        world.state().agents.get("agent-1").unwrap().state.pos,
        pos(9, 9)
    );

    let journal = world.journal().clone();
    let approval = signed_rollback_authorization(
        &snapshot,
        snapshot.journal_len,
        snapshot_state_root.as_str(),
        None,
        "ROLLBACK-LOW-LEVEL-TEST",
        "test-rollback",
        "nonce-low-level-1",
        &on_call_key,
        &governance_key,
    );
    world
        .rollback_to_snapshot(
            snapshot.clone(),
            journal,
            "test-rollback",
            None,
            approval,
            ROLLBACK_NOW_MS,
        )
        .unwrap();

    assert_eq!(world.state(), &snapshot.state);
    let last = world.journal().events.last().unwrap();
    assert!(matches!(last.body, WorldEventBody::RollbackApplied(_)));
}

#[test]
fn rollback_with_reconciliation_recovers_from_detected_tick_consensus_drift() {
    let on_call_key = rollback_test_key(7);
    let governance_key = rollback_test_key(9);
    let mut world = World::new();
    world
        .set_rollback_authority_registry(rollback_authority_registry(&on_call_key, &governance_key))
        .expect("configure rollback authorities");
    world
        .bind_node_identity("relay.node.1", "relay-public-key-1")
        .expect("bind relay identity");
    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        pos: pos(0, 0),
    });
    world.step().expect("step");

    let stable_snapshot = world.snapshot();
    let stable_journal = world.journal().clone();
    let stable_state_root = world.current_state_root_hash().expect("stable state root");

    world
        .record_tick_consensus_propagation_for_tick(0, "relay.node.1")
        .expect("inject propagation record that breaks parent ordering");
    let drift = world
        .first_tick_consensus_drift()
        .expect("drift report should be present");
    assert_eq!(drift.tick, 0);
    assert!(
        drift.reason.contains("parent hash mismatch"),
        "unexpected drift reason: {}",
        drift.reason
    );
    world
        .verify_tick_consensus_chain()
        .expect_err("drifted chain should fail verification");

    let approval = signed_rollback_authorization(
        &stable_snapshot,
        stable_journal.len(),
        stable_state_root.as_str(),
        None,
        "ROLLBACK-2313",
        "reconcile-after-drift",
        "nonce-reconcile-1",
        &on_call_key,
        &governance_key,
    );
    world
        .rollback_to_snapshot_with_reconciliation(
            stable_snapshot,
            stable_journal,
            "reconcile-after-drift",
            None,
            approval,
            ROLLBACK_NOW_MS,
        )
        .expect("rollback with reconciliation");

    assert!(
        world.first_tick_consensus_drift().is_none(),
        "drift should be fully reconciled after rollback"
    );
    world
        .verify_tick_consensus_chain()
        .expect("reconciled chain should verify");

    let rollback = world
        .journal()
        .events
        .iter()
        .rev()
        .find_map(|event| match &event.body {
            WorldEventBody::RollbackApplied(rollback) => Some(rollback),
            _ => None,
        })
        .expect("rollback audit event");
    assert_eq!(rollback.rollback_ticket, "ROLLBACK-2313");
    assert_eq!(rollback.on_call_authority_id, "on-call-alice");
    assert_eq!(rollback.governance_authority_id, "governance-bob");
    assert_eq!(rollback.authorization_nonce, "nonce-reconcile-1");
}

#[test]
fn rollback_replays_authoritative_journal_suffix_to_the_target_state() {
    let on_call_key = rollback_test_key(7);
    let governance_key = rollback_test_key(9);
    let mut world = World::new();
    world
        .set_rollback_authority_registry(rollback_authority_registry(&on_call_key, &governance_key))
        .expect("configure rollback authorities");
    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        pos: pos(0, 0),
    });
    world.step().expect("create stable snapshot state");
    let stable_snapshot = world.snapshot();

    world.submit_action(Action::MoveAgent {
        agent_id: "agent-1".to_string(),
        to: pos(9, 9),
    });
    world.step().expect("create authoritative catch-up suffix");
    let target_state = world.state().clone();
    let target_journal = world.journal().clone();
    let target_state_root = world.current_state_root_hash().expect("target state root");
    assert!(target_journal.len() > stable_snapshot.journal_len);
    let approval = signed_rollback_authorization(
        &stable_snapshot,
        target_journal.len(),
        target_state_root.as_str(),
        None,
        "ROLLBACK-2313-REPLAY-TARGET",
        "replay-to-authoritative-target",
        "nonce-replay-target-1",
        &on_call_key,
        &governance_key,
    );

    world
        .rollback_to_snapshot_with_reconciliation(
            stable_snapshot,
            target_journal,
            "replay-to-authoritative-target",
            None,
            approval,
            ROLLBACK_NOW_MS,
        )
        .expect("rollback and deterministic catch-up must succeed");

    assert_eq!(
        world.state(),
        &target_state,
        "rollback must replay the authoritative post-snapshot suffix to its target state"
    );
}

#[test]
fn rollback_replay_rejects_tampered_suffix_atomically_without_consuming_nonce() {
    let on_call_key = rollback_test_key(7);
    let governance_key = rollback_test_key(9);
    let mut world = World::new();
    world
        .set_rollback_authority_registry(rollback_authority_registry(&on_call_key, &governance_key))
        .expect("configure rollback authorities");
    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        pos: pos(0, 0),
    });
    world.step().expect("create stable snapshot state");
    let stable_snapshot = world.snapshot();
    world.submit_action(Action::MoveAgent {
        agent_id: "agent-1".to_string(),
        to: pos(9, 9),
    });
    world.step().expect("create authoritative suffix");
    let snapshot_before = world.snapshot();
    let journal_before = world.journal().clone();
    let target_state_root = world.current_state_root_hash().expect("target state root");
    let mut tampered_journal = journal_before.clone();
    tampered_journal.events[stable_snapshot.journal_len].id = stable_snapshot.last_event_id;
    let approval = signed_rollback_authorization(
        &stable_snapshot,
        tampered_journal.len(),
        target_state_root.as_str(),
        None,
        "ROLLBACK-2313-TAMPERED-REPLAY",
        "reject-tampered-replay",
        "nonce-tampered-replay-1",
        &on_call_key,
        &governance_key,
    );

    world
        .rollback_to_snapshot_with_reconciliation(
            stable_snapshot,
            tampered_journal,
            "reject-tampered-replay",
            None,
            approval,
            ROLLBACK_NOW_MS,
        )
        .expect_err("tampered replay suffix must be rejected before commit");

    assert_eq!(world.snapshot(), snapshot_before);
    assert_eq!(world.journal(), &journal_before);
    assert!(
        !world
            .snapshot()
            .consumed_rollback_nonces
            .contains("nonce-tampered-replay-1")
    );
}

#[test]
fn rollback_replay_target_range_and_state_root_failures_are_atomic() {
    let on_call_key = rollback_test_key(7);
    let governance_key = rollback_test_key(9);
    for invalid_case in ["target_range", "target_state_root"] {
        let mut world = World::new();
        world
            .set_rollback_authority_registry(rollback_authority_registry(
                &on_call_key,
                &governance_key,
            ))
            .expect("configure rollback authorities");
        world.submit_action(Action::RegisterAgent {
            agent_id: "agent-1".to_string(),
            pos: pos(0, 0),
        });
        world.step().expect("create stable snapshot state");
        let stable_snapshot = world.snapshot();
        world.submit_action(Action::MoveAgent {
            agent_id: "agent-1".to_string(),
            to: pos(9, 9),
        });
        world.step().expect("create replay target");
        let target_journal = world.journal().clone();
        let target_state_root = world.current_state_root_hash().expect("target state root");
        let snapshot_before = world.snapshot();
        let journal_before = world.journal().clone();
        let nonce = format!("nonce-{invalid_case}");
        let approval = signed_rollback_authorization(
            &stable_snapshot,
            if invalid_case == "target_range" {
                target_journal.len() + 1
            } else {
                target_journal.len()
            },
            if invalid_case == "target_state_root" {
                "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
            } else {
                target_state_root.as_str()
            },
            None,
            "ROLLBACK-2313-TARGET-VALIDATION",
            "validate-signed-replay-target",
            nonce.as_str(),
            &on_call_key,
            &governance_key,
        );

        let error = world
            .rollback_to_snapshot_with_reconciliation(
                stable_snapshot,
                target_journal,
                "validate-signed-replay-target",
                None,
                approval,
                ROLLBACK_NOW_MS,
            )
            .expect_err("invalid signed replay target must fail atomically");
        assert!(
            matches!(
                (invalid_case, error),
                (
                    "target_range",
                    WorldError::RollbackReplayTargetInvalid { .. }
                ) | (
                    "target_state_root",
                    WorldError::RollbackTargetStateRootMismatch { .. }
                )
            ),
            "unexpected error for {invalid_case}"
        );
        assert_eq!(world.snapshot(), snapshot_before);
        assert_eq!(world.journal(), &journal_before);
        assert!(!world.snapshot().consumed_rollback_nonces.contains(&nonce));
    }
}

#[test]
fn rollback_reconciliation_drift_rejection_leaves_world_unchanged() {
    let on_call_key = rollback_test_key(7);
    let governance_key = rollback_test_key(9);
    let mut world = World::new();
    world
        .set_rollback_authority_registry(rollback_authority_registry(&on_call_key, &governance_key))
        .expect("configure rollback authorities");
    world
        .bind_node_identity("relay.node.1", "relay-public-key-1")
        .expect("bind relay identity");
    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        pos: pos(0, 0),
    });
    world.step().expect("step");
    world
        .record_tick_consensus_propagation_for_tick(0, "relay.node.1")
        .expect("inject propagation record that breaks parent ordering");
    assert!(world.first_tick_consensus_drift().is_some());
    let drifted_snapshot = world.snapshot();
    let drifted_journal = world.journal().clone();
    let drifted_state_root = world.current_state_root_hash().expect("drifted state root");

    world.submit_action(Action::MoveAgent {
        agent_id: "agent-1".to_string(),
        to: pos(9, 9),
    });
    world
        .step()
        .expect("mutate original world after target snapshot");
    let snapshot_before = world.snapshot();
    let journal_before = world.journal().clone();
    assert_ne!(
        snapshot_before, drifted_snapshot,
        "test precondition: rejected rollback candidate must differ from the current world"
    );
    let approval = signed_rollback_authorization(
        &drifted_snapshot,
        drifted_journal.len(),
        drifted_state_root.as_str(),
        None,
        "ROLLBACK-2313-DRIFT",
        "reject-drifted-candidate",
        "nonce-drift-rejected-1",
        &on_call_key,
        &governance_key,
    );

    let error = world
        .rollback_to_snapshot_with_reconciliation(
            drifted_snapshot,
            drifted_journal,
            "reject-drifted-candidate",
            None,
            approval,
            ROLLBACK_NOW_MS,
        )
        .expect_err("drifted rollback candidate must be rejected");
    assert!(
        matches!(
            error,
            WorldError::RollbackReconciliationFailed { ref reason, .. }
                if reason.contains("tick consensus parent hash mismatch")
        ),
        "expected drift validation rejection, got {error:?}"
    );

    assert_eq!(world.snapshot(), snapshot_before);
    assert_eq!(world.journal(), &journal_before);
    assert!(
        !world
            .snapshot()
            .consumed_rollback_nonces
            .contains("nonce-drift-rejected-1")
    );
}

#[test]
fn rollback_rejects_tampered_expired_or_same_key_authorization_before_mutation() {
    let on_call_key = rollback_test_key(7);
    let governance_key = rollback_test_key(9);
    for invalid_case in [
        "tampered_ticket",
        "tampered_target_journal_len",
        "tampered_target_state_root",
        "expired",
        "same_key_wrong_role",
        "target_mismatch",
    ] {
        let mut world = World::new();
        world
            .set_rollback_authority_registry(rollback_authority_registry(
                &on_call_key,
                &governance_key,
            ))
            .expect("configure rollback authorities");
        world.submit_action(Action::RegisterAgent {
            agent_id: "agent-1".to_string(),
            pos: pos(0, 0),
        });
        world.step().expect("step");
        let stable_snapshot = world.snapshot();
        let stable_journal = world.journal().clone();
        let stable_state_root = world.current_state_root_hash().expect("stable state root");

        world.submit_action(Action::MoveAgent {
            agent_id: "agent-1".to_string(),
            to: pos(9, 9),
        });
        world.step().expect("mutate after snapshot");
        let state_before = world.state().clone();
        let journal_before = world.journal().clone();

        let mut approval = signed_rollback_authorization(
            &stable_snapshot,
            stable_journal.len(),
            stable_state_root.as_str(),
            Some("batch-stable-1"),
            "ROLLBACK-2313",
            "invalid-authorization-must-not-mutate",
            invalid_case,
            &on_call_key,
            &governance_key,
        );
        match invalid_case {
            "tampered_ticket" => approval.intent.rollback_ticket.push_str("-tampered"),
            "tampered_target_journal_len" => approval.intent.target_journal_len += 1,
            "tampered_target_state_root" => {
                approval.intent.expected_target_state_root = "f".repeat(64)
            }
            "expired" => approval.intent.expires_at_ms = ROLLBACK_NOW_MS - 1,
            "same_key_wrong_role" => {
                let payload = approval
                    .intent
                    .canonical_signing_payload()
                    .expect("canonical payload");
                approval.signatures[1].authority_id = "on-call-alice".to_string();
                approval.signatures[1].role = RollbackAuthorityRole::Governance;
                approval.signatures[1].signature_hex =
                    hex::encode(on_call_key.sign(&payload).to_bytes());
            }
            "target_mismatch" => {}
            _ => unreachable!(),
        }

        let expected_target_batch_id = if invalid_case == "target_mismatch" {
            Some("batch-other")
        } else {
            Some("batch-stable-1")
        };

        world
            .rollback_to_snapshot_with_reconciliation(
                stable_snapshot,
                stable_journal,
                "invalid-authorization-must-not-mutate",
                expected_target_batch_id,
                approval,
                ROLLBACK_NOW_MS,
            )
            .expect_err("invalid rollback approval must be rejected");

        assert_eq!(
            world.state(),
            &state_before,
            "state mutated before rejection"
        );
        assert_eq!(
            world.journal(),
            &journal_before,
            "journal mutated before rejection"
        );
    }
}

#[test]
fn rollback_rejects_same_id_journal_body_substitution_before_mutation() {
    // The signed target must commit to event contents, not merely IDs, length, and state root.
    let on_call_key = rollback_test_key(7);
    let governance_key = rollback_test_key(9);
    let mut world = World::new();
    world
        .set_rollback_authority_registry(rollback_authority_registry(&on_call_key, &governance_key))
        .expect("configure rollback authorities");
    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        pos: pos(0, 0),
    });
    world.step().expect("create stable checkpoint");

    let stable_snapshot = world.snapshot();
    let stable_journal = world.journal().clone();
    let mut substituted_journal = stable_journal.clone();
    let stable_state_root = world.current_state_root_hash().expect("stable state root");
    let original_event = substituted_journal
        .events
        .first_mut()
        .expect("checkpoint journal event");
    original_event.body = WorldEventBody::SnapshotCreated(SnapshotMeta {
        journal_len: stable_snapshot.journal_len,
    });

    world.submit_action(Action::MoveAgent {
        agent_id: "agent-1".to_string(),
        to: pos(9, 9),
    });
    world.step().expect("mutate live world after checkpoint");
    let snapshot_before = world.snapshot();
    let journal_before = world.journal().clone();
    let mut approval = signed_rollback_authorization(
        &stable_snapshot,
        substituted_journal.len(),
        stable_state_root.as_str(),
        Some("batch-stable-1"),
        "ROLLBACK-2313-JOURNAL-COMMITMENT",
        "same-id-body-substitution-must-reject",
        "nonce-journal-body-substitution-1",
        &on_call_key,
        &governance_key,
    );
    approval.intent.schema_version = 2;
    approval.intent.target_journal_commitment = Some(
        rollback_journal_commitment(&stable_snapshot, &stable_journal, stable_journal.len())
            .expect("canonical stable journal commitment"),
    );
    let payload = approval
        .intent
        .canonical_signing_payload()
        .expect("canonical v2 rollback payload");
    approval.signatures[0].signature_hex = hex::encode(on_call_key.sign(&payload).to_bytes());
    approval.signatures[1].signature_hex = hex::encode(governance_key.sign(&payload).to_bytes());

    world
        .rollback_to_snapshot_with_reconciliation(
            stable_snapshot,
            substituted_journal,
            "same-id-body-substitution-must-reject",
            Some("batch-stable-1"),
            approval,
            ROLLBACK_NOW_MS,
        )
        .expect_err("signed journal commitment must reject same-ID event body substitution");

    assert_eq!(
        world.snapshot(),
        snapshot_before,
        "snapshot mutated on rejection"
    );
    assert_eq!(
        world.journal(),
        &journal_before,
        "journal mutated on rejection"
    );
}

#[test]
fn rollback_v2_verifies_exact_nested_payload_bytes_and_rejects_field_tampering() {
    let on_call_key = rollback_test_key(7);
    let governance_key = rollback_test_key(9);
    let mut world = World::new();
    world
        .set_rollback_authority_registry(rollback_authority_registry(&on_call_key, &governance_key))
        .expect("configure rollback authorities");
    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        pos: pos(0, 0),
    });
    world.step().expect("stable target");
    let snapshot = world.snapshot();
    let journal = world.journal().clone();
    let state_root = world.current_state_root_hash().expect("target root");
    let commitment = rollback_journal_commitment(&snapshot, &journal, journal.len())
        .expect("journal commitment");
    let mut approval = signed_rollback_authorization(
        &snapshot,
        journal.len(),
        state_root.as_str(),
        Some("batch-t"),
        "ROLLBACK-V2-BYTES",
        "nested-v2-bytes",
        "nonce-v2-bytes",
        &on_call_key,
        &governance_key,
    );
    approval.intent.schema_version = 2;
    approval.intent.target_journal_commitment = Some(commitment.clone());
    let nested = serde_json::json!({
        "schema_version": 2,
        "rollback_ticket": "ROLLBACK-V2-BYTES",
        "rollback_checkpoint": {
            "batch_id": "batch-c",
            "snapshot_hash": approval.intent.snapshot_hash.clone(),
            "journal_len": snapshot.journal_len
        },
        "replay_target": {
            "batch_id": "batch-t",
            "journal_len": journal.len(),
            "state_root": state_root.clone(),
            "journal_commitment": commitment.clone()
        },
        "expected_reorg_epoch": 0,
        "max_replay_events": 0,
        "max_replay_bytes": 4096,
        "reason": "nested-v2-bytes",
        "issued_at_ms": approval.intent.issued_at_ms,
        "expires_at_ms": approval.intent.expires_at_ms,
        "nonce": "nonce-v2-bytes"
    });
    let encode = |value: &serde_json::Value| {
        let mut bytes = b"oasis7:governed-rollback-replay:v2\0".to_vec();
        bytes.extend(serde_json::to_vec(value).expect("encode nested v2 intent"));
        bytes
    };
    let payload = encode(&nested);
    approval.signatures[0].signature_hex = hex::encode(on_call_key.sign(&payload).to_bytes());
    approval.signatures[1].signature_hex = hex::encode(governance_key.sign(&payload).to_bytes());

    for path in [
        "/rollback_checkpoint/batch_id",
        "/expected_reorg_epoch",
        "/max_replay_events",
        "/max_replay_bytes",
    ] {
        let mut tampered = nested.clone();
        let slot = tampered.pointer_mut(path).expect("tamper field");
        *slot = match slot {
            serde_json::Value::String(value) => serde_json::Value::String(format!("{value}-x")),
            serde_json::Value::Number(value) => {
                serde_json::json!(value.as_u64().expect("u64") + 1)
            }
            _ => unreachable!(),
        };
        let mut candidate = world.clone();
        candidate
            .rollback_to_snapshot_with_reconciliation_v2(
                snapshot.clone(),
                journal.clone(),
                "nested-v2-bytes",
                Some("batch-t"),
                approval.clone(),
                &encode(&tampered),
                ROLLBACK_NOW_MS,
            )
            .expect_err("any nested signed-field mutation must invalidate both signatures");
    }

    world
        .rollback_to_snapshot_with_reconciliation_v2(
            snapshot,
            journal,
            "nested-v2-bytes",
            Some("batch-t"),
            approval,
            &payload,
            ROLLBACK_NOW_MS,
        )
        .expect("two valid role signatures over exact nested v2 bytes succeed");
}

#[test]
fn rollback_nonce_is_durable_and_cannot_be_replayed_through_an_old_snapshot() {
    let on_call_key = rollback_test_key(7);
    let governance_key = rollback_test_key(9);
    let mut world = World::new();
    world
        .set_rollback_authority_registry(rollback_authority_registry(&on_call_key, &governance_key))
        .expect("configure rollback authorities");
    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        pos: pos(0, 0),
    });
    world.step().expect("step");
    let old_snapshot = world.snapshot();
    let old_journal = world.journal().clone();
    let old_state_root = world.current_state_root_hash().expect("old state root");
    let approval = signed_rollback_authorization(
        &old_snapshot,
        old_journal.len(),
        old_state_root.as_str(),
        None,
        "ROLLBACK-2313-REPLAY",
        "durable-replay-test",
        "nonce-durable-1",
        &on_call_key,
        &governance_key,
    );

    world
        .rollback_to_snapshot(
            old_snapshot.clone(),
            old_journal.clone(),
            "durable-replay-test",
            None,
            approval.clone(),
            ROLLBACK_NOW_MS,
        )
        .expect("first use succeeds");
    let recovery_metadata = RollbackOutcomeRecoveryMetadata {
        target_batch_id: "batch-target-3".to_string(),
        prior_reorg_epoch: 7,
        committed_reorg_epoch: 8,
        invalidated_batch_ids: vec!["batch-fork-4".to_string(), "batch-fork-5".to_string()],
        dispositions: vec![RollbackEventDisposition {
            source_batch_id: "batch-fork-4".to_string(),
            source_event_id: 41,
            status: RollbackDispositionStatus::RejectedFork,
            compensation: None,
            player_id: Some("player-1".to_string()),
            action_id: Some("action-9".to_string()),
            system_authored: false,
        }],
    };
    world
        .record_rollback_outcome_recovery_metadata("nonce-durable-1", recovery_metadata.clone())
        .expect("complete durable rollback receipt metadata");
    let affected = vec![RollbackSourceEventIdentity {
        source_batch_id: "batch-fork-4".to_string(),
        source_event_id: 41,
    }];
    let readiness_evidence = RollbackReadinessEvidence {
        target_root_matches: true,
        epoch_matches: true,
        drift_free: true,
        consensus_chain_valid: true,
        receipt_retrievable: true,
    };
    assert!(matches!(
        world.rollback_readiness("nonce-durable-1", &affected, &readiness_evidence),
        RollbackReadiness::Blocked { .. }
    ));
    let committed = world
        .rollback_nonce_outcome("nonce-durable-1")
        .expect("committed outcome");
    let affected_census_digest = rollback_affected_census_digest(&affected).expect("census digest");
    let receipt = RollbackReceiptProjection {
        receipt_id: "receipt-durable-1".to_string(),
        canonical_intent_digest: committed.canonical_intent_hash,
        rollback_checkpoint_batch_id: "batch-checkpoint-2".to_string(),
        rollback_checkpoint_snapshot_hash: committed.rollback_checkpoint_snapshot_hash,
        rollback_checkpoint_journal_len: committed.rollback_checkpoint_journal_len,
        replay_target_batch_id: "batch-target-3".to_string(),
        replay_target_journal_len: committed.target_journal_len,
        replay_target_state_root: committed.target_state_root,
        replay_target_journal_commitment: committed.target_journal_commitment,
        affected_census_digest,
        affected_census_count: affected.len(),
        readiness_blockers: Vec::new(),
        snapshot_height: world.state().time,
        snapshot_hash: util::hash_json(&world.snapshot()).expect("receipt snapshot hash"),
        log_cursor: world.journal().len() as u64,
        acknowledged_at_tick: world.state().time,
    };
    let mut tampered_receipt = receipt.clone();
    tampered_receipt.affected_census_digest = "tampered-census".to_string();
    let mut tampered_world = world.clone();
    tampered_world
        .complete_rollback_outcome(
            "nonce-durable-1",
            recovery_metadata.clone(),
            &affected,
            tampered_receipt,
        )
        .expect_err("projection must reject a census digest not derived from affected identities");
    world
        .complete_rollback_outcome(
            "nonce-durable-1",
            recovery_metadata.clone(),
            &affected,
            receipt.clone(),
        )
        .expect("validate coverage and freeze immutable receipt");
    assert_eq!(
        world.rollback_receipt_projection("nonce-durable-1"),
        Some(receipt.clone())
    );
    world
        .validate_rollback_receipt_projection("nonce-durable-1", &affected)
        .expect("committed projection validates against independent census");
    world
        .complete_rollback_outcome(
            "nonce-durable-1",
            recovery_metadata.clone(),
            &affected,
            receipt.clone(),
        )
        .expect("exact completion retry is idempotent");
    assert!(matches!(
        world.rollback_readiness_without_evidence("nonce-durable-1"),
        RollbackReadiness::Blocked { reasons } if !reasons.is_empty()
    ));
    assert_eq!(
        world.rollback_readiness("nonce-durable-1", &affected, &readiness_evidence),
        RollbackReadiness::Ready
    );

    let dir = temp_dir("persist-rollback-replay-state");
    world
        .save_to_dir(&dir)
        .expect("persist rollback replay state");
    let mut world = World::load_from_dir(&dir).expect("restore rollback replay state");
    let restored_outcome = world
        .rollback_nonce_outcome("nonce-durable-1")
        .expect("public lookup returns persisted rollback outcome");
    assert_eq!(
        restored_outcome.target_batch_id,
        recovery_metadata.target_batch_id
    );
    assert_eq!(restored_outcome.prior_reorg_epoch, 7);
    assert_eq!(restored_outcome.committed_reorg_epoch, 8);
    assert_eq!(
        restored_outcome.invalidated_batch_ids,
        recovery_metadata.invalidated_batch_ids
    );
    assert_eq!(
        restored_outcome.dispositions,
        recovery_metadata.dispositions
    );
    assert_eq!(restored_outcome.receipt.as_ref(), Some(&receipt));
    assert_eq!(restored_outcome.affected_census_count, affected.len());
    assert_eq!(
        restored_outcome.affected_census_digest,
        receipt.affected_census_digest
    );
    world
        .validate_rollback_receipt_projection("nonce-durable-1", &affected)
        .expect("restored immutable projection validates");
    let restored_snapshot = world.snapshot();
    assert_eq!(
        restored_snapshot.rollback_authority_registry,
        rollback_authority_registry(&on_call_key, &governance_key)
    );
    assert!(
        restored_snapshot
            .consumed_rollback_nonces
            .contains("nonce-durable-1")
    );

    let state_after_first = world.state().clone();
    let journal_after_first = world.journal().clone();

    world
        .rollback_to_snapshot(
            old_snapshot.clone(),
            old_journal.clone(),
            "durable-replay-test",
            None,
            approval.clone(),
            ROLLBACK_NOW_MS,
        )
        .expect("exact retry must return the committed outcome without mutation");
    assert_eq!(world.state(), &state_after_first);
    assert_eq!(world.journal(), &journal_after_first);
    assert_eq!(
        world
            .rollback_nonce_outcome("nonce-durable-1")
            .expect("exact retry preserves stored outcome"),
        restored_outcome
    );

    let invalid_exact_retries = [
        {
            let mut missing = approval.clone();
            missing.signatures.clear();
            (missing, ROLLBACK_NOW_MS, false)
        },
        {
            let mut forged = approval.clone();
            for signature in &mut forged.signatures {
                signature.signature_hex = "00".repeat(64);
            }
            (forged, ROLLBACK_NOW_MS, false)
        },
        (approval.clone(), approval.intent.expires_at_ms + 1, false),
        (approval.clone(), ROLLBACK_NOW_MS, true),
    ];
    for (invalid_approval, verification_time_ms, rotate) in invalid_exact_retries {
        if rotate {
            world
                .set_rollback_authority_registry(rollback_authority_registry(
                    &rollback_test_key(11),
                    &rollback_test_key(13),
                ))
                .expect("rotate rollback authorities");
        }
        let observed = |world: &World| {
            (
                world.snapshot(),
                world.journal().clone(),
                world.rollback_nonce_outcome("nonce-durable-1"),
                world.rollback_readiness("nonce-durable-1", &affected, &readiness_evidence),
                world.rollback_receipt_projection("nonce-durable-1"),
            )
        };
        let before = observed(&world);
        world
            .rollback_to_snapshot(
                old_snapshot.clone(),
                old_journal.clone(),
                "durable-replay-test",
                None,
                invalid_approval,
                verification_time_ms,
            )
            .expect_err("invalid exact retry must not return committed success");
        assert_eq!(observed(&world), before, "committed state mutated");
    }
    let mut altered = approval;
    altered.intent.reason = "altered-intent".to_string();
    let error = world
        .rollback_to_snapshot(
            old_snapshot,
            old_journal,
            "altered-intent",
            None,
            altered,
            ROLLBACK_NOW_MS,
        )
        .expect_err("same nonce with altered canonical intent must conflict");
    assert!(matches!(error, WorldError::RollbackNonceConflict { .. }));
    assert_eq!(world.state(), &state_after_first);
    assert_eq!(world.journal(), &journal_after_first);
    let incomplete = RollbackOutcomeRecoveryMetadata {
        target_batch_id: "batch-target-3".to_string(),
        prior_reorg_epoch: 7,
        committed_reorg_epoch: 8,
        invalidated_batch_ids: vec!["batch-fork-4".to_string()],
        dispositions: Vec::new(),
    };
    world
        .record_rollback_outcome_recovery_metadata("nonce-durable-1", incomplete)
        .expect_err(
            "recovery readiness must remain false and reject metadata with uncovered events",
        );
    let mut changed_receipt = receipt;
    changed_receipt.log_cursor += 1;
    world
        .complete_rollback_outcome(
            "nonce-durable-1",
            recovery_metadata,
            &affected,
            changed_receipt,
        )
        .expect_err("committed receipt projection must be immutable");
    let _ = fs::remove_dir_all(&dir);
}

#[path = "persistence_signed_boundaries.rs"]
mod signed_boundaries;
