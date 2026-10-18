fn logistics_network_fixture() -> World {
    let mut world = World::new();
    for agent_id in ["owner-a", "sender-a", "sender-b"] {
        world.submit_action(Action::RegisterAgent {
            agent_id: agent_id.to_string(),
            pos: pos(0, 0),
        });
        world.step().expect("register logistics-network agent");
    }
    world
        .set_agent_resource_balance("sender-a", ResourceKind::Electricity, 100)
        .expect("seed sender-a electricity");
    world
        .set_agent_resource_balance("sender-b", ResourceKind::Electricity, 100)
        .expect("seed sender-b electricity");
    world
        .set_ledger_material_balance(MaterialLedgerId::site("source"), "steel_plate", 20)
        .expect("seed logistics-network source");
    world
}

fn started_logistics_network_transit_fixture() -> World {
    let mut world = logistics_network_fixture();
    let route = register_network_edge(&mut world, "owner-a", "source", "destination", 100, 10, 2);
    world.submit_action(transfer_over_path(
        "sender-a",
        "source",
        "destination",
        1,
        vec![route],
        false,
    ));
    world.step().expect("start transit for integrity test");
    assert_eq!(world.pending_material_transits_len(), 1);
    world
}

fn register_network_edge(
    world: &mut World,
    owner_agent_id: &str,
    from_ledger: &str,
    to_ledger: &str,
    distance_km: i64,
    capacity_units: i64,
    tariff_electricity_per_unit: i64,
) -> String {
    let journal_before = world.journal().events.len();
    world.submit_action(Action::RegisterLogisticsRoute {
        requester_agent_id: owner_agent_id.to_string(),
        from_ledger: MaterialLedgerId::site(from_ledger),
        to_ledger: MaterialLedgerId::site(to_ledger),
        kind: "steel_plate".to_string(),
        distance_km,
        priority: MaterialTransitPriority::Standard,
        capacity_units,
        tariff_electricity_per_unit,
    });
    world.step().expect("register logistics-network edge");
    world.journal().events[journal_before..]
        .iter()
        .rev()
        .find_map(|event| match &event.body {
            WorldEventBody::Domain(DomainEvent::LogisticsRouteRegistered {
                route_id: Some(route_id),
                from_ledger: event_from,
                to_ledger: event_to,
                kind,
                distance_km: event_distance,
                ..
            }) if event_from == &MaterialLedgerId::site(from_ledger)
                && event_to == &MaterialLedgerId::site(to_ledger)
                && kind == "steel_plate"
                && *event_distance == distance_km =>
            {
                Some(route_id.clone())
            }
            _ => None,
        })
        .expect("LogisticsRouteRegistered event in registration step")
}

fn transfer_over_path(
    requester_agent_id: &str,
    from_ledger: &str,
    to_ledger: &str,
    amount: i64,
    route_ids: Vec<String>,
    auto_reroute: bool,
) -> Action {
    Action::TransferMaterial {
        requester_agent_id: requester_agent_id.to_string(),
        from_ledger: MaterialLedgerId::site(from_ledger),
        to_ledger: MaterialLedgerId::site(to_ledger),
        kind: "steel_plate".to_string(),
        amount,
        // A non-empty explicit path derives distance from its edge snapshot;
        // zero keeps this fixture independent of any future aggregate-distance
        // balancing rule.
        distance_km: 0,
        priority: None,
        route_id: None,
        route_ids,
        auto_reroute,
    }
}

fn latest_started_path(world: &World) -> (Option<String>, Vec<String>, i64, u32) {
    world
        .journal()
        .events
        .iter()
        .rev()
        .find_map(|event| match &event.body {
            WorldEventBody::Domain(DomainEvent::MaterialTransitStarted {
                path_id,
                route_ids,
                tariff_electricity_total,
                reroute_count,
                ..
            }) => Some((
                path_id.clone(),
                route_ids.clone(),
                *tariff_electricity_total,
                *reroute_count,
            )),
            _ => None,
        })
        .expect("material transit started event")
}

#[test]
fn logistics_network_selects_deterministic_two_hop_path_and_rejects_cycle_or_unreachable() {
    let mut world = logistics_network_fixture();
    let ab = register_network_edge(&mut world, "owner-a", "source", "mid-a", 100, 10, 2);
    let bd = register_network_edge(&mut world, "owner-a", "mid-a", "destination", 100, 10, 2);
    let ac = register_network_edge(&mut world, "owner-a", "source", "mid-b", 100, 10, 2);
    let cd = register_network_edge(&mut world, "owner-a", "mid-b", "destination", 100, 10, 2);

    let expected = [vec![ab.clone(), bd.clone()], vec![ac.clone(), cd.clone()]]
        .into_iter()
        .min()
        .expect("two deterministic path candidates");
    world.submit_action(transfer_over_path(
        "sender-a",
        "source",
        "destination",
        1,
        Vec::new(),
        true,
    ));
    world.step().expect("select deterministic path");
    let (path_id, route_ids, tariff, reroute_count) = latest_started_path(&world);
    assert!(
        path_id.is_some(),
        "selected path must have persisted identity"
    );
    assert_eq!(route_ids, expected);
    assert_eq!(tariff, 4);
    assert_eq!(reroute_count, 0);

    let cycle_a = register_network_edge(&mut world, "owner-a", "cycle-a", "cycle-b", 100, 10, 1);
    let cycle_b = register_network_edge(&mut world, "owner-a", "cycle-b", "cycle-a", 100, 10, 1);
    let cycle_c =
        register_network_edge(&mut world, "owner-a", "cycle-a", "destination", 100, 10, 1);
    world
        .set_ledger_material_balance(MaterialLedgerId::site("cycle-a"), "steel_plate", 1)
        .expect("seed cycle source");
    let cycle_source_before =
        world.ledger_material_balance(&MaterialLedgerId::site("cycle-a"), "steel_plate");
    let journal_before_cycle = world.journal().events.len();
    world.submit_action(transfer_over_path(
        "sender-a",
        "cycle-a",
        "destination",
        1,
        vec![cycle_a, cycle_b, cycle_c],
        false,
    ));
    world.step().expect("reject cyclic path");
    assert!(
        world.journal().events[journal_before_cycle..]
            .iter()
            .any(|event| {
                matches!(
                    &event.body,
                    WorldEventBody::Domain(DomainEvent::ActionRejected { .. })
                )
            })
    );
    assert_eq!(
        world.ledger_material_balance(&MaterialLedgerId::site("cycle-a"), "steel_plate"),
        cycle_source_before,
        "cyclic path rejection must not debit source material"
    );
    assert_eq!(world.pending_material_transits_len(), 0);
    let cycle_rejection = latest_action_rejected_message(&world).to_ascii_lowercase();
    assert!(!cycle_rejection.contains("insufficient"));
    assert!(
        cycle_rejection.contains("cycle")
            || cycle_rejection.contains("loop")
            || cycle_rejection.contains("path")
            || cycle_rejection.contains("route")
    );

    let journal_before_unreachable = world.journal().events.len();
    world.submit_action(transfer_over_path(
        "sender-a",
        "source",
        "no-such-destination",
        1,
        Vec::new(),
        true,
    ));
    world.step().expect("reject unreachable destination");
    assert!(
        world.journal().events[journal_before_unreachable..]
            .iter()
            .any(|event| {
                matches!(
                    &event.body,
                    WorldEventBody::Domain(DomainEvent::ActionRejected { .. })
                )
            })
    );
    let unreachable_rejection = latest_action_rejected_message(&world).to_ascii_lowercase();
    assert!(!unreachable_rejection.contains("insufficient"));
    assert!(
        unreachable_rejection.contains("unreachable")
            || unreachable_rejection.contains("no route")
            || unreachable_rejection.contains("path")
            || unreachable_rejection.contains("route")
    );
}

#[test]
fn logistics_network_reserves_whole_path_atomically_and_releases_capacity_once() {
    let mut world = logistics_network_fixture();
    let ab = register_network_edge(&mut world, "owner-a", "source", "mid", 100, 1, 1);
    let bd = register_network_edge(&mut world, "owner-a", "mid", "destination", 100, 1, 1);
    let path = vec![ab, bd];

    world.submit_action(transfer_over_path(
        "sender-a",
        "source",
        "destination",
        1,
        path.clone(),
        false,
    ));
    world.step().expect("reserve first whole path");
    assert_eq!(world.pending_material_transits_len(), 1);
    let source_after_first =
        world.ledger_material_balance(&MaterialLedgerId::site("source"), "steel_plate");

    world.submit_action(transfer_over_path(
        "sender-b",
        "source",
        "destination",
        1,
        path.clone(),
        false,
    ));
    world
        .step()
        .expect("reject second path on capacity conflict");
    assert_eq!(world.pending_material_transits_len(), 1);
    assert_eq!(
        world.ledger_material_balance(&MaterialLedgerId::site("source"), "steel_plate"),
        source_after_first,
        "capacity rejection must not debit source or leave a partial reservation"
    );

    world
        .step()
        .expect("complete first path and release capacity");
    assert_eq!(world.pending_material_transits_len(), 0);
    world.submit_action(transfer_over_path(
        "sender-b",
        "source",
        "destination",
        1,
        path,
        false,
    ));
    world.step().expect("reserve path after release");
    assert_eq!(world.pending_material_transits_len(), 1);
}

#[test]
fn logistics_network_owner_only_availability_and_fixed_tariff_settle_once() {
    let mut world = logistics_network_fixture();
    let route = register_network_edge(&mut world, "owner-a", "source", "destination", 100, 10, 3);

    world.submit_action(Action::SetLogisticsRouteAvailability {
        requester_agent_id: "sender-a".to_string(),
        route_id: route.clone(),
        available: false,
    });
    world.step().expect("reject non-owner availability change");
    assert!(matches!(
        world.journal().events.last().map(|event| &event.body),
        Some(WorldEventBody::Domain(DomainEvent::ActionRejected { .. }))
    ));

    world.submit_action(Action::SetLogisticsRouteAvailability {
        requester_agent_id: "owner-a".to_string(),
        route_id: route.clone(),
        available: false,
    });
    world.step().expect("owner disables route");
    world.submit_action(transfer_over_path(
        "sender-a",
        "source",
        "destination",
        2,
        vec![route.clone()],
        false,
    ));
    world.step().expect("reject unavailable route");
    assert!(matches!(
        world.journal().events.last().map(|event| &event.body),
        Some(WorldEventBody::Domain(DomainEvent::ActionRejected { .. }))
    ));

    world.submit_action(Action::SetLogisticsRouteAvailability {
        requester_agent_id: "owner-a".to_string(),
        route_id: route.clone(),
        available: true,
    });
    world.step().expect("owner enables route");
    world
        .set_agent_resource_balance("sender-a", ResourceKind::Electricity, 20)
        .expect("seed sender electricity");
    world
        .set_agent_resource_balance("owner-a", ResourceKind::Electricity, 0)
        .expect("seed owner electricity");
    world.submit_action(transfer_over_path(
        "sender-a",
        "source",
        "destination",
        2,
        vec![route],
        false,
    ));
    world.step().expect("start tariff-bearing transit");
    world.step().expect("complete tariff-bearing transit");

    assert_eq!(
        world
            .agent_resource_balance("sender-a", ResourceKind::Electricity)
            .expect("sender balance"),
        14,
        "fixed tariff must debit sender once: 2 units * 3 Electricity"
    );
    assert_eq!(
        world
            .agent_resource_balance("owner-a", ResourceKind::Electricity)
            .expect("owner balance"),
        6,
        "fixed tariff must pay edge owner once"
    );
    let sender_after_settlement = world
        .agent_resource_balance("sender-a", ResourceKind::Electricity)
        .expect("sender balance after settlement");
    let owner_after_settlement = world
        .agent_resource_balance("owner-a", ResourceKind::Electricity)
        .expect("owner balance after settlement");
    world.step().expect("replay-free idle tick");
    assert_eq!(
        world
            .agent_resource_balance("sender-a", ResourceKind::Electricity)
            .expect("sender balance after idle tick"),
        sender_after_settlement
    );
    assert_eq!(
        world
            .agent_resource_balance("owner-a", ResourceKind::Electricity)
            .expect("owner balance after idle tick"),
        owner_after_settlement
    );
}

#[test]
fn logistics_network_insufficient_electricity_rejects_before_material_or_capacity_mutation() {
    let mut world = logistics_network_fixture();
    let route = register_network_edge(&mut world, "owner-a", "source", "destination", 100, 10, 3);
    world
        .set_agent_resource_balance("sender-a", ResourceKind::Electricity, 5)
        .expect("seed insufficient sender electricity");

    let source_ledger = MaterialLedgerId::site("source");
    let source_before = world.ledger_material_balance(&source_ledger, "steel_plate");
    let electricity_before = world
        .agent_resource_balance("sender-a", ResourceKind::Electricity)
        .expect("sender electricity before rejection");
    let pending_before = world.pending_material_transits_len();
    world.submit_action(transfer_over_path(
        "sender-a",
        "source",
        "destination",
        2,
        vec![route],
        false,
    ));
    world.step().expect("reject unaffordable tariff");

    let rejection = latest_action_rejected_message(&world).to_ascii_lowercase();
    assert!(
        rejection.contains("electricity") || rejection.contains("fund"),
        "expected insufficient Electricity rejection, got {rejection}"
    );
    assert_eq!(
        world.ledger_material_balance(&source_ledger, "steel_plate"),
        source_before,
        "insufficient tariff must not debit source material"
    );
    assert_eq!(
        world
            .agent_resource_balance("sender-a", ResourceKind::Electricity)
            .expect("sender electricity after rejection"),
        electricity_before,
        "insufficient tariff must not debit sender Electricity"
    );
    assert_eq!(
        world.pending_material_transits_len(),
        pending_before,
        "insufficient tariff must not reserve path capacity or enqueue transit"
    );
}

#[test]
fn logistics_network_opt_in_reroute_attempts_one_deterministic_alternate_without_double_charge() {
    let mut world = logistics_network_fixture();
    let primary_a = register_network_edge(&mut world, "owner-a", "source", "primary", 300, 1, 1);
    let primary_b =
        register_network_edge(&mut world, "owner-a", "primary", "destination", 300, 1, 1);
    let alternate_a =
        register_network_edge(&mut world, "owner-a", "source", "alternate", 300, 1, 5);
    let alternate_b =
        register_network_edge(&mut world, "owner-a", "alternate", "destination", 300, 1, 5);
    let primary = vec![primary_a, primary_b];
    let alternate = vec![alternate_a, alternate_b];

    world
        .set_agent_resource_balance("sender-b", ResourceKind::Electricity, 20)
        .expect("seed primary sender electricity");
    world.submit_action(transfer_over_path(
        "sender-b",
        "source",
        "destination",
        1,
        primary.clone(),
        false,
    ));
    world.step().expect("occupy primary path capacity");
    let started_before = world
        .journal()
        .events
        .iter()
        .filter(|event| {
            matches!(
                event.body,
                WorldEventBody::Domain(DomainEvent::MaterialTransitStarted { .. })
            )
        })
        .count();
    let source_before_no_reroute =
        world.ledger_material_balance(&MaterialLedgerId::site("source"), "steel_plate");
    world.submit_action(transfer_over_path(
        "sender-a",
        "source",
        "destination",
        1,
        primary.clone(),
        false,
    ));
    world.step().expect("reject without reroute opt-in");
    assert_eq!(
        world
            .journal()
            .events
            .iter()
            .filter(|event| {
                matches!(
                    event.body,
                    WorldEventBody::Domain(DomainEvent::MaterialTransitStarted { .. })
                )
            })
            .count(),
        started_before,
        "reroute is opt-in"
    );
    assert_eq!(
        world.ledger_material_balance(&MaterialLedgerId::site("source"), "steel_plate"),
        source_before_no_reroute,
        "failed primary attempt must not debit source"
    );

    world
        .set_agent_resource_balance("sender-a", ResourceKind::Electricity, 20)
        .expect("seed reroute sender electricity");
    world.submit_action(transfer_over_path(
        "sender-a",
        "source",
        "destination",
        1,
        primary,
        true,
    ));
    world.step().expect("perform one deterministic reroute");
    let (_path_id, route_ids, tariff, reroute_count) = latest_started_path(&world);
    assert_eq!(route_ids, alternate);
    assert_eq!(tariff, 10);
    assert_eq!(reroute_count, 1);
    assert_eq!(world.pending_material_transits_len(), 2);
    for _ in 0..8 {
        if world.pending_material_transits_len() == 0 {
            break;
        }
        world.step().expect("complete primary and rerouted paths");
    }
    assert_eq!(world.pending_material_transits_len(), 0);
    assert_eq!(
        world
            .agent_resource_balance("sender-a", ResourceKind::Electricity)
            .expect("reroute sender electricity"),
        10,
        "only the effective alternate path tariff is charged once"
    );
}

#[test]
fn logistics_network_reroute_persists_receipt_and_duplicate_completion_is_idempotent() {
    let mut world = logistics_network_fixture();
    let primary_a = register_network_edge(&mut world, "owner-a", "source", "primary", 300, 1, 1);
    let primary_b =
        register_network_edge(&mut world, "owner-a", "primary", "destination", 300, 1, 1);
    let alternate_a =
        register_network_edge(&mut world, "owner-a", "source", "alternate", 300, 1, 5);
    let alternate_b =
        register_network_edge(&mut world, "owner-a", "alternate", "destination", 300, 1, 5);
    let primary = vec![primary_a, primary_b];
    let alternate = vec![alternate_a, alternate_b];

    world
        .set_agent_resource_balance("sender-b", ResourceKind::Electricity, 20)
        .expect("seed primary sender electricity");
    world.submit_action(transfer_over_path(
        "sender-b",
        "source",
        "destination",
        1,
        primary.clone(),
        false,
    ));
    world.step().expect("occupy primary path capacity");

    world
        .set_agent_resource_balance("sender-a", ResourceKind::Electricity, 20)
        .expect("seed reroute sender electricity");
    world.submit_action(transfer_over_path(
        "sender-a",
        "source",
        "destination",
        1,
        primary,
        true,
    ));
    world.step().expect("perform one deterministic reroute");

    let rerouted_job_id = world
        .journal()
        .events
        .iter()
        .rev()
        .find_map(|event| match &event.body {
            WorldEventBody::Domain(DomainEvent::MaterialTransitStarted {
                job_id,
                route_ids,
                reroute_count,
                tariff_electricity_total,
                ..
            }) if *reroute_count == 1 => {
                assert_eq!(route_ids, &alternate);
                assert_eq!(*tariff_electricity_total, 10);
                Some(*job_id)
            }
            _ => None,
        })
        .expect("rerouted transit job");

    let receipt = world
        .journal()
        .events
        .iter()
        .rev()
        .find(|event| format!("{:?}", event.body).contains("LogisticsPathRerouted"))
        .expect("dedicated reroute receipt event");
    let receipt_debug = format!("{:?}", receipt.body);
    assert!(receipt_debug.contains("reroute"));
    assert!(receipt_debug.contains("tariff"));
    assert!(receipt_debug.contains("owner"));
    assert!(receipt_debug.contains("tax"));
    assert!(receipt_debug.contains(&alternate[0]) && receipt_debug.contains(&alternate[1]));

    for _ in 0..8 {
        if world.pending_material_transits_len() == 0 {
            break;
        }
        world.step().expect("complete rerouted transit");
    }
    let completed = world
        .journal()
        .events
        .iter()
        .find_map(|event| match &event.body {
            WorldEventBody::Domain(
                event @ DomainEvent::MaterialTransitCompleted { job_id, .. },
            ) if *job_id == rerouted_job_id => Some(event.clone()),
            _ => None,
        })
        .expect("rerouted transit completion");
    let mut replay = world.state().clone();
    let state_before_duplicate = serde_json::to_vec(&replay).expect("serialize settled state");
    replay
        .apply_domain_event(&completed, replay.time)
        .expect("duplicate completion replay");
    assert_eq!(
        serde_json::to_vec(&replay).expect("serialize replayed state"),
        state_before_duplicate,
        "duplicate completion must not double release capacity or settle tariff/payout"
    );
}

#[test]
fn industrial_integrity_unknown_material_transit_completion_fails_closed_before_mutation() {
    let world = started_logistics_network_transit_fixture();
    let pending = world
        .state()
        .pending_material_transits
        .values()
        .next()
        .expect("pending transit job")
        .clone();
    let mut replay = world.state().clone();
    let before = serde_json::to_vec(&replay).expect("serialize state before unknown completion");
    let event = DomainEvent::MaterialTransitCompleted {
        job_id: pending.job_id.saturating_add(1),
        requester_agent_id: pending.requester_agent_id,
        from_ledger: pending.from_ledger,
        to_ledger: pending.to_ledger,
        kind: pending.kind,
        sent_amount: pending.amount,
        received_amount: pending.amount,
        loss_amount: 0,
        distance_km: pending.distance_km,
        priority: pending.priority,
        route_id: pending.route_id,
        path_id: pending.path_id,
        route_ids: pending.route_ids,
        tariff_electricity_total: pending.tariff_electricity_total,
        reroute_count: pending.reroute_count,
    };

    let result = replay.apply_domain_event(&event, replay.time);
    assert!(result.is_err(), "unknown transit completion must be rejected");
    assert!(
        serde_json::to_vec(&replay).expect("serialize state after unknown completion") == before,
        "unknown transit completion must not mutate pending state or balances"
    );
}

#[test]
fn industrial_integrity_mismatched_material_transit_completion_fails_closed_before_mutation() {
    let world = started_logistics_network_transit_fixture();
    let pending = world
        .state()
        .pending_material_transits
        .values()
        .next()
        .expect("pending transit job")
        .clone();
    let mut replay = world.state().clone();
    let before = serde_json::to_vec(&replay).expect("serialize state before mismatched completion");
    let event = DomainEvent::MaterialTransitCompleted {
        job_id: pending.job_id,
        requester_agent_id: pending.requester_agent_id,
        from_ledger: MaterialLedgerId::site("tampered-source"),
        to_ledger: pending.to_ledger,
        kind: pending.kind,
        sent_amount: pending.amount,
        received_amount: pending.amount,
        loss_amount: 0,
        distance_km: pending.distance_km,
        priority: pending.priority,
        route_id: pending.route_id,
        path_id: pending.path_id,
        route_ids: pending.route_ids,
        tariff_electricity_total: pending.tariff_electricity_total,
        reroute_count: pending.reroute_count,
    };

    let result = replay.apply_domain_event(&event, replay.time);
    assert!(result.is_err(), "mismatched transit completion must be rejected");
    assert!(
        serde_json::to_vec(&replay).expect("serialize state after mismatched completion") == before,
        "mismatched transit completion must not mutate pending state or balances"
    );
}

#[test]
fn industrial_integrity_material_transit_destination_overflow_fails_closed_before_settlement() {
    let mut world = started_logistics_network_transit_fixture();
    world
        .set_ledger_material_balance(
            MaterialLedgerId::site("destination"),
            "steel_plate",
            i64::MAX,
        )
        .expect("seed destination at material balance ceiling");
    let pending = world
        .state()
        .pending_material_transits
        .values()
        .next()
        .expect("pending transit job")
        .clone();
    let mut replay = world.state().clone();
    let before = serde_json::to_vec(&replay).expect("serialize state before overflow completion");
    let event = DomainEvent::MaterialTransitCompleted {
        job_id: pending.job_id,
        requester_agent_id: pending.requester_agent_id,
        from_ledger: pending.from_ledger,
        to_ledger: pending.to_ledger,
        kind: pending.kind,
        sent_amount: pending.amount,
        received_amount: pending.amount,
        loss_amount: 0,
        distance_km: pending.distance_km,
        priority: pending.priority,
        route_id: pending.route_id,
        path_id: pending.path_id,
        route_ids: pending.route_ids,
        tariff_electricity_total: pending.tariff_electricity_total,
        reroute_count: pending.reroute_count,
    };

    let result = replay.apply_domain_event(&event, replay.time);
    assert!(
        result.is_err(),
        "destination material overflow must reject transit completion"
    );
    assert_eq!(
        serde_json::to_vec(&replay).expect("serialize state after overflow completion"),
        before,
        "overflow rejection must retain pending transit and avoid receipt/progress mutation"
    );
}

#[test]
fn logistics_network_legacy_empty_binding_keeps_direct_transfer_when_graph_edges_exist() {
    let mut world = logistics_network_fixture();
    let route = register_network_edge(&mut world, "owner-a", "source", "destination", 100, 10, 7);
    let sender_electricity_before = world
        .agent_resource_balance("sender-a", ResourceKind::Electricity)
        .expect("sender electricity before direct transfer");
    let source_before =
        world.ledger_material_balance(&MaterialLedgerId::site("source"), "steel_plate");
    let journal_before = world.journal().events.len();
    world.submit_action(Action::TransferMaterial {
        requester_agent_id: "sender-a".to_string(),
        from_ledger: MaterialLedgerId::site("source"),
        to_ledger: MaterialLedgerId::site("destination"),
        kind: "steel_plate".to_string(),
        amount: 1,
        distance_km: 0,
        priority: None,
        route_id: None,
        route_ids: Vec::new(),
        auto_reroute: false,
    });
    world
        .step()
        .expect("legacy direct transfer with graph edges");

    assert!(
        world.journal().events[journal_before..]
            .iter()
            .any(|event| {
                matches!(
                    &event.body,
                    WorldEventBody::Domain(DomainEvent::MaterialTransferred {
                        route_id: None,
                        distance_km: 0,
                        ..
                    })
                )
            })
    );
    assert!(
        !world.journal().events[journal_before..]
            .iter()
            .any(|event| {
                matches!(
                    &event.body,
                    WorldEventBody::Domain(DomainEvent::MaterialTransitStarted { .. })
                )
            })
    );
    assert_eq!(world.pending_material_transits_len(), 0);
    assert_eq!(
        world.ledger_material_balance(&MaterialLedgerId::site("source"), "steel_plate"),
        source_before - 1
    );
    assert_eq!(
        world
            .agent_resource_balance("sender-a", ResourceKind::Electricity)
            .expect("sender electricity after direct transfer"),
        sender_electricity_before,
        "legacy empty binding must not silently charge a graph tariff"
    );
    assert_eq!(
        world
            .state()
            .logistics_routes
            .get(&route)
            .expect("registered graph edge")
            .reserved_capacity_units,
        0,
        "legacy empty binding must not reserve graph edge capacity"
    );
}

#[test]
fn logistics_network_zero_distance_explicit_path_retains_path_authority_and_tariff() {
    let mut world = logistics_network_fixture();
    let route = register_network_edge(&mut world, "owner-a", "source", "destination", 0, 10, 4);
    let journal_before = world.journal().events.len();
    world.submit_action(transfer_over_path(
        "sender-a",
        "source",
        "destination",
        2,
        vec![route.clone()],
        false,
    ));
    world.step().expect("start explicit zero-distance path");
    let path_event_debug = world.journal().events[journal_before..]
        .iter()
        .map(|event| format!("{:?}", event.body))
        .find(|debug| {
            debug.contains("MaterialTransitStarted") || debug.contains("MaterialTransitSettled")
        })
        .expect("zero-distance explicit path settlement event");
    assert!(
        !path_event_debug.contains("MaterialTransferred"),
        "explicit path must not fall back to legacy immediate transfer"
    );
    assert!(path_event_debug.contains(&route));
    assert!(path_event_debug.contains("tariff") && path_event_debug.contains('8'));
}

#[test]
fn completed_multi_hop_path_is_the_only_recipe_path_binding_authority() {
    let factory_id = "factory.recipe.path.binding";
    let recipe_id = "recipe.recipe.path.binding";
    let mut world = recipe_route_fixture(factory_id);
    let route_a = register_recipe_route(&mut world, "source-path", "mid-path", "iron_ingot");
    let route_b = register_recipe_route(&mut world, "mid-path", "site-1", "iron_ingot");
    world
        .set_ledger_material_balance(MaterialLedgerId::site("source-path"), "iron_ingot", 2)
        .expect("seed multi-hop path source");
    world.submit_action(Action::TransferMaterial {
        requester_agent_id: "builder-a".to_string(),
        from_ledger: MaterialLedgerId::site("source-path"),
        to_ledger: MaterialLedgerId::site("site-1"),
        kind: "iron_ingot".to_string(),
        amount: 2,
        distance_km: 0,
        priority: Some(MaterialTransitPriority::Standard),
        route_id: None,
        route_ids: vec![route_a, route_b],
        auto_reroute: false,
    });
    world.step().expect("start multi-hop path");
    let started_path_id = world
        .journal()
        .events
        .iter()
        .rev()
        .find_map(|event| match &event.body {
            WorldEventBody::Domain(DomainEvent::MaterialTransitStarted {
                path_id: Some(path_id),
                ..
            }) => Some(path_id.clone()),
            _ => None,
        })
        .expect("multi-hop path id");
    while world.pending_material_transits_len() > 0 {
        world.step().expect("complete multi-hop path");
    }
    let completed_path_id = world
        .journal()
        .events
        .iter()
        .rev()
        .find_map(|event| match &event.body {
            WorldEventBody::Domain(DomainEvent::MaterialTransitCompleted {
                path_id: Some(path_id),
                ..
            }) => Some(path_id.clone()),
            _ => None,
        })
        .expect("completed multi-hop path id");
    assert_eq!(completed_path_id, started_path_id);

    let plan = RecipeExecutionPlan::accepted(
        1,
        vec![MaterialStack::new("iron_ingot", 2)],
        vec![MaterialStack::new("gear", 1)],
        Vec::new(),
        1,
        1,
    );
    let journal_before_unknown = world.journal().events.len();
    world.submit_action(Action::ScheduleRecipe {
        requester_agent_id: "builder-a".to_string(),
        factory_id: factory_id.to_string(),
        recipe_id: recipe_id.to_string(),
        plan: plan.clone(),
        logistics_route_ids: Vec::new(),
        logistics_path_ids: vec!["path-unknown".to_string()],
    });
    world.step().expect("reject unknown recipe path binding");
    assert!(
        world.journal().events[journal_before_unknown..]
            .iter()
            .any(|event| {
                matches!(
                    &event.body,
                    WorldEventBody::Domain(DomainEvent::ActionRejected { .. })
                )
            })
    );
    assert_eq!(world.pending_recipe_jobs_len(), 0);

    let journal_before_valid = world.journal().events.len();
    world.submit_action(Action::ScheduleRecipe {
        requester_agent_id: "builder-a".to_string(),
        factory_id: factory_id.to_string(),
        recipe_id: recipe_id.to_string(),
        plan,
        logistics_route_ids: Vec::new(),
        logistics_path_ids: vec![completed_path_id.clone()],
    });
    world.step().expect("start completed-path recipe");
    assert!(
        world.journal().events[journal_before_valid..]
            .iter()
            .any(|event| {
                matches!(
                    &event.body,
                    WorldEventBody::Domain(DomainEvent::RecipeStarted {
                        logistics_path_ids,
                        ..
                    }) if logistics_path_ids == &vec![completed_path_id.clone()]
                )
            })
    );
    assert_eq!(
        world
            .state()
            .pending_recipe_jobs
            .values()
            .next()
            .map(|job| job.logistics_path_ids.clone()),
        Some(vec![completed_path_id.clone()])
    );
    world.step().expect("complete completed-path recipe");
    assert!(world.journal().events.iter().rev().any(|event| {
        matches!(
            &event.body,
            WorldEventBody::Domain(DomainEvent::RecipeCompleted {
                logistics_path_ids,
                ..
            }) if logistics_path_ids == &vec![completed_path_id.clone()]
        )
    }));
    assert_eq!(
        world
            .state()
            .factories
            .get(factory_id)
            .and_then(|factory| factory
                .production
                .last_completed_canonical_snapshot
                .as_ref())
            .map(|snapshot| snapshot.logistics_path_ids.clone()),
        Some(vec![completed_path_id])
    );
}

#[test]
fn logistics_network_max_hops_is_inclusive_and_explicit_auto_plans_agree() {
    let mut world = logistics_network_fixture();
    let mut max_path = Vec::new();
    let mut from_ledger = "source".to_string();
    for hop in 0..8 {
        let to_ledger = format!("max-hop-{hop}");
        max_path.push(register_network_edge(
            &mut world,
            "owner-a",
            &from_ledger,
            &to_ledger,
            1,
            10,
            0,
        ));
        from_ledger = to_ledger;
    }
    let destination = from_ledger;
    world.submit_action(transfer_over_path(
        "sender-a",
        "source",
        &destination,
        1,
        max_path.clone(),
        false,
    ));
    world.step().expect("accept exactly configured max hops");
    let (_path_id, explicit_route_ids, _tariff, _reroute_count) = latest_started_path(&world);
    assert_eq!(explicit_route_ids, max_path);
    while world.pending_material_transits_len() > 0 {
        world.step().expect("complete max-hop explicit path");
    }

    let mut too_long_path = Vec::new();
    let mut from_ledger = "source".to_string();
    for hop in 0..9 {
        let to_ledger = format!("too-long-hop-{hop}");
        too_long_path.push(register_network_edge(
            &mut world,
            "owner-a",
            &from_ledger,
            &to_ledger,
            1,
            10,
            0,
        ));
        from_ledger = to_ledger;
    }
    let too_long_destination = from_ledger;
    let journal_before_too_long = world.journal().events.len();
    world.submit_action(transfer_over_path(
        "sender-a",
        "source",
        &too_long_destination,
        1,
        too_long_path,
        false,
    ));
    world.step().expect("reject max-hop overflow");
    assert!(
        world.journal().events[journal_before_too_long..]
            .iter()
            .any(|event| {
                matches!(
                    &event.body,
                    WorldEventBody::Domain(DomainEvent::ActionRejected { .. })
                )
            })
    );

    let journal_before_auto = world.journal().events.len();
    world.submit_action(transfer_over_path(
        "sender-a",
        "source",
        &destination,
        1,
        Vec::new(),
        true,
    ));
    world.step().expect("auto-plan exactly configured max hops");
    let (_path_id, auto_route_ids, _tariff, _reroute_count) = latest_started_path(&world);
    assert!(
        world.journal().events[journal_before_auto..]
            .iter()
            .any(|event| {
                matches!(
                    &event.body,
                    WorldEventBody::Domain(DomainEvent::MaterialTransitStarted { .. })
                )
            })
    );
    assert_eq!(auto_route_ids, max_path);
}

#[test]
fn logistics_network_path_and_reservation_identity_survive_serde_replay() {
    let mut world = logistics_network_fixture();
    let route = register_network_edge(&mut world, "owner-a", "source", "destination", 100, 3, 2);
    world.submit_action(transfer_over_path(
        "sender-a",
        "source",
        "destination",
        1,
        vec![route],
        false,
    ));
    world.step().expect("start persisted path");
    let (path_id, route_ids, tariff, reroute_count) = latest_started_path(&world);
    let job = world
        .state()
        .pending_material_transits
        .values()
        .next()
        .expect("pending path job");
    assert_eq!(job.path_id, path_id);
    assert_eq!(job.route_ids, route_ids);
    assert_eq!(job.tariff_electricity_total, tariff);
    assert_eq!(job.reroute_count, reroute_count);

    let state_json = serde_json::to_vec(world.state()).expect("serialize path state");
    let restored: crate::WorldState =
        serde_json::from_slice(&state_json).expect("deserialize path state");
    let restored_job = restored
        .pending_material_transits
        .values()
        .next()
        .expect("restored pending path job");
    assert_eq!(restored_job, job);
}
