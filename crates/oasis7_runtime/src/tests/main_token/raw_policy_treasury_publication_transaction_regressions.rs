use super::*;

fn fixtures() -> Vec<(World, DomainEvent)> {
    let mut policy = World::new();
    policy.set_main_token_config(MainTokenConfig {
        initial_supply: 100,
        ..Default::default()
    });
    policy.set_main_token_supply(MainTokenSupplyState {
        total_supply: 100,
        circulating_supply: 0,
        total_issued: 0,
        total_burned: 0,
    });
    let next = policy.main_token_config().clone();
    let mut treasury = World::new();
    treasury.set_main_token_supply(MainTokenSupplyState {
        total_supply: 100,
        circulating_supply: 0,
        total_issued: 100,
        total_burned: 0,
    });
    treasury
        .set_main_token_treasury_balance(MAIN_TOKEN_TREASURY_BUCKET_STAKING_REWARD, 50)
        .unwrap();
    vec![
        (
            policy,
            DomainEvent::MainTokenPolicyUpdateScheduled {
                proposal_id: 1,
                effective_epoch: 2,
                next,
            },
        ),
        (
            treasury,
            DomainEvent::MainTokenTreasuryDistributed {
                proposal_id: 1,
                distribution_id: "dist".into(),
                bucket_id: MAIN_TOKEN_TREASURY_BUCKET_STAKING_REWARD.into(),
                total_amount: 30,
                distributions: vec![
                    MainTokenTreasuryDistribution {
                        account_id: "alice".into(),
                        amount: 10,
                    },
                    MainTokenTreasuryDistribution {
                        account_id: "bob".into(),
                        amount: 20,
                    },
                ],
            },
        ),
    ]
}

fn unchanged(world: &World, before: &World, root: &str) {
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
fn policy_and_treasury_raw_events_are_atomic_retryable_and_replayable() {
    for (index, (mut world, event)) in fixtures().into_iter().enumerate() {
        let baseline = world.snapshot();
        let before = world.clone();
        let root = world.current_state_root_hash().unwrap();
        let len = world.journal().events.len();
        world.fail_next_append_after_publication_prepare_for_test();
        let error = world
            .append_event_for_test(
                WorldEventBody::Domain(event.clone()),
                Some(CausedBy::Action(4_500 + index as u64)),
            )
            .expect_err("postprepare fault");
        assert!(format!("{error:?}").contains("publication preparation"));
        unchanged(&world, &before, &root);
        let cause = Some(CausedBy::Action(4_510 + index as u64));
        world
            .append_event_for_test(WorldEventBody::Domain(event), cause.clone())
            .unwrap();
        assert_eq!(world.journal().events.len(), len + 1);
        assert_eq!(world.journal().events.last().unwrap().caused_by, cause);
        let replay = World::from_snapshot(baseline, world.journal().clone()).unwrap();
        assert_eq!(replay.snapshot(), world.snapshot());
        assert_eq!(
            replay.current_state_root_hash().unwrap(),
            world.current_state_root_hash().unwrap()
        );
    }
}

fn natural(mut world: World, event: DomainEvent, expected: &str) {
    let before = world.clone();
    let root = world.current_state_root_hash().unwrap();
    let error = world
        .append_event_for_test(WorldEventBody::Domain(event), None)
        .expect_err("natural error");
    assert!(format!("{error:?}").contains(expected), "{error:?}");
    unchanged(&world, &before, &root);
}

#[test]
fn policy_priority_and_second_recipient_overflow_are_nonmutating() {
    let (world, mut policy) = fixtures().swap_remove(0);
    if let DomainEvent::MainTokenPolicyUpdateScheduled {
        proposal_id,
        effective_epoch,
        ..
    } = &mut policy
    {
        *proposal_id = 0;
        *effective_epoch = 0;
    }
    natural(world, policy, "proposal_id must be > 0");
    let (mut world, distribution) = fixtures().swap_remove(1);
    world
        .set_main_token_account_balance("bob", u64::MAX, 0)
        .unwrap();
    natural(world, distribution, "treasury account overflow");
}

#[test]
fn success_compound_fields_materials_and_no_routing_are_exact() {
    let world_key = MaterialLedgerId::world();
    let other = MaterialLedgerId::site("other-41b1");
    for (mut world, event) in fixtures() {
        let mut state = world.state().clone();
        state.materials.insert("legacy".into(), 7);
        state.material_ledgers.remove(&world_key);
        state
            .material_ledgers
            .insert(other.clone(), [("ore".into(), 9)].into());
        world = World::new_with_state(state);
        world
            .append_event_for_test(WorldEventBody::Domain(event.clone()), None)
            .unwrap();
        assert_eq!(world.state().materials["legacy"], 7);
        assert_eq!(world.state().material_ledgers[&other]["ore"], 9);
        match event {
            DomainEvent::MainTokenPolicyUpdateScheduled {
                effective_epoch, ..
            } => assert!(
                world
                    .main_token_scheduled_policy_update(effective_epoch)
                    .is_some()
            ),
            DomainEvent::MainTokenTreasuryDistributed { .. } => {
                assert_eq!(
                    world.main_token_treasury_balance(MAIN_TOKEN_TREASURY_BUCKET_STAKING_REWARD),
                    20
                );
                assert_eq!(world.main_token_liquid_balance("alice"), 10);
                assert_eq!(world.main_token_liquid_balance("bob"), 20);
                assert_eq!(world.main_token_supply().circulating_supply, 30);
                assert!(
                    world
                        .main_token_treasury_distribution_record("dist")
                        .is_some()
                );
            }
            _ => unreachable!(),
        }
    }
}

#[test]
fn full_policy_and_treasury_validation_priority_table_is_nonmutating() {
    let policy_case = |mutate: fn(&mut DomainEvent), expected: &str| {
        let (world, mut event) = fixtures().swap_remove(0);
        mutate(&mut event);
        natural(world, event, expected);
    };
    policy_case(
        |event| {
            if let DomainEvent::MainTokenPolicyUpdateScheduled {
                effective_epoch, ..
            } = event
            {
                *effective_epoch = 0
            }
        },
        "effective_epoch must be > now",
    );
    policy_case(
        |event| {
            if let DomainEvent::MainTokenPolicyUpdateScheduled { next, .. } = event {
                next.inflation_policy.min_rate_bps = 900;
                next.inflation_policy.max_rate_bps = 800
            }
        },
        "config out of bounds",
    );
    policy_case(
        |event| {
            if let DomainEvent::MainTokenPolicyUpdateScheduled { next, .. } = event {
                next.initial_supply = 101
            }
        },
        "cannot change initial_supply",
    );
    let (mut world, mut event) = fixtures().swap_remove(0);
    world.set_main_token_supply(MainTokenSupplyState {
        total_supply: 110,
        circulating_supply: 0,
        total_issued: 110,
        total_burned: 0,
    });
    if let DomainEvent::MainTokenPolicyUpdateScheduled { next, .. } = &mut event {
        next.max_supply = Some(100);
    }
    natural(world, event, "max_supply cannot be below");
    for duplicate_proposal in [false, true] {
        let (mut world, event) = fixtures().swap_remove(0);
        world
            .append_event_for_test(WorldEventBody::Domain(event.clone()), None)
            .unwrap();
        let mut duplicate = event;
        if !duplicate_proposal {
            if let DomainEvent::MainTokenPolicyUpdateScheduled {
                proposal_id,
                effective_epoch,
                ..
            } = &mut duplicate
            {
                *proposal_id = 2;
                *effective_epoch = 2;
            }
            natural(world, duplicate, "effective_epoch already scheduled");
        } else {
            if let DomainEvent::MainTokenPolicyUpdateScheduled {
                effective_epoch, ..
            } = &mut duplicate
            {
                *effective_epoch = 3;
            }
            natural(world, duplicate, "proposal already scheduled");
        }
    }

    let treasury_case = |mutate: fn(&mut DomainEvent), expected: &str| {
        let (world, mut event) = fixtures().swap_remove(1);
        mutate(&mut event);
        natural(world, event, expected);
    };
    treasury_case(
        |event| {
            if let DomainEvent::MainTokenTreasuryDistributed {
                proposal_id,
                distribution_id,
                ..
            } = event
            {
                *proposal_id = 0;
                distribution_id.clear()
            }
        },
        "proposal_id must be > 0",
    );
    treasury_case(
        |event| {
            if let DomainEvent::MainTokenTreasuryDistributed {
                distribution_id, ..
            } = event
            {
                distribution_id.clear()
            }
        },
        "distribution_id cannot be empty",
    );
    treasury_case(
        |event| {
            if let DomainEvent::MainTokenTreasuryDistributed { bucket_id, .. } = event {
                *bucket_id = "invalid".into()
            }
        },
        "bucket is not allowed",
    );
    treasury_case(
        |event| {
            if let DomainEvent::MainTokenTreasuryDistributed {
                total_amount,
                distributions,
                ..
            } = event
            {
                *total_amount = 0;
                distributions.clear()
            }
        },
        "total_amount must be > 0",
    );
    treasury_case(
        |event| {
            if let DomainEvent::MainTokenTreasuryDistributed { distributions, .. } = event {
                distributions.clear()
            }
        },
        "list cannot be empty",
    );
    treasury_case(
        |event| {
            if let DomainEvent::MainTokenTreasuryDistributed { distributions, .. } = event {
                distributions[0].account_id.clear();
                distributions[0].amount = 0
            }
        },
        "account_id cannot be empty",
    );
    treasury_case(
        |event| {
            if let DomainEvent::MainTokenTreasuryDistributed { distributions, .. } = event {
                distributions[0].amount = 0
            }
        },
        "amount must be > 0",
    );
    treasury_case(
        |event| {
            if let DomainEvent::MainTokenTreasuryDistributed { distributions, .. } = event {
                distributions[1].account_id = distributions[0].account_id.clone()
            }
        },
        "duplicate main token treasury distribution account_id",
    );
    treasury_case(
        |event| {
            if let DomainEvent::MainTokenTreasuryDistributed { total_amount, .. } = event {
                *total_amount = 31
            }
        },
        "sum mismatch",
    );
    treasury_case(
        |event| {
            if let DomainEvent::MainTokenTreasuryDistributed {
                total_amount,
                distributions,
                ..
            } = event
            {
                *total_amount = u64::MAX;
                distributions[0].amount = u64::MAX;
                distributions[1].amount = 1;
            }
        },
        "sum overflow",
    );
    treasury_case(
        |event| {
            if let DomainEvent::MainTokenTreasuryDistributed {
                total_amount,
                distributions,
                ..
            } = event
            {
                *total_amount = 60;
                distributions[1].amount = 50
            }
        },
        "bucket insufficient",
    );
    let (mut world, event) = fixtures().swap_remove(1);
    world
        .append_event_for_test(WorldEventBody::Domain(event.clone()), None)
        .unwrap();
    natural(world, event, "distribution_id already exists");
}

#[test]
fn material_precedence_mailboxes_and_action_raw_published_roots_match() {
    let world_key = MaterialLedgerId::world();
    let unrelated = MaterialLedgerId::site("material-41b1");
    for mode in 0..3 {
        let (world, event) = fixtures().swap_remove(1);
        let mut state = world.state().clone();
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
        world
            .append_event_for_test(
                WorldEventBody::Domain(DomainEvent::AgentRegistered {
                    agent_id: "alice".into(),
                    pos: oasis7_agent_api::world_action::GeoPos::new(0, 0, 0),
                }),
                None,
            )
            .unwrap();
        let mailbox = world.state().agents["alice"].mailbox.clone();
        let baseline = world.snapshot();
        world
            .append_event_for_test(WorldEventBody::Domain(event), None)
            .unwrap();
        assert_eq!(world.state().agents["alice"].mailbox, mailbox);
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

    let mut base = World::new();
    base.set_main_token_config(MainTokenConfig {
        initial_supply: 100,
        ..Default::default()
    });
    base.set_main_token_supply(MainTokenSupplyState {
        total_supply: 100,
        circulating_supply: 0,
        total_issued: 0,
        total_burned: 0,
    });
    let proposal_id = base
        .propose_manifest_update(base.manifest().clone(), "alice")
        .unwrap();
    base.shadow_proposal(proposal_id).unwrap();
    base.approve_proposal(proposal_id, "bob", ProposalDecision::Approve)
        .unwrap();
    let next = base.main_token_config().clone();
    let mut action_world = base.clone();
    action_world.submit_action(Action::UpdateMainTokenPolicy { proposal_id, next });
    action_world.step().unwrap();
    let event = action_world
        .journal()
        .events
        .iter()
        .rev()
        .find_map(|entry| match &entry.body {
            WorldEventBody::Domain(event @ DomainEvent::MainTokenPolicyUpdateScheduled { .. }) => {
                Some(event.clone())
            }
            _ => None,
        })
        .unwrap();
    let mut raw = base;
    raw.step().unwrap();
    raw.append_event_for_test(WorldEventBody::Domain(event), None)
        .unwrap();
    assert_eq!(raw.state(), action_world.state());
    assert_eq!(
        raw.current_state_root_hash().unwrap(),
        action_world.current_state_root_hash().unwrap()
    );
}
