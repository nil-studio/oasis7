use super::super::*;
use super::pos;

fn register_agent(world: &mut World, agent_id: &str) {
    world.submit_action(Action::RegisterAgent {
        agent_id: agent_id.to_string(),
        pos: pos(0, 0),
    });
    world.step().expect("register agent");
}

#[test]
fn agent_claim_epoch_prepares_one_event_then_observes_prior_claim_debit() {
    let mut world = World::new();
    world
        .set_governance_execution_policy(GovernanceExecutionPolicy {
            epoch_length_ticks: 1,
            ..GovernanceExecutionPolicy::default()
        })
        .expect("set one-tick claim epochs");
    for agent_id in ["alice", "bob", "carol"] {
        register_agent(&mut world, agent_id);
    }
    world
        .set_agent_reputation_score("alice", 10)
        .expect("allow two claim slots");
    world.set_main_token_supply(MainTokenSupplyState {
        total_supply: 10_000,
        circulating_supply: 10_000,
        ..MainTokenSupplyState::default()
    });
    world
        .set_main_token_account_balance("alice", 10_000, 0)
        .expect("seed claim owner balance");

    for target_agent_id in ["bob", "carol"] {
        world.submit_action(Action::ClaimAgent {
            claimer_agent_id: "alice".to_string(),
            target_agent_id: target_agent_id.to_string(),
        });
        world.step().expect("claim target agent");
    }
    let bob_claim = world.agent_claim("bob").expect("bob claim");
    let next_epoch = world.state().time.saturating_add(1);
    let bob_amount_due = bob_claim
        .upkeep_per_epoch
        .saturating_mul(next_epoch.saturating_sub(bob_claim.upkeep_paid_through_epoch));
    world
        .set_main_token_account_balance("alice", bob_amount_due, 0)
        .expect("leave funds for only the first ordered claim");

    let snapshot_before_prepare = world.snapshot();
    let journal_before_prepare = world.journal().clone();
    let prepared = world
        .prepared_next_agent_claim_event_for_test("bob", world.state().time.saturating_add(1))
        .expect("prepare bob claim event")
        .expect("bob has a due claim event");
    assert_eq!(world.snapshot(), snapshot_before_prepare);
    assert_eq!(world.journal(), &journal_before_prepare);
    assert!(
        matches!(
            &prepared,
            DomainEvent::AgentClaimUpkeepSettled { target_agent_id, .. }
                if target_agent_id == "bob"
        ),
        "unexpected prepared bob event: {prepared:#?}"
    );

    let before = world.journal().events.len();
    world.step().expect("process ordered claim epoch");
    let claim_events = world.journal().events[before..]
        .iter()
        .filter_map(|event| match &event.body {
            WorldEventBody::Domain(event @ DomainEvent::AgentClaimUpkeepSettled { .. })
            | WorldEventBody::Domain(event @ DomainEvent::AgentClaimEnteredGrace { .. }) => {
                Some(event)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(
        matches!(
            claim_events.as_slice(),
            [
                DomainEvent::AgentClaimUpkeepSettled { target_agent_id: first, .. },
                DomainEvent::AgentClaimEnteredGrace { target_agent_id: second, .. }
            ] if first == "bob" && second == "carol"
        ),
        "unexpected ordered claim events: {claim_events:#?}"
    );
}
