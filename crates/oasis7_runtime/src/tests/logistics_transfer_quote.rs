use super::pos;
use crate::{
    Action, DomainEvent, MaterialDefaultPriority, MaterialLedgerId, MaterialProfileV1,
    MaterialTransitPriority, MaterialTransportLossClass, RejectReason, World, WorldEventBody,
};

#[expect(
    clippy::too_many_arguments,
    reason = "Logistics quote fixture keeps route protocol fields explicit at call sites."
)]
fn register_route(
    world: &mut World,
    requester_agent_id: &str,
    from_ledger: &str,
    to_ledger: &str,
    kind: &str,
    distance_km: i64,
    capacity_units: i64,
    tariff_electricity_per_unit: i64,
) -> String {
    let journal_before = world.journal().events.len();
    world.submit_action(Action::RegisterLogisticsRoute {
        requester_agent_id: requester_agent_id.to_string(),
        from_ledger: MaterialLedgerId::site(from_ledger),
        to_ledger: MaterialLedgerId::site(to_ledger),
        kind: kind.to_string(),
        distance_km,
        priority: MaterialTransitPriority::Standard,
        capacity_units,
        tariff_electricity_per_unit,
    });
    world.step().expect("register quote route");
    world.journal().events[journal_before..]
        .iter()
        .rev()
        .find_map(|event| match &event.body {
            WorldEventBody::Domain(DomainEvent::LogisticsRouteRegistered {
                route_id: Some(route_id),
                from_ledger: event_from,
                to_ledger: event_to,
                kind: event_kind,
                distance_km: event_distance,
                priority,
                ..
            }) if event_from == &MaterialLedgerId::site(from_ledger)
                && event_to == &MaterialLedgerId::site(to_ledger)
                && event_kind == kind
                && *event_distance == distance_km
                && *priority == MaterialTransitPriority::Standard =>
            {
                Some(route_id.clone())
            }
            _ => None,
        })
        .expect("registered quote route id")
}

fn transfer_over_path(
    requester_agent_id: &str,
    from_ledger: &str,
    to_ledger: &str,
    kind: &str,
    amount: i64,
    distance_km: i64,
    route_ids: Vec<String>,
) -> Action {
    Action::TransferMaterial {
        requester_agent_id: requester_agent_id.to_string(),
        from_ledger: MaterialLedgerId::site(from_ledger),
        to_ledger: MaterialLedgerId::site(to_ledger),
        kind: kind.to_string(),
        amount,
        distance_km,
        priority: None,
        route_id: None,
        route_ids,
        auto_reroute: false,
    }
}

#[test]
fn logistics_transfer_quote_is_read_only_deterministic_and_matches_transit_authority() {
    let mut world = World::new();
    let source = MaterialLedgerId::site("quote-source");
    let destination = MaterialLedgerId::site("quote-destination");
    world.submit_action(Action::RegisterAgent {
        agent_id: "quote-operator".to_string(),
        pos: pos(0, 0),
    });
    world.step().expect("register quote operator");
    world
        .upsert_material_profile(MaterialProfileV1 {
            kind: "copper_wire".to_string(),
            tier: 2,
            category: "intermediate".to_string(),
            stack_limit: 500,
            transport_loss_class: MaterialTransportLossClass::High,
            decay_bps_per_tick: 0,
            default_priority: MaterialDefaultPriority::Standard,
        })
        .expect("insert high-loss profile");
    world
        .set_ledger_material_balance(source.clone(), "copper_wire", 100)
        .expect("seed source");

    let state_before = world.snapshot();
    let journal_before = world.journal().clone();
    let quote = world
        .logistics_transfer_quote(
            "quote-operator",
            &source,
            &destination,
            "copper_wire",
            50,
            100,
            None,
        )
        .expect("default-priority logistics quote");
    let repeat = world
        .logistics_transfer_quote(
            "quote-operator",
            &source,
            &destination,
            "copper_wire",
            50,
            100,
            None,
        )
        .expect("repeat logistics quote");

    assert_eq!(quote, repeat);
    assert_eq!(world.snapshot(), state_before);
    assert_eq!(world.journal(), &journal_before);
    assert!(quote.conditional);
    assert!(quote.submission_feasible);
    assert_eq!(quote.max_transferable_amount, 100);
    assert_eq!(quote.sent_amount, 50);
    assert_eq!(quote.loss_bps, 20);
    assert_eq!(quote.expected_loss_amount, 10);
    assert_eq!(quote.expected_received_amount, 40);
    assert_eq!(quote.source_amount_before, 100);
    assert_eq!(quote.source_amount_after, 50);
    assert_eq!(quote.destination_amount_before, 0);
    assert_eq!(quote.destination_expected_amount_after, 40);
    assert_eq!(quote.ticks_until_arrival, 1);
    // `submit_action` is handled on the next `world.step()`, which advances
    // time before calculating the transit ready tick.
    assert_eq!(quote.ready_at, world.state().time.saturating_add(2));
    assert_eq!(quote.effective_priority, MaterialTransitPriority::Standard);
    assert!(!quote.priority_reason.is_empty());
    assert_eq!(quote.inflight_before, 0);
    assert!(quote.inflight_capacity >= 1);
    assert!(!quote.recommendation.is_empty());

    let zero_distance = world
        .logistics_transfer_quote(
            "quote-operator",
            &source,
            &destination,
            "copper_wire",
            10,
            0,
            Some(MaterialTransitPriority::Urgent),
        )
        .expect("zero-distance logistics quote");
    assert_eq!(zero_distance.loss_bps, 0);
    assert_eq!(zero_distance.expected_loss_amount, 0);
    assert_eq!(zero_distance.expected_received_amount, 10);
    assert_eq!(zero_distance.ticks_until_arrival, 0);
    assert_eq!(zero_distance.ready_at, world.state().time.saturating_add(1));
    assert_eq!(
        zero_distance.effective_priority,
        MaterialTransitPriority::Urgent
    );
    assert_ne!(zero_distance.priority_reason, quote.priority_reason);

    world.submit_action(Action::TransferMaterial {
        requester_agent_id: "quote-operator".to_string(),
        from_ledger: source.clone(),
        to_ledger: destination.clone(),
        kind: "copper_wire".to_string(),
        amount: 50,
        distance_km: 100,
        priority: None,
        route_id: None,
        route_ids: Vec::new(),
        auto_reroute: false,
    });
    world.step().expect("start quoted transit");
    let (started_loss_bps, started_ready_at, started_priority) = world
        .journal()
        .events
        .last()
        .and_then(|event| match &event.body {
            WorldEventBody::Domain(DomainEvent::MaterialTransitStarted {
                loss_bps,
                ready_at,
                priority,
                ..
            }) => Some((*loss_bps, *ready_at, *priority)),
            _ => None,
        })
        .expect("quoted transit started");
    assert_eq!(started_loss_bps, quote.loss_bps);
    assert_eq!(started_ready_at, quote.ready_at);
    assert_eq!(started_priority, quote.effective_priority);

    world.step().expect("complete quoted transit");
    let (received_amount, loss_amount) = world
        .journal()
        .events
        .last()
        .and_then(|event| match &event.body {
            WorldEventBody::Domain(DomainEvent::MaterialTransitCompleted {
                received_amount,
                loss_amount,
                ..
            }) => Some((*received_amount, *loss_amount)),
            _ => None,
        })
        .expect("quoted transit completed");
    assert_eq!(loss_amount, quote.expected_loss_amount);
    assert_eq!(received_amount, quote.expected_received_amount);
}

#[test]
fn logistics_transfer_quote_makes_capacity_and_amount_recovery_recommendations_conditional() {
    let mut world = World::new();
    let source = MaterialLedgerId::site("quote-source");
    let destination = MaterialLedgerId::site("quote-destination");
    world.submit_action(Action::RegisterAgent {
        agent_id: "quote-operator".to_string(),
        pos: pos(0, 0),
    });
    world.step().expect("register quote operator");
    world
        .set_ledger_material_balance(source.clone(), "iron_ingot", 100)
        .expect("seed source");

    for index in 0..2 {
        world.submit_action(Action::TransferMaterial {
            requester_agent_id: "quote-operator".to_string(),
            from_ledger: source.clone(),
            to_ledger: MaterialLedgerId::site(format!("quote-destination-{index}")),
            kind: "iron_ingot".to_string(),
            amount: 20,
            distance_km: 200,
            priority: None,
            route_id: None,
            route_ids: Vec::new(),
            auto_reroute: false,
        });
    }
    world.step().expect("start capacity-filling transits");
    assert_eq!(world.pending_material_transits_len(), 2);

    let state_before = world.snapshot();
    let journal_before = world.journal().clone();
    let capacity_quote = world
        .logistics_transfer_quote(
            "quote-operator",
            &source,
            &destination,
            "iron_ingot",
            20,
            200,
            None,
        )
        .expect("capacity quote");
    let amount_quote = world
        .logistics_transfer_quote(
            "quote-operator",
            &source,
            &destination,
            "iron_ingot",
            61,
            200,
            None,
        )
        .expect("over-available amount quote");

    assert_eq!(world.snapshot(), state_before);
    assert_eq!(world.journal(), &journal_before);
    assert!(capacity_quote.conditional);
    assert_eq!(
        capacity_quote.inflight_before,
        capacity_quote.inflight_capacity
    );
    assert!(!capacity_quote.recommendation.is_empty());
    assert!(amount_quote.conditional);
    assert!(!amount_quote.submission_feasible);
    assert_eq!(amount_quote.max_transferable_amount, 60);
    assert_eq!(amount_quote.sent_amount, 0);
    assert_eq!(amount_quote.expected_loss_amount, 0);
    assert_eq!(amount_quote.expected_received_amount, 0);
    assert_eq!(
        amount_quote.source_amount_after,
        amount_quote.source_amount_before
    );
    assert_eq!(
        amount_quote.destination_expected_amount_after,
        amount_quote.destination_amount_before
    );
    assert_eq!(
        amount_quote.recommendation,
        "reduce_amount_or_source_materials"
    );
}

#[test]
fn route_aware_transfer_quote_matches_explicit_two_hop_action_without_mutation() {
    let mut world = World::new();
    let requester = "route-quote-operator";
    let source = MaterialLedgerId::site("route-quote-source");
    let _relay = MaterialLedgerId::site("route-quote-relay");
    let destination = MaterialLedgerId::site("route-quote-destination");
    world.submit_action(Action::RegisterAgent {
        agent_id: requester.to_string(),
        pos: pos(0, 0),
    });
    world.step().expect("register route quote operator");
    world
        .set_agent_resource_balance(
            requester,
            oasis7_agent_api::world_action::ResourceKind::Electricity,
            1_000,
        )
        .expect("seed route quote electricity");
    world
        .set_ledger_material_balance(source.clone(), "iron_ingot", 100)
        .expect("seed route quote source");

    let route_a = register_route(
        &mut world,
        requester,
        "route-quote-source",
        "route-quote-relay",
        "iron_ingot",
        60,
        100,
        2,
    );
    let route_b = register_route(
        &mut world,
        requester,
        "route-quote-relay",
        "route-quote-destination",
        "iron_ingot",
        40,
        100,
        3,
    );
    let route_ids = vec![route_a, route_b];
    let state_before_quote = world.snapshot();
    let journal_before_quote = world.journal().clone();

    let quote = world
        .logistics_transfer_quote_with_path(
            requester,
            &source,
            &destination,
            "iron_ingot",
            100,
            0,
            None,
            route_ids.as_slice(),
            false,
        )
        .expect("explicit two-hop transfer quote");
    let repeat = world
        .logistics_transfer_quote_with_path(
            requester,
            &source,
            &destination,
            "iron_ingot",
            100,
            0,
            None,
            route_ids.as_slice(),
            false,
        )
        .expect("repeat explicit two-hop transfer quote");

    assert_eq!(quote, repeat);
    assert_eq!(world.snapshot(), state_before_quote);
    assert_eq!(world.journal(), &journal_before_quote);
    assert!(quote.conditional);
    assert!(quote.submission_feasible);
    assert_eq!(quote.max_transferable_amount, 100);
    assert_eq!(quote.sent_amount, 100);
    assert_eq!(quote.distance_km, 100);
    assert_eq!(quote.loss_bps, 5);
    assert_eq!(quote.expected_loss_amount, 5);
    assert_eq!(quote.expected_received_amount, 95);
    assert_eq!(quote.source_amount_before, 100);
    assert_eq!(quote.source_amount_after, 0);
    assert_eq!(quote.destination_amount_before, 0);
    assert_eq!(quote.destination_expected_amount_after, 95);
    assert_eq!(quote.ticks_until_arrival, 1);
    assert_eq!(quote.tariff_electricity_total, 500);
    assert_eq!(quote.reroute_count, 0);
    assert_eq!(quote.route_ids, route_ids);
    assert!(quote.path_id.is_some());
    assert_eq!(quote.effective_priority, MaterialTransitPriority::Standard);

    world.submit_action(transfer_over_path(
        requester,
        "route-quote-source",
        "route-quote-destination",
        "iron_ingot",
        100,
        0,
        quote.route_ids.clone(),
    ));
    world.step().expect("start quoted two-hop transfer");
    let started = world
        .journal()
        .events
        .iter()
        .rev()
        .find_map(|event| match &event.body {
            WorldEventBody::Domain(event @ DomainEvent::MaterialTransitStarted { .. }) => {
                Some(event.clone())
            }
            _ => None,
        })
        .expect("quoted two-hop transfer started");
    let DomainEvent::MaterialTransitStarted {
        distance_km,
        loss_bps,
        ready_at,
        priority,
        path_id,
        route_ids: started_route_ids,
        tariff_electricity_total,
        reroute_count,
        ..
    } = started
    else {
        unreachable!("matched MaterialTransitStarted above")
    };
    assert_eq!(distance_km, quote.distance_km);
    assert_eq!(loss_bps, quote.loss_bps);
    assert_eq!(ready_at, quote.ready_at);
    assert_eq!(priority, quote.effective_priority);
    assert_eq!(path_id, quote.path_id);
    assert_eq!(started_route_ids, quote.route_ids);
    assert_eq!(tariff_electricity_total, quote.tariff_electricity_total);
    assert_eq!(reroute_count, quote.reroute_count);

    world.step().expect("complete quoted two-hop transfer");
    let (received_amount, loss_amount) = world
        .journal()
        .events
        .iter()
        .rev()
        .find_map(|event| match &event.body {
            WorldEventBody::Domain(DomainEvent::MaterialTransitCompleted {
                received_amount,
                loss_amount,
                ..
            }) => Some((*received_amount, *loss_amount)),
            _ => None,
        })
        .expect("quoted two-hop transfer completed");
    assert_eq!(received_amount, quote.expected_received_amount);
    assert_eq!(loss_amount, quote.expected_loss_amount);
}

#[test]
fn route_capacity_quote_is_infeasible_and_never_mutates_or_allows_action() {
    let mut world = World::new();
    let requester = "route-capacity-operator";
    let source = MaterialLedgerId::site("route-capacity-source");
    let relay = MaterialLedgerId::site("route-capacity-relay");
    let destination = MaterialLedgerId::site("route-capacity-destination");
    world.submit_action(Action::RegisterAgent {
        agent_id: requester.to_string(),
        pos: pos(0, 0),
    });
    world.step().expect("register route capacity operator");
    world
        .set_ledger_material_balance(source.clone(), "iron_ingot", 100)
        .expect("seed route capacity source");

    let route_a = register_route(
        &mut world,
        requester,
        "route-capacity-source",
        "route-capacity-relay",
        "iron_ingot",
        500,
        20,
        0,
    );
    let route_b = register_route(
        &mut world,
        requester,
        "route-capacity-relay",
        "route-capacity-destination",
        "iron_ingot",
        100,
        100,
        0,
    );
    world.submit_action(Action::TransferMaterial {
        requester_agent_id: requester.to_string(),
        from_ledger: source.clone(),
        to_ledger: relay,
        kind: "iron_ingot".to_string(),
        amount: 20,
        distance_km: 0,
        priority: None,
        route_id: None,
        route_ids: vec![route_a.clone()],
        auto_reroute: false,
    });
    world.step().expect("occupy first route capacity");
    assert_eq!(world.pending_material_transits_len(), 1);

    let state_before_quote = world.snapshot();
    let journal_before_quote = world.journal().clone();
    let requested_route_ids = vec![route_a, route_b];
    let quote = world
        .logistics_transfer_quote_with_path(
            requester,
            &source,
            &destination,
            "iron_ingot",
            10,
            0,
            None,
            requested_route_ids.as_slice(),
            false,
        )
        .expect("route-capacity transfer quote");
    let repeat = world
        .logistics_transfer_quote_with_path(
            requester,
            &source,
            &destination,
            "iron_ingot",
            10,
            0,
            None,
            requested_route_ids.as_slice(),
            false,
        )
        .expect("repeat route-capacity transfer quote");

    assert_eq!(quote, repeat);
    assert_eq!(world.snapshot(), state_before_quote);
    assert_eq!(world.journal(), &journal_before_quote);
    assert!(quote.conditional);
    assert!(!quote.submission_feasible);
    assert_eq!(quote.sent_amount, 0);
    assert_eq!(quote.expected_loss_amount, 0);
    assert_eq!(quote.expected_received_amount, 0);
    assert_eq!(quote.source_amount_after, quote.source_amount_before);
    assert_eq!(
        quote.destination_expected_amount_after,
        quote.destination_amount_before
    );
    assert_eq!(quote.recommendation, "wait_for_transit_capacity");
    assert!(quote.inflight_before < quote.inflight_capacity);

    let occupied_route_id = requested_route_ids[0].clone();
    let route_reserved_before_action = world
        .state()
        .logistics_routes
        .get(&occupied_route_id)
        .expect("occupied route state")
        .reserved_capacity_units;
    let journal_before_rejected_action = world.journal().events.len();
    world.submit_action(transfer_over_path(
        requester,
        "route-capacity-source",
        "route-capacity-destination",
        "iron_ingot",
        10,
        0,
        requested_route_ids,
    ));
    world.step().expect("reject route-capacity action");

    assert!(matches!(
        world.journal().events[journal_before_rejected_action..]
            .iter()
            .find_map(|event| match &event.body {
                WorldEventBody::Domain(DomainEvent::ActionRejected { reason, .. }) => {
                    Some(reason)
                }
                _ => None,
            }),
        Some(RejectReason::RuleDenied { notes })
            if notes.iter().any(|note| note.contains("unavailable or at capacity"))
    ));
    assert_eq!(world.pending_material_transits_len(), 1);
    assert_eq!(
        world
            .state()
            .logistics_routes
            .get(&occupied_route_id)
            .map(|route| route.reserved_capacity_units),
        Some(route_reserved_before_action)
    );
    assert_eq!(
        world.ledger_material_balance(&source, "iron_ingot"),
        80,
        "capacity rejection must preserve source balance after the first 20-unit transit"
    );
    assert_eq!(
        world.ledger_material_balance(&destination, "iron_ingot"),
        0,
        "capacity rejection must not credit destination"
    );
    assert!(
        !world.journal().events[journal_before_rejected_action..]
            .iter()
            .any(|event| matches!(
                event.body,
                WorldEventBody::Domain(DomainEvent::MaterialTransitStarted { .. })
            ))
    );
}

#[test]
fn legacy_route_id_tuple_mismatch_is_rejected_by_quote_and_action() {
    let mut world = World::new();
    let requester = "legacy-route-quote-operator";
    let source = MaterialLedgerId::site("legacy-route-quote-source");
    let destination = MaterialLedgerId::site("legacy-route-quote-destination");
    world.submit_action(Action::RegisterAgent {
        agent_id: requester.to_string(),
        pos: pos(0, 0),
    });
    world.step().expect("register legacy route quote operator");
    world
        .set_ledger_material_balance(source.clone(), "iron_ingot", 10)
        .expect("seed legacy route quote source");
    let route_id = register_route(
        &mut world,
        requester,
        "legacy-route-quote-source",
        "legacy-route-quote-destination",
        "iron_ingot",
        100,
        10,
        0,
    );

    let explicit_path_quote = world
        .logistics_transfer_quote_with_path(
            requester,
            &source,
            &destination,
            "iron_ingot",
            1,
            200,
            None,
            std::slice::from_ref(&route_id),
            false,
        )
        .expect("singleton explicit path uses route-derived distance");
    assert_eq!(explicit_path_quote.distance_km, 100);
    assert_eq!(
        explicit_path_quote.route_ids,
        std::slice::from_ref(&route_id)
    );

    let state_before_quote = world.snapshot();
    let quote_reason = world
        .logistics_transfer_quote_with_route_id(
            requester,
            &source,
            &destination,
            "iron_ingot",
            1,
            200,
            None,
            Some(route_id.as_str()),
            &[],
            false,
        )
        .expect_err("legacy route_id tuple mismatch must reject the quote");
    assert!(matches!(
        &quote_reason,
        RejectReason::RuleDenied { notes }
            if notes.iter().any(|note| note.contains("tuple mismatch"))
    ));
    assert_eq!(world.snapshot(), state_before_quote);

    let journal_before_action = world.journal().events.len();
    let source_before_action = world.ledger_material_balance(&source, "iron_ingot");
    world.submit_action(Action::TransferMaterial {
        requester_agent_id: requester.to_string(),
        from_ledger: source.clone(),
        to_ledger: destination.clone(),
        kind: "iron_ingot".to_string(),
        amount: 1,
        distance_km: 200,
        priority: None,
        route_id: Some(route_id),
        route_ids: Vec::new(),
        auto_reroute: false,
    });
    world
        .step()
        .expect("evaluate mismatched legacy route action");
    let action_reason = world.journal().events[journal_before_action..]
        .iter()
        .find_map(|event| match &event.body {
            WorldEventBody::Domain(DomainEvent::ActionRejected { reason, .. }) => {
                Some(reason.clone())
            }
            _ => None,
        })
        .expect("legacy route_id tuple mismatch action rejection");
    assert_eq!(quote_reason, action_reason);
    assert_eq!(
        world.ledger_material_balance(&source, "iron_ingot"),
        source_before_action
    );
    assert_eq!(world.pending_material_transits_len(), 0);
}

#[test]
fn case_and_whitespace_material_kind_keep_quote_and_action_in_parity() {
    let mut world = World::new();
    let requester = "normalized-route-quote-operator";
    let source = MaterialLedgerId::site("normalized-route-quote-source");
    let destination = MaterialLedgerId::site("normalized-route-quote-destination");
    world.submit_action(Action::RegisterAgent {
        agent_id: requester.to_string(),
        pos: pos(0, 0),
    });
    world
        .step()
        .expect("register normalized route quote operator");
    world
        .set_ledger_material_balance(source.clone(), "iron_ingot", 10)
        .expect("seed normalized route quote source");
    let route_id = register_route(
        &mut world,
        requester,
        "normalized-route-quote-source",
        "normalized-route-quote-destination",
        "iron_ingot",
        100,
        10,
        0,
    );
    let request_kind = "  IRON_INGOT  ";
    let route_ids = vec![route_id];
    let state_before_quote = world.snapshot();
    let quote = world
        .logistics_transfer_quote_with_path(
            requester,
            &source,
            &destination,
            request_kind,
            2,
            0,
            None,
            route_ids.as_slice(),
            false,
        )
        .expect("case and whitespace variant route quote");
    assert_eq!(world.snapshot(), state_before_quote);
    assert_eq!(quote.kind, "iron_ingot");
    assert!(quote.submission_feasible);
    assert_eq!(quote.max_transferable_amount, 10);
    assert_eq!(quote.sent_amount, 2);

    world.submit_action(Action::TransferMaterial {
        requester_agent_id: requester.to_string(),
        from_ledger: source.clone(),
        to_ledger: destination,
        kind: request_kind.to_string(),
        amount: 2,
        distance_km: 0,
        priority: None,
        route_id: None,
        route_ids,
        auto_reroute: false,
    });
    world
        .step()
        .expect("start normalized route transfer action");
    let started_kind = world
        .journal()
        .events
        .iter()
        .rev()
        .find_map(|event| match &event.body {
            WorldEventBody::Domain(DomainEvent::MaterialTransitStarted { kind, .. }) => {
                Some(kind.clone())
            }
            _ => None,
        })
        .expect("normalized route transfer started");
    assert_eq!(started_kind, quote.kind);
    assert_eq!(world.ledger_material_balance(&source, "iron_ingot"), 8);
}

#[test]
fn malformed_or_disconnected_paths_precede_capacity_fallback() {
    let mut world = World::new();
    let requester = "path-precedence-operator";
    let source = MaterialLedgerId::site("path-precedence-source");
    let _relay = MaterialLedgerId::site("path-precedence-relay");
    let destination = MaterialLedgerId::site("path-precedence-destination");
    world.submit_action(Action::RegisterAgent {
        agent_id: requester.to_string(),
        pos: pos(0, 0),
    });
    world.step().expect("register path precedence operator");
    world
        .set_ledger_material_balance(source.clone(), "iron_ingot", 10)
        .expect("seed path precedence source");
    let unavailable_route = register_route(
        &mut world,
        requester,
        "path-precedence-source",
        "path-precedence-relay",
        "iron_ingot",
        100,
        10,
        0,
    );
    let next_route = register_route(
        &mut world,
        requester,
        "path-precedence-relay",
        "path-precedence-destination",
        "iron_ingot",
        100,
        10,
        0,
    );
    let disconnected_route = register_route(
        &mut world,
        requester,
        "path-precedence-other-source",
        "path-precedence-destination",
        "iron_ingot",
        100,
        10,
        0,
    );
    world.submit_action(Action::SetLogisticsRouteAvailability {
        requester_agent_id: requester.to_string(),
        route_id: unavailable_route.clone(),
        available: false,
    });
    world.step().expect("disable path precedence route");

    let valid_capacity_path = vec![unavailable_route.clone(), next_route];
    let state_before_valid_capacity_quote = world.snapshot();
    let valid_capacity_quote = world
        .logistics_transfer_quote_with_path(
            requester,
            &source,
            &destination,
            "iron_ingot",
            1,
            0,
            None,
            valid_capacity_path.as_slice(),
            false,
        )
        .expect("valid unavailable path is a conditional capacity quote");
    assert!(!valid_capacity_quote.submission_feasible);
    assert_eq!(valid_capacity_quote.recommendation, "route_unavailable");
    assert_eq!(valid_capacity_quote.max_transferable_amount, 0);
    assert_eq!(valid_capacity_quote.route_ids, valid_capacity_path);
    assert!(valid_capacity_quote.path_id.is_some());
    assert_eq!(valid_capacity_quote.tariff_electricity_total, 0);
    assert_eq!(valid_capacity_quote.reroute_count, 0);
    assert_eq!(world.snapshot(), state_before_valid_capacity_quote);

    let combined_blocker_quote = world
        .logistics_transfer_quote_with_path(
            requester,
            &source,
            &destination,
            "iron_ingot",
            20,
            0,
            None,
            valid_capacity_path.as_slice(),
            false,
        )
        .expect("closed route plus source shortage remains a conditional quote");
    assert_eq!(combined_blocker_quote.recommendation, "route_unavailable");
    assert_eq!(combined_blocker_quote.max_transferable_amount, 0);

    for (label, route_ids) in [
        (
            "malformed path",
            vec![
                unavailable_route.clone(),
                "missing-logistics-route".to_string(),
            ],
        ),
        (
            "disconnected path",
            vec![unavailable_route, disconnected_route],
        ),
    ] {
        let state_before_invalid_quote = world.snapshot();
        let reason = world
            .logistics_transfer_quote_with_path(
                requester,
                &source,
                &destination,
                "iron_ingot",
                1,
                0,
                None,
                route_ids.as_slice(),
                false,
            )
            .expect_err(label);
        assert!(matches!(
            reason,
            RejectReason::RuleDenied { ref notes }
                if notes.iter().any(|note| note.contains("cyclic, disconnected, or incompatible"))
        ));
        assert_eq!(world.snapshot(), state_before_invalid_quote);
    }

    let long_relay = "path-precedence-long-relay";
    let long_first = register_route(
        &mut world,
        requester,
        "path-precedence-source",
        long_relay,
        "iron_ingot",
        6_000,
        100,
        0,
    );
    let long_second = register_route(
        &mut world,
        requester,
        long_relay,
        "path-precedence-destination",
        "iron_ingot",
        6_000,
        100,
        0,
    );
    world.submit_action(Action::SetLogisticsRouteAvailability {
        requester_agent_id: requester.to_string(),
        route_id: long_first.clone(),
        available: false,
    });
    world.step().expect("disable over-limit path route");

    let state_before_over_limit_quote = world.snapshot();
    let reason = world
        .logistics_transfer_quote_with_path(
            requester,
            &source,
            &destination,
            "iron_ingot",
            1,
            0,
            None,
            &[long_first, long_second],
            false,
        )
        .expect_err("blocked over-limit path must preserve the action resolver rejection");
    assert!(matches!(
        &reason,
        RejectReason::RuleDenied { notes }
            if notes.iter().any(|note| note.contains("unavailable or at capacity"))
    ));
    assert_eq!(world.snapshot(), state_before_over_limit_quote);
}

#[test]
fn auto_reroute_preserves_over_limit_alternate_rejection() {
    let mut world = World::new();
    let requester = "over-limit-reroute-operator";
    let source = MaterialLedgerId::site("over-limit-reroute-source");
    let destination = MaterialLedgerId::site("over-limit-reroute-destination");
    world.submit_action(Action::RegisterAgent {
        agent_id: requester.to_string(),
        pos: pos(0, 0),
    });
    world.step().expect("register reroute operator");
    world
        .set_ledger_material_balance(source.clone(), "iron_ingot", 10)
        .expect("seed reroute source");

    let blocked_route = register_route(
        &mut world,
        requester,
        "over-limit-reroute-source",
        "over-limit-reroute-destination",
        "iron_ingot",
        100,
        100,
        0,
    );
    register_route(
        &mut world,
        requester,
        "over-limit-reroute-source",
        "over-limit-reroute-relay",
        "iron_ingot",
        6_000,
        100,
        0,
    );
    register_route(
        &mut world,
        requester,
        "over-limit-reroute-relay",
        "over-limit-reroute-destination",
        "iron_ingot",
        6_000,
        100,
        0,
    );
    world.submit_action(Action::SetLogisticsRouteAvailability {
        requester_agent_id: requester.to_string(),
        route_id: blocked_route.clone(),
        available: false,
    });
    world.step().expect("disable requested route");

    let state_before_quote = world.snapshot();
    let reason = world
        .logistics_transfer_quote_with_path(
            requester,
            &source,
            &destination,
            "iron_ingot",
            1,
            0,
            None,
            &[blocked_route],
            true,
        )
        .expect_err("over-limit reroute error must not become a blocked-path quote");
    assert!(matches!(
        reason,
        RejectReason::MaterialTransferDistanceExceeded {
            distance_km: 12_000,
            max_distance_km: 10_000
        }
    ));
    assert_eq!(world.snapshot(), state_before_quote);
}
