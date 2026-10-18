use super::super::*;
use ed25519_dalek::SigningKey;

#[path = "reward_asset_settlement_action/node_points_settlement_publication_transaction_regressions.rs"]
mod node_points_settlement_publication_transaction_regressions;

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

fn configure_main_token_bridge_budget(
    world: &mut World,
    epoch_index: u64,
    node_service_budget: u64,
    treasury_balance: u64,
) {
    world.set_main_token_config(MainTokenConfig {
        initial_supply: 1_000,
        ..MainTokenConfig::default()
    });
    world.set_main_token_supply(MainTokenSupplyState {
        total_supply: 1_000,
        circulating_supply: 0,
        total_issued: 0,
        total_burned: 0,
    });
    world
        .record_main_token_epoch_issuance(MainTokenEpochIssuanceRecord {
            epoch_index,
            inflation_rate_bps: 0,
            issued_amount: node_service_budget,
            staking_reward_amount: 0,
            node_service_reward_amount: node_service_budget,
            ecosystem_pool_amount: 0,
            security_reserve_amount: 0,
        })
        .expect("record main token issuance");
    world
        .set_main_token_treasury_balance(
            MAIN_TOKEN_TREASURY_BUCKET_NODE_SERVICE_REWARD,
            treasury_balance,
        )
        .expect("set node service treasury balance");
}

#[test]
fn reward_asset_settlement_action_applies_signed_records_via_step() {
    let mut world = World::new();
    configure_main_token_bridge_budget(&mut world, 20, 5, 5);
    bind_node_identity(&mut world, "node-a");
    let signer_private_key = bind_node_identity_with_seed(&mut world, "node-signer", 9);
    world.set_reward_signature_governance_policy(RewardSignatureGovernancePolicy {
        require_mintsig_v2: true,
        allow_mintsig_v1_fallback: false,
        require_redeem_signature: false,
        require_redeem_signer_match_node_id: false,
    });
    world.set_reward_asset_config(RewardAssetConfig {
        points_per_credit: 10,
        ..RewardAssetConfig::default()
    });

    let report = settlement_report(20, vec![settlement("node-a", 50)]);
    let mut preview = world.clone();
    let minted_records = preview
        .apply_node_points_settlement_mint_v2(&report, "node-signer", signer_private_key.as_str())
        .expect("build settlement records");
    assert_eq!(minted_records.len(), 1);

    world.submit_action(Action::ApplyNodePointsSettlementSigned {
        report: report.clone(),
        signer_node_id: "node-signer".to_string(),
        mint_records: minted_records.clone(),
    });
    world.step().expect("apply settlement action");

    assert_eq!(world.node_power_credit_balance("node-a"), 5);
    assert_eq!(world.reward_mint_records().len(), 1);
    assert_eq!(
        world.main_token_treasury_balance(MAIN_TOKEN_TREASURY_BUCKET_NODE_SERVICE_REWARD),
        0
    );
    let node_a_account_id = world
        .node_main_token_account("node-a")
        .expect("node-a main token account");
    assert_ne!(node_a_account_id, "node-a");
    assert_eq!(world.main_token_liquid_balance(node_a_account_id), 5);
    assert_eq!(world.main_token_supply().circulating_supply, 5);
    assert_eq!(
        world
            .main_token_node_points_bridge_record(20)
            .expect("main token bridge record")
            .total_amount,
        5
    );
    match &world.journal().events.last().expect("event").body {
        WorldEventBody::Domain(DomainEvent::NodePointsSettlementApplied {
            signer_node_id,
            settlement_hash,
            minted_records,
            main_token_bridge_total_amount,
            main_token_bridge_distributions,
            ..
        }) => {
            assert_eq!(signer_node_id, "node-signer");
            assert!(!settlement_hash.is_empty());
            assert_eq!(minted_records.len(), 1);
            assert_eq!(minted_records[0].node_id, "node-a");
            assert_eq!(minted_records[0].minted_power_credits, 5);
            assert_eq!(*main_token_bridge_total_amount, 5);
            assert_eq!(main_token_bridge_distributions.len(), 1);
            assert_eq!(main_token_bridge_distributions[0].node_id, "node-a");
            assert_eq!(
                main_token_bridge_distributions[0].account_id,
                node_a_account_id
            );
            assert_eq!(main_token_bridge_distributions[0].amount, 5);
        }
        other => panic!("expected NodePointsSettlementApplied, got {other:?}"),
    }
}

#[test]
fn reward_asset_settlement_action_bridge_remainder_prefers_higher_points() {
    let mut world = World::new();
    configure_main_token_bridge_budget(&mut world, 23, 4, 4);
    bind_node_identity(&mut world, "node-a");
    bind_node_identity(&mut world, "node-b");
    let signer_private_key = bind_node_identity_with_seed(&mut world, "node-signer", 12);
    world.set_reward_signature_governance_policy(RewardSignatureGovernancePolicy {
        require_mintsig_v2: true,
        allow_mintsig_v1_fallback: false,
        require_redeem_signature: false,
        require_redeem_signer_match_node_id: false,
    });
    world.set_reward_asset_config(RewardAssetConfig {
        points_per_credit: 1,
        ..RewardAssetConfig::default()
    });

    let report = settlement_report(23, vec![settlement("node-a", 1), settlement("node-b", 2)]);
    let mut preview = world.clone();
    let minted_records = preview
        .apply_node_points_settlement_mint_v2(&report, "node-signer", signer_private_key.as_str())
        .expect("build settlement records");

    world.submit_action(Action::ApplyNodePointsSettlementSigned {
        report,
        signer_node_id: "node-signer".to_string(),
        mint_records: minted_records,
    });
    world.step().expect("apply settlement action");

    match &world.journal().events.last().expect("event").body {
        WorldEventBody::Domain(DomainEvent::NodePointsSettlementApplied {
            main_token_bridge_total_amount,
            main_token_bridge_distributions,
            ..
        }) => {
            assert_eq!(*main_token_bridge_total_amount, 4);
            assert_eq!(
                main_token_bridge_distributions
                    .iter()
                    .map(|distribution| (distribution.node_id.as_str(), distribution.amount))
                    .collect::<Vec<_>>(),
                vec![("node-a", 1), ("node-b", 3)]
            );
        }
        other => panic!("expected NodePointsSettlementApplied, got {other:?}"),
    }
}

#[test]
fn reward_asset_settlement_action_bridge_remainder_ties_by_node_id() {
    let mut world = World::new();
    configure_main_token_bridge_budget(&mut world, 24, 5, 5);
    bind_node_identity(&mut world, "node-a");
    bind_node_identity(&mut world, "node-b");
    let signer_private_key = bind_node_identity_with_seed(&mut world, "node-signer", 13);
    world.set_reward_signature_governance_policy(RewardSignatureGovernancePolicy {
        require_mintsig_v2: true,
        allow_mintsig_v1_fallback: false,
        require_redeem_signature: false,
        require_redeem_signer_match_node_id: false,
    });
    world.set_reward_asset_config(RewardAssetConfig {
        points_per_credit: 1,
        ..RewardAssetConfig::default()
    });

    let report = settlement_report(24, vec![settlement("node-b", 1), settlement("node-a", 1)]);
    let mut preview = world.clone();
    let minted_records = preview
        .apply_node_points_settlement_mint_v2(&report, "node-signer", signer_private_key.as_str())
        .expect("build settlement records");

    world.submit_action(Action::ApplyNodePointsSettlementSigned {
        report,
        signer_node_id: "node-signer".to_string(),
        mint_records: minted_records,
    });
    world.step().expect("apply settlement action");

    match &world.journal().events.last().expect("event").body {
        WorldEventBody::Domain(DomainEvent::NodePointsSettlementApplied {
            main_token_bridge_total_amount,
            main_token_bridge_distributions,
            ..
        }) => {
            assert_eq!(*main_token_bridge_total_amount, 5);
            assert_eq!(
                main_token_bridge_distributions
                    .iter()
                    .map(|distribution| (distribution.node_id.as_str(), distribution.amount))
                    .collect::<Vec<_>>(),
                vec![("node-a", 3), ("node-b", 2)]
            );
        }
        other => panic!("expected NodePointsSettlementApplied, got {other:?}"),
    }
}

#[test]
fn reward_asset_settlement_action_rejects_tampered_mint_record() {
    let mut world = World::new();
    configure_main_token_bridge_budget(&mut world, 21, 4, 4);
    bind_node_identity(&mut world, "node-a");
    let signer_private_key = bind_node_identity_with_seed(&mut world, "node-signer", 10);
    world.set_reward_signature_governance_policy(RewardSignatureGovernancePolicy {
        require_mintsig_v2: true,
        allow_mintsig_v1_fallback: false,
        require_redeem_signature: false,
        require_redeem_signer_match_node_id: false,
    });
    world.set_reward_asset_config(RewardAssetConfig {
        points_per_credit: 10,
        ..RewardAssetConfig::default()
    });

    let report = settlement_report(21, vec![settlement("node-a", 40)]);
    let mut preview = world.clone();
    let mut minted_records = preview
        .apply_node_points_settlement_mint_v2(&report, "node-signer", signer_private_key.as_str())
        .expect("build settlement records");
    minted_records[0].signature = "mintsig:v2:deadbeef".to_string();

    world.submit_action(Action::ApplyNodePointsSettlementSigned {
        report,
        signer_node_id: "node-signer".to_string(),
        mint_records: minted_records,
    });
    world
        .step()
        .expect("settlement action should be rejected, not fail");

    assert_eq!(world.node_power_credit_balance("node-a"), 0);
    assert!(world.reward_mint_records().is_empty());
    match &world.journal().events.last().expect("event").body {
        WorldEventBody::Domain(DomainEvent::ActionRejected { reason, .. }) => match reason {
            RejectReason::RuleDenied { notes } => {
                assert!(
                    notes
                        .iter()
                        .any(|note| note.contains("mint record signature invalid"))
                );
            }
            other => panic!("expected rule denied reject, got {other:?}"),
        },
        other => panic!("expected ActionRejected, got {other:?}"),
    }
}

#[test]
fn reward_asset_settlement_action_rejects_when_main_token_bridge_treasury_insufficient() {
    let mut world = World::new();
    configure_main_token_bridge_budget(&mut world, 22, 6, 5);
    bind_node_identity(&mut world, "node-a");
    let signer_private_key = bind_node_identity_with_seed(&mut world, "node-signer", 11);
    world.set_reward_signature_governance_policy(RewardSignatureGovernancePolicy {
        require_mintsig_v2: true,
        allow_mintsig_v1_fallback: false,
        require_redeem_signature: false,
        require_redeem_signer_match_node_id: false,
    });
    world.set_reward_asset_config(RewardAssetConfig {
        points_per_credit: 10,
        ..RewardAssetConfig::default()
    });

    let report = settlement_report(22, vec![settlement("node-a", 60)]);
    let mut preview = world.clone();
    let minted_records = preview
        .apply_node_points_settlement_mint_v2(&report, "node-signer", signer_private_key.as_str())
        .expect("build settlement records");

    world.submit_action(Action::ApplyNodePointsSettlementSigned {
        report,
        signer_node_id: "node-signer".to_string(),
        mint_records: minted_records,
    });
    world
        .step()
        .expect("settlement action should reject when bridge treasury is insufficient");

    assert_eq!(world.node_power_credit_balance("node-a"), 0);
    assert!(world.reward_mint_records().is_empty());
    assert!(world.main_token_node_points_bridge_record(22).is_none());
    match &world.journal().events.last().expect("event").body {
        WorldEventBody::Domain(DomainEvent::ActionRejected { reason, .. }) => match reason {
            RejectReason::RuleDenied { notes } => {
                assert!(
                    notes
                        .iter()
                        .any(|note| { note.contains("main token bridge treasury insufficient") })
                );
            }
            other => panic!("expected RuleDenied, got {other:?}"),
        },
        other => panic!("expected ActionRejected, got {other:?}"),
    }
}
