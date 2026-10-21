//! Deterministic participant admission. No session or external service is authority here.
use super::SignedReadRequest;
use crate::World;
use oasis7_agent_api::game::*;
use oasis7_client_api::world_service::private_goal::verify_owner_signature_strict;
pub use oasis7_world_state::service_admission::participant::*;
pub fn validate_admission(
    world: &World,
    owner: &SignedReadRequest<GameParticipantRegisterV1>,
    admission: &SignedReadRequest<GameParticipantAdmissionV1>,
    current_branch: &str,
    current_reorg: u64,
) -> Result<(), String> {
    let r = &owner.request;
    r.validate()?;
    if owner.subject_public_key != r.owner_public_key {
        return Err("participant owner signer mismatch".into());
    }
    verify_owner_signature_strict(
        GAME_PARTICIPANT_REGISTER_DOMAIN,
        r,
        &owner.subject_public_key,
        &owner.signature_hex,
    )?;
    verify_owner_signature_strict(
        GAME_PARTICIPANT_ADMISSION_DOMAIN,
        &admission.request,
        &admission.subject_public_key,
        &admission.signature_hex,
    )?;
    let policy = world
        .capability_revocation_state()
        .participants
        .policy
        .as_ref()
        .ok_or("participant policy unavailable")?;
    let a = &admission.request;
    let tick = world.state().time;
    let binding = world
        .current_cognition_runtime_binding()
        .map_err(|e| format!("{e:?}"))?;
    if !namespace_was_authorized(world, &r.identity_namespace) {
        return Err("participant request namespace epoch unavailable".into());
    }
    if !policy.registry_complete
        || policy.api_rev != 1
        || !policy
            .identity_namespace
            .same_authority(&r.identity_namespace)
        || policy.world_id != r.world_id
        || policy.genesis_digest != r.genesis_digest
        || admission.subject_public_key != policy.issuer_public_key
        || a.api_rev != 1
        || a.policy_generation.0 != policy.generation
        || a.identity_namespace != policy.identity_namespace
        || a.account_id != r.account_id
        || a.world_id != r.world_id
        || a.genesis_digest != r.genesis_digest
        || a.owner_public_key != r.owner_public_key
        || a.business_digest != business_digest(r)?
        || a.branch_id != current_branch
        || a.reorg_epoch.0 != current_reorg
        || binding.world_id != r.world_id
        || binding.branch_id != current_branch
        || binding.reorg_epoch != current_reorg
        || world.chain_resource_manifest().genesis_ref.as_ref() != Some(&r.genesis_digest)
        || a.valid_from_tick.0 > tick
        || a.valid_until_tick.0 < tick
        || a.valid_until_tick
            .0
            .checked_sub(a.valid_from_tick.0)
            .is_none_or(|n| n > policy.max_grant_ticks)
    {
        return Err("participant admission policy, identity, history or tick mismatch".into());
    }
    if let Some(old) = world
        .capability_revocation_state()
        .participants
        .bindings
        .get(&account_key(r)?)
    {
        let identity = world
            .current_agent_identity(&old.agent_id)
            .ok_or("participant identity unavailable")?;
        let current_owner = world
            .current_agent_owner(&old.agent_id)
            .ok_or("participant canonical owner unavailable")?;
        if old.owner_public_key != r.owner_public_key
            || identity.owner_binding != old.owner_binding
            || identity.generation != old.owner_generation.0
            || current_owner.public_key.as_ref() != Some(&r.owner_public_key)
            || current_owner.player_id != old.owner_binding
            || current_owner.generation != old.owner_generation.0
        {
            return Err("participant current owner mismatch".into());
        }
    }
    Ok(())
}
/// Revalidate durable binding evidence, even when the admission window is historical.
pub fn validate_registry(world: &World) -> Result<(), String> {
    let registry = &world.capability_revocation_state().participants;
    if registry.policy.is_none()
        && (!registry.bindings.is_empty() || !registry.operations.is_empty())
    {
        return Err("participant registry missing policy".into());
    }
    if let Some(policy) = &registry.policy {
        if policy.api_rev != 1
            || policy.generation == 0
            || !policy.registry_complete
            || policy.max_grant_ticks == 0
            || policy.max_grant_ticks > 256
            || policy.max_participants == 0
            || policy.world_id != world.chain_resource_manifest().world_id
            || world.chain_resource_manifest().genesis_ref.as_ref() != Some(&policy.genesis_digest)
            || policy.identity_namespace.instance_epoch.0 == 0
            || policy.identity_namespace.service_id.trim().is_empty()
            || policy.identity_namespace.environment_id.trim().is_empty()
            || registry.bindings.len() as u64 > policy.max_participants
        {
            return Err("participant checkpoint policy invalid".into());
        }
        let bytes: [u8; 32] = hex::decode(&policy.issuer_public_key)
            .map_err(|e| e.to_string())?
            .try_into()
            .map_err(|_| "participant checkpoint issuer key invalid")?;
        if ed25519_dalek::VerifyingKey::from_bytes(&bytes)
            .map_err(|e| e.to_string())?
            .is_weak()
        {
            return Err("participant checkpoint issuer key weak".into());
        }
    }
    let mut agents = std::collections::BTreeSet::new();
    for (key, receipt) in &registry.bindings {
        let r = GameParticipantRegisterV1 {
            api_rev: 1,
            client_operation_id: receipt.client_operation_id.clone(),
            identity_namespace: receipt.identity_namespace.clone(),
            account_id: receipt.account_id.clone(),
            world_id: receipt.world_id.clone(),
            genesis_digest: receipt.genesis_digest.clone(),
            owner_public_key: receipt.owner_public_key.clone(),
            owner_key_version: DecimalU64(1),
        };
        r.validate()?;
        let policy = registry
            .policy
            .as_ref()
            .ok_or("participant policy missing")?;
        let expected_agent = format!("participant:{}", key.strip_prefix("blake3:").unwrap_or(key));
        if !agents.insert(&receipt.agent_id)
            || receipt.api_rev != 1
            || (receipt.imported != registry.migrated_bindings.contains_key(key))
            || (!receipt.imported
                && (receipt.agent_id != expected_agent
                    || receipt.owner_binding != *key
                    || receipt.owner_generation.0 != 1))
            || receipt.registered_tick.0 > world.state().time
            || !policy
                .identity_namespace
                .same_authority(&receipt.identity_namespace)
            || !namespace_was_authorized(world, &receipt.identity_namespace)
            || receipt.world_id != policy.world_id
            || receipt.genesis_digest != policy.genesis_digest
            || account_key(&r)? != *key
            || business_digest(&r)? != receipt.business_digest
            || !world.state().agents.contains_key(&receipt.agent_id)
            || world.current_agent_identity(&receipt.agent_id).is_none()
            || registry
                .operations
                .get(&operation_key(&r)?)
                .map(|op| &op.receipt)
                != Some(receipt)
        {
            return Err("participant registry evidence invalid".into());
        }
    }
    for (key, op) in &registry.operations {
        op.request.validate()?;
        if operation_key(&op.request)? != *key
            || registry.bindings.get(&account_key(&op.request)?) != Some(&op.receipt)
            || op.request.owner_public_key != op.receipt.owner_public_key
        {
            return Err("participant operation without matching binding".into());
        }
    }
    Ok(())
}

pub fn namespace_was_authorized(world: &World, namespace: &GameIdentityNamespaceV1) -> bool {
    let registry = &world.capability_revocation_state().participants;
    registry
        .policy
        .as_ref()
        .is_some_and(|p| &p.identity_namespace == namespace)
        || registry
            .policy_history
            .iter()
            .any(|a| &a.policy.identity_namespace == namespace)
}

/// New effects additionally require enabled registration; receipt reads do not.
pub fn validate_registration(
    world: &World,
    owner: &SignedReadRequest<GameParticipantRegisterV1>,
    admission: &SignedReadRequest<GameParticipantAdmissionV1>,
    current_branch: &str,
    current_reorg: u64,
) -> Result<(), String> {
    validate_admission(world, owner, admission, current_branch, current_reorg)?;
    let registry = &world.capability_revocation_state().participants;
    if !registry
        .policy
        .as_ref()
        .ok_or("participant policy unavailable")?
        .enabled
        && !registry
            .bindings
            .contains_key(&account_key(&owner.request)?)
    {
        return Err("participant new registration disabled".into());
    }
    Ok(())
}
