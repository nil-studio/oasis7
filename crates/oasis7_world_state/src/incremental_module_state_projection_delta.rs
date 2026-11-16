//! Dedicated opaque module-state changes to original Vec<u8> JSON DATA.
//! Original keys and value bytes are used directly; there is no Member/CBOR
//! reinterpretation or module execution permission in this producer.
use crate::{incremental_agent_mutation::BodyMutationError,incremental_module_fields::ModuleStateKeyV1,incremental_module_state_family::{ModuleStateFamilyQualificationV1,validate_module_state_transaction_v1},incremental_module_state_recovery::ModuleStateContentBudgetV1,incremental_field_overlay::WorldFieldOverlayContextV1,incremental_runtime_revocation::BusinessDiffProcessingWorkV1,incremental_semantic_key::write_semantic_key_v1};
use oasis7_formal_protocol::{incremental_execution::{ExecutionTransaction,ExecutionFence,PreparedGuard},incremental_state::{MerkleNodeReader,StateNamespaceV1 as Ns},incremental_value::{ValueNodeReader,ValueTreeError}};
use oasis7_proto::memory_budget::{MemoryBudget,MemoryClass,MemoryReservation,ReservedValue};
use std::{cell::RefCell,mem::size_of,io::{self,Write}};
struct Pending{key:Vec<u8>,after:Option<[u8;32]>}
struct Row{logical_bytes:u64,semantic:ReservedValue<Vec<u8>>,key:ReservedValue<Vec<u8>>,value:Option<ReservedValue<Vec<u8>>>}
pub struct CurrentModuleStateProjectionDeltaV1{rows:Vec<Row>,guard:PreparedGuard,parent:[u8;32],memory:MemoryBudget,_fee:MemoryReservation}
fn bad()->ValueTreeError{ValueTreeError::Invalid("Current module-state projection")}
fn limit()->ValueTreeError{ValueTreeError::ResourceLimited("module-state projection work")}
fn pay(work:&mut BusinessDiffProcessingWorkV1,bytes:usize,stop:&mut impl FnMut()->bool)->Result<(),ValueTreeError>{if stop(){return Err(ValueTreeError::Cancelled)}let bytes=u64::try_from(bytes).map_err(|_|limit())?;let steps=bytes.checked_add(1).ok_or_else(limit)?;let old=(work.steps_remaining,work.bytes_remaining);work.steps_remaining=work.steps_remaining.saturating_sub(steps);work.bytes_remaining=work.bytes_remaining.saturating_sub(bytes);if old.0<steps||old.1<bytes{return Err(limit())}Ok(())}
fn vector(memory:&MemoryBudget,n:usize)->Result<(Vec<u8>,MemoryReservation),ValueTreeError>{let fee=memory.reserve(n.checked_add(size_of::<Vec<u8>>()).ok_or_else(limit)?,MemoryClass::Ordinary)?;let bytes=Vec::with_capacity(n);if bytes.capacity()!=n{return Err(limit())}Ok((bytes,fee))}
struct Output<'a,C>{work:&'a mut BusinessDiffProcessingWorkV1,stop:&'a mut C,bytes:Option<Vec<u8>>,n:usize,bound:usize,first:Option<ValueTreeError>}
impl<C:FnMut()->bool> Write for Output<'_,C>{fn write(&mut self,bytes:&[u8])->io::Result<usize>{if self.first.is_some(){return Err(io::ErrorKind::Other.into())}for part in bytes.chunks(8192){if let Err(error)=pay(self.work,part.len(),self.stop){self.first=Some(error);return Err(io::ErrorKind::Other.into())}let Some(n)=self.n.checked_add(part.len()).filter(|n|*n<=self.bound)else{self.first=Some(limit());return Err(io::ErrorKind::Other.into())};if let Some(bytes)=&mut self.bytes{bytes.extend_from_slice(part)}self.n=n}Ok(bytes.len())}fn flush(&mut self)->io::Result<()>{Ok(())}}
fn semantic<C:FnMut()->bool>(key:&str,memory:&MemoryBudget,work:&mut BusinessDiffProcessingWorkV1,stop:&mut C)->Result<ReservedValue<Vec<u8>>,ValueTreeError>{
 let mut count=Output{work,stop,bytes:None,n:0,bound:usize::MAX,first:None};let result=write_semantic_key_v1(key,&mut count);if let Some(error)=count.first{return Err(error)}result.map_err(ValueTreeError::Io)?;let n=count.n;let (bytes,fee)=vector(memory,n)?;let mut output=Output{work:count.work,stop:count.stop,bytes:Some(bytes),n:0,bound:n,first:None};let result=write_semantic_key_v1(key,&mut output);if let Some(error)=output.first{return Err(error)}result.map_err(ValueTreeError::Io)?;if output.n!=n{return Err(bad())}Ok(fee.retain(output.bytes.ok_or_else(bad)?))
}
impl CurrentModuleStateProjectionDeltaV1{
 pub fn validate<M:MerkleNodeReader>(&self,tx:&ExecutionTransaction<'_,M>,memory:&MemoryBudget,live:&ExecutionFence)->Result<(),BodyMutationError>{tx.validate_read_source_guard(&self.guard,live)?;if !self.memory.same_pool(memory)||!self._fee.uses_budget(memory)||!tx.uses_budget(memory)||self.parent!=tx.parent_fence().parent_roots{return Err(bad().into())}Ok(())}
 pub fn visit_with_work_v1<E>(&self,mut work:impl FnMut(u64,u64)->Result<(),E>,mut emit:impl FnMut(&'static str,&[u8],&[u8],Option<&[u8]>)->Result<(),E>)->Result<(),E>{for row in &self.rows{work(1,0)?;emit("module_states",&row.semantic,&row.key,row.value.as_deref().map(Vec::as_slice))?}Ok(())}
}
fn decode_projection_row_v1<M:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool>(row:Pending,ctx:&mut WorldFieldOverlayContextV1<'_, '_, '_,M,R,E,C>,content:&mut ModuleStateContentBudgetV1,processing:&mut BusinessDiffProcessingWorkV1)->Result<Row,BodyMutationError>{let memory=ctx.values.memory.clone();pay(processing,row.key.len(),&mut ctx.cancelled)?;if ctx.transaction.read(Ns::Business,&row.key)?!=row.after{return Err(bad().into())}let key=ModuleStateKeyV1::state_key_from_bytes(&row.key).map_err(ValueTreeError::Invalid)?;
  let semantic=semantic(key,&memory,processing,&mut ctx.cancelled)?;let key_json=crate::incremental_world_rule_projection_delta::encode_original_projection_json_data_v1(&key,&memory,processing,&mut ctx.cancelled)?;
  let mut logical_bytes=0u64;let value=if let Some(id)=row.after{
   content.states_remaining=content.states_remaining.checked_sub(1).ok_or_else(limit)?;
   let root=ctx.values.load_root(&id)?;let length=root.0.logical_len;logical_bytes=length;if length>content.max_state_bytes||length>content.logical_bytes_remaining{return Err(limit().into())}content.logical_bytes_remaining-=length;
   let n=usize::try_from(length).map_err(|_|limit())?;pay(processing,n.checked_mul(16).ok_or_else(limit)?,&mut ctx.cancelled)?;let (mut bytes,_source_fee)=vector(&memory,n)?;
   ctx.values.read_range(&root,0,length,&mut bytes,&mut ctx.cancelled)?;if bytes.len()!=n{return Err(bad().into())}
   Some(crate::incremental_world_rule_projection_delta::encode_original_projection_json_data_v1(&bytes,&memory,processing,&mut ctx.cancelled)?)
  }else{None};Ok(Row{logical_bytes,semantic,key:key_json,value})
}
#[allow(clippy::too_many_arguments)]
pub fn read_current_module_state_projection_delta_v1<M:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool>(parent:&ReservedValue<ModuleStateFamilyQualificationV1>,ctx:&mut WorldFieldOverlayContextV1<'_, '_, '_,M,R,E,C>,content:&mut ModuleStateContentBudgetV1,pending_entries:&mut usize,processing:&mut BusinessDiffProcessingWorkV1,guard:&PreparedGuard,live:&ExecutionFence)->Result<CurrentModuleStateProjectionDeltaV1,BodyMutationError>{
 ctx.transaction.validate_guard(guard,live)?;validate_module_state_transaction_v1(parent,ctx.transaction,ctx.values.memory)?;if ctx.values.class!=MemoryClass::Ordinary{return Err(bad().into())}let memory=ctx.values.memory.clone();
 let mut count=0usize;let mut key_bytes=0usize;let stop=RefCell::new(&mut ctx.cancelled);let bank=RefCell::new(&mut *processing);
 ctx.transaction.visit_pending_writes(Ns::Business,pending_entries,||pay(&mut bank.borrow_mut(),0,&mut ||(stop.borrow_mut())()).map_err(BodyMutationError::from),|key,_,after|{pay(&mut bank.borrow_mut(),key.len(),&mut ||(stop.borrow_mut())())?;if ModuleStateKeyV1::is_state_key(key){ModuleStateKeyV1::state_key_from_bytes(key).map_err(ValueTreeError::Invalid)?;count=count.checked_add(1).ok_or_else(limit)?;key_bytes=key_bytes.checked_add(key.len()).ok_or_else(limit)?}let _=after;Ok::<_,BodyMutationError>(())})?;
 let n=count.checked_mul(size_of::<Pending>()).and_then(|n|n.checked_add(key_bytes)).ok_or_else(limit)?;pay(&mut bank.borrow_mut(),n,&mut ||(stop.borrow_mut())())?;let _pending_fee=memory.reserve(n,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;let mut pending=Vec::with_capacity(count);if pending.capacity()!=count{return Err(limit().into())}
 ctx.transaction.visit_pending_writes(Ns::Business,pending_entries,||pay(&mut bank.borrow_mut(),0,&mut ||(stop.borrow_mut())()).map_err(BodyMutationError::from),|key,_,after|{pay(&mut bank.borrow_mut(),key.len(),&mut ||(stop.borrow_mut())())?;if ModuleStateKeyV1::is_state_key(key){if pending.len()==count{return Err(limit().into())}let mut encoded=Vec::with_capacity(key.len());if encoded.capacity()!=key.len(){return Err(limit().into())}encoded.extend_from_slice(key);pending.push(Pending{key:encoded,after})}Ok::<_,BodyMutationError>(())})?;
 drop(bank);drop(stop);if pending.len()!=count{return Err(bad().into())}
 let n=count.checked_mul(size_of::<Row>()).and_then(|n|n.checked_add(size_of::<CurrentModuleStateProjectionDeltaV1>())).ok_or_else(limit)?;pay(processing,n,&mut ctx.cancelled)?;let fee=memory.reserve(n,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;let mut rows=Vec::with_capacity(count);if rows.capacity()!=count{return Err(limit().into())}
 for row in pending{rows.push(decode_projection_row_v1(row,ctx,content,processing)?);}

 if(ctx.cancelled)(){return Err(ValueTreeError::Cancelled.into())}if let Some(error)=(ctx.take_state_error)(){return Err(error.into())}let guard=ctx.transaction.guard();ctx.transaction.validate_guard(&guard,live)?;Ok(CurrentModuleStateProjectionDeltaV1{rows,guard,parent:ctx.transaction.parent_fence().parent_roots,memory,_fee:fee})
}

#[path="incremental_module_state_projection_delta_cold.rs"] mod cold;
pub use cold::{QualifiedModuleStateProjectionColumnsV1,read_qualified_module_state_projection_columns_v1};
