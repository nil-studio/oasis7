//! Legacy registration has no capability identity. Preserve its real effect and
//! cold recovery, while requiring private reads to fail closed. The governed
//! original scenario separately proves authorized immutable private Lookup.
use super::*;

pub(super) fn submit_after_recovery(
    config: &Path,
    owner: WorldServiceClientConfig,
    world: &WorldIdentity,
    original: &SubmitIntentRequest<WorldServicePayloadV1>,
    original_ref: &oasis7_client_api::world_service::CommitRef,
) -> Result<(), String> {
    let key = fixtures::key(71);
    let public_key = hex::encode(key.verifying_key().to_bytes());
    let mut action = oasis7_viewer_runtime::viewer::GameplayActionRequest {
        action_id: oasis7_viewer_runtime::viewer::ACTION_CLAIM_STARTER_OC.into(),
        target_agent_id: oasis7_viewer_runtime::viewer::FIRST_AGENT_CLAIM_TARGET_AGENT_ID.into(),
        actor_agent_id: None,
        player_id: "owner".into(),
        public_key: Some(public_key.clone()),
        auth: None,
    };
    action.auth = Some(
        oasis7_viewer_runtime::viewer::sign_gameplay_action_auth_proof(
            &action,
            2,
            &public_key,
            &hex::encode(key.to_bytes()),
        )?,
    );
    let payload = WorldServicePayloadV1::GameplayJson(
        serde_json::to_vec(&action).map_err(|error| error.to_string())?,
    );
    let request = SubmitIntentRequest {
        contract_version: WORLD_SERVICE_CONTRACT_VERSION,
        correlation: correlation::derive_correlation(world.clone(), &payload)
            .map_err(|error| error.to_string())?,
        deadline_unix_ms: None,
        signed_payload: payload,
    };
    assert_ne!(request.correlation.key, original.correlation.key);
    let mut runtime = FormalLiveRuntime::open_with_native_history(config)?;
    let before = runtime.published.current()?;
    let epoch = before.anchor.head().record().original_epoch;
    if epoch < 2 {
        return Err("survivor has not entered the successor epoch".into());
    }
    if before
        .world
        .state()
        .starter_oc_claims
        .contains_key(&action.target_agent_id)
    {
        return Err("survivor next request was already applied before submission".into());
    }
    runtime.start()?;
    // The bootstrap receipt belongs to the original public reader. The next
    // action has an Agent-private result and must use its exact owner scope.
    let original_client = RemoteWorldServiceClient::new(owner.clone()).map_err(|e| e.to_string())?;
    let mut next_owner = owner.clone();
    next_owner.scope_id = format!("agent:{}", action.target_agent_id);
    let client = RemoteWorldServiceClient::new(next_owner.clone()).map_err(|e| e.to_string())?;
    assert_volatile_admission(
        client.submit(request.clone()).map_err(|e| e.to_string())?,
        &request.correlation,
    )?;
    let result = wait_for_new_request_result(
        &runtime,
        &request,
        Duration::from_secs(120),
        "survivor next request",
    )?;
    if result.request != request || result.rejected.is_some() {
        return Err(format!(
            "survivor next request did not succeed: {:?}",
            result.rejected
        ));
    }
    let public_lookup_error = original_client
        .lookup_formal_original(request.clone())
        .err()
        .ok_or("survivor next private result was exposed through public scope")?;
    if !public_lookup_error
        .to_string()
        .contains("formal result private Agent scope mismatch")
    {
        return Err(format!(
            "unexpected public-scope rejection: {public_lookup_error}"
        ));
    }
    assert_identity_unavailable(&client, &request)?;
    let cut = runtime.published.current()?;
    if cut.world.current_agent_identity(&action.target_agent_id).is_some() {
        return Err("legacy RegisterAgent unexpectedly minted an identity".into());
    }
    // This is a verified observation cut, not a fabricated original CommitRef.
    let committed = fixtures::commit_ref(&cut.anchor, &fixtures::projection(&cut.world));
    if committed.position <= original_ref.position
        || lookup_original_commit(&original_client, original)? != *original_ref
    {
        return Err("survivor next request replaced or preceded the original result".into());
    }
    let publication = runtime
        .history
        .fixed_publication(&committed)
        .map_err(|error| error.to_string())?;
    let claim = publication
        .world()
        .state()
        .starter_oc_claims
        .get(&action.target_agent_id)
        .ok_or("survivor next owner claim missing")?
        .clone();
    if claim.player_id != "owner"
        || claim.public_key.as_deref() != Some(public_key.as_str())
        || claim.amount != 100_000_000
        || publication.world().main_token_liquid_balance(&action.target_agent_id) != claim.amount
        || publication.anchor().head().record().original_epoch != epoch
    {
        return Err("survivor next request changed owner, amount, or successor epoch".into());
    }
    stop_owned_protected_runtime(
        &mut runtime,
        Duration::from_secs(120),
        "survivor_next_request",
    )?;
    drop(runtime);
    let mut runtime = FormalLiveRuntime::open_with_native_history(config)?;
    runtime.start()?;
    let original_client = RemoteWorldServiceClient::new(owner).map_err(|e| e.to_string())?;
    let client = RemoteWorldServiceClient::new(next_owner).map_err(|e| e.to_string())?;
    assert_identity_unavailable(&client, &request)?;
    let reopened = runtime.published.current()?;
    let result_key = correlation::key_digest(&request.correlation.key).map_err(|e| e.to_string())?;
    if retained_result_from_hot_or_cold(&reopened.world, &result_key)? != result
        || reopened.world.current_agent_identity(&action.target_agent_id).is_some()
        || reopened.world.main_token_liquid_balance(&action.target_agent_id) != claim.amount
        || lookup_original_commit(&original_client, original)? != *original_ref
        || runtime
            .published
            .current()?
            .world
            .state()
            .starter_oc_claims
            .get(&action.target_agent_id)
            != Some(&claim)
    {
        return Err("survivor cold reopen changed new or original committed result".into());
    }
    stop_owned_protected_runtime(
        &mut runtime,
        Duration::from_secs(120),
        "survivor_next_reopen",
    )
}

fn assert_identity_unavailable(client: &RemoteWorldServiceClient, request: &SubmitIntentRequest<WorldServicePayloadV1>) -> Result<(), String> {
    let error = client.lookup_formal_original(request.clone()).err().ok_or("legacy private result leaked without a durable identity")?;
    if !error.to_string().contains("Agent identity unavailable") { return Err(format!("unexpected legacy private rejection: {error}")); }
    Ok(())
}
