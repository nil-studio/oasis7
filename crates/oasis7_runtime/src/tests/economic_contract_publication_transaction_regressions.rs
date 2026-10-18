use super::pos;
use crate::*;
use oasis7_agent_api::world_action::ResourceKind;

fn register(world: &mut World, id: &str) {
    world.submit_action(Action::RegisterAgent {
        agent_id: id.into(),
        pos: pos(0, 0),
    });
    world.step().unwrap();
}

fn capture(world: &World, action: Action, wanted: fn(&DomainEvent) -> bool) -> DomainEvent {
    let mut next = world.clone();
    let len = next.journal().events.len();
    next.submit_action(action);
    next.step().unwrap();
    next.journal().events[len..]
        .iter()
        .find_map(|entry| match &entry.body {
            WorldEventBody::Domain(event) if wanted(event) => Some(event.clone()),
            _ => None,
        })
        .expect("captured contract event")
}

fn base() -> World {
    let mut world = World::new();
    register(&mut world, "a");
    register(&mut world, "b");
    world
        .set_agent_resource_balance("a", ResourceKind::Electricity, 1_000)
        .unwrap();
    world
}

fn fixtures() -> Vec<(World, DomainEvent)> {
    let mut world = base();
    let opened = capture(
        &world,
        Action::OpenEconomicContract {
            creator_agent_id: "a".into(),
            contract_id: "c".into(),
            counterparty_agent_id: "b".into(),
            fulfillment_kind: EconomicContractFulfillmentKind::AtomicExchange,
            settlement_kind: ResourceKind::Electricity,
            settlement_amount: 100,
            reputation_stake: 10,
            expires_at: 20,
            description: "trade".into(),
        },
        |e| matches!(e, DomainEvent::EconomicContractOpened { .. }),
    );
    let opened_base = world.clone();
    world
        .append_event_for_test(WorldEventBody::Domain(opened.clone()), None)
        .unwrap();
    let accepted = capture(
        &world,
        Action::AcceptEconomicContract {
            accepter_agent_id: "b".into(),
            contract_id: "c".into(),
        },
        |e| matches!(e, DomainEvent::EconomicContractAccepted { .. }),
    );
    let accepted_base = world.clone();
    world
        .append_event_for_test(WorldEventBody::Domain(accepted.clone()), None)
        .unwrap();
    let settled = capture(
        &world,
        Action::SettleEconomicContract {
            operator_agent_id: "a".into(),
            contract_id: "c".into(),
            success: true,
            notes: "ok".into(),
        },
        |e| matches!(e, DomainEvent::EconomicContractSettled { .. }),
    );

    let mut expiry = base();
    let expiry_open = capture(
        &expiry,
        Action::OpenEconomicContract {
            creator_agent_id: "a".into(),
            contract_id: "x".into(),
            counterparty_agent_id: "b".into(),
            fulfillment_kind: EconomicContractFulfillmentKind::AtomicExchange,
            settlement_kind: ResourceKind::Electricity,
            settlement_amount: 5,
            reputation_stake: 6,
            expires_at: 4,
            description: "expire".into(),
        },
        |e| matches!(e, DomainEvent::EconomicContractOpened { .. }),
    );
    expiry
        .append_event_for_test(WorldEventBody::Domain(expiry_open), None)
        .unwrap();
    let expired = expiry
        .prepared_economic_contract_expiry_events_for_test(4)
        .pop()
        .unwrap();
    vec![
        (opened_base, opened),
        (accepted_base, accepted),
        (world, settled),
        (expiry, expired),
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

fn natural(mut world: World, event: DomainEvent, expected: &str) {
    let before = world.clone();
    let root = world.current_state_root_hash().unwrap();
    let error = world
        .append_event_for_test(WorldEventBody::Domain(event), None)
        .expect_err("raw validation error");
    assert!(format!("{error:?}").contains(expected), "{error:?}");
    unchanged(&world, &before, &root);
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

#[test]
fn opened_is_atomic_retryable_and_replayable() {
    let (w, e) = fixtures().remove(0);
    atomic_retry(w, e, 5_300);
}
#[test]
fn accepted_is_atomic_retryable_and_replayable() {
    let (w, e) = fixtures().remove(1);
    atomic_retry(w, e, 5_301);
}
#[test]
fn settled_is_atomic_retryable_and_replayable() {
    let (w, e) = fixtures().remove(2);
    atomic_retry(w, e, 5_302);
}
#[test]
fn expired_is_atomic_retryable_and_replayable() {
    let (w, e) = fixtures().remove(3);
    atomic_retry(w, e, 5_303);
}

#[test]
fn accepted_missing_agent_late_failure_is_nonmutating() {
    let (world, accepted) = fixtures().remove(1);
    let mut state = world.state().clone();
    state.agents.remove("b");
    let mut changed = accepted.clone();
    if let DomainEvent::EconomicContractAccepted {
        accepter_agent_id, ..
    } = &mut changed
    {
        *accepter_agent_id = "b".into();
    }
    let mut broken = World::new_with_state(state);
    let before = broken.clone();
    let root = broken.current_state_root_hash().unwrap();
    let error = broken
        .append_event_for_test(WorldEventBody::Domain(changed), None)
        .unwrap_err();
    assert!(format!("{error:?}").contains("AgentNotFound"));
    unchanged(&broken, &before, &root);
}

#[test]
fn settlement_recipient_overflow_is_nonmutating() {
    let (world, settled) = fixtures().remove(2);
    let mut state = world.state().clone();
    state
        .agents
        .get_mut("b")
        .unwrap()
        .state
        .resources
        .set(ResourceKind::Electricity, i64::MAX)
        .unwrap();
    let mut broken = World::new_with_state(state);
    let before = broken.clone();
    let root = broken.current_state_root_hash().unwrap();
    let error = broken
        .append_event_for_test(WorldEventBody::Domain(settled), None)
        .unwrap_err();
    assert!(format!("{error:?}").contains("overflow"));
    unchanged(&broken, &before, &root);
}

#[test]
fn success_routes_exact_parties_preserves_materials_and_replays() {
    let key = MaterialLedgerId::world();
    for mode in 0..3 {
        for (base, event) in fixtures() {
            let mut state = base.state().clone();
            state.materials.insert("legacy".into(), 7);
            match mode {
                0 => {
                    state.material_ledgers.remove(&key);
                }
                1 => {
                    state
                        .material_ledgers
                        .insert(key.clone(), Default::default());
                }
                _ => {
                    state
                        .material_ledgers
                        .insert(key.clone(), [("canonical".into(), 9)].into());
                }
            }
            let baseline = World::new_with_state(state);
            let a = baseline.state().agents["a"].mailbox.len();
            let b = baseline.state().agents["b"].mailbox.len();
            let actor = event.agent_id().map(str::to_string);
            let mut raw = baseline.clone();
            raw.append_event_for_test(WorldEventBody::Domain(event), None)
                .unwrap();
            assert_eq!(
                raw.state().agents["a"].mailbox.len(),
                a + usize::from(actor.as_deref() == Some("a"))
            );
            assert_eq!(
                raw.state().agents["b"].mailbox.len(),
                b + usize::from(actor.as_deref() == Some("b"))
            );
            let replay = World::from_snapshot(baseline.snapshot(), raw.journal().clone()).unwrap();
            assert_eq!(replay.snapshot(), raw.snapshot());
            assert_eq!(
                replay.current_state_root_hash().unwrap(),
                raw.current_state_root_hash().unwrap()
            );
        }
    }
}

#[test]
fn raw_status_fulfillment_and_settlement_priority_table_is_exact() {
    let all = fixtures();
    let (opened_world, opened) = all[0].clone();
    let mut changed = opened.clone();
    if let DomainEvent::EconomicContractOpened {
        creator_agent_id, ..
    } = &mut changed
    {
        *creator_agent_id = "missing".into();
    }
    natural(opened_world.clone(), changed, "AgentNotFound");
    let mut changed = opened;
    if let DomainEvent::EconomicContractOpened {
        counterparty_agent_id,
        ..
    } = &mut changed
    {
        *counterparty_agent_id = "missing".into();
    }
    natural(opened_world, changed, "AgentNotFound");

    let (accepted_world, accepted) = all[1].clone();
    let mut changed = accepted.clone();
    if let DomainEvent::EconomicContractAccepted {
        accepter_agent_id, ..
    } = &mut changed
    {
        *accepter_agent_id = "a".into();
    }
    natural(accepted_world.clone(), changed, "accepter mismatch");
    let mut finalized_state = accepted_world.state().clone();
    finalized_state
        .economic_contracts
        .get_mut("c")
        .unwrap()
        .status = EconomicContractStatus::Settled;
    natural(
        World::new_with_state(finalized_state),
        accepted,
        "status invalid",
    );

    let (settled_world, settled) = all[2].clone();
    let mut changed = settled.clone();
    if let DomainEvent::EconomicContractSettled { success, .. } = &mut changed {
        *success = false;
    }
    natural(settled_world.clone(), changed, "requires success=true");
    let mut changed = settled.clone();
    if let DomainEvent::EconomicContractSettled {
        transfer_amount, ..
    } = &mut changed
    {
        *transfer_amount = 0;
    }
    natural(settled_world.clone(), changed, "must be > 0");
    let mut changed = settled.clone();
    if let DomainEvent::EconomicContractSettled { tax_amount, .. } = &mut changed {
        *tax_amount = -1;
    }
    natural(settled_world.clone(), changed, "must be >= 0");
    let mut changed = settled.clone();
    if let DomainEvent::EconomicContractSettled {
        transfer_amount,
        tax_amount,
        ..
    } = &mut changed
    {
        *transfer_amount = i64::MAX;
        *tax_amount = 1;
    }
    natural(settled_world.clone(), changed, "debit overflow");
    let mut changed = settled.clone();
    if let DomainEvent::EconomicContractSettled {
        transfer_amount, ..
    } = &mut changed
    {
        *transfer_amount = 2_000;
    }
    natural(settled_world.clone(), changed, "debit failed");
    let mut service_state = settled_world.state().clone();
    service_state
        .economic_contracts
        .get_mut("c")
        .unwrap()
        .fulfillment_kind = EconomicContractFulfillmentKind::Service;
    natural(
        World::new_with_state(service_state),
        settled.clone(),
        "service contracts unavailable",
    );
    let mut treasury_state = settled_world.state().clone();
    treasury_state
        .resources
        .insert(ResourceKind::Electricity, i64::MAX);
    natural(
        World::new_with_state(treasury_state),
        settled.clone(),
        "treasury overflow",
    );
    let mut reputation_state = settled_world.state().clone();
    reputation_state
        .reputation_scores
        .insert("a".into(), i64::MAX);
    natural(
        World::new_with_state(reputation_state),
        settled,
        "reputation overflow",
    );

    let (expired_world, expired) = all[3].clone();
    let mut final_state = expired_world.state().clone();
    final_state.economic_contracts.get_mut("x").unwrap().status = EconomicContractStatus::Expired;
    natural(
        World::new_with_state(final_state),
        expired.clone(),
        "already finalized",
    );
    let mut service_state = expired_world.state().clone();
    service_state
        .economic_contracts
        .get_mut("x")
        .unwrap()
        .fulfillment_kind = EconomicContractFulfillmentKind::Service;
    natural(
        World::new_with_state(service_state),
        expired,
        "service contracts unavailable",
    );
}

#[test]
fn action_and_raw_compound_projections_match_for_open_accept_and_settle() {
    let mut raw = base();
    let actions = [
        Action::OpenEconomicContract {
            creator_agent_id: "a".into(),
            contract_id: "c".into(),
            counterparty_agent_id: "b".into(),
            fulfillment_kind: EconomicContractFulfillmentKind::AtomicExchange,
            settlement_kind: ResourceKind::Electricity,
            settlement_amount: 100,
            reputation_stake: 10,
            expires_at: 20,
            description: "trade".into(),
        },
        Action::AcceptEconomicContract {
            accepter_agent_id: "b".into(),
            contract_id: "c".into(),
        },
        Action::SettleEconomicContract {
            operator_agent_id: "a".into(),
            contract_id: "c".into(),
            success: true,
            notes: "ok".into(),
        },
    ];
    for (index, action) in actions.into_iter().enumerate() {
        let mut action_world = raw.clone();
        let before = action_world.journal().events.len();
        action_world.submit_action(action);
        action_world.step().unwrap();
        let event = action_world.journal().events[before..]
            .iter()
            .find_map(|entry| match &entry.body {
                WorldEventBody::Domain(event)
                    if matches!(
                        event,
                        DomainEvent::EconomicContractOpened { .. }
                            | DomainEvent::EconomicContractAccepted { .. }
                            | DomainEvent::EconomicContractSettled { .. }
                    ) =>
                {
                    Some(event.clone())
                }
                _ => None,
            })
            .unwrap();
        let actor = event.agent_id().unwrap().to_string();
        let mut state = raw.state().clone();
        state.time = action_world.state().time;
        raw = World::new_with_state(state);
        raw.append_event_for_test(WorldEventBody::Domain(event), None)
            .unwrap();
        assert_eq!(
            raw.state().economic_contracts,
            action_world.state().economic_contracts
        );
        assert_eq!(raw.state().resources, action_world.state().resources);
        assert_eq!(
            raw.state().reputation_scores,
            action_world.state().reputation_scores
        );
        assert_eq!(
            raw.state().contract_pair_last_success_settled_at,
            action_world.state().contract_pair_last_success_settled_at
        );
        assert_eq!(
            raw.state().reputation_reward_window_accumulated,
            action_world.state().reputation_reward_window_accumulated
        );
        for id in ["a", "b"] {
            assert_eq!(
                action_world.state().agents[id].mailbox.len(),
                raw.state().agents[id].mailbox.len() + usize::from(actor == id),
                "{id} action-accepted mailbox delta step {index}"
            );
        }
    }

    let (mut tick, _) = fixtures().remove(3);
    let (prefix, expired, tick_after) = loop {
        let prefix = tick.clone();
        let len = tick.journal().events.len();
        tick.submit_action(Action::QueryObservation {
            agent_id: "a".into(),
        });
        tick.step().unwrap();
        if let Some(event) =
            tick.journal().events[len..]
                .iter()
                .find_map(|entry| match &entry.body {
                    WorldEventBody::Domain(event @ DomainEvent::EconomicContractExpired { .. }) => {
                        Some(event.clone())
                    }
                    _ => None,
                })
        {
            break (prefix, event, tick);
        }
    };
    let mut raw_state = prefix.state().clone();
    raw_state.time = tick_after.state().time;
    let mut raw = World::new_with_state(raw_state);
    raw.append_event_for_test(WorldEventBody::Domain(expired), None)
        .unwrap();
    assert_eq!(
        raw.state().economic_contracts,
        tick_after.state().economic_contracts
    );
    assert_eq!(
        raw.state().reputation_scores,
        tick_after.state().reputation_scores
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
        tick_after
            .tick_consensus_records()
            .last()
            .unwrap()
            .block
            .header
            .state_root,
        tick_after.current_state_root_hash().unwrap()
    );
}
