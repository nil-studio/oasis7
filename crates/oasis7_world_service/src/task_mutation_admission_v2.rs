//! Local admission rejection only; actual committed Runtime checks remain final.
use oasis7_agent_api::game::*;
use oasis7_client_api::world_service::{
    task_cleanup_v2::TaskCleanupSubmitV2, task_stop_v2::TaskStopSubmitV2,
};
use oasis7_runtime::World;
fn live_target(
    world: &World,
    f: &GameTaskFenceV1,
    signer: &str,
    generation: u64,
    nonce: u64,
) -> Result<GameTaskControlV1, String> {
    let history = world.resident_guarded_task_history()?;
    let current = history
        .current_control(&f.agent_id)?
        .ok_or("resident task unavailable")?;
    let binding = world
        .current_cognition_runtime_binding()
        .map_err(|e| format!("{e:?}"))?;
    let identity = world
        .current_agent_identity(&f.agent_id)
        .ok_or("current identity unavailable")?;
    let delegation = world
        .capability_revocation_state()
        .agent_signer_delegations
        .get(&f.agent_id)
        .ok_or("current delegation unavailable")?;
    if crate::agent_authority::owner_public_key(world, &f.agent_id)? != f.owner_public_key
        || identity.owner_binding != f.owner_binding
        || identity.generation != f.owner_generation.0
        || binding.world_id != f.world_id
        || binding.branch_id != f.branch_id
        || binding.reorg_epoch != f.reorg_epoch.0
        || world.chain_resource_manifest().genesis_ref.as_ref() != Some(&f.genesis_digest)
        || delegation.revoked
        || delegation.delegate_public_key != signer
        || delegation.generation != generation
        || delegation.nonce != nonce
        || delegation.world_id != f.world_id
        || delegation.branch_id != f.branch_id
        || delegation.owner_binding != f.owner_binding
        || delegation.agent_identity_generation != f.owner_generation.0
    {
        return Err("task mutation current target drift".into());
    }
    Ok(current)
}
pub fn guarded_stop_in_resident_world(world: &World, q: &TaskStopSubmitV2) -> Result<(), String> {
    q.verify_owner_signature()?;
    let p = &q.original.request;
    let current = live_target(
        world,
        &p.initial_fence,
        &p.target_executor_public_key,
        p.target_executor_generation.0,
        p.target_executor_nonce.0,
    )?;
    p.derive_post_control(&current)?;
    Ok(())
}
pub fn cleanup_in_resident_world(world: &World, q: &TaskCleanupSubmitV2) -> Result<(), String> {
    q.verify_owner_signature()?;
    let p = &q.original.request;
    let initial = &p.permitted_branches.exact_initial_active;
    let current = live_target(
        world,
        initial,
        &p.signer_public_key,
        p.signer_generation.0,
        p.signer_nonce.0,
    )?;
    if p.revoke_generation.0 <= p.signer_generation.0 || p.revoke_nonce.0 <= p.signer_nonce.0 {
        return Err("cleanup HWM does not advance".into());
    }
    let fence = GameTaskFenceV1::from_control(&current);
    if fence == *initial {
        return Ok(());
    }
    let intent = p
        .permitted_branches
        .own_stop_intent
        .as_ref()
        .ok_or("own Stop branch not signed")?;
    let mut matched = false;
    if let Some(index) = &world.capability_revocation_state().task_stops_v2 {
        for record in index.records.values() {
            let stop = &record.original;
            let digest = oasis7_wasm_abi::canonical_hash(&(
                GAME_TASK_STOP_DOMAIN_V2,
                &stop.subject_public_key,
                &stop.request,
            ))
            .map_err(|e| e.to_string())?;
            if stop.request.client_operation_id != intent.operation_id
                || digest != intent.intent_digest
            {
                continue;
            }
            if matched
                || stop.request.initial_fence != *initial
                || stop.subject_public_key != q.original.subject_public_key
                || stop.request.custody_account_id != p.custody_account_id
                || stop.request.termination_plan_digest != p.termination_plan_digest
                || stop.request.target_executor_public_key != p.signer_public_key
                || stop.request.target_executor_generation != p.signer_generation
                || stop.request.target_executor_nonce != p.signer_nonce
                || GameTaskFenceV1::from_control(&record.after_control) != fence
                || record.committed_height > world.state().time
            {
                return Err("cleanup own Stop lineage mismatch".into());
            }
            matched = true;
        }
    }
    if !matched {
        return Err("exact canonical own Stop unavailable".into());
    }
    Ok(())
}
