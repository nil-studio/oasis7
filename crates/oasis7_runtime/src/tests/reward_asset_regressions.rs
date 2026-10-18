#[test]
fn reward_asset_redeem_power_rejected_when_reserve_insufficient() {
    let mut world = World::new();
    bind_node_identity(&mut world, "node-a");
    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        pos: oasis7_agent_api::world_action::GeoPos::new(0, 0, 0),
    });
    world.step().expect("register target agent");
    let initial_electricity = world
        .agent_resource_balance("agent-1", oasis7_agent_api::world_action::ResourceKind::Electricity)
        .expect("query target electricity");

    world.set_reward_asset_config(RewardAssetConfig {
        credits_per_power_unit: 1,
        ..RewardAssetConfig::default()
    });
    world.set_protocol_power_reserve(ProtocolPowerReserve {
        epoch_index: 4,
        available_power_units: 1,
        redeemed_power_units: 0,
    });
    world
        .mint_node_power_credits("node-a", 5)
        .expect("mint node credits");

    world.submit_action(Action::RedeemPower {
        node_id: "node-a".to_string(),
        target_agent_id: "agent-1".to_string(),
        redeem_credits: 3,
        nonce: 5,
    });
    world.step().expect("redeem power rejected");

    assert_eq!(world.node_power_credit_balance("node-a"), 5);
    assert_eq!(
        world
            .agent_resource_balance("agent-1", oasis7_agent_api::world_action::ResourceKind::Electricity)
            .expect("query target electricity after reject"),
        initial_electricity
    );
    assert_eq!(world.protocol_power_reserve().available_power_units, 1);
    assert_eq!(world.protocol_power_reserve().redeemed_power_units, 0);

    let event = world.journal().events.last().expect("reject event");
    match &event.body {
        WorldEventBody::Domain(DomainEvent::PowerRedeemRejected {
            node_id,
            target_agent_id,
            redeem_credits,
            nonce,
            reason,
        }) => {
            assert_eq!(node_id, "node-a");
            assert_eq!(target_agent_id, "agent-1");
            assert_eq!(*redeem_credits, 3);
            assert_eq!(*nonce, 5);
            assert!(reason.contains("insufficient protocol power reserve"));
        }
        other => panic!("expected PowerRedeemRejected, got {other:?}"),
    }
}

#[test]
fn reward_asset_redeem_power_rejects_below_min_redeem_unit() {
    let mut world = World::new();
    bind_node_identity(&mut world, "node-a");
    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        pos: oasis7_agent_api::world_action::GeoPos::new(0, 0, 0),
    });
    world.step().expect("register target agent");

    world.set_reward_asset_config(RewardAssetConfig {
        credits_per_power_unit: 4,
        max_redeem_power_per_epoch: 100,
        min_redeem_power_unit: 3,
        ..RewardAssetConfig::default()
    });
    world.set_protocol_power_reserve(ProtocolPowerReserve {
        epoch_index: 10,
        available_power_units: 50,
        redeemed_power_units: 0,
    });
    world
        .mint_node_power_credits("node-a", 12)
        .expect("mint node credits");

    world.submit_action(Action::RedeemPower {
        node_id: "node-a".to_string(),
        target_agent_id: "agent-1".to_string(),
        redeem_credits: 8,
        nonce: 1,
    });
    world.step().expect("redeem should be rejected");

    assert_eq!(world.node_power_credit_balance("node-a"), 12);
    assert_eq!(world.protocol_power_reserve().available_power_units, 50);
    assert_eq!(world.protocol_power_reserve().redeemed_power_units, 0);
    assert_eq!(world.node_last_redeem_nonce("node-a"), None);
    let event = world.journal().events.last().expect("reject event");
    match &event.body {
        WorldEventBody::Domain(DomainEvent::PowerRedeemRejected { reason, .. }) => {
            assert!(reason.contains("granted power below minimum unit"));
        }
        other => panic!("expected PowerRedeemRejected, got {other:?}"),
    }
}

#[test]
fn reward_asset_redeem_power_rejects_epoch_cap_exceeded() {
    let mut world = World::new();
    bind_node_identity(&mut world, "node-a");
    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        pos: oasis7_agent_api::world_action::GeoPos::new(0, 0, 0),
    });
    world.step().expect("register target agent");

    world.set_reward_asset_config(RewardAssetConfig {
        credits_per_power_unit: 1,
        max_redeem_power_per_epoch: 3,
        min_redeem_power_unit: 1,
        ..RewardAssetConfig::default()
    });
    world.set_protocol_power_reserve(ProtocolPowerReserve {
        epoch_index: 11,
        available_power_units: 50,
        redeemed_power_units: 2,
    });
    world
        .mint_node_power_credits("node-a", 10)
        .expect("mint node credits");

    world.submit_action(Action::RedeemPower {
        node_id: "node-a".to_string(),
        target_agent_id: "agent-1".to_string(),
        redeem_credits: 2,
        nonce: 1,
    });
    world.step().expect("redeem should be rejected by cap");

    assert_eq!(world.node_power_credit_balance("node-a"), 10);
    assert_eq!(world.protocol_power_reserve().available_power_units, 50);
    assert_eq!(world.protocol_power_reserve().redeemed_power_units, 2);
    assert_eq!(world.node_last_redeem_nonce("node-a"), None);
    let event = world.journal().events.last().expect("reject event");
    match &event.body {
        WorldEventBody::Domain(DomainEvent::PowerRedeemRejected { reason, .. }) => {
            assert!(reason.contains("epoch redeem cap exceeded"));
        }
        other => panic!("expected PowerRedeemRejected, got {other:?}"),
    }
}

#[test]
fn reward_asset_redeem_power_rejects_nonce_replay() {
    let mut world = World::new();
    bind_node_identity(&mut world, "node-a");
    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        pos: oasis7_agent_api::world_action::GeoPos::new(0, 0, 0),
    });
    world.step().expect("register target agent");

    world.set_reward_asset_config(RewardAssetConfig {
        credits_per_power_unit: 1,
        max_redeem_power_per_epoch: 100,
        min_redeem_power_unit: 1,
        ..RewardAssetConfig::default()
    });
    world.set_protocol_power_reserve(ProtocolPowerReserve {
        epoch_index: 12,
        available_power_units: 50,
        redeemed_power_units: 0,
    });
    world
        .mint_node_power_credits("node-a", 10)
        .expect("mint node credits");

    world.submit_action(Action::RedeemPower {
        node_id: "node-a".to_string(),
        target_agent_id: "agent-1".to_string(),
        redeem_credits: 2,
        nonce: 7,
    });
    world.step().expect("first redeem");
    assert_eq!(world.node_last_redeem_nonce("node-a"), Some(7));
    assert_eq!(world.node_power_credit_balance("node-a"), 8);

    world.submit_action(Action::RedeemPower {
        node_id: "node-a".to_string(),
        target_agent_id: "agent-1".to_string(),
        redeem_credits: 1,
        nonce: 7,
    });
    world.step().expect("replay nonce must be rejected");
    assert_eq!(world.node_last_redeem_nonce("node-a"), Some(7));
    assert_eq!(world.node_power_credit_balance("node-a"), 8);
    let replay_event = world.journal().events.last().expect("replay reject event");
    match &replay_event.body {
        WorldEventBody::Domain(DomainEvent::PowerRedeemRejected { reason, .. }) => {
            assert!(reason.contains("nonce replay detected"));
        }
        other => panic!("expected PowerRedeemRejected, got {other:?}"),
    }

    world.submit_action(Action::RedeemPower {
        node_id: "node-a".to_string(),
        target_agent_id: "agent-1".to_string(),
        redeem_credits: 1,
        nonce: 6,
    });
    world.step().expect("older nonce must be rejected");
    assert_eq!(world.node_last_redeem_nonce("node-a"), Some(7));
    assert_eq!(world.node_power_credit_balance("node-a"), 8);
}

#[test]
fn reward_asset_snapshot_roundtrip_persists_redeem_nonce() {
    let mut world = World::new();
    bind_node_identity(&mut world, "node-a");
    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        pos: oasis7_agent_api::world_action::GeoPos::new(0, 0, 0),
    });
    world.step().expect("register target agent");
    world.set_reward_asset_config(RewardAssetConfig {
        credits_per_power_unit: 1,
        max_redeem_power_per_epoch: 100,
        min_redeem_power_unit: 1,
        ..RewardAssetConfig::default()
    });
    world.set_protocol_power_reserve(ProtocolPowerReserve {
        epoch_index: 13,
        available_power_units: 30,
        redeemed_power_units: 0,
    });
    world
        .mint_node_power_credits("node-a", 5)
        .expect("mint node credits");
    world.submit_action(Action::RedeemPower {
        node_id: "node-a".to_string(),
        target_agent_id: "agent-1".to_string(),
        redeem_credits: 2,
        nonce: 3,
    });
    world.step().expect("redeem");

    let snapshot = world.snapshot();
    let restored = World::from_snapshot(snapshot, world.journal().clone()).expect("restore");
    assert_eq!(restored.node_last_redeem_nonce("node-a"), Some(3));
}
