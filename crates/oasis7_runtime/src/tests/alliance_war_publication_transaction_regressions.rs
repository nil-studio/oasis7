use super::pos;
use crate::*;
use oasis7_agent_api::world_action::ResourceKind;

fn register(world: &mut World, id: &str) {
    world.submit_action(Action::RegisterAgent {
        agent_id: id.into(),
        pos: pos(0, 0),
    });
    world.step().unwrap();
}

fn capture(world: &World, action: Action, matches: fn(&DomainEvent) -> bool) -> DomainEvent {
    let mut next = world.clone();
    let len = next.journal().events.len();
    next.submit_action(action);
    next.step().unwrap();
    next.journal().events[len..]
        .iter()
        .find_map(|entry| match &entry.body {
            WorldEventBody::Domain(event) if matches(event) => Some(event.clone()),
            _ => None,
        })
        .expect("lifecycle event")
}

fn fixtures() -> Vec<(&'static str, World, DomainEvent)> {
    let mut world = World::new();
    for id in ["a", "b", "c", "d"] {
        register(&mut world, id);
    }
    let formed = capture(
        &world,
        Action::FormAlliance {
            proposer_agent_id: "a".into(),
            alliance_id: "red".into(),
            members: vec!["b".into()],
            charter: "r".into(),
        },
        |e| matches!(e, DomainEvent::AllianceFormed { .. }),
    );
    let formed_base = world.clone();
    world
        .append_event_for_test(WorldEventBody::Domain(formed.clone()), None)
        .unwrap();
    let joined = capture(
        &world,
        Action::JoinAlliance {
            operator_agent_id: "a".into(),
            alliance_id: "red".into(),
            member_agent_id: "c".into(),
        },
        |e| matches!(e, DomainEvent::AllianceJoined { .. }),
    );
    let joined_base = world.clone();
    world
        .append_event_for_test(WorldEventBody::Domain(joined.clone()), None)
        .unwrap();
    let left = capture(
        &world,
        Action::LeaveAlliance {
            operator_agent_id: "a".into(),
            alliance_id: "red".into(),
            member_agent_id: "c".into(),
        },
        |e| matches!(e, DomainEvent::AllianceLeft { .. }),
    );
    let left_base = world.clone();
    world
        .append_event_for_test(WorldEventBody::Domain(left.clone()), None)
        .unwrap();
    let dissolved = capture(
        &world,
        Action::DissolveAlliance {
            operator_agent_id: "a".into(),
            alliance_id: "red".into(),
            reason: "done".into(),
        },
        |e| matches!(e, DomainEvent::AllianceDissolved { .. }),
    );
    let dissolved_base = world.clone();

    let blue = capture(
        &world,
        Action::FormAlliance {
            proposer_agent_id: "c".into(),
            alliance_id: "blue".into(),
            members: vec!["d".into()],
            charter: "b".into(),
        },
        |e| matches!(e, DomainEvent::AllianceFormed { .. }),
    );
    world
        .append_event_for_test(WorldEventBody::Domain(blue), None)
        .unwrap();
    world
        .set_agent_resource_balance("a", ResourceKind::Electricity, 10_000)
        .unwrap();
    world
        .set_agent_resource_balance("a", ResourceKind::Data, 10_000)
        .unwrap();
    let declared = capture(
        &world,
        Action::DeclareWar {
            initiator_agent_id: "a".into(),
            war_id: "war".into(),
            aggressor_alliance_id: "red".into(),
            defender_alliance_id: "blue".into(),
            objective: "hold".into(),
            intensity: 2,
        },
        |e| matches!(e, DomainEvent::WarDeclared { .. }),
    );
    let declared_base = world.clone();
    world
        .append_event_for_test(WorldEventBody::Domain(declared.clone()), None)
        .unwrap();
    let due = world.state().wars["war"].declared_at + world.state().wars["war"].max_duration_ticks;
    let concluded = world.prepared_next_due_war_event_for_test(due).unwrap();
    vec![
        ("formed", formed_base, formed),
        ("joined", joined_base, joined),
        ("left", left_base, left),
        ("dissolved", dissolved_base, dissolved),
        ("declared", declared_base, declared),
        ("concluded", world, concluded),
    ]
}

fn unchanged(world: &World, before: &World, root: &str) {
    assert_eq!(world.snapshot(), before.snapshot());
    assert_eq!(world.journal(), before.journal());
    assert_eq!(
        world.runtime_backpressure_stats(),
        before.runtime_backpressure_stats()
    );
    assert_eq!(
        world.tick_consensus_records(),
        before.tick_consensus_records()
    );
    assert_eq!(world.current_state_root_hash().unwrap(), root);
}

#[test]
fn all_six_raw_events_honor_postprepare_and_retry_replay() {
    let mut bypassed = Vec::new();
    for (index, (name, mut world, event)) in fixtures().into_iter().enumerate() {
        let baseline = world.snapshot();
        let before = world.clone();
        let root = world.current_state_root_hash().unwrap();
        world.fail_next_append_after_publication_prepare_for_test();
        match world.append_event_for_test(
            WorldEventBody::Domain(event.clone()),
            Some(CausedBy::Action(5_400 + index as u64)),
        ) {
            Ok(_) => bypassed.push(name),
            Err(error) => {
                assert!(format!("{error:?}").contains("publication preparation"));
                unchanged(&world, &before, &root);
                let cause = Some(CausedBy::Action(5_410 + index as u64));
                world
                    .append_event_for_test(WorldEventBody::Domain(event), cause.clone())
                    .unwrap();
                assert_eq!(world.journal().events.last().unwrap().caused_by, cause);
                let replay = World::from_snapshot(baseline, world.journal().clone()).unwrap();
                assert_eq!(
                    replay.snapshot(),
                    world.snapshot(),
                    "replay mismatch for {name}"
                );
                assert_eq!(
                    replay.current_state_root_hash().unwrap(),
                    world.current_state_root_hash().unwrap()
                );
            }
        }
    }
    assert!(
        bypassed.is_empty(),
        "events bypassed postprepare failpoint: {bypassed:?}"
    );
}

fn natural(mut world: World, event: DomainEvent, expected: &str) {
    let before = world.clone();
    let root = world.current_state_root_hash().unwrap();
    let error = world
        .append_event_for_test(WorldEventBody::Domain(event), None)
        .unwrap_err();
    assert!(format!("{error:?}").contains(expected), "{error:?}");
    unchanged(&world, &before, &root);
}

#[test]
fn dissolve_operator_failure_does_not_remove_alliance() {
    let (_, world, mut event) = fixtures().remove(3);
    if let DomainEvent::AllianceDissolved {
        operator_agent_id, ..
    } = &mut event
    {
        *operator_agent_id = "c".into();
    }
    natural(world, event, "not a member");
}

#[test]
fn war_second_resource_failure_does_not_debit_first() {
    let (_, world, event) = fixtures().remove(4);
    let mut state = world.state().clone();
    state
        .agents
        .get_mut("a")
        .unwrap()
        .state
        .resources
        .set(ResourceKind::Data, 0)
        .unwrap();
    natural(World::new_with_state(state), event, "data debit failed");
}

#[test]
fn success_routes_only_domain_actor_and_replays_roots() {
    for (_, base, event) in fixtures() {
        let actor = event.agent_id().map(str::to_string);
        let before = base
            .state()
            .agents
            .iter()
            .map(|(id, cell)| (id.clone(), cell.mailbox.len()))
            .collect::<std::collections::BTreeMap<_, _>>();
        let baseline = base.snapshot();
        let mut world = base;
        world
            .append_event_for_test(WorldEventBody::Domain(event), None)
            .unwrap();
        for (id, count) in before {
            assert_eq!(
                world.state().agents[&id].mailbox.len(),
                count + usize::from(actor.as_deref() == Some(id.as_str()))
            );
        }
        let replay = World::from_snapshot(baseline, world.journal().clone()).unwrap();
        assert_eq!(
            replay.snapshot(),
            world.snapshot(),
            "replay mismatch for {actor:?}"
        );
        assert_eq!(
            replay.current_state_root_hash().unwrap(),
            world.current_state_root_hash().unwrap()
        );
    }
}
