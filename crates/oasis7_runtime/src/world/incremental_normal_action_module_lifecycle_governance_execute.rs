//! Fixed Proposal event/state sequence. Earlier committed governance events
//! survive later business refusal; infrastructure failures poison the whole TX.
use super::*;
use super::{event::PreparedNormalModuleLifecycleEventV1,governance_state::*};
use crate::world::{incremental_step::IwsStepContext,incremental_body_event::{AgentEventOriginalV1,materials::CorePolicyMaterialContextV1},incremental_module_invocation::{AcceptedInvocationDirectoryV1,SelectedAcceptedDispositionV1}};
use oasis7_formal_protocol::{incremental_value::{ValueNodeReader,ValueNodeWriter,ValueReadContext},incremental_state::StatePageWriter};
use oasis7_world_state::{incremental_field_read::WorldFieldReadContextV1,incremental_field_member::MemberRecordWriteContext,incremental_field_overlay::WorldFieldOverlayContextV1};

pub(super) struct ExecutedCurrentModuleGovernanceV1{
 pub(super) first:PreparedNormalModuleLifecycleEventV1,
 pub(super) last:PreparedNormalModuleLifecycleEventV1,
 pub(super) proposal_id:u64,
}
#[derive(Clone,Copy)]
pub(super) enum CurrentModuleGovernanceFailureStageV1{Shadow,Approve}
pub(super) enum CurrentModuleGovernanceSequenceV1{
 Queued(ExecutedCurrentModuleGovernanceV1),
 Rejected{first:PreparedNormalModuleLifecycleEventV1,last:Option<PreparedNormalModuleLifecycleEventV1>,error:ReservedValue<crate::WorldError>,_facts:Option<super::instance_current::ModuleInstanceFactsFeesV1>,stage:CurrentModuleGovernanceFailureStageV1},
}
fn body_or_business(result:Result<CommittedModuleProposalStateV1,ModuleGovernanceMutationErrorV1>)->Result<Result<CommittedModuleProposalStateV1,ReservedValue<crate::WorldError>>,BodyMutationError>{match result{Ok(s)=>Ok(Ok(s)),Err(ModuleGovernanceMutationErrorV1::World(e))=>Ok(Err(e)),Err(ModuleGovernanceMutationErrorV1::Body(e))=>Err(e)}}
#[allow(clippy::too_many_arguments)]
pub(super) fn execute_current_module_governance_sequence_v1<M,P,V,W,D,E,C,PE,PC,CW,ME,F>(
 update:super::governance_plan::PreparedModuleManifestUpdateV1,
 selected:&SelectedPendingActionV1,outcome:&ReservedValue<RootRulePhaseOutcomeV1>,accepted:SelectedAcceptedDispositionV1,
 sources:&ModuleLifecycleSourcesV1<'_>,step:&mut IwsStepContext<'_, '_, '_, '_,M,V,W,D>,
 read:&mut WorldFieldReadContextV1<'_,P,V,E,C>,parent:&mut WorldFieldReadContextV1<'_,P,V,PE,PC>,
 write:&mut MemberRecordWriteContext<'_,W,CW>,materials:&mut CorePolicyMaterialContextV1<'_,M,ME>,
 module:&mut ModuleLifecycleWorkV1<'_>,factory:&mut FactoryScheduleWorkV1,common:&mut FormalBoundedContextV1<'_>,
 directory:&ReservedValue<AcceptedInvocationDirectoryV1>,max_event_bytes:usize,persist:&mut F,live:&ExecutionFence,
)->Result<CurrentModuleGovernanceSequenceV1,BodyMutationError>
where M:MerkleNodeReader,P:MerkleNodeReader,V:ValueNodeReader,W:ValueNodeReader+ValueNodeWriter+StatePageWriter,
 D:FnMut()->bool,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool,PE:FnMut()->Option<ValueTreeError>,PC:FnMut()->bool,CW:FnMut()->bool,ME:FnMut()->Option<ValueTreeError>,F:FnMut(&mut dyn AgentEventOriginalV1)->Result<(),BodyMutationError>{
 let attempt=std::panic::catch_unwind(std::panic::AssertUnwindSafe(||{
 accepted.validate(selected,step.transaction,common.memory())?;
 let effective=outcome.override_action_for_selected_v1(selected,step.transaction)?.unwrap_or(&selected.envelope().action);
 let actor=match effective{
 crate::Action::InstallModuleFromArtifact{installer_agent_id,..}|crate::Action::InstallModuleFromArtifactWithFinality{installer_agent_id,..}|crate::Action::InstallModuleToTargetFromArtifact{installer_agent_id,..}|crate::Action::InstallModuleToTargetFromArtifactWithFinality{installer_agent_id,..}=>installer_agent_id.as_str(),
 crate::Action::UpgradeModuleFromArtifact{upgrader_agent_id,..}|crate::Action::UpgradeModuleFromArtifactWithFinality{upgrader_agent_id,..}=>upgrader_agent_id.as_str(),
 crate::Action::RollbackModuleInstance{operator_agent_id,..}|crate::Action::RollbackModuleInstanceWithFinality{operator_agent_id,..}=>operator_agent_id.as_str(),
 _=>return Err(invalid()),};
 let now=step.time();
 let proposed=crate::world::incremental_step::factory_schedule_write_io::paid_with_module(step,read,parent,write,materials,factory,module,common,live,|step,read,parent,write,_,_,module,tail|{
 let mut ctx=WorldFieldOverlayContextV1{transaction:&mut*step.transaction,values:ValueReadContext{source:read.values.source,memory:read.values.memory,class:read.values.class,io:&mut*read.values.io},key_work:&mut*read.key_work,semantic_bytes:read.semantic_bytes,record_bytes:read.record_bytes,take_state_error:&mut read.take_state_error,cancelled:&mut read.cancelled};
 propose_current_module_update_v1(update,actor,sources,&mut ctx,parent,write,module,tail,now,live)
 })?;
 let proposal_id=proposed.after.original_id();
 let first=super::event::prepare_proposal_event_v1(selected,accepted.into_audit(),proposed,step,read,common,live)?;
 step.append_prepared_module_lifecycle_event_v1(&first,materials,common,live,max_event_bytes,&mut*persist)?;
 let mut last=None;
 for stage in 0..3{
 let proposal={let mut ctx=WorldFieldOverlayContextV1{transaction:&mut*step.transaction,values:ValueReadContext{source:read.values.source,memory:read.values.memory,class:read.values.class,io:&mut*read.values.io},key_work:&mut*read.key_work,semantic_bytes:read.semantic_bytes,record_bytes:read.record_bytes,take_state_error:&mut read.take_state_error,cancelled:&mut read.cancelled};
 crate::world::incremental_normal_action_factory_schedule::with_module_lifecycle_read_io_v1(sources.families.roots,&mut ctx,parent,factory,common,module,|ctx,parent,_,module|{let guard=ctx.transaction.guard();Ok(oasis7_world_state::incremental_runtime_proposal::read_current_runtime_proposal_v1(proposal_id,sources.proposals,sources.families.roots,parent.state,ctx,module.proposal_scan,module.proposal_read,&guard,live).map_err(overlay_error)?)})
 }.map_err(|e|match e{crate::world::incremental_normal_action_factory_schedule::FactoryScheduleReadErrorV1::Body(e)=>e,crate::world::incremental_normal_action_factory_schedule::FactoryScheduleReadErrorV1::World(_)=>invalid()})?;
 let mut proposal=Some(proposal);
 let shadow=if stage==0{
 let mut ctx=WorldFieldOverlayContextV1{transaction:&mut*step.transaction,values:ValueReadContext{source:read.values.source,memory:read.values.memory,class:read.values.class,io:&mut*read.values.io},key_work:&mut*read.key_work,semantic_bytes:read.semantic_bytes,record_bytes:read.record_bytes,take_state_error:&mut read.take_state_error,cancelled:&mut read.cancelled};
 match super::governance_shadow::prepare_current_module_shadow_v1(proposal.take().ok_or_else(invalid)?,sources,&mut ctx,parent,factory,module,common,directory,live).map_err(|e|match e{crate::world::incremental_normal_action_factory_schedule::FactoryScheduleReadErrorV1::Body(e)=>e,crate::world::incremental_normal_action_factory_schedule::FactoryScheduleReadErrorV1::World(_)=>invalid()})?{
 super::governance_shadow::CurrentModuleShadowDecisionV1::Ready(v)=>Some(v),
 super::governance_shadow::CurrentModuleShadowDecisionV1::Rejected{error,_facts}=>return Ok(CurrentModuleGovernanceSequenceV1::Rejected{first,last,error,_facts,stage:CurrentModuleGovernanceFailureStageV1::Shadow}),}
 }else{None};
 // The move is fixed per stage, not a caller-supplied mutation callback.
 let state=crate::world::incremental_step::factory_schedule_write_io::paid_with_module(step,read,parent,write,materials,factory,module,common,live,|step,read,parent,write,_,factory,module,tail|{
 let mut ctx=WorldFieldOverlayContextV1{transaction:&mut*step.transaction,values:ValueReadContext{source:read.values.source,memory:read.values.memory,class:read.values.class,io:&mut*read.values.io},key_work:&mut*read.key_work,semantic_bytes:read.semantic_bytes,record_bytes:read.record_bytes,take_state_error:&mut read.take_state_error,cancelled:&mut read.cancelled};
 if let Some(shadow)=shadow{return body_or_business(commit_validated_current_module_shadow_v1(shadow,sources,&mut ctx,parent,write,factory,module,tail,now,live))}
 body_or_business(transition_current_module_proposal_v1(proposal.take().ok_or_else(invalid)?,if stage==1{ModuleProposalStateStepV1::Approve{approver:actor}}else{ModuleProposalStateStepV1::Queue},sources,&mut ctx,parent,write,factory,module,tail,now,live))
 })?;
 let state=match state{Ok(s)=>s,Err(error)=>return Ok(CurrentModuleGovernanceSequenceV1::Rejected{first,last,error,_facts:None,stage:if stage==0{CurrentModuleGovernanceFailureStageV1::Shadow}else{CurrentModuleGovernanceFailureStageV1::Approve}})};
 let event=super::event::prepare_proposal_event_v1(selected,None,state,step,read,common,live)?;
 step.append_prepared_module_lifecycle_event_v1(&event,materials,common,live,max_event_bytes,&mut*persist)?;
 last=Some(event);
 }
 Ok(CurrentModuleGovernanceSequenceV1::Queued(ExecutedCurrentModuleGovernanceV1{first,last:last.ok_or_else(invalid)?,proposal_id}))
 }));
 match attempt{Ok(Ok(value))=>Ok(value),Ok(Err(error))=>{let _:Result<(),oasis7_formal_protocol::incremental_execution::ExecutionError>=step.transaction.contribute(|_|Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted));Err(error)},Err(panic)=>{let _:Result<(),oasis7_formal_protocol::incremental_execution::ExecutionError>=step.transaction.contribute(|_|Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted));std::panic::resume_unwind(panic)}}
}
