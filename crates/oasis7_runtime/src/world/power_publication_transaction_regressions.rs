use super::*;
use oasis7_agent_api::world_action::GeoPos;
use crate::{
    Action, CausedBy, DomainEvent, MaterialLedgerId, ProtocolPowerReserve, RewardAssetConfig,
    WorldEventBody,
};
use oasis7_agent_api::world_action::ResourceKind;

fn register(world: &mut World, id: &str) {
    world.submit_action(Action::RegisterAgent {
        agent_id: id.into(),
        pos: GeoPos::new(0, 0, 0),
    });
    world.step().unwrap();
}

fn redeem_world() -> World {
    let mut world = World::new();
    world
        .bind_node_identity("node-40b", "public-key-node-40b")
        .unwrap();
    register(&mut world, "target-40b");
    world.set_reward_asset_config(RewardAssetConfig {
        credits_per_power_unit: 4,
        ..RewardAssetConfig::default()
    });
    world.set_protocol_power_reserve(ProtocolPowerReserve {
        epoch_index: 2,
        available_power_units: 50,
        redeemed_power_units: 0,
    });
    world.mint_node_power_credits("node-40b", 20).unwrap();
    world
}

fn redeemed_event() -> DomainEvent {
    DomainEvent::PowerRedeemed {
        node_id: "node-40b".into(),
        target_agent_id: "target-40b".into(),
        burned_credits: 9,
        granted_power_units: 2,
        reserve_remaining: 48,
        nonce: 1,
    }
}

fn rejected_world() -> World {
    let mut world = World::new();
    register(&mut world, "node-40b");
    register(&mut world, "target-40b");
    world
}

fn rejected_event() -> DomainEvent {
    DomainEvent::PowerRedeemRejected {
        node_id: "node-40b".into(),
        target_agent_id: "target-40b".into(),
        redeem_credits: 0,
        nonce: 0,
        reason: String::new(),
    }
}

fn assert_unchanged(world: &World, before: &World, root: &str) {
    assert_eq!(world.snapshot(), before.snapshot());
    assert_eq!(world.journal(), before.journal());
    assert_eq!(
        (world.next_event_id, world.next_event_id_era),
        (before.next_event_id, before.next_event_id_era)
    );
    assert_eq!(
        world.runtime_backpressure_stats(),
        before.runtime_backpressure_stats()
    );
    assert_eq!(
        world.tick_consensus_records(),
        before.tick_consensus_records()
    );
    assert_eq!(world.current_state_root_hash().unwrap(), root);
    for id in ["node-40b", "target-40b"] {
        assert_eq!(
            world.state.agents.get(id).map(|cell| &cell.mailbox),
            before.state.agents.get(id).map(|cell| &cell.mailbox)
        );
    }
}

fn fail_retry(mut world: World, event: DomainEvent) -> World {
    let baseline = world.snapshot();
    let before = world.clone();
    let root = world.current_state_root_hash().unwrap();
    world.fail_next_append_after_publication_prepare_for_test();
    let error = world
        .append_event_for_test(
            WorldEventBody::Domain(event.clone()),
            Some(CausedBy::Action(420)),
        )
        .expect_err("honor postprepare failpoint");
    assert!(
        matches!(error, WorldError::ResourceBalanceInvalid { ref reason } if reason.contains("publication preparation"))
    );
    assert_unchanged(&world, &before, &root);
    let expected_id = world.next_event_id.max(1);
    let cause = Some(CausedBy::Action(421));
    assert_eq!(
        world
            .append_event_for_test(WorldEventBody::Domain(event), cause.clone())
            .unwrap(),
        expected_id
    );
    assert_eq!(
        world
            .journal
            .events
            .last()
            .and_then(|event| event.caused_by.clone()),
        cause
    );
    let replay = World::from_snapshot(baseline, world.journal.clone()).unwrap();
    assert_eq!(replay.snapshot(), world.snapshot());
    assert_eq!(
        replay.current_state_root_hash().unwrap(),
        world.current_state_root_hash().unwrap()
    );
    world
}

#[test]
fn power_redeemed_raw_is_atomic_then_retries_exactly_once() {
    let mut before = redeem_world();
    before.state.materials.insert("legacy-power".into(), 7);
    before
        .state
        .material_ledgers
        .remove(&MaterialLedgerId::world());
    let mailbox = before.state.agents["target-40b"].mailbox.len();
    let world = fail_retry(before, redeemed_event());
    assert_eq!(world.node_power_credit_balance("node-40b"), 11);
    assert_eq!(world.node_last_redeem_nonce("node-40b"), Some(1));
    assert_eq!(
        (
            world.protocol_power_reserve().available_power_units,
            world.protocol_power_reserve().redeemed_power_units
        ),
        (48, 2)
    );
    assert_eq!(
        world
            .agent_resource_balance("target-40b", ResourceKind::Electricity)
            .unwrap(),
        2
    );
    assert_eq!(world.state.agents["target-40b"].mailbox.len(), mailbox + 1);
    assert_eq!(world.state.materials["legacy-power"], 7);
    assert_eq!(
        world.state.material_ledgers[&MaterialLedgerId::world()]["legacy-power"],
        7
    );
}

#[test]
fn power_redeem_rejected_raw_is_atomic_permissive_and_target_routed() {
    let before = rejected_world();
    let node_mailbox = before.state.agents["node-40b"].mailbox.len();
    let target_mailbox = before.state.agents["target-40b"].mailbox.len();
    let world = fail_retry(before, rejected_event());
    assert_eq!(world.state.agents["node-40b"].last_active, world.state.time);
    assert_eq!(
        world.state.agents["target-40b"].last_active,
        world.state.time
    );
    assert_eq!(world.state.agents["node-40b"].mailbox.len(), node_mailbox);
    assert_eq!(
        world.state.agents["target-40b"].mailbox.len(),
        target_mailbox + 1
    );
}

fn assert_error(mut world: World, event: DomainEvent, expected: &str) {
    let before = world.clone();
    let root = world.current_state_root_hash().unwrap();
    let error = world
        .append_event_for_test(WorldEventBody::Domain(event), None)
        .expect_err("validation failure");
    assert!(format!("{error:?}").contains(expected), "{error:?}");
    assert_unchanged(&world, &before, &root);
}

#[test]
fn power_redeemed_validation_priority_precedes_compound_writes() {
    let valid = redeemed_event();
    let mut event = valid.clone();
    if let DomainEvent::PowerRedeemed {
        burned_credits,
        granted_power_units,
        ..
    } = &mut event
    {
        *burned_credits = 0;
        *granted_power_units = 0;
    }
    assert_error(redeem_world(), event, "burned_credits must be > 0");
    let mut world = redeem_world();
    world.state.node_redeem_nonces.insert("node-40b".into(), 1);
    assert_error(world, valid.clone(), "nonce replay detected");
    let mut world = redeem_world();
    world
        .state
        .node_asset_balances
        .get_mut("node-40b")
        .unwrap()
        .power_credit_balance = 1;
    assert_error(world, valid.clone(), "insufficient power credits");
    let mut world = redeem_world();
    world.state.protocol_power_reserve.available_power_units = 1;
    assert_error(world, valid.clone(), "insufficient protocol power reserve");
    let mut event = valid.clone();
    if let DomainEvent::PowerRedeemed {
        reserve_remaining, ..
    } = &mut event
    {
        *reserve_remaining = 47;
    }
    assert_error(redeem_world(), event, "reserve remaining mismatch");
    let mut world = redeem_world();
    world.state.agents.remove("target-40b");
    assert_error(world, valid, "target-40b");
}

#[test]
fn remaining_raw_redeem_validation_branches_are_non_mutating() {
    let valid = redeemed_event();
    let mut event = valid.clone();
    if let DomainEvent::PowerRedeemed {
        granted_power_units,
        nonce,
        ..
    } = &mut event
    {
        *granted_power_units = 0;
        *nonce = 0;
    }
    assert_error(redeem_world(), event, "granted_power_units must be > 0");

    let mut world = redeem_world();
    world.state.reward_asset_config.min_redeem_power_unit = 0;
    assert_error(
        world,
        valid.clone(),
        "min_redeem_power_unit must be positive",
    );
    let mut world = redeem_world();
    world.state.reward_asset_config.min_redeem_power_unit = 3;
    assert_error(world, valid.clone(), "granted_power_units below minimum");
    let mut event = valid.clone();
    if let DomainEvent::PowerRedeemed { nonce, .. } = &mut event {
        *nonce = 0;
    }
    assert_error(redeem_world(), event, "nonce must be > 0");

    let mut world = redeem_world();
    world.state.node_asset_balances.remove("node-40b");
    assert_error(world, valid.clone(), "node balance not found");
    let mut world = redeem_world();
    world
        .state
        .node_asset_balances
        .get_mut("node-40b")
        .unwrap()
        .total_burned_credits = u64::MAX;
    assert_error(world, valid.clone(), "total_burned_credits overflow");

    let mut world = redeem_world();
    world.state.reward_asset_config.max_redeem_power_per_epoch = 0;
    assert_error(
        world,
        valid.clone(),
        "max_redeem_power_per_epoch must be positive",
    );
    let mut world = redeem_world();
    world.state.protocol_power_reserve.redeemed_power_units = i64::MAX;
    assert_error(world, valid.clone(), "redeemed_power_units overflow");
    let mut world = redeem_world();
    world.state.protocol_power_reserve.redeemed_power_units = 9_999;
    assert_error(world, valid.clone(), "epoch redeem cap exceeded");
    let mut world = redeem_world();
    world
        .state
        .agents
        .get_mut("target-40b")
        .unwrap()
        .state
        .resources
        .amounts
        .insert(ResourceKind::Electricity, i64::MAX);
    assert_error(world, valid, "add electricity failed: overflow");
}

#[test]
fn redeemed_alias_merges_one_agent_cell_and_routes_once() {
    let mut world = World::new();
    register(&mut world, "alias-40b");
    world
        .bind_node_identity("alias-40b", "public-key-alias-40b")
        .unwrap();
    world.set_reward_asset_config(RewardAssetConfig {
        credits_per_power_unit: 4,
        ..RewardAssetConfig::default()
    });
    world.set_protocol_power_reserve(ProtocolPowerReserve {
        epoch_index: 2,
        available_power_units: 50,
        redeemed_power_units: 0,
    });
    world.mint_node_power_credits("alias-40b", 20).unwrap();
    let baseline = world.snapshot();
    let mailbox = world.state.agents["alias-40b"].mailbox.len();
    world
        .append_event_for_test(
            WorldEventBody::Domain(DomainEvent::PowerRedeemed {
                node_id: "alias-40b".into(),
                target_agent_id: "alias-40b".into(),
                burned_credits: 9,
                granted_power_units: 2,
                reserve_remaining: 48,
                nonce: 1,
            }),
            Some(CausedBy::Action(422)),
        )
        .unwrap();
    assert_eq!(world.node_power_credit_balance("alias-40b"), 11);
    assert_eq!(world.node_last_redeem_nonce("alias-40b"), Some(1));
    assert_eq!(world.state.agents["alias-40b"].mailbox.len(), mailbox + 1);
    assert_eq!(
        world.state.agents["alias-40b"].last_active,
        world.state.time
    );
    let replay = World::from_snapshot(baseline, world.journal.clone()).unwrap();
    assert_eq!(replay.snapshot(), world.snapshot());
    assert_eq!(
        replay.current_state_root_hash().unwrap(),
        world.current_state_root_hash().unwrap()
    );
}

#[test]
fn rejected_missing_and_alias_actors_remain_permissive_and_target_only_routed() {
    for (node_exists, target_exists, alias) in [
        (true, false, false),
        (false, true, false),
        (false, false, false),
        (true, true, true),
    ] {
        let mut world = World::new();
        let node = if alias {
            "alias-rejected"
        } else {
            "node-rejected"
        };
        let target = if alias {
            "alias-rejected"
        } else {
            "target-rejected"
        };
        if node_exists {
            register(&mut world, node);
        }
        if target_exists && target != node {
            register(&mut world, target);
        }
        let baseline = world.snapshot();
        let node_mailbox = world
            .state
            .agents
            .get(node)
            .map_or(0, |cell| cell.mailbox.len());
        let target_mailbox = world
            .state
            .agents
            .get(target)
            .map_or(0, |cell| cell.mailbox.len());
        world
            .append_event_for_test(
                WorldEventBody::Domain(DomainEvent::PowerRedeemRejected {
                    node_id: node.into(),
                    target_agent_id: target.into(),
                    redeem_credits: 0,
                    nonce: 0,
                    reason: String::new(),
                }),
                None,
            )
            .expect("missing actors do not invalidate a rejection audit");
        if let Some(cell) = world.state.agents.get(node) {
            assert_eq!(cell.last_active, world.state.time);
            let expected = node_mailbox + usize::from(alias);
            assert_eq!(cell.mailbox.len(), expected);
        }
        if target != node
            && let Some(cell) = world.state.agents.get(target)
        {
            assert_eq!(cell.last_active, world.state.time);
            assert_eq!(cell.mailbox.len(), target_mailbox + 1);
        }
        let replay = World::from_snapshot(baseline, world.journal.clone()).unwrap();
        assert_eq!(replay.snapshot(), world.snapshot());
        assert_eq!(
            replay.current_state_root_hash().unwrap(),
            world.current_state_root_hash().unwrap()
        );
    }
}
