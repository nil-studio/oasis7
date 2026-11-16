//! Sparse original Close write set. Every expected field is derived from a
//! genuine selected record proof and latest Current; callers supply no roots.
//! This structural writer grants no business or publication authority.
use super::*;
pub struct EmergencyNeedReservationChangeV1<'a>{pub selected:&'a ReservedValue<CurrentEmergencyNeedV1>,pub reserved:i64}
pub struct EmergencyBatchReleaseV1<'a>{pub selected:&'a ReservedValue<CurrentEmergencyBatchV1>}
#[derive(Clone,Copy)]enum Scalar {I64(i64),Bool(bool)}
impl Scalar{fn encoded(self)->Encoded<'static>{Encoded::Scalar(match self{Self::I64(v)=>MemberPayloadRefV1::I64(v),Self::Bool(v)=>MemberPayloadRefV1::Bool(v)})}}
struct Path {keys:Vec<ReservedValue<Vec<u8>>>,expected:[u8;32],after:Scalar}
fn record_body<M,R,E,C>(source:&ReservedValue<CurrentEmergencyLifecycleSourceV1>,selected:&CurrentEmergencyRecordCoreV1,collection:&'static str,ctx:&mut WorldFieldOverlayContextV1<'_,'_,'_,M,R,E,C>,work:&TypedRecordReadWorkV1)->Result<ReservedValue<StateRootV1>,WorldFieldOverlayErrorV1>
where M:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool{
 ctx.transaction.validate_transaction_identity(&selected.identity)?;
 if selected.parent!=source.parent||selected.collection!=collection||!selected.memory.same_pool(ctx.values.memory)||selected._fee.bytes()==0{return Err(bad().into());}
 let nested=collection_root(source.overlay.body(),collection,ctx,work)?;let p=WorldRuleCompositeBranchV1::EmergencySupply.profile();
 let mut records=InventoryValueReader {values:&mut ctx.values,key_work:&mut *ctx.key_work,semantic_bytes:ctx.semantic_bytes,record_bytes:ctx.record_bytes,cancelled:&mut ctx.cancelled};
 let key=records.key(p.field,WorldFieldRecordKindV1::Entry,&("entry",selected.outer_id.as_str()))?.0;
 let value=crate::incremental_record_tree::lookup(&nested,&key,&mut records,p.labels)?.ok_or_else(bad)?;
 if Some(value)!=selected.expected{return Err(bad().into());}
 let _phase=records.values.memory.reserve(key.as_bytes().len()+size_of::<StateEntryV1>()+512,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
 let row=read_column_row_v1(ColumnReadProfileV1 {field:p.field,schema:p.schema,groups:&[],reference_schema:p.reference_schema},&StateEntryV1 {namespace:StateNamespaceV1::Authorization,key:key.as_bytes().to_vec(),value},&mut records,work.max_tree_heap)?;
 let ColumnCellV1::Root(root)=&row.cell else{return Err(bad().into());};
 let fee=records.values.memory.reserve(size_of::<StateRootV1>()+512,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
 Ok(fee.retain(root.clone()))
}
fn field_leaf<M,R,E,C>(body:&StateRootV1,name:&str,expected:Scalar,ctx:&mut WorldFieldOverlayContextV1<'_,'_,'_,M,R,E,C>,work:&TypedRecordReadWorkV1)->Result<([u8;32],Option<ReservedValue<StateRootV1>>),WorldFieldOverlayErrorV1>
where M:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool{
 let p=WorldRuleCompositeBranchV1::EmergencySupply.profile();let mut records=InventoryValueReader {values:&mut ctx.values,key_work:&mut *ctx.key_work,semantic_bytes:ctx.semantic_bytes,record_bytes:ctx.record_bytes,cancelled:&mut ctx.cancelled};
 let key=records.key(p.field,WorldFieldRecordKindV1::Entry,name)?.0;let value=crate::incremental_record_tree::lookup(body,&key,&mut records,p.labels)?.ok_or_else(bad)?;
 let _phase=records.values.memory.reserve(key.as_bytes().len()+size_of::<StateEntryV1>()+512,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
 let row=read_column_row_v1(ColumnReadProfileV1 {field:p.field,schema:p.schema,groups:&[],reference_schema:p.reference_schema},&StateEntryV1 {namespace:StateNamespaceV1::Authorization,key:key.as_bytes().to_vec(),value},&mut records,work.max_tree_heap)?;
 match (&row.cell,expected){(ColumnCellV1::I64(a),Scalar::I64(b)) if *a==b=>Ok((value,None)),(ColumnCellV1::Bool(a),Scalar::Bool(b))if *a==b=>Ok((value,None)),(ColumnCellV1::Root(root),_) if name=="allocation"=>{let fee=records.values.memory.reserve(size_of::<StateRootV1>()+512,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;Ok((value,Some(fee.retain(root.clone()))))},_=>Err(bad().into())}
}
fn allocation_leaf<M,R,E,C>(body:&StateRootV1,id:&str,amount:i64,ctx:&mut WorldFieldOverlayContextV1<'_,'_,'_,M,R,E,C>,work:&TypedRecordReadWorkV1)->Result<[u8;32],WorldFieldOverlayErrorV1>
where M:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool{
 let p=WorldRuleCompositeBranchV1::EmergencySupply.profile();let mut records=InventoryValueReader {values:&mut ctx.values,key_work:&mut *ctx.key_work,semantic_bytes:ctx.semantic_bytes,record_bytes:ctx.record_bytes,cancelled:&mut ctx.cancelled};
 let key=records.key(p.field,WorldFieldRecordKindV1::Entry,&("entry",id))?.0;let value=crate::incremental_record_tree::lookup(body,&key,&mut records,p.labels)?.ok_or_else(bad)?;
 let _phase=records.values.memory.reserve(key.as_bytes().len()+size_of::<StateEntryV1>()+512,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
 let row=read_column_row_v1(ColumnReadProfileV1 {field:p.field,schema:p.schema,groups:&[],reference_schema:p.reference_schema},&StateEntryV1 {namespace:StateNamespaceV1::Authorization,key:key.as_bytes().to_vec(),value},&mut records,work.max_tree_heap)?;
 if !matches!(&row.cell,ColumnCellV1::I64(v) if *v==amount){return Err(bad().into());}Ok(value)
}
fn path<W:ValueNodeReader+StatePageWriter,CW:FnMut()->bool>(collection:&str,outer:&str,field:&str,member:Option<&str>,expected:[u8;32],after:Scalar,write:&mut MemberRecordWriteContext<'_,W,CW>)->Result<Path,WorldFieldOverlayErrorV1>{
 let mut keys=Vec::with_capacity(4);if keys.capacity()>4{return Err(bad().into());}
 keys.push(prepare_semantic_path_key_v1(collection,write)?);keys.push(prepare_semantic_path_key_v1(&("entry",outer),write)?);keys.push(prepare_semantic_path_key_v1(field,write)?);
 if let Some(member)=member{keys.push(prepare_semantic_path_key_v1(&("entry",member),write)?);}Ok(Path {keys,expected,after})
}
/// Commit only need.reserved, every supplied batch's entire allocation→zero,
/// and authorization remaining/active. Unknown and unrelated original fields
/// retain their exact authenticated roots, including huge frozen ledgers.
#[allow(clippy::too_many_arguments)]
pub fn commit_current_emergency_close_columns_v1<M,S,R,E,C,W,CW>(source:&ReservedValue<CurrentEmergencyLifecycleSourceV1>,authorization:&ReservedValue<CurrentEmergencyAuthorizationV1>,needs:&[EmergencyNeedReservationChangeV1<'_>],batches:&[EmergencyBatchReleaseV1<'_>],parent:&ReservedValue<WorldRuleCompositeQualificationV1<Emergency>>,roots:&RootSetV1,state:&S,ctx:&mut WorldFieldOverlayContextV1<'_,'_,'_,M,R,E,C>,write:&mut MemberRecordWriteContext<'_,W,CW>,scan:&mut StateScanBudget,read_work:&mut WorldRuleCompositeReadWorkV1,write_work:&mut WorldRuleCompositeWriteWorkV1,guard:&PreparedGuard,live:&ExecutionFence)->Result<ReservedValue<EmergencyOverlay>,WorldFieldOverlayErrorV1>
where M:MerkleNodeReader,S:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool,W:ValueNodeReader+ValueNodeWriter+StatePageWriter,CW:FnMut()->bool{
 let result=(||{
  validate_source(source,parent,roots,ctx,guard,live)?;
  if !write.memory.same_pool(ctx.values.memory)||!authorization.uses_budget(ctx.values.memory)||ctx.values.class!=MemoryClass::Ordinary{return Err(bad().into());}
  let auth=authorization.value().ok_or_else(bad)?;
  let count=batches.iter().try_fold(needs.len().checked_add(2).ok_or_else(bad)?,|n,b|n.checked_add(b.selected.value().ok_or_else(bad)?.allocation.len()).ok_or_else(bad))?;
  let metadata=count.checked_mul(size_of::<Path>()+4*size_of::<ReservedValue<Vec<u8>>>()+size_of::<Vec<&[u8]>>()+4*size_of::<&[u8]>()).ok_or_else(bad)?;
  let _phase=write.memory.reserve(metadata,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
  let mut paths=Vec::new();paths.try_reserve_exact(count).map_err(|_|bad())?;if paths.capacity()>count{return Err(bad().into());}
  let body=record_body(source,&authorization.core,"authorizations",ctx,read_work)?;
  let active=field_leaf(&body,"active",Scalar::Bool(auth.active),ctx,read_work)?.0;
  let remaining=field_leaf(&body,"remaining",Scalar::I64(auth.remaining),ctx,read_work)?.0;
  paths.push(path("authorizations",authorization.outer_id(),"active",None,active,Scalar::Bool(false),write)?);
  paths.push(path("authorizations",authorization.outer_id(),"remaining",None,remaining,Scalar::I64(0),write)?);
  for need in needs{if !need.selected.uses_budget(ctx.values.memory){return Err(bad().into());}let value=need.selected.value().ok_or_else(bad)?;let body=record_body(source,&need.selected.core,"needs",ctx,read_work)?;let leaf=field_leaf(&body,"reserved",Scalar::I64(value.reserved),ctx,read_work)?.0;paths.push(path("needs",need.selected.outer_id(),"reserved",None,leaf,Scalar::I64(need.reserved),write)?);}
  for batch in batches{if !batch.selected.uses_budget(ctx.values.memory){return Err(bad().into());}let value=batch.selected.value().ok_or_else(bad)?;
   // The lookup ID is the authorization's actual outer key, not its embedded
   // id, and committed batches are deliberately included by this operation.
   if value.authorization_id!=authorization.outer_id(){return Err(bad().into());}
   let body=record_body(source,&batch.selected.core,"batches",ctx,read_work)?;
   let(_,allocation)=field_leaf(&body,"allocation",Scalar::I64(0),ctx,read_work)?;let allocation=allocation.ok_or_else(bad)?;
   for(id,amount)in &value.allocation{let leaf=allocation_leaf(&allocation,id,*amount,ctx,read_work)?;paths.push(path("batches",batch.selected.outer_id(),"allocation",Some(id),leaf,Scalar::I64(0),write)?);}
  }
  if paths.len()!=count{return Err(bad().into());}
  let mut refs=Vec::new();refs.try_reserve_exact(count).map_err(|_|bad())?;if refs.capacity()>count{return Err(bad().into());}
  for p in &paths{let mut keys=Vec::with_capacity(4);if keys.capacity()>4{return Err(bad().into());}keys.extend(p.keys.iter().map(|k|k.as_slice()));refs.push(keys);}
  if (ctx.cancelled)()||(write.cancelled)(){return Err(ValueTreeError::Cancelled.into());}
  let guard=ctx.transaction.guard();let changes=paths.iter().zip(&refs).map(|(p,keys)|CompositePathPatchV1 {path:keys.as_slice(),expected:Some(p.expected),after:Some(p.after.encoded())});
  patch_world_rule_branch_v1(parent,roots,Some(&source.overlay),changes,state,ctx,write,scan,read_work,write_work,&guard,live)?.ok_or_else(||bad().into())
 })();if result.is_err(){let _:Result<(),ExecutionError>=ctx.transaction.contribute(|_|Err(ExecutionError::Aborted));}result
}
