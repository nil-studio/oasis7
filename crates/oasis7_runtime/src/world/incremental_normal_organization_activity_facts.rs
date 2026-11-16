//! The activity-close predicate reads only actual Current Agent presence.
use super::*;
use oasis7_world_state::{incremental_agent_subtree::{read_current_agent_presence_v1,CurrentAgentPresenceV1},
 incremental_field_read::WorldFieldReadContextV1,
 world_rules::{OrganizationRegistrationV1,OrganizationCurrentLifecycleFactsV1,OrganizationCurrentAgentFactV1}};
use std::collections::BTreeMap;
pub(super) struct ActivityCloseFactsV1 {
    value: ReservedValue<OrganizationCurrentLifecycleFactsV1>,
    guard: PreparedGuard,
}
impl ActivityCloseFactsV1 {
    pub(super) fn with_kernel<T>(&self, tx: &ExecutionTransaction<'_, impl MerkleNodeReader>, run: impl FnOnce(&OrganizationCurrentLifecycleFactsV1)->T) -> Result<T,BodyMutationError> {
        tx.validate_transaction_identity(&self.guard)?;
        Ok(run(&self.value))
    }
}
/// Every founder/controller is resolved through the existing Current reader.
/// Empty non-agent lanes correspond to fields unused by the close reducer.
#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn read_activity_facts_v1<M,P,R,E,C,PE,PC>(
 registration:&OrganizationRegistrationV1,sources:&NormalTailSourcesV1<'_,'_>,
 ctx:&mut oasis7_world_state::incremental_field_overlay::WorldFieldOverlayContextV1<'_,'_,'_,M,R,E,C>,
 parent:&mut WorldFieldReadContextV1<'_,P,R,PE,PC>,remaining:&mut usize,
 common:&mut crate::world::incremental_normal_tail::NormalTailWorkV1<'_, '_>,live:&ExecutionFence,
)->Result<ActivityCloseFactsV1,BodyMutationError>
where M:MerkleNodeReader,P:MerkleNodeReader,R:oasis7_formal_protocol::incremental_value::ValueNodeReader,
 E:FnMut()->Option<oasis7_formal_protocol::incremental_value::ValueTreeError>,C:FnMut()->bool,
 PE:FnMut()->Option<oasis7_formal_protocol::incremental_value::ValueTreeError>,PC:FnMut()->bool {
 let count=registration.controller_bindings.len().checked_add(1).ok_or_else(||invalid("activity presence size"))?;
 let bytes=registration.controller_bindings.keys().try_fold(registration.founder_id.len()+registration.organization_id.len(),|n,id|n.checked_add(id.len()).ok_or_else(||invalid("activity presence strings")))?
 .checked_add(count.checked_mul(size_of::<OrganizationCurrentAgentFactV1>()+size_of::<String>()+128).ok_or_else(||invalid("activity presence layout"))?).ok_or_else(||invalid("activity presence layout"))?;
 common.debit(count as u64,bytes as u64).map_err(|e|BodyMutationError::Original(Box::new(e)))?;
 let fee=common.memory().reserve(bytes+size_of::<OrganizationCurrentLifecycleFactsV1>(),MemoryClass::Ordinary)
 .map_err(oasis7_formal_protocol::incremental_value::ValueTreeError::Memory)?;
 let mut agents=BTreeMap::new();
 for id in std::iter::once(&registration.founder_id).chain(registration.controller_bindings.keys()) {
 if agents.contains_key(id){continue}
 let guard=ctx.transaction.guard();
 let present=read_current_agent_presence_v1(id,sources.raw().agents(),sources.roots(),ctx,parent,remaining,&guard,live)
 .map_err(|e|BodyMutationError::Original(Box::new(e)))?==CurrentAgentPresenceV1::Existing;
 agents.insert(id.clone(),OrganizationCurrentAgentFactV1{agent_id:id.clone(),present,owner:None});
 }
 Ok(ActivityCloseFactsV1 { guard: ctx.transaction.guard(), value: fee.retain(OrganizationCurrentLifecycleFactsV1{organization_id:registration.organization_id.clone(),agents,
 alliance:None,contracts:BTreeMap::new(),estate:BTreeMap::new(),requests:BTreeMap::new()})})
}
