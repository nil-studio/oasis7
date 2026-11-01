//! Typed Node-side fixture for Core protocol tests, not a Runtime grant installation.
use super::*;
use resource::*;
pub(super) fn transfer_fact(
    q: &OwnerSetupFinalResourceReadV2,
    owner: &ResourceOwnerStateV1,
) -> ResourceBasisFactV1 {
    let ResourceBasisSelectionV1::Transfer {
        from,
        to,
        material,
        amount,
        distance_km,
        priority,
        ..
    } = &q.selection
    else {
        panic!("transfer fixture")
    };
    let actor = ResourceActorOwnerV1 {
        player_id: owner.owner.player_id.clone(),
        source_id: "fixture-source".into(),
        public_key: Some(owner.owner.public_key.clone()),
        generation: owner.owner.agent_identity_generation,
    };
    let agent = owner.owner.agent_id.clone();
    let object = format!("{}->{}:{material}", from.reference(), to.reference());
    let grant = ResourceGrantV1 {
        grant_id: "fixture-grant".into(),
        source_id: actor.source_id.clone(),
        issuer_id: "fixture-issuer".into(),
        owner_id: actor.player_id.clone(),
        organization_id: None,
        agent_id: agent.clone(),
        object_id: object,
        action_kinds: vec!["TransferMaterial".into()],
        revision: DecimalU64(1),
        period_id: "period-one".into(),
        valid_from_tick: DecimalU64(0),
        valid_until_tick: DecimalU64(1000),
        limit_units: DecimalU64(100),
        resource_kind: "electricity".into(),
        revoked: false,
    };
    let action = ResourceQuoteActionV1::TransferMaterial {
        requester_agent_id: agent.clone(),
        from_ledger: from.clone(),
        to_ledger: to.clone(),
        kind: material.clone(),
        amount: *amount,
        distance_km: *distance_km,
        priority: Some(*priority),
        route_id: None,
        route_ids: vec![],
        auto_reroute: false,
    };
    let quote = ResourceLogisticsTransferQuoteV1 {
        requester_agent_id: agent.clone(),
        from_ledger: from.clone(),
        to_ledger: to.clone(),
        kind: material.clone(),
        requested_amount: *amount,
        submission_feasible: true,
        max_transferable_amount: ResourceDecimalI64V1(20),
        sent_amount: *amount,
        distance_km: *distance_km,
        loss_bps: ResourceDecimalI64V1(0),
        expected_loss_amount: ResourceDecimalI64V1(0),
        expected_received_amount: *amount,
        source_amount_before: ResourceDecimalI64V1(20),
        source_amount_after: ResourceDecimalI64V1(20 - amount.0),
        destination_amount_before: ResourceDecimalI64V1(0),
        destination_expected_amount_after: *amount,
        ticks_until_arrival: DecimalU64(1),
        ready_at: DecimalU64(owner.logical_tick.0.saturating_add(1).saturating_add(1)),
        effective_priority: *priority,
        priority_reason: "fixture standard".into(),
        inflight_before: DecimalU64(0),
        inflight_capacity: DecimalU64(64),
        path_id: None,
        route_ids: vec![],
        tariff_electricity_total: ResourceDecimalI64V1(0),
        reroute_count: 0,
        recommendation: "fixture accepted".into(),
        conditional: false,
    };
    let ResourceLedgerV1::Site { site_id } = to else {
        panic!("site fixture")
    };
    let ledgers = vec![
        ResourceLedgerFactV1 {
            ledger: from.clone(),
            owner: ResourceLedgerOwnerV1::Agent {
                agent_id: agent.clone(),
                owner: actor.clone(),
            },
            exists: true,
            materials: vec![ResourceMaterialBalanceV1 {
                material: material.clone(),
                amount: Some(ResourceDecimalI64V1(20)),
            }],
        },
        ResourceLedgerFactV1 {
            ledger: to.clone(),
            owner: ResourceLedgerOwnerV1::Site {
                site_id: site_id.clone(),
                owner_agent_id: agent.clone(),
                authority_revision: DecimalU64(1),
            },
            exists: true,
            materials: vec![ResourceMaterialBalanceV1 {
                material: material.clone(),
                amount: Some(ResourceDecimalI64V1(0)),
            }],
        },
    ];
    let refs =
        std::collections::BTreeSet::from([from.reference(), to.reference(), material.clone()])
            .into_iter()
            .collect();
    ResourceBasisFactV1 {
        agent_id: agent,
        actor_owner: actor,
        quoted_at_tick: owner.logical_tick,
        actor_electricity_before: ResourceDecimalI64V1(100),
        selection: q.selection.clone(),
        quote: ResourceBasisQuoteV1::Transfer(Box::new(ResourceTransferQuoteV1 {
            action,
            authorization: ResourceAuthorizationV1 {
                grant,
                cost_units: DecimalU64(1),
                spent_units: DecimalU64(0),
                remaining_units: DecimalU64(100),
            },
            quote,
        })),
        ledgers,
        objects: vec![],
        derived_resource_refs: refs,
    }
}

pub(super) fn response(
    s: &AccountProvisionApprovalState,
    c: &CanonicalState,
    q: &SignedResourceReadV1<OwnerSetupFinalResourceReadV2>,
    ready: bool,
) -> SignedResourceResponseV1<OwnerSetupFinalResourceObservationV2> {
    let f = &q.request.expected_fence;
    let owner = ResourceOwnerStateV1 {
        schema_revision: 1,
        world: q.request.world.clone(),
        read_commit: (&c.commit).into(),
        logical_tick: DecimalU64(111),
        owner: ResourceOwnerFenceV1 {
            agent_id: f.agent_id.clone(),
            player_id: f.owner_binding.clone(),
            public_key: f.owner_public_key.clone(),
            world_id: f.world_id.clone(),
            reorg_epoch: f.reorg_epoch,
            branch_id: f.branch_id.clone(),
            agent_identity_generation: f.owner_generation,
            current_intent_id: None,
        },
        control: c.task.clone().unwrap(),
        signer_delegation: Some(q.request.expected_signer_delegation.clone()),
    };
    let OwnerSetupResultV1::PreparedGoal(prepared) = s
        .owner_setup_outbox("setup-one", OwnerSetupStageV1::PrepareGoal)
        .unwrap()
        .result
        .as_ref()
        .unwrap()
    else {
        panic!("actual prepared goal")
    };
    let OwnerSetupResultV1::Canonical { response, .. } = s
        .owner_setup_outbox("setup-one", OwnerSetupStageV1::PublishGoal)
        .unwrap()
        .result
        .as_ref()
        .unwrap()
    else {
        panic!("actual published goal")
    };
    let ws::IntentOutcome::Committed { commit, .. } = &response.payload.outcome else {
        panic!("actual committed publish")
    };
    let current_goal = ResourceCurrentGoalV2 {
        schema_revision: 1,
        control: c.task.clone().unwrap(),
        read_commit: (&c.commit).into(),
        activation_commit: Some(commit.as_ref().into()),
        active_goal: Some(prepared.payload.clone()),
    };
    let resource = if ready {
        ResourceBasisResolutionV1::Ready {
            fact: Box::new(transfer_fact(&q.request, &owner)),
        }
    } else {
        ResourceBasisResolutionV1::Blocked {
            reason: ResourceBasisBlockerV1::AdmissionUnavailable,
        }
    };
    let payload = OwnerSetupFinalResourceObservationV2 {
        api_version: 2,
        owner,
        current_goal,
        selection: q.request.selection.clone(),
        resource,
    };
    ws::sign_service_response(
        RESOURCE_GOAL_PATH_V2,
        ws::request_digest(RESOURCE_GOAL_PATH_V2, q).unwrap(),
        payload,
        &"3f".repeat(32),
    )
    .unwrap()
    .into()
}
