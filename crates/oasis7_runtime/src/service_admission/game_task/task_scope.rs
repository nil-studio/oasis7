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
        matches_current_task_scope_data_v1(CurrentTaskScopeBorrowedDataV1{agent:&self.agent,world:&self.world,genesis:self.genesis.as_deref(),branch:&self.branch,reorg:self.reorg,owner:&self.owner,binding:&self.binding,generation:self.generation},fence,&mut|_,_|Ok::<(),std::convert::Infallible>(())).unwrap()

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

/// Pure original fence comparison DATA. This carries no Current authority.
pub(crate) struct CurrentTaskScopeBorrowedDataV1<'a>{pub(crate) agent:&'a str,pub(crate) world:&'a str,pub(crate) genesis:Option<&'a str>,pub(crate) branch:&'a str,pub(crate) reorg:u64,pub(crate) owner:&'a str,pub(crate) binding:&'a str,pub(crate) generation:u64}
pub(crate) fn matches_current_task_scope_data_v1<E>(scope:CurrentTaskScopeBorrowedDataV1<'_>,fence:&GameTaskFenceV1,work:&mut impl FnMut(u64,usize)->Result<(),E>)->Result<bool,E>{
 work(1,fence.agent_id.len()+scope.agent.len())?;if fence.agent_id!=scope.agent{return Ok(false)}
 work(1,fence.world_id.len()+scope.world.len())?;if fence.world_id!=scope.world{return Ok(false)}
 if let Some(genesis)=scope.genesis{work(1,genesis.len()+fence.genesis_digest.len())?;if genesis!=fence.genesis_digest{return Ok(false)}}
 work(1,fence.branch_id.len()+scope.branch.len())?;if fence.branch_id!=scope.branch{return Ok(false)}work(1,0)?;if fence.reorg_epoch.0!=scope.reorg{return Ok(false)}
 work(1,fence.owner_public_key.len()+scope.owner.len())?;if fence.owner_public_key!=scope.owner{return Ok(false)}work(1,fence.owner_binding.len()+scope.binding.len())?;if fence.owner_binding!=scope.binding{return Ok(false)}work(1,0)?;Ok(fence.owner_generation.0==scope.generation)
}
