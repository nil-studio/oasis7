//! Real Current common header, shared by all five fixed lifecycle lanes.
use super::*;
use super::super::delivery::{self,IndustrialDeliverySourcesV1,IndustrialDeliveryWorkV1,IndustrialDeliveryErrorV1};
use crate::world::{incremental_body_event::materials::CorePolicyMaterialContextV1,incremental_industrial_eligibility::read_current_world_binding_v1};
use oasis7_formal_protocol::{controlled_authority::formal::bounded_context::FormalBoundedContextV1,incremental_state::{StateNamespaceV1,StatePageWriter},incremental_value::{ValueNodeReader,ValueNodeWriter,ValueReadContext,ValueTreeError}};
use oasis7_world_state::{incremental_field_overlay::WorldFieldOverlayContextV1,incremental_field_read::WorldFieldReadContextV1,incremental_field_member::MemberRecordWriteContext,incremental_world_rules_family::WorldRulesQualificationV1,incremental_runtime_governance::{RuntimeGovernanceQualificationV1,read_current_governance_epoch_length_v1},incremental_agent_subtree::{read_current_agent_presence_v1,CurrentAgentPresenceV1},incremental_world_rule_requests_current::read_current_world_rule_request_v1,incremental_world_rule_composite_family::read_current_agent_order_v1,incremental_world_native::recover_gameplay_policy_overlay_v1};
use oasis7_proto::memory_budget::MemoryClass;
use std::mem::size_of;
pub(in crate::world) struct LifecycleHeaderSourcesV1<'a,'b,'source>{
 pub rules:&'a ReservedValue<WorldRulesQualificationV1>,
 pub delivery:&'a IndustrialDeliverySourcesV1<'b,'source>,
 pub governance:&'a ReservedValue<RuntimeGovernanceQualificationV1>,
}
pub(in crate::world) struct PreparedRetainedLifecycleHeaderV1{
 pub(in crate::world::incremental_normal_world_rule_lifecycle) event:crate::DomainEvent,pub(in crate::world::incremental_normal_world_rule_lifecycle) operation:LifecycleOperationV1,
 pub(in crate::world::incremental_normal_world_rule_lifecycle) request:ReservedValue<oasis7_world_state::incremental_world_rule_requests_current::CurrentWorldRuleRequestV1>,
 pub(in crate::world::incremental_normal_world_rule_lifecycle) guard:PreparedGuard,pub(in crate::world::incremental_normal_world_rule_lifecycle) identity:WorkIdentityV1,pub(in crate::world::incremental_normal_world_rule_lifecycle) memory:MemoryBudget,pub(in crate::world::incremental_normal_world_rule_lifecycle) now:u64,pub(in crate::world::incremental_normal_world_rule_lifecycle) phase:MemoryReservation,
}
pub(in crate::world) enum LifecycleHeaderPreparationV1{Fresh(PreparedFreshLifecycleHeaderV1),Retained(PreparedRetainedLifecycleHeaderV1),Rejected(ReservedValue<crate::WorldError>)}
/// Every Source/Work is borrowed from the real Current stage. No input boolean
/// or caller-supplied epoch/root can mint the returned private fresh header.
#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn prepare_current_lifecycle_header_v1<M,P,V,R,W,E,C,PE,PC,CW,ME>(selected:&ReservedValue<super::super::work::LifecycleWorkDataV1>,sources:&LifecycleHeaderSourcesV1<'_,'_,'_>,state:&M,ctx:&mut WorldFieldOverlayContextV1<'_,'_,'_,M,R,E,C>,parent:&mut WorldFieldReadContextV1<'_,P,V,PE,PC>,write:&mut MemberRecordWriteContext<'_,W,CW>,materials:&mut CorePolicyMaterialContextV1<'_,M,ME>,work:&mut IndustrialDeliveryWorkV1,common:&mut FormalBoundedContextV1<'_>,now:u64,live:&ExecutionFence)->Result<LifecycleHeaderPreparationV1,IndustrialDeliveryErrorV1>
where M:MerkleNodeReader,P:MerkleNodeReader,V:ValueNodeReader,R:ValueNodeReader,W:ValueNodeReader+ValueNodeWriter+StatePageWriter,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool,PE:FnMut()->Option<ValueTreeError>,PC:FnMut()->bool,CW:FnMut()->bool,ME:FnMut()->Option<ValueTreeError>{
 delivery::paid_io::run(ctx,parent,write,materials,work,common,|ctx,parent,_write,_materials,work,tail|{
  let roots=sources.delivery.eligibility.roots;
  if !selected.uses_budget(tail.memory())||!sources.rules.uses_budget(tail.memory())||!sources.governance.uses_budget(tail.memory()){return Err(BodyMutationError::Invalid("lifecycle header source pool").into());}
  tail.charge(8192)?;
  let rejection=tail.memory().reserve(512+size_of::<ReservedValue<crate::WorldError>>(),MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
  let guard=ctx.transaction.guard();
  let epoch_length=read_current_governance_epoch_length_v1(sources.governance,roots,state,ctx,&mut work.fixed_scan,work.composite_read.max_tree_heap,&guard,live)?;
  let guard=ctx.transaction.guard();
  let binding=read_current_world_binding_v1(sources.delivery.eligibility.artifacts,ctx,parent,&mut work.eligibility,&guard,live)?;
  let bytes=selected.actor.len().checked_add(selected.request.len()).and_then(|n|n.checked_add(command_comparison_bytes_v1(&selected.command,&selected.command).ok()?)).and_then(|n|n.checked_add(binding.as_ref().map_or(0,String::len))).and_then(|n|n.checked_add(size_of::<PreparedFreshLifecycleHeaderV1>()+size_of::<PreparedRetainedLifecycleHeaderV1>())).ok_or(BodyMutationError::Invalid("lifecycle header clone layout"))?;
  tail.charge(bytes)?;let phase=tail.memory().reserve(bytes,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
  let event=crate::DomainEvent::WorldRuleCommitted{action_id:0,actor_id:selected.actor.clone(),request_id:selected.request.clone(),command:selected.command.clone(),governance_epoch:now/epoch_length.max(1),world_binding:binding.as_ref().cloned()};
  // This original pre-acquisition check precedes PreparedWorldRule::prepare,
  // including its retained-request no-op arm.
  if let WorldRuleCommandV1::DeliverAgentOrder{order_id}=&selected.command{
   match delivery::validate_current_lifecycle_delivery_acquisition_v1(order_id,sources.delivery,state,ctx,parent,work,tail,live){
    Ok(())=>{},Err(IndustrialDeliveryErrorV1::World(error))=>return Ok(LifecycleHeaderPreparationV1::Rejected(rejection.retain(error))),Err(error)=>return Err(error),
   }
  }
  tail.charge(selected.actor.len().checked_add(selected.request.len()).ok_or(BodyMutationError::Invalid("lifecycle IDs layout"))?)?;
  if let Err(error)=oasis7_world_state::world_rules::validate_id(&selected.actor).and_then(|_|oasis7_world_state::world_rules::validate_id(&selected.request)){return Ok(LifecycleHeaderPreparationV1::Rejected(rejection.retain(error)));}
  let guard=ctx.transaction.guard();
  if read_current_agent_presence_v1(&selected.actor,sources.delivery.eligibility.agents,roots,ctx,parent,&mut work.eligibility.pending_entries_remaining,&guard,live)?!=CurrentAgentPresenceV1::Existing{return Ok(LifecycleHeaderPreparationV1::Rejected(rejection.retain(crate::world_rules::invalid("actor_not_found"))));}
  let guard=ctx.transaction.guard();
  let request=read_current_world_rule_request_v1(&selected.actor,&selected.request,sources.rules.requests(),roots,state,ctx,&mut work.fixed_scan,&mut work.composite_read,&guard,live)?;
  match classify_current_request_v1(&request,selected,tail)?{
   LifecycleRequestDispositionV1::ExactRetry=>return Ok(LifecycleHeaderPreparationV1::Retained(PreparedRetainedLifecycleHeaderV1{event,operation:selected.operation,request,guard:ctx.transaction.guard(),identity:tail.identity(),memory:tail.memory().clone(),now,phase})),
   LifecycleRequestDispositionV1::Conflict(error)=>return Ok(LifecycleHeaderPreparationV1::Rejected(error)),
   LifecycleRequestDispositionV1::Fresh=>{},
  }
  let view=match &selected.command{WorldRuleCommandV1::AdvanceAgentOrder{order_id}|WorldRuleCommandV1::DeliverAgentOrder{order_id}=>Some(delivery::read_current_world_rule_order_view_v1(order_id,sources.delivery,state,ctx,parent,work,tail,live)?),_=>None};
  sources.delivery.eligibility.orders.validate_current(ctx,&ctx.transaction.guard(),live).map_err(crate::world::incremental_industrial_eligibility::IndustrialEligibilityErrorV1::from)?;
  let mut manufactured=None;
  for(id,index)in sources.delivery.eligibility.orders.iter_order_indices(){
   tail.charge(index.target_agent_id().len().min(selected.actor.len()))?;
   if index.target_agent_id()==selected.actor{
    let guard=ctx.transaction.guard();manufactured=Some(read_current_agent_order_v1(sources.delivery.eligibility.orders.source(),id,state,ctx,&mut work.fixed_scan,&mut work.composite_read,&guard,live)?.ok_or(BodyMutationError::Invalid("lifecycle selected scope order absent"))?);break;
   }
  }
  if let Some(order)=&manufactured{tail.charge(order.value().quote.world_binding.as_ref().map_or(0,String::len).checked_add(binding.as_ref().map_or(0,String::len)).ok_or(BodyMutationError::Invalid("lifecycle scope comparison"))?)?;}
  if let Err(error)=oasis7_world_state::world_rules::validate_manufactured_world_scope_from_order_v1(manufactured.as_ref().map(|order|order.value()),binding.as_ref().map(String::as_str)){return Ok(LifecycleHeaderPreparationV1::Rejected(rejection.retain(error)));}
  if let Some(error)=validate_fresh_command_size_v1(&selected.command,tail)?{return Ok(LifecycleHeaderPreparationV1::Rejected(error));}
  // Only Deliver lacks the five-lane system blocked-agent exception.
  if matches!(selected.command,WorldRuleCommandV1::DeliverAgentOrder{..}){
   let mut read=WorldFieldReadContextV1{root:roots.state_root(StateNamespaceV1::Business),state:parent.state,values:ValueReadContext{source:parent.values.source,memory:parent.values.memory,class:parent.values.class,io:&mut *parent.values.io},key_work:&mut *parent.key_work,semantic_bytes:parent.semantic_bytes,record_bytes:parent.record_bytes,take_state_error:&mut parent.take_state_error,cancelled:&mut parent.cancelled};
   let policy=recover_gameplay_policy_overlay_v1(ctx.transaction,live,&mut read,&mut work.eligibility.scan,&mut work.eligibility.typed)?;
   for id in &policy.value().value().blocked_agents{tail.charge(id.len().min(selected.actor.len()))?;if id==&selected.actor{return Ok(LifecycleHeaderPreparationV1::Rejected(rejection.retain(crate::world_rules::invalid("actor_blocked"))));}}
   if let Some(view)=&view{if view.virtual_blocks_actor_v1(&selected.actor,tail)?{return Ok(LifecycleHeaderPreparationV1::Rejected(rejection.retain(crate::world_rules::invalid("actor_blocked"))));}}
  }
  Ok(LifecycleHeaderPreparationV1::Fresh(PreparedFreshLifecycleHeaderV1{event,view,operation:selected.operation,request,guard:ctx.transaction.guard(),identity:tail.identity(),memory:tail.memory().clone(),now,phase}))
 })
}
