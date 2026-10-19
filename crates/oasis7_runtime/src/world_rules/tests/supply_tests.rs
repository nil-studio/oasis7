use super::*;

#[test]
fn supply_freezes_allocates_and_revokes_without_value_duplication() {
    let mut state = fixture();
    state.crises.insert(
        "crisis".into(),
        crate::CrisisState {
            crisis_id: "crisis".into(),
            kind: "systemic_shortage".into(),
            severity: 3,
            status: crate::CrisisStatus::Active,
            opened_at: 0,
            expires_at: 1000,
            resolver_agent_id: None,
            strategy: None,
            success: None,
            impact: 100,
            resolved_at: None,
        },
    );
    let auth = WorldRuleCommandV1::AuthorizeSupply {
        proposal_key: "approval".into(),
        crisis_id: "crisis".into(),
        material: "alloy_plate".into(),
        beneficiaries: ["primary".into(), "recipient".into()].into(),
        budget: 9,
        expires_at: 200,
    };
    assert!(PreparedWorldRule::prepare(&state, &event("owner", "auth", auth.clone()), 10).is_err());
    approve(&mut state, "owner", "approval", &auth);
    apply(&mut state, &event("owner", "auth", auth), 10);
    let id = request_key("owner", "auth");
    for actor in ["primary", "recipient"] {
        apply(
            &mut state,
            &event(
                actor,
                "demand",
                WorldRuleCommandV1::RequestSupply {
                    authorization_id: id.clone(),
                    cumulative_demand: 10,
                },
            ),
            12,
        );
    }
    apply(
        &mut state,
        &event(
            "owner",
            "freeze",
            WorldRuleCommandV1::FreezeSupply {
                authorization_id: id.clone(),
            },
        ),
        20,
    );
    let batch_id = request_key("owner", "freeze");
    apply(
        &mut state,
        &event(
            "owner",
            "allocate",
            WorldRuleCommandV1::AllocateSupply {
                batch_id: batch_id.clone(),
            },
        ),
        21,
    );
    let allocation = &state.world_rules.as_ref().unwrap().supply_batches[&batch_id].allocation;
    assert_eq!(allocation.values().sum::<i64>(), 9);
    assert_eq!(allocation["primary"], 5);
    apply(
        &mut state,
        &event(
            "owner",
            "revoke",
            WorldRuleCommandV1::CloseSupply {
                authorization_id: id,
            },
        ),
        22,
    );
    assert_eq!(
        state.material_ledgers[&MaterialLedgerId::agent("owner")]["alloy_plate"],
        120
    );
    assert!(
        state
            .world_rules
            .as_ref()
            .unwrap()
            .supply_lineages
            .values()
            .all(|l| l.reserved == 0 && l.fulfilled == 0)
    );
}

#[test]
fn supply_dispatch_uses_real_logistics_then_settles_after_expiry() {
    let mut state = fixture();
    state.crises.insert(
        "crisis".into(),
        crate::CrisisState {
            crisis_id: "crisis".into(),
            kind: "systemic_shortage".into(),
            severity: 3,
            status: crate::CrisisStatus::Active,
            opened_at: 0,
            expires_at: 1000,
            resolver_agent_id: None,
            strategy: None,
            success: None,
            impact: 100,
            resolved_at: None,
        },
    );
    let auth = WorldRuleCommandV1::AuthorizeSupply {
        proposal_key: "approval".into(),
        crisis_id: "crisis".into(),
        material: "alloy_plate".into(),
        beneficiaries: ["recipient".into()].into(),
        budget: 10,
        expires_at: 40,
    };
    approve(&mut state, "owner", "approval", &auth);
    apply(&mut state, &event("owner", "auth", auth), 10);
    let auth_id = request_key("owner", "auth");
    apply(
        &mut state,
        &event(
            "recipient",
            "request",
            WorldRuleCommandV1::RequestSupply {
                authorization_id: auth_id.clone(),
                cumulative_demand: 10,
            },
        ),
        11,
    );
    apply(
        &mut state,
        &event(
            "owner",
            "freeze",
            WorldRuleCommandV1::FreezeSupply {
                authorization_id: auth_id.clone(),
            },
        ),
        20,
    );
    let batch = request_key("owner", "freeze");
    apply(
        &mut state,
        &event(
            "owner",
            "allocate",
            WorldRuleCommandV1::AllocateSupply {
                batch_id: batch.clone(),
            },
        ),
        21,
    );
    state.logistics_routes.insert(
        "route".into(),
        crate::LogisticsRouteV1 {
            route_id: "route".into(),
            from_ledger: MaterialLedgerId::agent("owner"),
            to_ledger: MaterialLedgerId::agent("recipient"),
            kind: "alloy_plate".into(),
            distance_km: 2,
            priority: crate::MaterialTransitPriority::Standard,
            owner_agent_id: "owner".into(),
            available: true,
            capacity_units: 5,
            reserved_capacity_units: 0,
            tariff_electricity_per_unit: 0,
        },
    );
    let dispatch = event(
        "owner",
        "dispatch",
        WorldRuleCommandV1::DispatchSupply {
            batch_id: batch.clone(),
            beneficiary_id: "recipient".into(),
            amount: 6,
            route_id: "route".into(),
        },
    );
    assert!(PreparedWorldRule::prepare(&state, &dispatch, 22).is_err());
    let dispatch = event(
        "owner",
        "dispatch",
        WorldRuleCommandV1::DispatchSupply {
            batch_id: batch,
            beneficiary_id: "recipient".into(),
            amount: 5,
            route_id: "route".into(),
        },
    );
    apply(&mut state, &dispatch, 22);
    assert_eq!(
        state.material_ledgers[&MaterialLedgerId::agent("owner")]["alloy_plate"],
        110
    );
    assert_eq!(state.logistics_routes["route"].reserved_capacity_units, 5);
    let extra = state.pending_material_transits[&1_000_000].clone();
    state.pending_material_transits.insert(999_999, extra);
    let batch_id = request_key("owner", "freeze");
    let mut blocked = event(
        "owner",
        "capacity",
        WorldRuleCommandV1::DispatchSupply {
            batch_id,
            beneficiary_id: "recipient".into(),
            amount: 1,
            route_id: "route".into(),
        },
    );
    if let DomainEvent::WorldRuleCommitted { action_id, .. } = &mut blocked {
        *action_id = 1_000_001;
    }
    assert!(
        format!(
            "{:?}",
            PreparedWorldRule::prepare(&state, &blocked, 23).unwrap_err()
        )
        .contains("material_transit_capacity_exceeded")
    );
    state.pending_material_transits.remove(&999_999);

    assert!(
        !state
            .material_ledgers
            .contains_key(&MaterialLedgerId::agent("recipient"))
    );
    apply(
        &mut state,
        &event(
            "owner",
            "exit",
            WorldRuleCommandV1::CloseSupply {
                authorization_id: auth_id,
            },
        ),
        40,
    );
    assert_eq!(
        state.material_ledgers[&MaterialLedgerId::agent("owner")]["alloy_plate"],
        115
    );
    let job = state.pending_material_transits[&1_000_000].clone();
    let shipment = state.world_rules.as_ref().unwrap().supply_shipments[&job.job_id].clone();
    let complete = DomainEvent::MaterialTransitCompleted {
        job_id: job.job_id,
        requester_agent_id: job.requester_agent_id,
        from_ledger: job.from_ledger,
        to_ledger: job.to_ledger,
        kind: job.kind,
        sent_amount: job.amount,
        received_amount: shipment.delivered_amount,
        loss_amount: job.amount - shipment.delivered_amount,
        distance_km: job.distance_km,
        priority: job.priority,
        route_id: job.route_id,
        path_id: job.path_id,
        route_ids: job.route_ids,
        tariff_electricity_total: job.tariff_electricity_total,
        reroute_count: job.reroute_count,
    };
    state.apply_domain_event(&complete, 41).unwrap();
    state.route_domain_event(&complete);
    let settle = event(
        "recipient",
        "settle",
        WorldRuleCommandV1::SettleSupply { job_id: job.job_id },
    );
    apply(&mut state, &settle, 41);
    apply(&mut state, &settle, 42);
    let ledger = state
        .world_rules
        .as_ref()
        .unwrap()
        .supply_lineages
        .values()
        .next()
        .unwrap();
    assert_eq!(
        (ledger.fulfilled, ledger.reserved),
        (shipment.delivered_amount, 0)
    );
    assert_eq!(state.logistics_routes["route"].reserved_capacity_units, 0);
    assert_eq!(
        state.material_ledgers[&MaterialLedgerId::agent("recipient")]["alloy_plate"],
        shipment.delivered_amount
    );
    let restored: WorldState =
        serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
    assert_eq!(restored, state);
}

#[test]
fn supply_private_demands_are_scoped_to_beneficiary_or_provider() {
    let mut state = fixture();
    state.crises.insert(
        "crisis".into(),
        crate::CrisisState {
            crisis_id: "crisis".into(),
            kind: "systemic_shortage".into(),
            severity: 3,
            status: crate::CrisisStatus::Active,
            opened_at: 0,
            expires_at: 1000,
            resolver_agent_id: None,
            strategy: None,
            success: None,
            impact: 100,
            resolved_at: None,
        },
    );
    let auth = WorldRuleCommandV1::AuthorizeSupply {
        proposal_key: "approval".into(),
        crisis_id: "crisis".into(),
        material: "alloy_plate".into(),
        beneficiaries: ["primary".into(), "recipient".into()].into(),
        budget: 10,
        expires_at: 200,
    };
    approve(&mut state, "owner", "approval", &auth);
    apply(&mut state, &event("owner", "auth", auth), 10);
    let id = request_key("owner", "auth");
    apply(
        &mut state,
        &event(
            "recipient",
            "demand",
            WorldRuleCommandV1::RequestSupply {
                authorization_id: id.clone(),
                cumulative_demand: 10,
            },
        ),
        11,
    );
    let rules = state.world_rules.as_ref().unwrap();
    assert!(
        rules.for_reader(None).supply_authorizations[&id]
            .requests
            .is_empty()
    );
    assert!(rules.for_reader(Some("primary")).supply_lineages.is_empty());
    assert_eq!(
        rules.for_reader(Some("recipient")).supply_authorizations[&id].requests["recipient"],
        10
    );
    assert_eq!(
        rules.for_reader(Some("owner")).supply_authorizations[&id].requests["recipient"],
        10
    );
}

#[test]
fn supply_approval_cannot_be_reused_and_evidence_drift_closes_scope() {
    let mut state = fixture();
    state.crises.insert(
        "crisis".into(),
        crate::CrisisState {
            crisis_id: "crisis".into(),
            kind: "systemic_shortage".into(),
            severity: 3,
            status: crate::CrisisStatus::Active,
            opened_at: 0,
            expires_at: 1000,
            resolver_agent_id: None,
            strategy: None,
            success: None,
            impact: 100,
            resolved_at: None,
        },
    );
    let command = WorldRuleCommandV1::AuthorizeSupply {
        proposal_key: "approval".into(),
        crisis_id: "crisis".into(),
        material: "alloy_plate".into(),
        beneficiaries: ["recipient".into()].into(),
        budget: 10,
        expires_at: 200,
    };
    approve(&mut state, "owner", "approval", &command);
    apply(&mut state, &event("owner", "auth", command.clone()), 10);
    assert!(
        PreparedWorldRule::prepare(&state, &event("owner", "duplicate-vote", command), 11).is_err()
    );
    state.crises.get_mut("crisis").unwrap().severity = 1;
    let id = request_key("owner", "auth");
    assert!(
        PreparedWorldRule::prepare(
            &state,
            &event(
                "recipient",
                "demand",
                WorldRuleCommandV1::RequestSupply {
                    authorization_id: id.clone(),
                    cumulative_demand: 10
                }
            ),
            12
        )
        .is_err()
    );
    apply(
        &mut state,
        &event(
            "owner",
            "release",
            WorldRuleCommandV1::CloseSupply {
                authorization_id: id,
            },
        ),
        12,
    );
    assert_eq!(
        state.material_ledgers[&MaterialLedgerId::agent("owner")]["alloy_plate"],
        120
    );
}
