use super::*;
use crate::{CausedBy, WorldError};

fn fixture() -> (World, DomainEvent) {
    let mut world = World::new();
    configure_main_token_bridge_budget(&mut world, 40, 5, 5);
    bind_node_identity(&mut world, "node-a");
    let signer_private_key = bind_node_identity_with_seed(&mut world, "node-signer", 40);
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
    let report = settlement_report(40, vec![settlement("node-a", 50)]);
    let mut preview = world.clone();
    let minted_records = preview
        .apply_node_points_settlement_mint_v2(&report, "node-signer", signer_private_key.as_str())
        .expect("build signed mint record");
    let mut capture = world.clone();
    capture.submit_action(Action::ApplyNodePointsSettlementSigned {
        report,
        signer_node_id: "node-signer".into(),
        mint_records: minted_records,
    });
    capture.step().expect("capture valid settlement event");
    let event = capture
        .journal()
        .events
        .iter()
        .rev()
        .find_map(|entry| match &entry.body {
            WorldEventBody::Domain(event @ DomainEvent::NodePointsSettlementApplied { .. }) => {
                Some(event.clone())
            }
            _ => None,
        })
        .expect("captured settlement event");
    (world, event)
}

fn assert_unchanged(world: &World, before: &World, root: &str) {
    assert_eq!(world.snapshot(), before.snapshot());
    assert_eq!(world.journal(), before.journal());
    assert_eq!(
        world.runtime_backpressure_stats(),
        before.runtime_backpressure_stats()
    );
    assert_eq!(
        world.tick_consensus_records(),
        before.tick_consensus_records()
    );
    assert_eq!(world.current_state_root_hash().unwrap(), root);
}

fn assert_success(world: &World) {
    assert_eq!(world.node_power_credit_balance("node-a"), 5);
    assert_eq!(world.reward_mint_records().len(), 1);
    assert_eq!(
        world.main_token_treasury_balance(MAIN_TOKEN_TREASURY_BUCKET_NODE_SERVICE_REWARD),
        0
    );
    let account = world
        .node_main_token_account("node-a")
        .expect("node account");
    assert_eq!(world.main_token_liquid_balance(account), 5);
    assert_eq!(world.main_token_supply().circulating_supply, 5);
    assert_eq!(
        world
            .main_token_node_points_bridge_record(40)
            .expect("bridge record")
            .total_amount,
        5
    );
}

#[test]
fn raw_settlement_failpoint_is_atomic_then_same_world_retry_commits_once() {
    let (mut world, event) = fixture();
    let baseline = world.snapshot();
    let before = world.clone();
    let root = world.current_state_root_hash().unwrap();
    let journal_len = world.journal().events.len();
    world.fail_next_append_after_publication_prepare_for_test();
    let error = world
        .append_event_for_test(
            WorldEventBody::Domain(event.clone()),
            Some(CausedBy::Action(4000)),
        )
        .expect_err("settlement must honor postprepare failure");
    assert!(
        matches!(error, WorldError::ResourceBalanceInvalid { ref reason } if reason.contains("publication preparation")),
        "{error:?}"
    );
    assert_unchanged(&world, &before, &root);

    let cause = Some(CausedBy::Action(4001));
    let event_id = world
        .append_event_for_test(WorldEventBody::Domain(event), cause.clone())
        .expect("same-world retry");
    assert_eq!(world.journal().events.len(), journal_len + 1);
    assert_eq!(world.journal().events.last().unwrap().id, event_id);
    assert_eq!(world.journal().events.last().unwrap().caused_by, cause);
    assert_success(&world);
    let replay = World::from_snapshot(baseline, world.journal().clone()).expect("replay retry");
    assert_eq!(replay.snapshot(), world.snapshot());
    assert_eq!(
        replay.current_state_root_hash().unwrap(),
        world.current_state_root_hash().unwrap()
    );
}

#[test]
fn late_bridge_validation_failure_does_not_leak_mints_budget_or_token_state() {
    let (mut world, mut event) = fixture();
    if let DomainEvent::NodePointsSettlementApplied {
        main_token_bridge_distributions,
        ..
    } = &mut event
    {
        main_token_bridge_distributions[0].amount = 4;
    } else {
        unreachable!();
    }
    let before = world.clone();
    let root = world.current_state_root_hash().unwrap();
    let error = world
        .append_event_for_test(WorldEventBody::Domain(event), None)
        .expect_err("bridge sum mismatch");
    assert!(
        matches!(error, WorldError::ResourceBalanceInvalid { ref reason } if reason.contains("main token bridge sum mismatch")),
        "{error:?}"
    );
    assert_unchanged(&world, &before, &root);
}

fn assert_error(mut world: World, event: DomainEvent, expected: &str) {
    let before = world.clone();
    let root = world.current_state_root_hash().unwrap();
    let error = world
        .append_event_for_test(WorldEventBody::Domain(event), None)
        .expect_err("invalid settlement");
    assert!(format!("{error:?}").contains(expected), "{error:?}");
    assert_unchanged(&world, &before, &root);
}

#[test]
fn settlement_validation_priority_and_zero_mailbox_routing_are_stable() {
    let (world, event) = fixture();
    let mut blank_signer = event.clone();
    if let DomainEvent::NodePointsSettlementApplied {
        signer_node_id,
        settlement_hash,
        ..
    } = &mut blank_signer
    {
        signer_node_id.clear();
        settlement_hash.clear();
    }
    assert_error(
        world.clone(),
        blank_signer,
        "signer_node_id cannot be empty",
    );

    let mut bad_hash = event.clone();
    if let DomainEvent::NodePointsSettlementApplied {
        settlement_hash, ..
    } = &mut bad_hash
    {
        *settlement_hash = "wrong".into();
    }
    let mut zero_config = world.clone();
    zero_config.set_reward_asset_config(RewardAssetConfig {
        points_per_credit: 0,
        ..RewardAssetConfig::default()
    });
    assert_error(zero_config, bad_hash, "settlement_hash mismatch");

    let before_mailboxes = world
        .state()
        .agents
        .iter()
        .map(|(id, cell)| (id.clone(), cell.mailbox.clone()))
        .collect::<Vec<_>>();
    let baseline = world.snapshot();
    let mut success = world;
    success
        .append_event_for_test(WorldEventBody::Domain(event), Some(CausedBy::Action(4002)))
        .expect("valid settlement");
    assert_success(&success);
    assert_eq!(
        success
            .state()
            .agents
            .iter()
            .map(|(id, cell)| (id.clone(), cell.mailbox.clone()))
            .collect::<Vec<_>>(),
        before_mailboxes
    );
    let replay = World::from_snapshot(baseline, success.journal().clone()).expect("replay success");
    assert_eq!(replay.snapshot(), success.snapshot());
}

fn multi_fixture() -> (World, DomainEvent) {
    let mut world = World::new();
    configure_main_token_bridge_budget(&mut world, 41, 8, 8);
    bind_node_identity(&mut world, "node-a");
    bind_node_identity(&mut world, "node-b");
    let key = bind_node_identity_with_seed(&mut world, "node-signer", 41);
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
    let report = settlement_report(41, vec![settlement("node-a", 50), settlement("node-b", 30)]);
    let mut preview = world.clone();
    let records = preview
        .apply_node_points_settlement_mint_v2(&report, "node-signer", &key)
        .expect("build multi-node records");
    let mut capture = world.clone();
    capture.submit_action(Action::ApplyNodePointsSettlementSigned {
        report,
        signer_node_id: "node-signer".into(),
        mint_records: records,
    });
    capture.step().expect("capture multi-node settlement");
    let event = capture
        .journal()
        .events
        .iter()
        .rev()
        .find_map(|entry| match &entry.body {
            WorldEventBody::Domain(event @ DomainEvent::NodePointsSettlementApplied { .. }) => {
                Some(event.clone())
            }
            _ => None,
        })
        .unwrap();
    (world, event)
}

#[test]
fn multi_distribution_shared_account_and_token_overflow_contract_is_stable() {
    let (world, event) = multi_fixture();
    let mut shared = event.clone();
    let shared_account = if let DomainEvent::NodePointsSettlementApplied {
        main_token_bridge_distributions,
        ..
    } = &mut shared
    {
        let account = main_token_bridge_distributions[0].account_id.clone();
        main_token_bridge_distributions[1].account_id = account.clone();
        account
    } else {
        unreachable!()
    };
    let mut success = world.clone();
    success
        .append_event_for_test(WorldEventBody::Domain(shared.clone()), None)
        .expect("shared account distributions accumulate");
    assert_eq!(success.node_power_credit_balance("node-a"), 5);
    assert_eq!(success.node_power_credit_balance("node-b"), 3);
    assert_eq!(success.reward_mint_records().len(), 2);
    assert_eq!(success.main_token_liquid_balance(&shared_account), 8);

    let mut account_overflow = world.clone();
    account_overflow
        .set_main_token_account_balance(&shared_account, u64::MAX, 0)
        .unwrap();
    assert_error(account_overflow, shared.clone(), "bridge account overflow");

    let mut circulating_overflow = world.clone();
    circulating_overflow.set_main_token_supply(MainTokenSupplyState {
        total_supply: u64::MAX,
        circulating_supply: u64::MAX,
        total_issued: 0,
        total_burned: 0,
    });
    assert_error(
        circulating_overflow,
        event.clone(),
        "bridge circulating overflow",
    );
    let mut exceeds_total = world;
    exceeds_total.set_main_token_supply(MainTokenSupplyState {
        total_supply: 7,
        circulating_supply: 0,
        total_issued: 0,
        total_burned: 0,
    });
    assert_error(exceeds_total, event, "circulating exceeds total");
}

#[test]
fn zero_bridge_normalizes_all_legacy_material_forms_and_preserves_other_ledgers() {
    let (base, mut event) = fixture();
    if let DomainEvent::NodePointsSettlementApplied {
        main_token_bridge_total_amount,
        main_token_bridge_distributions,
        ..
    } = &mut event
    {
        *main_token_bridge_total_amount = 0;
        main_token_bridge_distributions.clear();
    }
    let world_key = MaterialLedgerId::world();
    let unrelated = MaterialLedgerId::site("unrelated-40c");
    for mode in 0..3 {
        let mut state = base.state().clone();
        state.materials.insert("legacy".into(), 7);
        state
            .material_ledgers
            .insert(unrelated.clone(), [("ore".into(), 9)].into());
        match mode {
            0 => {
                state.material_ledgers.remove(&world_key);
            }
            1 => {
                state
                    .material_ledgers
                    .insert(world_key.clone(), Default::default());
            }
            2 => {
                state
                    .material_ledgers
                    .insert(world_key.clone(), [("canonical".into(), 11)].into());
            }
            _ => unreachable!(),
        }
        let mut world = World::new_with_state(state);
        let baseline = world.snapshot();
        let before = world.clone();
        let root = world.current_state_root_hash().unwrap();
        world.fail_next_append_after_publication_prepare_for_test();
        world
            .append_event_for_test(WorldEventBody::Domain(event.clone()), None)
            .expect_err("zero bridge honors failpoint");
        assert_unchanged(&world, &before, &root);
        world
            .append_event_for_test(WorldEventBody::Domain(event.clone()), None)
            .expect("zero bridge retry");
        assert_eq!(world.state().material_ledgers[&unrelated]["ore"], 9);
        assert_eq!(
            world.state().materials,
            world.state().material_ledgers[&world_key]
        );
        assert_eq!(world.main_token_supply().circulating_supply, 0);
        assert_eq!(
            world
                .main_token_node_points_bridge_record(40)
                .unwrap()
                .total_amount,
            0
        );
        if mode == 2 {
            assert_eq!(world.state().materials["canonical"], 11);
        } else {
            assert_eq!(world.state().materials["legacy"], 7);
        }
        let replay = World::from_snapshot(baseline, world.journal().clone()).unwrap();
        assert_eq!(replay.snapshot(), world.snapshot());
        assert_eq!(
            replay.current_state_root_hash().unwrap(),
            world.current_state_root_hash().unwrap()
        );
    }
}

#[test]
fn remaining_raw_priorities_and_action_preview_match_publication() {
    let (world, event) = fixture();
    let mut zero_config = world.clone();
    zero_config.set_reward_asset_config(RewardAssetConfig {
        points_per_credit: 0,
        ..RewardAssetConfig::default()
    });
    assert_error(
        zero_config,
        event.clone(),
        "points_per_credit must be positive",
    );

    let mut missing_signer_state = world.state().clone();
    missing_signer_state
        .node_identity_bindings
        .remove("node-signer");
    assert_error(
        World::new_with_state(missing_signer_state),
        event.clone(),
        "node identity is not bound: node-signer",
    );
    let mut over_budget = event.clone();
    if let DomainEvent::NodePointsSettlementApplied {
        main_token_bridge_total_amount,
        ..
    } = &mut over_budget
    {
        *main_token_bridge_total_amount = 6;
    }
    assert_error(
        world.clone(),
        over_budget,
        "total exceeds epoch node_service budget",
    );

    let (report, signer_node_id, minted_records) = match &event {
        DomainEvent::NodePointsSettlementApplied {
            report,
            signer_node_id,
            minted_records,
            ..
        } => (
            report.clone(),
            signer_node_id.clone(),
            minted_records.clone(),
        ),
        _ => unreachable!(),
    };
    let mut raw = world.clone();
    raw.step().expect("match action execution time");
    raw.append_event_for_test(WorldEventBody::Domain(event), None)
        .unwrap();
    let mut action = world;
    action.submit_action(Action::ApplyNodePointsSettlementSigned {
        report,
        signer_node_id,
        mint_records: minted_records,
    });
    action.step().unwrap();
    assert_eq!(action.state(), raw.state());
    assert_eq!(
        action.current_state_root_hash().unwrap(),
        raw.current_state_root_hash().unwrap()
    );
}
