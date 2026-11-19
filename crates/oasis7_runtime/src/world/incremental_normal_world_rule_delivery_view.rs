//! Genuine original temporary WorldRule policy view. Only an actual Current
//! order and actual Runtime identity can derive its private virtual mark.
use super::*;
use crate::world::{incremental_normal_tail::NormalTailWorkV1,incremental_normal_world_rule_lifecycle::delivery::{IndustrialDeliverySourcesV1,IndustrialDeliveryWorkV1,IndustrialDeliveryErrorV1}};
use oasis7_formal_protocol::controlled_authority::formal::bounded_context::WorkIdentityV1;
use oasis7_proto::memory_budget::MemoryBudget;
pub(in crate::world) struct CurrentWorldRuleOrderViewV1 {
 order_id:String,order:Option<oasis7_world_state::incremental_runtime_revocation::RevocationRecoveredV1<oasis7_world_state::world_rules::AgentIndustrialOrderV1>>,
 identity:Option<ReservedValue<Option<String>>>,virtual_blocked:bool,guard:PreparedGuard,memory:MemoryBudget,core:WorkIdentityV1,
}
impl CurrentWorldRuleOrderViewV1 {
 pub(in crate::world) fn order_id(&self)->&str{&self.order_id}
 pub(in crate::world) fn virtual_blocks_actor_v1(&self,actor:&str,tail:&mut NormalTailWorkV1<'_,'_>)->Result<bool,IndustrialDeliveryErrorV1>{
  if self.core!=tail.identity()||!self.memory.same_pool(tail.memory()){return Err(ValueTreeError::Invalid("world-rule order view Core/pool").into());}
  let Some(order)=&self.order else{return Ok(false)};tail.charge(order.value().owner_id.len().min(actor.len()))?;Ok(self.virtual_blocked&&order.value().owner_id==actor)
 }
}
pub(in crate::world) fn read_current_world_rule_order_view_v1<M,P,V,R,E,C,PE,PC>(id:&str,sources:&IndustrialDeliverySourcesV1<'_,'_>,state:&M,ctx:&mut WorldFieldOverlayContextV1<'_,'_,'_,M,R,E,C>,parent:&mut WorldFieldReadContextV1<'_,P,V,PE,PC>,work:&mut IndustrialDeliveryWorkV1,tail:&mut NormalTailWorkV1<'_,'_>,live:&ExecutionFence)->Result<ReservedValue<CurrentWorldRuleOrderViewV1>,IndustrialDeliveryErrorV1>
where M:MerkleNodeReader,P:MerkleNodeReader,V:ValueNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool,PE:FnMut()->Option<ValueTreeError>,PC:FnMut()->bool {
 let result=(||{
  tail.charge(id.len())?;let fee=tail.reserve(std::mem::size_of::<CurrentWorldRuleOrderViewV1>().checked_add(id.len()).ok_or(ValueTreeError::ResourceLimited("world-rule order view"))?,MemoryClass::Ordinary)?;
  let guard=ctx.transaction.guard();sources.eligibility.orders.validate_current(ctx,&guard,live).map_err(IndustrialEligibilityErrorV1::from)?;
  let guard=ctx.transaction.guard();let order=read_current_agent_order_v1(sources.eligibility.orders.source(),id,state,ctx,&mut work.eligibility.scan,&mut work.eligibility.typed,&guard,live)?;
  let identity=if let Some(order)=&order {let guard=ctx.transaction.guard();Some(super::incremental_industrial_eligibility_owner::read_current_industrial_identity_owner_v1(&sources.eligibility,&order.value().owner_id,state,ctx,parent,&mut work.eligibility,&guard,live)?)}else{None};
  let virtual_blocked=if let (Some(order),Some(identity))=(&order,&identity){if let Some(owner)=identity.as_deref(){tail.charge(owner.len().min(order.value().quote.owner_player_id.len()))?;owner!=order.value().quote.owner_player_id}else{false}}else{false};
  let guard=ctx.transaction.guard();ctx.transaction.validate_guard(&guard,live)?;
  Ok(fee.retain(CurrentWorldRuleOrderViewV1 {order_id:id.to_owned(),order,identity,virtual_blocked,guard,memory:tail.memory().clone(),core:tail.identity()}))
 })();if result.is_err(){let _:Result<(),ExecutionError>=ctx.transaction.contribute(|_|Err(ExecutionError::Aborted));}result
}
/// Validate only this original policy view's dependencies. Other order fields
/// are irrelevant to the virtual mark and are validated by the actual reducer.
/// This deliberately does not turn arbitrary read-set revision inheritance
/// into authorization; actual identity is reopened and compared on every use.
pub(super) fn validate_current_view<M,P,V,R,E,C,PE,PC>(view:&ReservedValue<CurrentWorldRuleOrderViewV1>,id:&str,order:&oasis7_world_state::world_rules::AgentIndustrialOrderV1,sources:&IndustrialEligibilitySourcesV1<'_,'_>,state:&M,ctx:&mut WorldFieldOverlayContextV1<'_,'_,'_,M,R,E,C>,parent:&mut WorldFieldReadContextV1<'_,P,V,PE,PC>,work:&mut IndustrialEligibilityWorkV1,tail:&mut NormalTailWorkV1<'_,'_>,live:&ExecutionFence)->Result<bool,IndustrialDeliveryErrorV1>
where M:MerkleNodeReader,P:MerkleNodeReader,V:ValueNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool,PE:FnMut()->Option<ValueTreeError>,PC:FnMut()->bool {
 ctx.transaction.validate_transaction_identity(&view.guard)?;
 if !view.uses_budget(tail.memory())||!view.memory.same_pool(tail.memory())||view.core!=tail.identity()||!ctx.transaction.uses_budget(tail.memory()){return Err(ValueTreeError::Invalid("world-rule policy view scope").into());}
 let old=view.order.as_ref().ok_or(ValueTreeError::Invalid("world-rule view order missing"))?.value();
 tail.charge(view.order_id.len().min(id.len()).checked_add(old.owner_id.len().min(order.owner_id.len())).and_then(|n|n.checked_add(old.quote.owner_player_id.len().min(order.quote.owner_player_id.len()))).ok_or(ValueTreeError::ResourceLimited("world-rule view comparisons"))?)?;
 if view.order_id!=id||old.owner_id!=order.owner_id||old.quote.owner_player_id!=order.quote.owner_player_id{return Err(ValueTreeError::Invalid("world-rule view actual order dependency" ).into());}
 let guard=ctx.transaction.guard();let identity=super::incremental_industrial_eligibility_owner::read_current_industrial_identity_owner_v1(sources,&order.owner_id,state,ctx,parent,work,&guard,live)?;
 let before=view.identity.as_ref().and_then(|v|v.as_deref());let after=identity.as_deref();tail.charge(match(before,after){(Some(a),Some(b))=>a.len().min(b.len()),_=>0})?;
 if before!=after{return Err(ValueTreeError::Invalid("world-rule view actual identity dependency").into());}
 ctx.transaction.validate_guard(&ctx.transaction.guard(),live)?;Ok(view.virtual_blocked)
}
