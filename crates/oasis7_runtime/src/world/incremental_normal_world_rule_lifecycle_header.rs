//! Fixed lifecycle header provenance. Only this module's actual Current
//! header producer may construct a fresh header; consumers only borrow it.
use super::*;
pub(in crate::world) struct PreparedFreshLifecycleHeaderV1{
 pub(super) event:crate::DomainEvent,
 pub(super) view:Option<ReservedValue<super::delivery::CurrentWorldRuleOrderViewV1>>,
 pub(super) operation:LifecycleOperationV1,
 pub(super) request:ReservedValue<oasis7_world_state::incremental_world_rule_requests_current::CurrentWorldRuleRequestV1>,
 pub(super) guard:PreparedGuard,pub(super) identity:WorkIdentityV1,
 pub(super) memory:MemoryBudget,pub(super) now:u64,pub(super) phase:MemoryReservation,
}
impl PreparedFreshLifecycleHeaderV1{
 pub(in crate::world) fn world_rule_order_view(&self)->Option<&ReservedValue<super::delivery::CurrentWorldRuleOrderViewV1>>{self.view.as_ref()}

 pub(in crate::world) fn event(&self)->&crate::DomainEvent{&self.event}
 pub(in crate::world) fn validate<M:MerkleNodeReader>(&self,tx:&ExecutionTransaction<'_,M>,memory:&MemoryBudget,now:u64,identity:WorkIdentityV1,live:&ExecutionFence)->Result<(),BodyMutationError>{
  tx.validate_guard(&self.guard,live)?;
  if self.identity!=identity||!self.memory.same_pool(memory)||!self.request.uses_budget(memory)||!tx.uses_budget(memory)||self.now!=now||self.request.value().is_some(){return Err(BodyMutationError::Invalid("fresh lifecycle header binding"));}
  match &self.event{crate::DomainEvent::WorldRuleCommitted{action_id:0,..}=>Ok(()),_=>Err(BodyMutationError::Invalid("fresh lifecycle system header"))}
 }
}

/// Private fee-only seam: it cannot replace a business predicate or source.
pub(super) trait LifecycleFeeV1{
 fn memory(&self)->&MemoryBudget;
 fn charge(&mut self,bytes:usize)->Result<(),BodyMutationError>;
}
impl LifecycleFeeV1 for oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedContextV1<'_>{
 fn memory(&self)->&MemoryBudget{oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedContextV1::memory(self)}
 fn charge(&mut self,bytes:usize)->Result<(),BodyMutationError>{
  oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedContextV1::charge(self,1,u64::try_from(bytes).map_err(|_|BodyMutationError::Invalid("lifecycle fee bytes"))?).map_err(|e|BodyMutationError::Original(Box::new(e)))
 }
}
impl LifecycleFeeV1 for crate::world::incremental_normal_tail::NormalTailWorkV1<'_,'_>{
 fn memory(&self)->&MemoryBudget{crate::world::incremental_normal_tail::NormalTailWorkV1::memory(self)}
 fn charge(&mut self,bytes:usize)->Result<(),BodyMutationError>{crate::world::incremental_normal_tail::NormalTailWorkV1::charge(self,bytes)}
}

/// Exact retained request classification, before manufactured-world scope and
/// command-size checks. This is DATA only: neither arm grants fresh mutation
/// or event publication. The caller must first establish actual actor presence.
pub(super) enum LifecycleRequestDispositionV1 {
 Fresh,
 ExactRetry,
 Conflict(ReservedValue<crate::WorldError>),
}
pub(super) fn classify_current_request_v1<F:LifecycleFeeV1>(
 request:&ReservedValue<oasis7_world_state::incremental_world_rule_requests_current::CurrentWorldRuleRequestV1>,
 selected:&super::work::LifecycleWorkDataV1,
 common:&mut F,
)->Result<LifecycleRequestDispositionV1,BodyMutationError>{
 use oasis7_proto::memory_budget::MemoryClass;
 use oasis7_formal_protocol::{incremental_value::ValueTreeError};
 if !request.uses_budget(common.memory()) {return Err(BodyMutationError::Invalid("lifecycle request disposition pool"));}
 let Some(receipt)=request.value() else{return Ok(LifecycleRequestDispositionV1::Fresh)};
 // Only the five closed lifecycle commands can be selected here. For a
 // different retained variant, original enum PartialEq returns before its
 // payload; never walk an unrelated historical command's recursive content.
 let compared=command_comparison_bytes_v1(&receipt.command,&selected.command)?
  .checked_add(receipt.actor_id.len()).and_then(|n|n.checked_add(selected.actor.len()))
  .ok_or_else(||BodyMutationError::from(ValueTreeError::ResourceLimited("lifecycle exact request comparison")))?;
 common.charge(compared)?;
 if receipt.actor_id==selected.actor && receipt.command==selected.command{return Ok(LifecycleRequestDispositionV1::ExactRetry)}
 let fee=common.memory().reserve(128+std::mem::size_of::<ReservedValue<crate::WorldError>>(),MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
 Ok(LifecycleRequestDispositionV1::Conflict(fee.retain(crate::world_rules::invalid("request_content_conflict"))))
}

pub(super) fn command_comparison_bytes_v1(before:&WorldRuleCommandV1,after:&WorldRuleCommandV1)->Result<usize,BodyMutationError>{
 let pair=match after{
  WorldRuleCommandV1::CloseSupply{authorization_id:a}=>match before{WorldRuleCommandV1::CloseSupply{authorization_id:b}=>Some((a.as_str(),b.as_str())),_=>None},
  WorldRuleCommandV1::AdvanceAgentOrder{order_id:a}=>match before{WorldRuleCommandV1::AdvanceAgentOrder{order_id:b}=>Some((a.as_str(),b.as_str())),_=>None},
  WorldRuleCommandV1::DeliverAgentOrder{order_id:a}=>match before{WorldRuleCommandV1::DeliverAgentOrder{order_id:b}=>Some((a.as_str(),b.as_str())),_=>None},
  WorldRuleCommandV1::SettleSupply{..}=>None,
  WorldRuleCommandV1::EmergencySupply{command:EmergencySupplyCommandV1::Observe{region_id:a}}=>match before{WorldRuleCommandV1::EmergencySupply{command:EmergencySupplyCommandV1::Observe{region_id:b}}=>Some((a.as_str(),b.as_str())),_=>None},
  WorldRuleCommandV1::EmergencySupply{command:EmergencySupplyCommandV1::Close{authorization_id:a}}=>match before{WorldRuleCommandV1::EmergencySupply{command:EmergencySupplyCommandV1::Close{authorization_id:b}}=>Some((a.as_str(),b.as_str())),_=>None},
  _=>return Err(BodyMutationError::Invalid("selected lifecycle command domain")),
 };
 pair.map_or(Some(32),|(a,b)|a.len().checked_add(b.len()).and_then(|n|n.checked_add(32)))
  .ok_or(BodyMutationError::Invalid("lifecycle command comparison length"))
}

/// Original command JSON size, streamed under the same Common without
/// allocating a World, command clone, or wire buffer. Only fee/cancellation
/// callbacks are supplied to serde; command predicates remain fixed above.
fn charge_command_scan_v1<F:LifecycleFeeV1>(command:&WorldRuleCommandV1,common:&mut F)->Result<usize,BodyMutationError>{
 struct Counter<'a,F>{common:&'a mut F,bytes:usize,first:Option<BodyMutationError>}
 impl<F:LifecycleFeeV1> std::io::Write for Counter<'_,F>{
  fn write(&mut self,bytes:&[u8])->std::io::Result<usize>{
   let result:Result<(),BodyMutationError>=(||{
    self.common.charge(bytes.len())?;
    self.bytes=self.bytes.checked_add(bytes.len()).ok_or(BodyMutationError::Invalid("lifecycle command JSON length"))?;Ok(())
   })();
   if let Err(error)=result{self.first=Some(error);return Err(std::io::Error::other("lifecycle command work"))}Ok(bytes.len())
  }
  fn flush(&mut self)->std::io::Result<()>{Ok(())}
 }
 let _phase=common.memory().reserve(std::mem::size_of::<Counter<'_, F>>()+256,oasis7_proto::memory_budget::MemoryClass::Ordinary)
  .map_err(oasis7_formal_protocol::incremental_value::ValueTreeError::Memory)?;
 let mut count=Counter{common,bytes:0,first:None};let result=serde_json::to_writer(&mut count,command);
 if let Some(error)=count.first{return Err(error)}
 result.map_err(|e|BodyMutationError::Original(Box::new(e)))?;Ok(count.bytes)
}
/// Called only after exact-request handling and manufactured-world scope,
/// preserving the legacy first-error order and exact 65,536-byte boundary.
pub(super) fn validate_fresh_command_size_v1<F:LifecycleFeeV1>(command:&WorldRuleCommandV1,common:&mut F)->Result<Option<ReservedValue<crate::WorldError>>,BodyMutationError>{
 if charge_command_scan_v1(command,common)?<=65_536{return Ok(None)}
 let fee=common.memory().reserve(128+std::mem::size_of::<ReservedValue<crate::WorldError>>(),oasis7_proto::memory_budget::MemoryClass::Ordinary)
  .map_err(oasis7_formal_protocol::incremental_value::ValueTreeError::Memory)?;
 Ok(Some(fee.retain(crate::world_rules::invalid("world_rule_command_too_large"))))
}

#[path="incremental_normal_world_rule_lifecycle_header_current.rs"]
mod current;
pub(super) use current::{LifecycleHeaderSourcesV1,LifecycleHeaderPreparationV1,PreparedRetainedLifecycleHeaderV1,prepare_current_lifecycle_header_v1};
