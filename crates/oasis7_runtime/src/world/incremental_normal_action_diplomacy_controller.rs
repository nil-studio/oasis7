//! Registered-alliance controller binding from the sole original Current resolver.
//! The complete resident order directory is data, never an ownership predicate.
use crate::world::incremental_body::BodyMutationError;
pub(super) use crate::world::incremental_industrial_eligibility::CurrentStateOwnerSourcesV1 as ControllerSourcesV1;
pub(super) use crate::world::incremental_industrial_eligibility::IndustrialEligibilityWorkV1 as ControllerWorkV1;
use crate::world::incremental_industrial_eligibility::{
    CurrentStateOwnerSourcesV1, IndustrialEligibilityErrorV1, IndustrialEligibilityWorkV1,
    resolve_current_state_owner_v1,
};
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::{FormalBoundedContextV1, WorkIdentityV1},
    incremental_execution::{ExecutionError, ExecutionFence, ExecutionTransaction, PreparedGuard},
    incremental_state::MerkleNodeReader,
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use oasis7_world_state::{
    incremental_field_overlay::WorldFieldOverlayContextV1,
    incremental_field_read::WorldFieldReadContextV1, world_rules::OrganizationControllerBindingV1,
};
use std::mem::size_of;
fn bad() -> BodyMutationError {
    BodyMutationError::Invalid("Current organization controller source")
}
fn limit() -> BodyMutationError {
    ValueTreeError::ResourceLimited("organization controller binding layout").into()
}
fn charge(ctx: &mut FormalBoundedContextV1<'_>, n: usize) -> Result<(), BodyMutationError> {
    ctx.charge(1, u64::try_from(n).map_err(|_| limit())?)
        .map_err(|e| BodyMutationError::Original(Box::new(e)))
}
#[derive(Debug)]
struct ControllerReadErrorV1(IndustrialEligibilityErrorV1);
impl std::fmt::Display for ControllerReadErrorV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.0)
    }
}
impl std::error::Error for ControllerReadErrorV1 {}
pub(super) struct CurrentControllerBindingV1 {
    value: ReservedValue<OrganizationControllerBindingV1>,
    identity: PreparedGuard,
    core: WorkIdentityV1,
    memory: MemoryBudget,
}
impl CurrentControllerBindingV1 {
    pub(super) fn value(&self) -> &OrganizationControllerBindingV1 {
        &self.value
    }
    pub(super) fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        common: &FormalBoundedContextV1<'_>,
    ) -> Result<(), BodyMutationError> {
        tx.validate_transaction_identity(&self.identity)?;
        if self.core != common.work().identity()
            || !self.memory.same_pool(common.memory())
            || !self.value.uses_budget(common.memory())
        {
            return Err(bad());
        }
        Ok(())
    }
}
fn copy(text: &str, common: &mut FormalBoundedContextV1<'_>) -> Result<String, BodyMutationError> {
    charge(common, text.len())?;
    let value = text.to_owned();
    if value.capacity() > text.len() {
        return Err(limit());
    }
    Ok(value)
}
fn retain(
    root: &str,
    player: &str,
    source: &str,
    generation: u64,
    tx_identity: PreparedGuard,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<CurrentControllerBindingV1, BodyMutationError> {
    let bytes = size_of::<CurrentControllerBindingV1>()
        .checked_add(size_of::<OrganizationControllerBindingV1>())
        .and_then(|n| n.checked_add(root.len()))
        .and_then(|n| n.checked_add(player.len()))
        .and_then(|n| n.checked_add(source.len()))
        .ok_or_else(limit)?;
    let fee = common
        .memory()
        .reserve(bytes, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let value = OrganizationControllerBindingV1 {
        root_agent_id: copy(root, common)?,
        player_id: copy(player, common)?,
        source_id: copy(source, common)?,
        generation,
    };
    Ok(CurrentControllerBindingV1 {
        value: fee.retain(value),
        identity: tx_identity,
        core: common.work().identity(),
        memory: common.memory().clone(),
    })
}
#[allow(clippy::too_many_arguments)]
pub(super) fn resolve_current_controller_v1<M, P, R, V, E, C, PE, PC>(
    agent: &str,
    sources: &CurrentStateOwnerSourcesV1<'_, '_>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    parent_read: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    work: &mut IndustrialEligibilityWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<Option<CurrentControllerBindingV1>, BodyMutationError>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    V: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
{
    let result = (|| {
        charge(common, agent.len())?;
        if !context.values.memory.same_pool(common.memory())
            || context.values.class != MemoryClass::Ordinary
        {
            return Err(bad());
        }
        let guard = context.transaction.guard();
        let owner = resolve_current_state_owner_v1(
            sources,
            agent,
            context,
            parent_read,
            work,
            &guard,
            live,
        )
        .map_err(|e| BodyMutationError::Original(Box::new(ControllerReadErrorV1(e))))?;
        let Some(owner) = owner.as_ref() else {
            return Ok(None);
        };
        retain(
            owner.primary_agent_id(),
            owner.player_id(),
            owner.source_id(),
            owner.generation(),
            context.transaction.guard(),
            common,
        )
        .map(Some)
    })();
    if result.is_err() {
        let _: Result<(), ExecutionError> = context
            .transaction
            .contribute(|_| Err(ExecutionError::Aborted));
    }
    result
}
