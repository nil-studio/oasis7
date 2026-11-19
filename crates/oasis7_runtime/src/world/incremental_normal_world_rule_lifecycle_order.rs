//! Fixed Current AdvanceAgentOrder preparation. Refund-needed is a distinct
//! private state and cannot be passed to the no-refund committer.
use super::*;
use super::delivery::{self,IndustrialDeliverySourcesV1,IndustrialDeliveryWorkV1,IndustrialDeliveryErrorV1};
use crate::world::{incremental_body_event::materials::CorePolicyMaterialContextV1,incremental_industrial_eligibility::{read_current_world_rule_order_production_facts_v1,read_current_world_rule_order_refund_source_v1,validate_current_order_budgeted_v1,CurrentOrderRefundSourceV1}};
use oasis7_formal_protocol::{controlled_authority::formal::bounded_context::FormalBoundedContextV1,incremental_state::StatePageWriter,incremental_value::{ValueNodeReader,ValueNodeWriter,ValueTreeError}};
use oasis7_world_state::{incremental_field_overlay::WorldFieldOverlayContextV1,incremental_field_read::WorldFieldReadContextV1,incremental_field_member::MemberRecordWriteContext,incremental_world_rule_composite_family::read_current_agent_order_v1,incremental_runtime_revocation::RevocationRecoveredV1,world_rules::{AgentIndustrialOrderV1,IndustrialAdvanceEligibilityV1,IndustrialAdvanceOutcomeV1,advance_industrial_order_from_source_v1,industrial_advance_phase_v1,IndustrialAdvancePhaseV1,mark_industrial_order_terminated_v1}};
use oasis7_proto::memory_budget::MemoryClass;
use std::mem::size_of;
struct PreparedOrderAdvanceV1{
 event:crate::DomainEvent,key:String,before:RevocationRecoveredV1<AgentIndustrialOrderV1>,after:AgentIndustrialOrderV1,
 refund:Option<ReservedValue<CurrentOrderRefundSourceV1>>,
 guard:PreparedGuard,identity:WorkIdentityV1,memory:MemoryBudget,now:u64,_phase:MemoryReservation,
}
pub(in crate::world) struct ReadyOrderAdvanceV1(PreparedOrderAdvanceV1);
pub(in crate::world) struct RefundOrderAdvanceV1(PreparedOrderAdvanceV1);
pub(in crate::world) enum OrderAdvancePreparationV1{Ready(ReadyOrderAdvanceV1),Refund(RefundOrderAdvanceV1),Rejected(ReservedValue<crate::WorldError>)}
fn bad()->BodyMutationError{BodyMutationError::Invalid("Current order Advance source")}
#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn prepare_current_order_advance_v1<M,P,V,R,W,E,C,PE,PC,CW,ME>(header:&header::PreparedFreshLifecycleHeaderV1,sources:&IndustrialDeliverySourcesV1<'_,'_>,state:&M,ctx:&mut WorldFieldOverlayContextV1<'_,'_,'_,M,R,E,C>,parent:&mut WorldFieldReadContextV1<'_,P,V,PE,PC>,write:&mut MemberRecordWriteContext<'_,W,CW>,materials:&mut CorePolicyMaterialContextV1<'_,M,ME>,work:&mut IndustrialDeliveryWorkV1,common:&mut FormalBoundedContextV1<'_>,now:u64,live:&ExecutionFence)->Result<OrderAdvancePreparationV1,IndustrialDeliveryErrorV1>
where M:MerkleNodeReader,P:MerkleNodeReader,V:ValueNodeReader,R:ValueNodeReader,W:ValueNodeReader+ValueNodeWriter+StatePageWriter,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool,PE:FnMut()->Option<ValueTreeError>,PC:FnMut()->bool,CW:FnMut()->bool,ME:FnMut()->Option<ValueTreeError>{
 header.validate(ctx.transaction,common.memory(),now,common.work().identity(),live)?;
 let crate::DomainEvent::WorldRuleCommitted{actor_id,command:WorldRuleCommandV1::AdvanceAgentOrder{order_id},world_binding,..}=header.event()else{return Err(bad().into())};
 delivery::paid_io::run(ctx,parent,write,materials,work,common,|ctx,parent,_write,_materials,work,tail|{
  tail.charge(8192)?;
  let reject=tail.memory().reserve(512+size_of::<ReservedValue<crate::WorldError>>(),MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
  sources.eligibility.orders.validate_current(ctx,&ctx.transaction.guard(),live).map_err(crate::world::incremental_industrial_eligibility::IndustrialEligibilityErrorV1::from)?;
  let guard=ctx.transaction.guard();
  let Some(before)=read_current_agent_order_v1(sources.eligibility.orders.source(),order_id,state,ctx,&mut work.fixed_scan,&mut work.composite_read,&guard,live)?else{return Ok(OrderAdvancePreparationV1::Rejected(reject.retain(crate::world_rules::invalid("order_not_found"))))};
  let original=before.value();
  if let Err(error)=validate_current_order_budgeted_v1(original,&before,ctx,&mut work.eligibility)?{return Ok(OrderAdvancePreparationV1::Rejected(reject.retain(error)));}
  tail.charge(original.quote.world_binding.as_ref().map_or(0,String::len).checked_add(world_binding.as_ref().map_or(0,String::len)).and_then(|n|n.checked_add(actor_id.len())).and_then(|n|n.checked_add(original.owner_id.len())).ok_or_else(bad)?)?;
  if original.quote.world_binding!=*world_binding{return Ok(OrderAdvancePreparationV1::Rejected(reject.retain(crate::world_rules::invalid("manufacturing_world_binding_mismatch"))))}
  if original.owner_id!=*actor_id{return Ok(OrderAdvancePreparationV1::Rejected(reject.retain(crate::world_rules::invalid("order_owner_required"))))}
  let cloned=before.retained_allocation_bytes_v1(&mut work.eligibility.read.cells_remaining,&mut ctx.cancelled)?;
  let event_bytes=match header.event(){crate::DomainEvent::WorldRuleCommitted{actor_id,request_id,command:WorldRuleCommandV1::AdvanceAgentOrder{order_id},world_binding,..}=>actor_id.len().checked_add(request_id.len()).and_then(|n|n.checked_add(order_id.len())).and_then(|n|n.checked_add(world_binding.as_ref().map_or(0,String::len))).ok_or_else(bad)?,_=>return Err(bad().into())};
  let bytes=cloned.checked_add(order_id.len()).and_then(|n|n.checked_add(event_bytes)).and_then(|n|n.checked_add(size_of::<PreparedOrderAdvanceV1>()+1024)).ok_or_else(bad)?;
  tail.charge(bytes)?;let phase=tail.memory().reserve(bytes,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
  let mut after=original.clone();let mut refund=None;
  if original.is_terminal(){
   if !original.refund_pending{return Ok(OrderAdvancePreparationV1::Rejected(reject.retain(crate::world_rules::invalid("order_already_terminal"))))}
   let guard=ctx.transaction.guard();let source=read_current_world_rule_order_refund_source_v1(header.world_rule_order_view().ok_or_else(bad)?,&sources.eligibility,order_id,state,ctx,parent,&mut work.eligibility,tail,&guard,live)?;
   source.validate(ctx.transaction,tail.memory(),live)?;tail.charge(source.order_id().len().min(order_id.len()))?;
   if source.order_id()!=order_id{return Err(bad().into());}
   if !source.is_current(){return Ok(OrderAdvancePreparationV1::Rejected(reject.retain(crate::world_rules::invalid("refund_original_source_recovery_required"))))}
   refund=Some(source);
  }else{
   let outcome=match industrial_advance_phase_v1(original,now){
    Err(error)=>return Ok(OrderAdvancePreparationV1::Rejected(reject.retain(error))),
    Ok(IndustrialAdvancePhaseV1::Terminate(reason))=>IndustrialAdvanceOutcomeV1::Terminate(reason),
    Ok(_)=>{
     let guard=ctx.transaction.guard();let facts=read_current_world_rule_order_production_facts_v1(header.world_rule_order_view().ok_or_else(bad)?,&sources.eligibility,order_id,state,ctx,parent,&mut work.eligibility,tail,&guard,live)?;
     facts.validate(ctx.transaction,tail.memory(),live)?;tail.charge(facts.order_id().len().min(order_id.len()))?;
     if facts.order_id()!=order_id{return Err(bad().into());}
     match advance_industrial_order_from_source_v1(&mut after,now,IndustrialAdvanceEligibilityV1::CurrentFacts(facts.facts())){
      Ok(outcome)=>outcome,Err(error)=>return Ok(OrderAdvancePreparationV1::Rejected(reject.retain(error))),
     }
    },
   };
   if let IndustrialAdvanceOutcomeV1::Terminate(reason)=outcome{
    mark_industrial_order_terminated_v1(&mut after,reason);
    let guard=ctx.transaction.guard();let source=read_current_world_rule_order_refund_source_v1(header.world_rule_order_view().ok_or_else(bad)?,&sources.eligibility,order_id,state,ctx,parent,&mut work.eligibility,tail,&guard,live)?;
    source.validate(ctx.transaction,tail.memory(),live)?;tail.charge(source.order_id().len().min(order_id.len()))?;
    if source.order_id()!=order_id{return Err(bad().into());}
    if source.is_current(){refund=Some(source);}
   }
  }
  let plan=PreparedOrderAdvanceV1{event:header.event().clone(),key:order_id.clone(),before,after,refund,guard:ctx.transaction.guard(),identity:tail.identity(),memory:tail.memory().clone(),now,_phase:phase};
  if plan.refund.is_some(){Ok(OrderAdvancePreparationV1::Refund(RefundOrderAdvanceV1(plan)))}else{Ok(OrderAdvancePreparationV1::Ready(ReadyOrderAdvanceV1(plan)))}
 })
}

#[path="incremental_normal_world_rule_lifecycle_order_refund.rs"]
mod refund;

#[path="incremental_normal_world_rule_lifecycle_order_commit.rs"]
mod commit;

pub(super) use refund::{CurrentOrderRefundOutcomeV1,refund_current_order_advance_v1};
pub(super) use commit::{CommittedCurrentOrderAdvanceV1,commit_current_order_advance_v1};
