//! Actual selected Proposal shadow admission, followed by its Current state CAS.
use super::*;
use crate::world::incremental_normal_action_factory_schedule::FactoryScheduleReadErrorV1;
use super::{instance_current::{CurrentModuleInstanceFactsV1,ModuleInstanceFactsFeesV1},registry_rules::ModuleRegistryRuleErrorV1};
use oasis7_formal_protocol::incremental_value::ValueNodeReader;
use oasis7_world_state::{incremental_runtime_proposal::CurrentRuntimeProposalV1,incremental_field_overlay::WorldFieldOverlayContextV1,incremental_field_read::WorldFieldReadContextV1};
use crate::world::incremental_module_invocation::AcceptedInvocationDirectoryV1;

pub(super) struct ValidatedCurrentModuleShadowV1{
 pub(super) proposal:ReservedValue<CurrentRuntimeProposalV1>,
 pub(super) manifest_hash:ReservedValue<String>,
 pub(super) facts:ModuleInstanceFactsFeesV1,
 pub(super) decoded:Option<crate::ModuleChangeSet>,pub(super) fee:MemoryReservation,
}
pub(super) enum CurrentModuleShadowDecisionV1{
 Ready(ValidatedCurrentModuleShadowV1),
 Rejected{error:ReservedValue<crate::WorldError>,_facts:Option<ModuleInstanceFactsFeesV1>},
}

#[allow(clippy::too_many_arguments)]
pub(super) fn prepare_current_module_shadow_v1<M,P,R,E,C,PE,PC>(
 proposal:ReservedValue<CurrentRuntimeProposalV1>,sources:&ModuleLifecycleSourcesV1<'_>,
 ctx:&mut WorldFieldOverlayContextV1<'_, '_, '_,M,R,E,C>,parent:&mut WorldFieldReadContextV1<'_,P,R,PE,PC>,
 factory:&mut FactoryScheduleWorkV1,module:&mut ModuleLifecycleWorkV1<'_>,common:&mut FormalBoundedContextV1<'_>,
 directory:&ReservedValue<AcceptedInvocationDirectoryV1>,live:&ExecutionFence,
)->Result<CurrentModuleShadowDecisionV1,FactoryScheduleReadErrorV1>
where M:MerkleNodeReader,P:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool,PE:FnMut()->Option<ValueTreeError>,PC:FnMut()->bool{
 proposal.validate_strict(ctx.transaction,common.memory(),live).map_err(overlay_error)?;
 let id=proposal.original_id();
 // Source graph and parsed output coexist. Admission precedes original clone.
 let count=proposal.output_reservation_count_v1()?;
 pay(common,count.checked_mul(std::mem::size_of::<MemoryReservation>()).ok_or_else(invalid)?)?;
 let heap=proposal.output_reserved_bytes_v1()?.checked_mul(4).and_then(|n|n.checked_add(8192)).ok_or_else(invalid)?;
 pay(common,heap)?;
 let fee=common.memory().reserve(heap,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
 let Some(value)=proposal.value()else{return Ok(CurrentModuleShadowDecisionV1::Rejected{error:fee.retain(crate::WorldError::ProposalNotFound{proposal_id:id}),_facts:None})};
 if !matches!(value.status,oasis7_world_state::ProposalStatus::Proposed){return Ok(CurrentModuleShadowDecisionV1::Rejected{error:fee.retain(crate::WorldError::ProposalInvalidState{proposal_id:id,expected:"proposed".to_owned(),found:value.status.label()}),_facts:None})}
 let changes=match value.manifest.module_changes(){Ok(v)=>v,Err(error)=>return Ok(CurrentModuleShadowDecisionV1::Rejected{error:fee.retain(error),_facts:None})};
 let mut source=CurrentModuleInstanceFactsV1::new(sources,ctx,parent,factory,module,common,live,directory)?;
 let validation=(||{
 if let Some(changes)=&changes{
  super::registry_rules::validate_module_changes_from_source_v1(&mut source,changes)?;
  super::manifest_rules::shadow_validate_module_changes_from_source_v1(&mut source,changes)?;
 }Ok(())})();
 let facts=source.into_parts();
 if let Err(error)=validation{let error=match error{ModuleRegistryRuleErrorV1::World(e)=>e,ModuleRegistryRuleErrorV1::Source(FactoryScheduleReadErrorV1::World(e))=>e,ModuleRegistryRuleErrorV1::Source(e)=>return Err(e)};return Ok(CurrentModuleShadowDecisionV1::Rejected{error:fee.retain(error),_facts:Some(facts)})}
 let manifest_hash=crate::world::incremental_normal_action_diplomacy::fixed_json_digest_v1(&value.manifest,common)?;
 Ok(CurrentModuleShadowDecisionV1::Ready(ValidatedCurrentModuleShadowV1{proposal,manifest_hash,facts,decoded:changes,fee}))
}
