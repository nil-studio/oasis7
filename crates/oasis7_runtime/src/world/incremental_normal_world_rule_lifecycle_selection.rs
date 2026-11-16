//! Paid snapshot selection DATA. The selector captures all seeds before any
//! domain write; hashing runs after the exclusive read child has finished.
//! Neither a seed nor this worklist grants a transition/event capability.
use super::*;
use super::work::{LifecycleRequestV1,OrderLifecycleChoiceV1,LifecycleWorkDataV1,prepare_lifecycle_work_v1};
use crate::world::incremental_normal_tail::NormalTailWorkV1;
use oasis7_formal_protocol::{controlled_authority::formal::bounded_context::FormalBoundedContextV1,incremental_value::ValueTreeError};
use oasis7_proto::memory_budget::MemoryClass;
use std::mem::size_of;
enum SelectedRequestSeedV1{
 SupplyClose(String),SupplySettle(u64),Observe{region:String,now:u64},
 Order{order:String,now:u64,command:OrderLifecycleChoiceV1},EmergencyClose(String),
}
impl SelectedRequestSeedV1{
 fn borrowed(&self)->LifecycleRequestV1<'_>{match self{
  Self::SupplyClose(id)=>LifecycleRequestV1::SupplyClose(id),Self::SupplySettle(id)=>LifecycleRequestV1::SupplySettle(*id),
  Self::Observe{region,now}=>LifecycleRequestV1::Observe{region,now:*now},
  Self::Order{order,now,command}=>LifecycleRequestV1::Order{order,now:*now,command:*command},
  Self::EmergencyClose(id)=>LifecycleRequestV1::EmergencyClose(id),
 }}
}
struct LifecycleSeedV1{actor:String,request:SelectedRequestSeedV1}
fn limited()->BodyMutationError{ValueTreeError::ResourceLimited("world-rule lifecycle snapshot worklist").into()}
fn grow<T>(items:&mut Vec<T>,fee:&mut Option<MemoryReservation>,memory:&MemoryBudget,mut charge:impl FnMut(usize)->Result<(),BodyMutationError>)->Result<(),BodyMutationError>{
 if items.len()<items.capacity(){return Ok(())}
 let next=items.capacity().checked_mul(2).unwrap_or(usize::MAX).max(4);
 let bytes=next.checked_mul(size_of::<T>()).ok_or_else(limited)?;
 charge(bytes)?;let after=memory.reserve(bytes,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
 items.try_reserve_exact(next.checked_sub(items.len()).ok_or_else(limited)?).map_err(|error|BodyMutationError::Original(Box::new(error)))?;
 if items.capacity()!=next{return Err(limited());}*fee=Some(after);Ok(())
}
pub(super) struct LifecycleSeedListV1{
 entries:Vec<ReservedValue<LifecycleSeedV1>>,buffer:Option<MemoryReservation>,memory:MemoryBudget,identity:WorkIdentityV1,_header:MemoryReservation,
}
impl LifecycleSeedListV1{
 pub(super) fn new(tail:&mut NormalTailWorkV1<'_,'_>)->Result<Self,BodyMutationError>{
  tail.charge(size_of::<Self>()+512)?;let header=tail.memory().reserve(size_of::<Self>()+512,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
  Ok(Self{entries:Vec::new(),buffer:None,memory:tail.memory().clone(),identity:tail.identity(),_header:header})
 }
 /// Only fixed selector lanes call this with values read from their real
 /// Current completeness owners; the stored enum is request DATA, not a flag.
 pub(super) fn push(&mut self,actor:&str,request:LifecycleRequestV1<'_>,tail:&mut NormalTailWorkV1<'_,'_>)->Result<(),BodyMutationError>{
  if !self.memory.same_pool(tail.memory())||self.identity!=tail.identity(){return Err(BodyMutationError::Invalid("lifecycle selected seed Core"));}
  let text=match &request{LifecycleRequestV1::SupplyClose(id)|LifecycleRequestV1::EmergencyClose(id)=>id.len(),LifecycleRequestV1::Observe{region,..}=>region.len(),LifecycleRequestV1::Order{order,..}=>order.len(),LifecycleRequestV1::SupplySettle(_)=>0};
  let bytes=actor.len().checked_add(text).and_then(|n|n.checked_add(size_of::<ReservedValue<LifecycleSeedV1>>())).ok_or_else(limited)?;
  tail.charge(bytes)?;let seed_fee=self.memory.reserve(bytes,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
  grow(&mut self.entries,&mut self.buffer,&self.memory,|n|tail.charge(n))?;
  let request=match request{LifecycleRequestV1::SupplyClose(id)=>SelectedRequestSeedV1::SupplyClose(id.to_owned()),LifecycleRequestV1::SupplySettle(id)=>SelectedRequestSeedV1::SupplySettle(id),LifecycleRequestV1::Observe{region,now}=>SelectedRequestSeedV1::Observe{region:region.to_owned(),now},LifecycleRequestV1::Order{order,now,command}=>SelectedRequestSeedV1::Order{order:order.to_owned(),now,command},LifecycleRequestV1::EmergencyClose(id)=>SelectedRequestSeedV1::EmergencyClose(id.to_owned())};
  self.entries.push(seed_fee.retain(LifecycleSeedV1{actor:actor.to_owned(),request}));Ok(())
 }
 /// Called after paid_io::run has settled and released its child. It cannot
 /// borrow/reset a second Common or invoke a serializer inside a live child.
 pub(super) fn into_work(self,common:&mut FormalBoundedContextV1<'_>)->Result<LifecycleWorkIteratorV1,BodyMutationError>{
  if !self.memory.same_pool(common.memory())||self.identity!=common.work().identity(){return Err(BodyMutationError::Invalid("lifecycle selected work Core"));}
  let Self{entries,buffer:seed_buffer,memory,identity:_,_header:header}=self;
  let mut work=Vec::new();let mut buffer=None;
  for seed in entries{
   let selected=prepare_lifecycle_work_v1(&seed.actor,seed.request.borrowed(),common)?;
   grow(&mut work,&mut buffer,&memory,|n|common.charge(1,n as u64).map_err(|error|BodyMutationError::Original(Box::new(error))))?;
   work.push(selected);
  }
  drop(seed_buffer);
  Ok(LifecycleWorkIteratorV1{entries:work.into_iter(),_buffer:buffer,_header:header})
 }
}
/// Buffer admission outlives the actual IntoIter allocation. Items retain
/// their own real field fees independently when moved into command execution.
pub(super) struct LifecycleWorkIteratorV1{
 entries:std::vec::IntoIter<ReservedValue<LifecycleWorkDataV1>>,_buffer:Option<MemoryReservation>,_header:MemoryReservation,
}
impl Iterator for LifecycleWorkIteratorV1{type Item=ReservedValue<LifecycleWorkDataV1>;fn next(&mut self)->Option<Self::Item>{self.entries.next()}fn size_hint(&self)->(usize,Option<usize>){self.entries.size_hint()}}
impl ExactSizeIterator for LifecycleWorkIteratorV1{}
