//! Recovery regression coverage for pending capability contexts.

use super::super::*;
use super::capability_grant_v2::*;
use oasis7_wasm_abi::{ModuleEffectIntent, ModuleEmit, ModuleOutput};
use serde_json::json;

#[test]
fn capability_recovery_requires_commit_for_new_context_after_historical_receipt() {
    let mut world = fixture_world();
    let historical_grant = signed_grant(grant_json(json!({
        "grant_nonce": "historical-receipt-grant"
    })));
    install_budget_for_grant(&mut world, &historical_grant, 128);
    let (historical_catalog, historical_response) = prepared_invocation(
        &world,
        &historical_grant,
        catalog_json(json!({})),
        response_json(json!({
            "response_nonce": "response-history"
        })),
    );
    install_invocation_context(
        &mut world,
        &historical_grant,
        &historical_catalog,
        &historical_response,
    );
    execute_without_invocation_context(
        &mut world,
        historical_grant,
        historical_catalog,
        historical_response,
        &mut RecordingSandbox::default(),
    )
    .expect("historical command should leave a durable receipt");

    let crashed_grant = signed_grant(grant_json(json!({
        "grant_nonce": "crashed-command-grant"
    })));
    install_budget_for_grant(&mut world, &crashed_grant, 128);
    let (crashed_catalog, crashed_response) = prepared_invocation(
        &world,
        &crashed_grant,
        catalog_json(json!({})),
        response_json(json!({
            "response_nonce": "response-crashed"
        })),
    );
    install_invocation_context(
        &mut world,
        &crashed_grant,
        &crashed_catalog,
        &crashed_response,
    );
    let stale_snapshot = world.snapshot();
    assert!(!stale_snapshot.capability_authorization_receipts.is_empty());
    assert!(
        stale_snapshot
            .capability_invocation_contexts
            .values()
            .any(|context| { context.response_nonce == "response-crashed" })
    );

    let mut journal = world.journal().clone();
    execute_without_invocation_context(
        &mut world,
        crashed_grant,
        crashed_catalog,
        crashed_response,
        &mut RecordingSandbox::default(),
    )
    .expect("the simulated command should produce a pre-commit journal tail");
    let command_commit = world.journal().events.last().expect("command commit event");
    assert!(matches!(
        command_commit.body,
        WorldEventBody::CapabilityAuthorization(
            CapabilityAuthorizationEvent::CommandCommitted { .. }
        )
    ));
    journal.events.extend(
        world.journal().events[stale_snapshot.journal_len..world.journal().events.len() - 1]
            .iter()
            .cloned(),
    );

    let error = World::from_snapshot(stale_snapshot, journal)
        .expect_err("a new pending context without its command commit must fail closed");
    assert!(matches!(error, WorldError::JournalMismatch));
}

#[test]
fn capability_recovery_requires_a_commit_for_each_pending_context() {
    let mut world = fixture_world();
    let first_grant = signed_grant(grant_json(json!({
        "grant_nonce": "pending-context-one"
    })));
    let second_grant = signed_grant(grant_json(json!({
        "grant_nonce": "pending-context-two"
    })));
    install_budget_for_grant(&mut world, &first_grant, 128);
    install_budget_for_grant(&mut world, &second_grant, 128);

    let base_head = world
        .journal()
        .events
        .last()
        .map(|event| event.id)
        .unwrap_or(0);
    let expected_context_head = base_head.saturating_add(2);
    let mut first_catalog = prepared_catalog(&world, &first_grant, catalog_json(json!({})));
    first_catalog.world_head = expected_context_head;
    first_catalog.snapshot_id = first_catalog
        .canonical_hash()
        .expect("compute first pending context catalog hash");
    let first_response = prepared_response(
        response_json(json!({
            "response_nonce": "response-pending-one"
        })),
        &first_catalog,
    );
    let mut second_catalog = prepared_catalog(&world, &second_grant, catalog_json(json!({})));
    second_catalog.world_head = expected_context_head;
    second_catalog.snapshot_id = second_catalog
        .canonical_hash()
        .expect("compute second pending context catalog hash");
    let second_response = prepared_response(
        response_json(json!({
            "response_nonce": "response-pending-two"
        })),
        &second_catalog,
    );
    install_invocation_context(&mut world, &first_grant, &first_catalog, &first_response);
    install_invocation_context(&mut world, &second_grant, &second_catalog, &second_response);
    let stale_snapshot = world.snapshot();

    let mut sandbox = RecordingSandbox::default();
    execute_without_invocation_context(
        &mut world,
        first_grant,
        first_catalog,
        first_response,
        &mut sandbox,
    )
    .expect("the first pending context should commit");
    let mut journal = world.journal().clone();
    let next_event_id = journal
        .events
        .last()
        .map(|event| event.id.saturating_add(1))
        .expect("first command has a journal tail");
    journal.append(WorldEvent {
        id: next_event_id,
        time: world.state().time,
        caused_by: None,
        body: WorldEventBody::ModuleStateUpdated(oasis7_wasm_abi::ModuleStateUpdate {
            module_id: MODULE_ID.to_string(),
            trace_id: "trusted-command-response-pending-two".to_string(),
            state: vec![0x78],
        }),
    });
    assert!(journal.events.iter().any(|event| matches!(
        &event.body,
        WorldEventBody::CapabilityAuthorization(
            CapabilityAuthorizationEvent::CommandCommitted { receipt, .. }
        ) if receipt.response_nonce.as_deref() == Some("response-pending-one")
    )));
    assert!(journal.events.iter().any(|event| matches!(
        &event.body,
        WorldEventBody::ModuleStateUpdated(update)
            if update.trace_id == "trusted-command-response-pending-two"
    )));

    let error = World::from_snapshot(stale_snapshot, journal)
        .expect_err("one matching commit must not discharge another pending context");
    assert!(matches!(error, WorldError::JournalMismatch));
}

#[test]
fn capability_recovery_rejects_effect_only_tail_before_command_commit() {
    let mut world = fixture_world();
    let grant = signed_grant(grant_json(json!({
        "grant_nonce": "effect-only-crash-window"
    })));
    install_budget_for_grant(&mut world, &grant, 128);
    let (catalog, response) = prepared_invocation(
        &world,
        &grant,
        catalog_json(json!({})),
        response_json(json!({
            "response_nonce": "effect-only-response"
        })),
    );
    install_invocation_context(&mut world, &grant, &catalog, &response);
    let stale_snapshot = world.snapshot();

    let effect_grant = signed_effect_grant();
    let mut sandbox = ConfiguredSandbox {
        calls: 0,
        output: ModuleOutput {
            new_state: None,
            effects: vec![ModuleEffectIntent {
                kind: "weather.publish".to_string(),
                params: json!({"station": "station-crash-window"}),
                cap_ref: effect_grant.grant_id,
                cap_slot: None,
            }],
            emits: Vec::new(),
            tick_lifecycle: None,
            output_bytes: 16,
        },
    };
    execute_without_invocation_context(
        &mut world,
        grant.clone(),
        catalog.clone(),
        response.clone(),
        &mut sandbox,
    )
    .expect("effect-only command should commit before the simulated crash");

    let mut incomplete_journal = world.journal().clone();
    let commit = incomplete_journal
        .events
        .pop()
        .expect("effect-only command commit event");
    assert!(matches!(
        commit.body,
        WorldEventBody::CapabilityAuthorization(CapabilityAuthorizationEvent::CommandCommitted {
            receipt,
            ..
        }) if receipt.response_nonce.as_deref() == Some("effect-only-response")
    ));
    let queued_intent = incomplete_journal
        .events
        .iter()
        .find_map(|event| match &event.body {
            WorldEventBody::EffectQueued(intent)
                if intent.kind == "weather.publish"
                    && intent.params["station"] == "station-crash-window" =>
            {
                Some(intent.intent_id.clone())
            }
            _ => None,
        })
        .expect("crash tail contains the effect before CommandCommitted");
    assert!(!incomplete_journal.events.iter().any(|event| matches!(
        &event.body,
        WorldEventBody::CapabilityAuthorization(
            CapabilityAuthorizationEvent::CommandCommitted { receipt, .. }
        ) if receipt.response_nonce.as_deref() == Some("effect-only-response")
    )));

    let recovery = World::from_snapshot(stale_snapshot, incomplete_journal);
    match recovery {
        Err(error) => assert!(matches!(error, WorldError::JournalMismatch)),
        Ok(mut recovered) => {
            assert_eq!(
                recovered.pending_effects_len(),
                0,
                "an incomplete authorization must not leave a dispatchable effect"
            );
            assert!(
                !recovered
                    .capability_effect_receipt_links()
                    .contains_key(&queued_intent),
                "an effect without CommandCommitted must never be receipt-unlinked"
            );
            let mut retry_sandbox = ConfiguredSandbox {
                calls: 0,
                output: ModuleOutput {
                    new_state: None,
                    effects: Vec::new(),
                    emits: Vec::new(),
                    tick_lifecycle: None,
                    output_bytes: 0,
                },
            };
            let retry = execute_without_invocation_context(
                &mut recovered,
                grant,
                catalog,
                response,
                &mut retry_sandbox,
            );
            assert!(
                retry.is_err(),
                "an incomplete effect-only transaction must not be retryable"
            );
            assert_eq!(
                retry_sandbox.calls, 0,
                "crash recovery must not execute the effect command a second time"
            );
            panic!("effect-only crash tail was accepted instead of returning JournalMismatch");
        }
    }
}

#[test]
fn capability_recovery_rejects_emit_only_tail_before_command_commit() {
    let mut world = fixture_world();
    let grant = signed_grant(grant_json(json!({
        "grant_nonce": "emit-only-crash-window"
    })));
    install_budget_for_grant(&mut world, &grant, 128);
    let (catalog, response) = prepared_invocation(
        &world,
        &grant,
        catalog_json(json!({})),
        response_json(json!({
            "response_nonce": "emit-only-response"
        })),
    );
    install_invocation_context(&mut world, &grant, &catalog, &response);
    let stale_snapshot = world.snapshot();

    let mut sandbox = ConfiguredSandbox {
        calls: 0,
        output: ModuleOutput {
            new_state: None,
            effects: Vec::new(),
            emits: vec![ModuleEmit {
                kind: "weather.crash-window".to_string(),
                payload: json!({"station": "station-crash-window"}),
            }],
            tick_lifecycle: None,
            output_bytes: 16,
        },
    };
    execute_without_invocation_context(
        &mut world,
        grant.clone(),
        catalog.clone(),
        response.clone(),
        &mut sandbox,
    )
    .expect("emit-only command should commit before the simulated crash");

    let mut incomplete_journal = world.journal().clone();
    let commit = incomplete_journal
        .events
        .pop()
        .expect("emit-only command commit event");
    assert!(matches!(
        commit.body,
        WorldEventBody::CapabilityAuthorization(CapabilityAuthorizationEvent::CommandCommitted {
            receipt,
            ..
        }) if receipt.response_nonce.as_deref() == Some("emit-only-response")
    ));
    assert!(incomplete_journal.events.iter().any(|event| matches!(
        &event.body,
        WorldEventBody::ModuleEmitted(event)
            if event.kind == "weather.crash-window"
                && event.payload["station"] == "station-crash-window"
    )));
    assert!(!incomplete_journal.events.iter().any(|event| matches!(
        &event.body,
        WorldEventBody::CapabilityAuthorization(
            CapabilityAuthorizationEvent::CommandCommitted { receipt, .. }
        ) if receipt.response_nonce.as_deref() == Some("emit-only-response")
    )));

    let recovery = World::from_snapshot(stale_snapshot, incomplete_journal);
    match recovery {
        Err(error) => assert!(matches!(error, WorldError::JournalMismatch)),
        Ok(mut recovered) => {
            let mut retry_sandbox = ConfiguredSandbox {
                calls: 0,
                output: ModuleOutput {
                    new_state: None,
                    effects: Vec::new(),
                    emits: Vec::new(),
                    tick_lifecycle: None,
                    output_bytes: 0,
                },
            };
            let retry = execute_without_invocation_context(
                &mut recovered,
                grant,
                catalog,
                response,
                &mut retry_sandbox,
            );
            assert!(
                retry.is_err(),
                "an incomplete emit-only transaction must not be retryable"
            );
            assert_eq!(
                retry_sandbox.calls, 0,
                "crash recovery must not execute the emit command a second time"
            );
            panic!("emit-only crash tail was accepted instead of returning JournalMismatch");
        }
    }
}
