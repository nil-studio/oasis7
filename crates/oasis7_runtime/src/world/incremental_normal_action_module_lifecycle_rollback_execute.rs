//! Actual selected rollback prelude and the mandatory governed update source.
use super::*;
use super::{instance_current::{CurrentModuleInstanceFactsV1,ModuleInstanceFactsFeesV1},rollback_rules::{ModuleRollbackPreludeV1,prepare_module_rollback_prelude_v1},upgrade_rules::ModuleInstanceRecordV1,registry_rules::ModuleManifestRecordV1};
use crate::world::{incremental_module_invocation::{AcceptedInvocationDirectoryV1,SelectedAcceptedDispositionV1},incremental_normal_action_factory_schedule::FactoryScheduleReadErrorV1};
use oasis7_formal_protocol::incremental_value::ValueNodeReader;
use oasis7_world_state::{incremental_field_overlay::WorldFieldOverlayContextV1,incremental_field_read::WorldFieldReadContextV1,incremental_runtime_artifact_config::SelectedModuleManifestV1};
pub(super) enum CurrentSelectedRollbackDecisionV1{
 Rejected{reason:crate::RejectReason,facts:ModuleInstanceFactsFeesV1,_fee:MemoryReservation},
 Governed{update:super::governance_plan::PreparedModuleManifestUpdateV1,instance:ReservedValue<crate::world::incremental_module_read::PreparedModuleInstanceFields>,snapshot:ReservedValue<crate::ModuleInstanceState>,target_source:ReservedValue<SelectedModuleManifestV1>,target_manifest:crate::ModuleManifest,from_wasm_hash:String,from_release_request_id:Option<u64>,facts:ModuleInstanceFactsFeesV1,fee_kind:oasis7_agent_api::world_action::ResourceKind,fee_amount:i64},
}
#[allow(clippy::too_many_arguments)]
pub(super) fn prepare_current_selected_rollback_v1<M,P,R,E,C,PE,PC>(selected:&SelectedPendingActionV1,outcome:&ReservedValue<RootRulePhaseOutcomeV1>,accepted:&SelectedAcceptedDispositionV1,sources:&ModuleLifecycleSourcesV1<'_>,ctx:&mut WorldFieldOverlayContextV1<'_, '_, '_,M,R,E,C>,parent:&mut WorldFieldReadContextV1<'_,P,R,PE,PC>,factory:&mut FactoryScheduleWorkV1,module:&mut ModuleLifecycleWorkV1<'_>,common:&mut FormalBoundedContextV1<'_>,directory:&ReservedValue<AcceptedInvocationDirectoryV1>,live:&ExecutionFence)->Result<CurrentSelectedRollbackDecisionV1,FactoryScheduleReadErrorV1>
where M:MerkleNodeReader,P:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool,PE:FnMut()->Option<ValueTreeError>,PC:FnMut()->bool{
 accepted.validate(selected,ctx.transaction,common.memory())?;
 let action=outcome.override_action_for_selected_v1(selected,ctx.transaction)?.unwrap_or(&selected.envelope().action);
 let(actor,id,target)=match action{crate::Action::RollbackModuleInstance{operator_agent_id,instance_id,target_module_version}|crate::Action::RollbackModuleInstanceWithFinality{operator_agent_id,instance_id,target_module_version,..}=>(operator_agent_id.as_str(),instance_id.as_str(),target_module_version.as_str()),_=>return Err(invalid().into())};
 let mut source=CurrentModuleInstanceFactsV1::new(sources,ctx,parent,factory,module,common,live,directory)?;
 let result=prepare_module_rollback_prelude_v1(&mut source,actor,id,target);let facts=source.into_parts();
 let(instance,target_source,target_manifest,changes,from_wasm_hash,from_release_request_id,fee_kind,fee_amount)=match result?{
 ModuleRollbackPreludeV1::Rejected(reason)=>{pay(common,4096)?;let fee=common.memory().reserve(4096,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;return Ok(CurrentSelectedRollbackDecisionV1::Rejected{reason,facts,_fee:fee})},
 ModuleRollbackPreludeV1::Plan{instance,target_source,target_manifest,changes,from_wasm_hash,from_release_request_id,fee_kind,fee_amount}=>(instance,target_source,target_manifest,changes,from_wasm_hash,from_release_request_id,fee_kind,fee_amount)};
 let snapshot=instance.snapshot_paid_v1(common)?;
 let instance=match instance{ModuleInstanceRecordV1::Current(owner)=>owner,ModuleInstanceRecordV1::Legacy(_)=>return Err(invalid().into())};
 let target_source=match target_source{ModuleManifestRecordV1::Current(owner)=>owner,ModuleManifestRecordV1::Legacy(_)=>return Err(invalid().into())};
 let before=crate::world::incremental_normal_action_factory_schedule::with_module_lifecycle_read_io_v1(sources.families.roots,ctx,parent,factory,common,module,|ctx,parent,factory,module|Ok(super::governance_manifest::read_current_module_world_manifest_v1(sources,ctx,parent,factory,module,live)?))?;
 match super::governance_plan::prepare_module_rollback_manifest_update_v1(before,&changes,ctx.transaction,common,live)?{
 super::governance_plan::ModuleManifestUpdateDecisionV1::Ready(update)=>Ok(CurrentSelectedRollbackDecisionV1::Governed{update,instance,snapshot,target_source,target_manifest,from_wasm_hash,from_release_request_id,facts,fee_kind,fee_amount}),
 super::governance_plan::ModuleManifestUpdateDecisionV1::Rejected{reason,_fee}=>Ok(CurrentSelectedRollbackDecisionV1::Rejected{reason,facts,_fee}),}
}
