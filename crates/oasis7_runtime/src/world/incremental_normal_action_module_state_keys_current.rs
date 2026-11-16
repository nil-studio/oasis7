//! Complete resident module-state keys plus the real transaction's net edits.
//! This owner is DATA, not M1 compatibility or activation authorization.
use crate::world::incremental_body::BodyMutationError;
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFence,ExecutionTransaction,PreparedGuard,RootSetV1},
    incremental_state::{MerkleNodeReader,StateNamespaceV1,StateScanBudget},
    incremental_value::{ValueNodeReader,ValueTreeError},
};
use oasis7_world_state::{incremental_module_fields::ModuleStateKeyV1,
    incremental_module_state_family::{ModuleStateFamilyQualificationV1,validate_module_state_transaction_v1},
    incremental_field_overlay::WorldFieldOverlayContextV1};
use oasis7_proto::memory_budget::{MemoryBudget,MemoryClass,MemoryReservation,ReservedValue};
use std::{cell::RefCell,collections::BTreeMap};

pub(in crate::world) struct PaidCurrentModuleStateKeysV1 {
    keys:BTreeMap<String,MemoryReservation>,guard:PreparedGuard,parent:[u8;32],
}
impl PaidCurrentModuleStateKeysV1 {
    pub(in crate::world) fn keys(&self)->impl ExactSizeIterator<Item=&str>{self.keys.keys().map(String::as_str)}
    pub(in crate::world) fn validate<M:MerkleNodeReader>(&self,tx:&ExecutionTransaction<'_,M>,memory:&MemoryBudget,live:&ExecutionFence)->Result<(),BodyMutationError>{
        tx.validate_transaction_identity(&self.guard)?;tx.validate_guard(&tx.guard(),live)?;
        if !tx.uses_budget(memory)||tx.parent_fence().parent_roots!=self.parent{return Err(bad())}Ok(())
    }
    pub(in crate::world) fn validate_strict<M:MerkleNodeReader>(&self,tx:&ExecutionTransaction<'_,M>,memory:&MemoryBudget,live:&ExecutionFence)->Result<(),BodyMutationError>{
        self.validate(tx,memory,live)?;tx.validate_guard(&self.guard,live)?;Ok(())
    }
}
fn bad()->BodyMutationError{BodyMutationError::Invalid("Current module state key directory")}
fn charge(key:&str,count:usize,cells:&mut u64,bytes:&mut u64)->Result<(),BodyMutationError>{
    let comparisons=(u64::from(usize::BITS-count.max(1).leading_zeros())+2)*32;
    *cells=cells.checked_sub(comparisons).ok_or(ValueTreeError::ResourceLimited("Current module state key cells"))?;
    let n=(key.len()as u64).checked_add(1).and_then(|n|n.checked_mul(comparisons)).ok_or_else(bad)?;
    *bytes=bytes.checked_sub(n).ok_or(ValueTreeError::ResourceLimited("Current module state key bytes"))?;Ok(())
}
fn insert(keys:&mut BTreeMap<String,MemoryReservation>,key:&str,memory:&MemoryBudget,heap:&mut usize)->Result<(),BodyMutationError>{
    let n=key.len().checked_add(4096).ok_or_else(bad)?;
    *heap=heap.checked_sub(n).ok_or(ValueTreeError::ResourceLimited("Current module state key heap"))?;
    let fee=memory.reserve(n,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
    if keys.insert(key.to_owned(),fee).is_some(){return Err(bad())}Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn read_current_module_state_keys_v1<M,P,R,E,C>(
    qualification:&ReservedValue<ModuleStateFamilyQualificationV1>,roots:&RootSetV1,state:&P,
    ctx:&mut WorldFieldOverlayContextV1<'_, '_, '_,M,R,E,C>,scan:&mut StateScanBudget,
    cells:&mut u64,pending:&mut usize,max_heap:usize,guard:&PreparedGuard,live:&ExecutionFence,
)->Result<ReservedValue<PaidCurrentModuleStateKeysV1>,BodyMutationError>
where M:MerkleNodeReader,P:MerkleNodeReader,R:ValueNodeReader,
    E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool {
    ctx.transaction.contribute(|tx|{
        tx.validate_guard(guard,live)?;
        validate_module_state_transaction_v1(qualification,tx,ctx.values.memory)?;
        qualification.validate_binding(roots)?;
        if ctx.values.class!=MemoryClass::Ordinary
            ||state.allocation_class()!=Some(MemoryClass::Ordinary)
            ||!state.allocation_budget().is_some_and(|m|m.same_pool(ctx.values.memory)){return Err(bad())}
        let layout=std::mem::size_of::<PaidCurrentModuleStateKeysV1>()+128;
        let mut heap=max_heap.checked_sub(layout).ok_or(ValueTreeError::ResourceLimited("Current module state directory heap"))?;
        let fee=ctx.values.memory.reserve(layout,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
        let mut keys=BTreeMap::new();
        for key in qualification.state_keys(){
            if (ctx.cancelled)(){return Err(ValueTreeError::Cancelled.into())}
            scan.nodes_remaining=scan.nodes_remaining.checked_sub(1).ok_or(ValueTreeError::ResourceLimited("Current module state resident keys"))?;
            charge(key,keys.len(),cells,&mut ctx.key_work.bytes_remaining)?;
            insert(&mut keys,key,ctx.values.memory,&mut heap)?;
        }
        let cancel=RefCell::new(&mut ctx.cancelled);
        for ns in [StateNamespaceV1::Business,StateNamespaceV1::Allocator,StateNamespaceV1::Scheduler,StateNamespaceV1::Authorization,StateNamespaceV1::Effects,StateNamespaceV1::Artifacts]{
            tx.visit_pending_writes::<BodyMutationError>(ns,pending,
                ||if (cancel.borrow_mut())(){Err(ValueTreeError::Cancelled.into())}else{Ok(())},
                |raw,before,after|{
                    if !ModuleStateKeyV1::is_state_key(raw){return Ok(())}
                    if ns!=StateNamespaceV1::Business{return Err(bad())}
                    let key=ModuleStateKeyV1::state_key_from_bytes(raw).map_err(BodyMutationError::Invalid)?;
                    charge(key,keys.len(),cells,&mut ctx.key_work.bytes_remaining)?;
                    if keys.contains_key(key)!=before.is_some(){return Err(bad())}
                    match (before.is_some(),after.is_some()){
                        (false,true)=>insert(&mut keys,key,ctx.values.memory,&mut heap)?,
                        (true,false)=>{keys.remove(key);},_=>{},
                    }Ok(())
                })?;
        }
        if (cancel.borrow_mut())(){return Err(ValueTreeError::Cancelled.into())}
        tx.validate_guard(&tx.guard(),live)?;
        Ok(fee.retain(PaidCurrentModuleStateKeysV1{keys,guard:tx.guard(),parent:tx.parent_fence().parent_roots}))
    })
}
