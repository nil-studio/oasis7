//! Regression coverage for persisted capability authorization and replay.

use super::super::*;
use super::capability_grant_v2::*;
use oasis7_wasm_abi::{ModuleCallCaller, ModuleCallInput, ModuleEffectIntent, ModuleOutput};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};

#[test]
fn authorization_nonce_same_hash_is_idempotent_but_different_hash_conflicts_without_effect() {
    let mut world = fixture_world();
    let grant = signed_grant(grant_json(json!({})));
    let (catalog, response) = prepared_invocation(
        &world,
        &grant,
        catalog_json(json!({})),
        response_json(json!({})),
    );
    install_invocation_context(&mut world, &grant, &catalog, &response);
    let mut sandbox = RecordingSandbox::default();
    let first = execute_without_invocation_context(
        &mut world,
        grant.clone(),
        catalog.clone(),
        response.clone(),
        &mut sandbox,
    )
    .expect("first exact request");
    let second = execute_without_invocation_context(
        &mut world,
        grant.clone(),
        catalog.clone(),
        response.clone(),
        &mut sandbox,
    )
    .expect("same request is idempotent");
    assert_eq!(first.receipt_id, second.receipt_id);
    assert_eq!(second.decision, "idempotent");
    assert_eq!(sandbox.calls, 1);

    let mut conflict_response = response;
    conflict_response.envelope.payload = vec![123, 34, 120, 34, 125];
    let error = execute_without_invocation_context(
        &mut world,
        grant,
        catalog,
        conflict_response,
        &mut sandbox,
    )
    .expect_err("same nonce with a different request hash must conflict");
    assert!(matches!(error, WorldError::CapabilityNonceConflict { .. }));
    assert_eq!(sandbox.calls, 1);
}

#[test]
#[allow(deprecated)]
fn capability_authority_install_rejects_unverified_finality_metadata() {
    let mut world = World::new();
    let signing_key = capability_issuer_signing_key();
    let record = CapabilityAuthorityRecord {
        issuer_id: ISSUER_ID.to_string(),
        issuer_kind: "governance".to_string(),
        key_id: "governance-key-1".to_string(),
        public_key_hex: hex::encode(signing_key.verifying_key().to_bytes()),
        issuer_key_epoch: 3,
        governance_epoch: 9,
        finalized_receipt_id: "unverified-receipt".to_string(),
        authority_rotation_receipt_id: None,
        world_id: WORLD_ID.to_string(),
        branch_id: BRANCH_ID.to_string(),
        finality_epoch: 4,
        finality_block_hash: "forged-finality-block".to_string(),
        finality_status: "finalized".to_string(),
        revocation_epoch: 2,
        revoked_grant_ids: BTreeSet::new(),
        superseded_by: BTreeMap::new(),
    };

    let error = world
        .install_capability_authority_record(record)
        .expect_err("caller-supplied finalized metadata must not create a trust root");
    assert!(matches!(
        error,
        WorldError::CapabilityAuthorizationDenied { .. }
    ));
    assert!(
        world
            .capability_revocation_state()
            .authority_records
            .is_empty()
    );
}

#[test]
fn capability_authority_install_rejects_verified_finality_metadata_mismatch() {
    let cases = [
        ("receipt", "forged-receipt", BRANCH_ID, "block-hash-4"),
        ("branch", "finality-9", "forged-branch", "block-hash-4"),
        ("block", "finality-9", BRANCH_ID, "forged-block"),
        ("issuer key epoch", "finality-9", BRANCH_ID, "block-hash-4"),
        ("governance epoch", "finality-9", BRANCH_ID, "block-hash-4"),
        ("rotated key", "finality-9", BRANCH_ID, "block-hash-4"),
    ];
    for (index, (label, receipt_id, branch_id, block_hash)) in cases.into_iter().enumerate() {
        let mut world = World::new();
        let error = install_test_capability_authority_with_metadata(
            &mut world,
            BTreeSet::new(),
            receipt_id,
            branch_id,
            block_hash,
            if index == 3 { 4 } else { 3 },
            if index == 4 { 10 } else { 9 },
            label == "rotated key",
        )
        .expect_err(label);
        assert!(
            matches!(error, WorldError::CapabilityAuthorizationDenied { .. }),
            "{label} mismatch must be denied, got {error:?}"
        );
    }
}

#[test]
fn authority_replay_rejects_record_without_finality_proof() {
    let snapshot = World::new().snapshot();
    let record = CapabilityAuthorityRecord {
        issuer_id: ISSUER_ID.to_string(),
        issuer_kind: "governance".to_string(),
        key_id: "governance-key-1".to_string(),
        public_key_hex: "11".repeat(32),
        issuer_key_epoch: 3,
        governance_epoch: 9,
        finalized_receipt_id: "finality-9".to_string(),
        authority_rotation_receipt_id: None,
        world_id: WORLD_ID.to_string(),
        branch_id: BRANCH_ID.to_string(),
        finality_epoch: 4,
        finality_block_hash: "block-hash-4".to_string(),
        finality_status: "finalized".to_string(),
        revocation_epoch: 2,
        revoked_grant_ids: BTreeSet::new(),
        superseded_by: BTreeMap::new(),
    };
    let certificate = GovernanceFinalityCertificate {
        proposal_id: 0,
        manifest_hash: String::new(),
        consensus_height: 0,
        epoch_id: 0,
        validator_set_hash: String::new(),
        stake_root: String::new(),
        threshold_bps: 0,
        min_unique_signers: 0,
        threshold: 0,
        signatures: BTreeMap::new(),
    };
    let events = [
        (
            "record-only",
            CapabilityAuthorizationEvent::AuthorityInstalled {
                record: record.clone(),
            },
        ),
        (
            "certificate-only",
            CapabilityAuthorizationEvent::AuthorityInstalledWithFinality {
                record,
                certificate,
            },
        ),
    ];
    for (label, authorization_event) in events {
        let mut replay_snapshot = snapshot.clone();
        let mut journal = Journal::new();
        journal.append(WorldEvent {
            id: 1,
            time: 0,
            caused_by: None,
            body: WorldEventBody::CapabilityAuthorization(authorization_event),
        });
        replay_snapshot.last_event_id = 0;
        let error = World::from_snapshot(replay_snapshot, journal).expect_err(label);
        assert!(matches!(
            error,
            WorldError::CapabilityAuthorizationDenied { .. }
        ));
    }
}

#[test]
fn capability_invocation_context_allows_two_distinct_nonces_for_one_grant() {
    let mut world = fixture_world();
    let grant = signed_grant(grant_json(json!({"issued_at_tick": 0})));
    let catalog = prepared_catalog(&world, &grant, catalog_json(json!({})));
    let response = prepared_response(response_json(json!({})), &catalog);
    let mut second_response = response.clone();
    second_response.response_nonce = "response-2".to_string();

    install_invocation_context(&mut world, &grant, &catalog, &response);
    install_invocation_context(&mut world, &grant, &catalog, &second_response);

    assert_eq!(world.capability_invocation_contexts().len(), 2);
    assert!(
        world
            .capability_invocation_contexts()
            .values()
            .any(|context| context.response_nonce == "response-1")
    );
    assert!(
        world
            .capability_invocation_contexts()
            .values()
            .any(|context| context.response_nonce == "response-2")
    );
}

#[test]
fn trusted_executor_replays_stale_snapshot_authz_tail_exactly_once() {
    let mut world = fixture_world();
    let grant = signed_grant(grant_json(json!({
        "issued_at_tick": 0,
        "scope": {
            "entity_selector": null,
            "resource_selector": null
        }
    })));
    install_budget_for_grant(&mut world, &grant, 128);
    let (catalog, response) = prepared_invocation(
        &world,
        &grant,
        catalog_json(json!({})),
        response_json(json!({
            "subject": {
                "kind": "agent",
                "agent_id": SUBJECT_ID,
                "owner_binding": "owner-7",
                "generation": 1
            }
        })),
    );
    install_invocation_context(&mut world, &grant, &catalog, &response);
    let stale_snapshot = world.snapshot();
    let mut sandbox = RecordingSandbox::default();
    let first = execute_without_invocation_context(
        &mut world,
        grant.clone(),
        catalog.clone(),
        response.clone(),
        &mut sandbox,
    )
    .expect("first command should commit before the simulated crash");
    let committed_journal = world.journal().clone();

    let mut restored = World::from_snapshot(stale_snapshot, committed_journal)
        .expect("stale snapshot plus journal tail should recover");
    assert_eq!(
        restored.capability_grants_v2().get(&grant.grant_id),
        world.capability_grants_v2().get(&grant.grant_id)
    );
    assert_eq!(
        restored.capability_nonce_records(),
        world.capability_nonce_records()
    );
    assert_eq!(
        restored.capability_authorization_receipts(),
        world.capability_authorization_receipts()
    );
    assert_eq!(
        restored.capability_budget_accounts(),
        world.capability_budget_accounts()
    );

    let mut retry_sandbox = RecordingSandbox::default();
    let retry = execute_without_invocation_context(
        &mut restored,
        grant,
        catalog,
        response,
        &mut retry_sandbox,
    )
    .expect("replayed command must be idempotent, not re-executed");
    assert_eq!(retry.decision, "idempotent");
    assert_eq!(retry.receipt_id, first.receipt_id);
    assert_eq!(retry_sandbox.calls, 0);
}

#[test]
fn trusted_executor_replays_effect_receipt_ack_after_closure_window_exactly_once() {
    let mut world = fixture_world();
    let signer = ReceiptSigner::hmac_sha256(b"capability-receipt-test-key");
    world.set_receipt_signer(signer.clone());
    let grant = signed_grant(grant_json(json!({})));
    let (catalog, response) = prepared_invocation(
        &world,
        &grant,
        catalog_json(json!({})),
        response_json(json!({})),
    );
    install_invocation_context(&mut world, &grant, &catalog, &response);
    let effect_grant = signed_effect_grant();
    let mut sandbox = ConfiguredSandbox {
        calls: 0,
        output: ModuleOutput {
            new_state: Some(vec![0x99]),
            effects: vec![ModuleEffectIntent {
                kind: "weather.publish".to_string(),
                params: json!({"station": "station-1"}),
                cap_ref: effect_grant.grant_id.clone(),
                cap_slot: None,
            }],
            emits: Vec::new(),
            tick_lifecycle: None,
            output_bytes: 64,
        },
    };

    let authorization =
        execute_without_invocation_context(&mut world, grant, catalog, response, &mut sandbox)
            .expect("effect-producing command should commit before the receipt window");
    let intent = world
        .take_next_effect()
        .expect("committed effect should be dispatched before receipt acknowledgement");
    let stale_snapshot = world.snapshot();
    let journal_before_receipt = world.journal().clone();
    let receipt = EffectReceipt {
        birth_era: None,
        world: None,
        intent_payload_digest: None,
        intent_id: intent.intent_id.clone(),
        status: "ok".to_string(),
        payload: json!({"published": true}),
        cost_cents: Some(1),
        signature: None,
    };

    world
        .ingest_receipt(receipt.clone())
        .expect("live receipt ingestion should close authorization and acknowledge effect");
    let mut closure_only_journal = world.journal().clone();
    let acknowledgement = closure_only_journal
        .events
        .pop()
        .expect("receipt acknowledgement event should be journaled last");
    let signed_receipt = match acknowledgement.body {
        WorldEventBody::ReceiptAppended(receipt) => receipt,
        body => panic!("expected receipt acknowledgement, got {body:?}"),
    };
    assert_eq!(
        closure_only_journal.events.len(),
        journal_before_receipt.events.len() + 1,
        "the simulated crash preserves the authorization closure but not receipt acknowledgement"
    );
    assert!(closure_only_journal.events.iter().any(|event| matches!(
        &event.body,
        WorldEventBody::CapabilityAuthorization(
            CapabilityAuthorizationEvent::EffectReceiptCommitted { intent_id, .. }
        ) if intent_id == &intent.intent_id
    )));

    let mut recovered = World::from_snapshot(stale_snapshot, closure_only_journal)
        .expect("recovery should replay the durable authorization closure");
    recovered.set_receipt_signer(signer);
    let recovered_audit = recovered
        .capability_authorization_receipts()
        .get(&authorization.receipt_id)
        .expect("authorization audit receipt survives the crash window");
    assert_eq!(
        recovered_audit.committed_effect_receipt_id.as_deref(),
        Some(intent.intent_id.as_str())
    );
    assert!(
        !recovered
            .capability_effect_receipt_links()
            .contains_key(&intent.intent_id)
    );

    recovered
        .ingest_receipt(signed_receipt)
        .expect("receipt acknowledgement should remain retryable after closure replay");
    assert_eq!(
        recovered
            .journal()
            .events
            .iter()
            .filter(|event| matches!(
                &event.body,
                WorldEventBody::CapabilityAuthorization(
                    CapabilityAuthorizationEvent::EffectReceiptCommitted { intent_id, .. }
                ) if intent_id == &intent.intent_id
            ))
            .count(),
        1,
        "receipt retry must not duplicate authorization closure"
    );
    assert_eq!(
        recovered
            .journal()
            .events
            .iter()
            .filter(|event| matches!(
                &event.body,
                WorldEventBody::ReceiptAppended(receipt) if receipt.intent_id == intent.intent_id
            ))
            .count(),
        1,
        "receipt acknowledgement is appended exactly once"
    );
    assert!(
        !recovered
            .capability_effect_receipt_links()
            .contains_key(&intent.intent_id)
    );
}

#[test]
fn trusted_executor_replays_multi_effect_receipt_closure_exactly_once() {
    let effect_grant = signed_effect_grant_with_selectors();
    let mut world = fixture_world_with_revocations_and_budget_and_effect_grant(
        BTreeSet::new(),
        128,
        effect_grant.clone(),
    );
    let grant = signed_grant(grant_json(json!({})));
    let (catalog, response) = prepared_invocation(
        &world,
        &grant,
        catalog_json(json!({})),
        response_json(json!({})),
    );
    install_invocation_context(&mut world, &grant, &catalog, &response);
    let mut sandbox = ConfiguredSandbox {
        calls: 0,
        output: ModuleOutput {
            new_state: Some(vec![0x9a]),
            effects: vec![
                ModuleEffectIntent {
                    kind: "weather.publish".to_string(),
                    params: json!({"entity_id": "station-1", "resource_id": "weather.read"}),
                    cap_ref: effect_grant.grant_id.clone(),
                    cap_slot: None,
                },
                ModuleEffectIntent {
                    kind: "weather.publish".to_string(),
                    params: json!({"entity_id": "station-1", "resource_id": "weather.read"}),
                    cap_ref: effect_grant.grant_id,
                    cap_slot: None,
                },
            ],
            emits: Vec::new(),
            tick_lifecycle: None,
            output_bytes: 0,
        },
    };

    let authorization =
        execute_without_invocation_context(&mut world, grant, catalog, response, &mut sandbox)
            .expect("multi-effect command should commit");
    let first = world.take_next_effect().expect("first effect is queued");
    let second = world.take_next_effect().expect("second effect is queued");
    assert_ne!(first.intent_id, second.intent_id);
    assert_eq!(
        world
            .capability_effect_receipt_links()
            .values()
            .filter(|link| link.authorization_receipt_id == authorization.receipt_id)
            .count(),
        2,
        "both effects remain linked until their own receipts arrive"
    );

    let stale_snapshot = world.snapshot();
    let journal_before_receipt = world.journal().clone();
    let first_receipt = EffectReceipt {
        birth_era: None,
        world: None,
        intent_payload_digest: None,
        intent_id: first.intent_id.clone(),
        status: "ok".to_string(),
        payload: json!({"published": true, "effect": 1}),
        cost_cents: Some(1),
        signature: None,
    };
    let second_receipt = EffectReceipt {
        birth_era: None,
        world: None,
        intent_payload_digest: None,
        intent_id: second.intent_id.clone(),
        status: "ok".to_string(),
        payload: json!({"published": true, "effect": 2}),
        cost_cents: Some(1),
        signature: None,
    };

    world
        .ingest_receipt(first_receipt.clone())
        .expect("first linked effect receipt should close independently");
    let mut closure_only_journal = world.journal().clone();
    let signed_first_receipt = match closure_only_journal
        .events
        .pop()
        .expect("first receipt acknowledgement event")
        .body
    {
        WorldEventBody::ReceiptAppended(receipt) => receipt,
        body => panic!("expected first receipt acknowledgement, got {body:?}"),
    };
    assert_eq!(
        closure_only_journal.events.len(),
        journal_before_receipt.events.len() + 1,
        "the crash window preserves one closure but not its external acknowledgement"
    );

    let mut recovered = World::from_snapshot(stale_snapshot, closure_only_journal)
        .expect("replay should preserve the first effect closure and second pending link");
    let recovered_audit = recovered
        .capability_authorization_receipts()
        .values()
        .next()
        .expect("authorization audit receipt survives replay");
    assert_eq!(
        recovered_audit.committed_effect_receipt_ids,
        BTreeSet::from([first.intent_id.clone()]),
        "replayed closure records exactly the first effect"
    );
    assert!(
        !recovered
            .capability_effect_receipt_links()
            .contains_key(&first.intent_id)
    );
    assert!(
        recovered
            .capability_effect_receipt_links()
            .contains_key(&second.intent_id)
    );

    recovered
        .ingest_receipt(signed_first_receipt)
        .expect("receipt acknowledgement should remain retryable after closure replay");
    recovered
        .ingest_receipt(second_receipt)
        .expect("second linked effect receipt should close independently");
    assert!(
        recovered
            .capability_effect_receipt_links()
            .values()
            .all(|link| link.authorization_receipt_id != authorization.receipt_id),
        "all links for one authorization receipt must close"
    );
    let recovered_audit = recovered
        .capability_authorization_receipts()
        .get(&authorization.receipt_id)
        .expect("authorization audit receipt remains durable");
    assert_eq!(
        recovered_audit.committed_effect_receipt_ids,
        BTreeSet::from([first.intent_id.clone(), second.intent_id.clone()]),
        "replay records every effect closure without overwriting prior receipts"
    );
    assert_eq!(
        recovered
            .journal()
            .events
            .iter()
            .filter(|event| matches!(
                &event.body,
                WorldEventBody::CapabilityAuthorization(
                    CapabilityAuthorizationEvent::EffectReceiptCommitted { intent_id, .. }
                ) if intent_id == &first.intent_id || intent_id == &second.intent_id
            ))
            .count(),
        2,
        "each effect gets exactly one durable authorization closure"
    );
}

#[test]
fn stale_capability_context_replay_ignores_unrelated_module_state_tail() {
    let mut world = fixture_world();
    let grant = signed_grant(grant_json(json!({})));
    let (catalog, response) = prepared_invocation(
        &world,
        &grant,
        catalog_json(json!({})),
        response_json(json!({})),
    );
    install_invocation_context(&mut world, &grant, &catalog, &response);

    let stale_snapshot = world.snapshot();
    let mut journal = world.journal().clone();
    journal.append(WorldEvent {
        id: stale_snapshot.last_event_id + 1,
        time: world.state().time,
        caused_by: None,
        body: WorldEventBody::ModuleStateUpdated(oasis7_wasm_abi::ModuleStateUpdate {
            module_id: "module.unrelated".to_string(),
            trace_id: "unrelated-tail".to_string(),
            state: vec![0x77],
        }),
    });

    assert_eq!(stale_snapshot.journal_len + 1, journal.len());
    assert!(stale_snapshot.capability_authorization_receipts.is_empty());
    assert!(!stale_snapshot.capability_invocation_contexts.is_empty());
    let recovered = World::from_snapshot(stale_snapshot, journal)
        .expect("unrelated module state tail must not look like a missing capability commit");
    assert_eq!(
        recovered.state().module_states.get("module.unrelated"),
        Some(&vec![0x77])
    );
    assert_eq!(recovered.capability_invocation_contexts().len(), 1);
}

#[test]
fn completed_context_replay_ignores_ordinary_trusted_trace_tail() {
    let mut world = fixture_world();
    let grant = signed_grant(grant_json(json!({})));
    let (catalog, response) = prepared_invocation(
        &world,
        &grant,
        catalog_json(json!({})),
        response_json(json!({})),
    );
    install_invocation_context(&mut world, &grant, &catalog, &response);
    let trusted_trace_id = format!("trusted-command-{}", response.response_nonce);
    execute_without_invocation_context(
        &mut world,
        grant,
        catalog,
        response,
        &mut RecordingSandbox::default(),
    )
    .expect("the completed capability context should have a durable receipt");

    let stale_snapshot = world.snapshot();
    assert!(!stale_snapshot.capability_authorization_receipts.is_empty());
    let mut journal = world.journal().clone();
    let next_event_id = journal
        .events
        .last()
        .map(|event| event.id.saturating_add(1))
        .expect("completed command should leave a journal tail");
    journal.append(WorldEvent {
        id: next_event_id,
        time: world.state().time,
        caused_by: None,
        body: WorldEventBody::ModuleStateUpdated(oasis7_wasm_abi::ModuleStateUpdate {
            module_id: MODULE_ID.to_string(),
            trace_id: trusted_trace_id,
            state: vec![0x7a],
        }),
    });

    let recovered = World::from_snapshot(stale_snapshot, journal).expect(
        "a completed context must not turn an ordinary matching trace into JournalMismatch",
    );
    assert_eq!(
        recovered.state().module_states.get(MODULE_ID),
        Some(&vec![0x7a]),
        "the ordinary module update should still replay after the completed context"
    );
}

#[test]
fn trusted_executor_rejects_future_issued_grant_before_sandbox() {
    let mut world = fixture_world();
    let future_tick = world.state().time.saturating_add(1);
    let grant = signed_grant(grant_json(json!({
        "issued_at_tick": future_tick
    })));
    install_budget_for_grant(&mut world, &grant, 128);
    let (catalog, response) = prepared_invocation(
        &world,
        &grant,
        catalog_json(json!({})),
        response_json(json!({})),
    );
    install_invocation_context(&mut world, &grant, &catalog, &response);
    let mut sandbox = RecordingSandbox::default();

    let error =
        execute_without_invocation_context(&mut world, grant, catalog, response, &mut sandbox)
            .expect_err("future-issued grant must fail closed");
    assert!(matches!(
        error,
        WorldError::CapabilityAuthorizationDenied { .. }
    ));
    assert_eq!(sandbox.calls, 0);
}

#[test]
fn trusted_executor_rejects_catalog_entry_without_grant_eligibility() {
    for eligible_grant_ids in [json!([]), json!(["grant-not-issued-to-this-command"])] {
        let mut world = fixture_world();
        let grant = signed_grant(grant_json(json!({})));
        let (catalog, response) = prepared_invocation(
            &world,
            &grant,
            catalog_json(json!({
                "entries": [{
                    "module_id": MODULE_ID,
                    "module_version": MODULE_VERSION,
                    "namespace": "weather",
                    "command": "observe",
                    "schema_version": 1,
                    "schema_hash": SCHEMA_HASH,
                    "max_payload_bytes": 128,
                    "eligible_grant_ids": eligible_grant_ids
                }]
            })),
            response_json(json!({})),
        );
        install_invocation_context(&mut world, &grant, &catalog, &response);
        let mut sandbox = RecordingSandbox::default();

        let error =
            execute_without_invocation_context(&mut world, grant, catalog, response, &mut sandbox)
                .expect_err("catalog eligibility must be checked against the live entry");
        assert!(matches!(
            error,
            WorldError::CapabilityAuthorizationDenied { .. }
        ));
        assert_eq!(sandbox.calls, 0);
    }
}

#[test]
fn trusted_executor_rejects_omitted_selector_for_targeted_command() {
    let mut world = fixture_world();
    let grant = signed_grant(grant_json(json!({})));
    let payload = serde_json::to_vec(&json!({
        "entity_id": "station-1",
        "resource_id": "weather.read"
    }))
    .expect("encode targeted command payload");
    let (catalog, response) = prepared_invocation(
        &world,
        &grant,
        catalog_json(json!({})),
        response_json(json!({
            "envelope": {"payload": payload}
        })),
    );
    install_invocation_context(&mut world, &grant, &catalog, &response);
    let mut sandbox = RecordingSandbox::default();

    let error =
        execute_without_invocation_context(&mut world, grant, catalog, response, &mut sandbox)
            .expect_err("an omitted selector must not act as a target wildcard");
    assert!(matches!(
        error,
        WorldError::CapabilityAuthorizationDenied { .. }
    ));
    assert_eq!(sandbox.calls, 0);
}

#[test]
fn snapshot_restore_rejects_missing_authorization_root_with_v2_state() {
    let world = fixture_world();
    let mut snapshot = world.snapshot();
    snapshot.capability_authorization_root.clear();

    let error = World::from_snapshot(snapshot, world.journal().clone())
        .expect_err("populated v2 state must not permit root recomputation");
    assert!(matches!(
        error,
        WorldError::CapabilityAuthorizationDenied { .. }
    ));
}

#[test]
fn trusted_executor_rejects_agent_subject_without_live_owner_generation() {
    let cases = [
        ("agent-missing", "owner-7", 1_u64),
        (SUBJECT_ID, "stale-owner", 1_u64),
        (SUBJECT_ID, "owner-7", 99_u64),
    ];
    for (agent_id, owner_binding, generation) in cases {
        let mut world = fixture_world();
        let subject = json!({
            "kind": "agent",
            "agent_id": agent_id,
            "owner_binding": owner_binding,
            "generation": generation
        });
        let grant = signed_grant(grant_json(json!({"subject": subject.clone()})));
        install_budget_for_grant(&mut world, &grant, 128);
        let (catalog, response) = prepared_invocation(
            &world,
            &grant,
            catalog_json(json!({"subject": subject.clone()})),
            response_json(json!({"subject": subject})),
        );
        install_invocation_context(&mut world, &grant, &catalog, &response);
        let mut sandbox = RecordingSandbox::default();

        let error =
            execute_without_invocation_context(&mut world, grant, catalog, response, &mut sandbox)
                .expect_err("agent authorization requires a live owner/generation binding");
        assert!(matches!(
            error,
            WorldError::CapabilityAuthorizationDenied { .. }
        ));
        assert_eq!(sandbox.calls, 0);
    }
}

#[test]
fn trusted_executor_rejects_unknown_module_instance_subject() {
    let mut world = fixture_world();
    let subject = json!({
        "kind": "module",
        "module_id": MODULE_ID,
        "module_version": MODULE_VERSION,
        "instance_id": "forged-module-instance"
    });
    let grant = signed_grant(grant_json(json!({"subject": subject.clone()})));
    install_budget_for_grant(&mut world, &grant, 128);
    let (catalog, response) = prepared_invocation(
        &world,
        &grant,
        catalog_json(json!({"subject": subject.clone()})),
        response_json(json!({"subject": subject})),
    );
    install_invocation_context(&mut world, &grant, &catalog, &response);
    let mut sandbox = RecordingSandbox::default();

    let error =
        execute_without_invocation_context(&mut world, grant, catalog, response, &mut sandbox)
            .expect_err("module subject must identify a live active instance");
    assert!(matches!(
        error,
        WorldError::CapabilityAuthorizationDenied { .. }
    ));
    assert_eq!(sandbox.calls, 0);
}

#[test]
fn trusted_executor_receipt_world_head_includes_command_commit() {
    let mut world = fixture_world();
    let grant = signed_grant(grant_json(json!({})));
    let (catalog, response) = prepared_invocation(
        &world,
        &grant,
        catalog_json(json!({})),
        response_json(json!({})),
    );
    install_invocation_context(&mut world, &grant, &catalog, &response);
    let mut sandbox = RecordingSandbox::default();
    let receipt =
        execute_without_invocation_context(&mut world, grant, catalog, response, &mut sandbox)
            .expect("trusted command should commit");

    let last_event = world.journal().events.last().expect("command commit event");
    assert!(matches!(
        last_event.body,
        WorldEventBody::CapabilityAuthorization(
            CapabilityAuthorizationEvent::CommandCommitted { .. }
        )
    ));
    assert_eq!(receipt.world_head_after, Some(last_event.id));
}

#[test]
fn trusted_executor_failure_after_prepare_publishes_no_observable_delta() {
    let mut world = fixture_world();
    let grant = signed_grant(grant_json(json!({})));
    let (catalog, response) = prepared_invocation(
        &world,
        &grant,
        catalog_json(json!({})),
        response_json(json!({})),
    );
    install_invocation_context(&mut world, &grant, &catalog, &response);
    let snapshot_before = world.snapshot();
    let journal_before = world.journal().clone();
    let consensus_before = world.tick_consensus_records().to_vec();
    let mut sandbox = ConfiguredSandbox {
        calls: 0,
        output: ModuleOutput {
            new_state: Some(vec![0x52]),
            effects: Vec::new(),
            emits: vec![oasis7_wasm_abi::ModuleEmit {
                kind: "trusted.prepare.failure".to_string(),
                payload: json!({"prepared": true}),
            }],
            tick_lifecycle: None,
            output_bytes: 8,
        },
    };

    world.fail_next_append_after_publication_prepare_for_test();
    execute_without_invocation_context(&mut world, grant, catalog, response, &mut sandbox)
        .expect_err("injected post-prepare failure must abort trusted command publication");

    assert_eq!(world.snapshot(), snapshot_before);
    assert_eq!(world.journal(), &journal_before);
    assert_eq!(world.tick_consensus_records(), consensus_before.as_slice());
}

#[test]
fn trusted_executor_rejects_linked_effect_queue_overflow_atomically() {
    let effect_grant = signed_effect_grant_with_selectors();
    let mut world = fixture_world_with_revocations_and_budget_and_effect_grant(
        BTreeSet::new(),
        128,
        effect_grant.clone(),
    )
    .with_runtime_memory_limits(WorldRuntimeMemoryLimits {
        max_pending_effects: 1,
        ..WorldRuntimeMemoryLimits::default()
    });
    let grant = signed_grant(grant_json(json!({})));
    let (catalog, response) = prepared_invocation(
        &world,
        &grant,
        catalog_json(json!({})),
        response_json(json!({})),
    );
    install_invocation_context(&mut world, &grant, &catalog, &response);
    let before = world.snapshot();
    let mut sandbox = ConfiguredSandbox {
        calls: 0,
        output: ModuleOutput {
            new_state: Some(vec![0x51]),
            effects: vec![
                ModuleEffectIntent {
                    kind: "weather.publish".to_string(),
                    params: json!({"entity_id": "station-1", "resource_id": "weather.read"}),
                    cap_ref: effect_grant.grant_id.clone(),
                    cap_slot: None,
                },
                ModuleEffectIntent {
                    kind: "weather.publish".to_string(),
                    params: json!({"entity_id": "station-1", "resource_id": "weather.read"}),
                    cap_ref: effect_grant.grant_id,
                    cap_slot: None,
                },
            ],
            emits: Vec::new(),
            tick_lifecycle: None,
            output_bytes: 0,
        },
    };

    let error =
        execute_without_invocation_context(&mut world, grant, catalog, response, &mut sandbox)
            .expect_err("bounded linked-effect overflow must fail deterministically");
    assert!(matches!(
        error,
        WorldError::CapabilityAuthorizationDenied { .. }
    ));
    assert_eq!(
        world.snapshot(),
        before,
        "debit and queued effects must roll back together"
    );
}

#[test]
fn trusted_executor_rejects_command_selector_target_mismatch_before_sandbox() {
    let cases = [
        (
            "entity target mismatch",
            json!({
                "entity_id": "station-2",
                "resource_id": "weather.read"
            }),
        ),
        (
            "resource target mismatch",
            json!({
                "entity_id": "station-1",
                "resource_id": "weather.write"
            }),
        ),
    ];
    for (label, payload) in cases {
        let mut world = fixture_world();
        let grant = signed_grant(grant_json(json!({
            "issued_at_tick": 0,
            "scope": {
                "entity_selector": ["station-1"],
                "resource_selector": ["weather.read"]
            }
        })));
        install_budget_for_grant(&mut world, &grant, 128);
        let payload = serde_json::to_vec(&payload).expect("encode selector target payload");
        let (catalog, response) = prepared_invocation(
            &world,
            &grant,
            catalog_json(json!({})),
            response_json(json!({
                "envelope": {"payload": payload}
            })),
        );
        install_invocation_context(&mut world, &grant, &catalog, &response);
        let mut sandbox = RecordingSandbox::default();

        let error =
            execute_without_invocation_context(&mut world, grant, catalog, response, &mut sandbox)
                .expect_err(label);
        assert!(matches!(
            error,
            WorldError::CapabilityAuthorizationDenied { .. }
        ));
        assert_eq!(sandbox.calls, 0, "{label} must fail before sandbox");
    }
}

#[test]
fn trusted_executor_rejects_effect_selector_target_mismatch_before_commit() {
    let effect_grant = signed_effect_grant_with_selectors();
    let mut world = fixture_world_with_revocations_and_budget_and_effect_grant(
        BTreeSet::new(),
        128,
        effect_grant.clone(),
    );
    let grant = signed_grant(grant_json(json!({})));
    let (catalog, response) = prepared_invocation(
        &world,
        &grant,
        catalog_json(json!({})),
        response_json(json!({})),
    );
    install_invocation_context(&mut world, &grant, &catalog, &response);
    let mut sandbox = ConfiguredSandbox {
        calls: 0,
        output: ModuleOutput {
            new_state: Some(vec![0xee]),
            effects: vec![ModuleEffectIntent {
                kind: "weather.publish".to_string(),
                params: json!({
                    "entity_id": "station-2",
                    "resource_id": "weather.read"
                }),
                cap_ref: effect_grant.grant_id,
                cap_slot: None,
            }],
            emits: Vec::new(),
            tick_lifecycle: None,
            output_bytes: 0,
        },
    };

    let error =
        execute_without_invocation_context(&mut world, grant, catalog, response, &mut sandbox)
            .expect_err("effect target outside the selector must fail closed");
    assert!(matches!(
        error,
        WorldError::CapabilityAuthorizationDenied { .. }
    ));
    assert_eq!(world.pending_effects_len(), 0);
    assert!(world.capability_nonce_records().is_empty());
    assert!(world.capability_authorization_receipts().is_empty());
}

#[test]
fn trusted_executor_preserves_module_and_system_subject_provenance() {
    let cases = [
        (
            json!({
                "kind": "module",
                "module_id": MODULE_ID,
                "module_version": MODULE_VERSION,
                "instance_id": "module.weather#1"
            }),
            ModuleCallCaller::Module {
                module_id: MODULE_ID.to_string(),
            },
        ),
        (
            json!({
                "kind": "system",
                "system_id": "system-weather",
                "epoch": 1
            }),
            ModuleCallCaller::System {
                system_id: "system-weather".to_string(),
            },
        ),
    ];
    for (subject, expected_caller) in cases {
        let mut world = fixture_world();
        let grant = signed_grant(grant_json(json!({
            "issued_at_tick": 0,
            "subject": subject,
            "scope": {
                "entity_selector": null,
                "resource_selector": null
            }
        })));
        install_budget_for_grant(&mut world, &grant, 128);
        let subject = serde_json::to_value(&grant.subject).expect("encode capability subject");
        let (catalog, response) = prepared_invocation(
            &world,
            &grant,
            catalog_json(json!({"subject": subject.clone()})),
            response_json(json!({"subject": subject})),
        );
        install_invocation_context(&mut world, &grant, &catalog, &response);
        let mut sandbox = ProvenanceSandbox::default();

        execute_without_invocation_context(&mut world, grant, catalog, response, &mut sandbox)
            .expect("subject-specific command should execute");
        assert_eq!(sandbox.requests.len(), 1);
        let input: ModuleCallInput = serde_cbor::from_slice(&sandbox.requests[0].input)
            .expect("decode trusted module call input");
        assert_eq!(input.ctx.caller, expected_caller);
    }
}
