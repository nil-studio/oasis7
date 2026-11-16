//! Exact original GEN emissions retained until actual accepted-point checks.
//! This witness is produced inside the immutable World GEN method, never from
//! caller roots, counts or a list of claimed semantic columns.
use super::*;
use oasis7_formal_protocol::{incremental_execution::ExecutionWriteV1,incremental_state::StateNamespaceV1};
use std::mem::size_of;
pub(in crate::world) struct EmittedRowV1{namespace:StateNamespaceV1,key:Vec<u8>,value:[u8;32]}
pub(in crate::world) struct ProjectionGenesisWitnessV1{rows:Vec<ReservedValue<EmittedRowV1>>,slots:MemoryReservation,memory:MemoryBudget,core:WorkIdentityV1,_fee:MemoryReservation}
impl ProjectionGenesisWitnessV1{
 pub(in crate::world) fn new(common:&mut FormalBoundedContextV1<'_>)->Result<Self,BodyMutationError>{let n=size_of::<Self>();common.charge(1,n as u64).map_err(paid)?;let fee=common.memory().reserve(n,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;let slots=common.memory().reserve(0,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;Ok(Self{rows:Vec::new(),slots,memory:common.memory().clone(),core:common.work().identity(),_fee:fee})}
 pub(in crate::world) fn observe(&mut self,row:&ExecutionWriteV1,common:&mut FormalBoundedContextV1<'_>)->Result<(),BodyMutationError>{
  self.validate_context(common)?;
  if row.mutation.before.is_some(){return Err(invalid("GEN projection emitted noncreation"))}
  let value=row.mutation.after.ok_or_else(||invalid("GEN projection emitted absent value"))?;
  let n=row.mutation.key.len().checked_add(size_of::<EmittedRowV1>()).ok_or_else(||invalid("GEN projection emission layout"))?;
  common.charge(1,n as u64).map_err(paid)?;let fee=common.memory().reserve(n,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
  let mut first=None;let position=self.rows.binary_search_by(|old|{if first.is_some(){return std::cmp::Ordering::Equal}let n=old.key.len().checked_add(row.mutation.key.len()).and_then(|n|n.checked_add(32));match n{Some(n)=>{if let Err(e)=common.charge(1,n as u64){first=Some(paid(e));return std::cmp::Ordering::Equal}},None=>{first=Some(invalid("GEN projection emission compare"));return std::cmp::Ordering::Equal}}old.namespace.cmp(&row.namespace).then_with(||old.key.cmp(&row.mutation.key))});if let Some(e)=first{return Err(e)}let position=position.err().ok_or_else(||invalid("duplicate GEN projection emission"))?;
  if self.rows.len()==self.rows.capacity(){let cap=self.rows.capacity().checked_mul(2).unwrap_or(usize::MAX).max(4);let n=cap.checked_mul(size_of::<ReservedValue<EmittedRowV1>>()).ok_or_else(||invalid("GEN witness vector layout"))?;common.charge(1,n as u64).map_err(paid)?;let fee=common.memory().reserve(n,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;self.rows.try_reserve_exact(cap-self.rows.len()).map_err(|_|invalid("GEN witness vector allocation"))?;if self.rows.capacity()!=cap{return Err(invalid("GEN witness vector capacity"))}self.slots=fee;}
  common.charge(1,((self.rows.len()-position)*size_of::<ReservedValue<EmittedRowV1>>())as u64).map_err(paid)?;
  let mut key=Vec::with_capacity(row.mutation.key.len());if key.capacity()!=row.mutation.key.len(){return Err(invalid("GEN witness key capacity"))}key.extend_from_slice(&row.mutation.key);
  self.rows.insert(position,fee.retain(EmittedRowV1{namespace:row.namespace,key,value}));Ok(())
 }
 pub(in crate::world) fn validate_context(&self,common:&mut FormalBoundedContextV1<'_>)->Result<(),BodyMutationError>{common.consume_steps(1).map_err(paid)?;if self.core!=common.work().identity()||!self.memory.same_pool(common.memory())||!self._fee.uses_budget(common.memory())||!self.slots.uses_budget(common.memory()){return Err(invalid("GEN witness context"))}Ok(())}
 pub(in crate::world) fn verify_actual_genesis_v1<M:oasis7_formal_protocol::incremental_state::MerkleNodeReader>(&self,parent:&crate::world::incremental_normal_sources::BoundNormalTickParentV1<'_>,tx:&mut oasis7_formal_protocol::incremental_execution::ExecutionTransaction<'_,M>,common:&mut FormalBoundedContextV1<'_>,live:&oasis7_formal_protocol::incremental_execution::ExecutionFence)->Result<(),BodyMutationError>{
  self.validate_context(common)?;parent.validate_context(common).map_err(paid)?;tx.validate_bootstrap_read_only_v1(live)?;
  if !tx.uses_budget(common.memory())||tx.parent_fence()!=parent.fence(){return Err(invalid("GEN projection exact bootstrap TX"))}
  let refs=parent.refs();let record=refs.head.record();
  if record.kind!=oasis7_formal_protocol::controlled_authority::formal::FormalRecordKindV1::InitialActivation||record.position!=1||!oasis7_formal_protocol::controlled_authority::formal::incremental::is_iws_record(record.schema_version)||refs.core.body().height!=0||refs.core.body().transition.is_some(){return Err(invalid("GEN projection accepted profile"))}
  let mut counts=[0u64;6];
  for row in &self.rows{common.charge(1,(row.key.len()+64)as u64).map_err(paid)?;let i=refs.roots.state.iter().position(|root|root.namespace==row.namespace).ok_or_else(||invalid("GEN witness namespace"))?;counts[i]=counts[i].checked_add(1).ok_or_else(||invalid("GEN witness count"))?;if tx.read(row.namespace,&row.key)?!=Some(row.value){return Err(invalid("GEN projection actual typed/raw value differs"))}}
  for(i,root)in refs.roots.state.iter().enumerate(){common.charge(1,16).map_err(paid)?;if root.leaf_count!=counts[i]{return Err(invalid("GEN projection complete emitted key coverage"))}}
  tx.validate_bootstrap_read_only_v1(live)?;Ok(())
 }
}
