use std::collections::BTreeMap;

use super::{
    CognitionEconomyOutput, MonetaryDomainDelta, StagedBootstrapCatalogDefaults,
    StagedCognitionEconomyOp, StagedRuleFeeSettlement, plan_resource_delta,
};
use crate::world::event_processing::publication::staged_monetary::StagedMonetaryPublication;
use crate::world::transition::{
    BorrowedWorldView, DurableCommitConfirmation, StagedRootTransaction, StagedTransition,
    TransitionContext, TransitionReject,
};
use crate::{
    Action, CognitionLeaseQuoteV1, CognitionLeaseRequestV1, DomainEvent, ModuleRuntimeChargeEvent,
    ResourceDelta, World, WorldError, WorldEventBody,
};
use oasis7_agent_api::world_action::{GeoPos, ResourceKind};
use oasis7_domain_types::rule_settlement::RuleActionDisposition;

fn costs() -> (ResourceDelta, ResourceDelta) {
    (
        ResourceDelta {
            entries: BTreeMap::from([(ResourceKind::Data, -2)]),
        },
        ResourceDelta {
            entries: BTreeMap::from([
                (ResourceKind::Data, -2),
                (ResourceKind::Electricity, -4),
            ]),
        },
    )
}

fn fee<'a>(
    general: &'a ResourceDelta,
    with_distance: &'a ResourceDelta,
    disposition: RuleActionDisposition,
) -> StagedRuleFeeSettlement<'a> {
    StagedRuleFeeSettlement {
        general,
        with_distance,
        disposition,
    }
}

#[test]
fn staged_rule_fee_keeps_rejected_then_accepted_movement_scope_on_one_real_world() {
    let mut world = World::new();
    world.set_resource_balance(ResourceKind::Data, 12);
    world.set_resource_balance(ResourceKind::Electricity, 8);
    let before_journal = world.journal().clone();

    // First nonempty fee: denied movement only pays the original general
    // attempt cost. The Stage 1 view must not change canonical memory.
    let (general, with_distance) = costs();
    let rejected = fee(
        &general,
        &with_distance,
        RuleActionDisposition::RejectedMovement,
    );
    let prepared = {
        let view = BorrowedWorldView::new(&world).unwrap();
        rejected
            .prepare(&view, &TransitionContext::new(view.time(), None))
            .unwrap()
    };
    assert_eq!(world.resource_balance(ResourceKind::Data), 12);
    assert_eq!(world.resource_balance(ResourceKind::Electricity), 8);
    assert_eq!(world.journal(), &before_journal);

    // A simulated indeterminate durable ack must fence without installing.
    assert!(matches!(
        prepared.install_confirmed(
            &mut world,
            DurableCommitConfirmation::StatusUnknown {
                reason: "lost staged publication ack".into(),
            },
        ),
        Err(TransitionReject::StatusUnknownFenced { .. })
    ));
    assert_eq!(world.resource_balance(ResourceKind::Data), 12);
    assert_eq!(world.resource_balance(ResourceKind::Electricity), 8);

    let prepared = {
        let view = BorrowedWorldView::new(&world).unwrap();
        rejected
            .prepare(&view, &TransitionContext::new(view.time(), None))
            .unwrap()
    };
    prepared
        .install_confirmed(&mut world, DurableCommitConfirmation::Confirmed)
        .unwrap();
    assert_eq!(world.resource_balance(ResourceKind::Data), 10);
    assert_eq!(world.resource_balance(ResourceKind::Electricity), 8);

    // The next accepted fee uses the already-priced total exactly once,
    // borrowing the same canonical world rather than creating a shadow pool.
    let accepted = fee(&general, &with_distance, RuleActionDisposition::Accepted);
    let prepared = {
        let view = BorrowedWorldView::new(&world).unwrap();
        accepted
            .prepare(&view, &TransitionContext::new(view.time(), None))
            .unwrap()
    };
    prepared
        .install_confirmed(&mut world, DurableCommitConfirmation::Confirmed)
        .unwrap();
    assert_eq!(world.resource_balance(ResourceKind::Data), 8);
    assert_eq!(world.resource_balance(ResourceKind::Electricity), 4);
    // Fee staging alone never fabricates a business event or durable receipt.
    assert_eq!(world.journal(), &before_journal);
}

#[test]
fn staged_fee_rejects_missing_balance_without_charge_or_publication() {
    let mut world = World::new();
    world.set_resource_balance(ResourceKind::Data, 1);
    let before_journal = world.journal().clone();
    let (general, with_distance) = costs();
    let transition = fee(
        &general,
        &with_distance,
        RuleActionDisposition::RejectedMovement,
    );
    let error = {
        let view = BorrowedWorldView::new(&world).unwrap();
        transition
            .prepare(&view, &TransitionContext::default())
            .err()
            .expect("insufficient general fee")
    };
    assert!(matches!(
        error,
        TransitionReject::World(WorldError::ResourceBalanceInvalid { .. })
    ));
    assert_eq!(world.resource_balance(ResourceKind::Data), 1);
    assert_eq!(world.journal(), &before_journal);
}

#[test]
fn staged_fee_overflow_preserves_original_checked_arithmetic_and_all_balances() {
    let mut world = World::new();
    world.set_resource_balance(ResourceKind::Data, i64::MAX);
    world.set_resource_balance(ResourceKind::Electricity, 5);
    let general = ResourceDelta {
        entries: BTreeMap::from([(ResourceKind::Data, 1)]),
    };
    let with_distance = ResourceDelta::default();
    let transition = fee(
        &general,
        &with_distance,
        RuleActionDisposition::RejectedMovement,
    );
    let error = {
        let view = BorrowedWorldView::new(&world).unwrap();
        transition
            .prepare(&view, &TransitionContext::default())
            .err()
            .expect("positive overflow must be rejected before Stage 3")
    };
    assert!(matches!(
        error,
        TransitionReject::World(WorldError::ResourceBalanceInvalid { .. })
    ));
    assert_eq!(world.resource_balance(ResourceKind::Data), i64::MAX);
    assert_eq!(world.resource_balance(ResourceKind::Electricity), 5);

    let plan = plan_resource_delta(
        &BTreeMap::from([(ResourceKind::Data, i64::MAX)]),
        transition.general,
    );
    assert!(matches!(
        plan,
        Err(WorldError::ResourceBalanceInvalid { .. })
    ));
}

#[test]
fn staged_fee_rejects_stale_world_head_after_prepare() {
    let mut world = World::new();
    world.set_resource_balance(ResourceKind::Data, 12);
    let (general, with_distance) = costs();
    let prepared = {
        let view = BorrowedWorldView::new(&world).unwrap();
        fee(&general, &with_distance, RuleActionDisposition::RejectedMovement)
            .prepare(&view, &TransitionContext::default())
            .unwrap()
    };
    world.set_resource_balance(ResourceKind::Data, 11);
    assert!(matches!(
        prepared.install_confirmed(&mut world, DurableCommitConfirmation::Confirmed),
        Err(TransitionReject::BaseHeadStale)
    ));
    assert_eq!(world.resource_balance(ResourceKind::Data), 11);
}

#[test]
fn live_and_staged_rule_fee_share_selected_affordability_before_arithmetic() {
    let mut world = World::new();
    world.set_resource_balance(ResourceKind::Data, 2);
    world.set_resource_balance(ResourceKind::Electricity, 0);
    let (general, with_distance) = costs();
    let denied = fee(
        &general,
        &with_distance,
        RuleActionDisposition::RejectedMovement,
    );
    let accepted = fee(&general, &with_distance, RuleActionDisposition::Accepted);

    assert_eq!(
        denied.plan_checked(&world.state().resources).unwrap(),
        vec![(ResourceKind::Data, 0)]
    );
    let live_error = accepted
        .plan_checked(&world.state().resources)
        .unwrap_err();
    let staged_error = {
        let view = BorrowedWorldView::new(&world).unwrap();
        accepted
            .prepare(&view, &TransitionContext::default())
            .err()
            .expect("distance-only deficit must deny accepted movement")
    };
    let TransitionReject::World(staged_error) = staged_error else {
        panic!("unexpected staged rejection");
    };
    assert_eq!(format!("{live_error:?}"), format!("{staged_error:?}"));
    assert_eq!(world.resource_balance(ResourceKind::Data), 2);
    assert_eq!(world.resource_balance(ResourceKind::Electricity), 0);

    // A mixed debit/credit with a missing payer balance must reject before
    // installing any credit, even if a separate positive entry overflows.
    world.set_resource_balance(ResourceKind::Data, i64::MAX);
    let mixed = ResourceDelta {
        entries: BTreeMap::from([
            (ResourceKind::Data, 1),
            (ResourceKind::Electricity, -1),
        ]),
    };
    let mixed_scope = fee(&mixed, &mixed, RuleActionDisposition::Accepted);
    let error = mixed_scope
        .plan_checked(&world.state().resources)
        .unwrap_err();
    assert!(matches!(
        error,
        WorldError::ResourceBalanceInvalid { reason }
            if reason.contains("insufficient resources")
    ));
    assert_eq!(world.resource_balance(ResourceKind::Data), i64::MAX);
    assert_eq!(world.resource_balance(ResourceKind::Electricity), 0);
}

fn lease_req(key: &str, amount: u64, valid_until: Option<u64>) -> CognitionLeaseRequestV1 {
    let mut quote = CognitionLeaseQuoteV1::new(format!("q-{key}"), "units", amount);
    if let Some(tick) = valid_until {
        quote = quote.with_valid_until_tick(tick);
    }
    CognitionLeaseRequestV1::new(
        key, "acct", "agent-a", "sess", "turn", format!("r-{key}"), format!("d-{key}"), quote,
    )
}

fn stage_op(world: &World, op: &StagedCognitionEconomyOp<'_>) -> super::StagedCognitionPrepared {
    let view = BorrowedWorldView::new(world).unwrap();
    op.prepare_with_output(&view, &TransitionContext::new(view.time(), None))
        .unwrap()
}

#[test]
fn staged_cognition_lease_lifecycle_matches_live_world_and_fences_unknown_or_stale() {
    let (mut staged, mut live) = (World::new(), World::new());
    staged.set_cognition_resource_balance("acct", "units", 30).unwrap();
    live.set_cognition_resource_balance("acct", "units", 30).unwrap();

    let req1 = lease_req("k1", 10, None);
    let (CognitionEconomyOutput::Lease(staged_lease), unknown) =
        stage_op(&staged, &StagedCognitionEconomyOp::ReserveLease(&req1))
    else {
        panic!("expected lease output");
    };
    let unk = DurableCommitConfirmation::StatusUnknown { reason: "lost ack".into() };
    assert!(matches!(
        unknown.install_confirmed(&mut staged, unk),
        Err(TransitionReject::StatusUnknownFenced { .. })
    ));
    assert_eq!(staged.cognition_economy().unwrap().reserved_balance("acct", "units"), 0);

    let live_lease = live.reserve_cognition_lease(req1.clone()).unwrap();
    assert_eq!(staged_lease, live_lease);
    stage_op(&staged, &StagedCognitionEconomyOp::ReserveLease(&req1))
        .1
        .install_confirmed(&mut staged, DurableCommitConfirmation::Confirmed)
        .unwrap();
    assert_eq!(staged.cognition(), live.cognition());

    let (settle_out, settle_commit) =
        stage_op(&staged, &StagedCognitionEconomyOp::SettleLease(&staged_lease.lease_id, 6));
    let live_settle = live.settle_cognition_lease(&live_lease.lease_id, 6).unwrap();
    settle_commit
        .install_confirmed(&mut staged, DurableCommitConfirmation::Confirmed)
        .unwrap();
    assert_eq!(settle_out, CognitionEconomyOutput::Receipt(live_settle.clone()));
    assert_eq!(staged.cognition(), live.cognition());

    let (_, stale_refund) = stage_op(
        &staged,
        &StagedCognitionEconomyOp::RefundSettledLease(
            &staged_lease.lease_id, 2, &live_settle.receipt_id, "partial",
        ),
    );
    let (req2, req3) = (lease_req("k2", 4, None), lease_req("k3", 5, Some(1)));
    let lease2 = staged.reserve_cognition_lease(req2.clone()).unwrap();
    live.reserve_cognition_lease(req2).unwrap();
    assert!(matches!(
        stale_refund.install_confirmed(&mut staged, DurableCommitConfirmation::Confirmed),
        Err(TransitionReject::BaseHeadStale)
    ));

    let lease3 = staged.reserve_cognition_lease(req3.clone()).unwrap();
    live.reserve_cognition_lease(req3).unwrap();
    staged.release_cognition_lease(&lease2.lease_id).unwrap();
    live.release_cognition_lease(&lease2.lease_id).unwrap();
    for _ in 0..2 {
        staged.step().unwrap();
        live.step().unwrap();
    }
    staged.expire_cognition_lease(&lease3.lease_id).unwrap();
    live.expire_cognition_lease(&lease3.lease_id).unwrap();
    staged.refund_cognition_lease(&staged_lease.lease_id).unwrap();
    live.refund_cognition_lease(&live_lease.lease_id).unwrap();
    assert_eq!(staged.cognition(), live.cognition());
}

#[test]
fn staged_root_transaction_composes_rule_fee_and_cognition_lease_with_nested_savepoint() {
    let mut world = World::new();
    world.set_resource_balance(ResourceKind::Data, 10);
    world.set_resource_balance(ResourceKind::Electricity, 8);
    world.set_cognition_resource_balance("acct", "units", 15).unwrap();

    let (general, with_distance) = costs();
    let accepted_fee = fee(&general, &with_distance, RuleActionDisposition::Accepted);
    let denied_fee = fee(&general, &with_distance, RuleActionDisposition::RejectedMovement);
    let (valid_req, oversized_req) = (lease_req("ok", 6, None), lease_req("big", 99, None));

    let prepared = {
        let ctx = TransitionContext::new(world.state().time, None);
        let mut root = StagedRootTransaction::<MonetaryDomainDelta>::begin(&world, ctx).unwrap();
        accepted_fee.stage_in_root(&mut root).unwrap();
        let CognitionEconomyOutput::Lease(lease) = StagedCognitionEconomyOp::ReserveLease(&valid_req)
            .stage_in_root(&mut root)
            .unwrap()
        else {
            panic!("expected reserved lease");
        };

        let nested_err = root.stage_nested(|tx| {
            StagedCognitionEconomyOp::SetBalance("spec", "units", 1).stage_in_root(tx)?;
            StagedCognitionEconomyOp::ReserveLease(&oversized_req)
                .stage_in_root(tx)
                .map(|_| ())
        });
        assert!(nested_err.is_err());
        assert_eq!(root.staged_count(), 2);

        root.stage_nested(|tx| {
            denied_fee.stage_in_root(tx)?;
            StagedCognitionEconomyOp::SetBalance("second", "units", 9).stage_in_root(tx)?;
            StagedCognitionEconomyOp::SettleLease(&lease.lease_id, 4)
                .stage_in_root(tx)
                .map(|_| ())
        })
        .unwrap();
        assert_eq!(root.staged_count(), 5);
        root.finish_prepare()
    };

    assert_eq!(world.resource_balance(ResourceKind::Data), 10);
    assert_eq!(world.cognition_economy().unwrap().reserved_balance("acct", "units"), 0);
    prepared.install_confirmed(&mut world, DurableCommitConfirmation::Confirmed).unwrap();
    assert_eq!(world.resource_balance(ResourceKind::Data), 6);
    assert_eq!(world.resource_balance(ResourceKind::Electricity), 4);
    let econ = world.cognition_economy().unwrap();
    assert_eq!(econ.reserved_balance("acct", "units"), 0);
    assert_eq!(econ.available_balance("acct", "units"), 11);
    assert_eq!(econ.available_balance("second", "units"), 9);
    assert_eq!(econ.available_balance("spec", "units"), 0);
}

fn register_test_agent(world: &mut World, agent_id: &str) {
    world.submit_action(Action::RegisterAgent {
        agent_id: agent_id.to_string(),
        pos: GeoPos::new(0, 0, 0),
    });
    world.step().unwrap();
}

#[test]
fn staged_monetary_publication_matches_live_transfer_and_rolls_back_without_double_debit() {
    let mut live = World::new();
    register_test_agent(&mut live, "alice");
    register_test_agent(&mut live, "bob");
    live.set_agent_resource_balance("alice", ResourceKind::Electricity, 20).unwrap();
    let mut staged = live.clone();

    let transfer = WorldEventBody::Domain(DomainEvent::ResourceTransferred {
        from_agent_id: "alice".into(),
        to_agent_id: "bob".into(),
        kind: ResourceKind::Electricity,
        amount: 7,
    });
    let (before, before_journal) = (staged.snapshot(), staged.journal().clone());
    staged.fail_next_append_after_publication_prepare_for_test();
    assert!(staged.append_event(transfer.clone(), None).is_err());
    assert_eq!((staged.snapshot(), staged.journal()), (before, &before_journal));

    let transition = StagedMonetaryPublication::from_body(&transfer).unwrap();
    let prepared = {
        let view = BorrowedWorldView::new(&staged).unwrap();
        transition.prepare(&view, &TransitionContext::new(view.time(), None)).unwrap()
    };
    assert!(matches!(
        prepared.install_confirmed(&mut staged, DurableCommitConfirmation::StatusUnknown { reason: "lost ack".into() }),
        Err(TransitionReject::StatusUnknownFenced { .. })
    ));
    assert_eq!(staged.agent_resource_balance("alice", ResourceKind::Electricity).unwrap(), 20);
    assert_eq!(staged.agent_resource_balance("bob", ResourceKind::Electricity).unwrap(), 0);

    live.append_event(transfer.clone(), None).unwrap();
    staged.append_staged_monetary_event(transition, None).unwrap();
    assert_eq!(staged.snapshot(), live.snapshot());
    assert_eq!(staged.agent_resource_balance("alice", ResourceKind::Electricity).unwrap(), 13);
    assert_eq!(staged.agent_resource_balance("bob", ResourceKind::Electricity).unwrap(), 7);
}

#[test]
fn staged_root_composes_module_runtime_charge_and_bootstrap_catalog_with_rollback() {
    let mut world = World::new();
    register_test_agent(&mut world, "payer");
    world.set_agent_resource_balance("payer", ResourceKind::Electricity, 50).unwrap();
    world.set_agent_resource_balance("payer", ResourceKind::Data, 30).unwrap();

    let charge_body = WorldEventBody::ModuleRuntimeCharged(ModuleRuntimeChargeEvent {
        module_id: "m.test".into(), trace_id: "trace-1".into(), payer_agent_id: "payer".into(),
        compute_fee_kind: ResourceKind::Data, compute_fee_amount: 4,
        electricity_fee_kind: ResourceKind::Electricity, electricity_fee_amount: 6,
        input_bytes: 16, output_bytes: 32, effect_count: 0, emit_count: 1,
    });
    let pub_transition = StagedMonetaryPublication::from_body(&charge_body).unwrap();

    let stale_prepared = {
        let mut root = StagedRootTransaction::<MonetaryDomainDelta>::begin(&world, TransitionContext::new(world.state().time, None)).unwrap();
        StagedBootstrapCatalogDefaults.stage_in_root(&mut root).unwrap();
        pub_transition.stage_in_root(&mut root).unwrap();
        root.finish_prepare()
    };
    world.set_agent_resource_balance("payer", ResourceKind::Data, 29).unwrap();
    assert!(matches!(stale_prepared.install_confirmed(&mut world, DurableCommitConfirmation::Confirmed), Err(TransitionReject::BaseHeadStale)));
    assert_eq!(world.agent_resource_balance("payer", ResourceKind::Electricity).unwrap(), 50);
    assert_eq!(world.agent_resource_balance("payer", ResourceKind::Data).unwrap(), 29);
    assert!(world.material_profile("iron_ore").is_none());

    let prepared = {
        let mut root = StagedRootTransaction::<MonetaryDomainDelta>::begin(&world, TransitionContext::new(world.state().time, None)).unwrap();
        StagedBootstrapCatalogDefaults.stage_in_root(&mut root).unwrap();
        pub_transition.stage_in_root(&mut root).unwrap();
        assert_eq!(root.staged_count(), 2);
        root.finish_prepare()
    };
    prepared.install_confirmed(&mut world, DurableCommitConfirmation::Confirmed).unwrap();
    assert_eq!(world.agent_resource_balance("payer", ResourceKind::Electricity).unwrap(), 44);
    assert_eq!(world.agent_resource_balance("payer", ResourceKind::Data).unwrap(), 25);
    assert!(world.material_profile("iron_ore").is_some());
    assert!(world.product_profile("iron_ingot").is_some());
    assert!(world.recipe_profile("recipe.smelter.iron_ingot").is_some());
}
