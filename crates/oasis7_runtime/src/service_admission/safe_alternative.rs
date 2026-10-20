//! Separate owner signature domain for each alternative operation.
use super::{SignedReadRequest, agent_authority, verify_read_request};
use crate::World;
use oasis7_client_api::safe_alternative::SafeAlternativeCommandV1;
pub fn validate_owner(
    world: &World,
    signed: &SignedReadRequest<SafeAlternativeCommandV1>,
) -> Result<(), String> {
    let c = &signed.request;
    c.validate()?;
    verify_read_request(c.signing_domain(), signed)?;
    let identity = world
        .current_agent_identity(&c.agent_id)
        .ok_or("authority_changed")?;
    let binding = world
        .current_cognition_runtime_binding()
        .map_err(|_| "authority_changed")?;
    if agent_authority::owner_public_key(world, &c.agent_id)? != signed.subject_public_key
        || c.owner_binding != identity.owner_binding
        || c.owner_generation != identity.generation
        || c.world_id != binding.world_id
        || c.branch_id != binding.branch_id
        || c.reorg_epoch != binding.reorg_epoch
        || world
            .chain_resource_manifest()
            .genesis_ref
            .as_ref()
            .is_none_or(|g| g != &c.genesis_digest)
    {
        return Err("authority_changed".into());
    }
    Ok(())
}
