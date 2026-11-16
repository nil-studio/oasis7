use super::*;
struct Pending{key:Vec<u8>,before:Option<[u8;32]>,after:Option<[u8;32]>}
fn fold<M:MerkleNodeReader,P:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool>(directory:&ReservedValue<ThreatKeyDirectoryV1>,q:&ReservedValue<RuntimeBusinessQualificationV1>,roots:&RootSetV1,tx:&mut ExecutionTransaction<'_,M>,ctx:&mut WorldFieldReadContextV1<'_,P,R,E,C>,work:&mut ThreatKeyDirectoryWorkV1)->Result<(Option<tree::NodeRef>,u64),WorldFieldOverlayErrorV1>{
 parents(q,roots,ctx,work)?;work.step(root_bytes(roots)?,&mut ctx.cancelled)?;directory.validate_binding(roots,ctx.values.memory)?;
 if !directory.uses_budget(ctx.values.memory)||!tx.uses_budget(ctx.values.memory)||directory.roots!=tx.parent_fence().parent_roots||directory.count!=q.heatmap_entries(){return Err(invalid().into())}
 // The original field has a fixed empty Presence metadata envelope, not a
 // caller-provided entry count. Authenticate its actual Current value too.
 {use crate::{incremental_world_field_key::{WorldFieldKeyV1,WorldFieldRecordKindV1},incremental_runtime_business::RuntimeBusinessFieldV1};let metadata=WorldFieldKeyV1::new(RuntimeBusinessFieldV1::ThreatHeatmap.descriptor(),WorldFieldRecordKindV1::Metadata,None).map_err(ValueTreeError::Invalid)?;work.step(metadata.as_bytes().len(),&mut ctx.cancelled)?;let root=tx.read(StateNamespaceV1::Business,metadata.as_bytes())?.ok_or_else(invalid)?;ctx.records().presence(&root,&metadata,0)?;}
 let stop=RefCell::new(&mut ctx.cancelled);let mut count=0usize;let mut payload=0usize;let mut remaining=work.pending_entries_remaining;
 let result=tx.visit_pending_writes(StateNamespaceV1::Business,&mut remaining,||{if (stop.borrow_mut())(){return Err(WorldFieldOverlayErrorV1::from(ValueTreeError::Cancelled))}Ok(())},|key,before,after|{work.step(key.len(),&mut ||(stop.borrow_mut())())?;if before!=after&&matches_key(key)?{count=count.checked_add(1).ok_or_else(limited)?;payload=payload.checked_add(key.len()).ok_or_else(limited)?;}Ok(())});work.pending_entries_remaining=remaining;result?;
 let bytes=count.checked_mul(size_of::<Pending>()).and_then(|n|n.checked_add(payload)).ok_or_else(limited)?;let _phase=ctx.values.memory.reserve(bytes,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;let mut pending=Vec::new();pending.try_reserve_exact(count).map_err(|_|limited())?;if pending.capacity()>count{return Err(limited().into())}
 let result=tx.visit_pending_writes(StateNamespaceV1::Business,&mut remaining,||{if (stop.borrow_mut())(){return Err(WorldFieldOverlayErrorV1::from(ValueTreeError::Cancelled))}Ok(())},|key,before,after|{work.step(key.len(),&mut ||(stop.borrow_mut())())?;if before!=after&&matches_key(key)?{if pending.len()>=count{return Err(invalid().into())}pending.push(Pending{key:key.to_vec(),before,after});}Ok(())});work.pending_entries_remaining=remaining;result?;drop(stop);
 let mut tree=directory.tree.clone();let mut size=directory.count;
 for change in pending{
  let mut entry=StateEntryV1{namespace:StateNamespaceV1::Business,key:change.key,value:[0;32]};
  work.step(entry.key.len(),&mut ctx.cancelled)?;
  if tx.read(StateNamespaceV1::Business,&entry.key)?!=change.after{return Err(invalid().into())}
  let before=change.before.map(|value|{entry.value=value;inspect(&entry,ctx,work)}).transpose()?;
  let after=change.after.map(|value|{entry.value=value;inspect(&entry,ctx,work)}).transpose()?;
  let identity=after.as_ref().or(before.as_ref()).ok_or_else(invalid)?;
  if let(Some(before),Some(after))=(&before,&after){work.step(before.key.len().min(after.key.len()),&mut ctx.cancelled)?;if before.key!=after.key{return Err(invalid().into())}}
  let old=tree::find(&tree,&identity.key,work,&mut ctx.cancelled)?;
  if old.map(|row|row.member)!=change.before{return Err(invalid().into())}
  if let Some(old)=old{work.step(old.entry_key.len().min(entry.key.len()),&mut ctx.cancelled)?;if old.entry_key!=entry.key{return Err(invalid().into())}}
  if let Some(after)=after{let(next,new)=tree::put(&tree,after,ctx.values.memory,work,&mut ctx.cancelled)?;if new!=change.before.is_none(){return Err(invalid().into())}tree=Some(next);if new{size=size.checked_add(1).ok_or_else(limited)?;}}
  else{let before=before.as_ref().ok_or_else(invalid)?;let(next,removed)=tree::remove(&tree,&before.key,ctx.values.memory,work,&mut ctx.cancelled)?;if !removed{return Err(invalid().into())}tree=next;size=size.checked_sub(1).ok_or_else(invalid)?;}
 }
 if size>work.max_rows as u64{return Err(limited().into())}ctx.check()?;Ok((tree,size))
}
#[allow(clippy::too_many_arguments)]
pub fn read_current_threat_key_directory_v1<M:MerkleNodeReader,P:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool>(directory:&ReservedValue<ThreatKeyDirectoryV1>,q:&ReservedValue<RuntimeBusinessQualificationV1>,roots:&RootSetV1,tx:&mut ExecutionTransaction<'_,M>,ctx:&mut WorldFieldReadContextV1<'_,P,R,E,C>,_scan:&mut StateScanBudget,work:&mut ThreatKeyDirectoryWorkV1,guard:&PreparedGuard,live:&ExecutionFence)->Result<ReservedValue<CurrentThreatKeyDirectoryV1>,WorldFieldOverlayErrorV1>{tx.contribute(|tx|{tx.validate_guard(guard,live)?;let(tree,_)=fold(directory,q,roots,tx,ctx,work)?;let fee=ctx.values.memory.reserve(size_of::<CurrentThreatKeyDirectoryV1>(),MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;let guard=tx.guard();tx.validate_guard(&guard,live)?;Ok(fee.retain(CurrentThreatKeyDirectoryV1{tree,guard,parent:tx.parent_fence().parent_roots,memory:ctx.values.memory.clone()}))})}
#[allow(clippy::too_many_arguments)]
pub fn freeze_current_threat_key_directory_v1<M:MerkleNodeReader,P:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool>(directory:&ReservedValue<ThreatKeyDirectoryV1>,q:&ReservedValue<RuntimeBusinessQualificationV1>,roots:&RootSetV1,tx:&mut ExecutionTransaction<'_,M>,ctx:&mut WorldFieldReadContextV1<'_,P,R,E,C>,_scan:&mut StateScanBudget,work:&mut ThreatKeyDirectoryWorkV1,guard:&PreparedGuard,live:&ExecutionFence)->Result<ReservedValue<FrozenThreatKeyDirectoryV1>,WorldFieldOverlayErrorV1>{tx.contribute(|tx|{tx.validate_guard(guard,live)?;let(tree,count)=fold(directory,q,roots,tx,ctx,work)?;let fee=ctx.values.memory.reserve(size_of::<FrozenThreatKeyDirectoryV1>(),MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;let guard=tx.guard();tx.validate_guard(&guard,live)?;Ok(fee.retain(FrozenThreatKeyDirectoryV1{tree,count,guard,memory:ctx.values.memory.clone()}))})}
