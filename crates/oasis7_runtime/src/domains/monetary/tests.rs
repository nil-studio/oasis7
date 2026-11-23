use std::collections::BTreeMap;

use super::{StagedRuleFeeSettlement, plan_resource_delta};
use crate::world::transition::{
    BorrowedWorldView, DurableCommitConfirmation, StagedTransition, TransitionContext,
    TransitionReject,
};
use crate::{ResourceDelta, World, WorldError};
use oasis7_agent_api::world_action::ResourceKind;
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
