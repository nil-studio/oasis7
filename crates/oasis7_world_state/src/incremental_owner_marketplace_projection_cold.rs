//! One-time actual accepted-root enumeration. Stable IDs, AVL nodes and the
//! allocator are read from the qualified physical family, never a logical Vec.
use super::*;
pub struct QualifiedMarketplaceProjectionColumnsV1{data:CurrentMarketplaceProjectionDeltaV1}
impl QualifiedMarketplaceProjectionColumnsV1{
 pub fn validate<M:MerkleNodeReader>(&self,tx:&ExecutionTransaction<'_,M>,memory:&MemoryBudget,live:&ExecutionFence)->Result<(),BodyMutationError>{tx.validate_bootstrap_read_only_v1(live)?;self.data.validate(tx,memory,live)}
 pub fn projection_data_v1(&self)->&CurrentMarketplaceProjectionDeltaV1{&self.data}
}
pub fn read_qualified_marketplace_projection_columns_v1<M:MerkleNodeReader,P:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool>(family:&ReservedValue<ModuleMarketplaceFamilyQualificationV1>,roots:&RootSetV1,state:&P,ctx:&mut WorldFieldOverlayContextV1<'_, '_, '_,M,R,E,C>,scan:&mut StateScanBudget,cells:&mut u64,p:&mut BusinessDiffProcessingWorkV1,max:u64,guard:&PreparedGuard,live:&ExecutionFence)->Result<QualifiedMarketplaceProjectionColumnsV1,BodyMutationError>{let result=(||{
 ctx.transaction.validate_guard(guard,live)?;ctx.transaction.validate_bootstrap_read_only_v1(live)?;
 let memory=ctx.values.memory.clone();if ctx.values.class!=MemoryClass::Ordinary||!family.uses_budget(&memory)||family.allocation_class()!=MemoryClass::Ordinary||!ctx.transaction.uses_budget(&memory){return Err(invalid().into())}family.validate_binding(roots).map_err(owner)?;if roots.digest()?!=ctx.transaction.parent_fence().parent_roots{return Err(invalid().into())}
 let expected=family.record_counts();let count=expected.iter().try_fold(0usize,|total,n|total.checked_add(usize::try_from(*n).map_err(|_|limited())?).ok_or_else(limited))?;
 let mut changed_fee=reserve(&memory,p,count.checked_mul(size_of::<Change>()).ok_or_else(limited)?)?;
 let mut changes=Vec::with_capacity(count);if changes.capacity()!=count{return Err(limited().into())}
 let mut counts=[0u64;3];let mut metadata=[false;3];
 {
  let stop=RefCell::new(&mut ctx.cancelled);
  StateScanContext{source:state,memory:&memory,class:MemoryClass::Ordinary,io:scan}.visit_entries_with_reader(roots.state_root(StateNamespaceV1::Business),|entry,_reader|{
   if(stop.borrow_mut())(){return Err(ValueTreeError::Cancelled.into())}pay(p,entry.key.len())?;
   let Some(kind)=identify(&entry.key)?else{return Ok(())};if entry.namespace!=StateNamespaceV1::Business{return Err(invalid().into())}
   let index=Kind::ALL.into_iter().position(|candidate|candidate==kind).ok_or_else(invalid)?;
   let parsed=WorldFieldKeyV1::from_bytes(&entry.key).map_err(ValueTreeError::Invalid)?;
   if parsed.kind==WorldFieldRecordKindV1::Metadata{
    if metadata[index]{return Err(invalid().into())}let key=WorldFieldKeyV1::new(kind.descriptor().map_err(|_|invalid())?,parsed.kind,parsed.semantic_key).map_err(ValueTreeError::Invalid)?;
    let mut cancelled=||(stop.borrow_mut())();let mut records=InventoryValueReader{values:&mut ctx.values,key_work:&mut*ctx.key_work,semantic_bytes:ctx.semantic_bytes,record_bytes:ctx.record_bytes,cancelled:&mut cancelled};records.presence(&entry.value,&key,0)?;metadata[index]=true;return Ok(())
   }
   if parsed.kind!=WorldFieldRecordKindV1::Entry||changes.len()==count{return Err(invalid().into())}
   counts[index]=counts[index].checked_add(1).ok_or_else(limited)?;
   let mut fee=reserve(&memory,p,entry.key.len())?;changed_fee.merge(&mut fee).map_err(ValueTreeError::Invalid)?;
   let mut key=Vec::with_capacity(entry.key.len());if key.capacity()!=entry.key.len(){return Err(limited().into())}for chunk in entry.key.chunks(8192){if(stop.borrow_mut())(){return Err(ValueTreeError::Cancelled.into())}pay(p,chunk.len())?;key.extend_from_slice(chunk);}
   // This is an actual accepted leaf, not a synthetic pending mutation.
   changes.push(Change{kind,key,before:None,after:Some(entry.value)});Ok::<_,BodyMutationError>(())
  },||(stop.borrow_mut())()).map_err(scan_error)?;
 }
 if counts!=expected||metadata!=[true;3]||changes.len()!=count{return Err(invalid().into())}
 let fee=reserve(&memory,p,count.checked_mul(size_of::<Entity>()).and_then(|n|n.checked_add(size_of::<QualifiedMarketplaceProjectionColumnsV1>())).ok_or_else(limited)?)?;
 let mut entities=Vec::with_capacity(count);if entities.capacity()!=count{return Err(limited().into())}
 for change in changes{entities.push(decode_projection_entity_v1(change,state,ctx,scan,cells,p,max)?);}
 if let Some(error)=(ctx.take_state_error)(){return Err(error.into())}if(ctx.cancelled)(){return Err(ValueTreeError::Cancelled.into())}pay(p,0)?;ctx.transaction.validate_bootstrap_read_only_v1(live)?;
 let out=QualifiedMarketplaceProjectionColumnsV1{data:CurrentMarketplaceProjectionDeltaV1{entities,guard:ctx.transaction.guard(),parent:ctx.transaction.parent_fence().parent_roots,memory,_fee:fee}};out.validate(ctx.transaction,ctx.values.memory,live)?;Ok(out)
 })();if let Some(error)=(ctx.take_state_error)(){return Err(error.into())}if(ctx.cancelled)(){return Err(ValueTreeError::Cancelled.into())}result}
