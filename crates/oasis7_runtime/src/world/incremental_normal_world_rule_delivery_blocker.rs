//! Exact World::industrial_order_blocker for the initial readonly lifecycle
//! snapshot. No acquisition/funding/write operation or permission is produced.
use super::*;
use crate::world::{incremental_normal_tail::NormalTailWorkV1,incremental_normal_world_rule_lifecycle::delivery::{IndustrialDeliverySourcesV1,IndustrialDeliveryWorkV1,IndustrialDeliveryErrorV1,validate_current_industrial_capacity_v1}};
pub(in crate::world) fn current_lifecycle_order_delivery_blocker_v1<M,P,V,R,E,C,PE,PC>(id:&str,sources:&IndustrialDeliverySourcesV1<'_,'_>,state:&M,ctx:&mut WorldFieldOverlayContextV1<'_,'_,'_,M,R,E,C>,parent:&mut WorldFieldReadContextV1<'_,P,V,PE,PC>,work:&mut IndustrialDeliveryWorkV1,tail:&mut NormalTailWorkV1<'_,'_>,now:u64,live:&ExecutionFence)->Result<ReservedValue<CurrentIndustrialBlockerV1>,IndustrialDeliveryErrorV1>
where M:MerkleNodeReader,P:MerkleNodeReader,V:ValueNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool,PE:FnMut()->Option<ValueTreeError>,PC:FnMut()->bool {
 use super::incremental_industrial_eligibility_owner::{read_current_industrial_world_binding_v1,read_current_industrial_identity_owner_v1};
 let result=(||{
  tail.charge_record()?;let guard=ctx.transaction.guard();sources.eligibility.orders.validate_current(ctx,&guard,live).map_err(IndustrialEligibilityErrorV1::from)?;
  let guard=ctx.transaction.guard();let owned=read_current_agent_order_v1(sources.eligibility.orders.source(),id,state,ctx,&mut work.eligibility.scan,&mut work.eligibility.typed,&guard,live)?.ok_or(ValueTreeError::Invalid("initial Current order directory member missing"))?;
  let order=owned.value();
  if order.phase==AgentIndustrialPhaseV1::Producing {let guard=ctx.transaction.guard();return current_producing_order_blocker_v1(&sources.eligibility,id,now,state,ctx,parent,&mut work.eligibility,&guard,live).map_err(Into::into);}
  let cap=usize::try_from(work.eligibility.read.max_text_bytes).map_err(|_|ValueTreeError::ResourceLimited("blocker output address"))?;
  let fee=tail.reserve(cap.checked_mul(2).and_then(|n|n.checked_add(std::mem::size_of::<CurrentIndustrialBlockerV1>()+4096)).ok_or(ValueTreeError::ResourceLimited("blocker output admission"))?,MemoryClass::Ordinary)?;
  if let Err(error)=validate_current_order_budgeted_v1(order,&owned,ctx,&mut work.eligibility)? {return Ok(fee.retain(CurrentIndustrialBlockerV1 {blocker:Some(bounded_debug(&error,cap)?)}));}
  let guard=ctx.transaction.guard();let binding=read_current_industrial_world_binding_v1(&sources.eligibility,state,ctx,parent,&mut work.eligibility,&guard,live)?;
  tail.charge(match(order.quote.world_binding.as_deref(),binding.as_deref()){(Some(a),Some(b))=>a.len().min(b.len()),_=>0})?;
  if order.quote.world_binding.as_deref()!=binding.as_deref(){return Ok(fee.retain(CurrentIndustrialBlockerV1 {blocker:Some(bounded_literal("manufacturing_world_binding_mismatch",cap)?)}));}
  let guard=ctx.transaction.guard();let identity=read_current_industrial_identity_owner_v1(&sources.eligibility,&order.owner_id,state,ctx,parent,&mut work.eligibility,&guard,live)?;
  if let Some(owner)=identity.as_deref(){tail.charge(owner.len().min(order.quote.owner_player_id.len()))?;if owner!=order.quote.owner_player_id{return Ok(fee.retain(CurrentIndustrialBlockerV1 {blocker:Some(bounded_literal("manufacturing_owner_identity_changed",cap)?)}));}}
  let refund=if order.refund_pending {let guard=ctx.transaction.guard();Some(read_current_order_refund_source_v1(&sources.eligibility,id,state,ctx,parent,&mut work.eligibility,&guard,live)?)}else{None};
  if let Some(reason)=order.delivery_prefix_after_validation_v1(now,Ok(()),refund.as_ref().map(|s|s.is_current())){tail.charge(reason.len())?;return Ok(fee.retain(CurrentIndustrialBlockerV1 {blocker:Some(reason)}));}
  let observed=delivery_facts::from_order(&sources.eligibility,id,order,state,ctx,parent,&mut work.eligibility,live)?;
  if let Err(error)=order_production_eligibility_from_facts_v1(observed.facts(),order){let reason=format!("restore_production_authority:{error:?}");tail.charge(reason.len())?;if reason.len()>cap{return Err(ValueTreeError::ResourceLimited("blocker reason output").into());}return Ok(fee.retain(CurrentIndustrialBlockerV1 {blocker:Some(reason)}));}
  match validate_current_industrial_capacity_v1(order,sources,ctx,parent,work,observed.facts().primary,tail,live){
   Err(IndustrialDeliveryErrorV1::World(error))=>{let reason=format!("restore_acquisition_capacity:{error:?}");tail.charge(reason.len())?;if reason.len()>cap{return Err(ValueTreeError::ResourceLimited("blocker reason output").into());}Ok(fee.retain(CurrentIndustrialBlockerV1 {blocker:Some(reason)}))},
   Err(error)=>Err(error),Ok(())=>Ok(fee.retain(CurrentIndustrialBlockerV1 {blocker:None})),
  }
 })();
 if result.is_err(){let _:Result<(),ExecutionError>=ctx.transaction.contribute(|_|Err(ExecutionError::Aborted));}result
}
