use super::pos;
use crate::state::LogisticsPathAuthorityV1;
use crate::*;
use oasis7_agent_api::world_action::ResourceKind;
use oasis7_wasm_abi::{FactoryModuleSpec, MaterialStack};

fn register(world: &mut World, id: &str) {
    world.submit_action(Action::RegisterAgent {
        agent_id: id.into(),
        pos: pos(0, 0),
    });
    world.step().unwrap();
}

fn append(world: &mut World, event: DomainEvent) {
    world
        .append_event_for_test(WorldEventBody::Domain(event), None)
        .unwrap();
}

fn with_time(world: &World, time: u64) -> World {
    let mut state = world.state().clone();
    state.time = time;
    World::new_with_state(state)
}

fn spec() -> FactoryModuleSpec {
    FactoryModuleSpec {
        factory_id: "factory".into(),
        display_name: "Factory".into(),
        tier: 1,
        tags: vec!["assembly".into()],
        build_cost: vec![MaterialStack::new("steel", 2)],
        build_time_ticks: 1,
        base_power_draw: 1,
        recipe_slots: 1,
        throughput_bps: 10_000,
        maintenance_per_tick: 1,
    }
}

fn built_world() -> World {
    let mut world = World::new();
    register(&mut world, "actor");
    world.set_material_balance("steel", 10).unwrap();
    let ready_at = world.state().time + 1;
    append(
        &mut world,
        DomainEvent::FactoryBuildStarted {
            job_id: 101,
            builder_agent_id: "actor".into(),
            site_id: "site".into(),
            spec: spec(),
            consume_ledger: MaterialLedgerId::world(),
            ready_at,
            contract_version: Some(0),
            site_authority_revision: None,
            site_location_id: None,
            location_anchor_revision: None,
            construction_power_obligation: None,
        },
    );
    let mut world = with_time(&world, world.state().time + 1);
    append(
        &mut world,
        DomainEvent::FactoryBuilt {
            job_id: 101,
            builder_agent_id: "actor".into(),
            site_id: "site".into(),
            spec: spec(),
        },
    );
    world
}

fn fixtures() -> Vec<(&'static str, World, DomainEvent)> {
    let mut base = World::new();
    register(&mut base, "actor");
    register(&mut base, "owner");
    base.set_agent_resource_balance("actor", ResourceKind::Electricity, 100)
        .unwrap();
    base.set_ledger_material_balance(MaterialLedgerId::site("from"), "steel", 20)
        .unwrap();

    let route = DomainEvent::LogisticsRouteRegistered {
        requester_agent_id: "actor".into(),
        route_id: Some("route".into()),
        from_ledger: MaterialLedgerId::site("from"),
        to_ledger: MaterialLedgerId::site("to"),
        kind: "steel".into(),
        distance_km: 1,
        priority: MaterialTransitPriority::Standard,
        owner_agent_id: "owner".into(),
        available: true,
        capacity_units: 20,
        tariff_electricity_per_unit: 0,
    };
    let mut routed = base.clone();
    append(&mut routed, route.clone());
    let availability = DomainEvent::LogisticsRouteAvailabilityChanged {
        requester_agent_id: "owner".into(),
        route_id: "route".into(),
        available: false,
        owner_agent_id: "owner".into(),
    };
    let reroute = DomainEvent::LogisticsPathRerouted {
        requester_agent_id: "actor".into(),
        job_id: 12,
        original_path_id: Some("old".into()),
        original_route_ids: vec!["route".into()],
        effective_path_id: Some("new".into()),
        effective_route_ids: vec![],
        reason: "capacity".into(),
        reroute_count: 1,
        tariff_electricity_total: 0,
        owner_payouts: vec![],
        governance_tax_electricity: 0,
    };
    let transfer = DomainEvent::MaterialTransferred {
        transfer_id: Some(13),
        requester_agent_id: "actor".into(),
        from_ledger: MaterialLedgerId::site("from"),
        to_ledger: MaterialLedgerId::site("to"),
        kind: "steel".into(),
        amount: 2,
        distance_km: 0,
        priority: MaterialTransitPriority::Standard,
        route_id: None,
    };
    let transit = DomainEvent::MaterialTransitStarted {
        job_id: 14,
        requester_agent_id: "actor".into(),
        from_ledger: MaterialLedgerId::site("from"),
        to_ledger: MaterialLedgerId::site("to"),
        kind: "steel".into(),
        amount: 2,
        distance_km: 1,
        loss_bps: 0,
        ready_at: base.state().time + 1,
        priority: MaterialTransitPriority::Standard,
        route_id: None,
        path_id: None,
        route_ids: vec![],
        tariff_electricity_total: 0,
        reroute_count: 0,
    };
    let mut transit_base = base.clone();
    append(&mut transit_base, transit.clone());
    transit_base = with_time(
        &transit_base,
        transit_base.state().pending_material_transits[&14].ready_at,
    );
    let transit_done = DomainEvent::MaterialTransitCompleted {
        job_id: 14,
        requester_agent_id: "actor".into(),
        from_ledger: MaterialLedgerId::site("from"),
        to_ledger: MaterialLedgerId::site("to"),
        kind: "steel".into(),
        sent_amount: 2,
        received_amount: 2,
        loss_amount: 0,
        distance_km: 1,
        priority: MaterialTransitPriority::Standard,
        route_id: None,
        path_id: None,
        route_ids: vec![],
        tariff_electricity_total: 0,
        reroute_count: 0,
    };

    let mut build_base = World::new();
    register(&mut build_base, "actor");
    build_base.set_material_balance("steel", 10).unwrap();
    let build = DomainEvent::FactoryBuildStarted {
        job_id: 101,
        builder_agent_id: "actor".into(),
        site_id: "site".into(),
        spec: spec(),
        consume_ledger: MaterialLedgerId::world(),
        ready_at: build_base.state().time + 1,
        contract_version: Some(0),
        site_authority_revision: None,
        site_location_id: None,
        location_anchor_revision: None,
        construction_power_obligation: None,
    };
    let mut built_base = build_base.clone();
    append(&mut built_base, build.clone());
    built_base = with_time(&built_base, built_base.state().time + 1);
    let built = DomainEvent::FactoryBuilt {
        job_id: 101,
        builder_agent_id: "actor".into(),
        site_id: "site".into(),
        spec: spec(),
    };
    let factory = built_world();
    let durability = DomainEvent::FactoryDurabilityChanged {
        factory_id: "factory".into(),
        previous_durability_ppm: 1_000_000,
        durability_ppm: 900_000,
        reason: "wear".into(),
    };
    let mut maintained_base = factory.clone();
    maintained_base
        .set_ledger_material_balance(MaterialLedgerId::world(), "hardware_part", 2)
        .unwrap();
    let maintained = DomainEvent::FactoryMaintained {
        operator_agent_id: "actor".into(),
        factory_id: "factory".into(),
        consume_ledger: MaterialLedgerId::world(),
        consumed_parts: 1,
        durability_ppm: 1_000_000,
    };
    let recycled = DomainEvent::FactoryRecycled {
        operator_agent_id: "actor".into(),
        factory_id: "factory".into(),
        recycle_ledger: MaterialLedgerId::world(),
        recovered: vec![MaterialStack::new("steel", 1)],
        durability_ppm: 900_000,
    };
    let mut recipe_base = factory.clone();
    recipe_base.set_material_balance("ore", 4).unwrap();
    recipe_base
        .set_agent_resource_balance("actor", ResourceKind::Electricity, 10)
        .unwrap();
    let recipe = DomainEvent::RecipeStarted {
        committed_recipe_origin: None,
        job_id: 201,
        requester_agent_id: "actor".into(),
        factory_id: "factory".into(),
        recipe_id: "smelt".into(),
        accepted_batches: 1,
        consume: vec![MaterialStack::new("ore", 1)],
        produce: vec![MaterialStack::new("ingot", 1)],
        byproducts: vec![],
        power_required: 1,
        power_owner_agent_id: Some("actor".into()),
        duration_ticks: 1,
        consume_ledger: MaterialLedgerId::world(),
        output_ledger: MaterialLedgerId::world(),
        bottleneck_tags: vec![],
        market_quotes: vec![],
        logistics_route_ids: vec![],
        logistics_path_ids: vec![],
        ready_at: recipe_base.state().time + 1,
    };
    let mut recipe_done_base = recipe_base.clone();
    append(&mut recipe_done_base, recipe.clone());
    recipe_done_base = with_time(&recipe_done_base, recipe_done_base.state().time + 1);
    let recipe_done = DomainEvent::RecipeCompleted {
        committed_recipe_origin: None,
        job_id: 201,
        requester_agent_id: "actor".into(),
        factory_id: "factory".into(),
        recipe_id: "smelt".into(),
        accepted_batches: 1,
        produce: vec![MaterialStack::new("ingot", 1)],
        byproducts: vec![],
        output_ledger: MaterialLedgerId::world(),
        bottleneck_tags: vec![],
        logistics_route_ids: vec![],
        logistics_path_ids: vec![],
    };
    let blocked = DomainEvent::FactoryProductionBlocked {
        action_id: 301,
        requester_agent_id: "actor".into(),
        factory_id: "factory".into(),
        recipe_id: "smelt".into(),
        blocker_kind: "capacity".into(),
        blocker_detail: "busy".into(),
    };
    let resumed = DomainEvent::FactoryProductionResumed {
        job_id: 301,
        requester_agent_id: "actor".into(),
        factory_id: "factory".into(),
        recipe_id: "smelt".into(),
        previous_blocked_at: None,
        previous_blocker_kind: Some("capacity".into()),
        previous_blocker_detail: Some("busy".into()),
    };
    let paused = DomainEvent::FactoryProductionPaused {
        action_id: 302,
        requester_agent_id: "actor".into(),
        factory_id: "factory".into(),
        reason: "operator".into(),
    };

    vec![
        ("route registered", base.clone(), route),
        ("route availability", routed, availability),
        ("path rerouted", base.clone(), reroute),
        ("material transferred", base.clone(), transfer),
        ("transit started", base, transit),
        ("transit completed", transit_base, transit_done),
        ("build started", build_base, build),
        ("built", built_base, built),
        ("durability", factory.clone(), durability),
        ("maintained", maintained_base, maintained),
        ("recycled", factory.clone(), recycled),
        ("recipe started", recipe_base, recipe),
        ("recipe completed", recipe_done_base, recipe_done),
        ("production blocked", factory.clone(), blocked),
        ("production resumed", factory.clone(), resumed),
        ("production paused", factory, paused),
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

fn assert_atomic_group(range: std::ops::Range<usize>) {
    let mut bypassed = Vec::new();
    for (index, (name, mut world, event)) in fixtures()
        .into_iter()
        .enumerate()
        .filter(|(index, _)| range.contains(index))
    {
        let baseline = world.snapshot();
        let before = world.clone();
        let root = world.current_state_root_hash().unwrap();
        world.fail_next_append_after_publication_prepare_for_test();
        match world.append_event_for_test(
            WorldEventBody::Domain(event.clone()),
            Some(CausedBy::Action(7_000 + index as u64)),
        ) {
            Ok(_) => bypassed.push(name),
            Err(error) => {
                assert!(format!("{error:?}").contains("publication preparation"));
                unchanged(&world, &before, &root);
                let cause = Some(CausedBy::Action(7_100 + index as u64));
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
                    "published root mismatch for {name}"
                );
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
        "events bypassed failpoint: {bypassed:?}"
    );
}

#[test]
fn logistics_topology_events_honor_postprepare_and_same_world_retry() {
    assert_atomic_group(0..3);
}

#[test]
fn direct_transfer_honors_postprepare_and_same_world_retry() {
    assert_atomic_group(3..4);
}

#[test]
fn transit_events_honor_postprepare_and_same_world_retry() {
    assert_atomic_group(4..6);
}

#[test]
fn factory_build_events_honor_postprepare_and_same_world_retry() {
    assert_atomic_group(6..8);
}

#[test]
fn factory_lifecycle_events_honor_postprepare_and_same_world_retry() {
    assert_atomic_group(8..11);
}

#[test]
fn recipe_and_production_events_honor_postprepare_and_same_world_retry() {
    assert_atomic_group(11..16);
}

#[test]
fn successful_industry_events_route_only_the_declared_actor() {
    for (name, mut world, event) in fixtures() {
        let actor = event.agent_id().map(str::to_owned);
        let before = world
            .state()
            .agents
            .iter()
            .map(|(id, cell)| (id.clone(), cell.mailbox.len()))
            .collect::<std::collections::BTreeMap<_, _>>();
        world
            .append_event_for_test(WorldEventBody::Domain(event), None)
            .unwrap();
        for (id, count) in before {
            assert_eq!(
                world.state().agents[&id].mailbox.len(),
                count + usize::from(actor.as_deref() == Some(id.as_str())),
                "routing mismatch for {name}/{id}"
            );
        }
    }
}

#[test]
fn transfer_recipient_overflow_does_not_leak_sender_debit_or_ledger_migration() {
    let (_, mut world, mut event) = fixtures().remove(3);
    world
        .set_ledger_material_balance(MaterialLedgerId::site("to"), "steel", i64::MAX)
        .unwrap();
    if let DomainEvent::MaterialTransferred { amount, .. } = &mut event {
        *amount = 1;
    }
    let before = world.clone();
    let root = world.current_state_root_hash().unwrap();
    let error = world
        .append_event_for_test(WorldEventBody::Domain(event), None)
        .unwrap_err();
    assert!(format!("{error:?}").contains("preflight"));
    unchanged(&world, &before, &root);
}

#[test]
fn duplicate_transfer_is_state_idempotent_but_routes_once_and_publishes_live_root() {
    let (_, mut world, event) = fixtures().remove(3);
    world
        .set_ledger_material_balance(MaterialLedgerId::site("unrelated"), "copper", 77)
        .unwrap();
    let baseline = world.snapshot();
    world
        .append_event_for_test(WorldEventBody::Domain(event.clone()), None)
        .unwrap();
    let source = world.ledger_material_balance(&MaterialLedgerId::site("from"), "steel");
    let target = world.ledger_material_balance(&MaterialLedgerId::site("to"), "steel");
    let mailbox = world.state().agents["actor"].mailbox.len();
    let receipts = world.state().direct_material_transfer_receipts.clone();
    world
        .append_event_for_test(WorldEventBody::Domain(event), None)
        .unwrap();
    assert_eq!(
        world.ledger_material_balance(&MaterialLedgerId::site("from"), "steel"),
        source
    );
    assert_eq!(
        world.ledger_material_balance(&MaterialLedgerId::site("to"), "steel"),
        target
    );
    assert_eq!(
        world.ledger_material_balance(&MaterialLedgerId::site("unrelated"), "copper"),
        77
    );
    assert_eq!(world.state().direct_material_transfer_receipts, receipts);
    assert_eq!(world.state().agents["actor"].mailbox.len(), mailbox + 1);
    assert_eq!(
        world
            .tick_consensus_records()
            .last()
            .unwrap()
            .block
            .header
            .state_root,
        world.current_state_root_hash().unwrap()
    );
    let replay = World::from_snapshot(baseline, world.journal().clone()).unwrap();
    assert_eq!(replay.snapshot(), world.snapshot());
    assert_eq!(
        replay.current_state_root_hash().unwrap(),
        world.current_state_root_hash().unwrap()
    );
}

#[test]
fn settled_transit_is_state_idempotent_but_routes_and_publishes_live_root() {
    let fixtures = fixtures();
    let (_, mut world, completed) = fixtures[5].clone();
    let baseline = world.snapshot();
    world
        .append_event_for_test(WorldEventBody::Domain(completed.clone()), None)
        .unwrap();
    let state = world.state().clone();
    let mailbox = state.agents["actor"].mailbox.len();
    world
        .append_event_for_test(WorldEventBody::Domain(completed), None)
        .unwrap();
    assert_eq!(
        world.state().pending_material_transits,
        state.pending_material_transits
    );
    assert_eq!(
        world.state().settled_logistics_transit_ids,
        state.settled_logistics_transit_ids
    );
    assert_eq!(
        world.state().logistics_settlement_receipts,
        state.logistics_settlement_receipts
    );
    assert_eq!(world.state().material_ledgers, state.material_ledgers);
    assert_eq!(
        world.state().agents["actor"].last_active,
        state.agents["actor"].last_active
    );
    assert_eq!(world.state().agents["actor"].mailbox.len(), mailbox + 1);
    assert_eq!(
        world
            .tick_consensus_records()
            .last()
            .unwrap()
            .block
            .header
            .state_root,
        world.current_state_root_hash().unwrap()
    );
    let replay = World::from_snapshot(baseline, world.journal().clone()).unwrap();
    assert_eq!(replay.snapshot(), world.snapshot());
    assert_eq!(
        replay.current_state_root_hash().unwrap(),
        world.current_state_root_hash().unwrap()
    );
}

fn assert_transit_failure_unchanged(mut world: World, event: DomainEvent, expected: &str) {
    let before = world.clone();
    let root = world.current_state_root_hash().unwrap();
    let error = world
        .append_event_for_test(WorldEventBody::Domain(event), None)
        .unwrap_err();
    assert!(format!("{error:?}").contains(expected), "{error:?}");
    unchanged(&world, &before, &root);
}

#[test]
fn transit_completion_late_failures_preserve_pending_routes_ledgers_receipts_and_payouts() {
    let (_, world, completed) = fixtures().remove(5);
    let mut identity = completed.clone();
    if let DomainEvent::MaterialTransitCompleted { kind, .. } = &mut identity {
        *kind = "copper".into();
    }
    assert_transit_failure_unchanged(world.clone(), identity, "pending commitment");
    let mut conservation = completed.clone();
    if let DomainEvent::MaterialTransitCompleted {
        received_amount, ..
    } = &mut conservation
    {
        *received_amount = 1;
    }
    assert_transit_failure_unchanged(world.clone(), conservation, "pending commitment");
    let mut overflow_world = world;
    overflow_world
        .set_ledger_material_balance(MaterialLedgerId::site("to"), "steel", i64::MAX)
        .unwrap();
    assert_transit_failure_unchanged(overflow_world, completed, "preflight");

    let all = fixtures();
    let mut routed = all[1].1.clone();
    let mut started = all[4].2.clone();
    if let DomainEvent::MaterialTransitStarted {
        job_id,
        route_id,
        path_id,
        route_ids,
        tariff_electricity_total,
        ..
    } = &mut started
    {
        *job_id = 404;
        *route_id = Some("route".into());
        *path_id = Some("path".into());
        *route_ids = vec!["route".into()];
        *tariff_electricity_total = 2;
    }
    let mut routed_state = routed.state().clone();
    routed_state
        .logistics_routes
        .get_mut("route")
        .unwrap()
        .tariff_electricity_per_unit = 1;
    routed = World::new_with_state(routed_state);
    routed
        .append_event_for_test(WorldEventBody::Domain(started.clone()), None)
        .unwrap();
    routed = with_time(
        &routed,
        routed.state().pending_material_transits[&404].ready_at,
    );
    let mut completion = DomainEvent::MaterialTransitCompleted {
        job_id: 404,
        requester_agent_id: "actor".into(),
        from_ledger: MaterialLedgerId::site("from"),
        to_ledger: MaterialLedgerId::site("to"),
        kind: "steel".into(),
        sent_amount: 2,
        received_amount: 2,
        loss_amount: 0,
        distance_km: 1,
        priority: MaterialTransitPriority::Standard,
        route_id: Some("route".into()),
        path_id: Some("path".into()),
        route_ids: vec!["route".into()],
        tariff_electricity_total: 2,
        reroute_count: 0,
    };
    let mut bad_path_state = routed.state().clone();
    bad_path_state.completed_logistics_paths.insert(
        "path".into(),
        LogisticsPathAuthorityV1 {
            path_id: "path".into(),
            route_ids: vec!["wrong".into()],
            from_ledger: MaterialLedgerId::site("from"),
            to_ledger: MaterialLedgerId::site("to"),
            kind: "steel".into(),
            settled_amount: 1,
            remaining_recipe_amount: 1,
        },
    );
    assert_transit_failure_unchanged(
        World::new_with_state(bad_path_state),
        completion.clone(),
        "path authority",
    );
    let mut payout_state = routed.state().clone();
    payout_state
        .agents
        .get_mut("owner")
        .unwrap()
        .state
        .resources
        .set(ResourceKind::Electricity, i64::MAX)
        .unwrap();
    if let DomainEvent::MaterialTransitCompleted { path_id, .. } = &mut completion {
        *path_id = Some("path".into());
    }
    assert_transit_failure_unchanged(
        World::new_with_state(payout_state),
        completion,
        "tariff credit failed",
    );
}

#[test]
fn transit_world_ledger_keeps_compat_material_cache_in_sync_on_debit_and_credit() {
    let mut debit = base_for_world_transit();
    let mut started = fixtures()[4].2.clone();
    if let DomainEvent::MaterialTransitStarted {
        from_ledger,
        to_ledger,
        ..
    } = &mut started
    {
        *from_ledger = MaterialLedgerId::world();
        *to_ledger = MaterialLedgerId::site("to");
    }
    debit
        .append_event_for_test(WorldEventBody::Domain(started), None)
        .unwrap();
    assert_eq!(debit.material_balance("steel"), 3);
    assert_eq!(
        debit.ledger_material_balance(&MaterialLedgerId::world(), "steel"),
        3
    );

    let mut credit = World::new();
    register(&mut credit, "actor");
    credit
        .set_ledger_material_balance(MaterialLedgerId::site("from"), "steel", 5)
        .unwrap();
    let mut started = fixtures()[4].2.clone();
    if let DomainEvent::MaterialTransitStarted { to_ledger, .. } = &mut started {
        *to_ledger = MaterialLedgerId::world();
    }
    credit
        .append_event_for_test(WorldEventBody::Domain(started), None)
        .unwrap();
    credit = with_time(
        &credit,
        credit.state().pending_material_transits[&14].ready_at,
    );
    let mut completed = fixtures()[5].2.clone();
    if let DomainEvent::MaterialTransitCompleted { to_ledger, .. } = &mut completed {
        *to_ledger = MaterialLedgerId::world();
    }
    credit
        .append_event_for_test(WorldEventBody::Domain(completed), None)
        .unwrap();
    assert_eq!(credit.material_balance("steel"), 2);
    assert_eq!(
        credit.ledger_material_balance(&MaterialLedgerId::world(), "steel"),
        2
    );
}

fn base_for_world_transit() -> World {
    let mut world = World::new();
    register(&mut world, "actor");
    world.set_material_balance("steel", 5).unwrap();
    world
}

#[test]
fn duplicate_route_ids_accumulate_and_release_every_reservation() {
    let all = fixtures();
    let mut world = all[1].1.clone();
    let mut started = all[4].2.clone();
    if let DomainEvent::MaterialTransitStarted {
        route_id,
        route_ids,
        ..
    } = &mut started
    {
        *route_id = Some("route".into());
        *route_ids = vec!["route".into(), "route".into()];
    }
    world
        .append_event_for_test(WorldEventBody::Domain(started), None)
        .unwrap();
    world = with_time(
        &world,
        world.state().pending_material_transits[&14].ready_at,
    );
    assert_eq!(
        world.state().logistics_routes["route"].reserved_capacity_units,
        4
    );
    let mut completed = all[5].2.clone();
    if let DomainEvent::MaterialTransitCompleted {
        route_id,
        route_ids,
        ..
    } = &mut completed
    {
        *route_id = Some("route".into());
        *route_ids = vec!["route".into(), "route".into()];
    }
    world
        .append_event_for_test(WorldEventBody::Domain(completed), None)
        .unwrap();
    assert_eq!(
        world.state().logistics_routes["route"].reserved_capacity_units,
        0
    );
    assert_eq!(
        world.state().logistics_settlement_receipts[&14]
            .route_ids
            .len(),
        2
    );
    assert_eq!(
        world
            .tick_consensus_records()
            .last()
            .unwrap()
            .block
            .header
            .state_root,
        world.current_state_root_hash().unwrap()
    );
}

#[test]
fn factory_build_world_ledger_and_settled_noop_preserve_compatibility_and_roots() {
    let all = fixtures();
    let (_, mut started_world, started) = all[6].clone();
    started_world
        .set_ledger_material_balance(MaterialLedgerId::site("unrelated"), "copper", 33)
        .unwrap();
    started_world
        .append_event_for_test(WorldEventBody::Domain(started), None)
        .unwrap();
    assert_eq!(started_world.material_balance("steel"), 8);
    assert_eq!(
        started_world.ledger_material_balance(&MaterialLedgerId::world(), "steel"),
        8
    );
    assert_eq!(
        started_world.ledger_material_balance(&MaterialLedgerId::site("unrelated"), "copper"),
        33
    );

    let (_, mut world, built) = all[7].clone();
    world
        .append_event_for_test(WorldEventBody::Domain(built.clone()), None)
        .unwrap();
    let state = world.state().clone();
    let mailbox = state.agents["actor"].mailbox.len();
    world
        .append_event_for_test(WorldEventBody::Domain(built), None)
        .unwrap();
    assert_eq!(world.state().factories, state.factories);
    assert_eq!(
        world.state().pending_factory_builds,
        state.pending_factory_builds
    );
    assert_eq!(
        world.state().settled_factory_build_ids,
        state.settled_factory_build_ids
    );
    assert_eq!(
        world.state().agents["actor"].last_active,
        state.agents["actor"].last_active
    );
    assert_eq!(world.state().agents["actor"].mailbox.len(), mailbox + 1);
    assert_eq!(
        world
            .tick_consensus_records()
            .last()
            .unwrap()
            .block
            .header
            .state_root,
        world.current_state_root_hash().unwrap()
    );
}

#[test]
fn factory_build_late_validation_failures_leave_pending_ledgers_agents_and_progress_unchanged() {
    let all = fixtures();
    let (_, settled, mut mismatch) = all[7].clone();
    let mut settled = {
        let mut world = settled;
        world
            .append_event_for_test(WorldEventBody::Domain(mismatch.clone()), None)
            .unwrap();
        world
    };
    if let DomainEvent::FactoryBuilt { site_id, .. } = &mut mismatch {
        *site_id = "wrong".into();
    }
    assert_transit_failure_unchanged(settled.clone(), mismatch, "settled factory build");

    let (_, mut early, started) = all[6].clone();
    early
        .append_event_for_test(WorldEventBody::Domain(started), None)
        .unwrap();
    assert_transit_failure_unchanged(early, all[7].2.clone(), "completion is early");

    let mut retired_state = all[7].1.state().clone();
    retired_state.retired_factory_ids.insert("factory".into());
    assert_transit_failure_unchanged(
        World::new_with_state(retired_state),
        all[7].2.clone(),
        "retired identity",
    );

    let mut payload = all[7].2.clone();
    if let DomainEvent::FactoryBuilt {
        builder_agent_id, ..
    } = &mut payload
    {
        *builder_agent_id = "other".into();
    }
    assert_transit_failure_unchanged(all[7].1.clone(), payload, "pending commitment");

    let existing = built_world().state().factories["factory"].clone();
    let mut overwrite_state = all[7].1.state().clone();
    overwrite_state.factories.insert("factory".into(), existing);
    assert_transit_failure_unchanged(
        World::new_with_state(overwrite_state),
        all[7].2.clone(),
        "overwrite active factory",
    );

    let before = settled.clone();
    let root = settled.current_state_root_hash().unwrap();
    let error = settled
        .append_event_for_test(WorldEventBody::Domain(all[6].2.clone()), None)
        .unwrap_err();
    assert!(format!("{error:?}").contains("already settled or pending"));
    unchanged(&settled, &before, &root);
}

#[test]
fn factory_recycle_success_and_retired_noop_preserve_terminal_contract_and_roots() {
    let (_, mut world, recycled) = fixtures().remove(10);
    let progress_before = world.state().industry_progress.clone();
    let mailbox_before = world.state().agents["actor"].mailbox.len();
    world
        .append_event_for_test(WorldEventBody::Domain(recycled.clone()), None)
        .unwrap();
    assert!(!world.state().factories.contains_key("factory"));
    assert!(world.state().retired_factory_ids.contains("factory"));
    assert_eq!(world.material_balance("steel"), 9);
    assert_eq!(
        world.ledger_material_balance(&MaterialLedgerId::world(), "steel"),
        9
    );
    assert_eq!(
        world.state().agents["actor"].mailbox.len(),
        mailbox_before + 1
    );
    assert!(world.state().agents["actor"].last_active >= progress_before.stage_updated_at);
    let terminal = world.state().clone();
    let terminal_mailbox = terminal.agents["actor"].mailbox.len();
    world
        .append_event_for_test(WorldEventBody::Domain(recycled), None)
        .unwrap();
    assert_eq!(world.state().factories, terminal.factories);
    assert_eq!(
        world.state().retired_factory_ids,
        terminal.retired_factory_ids
    );
    assert_eq!(world.state().material_ledgers, terminal.material_ledgers);
    assert_eq!(world.state().materials, terminal.materials);
    assert_eq!(world.state().industry_progress, terminal.industry_progress);
    assert_eq!(
        world.state().agents["actor"].last_active,
        terminal.agents["actor"].last_active
    );
    assert_eq!(
        world.state().agents["actor"].mailbox.len(),
        terminal_mailbox + 1
    );
    assert_eq!(
        world
            .tick_consensus_records()
            .last()
            .unwrap()
            .block
            .header
            .state_root,
        world.current_state_root_hash().unwrap()
    );
}

#[test]
fn factory_recycle_failures_do_not_delete_factory_or_leak_recovery() {
    let (_, world, recycled) = fixtures().remove(10);
    let mut unknown = recycled.clone();
    if let DomainEvent::FactoryRecycled { factory_id, .. } = &mut unknown {
        *factory_id = "missing".into();
    }
    assert_transit_failure_unchanged(world.clone(), unknown, "unknown factory");
    let mut wrong_operator = recycled.clone();
    if let DomainEvent::FactoryRecycled {
        operator_agent_id, ..
    } = &mut wrong_operator
    {
        *operator_agent_id = "owner".into();
    }
    assert_transit_failure_unchanged(world.clone(), wrong_operator, "operator mismatch");

    let (_, mut active, recipe) = fixtures().remove(11);
    active
        .append_event_for_test(WorldEventBody::Domain(recipe), None)
        .unwrap();
    assert_transit_failure_unchanged(active, recycled.clone(), "active recipe");

    let mut overflow = world;
    overflow.set_material_balance("steel", i64::MAX).unwrap();
    assert_transit_failure_unchanged(overflow, recycled, "preflight");
}

#[test]
fn recipe_start_late_failures_preserve_ledgers_power_paths_slots_and_world() {
    let (_, base, started) = fixtures().remove(11);
    let mut cases = Vec::new();

    let mut material = base.clone();
    material.set_material_balance("ore", 0).unwrap();
    cases.push((material, started.clone(), "insufficient material"));

    let mut power = base.clone();
    power
        .set_agent_resource_balance("actor", ResourceKind::Electricity, 0)
        .unwrap();
    cases.push((power, started.clone(), "insufficient electricity"));

    let mut slot_state = base.state().clone();
    slot_state
        .factories
        .get_mut("factory")
        .unwrap()
        .production
        .active_jobs = 1;
    cases.push((
        World::new_with_state(slot_state),
        started.clone(),
        "no free execution slot",
    ));

    let mut path = started.clone();
    if let DomainEvent::RecipeStarted {
        logistics_path_ids, ..
    } = &mut path
    {
        *logistics_path_ids = vec!["missing-path".into()];
    }
    cases.push((base.clone(), path, "logistics path authority"));

    let factory = &base.state().factories["factory"];
    let mut routed = base.clone();
    routed
        .set_ledger_material_balance(factory.input_ledger.clone(), "ore", 4)
        .unwrap();
    let mut routed_event = started;
    if let DomainEvent::RecipeStarted {
        consume_ledger,
        output_ledger,
        ..
    } = &mut routed_event
    {
        *consume_ledger = factory.input_ledger.clone();
        *output_ledger = factory.output_ledger.clone();
    }
    let unrelated = routed.ledger_material_balance(&MaterialLedgerId::world(), "ore");
    routed
        .append_event_for_test(WorldEventBody::Domain(routed_event), None)
        .unwrap();
    assert_eq!(
        routed.ledger_material_balance(&factory.input_ledger, "ore"),
        3
    );
    assert_eq!(
        routed.ledger_material_balance(&MaterialLedgerId::world(), "ore"),
        unrelated
    );

    for (world, event, needle) in cases {
        assert_transit_failure_unchanged(world, event, needle);
    }
}

#[test]
fn recipe_completion_overflow_and_settled_noop_preserve_atomic_roots_and_route() {
    let (_, base, completed) = fixtures().remove(12);
    let mut overflow = base.clone();
    overflow.set_material_balance("ingot", i64::MAX).unwrap();
    assert_transit_failure_unchanged(overflow, completed.clone(), "output preflight");

    let mut world = base;
    world
        .append_event_for_test(WorldEventBody::Domain(completed.clone()), None)
        .unwrap();
    let state = world.state().clone();
    let actor_mailbox = world.state().agents["actor"].mailbox.len();
    world
        .append_event_for_test(WorldEventBody::Domain(completed), None)
        .unwrap();
    assert_eq!(world.state().materials, state.materials);
    assert_eq!(world.state().material_ledgers, state.material_ledgers);
    assert_eq!(world.state().factories, state.factories);
    assert_eq!(world.state().pending_recipe_jobs, state.pending_recipe_jobs);
    assert_eq!(
        world.state().settled_recipe_job_ids,
        state.settled_recipe_job_ids
    );
    assert_eq!(world.state().industry_progress, state.industry_progress);
    assert_eq!(
        world.state().agents["actor"].last_active,
        state.agents["actor"].last_active
    );
    assert_eq!(
        world.state().agents["actor"].activity,
        state.agents["actor"].activity
    );
    assert_eq!(
        world.state().agents["actor"].mailbox.len(),
        actor_mailbox + 1
    );
    assert_eq!(
        world
            .tick_consensus_records()
            .last()
            .unwrap()
            .block
            .header
            .state_root,
        world.current_state_root_hash().unwrap()
    );
}

#[test]
fn production_terminal_noop_mismatch_and_missing_factory_routes_are_compatible() {
    let all = fixtures();
    let (_, factory, blocked) = all[13].clone();
    let mut terminal = blocked.clone();
    if let DomainEvent::FactoryProductionBlocked { blocker_kind, .. } = &mut terminal {
        *blocker_kind = "product_validation".into();
    }
    let before = factory.state().clone();
    let mailbox = factory.state().agents["actor"].mailbox.len();
    let mut noop = factory.clone();
    noop.append_event_for_test(WorldEventBody::Domain(terminal.clone()), None)
        .unwrap();
    assert_eq!(noop.state().factories, before.factories);
    assert_eq!(noop.state().industry_progress, before.industry_progress);
    assert_eq!(noop.state().agents["actor"].mailbox.len(), mailbox + 1);
    assert_eq!(
        noop.tick_consensus_records()
            .last()
            .unwrap()
            .block
            .header
            .state_root,
        noop.current_state_root_hash().unwrap()
    );

    let (_, active, started) = all[11].clone();
    let mut active = active;
    active
        .append_event_for_test(WorldEventBody::Domain(started), None)
        .unwrap();
    let mut mismatch = terminal;
    if let DomainEvent::FactoryProductionBlocked {
        action_id,
        recipe_id,
        ..
    } = &mut mismatch
    {
        *action_id = 201;
        *recipe_id = "wrong".into();
    }
    assert_transit_failure_unchanged(active, mismatch, "does not match pending commitment");

    for index in [14, 15] {
        let (_, world, mut event) = all[index].clone();
        if let DomainEvent::FactoryProductionResumed { factory_id, .. }
        | DomainEvent::FactoryProductionPaused { factory_id, .. } = &mut event
        {
            *factory_id = "missing".into();
        }
        let mut world = world;
        let mailbox = world.state().agents["actor"].mailbox.len();
        world
            .append_event_for_test(WorldEventBody::Domain(event), None)
            .unwrap();
        assert!(!world.state().factories.contains_key("missing"));
        assert_eq!(world.state().agents["actor"].mailbox.len(), mailbox + 1);
        assert_eq!(
            world
                .tick_consensus_records()
                .last()
                .unwrap()
                .block
                .header
                .state_root,
            world.current_state_root_hash().unwrap()
        );
    }
}
