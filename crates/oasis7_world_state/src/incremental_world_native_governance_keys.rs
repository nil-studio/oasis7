//! Private accepted native-governance map keys. No key is a record authority.
use super::*;
use oasis7_proto::memory_budget::MemoryReservation;
use std::{collections::BTreeMap,sync::Arc};
pub(crate) fn tracked<F:WorldNativeFieldSpecV1>()->bool{matches!(F::NAME,"governance_validator_admissions"|"governance_finality_key_rotations"|"governance_bootstrap_finality_key_rotations"|"module_release_requests")}
pub(crate) struct GovernanceKeysV1{keys:BTreeMap<Vec<u8>,MemoryReservation>,_layout:MemoryReservation,bytes:usize,class:MemoryClass}
impl GovernanceKeysV1{
    pub(crate) fn empty(memory:&MemoryBudget,class:MemoryClass)->Result<Self,ValueTreeError>{let fee=memory.reserve(std::mem::size_of::<Self>()+128,class)?;let bytes=fee.bytes();Ok(Self{keys:BTreeMap::new(),_layout:fee,bytes,class})}
    pub(crate) fn len(&self)->usize{self.keys.len()}
    pub(crate) fn iter(&self)->impl ExactSizeIterator<Item=&[u8]>{self.keys.keys().map(Vec::as_slice)}
    pub(crate) fn pay(key:&[u8],count:usize,ctx:&mut WorldFieldReadContextV1<'_,impl MerkleNodeReader,impl ValueNodeReader,impl FnMut()->Option<ValueTreeError>,impl FnMut()->bool>,work:&mut WorldNativeFieldReadWorkV1)->Result<(),ValueTreeError>{
        read::check(ctx)?;let n=(u64::from(usize::BITS-count.max(1).leading_zeros())+2)*32;
        work.records_remaining=work.records_remaining.checked_sub(n).ok_or(ValueTreeError::ResourceLimited("governance directory cells"))?;
        let bytes=(key.len()as u64+1).checked_mul(n).ok_or_else(invalid)?;
        ctx.key_work.bytes_remaining=ctx.key_work.bytes_remaining.checked_sub(bytes).ok_or(ValueTreeError::ResourceLimited("governance directory bytes"))?;Ok(())
    }
    pub(crate) fn insert(&mut self,key:&[u8],memory:&MemoryBudget,class:MemoryClass)->Result<(),ValueTreeError>{
        if self.keys.contains_key(key){return Err(invalid())}let fee=memory.reserve(key.len().checked_add(4096).ok_or_else(invalid)?,class)?;
        self.bytes=self.bytes.checked_add(fee.bytes()).ok_or_else(invalid)?;self.keys.insert(key.to_vec(),fee);Ok(())
    }
    pub(crate) fn copy(&self,ctx:&mut WorldFieldReadContextV1<'_,impl MerkleNodeReader,impl ValueNodeReader,impl FnMut()->Option<ValueTreeError>,impl FnMut()->bool>,work:&mut WorldNativeFieldReadWorkV1)->Result<Self,ValueTreeError>{
        let mut next=Self::empty(ctx.values.memory,MemoryClass::Ordinary)?;for key in self.iter(){Self::pay(key,self.len(),ctx,work)?;next.insert(key,ctx.values.memory,MemoryClass::Ordinary)?;}Ok(next)
    }
    pub(crate) fn change(&mut self,key:&[u8],before:bool,after:bool,memory:&MemoryBudget)->Result<(),ValueTreeError>{
        if self.keys.contains_key(key)!=before{return Err(invalid())}match(before,after){(false,true)=>self.insert(key,memory,MemoryClass::Ordinary)?,(true,false)=>{if let Some(fee)=self.keys.remove(key){self.bytes=self.bytes.checked_sub(fee.bytes()).ok_or_else(invalid)?;}},_=>{}}Ok(())
    }
    pub(crate) fn ordinary_coverage(&self,memory:&MemoryBudget)->Result<Option<MemoryReservation>,ValueTreeError>{if !self._layout.uses_budget(memory){return Err(invalid())}if self.class==MemoryClass::Ordinary{Ok(None)}else{Ok(Some(memory.reserve(self.bytes,MemoryClass::Ordinary)?))}}
}
pub(crate) fn retain(value:GovernanceKeysV1)->Arc<GovernanceKeysV1>{Arc::new(value)}
