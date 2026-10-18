use super::*;

#[test]
fn first_agent_claim_is_non_free_and_locks_bond() {
    let mut world = setup_claim_world(1_000, 0);

    world.submit_action(Action::ClaimAgent {
        claimer_agent_id: "alice".to_string(),
        target_agent_id: "bob".to_string(),
    });
    world.step().expect("claim first agent");

    let claim = world.agent_claim("bob").expect("claim persisted");
    assert_eq!(claim.claim_owner_id, "alice");
    assert_eq!(claim.slot_index, 1);
    assert_eq!(claim.reputation_tier, 0);
    assert!(claim.activation_fee_amount > 0);
    assert!(claim.claim_bond_amount > 0);
    assert!(claim.upkeep_per_epoch > 0);
    assert_eq!(claim.locked_bond_amount, claim.claim_bond_amount);
    let upfront_amount = claim_upfront_amount(claim);
    assert_eq!(
        world.main_token_liquid_balance("alice"),
        1_000 - upfront_amount
    );
    assert_eq!(
        world.main_token_treasury_balance("ecosystem_pool"),
        claim.activation_fee_treasury_amount + claim.upkeep_per_epoch
    );
    assert_eq!(
        world.main_token_supply().total_supply,
        1_000 - claim.activation_fee_burn_amount
    );
    assert_eq!(
        world.main_token_supply().circulating_supply,
        1_000 - upfront_amount
    );
}
