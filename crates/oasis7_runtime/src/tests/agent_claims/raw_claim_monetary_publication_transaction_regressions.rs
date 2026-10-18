use super::*;

fn claim_fixture() -> (World, DomainEvent) {
    let base = setup_claim_world_with_balances(250, 75, 0);
    let mut action = base.clone();
    action.submit_action(Action::ClaimAgent {
        claimer_agent_id: "alice".into(),
        target_agent_id: "bob".into(),
    });
    action.step().expect("capture claim action");
    let event = action
        .journal()
        .events
        .iter()
        .rev()
        .find_map(|entry| match &entry.body {
            WorldEventBody::Domain(event @ DomainEvent::AgentClaimed { .. }) => Some(event.clone()),
            _ => None,
        })
        .expect("claim event");
    (base, event)
}

fn auto_funded_claim_fixture() -> (World, DomainEvent) {
    let base = setup_claim_world_with_treasury(225, 100, 0);
    let mut action = base.clone();
    action.submit_action(Action::ClaimAgent {
        claimer_agent_id: "alice".into(),
        target_agent_id: "bob".into(),
    });
    action.step().expect("capture auto-funded claim");
    let event = action
        .journal()
        .events
        .iter()
        .rev()
        .find_map(|entry| match &entry.body {
            WorldEventBody::Domain(event @ DomainEvent::AgentClaimed { .. }) => Some(event.clone()),
            _ => None,
        })
        .expect("auto-funded claim event");
    (base, event)
}

fn upkeep_fixture() -> (World, DomainEvent) {
    let mut world = setup_claim_world_with_balances(1_000, 100, 0);
    world.submit_action(Action::ClaimAgent {
        claimer_agent_id: "alice".into(),
        target_agent_id: "bob".into(),
    });
    world.step().expect("seed claim");
    let next_epoch = world.current_governance_epoch().saturating_add(1);
    let event = world
        .prepared_next_agent_claim_event_for_test("bob", next_epoch)
        .expect("prepare upkeep")
        .expect("upkeep event");
    assert!(matches!(event, DomainEvent::AgentClaimUpkeepSettled { .. }));
    (world, event)
}

fn fixtures() -> [(World, DomainEvent); 2] {
    [claim_fixture(), upkeep_fixture()]
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
        .expect_err("natural rejection");
    assert!(format!("{error:?}").contains(expected), "{error:?}");
    unchanged(&world, &before, &root);
}

fn atomic_retry(mut world: World, event: DomainEvent, cause_id: u64) {
    let baseline = world.snapshot();
    let before = world.clone();
    let root = world.current_state_root_hash().unwrap();
    let journal_len = world.journal().events.len();
    world.fail_next_append_after_publication_prepare_for_test();
    let error = world
        .append_event_for_test(
            WorldEventBody::Domain(event.clone()),
            Some(CausedBy::Action(cause_id)),
        )
        .expect_err("postprepare failure");
    assert!(format!("{error:?}").contains("publication preparation"));
    unchanged(&world, &before, &root);

    let cause = Some(CausedBy::Action(cause_id + 10));
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

#[test]
fn claim_is_atomic_retryable_and_replayable() {
    let (world, event) = claim_fixture();
    atomic_retry(world, event, 5_100);
}

#[test]
fn upkeep_is_atomic_retryable_and_replayable() {
    let (world, event) = upkeep_fixture();
    atomic_retry(world, event, 5_101);
}

#[test]
fn raw_validation_priority_is_exact() {
    let (world, event) = claim_fixture();
    let mut changed = event.clone();
    if let DomainEvent::AgentClaimed {
        claimer_agent_id, ..
    } = &mut changed
    {
        *claimer_agent_id = "missing".into();
    }
    natural(world.clone(), changed, "AgentNotFound");
    let mut changed = event.clone();
    if let DomainEvent::AgentClaimed {
        target_agent_id, ..
    } = &mut changed
    {
        *target_agent_id = "missing".into();
    }
    natural(world.clone(), changed, "AgentNotFound");
    let mut changed = event.clone();
    if let DomainEvent::AgentClaimed {
        reputation_tier, ..
    } = &mut changed
    {
        *reputation_tier += 1;
    }
    natural(world.clone(), changed, "quote fields diverged");
    let mut changed = event.clone();
    if let DomainEvent::AgentClaimed {
        activation_fee_amount,
        ..
    } = &mut changed
    {
        *activation_fee_amount = 0;
    }
    natural(world.clone(), changed, "quote fields diverged");
    let mut changed = event.clone();
    if let DomainEvent::AgentClaimed {
        activation_fee_amount,
        ..
    } = &mut changed
    {
        *activation_fee_amount = u64::MAX;
    }
    natural(world.clone(), changed, "quote fields diverged");
    let mut changed = event.clone();
    if let DomainEvent::AgentClaimed {
        upkeep_paid_through_epoch,
        claimed_at_epoch,
        ..
    } = &mut changed
    {
        *upkeep_paid_through_epoch = claimed_at_epoch.saturating_sub(1);
    }
    natural(world.clone(), changed, "upkeep epoch mismatch");
    let mut changed = event.clone();
    if let DomainEvent::AgentClaimed {
        auto_issued_restricted_amount,
        ..
    } = &mut changed
    {
        *auto_issued_restricted_amount += 1;
    }
    natural(world.clone(), changed, "auto funding mismatch");
    let mut changed = event;
    if let DomainEvent::AgentClaimed {
        upfront_liquid_spent_amount,
        ..
    } = &mut changed
    {
        *upfront_liquid_spent_amount += 1;
    }
    natural(world, changed, "funding split mismatch");

    let (mut claimed, duplicate) = claim_fixture();
    claimed
        .append_event_for_test(WorldEventBody::Domain(duplicate.clone()), None)
        .unwrap();
    natural(claimed, duplicate, "already exists");

    let (world, event) = upkeep_fixture();
    let mut changed = event.clone();
    if let DomainEvent::AgentClaimUpkeepSettled {
        claimer_agent_id, ..
    } = &mut changed
    {
        *claimer_agent_id = "missing".into();
    }
    natural(world.clone(), changed, "AgentNotFound");
    let mut changed = event.clone();
    if let DomainEvent::AgentClaimUpkeepSettled {
        target_agent_id, ..
    } = &mut changed
    {
        *target_agent_id = "missing".into();
    }
    natural(world.clone(), changed, "agent claim not found");
    let mut changed = event.clone();
    if let DomainEvent::AgentClaimUpkeepSettled {
        claimer_agent_id, ..
    } = &mut changed
    {
        *claimer_agent_id = "carol".into();
    }
    natural(world.clone(), changed, "owner mismatch");
    let mut changed = event.clone();
    if let DomainEvent::AgentClaimUpkeepSettled { charged_epochs, .. } = &mut changed {
        *charged_epochs = 0;
    }
    natural(world.clone(), changed, "must be positive");
    let mut changed = event.clone();
    if let DomainEvent::AgentClaimUpkeepSettled { charged_epochs, .. } = &mut changed {
        *charged_epochs = u64::MAX;
    }
    natural(world.clone(), changed, "upkeep overflow");
    let mut changed = event;
    if let DomainEvent::AgentClaimUpkeepSettled { amount, .. } = &mut changed {
        *amount += 1;
    }
    natural(world, changed, "upkeep mismatch");
    let (world, mut changed) = upkeep_fixture();
    if let DomainEvent::AgentClaimUpkeepSettled {
        restricted_spent_amount,
        liquid_spent_amount,
        ..
    } = &mut changed
    {
        *restricted_spent_amount = 0;
        *liquid_spent_amount = 0;
    }
    natural(world, changed, "funding split mismatch");
}

#[test]
fn claim_late_treasury_failure_is_nonmutating() {
    let (world, event) = auto_funded_claim_fixture();
    let mut state = world.state().clone();
    state
        .main_token_treasury_balances
        .insert(MAIN_TOKEN_TREASURY_BUCKET_ECOSYSTEM_POOL.into(), u64::MAX);
    let world = World::new_with_state(state);
    natural(world, event, "treasury");

    let (world, event) = claim_fixture();
    let mut state = world.state().clone();
    state.main_token_supply.circulating_supply = 0;
    natural(World::new_with_state(state), event, "circulating");

    let (world, event) = claim_fixture();
    let mut state = world.state().clone();
    state.main_token_supply.total_supply = 0;
    natural(World::new_with_state(state), event, "supply");
}

#[test]
fn upkeep_late_treasury_failure_is_nonmutating() {
    let (world, event) = upkeep_fixture();
    let mut state = world.state().clone();
    state
        .main_token_treasury_balances
        .insert(MAIN_TOKEN_TREASURY_BUCKET_ECOSYSTEM_POOL.into(), u64::MAX);
    let world = World::new_with_state(state);
    natural(world, event, "overflow");
}

#[test]
fn success_preserves_material_precedence_routes_owner_and_matches_action_or_tick() {
    let world_key = MaterialLedgerId::world();
    let unrelated = MaterialLedgerId::site("claim-money-unrelated");
    for mode in 0..3 {
        for (base, event) in fixtures() {
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
            let baseline = World::new_with_state(state);
            let target_before = baseline.state().agents["bob"].clone();
            let owner_mail = baseline.state().agents["alice"].mailbox.len();
            let mut raw = baseline.clone();
            raw.append_event_for_test(WorldEventBody::Domain(event.clone()), None)
                .unwrap();
            assert_eq!(raw.state().agents["bob"], target_before);
            assert_eq!(raw.state().agents["alice"].mailbox.len(), owner_mail + 1);
            assert_eq!(
                raw.state().material_ledgers[&unrelated],
                baseline.state().material_ledgers[&unrelated]
            );
            assert!(raw.state().agents["alice"].last_active >= baseline.state().time);

            let replay = World::from_snapshot(baseline.snapshot(), raw.journal().clone()).unwrap();
            assert_eq!(replay.snapshot(), raw.snapshot());
            assert_eq!(
                replay.current_state_root_hash().unwrap(),
                raw.current_state_root_hash().unwrap()
            );
        }
    }

    let (base, claim) = claim_fixture();
    let mut action = base.clone();
    action.submit_action(Action::ClaimAgent {
        claimer_agent_id: "alice".into(),
        target_agent_id: "bob".into(),
    });
    action.step().unwrap();
    let mut raw_state = base.state().clone();
    raw_state.time = action.state().time;
    let mut raw = World::new_with_state(raw_state);
    raw.append_event_for_test(WorldEventBody::Domain(claim), None)
        .unwrap();
    assert_eq!(raw.state().agent_claims, action.state().agent_claims);
    assert_eq!(
        raw.state().main_token_balances,
        action.state().main_token_balances
    );
    assert_eq!(
        raw.state().main_token_treasury_balances,
        action.state().main_token_treasury_balances
    );
    assert_eq!(raw.main_token_supply(), action.main_token_supply());
    assert_eq!(
        raw.state().agent_claim_last_processed_epoch,
        action.state().agent_claim_last_processed_epoch
    );

    let (base, upkeep) = upkeep_fixture();
    let mut raw_state = base.state().clone();
    raw_state.time = raw_state.time.saturating_add(1);
    let mut raw = World::new_with_state(raw_state);
    raw.append_event_for_test(WorldEventBody::Domain(upkeep), None)
        .unwrap();
    let mut tick = base;
    tick.step().unwrap();
    assert_eq!(raw.state().agent_claims, tick.state().agent_claims);
    assert_eq!(
        raw.state().main_token_balances,
        tick.state().main_token_balances
    );
    assert_eq!(
        raw.state().main_token_treasury_balances,
        tick.state().main_token_treasury_balances
    );
    assert_eq!(raw.main_token_supply(), tick.main_token_supply());
    assert_eq!(
        raw.state().agent_claim_last_processed_epoch,
        tick.state().agent_claim_last_processed_epoch
    );
}
