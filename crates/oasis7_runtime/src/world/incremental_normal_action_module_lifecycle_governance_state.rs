//! Real selected Current proposal transitions for the governed module stage.
//! This state evidence is DATA, not permission to publish a governance event.
use super::*;
use oasis7_formal_protocol::incremental_value::{ValueNodeReader,ValueNodeWriter};
use oasis7_world_state::{Proposal,ProposalStatus,ProposalDecision,
 incremental_runtime_proposal::{CurrentRuntimeProposalV1,read_current_runtime_proposal_v1,replace_current_runtime_proposal_v1},
 incremental_field_overlay::WorldFieldOverlayContextV1,incremental_field_read::WorldFieldReadContextV1,incremental_field_member::MemberRecordWriteContext};
use crate::world::incremental_normal_tail::NormalTailWorkV1;

pub(super) enum ModuleProposalStateStepV1<'a>{Shadow{manifest_hash:&'a str},Approve{approver:&'a str},Queue}
#[derive(Clone,Copy)]
pub(super) enum ModuleProposalStateKindV1{Proposed,Shadow,Approved,Queued}
pub(super) struct CommittedModuleProposalStateV1{
 pub(super) before:ReservedValue<CurrentRuntimeProposalV1>,pub(super) after:ReservedValue<CurrentRuntimeProposalV1>,
 pub(super) kind:ModuleProposalStateKindV1,guard:PreparedGuard,core:WorkIdentityV1,now:u64,_fee:MemoryReservation,
 pub(super) shadow_sources:Option<RetainedModuleShadowSourcesV1>,
 _manifest:Option<super::governance_manifest::CurrentModuleWorldManifestV1>,
 _update_fees:Option<(MemoryReservation,MemoryReservation)>,
}
pub(super) struct RetainedModuleShadowSourcesV1{
 pub(super) hash:ReservedValue<String>,pub(super) facts:super::instance_current::ModuleInstanceFactsFeesV1,
 pub(super) decoded:Option<crate::ModuleChangeSet>,pub(super) fee:MemoryReservation,
}
impl CommittedModuleProposalStateV1{
 pub(super) fn validate_evidence_v1<M:MerkleNodeReader>(&self,tx:&ExecutionTransaction<'_,M>,memory:&oasis7_proto::memory_budget::MemoryBudget,now:u64,core:WorkIdentityV1,live:&ExecutionFence)->Result<(),BodyMutationError>{
  tx.validate_transaction_identity(&self.guard)?;
  self.before.validate_provenance(tx,memory,live).map_err(overlay_error)?;
  self.after.validate_provenance(tx,memory,live).map_err(overlay_error)?;
  if self.shadow_sources.as_ref().is_some_and(|s|!s.hash.uses_budget(memory)||!s.fee.uses_budget(memory)){return Err(invalid())}
  if let Some(source)=&self._manifest{source.validate_evidence_v1(tx,memory,live)?;}
  if self._update_fees.as_ref().is_some_and(|(a,b)|!a.uses_budget(memory)||!b.uses_budget(memory)){return Err(invalid())}
  if self.core!=core||self.now!=now||!self._fee.uses_budget(memory)||!self.before.uses_budget(memory)||!self.after.uses_budget(memory){return Err(invalid())}Ok(())
 }
}
pub(super) enum ModuleGovernanceMutationErrorV1{Body(BodyMutationError),World(ReservedValue<crate::WorldError>)}
impl From<BodyMutationError> for ModuleGovernanceMutationErrorV1{fn from(e:BodyMutationError)->Self{Self::Body(e)}}
impl From<ValueTreeError> for ModuleGovernanceMutationErrorV1{fn from(e:ValueTreeError)->Self{Self::Body(e.into())}}

/// The exclusive caller supplies the *same* paid child used by the real state
/// writer. Clone admission uses the selected decoder's retained output graph,
/// rather than a maximum or a borrowed source's already-live reservation.
#[allow(clippy::too_many_arguments)]
pub(super) fn transition_current_module_proposal_v1<M,P,R,V,E,C,PE,PC,W,CW>(
 before:ReservedValue<CurrentRuntimeProposalV1>,step:ModuleProposalStateStepV1<'_>,sources:&ModuleLifecycleSourcesV1<'_>,
 ctx:&mut WorldFieldOverlayContextV1<'_, '_, '_,M,R,E,C>,parent:&mut WorldFieldReadContextV1<'_,P,V,PE,PC>,
 write:&mut MemberRecordWriteContext<'_,W,CW>,factory:&mut FactoryScheduleWorkV1,module:&mut ModuleLifecycleWorkV1<'_>,
 tail:&mut NormalTailWorkV1<'_, '_>,now:u64,live:&ExecutionFence,
)->Result<CommittedModuleProposalStateV1,ModuleGovernanceMutationErrorV1>
where M:MerkleNodeReader,P:MerkleNodeReader,R:ValueNodeReader,V:ValueNodeReader,W:ValueNodeReader+ValueNodeWriter+oasis7_formal_protocol::incremental_state::StatePageWriter,
 E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool,PE:FnMut()->Option<ValueTreeError>,PC:FnMut()->bool,CW:FnMut()->bool{
 before.validate_provenance(ctx.transaction,ctx.values.memory,live).map_err(overlay_error)?;
 let headers=before.output_reservation_count_v1()?;
 let header_bytes=headers.checked_mul(std::mem::size_of::<MemoryReservation>()).ok_or_else(invalid)?;
 tail.debit(u64::try_from(headers).map_err(|_|invalid())?,u64::try_from(header_bytes).map_err(|_|invalid())?)?;
 let heap=before.output_reserved_bytes_v1()?;
 let extra=match &step{ModuleProposalStateStepV1::Shadow{manifest_hash}=>manifest_hash.len(),ModuleProposalStateStepV1::Approve{approver}=>approver.len(),ModuleProposalStateStepV1::Queue=>0};
 let bound=heap.checked_add(extra.checked_mul(4).ok_or_else(invalid)?).and_then(|n|n.checked_add(8192)).ok_or_else(invalid)?;
 tail.charge(bound)?;
 let clone_fee=ctx.values.memory.reserve(bound,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
 let id=before.original_id();
 let Some(proposal)=before.value() else{return Err(ModuleGovernanceMutationErrorV1::World(clone_fee.retain(crate::WorldError::ProposalNotFound{proposal_id:id})))};
 let (kind,after):(_,Result<Proposal,crate::WorldError>)=match step{
  ModuleProposalStateStepV1::Shadow{manifest_hash}=>{
   let after=if !matches!(proposal.status,ProposalStatus::Proposed){Err(crate::WorldError::ProposalInvalidState{proposal_id:id,expected:"proposed".to_owned(),found:proposal.status.label()})}else{let mut after=proposal.clone();after.status=ProposalStatus::Shadowed{manifest_hash:manifest_hash.to_owned()};Ok(after)};
   (ModuleProposalStateKindV1::Shadow,after)
  },
  ModuleProposalStateStepV1::Approve{approver}=>(ModuleProposalStateKindV1::Approved,crate::world::event_processing::prepared_governance_events::prepare_approved_proposal(proposal,id,approver,&ProposalDecision::Approve)),
  ModuleProposalStateStepV1::Queue=>{
   let guard=ctx.transaction.guard();
   let policy=oasis7_world_state::incremental_runtime_governance::read_current_governance_execution_policy_v1(sources.governance,sources.families.roots,parent.state,ctx,module.proposal_scan,module.max_selected_heap,&guard,live).map_err(overlay_error)?;
   let hash=match &proposal.status{ProposalStatus::Approved{manifest_hash,..}=>manifest_hash.as_str(),_=>{
    return Err(ModuleGovernanceMutationErrorV1::World(clone_fee.retain(crate::WorldError::ProposalInvalidState{proposal_id:id,expected:"approved".into(),found:proposal.status.label()})))
   }};
   let epoch=now/policy.epoch_length_ticks.max(1);
   (ModuleProposalStateKindV1::Queued,crate::world::event_processing::prepared_governance_events::prepare_queued_proposal(proposal,id,hash,now,now.saturating_add(policy.timelock_ticks),epoch.saturating_add(policy.activation_delay_epochs),policy.timelock_ticks))
  },
 };
 let after=match after{Ok(value)=>value,Err(error)=>return Err(ModuleGovernanceMutationErrorV1::World(clone_fee.retain(error)))};
 let guard=ctx.transaction.guard();
 replace_current_runtime_proposal_v1(&before,&after,ctx,write,module.proposal_write,&guard,live).map_err(overlay_error)?;
 let guard=ctx.transaction.guard();
 let actual=read_current_runtime_proposal_v1(id,sources.proposals,sources.families.roots,parent.state,ctx,module.proposal_scan,module.proposal_read,&guard,live).map_err(overlay_error)?;
 let layout=ctx.values.memory.reserve(std::mem::size_of::<CommittedModuleProposalStateV1>(),MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
 // Reopen/closed recovery is from the actual after outer, never the old parent.
 // The fixed writer already compared and encoded exactly `after` above.
 if actual.value().is_none(){return Err(invalid().into())}
 drop(after);
 Ok(CommittedModuleProposalStateV1{before,after:actual,kind,guard:ctx.transaction.guard(),core:tail.identity(),now,_fee:layout,shadow_sources:None,_manifest:None,_update_fees:None})
}

/// Allocate the actual current Proposal sequence and create the exact proposed
/// row. The selected Manifest source and its simultaneous output graph remain
/// retained as historical input evidence; no old strict guard is refreshed.
#[allow(clippy::too_many_arguments)]
pub(super) fn propose_current_module_update_v1<M,P,R,V,E,C,PE,PC,W,CW>(
 update:super::governance_plan::PreparedModuleManifestUpdateV1,author:&str,
 sources:&ModuleLifecycleSourcesV1<'_>,ctx:&mut WorldFieldOverlayContextV1<'_, '_, '_,M,R,E,C>,
 parent:&mut WorldFieldReadContextV1<'_,P,V,PE,PC>,write:&mut MemberRecordWriteContext<'_,W,CW>,
 module:&mut ModuleLifecycleWorkV1<'_>,tail:&mut NormalTailWorkV1<'_, '_>,now:u64,live:&ExecutionFence,
)->Result<CommittedModuleProposalStateV1,BodyMutationError>
where M:MerkleNodeReader,P:MerkleNodeReader,R:ValueNodeReader,V:ValueNodeReader,
 W:ValueNodeReader+ValueNodeWriter+oasis7_formal_protocol::incremental_state::StatePageWriter,
 E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool,PE:FnMut()->Option<ValueTreeError>,PC:FnMut()->bool,CW:FnMut()->bool{
 update.before.validate_in_child_v1(ctx.transaction,tail,live)?;
 let bytes=std::mem::size_of::<Proposal>().checked_add(author.len()).and_then(|n|n.checked_add(update.base_hash.len())).and_then(|n|n.checked_add(4096)).ok_or_else(invalid)?;
 tail.charge(bytes)?;
 let fee=ctx.values.memory.reserve(bytes,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
 let allocator=crate::world::incremental_allocators::RuntimeAllocatorV1::Proposal;
 let expected={let mut mutation=crate::world::incremental_body::BodyMutationContext{sink:&mut*write.sink,memory:write.memory,io:&mut*write.io,cancelled:&mut write.cancelled};crate::World::read_iws_allocator(allocator,ctx.transaction,ctx.values.source,&mut mutation)?};
 let(id,_,_)=crate::World::preview_rolling_sequence_id(expected.next,expected.era);
 let guard=ctx.transaction.guard();
 let before=read_current_runtime_proposal_v1(id,sources.proposals,sources.families.roots,parent.state,ctx,module.proposal_scan,module.proposal_read,&guard,live).map_err(overlay_error)?;
 let super::governance_plan::PreparedModuleManifestUpdateV1{before:manifest_source,manifest,base_hash,clone_fee,changes_fee}=update;
 let proposal=Proposal{id,author:author.to_owned(),base_manifest_hash:(*base_hash).clone(),manifest,patch:None,queued_at_tick:None,not_before_tick:None,activate_epoch:None,timelock_ticks:0,status:ProposalStatus::Proposed};
 {let mut mutation=crate::world::incremental_body::BodyMutationContext{sink:&mut*write.sink,memory:write.memory,io:&mut*write.io,cancelled:&mut write.cancelled};let(allocated,_)=crate::World::contribute_iws_allocate(allocator,expected,ctx.transaction,ctx.values.source,&mut mutation)?;if allocated!=id{return Err(invalid())}}
 let guard=ctx.transaction.guard();
 replace_current_runtime_proposal_v1(&before,&proposal,ctx,write,module.proposal_write,&guard,live).map_err(overlay_error)?;
 let guard=ctx.transaction.guard();
 let after=read_current_runtime_proposal_v1(id,sources.proposals,sources.families.roots,parent.state,ctx,module.proposal_scan,module.proposal_read,&guard,live).map_err(overlay_error)?;
 if after.value().is_none(){return Err(invalid())}
 drop(proposal);
 Ok(CommittedModuleProposalStateV1{before,after,kind:ModuleProposalStateKindV1::Proposed,guard:ctx.transaction.guard(),core:tail.identity(),now,_fee:fee,shadow_sources:None,_manifest:Some(manifest_source),_update_fees:Some((clone_fee,changes_fee))})
}

#[allow(clippy::too_many_arguments)]
pub(super) fn commit_validated_current_module_shadow_v1<M,P,R,V,E,C,PE,PC,W,CW>(
 validated:super::governance_shadow::ValidatedCurrentModuleShadowV1,sources:&ModuleLifecycleSourcesV1<'_>,
 ctx:&mut WorldFieldOverlayContextV1<'_, '_, '_,M,R,E,C>,parent:&mut WorldFieldReadContextV1<'_,P,V,PE,PC>,
 write:&mut MemberRecordWriteContext<'_,W,CW>,factory:&mut FactoryScheduleWorkV1,module:&mut ModuleLifecycleWorkV1<'_>,
 tail:&mut NormalTailWorkV1<'_, '_>,now:u64,live:&ExecutionFence,
)->Result<CommittedModuleProposalStateV1,ModuleGovernanceMutationErrorV1>
where M:MerkleNodeReader,P:MerkleNodeReader,R:ValueNodeReader,V:ValueNodeReader,W:ValueNodeReader+ValueNodeWriter+oasis7_formal_protocol::incremental_state::StatePageWriter,
 E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool,PE:FnMut()->Option<ValueTreeError>,PC:FnMut()->bool,CW:FnMut()->bool{
 let super::governance_shadow::ValidatedCurrentModuleShadowV1{proposal,manifest_hash,facts,decoded,fee}=validated;
 let mut state=transition_current_module_proposal_v1(proposal,ModuleProposalStateStepV1::Shadow{manifest_hash:&manifest_hash},sources,ctx,parent,write,factory,module,tail,now,live)?;
 state.shadow_sources=Some(RetainedModuleShadowSourcesV1{hash:manifest_hash,facts,decoded,fee});Ok(state)
}
