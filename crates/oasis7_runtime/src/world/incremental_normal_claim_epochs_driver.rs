//! Sole fixed consumer of the two adjacent claim epoch phases. Each phase
//! persists its complete batch before routing; claim follow-ups reread Current.
use super::*;
use crate::world::{incremental_body_event::{AgentEventOriginalV1,materials::CorePolicyMaterialContextV1},incremental_module_invocation::{RootModuleExecutionContextV1,RootModuleExecutionErrorV1,AcceptedInvocationDirectoryV1,CurrentInvocationEnumerationWorkV1},incremental_normal_sources::BoundNormalTickParentV1,incremental_normal_tail::NormalTailWorkV1,incremental_normal_world_rule_lifecycle::delivery::{self,IndustrialDeliveryWorkV1,IndustrialDeliveryErrorV1},incremental_normal_industry_due::deferred::DueEventQueueV1,incremental_world_event_counter::WorldJournalRetentionV1};
use oasis7_formal_protocol::{incremental_material::OriginalObjectStreamReader,incremental_value::{ValueNodeReader,ValueNodeWriter,ValueReadContext,ValueTreeError},incremental_state::StatePageWriter};
use oasis7_world_state::{incremental_field_overlay::WorldFieldOverlayContextV1,incremental_field_read::WorldFieldReadContextV1,incremental_world_native::{IndustrialClaimDirectoryV1,recover_current_industrial_claim_snapshot_v1,industrial_claim_namespace_v1},incremental_main_token_policy_records::{MainTokenPolicyFamilyQualificationV1,RestrictedClaimGrantDirectoryV1,recover_current_restricted_claim_grant_snapshot_v1},incremental_runtime_governance::{RuntimeGovernanceQualificationV1,read_current_governance_epoch_length_v1},incremental_world_u64::*,RestrictedStarterClaimGrantStatus};
pub(in crate::world) struct NormalClaimEpochSourcesV1<'a>{
 pub state:ClaimEpochStateSourcesV1<'a>,pub policy:&'a ReservedValue<MainTokenPolicyFamilyQualificationV1>,pub governance:&'a ReservedValue<RuntimeGovernanceQualificationV1>,pub grants:&'a ReservedValue<RestrictedClaimGrantDirectoryV1>,pub claims:&'a ReservedValue<IndustrialClaimDirectoryV1>,pub retention:&'a ReservedValue<WorldJournalRetentionV1>,
}
#[derive(Debug)]struct ClaimEpochStageErrorV1(IndustrialDeliveryErrorV1);
impl std::fmt::Display for ClaimEpochStageErrorV1{fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result{write!(f,"claim epoch: {:?}",self.0)}}
impl std::error::Error for ClaimEpochStageErrorV1{}
fn stage_error<OE>(error:IndustrialDeliveryErrorV1)->RootModuleExecutionErrorV1<OE>{match error{IndustrialDeliveryErrorV1::Body(error)=>RootModuleExecutionErrorV1::Body(error),other=>RootModuleExecutionErrorV1::Body(BodyMutationError::Original(Box::new(ClaimEpochStageErrorV1(other))))}}
impl<'a,'t,'s,'v,M,R,E,C,W,F,O> RootModuleExecutionContextV1<'a,'t,'s,'v,M,R,E,C,W,F,O>
where M:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool,W:ValueNodeReader+ValueNodeWriter+StatePageWriter,F:FnMut(&mut dyn AgentEventOriginalV1)->Result<(),BodyMutationError>,O:OriginalObjectStreamReader{
 #[allow(clippy::too_many_arguments)]
 pub(in crate::world) fn execute_normal_claim_epochs_v1<P,CE,CC,PE,PC,ME>(&mut self,normal_parent:&BoundNormalTickParentV1<'_>,sources:&NormalClaimEpochSourcesV1<'_>,work:&mut IndustrialDeliveryWorkV1,materials:&mut CorePolicyMaterialContextV1<'_,M,ME>,state:&M,current:&mut WorldFieldReadContextV1<'_,P,R,CE,CC>,parent:&mut WorldFieldReadContextV1<'_,P,R,PE,PC>,normal:&mut NormalTailWorkV1<'_,'_>,directory:&ReservedValue<AcceptedInvocationDirectoryV1>,enumeration:&mut CurrentInvocationEnumerationWorkV1,live_owner:&mut dyn FnMut()->ExecutionFence)->Result<(),RootModuleExecutionErrorV1<O::Error>>
 where P:MerkleNodeReader,CE:FnMut()->Option<ValueTreeError>,CC:FnMut()->bool,PE:FnMut()->Option<ValueTreeError>,PC:FnMut()->bool,ME:FnMut()->Option<ValueTreeError>{
 let result=(||{
  normal.validate_parent(normal_parent).map_err(RootModuleExecutionErrorV1::Body)?;
  normal.charge(std::mem::size_of_val(sources)+8192).map_err(RootModuleExecutionErrorV1::Body)?;
  self.events.world_events.bind_journal_retention_v1(self.ctx.transaction,sources.retention).map_err(RootModuleExecutionErrorV1::Body)?;
  macro_rules! overlay{()=>{WorldFieldOverlayContextV1{transaction:&mut*self.ctx.transaction,values:ValueReadContext{source:current.values.source,memory:current.values.memory,class:current.values.class,io:&mut*current.values.io},key_work:&mut*current.key_work,semantic_bytes:current.semantic_bytes,record_bytes:current.record_bytes,take_state_error:&mut current.take_state_error,cancelled:&mut current.cancelled}}}
  // Snapshot the complete grant selector before any expiry state mutation.
  let(epoch,grants)={let live=live_owner();let mut ctx=overlay!();delivery::paid_io::run(&mut ctx,parent,self.members,materials,work,normal.common_mut().map_err(RootModuleExecutionErrorV1::Body)?,|ctx,_,_,_,work,tail|{
   tail.charge(8192)?;let guard=ctx.transaction.guard();let length=read_current_governance_epoch_length_v1(sources.governance,sources.state.roots,state,ctx,&mut work.fixed_scan,work.max_identity_bytes,&guard,&live)?;
   let epoch=self.now/length.max(1);let guard=ctx.transaction.guard();let snapshot=recover_current_restricted_claim_grant_snapshot_v1(sources.grants,sources.policy,sources.state.roots,state,ctx,&mut work.fixed_scan,&mut work.fixed_cells_remaining,work.max_identity_bytes as u64,&mut work.eligibility.pending_entries_remaining,&guard,&live)?;Ok((epoch,snapshot))
  }).map_err(stage_error)?};
  let mut expired=DueEventQueueV1::new(self.ctx.transaction,normal.common_mut().map_err(RootModuleExecutionErrorV1::Body)?,self.now,&live_owner()).map_err(RootModuleExecutionErrorV1::Body)?;
  for entry in grants.entries(){
   normal.charge(entry.outer_id().len()).map_err(RootModuleExecutionErrorV1::Body)?;
   if entry.status()!=RestrictedStarterClaimGrantStatus::Issued||epoch<entry.expires_at_epoch(){continue}
   let publication={let live=live_owner();let mut ctx=overlay!();let mutation=delivery::paid_io::run(&mut ctx,parent,self.members,materials,work,normal.common_mut().map_err(RootModuleExecutionErrorV1::Body)?,|ctx,_,write,materials,work,tail|grant::apply_current_grant_expiry_v1(entry.outer_id(),epoch,sources.policy,&sources.state,state,ctx,write,materials,work,tail,self.now,&live)).map_err(stage_error)?;
    mutation.map(|mutation|finalize_current_claim_epoch_v1(mutation,&mut ctx,normal.common_mut()?,&live)).transpose().map_err(RootModuleExecutionErrorV1::Body)?};
   if let Some(publication)=publication{let event=self.persist_due_claim_epoch_v1(publication,materials,normal.common_mut().map_err(RootModuleExecutionErrorV1::Body)?,live_owner)?;expired.push(event,self.ctx.transaction,normal.common_mut().map_err(RootModuleExecutionErrorV1::Body)?,self.now,&live_owner()).map_err(RootModuleExecutionErrorV1::Body)?;}
  }
  self.flush_due_events_v1(&mut expired,normal.common_mut().map_err(RootModuleExecutionErrorV1::Body)?,directory,enumeration,live_owner)?;
  // Grant event routes may change policy or claims: read the second phase's
  // epoch and marker only now, exactly as the original two-stage caller.
  let(length,epoch,claims)={let live=live_owner();let mut ctx=overlay!();delivery::paid_io::run(&mut ctx,parent,self.members,materials,work,normal.common_mut().map_err(RootModuleExecutionErrorV1::Body)?,|ctx,parent,_,_,work,tail|{
   tail.charge(8192)?;let guard=ctx.transaction.guard();let length=read_current_governance_epoch_length_v1(sources.governance,sources.state.roots,state,ctx,&mut work.fixed_scan,work.max_identity_bytes,&guard,&live)?.max(1);let epoch=self.now/length;
   let last=read_world_u64_overlay_v1(WorldU64FieldV1::AgentClaimLastProcessedEpoch,ctx)?;if epoch<=last{return Ok((length,epoch,None))}
   let mut read=WorldFieldReadContextV1{root:sources.state.roots.state_root(industrial_claim_namespace_v1()?),state:parent.state,values:ValueReadContext{source:parent.values.source,memory:parent.values.memory,class:parent.values.class,io:&mut*parent.values.io},key_work:&mut*parent.key_work,semantic_bytes:parent.semantic_bytes,record_bytes:parent.record_bytes,take_state_error:&mut parent.take_state_error,cancelled:&mut parent.cancelled};
   let guard=ctx.transaction.guard();let snapshot=recover_current_industrial_claim_snapshot_v1(sources.claims,sources.state.native,sources.state.roots,ctx.transaction,&mut read,&mut work.eligibility.scan,&mut work.eligibility.typed,&mut work.claims,&guard,&live)?;Ok((length,epoch,Some(snapshot)))
  }).map_err(stage_error)?};
  let Some(claims)=claims else{return Ok(())};
  let mut claims_events=DueEventQueueV1::new(self.ctx.transaction,normal.common_mut().map_err(RootModuleExecutionErrorV1::Body)?,self.now,&live_owner()).map_err(RootModuleExecutionErrorV1::Body)?;
  for entry in claims.entries(){
   let mut pass=claim::ClaimEpochPassV1::Initial;
   loop{
    let publication={let live=live_owner();let mut ctx=overlay!();let mutation=delivery::paid_io::run(&mut ctx,parent,self.members,materials,work,normal.common_mut().map_err(RootModuleExecutionErrorV1::Body)?,|ctx,parent,write,materials,work,tail|claim::apply_current_claim_epoch_v1(entry.target_agent_id(),pass,epoch,length,sources.policy,&sources.state,state,ctx,parent,write,materials,work,tail,self.now,&live)).map_err(stage_error)?;
     mutation.map(|mutation|finalize_current_claim_epoch_v1(mutation,&mut ctx,normal.common_mut()?,&live)).transpose().map_err(RootModuleExecutionErrorV1::Body)?};
    let Some(publication)=publication else{break};let upkeep=matches!(&publication.event().body,crate::WorldEventBody::Domain(crate::DomainEvent::AgentClaimUpkeepSettled{..}));
    let event=self.persist_due_claim_epoch_v1(publication,materials,normal.common_mut().map_err(RootModuleExecutionErrorV1::Body)?,live_owner)?;claims_events.push(event,self.ctx.transaction,normal.common_mut().map_err(RootModuleExecutionErrorV1::Body)?,self.now,&live_owner()).map_err(RootModuleExecutionErrorV1::Body)?;
    if matches!(pass,claim::ClaimEpochPassV1::Initial)&&upkeep{pass=claim::ClaimEpochPassV1::AfterUpkeep}else{break}
   }
  }
  {let live=live_owner();let mut ctx=overlay!();delivery::paid_io::run(&mut ctx,parent,self.members,materials,work,normal.common_mut().map_err(RootModuleExecutionErrorV1::Body)?,|ctx,_,write,_,_,tail|{tail.charge_record()?;let before=read_world_u64_overlay_v1(WorldU64FieldV1::AgentClaimLastProcessedEpoch,ctx)?;let guard=ctx.transaction.guard();change_world_u64_overlay_v1(WorldU64FieldV1::AgentClaimLastProcessedEpoch,before,epoch,ctx,write,&guard,&live)?;Ok(())}).map_err(stage_error)?;}
  self.flush_due_events_v1(&mut claims_events,normal.common_mut().map_err(RootModuleExecutionErrorV1::Body)?,directory,enumeration,live_owner)?;Ok(())
 })();
 if result.is_err(){let _:Result<(),oasis7_formal_protocol::incremental_execution::ExecutionError>=self.ctx.transaction.contribute(|_|Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted));}result
 }
}
