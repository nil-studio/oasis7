use super::*;

fn starter_fixture(mode: bool) -> (World, DomainEvent) {
    let mut world = World::new();
    world
        .append_event_for_test(
            WorldEventBody::Domain(DomainEvent::AgentRegistered {
                agent_id: "alice".into(),
                pos: oasis7_agent_api::world_action::GeoPos::new(0, 0, 0),
            }),
            None,
        )
        .unwrap();
    let source = mode.then(|| MAIN_TOKEN_TREASURY_BUCKET_ECOSYSTEM_POOL.to_string());
    if mode {
        world.set_main_token_supply(MainTokenSupplyState {
            total_supply: 100,
            circulating_supply: 0,
            total_issued: 100,
            total_burned: 0,
        });
        world
            .set_main_token_treasury_balance(MAIN_TOKEN_TREASURY_BUCKET_ECOSYSTEM_POOL, 50)
            .unwrap();
    }
    (
        world,
        DomainEvent::StarterOcClaimed {
            agent_id: "alice".into(),
            player_id: "player".into(),
            public_key: Some("key".into()),
            amount: 10,
            claimed_at: 0,
            source_treasury_bucket_id: source,
        },
    )
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
fn raw_starter_claim_is_atomic_retryable_and_replayable_in_both_funding_modes() {
    for (index, mode) in [false, true].into_iter().enumerate() {
        let (mut world, event) = starter_fixture(mode);
        let baseline = world.snapshot();
        let before = world.clone();
        let root = world.current_state_root_hash().unwrap();
        let len = world.journal().events.len();
        world.fail_next_append_after_publication_prepare_for_test();
        let error = world
            .append_event_for_test(
                WorldEventBody::Domain(event.clone()),
                Some(CausedBy::Action(4_700 + index as u64)),
            )
            .expect_err("postprepare failure");
        assert!(format!("{error:?}").contains("publication preparation"));
        unchanged(&world, &before, &root);
        let cause = Some(CausedBy::Action(4_710 + index as u64));
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

#[test]
fn treasury_and_mint_late_failures_preserve_the_entire_world() {
    for mode in [false, true] {
        let (world, event) = starter_fixture(mode);
        let mut state = world.state().clone();
        state.main_token_balances.insert(
            "alice".into(),
            MainTokenAccountBalance {
                account_id: "wrong".into(),
                ..Default::default()
            },
        );
        natural(
            World::new_with_state(state),
            event.clone(),
            "account key mismatch",
        );

        let (world, event) = starter_fixture(mode);
        let mut state = world.state().clone();
        state.main_token_balances.insert(
            "alice".into(),
            MainTokenAccountBalance {
                account_id: "alice".into(),
                liquid_balance: u64::MAX,
                ..Default::default()
            },
        );
        natural(
            World::new_with_state(state),
            event,
            "liquid balance overflow",
        );
    }

    let (world, event) = starter_fixture(true);
    let mut state = world.state().clone();
    state.main_token_supply.circulating_supply = u64::MAX;
    state.main_token_supply.total_supply = u64::MAX;
    natural(World::new_with_state(state), event, "circulating overflow");
    let (world, event) = starter_fixture(true);
    let mut state = world.state().clone();
    state.main_token_supply.circulating_supply = 100;
    natural(
        World::new_with_state(state),
        event,
        "circulating exceeds total",
    );
    let (world, event) = starter_fixture(true);
    let mut state = world.state().clone();
    state
        .main_token_treasury_balances
        .insert(MAIN_TOKEN_TREASURY_BUCKET_ECOSYSTEM_POOL.into(), 9);
    natural(World::new_with_state(state), event, "treasury insufficient");

    let (world, event) = starter_fixture(false);
    let mut state = world.state().clone();
    state.main_token_supply.total_supply = u64::MAX;
    natural(
        World::new_with_state(state),
        event.clone(),
        "total supply overflow",
    );
    let (world, event) = starter_fixture(false);
    let mut state = world.state().clone();
    state.main_token_supply.total_issued = u64::MAX;
    natural(
        World::new_with_state(state),
        event.clone(),
        "total issued overflow",
    );
    let (world, event) = starter_fixture(false);
    let mut state = world.state().clone();
    state.main_token_supply.circulating_supply = u64::MAX;
    natural(World::new_with_state(state), event, "circulating overflow");
}

#[test]
fn priorities_success_routing_materials_and_action_raw_parity_are_exact() {
    let (world, mut event) = starter_fixture(true);
    if let DomainEvent::StarterOcClaimed {
        agent_id,
        player_id,
        amount,
        ..
    } = &mut event
    {
        *agent_id = "missing".into();
        player_id.clear();
        *amount = 0;
    }
    natural(world, event, "AgentNotFound");
    let (world, mut event) = starter_fixture(true);
    if let DomainEvent::StarterOcClaimed {
        player_id, amount, ..
    } = &mut event
    {
        player_id.clear();
        *amount = 0;
    }
    natural(world, event, "player_id cannot be empty");
    let (world, mut event) = starter_fixture(true);
    if let DomainEvent::StarterOcClaimed { amount, .. } = &mut event {
        *amount = 0;
    }
    natural(world, event, "amount must be > 0");
    let (mut duplicate, event) = starter_fixture(true);
    duplicate
        .append_event_for_test(WorldEventBody::Domain(event.clone()), None)
        .unwrap();
    natural(duplicate, event, "already claimed");

    let world_key = MaterialLedgerId::world();
    let unrelated = MaterialLedgerId::site("starter-oc-unrelated");
    for funding_mode in [false, true] {
        for material_mode in 0..3 {
            let (mut world, event) = starter_fixture(funding_mode);
            world
                .append_event_for_test(
                    WorldEventBody::Domain(DomainEvent::AgentRegistered {
                        agent_id: "bob".into(),
                        pos: oasis7_agent_api::world_action::GeoPos::new(1, 0, 0),
                    }),
                    None,
                )
                .unwrap();
            let mut state = world.state().clone();
            state.time = 7;
            state.materials.insert("legacy".into(), 7);
            state
                .material_ledgers
                .insert(unrelated.clone(), [("ore".into(), 9)].into());
            match material_mode {
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
            let mailbox = world.state().agents["alice"].mailbox.len();
            let unrelated_agent = world.state().agents["bob"].clone();
            world
                .append_event_for_test(WorldEventBody::Domain(event), None)
                .unwrap();
            assert_eq!(world.main_token_liquid_balance("alice"), 10);
            let claim = &world.state().starter_oc_claims["alice"];
            assert_eq!(claim.agent_id, "alice");
            assert_eq!(claim.player_id, "player");
            assert_eq!(claim.public_key.as_deref(), Some("key"));
            assert_eq!(claim.amount, 10);
            assert_eq!(claim.claimed_at, 0);
            assert_eq!(claim.source_treasury_bucket_id.is_some(), funding_mode);
            assert_eq!(world.state().agents["alice"].mailbox.len(), mailbox + 1);
            assert_eq!(world.state().agents["alice"].last_active, 7);
            assert_eq!(world.state().agents["bob"], unrelated_agent);
            assert_eq!(world.state().material_ledgers[&unrelated]["ore"], 9);
            assert_eq!(
                world.state().materials,
                world.state().material_ledgers[&world_key]
            );
            if material_mode == 2 {
                assert_eq!(world.state().materials["canonical"], 11);
            } else {
                assert_eq!(world.state().materials["legacy"], 7);
            }
            if funding_mode {
                assert_eq!(
                    world.main_token_treasury_balance(MAIN_TOKEN_TREASURY_BUCKET_ECOSYSTEM_POOL),
                    40
                );
                assert_eq!(world.main_token_supply().total_supply, 100);
            } else {
                assert_eq!(world.main_token_supply().total_supply, 10);
                assert_eq!(world.main_token_supply().total_issued, 10);
            }
            assert_eq!(world.main_token_supply().circulating_supply, 10);
        }
    }

    let (base, _) = starter_fixture(false);
    let mut action = base.clone();
    action.submit_action(Action::ClaimStarterOc {
        agent_id: "alice".into(),
        player_id: " player ".into(),
        public_key: Some("key".into()),
    });
    action.step().unwrap();
    let event = action
        .journal()
        .events
        .iter()
        .rev()
        .find_map(|entry| match &entry.body {
            WorldEventBody::Domain(event @ DomainEvent::StarterOcClaimed { .. }) => {
                Some(event.clone())
            }
            _ => None,
        })
        .expect("action claim event");
    let mut raw = base;
    raw.step().unwrap();
    raw.append_event_for_test(WorldEventBody::Domain(event), None)
        .unwrap();
    assert_eq!(raw.state(), action.state());
    assert_eq!(
        raw.current_state_root_hash().unwrap(),
        action.current_state_root_hash().unwrap()
    );
}
