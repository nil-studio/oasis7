use std::sync::Arc;

use super::{
    ModuleArtifactRegistrationDelta, ModuleArtifactRetirementDelta, StagedModuleMarketplaceOrder,
};
use crate::world::transition::{
    BorrowedWorldView, DurableCommitConfirmation, StagedRootTransaction, StagedTransition,
    TransitionContext, TransitionReject,
};
use crate::{Action, ActionEnvelope, DomainEvent, World, WorldError};
use oasis7_agent_api::world_action::ResourceKind;

#[derive(Debug, Clone)]
enum LifecycleTestDelta {
    Register(ModuleArtifactRegistrationDelta),
    Retire(ModuleArtifactRetirementDelta),
}

impl crate::world::transition::SubsystemDelta for LifecycleTestDelta {
    fn apply_infallible(self, ctx: &mut crate::world::transition::RootInstallContext<'_>) {
        match self {
            Self::Register(delta) => delta.apply_infallible(ctx),
            Self::Retire(delta) => delta.apply_infallible(ctx),
        }
    }
}

#[test]
fn staged_root_transaction_enforces_nested_savepoint_and_fences_status_unknown() {
    let mut world = World::new();
    let hash_a = crate::util::sha256_hex(b"artifact-a");
    let hash_b = crate::util::sha256_hex(b"artifact-b");

    let mut root_tx =
        StagedRootTransaction::<LifecycleTestDelta>::begin(&world, TransitionContext::default())
            .expect("begin staged root transaction");

    // Stage first artifact registration in the root transaction.
    root_tx.stage_delta(LifecycleTestDelta::Register(
        ModuleArtifactRegistrationDelta::new(hash_a.clone(), Arc::<[u8]>::from(&b"artifact-a"[..])),
    ));
    assert_eq!(root_tx.staged_count(), 1);
    assert!(!world.module_artifacts.contains(&hash_a));

    // A failing nested operation must roll back its own staged deltas without touching earlier ones.
    let nested_err = root_tx.stage_nested(|tx| {
        tx.stage_delta(LifecycleTestDelta::Register(
            ModuleArtifactRegistrationDelta::new(
                hash_b.clone(),
                Arc::<[u8]>::from(&b"artifact-b"[..]),
            ),
        ));
        assert_eq!(tx.staged_count(), 2);
        Err::<(), TransitionReject>(TransitionReject::World(WorldError::ModuleChangeInvalid {
            reason: "nested validation failed".to_string(),
        }))
    });
    assert!(nested_err.is_err());
    assert_eq!(root_tx.staged_count(), 1);

    let prepared = root_tx.finish_prepare();

    // Stage 2 StatusUnknown MUST fence and refuse to mutate canonical memory.
    let unknown_res = prepared.clone().install_confirmed(
        &mut world,
        DurableCommitConfirmation::StatusUnknown {
            reason: "post-prepare durability ack lost".to_string(),
        },
    );
    assert!(matches!(
        unknown_res,
        Err(TransitionReject::StatusUnknownFenced { .. })
    ));
    assert!(!world.module_artifacts.contains(&hash_a));
    assert!(!world.module_artifacts.contains(&hash_b));

    // Stage 2 Confirmed Commit installs Stage 1 deltas infallibly via RootInstallContext.
    prepared
        .install_confirmed(&mut world, DurableCommitConfirmation::Confirmed)
        .expect("confirmed commit should install");
    assert!(world.module_artifacts.contains(&hash_a));
    assert!(!world.module_artifacts.contains(&hash_b));

    // Next transaction: base-head drift between Stage 1 and Stage 3 must be rejected as BaseHeadStale.
    let mut retire_tx =
        StagedRootTransaction::<LifecycleTestDelta>::begin(&world, TransitionContext::default())
            .expect("begin retire transaction");
    retire_tx.stage_delta(LifecycleTestDelta::Retire(
        ModuleArtifactRetirementDelta::prepare(&world, hash_a.clone()),
    ));
    let prepared_retire = retire_tx.finish_prepare();

    // Mutate canonical world before installing prepared_retire.
    world.module_artifacts.insert(hash_b.clone());
    let stale_res =
        prepared_retire.install_confirmed(&mut world, DurableCommitConfirmation::Confirmed);
    assert!(matches!(stale_res, Err(TransitionReject::BaseHeadStale)));
    assert!(world.module_artifacts.contains(&hash_a));
}

#[test]
fn staged_marketplace_order_transition_prepares_on_borrowed_view_and_fences_status_unknown() {
    let mut world = World::new();
    for agent_id in ["seller", "buyer"] {
        world.submit_action(Action::RegisterAgent {
            agent_id: agent_id.into(),
            pos: crate::tests::pos(0, 0),
        });
        world.step().unwrap();
        world
            .set_agent_resource_balance(agent_id, ResourceKind::Electricity, 1_000)
            .unwrap();
    }
    let bytes = b"staged-marketplace-artifact";
    let wasm_hash = crate::util::sha256_hex(bytes);
    world
        .try_apply_runtime_module_action(&ActionEnvelope {
            id: 100,
            action: Action::DeployModuleArtifact {
                publisher_agent_id: "seller".into(),
                wasm_hash: wasm_hash.clone(),
                wasm_bytes: bytes.to_vec(),
            },
            governance_authorization: None,
            committed_recipe_origin: None,
        })
        .unwrap();

    let transition = StagedModuleMarketplaceOrder {
        order: DomainEvent::ModuleArtifactListed {
            seller_agent_id: "seller".into(),
            wasm_hash: wasm_hash.clone(),
            price_kind: ResourceKind::Electricity,
            price_amount: 25,
            order_id: 77,
            fee_kind: ResourceKind::Electricity,
            fee_amount: 0,
        },
    };
    let snapshot_before = world.snapshot();
    let journal_before = world.journal().clone();

    // Stage 1: prepare over BorrowedWorldView leaves canonical world untouched.
    let prepared = {
        let view = BorrowedWorldView::new(&world).expect("borrowed world view");
        let ctx = TransitionContext::new(view.time(), None);
        transition
            .prepare(&view, &ctx)
            .expect("marketplace listing should prepare")
    };
    assert_eq!(world.snapshot(), snapshot_before);
    assert_eq!(world.journal(), &journal_before);

    // Stage 2: StatusUnknown must fence without mutating canonical state.
    let fenced = DomainEvent::ModuleArtifactBidPlaced {
        bidder_agent_id: "buyer".into(),
        wasm_hash: wasm_hash.clone(),
        price_kind: ResourceKind::Electricity,
        price_amount: 25,
        order_id: 78,
    };
    let _ = fenced;
    let unknown_err = prepared.install_confirmed(
        &mut world,
        DurableCommitConfirmation::StatusUnknown {
            reason: "simulated durability timeout".into(),
        },
    );
    assert!(matches!(
        unknown_err,
        Err(TransitionReject::StatusUnknownFenced { .. })
    ));
    assert_eq!(world.snapshot(), snapshot_before);
    assert_eq!(world.journal(), &journal_before);

    // Re-prepare and confirm Stage 2 -> Stage 3 install.
    let prepared = {
        let view = BorrowedWorldView::new(&world).expect("borrowed world view");
        let ctx = TransitionContext::new(view.time(), None);
        transition
            .prepare(&view, &ctx)
            .expect("marketplace listing should prepare")
    };
    prepared
        .install_confirmed(&mut world, DurableCommitConfirmation::Confirmed)
        .expect("confirmed marketplace order should install");
    assert!(
        world
            .state
            .module_artifact_listings
            .contains_key(&wasm_hash)
    );
}
