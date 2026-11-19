//! Actual original selected order replacement after complete phase/refund.
use super::*;
use crate::world::incremental_normal_tail::NormalTailWorkV1;
use oasis7_world_state::{incremental_world_rule_receipts::AgentOrdersOverlayV1,incremental_world_rule_composite_family::replace_current_industrial_order_v1};
pub(in crate::world) struct CommittedCurrentOrderAdvanceV1{
 source:PreparedOrderAdvanceV1,orders:ReservedValue<AgentOrdersOverlayV1>,guard:PreparedGuard,
}
impl CommittedCurrentOrderAdvanceV1{
 pub(in crate::world) fn result_id(&self)->&str{&self.source.key}
 pub(in crate::world) fn orders_overlay(&self)->&ReservedValue<AgentOrdersOverlayV1>{&self.orders}
 pub(in crate::world) fn validate<M:MerkleNodeReader>(&self,header:&header::PreparedFreshLifecycleHeaderV1,tx:&ExecutionTransaction<'_,M>,now:u64,tail:&mut NormalTailWorkV1<'_,'_>,live:&ExecutionFence)->Result<(),BodyMutationError>{
  tx.validate_guard(&self.guard,live)?;
  if self.source.identity!=tail.identity()||!self.source.memory.same_pool(tail.memory())||!self.orders.uses_budget(tail.memory())||self.source.now!=now{return Err(bad());}
  compare_header(&self.source.event,header.event(),tail)
 }
}
fn compare_header(before:&crate::DomainEvent,after:&crate::DomainEvent,tail:&mut NormalTailWorkV1<'_,'_>)->Result<(),BodyMutationError>{
 let(crate::DomainEvent::WorldRuleCommitted{action_id:a,actor_id:aa,request_id:ar,command:WorldRuleCommandV1::AdvanceAgentOrder{order_id:ao},governance_epoch:ae,world_binding:ab},crate::DomainEvent::WorldRuleCommitted{action_id:b,actor_id:ba,request_id:br,command:WorldRuleCommandV1::AdvanceAgentOrder{order_id:bo},governance_epoch:be,world_binding:bb})=(before,after)else{return Err(bad())};
 let compared=aa.len().min(ba.len()).checked_add(ar.len().min(br.len())).and_then(|n|n.checked_add(ao.len().min(bo.len()))).and_then(|n|n.checked_add(match(ab,bb){(Some(a),Some(b))=>a.len().min(b.len()),_=>0})).ok_or_else(bad)?;
 tail.charge(compared)?;
 if a!=b||aa!=ba||ar!=br||ao!=bo||ae!=be||ab!=bb{return Err(bad());}Ok(())
}
#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn commit_current_order_advance_v1<M,P,V,R,W,E,C,PE,PC,CW,ME>(source:ReadyOrderAdvanceV1,header:&header::PreparedFreshLifecycleHeaderV1,sources:&IndustrialDeliverySourcesV1<'_,'_>,state:&M,ctx:&mut WorldFieldOverlayContextV1<'_,'_,'_,M,R,E,C>,parent:&mut WorldFieldReadContextV1<'_,P,V,PE,PC>,write:&mut MemberRecordWriteContext<'_,W,CW>,materials:&mut CorePolicyMaterialContextV1<'_,M,ME>,work:&mut IndustrialDeliveryWorkV1,common:&mut FormalBoundedContextV1<'_>,now:u64,live:&ExecutionFence)->Result<CommittedCurrentOrderAdvanceV1,IndustrialDeliveryErrorV1>
where M:MerkleNodeReader,P:MerkleNodeReader,V:ValueNodeReader,R:ValueNodeReader,W:ValueNodeReader+ValueNodeWriter+StatePageWriter,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool,PE:FnMut()->Option<ValueTreeError>,PC:FnMut()->bool,CW:FnMut()->bool,ME:FnMut()->Option<ValueTreeError>{
 let source=source.0;
 delivery::paid_io::run(ctx,parent,write,materials,work,common,|ctx,_parent,write,_materials,work,tail|{
  ctx.transaction.validate_guard(&source.guard,live)?;
  if source.identity!=tail.identity()||!source.memory.same_pool(tail.memory())||source.now!=now||source.refund.is_some(){return Err(bad().into());}
  compare_header(&source.event,header.event(),tail)?;tail.charge(8192)?;
  let guard=ctx.transaction.guard();
  let orders=replace_current_industrial_order_v1(sources.orders_parent,sources.eligibility.orders.source(),sources.eligibility.roots,&source.key,&source.after,state,ctx,write,&mut work.fixed_scan,&mut work.composite_read,&mut work.composite_write,&guard,live)?;
  Ok(CommittedCurrentOrderAdvanceV1{source,orders,guard:ctx.transaction.guard()})
 })
}
