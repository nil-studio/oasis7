use super::*;

fn event_after_steps(mut world: World, wanted: fn(&DomainEvent) -> bool) -> (World, DomainEvent) {
    for _ in 0..40 {
        let before = world.clone();
        let len = world.journal().events.len();
        world.step().unwrap();
        if let Some(event) =
            world.journal().events[len..]
                .iter()
                .find_map(|entry| match &entry.body {
                    WorldEventBody::Domain(event) if wanted(event) => Some(event.clone()),
                    _ => None,
                })
        {
            return (before, event);
        }
    }
    panic!("terminal claim event not produced")
}

fn released(event: &DomainEvent) -> bool {
    matches!(event, DomainEvent::AgentClaimReleased { .. })
}

fn reclaimed(event: &DomainEvent) -> bool {
    matches!(event, DomainEvent::AgentClaimReclaimed { .. })
}

fn release_fixture() -> (World, DomainEvent) {
    let mut world = setup_claim_world(2_000, 0);
    world.submit_action(Action::ClaimAgent {
        claimer_agent_id: "alice".into(),
        target_agent_id: "bob".into(),
    });
    world.step().unwrap();
    world.submit_action(Action::ReleaseAgentClaim {
        claimer_agent_id: "alice".into(),
        target_agent_id: "bob".into(),
    });
    world.step().unwrap();
    let mut state = world.state().clone();
    state
        .agent_claims
        .get_mut("bob")
        .unwrap()
        .upkeep_paid_through_epoch = 100;
    world = World::new_with_state(state);
    world
        .set_governance_execution_policy(GovernanceExecutionPolicy {
            epoch_length_ticks: 1,
            ..GovernanceExecutionPolicy::default()
        })
        .unwrap();
    event_after_steps(world, released)
}

fn reclaim_fixture() -> (World, DomainEvent) {
    let mut world = setup_claim_world(325, 0);
    world.submit_action(Action::ClaimAgent {
        claimer_agent_id: "alice".into(),
        target_agent_id: "bob".into(),
    });
    world.step().unwrap();
    event_after_steps(world, reclaimed)
}

fn terminal_from(mut world: World, release: bool) -> (World, DomainEvent) {
    world.submit_action(Action::ClaimAgent {
        claimer_agent_id: "alice".into(),
        target_agent_id: "bob".into(),
    });
    world.step().unwrap();
    if release {
        world.submit_action(Action::ReleaseAgentClaim {
            claimer_agent_id: "alice".into(),
            target_agent_id: "bob".into(),
        });
        world.step().unwrap();
        let mut state = world.state().clone();
        state
            .agent_claims
            .get_mut("bob")
            .unwrap()
            .upkeep_paid_through_epoch = 100;
        world = World::new_with_state(state);
        world
            .set_governance_execution_policy(GovernanceExecutionPolicy {
                epoch_length_ticks: 1,
                ..GovernanceExecutionPolicy::default()
            })
            .unwrap();
        event_after_steps(world, released)
    } else {
        event_after_steps(world, reclaimed)
    }
}

fn source_release_fixture() -> (World, DomainEvent) {
    terminal_from(setup_claim_world_with_treasury(225, 100, 0), true)
}

fn source_reclaim_fixture() -> (World, DomainEvent) {
    terminal_from(setup_claim_world_with_treasury(225, 100, 0), false)
}

fn beneficiary_release_fixture() -> (World, DomainEvent) {
    terminal_from(setup_claim_world_with_balances(0, 325, 0), true)
}

fn zero_reclaim_fixture() -> (World, DomainEvent) {
    let (world, mut event) = reclaim_fixture();
    if let DomainEvent::AgentClaimReclaimed {
        upkeep_arrears_amount,
        collected_upkeep_amount,
        penalty_amount,
        refunded_bond_amount,
        refunded_bond_restricted_amount,
        refunded_bond_liquid_amount,
        ..
    } = &mut event
    {
        *upkeep_arrears_amount = 0;
        *collected_upkeep_amount = 0;
        *penalty_amount = 40;
        *refunded_bond_amount = 160;
        *refunded_bond_restricted_amount = 0;
        *refunded_bond_liquid_amount = 160;
    }
    (world, event)
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

fn atomic_retry(mut world: World, event: DomainEvent, id: u64) {
    let baseline = world.snapshot();
    let before = world.clone();
    let root = world.current_state_root_hash().unwrap();
    let len = world.journal().events.len();
    world.fail_next_append_after_publication_prepare_for_test();
    let error = world
        .append_event_for_test(
            WorldEventBody::Domain(event.clone()),
            Some(CausedBy::Action(id)),
        )
        .expect_err("postprepare failure");
    assert!(format!("{error:?}").contains("publication preparation"));
    unchanged(&world, &before, &root);
    let cause = Some(CausedBy::Action(id + 10));
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

fn natural(mut world: World, event: DomainEvent, expected: &str) {
    let before = world.clone();
    let root = world.current_state_root_hash().unwrap();
    let error = world
        .append_event_for_test(WorldEventBody::Domain(event), None)
        .expect_err("natural rejection");
    assert!(format!("{error:?}").contains(expected), "{error:?}");
    unchanged(&world, &before, &root);
}

#[test]
fn release_is_atomic_retryable_and_replayable() {
    let (world, event) = release_fixture();
    atomic_retry(world, event, 5_200);
}

#[test]
fn reclaim_is_atomic_retryable_and_replayable() {
    let (world, event) = reclaim_fixture();
    atomic_retry(world, event, 5_201);
}

#[test]
fn release_priorities_and_late_credit_failures_do_not_remove_claim() {
    let (world, event) = release_fixture();
    let mut changed = event.clone();
    if let DomainEvent::AgentClaimReleased {
        claimer_agent_id, ..
    } = &mut changed
    {
        *claimer_agent_id = "carol".into();
    }
    natural(world.clone(), changed, "owner mismatch");
    let mut changed = event.clone();
    if let DomainEvent::AgentClaimReleased {
        refunded_bond_amount,
        ..
    } = &mut changed
    {
        *refunded_bond_amount += 1;
    }
    natural(world.clone(), changed, "refund mismatch");
    let mut changed = event.clone();
    if let DomainEvent::AgentClaimReleased {
        refunded_bond_liquid_amount,
        ..
    } = &mut changed
    {
        *refunded_bond_liquid_amount = 0;
    }
    natural(world.clone(), changed, "refund provenance mismatch");
    let mut changed = event.clone();
    if let DomainEvent::AgentClaimReleased {
        refunded_bond_restricted_sink_bucket_id,
        ..
    } = &mut changed
    {
        *refunded_bond_restricted_sink_bucket_id = "wrong".into();
    }
    natural(world.clone(), changed, "refund sink mismatch");
    let mut state = world.state().clone();
    state
        .main_token_balances
        .get_mut("alice")
        .unwrap()
        .liquid_balance = u64::MAX;
    natural(World::new_with_state(state), event, "overflow");
}

#[test]
fn reclaim_priorities_and_late_treasury_failures_do_not_remove_claim() {
    let (world, event) = reclaim_fixture();
    let mut changed = event.clone();
    if let DomainEvent::AgentClaimReclaimed {
        claimer_agent_id, ..
    } = &mut changed
    {
        *claimer_agent_id = "carol".into();
    }
    natural(world.clone(), changed, "owner mismatch");
    let mut changed = event.clone();
    if let DomainEvent::AgentClaimReclaimed { reason, .. } = &mut changed {
        *reason = " raw reason ".into();
    }
    let mut permissive = world.clone();
    permissive
        .append_event_for_test(WorldEventBody::Domain(changed), None)
        .unwrap();
    let mut changed = event.clone();
    if let DomainEvent::AgentClaimReclaimed { penalty_amount, .. } = &mut changed {
        *penalty_amount += 1;
    }
    natural(world.clone(), changed, "settlement mismatch");
    let mut state = world.state().clone();
    state
        .main_token_treasury_balances
        .insert(MAIN_TOKEN_TREASURY_BUCKET_ECOSYSTEM_POOL.into(), u64::MAX);
    natural(World::new_with_state(state), event, "overflow");
}

#[test]
fn successes_route_owner_only_preserve_material_precedence_and_match_tick_projection() {
    let world_key = MaterialLedgerId::world();
    let unrelated = MaterialLedgerId::site("claim-terminal-unrelated");
    for mode in 0..3 {
        for (base, event) in [release_fixture(), reclaim_fixture()] {
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
            let mut raw = World::new_with_state(state);
            let target = raw.state().agents["bob"].clone();
            let owner_mail = raw.state().agents["alice"].mailbox.len();
            raw.append_event_for_test(WorldEventBody::Domain(event), None)
                .unwrap();
            assert_eq!(raw.state().agents["bob"], target);
            assert_eq!(raw.state().agents["alice"].mailbox.len(), owner_mail + 1);
            assert_eq!(raw.state().material_ledgers[&unrelated]["ore"], 9);
            assert!(raw.agent_claim("bob").is_none());
        }
    }
}

#[test]
fn restricted_refund_variants_zero_reclaim_and_material_replay_are_exact() {
    let world_key = MaterialLedgerId::world();
    let unrelated = MaterialLedgerId::site("claim-terminal-matrix");
    for (index, (base, event)) in [
        source_release_fixture(),
        source_reclaim_fixture(),
        beneficiary_release_fixture(),
        zero_reclaim_fixture(),
    ]
    .into_iter()
    .enumerate()
    {
        let mut state = base.state().clone();
        state.materials.insert("legacy".into(), 7);
        state
            .material_ledgers
            .insert(unrelated.clone(), [("ore".into(), 9)].into());
        match index % 3 {
            0 => {
                state.material_ledgers.remove(&world_key);
            }
            1 => {
                state
                    .material_ledgers
                    .insert(world_key.clone(), Default::default());
            }
            _ => {
                state
                    .material_ledgers
                    .insert(world_key.clone(), [("canonical".into(), 11)].into());
            }
        }
        let baseline = World::new_with_state(state);
        let owner_before = baseline.state().agents["alice"].clone();
        let target_before = baseline.state().agents["bob"].clone();
        let mut raw = baseline.clone();
        raw.append_event_for_test(WorldEventBody::Domain(event.clone()), None)
            .unwrap();
        assert!(raw.agent_claim("bob").is_none());
        assert_eq!(raw.state().agents["bob"], target_before);
        assert_eq!(
            raw.state().agents["alice"].mailbox.len(),
            owner_before.mailbox.len() + 1
        );
        assert!(raw.state().agents["alice"].last_active >= baseline.state().time);
        assert_eq!(raw.state().material_ledgers[&unrelated]["ore"], 9);
        match event {
            DomainEvent::AgentClaimReleased {
                refunded_bond_restricted_sink,
                refunded_bond_restricted_amount,
                refunded_bond_liquid_amount,
                released_at_epoch,
                ..
            } => {
                assert!(refunded_bond_restricted_amount + refunded_bond_liquid_amount > 0);
                if index == 0 {
                    assert_eq!(
                        refunded_bond_restricted_sink,
                        RestrictedStarterClaimRefundSink::SourceTreasuryBucket
                    );
                }
                if index == 2 {
                    assert_eq!(
                        refunded_bond_restricted_sink,
                        RestrictedStarterClaimRefundSink::BeneficiaryRestrictedBalance
                    );
                    assert!(refunded_bond_restricted_amount > 0);
                }
                assert!(raw.state().agent_claim_last_processed_epoch >= released_at_epoch);
            }
            DomainEvent::AgentClaimReclaimed {
                collected_upkeep_amount,
                penalty_amount,
                refunded_bond_amount,
                reclaimed_at_epoch,
                ..
            } => {
                if index == 3 {
                    assert_eq!(collected_upkeep_amount, 0);
                }
                assert_eq!(
                    collected_upkeep_amount + penalty_amount + refunded_bond_amount,
                    200
                );
                assert!(raw.state().agent_claim_last_processed_epoch >= reclaimed_at_epoch);
            }
            _ => unreachable!(),
        }
        let replay = World::from_snapshot(baseline.snapshot(), raw.journal().clone()).unwrap();
        assert_eq!(replay.snapshot(), raw.snapshot());
        assert_eq!(
            replay.current_state_root_hash().unwrap(),
            raw.current_state_root_hash().unwrap()
        );
    }
}

#[test]
fn all_late_refund_treasury_and_circulating_failures_are_nonmutating() {
    let (world, event) = source_release_fixture();
    let mut state = world.state().clone();
    state.main_token_treasury_balances.insert(
        MAIN_TOKEN_TREASURY_BUCKET_RESTRICTED_STARTER_CLAIM_LIVEOPS_POOL.into(),
        u64::MAX,
    );
    natural(World::new_with_state(state), event, "overflow");

    let (world, event) = beneficiary_release_fixture();
    let mut state = world.state().clone();
    state
        .main_token_balances
        .get_mut("alice")
        .unwrap()
        .restricted_starter_claim_balance = u64::MAX;
    natural(World::new_with_state(state), event.clone(), "overflow");
    let mut state = world.state().clone();
    state.main_token_supply.circulating_supply = u64::MAX;
    natural(World::new_with_state(state), event, "circulating");

    let (world, event) = reclaim_fixture();
    let mut state = world.state().clone();
    state
        .main_token_treasury_balances
        .insert(MAIN_TOKEN_TREASURY_BUCKET_SLASH.into(), u64::MAX);
    natural(World::new_with_state(state), event, "overflow");

    let (world, event) = source_reclaim_fixture();
    let mut state = world.state().clone();
    state.main_token_treasury_balances.insert(
        MAIN_TOKEN_TREASURY_BUCKET_RESTRICTED_STARTER_CLAIM_LIVEOPS_POOL.into(),
        u64::MAX,
    );
    natural(World::new_with_state(state), event, "overflow");
}

#[test]
fn harvested_tick_and_raw_terminal_projections_match() {
    for (base, event) in [release_fixture(), reclaim_fixture()] {
        let mut raw_state = base.state().clone();
        raw_state.time = raw_state.time.saturating_add(1);
        let mut raw = World::new_with_state(raw_state);
        raw.append_event_for_test(WorldEventBody::Domain(event), None)
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
        assert_eq!(
            raw.tick_consensus_records()
                .last()
                .unwrap()
                .block
                .header
                .state_root,
            raw.current_state_root_hash().unwrap()
        );
        assert_eq!(
            tick.tick_consensus_records()
                .last()
                .unwrap()
                .block
                .header
                .state_root,
            tick.current_state_root_hash().unwrap()
        );
    }
}
