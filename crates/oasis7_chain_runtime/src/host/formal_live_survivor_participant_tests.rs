//! Governed registration across R-only Frozen/Prepared recovery, or after it.
//! Custody and governance keys are isolated test authorities, not production qualification.
use super::*;
use oasis7_agent_api::game::*;
use oasis7_runtime::service_admission::participant::business_digest;
#[path = "formal_live_survivor_participant_policy.rs"]
mod policy;

pub(crate) fn install_genesis_policy(
    world: &mut oasis7_runtime::World,
    identity: &FormalWorldIdentityV1,
) -> Result<(), String> {
    if world.state().time != 0 || !world.state().agents.is_empty() {
        return Err("participant policy fixture requires an empty genesis".into());
    }
    policy::install_for_identity(
        world,
        &WorldIdentity { world_id: identity.world_id.clone(), genesis_digest: identity.genesis_digest.clone() },
        1,
        true,
        GameIdentityNamespaceV1 { service_id: "survivor-test-issuer".into(), environment_id: "isolated-test".into(), instance_epoch: DecimalU64(1) },
        73,
    );
    if world.state().time != 0 || !world.state().agents.is_empty()
        || !world.capability_revocation_state().agent_identities.is_empty()
    {
        return Err("policy installation synthesized an Agent identity".into());
    }
    Ok(())
}

pub(crate) fn update_fixture_policy(
    world: &mut oasis7_runtime::World,
    identity: &WorldIdentity,
    generation: u64,
    enabled: bool,
    namespace: GameIdentityNamespaceV1,
    issuer_seed: u8,
) -> Result<(), String> {
    policy::install_for_identity(world, identity, generation, enabled, namespace, issuer_seed);
    Ok(())
}

// Called by the existing trusted survivor test, not a new ignored test mapping.
pub(super) fn run_recovery_scenario(phase: &str, registered_original: bool) -> Result<(), String> {
    run_protected_runtime_fixture(
        true,
        Some(phase),
        false,
        None,
        None,
        None,
        if registered_original { FixtureCase::RegisteredSurvivor } else { FixtureCase::GovernedParticipant },
    )
}

fn request(world: &WorldIdentity, payload: WorldServicePayloadV1) -> Result<SubmitIntentRequest<WorldServicePayloadV1>, String> {
    Ok(SubmitIntentRequest {
        contract_version: WORLD_SERVICE_CONTRACT_VERSION,
        correlation: correlation::derive_correlation(world.clone(), &payload).map_err(|e| e.to_string())?,
        deadline_unix_ms: None,
        signed_payload: payload,
    })
}

pub(crate) fn registration_request(state: &oasis7_runtime::World, world: &WorldIdentity) -> Result<SubmitIntentRequest<WorldServicePayloadV1>, String> {
    let policy = state.capability_revocation_state().participants.policy.as_ref().ok_or("participant policy missing")?;
    let key = fixtures::key(72);
    let secret = hex::encode(key.to_bytes());
    let public_key = hex::encode(key.verifying_key().to_bytes());
    let binding = state.current_cognition_runtime_binding().map_err(|e| format!("{e:?}"))?;
    let registration = GameParticipantRegisterV1 {
        api_rev: 1, client_operation_id: "survivor-register".into(), identity_namespace: policy.identity_namespace.clone(),
        account_id: "survivor-account".into(), world_id: world.world_id.clone(), genesis_digest: world.genesis_digest.clone(),
        owner_public_key: public_key.clone(), owner_key_version: DecimalU64(1),
    };
    let tick = state.state().time;
    let admission = GameParticipantAdmissionV1 {
        api_rev: 1, business_digest: business_digest(&registration)?, identity_namespace: policy.identity_namespace.clone(),
        account_id: registration.account_id.clone(), world_id: world.world_id.clone(), genesis_digest: world.genesis_digest.clone(),
        owner_public_key: public_key.clone(), branch_id: binding.branch_id.clone(), reorg_epoch: DecimalU64(binding.reorg_epoch),
        policy_generation: DecimalU64(policy.generation), valid_from_tick: DecimalU64(tick), valid_until_tick: DecimalU64(tick.checked_add(128).ok_or("admission tick overflow")?),
    };
    request(world, WorldServicePayloadV1::GameParticipantRegister {
        owner: sign_read_request(GAME_PARTICIPANT_REGISTER_DOMAIN, registration, &secret)?,
        admission: sign_read_request(GAME_PARTICIPANT_ADMISSION_DOMAIN, admission, &hex::encode(fixtures::key(73).to_bytes()))?,
    })
}

pub(super) fn submit_after_recovery(
    config: &Path,
    owner: WorldServiceClientConfig,
    world: &WorldIdentity,
    original: &SubmitIntentRequest<WorldServicePayloadV1>,
    original_ref: &oasis7_client_api::world_service::CommitRef,
) -> Result<(), String> {
    let mut runtime = FormalLiveRuntime::open_with_native_history(config)?;
    let before = runtime.published.current()?;
    let epoch = before.anchor.head().record().original_epoch;
    if epoch < 2 { return Err("participant fixture did not recover to successor epoch".into()); }
    let registry = &before.world.capability_revocation_state().participants;
    registry.policy.as_ref().ok_or("governed participant policy missing")?;
    let registered_original = matches!(&original.signed_payload, WorldServicePayloadV1::GameParticipantRegister { .. });
    if registry.policy_history.is_empty() || !registry.devlocal_policy_history.is_empty() || (!registered_original && !registry.bindings.is_empty()) {
        return Err("participant fixture requires authentic governed policy and no existing registration".into());
    }
    let key = fixtures::key(72);
    let secret = hex::encode(key.to_bytes());
    let public_key = hex::encode(key.verifying_key().to_bytes());
    let register = if registered_original { original.clone() } else { registration_request(&before.world, world)? };
    runtime.start()?;
    let original_client = RemoteWorldServiceClient::new(owner.clone()).map_err(|e| e.to_string())?;
    if !registered_original {
        assert_volatile_admission(original_client.submit(register.clone()).map_err(|e| e.to_string())?, &register.correlation)?;
    }
    // For an original registration, read its recovered result without resubmission.
    // Duplicate execution was already checked at both survivor reopens.
    let result = wait_for_new_request_result(&runtime, &register, Duration::from_secs(120), "survivor governed registration")?;
    if result.request != register || result.rejected.is_some() {
        return Err(format!("governed registration failed: {:?}", result.rejected));
    }
    let receipt: GameParticipantReceiptV1 = serde_json::from_value(result.receipt.clone()).map_err(|e| e.to_string())?;
    let mut private = owner.clone();
    private.read_private_key_hex = hex::encode(fixtures::key(71).to_bytes());
    private.scope_id = format!("agent:{}", receipt.agent_id);
    // Scope selection is not authority: the original bootstrap signer is not
    // the newly registered owner and has no delegation for this participant.
    let unrelated_reader = RemoteWorldServiceClient::new(private.clone()).map_err(|e| e.to_string())?;
    let denied = unrelated_reader.lookup_formal_original(register.clone()).err()
        .ok_or("bootstrap signer exposed the new participant's private result")?;
    if !denied.to_string().contains("formal scope delegation unavailable") {
        return Err(format!("unexpected unrelated-reader rejection: {denied}"));
    }
    private.read_private_key_hex = secret.clone();
    let client = RemoteWorldServiceClient::new(private.clone()).map_err(|e| e.to_string())?;
    let registered_ref = lookup_original_commit(&client, &register)?;
    let registered = runtime.history.fixed_publication(&registered_ref).map_err(|e| e.to_string())?;
    let identity = registered.world().current_agent_identity(&receipt.agent_id).ok_or("registration did not install real identity")?;
    if (if registered_original { registered_ref != *original_ref } else { registered_ref.position <= original_ref.position }) || identity.owner_binding != receipt.owner_binding
        || identity.generation != receipt.owner_generation.0 || receipt.owner_public_key != public_key
    { return Err("governed registration binding or position mismatch".into()); }
    stop_owned_protected_runtime(&mut runtime, Duration::from_secs(120), "survivor_participant_registered")?;
    drop(runtime);
    // The identity is loaded solely from the qualified native archive, never installed on reopen.
    let mut runtime = FormalLiveRuntime::open_with_native_history(config)?;
    let restored = runtime.published.current()?;
    if restored.world.current_agent_identity(&receipt.agent_id).as_ref() != Some(&identity)
        || restored.anchor.head().record().original_epoch != epoch
    { return Err("cold reopen lost governed participant identity or epoch".into()); }
    let binding = restored.world.current_cognition_runtime_binding().map_err(|e| format!("{e:?}"))?;
    let starter = request(world, WorldServicePayloadV1::GameStarterClaim(sign_read_request(
        GAME_STARTER_CLAIM_DOMAIN,
        GameStarterClaimV1 { api_rev: 1, client_operation_id: "survivor-starter".into(), world_id: world.world_id.clone(),
            genesis_digest: world.genesis_digest.clone(), branch_id: binding.branch_id, reorg_epoch: DecimalU64(binding.reorg_epoch),
            agent_id: receipt.agent_id.clone(), owner_binding: receipt.owner_binding.clone(), owner_generation: receipt.owner_generation },
        &secret,
    )?))?;
    runtime.start()?;
    assert_volatile_admission(client.submit(starter.clone()).map_err(|e| e.to_string())?, &starter.correlation)?;
    let result = wait_for_new_request_result(&runtime, &starter, Duration::from_secs(120), "survivor governed starter")?;
    if result.request != starter || result.rejected.is_some() { return Err(format!("governed starter failed: {:?}", result.rejected)); }
    let committed = lookup_original_commit(&client, &starter)?;
    let publication = runtime.history.fixed_publication(&committed).map_err(|e| e.to_string())?;
    let claim = publication.world().state().starter_oc_claims.get(&receipt.agent_id).ok_or("starter did not create actual claim")?.clone();
    if committed.position <= registered_ref.position || claim.player_id != receipt.owner_binding
        || claim.public_key.as_deref() != Some(public_key.as_str()) || claim.amount != 100_000_000
        || publication.world().main_token_liquid_balance(&receipt.agent_id) != claim.amount
    { return Err("starter effect differs from registered owner or exact balance".into()); }
    let mut public_config = owner.clone();
    public_config.scope_id = "public".into();
    let public_client = RemoteWorldServiceClient::new(public_config).map_err(|e| e.to_string())?;
    let public_error = public_client.lookup_formal_original(starter.clone()).err().ok_or("public scope exposed private starter result")?;
    if !public_error.to_string().contains("formal result private Agent scope mismatch") { return Err(format!("unexpected public Lookup rejection: {public_error}")); }
    if lookup_original_commit(&original_client, original)? != *original_ref { return Err("registration replaced original request".into()); }
    stop_owned_protected_runtime(&mut runtime, Duration::from_secs(120), "survivor_participant_starter")?;
    drop(runtime);
    let mut runtime = FormalLiveRuntime::open_with_native_history(config)?;
    runtime.start()?;
    let final_cut = runtime.published.current()?;
    if final_cut.world.current_agent_identity(&receipt.agent_id).as_ref() != Some(&identity)
        || final_cut.world.main_token_liquid_balance(&receipt.agent_id) != claim.amount
        || lookup_original_commit(&client, &starter)? != committed || lookup_original_commit(&client, &register)? != registered_ref
        || lookup_original_commit(&original_client, original)? != *original_ref
        || final_cut.world.state().starter_oc_claims.get(&receipt.agent_id) != Some(&claim)
    { return Err("final cold reopen changed governed or original request effects".into()); }
    stop_owned_protected_runtime(&mut runtime, Duration::from_secs(120), "survivor_participant_final_reopen")
}

/// Compare the actual registered Agent, durable authority and exact result across reopens.
pub(crate) fn registered_effect(state: &oasis7_runtime::World, original: &SubmitIntentRequest<WorldServicePayloadV1>) -> Result<Vec<u8>, String> {
    let WorldServicePayloadV1::GameParticipantRegister { owner, .. } = &original.signed_payload else { return Err("registration original required".into()); };
    let key = correlation::key_digest(&original.correlation.key).map_err(|e| e.to_string())?;
    let result = retained_result_from_hot_or_cold(state, &key)?;
    if result.request != *original || result.rejected.is_some() { return Err("recovered registration result differs".into()); }
    let receipt: GameParticipantReceiptV1 = serde_json::from_value(result.receipt.clone()).map_err(|e| e.to_string())?;
    let identity = state.current_agent_identity(&receipt.agent_id).ok_or("recovered registration identity absent")?;
    let agent = state.state().agents.get(&receipt.agent_id).ok_or("recovered registered Agent absent")?;
    if state.state().agents.len() != 1 || receipt.owner_public_key != owner.request.owner_public_key
        || identity.owner_binding != receipt.owner_binding || identity.generation != receipt.owner_generation.0
        || state.capability_revocation_state().participants.bindings.get(&receipt.owner_binding) != Some(&receipt)
    { return Err("recovered registration authority differs from signed owner".into()); }
    serde_cbor::to_vec(&(agent, identity, receipt, result)).map_err(|e| e.to_string())
}

pub(crate) fn registered_scope(original: &SubmitIntentRequest<WorldServicePayloadV1>) -> Result<String, String> {
    let WorldServicePayloadV1::GameParticipantRegister { owner, .. } = &original.signed_payload else { return Err("registration original required".into()); };
    let key = oasis7_runtime::service_admission::participant::account_key(&owner.request)?;
    Ok(format!("agent:participant:{}", key.strip_prefix("blake3:").unwrap_or(&key)))
}
