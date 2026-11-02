use super::*;

#[test]
fn config_projection_is_shared_and_preserves_original_domains() {
    let config = MainTokenConfig {
        symbol: " Ω ".into(),
        decimals: u8::MAX,
        initial_supply: u64::MAX,
        max_supply: Some(0),
        inflation_policy: MainTokenInflationPolicy {
            base_rate_bps: 1,
            min_rate_bps: 2,
            max_rate_bps: 3,
            target_stake_ratio_bps: 4,
            stake_feedback_gain_bps: 5,
            epochs_per_year: u32::MAX,
        },
        issuance_split: MainTokenIssuanceSplitPolicy {
            staking_reward_bps: 6,
            node_service_reward_bps: 7,
            ecosystem_pool_bps: 8,
            security_reserve_bps: 9,
        },
        burn_policy: MainTokenBurnPolicy {
            gas_base_fee_burn_bps: 10,
            slash_burn_bps: 11,
            module_fee_burn_bps: 12,
        },
    };
    let update = MainTokenScheduledPolicyUpdate {
        proposal_id: u64::MAX,
        effective_epoch: 0,
        next_config: config.clone(),
    };
    fn render(p: MemberPayloadRefV1<'_>) -> String {
        match p {
            MemberPayloadRefV1::Unit => "none".into(),
            MemberPayloadRefV1::U64(n) => n.to_string(),
            MemberPayloadRefV1::Text(s) => s.to_owned(),
            _ => panic!("unexpected config primitive"),
        }
    }
    let mut global = Vec::new();
    visit_main_token_policy_record_v1(MainTokenPolicyRecordRefV1::Config(&config), |c, p| {
        if let MainTokenPolicyCellV1::Config(c) = c {
            global.push((c.name(), render(p)));
        }
        Ok::<_, MemberRecordError>(())
    })
    .unwrap();
    let mut scheduled = Vec::new();
    visit_main_token_policy_record_v1(MainTokenPolicyRecordRefV1::Scheduled(&update), |c, p| {
        if let MainTokenPolicyCellV1::NextConfig(c) = c {
            scheduled.push((c.name(), render(p)));
        }
        Ok::<_, MemberRecordError>(())
    })
    .unwrap();
    let expected = [
        ("symbol", " Ω "),
        ("decimals", "255"),
        ("initial_supply", "18446744073709551615"),
        ("max_supply", "0"),
        ("base_rate_bps", "1"),
        ("min_rate_bps", "2"),
        ("max_rate_bps", "3"),
        ("target_stake_ratio_bps", "4"),
        ("stake_feedback_gain_bps", "5"),
        ("epochs_per_year", "4294967295"),
        ("staking_reward_bps", "6"),
        ("node_service_reward_bps", "7"),
        ("ecosystem_pool_bps", "8"),
        ("security_reserve_bps", "9"),
        ("gas_base_fee_burn_bps", "10"),
        ("slash_burn_bps", "11"),
        ("module_fee_burn_bps", "12"),
    ]
    .map(|(k, v)| (k, v.to_owned()));
    assert_eq!(global, expected);
    assert_eq!(scheduled, expected);
    assert!(!MainTokenConfigColumnV1::Decimals.accepts(MemberPayloadRefV1::U64(256)));
    assert!(
        !MainTokenConfigColumnV1::BaseRateBps
            .accepts(MemberPayloadRefV1::U64(u64::from(u32::MAX) + 1))
    );
    assert!(MainTokenConfigColumnV1::MaxSupply.accepts(MemberPayloadRefV1::Unit));
    assert!(MainTokenConfigColumnV1::MaxSupply.accepts(MemberPayloadRefV1::U64(0)));
    assert!(!MainTokenPolicyColumnV1::Status.accepts(MemberPayloadRefV1::Text("Issued")));
}
