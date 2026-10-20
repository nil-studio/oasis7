//! Current canonical owner authorization for the independent starter action.
use super::*;
use oasis7_agent_api::game::{GAME_STARTER_CLAIM_DOMAIN, GameStarterClaimV1};
pub fn validate_owner(
    world: &crate::World,
    signed: &SignedReadRequest<GameStarterClaimV1>,
    current_branch: &str,
    current_reorg: u64,
) -> Result<(), String> {
    signed.request.validate()?;
    oasis7_client_api::world_service::private_goal::verify_owner_signature_strict(
        GAME_STARTER_CLAIM_DOMAIN,
        &signed.request,
        &signed.subject_public_key,
        &signed.signature_hex,
    )?;
    let r = &signed.request;
    let identity = world
        .current_agent_identity(&r.agent_id)
        .ok_or("starter identity unavailable")?;
    let owner = agent_authority::owner_public_key(world, &r.agent_id)?;
    let binding = world
        .current_cognition_runtime_binding()
        .map_err(|e| format!("{e:?}"))?;
    if owner != signed.subject_public_key
        || identity.owner_binding != r.owner_binding
        || identity.generation != r.owner_generation.0
        || r.world_id != binding.world_id
        || r.branch_id != current_branch
        || r.reorg_epoch.0 != current_reorg
        || binding.branch_id != current_branch
        || binding.reorg_epoch != current_reorg
        || world.chain_resource_manifest().genesis_ref.as_ref() != Some(&r.genesis_digest)
    {
        return Err("starter current owner, world or history mismatch".into());
    }
    Ok(())
}
pub fn eligibility(
    world: &crate::World,
    agent: &str,
    owner_public_key: &str,
) -> Result<serde_json::Value, String> {
    world.game_starter_eligibility(agent, owner_public_key)
}
pub(crate) fn validate_result(
    world: &crate::World,
    record: &serde_json::Value,
) -> Result<(), String> {
    let result: CanonicalIntentResultV1 =
        serde_json::from_value(record.clone()).map_err(|e| e.to_string())?;
    if let WorldServicePayloadV1::GameStarterClaim(signed) = &result.request.signed_payload {
        if result.rejected.is_some() {
            if !result.receipt.is_null() {
                return Err("rejected starter action has success receipt".into());
            }
        } else {
            let claim = world
                .state()
                .starter_oc_claims
                .get(&signed.request.agent_id)
                .ok_or("starter success has no actual claim")?;
            if claim.player_id != signed.request.owner_binding
                || claim.public_key.as_ref() != Some(&signed.subject_public_key)
                || result.receipt.get("claim")
                    != Some(&serde_json::to_value(claim).map_err(|e| e.to_string())?)
            {
                return Err("starter receipt differs from actual canonical claim".into());
            }
        }
    }
    Ok(())
}
