//! Full original release rows in a paid persistent AVL directory.
use super::*;
use std::{sync::Arc,cmp::Ordering};
use oasis7_proto::memory_budget::MemoryReservation;
use crate::incremental_world_native_gameplay::ModuleReleaseRequestsFieldV1 as ReleaseField;
pub(crate) struct ReleaseRowV1{pub key:u64,pub outer:[u8;32],pub record:WorldNativeRecoveredRecordV1<crate::state::ModuleReleaseRequestState>,pub bytes:usize,pub class:MemoryClass}
#[path="incremental_world_native_release_tree.rs"]
mod tree;
#[derive(Clone)]
pub(crate) struct ReleaseRowsV1{root:Option<tree::NodeRef>,pub count:u64,class:MemoryClass}
fn step(work:&mut u64,c:&mut impl FnMut()->bool)->Result<(),ValueTreeError>{if c(){return Err(ValueTreeError::Cancelled)}*work=work.checked_sub(1).ok_or(ValueTreeError::ResourceLimited("release directory work"))?;Ok(())}
fn compare(a:u64,b:u64,work:&mut u64,c:&mut impl FnMut()->bool)->Result<Ordering,ValueTreeError>{step(work,c)?;Ok(a.cmp(&b))}
impl ReleaseRowsV1{
 pub(crate) fn empty(class:MemoryClass)->Self{Self{root:None,count:0,class}}
 pub(crate) fn coverage(&self,memory:&MemoryBudget)->Result<Option<MemoryReservation>,ValueTreeError>{if self.class==MemoryClass::Ordinary{Ok(None)}else{Ok(Some(memory.reserve(self.root.as_ref().map_or(0,|n|n.bytes),MemoryClass::Ordinary)?))}}
 pub(crate) fn replace<M:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool>(&mut self,key:u64,outer:Option<[u8;32]>,body:Option<&StateRootV1>,ctx:&mut WorldFieldReadContextV1<'_,M,R,E,C>,scan:&mut StateScanBudget,typed:&mut crate::incremental_typed_record_codec::TypedRecordReadWorkV1,cells:&mut u64)->Result<(),ValueTreeError>{
   if let Some(body)=body{
     let profile=ReleaseField::profile()?;
     let record=recover_typed_record_body_v1::<WorldNativeRecordV1<crate::state::ModuleReleaseRequestState>,_,_,_,_>(body,&profile,ctx,scan,typed)?;
     let bytes=record.retained_allocation_bytes_v1(&mut typed.json.cells_remaining,&mut ctx.cancelled)?.checked_add(std::mem::size_of::<ReleaseRowV1>()+4096).ok_or_else(invalid)?;
     ctx.key_work.bytes_remaining=ctx.key_work.bytes_remaining.checked_sub(bytes as u64).ok_or(ValueTreeError::ResourceLimited("release row retain"))?;
     let fee=ctx.values.memory.reserve(std::mem::size_of::<ReleaseRowV1>()+4096,ctx.values.class)?;
     let row=Arc::new(fee.retain(ReleaseRowV1{key,outer:outer.ok_or_else(invalid)?,record,bytes,class:ctx.values.class}));
     let (root,new)=tree::put(&self.root,row,ctx.values.memory,ctx.values.class,cells,&mut ctx.cancelled)?;self.root=Some(root);if new{self.count=self.count.checked_add(1).ok_or_else(invalid)?;}
   }else{let(root,removed)=tree::remove(&self.root,key,ctx.values.memory,ctx.values.class,cells,&mut ctx.cancelled)?;if !removed{return Err(invalid())}self.root=root;self.count=self.count.checked_sub(1).ok_or_else(invalid)?;}
   Ok(())
 }
 pub(crate) fn visit<E:From<ValueTreeError>>(&self,cells:&mut u64,c:&mut impl FnMut()->bool,emit:&mut impl FnMut(u64,&crate::state::ModuleReleaseRequestState,&mut u64)->Result<(),E>)->Result<(),E>{tree::visit(&self.root,cells,c,&mut|row,work|emit(row.key,row.record.value().value(),work))}
 pub(crate) fn find(&self,key:u64,cells:&mut u64,c:&mut impl FnMut()->bool)->Result<Option<&ReleaseRowV1>,ValueTreeError>{tree::find(&self.root,key,cells,c)}
}
pub(crate) fn original(bytes:&[u8])->Result<u64,ValueTreeError>{match crate::incremental_semantic_key::read_semantic_key_v1(bytes).map_err(ValueTreeError::Invalid)?{crate::incremental_semantic_key::SemanticKeyRefV1::Unsigned(id)=>Ok(id),_=>Err(invalid())}}
