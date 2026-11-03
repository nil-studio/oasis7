//! One actual current Owner/history scope for selection and terminal fallback.
use super::*;
use oasis7_agent_api::game::GameTaskFenceV1;
pub(super) struct CurrentTaskScope {
    agent: String,
    world: String,
    genesis: Option<String>,
    branch: String,
    reorg: u64,
    owner: String,
    binding: String,
    generation: u64,
}
impl CurrentTaskScope {
    pub(super) fn matches(&self, fence: &GameTaskFenceV1) -> bool {
        fence.agent_id == self.agent
            && fence.world_id == self.world
            && self
                .genesis
                .as_ref()
                .is_none_or(|g| g == &fence.genesis_digest)
            && fence.branch_id == self.branch
            && fence.reorg_epoch.0 == self.reorg
            && fence.owner_public_key == self.owner
            && fence.owner_binding == self.binding
            && fence.owner_generation.0 == self.generation
    }
}
pub(super) fn current_task_scope(world: &World, agent: &str) -> Result<CurrentTaskScope, String> {
    let identity = world
        .current_agent_identity(agent)
        .ok_or("task current owner identity unavailable")?;
    let runtime = world
        .current_cognition_runtime_binding()
        .map_err(|e| format!("{e:?}"))?;
    Ok(CurrentTaskScope {
        agent: agent.into(),
        world: runtime.world_id,
        genesis: world.chain_resource_manifest().genesis_ref.clone(),
        branch: runtime.branch_id,
        reorg: runtime.reorg_epoch,
        owner: agent_authority::owner_public_key(world, agent)?,
        binding: identity.owner_binding.clone(),
        generation: identity.generation,
    })
}
pub(super) fn has_closed_in_current_scope(world: &World, agent: &str) -> Result<bool, String> {
    let Some(index) = &world.capability_revocation_state().task_terminals_v2 else {
        return Ok(false);
    };
    if !index
        .records
        .values()
        .any(|r| r.closed.initial_fence.agent_id == agent)
    {
        return Ok(false);
    }
    let scope = current_task_scope(world, agent)?;
    if scope.genesis.is_none() {
        return Err("terminal fallback requires exact current genesis".into());
    }
    Ok(index
        .records
        .values()
        .any(|r| scope.matches(&r.closed.initial_fence)))
}
