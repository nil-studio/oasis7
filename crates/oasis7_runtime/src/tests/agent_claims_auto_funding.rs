use super::super::*;
use super::pos;

fn register_agent(world: &mut World, agent_id: &str) {
    world.submit_action(Action::RegisterAgent {
        agent_id: agent_id.to_string(),
        pos: pos(0, 0),
    });
    world.step().expect("register agent");
}

fn setup_claim_world_with_treasury(
    treasury_balance: u64,
    alice_liquid_balance: u64,
    reputation_score: i64,
) -> World {
    let mut world = World::new();
    world
        .set_governance_execution_policy(GovernanceExecutionPolicy {
            epoch_length_ticks: 1,
            ..GovernanceExecutionPolicy::default()
        })
        .expect("set governance epoch length");
    register_agent(&mut world, "alice");
    register_agent(&mut world, "bob");
    register_agent(&mut world, "carol");
    world
        .set_agent_reputation_score("alice", reputation_score)
        .expect("set alice reputation");
    world.set_main_token_supply(MainTokenSupplyState {
        total_supply: treasury_balance.saturating_add(alice_liquid_balance),
        circulating_supply: alice_liquid_balance,
        ..MainTokenSupplyState::default()
    });
    world
        .set_main_token_treasury_balance(
            MAIN_TOKEN_TREASURY_BUCKET_RESTRICTED_STARTER_CLAIM_LIVEOPS_POOL,
            treasury_balance,
        )
        .expect("seed restricted liveops treasury");
    if alice_liquid_balance > 0 {
        world
            .set_main_token_account_balance("alice", alice_liquid_balance, 0)
            .expect("seed alice liquid balance");
    }
    world
}

#[test]
fn first_slot_claim_auto_funds_shortfall_from_dedicated_pool_without_approval() {
    let mut world = setup_claim_world_with_treasury(225, 100, 0);

    world.submit_action(Action::ClaimAgent {
        claimer_agent_id: "alice".to_string(),
        target_agent_id: "bob".to_string(),
    });
    world.step().expect("auto-fund and claim slot 1");

    let claim = world.agent_claim("bob").expect("claim exists");
    assert_eq!(claim.claim_owner_id, "alice");
    assert_eq!(claim.slot_index, 1);
    assert_eq!(claim.upfront_restricted_spent_amount, 225);
    assert_eq!(claim.upfront_liquid_spent_amount, 100);
    assert_eq!(claim.claim_bond_locked_restricted_amount, 125);
    assert_eq!(
        claim
            .claim_bond_restricted_source_treasury_bucket_id
            .as_deref(),
        Some(MAIN_TOKEN_TREASURY_BUCKET_RESTRICTED_STARTER_CLAIM_LIVEOPS_POOL)
    );
    assert_eq!(
        world.main_token_restricted_starter_claim_balance("alice"),
        0
    );
    assert_eq!(
        world.main_token_treasury_balance(
            MAIN_TOKEN_TREASURY_BUCKET_RESTRICTED_STARTER_CLAIM_LIVEOPS_POOL
        ),
        0
    );
    assert!(world.restricted_starter_claim_grant("alice").is_none());

    match &world.journal().events.last().expect("event").body {
        WorldEventBody::Domain(DomainEvent::AgentClaimed {
            auto_issued_restricted_amount,
            auto_issued_restricted_source_treasury_bucket_id,
            ..
        }) => {
            assert_eq!(*auto_issued_restricted_amount, 225);
            assert_eq!(
                auto_issued_restricted_source_treasury_bucket_id.as_deref(),
                Some(MAIN_TOKEN_TREASURY_BUCKET_RESTRICTED_STARTER_CLAIM_LIVEOPS_POOL)
            );
        }
        other => panic!("expected AgentClaimed, got {other:?}"),
    }
}

#[test]
fn auto_funded_restricted_bond_refund_returns_to_dedicated_pool_on_release() {
    let mut world = setup_claim_world_with_treasury(225, 100, 0);

    world.submit_action(Action::ClaimAgent {
        claimer_agent_id: "alice".to_string(),
        target_agent_id: "bob".to_string(),
    });
    world.step().expect("auto-fund and claim slot 1");
    let claim = world.agent_claim("bob").expect("claim exists").clone();
    let upkeep_topup = claim
        .upkeep_per_epoch
        .saturating_mul(claim.release_cooldown_epochs.saturating_add(1));
    let mut supply = world.main_token_supply().clone();
    supply.total_supply = supply.total_supply.saturating_add(upkeep_topup);
    supply.circulating_supply = supply.circulating_supply.saturating_add(upkeep_topup);
    world.set_main_token_supply(supply);
    world
        .set_main_token_account_balance("alice", upkeep_topup, 0)
        .expect("top up alice for release upkeep");

    world.submit_action(Action::ReleaseAgentClaim {
        claimer_agent_id: "alice".to_string(),
        target_agent_id: "bob".to_string(),
    });
    world.step().expect("request release");
    for _ in 0..claim.release_cooldown_epochs.saturating_sub(1) {
        world.step().expect("advance release cooldown");
    }
    world.step().expect("finalize release");

    let released_event = world
        .journal()
        .events
        .iter()
        .rev()
        .find(|event| {
            matches!(
                event.body,
                WorldEventBody::Domain(DomainEvent::AgentClaimReleased { .. })
            )
        })
        .expect("missing AgentClaimReleased event");

    match &released_event.body {
        WorldEventBody::Domain(DomainEvent::AgentClaimReleased {
            refunded_bond_restricted_amount,
            refunded_bond_restricted_sink,
            refunded_bond_restricted_sink_bucket_id,
            ..
        }) => {
            assert_eq!(*refunded_bond_restricted_amount, 125);
            assert_eq!(
                *refunded_bond_restricted_sink,
                RestrictedStarterClaimRefundSink::SourceTreasuryBucket
            );
            assert_eq!(
                refunded_bond_restricted_sink_bucket_id,
                MAIN_TOKEN_TREASURY_BUCKET_RESTRICTED_STARTER_CLAIM_LIVEOPS_POOL
            );
        }
        other => panic!("expected AgentClaimReleased, got {other:?}"),
    }
    assert_eq!(
        world.main_token_restricted_starter_claim_balance("alice"),
        0
    );
    assert_eq!(
        world.main_token_treasury_balance(
            MAIN_TOKEN_TREASURY_BUCKET_RESTRICTED_STARTER_CLAIM_LIVEOPS_POOL
        ),
        125
    );
}
