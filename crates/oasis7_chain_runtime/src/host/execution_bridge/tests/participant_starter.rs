use super::*;
fn starter_request(
    root: &Path,
    identity: &WorldIdentity,
    agent: &GameParticipantReceiptV1,
    key: &str,
    op: &str,
) -> SubmitIntentRequest<WorldServicePayloadV1> {
    let current = pin(root, identity);
    let binding = current.world.current_cognition_runtime_binding().unwrap();
    request(
        identity,
        WorldServicePayloadV1::GameStarterClaim(
            sign_read_request(
                GAME_STARTER_CLAIM_DOMAIN,
                GameStarterClaimV1 {
                    api_rev: 1,
                    client_operation_id: op.into(),
                    world_id: identity.world_id.clone(),
                    genesis_digest: identity.genesis_digest.clone(),
                    branch_id: binding.branch_id,
                    reorg_epoch: DecimalU64(binding.reorg_epoch),
                    agent_id: agent.agent_id.clone(),
                    owner_binding: agent.owner_binding.clone(),
                    owner_generation: agent.owner_generation,
                },
                key,
            )
            .unwrap(),
        ),
    )
}
pub(super) fn recovered(
    client: &RemoteWorldServiceClient,
    original: &SubmitIntentRequest<WorldServicePayloadV1>,
) -> serde_json::Value {
    let deadline = Instant::now() + Duration::from_secs(40);
    while Instant::now() < deadline {
        if let Ok(response) = client.lookup(
            LookupIntentRequest {
                contract_version: 1,
                key: original.correlation.key.clone(),
            },
            original.signed_payload.clone(),
        ) && let IntentOutcome::Committed { receipt, .. } = response.outcome {
                return receipt;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!("starter canonical HTTP result did not settle");
}
pub(super) fn exercise(
    a: &RemoteWorldServiceClient,
    b: &RemoteWorldServiceClient,
    root: &Path,
    identity: &WorldIdentity,
    agent: &GameParticipantReceiptV1,
    key_a: &str,
    key_b: &str,
) -> SubmitIntentRequest<WorldServicePayloadV1> {
    let initial = pin(root, identity);
    assert_eq!(
        oasis7_runtime::service_admission::starter::eligibility(
            &initial.world,
            &agent.agent_id,
            &agent.owner_public_key
        )
        .unwrap()["status"],
        "eligible"
    );
    let before_supply = initial.world.main_token_supply().total_supply;
    let before_circulating = initial.world.main_token_supply().circulating_supply;
    let before_pool = initial
        .world
        .main_token_treasury_balance(oasis7_runtime::MAIN_TOKEN_TREASURY_BUCKET_ECOSYSTEM_POOL);
    let original = starter_request(root, identity, agent, key_a, "starter-a");
    a.submit(original.clone()).unwrap();
    let first = recovered(a, &original);
    let amount = first["claim"]["amount"].as_u64().unwrap();
    assert!(amount > 0);
    assert!(first["events"].as_array().is_some_and(|e| !e.is_empty()));
    let claimed = pin(root, identity);
    assert_eq!(
        claimed.world.main_token_supply().circulating_supply,
        before_circulating + amount
    );
    assert_eq!(
        claimed
            .world
            .main_token_treasury_balance(oasis7_runtime::MAIN_TOKEN_TREASURY_BUCKET_ECOSYSTEM_POOL),
        before_pool - amount
    );
    assert_eq!(
        first["claim"]["source_treasury_bucket_id"],
        oasis7_runtime::MAIN_TOKEN_TREASURY_BUCKET_ECOSYSTEM_POOL
    );
    assert_eq!(
        claimed.world.main_token_liquid_balance(&agent.agent_id),
        amount
    );
    assert_eq!(
        claimed.world.main_token_supply().total_supply,
        before_supply
    );
    assert_eq!(
        oasis7_runtime::service_admission::starter::eligibility(
            &claimed.world,
            &agent.agent_id,
            &agent.owner_public_key
        )
        .unwrap()["status"],
        "claimed"
    );
    a.submit(original.clone()).unwrap();
    assert_eq!(recovered(a, &original), first);
    let different = starter_request(root, identity, agent, key_a, "starter-already-claimed");
    a.submit(different.clone()).unwrap();
    assert_eq!(recovered(a, &different)["claim"], first["claim"]);
    assert!(b
        .submit(starter_request(
            root,
            identity,
            agent,
            key_b,
            "starter-wrong-owner"
        ))
        .is_err());
    let after = pin(root, identity);
    assert_eq!(after.world.main_token_supply().total_supply, before_supply);
    assert_eq!(after.world.journal().events.iter().filter(|e|matches!(&e.body,oasis7_runtime::WorldEventBody::Domain(oasis7_runtime::DomainEvent::StarterOcClaimed {agent_id,..}) if agent_id==&agent.agent_id)).count(),1);
    original
}

/// Explicit initial private test ledger, never a production funding or claim path.
pub(super) fn fund_fixture_reserve(world: &mut RuntimeWorld) {
    let reserve = world.state().starter_oc_claims["agent-a"].amount * 4;
    let mut supply = world.main_token_supply().clone();
    supply.total_supply += reserve;
    supply.total_issued += reserve;
    world.set_main_token_supply(supply);
    world
        .set_main_token_treasury_balance(
            oasis7_runtime::MAIN_TOKEN_TREASURY_BUCKET_ECOSYSTEM_POOL,
            reserve,
        )
        .unwrap();
}
