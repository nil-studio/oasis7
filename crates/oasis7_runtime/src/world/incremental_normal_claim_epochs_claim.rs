//! Claim decision, actual event-target re-read, original fixed mutation and CAS.
use super::*;
use crate::world::{incremental_normal_tail::NormalTailWorkV1,incremental_normal_world_rule_lifecycle::delivery::{IndustrialDeliveryWorkV1,IndustrialDeliveryErrorV1},incremental_body_event::materials::CorePolicyMaterialContextV1,incremental_body::BodyMutationContext};
use oasis7_formal_protocol::{incremental_value::{ValueNodeReader,ValueNodeWriter,ValueReadContext,ValueTreeError},incremental_state::StatePageWriter};
use oasis7_world_state::{
    AgentClaimState,
    incremental_field_member::MemberRecordWriteContext,
    incremental_field_overlay::WorldFieldOverlayContextV1,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_main_token_policy_records::{
        CurrentRestrictedClaimGrantV1, MainTokenPolicyFamilyQualificationV1,
        read_current_restricted_claim_grant_v1,
    },
    incremental_world_native::*,
    incremental_world_u64::*,
};
use oasis7_proto::memory_budget::MemoryClass;
#[derive(Clone,Copy)]pub(super) enum ClaimEpochPassV1{Initial,AfterUpkeep}
pub(super) struct CommittedClaimEpochStateV1{
 _builder:ReservedValue<CurrentIndustrialClaimRecordV1<AgentClaimState>>,
 _applied:ReservedValue<CurrentIndustrialClaimRecordV1<AgentClaimState>>,
 _builder_grant:ReservedValue<CurrentRestrictedClaimGrantV1>,_applied_grant:ReservedValue<CurrentRestrictedClaimGrantV1>,
 _refund_fee:MemoryReservation,_applied_refund_fee:MemoryReservation,_after:Option<AgentClaimState>,_money:Option<super::money::SelectedClaimMoneyV1>,
}
fn heap(claim:&AgentClaimState)->Result<usize,BodyMutationError>{
 let mut bytes=claim.target_agent_id.len().checked_add(claim.claim_owner_id.len()).and_then(|n|n.checked_add(claim.claim_bond_restricted_source_treasury_bucket_id.as_ref().map_or(0,String::len))).ok_or(BodyMutationError::Invalid("claim clone payload"))?;
 if let Some(source)=&claim.control_source{bytes=bytes.checked_add(source.root_agent_id.len()).and_then(|n|n.checked_add(source.player_id.len())).and_then(|n|n.checked_add(source.acquired_event_digest.len())).ok_or(BodyMutationError::Invalid("claim source payload"))?;}
 bytes.checked_mul(6).and_then(|n|n.checked_add(8192+std::mem::size_of::<AgentClaimState>()*2)).ok_or(BodyMutationError::Invalid("claim epoch phase layout"))
}
#[allow(clippy::too_many_arguments)]
fn read_claim<M:MerkleNodeReader,P:MerkleNodeReader,V:ValueNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool,PE:FnMut()->Option<ValueTreeError>,PC:FnMut()->bool>(id:&str,sources:&ClaimEpochStateSourcesV1<'_>,ctx:&mut WorldFieldOverlayContextV1<'_,'_,'_,M,R,E,C>,parent:&mut WorldFieldReadContextV1<'_,P,V,PE,PC>,work:&mut IndustrialDeliveryWorkV1,tail:&mut NormalTailWorkV1<'_,'_>,live:&ExecutionFence)->Result<ReservedValue<CurrentIndustrialClaimRecordV1<AgentClaimState>>,IndustrialDeliveryErrorV1>{
 tail.charge(id.len().checked_add(8192).ok_or(BodyMutationError::Invalid("claim metadata fee"))?)?;
 let mut read=WorldFieldReadContextV1{root:sources.roots.state_root(industrial_claim_namespace_v1()?),state:parent.state,values:ValueReadContext{source:parent.values.source,memory:parent.values.memory,class:parent.values.class,io:&mut*parent.values.io},key_work:&mut*parent.key_work,semantic_bytes:parent.semantic_bytes,record_bytes:parent.record_bytes,take_state_error:&mut parent.take_state_error,cancelled:&mut parent.cancelled};
 let guard=ctx.transaction.guard();Ok(read_current_industrial_agent_claim_v1(id,sources.native,sources.roots,ctx.transaction,&mut read,&mut work.eligibility.scan,&mut work.eligibility.typed,&guard,live)?)
}
#[allow(clippy::too_many_arguments)]
pub(super) fn apply_current_claim_epoch_v1<M:MerkleNodeReader,P:MerkleNodeReader,V:ValueNodeReader,R:ValueNodeReader,W:ValueNodeReader+ValueNodeWriter+StatePageWriter,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool,PE:FnMut()->Option<ValueTreeError>,PC:FnMut()->bool,CW:FnMut()->bool,ME:FnMut()->Option<ValueTreeError>>(target:&str,pass:ClaimEpochPassV1,epoch:u64,epoch_length:u64,policy:&ReservedValue<MainTokenPolicyFamilyQualificationV1>,sources:&ClaimEpochStateSourcesV1<'_>,state:&M,ctx:&mut WorldFieldOverlayContextV1<'_,'_,'_,M,R,E,C>,parent:&mut WorldFieldReadContextV1<'_,P,V,PE,PC>,write:&mut MemberRecordWriteContext<'_,W,CW>,materials:&mut CorePolicyMaterialContextV1<'_,M,ME>,work:&mut IndustrialDeliveryWorkV1,tail:&mut NormalTailWorkV1<'_,'_>,now:u64,live:&ExecutionFence)->Result<Option<PreparedClaimEpochMutationV1>,IndustrialDeliveryErrorV1>{
 let builder=read_claim(target,sources,ctx,parent,work,tail,live)?;let Some(current)=builder.value()else{return Ok(None)};
 let amount=heap(current)?;tail.charge(amount)?;let phase=tail.reserve(amount,MemoryClass::Ordinary)?;
 let guard=ctx.transaction.guard();let builder_grant=read_current_restricted_claim_grant_v1(&current.claim_owner_id,policy,sources.roots,state,ctx,&mut work.fixed_scan,&mut work.fixed_cells_remaining,work.max_identity_bytes as u64,&guard,live)?;
 let refund_bytes=builder_grant.value().map_or(0,|g|g.source_treasury_bucket_id.len()).checked_add(current.claim_bond_restricted_source_treasury_bucket_id.as_ref().map_or(0,String::len)).and_then(|n|n.checked_mul(4)).ok_or(BodyMutationError::Invalid("claim refund source payload"))?;tail.charge(refund_bytes)?;let refund_fee=tail.reserve(refund_bytes,MemoryClass::Ordinary)?;
 let refund=evaluate::refund_sink_v1(current,builder_grant.value());
 let last_control=super::agent::last_active(&current.target_agent_id,sources,ctx,parent,work,tail,live)?.map_or(epoch,|tick|tick/epoch_length.max(1));
 let event=match pass{ClaimEpochPassV1::Initial=>{let(balance,_fee)=crate::world::incremental_normal_world_rule_lifecycle::delivery::economy::balance(&current.claim_owner_id,ctx,work.max_identity_bytes,tail)?;evaluate::prepare_epoch_v1(epoch,current,balance.as_ref().map_or(0,|v|v.liquid_balance),balance.as_ref().map_or(0,|v|v.restricted_starter_claim_balance),last_control,&refund)?},ClaimEpochPassV1::AfterUpkeep=>evaluate::prepare_release_or_idle_v1(epoch,current,last_control,&refund)?};
 let Some(event)=event else{return Ok(None)};
 let(actor,event_target)=match &event{crate::DomainEvent::AgentClaimUpkeepSettled{claimer_agent_id,target_agent_id,..}|crate::DomainEvent::AgentClaimEnteredGrace{claimer_agent_id,target_agent_id,..}|crate::DomainEvent::AgentClaimIdleWarning{claimer_agent_id,target_agent_id,..}|crate::DomainEvent::AgentClaimReleased{claimer_agent_id,target_agent_id,..}|crate::DomainEvent::AgentClaimReclaimed{claimer_agent_id,target_agent_id,..}=>(claimer_agent_id.as_str(),target_agent_id.as_str()),_=>return Err(BodyMutationError::Invalid("claim epoch fixed event").into())};
 // Original publication looks up the actual actor before the event's target;
 // neither the snapshot outer key nor its inner target substitutes this read.
 let last_active=super::agent::last_active(actor,sources,ctx,parent,work,tail,live)?.ok_or_else(||crate::WorldError::AgentNotFound{agent_id:actor.to_owned()})?;
 let applied=read_claim(event_target,sources,ctx,parent,work,tail,live)?;let selected_after=applied.value().ok_or_else(||crate::WorldError::ResourceBalanceInvalid{reason:format!("agent claim not found: target={event_target}")})?;
 let after_heap=heap(selected_after)?;tail.charge(after_heap)?;let after_fee=tail.reserve(after_heap,MemoryClass::Ordinary)?;let mut after=selected_after.clone();
 let guard=ctx.transaction.guard();let applied_grant=read_current_restricted_claim_grant_v1(actor,policy,sources.roots,state,ctx,&mut work.fixed_scan,&mut work.fixed_cells_remaining,work.max_identity_bytes as u64,&guard,live)?;
 let applied_refund_bytes=applied_grant.value().map_or(0,|g|g.source_treasury_bucket_id.len()).checked_mul(4).ok_or(BodyMutationError::Invalid("claim applied refund payload"))?;tail.charge(applied_refund_bytes)?;let applied_refund_fee=tail.reserve(applied_refund_bytes,MemoryClass::Ordinary)?;
 let mut money=None;let terminal=matches!(event,crate::DomainEvent::AgentClaimReleased{..}|crate::DomainEvent::AgentClaimReclaimed{..});
 match &event{
  crate::DomainEvent::AgentClaimEnteredGrace{..}|crate::DomainEvent::AgentClaimIdleWarning{..}=>{let mut activity=last_active;oasis7_world_state::world::agent_claim_light_lifecycle_publication::apply_selected_agent_claim_light_v1(&event,&mut after,&mut activity,now)?;},
  crate::DomainEvent::AgentClaimUpkeepSettled{..}=>{let mut selected=super::money::read(actor,None,sources,state,ctx,work,tail,live)?;oasis7_world_state::world::agent_claim_economic_publication::apply_selected_agent_claim_upkeep_v1(&event,actor,&mut after,&mut selected.after,&mut selected.supply_after,&selected.treasury_before,&mut selected.treasury_after)?;money=Some(selected);},
  crate::DomainEvent::AgentClaimReleased{refunded_bond_restricted_sink_bucket_id,..}|crate::DomainEvent::AgentClaimReclaimed{refunded_bond_restricted_sink_bucket_id,..}=>{let mut selected=super::money::read(actor,Some(refunded_bond_restricted_sink_bucket_id),sources,state,ctx,work,tail,live)?;oasis7_world_state::world::agent_claim_terminal_publication::apply_selected_agent_claim_terminal_v1(&event,actor,event_target,&after,&oasis7_world_state::world::agent_claim_terminal_publication::SelectedClaimTerminalSourceV1{treasury:&selected.treasury_before,grant:applied_grant.value()},&mut selected.after,&mut selected.supply_after,&mut selected.treasury_after)?;money=Some(selected);},
  _=>return Err(BodyMutationError::Invalid("claim epoch selected application").into()),
 }
 {let mut mutation=BodyMutationContext{sink:&mut*write.sink,memory:write.memory,io:&mut*write.io,cancelled:&mut write.cancelled};let guard=ctx.transaction.guard();materials.contribute(ctx.transaction,ctx.values.source,&mut mutation,&guard,live)?;}
 if let Some(selected)=&money{super::money::commit(selected,actor,ctx,write,work,live)?;}
 {let mut read=WorldFieldReadContextV1{root:sources.roots.state_root(industrial_claim_namespace_v1()?),state:parent.state,values:ValueReadContext{source:parent.values.source,memory:parent.values.memory,class:parent.values.class,io:&mut*parent.values.io},key_work:&mut*parent.key_work,semantic_bytes:parent.semantic_bytes,record_bytes:parent.record_bytes,take_state_error:&mut parent.take_state_error,cancelled:&mut parent.cancelled};let guard=ctx.transaction.guard();if terminal{delete_current_industrial_agent_claim_v1(event_target,&applied,ctx.transaction,&mut read,&guard,live)?;}else{write_current_industrial_agent_claim_v1(event_target,&applied,&after,ctx.transaction,&mut read,write,&mut work.records_remaining,&mut work.native_write,&guard,live)?;}}
 if terminal{super::agent::write_last_active(actor,last_active,now,sources,ctx,write,work,tail,live)?;}
 if matches!(event,crate::DomainEvent::AgentClaimUpkeepSettled{..})||terminal{let before=read_world_u64_overlay_v1(WorldU64FieldV1::AgentClaimLastProcessedEpoch,ctx)?;let guard=ctx.transaction.guard();change_world_u64_overlay_v1(WorldU64FieldV1::AgentClaimLastProcessedEpoch,before,before.max(epoch),ctx,write,&guard,live)?;}
 let committed=CommittedClaimEpochStateV1{_builder:builder,_applied:applied,_builder_grant:builder_grant,_applied_grant:applied_grant,_refund_fee:refund_fee,_applied_refund_fee:applied_refund_fee,_after:(!terminal).then_some(after),_money:money};
 let preserved_last_active=matches!(event,crate::DomainEvent::AgentClaimUpkeepSettled{..}|crate::DomainEvent::AgentClaimEnteredGrace{..}|crate::DomainEvent::AgentClaimIdleWarning{..}).then_some(last_active);
 Ok(Some(PreparedClaimEpochMutationV1{event:Some(event),guard:ctx.transaction.guard(),identity:tail.identity(),memory:tail.memory().clone(),now,preserved_last_active,_phase:phase,_state:CommittedEpochStateV1::Claim{source:committed,_after_fee:after_fee}}))
}
