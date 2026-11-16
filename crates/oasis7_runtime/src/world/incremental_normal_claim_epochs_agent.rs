//! Actual Current Agent facts and sparse activity CAS for epoch reducers.
use super::*;
use crate::world::{incremental_normal_tail::NormalTailWorkV1,incremental_normal_world_rule_lifecycle::delivery::{IndustrialDeliveryWorkV1,IndustrialDeliveryErrorV1},incremental_body::BodyMutationContext};
use oasis7_formal_protocol::{incremental_value::{ValueNodeReader,ValueNodeWriter,ValueTreeError},incremental_state::{StateNamespaceV1,StatePageWriter}};
use oasis7_world_state::{incremental_field_overlay::WorldFieldOverlayContextV1,incremental_field_read::WorldFieldReadContextV1,incremental_field_member::MemberRecordWriteContext,incremental_agent_subtree::{read_current_agent_presence_v1,CurrentAgentPresenceV1},incremental_agent_body::{AgentBodyKeyV1,AgentBodyValueV1,AGENT_BODY_VALUE_BYTES}};
#[allow(clippy::too_many_arguments)]
pub(super) fn last_active<M:MerkleNodeReader,P:MerkleNodeReader,V:ValueNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool,PE:FnMut()->Option<ValueTreeError>,PC:FnMut()->bool>(actor:&str,sources:&ClaimEpochStateSourcesV1<'_>,ctx:&mut WorldFieldOverlayContextV1<'_,'_,'_,M,R,E,C>,parent:&mut WorldFieldReadContextV1<'_,P,V,PE,PC>,work:&mut IndustrialDeliveryWorkV1,tail:&mut NormalTailWorkV1<'_,'_>,live:&ExecutionFence)->Result<Option<u64>,IndustrialDeliveryErrorV1>{
 tail.charge(actor.len().checked_add(4096).ok_or(BodyMutationError::Invalid("claim agent metadata"))?)?;
 let guard=ctx.transaction.guard();if read_current_agent_presence_v1(actor,sources.agents,sources.roots,ctx,parent,&mut work.eligibility.pending_entries_remaining,&guard,live)?==CurrentAgentPresenceV1::Absent{return Ok(None)}
 let mut discard=crate::world::incremental_body::Discard;
 let mut read=BodyMutationContext{sink:&mut discard,memory:ctx.values.memory,io:&mut*ctx.values.io,cancelled:&mut ctx.cancelled};
 let key=AgentBodyKeyV1::new(actor).map_err(BodyMutationError::Invalid)?;
 let body=AgentBodyValueV1::from_canonical_bytes(&crate::world::incremental_body_event::read_fixed::<AGENT_BODY_VALUE_BYTES,_,_,_,_>(ctx.transaction,StateNamespaceV1::Business,key.as_bytes(),ctx.values.source,&mut read)?).map_err(BodyMutationError::Invalid)?;
 Ok(Some(body.last_active))
}
#[allow(clippy::too_many_arguments)]
pub(super) fn write_last_active<M:MerkleNodeReader,R:ValueNodeReader,W:ValueNodeReader+ValueNodeWriter+StatePageWriter,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool,CW:FnMut()->bool>(actor:&str,before:u64,after:u64,sources:&ClaimEpochStateSourcesV1<'_>,ctx:&mut WorldFieldOverlayContextV1<'_,'_,'_,M,R,E,C>,write:&mut MemberRecordWriteContext<'_,W,CW>,work:&mut IndustrialDeliveryWorkV1,tail:&mut NormalTailWorkV1<'_,'_>,live:&ExecutionFence)->Result<(),IndustrialDeliveryErrorV1>{
 tail.charge(actor.len().checked_add(2*AGENT_BODY_VALUE_BYTES).ok_or(BodyMutationError::Invalid("claim agent activity layout"))?)?;
 let mut mutation=BodyMutationContext{sink:&mut*write.sink,memory:write.memory,io:&mut*write.io,cancelled:&mut write.cancelled};
 let key=AgentBodyKeyV1::new(actor).map_err(BodyMutationError::Invalid)?;
 let body=AgentBodyValueV1::from_canonical_bytes(&crate::world::incremental_body_event::read_fixed::<AGENT_BODY_VALUE_BYTES,_,_,_,_>(ctx.transaction,StateNamespaceV1::Business,key.as_bytes(),ctx.values.source,&mut mutation)?).map_err(BodyMutationError::Invalid)?;
 if body.last_active!=before{return Err(BodyMutationError::Invalid("claim Agent activity expected value").into())}
 let after=crate::world::incremental_body::write_body_value(&AgentBodyValueV1{body_view:body.body_view,last_active:after},&mut mutation)?;ctx.transaction.write(StateNamespaceV1::Business,key.as_bytes(),Some(after))?;Ok(())
}
