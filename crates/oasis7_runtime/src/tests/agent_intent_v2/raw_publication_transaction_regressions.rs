use super::*;

fn base_world() -> World {
    let mut state = WorldState::default();
    state.agents.insert(AGENT_ID.into(), legacy_agent_cell());
    World::new_with_state(state)
}

fn fixtures() -> Vec<(World, DomainEvent)> {
    let base = base_world();
    let mut lifecycle = base.clone();
    lifecycle
        .record_agent_chat_intent("player-39b", AGENT_ID, 1, "Start recipe")
        .expect("capture lifecycle");
    let lifecycle_events: Vec<_> = lifecycle
        .journal()
        .events
        .iter()
        .map(|event| match &event.body {
            WorldEventBody::Domain(
                event @ (DomainEvent::AgentIntentProposed { .. }
                | DomainEvent::AgentIntentSubmitted { .. }
                | DomainEvent::AgentIntentAccepted { .. }),
            ) => event.clone(),
            other => panic!("unexpected lifecycle event {other:?}"),
        })
        .collect();
    let mut out = Vec::new();
    let mut prefix = base;
    for event in lifecycle_events {
        out.push((prefix.clone(), event.clone()));
        prefix
            .append_event_for_test(WorldEventBody::Domain(event), None)
            .expect("apply lifecycle prefix");
    }
    let accepted = prefix;
    let current = accepted.state().agents[AGENT_ID]
        .intent
        .as_ref()
        .expect("accepted")
        .clone();
    let mut replacement_probe = accepted.clone();
    replacement_probe
        .record_agent_chat_intent_with_authority(
            "player-39b-next",
            AGENT_ID,
            2,
            "Start recipe",
            AgentIntentAuthorityContext {
                replaces_intent_id: Some(current.intent_id.clone()),
                ..AgentIntentAuthorityContext::default()
            },
        )
        .expect("capture replacement");
    let replacement = replacement_probe.journal().events[accepted.journal().events.len()..]
        .iter()
        .find_map(|event| match &event.body {
            WorldEventBody::Domain(event @ DomainEvent::AgentIntentReplaced { .. }) => {
                Some(event.clone())
            }
            _ => None,
        })
        .expect("replacement event");
    out.push((accepted.clone(), replacement));
    let mut terminal_probe = accepted.clone();
    terminal_probe
        .expire_agent_intent_exact(AGENT_ID, &current.intent_id, &current.request_digest)
        .expect("capture transition");
    let transitioned = match &terminal_probe
        .journal()
        .events
        .last()
        .expect("terminal event")
        .body
    {
        WorldEventBody::Domain(event @ DomainEvent::AgentIntentTransitioned { .. }) => {
            event.clone()
        }
        other => panic!("unexpected {other:?}"),
    };
    out.push((accepted, transitioned));
    out
}

fn intent_mut(event: &mut DomainEvent) -> &mut AgentIntentV2 {
    match event {
        DomainEvent::AgentIntentProposed { intent }
        | DomainEvent::AgentIntentSubmitted { intent }
        | DomainEvent::AgentIntentAccepted { intent }
        | DomainEvent::AgentIntentReplaced { intent }
        | DomainEvent::AgentIntentTransitioned { intent } => intent,
        other => panic!("unexpected intent event {other:?}"),
    }
}

fn completed_fixture() -> (World, DomainEvent) {
    let world = base_world();
    let snapshot = world.snapshot();
    let mut accepted = canonical_intent("intent-39b-completed", "accepted", "Start recipe", None);
    accepted["effect_intent_id"] = serde_json::json!("effect-39b-completed");
    accepted["world_id"] = serde_json::json!("runtime-world");
    accepted["reorg_epoch"] = serde_json::json!(2);
    accepted["authority_scope"] = serde_json::json!("player_agent_chat");
    accepted["event_seq"] = serde_json::json!(3);
    let mut proposed = accepted.clone();
    proposed["status"] = serde_json::json!("proposed");
    proposed["summary"] = serde_json::json!(lifecycle_summary("proposed"));
    proposed["event_seq"] = serde_json::json!(1);
    let mut submitted = accepted.clone();
    submitted["status"] = serde_json::json!("submitted");
    submitted["summary"] = serde_json::json!(lifecycle_summary("submitted"));
    submitted["event_seq"] = serde_json::json!(2);
    let prefix = Journal {
        events: vec![
            WorldEvent {
                id: 1,
                time: 7,
                caused_by: None,
                body: WorldEventBody::Domain(intent_event("AgentIntentProposed", proposed)),
            },
            WorldEvent {
                id: 2,
                time: 7,
                caused_by: None,
                body: WorldEventBody::Domain(intent_event("AgentIntentSubmitted", submitted)),
            },
            WorldEvent {
                id: 3,
                time: 7,
                caused_by: None,
                body: WorldEventBody::Domain(intent_event("AgentIntentAccepted", accepted.clone())),
            },
            WorldEvent {
                id: 4,
                time: 7,
                caused_by: None,
                body: WorldEventBody::EffectQueued(EffectIntent {
                    birth_era: None,
                    intent_id: "effect-39b-completed".into(),
                    kind: "test_effect".into(),
                    params: serde_json::json!({}),
                    cap_ref: "test".into(),
                    origin: EffectOrigin::System,
                }),
            },
            WorldEvent {
                id: 5,
                time: 7,
                caused_by: None,
                body: WorldEventBody::ReceiptAppended(EffectReceipt {
                    birth_era: None,
                    world: None,
                    intent_payload_digest: None,
                    intent_id: "effect-39b-completed".into(),
                    status: "ok".into(),
                    payload: serde_json::json!({}),
                    cost_cents: None,
                    signature: None,
                }),
            },
        ],
    };
    let world = World::from_snapshot(snapshot, prefix).expect("valid receipt witness prefix");
    let mut completed = accepted;
    completed["status"] = serde_json::json!("completed");
    completed["summary"] = serde_json::json!(lifecycle_summary("completed"));
    completed["event_seq"] = serde_json::json!(6);
    completed["updated_at"] = serde_json::json!(8);
    completed["receipt_ref"] = serde_json::json!("world-event:5");
    (world, intent_event("AgentIntentTransitioned", completed))
}

fn assert_unchanged(world: &World, before: &World, root: &str) {
    assert_eq!(world.snapshot(), before.snapshot());
    assert_eq!(
        world.state().agent_intent_ledger,
        before.state().agent_intent_ledger
    );
    assert_eq!(
        world.state().agents[AGENT_ID].intent,
        before.state().agents[AGENT_ID].intent
    );
    assert_eq!(
        world.state().agents[AGENT_ID].mailbox,
        before.state().agents[AGENT_ID].mailbox
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
    assert_eq!(world.current_state_root_hash().expect("root"), root);
}

fn assert_fail_retry(index: usize) {
    let (mut world, event) = fixtures().swap_remove(index);
    let before = world.clone();
    let baseline = world.snapshot();
    let root = world.current_state_root_hash().expect("root before");
    world.fail_next_append_after_publication_prepare_for_test();
    let error = world
        .append_event_for_test(
            WorldEventBody::Domain(event.clone()),
            Some(CausedBy::Action(393)),
        )
        .expect_err("intent event honors failpoint");
    assert!(
        matches!(error, WorldError::ResourceBalanceInvalid { ref reason } if reason.contains("publication preparation"))
    );
    assert_unchanged(&world, &before, &root);
    let cause = Some(CausedBy::Action(394));
    world
        .append_event_for_test(WorldEventBody::Domain(event), cause.clone())
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
        World::from_snapshot(baseline, world.journal().clone()).expect("replay intent retry");
    assert_eq!(replay.snapshot(), world.snapshot());
    assert_eq!(
        replay.current_state_root_hash().expect("replay root"),
        world.current_state_root_hash().expect("live root")
    );
}

#[test]
fn proposed_raw_is_atomic() {
    assert_fail_retry(0);
}
#[test]
fn submitted_raw_is_atomic() {
    assert_fail_retry(1);
}
#[test]
fn accepted_raw_is_atomic() {
    assert_fail_retry(2);
}
#[test]
fn replaced_raw_is_atomic() {
    assert_fail_retry(3);
}
#[test]
fn transitioned_raw_is_atomic() {
    assert_fail_retry(4);
}

#[test]
fn natural_validation_priority_is_precise_and_non_mutating() {
    let mut cases = Vec::new();
    let (world, mut missing_agent) = fixtures().swap_remove(0);
    intent_mut(&mut missing_agent).agent_id = "missing-agent".into();
    cases.push((world, missing_agent, "missing-agent"));

    let (world, mut wrong_status) = fixtures().swap_remove(1);
    intent_mut(&mut wrong_status).status = "accepted".into();
    intent_mut(&mut wrong_status).summary = lifecycle_summary("accepted").to_string();
    cases.push((
        world,
        wrong_status,
        "submitted event must carry status submitted",
    ));

    let (world, mut stale_seq) = fixtures().swap_remove(3);
    intent_mut(&mut stale_seq).event_seq = 3;
    cases.push((
        world,
        stale_seq,
        "event_seq 3 does not match world event envelope 4",
    ));

    for (mut world, event, expected) in cases {
        let before = world.clone();
        let root = world.current_state_root_hash().expect("root");
        let error = world
            .append_event_for_test(WorldEventBody::Domain(event), None)
            .expect_err("natural validation failure");
        let debug = format!("{error:?}");
        assert!(debug.contains(expected), "{debug}");
        assert_unchanged(&world, &before, &root);
    }
}

#[test]
fn completed_transition_requires_the_exact_committed_receipt_without_partial_state() {
    for (corrupt, expected) in [
        ("world-event:4", "not committed for effect intent"),
        ("world-event:999", "must precede transition event"),
    ] {
        let (mut world, mut event) = completed_fixture();
        intent_mut(&mut event).receipt_ref = Some(corrupt.into());
        let before = world.clone();
        let root = world.current_state_root_hash().expect("root");
        let error = world
            .append_event_for_test(WorldEventBody::Domain(event), None)
            .expect_err("receipt witness mismatch");
        let debug = format!("{error:?}");
        assert!(debug.contains(expected), "{debug}");
        assert_unchanged(&world, &before, &root);
    }
}
