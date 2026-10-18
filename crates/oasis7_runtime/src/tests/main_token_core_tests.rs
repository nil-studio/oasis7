#[test]
fn main_token_queries_return_defaults_when_uninitialized() {
    let world = World::new();

    let config = world.main_token_config();
    assert_eq!(config.symbol, "OC");
    assert_eq!(config.decimals, 9);
    assert_eq!(config.initial_supply, 0);
    assert_eq!(world.main_token_liquid_balance("missing-account"), 0);
    assert_eq!(world.main_token_treasury_balance("missing-bucket"), 0);
    assert!(world.main_token_genesis_bucket("missing-bucket").is_none());
    assert!(world.main_token_epoch_issuance_record(1).is_none());
    assert!(world
        .main_token_treasury_distribution_record("missing-distribution")
        .is_none());
}

#[test]
fn production_hardened_main_token_config_freezes_10b_supply() {
    let config = production_hardened_main_token_config();
    assert_eq!(config.symbol, "OC");
    assert_eq!(config.decimals, 9);
    assert_eq!(config.initial_supply, FROZEN_MAIN_TOKEN_INITIAL_SUPPLY);
    assert_eq!(config.max_supply, None);
}

fn frozen_supply_genesis_sheet_fixture() -> Vec<MainTokenGenesisAllocationPlan> {
    vec![
        MainTokenGenesisAllocationPlan {
            bucket_id: "team_long_term_vesting".to_string(),
            ratio_bps: 2_000,
            recipient: "protocol:team-core-vesting".to_string(),
            cliff_epochs: 365,
            linear_unlock_epochs: 1_095,
            start_epoch: 0,
        },
        MainTokenGenesisAllocationPlan {
            bucket_id: "early_contributor_reward_reserve".to_string(),
            ratio_bps: 1_500,
            recipient: "protocol:early-contributor-reward".to_string(),
            cliff_epochs: 0,
            linear_unlock_epochs: 3_650,
            start_epoch: 0,
        },
        MainTokenGenesisAllocationPlan {
            bucket_id: "node_service_genesis_custody".to_string(),
            ratio_bps: 2_000,
            recipient: "protocol:node-service-custody".to_string(),
            cliff_epochs: 180,
            linear_unlock_epochs: 1_825,
            start_epoch: 0,
        },
        MainTokenGenesisAllocationPlan {
            bucket_id: "staking_genesis_custody".to_string(),
            ratio_bps: 1_500,
            recipient: "protocol:staking-custody".to_string(),
            cliff_epochs: 180,
            linear_unlock_epochs: 1_825,
            start_epoch: 0,
        },
        MainTokenGenesisAllocationPlan {
            bucket_id: "ecosystem_governance_reserve".to_string(),
            ratio_bps: 1_500,
            recipient: "protocol:ecosystem-governance".to_string(),
            cliff_epochs: 90,
            linear_unlock_epochs: 1_460,
            start_epoch: 0,
        },
        MainTokenGenesisAllocationPlan {
            bucket_id: "security_reserve_emergency".to_string(),
            ratio_bps: 1_000,
            recipient: "protocol:security-council-reserve".to_string(),
            cliff_epochs: 0,
            linear_unlock_epochs: 0,
            start_epoch: 0,
        },
        MainTokenGenesisAllocationPlan {
            bucket_id: "foundation_ops_reserve".to_string(),
            ratio_bps: 500,
            recipient: "protocol:foundation-ops".to_string(),
            cliff_epochs: 90,
            linear_unlock_epochs: 730,
            start_epoch: 0,
        },
    ]
}

#[test]
fn production_hardened_genesis_plan_matches_frozen_10b_sheet() {
    let mut world = World::new();
    world.set_main_token_config(production_hardened_main_token_config());

    let plans = frozen_supply_genesis_sheet_fixture();
    assert_eq!(plans.len(), 7);
    assert_eq!(
        plans
            .iter()
            .map(|plan| u64::from(plan.ratio_bps))
            .sum::<u64>(),
        10_000
    );

    world.submit_action(Action::InitializeMainTokenGenesis { allocations: plans });
    world
        .step()
        .expect("initialize production-hardened main token genesis");

    assert_eq!(
        world.main_token_supply().total_supply,
        FROZEN_MAIN_TOKEN_INITIAL_SUPPLY
    );
    assert_eq!(world.main_token_supply().circulating_supply, 0);
    assert_eq!(
        world
            .main_token_genesis_bucket("team_long_term_vesting")
            .expect("team bucket")
            .allocated_amount,
        2_000_000_000
    );
    assert_eq!(
        world
            .main_token_genesis_bucket("early_contributor_reward_reserve")
            .expect("reward bucket")
            .allocated_amount,
        1_500_000_000
    );
    assert_eq!(
        world
            .main_token_genesis_bucket("node_service_genesis_custody")
            .expect("node service bucket")
            .allocated_amount,
        2_000_000_000
    );
    assert_eq!(
        world
            .main_token_genesis_bucket("staking_genesis_custody")
            .expect("staking bucket")
            .allocated_amount,
        1_500_000_000
    );
    assert_eq!(
        world
            .main_token_genesis_bucket("ecosystem_governance_reserve")
            .expect("ecosystem bucket")
            .allocated_amount,
        1_500_000_000
    );
    assert_eq!(
        world
            .main_token_genesis_bucket("security_reserve_emergency")
            .expect("security bucket")
            .allocated_amount,
        1_000_000_000
    );
    assert_eq!(
        world
            .main_token_genesis_bucket("foundation_ops_reserve")
            .expect("ops bucket")
            .allocated_amount,
        500_000_000
    );
    assert_eq!(
        world
            .main_token_account_balance("protocol:team-core-vesting")
            .expect("team account")
            .vested_balance,
        2_000_000_000
    );
    assert_eq!(
        world
            .main_token_account_balance("protocol:early-contributor-reward")
            .expect("reward account")
            .vested_balance,
        1_500_000_000
    );
}

#[test]
fn enabling_production_release_policy_leaves_main_token_config_unchanged() {
    let mut world = World::new();
    assert_eq!(world.main_token_config().initial_supply, 0);

    world.enable_production_release_policy();
    assert_eq!(world.main_token_config().initial_supply, 0);

    let mut custom_world = World::new();
    custom_world.set_main_token_config(MainTokenConfig {
        symbol: "OC".to_string(),
        decimals: 9,
        initial_supply: 777,
        max_supply: Some(999),
        inflation_policy: MainTokenInflationPolicy::default(),
        issuance_split: MainTokenIssuanceSplitPolicy::default(),
        burn_policy: MainTokenBurnPolicy::default(),
    });
    custom_world.enable_production_release_policy();
    assert_eq!(custom_world.main_token_config().initial_supply, 777);
    assert_eq!(custom_world.main_token_config().max_supply, Some(999));
}

#[test]
fn setting_production_release_policy_leaves_main_token_config_unchanged() {
    let mut world = World::new();
    world.set_release_security_policy(ReleaseSecurityPolicy::production_hardened());
    assert_eq!(world.main_token_config().initial_supply, 0);

    let mut custom_world = World::new();
    custom_world.set_main_token_config(MainTokenConfig {
        symbol: "OC".to_string(),
        decimals: 9,
        initial_supply: 555,
        max_supply: Some(888),
        inflation_policy: MainTokenInflationPolicy::default(),
        issuance_split: MainTokenIssuanceSplitPolicy::default(),
        burn_policy: MainTokenBurnPolicy::default(),
    });
    custom_world.set_release_security_policy(ReleaseSecurityPolicy::production_hardened());
    assert_eq!(custom_world.main_token_config().initial_supply, 555);
    assert_eq!(custom_world.main_token_config().max_supply, Some(888));
}

#[test]
fn main_token_snapshot_roundtrip_persists_state() {
    let mut world = World::new();
    world.set_main_token_config(MainTokenConfig {
        symbol: "OC".to_string(),
        decimals: 9,
        initial_supply: 1_000_000_000,
        max_supply: Some(5_000_000_000),
        inflation_policy: MainTokenInflationPolicy {
            base_rate_bps: 410,
            ..MainTokenInflationPolicy::default()
        },
        issuance_split: MainTokenIssuanceSplitPolicy {
            node_service_reward_bps: 2_200,
            ..MainTokenIssuanceSplitPolicy::default()
        },
        burn_policy: MainTokenBurnPolicy {
            gas_base_fee_burn_bps: 3_200,
            ..MainTokenBurnPolicy::default()
        },
    });
    world.set_main_token_supply(MainTokenSupplyState {
        total_supply: 1_050_000_000,
        circulating_supply: 820_000_000,
        total_issued: 100_000_000,
        total_burned: 50_000_000,
    });
    world
        .set_main_token_account_balance("protocol:treasury", 320_000_000, 0)
        .expect("set treasury account balance");
    world
        .set_main_token_account_balance("player:alice", 1_250_000, 350_000)
        .expect("set alice account balance");
    world
        .set_main_token_treasury_balance("ecosystem_pool", 120_000_000)
        .expect("set ecosystem pool");
    world
        .set_main_token_genesis_bucket(MainTokenGenesisAllocationBucketState {
            bucket_id: "ecosystem_growth_pool".to_string(),
            ratio_bps: 2_500,
            recipient: "protocol:treasury".to_string(),
            cliff_epochs: 30,
            linear_unlock_epochs: 360,
            start_epoch: 1,
            allocated_amount: 250_000_000,
            claimed_amount: 20_000_000,
        })
        .expect("set genesis bucket");
    world
        .record_main_token_epoch_issuance(MainTokenEpochIssuanceRecord {
            epoch_index: 12,
            inflation_rate_bps: 405,
            issued_amount: 1_337_000,
            staking_reward_amount: 802_200,
            node_service_reward_amount: 267_400,
            ecosystem_pool_amount: 200_550,
            security_reserve_amount: 66_850,
        })
        .expect("record issuance");

    let snapshot = world.snapshot();
    let restored = World::from_snapshot(snapshot, world.journal().clone()).expect("restore");

    assert_eq!(restored.main_token_config().initial_supply, 1_000_000_000);
    assert_eq!(restored.main_token_supply().total_supply, 1_050_000_000);
    assert_eq!(
        restored.main_token_liquid_balance("protocol:treasury"),
        320_000_000
    );
    assert_eq!(
        restored.main_token_liquid_balance("player:alice"),
        1_250_000
    );
    assert_eq!(
        restored.main_token_account_balance("player:alice"),
        Some(&MainTokenAccountBalance {
            account_id: "player:alice".to_string(),
            liquid_balance: 1_250_000,
            vested_balance: 350_000,
            restricted_starter_claim_balance: 0,
        })
    );
    assert_eq!(
        restored.main_token_treasury_balance("ecosystem_pool"),
        120_000_000
    );
    assert_eq!(
        restored
            .main_token_genesis_bucket("ecosystem_growth_pool")
            .expect("genesis bucket"),
        &MainTokenGenesisAllocationBucketState {
            bucket_id: "ecosystem_growth_pool".to_string(),
            ratio_bps: 2_500,
            recipient: "protocol:treasury".to_string(),
            cliff_epochs: 30,
            linear_unlock_epochs: 360,
            start_epoch: 1,
            allocated_amount: 250_000_000,
            claimed_amount: 20_000_000,
        }
    );
    assert_eq!(
        restored.main_token_epoch_issuance_record(12),
        Some(&MainTokenEpochIssuanceRecord {
            epoch_index: 12,
            inflation_rate_bps: 405,
            issued_amount: 1_337_000,
            staking_reward_amount: 802_200,
            node_service_reward_amount: 267_400,
            ecosystem_pool_amount: 200_550,
            security_reserve_amount: 66_850,
        })
    );
}

#[test]
fn main_token_initialize_genesis_action_populates_buckets_and_vested_balances() {
    let mut world = World::new();
    world.set_main_token_config(MainTokenConfig {
        initial_supply: 1_000,
        ..MainTokenConfig::default()
    });

    world.submit_action(Action::InitializeMainTokenGenesis {
        allocations: vec![
            MainTokenGenesisAllocationPlan {
                bucket_id: "consensus_bootstrap_pool".to_string(),
                ratio_bps: 6_000,
                recipient: "node:validator".to_string(),
                cliff_epochs: 0,
                linear_unlock_epochs: 10,
                start_epoch: 0,
            },
            MainTokenGenesisAllocationPlan {
                bucket_id: "ecosystem_growth_pool".to_string(),
                ratio_bps: 4_000,
                recipient: "protocol:treasury".to_string(),
                cliff_epochs: 0,
                linear_unlock_epochs: 20,
                start_epoch: 0,
            },
        ],
    });
    world.step().expect("initialize main token genesis");

    assert_eq!(world.main_token_supply().total_supply, 1_000);
    assert_eq!(world.main_token_supply().circulating_supply, 0);
    assert_eq!(world.main_token_liquid_balance("node:validator"), 0);
    assert_eq!(world.main_token_liquid_balance("protocol:treasury"), 0);
    assert_eq!(
        world
            .main_token_account_balance("node:validator")
            .expect("validator account")
            .vested_balance,
        600
    );
    assert_eq!(
        world
            .main_token_account_balance("protocol:treasury")
            .expect("treasury account")
            .vested_balance,
        400
    );
    let total_allocated = world
        .state()
        .main_token_genesis_buckets
        .values()
        .map(|bucket| bucket.allocated_amount)
        .sum::<u64>();
    assert_eq!(total_allocated, 1_000);
    match &world.journal().events.last().expect("event").body {
        WorldEventBody::Domain(DomainEvent::MainTokenGenesisInitialized {
            total_supply,
            allocations,
        }) => {
            assert_eq!(*total_supply, 1_000);
            assert_eq!(allocations.len(), 2);
        }
        other => panic!("expected MainTokenGenesisInitialized, got {other:?}"),
    }
}

#[test]
fn main_token_claim_vesting_action_releases_unlocked_balance_and_rejects_nonce_replay() {
    let mut world = World::new();
    world.set_main_token_config(MainTokenConfig {
        initial_supply: 1_000,
        ..MainTokenConfig::default()
    });
    world.submit_action(Action::InitializeMainTokenGenesis {
        allocations: vec![MainTokenGenesisAllocationPlan {
            bucket_id: "core_contributor_vesting".to_string(),
            ratio_bps: 10_000,
            recipient: "player:alice".to_string(),
            cliff_epochs: 0,
            linear_unlock_epochs: 10,
            start_epoch: 0,
        }],
    });
    world.step().expect("initialize main token genesis");

    world.submit_action(Action::ClaimMainTokenVesting {
        bucket_id: "core_contributor_vesting".to_string(),
        beneficiary: "player:alice".to_string(),
        nonce: 1,
    });
    world.step().expect("claim vesting");

    let alice = world
        .main_token_account_balance("player:alice")
        .expect("alice account");
    assert_eq!(alice.liquid_balance, 200);
    assert_eq!(alice.vested_balance, 800);
    assert_eq!(world.main_token_supply().circulating_supply, 200);
    assert_eq!(world.main_token_last_claim_nonce("player:alice"), Some(1));
    assert_eq!(
        world
            .main_token_genesis_bucket("core_contributor_vesting")
            .expect("bucket")
            .claimed_amount,
        200
    );

    world.submit_action(Action::ClaimMainTokenVesting {
        bucket_id: "core_contributor_vesting".to_string(),
        beneficiary: "player:alice".to_string(),
        nonce: 1,
    });
    world.step().expect("nonce replay should be rejected");

    match &world.journal().events.last().expect("event").body {
        WorldEventBody::Domain(DomainEvent::ActionRejected { reason, .. }) => match reason {
            RejectReason::RuleDenied { notes } => {
                assert!(notes.iter().any(|note| note.contains("nonce replay")));
            }
            other => panic!("expected RuleDenied, got {other:?}"),
        },
        other => panic!("expected ActionRejected, got {other:?}"),
    }
}

#[test]
fn main_token_transfer_action_moves_liquid_balance_and_updates_nonce() {
    let mut world = World::new();
    world.set_main_token_config(MainTokenConfig {
        initial_supply: 1_000,
        ..MainTokenConfig::default()
    });
    world.set_main_token_supply(MainTokenSupplyState {
        total_supply: 1_000,
        circulating_supply: 1_000,
        total_issued: 0,
        total_burned: 0,
    });
    world
        .set_main_token_account_balance("player:alice", 900, 0)
        .expect("seed alice");
    world
        .set_main_token_account_balance("player:bob", 100, 0)
        .expect("seed bob");

    world.submit_action(Action::TransferMainToken {
        from_account_id: "player:alice".to_string(),
        to_account_id: "player:bob".to_string(),
        amount: 250,
        nonce: 1,
        asset_id: Some("main_token".to_string()),
        memo: Some("bridge:deposit:alpha".to_string()),
        chain_id: None,
        network_id: None,
        tx_version: None,
        tx_type: None,
        valid_until_unix_ms: None,
        max_fee: None,
        fee_asset_id: None,
        application_payload_hash: None,
        client_request_id: None,
    });
    world.step().expect("transfer main token");

    assert_eq!(world.main_token_liquid_balance("player:alice"), 650);
    assert_eq!(world.main_token_liquid_balance("player:bob"), 350);
    assert_eq!(
        world.main_token_last_transfer_nonce("player:alice"),
        Some(1)
    );
    assert_eq!(world.main_token_supply().total_supply, 1_000);
    assert_eq!(world.main_token_supply().circulating_supply, 1_000);

    match &world.journal().events.last().expect("event").body {
        WorldEventBody::Domain(DomainEvent::MainTokenTransferred {
            from_account_id,
            to_account_id,
            amount,
            nonce,
            asset_id,
            memo,
        }) => {
            assert_eq!(from_account_id, "player:alice");
            assert_eq!(to_account_id, "player:bob");
            assert_eq!(*amount, 250);
            assert_eq!(*nonce, 1);
            assert_eq!(asset_id.as_deref(), Some("main_token"));
            assert_eq!(memo.as_deref(), Some("bridge:deposit:alpha"));
        }
        other => panic!("expected MainTokenTransferred, got {other:?}"),
    }
}

#[test]
fn main_token_transfer_action_rejects_insufficient_balance_without_mutation() {
    let mut world = World::new();
    world.set_main_token_config(MainTokenConfig {
        initial_supply: 100,
        ..MainTokenConfig::default()
    });
    world.set_main_token_supply(MainTokenSupplyState {
        total_supply: 100,
        circulating_supply: 100,
        total_issued: 0,
        total_burned: 0,
    });
    world
        .set_main_token_account_balance("player:alice", 10, 0)
        .expect("seed alice");
    world
        .set_main_token_account_balance("player:bob", 0, 0)
        .expect("seed bob");

    world.submit_action(Action::TransferMainToken {
        from_account_id: "player:alice".to_string(),
        to_account_id: "player:bob".to_string(),
        amount: 11,
        nonce: 1,
        asset_id: Some("main_token".to_string()),
        memo: Some("overspend".to_string()),
        chain_id: None,
        network_id: None,
        tx_version: None,
        tx_type: None,
        valid_until_unix_ms: None,
        max_fee: None,
        fee_asset_id: None,
        application_payload_hash: None,
        client_request_id: None,
    });
    world
        .step()
        .expect("insufficient transfer should be rejected");

    assert_eq!(world.main_token_liquid_balance("player:alice"), 10);
    assert_eq!(world.main_token_liquid_balance("player:bob"), 0);
    assert_eq!(world.main_token_last_transfer_nonce("player:alice"), None);

    match &world.journal().events.last().expect("event").body {
        WorldEventBody::Domain(DomainEvent::ActionRejected { reason, .. }) => match reason {
            RejectReason::RuleDenied { notes } => {
                assert!(notes.iter().any(|note| note.contains("insufficient")));
            }
            other => panic!("expected RuleDenied, got {other:?}"),
        },
        other => panic!("expected ActionRejected, got {other:?}"),
    }
}

#[test]
fn main_token_transfer_action_rejects_nonce_replay() {
    let mut world = World::new();
    world.set_main_token_config(MainTokenConfig {
        initial_supply: 100,
        ..MainTokenConfig::default()
    });
    world.set_main_token_supply(MainTokenSupplyState {
        total_supply: 100,
        circulating_supply: 100,
        total_issued: 0,
        total_burned: 0,
    });
    world
        .set_main_token_account_balance("player:alice", 100, 0)
        .expect("seed alice");
    world
        .set_main_token_account_balance("player:bob", 0, 0)
        .expect("seed bob");

    world.submit_action(Action::TransferMainToken {
        from_account_id: "player:alice".to_string(),
        to_account_id: "player:bob".to_string(),
        amount: 30,
        nonce: 1,
        asset_id: None,
        memo: None,
        chain_id: None,
        network_id: None,
        tx_version: None,
        tx_type: None,
        valid_until_unix_ms: None,
        max_fee: None,
        fee_asset_id: None,
        application_payload_hash: None,
        client_request_id: None,
    });
    world.step().expect("first transfer");
    assert_eq!(
        world.main_token_last_transfer_nonce("player:alice"),
        Some(1)
    );

    world.submit_action(Action::TransferMainToken {
        from_account_id: "player:alice".to_string(),
        to_account_id: "player:bob".to_string(),
        amount: 10,
        nonce: 1,
        asset_id: None,
        memo: None,
        chain_id: None,
        network_id: None,
        tx_version: None,
        tx_type: None,
        valid_until_unix_ms: None,
        max_fee: None,
        fee_asset_id: None,
        application_payload_hash: None,
        client_request_id: None,
    });
    world.step().expect("replay transfer should be rejected");

    assert_eq!(world.main_token_liquid_balance("player:alice"), 70);
    assert_eq!(world.main_token_liquid_balance("player:bob"), 30);
    assert_eq!(
        world.main_token_last_transfer_nonce("player:alice"),
        Some(1)
    );

    match &world.journal().events.last().expect("event").body {
        WorldEventBody::Domain(DomainEvent::ActionRejected { reason, .. }) => match reason {
            RejectReason::RuleDenied { notes } => {
                assert!(notes.iter().any(|note| note.contains("nonce replay")));
            }
            other => panic!("expected RuleDenied, got {other:?}"),
        },
        other => panic!("expected ActionRejected, got {other:?}"),
    }
}

#[test]
fn main_token_epoch_issuance_applies_formula_clamp_split_and_rejects_duplicate_epoch() {
    let mut world = World::new();
    world.set_main_token_config(MainTokenConfig {
        initial_supply: 1_000,
        max_supply: Some(1_005),
        inflation_policy: MainTokenInflationPolicy {
            base_rate_bps: 300,
            min_rate_bps: 200,
            max_rate_bps: 800,
            target_stake_ratio_bps: 6_000,
            stake_feedback_gain_bps: 1_000,
            epochs_per_year: 10,
        },
        issuance_split: MainTokenIssuanceSplitPolicy::default(),
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
        total_supply: 1_000,
        circulating_supply: 1_000,
        total_issued: 0,
        total_burned: 0,
    });

    world.submit_action(Action::ApplyMainTokenEpochIssuance {
        epoch_index: 1,
        actual_stake_ratio_bps: 6_000,
    });
    world.step().expect("apply epoch issuance #1");

    assert_eq!(world.main_token_supply().total_issued, 3);
    assert_eq!(world.main_token_supply().total_supply, 1_003);
    assert_eq!(
        world.main_token_epoch_issuance_record(1),
        Some(&MainTokenEpochIssuanceRecord {
            epoch_index: 1,
            inflation_rate_bps: 300,
            issued_amount: 3,
            staking_reward_amount: 1,
            node_service_reward_amount: 0,
            ecosystem_pool_amount: 0,
            security_reserve_amount: 2,
        })
    );
    assert_eq!(
        world.main_token_treasury_balance(MAIN_TOKEN_TREASURY_BUCKET_STAKING_REWARD),
        1
    );
    assert_eq!(
        world.main_token_treasury_balance(MAIN_TOKEN_TREASURY_BUCKET_NODE_SERVICE_REWARD),
        0
    );
    assert_eq!(
        world.main_token_treasury_balance(MAIN_TOKEN_TREASURY_BUCKET_ECOSYSTEM_POOL),
        0
    );
    assert_eq!(
        world.main_token_treasury_balance(MAIN_TOKEN_TREASURY_BUCKET_SECURITY_RESERVE),
        2
    );

    world.submit_action(Action::ApplyMainTokenEpochIssuance {
        epoch_index: 2,
        actual_stake_ratio_bps: 0,
    });
    world.step().expect("apply epoch issuance #2");

    assert_eq!(world.main_token_supply().total_issued, 5);
    assert_eq!(world.main_token_supply().total_supply, 1_005);
    assert_eq!(
        world.main_token_epoch_issuance_record(2),
        Some(&MainTokenEpochIssuanceRecord {
            epoch_index: 2,
            inflation_rate_bps: 800,
            issued_amount: 2,
            staking_reward_amount: 1,
            node_service_reward_amount: 0,
            ecosystem_pool_amount: 0,
            security_reserve_amount: 1,
        })
    );
    assert_eq!(
        world.main_token_treasury_balance(MAIN_TOKEN_TREASURY_BUCKET_STAKING_REWARD),
        2
    );
    assert_eq!(
        world.main_token_treasury_balance(MAIN_TOKEN_TREASURY_BUCKET_SECURITY_RESERVE),
        3
    );

    world.submit_action(Action::ApplyMainTokenEpochIssuance {
        epoch_index: 2,
        actual_stake_ratio_bps: 6_000,
    });
    world
        .step()
        .expect("duplicate epoch issuance should be rejected");

    match &world.journal().events.last().expect("event").body {
        WorldEventBody::Domain(DomainEvent::ActionRejected { reason, .. }) => match reason {
            RejectReason::RuleDenied { notes } => {
                assert!(notes.iter().any(|note| note.contains("already exists")));
            }
            other => panic!("expected RuleDenied, got {other:?}"),
        },
        other => panic!("expected ActionRejected, got {other:?}"),
    }
}

#[test]
fn main_token_fee_settlement_burns_supply_and_tracks_treasury_buckets() {
    let mut world = World::new();
    world.set_main_token_config(MainTokenConfig {
        initial_supply: 1_000,
        burn_policy: MainTokenBurnPolicy {
            gas_base_fee_burn_bps: 3_000,
            slash_burn_bps: 5_000,
            module_fee_burn_bps: 2_000,
        },
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
        total_supply: 1_000,
        circulating_supply: 500,
        total_issued: 0,
        total_burned: 0,
    });

    world.submit_action(Action::SettleMainTokenFee {
        fee_kind: MainTokenFeeKind::GasBaseFee,
        amount: 100,
    });
    world.step().expect("settle gas fee");

    world.submit_action(Action::SettleMainTokenFee {
        fee_kind: MainTokenFeeKind::SlashPenalty,
        amount: 100,
    });
    world.step().expect("settle slash fee");

    world.submit_action(Action::SettleMainTokenFee {
        fee_kind: MainTokenFeeKind::ModuleFee,
        amount: 100,
    });
    world.step().expect("settle module fee");

    assert_eq!(world.main_token_supply().total_supply, 900);
    assert_eq!(world.main_token_supply().circulating_supply, 200);
    assert_eq!(world.main_token_supply().total_burned, 100);
    assert_eq!(
        world.main_token_treasury_balance(MAIN_TOKEN_TREASURY_BUCKET_GAS_FEE),
        70
    );
    assert_eq!(
        world.main_token_treasury_balance(MAIN_TOKEN_TREASURY_BUCKET_SLASH),
        50
    );
    assert_eq!(
        world.main_token_treasury_balance(MAIN_TOKEN_TREASURY_BUCKET_MODULE_FEE),
        80
    );

    world.submit_action(Action::SettleMainTokenFee {
        fee_kind: MainTokenFeeKind::GasBaseFee,
        amount: 250,
    });
    world
        .step()
        .expect("insufficient circulating should reject");

    match &world.journal().events.last().expect("event").body {
        WorldEventBody::Domain(DomainEvent::ActionRejected { reason, .. }) => match reason {
            RejectReason::RuleDenied { notes } => {
                assert!(notes.iter().any(|note| note.contains("circulating")));
            }
            other => panic!("expected RuleDenied, got {other:?}"),
        },
        other => panic!("expected ActionRejected, got {other:?}"),
    }
}
