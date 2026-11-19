//! Canonical owner authorization for native action-budget grants.
use super::{CanonicalIntentResultV1, SignedReadRequest, agent_authority, game_task};
use crate::World;
use oasis7_agent_api::game::{
    GAME_NATIVE_DELEGATION_DOMAIN, GameNativeDelegationControlV1, GameNativeDelegationOperationV1,
};
use oasis7_client_api::world_service::private_goal::verify_owner_signature_strict;

pub fn validate_owner(
    world: &World,
    signed: &SignedReadRequest<GameNativeDelegationControlV1>,
) -> Result<(), String> {
    validate_owner_staged(world, signed, std::iter::empty())
}

/// An original settled request may outlive its task fence. Receipt access still
/// requires the original owner signature and that owner's current authority.
pub fn validate_receipt_owner(
    world: &World,
    signed: &SignedReadRequest<GameNativeDelegationControlV1>,
) -> Result<(), String> {
    signed.request.validate()?;
    verify_owner_signature_strict(
        GAME_NATIVE_DELEGATION_DOMAIN,
        &signed.request,
        &signed.subject_public_key,
        &signed.signature_hex,
    )?;
    if signed.subject_public_key != signed.request.fence.owner_public_key
        || agent_authority::owner_public_key(world, &signed.request.fence.agent_id)?
            != signed.subject_public_key
    {
        return Err("native delegation receipt requires current owner".into());
    }
    Ok(())
}

pub fn validate_owner_staged<'a>(
    world: &World,
    signed: &SignedReadRequest<GameNativeDelegationControlV1>,
    staged: impl IntoIterator<Item = &'a CanonicalIntentResultV1>,
) -> Result<(), String> {
    signed.request.validate()?;
    verify_owner_signature_strict(
        GAME_NATIVE_DELEGATION_DOMAIN,
        &signed.request,
        &signed.subject_public_key,
        &signed.signature_hex,
    )?;
    let fence = &signed.request.fence;
    let identity = world
        .current_agent_identity(&fence.agent_id)
        .ok_or("native delegation owner identity unavailable")?;
    let binding = world
        .current_cognition_runtime_binding()
        .map_err(|e| format!("{e:?}"))?;
    if agent_authority::owner_public_key(world, &fence.agent_id)? != signed.subject_public_key
        || fence.owner_public_key != signed.subject_public_key
        || fence.owner_binding != identity.owner_binding
        || fence.owner_generation.0 != identity.generation
        || fence.world_id != binding.world_id
        || fence.branch_id != binding.branch_id
        || fence.reorg_epoch.0 != binding.reorg_epoch
        || world
            .chain_resource_manifest()
            .genesis_ref
            .as_ref()
            .is_some_and(|genesis| genesis != &fence.genesis_digest)
    {
        return Err("native delegation current owner or history fence mismatch".into());
    }
    game_task::validate_owner_task_fence_staged(world, fence, staged)?;
    if let GameNativeDelegationOperationV1::Install {
        valid_until_tick, ..
    } = signed.request.operation
        && valid_until_tick.0 < world.state().time
    {
        return Err("native delegation grant already expired".into());
    }
    Ok(())
}
