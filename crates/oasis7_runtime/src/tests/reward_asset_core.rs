use super::super::*;
use ed25519_dalek::SigningKey;

fn settlement(node_id: &str, awarded_points: u64) -> NodeSettlement {
    NodeSettlement {
        node_id: node_id.to_string(),
        obligation_met: true,
        compute_score: 0.0,
        storage_score: 0.0,
        uptime_score: 0.0,
        reliability_score: 0.0,
        storage_reward_score: 0.0,
        rewardable_storage_bytes: 0,
        penalty_score: 0.0,
        total_score: 0.0,
        main_awarded_points: awarded_points,
        storage_awarded_points: 0,
        awarded_points,
        cumulative_points: awarded_points,
    }
}

fn settlement_report(epoch_index: u64, settlements: Vec<NodeSettlement>) -> EpochSettlementReport {
    let distributed_points = settlements
        .iter()
        .map(|settlement| settlement.awarded_points)
        .sum::<u64>();
    EpochSettlementReport {
        epoch_index,
        pool_points: distributed_points,
        storage_pool_points: 0,
        distributed_points,
        storage_distributed_points: 0,
        total_distributed_points: distributed_points,
        settlements,
    }
}

fn bind_node_identity(world: &mut World, node_id: &str) {
    let public_key = format!("public-key-{node_id}");
    world
        .bind_node_identity(node_id, public_key.as_str())
        .expect("bind node identity");
}

fn bind_node_identity_with_seed(world: &mut World, node_id: &str, seed: u8) -> String {
    let private = [seed; 32];
    let signing_key = SigningKey::from_bytes(&private);
    let private_key_hex = hex::encode(signing_key.to_bytes());
    let public_key_hex = hex::encode(signing_key.verifying_key().to_bytes());
    world
        .bind_node_identity(node_id, public_key_hex.as_str())
        .expect("bind node identity with keypair");
    private_key_hex
}

#[test]
fn reward_asset_mint_and_burn_updates_balance() {
    let mut world = World::new();
    assert_eq!(world.node_power_credit_balance("node-a"), 0);

    world
        .mint_node_power_credits("node-a", 120)
        .expect("mint power credits");
    assert_eq!(world.node_power_credit_balance("node-a"), 120);

    world
        .burn_node_power_credits("node-a", 20)
        .expect("burn power credits");
    assert_eq!(world.node_power_credit_balance("node-a"), 100);

    let balance = world
        .node_asset_balance("node-a")
        .expect("node asset balance");
    assert_eq!(balance.node_id, "node-a");
    assert_eq!(balance.power_credit_balance, 100);
    assert_eq!(balance.total_minted_credits, 120);
    assert_eq!(balance.total_burned_credits, 20);

    let err = world
        .burn_node_power_credits("node-a", 101)
        .expect_err("insufficient credits must fail");
    assert!(matches!(err, WorldError::ResourceBalanceInvalid { .. }));
}

#[test]
fn reward_asset_rejects_empty_node_id() {
    let mut world = World::new();
    let err = world
        .mint_node_power_credits("", 10)
        .expect_err("empty node id must fail");
    assert!(matches!(err, WorldError::ResourceBalanceInvalid { .. }));
}

#[test]
fn reward_asset_snapshot_roundtrip_persists_state() {
    let mut world = World::new();
    let config = RewardAssetConfig {
        points_per_credit: 5,
        credits_per_power_unit: 2,
        max_redeem_power_per_epoch: 5000,
        min_redeem_power_unit: 10,
    };
    world.set_reward_asset_config(config.clone());

    let reserve = ProtocolPowerReserve {
        epoch_index: 7,
        available_power_units: 3200,
        redeemed_power_units: 100,
    };
    world.set_protocol_power_reserve(reserve.clone());
    world
        .mint_node_power_credits("node-1", 90)
        .expect("mint node-1");
    world
        .burn_node_power_credits("node-1", 15)
        .expect("burn node-1");

    let snapshot = world.snapshot();
    let restored = World::from_snapshot(snapshot, world.journal().clone()).expect("restore");

    assert_eq!(restored.reward_asset_config(), &config);
    assert_eq!(restored.protocol_power_reserve(), &reserve);
    assert_eq!(restored.node_power_credit_balance("node-1"), 75);
    let balance = restored
        .node_asset_balance("node-1")
        .expect("node-1 balance should exist");
    assert_eq!(balance.total_minted_credits, 90);
    assert_eq!(balance.total_burned_credits, 15);
}

#[test]
fn reward_asset_settlement_mint_records_balance_changes() {
    let mut world = World::new();
    bind_node_identity(&mut world, "node-a");
    bind_node_identity(&mut world, "node-b");
    bind_node_identity(&mut world, "node-signer");
    world.set_reward_asset_config(RewardAssetConfig {
        points_per_credit: 10,
        ..RewardAssetConfig::default()
    });

    let report = settlement_report(3, vec![settlement("node-a", 27), settlement("node-b", 9)]);

    let minted = world
        .apply_node_points_settlement_mint(&report, "node-signer")
        .expect("apply settlement mint");
    assert_eq!(minted.len(), 1);
    let record = &minted[0];
    assert_eq!(record.epoch_index, 3);
    assert_eq!(record.node_id, "node-a");
    assert_eq!(record.source_awarded_points, 27);
    assert_eq!(record.minted_power_credits, 2);
    assert_eq!(record.signer_node_id, "node-signer");
    assert!(!record.settlement_hash.is_empty());

    assert_eq!(world.node_power_credit_balance("node-a"), 2);
    assert_eq!(world.node_power_credit_balance("node-b"), 0);
    assert_eq!(world.reward_mint_records().len(), 1);
}

#[test]
fn reward_asset_settlement_mint_is_idempotent_per_epoch_node() {
    let mut world = World::new();
    bind_node_identity(&mut world, "node-a");
    bind_node_identity(&mut world, "node-b");
    bind_node_identity(&mut world, "node-signer");
    world.set_reward_asset_config(RewardAssetConfig {
        points_per_credit: 5,
        ..RewardAssetConfig::default()
    });

    let report = settlement_report(5, vec![settlement("node-a", 25), settlement("node-b", 11)]);

    let first = world
        .apply_node_points_settlement_mint(&report, "node-signer")
        .expect("first apply");
    assert_eq!(first.len(), 2);
    assert_eq!(world.node_power_credit_balance("node-a"), 5);
    assert_eq!(world.node_power_credit_balance("node-b"), 2);

    let second = world
        .apply_node_points_settlement_mint(&report, "node-signer")
        .expect("second apply should be idempotent");
    assert!(second.is_empty());
    assert_eq!(world.node_power_credit_balance("node-a"), 5);
    assert_eq!(world.node_power_credit_balance("node-b"), 2);
    assert_eq!(world.reward_mint_records().len(), 2);
}

#[test]
fn reward_asset_settlement_mint_emits_verifiable_signature() {
    let mut world = World::new();
    bind_node_identity(&mut world, "node-a");
    bind_node_identity(&mut world, "node-signer");
    world.set_reward_asset_config(RewardAssetConfig {
        points_per_credit: 10,
        ..RewardAssetConfig::default()
    });

    let report = settlement_report(10, vec![settlement("node-a", 25)]);
    let minted = world
        .apply_node_points_settlement_mint(&report, "node-signer")
        .expect("apply settlement mint");
    assert_eq!(minted.len(), 1);
    let record = &minted[0];
    assert!(record.signature.starts_with("mintsig:v1:"));
    assert_eq!(record.signature.len(), "mintsig:v1:".len() + 64);
    world
        .verify_reward_mint_record_signature(record)
        .expect("mint signature should be verifiable");
}

#[test]
fn reward_asset_settlement_signature_verification_rejects_tamper() {
    let mut world = World::new();
    bind_node_identity(&mut world, "node-a");
    bind_node_identity(&mut world, "node-signer");
    world.set_reward_asset_config(RewardAssetConfig {
        points_per_credit: 10,
        ..RewardAssetConfig::default()
    });

    let report = settlement_report(11, vec![settlement("node-a", 30)]);
    let minted = world
        .apply_node_points_settlement_mint(&report, "node-signer")
        .expect("apply settlement mint");
    let record = minted.first().cloned().expect("minted record should exist");

    let mut tampered = record.clone();
    tampered.minted_power_credits = tampered.minted_power_credits.saturating_add(1);
    let err = world
        .verify_reward_mint_record_signature(&tampered)
        .expect_err("tampered signature should fail");
    assert!(err.contains("signature mismatch"));

    let mut unbound_signer = record.clone();
    unbound_signer.signer_node_id = "node-unbound".to_string();
    let err = world
        .verify_reward_mint_record_signature(&unbound_signer)
        .expect_err("unbound signer should fail");
    assert!(err.contains("not bound"));
}

#[test]
fn reward_asset_settlement_mint_v2_emits_verifiable_signature() {
    let mut world = World::new();
    bind_node_identity(&mut world, "node-a");
    let signer_private_key = bind_node_identity_with_seed(&mut world, "node-signer", 7);
    world.set_reward_asset_config(RewardAssetConfig {
        points_per_credit: 10,
        ..RewardAssetConfig::default()
    });

    let report = settlement_report(12, vec![settlement("node-a", 40)]);
    let minted = world
        .apply_node_points_settlement_mint_v2(&report, "node-signer", signer_private_key.as_str())
        .expect("apply settlement mint v2");
    assert_eq!(minted.len(), 1);
    let record = &minted[0];
    assert!(record.signature.starts_with("mintsig:v2:"));
    assert_eq!(record.signature.len(), "mintsig:v2:".len() + 128);
    world
        .verify_reward_mint_record_signature(record)
        .expect("mint signature v2 should be verifiable");
}

#[test]
fn reward_asset_settlement_governance_requires_v2_rejects_legacy_mint() {
    let mut world = World::new();
    bind_node_identity(&mut world, "node-a");
    bind_node_identity(&mut world, "node-signer");
    world.set_reward_signature_governance_policy(RewardSignatureGovernancePolicy {
        require_mintsig_v2: true,
        allow_mintsig_v1_fallback: false,
        require_redeem_signature: false,
        require_redeem_signer_match_node_id: false,
    });
    let report = settlement_report(13, vec![settlement("node-a", 20)]);

    let err = world
        .apply_node_points_settlement_mint(&report, "node-signer")
        .expect_err("legacy mint should be rejected by governance");
    match err {
        WorldError::ResourceBalanceInvalid { reason } => {
            assert!(reason.contains("mintsig:v2 is required"));
        }
        other => panic!("expected ResourceBalanceInvalid, got {other:?}"),
    }
}

#[test]
fn reward_asset_settlement_mint_v2_rejects_private_key_mismatch() {
    let mut world = World::new();
    bind_node_identity(&mut world, "node-a");
    let _ = bind_node_identity_with_seed(&mut world, "node-signer", 7);
    world.set_reward_asset_config(RewardAssetConfig {
        points_per_credit: 10,
        ..RewardAssetConfig::default()
    });

    let wrong_private_key = hex::encode([9_u8; 32]);
    let report = settlement_report(14, vec![settlement("node-a", 20)]);
    let err = world
        .apply_node_points_settlement_mint_v2(&report, "node-signer", wrong_private_key.as_str())
        .expect_err("mismatched private key should be rejected");
    match err {
        WorldError::ResourceBalanceInvalid { reason } => {
            assert!(reason.contains("does not match private key"));
        }
        other => panic!("expected ResourceBalanceInvalid, got {other:?}"),
    }
}

#[test]
fn reward_asset_invariant_report_is_clean_for_valid_state() {
    let mut world = World::new();
    bind_node_identity(&mut world, "node-a");
    bind_node_identity(&mut world, "node-signer");
    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        pos: oasis7_agent_api::world_action::GeoPos::new(0, 0, 0),
    });
    world.step().expect("register target agent");
    world.set_reward_asset_config(RewardAssetConfig {
        points_per_credit: 10,
        credits_per_power_unit: 1,
        ..RewardAssetConfig::default()
    });
    world.set_protocol_power_reserve(ProtocolPowerReserve {
        epoch_index: 18,
        available_power_units: 100,
        redeemed_power_units: 0,
    });
    let report = settlement_report(18, vec![settlement("node-a", 30)]);
    world
        .apply_node_points_settlement_mint(&report, "node-signer")
        .expect("mint");
    world.submit_action(Action::RedeemPower {
        node_id: "node-a".to_string(),
        target_agent_id: "agent-1".to_string(),
        redeem_credits: 2,
        nonce: 1,
    });
    world.step().expect("redeem");

    let invariant = world.reward_asset_invariant_report();
    assert!(invariant.is_ok(), "{:?}", invariant.violations);
    assert_eq!(invariant.total_nodes, 1);
    assert_eq!(invariant.total_minted_credits, 3);
    assert_eq!(invariant.total_burned_credits, 2);
    assert_eq!(invariant.total_power_credit_balance, 1);
    assert_eq!(invariant.mint_record_count, 1);
}

#[test]
fn reward_asset_invariant_report_detects_signature_and_balance_drift() {
    let mut world = World::new();
    bind_node_identity(&mut world, "node-a");
    bind_node_identity(&mut world, "node-signer");
    world.set_reward_asset_config(RewardAssetConfig {
        points_per_credit: 10,
        ..RewardAssetConfig::default()
    });
    let report = settlement_report(19, vec![settlement("node-a", 30)]);
    world
        .apply_node_points_settlement_mint(&report, "node-signer")
        .expect("mint");

    let mut snapshot = world.snapshot();
    snapshot
        .state
        .node_asset_balances
        .get_mut("node-a")
        .expect("node-a balance")
        .power_credit_balance = 99;
    snapshot.state.reward_mint_records[0].signature = "mintsig:v1:deadbeef".to_string();

    let tampered = World::from_snapshot(snapshot, world.journal().clone()).expect("restore");
    let invariant = tampered.reward_asset_invariant_report();
    assert!(!invariant.is_ok());
    assert!(invariant
        .violations
        .iter()
        .any(|violation| violation.code == "node_balance_mismatch"));
    assert!(invariant
        .violations
        .iter()
        .any(|violation| violation.code == "global_balance_mismatch"));
    assert!(invariant
        .violations
        .iter()
        .any(|violation| violation.code == "mint_signature_invalid"));
}

#[test]
fn reward_asset_settlement_rejects_zero_points_per_credit() {
    let mut world = World::new();
    world.set_reward_asset_config(RewardAssetConfig {
        points_per_credit: 0,
        ..RewardAssetConfig::default()
    });
    let report = settlement_report(1, vec![settlement("node-a", 10)]);

    let err = world
        .apply_node_points_settlement_mint(&report, "node-signer")
        .expect_err("points_per_credit=0 should fail");
    assert!(matches!(err, WorldError::ResourceBalanceInvalid { .. }));
}

#[test]
fn reward_asset_settlement_rejects_empty_signer_node_id() {
    let mut world = World::new();
    let report = settlement_report(1, vec![settlement("node-a", 10)]);

    let err = world
        .apply_node_points_settlement_mint(&report, "   ")
        .expect_err("empty signer_node_id should fail");
    assert!(matches!(err, WorldError::ResourceBalanceInvalid { .. }));
}

#[test]
fn reward_asset_settlement_rejects_unbound_signer_node_id() {
    let mut world = World::new();
    bind_node_identity(&mut world, "node-a");
    let report = settlement_report(1, vec![settlement("node-a", 10)]);

    let err = world
        .apply_node_points_settlement_mint(&report, "node-signer")
        .expect_err("unbound signer should fail");
    match err {
        WorldError::ResourceBalanceInvalid { reason } => {
            assert!(reason.contains("node identity is not bound"));
        }
        other => panic!("expected ResourceBalanceInvalid, got {other:?}"),
    }
}

#[test]
fn reward_asset_settlement_rejects_unbound_settlement_node() {
    let mut world = World::new();
    bind_node_identity(&mut world, "node-signer");
    let report = settlement_report(2, vec![settlement("node-a", 10)]);

    let err = world
        .apply_node_points_settlement_mint(&report, "node-signer")
        .expect_err("unbound settlement node should fail");
    match err {
        WorldError::ResourceBalanceInvalid { reason } => {
            assert!(reason.contains("node identity is not bound: node-a"));
        }
        other => panic!("expected ResourceBalanceInvalid, got {other:?}"),
    }
}

#[test]
fn reward_asset_snapshot_roundtrip_persists_mint_records() {
    let mut world = World::new();
    bind_node_identity(&mut world, "node-a");
    bind_node_identity(&mut world, "node-signer");
    world.set_reward_asset_config(RewardAssetConfig {
        points_per_credit: 10,
        ..RewardAssetConfig::default()
    });
    let report = settlement_report(9, vec![settlement("node-a", 30)]);
    let minted = world
        .apply_node_points_settlement_mint(&report, "node-signer")
        .expect("mint should succeed");
    assert_eq!(minted.len(), 1);

    let snapshot = world.snapshot();
    let restored = World::from_snapshot(snapshot, world.journal().clone()).expect("restore");
    assert_eq!(restored.reward_mint_records().len(), 1);
    assert_eq!(restored.node_power_credit_balance("node-a"), 3);
    let record = &restored.reward_mint_records()[0];
    assert_eq!(record.epoch_index, 9);
    assert_eq!(record.node_id, "node-a");
    assert_eq!(record.minted_power_credits, 3);
}

#[test]
fn reward_asset_snapshot_roundtrip_persists_node_identity_bindings() {
    let mut world = World::new();
    bind_node_identity(&mut world, "node-a");
    bind_node_identity(&mut world, "node-signer");

    let snapshot = world.snapshot();
    let restored = World::from_snapshot(snapshot, world.journal().clone()).expect("restore");
    assert_eq!(
        restored.node_identity_public_key("node-a"),
        Some("public-key-node-a")
    );
    assert_eq!(
        restored.node_identity_public_key("node-signer"),
        Some("public-key-node-signer")
    );
}

#[test]
fn reward_asset_settlement_mint_respects_system_order_pool_budget() {
    let mut world = World::new();
    bind_node_identity(&mut world, "node-a");
    bind_node_identity(&mut world, "node-b");
    bind_node_identity(&mut world, "node-signer");
    world.set_reward_asset_config(RewardAssetConfig {
        points_per_credit: 10,
        ..RewardAssetConfig::default()
    });
    world.set_system_order_pool_budget(6, 8);
    let report = settlement_report(
        6,
        vec![settlement("node-a", 100), settlement("node-b", 100)],
    );

    let minted = world
        .apply_node_points_settlement_mint(&report, "node-signer")
        .expect("apply settlement mint with budget");
    assert_eq!(minted.len(), 2);
    assert_eq!(
        minted
            .iter()
            .map(|record| record.minted_power_credits)
            .sum::<u64>(),
        8
    );
    assert_eq!(world.node_power_credit_balance("node-a"), 4);
    assert_eq!(world.node_power_credit_balance("node-b"), 4);

    let budget = world
        .system_order_pool_budget(6)
        .expect("epoch 6 budget should exist");
    assert_eq!(budget.remaining_credit_budget, 0);
    assert_eq!(budget.node_credit_caps.get("node-a"), Some(&4));
    assert_eq!(budget.node_credit_caps.get("node-b"), Some(&4));
    assert_eq!(budget.node_credit_allocated.get("node-a"), Some(&4));
    assert_eq!(budget.node_credit_allocated.get("node-b"), Some(&4));
}

#[test]
fn reward_asset_settlement_mint_budget_remainder_prefers_higher_points() {
    let mut world = World::new();
    bind_node_identity(&mut world, "node-a");
    bind_node_identity(&mut world, "node-b");
    bind_node_identity(&mut world, "node-signer");
    world.set_reward_asset_config(RewardAssetConfig {
        points_per_credit: 10,
        ..RewardAssetConfig::default()
    });
    world.set_system_order_pool_budget(7, 5);
    let report = settlement_report(
        7,
        vec![settlement("node-a", 300), settlement("node-b", 100)],
    );

    let minted = world
        .apply_node_points_settlement_mint(&report, "node-signer")
        .expect("apply settlement mint with weighted budget");
    assert_eq!(minted.len(), 2);

    let node_a = minted
        .iter()
        .find(|record| record.node_id == "node-a")
        .expect("node-a record");
    let node_b = minted
        .iter()
        .find(|record| record.node_id == "node-b")
        .expect("node-b record");
    assert_eq!(node_a.minted_power_credits, 4);
    assert_eq!(node_b.minted_power_credits, 1);
    assert_eq!(world.node_power_credit_balance("node-a"), 4);
    assert_eq!(world.node_power_credit_balance("node-b"), 1);
}

#[test]
fn reward_asset_snapshot_roundtrip_persists_system_order_pool_budget() {
    let mut world = World::new();
    bind_node_identity(&mut world, "node-a");
    bind_node_identity(&mut world, "node-b");
    bind_node_identity(&mut world, "node-signer");
    world.set_reward_asset_config(RewardAssetConfig {
        points_per_credit: 10,
        ..RewardAssetConfig::default()
    });
    world.set_system_order_pool_budget(8, 6);
    let report = settlement_report(
        8,
        vec![settlement("node-a", 200), settlement("node-b", 100)],
    );
    world
        .apply_node_points_settlement_mint(&report, "node-signer")
        .expect("mint with budget");

    let snapshot = world.snapshot();
    let restored = World::from_snapshot(snapshot, world.journal().clone()).expect("restore");
    let budget = restored
        .system_order_pool_budget(8)
        .expect("budget should persist");
    assert_eq!(budget.total_credit_budget, 6);
    assert_eq!(budget.remaining_credit_budget, 0);
    assert_eq!(budget.node_credit_allocated.get("node-a"), Some(&4));
    assert_eq!(budget.node_credit_allocated.get("node-b"), Some(&2));
}

#[test]
fn reward_asset_redeem_power_signed_action_succeeds_when_policy_requires_signature() {
    let mut world = World::new();
    bind_node_identity(&mut world, "node-a");
    let signer_private_key = bind_node_identity_with_seed(&mut world, "node-signer", 21);
    world.set_reward_signature_governance_policy(RewardSignatureGovernancePolicy {
        require_mintsig_v2: false,
        allow_mintsig_v1_fallback: true,
        require_redeem_signature: true,
        require_redeem_signer_match_node_id: false,
    });
    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        pos: oasis7_agent_api::world_action::GeoPos::new(0, 0, 0),
    });
    world.step().expect("register target agent");
    world.set_reward_asset_config(RewardAssetConfig {
        credits_per_power_unit: 2,
        ..RewardAssetConfig::default()
    });
    world.set_protocol_power_reserve(ProtocolPowerReserve {
        epoch_index: 15,
        available_power_units: 100,
        redeemed_power_units: 0,
    });
    world
        .mint_node_power_credits("node-a", 6)
        .expect("mint node credits");

    let signer_public_key = world
        .node_identity_public_key("node-signer")
        .expect("signer public key");
    let signature = reward_redeem_signature_v1(
        "node-a",
        "agent-1",
        4,
        9,
        "node-signer",
        signer_public_key,
        signer_private_key.as_str(),
    )
    .expect("build redeem signature");
    world.submit_action(Action::RedeemPowerSigned {
        node_id: "node-a".to_string(),
        target_agent_id: "agent-1".to_string(),
        redeem_credits: 4,
        nonce: 9,
        signer_node_id: "node-signer".to_string(),
        signature,
    });
    world.step().expect("signed redeem");

    assert_eq!(world.node_power_credit_balance("node-a"), 2);
    assert_eq!(world.protocol_power_reserve().available_power_units, 98);
    assert_eq!(world.protocol_power_reserve().redeemed_power_units, 2);
}

#[test]
fn reward_asset_redeem_power_signed_rejects_signer_node_mismatch_when_policy_requires_match() {
    let mut world = World::new();
    bind_node_identity(&mut world, "node-a");
    let signer_private_key = bind_node_identity_with_seed(&mut world, "node-signer", 34);
    world.set_reward_signature_governance_policy(RewardSignatureGovernancePolicy {
        require_mintsig_v2: false,
        allow_mintsig_v1_fallback: true,
        require_redeem_signature: true,
        require_redeem_signer_match_node_id: true,
    });
    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        pos: oasis7_agent_api::world_action::GeoPos::new(0, 0, 0),
    });
    world.step().expect("register target agent");
    world.set_reward_asset_config(RewardAssetConfig {
        credits_per_power_unit: 2,
        ..RewardAssetConfig::default()
    });
    world.set_protocol_power_reserve(ProtocolPowerReserve {
        epoch_index: 15,
        available_power_units: 100,
        redeemed_power_units: 0,
    });
    world
        .mint_node_power_credits("node-a", 6)
        .expect("mint node credits");

    let signer_public_key = world
        .node_identity_public_key("node-signer")
        .expect("signer public key");
    let signature = reward_redeem_signature_v1(
        "node-a",
        "agent-1",
        4,
        3,
        "node-signer",
        signer_public_key,
        signer_private_key.as_str(),
    )
    .expect("build redeem signature");
    world.submit_action(Action::RedeemPowerSigned {
        node_id: "node-a".to_string(),
        target_agent_id: "agent-1".to_string(),
        redeem_credits: 4,
        nonce: 3,
        signer_node_id: "node-signer".to_string(),
        signature,
    });
    world.step().expect("redeem should be rejected");

    let event = world.journal().events.last().expect("reject event");
    match &event.body {
        WorldEventBody::Domain(DomainEvent::PowerRedeemRejected { reason, .. }) => {
            assert!(reason.contains("must match node_id"));
        }
        other => panic!("expected PowerRedeemRejected, got {other:?}"),
    }
    assert_eq!(world.node_power_credit_balance("node-a"), 6);
    assert_eq!(world.protocol_power_reserve().available_power_units, 100);
    assert_eq!(world.protocol_power_reserve().redeemed_power_units, 0);
}

#[test]
fn reward_asset_redeem_power_rejects_unsigned_when_policy_requires_signature() {
    let mut world = World::new();
    bind_node_identity(&mut world, "node-a");
    world.set_reward_signature_governance_policy(RewardSignatureGovernancePolicy {
        require_mintsig_v2: false,
        allow_mintsig_v1_fallback: true,
        require_redeem_signature: true,
        require_redeem_signer_match_node_id: false,
    });
    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        pos: oasis7_agent_api::world_action::GeoPos::new(0, 0, 0),
    });
    world.step().expect("register target agent");
    world.set_protocol_power_reserve(ProtocolPowerReserve {
        epoch_index: 15,
        available_power_units: 100,
        redeemed_power_units: 0,
    });
    world
        .mint_node_power_credits("node-a", 5)
        .expect("mint node credits");

    world.submit_action(Action::RedeemPower {
        node_id: "node-a".to_string(),
        target_agent_id: "agent-1".to_string(),
        redeem_credits: 2,
        nonce: 1,
    });
    world.step().expect("unsigned redeem should be rejected");
    let event = world.journal().events.last().expect("reject event");
    match &event.body {
        WorldEventBody::Domain(DomainEvent::PowerRedeemRejected { reason, .. }) => {
            assert!(reason.contains("redeem signature is required"));
        }
        other => panic!("expected PowerRedeemRejected, got {other:?}"),
    }
}

#[test]
fn reward_asset_redeem_power_signed_rejects_invalid_signature() {
    let mut world = World::new();
    bind_node_identity(&mut world, "node-a");
    let signer_private_key = bind_node_identity_with_seed(&mut world, "node-signer", 29);
    world.set_reward_signature_governance_policy(RewardSignatureGovernancePolicy {
        require_mintsig_v2: false,
        allow_mintsig_v1_fallback: true,
        require_redeem_signature: true,
        require_redeem_signer_match_node_id: false,
    });
    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        pos: oasis7_agent_api::world_action::GeoPos::new(0, 0, 0),
    });
    world.step().expect("register target agent");
    world.set_protocol_power_reserve(ProtocolPowerReserve {
        epoch_index: 16,
        available_power_units: 100,
        redeemed_power_units: 0,
    });
    world
        .mint_node_power_credits("node-a", 5)
        .expect("mint node credits");

    let signer_public_key = world
        .node_identity_public_key("node-signer")
        .expect("signer public key");
    let signature = reward_redeem_signature_v1(
        "node-a",
        "agent-1",
        3,
        2,
        "node-signer",
        signer_public_key,
        signer_private_key.as_str(),
    )
    .expect("build redeem signature");
    world.submit_action(Action::RedeemPowerSigned {
        node_id: "node-a".to_string(),
        target_agent_id: "agent-1".to_string(),
        redeem_credits: 4,
        nonce: 2,
        signer_node_id: "node-signer".to_string(),
        signature,
    });
    world.step().expect("redeem should be rejected");

    let event = world.journal().events.last().expect("reject event");
    match &event.body {
        WorldEventBody::Domain(DomainEvent::PowerRedeemRejected { reason, .. }) => {
            assert!(reason.contains("signature verification failed"));
        }
        other => panic!("expected PowerRedeemRejected, got {other:?}"),
    }
}

#[test]
fn reward_asset_redeem_power_rejects_unbound_node_identity() {
    let mut world = World::new();
    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        pos: oasis7_agent_api::world_action::GeoPos::new(0, 0, 0),
    });
    world.step().expect("register target agent");
    world.set_reward_asset_config(RewardAssetConfig {
        credits_per_power_unit: 1,
        ..RewardAssetConfig::default()
    });
    world.set_protocol_power_reserve(ProtocolPowerReserve {
        epoch_index: 15,
        available_power_units: 100,
        redeemed_power_units: 0,
    });
    world
        .mint_node_power_credits("node-a", 5)
        .expect("mint node credits");

    world.submit_action(Action::RedeemPower {
        node_id: "node-a".to_string(),
        target_agent_id: "agent-1".to_string(),
        redeem_credits: 2,
        nonce: 1,
    });
    world.step().expect("redeem should be rejected");
    let event = world.journal().events.last().expect("reject event");
    match &event.body {
        WorldEventBody::Domain(DomainEvent::PowerRedeemRejected { reason, .. }) => {
            assert!(reason.contains("node identity not bound"));
        }
        other => panic!("expected PowerRedeemRejected, got {other:?}"),
    }
}

#[test]
fn reward_asset_redeem_power_action_updates_balances_and_reserve() {
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
        credits_per_power_unit: 4,
        ..RewardAssetConfig::default()
    });
    world.set_protocol_power_reserve(ProtocolPowerReserve {
        epoch_index: 2,
        available_power_units: 50,
        redeemed_power_units: 0,
    });
    world
        .mint_node_power_credits("node-a", 20)
        .expect("mint node credits");

    world.submit_action(Action::RedeemPower {
        node_id: "node-a".to_string(),
        target_agent_id: "agent-1".to_string(),
        redeem_credits: 9,
        nonce: 1,
    });
    world.step().expect("redeem power");

    assert_eq!(world.node_power_credit_balance("node-a"), 11);
    assert_eq!(
        world
            .agent_resource_balance("agent-1", oasis7_agent_api::world_action::ResourceKind::Electricity)
            .expect("query target electricity after redeem"),
        initial_electricity + 2
    );
    assert_eq!(world.protocol_power_reserve().available_power_units, 48);
    assert_eq!(world.protocol_power_reserve().redeemed_power_units, 2);

    let event = world.journal().events.last().expect("redeem event");
    match &event.body {
        WorldEventBody::Domain(DomainEvent::PowerRedeemed {
            node_id,
            target_agent_id,
            burned_credits,
            granted_power_units,
            reserve_remaining,
            nonce,
        }) => {
            assert_eq!(node_id, "node-a");
            assert_eq!(target_agent_id, "agent-1");
            assert_eq!(*burned_credits, 9);
            assert_eq!(*granted_power_units, 2);
            assert_eq!(*reserve_remaining, 48);
            assert_eq!(*nonce, 1);
        }
        other => panic!("expected PowerRedeemed, got {other:?}"),
    }
}

#[test]
fn reward_asset_redeem_power_overflow_keeps_state_atomic() {
    let mut world = World::new();
    bind_node_identity(&mut world, "node-a");
    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        pos: oasis7_agent_api::world_action::GeoPos::new(0, 0, 0),
    });
    world.step().expect("register target agent");
    world
        .set_agent_resource_balance(
            "agent-1",
            oasis7_agent_api::world_action::ResourceKind::Electricity,
            i64::MAX,
        )
        .expect("seed target electricity at boundary");
    world.set_reward_asset_config(RewardAssetConfig {
        credits_per_power_unit: 1,
        ..RewardAssetConfig::default()
    });
    world.set_protocol_power_reserve(ProtocolPowerReserve {
        epoch_index: 2,
        available_power_units: 50,
        redeemed_power_units: 0,
    });
    world
        .mint_node_power_credits("node-a", 20)
        .expect("mint node credits");
    let events_before = world.journal().len();

    world.submit_action(Action::RedeemPower {
        node_id: "node-a".to_string(),
        target_agent_id: "agent-1".to_string(),
        redeem_credits: 1,
        nonce: 1,
    });
    let err = world.step().expect_err("redeem overflow must fail");
    assert!(
        matches!(err, WorldError::ResourceBalanceInvalid { .. }),
        "unexpected error: {err:?}"
    );

    assert_eq!(world.node_power_credit_balance("node-a"), 20);
    assert_eq!(world.protocol_power_reserve().available_power_units, 50);
    assert_eq!(world.protocol_power_reserve().redeemed_power_units, 0);
    assert_eq!(world.node_last_redeem_nonce("node-a"), None);
    assert_eq!(
        world
            .agent_resource_balance("agent-1", oasis7_agent_api::world_action::ResourceKind::Electricity)
            .expect("query target electricity"),
        i64::MAX
    );
    assert_eq!(world.journal().len(), events_before);
}
