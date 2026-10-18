use super::*;
use crate::geometry::GeoPos;
use crate::models::AgentState;
use crate::runtime::{
    Action, AgentActivityV1, AgentCell, FactoryModuleSpec, FactoryProductionState, FactoryState,
    MaterialLedgerId, MaterialStack, SnapshotMeta,
};
use crate::simulator::WorldKernel;
use crate::viewer::runtime_live::support::FORMAL_RELEASE_DEFAULT_BOOTSTRAP_AGENT_ID;
use crate::viewer::runtime_live::{ViewerRuntimeLiveServer, ViewerRuntimeLiveServerConfig};
use oasis7_agent_api::world_action::ResourceKind;
use oasis7_simulator::WorldScenario;

fn serialized_mapped_agent_activity(activity: Option<AgentActivityV1>) -> serde_json::Value {
    let mut state = crate::runtime::WorldState::default();
    let mut cell = AgentCell::new(AgentState::new("agent-activity", GeoPos::new(0, 0, 0)), 0);
    cell.activity = activity;
    state.agents.insert("agent-activity".to_string(), cell);

    let sidecar = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Script);
    let model = runtime_state_to_simulator_model(&state, &sidecar, None);
    serde_json::to_value(model.agents.get("agent-activity").expect("mapped agent"))
        .expect("serialize mapped agent")
}

#[test]
fn runtime_state_to_simulator_model_projects_authoritative_agent_activity() {
    for (activity, expected_status, expected_reason, expected_updated_at) in [
        (
            AgentActivityV1::executing("recipe", 17, "factory-activity", 7),
            "executing",
            None,
            7,
        ),
        (
            AgentActivityV1::blocked(
                "recipe",
                18,
                "factory-activity",
                "power_low",
                "insufficient electricity",
                8,
            ),
            "blocked",
            Some("insufficient electricity"),
            8,
        ),
    ] {
        let agent = serialized_mapped_agent_activity(Some(activity));
        let projected = agent
            .get("activity")
            .expect("mapped Agent must publish activity")
            .as_object()
            .expect("mapped activity object");

        assert_eq!(
            projected.get("status"),
            Some(&serde_json::json!(expected_status))
        );
        assert_eq!(
            projected.get("operation"),
            Some(&serde_json::json!("recipe"))
        );
        assert_eq!(
            projected.get("target"),
            Some(&serde_json::json!("factory-activity"))
        );
        assert_eq!(
            projected.get("reason").and_then(serde_json::Value::as_str),
            expected_reason
        );
        assert_eq!(
            projected.get("updated_at"),
            Some(&serde_json::json!(expected_updated_at))
        );
    }
}

#[test]
fn runtime_state_to_simulator_model_does_not_synthesize_missing_activity() {
    let agent = serialized_mapped_agent_activity(None);
    assert!(
        agent.get("activity").is_none() || agent.get("activity") == Some(&serde_json::Value::Null)
    );
}

#[test]
fn runtime_state_to_simulator_model_preserves_formal_release_seed_fragment_location() {
    let (world, _) = super::super::support::bootstrap_formal_release_runtime_world()
        .expect("formal release runtime world");
    let sidecar = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Script);
    let model = runtime_state_to_simulator_model(world.state(), &sidecar, None);
    let agent = model
        .agents
        .get(FORMAL_RELEASE_DEFAULT_BOOTSTRAP_AGENT_ID)
        .expect("formal release starter agent should be mapped");
    let location = model
        .locations
        .get(&agent.location_id)
        .expect("formal release starter location should be mapped");

    assert!(agent.location_id.starts_with("frag-"));
    assert!(!agent.location_id.starts_with("runtime:"));
    assert_eq!(agent.location_id, location.id);
    assert!(location.fragment_budget.is_some());
}
#[test]
fn runtime_state_to_simulator_model_recomputes_seeded_location_after_agent_moves() {
    let mut world = crate::runtime::World::default();
    let from_pos = GeoPos::new(10, 0, 0);
    let to_pos = GeoPos::new(20, 0, 0);
    world.submit_action(Action::RegisterAgent {
        agent_id: "a1".to_string(),
        pos: from_pos,
    });
    world.step().expect("register runtime agent");
    world.submit_action(Action::MoveAgent {
        agent_id: "a1".to_string(),
        to: to_pos,
    });
    world.step().expect("move runtime agent");

    let mut seed_model = WorldModel::default();
    seed_model.locations.insert(
        "frag-from".to_string(),
        Location::new("frag-from", "from fragment", from_pos),
    );
    seed_model.locations.insert(
        "frag-to".to_string(),
        Location::new("frag-to", "to fragment", to_pos),
    );
    seed_model.agents.insert(
        "a1".to_string(),
        Agent::new("a1", "frag-from".to_string(), from_pos),
    );
    let sidecar =
        RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Script).with_runtime_seed_model(&seed_model);

    let model = runtime_state_to_simulator_model(world.state(), &sidecar, Some(&seed_model));
    let agent = model.agents.get("a1").expect("mapped agent");
    assert_eq!(agent.location_id, "frag-to");
    assert_eq!(agent.pos, to_pos);
}
#[test]
fn runtime_state_to_simulator_model_projects_runtime_factory_for_canonical_schedule_quote() {
    let mut world = crate::runtime::World::default();
    world.submit_action(Action::RegisterAgent {
        agent_id: "builder-a".to_string(),
        pos: GeoPos::new(10, 20, 0),
    });
    world.step().expect("register runtime builder");
    world
        .set_agent_resource_balance("builder-a", ResourceKind::Electricity, 100)
        .expect("seed electricity");
    world
        .set_agent_resource_balance("builder-a", ResourceKind::Data, 100)
        .expect("seed hardware");
    let mut state = world.state().clone();
    state.factories.insert(
        "factory.smelter.alpha".to_string(),
        FactoryState {
            factory_id: "factory.smelter.alpha".to_string(),
            site_id: "site-smelter".to_string(),
            builder_agent_id: "builder-a".to_string(),
            spec: FactoryModuleSpec {
                factory_id: "factory.smelter.mk1".to_string(),
                display_name: "Smelter MK1".to_string(),
                tier: 2,
                tags: vec!["smelter".to_string()],
                build_cost: vec![],
                build_time_ticks: 1,
                base_power_draw: 20,
                recipe_slots: 2,
                throughput_bps: 10_000,
                maintenance_per_tick: 1,
            },
            input_ledger: MaterialLedgerId::site("site-smelter"),
            output_ledger: MaterialLedgerId::site("site-smelter"),
            durability_ppm: 1_000_000,
            production: FactoryProductionState::default(),
            site_authority_revision: None,
            site_location_id: None,
            location_anchor_revision: None,
            construction_power_profile_key: None,
            construction_power_profile_revision: None,
            built_at: 1,
        },
    );
    let world = crate::runtime::World::new_with_state(state);
    let sidecar = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Script);
    let model = runtime_state_to_simulator_model(world.state(), &sidecar, None);
    let factory = model
        .factories
        .get("factory.smelter.alpha")
        .expect("runtime factory is projected");
    let builder = model.agents.get("builder-a").expect("mapped builder");
    assert_eq!(
        factory.owner,
        ResourceOwner::Agent {
            agent_id: "builder-a".to_string()
        }
    );
    assert_eq!(factory.location_id, builder.location_id);
    assert_eq!(factory.kind, "factory.smelter.mk1");
    WorldKernel::with_model(WorldConfig::default(), model)
        .quote_schedule_recipe(
            &ResourceOwner::Agent {
                agent_id: "builder-a".to_string(),
            },
            "factory.smelter.alpha",
            "recipe.smelter.iron_ingot",
            1,
        )
        .expect("projected runtime factory supports the canonical quote");
}
#[test]
fn map_runtime_domain_event_agent_registered_uses_runtime_location_id() {
    let event = RuntimeDomainEvent::AgentRegistered {
        agent_id: "a1".to_string(),
        pos: GeoPos::new(12, 34, 56),
    };
    let mapped =
        map_runtime_domain_event(&event, &WorldConfig::default(), None).expect("mapped event");
    match mapped {
        WorldEventKind::AgentRegistered {
            agent_id,
            location_id,
            pos,
        } => {
            assert_eq!(agent_id, "a1");
            assert_eq!(location_id, "runtime:12:34:56");
            assert_eq!(pos, GeoPos::new(12, 34, 56));
        }
        other => panic!("unexpected mapped event: {other:?}"),
    }
}
#[test]
fn map_runtime_domain_event_agent_moved_sets_distance_and_cost() {
    let config = WorldConfig::default();
    let event = RuntimeDomainEvent::AgentMoved {
        agent_id: "a1".to_string(),
        from: GeoPos::new(0, 0, 0),
        to: GeoPos::new(100_000, 0, 0),
    };
    let mapped = map_runtime_domain_event(&event, &config, None).expect("mapped event");
    match mapped {
        WorldEventKind::AgentMoved {
            distance_cm,
            electricity_cost,
            ..
        } => {
            assert_eq!(distance_cm, 100_000);
            assert_eq!(electricity_cost, config.movement_cost(distance_cm));
        }
        other => panic!("unexpected mapped event: {other:?}"),
    }
}

#[test]
fn map_runtime_domain_event_uses_generated_seed_location_ids() {
    let mut seed_model = WorldModel::default();
    let from_pos = GeoPos::new(10, 0, 0);
    let to_pos = GeoPos::new(20, 0, 0);
    seed_model.locations.insert(
        "frag-from".to_string(),
        Location::new("frag-from", "from fragment", from_pos),
    );
    seed_model.locations.insert(
        "frag-to".to_string(),
        Location::new("frag-to", "to fragment", to_pos),
    );
    seed_model.agents.insert(
        "a1".to_string(),
        Agent::new("a1", "frag-from".to_string(), from_pos),
    );

    let registered = map_runtime_domain_event(
        &RuntimeDomainEvent::AgentRegistered {
            agent_id: "a1".to_string(),
            pos: from_pos,
        },
        &WorldConfig::default(),
        Some(&seed_model),
    )
    .expect("registered event mapped");
    match registered {
        WorldEventKind::AgentRegistered { location_id, .. } => {
            assert_eq!(location_id, "frag-from");
        }
        other => panic!("unexpected registered event: {other:?}"),
    }

    let moved = map_runtime_domain_event(
        &RuntimeDomainEvent::AgentMoved {
            agent_id: "a1".to_string(),
            from: from_pos,
            to: to_pos,
        },
        &WorldConfig::default(),
        Some(&seed_model),
    )
    .expect("moved event mapped");
    match moved {
        WorldEventKind::AgentMoved { from, to, .. } => {
            assert_eq!(from, "frag-from");
            assert_eq!(to, "frag-to");
        }
        other => panic!("unexpected moved event: {other:?}"),
    }
}

#[test]
fn map_runtime_domain_event_action_accepted_emits_structured_runtime_event() {
    let event = RuntimeDomainEvent::ActionAccepted {
        action_id: 7,
        action_kind: "".to_string(),
        actor_id: "".to_string(),
        eta_ticks: 3,
        notes: vec!["raw note must not be exposed".to_string()],
    };
    let mapped =
        map_runtime_domain_event(&event, &WorldConfig::default(), None).expect("mapped event");
    match mapped {
        WorldEventKind::RuntimeEvent { kind, domain_kind } => {
            assert_eq!(kind, "runtime.action_accepted");
            let summary = domain_kind.expect("domain summary");
            assert!(summary.contains("action_id=7"));
            assert!(summary.contains("action_kind=unknown_action"));
            assert!(summary.contains("actor_id=system"));
            assert!(summary.contains("eta_ticks=3"));
            assert!(summary.contains("player_feedback=Action accepted — processing queued."));
            assert!(!summary.contains("raw note must not be exposed"));
        }
        other => panic!("unexpected mapped event: {other:?}"),
    }
}

#[test]
fn map_runtime_domain_event_action_accepted_empty_or_whitespace_notes_use_plain_feedback() {
    for notes in [vec![], vec![" \t\n ".to_string()]] {
        let event = RuntimeDomainEvent::ActionAccepted {
            action_id: 7,
            action_kind: "act".to_string(),
            actor_id: "actor".to_string(),
            eta_ticks: 3,
            notes,
        };
        let mapped =
            map_runtime_domain_event(&event, &WorldConfig::default(), None).expect("mapped event");
        match mapped {
            WorldEventKind::RuntimeEvent { kind, domain_kind } => {
                assert_eq!(kind, "runtime.action_accepted");
                assert_eq!(
                    domain_kind.as_deref(),
                    Some(
                        "action_id=7 action_kind=act actor_id=actor eta_ticks=3 \
                             player_feedback=Action accepted."
                    ),
                );
            }
            other => panic!("unexpected mapped event: {other:?}"),
        }
    }
}

#[test]
fn map_runtime_domain_event_factory_built_emits_structured_runtime_event() {
    let event = RuntimeDomainEvent::FactoryBuilt {
        job_id: 11,
        builder_agent_id: "builder.alpha".to_string(),
        site_id: "site.alpha".to_string(),
        spec: FactoryModuleSpec {
            factory_id: "factory.alpha".to_string(),
            display_name: "Alpha Plant".to_string(),
            tier: 1,
            tags: vec!["assembly".to_string()],
            build_cost: vec![MaterialStack::new("steel_plate", 10)],
            build_time_ticks: 4,
            base_power_draw: 8,
            recipe_slots: 1,
            throughput_bps: 10_000,
            maintenance_per_tick: 1,
        },
    };
    let mapped =
        map_runtime_domain_event(&event, &WorldConfig::default(), None).expect("mapped event");
    match mapped {
        WorldEventKind::RuntimeEvent { kind, domain_kind } => {
            assert_eq!(kind, "runtime.economy.factory_built");
            let summary = domain_kind.expect("domain summary");
            assert!(summary.contains("factory=factory.alpha"));
            assert!(summary.contains("builder=builder.alpha"));
            assert!(summary.contains("site=site.alpha"));
        }
        other => panic!("unexpected mapped event: {other:?}"),
    }
}

#[test]
fn map_runtime_domain_event_recipe_started_and_completed_emit_structured_runtime_events() {
    let started = RuntimeDomainEvent::RecipeStarted {
        committed_recipe_origin: None,
        job_id: 21,
        requester_agent_id: "agent.alpha".to_string(),
        factory_id: "factory.alpha".to_string(),
        recipe_id: "recipe.motor".to_string(),
        accepted_batches: 2,
        consume: vec![MaterialStack::new("iron_ingot", 4)],
        produce: vec![MaterialStack::new("motor_mk1", 2)],
        byproducts: Vec::new(),
        power_required: 12,
        power_owner_agent_id: Some("agent.alpha".to_string()),
        duration_ticks: 3,
        consume_ledger: MaterialLedgerId::world(),
        output_ledger: MaterialLedgerId::world(),
        bottleneck_tags: Vec::new(),
        market_quotes: Vec::new(),
        logistics_route_ids: Vec::new(),
        logistics_path_ids: Vec::new(),
        ready_at: 99,
    };
    let completed = RuntimeDomainEvent::RecipeCompleted {
        committed_recipe_origin: None,
        job_id: 21,
        requester_agent_id: "agent.alpha".to_string(),
        factory_id: "factory.alpha".to_string(),
        recipe_id: "recipe.motor".to_string(),
        accepted_batches: 2,
        produce: vec![MaterialStack::new("motor_mk1", 2)],
        byproducts: Vec::new(),
        output_ledger: MaterialLedgerId::world(),
        bottleneck_tags: Vec::new(),
        logistics_route_ids: Vec::new(),
        logistics_path_ids: Vec::new(),
    };

    for (event, expected_kind) in [
        (started, "runtime.economy.recipe_started"),
        (completed, "runtime.economy.recipe_completed"),
    ] {
        let mapped =
            map_runtime_domain_event(&event, &WorldConfig::default(), None).expect("mapped event");
        match mapped {
            WorldEventKind::RuntimeEvent { kind, domain_kind } => {
                assert_eq!(kind, expected_kind);
                let summary = domain_kind.expect("domain summary");
                assert!(summary.contains("factory=factory.alpha"));
                assert!(summary.contains("recipe=recipe.motor"));
                assert!(summary.contains("requester=agent.alpha"));
                assert!(summary.contains("outputs=motor_mk1x2"));
            }
            other => panic!("unexpected mapped event: {other:?}"),
        }
    }
}

#[test]
fn map_runtime_domain_event_factory_blocked_and_resumed_emit_structured_runtime_events() {
    let blocked = RuntimeDomainEvent::FactoryProductionBlocked {
        action_id: 31,
        requester_agent_id: "agent.alpha".to_string(),
        factory_id: "factory.alpha".to_string(),
        recipe_id: "recipe.motor".to_string(),
        blocker_kind: "material_shortage".to_string(),
        blocker_detail: "material_shortage:iron_ingot".to_string(),
    };
    let resumed = RuntimeDomainEvent::FactoryProductionResumed {
        job_id: 32,
        requester_agent_id: "agent.alpha".to_string(),
        factory_id: "factory.alpha".to_string(),
        recipe_id: "recipe.motor".to_string(),
        previous_blocked_at: Some(88),
        previous_blocker_kind: Some("material_shortage".to_string()),
        previous_blocker_detail: Some("material_shortage:iron_ingot".to_string()),
    };

    for (event, expected_kind, expected_fragment) in [
        (
            blocked,
            "runtime.economy.factory_production_blocked",
            "reason=material_shortage",
        ),
        (
            resumed,
            "runtime.economy.factory_production_resumed",
            "previous_reason=material_shortage",
        ),
    ] {
        let mapped =
            map_runtime_domain_event(&event, &WorldConfig::default(), None).expect("mapped event");
        match mapped {
            WorldEventKind::RuntimeEvent { kind, domain_kind } => {
                assert_eq!(kind, expected_kind);
                let summary = domain_kind.expect("domain summary");
                assert!(summary.contains("factory=factory.alpha"));
                assert!(summary.contains("recipe=recipe.motor"));
                assert!(summary.contains(expected_fragment));
            }
            other => panic!("unexpected mapped event: {other:?}"),
        }
    }
}

#[test]
fn map_runtime_domain_event_governance_finalize_keeps_compat_fallbacks() {
    let event = RuntimeDomainEvent::GovernanceProposalFinalized {
        proposal_key: "proposal.alpha".to_string(),
        winning_option: None,
        winning_weight: 0,
        total_weight: 0,
        passed: false,
    };
    let mapped =
        map_runtime_domain_event(&event, &WorldConfig::default(), None).expect("mapped event");
    match mapped {
        WorldEventKind::RuntimeEvent { kind, domain_kind } => {
            assert_eq!(kind, "runtime.gameplay.governance_proposal_finalized");
            let summary = domain_kind.expect("domain summary");
            assert!(summary.contains("proposal_key=proposal.alpha"));
            assert!(summary.contains("winning_option=none"));
            assert!(summary.contains("passed=false"));
        }
        other => panic!("unexpected mapped event: {other:?}"),
    }
}

#[test]
fn runtime_reject_reason_maps_agent_not_found() {
    let reason = RuntimeRejectReason::AgentNotFound {
        agent_id: "ghost".to_string(),
    };
    let mapped = runtime_reject_reason_to_simulator(&reason);
    match mapped {
        SimulatorRejectReason::AgentNotFound { agent_id } => {
            assert_eq!(agent_id, "ghost");
        }
        other => panic!("unexpected reject mapping: {other:?}"),
    }
}

#[test]
fn runtime_reject_reason_unmapped_falls_back_to_rule_denied() {
    let reason = RuntimeRejectReason::InsufficientMaterial {
        material_kind: "iron".to_string(),
        requested: 10,
        available: 0,
    };
    let mapped = runtime_reject_reason_to_simulator(&reason);
    match mapped {
        SimulatorRejectReason::RuleDenied { notes } => {
            assert_eq!(notes.len(), 1);
            assert!(notes[0].contains("runtime reject"));
        }
        other => panic!("unexpected reject mapping: {other:?}"),
    }
}

#[test]
fn map_runtime_event_fallback_includes_runtime_payload() {
    let event = RuntimeWorldEvent {
        id: 9,
        time: 42,
        caused_by: None,
        body: RuntimeWorldEventBody::SnapshotCreated(SnapshotMeta { journal_len: 1 }),
    };
    let mapped = map_runtime_event(&event, &WorldConfig::default(), None);
    assert!(matches!(mapped.kind, WorldEventKind::RuntimeEvent { .. }));
    assert!(mapped.runtime_event.is_some());
    assert_eq!(mapped.id, 9);
    assert_eq!(mapped.time, 42);
}

#[test]
fn map_runtime_event_unmapped_domain_event_keeps_payload_with_domain_label() {
    let event = RuntimeWorldEvent {
        id: 10,
        time: 43,
        caused_by: None,
        body: RuntimeWorldEventBody::Domain(RuntimeDomainEvent::DataCollected {
            collector_agent_id: "agent.alpha".to_string(),
            electricity_cost: 2,
            data_amount: 5,
        }),
    };
    let mapped = map_runtime_event(&event, &WorldConfig::default(), None);
    match mapped.kind {
        WorldEventKind::RuntimeEvent { kind, domain_kind } => {
            assert_eq!(kind, "domain");
            assert_eq!(domain_kind, None);
        }
        other => panic!("unexpected mapped event: {other:?}"),
    }
    assert_eq!(mapped.runtime_event, Some(event));
}

#[test]
fn fallback_non_empty_trims_values_and_labels_empty_summaries() {
    assert_eq!(
        fallback_non_empty("  agent.alpha  ", "system"),
        "agent.alpha"
    );
    assert_eq!(fallback_non_empty("  ", "unknown_action"), "unknown_action");
}

#[test]
fn runtime_live_snapshot_includes_runtime_snapshot_payload() {
    let mut server =
        ViewerRuntimeLiveServer::new(ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal))
            .expect("runtime server");
    let snapshot = server.compat_snapshot(None);
    assert!(snapshot.runtime_snapshot.is_some());
    assert_eq!(
        snapshot.runtime_snapshot.as_ref().unwrap().journal_len,
        server.world.snapshot().journal_len
    );
}
