use super::super::*;
use oasis7_wasm_abi::{ModuleEffectIntent, ModuleOutput};
use serde_json::json;

pub(super) fn captured_command() -> (World, CapabilityAuthorizationEvent) {
    let mut world = super::capability_grant_v2::fixture_world();
    let grant =
        super::capability_grant_v2::signed_grant(super::capability_grant_v2::grant_json(json!({})));
    let (catalog, response) = super::capability_grant_v2::prepared_invocation(
        &world,
        &grant,
        super::capability_grant_v2::catalog_json(json!({})),
        super::capability_grant_v2::response_json(json!({})),
    );
    super::capability_grant_v2::install_invocation_context(&mut world, &grant, &catalog, &response);
    let mut precommit = world.clone();
    let command_start = world.journal().events.len();
    let mut sandbox = super::capability_grant_v2::ConfiguredSandbox {
        calls: 0,
        output: ModuleOutput {
            new_state: Some(vec![0x38]),
            effects: vec![ModuleEffectIntent {
                kind: "weather.publish".into(),
                params: json!({"station": "raw-command"}),
                cap_ref: super::capability_grant_v2::signed_effect_grant().grant_id,
                cap_slot: None,
            }],
            emits: Vec::new(),
            tick_lifecycle: None,
            output_bytes: 32,
        },
    };
    super::capability_grant_v2::execute_without_invocation_context(
        &mut world,
        grant,
        catalog,
        response,
        &mut sandbox,
    )
    .expect("capture valid trusted command");
    let command_events = &world.journal().events[command_start..];
    let event = command_events
        .last()
        .expect("command commit tail")
        .body
        .clone();
    let WorldEventBody::CapabilityAuthorization(
        event @ CapabilityAuthorizationEvent::CommandCommitted { .. },
    ) = event
    else {
        panic!("trusted command tail is not CommandCommitted");
    };
    for preceding in &command_events[..command_events.len() - 1] {
        precommit
            .append_event_for_test(preceding.body.clone(), preceding.caused_by.clone())
            .expect("replay trusted-command prefix into exact pre-commit world");
    }
    (precommit, event)
}

fn assert_unchanged(world: &World, before: &World, root: &str) {
    assert_eq!(world.snapshot(), before.snapshot());
    assert_eq!(world.capability_grants_v2(), before.capability_grants_v2());
    assert_eq!(
        world.capability_revocation_state(),
        before.capability_revocation_state()
    );
    assert_eq!(
        world.capability_nonce_records(),
        before.capability_nonce_records()
    );
    assert_eq!(
        world.capability_authorization_receipts(),
        before.capability_authorization_receipts()
    );
    assert_eq!(
        world.capability_invocation_contexts(),
        before.capability_invocation_contexts()
    );
    assert_eq!(
        world.capability_budget_accounts(),
        before.capability_budget_accounts()
    );
    assert_eq!(
        world.capability_effect_receipt_links(),
        before.capability_effect_receipt_links()
    );
    assert_eq!(
        world.capability_authorization_root(),
        before.capability_authorization_root()
    );
    assert_eq!(world.journal(), before.journal());
    assert_eq!(
        world.runtime_backpressure_stats(),
        before.runtime_backpressure_stats()
    );
    assert_eq!(
        world.tick_consensus_records(),
        before.tick_consensus_records()
    );
    assert_eq!(world.pending_effects_len(), before.pending_effects_len());
    assert_eq!(world.state().time, before.state().time);
    assert_eq!(world.current_state_root_hash().expect("root"), root);
}

#[test]
fn raw_command_commit_fails_atomically_after_prepare() {
    let (mut world, event) = captured_command();
    let before = world.clone();
    let baseline = world.snapshot();
    let root = world.current_state_root_hash().expect("root before");
    world.fail_next_append_after_publication_prepare_for_test();
    let error = world
        .append_event_for_test(
            WorldEventBody::CapabilityAuthorization(event.clone()),
            Some(CausedBy::Action(381)),
        )
        .expect_err("raw command commit must honor post-prepare failure");
    assert!(
        matches!(error, WorldError::ResourceBalanceInvalid { ref reason } if reason.contains("publication preparation"))
    );
    assert_unchanged(&world, &before, &root);

    let retry_cause = Some(CausedBy::Action(383));
    world
        .append_event_for_test(
            WorldEventBody::CapabilityAuthorization(event),
            retry_cause.clone(),
        )
        .expect("same-world retry succeeds after injected failure");
    assert_eq!(
        world
            .journal()
            .events
            .last()
            .and_then(|event| event.caused_by.clone()),
        retry_cause
    );
    let replay = World::from_snapshot(baseline, world.journal().clone())
        .expect("replay same-world command retry");
    assert_eq!(
        replay.capability_nonce_records(),
        world.capability_nonce_records()
    );
    assert_eq!(
        replay.capability_authorization_receipts(),
        world.capability_authorization_receipts()
    );
    assert_eq!(
        replay.capability_budget_accounts(),
        world.capability_budget_accounts()
    );
    assert_eq!(
        replay.capability_effect_receipt_links(),
        world.capability_effect_receipt_links()
    );
    assert_eq!(
        replay.capability_authorization_root(),
        world.capability_authorization_root()
    );
    assert_eq!(
        replay.current_state_root_hash().expect("replay root"),
        world.current_state_root_hash().expect("live root")
    );
}

#[test]
fn raw_command_commit_preserves_late_validation_priority() {
    for expected in [
        "nonce journal record is invalid",
        "budget predecessor is invalid",
        "capability effect receipt journal binding is invalid",
    ] {
        let (mut world, mut event) = captured_command();
        let CapabilityAuthorizationEvent::CommandCommitted {
            budget_key,
            budget_account,
            nonce_record,
            effect_receipt_links,
            ..
        } = &mut event
        else {
            unreachable!()
        };
        match expected {
            "nonce journal record is invalid" => nonce_record.state = "pending".into(),
            "budget predecessor is invalid" => {
                let mut prior = budget_account.clone();
                prior.remaining_units -= 1;
                world.seed_capability_budget_account_for_test(budget_key.clone(), prior);
            }
            _ => {
                effect_receipt_links
                    .values_mut()
                    .next()
                    .expect("effect link")
                    .authorization_receipt_id = "wrong".into()
            }
        }
        let before = world.clone();
        let root = world.current_state_root_hash().expect("root before");
        let error = world
            .append_event_for_test(WorldEventBody::CapabilityAuthorization(event), None)
            .expect_err(expected);
        assert!(
            matches!(error, WorldError::CapabilityAuthorizationDenied { ref reason } if reason.contains(expected)),
            "{error:?}"
        );
        assert_unchanged(&world, &before, &root);
    }
}

#[test]
fn raw_command_commit_retry_preserves_cause_and_replays_projection() {
    let (mut world, event) = captured_command();
    let baseline = world.snapshot();
    let cause = Some(CausedBy::Action(382));
    world
        .append_event_for_test(
            WorldEventBody::CapabilityAuthorization(event),
            cause.clone(),
        )
        .expect("raw commit");
    assert_eq!(
        world
            .journal()
            .events
            .last()
            .and_then(|event| event.caused_by.clone()),
        cause
    );
    let replay =
        World::from_snapshot(baseline, world.journal().clone()).expect("replay raw commit");
    assert_eq!(
        replay.capability_nonce_records(),
        world.capability_nonce_records()
    );
    assert_eq!(
        replay.capability_authorization_receipts(),
        world.capability_authorization_receipts()
    );
    assert_eq!(
        replay.capability_budget_accounts(),
        world.capability_budget_accounts()
    );
    assert_eq!(
        replay.capability_effect_receipt_links(),
        world.capability_effect_receipt_links()
    );
    assert_eq!(
        replay.capability_authorization_root(),
        world.capability_authorization_root()
    );
    assert_eq!(
        replay.current_state_root_hash().expect("replay root"),
        world.current_state_root_hash().expect("live root")
    );
}
