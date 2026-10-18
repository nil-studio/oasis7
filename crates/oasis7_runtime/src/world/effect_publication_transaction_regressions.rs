use super::super::*;
use super::World;

fn intent(id: &str) -> EffectIntent {
    EffectIntent {
        birth_era: None,
        intent_id: id.into(),
        kind: "fixture.effect".into(),
        params: serde_json::json!({"intent": id}),
        cap_ref: "fixture-capability".into(),
        origin: EffectOrigin::System,
    }
}

fn receipt(id: &str) -> EffectReceipt {
    EffectReceipt {
        birth_era: None,
        world: None,
        intent_payload_digest: None,
        intent_id: id.into(),
        status: "ok".into(),
        payload: serde_json::json!({"intent": id}),
        cost_cents: Some(1),
        signature: None,
    }
}

fn append_effect(world: &mut World, id: &str) {
    world
        .append_event(WorldEventBody::EffectQueued(intent(id)), None)
        .expect("append fixture effect");
}

fn assert_publication_unchanged(world: &World, expected: &World, expected_root: &str) {
    assert_eq!(world.snapshot(), expected.snapshot());
    assert_eq!(world.pending_effects, expected.pending_effects);
    assert_eq!(world.inflight_effects, expected.inflight_effects);
    assert_eq!(world.journal, expected.journal);
    assert_eq!(world.next_event_id, expected.next_event_id);
    assert_eq!(world.next_event_id_era, expected.next_event_id_era);
    assert_eq!(
        world.runtime_backpressure_stats(),
        expected.runtime_backpressure_stats()
    );
    assert_eq!(
        world.tick_consensus_records(),
        expected.tick_consensus_records()
    );
    assert_eq!(world.state.time, expected.state.time);
    assert_eq!(
        world.current_state_root_hash().expect("current state root"),
        expected_root
    );
}

fn assert_injected(error: &WorldError) {
    assert!(
        matches!(
            error,
            WorldError::ResourceBalanceInvalid { reason }
                if reason == "injected append_event failure after publication preparation"
        ),
        "unexpected error: {error:?}"
    );
}

#[test]
fn raw_effect_queue_pressure_post_prepare_failure_preserves_publication_state() {
    let mut world = World::new().with_runtime_memory_limits(WorldRuntimeMemoryLimits {
        max_pending_effects: 1,
        ..WorldRuntimeMemoryLimits::default()
    });
    append_effect(&mut world, "intent-existing");
    let before = world.clone();
    let root_before = world.current_state_root_hash().expect("state root before");

    world.fail_next_append_after_publication_prepare_for_test();
    let error = world
        .append_event(
            WorldEventBody::EffectQueued(intent("intent-replacement")),
            None,
        )
        .expect_err("post-prepare fault must reject raw effect publication");

    assert_injected(&error);
    assert_publication_unchanged(&world, &before, &root_before);
}

#[test]
fn raw_pending_receipt_post_prepare_failure_preserves_publication_state() {
    let mut world = World::new();
    append_effect(&mut world, "intent-pending");
    let before = world.clone();
    let root_before = world.current_state_root_hash().expect("state root before");

    world.fail_next_append_after_publication_prepare_for_test();
    let error = world
        .append_event(
            WorldEventBody::ReceiptAppended(receipt("intent-pending")),
            None,
        )
        .expect_err("post-prepare fault must reject pending receipt publication");

    assert_injected(&error);
    assert_publication_unchanged(&world, &before, &root_before);
}

#[test]
fn raw_inflight_receipt_post_prepare_failure_preserves_publication_state() {
    let mut world = World::new();
    append_effect(&mut world, "intent-inflight");
    assert_eq!(
        world
            .take_next_effect()
            .expect("dispatch fixture effect")
            .intent_id,
        "intent-inflight"
    );
    let before = world.clone();
    let root_before = world.current_state_root_hash().expect("state root before");

    world.fail_next_append_after_publication_prepare_for_test();
    let error = world
        .append_event(
            WorldEventBody::ReceiptAppended(receipt("intent-inflight")),
            None,
        )
        .expect_err("post-prepare fault must reject inflight receipt publication");

    assert_injected(&error);
    assert_publication_unchanged(&world, &before, &root_before);
}

#[test]
fn raw_effect_queue_eviction_remains_exact_and_deterministic() {
    let mut world = World::new().with_runtime_memory_limits(WorldRuntimeMemoryLimits {
        max_pending_effects: 1,
        ..WorldRuntimeMemoryLimits::default()
    });

    append_effect(&mut world, "intent-first");
    append_effect(&mut world, "intent-second");

    assert_eq!(world.pending_effects.len(), 1);
    assert_eq!(world.pending_effects[0], intent("intent-second"));
    assert_eq!(world.runtime_backpressure_stats.pending_effects_evicted, 1);
}

#[test]
fn unknown_receipt_error_precedes_post_prepare_fault() {
    let mut world = World::new();
    append_effect(&mut world, "intent-known");
    let before = world.clone();
    let root_before = world.current_state_root_hash().expect("state root before");
    world.fail_next_append_after_publication_prepare_for_test();

    let error = world
        .append_event(
            WorldEventBody::ReceiptAppended(receipt("intent-unknown")),
            None,
        )
        .expect_err("unknown receipt must fail before publication preparation");
    assert!(matches!(
        error,
        WorldError::ReceiptUnknownIntent { intent_id } if intent_id == "intent-unknown"
    ));
    assert_publication_unchanged(&world, &before, &root_before);

    let error = world
        .append_event(
            WorldEventBody::ReceiptAppended(receipt("intent-known")),
            None,
        )
        .expect_err("unknown receipt must not consume the post-prepare fault");
    assert_injected(&error);
    assert_publication_unchanged(&world, &before, &root_before);
}

#[test]
fn successful_receipts_remove_pending_and_inflight_duplicates_without_cross_removal() {
    let mut world = World::new().with_runtime_memory_limits(WorldRuntimeMemoryLimits {
        max_pending_effects: 4,
        ..WorldRuntimeMemoryLimits::default()
    });
    append_effect(&mut world, "intent-inflight");
    append_effect(&mut world, "intent-pending");
    append_effect(&mut world, "intent-pending");
    assert_eq!(
        world
            .take_next_effect()
            .expect("dispatch first effect")
            .intent_id,
        "intent-inflight"
    );

    world
        .append_event(
            WorldEventBody::ReceiptAppended(receipt("intent-inflight")),
            None,
        )
        .expect("remove inflight effect");
    assert!(!world.inflight_effects.contains_key("intent-inflight"));
    assert_eq!(world.pending_effects.len(), 2);
    assert!(
        world
            .pending_effects
            .iter()
            .all(|pending| pending.intent_id == "intent-pending")
    );

    world
        .append_event(
            WorldEventBody::ReceiptAppended(receipt("intent-pending")),
            None,
        )
        .expect("remove every pending duplicate");
    assert!(world.pending_effects.is_empty());
    assert!(world.inflight_effects.is_empty());
}
