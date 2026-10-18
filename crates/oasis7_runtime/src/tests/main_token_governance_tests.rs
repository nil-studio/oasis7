#[test]
fn main_token_policy_update_is_delayed_and_audited_before_it_affects_issuance() {
    let mut world = World::new();
    world.set_main_token_config(MainTokenConfig {
        initial_supply: 1_000,
        inflation_policy: MainTokenInflationPolicy {
            base_rate_bps: 300,
            min_rate_bps: 300,
            max_rate_bps: 300,
            target_stake_ratio_bps: 6_000,
            stake_feedback_gain_bps: 0,
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

    let schedule_base_epoch = world.state().time;
    let mut scheduled_config = world.main_token_config().clone();
    scheduled_config.initial_supply = 1_000;
    scheduled_config.inflation_policy = MainTokenInflationPolicy {
        base_rate_bps: 800,
        min_rate_bps: 800,
        max_rate_bps: 800,
        target_stake_ratio_bps: 6_000,
        stake_feedback_gain_bps: 0,
        epochs_per_year: 10,
    };
    let proposal_id = world
        .propose_manifest_update(world.manifest().clone(), "alice")
        .expect("create governance proposal");
    world
        .shadow_proposal(proposal_id)
        .expect("shadow governance proposal");
    world
        .approve_proposal(proposal_id, "bob", ProposalDecision::Approve)
        .expect("approve governance proposal");
    world.submit_action(Action::UpdateMainTokenPolicy {
        proposal_id,
        next: scheduled_config,
    });
    world.step().expect("schedule main token policy update");

    let effective_epoch = match &world.journal().events.last().expect("event").body {
        WorldEventBody::Domain(DomainEvent::MainTokenPolicyUpdateScheduled {
            proposal_id: event_proposal_id,
            effective_epoch: event_effective_epoch,
            ..
        }) => {
            assert_eq!(*event_proposal_id, proposal_id);
            assert!(*event_effective_epoch > schedule_base_epoch);
            *event_effective_epoch
        }
        other => panic!("expected MainTokenPolicyUpdateScheduled, got {other:?}"),
    };
    assert!(world
        .main_token_scheduled_policy_update(effective_epoch)
        .is_some());

    world.submit_action(Action::ApplyMainTokenEpochIssuance {
        epoch_index: effective_epoch - 1,
        actual_stake_ratio_bps: 6_000,
    });
    world.step().expect("issue on old policy epoch");
    assert_eq!(
        world
            .main_token_epoch_issuance_record(effective_epoch - 1)
            .expect("old policy issuance")
            .inflation_rate_bps,
        300
    );

    world.submit_action(Action::ApplyMainTokenEpochIssuance {
        epoch_index: effective_epoch,
        actual_stake_ratio_bps: 6_000,
    });
    world.step().expect("issue on effective policy epoch");
    assert_eq!(
        world
            .main_token_epoch_issuance_record(effective_epoch)
            .expect("new policy issuance")
            .inflation_rate_bps,
        800
    );
}

#[test]
fn main_token_policy_update_requires_approved_or_applied_governance_proposal() {
    let mut world = World::new();
    world.set_main_token_config(MainTokenConfig {
        initial_supply: 1_000,
        ..MainTokenConfig::default()
    });

    let mut next_config = world.main_token_config().clone();
    next_config.inflation_policy.base_rate_bps = 500;

    world.submit_action(Action::UpdateMainTokenPolicy {
        proposal_id: 9_999,
        next: next_config.clone(),
    });
    world
        .step()
        .expect("missing governance proposal should reject");
    match &world.journal().events.last().expect("event").body {
        WorldEventBody::Domain(DomainEvent::ActionRejected { reason, .. }) => match reason {
            RejectReason::RuleDenied { notes } => {
                assert!(notes
                    .iter()
                    .any(|note| note.contains("governance proposal not found")));
            }
            other => panic!("expected RuleDenied, got {other:?}"),
        },
        other => panic!("expected ActionRejected, got {other:?}"),
    }

    let proposal_id = world
        .propose_manifest_update(world.manifest().clone(), "alice")
        .expect("create governance proposal");
    world.submit_action(Action::UpdateMainTokenPolicy {
        proposal_id,
        next: next_config.clone(),
    });
    world
        .step()
        .expect("unapproved governance proposal should reject");
    match &world.journal().events.last().expect("event").body {
        WorldEventBody::Domain(DomainEvent::ActionRejected { reason, .. }) => match reason {
            RejectReason::RuleDenied { notes } => {
                assert!(notes.iter().any(|note| {
                    note.contains("governance proposal must be approved or applied")
                }));
            }
            other => panic!("expected RuleDenied, got {other:?}"),
        },
        other => panic!("expected ActionRejected, got {other:?}"),
    }

    world
        .shadow_proposal(proposal_id)
        .expect("shadow governance proposal");
    world
        .approve_proposal(proposal_id, "bob", ProposalDecision::Approve)
        .expect("approve governance proposal");
    world.submit_action(Action::UpdateMainTokenPolicy {
        proposal_id,
        next: next_config,
    });
    world
        .step()
        .expect("approved governance proposal should schedule policy update");
    match &world.journal().events.last().expect("event").body {
        WorldEventBody::Domain(DomainEvent::MainTokenPolicyUpdateScheduled {
            proposal_id: event_proposal_id,
            ..
        }) => {
            assert_eq!(*event_proposal_id, proposal_id);
        }
        other => panic!("expected MainTokenPolicyUpdateScheduled, got {other:?}"),
    }
}

#[test]
fn main_token_policy_update_rejects_out_of_bounds_configuration() {
    let mut world = World::new();
    world.set_main_token_config(MainTokenConfig {
        initial_supply: 1_000,
        ..MainTokenConfig::default()
    });

    let mut invalid_config = world.main_token_config().clone();
    invalid_config.initial_supply = 1_000;
    invalid_config.issuance_split = MainTokenIssuanceSplitPolicy {
        staking_reward_bps: 6_000,
        node_service_reward_bps: 2_000,
        ecosystem_pool_bps: 1_500,
        security_reserve_bps: 100,
    };
    let proposal_id = world
        .propose_manifest_update(world.manifest().clone(), "alice")
        .expect("create governance proposal");
    world
        .shadow_proposal(proposal_id)
        .expect("shadow governance proposal");
    world
        .approve_proposal(proposal_id, "bob", ProposalDecision::Approve)
        .expect("approve governance proposal");
    world.submit_action(Action::UpdateMainTokenPolicy {
        proposal_id,
        next: invalid_config,
    });
    world
        .step()
        .expect("invalid main token policy update should be rejected");

    match &world.journal().events.last().expect("event").body {
        WorldEventBody::Domain(DomainEvent::ActionRejected { reason, .. }) => match reason {
            RejectReason::RuleDenied { notes } => {
                assert!(notes
                    .iter()
                    .any(|note| note.contains("update main token policy rejected")));
                assert!(notes
                    .iter()
                    .any(|note| note.contains("split sum must be 10000")));
            }
            other => panic!("expected RuleDenied, got {other:?}"),
        },
        other => panic!("expected ActionRejected, got {other:?}"),
    }
    assert!(world.state().main_token_scheduled_policy_updates.is_empty());
}

#[test]
fn main_token_treasury_distribution_applies_closed_loop_and_records_audit() {
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
        circulating_supply: 1_000,
        total_issued: 500,
        total_burned: 0,
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
        distribution_id: "dist-1".to_string(),
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

    match &world.journal().events.last().expect("event").body {
        WorldEventBody::Domain(DomainEvent::MainTokenTreasuryDistributed {
            proposal_id: event_proposal_id,
            distribution_id,
            bucket_id,
            total_amount,
            distributions,
        }) => {
            assert_eq!(*event_proposal_id, proposal_id);
            assert_eq!(distribution_id, "dist-1");
            assert_eq!(bucket_id, MAIN_TOKEN_TREASURY_BUCKET_STAKING_REWARD);
            assert_eq!(*total_amount, 200);
            assert_eq!(distributions.len(), 2);
        }
        other => panic!("expected MainTokenTreasuryDistributed, got {other:?}"),
    }
    assert_eq!(
        world.main_token_treasury_balance(MAIN_TOKEN_TREASURY_BUCKET_STAKING_REWARD),
        100
    );
    assert_eq!(world.main_token_liquid_balance("node:alice"), 120);
    assert_eq!(world.main_token_liquid_balance("node:bob"), 80);
    assert_eq!(world.main_token_supply().circulating_supply, 1_200);

    let record = world
        .main_token_treasury_distribution_record("dist-1")
        .expect("distribution record");
    assert_eq!(record.proposal_id, proposal_id);
    assert_eq!(record.bucket_id, MAIN_TOKEN_TREASURY_BUCKET_STAKING_REWARD);
    assert_eq!(record.total_amount, 200);
    assert_eq!(record.distributions.len(), 2);
}

#[test]
fn main_token_treasury_distribution_requires_approved_or_applied_governance_proposal() {
    let mut world = World::new();
    world.set_main_token_config(MainTokenConfig {
        initial_supply: 1_000,
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
    world
        .set_main_token_treasury_balance(MAIN_TOKEN_TREASURY_BUCKET_STAKING_REWARD, 100)
        .expect("set staking treasury balance");

    world.submit_action(Action::DistributeMainTokenTreasury {
        proposal_id: 9_999,
        distribution_id: "dist-missing".to_string(),
        bucket_id: MAIN_TOKEN_TREASURY_BUCKET_STAKING_REWARD.to_string(),
        distributions: vec![MainTokenTreasuryDistribution {
            account_id: "node:alice".to_string(),
            amount: 50,
        }],
    });
    world
        .step()
        .expect("missing governance proposal should reject");
    match &world.journal().events.last().expect("event").body {
        WorldEventBody::Domain(DomainEvent::ActionRejected { reason, .. }) => match reason {
            RejectReason::RuleDenied { notes } => {
                assert!(notes
                    .iter()
                    .any(|note| note.contains("governance proposal not found")));
            }
            other => panic!("expected RuleDenied, got {other:?}"),
        },
        other => panic!("expected ActionRejected, got {other:?}"),
    }

    let proposal_id = world
        .propose_manifest_update(world.manifest().clone(), "alice")
        .expect("create governance proposal");
    world.submit_action(Action::DistributeMainTokenTreasury {
        proposal_id,
        distribution_id: "dist-unapproved".to_string(),
        bucket_id: MAIN_TOKEN_TREASURY_BUCKET_STAKING_REWARD.to_string(),
        distributions: vec![MainTokenTreasuryDistribution {
            account_id: "node:alice".to_string(),
            amount: 50,
        }],
    });
    world
        .step()
        .expect("unapproved governance proposal should reject");
    match &world.journal().events.last().expect("event").body {
        WorldEventBody::Domain(DomainEvent::ActionRejected { reason, .. }) => match reason {
            RejectReason::RuleDenied { notes } => {
                assert!(notes.iter().any(|note| {
                    note.contains("governance proposal must be approved or applied")
                }));
            }
            other => panic!("expected RuleDenied, got {other:?}"),
        },
        other => panic!("expected ActionRejected, got {other:?}"),
    }
}

#[test]
fn main_token_treasury_distribution_rejects_unsupported_bucket_and_duplicate_distribution_id() {
    let mut world = World::new();
    world.set_main_token_config(MainTokenConfig {
        initial_supply: 1_000,
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
        circulating_supply: 600,
        total_issued: 0,
        total_burned: 0,
    });
    world
        .set_main_token_treasury_balance(MAIN_TOKEN_TREASURY_BUCKET_STAKING_REWARD, 100)
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
        distribution_id: "dist-invalid-bucket".to_string(),
        bucket_id: MAIN_TOKEN_TREASURY_BUCKET_NODE_SERVICE_REWARD.to_string(),
        distributions: vec![MainTokenTreasuryDistribution {
            account_id: "node:alice".to_string(),
            amount: 30,
        }],
    });
    world.step().expect("unsupported bucket should reject");
    match &world.journal().events.last().expect("event").body {
        WorldEventBody::Domain(DomainEvent::ActionRejected { reason, .. }) => match reason {
            RejectReason::RuleDenied { notes } => {
                assert!(notes.iter().any(|note| note.contains("unsupported bucket")));
            }
            other => panic!("expected RuleDenied, got {other:?}"),
        },
        other => panic!("expected ActionRejected, got {other:?}"),
    }

    world.submit_action(Action::DistributeMainTokenTreasury {
        proposal_id,
        distribution_id: "dist-dup".to_string(),
        bucket_id: MAIN_TOKEN_TREASURY_BUCKET_STAKING_REWARD.to_string(),
        distributions: vec![MainTokenTreasuryDistribution {
            account_id: "node:alice".to_string(),
            amount: 40,
        }],
    });
    world
        .step()
        .expect("first treasury distribution should pass");
    assert_eq!(
        world.main_token_treasury_balance(MAIN_TOKEN_TREASURY_BUCKET_STAKING_REWARD),
        60
    );

    world.submit_action(Action::DistributeMainTokenTreasury {
        proposal_id,
        distribution_id: "dist-dup".to_string(),
        bucket_id: MAIN_TOKEN_TREASURY_BUCKET_STAKING_REWARD.to_string(),
        distributions: vec![MainTokenTreasuryDistribution {
            account_id: "node:bob".to_string(),
            amount: 20,
        }],
    });
    world
        .step()
        .expect("duplicate distribution_id should reject");
    match &world.journal().events.last().expect("event").body {
        WorldEventBody::Domain(DomainEvent::ActionRejected { reason, .. }) => match reason {
            RejectReason::RuleDenied { notes } => {
                assert!(notes
                    .iter()
                    .any(|note| note.contains("distribution_id already exists")));
            }
            other => panic!("expected RuleDenied, got {other:?}"),
        },
        other => panic!("expected ActionRejected, got {other:?}"),
    }
}

#[test]
fn restricted_claim_liveops_pool_top_up_moves_balance_and_records_event() {
    let mut world = World::new();
    set_main_token_controller_registry_for_tests(&mut world, "msig.ecosystem_governance.v1");
    world.set_main_token_supply(MainTokenSupplyState {
        total_supply: 1_000,
        circulating_supply: 0,
        total_issued: 1_000,
        total_burned: 0,
    });
    world
        .set_main_token_treasury_balance(MAIN_TOKEN_TREASURY_BUCKET_ECOSYSTEM_POOL, 900)
        .expect("seed ecosystem treasury");

    world.submit_action(Action::TopUpRestrictedStarterClaimLiveopsPool {
        controller_account_id: "msig.ecosystem_governance.v1".to_string(),
        top_up_id: "liveops-topup-1".to_string(),
        amount: 300,
    });
    world.step().expect("top up restricted claim liveops pool");

    match &world.journal().events.last().expect("event").body {
        WorldEventBody::Domain(DomainEvent::RestrictedStarterClaimLiveopsPoolToppedUp {
            controller_account_id,
            top_up_id,
            source_treasury_bucket_id,
            target_treasury_bucket_id,
            amount,
            ..
        }) => {
            assert_eq!(controller_account_id, "msig.ecosystem_governance.v1");
            assert_eq!(top_up_id, "liveops-topup-1");
            assert_eq!(
                source_treasury_bucket_id,
                MAIN_TOKEN_TREASURY_BUCKET_ECOSYSTEM_POOL
            );
            assert_eq!(
                target_treasury_bucket_id,
                MAIN_TOKEN_TREASURY_BUCKET_RESTRICTED_STARTER_CLAIM_LIVEOPS_POOL
            );
            assert_eq!(*amount, 300);
        }
        other => panic!(
            "expected RestrictedStarterClaimLiveopsPoolToppedUp, got {other:?}"
        ),
    }
    assert_eq!(
        world.main_token_treasury_balance(MAIN_TOKEN_TREASURY_BUCKET_ECOSYSTEM_POOL),
        600
    );
    assert_eq!(
        world.main_token_treasury_balance(
            MAIN_TOKEN_TREASURY_BUCKET_RESTRICTED_STARTER_CLAIM_LIVEOPS_POOL
        ),
        300
    );
    let record = world
        .restricted_starter_claim_liveops_pool_top_up_record("liveops-topup-1")
        .expect("top-up record");
    assert_eq!(record.controller_account_id, "msig.ecosystem_governance.v1");
    assert_eq!(record.amount, 300);
}

#[test]
fn restricted_claim_liveops_pool_top_up_rejects_wrong_controller_slot_and_duplicate_top_up_id() {
    let mut world = World::new();
    set_main_token_controller_registry_for_tests(&mut world, "msig.ecosystem_governance.v1");
    world.set_main_token_supply(MainTokenSupplyState {
        total_supply: 1_000,
        circulating_supply: 0,
        total_issued: 1_000,
        total_burned: 0,
    });
    world
        .set_main_token_treasury_balance(MAIN_TOKEN_TREASURY_BUCKET_ECOSYSTEM_POOL, 900)
        .expect("seed ecosystem treasury");

    world.submit_action(Action::TopUpRestrictedStarterClaimLiveopsPool {
        controller_account_id: "msig.wrong_controller.v1".to_string(),
        top_up_id: "liveops-topup-reject".to_string(),
        amount: 300,
    });
    world.step().expect("reject wrong controller slot");
    match &world.journal().events.last().expect("event").body {
        WorldEventBody::Domain(DomainEvent::ActionRejected { reason, .. }) => match reason {
            RejectReason::RuleDenied { notes } => {
                assert!(notes
                    .iter()
                    .any(|note| note.contains("ecosystem treasury controller slot")));
            }
            other => panic!("expected RuleDenied, got {other:?}"),
        },
        other => panic!("expected ActionRejected, got {other:?}"),
    }

    world.submit_action(Action::TopUpRestrictedStarterClaimLiveopsPool {
        controller_account_id: "msig.ecosystem_governance.v1".to_string(),
        top_up_id: "liveops-topup-dup".to_string(),
        amount: 300,
    });
    world.step().expect("first top-up should pass");

    world.submit_action(Action::TopUpRestrictedStarterClaimLiveopsPool {
        controller_account_id: "msig.ecosystem_governance.v1".to_string(),
        top_up_id: "liveops-topup-dup".to_string(),
        amount: 50,
    });
    world.step().expect("duplicate top_up_id should reject");
    match &world.journal().events.last().expect("event").body {
        WorldEventBody::Domain(DomainEvent::ActionRejected { reason, .. }) => match reason {
            RejectReason::RuleDenied { notes } => {
                assert!(notes
                    .iter()
                    .any(|note| note.contains("top_up_id already exists")));
            }
            other => panic!("expected RuleDenied, got {other:?}"),
        },
        other => panic!("expected ActionRejected, got {other:?}"),
    }
}
