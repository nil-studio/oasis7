use super::super::*;

#[test]
fn longrun_operability_release_gate_passes_with_relaxed_thresholds() {
    let world = World::new();
    let report = world.evaluate_longrun_operability_release_gate(
        LongRunReleaseStage::Full,
        0,
        LongRunOperabilityReleaseGateThresholds {
            max_logistics_breach_bps: 10_000,
            max_pending_actions_evicted: u64::MAX,
            max_journal_events_evicted: u64::MAX,
            max_tick_consensus_rejections: usize::MAX,
            min_rollback_drills: 0,
            required_release_stage: LongRunReleaseStage::Canary,
            economy_thresholds: MainTokenEconomyAuditThresholds {
                max_net_flow_bps_of_total_supply: 10_000,
                max_epoch_issued_bps_of_total_supply: 10_000,
                max_treasury_distribution_bps_of_total_supply: 10_000,
            },
        },
    );

    assert!(report.gate_passed());
    assert!(report.violations.is_empty());
    assert!(report.economy_report.gate_passed());
}

#[test]
fn longrun_operability_release_gate_blocks_stage_and_economy_pressure() {
    let mut world = World::new();
    world.set_main_token_supply(MainTokenSupplyState {
        total_supply: 1_000,
        circulating_supply: 750,
        total_issued: 400,
        total_burned: 10,
    });
    world
        .record_main_token_epoch_issuance(MainTokenEpochIssuanceRecord {
            epoch_index: 5,
            inflation_rate_bps: 700,
            issued_amount: 200,
            staking_reward_amount: 100,
            node_service_reward_amount: 50,
            ecosystem_pool_amount: 30,
            security_reserve_amount: 20,
        })
        .expect("record issuance");

    let thresholds = LongRunOperabilityReleaseGateThresholds {
        max_logistics_breach_bps: 10_000,
        max_pending_actions_evicted: u64::MAX,
        max_journal_events_evicted: u64::MAX,
        max_tick_consensus_rejections: usize::MAX,
        min_rollback_drills: 1,
        required_release_stage: LongRunReleaseStage::Full,
        economy_thresholds: MainTokenEconomyAuditThresholds {
            max_net_flow_bps_of_total_supply: 10_000,
            max_epoch_issued_bps_of_total_supply: 1_000,
            max_treasury_distribution_bps_of_total_supply: 10_000,
        },
    };

    let report = world.evaluate_longrun_operability_release_gate(
        LongRunReleaseStage::Canary,
        5,
        thresholds.clone(),
    );
    assert!(!report.gate_passed());
    assert!(
        report
            .violations
            .iter()
            .any(|item| item.gate == "rollout.stage")
    );
    assert!(
        report
            .violations
            .iter()
            .any(|item| item.gate == "drill.rollback")
    );
    assert!(
        report
            .violations
            .iter()
            .any(|item| item.gate == "economy.epoch_issued_bps_of_total_supply")
    );

    let err = world
        .enforce_longrun_operability_release_gate(LongRunReleaseStage::Canary, 5, thresholds)
        .expect_err("gate should be blocked");
    let WorldError::ResourceBalanceInvalid { reason } = err else {
        panic!("expected resource balance invalid");
    };
    assert!(reason.contains("rollout.stage"));
}
