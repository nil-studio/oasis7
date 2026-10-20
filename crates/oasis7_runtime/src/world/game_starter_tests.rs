use super::participant_tests as fixture;
use crate::service_admission::{sign_read_request, starter};
use oasis7_agent_api::game::*;
fn claim(
    agent: &GameParticipantReceiptV1,
    key: &str,
    op: &str,
) -> crate::service_admission::SignedReadRequest<GameStarterClaimV1> {
    sign_read_request(
        GAME_STARTER_CLAIM_DOMAIN,
        GameStarterClaimV1 {
            api_rev: 1,
            client_operation_id: op.into(),
            world_id: agent.world_id.clone(),
            genesis_digest: agent.genesis_digest.clone(),
            branch_id: "main".into(),
            reorg_epoch: DecimalU64(0),
            agent_id: agent.agent_id.clone(),
            owner_binding: agent.owner_binding.clone(),
            owner_generation: agent.owner_generation,
        },
        key,
    )
    .unwrap()
}
#[test]
fn game_starter_independent_actual_gift_once_owner_checks_and_replay() {
    let mut world = fixture::fixture();
    world
        .install_private_participant_fixture_policy(fixture::policy())
        .unwrap();
    let (r, a) = fixture::request("account", "register");
    let receipt = world
        .apply_participant_registration(&r, &a, "main", 0)
        .unwrap();
    assert!(world.state().starter_oc_claims.is_empty());
    assert_eq!(world.main_token_liquid_balance(&receipt.agent_id), 0);
    assert_eq!(
        starter::eligibility(&world, &receipt.agent_id, &r.subject_public_key).unwrap()["status"],
        "eligible"
    );
    let checkpoint = world.snapshot();
    let key = hex::encode([72; 32]);
    let first = claim(&receipt, &key, "starter");
    let (result, action) = world.apply_game_starter_claim(&first, "main", 0).unwrap();
    assert!(action.is_some());
    let amount = result["claim"]["amount"].as_u64().unwrap();
    assert!(amount > 0);
    assert_eq!(world.main_token_liquid_balance(&receipt.agent_id), amount);
    assert_eq!(
        starter::eligibility(&world, &receipt.agent_id, &r.subject_public_key).unwrap()["status"],
        "claimed"
    );
    let supply = world.main_token_supply().clone();
    let count = world.journal().events.len();
    assert_eq!(
        world.apply_game_starter_claim(&first, "main", 0).unwrap(),
        (result.clone(), None)
    );
    assert_eq!(
        world
            .apply_game_starter_claim(&claim(&receipt, &key, "another-op"), "main", 0)
            .unwrap(),
        (result, None)
    );
    assert_eq!(world.main_token_supply(), &supply);
    assert_eq!(world.journal().events.len(), count);
    assert!(
        world
            .apply_game_starter_claim(
                &claim(&receipt, &hex::encode([73; 32]), "wrong-owner"),
                "main",
                0
            )
            .is_err()
    );
    assert!(world.apply_game_starter_claim(&first, "stale", 0).is_err());
    let replayed = crate::World::from_snapshot(checkpoint, world.journal().clone()).unwrap();
    assert_eq!(
        replayed.state().starter_oc_claims,
        world.state().starter_oc_claims
    );
    assert_eq!(
        replayed.main_token_liquid_balance(&receipt.agent_id),
        amount
    );
    assert_eq!(replayed.main_token_supply(), &supply);
}
#[test]
fn game_starter_eligibility_and_submission_preserve_existing_positive_balance_blocker() {
    let mut world = fixture::fixture();
    world
        .install_private_participant_fixture_policy(fixture::policy())
        .unwrap();
    let (r, a) = fixture::request("account", "register");
    let receipt = world
        .apply_participant_registration(&r, &a, "main", 0)
        .unwrap();
    world
        .set_main_token_account_balance(&receipt.agent_id, 1, 0)
        .unwrap();
    assert_eq!(
        starter::eligibility(&world, &receipt.agent_id, &r.subject_public_key).unwrap()["status"],
        "blocked"
    );
    let before = world.snapshot();
    assert!(
        world
            .apply_game_starter_claim(
                &claim(&receipt, &hex::encode([72; 32]), "starter"),
                "main",
                0
            )
            .is_err()
    );
    assert_eq!(world.snapshot(), before);
    assert!(world.state().starter_oc_claims.is_empty());
}
