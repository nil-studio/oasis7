use super::pos;
use oasis7_agent_api::world_action::BodyKernelView;
use crate::agent_state::{BodySlotType, CargoEntityEntry, CargoEntityKind};
use crate::events::Observation;
use crate::*;

fn register(world: &mut World, id: &str) {
    world.submit_action(Action::RegisterAgent {
        agent_id: id.into(),
        pos: pos(0, 0),
    });
    world.step().unwrap();
}

fn profile() -> MaterialProfileV1 {
    MaterialProfileV1 {
        kind: "wire".into(),
        tier: 2,
        category: "component".into(),
        stack_limit: 100,
        transport_loss_class: MaterialTransportLossClass::Low,
        decay_bps_per_tick: 1,
        default_priority: MaterialDefaultPriority::Standard,
    }
}

fn base() -> World {
    let mut world = World::new();
    register(&mut world, "a");
    world
}

fn fixtures() -> Vec<(&'static str, World, DomainEvent)> {
    let world = base();
    let mut expand_world = world.clone();
    expand_world
        .add_agent_cargo_entity(
            "a",
            CargoEntityEntry {
                entity_id: "kit".into(),
                entity_kind: CargoEntityKind::InterfaceModuleItem,
                quantity: 1,
                size_per_unit: 1,
            },
        )
        .unwrap();
    vec![
        (
            "registered",
            World::new(),
            DomainEvent::AgentRegistered {
                agent_id: "new".into(),
                pos: pos(1, 2),
            },
        ),
        (
            "moved",
            world.clone(),
            DomainEvent::AgentMoved {
                agent_id: "a".into(),
                from: pos(0, 0),
                to: pos(3, 4),
            },
        ),
        (
            "accepted",
            world.clone(),
            DomainEvent::ActionAccepted {
                action_id: 10,
                action_kind: "move".into(),
                actor_id: "a".into(),
                eta_ticks: 1,
                notes: vec!["ok".into()],
            },
        ),
        (
            "rejected",
            world.clone(),
            DomainEvent::ActionRejected {
                action_id: 11,
                reason: RejectReason::InvalidAmount { amount: -1 },
            },
        ),
        (
            "observation",
            world.clone(),
            DomainEvent::Observation {
                observation: Observation {
                    time: world.state().time,
                    agent_id: "a".into(),
                    pos: pos(0, 0),
                    visibility_range_cm: 10,
                    visible_agents: vec![],
                },
            },
        ),
        (
            "body updated",
            world.clone(),
            DomainEvent::BodyAttributesUpdated {
                agent_id: "a".into(),
                view: BodyKernelView {
                    mass_kg: 100,
                    radius_cm: 50,
                    thrust_limit: 10,
                    cross_section_cm2: 20,
                },
                reason: "upgrade".into(),
            },
        ),
        (
            "body rejected",
            world.clone(),
            DomainEvent::BodyAttributesRejected {
                agent_id: "a".into(),
                reason: "invalid".into(),
            },
        ),
        (
            "interface expanded",
            expand_world,
            DomainEvent::BodyInterfaceExpanded {
                agent_id: "a".into(),
                slot_capacity: 8,
                expansion_level: 1,
                consumed_item_id: "kit".into(),
                new_slot_id: "slot-8".into(),
                slot_type: BodySlotType::Universal,
            },
        ),
        (
            "interface rejected",
            world.clone(),
            DomainEvent::BodyInterfaceExpandRejected {
                agent_id: "a".into(),
                consumed_item_id: "missing".into(),
                reason: "missing".into(),
            },
        ),
        (
            "policy",
            world.clone(),
            DomainEvent::GameplayPolicyUpdated {
                operator_agent_id: "a".into(),
                electricity_tax_bps: 10,
                data_tax_bps: 20,
                power_trade_fee_bps: 30,
                max_open_contracts_per_agent: 4,
                blocked_agents: vec![" b ".into(), "b".into(), "".into()],
                forbidden_location_ids: vec![" z ".into(), "z".into()],
            },
        ),
        (
            "material profile",
            world,
            DomainEvent::MaterialProfileGoverned {
                operator_agent_id: "a".into(),
                proposal_id: 7,
                profile: profile(),
            },
        ),
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
fn all_eleven_raw_events_use_typed_postprepare_and_retry_replay() {
    let mut bypassed = Vec::new();
    for (index, (name, mut world, event)) in fixtures().into_iter().enumerate() {
        let snapshot = world.snapshot();
        let before = world.clone();
        let root = world.current_state_root_hash().unwrap();
        world.fail_next_append_after_publication_prepare_for_test();
        match world.append_event_for_test(
            WorldEventBody::Domain(event.clone()),
            Some(CausedBy::Action(8_000 + index as u64)),
        ) {
            Ok(_) => bypassed.push(name),
            Err(error) => {
                assert!(format!("{error:?}").contains("publication preparation"));
                unchanged(&world, &before, &root);
                let cause = Some(CausedBy::Action(8_100 + index as u64));
                world
                    .append_event_for_test(WorldEventBody::Domain(event), cause.clone())
                    .unwrap();
                assert_eq!(world.journal().events.last().unwrap().caused_by, cause);
                assert_eq!(
                    world
                        .tick_consensus_records()
                        .last()
                        .unwrap()
                        .block
                        .header
                        .state_root,
                    world.current_state_root_hash().unwrap(),
                    "published root mismatch: {name}"
                );
                let replay = World::from_snapshot(snapshot, world.journal().clone()).unwrap();
                assert_eq!(
                    replay.snapshot(),
                    world.snapshot(),
                    "replay mismatch: {name}"
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
        "events bypassed typed preparation: {bypassed:?}"
    );
}

#[test]
fn compatibility_pins_state_and_exact_actor_or_no_actor_routing() {
    for (name, mut world, event) in fixtures() {
        let actor = event.agent_id().map(str::to_owned);
        let counts = world
            .state()
            .agents
            .iter()
            .map(|(id, cell)| (id.clone(), cell.mailbox.len()))
            .collect::<std::collections::BTreeMap<_, _>>();
        world
            .append_event_for_test(WorldEventBody::Domain(event), None)
            .unwrap();
        for (id, count) in counts {
            assert_eq!(
                world.state().agents[&id].mailbox.len(),
                count + usize::from(actor.as_deref() == Some(id.as_str())),
                "routing mismatch: {name}/{id}"
            );
        }
        match name {
            "registered" => assert_eq!(world.state().agents["new"].mailbox.len(), 1),
            "moved" => assert_eq!(world.state().agents["a"].state.pos, pos(3, 4)),
            "body updated" => assert_eq!(world.state().agents["a"].state.body_view.mass_kg, 100),
            "interface expanded" => {
                assert_eq!(
                    world.state().agents["a"].state.body_state.expansion_level,
                    1
                )
            }
            "policy" => {
                assert_eq!(world.state().gameplay_policy.blocked_agents, vec!["b"]);
                assert_eq!(
                    world.state().gameplay_policy.forbidden_location_ids,
                    vec!["z"]
                );
            }
            "material profile" => assert_eq!(world.state().material_profiles["wire"], profile()),
            _ => {}
        }
    }

    let mut missing = base();
    let counts = missing.state().agents["a"].mailbox.len();
    missing
        .append_event_for_test(
            WorldEventBody::Domain(DomainEvent::AgentMoved {
                agent_id: "missing".into(),
                from: pos(0, 0),
                to: pos(9, 9),
            }),
            None,
        )
        .unwrap();
    assert_eq!(missing.state().agents["a"].mailbox.len(), counts);
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
fn natural_failures_preserve_priority_and_all_state() {
    natural(
        base(),
        DomainEvent::BodyAttributesRejected {
            agent_id: "missing".into(),
            reason: "x".into(),
        },
        "AgentNotFound",
    );
    natural(
        base(),
        DomainEvent::BodyInterfaceExpanded {
            agent_id: "a".into(),
            slot_capacity: 8,
            expansion_level: 1,
            consumed_item_id: "missing".into(),
            new_slot_id: "slot".into(),
            slot_type: BodySlotType::Universal,
        },
        "consume interface module item failed",
    );
    natural(
        base(),
        DomainEvent::GameplayPolicyUpdated {
            operator_agent_id: "missing".into(),
            electricity_tax_bps: 0,
            data_tax_bps: 0,
            power_trade_fee_bps: 0,
            max_open_contracts_per_agent: 0,
            blocked_agents: vec![],
            forbidden_location_ids: vec![],
        },
        "AgentNotFound",
    );
    let mut invalid = profile();
    invalid.kind.clear();
    natural(
        base(),
        DomainEvent::MaterialProfileGoverned {
            operator_agent_id: "missing".into(),
            proposal_id: 0,
            profile: invalid,
        },
        "AgentNotFound",
    );
    natural(
        base(),
        DomainEvent::MaterialProfileGoverned {
            operator_agent_id: "a".into(),
            proposal_id: 0,
            profile: profile(),
        },
        "proposal_id",
    );
}

#[test]
fn duplicate_and_missing_route_only_events_preserve_legacy_noop_semantics() {
    let mut duplicate = base();
    let registered = DomainEvent::AgentRegistered {
        agent_id: "a".into(),
        pos: pos(8, 9),
    };
    duplicate
        .append_event_for_test(WorldEventBody::Domain(registered.clone()), None)
        .unwrap();
    duplicate
        .append_event_for_test(WorldEventBody::Domain(registered), None)
        .unwrap();
    assert_eq!(duplicate.state().agents["a"].state.pos, pos(8, 9));
    assert_eq!(duplicate.state().agents["a"].mailbox.len(), 1);

    for event in [
        DomainEvent::ActionAccepted {
            action_id: 70,
            action_kind: "move".into(),
            actor_id: "missing".into(),
            eta_ticks: 1,
            notes: vec![],
        },
        DomainEvent::Observation {
            observation: Observation {
                time: duplicate.state().time,
                agent_id: "missing".into(),
                pos: pos(0, 0),
                visibility_range_cm: 1,
                visible_agents: vec![],
            },
        },
        DomainEvent::AgentMoved {
            agent_id: "missing".into(),
            from: pos(0, 0),
            to: pos(1, 1),
        },
    ] {
        let before = duplicate.state().clone();
        duplicate
            .append_event_for_test(WorldEventBody::Domain(event), None)
            .unwrap();
        assert_eq!(duplicate.state(), &before);
        assert_eq!(
            duplicate
                .tick_consensus_records()
                .last()
                .unwrap()
                .block
                .header
                .state_root,
            duplicate.current_state_root_hash().unwrap()
        );
    }

    let before = duplicate.state().clone();
    let rejected = DomainEvent::ActionRejected {
        action_id: 71,
        reason: RejectReason::InvalidAmount { amount: -1 },
    };
    duplicate
        .append_event_for_test(WorldEventBody::Domain(rejected.clone()), None)
        .unwrap();
    duplicate
        .append_event_for_test(WorldEventBody::Domain(rejected), None)
        .unwrap();
    assert_eq!(duplicate.state(), &before);
}
