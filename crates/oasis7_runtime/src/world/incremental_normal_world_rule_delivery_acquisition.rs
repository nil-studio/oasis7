//! Original lifecycle acquisition preflight, before exact-receipt dispatch.
//! This returns DATA validation only; no business/event capability is issued.
use super::*;
pub(in crate::world) fn validate_current_lifecycle_delivery_acquisition_v1<M,P,V,R,E,C,PE,PC>(
 order_id:&str,sources:&IndustrialDeliverySourcesV1<'_,'_>,state:&M,ctx:&mut WorldFieldOverlayContextV1<'_,'_,'_,M,R,E,C>,parent:&mut WorldFieldReadContextV1<'_,P,V,PE,PC>,work:&mut IndustrialDeliveryWorkV1,tail:&mut NormalTailWorkV1<'_,'_>,live:&ExecutionFence,
)->Result<(),IndustrialDeliveryErrorV1>
where M:MerkleNodeReader,P:MerkleNodeReader,V:ValueNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool,PE:FnMut()->Option<ValueTreeError>,PC:FnMut()->bool {
 tail.charge_record()?;
 let guard=ctx.transaction.guard();sources.eligibility.orders.validate_current(ctx,&guard,live).map_err(IndustrialEligibilityErrorV1::from)?;
 let guard=ctx.transaction.guard();let order=oasis7_world_state::incremental_world_rule_composite_family::read_current_agent_order_v1(sources.eligibility.orders.source(),order_id,state,ctx,&mut work.eligibility.scan,&mut work.eligibility.typed,&guard,live)?;
 let Some(order)=order else{return Ok(())};if order.value().delivered_at.is_some(){return Ok(())}
 let target=&order.value().target_agent_id;
 use oasis7_world_state::incremental_world_number_maps::*;
 let previous=match read_world_number_map_overlay_v1(WorldNumberMapFieldV1::AgentClaimAcquisitionSequences,WorldNumberMapKeyRefV1::Text(target),ctx)?{None=>0,Some(WorldNumberV1::Unsigned(n))=>n,_=>return Err(invalid())};
 let sequence=previous.checked_add(1).ok_or_else(||denied("manufactured_identity_generation_overflow"))?;
 let guard=ctx.transaction.guard();let current=oasis7_world_state::incremental_runtime_revocation::read_current_revocation_agent_identity_v1(target,sources.eligibility.revocation,ctx,parent,&mut work.eligibility.scan,&mut work.eligibility.revocation,&guard,live)?;
 oasis7_world_state::incremental_runtime_revocation::resolve_manufactured_identity_generation_v1(current.identity(),current.acquisition_sequence(),sequence).ok_or_else(||denied("manufactured_identity_generation_overflow"))?;
 tail.charge_record()?;Ok(())
}
