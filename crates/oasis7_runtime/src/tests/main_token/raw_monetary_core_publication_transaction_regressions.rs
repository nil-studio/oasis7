use super::*;
use oasis7_agent_api::world_action::GeoPos;

fn captured(
    mut world: World,
    action: Action,
    matches: fn(&DomainEvent) -> bool,
) -> (World, DomainEvent) {
    let base = world.clone();
    world.submit_action(action);
    world.step().expect("capture monetary event");
    let event = world
        .journal()
        .events
        .iter()
        .rev()
        .find_map(|entry| match &entry.body {
            WorldEventBody::Domain(event) if matches(event) => Some(event.clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("captured requested event; journal={:?}", world.journal()));
    (base, event)
}

fn genesis_plan() -> Vec<MainTokenGenesisAllocationPlan> {
    vec![MainTokenGenesisAllocationPlan {
        bucket_id: "vesting".into(),
        ratio_bps: 10_000,
        recipient: "alice".into(),
        cliff_epochs: 0,
        linear_unlock_epochs: 0,
        start_epoch: 0,
    }]
}

fn genesis_fixture() -> (World, DomainEvent) {
    let mut world = World::new();
    world.set_main_token_config(MainTokenConfig {
        initial_supply: 1_000,
        ..MainTokenConfig::default()
    });
    captured(
        world,
        Action::InitializeMainTokenGenesis {
            allocations: genesis_plan(),
        },
        |event| matches!(event, DomainEvent::MainTokenGenesisInitialized { .. }),
    )
}

fn initialized_world() -> World {
    let (mut world, event) = genesis_fixture();
    world
        .append_event_for_test(WorldEventBody::Domain(event), None)
        .unwrap();
    world
}

fn fixtures() -> Vec<(World, DomainEvent)> {
    let genesis = genesis_fixture();
    let vesting = captured(
        initialized_world(),
        Action::ClaimMainTokenVesting {
            bucket_id: "vesting".into(),
            beneficiary: "alice".into(),
            nonce: 1,
        },
        |event| matches!(event, DomainEvent::MainTokenVestingClaimed { .. }),
    );
    let mut transfer_world = World::new();
    transfer_world.set_main_token_supply(MainTokenSupplyState {
        total_supply: 100,
        circulating_supply: 100,
        total_issued: 0,
        total_burned: 0,
    });
    transfer_world
        .set_main_token_account_balance("alice", 80, 0)
        .unwrap();
    transfer_world
        .set_main_token_account_balance("bob", 20, 0)
        .unwrap();
    let transfer = captured(
        transfer_world,
        Action::TransferMainToken {
            from_account_id: "alice".into(),
            to_account_id: "bob".into(),
            amount: 25,
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
        },
        |event| matches!(event, DomainEvent::MainTokenTransferred { .. }),
    );
    let mut issuance_world = initialized_world();
    issuance_world.set_main_token_config(MainTokenConfig {
        initial_supply: 1_000_000,
        ..MainTokenConfig::default()
    });
    issuance_world.set_main_token_supply(MainTokenSupplyState {
        total_supply: 1_000_000,
        circulating_supply: 1_000_000,
        total_issued: 0,
        total_burned: 0,
    });
    let issuance = captured(
        issuance_world,
        Action::ApplyMainTokenEpochIssuance {
            epoch_index: 1,
            actual_stake_ratio_bps: 6_000,
        },
        |event| matches!(event, DomainEvent::MainTokenEpochIssued { .. }),
    );
    let mut fee_world = initialized_world();
    fee_world.set_main_token_supply(MainTokenSupplyState {
        total_supply: 100,
        circulating_supply: 100,
        total_issued: 0,
        total_burned: 0,
    });
    let fee = captured(
        fee_world,
        Action::SettleMainTokenFee {
            fee_kind: MainTokenFeeKind::GasBaseFee,
            amount: 10,
        },
        |event| matches!(event, DomainEvent::MainTokenFeeSettled { .. }),
    );
    vec![genesis, vesting, transfer, issuance, fee]
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

#[test]
fn all_raw_monetary_events_are_atomic_retryable_and_replayable() {
    for (index, (mut world, event)) in fixtures().into_iter().enumerate() {
        let baseline = world.snapshot();
        let before = world.clone();
        let root = world.current_state_root_hash().unwrap();
        let journal_len = world.journal().events.len();
        world.fail_next_append_after_publication_prepare_for_test();
        let error = world
            .append_event_for_test(
                WorldEventBody::Domain(event.clone()),
                Some(CausedBy::Action(4_100 + index as u64)),
            )
            .expect_err("raw monetary event honors postprepare fault");
        assert!(format!("{error:?}").contains("publication preparation"));
        assert_unchanged(&world, &before, &root);
        let cause = Some(CausedBy::Action(4_200 + index as u64));
        world
            .append_event_for_test(WorldEventBody::Domain(event), cause.clone())
            .expect("same-world retry");
        assert_eq!(world.journal().events.len(), journal_len + 1);
        assert_eq!(world.journal().events.last().unwrap().caused_by, cause);
        let replay = World::from_snapshot(baseline, world.journal().clone()).unwrap();
        assert_eq!(replay.snapshot(), world.snapshot());
        assert_eq!(
            replay.current_state_root_hash().unwrap(),
            world.current_state_root_hash().unwrap()
        );
    }
}

fn assert_error(mut world: World, event: DomainEvent, expected: &str) {
    let before = world.clone();
    let root = world.current_state_root_hash().unwrap();
    let error = world
        .append_event_for_test(WorldEventBody::Domain(event), None)
        .expect_err("invalid monetary event");
    assert!(format!("{error:?}").contains(expected), "{error:?}");
    assert_unchanged(&world, &before, &root);
}

fn add_agent(world: &mut World, agent_id: &str) {
    world
        .append_event_for_test(
            WorldEventBody::Domain(DomainEvent::AgentRegistered {
                agent_id: agent_id.into(),
                pos: GeoPos::new(0, 0, 0),
            }),
            None,
        )
        .unwrap();
}

#[test]
fn monetary_validation_priority_and_late_failures_do_not_mutate() {
    let (world, mut genesis) = genesis_fixture();
    if let DomainEvent::MainTokenGenesisInitialized {
        total_supply,
        allocations,
    } = &mut genesis
    {
        *total_supply = 0;
        allocations.clear();
    }
    assert_error(world, genesis, "total_supply must be > 0");

    let (world, mut vesting) = fixtures().swap_remove(1);
    if let DomainEvent::MainTokenVestingClaimed { amount, nonce, .. } = &mut vesting {
        *amount = 0;
        *nonce = 0;
    }
    assert_error(world, vesting, "claim amount must be > 0");

    let (world, mut transfer) = fixtures().swap_remove(2);
    if let DomainEvent::MainTokenTransferred { amount, nonce, .. } = &mut transfer {
        *amount = 0;
        *nonce = 0;
    }
    assert_error(world, transfer, "transfer amount must be > 0");

    let (world, mut issuance) = fixtures().swap_remove(3);
    if let DomainEvent::MainTokenEpochIssued {
        issued_amount,
        staking_reward_amount,
        ..
    } = &mut issuance
    {
        *issued_amount = 1;
        *staking_reward_amount = u64::MAX;
    }
    assert_error(world, issuance, "epoch split overflow");

    let (mut world, mut fee) = fixtures().swap_remove(4);
    world.set_main_token_supply(MainTokenSupplyState {
        total_supply: 100,
        circulating_supply: 100,
        total_issued: 0,
        total_burned: u64::MAX,
    });
    if let DomainEvent::MainTokenFeeSettled {
        burn_amount,
        treasury_amount,
        amount,
        ..
    } = &mut fee
    {
        *amount = 10;
        *burn_amount = 1;
        *treasury_amount = 9;
    }
    assert_error(world, fee, "total_burned overflow");
}

#[test]
fn successful_compound_writes_and_routing_are_exact() {
    for (mut world, event) in fixtures() {
        let mailboxes = world
            .state()
            .agents
            .iter()
            .map(|(id, cell)| (id.clone(), cell.mailbox.clone()))
            .collect::<Vec<_>>();
        world
            .append_event_for_test(WorldEventBody::Domain(event.clone()), None)
            .unwrap();
        assert_eq!(
            world
                .state()
                .agents
                .iter()
                .map(|(id, cell)| (id.clone(), cell.mailbox.clone()))
                .collect::<Vec<_>>(),
            mailboxes
        );
        match event {
            DomainEvent::MainTokenGenesisInitialized { .. } => {
                assert_eq!(world.main_token_supply().total_supply, 1_000)
            }
            DomainEvent::MainTokenVestingClaimed { .. } => {
                assert_eq!(world.main_token_last_claim_nonce("alice"), Some(1));
                assert!(world.main_token_liquid_balance("alice") > 0);
            }
            DomainEvent::MainTokenTransferred { .. } => {
                assert_eq!(world.main_token_liquid_balance("alice"), 55);
                assert_eq!(world.main_token_liquid_balance("bob"), 45);
                assert_eq!(world.main_token_last_transfer_nonce("alice"), Some(1));
            }
            DomainEvent::MainTokenEpochIssued { epoch_index, .. } => {
                assert!(
                    world
                        .main_token_epoch_issuance_record(epoch_index)
                        .is_some()
                );
                assert!(world.main_token_supply().total_issued > 0);
            }
            DomainEvent::MainTokenFeeSettled {
                treasury_amount, ..
            } => {
                assert_eq!(world.main_token_supply().circulating_supply, 90);
                assert_eq!(
                    world.main_token_treasury_balance(MAIN_TOKEN_TREASURY_BUCKET_GAS_FEE),
                    treasury_amount
                );
            }
            _ => unreachable!(),
        }
    }
}

#[test]
fn late_errors_and_genesis_guards_preserve_every_monetary_field() {
    let (mut vesting_world, vesting) = fixtures().swap_remove(1);
    vesting_world
        .set_main_token_account_balance("alice", u64::MAX, 1_000)
        .unwrap();
    assert_error(vesting_world, vesting, "liquid balance overflow");

    let (mut transfer_world, transfer) = fixtures().swap_remove(2);
    transfer_world
        .set_main_token_account_balance("bob", u64::MAX, 0)
        .unwrap();
    assert_error(transfer_world, transfer, "target balance overflow");

    let (mut issuance_world, issuance) = fixtures().swap_remove(3);
    issuance_world
        .set_main_token_treasury_balance(MAIN_TOKEN_TREASURY_BUCKET_NODE_SERVICE_REWARD, u64::MAX)
        .unwrap();
    assert_error(issuance_world, issuance, "treasury balance overflow");

    let (mut genesis_world, genesis) = genesis_fixture();
    genesis_world
        .set_main_token_account_balance("preexisting", 1, 0)
        .unwrap();
    assert_error(genesis_world, genesis, "ledger is not empty");
}

#[test]
fn beneficiary_and_padded_transfer_routing_and_compound_writes_are_exact() {
    let (mut vesting_world, vesting) = fixtures().swap_remove(1);
    add_agent(&mut vesting_world, "alice");
    let before_active = vesting_world.state().agents["alice"].last_active;
    vesting_world
        .append_event_for_test(WorldEventBody::Domain(vesting.clone()), None)
        .unwrap();
    let alice = vesting_world.main_token_account_balance("alice").unwrap();
    assert_eq!((alice.liquid_balance, alice.vested_balance), (1_000, 0));
    assert_eq!(
        vesting_world
            .main_token_genesis_bucket("vesting")
            .unwrap()
            .claimed_amount,
        1_000
    );
    assert_eq!(vesting_world.main_token_supply().circulating_supply, 1_000);
    assert_eq!(
        vesting_world.state().agents["alice"].mailbox.back(),
        Some(&vesting)
    );
    assert_eq!(
        vesting_world.state().agents["alice"].last_active,
        before_active
    );

    let (mut transfer_world, mut transfer) = fixtures().swap_remove(2);
    add_agent(&mut transfer_world, "alice");
    let mailbox_len = transfer_world.state().agents["alice"].mailbox.len();
    if let DomainEvent::MainTokenTransferred {
        from_account_id, ..
    } = &mut transfer
    {
        *from_account_id = " alice ".into();
    }
    transfer_world
        .append_event_for_test(WorldEventBody::Domain(transfer), None)
        .unwrap();
    assert_eq!(transfer_world.main_token_liquid_balance("alice"), 55);
    assert_eq!(transfer_world.main_token_liquid_balance("bob"), 45);
    assert_eq!(
        transfer_world.state().agents["alice"].mailbox.len(),
        mailbox_len
    );

    let (mut issuance_world, issuance) = fixtures().swap_remove(3);
    issuance_world
        .append_event_for_test(WorldEventBody::Domain(issuance.clone()), None)
        .unwrap();
    let DomainEvent::MainTokenEpochIssued {
        epoch_index,
        issued_amount,
        staking_reward_amount,
        node_service_reward_amount,
        ecosystem_pool_amount,
        security_reserve_amount,
        ..
    } = issuance
    else {
        unreachable!()
    };
    assert_eq!(
        issuance_world.main_token_supply().total_issued,
        issued_amount
    );
    assert_eq!(
        issuance_world.main_token_supply().total_supply,
        1_000_000 + issued_amount
    );
    assert_eq!(
        issuance_world.main_token_treasury_balance(MAIN_TOKEN_TREASURY_BUCKET_STAKING_REWARD),
        staking_reward_amount
    );
    assert_eq!(
        issuance_world.main_token_treasury_balance(MAIN_TOKEN_TREASURY_BUCKET_NODE_SERVICE_REWARD),
        node_service_reward_amount
    );
    assert_eq!(
        issuance_world.main_token_treasury_balance(MAIN_TOKEN_TREASURY_BUCKET_ECOSYSTEM_POOL),
        ecosystem_pool_amount
    );
    assert_eq!(
        issuance_world.main_token_treasury_balance(MAIN_TOKEN_TREASURY_BUCKET_SECURITY_RESERVE),
        security_reserve_amount
    );
    assert_eq!(
        issuance_world
            .main_token_epoch_issuance_record(epoch_index)
            .unwrap()
            .issued_amount,
        issued_amount
    );
}

#[test]
fn monetary_events_normalize_material_forms_and_action_matches_raw_projection() {
    let (base, event) = fixtures().swap_remove(2);
    let world_key = MaterialLedgerId::world();
    let unrelated = MaterialLedgerId::site("unrelated-41a");
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
        world
            .append_event_for_test(WorldEventBody::Domain(event.clone()), None)
            .unwrap();
        assert_eq!(world.state().material_ledgers[&unrelated]["ore"], 9);
        assert_eq!(
            world.state().materials,
            world.state().material_ledgers[&world_key]
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

    let (base, event) = fixtures().swap_remove(2);
    let DomainEvent::MainTokenTransferred {
        from_account_id,
        to_account_id,
        amount,
        nonce,
        asset_id,
        memo,
    } = &event
    else {
        unreachable!()
    };
    let action = Action::TransferMainToken {
        from_account_id: from_account_id.clone(),
        to_account_id: to_account_id.clone(),
        amount: *amount,
        nonce: *nonce,
        asset_id: asset_id.clone(),
        memo: memo.clone(),
        chain_id: None,
        network_id: None,
        tx_version: None,
        tx_type: None,
        valid_until_unix_ms: None,
        max_fee: None,
        fee_asset_id: None,
        application_payload_hash: None,
        client_request_id: None,
    };
    let mut raw = base.clone();
    raw.step().unwrap();
    raw.append_event_for_test(WorldEventBody::Domain(event), None)
        .unwrap();
    let mut action_world = base;
    action_world.submit_action(action);
    action_world.step().unwrap();
    assert_eq!(action_world.state(), raw.state());
    assert_eq!(
        action_world.current_state_root_hash().unwrap(),
        raw.current_state_root_hash().unwrap()
    );
}
