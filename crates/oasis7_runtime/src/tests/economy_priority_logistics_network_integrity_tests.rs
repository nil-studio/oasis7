#[test]
fn industrial_integrity_early_material_transit_completion_fails_closed_before_settlement() {
    let world = started_logistics_network_transit_fixture();
    let pending = world
        .state()
        .pending_material_transits
        .values()
        .next()
        .expect("pending transit job")
        .clone();
    assert_eq!(pending.amount, 1, "fixture uses one material unit");
    let early_at = pending
        .ready_at
        .checked_sub(1)
        .expect("fixture transit has a positive ready_at");
    let event = DomainEvent::MaterialTransitCompleted {
        job_id: pending.job_id,
        requester_agent_id: pending.requester_agent_id.clone(),
        from_ledger: pending.from_ledger.clone(),
        to_ledger: pending.to_ledger.clone(),
        kind: pending.kind.clone(),
        sent_amount: 1,
        received_amount: 1,
        loss_amount: 0,
        distance_km: pending.distance_km,
        priority: pending.priority,
        route_id: pending.route_id.clone(),
        path_id: pending.path_id.clone(),
        route_ids: pending.route_ids.clone(),
        tariff_electricity_total: pending.tariff_electricity_total,
        reroute_count: pending.reroute_count,
    };
    let mut replay = world.state().clone();
    let before = serde_json::to_vec(&replay).expect("serialize state before early completion");

    let result = replay.apply_domain_event(&event, early_at);
    assert!(
        matches!(
            result,
            Err(crate::WorldError::ResourceBalanceInvalid { reason })
                if reason.contains("material transit completion is early")
        ),
        "an otherwise valid completion before ready_at must fail closed"
    );
    assert_eq!(
        serde_json::to_vec(&replay).expect("serialize state after early completion"),
        before,
        "early completion must not credit, settle, release capacity, or advance progress"
    );

    replay
        .apply_domain_event(&event, pending.ready_at)
        .expect("complete transit at ready_at");
    assert!(!replay.pending_material_transits.contains_key(&pending.job_id));
    assert!(replay.settled_logistics_transit_ids.contains(&pending.job_id));
    assert_eq!(
        replay
            .material_ledgers
            .get(&pending.to_ledger)
            .and_then(|ledger| ledger.get(&pending.kind))
            .copied(),
        Some(1)
    );

    let settled = serde_json::to_vec(&replay).expect("serialize settled transit state");
    replay
        .apply_domain_event(&event, pending.ready_at.saturating_add(1))
        .expect("replay settled transit completion");
    assert_eq!(
        serde_json::to_vec(&replay).expect("serialize replayed transit state"),
        settled,
        "settled completion replay must remain a byte-stable no-op"
    );
}
