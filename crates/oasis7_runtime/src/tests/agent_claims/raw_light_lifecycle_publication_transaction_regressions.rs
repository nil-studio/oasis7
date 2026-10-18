use super::*;

fn claimed(balance: u64) -> World {
    let mut world = setup_claim_world(balance, 0);
    world.submit_action(Action::ClaimAgent {
        claimer_agent_id: "alice".into(),
        target_agent_id: "bob".into(),
    });
    world.step().unwrap();
    world
}

fn release() -> (World, DomainEvent) {
    let world = claimed(2_000);
    let mut state = world.state().clone();
    state
        .agent_claims
        .get_mut("bob")
        .unwrap()
        .upkeep_paid_through_epoch = 100;
    let base = World::new_with_state(state);
    let mut action = base.clone();
    action.submit_action(Action::ReleaseAgentClaim {
        claimer_agent_id: "alice".into(),
        target_agent_id: "bob".into(),
    });
    action.step().unwrap();
    let event = action
        .journal()
        .events
        .iter()
        .rev()
        .find_map(|entry| match &entry.body {
            WorldEventBody::Domain(event @ DomainEvent::AgentClaimReleaseRequested { .. }) => {
                Some(event.clone())
            }
            _ => None,
        })
        .unwrap();
    (base, event)
}

fn tick(idle: bool) -> (World, DomainEvent) {
    let mut world = claimed(if idle { 2_000 } else { 325 });
    for _ in 0..400 {
        let before = world.clone();
        let len = world.journal().events.len();
        world.step().unwrap();
        let found = world.journal().events[len..]
            .iter()
            .find_map(|entry| match &entry.body {
                WorldEventBody::Domain(event @ DomainEvent::AgentClaimEnteredGrace { .. })
                    if !idle =>
                {
                    Some(event.clone())
                }
                WorldEventBody::Domain(event @ DomainEvent::AgentClaimIdleWarning { .. })
                    if idle =>
                {
                    Some(event.clone())
                }
                _ => None,
            });
        if let Some(event) = found {
            return (before, event);
        }
    }
    panic!("tick lifecycle event not produced")
}

fn fixtures() -> Vec<(World, DomainEvent)> {
    vec![release(), tick(false), tick(true)]
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
fn all_three_are_atomic_retryable_and_replayable() {
    for (index, (mut world, event)) in fixtures().into_iter().enumerate() {
        let baseline = world.snapshot();
        let before = world.clone();
        let root = world.current_state_root_hash().unwrap();
        let len = world.journal().events.len();
        world.fail_next_append_after_publication_prepare_for_test();
        let error = world
            .append_event_for_test(
                WorldEventBody::Domain(event.clone()),
                Some(CausedBy::Action(4_800 + index as u64)),
            )
            .expect_err("postprepare failure");
        assert!(format!("{error:?}").contains("publication preparation"));
        unchanged(&world, &before, &root);
        let cause = Some(CausedBy::Action(4_810 + index as u64));
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
fn actor_claim_and_epoch_priorities_are_exact() {
    for (world, event) in fixtures() {
        let mut changed = event.clone();
        set_ids(&mut changed, "missing", None);
        natural(world.clone(), changed, "AgentNotFound");
        let mut changed = event.clone();
        set_ids(&mut changed, "alice", Some("missing"));
        natural(world.clone(), changed, "agent claim not found");
        let mut changed = event;
        set_ids(&mut changed, "carol", None);
        natural(world, changed, "owner mismatch");
    }
    let (world, mut event) = release();
    if let DomainEvent::AgentClaimReleaseRequested {
        requested_at_epoch,
        ready_at_epoch,
        ..
    } = &mut event
    {
        *requested_at_epoch = 2;
        *ready_at_epoch = 1;
    }
    natural(world, event, "release epoch mismatch");
    let (mut world, event) = release();
    world
        .append_event_for_test(WorldEventBody::Domain(event.clone()), None)
        .unwrap();
    natural(world, event, "already requested");
    let (world, mut event) = tick(false);
    if let DomainEvent::AgentClaimEnteredGrace {
        upkeep_arrears_amount,
        grace_deadline_epoch,
        delinquent_since_epoch,
        ..
    } = &mut event
    {
        *upkeep_arrears_amount = 0;
        *grace_deadline_epoch = delinquent_since_epoch.saturating_sub(1);
    }
    natural(world, event, "arrears must be positive");
    let (world, mut event) = tick(false);
    if let DomainEvent::AgentClaimEnteredGrace {
        grace_deadline_epoch,
        delinquent_since_epoch,
        ..
    } = &mut event
    {
        *grace_deadline_epoch = delinquent_since_epoch.saturating_sub(1);
    }
    natural(world, event, "grace epoch mismatch");
    let (world, mut event) = tick(true);
    if let DomainEvent::AgentClaimIdleWarning {
        warning_emitted_at_epoch,
        forced_reclaim_at_epoch,
        ..
    } = &mut event
    {
        *forced_reclaim_at_epoch = warning_emitted_at_epoch.saturating_sub(1);
    }
    natural(world, event, "idle warning epoch mismatch");
}

fn set_ids(event: &mut DomainEvent, owner: &str, target: Option<&str>) {
    match event {
        DomainEvent::AgentClaimReleaseRequested {
            claimer_agent_id,
            target_agent_id,
            ..
        }
        | DomainEvent::AgentClaimEnteredGrace {
            claimer_agent_id,
            target_agent_id,
            ..
        }
        | DomainEvent::AgentClaimIdleWarning {
            claimer_agent_id,
            target_agent_id,
            ..
        } => {
            *claimer_agent_id = owner.into();
            if let Some(target) = target {
                *target_agent_id = target.into();
            }
        }
        _ => unreachable!(),
    }
}

#[test]
fn success_has_no_monetary_side_effects_routes_owner_only_and_matches_action() {
    let world_key = MaterialLedgerId::world();
    let unrelated = MaterialLedgerId::site("claim-light-unrelated");
    for material_mode in 0..3 {
        for (world, event) in fixtures() {
            let mut state = world.state().clone();
            state.time += 7;
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
            let supply = world.main_token_supply().clone();
            let balances = world.state().main_token_balances.clone();
            let treasury = world.state().main_token_treasury_balances.clone();
            let owner_mail = world.state().agents["alice"].mailbox.len();
            let owner_activity = world.state().agents["alice"].last_active;
            let target = world.state().agents["bob"].clone();
            let last_processed = world.state().agent_claim_last_processed_epoch;
            world
                .append_event_for_test(WorldEventBody::Domain(event.clone()), None)
                .unwrap();
            assert_eq!(world.main_token_supply(), &supply);
            assert_eq!(world.state().main_token_balances, balances);
            assert_eq!(world.state().main_token_treasury_balances, treasury);
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
            assert_eq!(world.state().agents["alice"].mailbox.len(), owner_mail + 1);
            assert_eq!(world.state().agents["bob"], target);
            assert_eq!(
                world.state().agent_claim_last_processed_epoch,
                last_processed
            );
            let claim = world.agent_claim("bob").unwrap();
            match event {
                DomainEvent::AgentClaimReleaseRequested {
                    requested_at_epoch,
                    ready_at_epoch,
                    ..
                } => {
                    assert_eq!(claim.release_requested_at_epoch, Some(requested_at_epoch));
                    assert_eq!(claim.release_ready_at_epoch, Some(ready_at_epoch));
                    assert_eq!(
                        world.state().agents["alice"].last_active,
                        world.state().time
                    );
                }
                DomainEvent::AgentClaimEnteredGrace {
                    delinquent_since_epoch,
                    grace_deadline_epoch,
                    ..
                } => {
                    assert_eq!(claim.delinquent_since_epoch, Some(delinquent_since_epoch));
                    assert_eq!(claim.grace_deadline_epoch, Some(grace_deadline_epoch));
                    assert_eq!(world.state().agents["alice"].last_active, owner_activity);
                }
                DomainEvent::AgentClaimIdleWarning {
                    warning_emitted_at_epoch,
                    ..
                } => {
                    assert_eq!(
                        claim.idle_warning_emitted_at_epoch,
                        Some(warning_emitted_at_epoch)
                    );
                    assert_eq!(world.state().agents["alice"].last_active, owner_activity);
                }
                _ => unreachable!(),
            }
        }
    }
    let (base, _) = release();
    let mut action = base.clone();
    action.submit_action(Action::ReleaseAgentClaim {
        claimer_agent_id: "alice".into(),
        target_agent_id: "bob".into(),
    });
    action.step().unwrap();
    let event = action
        .journal()
        .events
        .iter()
        .rev()
        .find_map(|entry| match &entry.body {
            WorldEventBody::Domain(event @ DomainEvent::AgentClaimReleaseRequested { .. }) => {
                Some(event.clone())
            }
            _ => None,
        })
        .unwrap();
    let mut raw_state = base.state().clone();
    raw_state.time = action.state().time;
    let mut raw = World::new_with_state(raw_state);
    raw.append_event_for_test(WorldEventBody::Domain(event), None)
        .unwrap();
    let mut expected = action.state().clone();
    expected.agents = raw.state().agents.clone();
    assert_eq!(raw.state(), &expected);
    assert_eq!(
        raw.current_state_root_hash().unwrap(),
        World::new_with_state(expected)
            .current_state_root_hash()
            .unwrap()
    );
}

#[test]
fn grace_and_idle_tick_raw_claim_and_published_roots_match() {
    for idle in [false, true] {
        let (before, event) = tick(idle);
        let mut tick_world = before.clone();
        tick_world.step().unwrap();
        let mut raw_state = before.state().clone();
        raw_state.time = tick_world.state().time;
        let mut raw = World::new_with_state(raw_state);
        raw.append_event_for_test(WorldEventBody::Domain(event), None)
            .unwrap();
        let raw_claim = raw.agent_claim("bob").unwrap();
        let tick_claim = tick_world.agent_claim("bob").unwrap();
        if idle {
            assert_eq!(
                raw_claim.idle_warning_emitted_at_epoch,
                tick_claim.idle_warning_emitted_at_epoch
            );
        } else {
            assert_eq!(
                raw_claim.delinquent_since_epoch,
                tick_claim.delinquent_since_epoch
            );
            assert_eq!(
                raw_claim.grace_deadline_epoch,
                tick_claim.grace_deadline_epoch
            );
        }
        let mut expected = tick_world.state().clone();
        expected.agents = raw.state().agents.clone();
        expected.agent_claims = raw.state().agent_claims.clone();
        expected.agent_claim_last_processed_epoch = raw.state().agent_claim_last_processed_epoch;
        expected.main_token_supply = raw.state().main_token_supply.clone();
        expected.main_token_balances = raw.state().main_token_balances.clone();
        expected.main_token_treasury_balances = raw.state().main_token_treasury_balances.clone();
        assert_eq!(raw.state(), &expected);
        assert_eq!(
            raw.current_state_root_hash().unwrap(),
            World::new_with_state(expected)
                .current_state_root_hash()
                .unwrap()
        );
    }
}
