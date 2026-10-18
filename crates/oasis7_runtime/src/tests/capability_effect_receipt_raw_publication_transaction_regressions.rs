use super::super::*;
use serde_json::json;

fn captured_closure() -> (World, CapabilityAuthorizationEvent) {
    let (mut world, command) =
        super::capability_command_raw_publication_transaction_regressions::captured_command();
    world
        .append_event_for_test(WorldEventBody::CapabilityAuthorization(command), None)
        .expect("install command link");
    let intent = world.take_next_effect().expect("linked pending effect");
    let mut preclosure = world.clone();
    let start = world.journal().events.len();
    world
        .ingest_receipt(EffectReceipt {
            birth_era: None,
            world: None,
            intent_payload_digest: None,
            intent_id: intent.intent_id,
            status: "ok".into(),
            payload: json!({"closed": true}),
            cost_cents: Some(1),
            signature: None,
        })
        .expect("capture specialized closure");
    let events = &world.journal().events[start..];
    let closure_index = events
        .iter()
        .position(|event| {
            matches!(
                event.body,
                WorldEventBody::CapabilityAuthorization(
                    CapabilityAuthorizationEvent::EffectReceiptCommitted { .. }
                )
            )
        })
        .expect("specialized closure event");
    let event = events[closure_index].body.clone();
    let WorldEventBody::CapabilityAuthorization(
        event @ CapabilityAuthorizationEvent::EffectReceiptCommitted { .. },
    ) = event
    else {
        panic!("closure tail is not EffectReceiptCommitted");
    };
    for preceding in &events[..closure_index] {
        preclosure
            .append_event_for_test(preceding.body.clone(), preceding.caused_by.clone())
            .expect("replay receipt prefix");
    }
    (preclosure, event)
}

fn assert_unchanged(world: &World, before: &World, root: &str) {
    assert_eq!(world.snapshot(), before.snapshot());
    assert_eq!(
        world.capability_authorization_receipts(),
        before.capability_authorization_receipts()
    );
    assert_eq!(
        world.capability_effect_receipt_links(),
        before.capability_effect_receipt_links()
    );
    assert_eq!(
        world.capability_authorization_root(),
        before.capability_authorization_root()
    );
    assert_eq!(world.pending_effects_len(), before.pending_effects_len());
    assert_eq!(world.journal(), before.journal());
    assert_eq!(
        world.runtime_backpressure_stats(),
        before.runtime_backpressure_stats()
    );
    assert_eq!(
        world.tick_consensus_records(),
        before.tick_consensus_records()
    );
    assert_eq!(world.state().time, before.state().time);
    assert_eq!(world.current_state_root_hash().expect("root"), root);
}

#[test]
fn raw_effect_receipt_closure_fails_atomically_then_retries_same_world() {
    let (mut world, event) = captured_closure();
    let before = world.clone();
    let baseline = world.snapshot();
    let root = world.current_state_root_hash().expect("root before");
    world.fail_next_append_after_publication_prepare_for_test();
    let error = world
        .append_event_for_test(
            WorldEventBody::CapabilityAuthorization(event.clone()),
            Some(CausedBy::Action(384)),
        )
        .expect_err("raw closure honors failpoint");
    assert!(
        matches!(error, WorldError::ResourceBalanceInvalid { ref reason } if reason.contains("publication preparation"))
    );
    assert_unchanged(&world, &before, &root);
    let cause = Some(CausedBy::Action(385));
    world
        .append_event_for_test(
            WorldEventBody::CapabilityAuthorization(event),
            cause.clone(),
        )
        .expect("same-world retry");
    assert_eq!(
        world
            .journal()
            .events
            .last()
            .and_then(|event| event.caused_by.clone()),
        cause
    );
    let replay =
        World::from_snapshot(baseline, world.journal().clone()).expect("replay closure retry");
    assert_eq!(
        replay.capability_authorization_receipts(),
        world.capability_authorization_receipts()
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
fn raw_effect_receipt_closure_preserves_binding_priorities_and_idempotency() {
    for expected in [
        "authorization link is missing",
        "authorization link does not match",
    ] {
        let (mut world, mut event) = captured_closure();
        let CapabilityAuthorizationEvent::EffectReceiptCommitted {
            intent_id,
            authorization_receipt_id,
            ..
        } = &mut event
        else {
            unreachable!()
        };
        if expected.contains("missing") {
            *intent_id = "missing-intent".into();
        } else {
            *authorization_receipt_id = "wrong-audit".into();
        }
        let before = world.clone();
        let root = world.current_state_root_hash().expect("root before");
        let error = world
            .append_event_for_test(WorldEventBody::CapabilityAuthorization(event), None)
            .expect_err(expected);
        assert!(
            matches!(error, WorldError::CapabilityAuthorizationDenied { ref reason } if reason.contains(expected))
        );
        assert_unchanged(&world, &before, &root);
    }
    let (mut world, event) = captured_closure();
    world
        .append_event_for_test(WorldEventBody::CapabilityAuthorization(event.clone()), None)
        .expect("first closure");
    let receipts = world.capability_authorization_receipts().clone();
    let links = world.capability_effect_receipt_links().clone();
    world
        .append_event_for_test(WorldEventBody::CapabilityAuthorization(event), None)
        .expect("already committed closure is idempotent");
    assert_eq!(world.capability_authorization_receipts(), &receipts);
    assert_eq!(world.capability_effect_receipt_links(), &links);
}
