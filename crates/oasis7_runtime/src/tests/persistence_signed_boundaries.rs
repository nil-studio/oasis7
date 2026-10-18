use super::*;
#[test]
fn typed_governed_rollback_v2_retries_exactly_and_rejects_nonce_reuse() {
    let on_call_key = rollback_test_key(7);
    let governance_key = rollback_test_key(9);
    let mut world = World::new();
    world
        .set_rollback_authority_registry(rollback_authority_registry(&on_call_key, &governance_key))
        .unwrap();
    world.submit_action(Action::RegisterAgent {
        agent_id: "typed-rollback-agent".into(),
        pos: pos(0, 0),
    });
    world.step().unwrap();
    let snapshot = world.snapshot();
    let journal = world.journal().clone();
    let retained_target = world.snapshot();
    let root = world.current_state_root_hash().unwrap();
    let flat = signed_rollback_authorization(
        &snapshot,
        journal.len(),
        &root,
        Some("typed-target"),
        "typed-ticket",
        "typed-reason",
        "typed-nonce",
        &on_call_key,
        &governance_key,
    );
    let mut typed: oasis7_proto::viewer::RollbackAuthorizationEnvelopeV2 = serde_json::from_value(json!({
        "intent": {"schema_version":2,"rollback_ticket":"typed-ticket", "rollback_checkpoint":{"batch_id":"typed-checkpoint","snapshot_hash":blake3::hash(&serde_json::to_vec(&snapshot).unwrap()).to_hex().to_string(),"journal_len":snapshot.journal_len},
        "replay_target":{"batch_id":"typed-target","journal_len":journal.len(),"state_root":blake3::hash(&serde_json::to_vec(&world.snapshot()).unwrap()).to_hex().to_string(),"journal_commitment":rollback_journal_commitment(&snapshot,&journal,journal.len()).unwrap()},
        "expected_reorg_epoch":0,"max_replay_events":0,"max_replay_bytes":4096,"reason":"typed-reason","issued_at_ms":flat.intent.issued_at_ms,"expires_at_ms":flat.intent.expires_at_ms,"nonce":"typed-nonce"},
        "signatures":flat.signatures
    })).unwrap();
    let payload = typed.intent.canonical_signing_payload().unwrap();
    typed.signatures[0].signature_hex = hex::encode(on_call_key.sign(&payload).to_bytes());
    typed.signatures[1].signature_hex = hex::encode(governance_key.sign(&payload).to_bytes());
    let unchanged = serde_json::to_vec(&world).unwrap();
    for checkpoint_hash in [true, false] {
        let mut invalid = typed.clone();
        if checkpoint_hash {
            invalid.intent.rollback_checkpoint.snapshot_hash = "00".repeat(32);
        } else {
            invalid.intent.replay_target.expected_target_state_root = "00".repeat(32);
        }
        let payload = invalid.intent.canonical_signing_payload().unwrap();
        invalid.signatures[0].signature_hex = hex::encode(on_call_key.sign(&payload).to_bytes());
        invalid.signatures[1].signature_hex = hex::encode(governance_key.sign(&payload).to_bytes());
        assert!(
            world
                .apply_governed_rollback_v2(
                    snapshot.clone(),
                    journal.clone(),
                    retained_target.clone(),
                    &invalid,
                    ROLLBACK_NOW_MS
                )
                .is_err()
        );
        assert_eq!(serde_json::to_vec(&world).unwrap(), unchanged);
    }
    let mut unrelated_target = retained_target.clone();
    unrelated_target.state.time = unrelated_target.state.time.saturating_add(1);
    let mut unrelated_approval = typed.clone();
    unrelated_approval
        .intent
        .replay_target
        .expected_target_state_root = blake3::hash(&serde_json::to_vec(&unrelated_target).unwrap())
        .to_hex()
        .to_string();
    let payload = unrelated_approval
        .intent
        .canonical_signing_payload()
        .unwrap();
    unrelated_approval.signatures[0].signature_hex =
        hex::encode(on_call_key.sign(&payload).to_bytes());
    unrelated_approval.signatures[1].signature_hex =
        hex::encode(governance_key.sign(&payload).to_bytes());
    assert!(
        world
            .apply_governed_rollback_v2(
                snapshot.clone(),
                journal.clone(),
                unrelated_target,
                &unrelated_approval,
                ROLLBACK_NOW_MS
            )
            .is_err()
    );
    assert_eq!(serde_json::to_vec(&world).unwrap(), unchanged);
    world
        .apply_governed_rollback_v2(
            snapshot.clone(),
            journal.clone(),
            retained_target.clone(),
            &typed,
            ROLLBACK_NOW_MS,
        )
        .unwrap();
    assert!(
        world
            .is_authenticated_committed_rollback_v2_retry(&typed, ROLLBACK_NOW_MS)
            .unwrap()
    );
    let before = serde_json::to_vec(&world).unwrap();
    world
        .apply_governed_rollback_v2(
            snapshot.clone(),
            journal.clone(),
            retained_target.clone(),
            &typed,
            ROLLBACK_NOW_MS,
        )
        .unwrap();
    assert_eq!(serde_json::to_vec(&world).unwrap(), before);
    typed.intent.reason = "a different complete signed intent".into();
    let payload = typed.intent.canonical_signing_payload().unwrap();
    typed.signatures[0].signature_hex = hex::encode(on_call_key.sign(&payload).to_bytes());
    typed.signatures[1].signature_hex = hex::encode(governance_key.sign(&payload).to_bytes());
    assert!(matches!(
        world.is_authenticated_committed_rollback_v2_retry(&typed, ROLLBACK_NOW_MS),
        Err(WorldError::RollbackNonceConflict { .. })
    ));
    assert!(matches!(
        world.apply_governed_rollback_v2(
            snapshot,
            journal,
            retained_target,
            &typed,
            ROLLBACK_NOW_MS
        ),
        Err(WorldError::RollbackNonceConflict { .. })
    ));
    assert_eq!(serde_json::to_vec(&world).unwrap(), before);
}

#[test]
fn typed_governed_rollback_v2_enforces_signed_suffix_event_and_byte_limits() {
    let on_call_key = rollback_test_key(7);
    let governance_key = rollback_test_key(9);
    let mut world = World::new();
    world
        .set_rollback_authority_registry(rollback_authority_registry(&on_call_key, &governance_key))
        .unwrap();
    world.submit_action(Action::RegisterAgent {
        agent_id: "checkpoint-agent".into(),
        pos: pos(0, 0),
    });
    world.step().unwrap();
    let snapshot = world.snapshot();
    world.submit_action(Action::RegisterAgent {
        agent_id: "suffix-agent".into(),
        pos: pos(1, 0),
    });
    world.step().unwrap();
    let journal = world.journal().clone();
    let retained_target = world.snapshot();
    assert!(journal.len() > snapshot.journal_len);
    let root = world.current_state_root_hash().unwrap();
    let flat = signed_rollback_authorization(
        &snapshot,
        journal.len(),
        &root,
        Some("limited-target"),
        "limited-ticket",
        "limited-reason",
        "limited-nonce",
        &on_call_key,
        &governance_key,
    );
    let mut typed: oasis7_proto::viewer::RollbackAuthorizationEnvelopeV2 = serde_json::from_value(json!({
        "intent":{"schema_version":2,"rollback_ticket":"limited-ticket", "rollback_checkpoint":{"batch_id":"limited-checkpoint","snapshot_hash":blake3::hash(&serde_json::to_vec(&snapshot).unwrap()).to_hex().to_string(),"journal_len":snapshot.journal_len},
        "replay_target":{"batch_id":"limited-target","journal_len":journal.len(),"state_root":blake3::hash(&serde_json::to_vec(&world.snapshot()).unwrap()).to_hex().to_string(),"journal_commitment":rollback_journal_commitment(&snapshot,&journal,journal.len()).unwrap()},
        "expected_reorg_epoch":0,"max_replay_events":0,"max_replay_bytes":4096,"reason":"limited-reason","issued_at_ms":flat.intent.issued_at_ms,"expires_at_ms":flat.intent.expires_at_ms,"nonce":"limited-nonce"},
        "signatures":flat.signatures
    })).unwrap();
    let before = serde_json::to_vec(&world).unwrap();
    for (events, bytes) in [(0, usize::MAX), (usize::MAX, 0)] {
        typed.intent.max_replay_events = events;
        typed.intent.max_replay_bytes = bytes;
        let payload = typed.intent.canonical_signing_payload().unwrap();
        typed.signatures[0].signature_hex = hex::encode(on_call_key.sign(&payload).to_bytes());
        typed.signatures[1].signature_hex = hex::encode(governance_key.sign(&payload).to_bytes());
        assert!(
            world
                .apply_governed_rollback_v2(
                    snapshot.clone(),
                    journal.clone(),
                    retained_target.clone(),
                    &typed,
                    ROLLBACK_NOW_MS
                )
                .is_err(),
            "signed limit events={events} bytes={bytes} was bypassed"
        );
        assert_eq!(serde_json::to_vec(&world).unwrap(), before);
    }
}
