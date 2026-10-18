use super::super::*;

#[test]
fn main_token_economy_audit_report_tracks_source_sink_metrics_and_arbitrage_alert() {
    let mut world = World::new();
    world.set_main_token_config(MainTokenConfig {
        initial_supply: 1_500,
        ..MainTokenConfig::default()
    });
    world.submit_action(Action::InitializeMainTokenGenesis {
        allocations: vec![MainTokenGenesisAllocationPlan {
            bucket_id: "genesis_pool".to_string(),
            ratio_bps: 10_000,
            recipient: "protocol:treasury".to_string(),
            cliff_epochs: 0,
            linear_unlock_epochs: 0,
            start_epoch: 0,
        }],
    });
    world.step().expect("initialize main token genesis");
    world.set_main_token_supply(MainTokenSupplyState {
        total_supply: 1_500,
        circulating_supply: 900,
        total_issued: 600,
        total_burned: 100,
    });
    world
        .set_main_token_treasury_balance(MAIN_TOKEN_TREASURY_BUCKET_STAKING_REWARD, 300)
        .expect("set staking treasury balance");

    let proposal_id = world
        .propose_manifest_update(world.manifest().clone(), "alice")
        .expect("create governance proposal");
    world
        .shadow_proposal(proposal_id)
        .expect("shadow governance proposal");
    world
        .approve_proposal(proposal_id, "bob", ProposalDecision::Approve)
        .expect("approve governance proposal");
    world.submit_action(Action::DistributeMainTokenTreasury {
        proposal_id,
        distribution_id: "dist-audit-arb".to_string(),
        bucket_id: MAIN_TOKEN_TREASURY_BUCKET_STAKING_REWARD.to_string(),
        distributions: vec![
            MainTokenTreasuryDistribution {
                account_id: "node:alice".to_string(),
                amount: 120,
            },
            MainTokenTreasuryDistribution {
                account_id: "node:bob".to_string(),
                amount: 80,
            },
        ],
    });
    world.step().expect("distribute main token treasury");

    let epoch_index = world.state().time;
    let report = world.main_token_economy_audit_report(
        epoch_index,
        MainTokenEconomyAuditThresholds {
            max_net_flow_bps_of_total_supply: 3_500,
            max_epoch_issued_bps_of_total_supply: 1_000,
            max_treasury_distribution_bps_of_total_supply: 1_000,
        },
    );
    assert_eq!(report.epoch_index, epoch_index);
    assert_eq!(report.mint_total, 600);
    assert_eq!(report.burn_total, 100);
    assert_eq!(report.net_flow, 500);
    assert_eq!(report.treasury_distributed_this_epoch, 200);
    assert_eq!(report.net_flow_bps_of_total_supply, 3_333);
    assert_eq!(report.treasury_distribution_bps_of_total_supply, 1_333);
    assert!(!report.gate_passed());
    assert!(
        report
            .alerts
            .iter()
            .any(|alert| alert.exploit_signature == "arbitrage:treasury_distribution_pressure")
    );
}

#[test]
fn main_token_economy_gate_blocks_epoch_inflation_pressure() {
    let mut world = World::new();
    world.set_main_token_supply(MainTokenSupplyState {
        total_supply: 1_000,
        circulating_supply: 800,
        total_issued: 400,
        total_burned: 20,
    });
    world
        .record_main_token_epoch_issuance(MainTokenEpochIssuanceRecord {
            epoch_index: 9,
            inflation_rate_bps: 650,
            issued_amount: 180,
            staking_reward_amount: 90,
            node_service_reward_amount: 45,
            ecosystem_pool_amount: 30,
            security_reserve_amount: 15,
        })
        .expect("record issuance");

    let err = world
        .enforce_main_token_economy_gate(
            9,
            MainTokenEconomyAuditThresholds {
                max_net_flow_bps_of_total_supply: 10_000,
                max_epoch_issued_bps_of_total_supply: 1_200,
                max_treasury_distribution_bps_of_total_supply: 10_000,
            },
        )
        .expect_err("epoch inflation threshold should be blocked");
    let WorldError::ResourceBalanceInvalid { reason } = err else {
        panic!("expected resource balance invalid");
    };
    assert!(reason.contains("inflation:epoch_issued_pressure"));
}
