//! Actual Current listing records for the fixed module Action owner. Original
//! marketplace schema/member/record-root codecs are reused without a new grammar.
use super::*;
use crate::incremental_module_marketplace::ModuleMarketplaceFamilyQualificationV1;
use oasis7_formal_protocol::incremental_execution::{ExecutionTransaction,RootSetV1};
use crate::incremental_field_member::MemberPayloadCodecV1;
use oasis7_proto::memory_budget::MemoryReservation;
pub struct CurrentModuleLifecycleListingV1{
 key:String,outer:WorldFieldKeyV1,before:Option<[u8;32]>,value:Option<ModuleArtifactListingState>,
 guard:PreparedGuard,parent:[u8;32],_fee:MemoryReservation,
}
impl CurrentModuleLifecycleListingV1{
 pub fn value(&self)->Option<&ModuleArtifactListingState>{self.value.as_ref()}
 pub fn outer_id(&self)->&str{&self.key}
 fn validate<M:MerkleNodeReader>(&self,tx:&ExecutionTransaction<'_,M>,memory:&oasis7_proto::memory_budget::MemoryBudget,live:&ExecutionFence)->Result<(),WorldFieldOverlayErrorV1>{
  tx.validate_transaction_identity(&self.guard)?;tx.validate_guard(&tx.guard(),live)?;
  if !self._fee.uses_budget(memory)||!tx.uses_budget(memory)||tx.parent_fence().parent_roots!=self.parent{return Err(ValueTreeError::Invalid("module lifecycle listing provenance").into())}Ok(())
 }
}
fn bad()->ValueTreeError{ValueTreeError::Invalid("module lifecycle Current listing")}
fn step(cells:&mut u64,c:&mut impl FnMut()->bool)->Result<(),ValueTreeError>{if c(){return Err(ValueTreeError::Cancelled)}*cells=cells.checked_sub(1).ok_or(ValueTreeError::ResourceLimited("module lifecycle selected columns"))?;Ok(())}
fn child_root<M,R,E,C>(field:ModuleMarketplaceFieldV1,key:&str,ctx:&mut WorldFieldOverlayContextV1<'_, '_, '_,M,R,E,C>)->Result<(WorldFieldKeyV1,Option<[u8;32]>,Option<StateRootV1>),WorldFieldOverlayErrorV1>
where M:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool{
 let descriptor=field.descriptor()?;
 let mut records=InventoryValueReader{values:&mut ctx.values,key_work:ctx.key_work,semantic_bytes:ctx.semantic_bytes,record_bytes:ctx.record_bytes,cancelled:&mut ctx.cancelled};
 let metadata=WorldFieldKeyV1::new(descriptor,WorldFieldRecordKindV1::Metadata,None).map_err(ValueTreeError::Invalid)?;
 let root=crate::incremental_field_overlay::lookup(ctx.transaction,&metadata,&mut records,&mut ctx.take_state_error)?.ok_or_else(bad)?;
 records.presence(&root,&metadata,0)?;
 let(outer,semantic)=records.key(descriptor,WorldFieldRecordKindV1::Entry,key)?;
 let before=crate::incremental_field_overlay::lookup(ctx.transaction,&outer,&mut records,&mut ctx.take_state_error)?;
 let Some(value)=before else{return Ok((outer,None,None))};
 let binding=WorldFieldKeyV1::from_bytes(outer.as_bytes()).map_err(ValueTreeError::Invalid)?;
 let owner=binding.semantic_key.ok_or_else(bad)?;let mut bytes=[0u8;oasis7_formal_protocol::incremental_record_root::RECORD_ROOT_BYTES];
 let info=read_member_record_profile_v1(&value,&outer,records.values,MemberReadProfileV1{codec:MemberPayloadCodecV1::RecordRoot,semantic_bytes:semantic,limits:crate::incremental_field_member::MemberRecordLimitsV1{semantic_bytes:records.semantic_bytes,payload_bytes:bytes.len()as u64,record_bytes:records.record_bytes}},&mut std::io::sink(),&mut &mut bytes[..],records.cancelled)?;
 if info.payload_bytes!=bytes.len()as u64{return Err(bad().into())}
 let reference=RecordRootRefV1::from_canonical_bytes(&bytes).map_err(|_|bad())?;
 let root=reference.validate_binding(&field.schema(),&owner,StateNamespaceV1::Business).map_err(|_|bad())?.clone();
 if field==ModuleMarketplaceFieldV1::ArtifactListings && root.leaf_count!=5{return Err(bad().into())}Ok((outer,Some(value),Some(root)))
}
fn column<R:ValueNodeReader,C:FnMut()->bool,K:SemanticKeyV1+?Sized>(root:&StateRootV1,field:ModuleMarketplaceFieldV1,name:&K,codec:MemberPayloadCodecV1,values:&mut oasis7_formal_protocol::incremental_value::ValueReadContext<'_,R>,keys:&mut crate::incremental_semantic_key::SemanticKeyBudget,semantic_limit:u64,record_limit:u64,max_payload:u64,out:&mut impl Write,c:&mut C)->Result<u64,ValueTreeError>{
 let descriptor=field.descriptor().map_err(|_|bad())?;
 let buffer=SemanticKeyBuffer::encode(name)?;let semantic=CanonicalSemanticKeyRefV1::new(buffer.as_slice()).map_err(ValueTreeError::Invalid)?;
 let hash=hash_semantic_key_v1(&semantic,values.memory,values.class,keys,&mut *c).map_err(map_semantic_error)?;
 let key=WorldFieldKeyV1::new(descriptor,WorldFieldRecordKindV1::Entry,Some(hash.digest)).map_err(ValueTreeError::Invalid)?;
 let value=crate::incremental_record_tree::get(root,&key,values,c,RECORD_TREE_LABELS)?.ok_or_else(bad)?;
 let info=read_member_record_profile_v1(&value,&key,values,MemberReadProfileV1{codec,semantic_bytes:hash.encoded_bytes,limits:crate::incremental_field_member::MemberRecordLimitsV1{semantic_bytes:semantic_limit,payload_bytes:max_payload,record_bytes:record_limit}},&mut std::io::sink(),out,c)?;
 Ok(info.payload_bytes)
}
pub fn read_current_module_lifecycle_listing_v1<M,R,E,C>(key:&str,parent:&ReservedValue<ModuleMarketplaceFamilyQualificationV1>,roots:&RootSetV1,ctx:&mut WorldFieldOverlayContextV1<'_, '_, '_,M,R,E,C>,cells:&mut u64,max_text:usize,guard:&PreparedGuard,live:&ExecutionFence)->Result<CurrentModuleLifecycleListingV1,WorldFieldOverlayErrorV1>
where M:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool{
 ctx.transaction.validate_guard(guard,live)?;
 let _phase=ctx.values.memory.reserve(8192,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
 if ctx.values.class!=MemoryClass::Ordinary||!parent.uses_budget(ctx.values.memory)||parent.allocation_class()!=MemoryClass::Ordinary||!ctx.transaction.uses_budget(ctx.values.memory){return Err(bad().into())}
 parent.validate_binding(roots).map_err(|_|bad())?;let digest=roots.digest()?;if digest!=ctx.transaction.parent_fence().parent_roots{return Err(bad().into())}
 step(cells,&mut ctx.cancelled)?;let (outer,before,root)=child_root(ModuleMarketplaceFieldV1::ArtifactListings,key,ctx)?;
 let mut text=Vec::new();let mut text_fee=None;
 let value=if let Some(root)=root{
  step(cells,&mut ctx.cancelled)?;
  let len=column(&root,ModuleMarketplaceFieldV1::ArtifactListings,"seller_agent_id",MemberPayloadCodecV1::Text,&mut ctx.values,ctx.key_work,ctx.semantic_bytes,ctx.record_bytes,max_text as u64,&mut std::io::sink(),&mut ctx.cancelled)?;
  let len=usize::try_from(len).map_err(|_|bad())?;let fee=ctx.values.memory.reserve(len,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
  text.try_reserve_exact(len).map_err(|_|ValueTreeError::ResourceLimited("module listing seller allocation"))?;if text.capacity()>len{return Err(ValueTreeError::ResourceLimited("module listing seller capacity").into())}
  step(cells,&mut ctx.cancelled)?;
  let actual=column(&root,ModuleMarketplaceFieldV1::ArtifactListings,"seller_agent_id",MemberPayloadCodecV1::Text,&mut ctx.values,ctx.key_work,ctx.semantic_bytes,ctx.record_bytes,max_text as u64,&mut text,&mut ctx.cancelled)?;if actual!=len as u64{return Err(bad().into())}text_fee=Some(fee);
  let mut scalar=|name:&str,codec|->Result<[u8;8],WorldFieldOverlayErrorV1>{step(cells,&mut ctx.cancelled)?;let mut bytes=[0u8;8];let n=column(&root,ModuleMarketplaceFieldV1::ArtifactListings,name,codec,&mut ctx.values,ctx.key_work,ctx.semantic_bytes,ctx.record_bytes,8,&mut &mut bytes[..],&mut ctx.cancelled)?;if n!=8{return Err(bad().into())}Ok(bytes)};
  let order_id=u64::from_be_bytes(scalar("order_id",MemberPayloadCodecV1::U64)?);
  let price_kind=match u64::from_be_bytes(scalar("price_kind",MemberPayloadCodecV1::U64)?){0=>ResourceKind::Electricity,1=>ResourceKind::Data,_=>return Err(bad().into())};
  let price_amount=i64::from_be_bytes(scalar("price_amount",MemberPayloadCodecV1::I64)?);
  let listed_at=u64::from_be_bytes(scalar("listed_at",MemberPayloadCodecV1::U64)?);
  Some(ModuleArtifactListingState{order_id,seller_agent_id:String::from_utf8(text).map_err(|_|bad())?,price_kind,price_amount,listed_at})
 }else{None};
 let mut fee=ctx.values.memory.reserve(std::mem::size_of::<CurrentModuleLifecycleListingV1>()+key.len()+512,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
 if let Some(mut text)=text_fee{fee.merge(&mut text).map_err(ValueTreeError::Invalid)?;}
 Ok(CurrentModuleLifecycleListingV1{key:key.to_owned(),outer,before,value,guard:ctx.transaction.guard(),parent:digest,_fee:fee})
}
/// Storage-only exact selected CAS. Domain admission/fees and atomic matching
/// sale remain with the private Runtime Action producer.
pub fn replace_current_module_lifecycle_listing_v1<M,R,E,C,W,CW>(before:&CurrentModuleLifecycleListingV1,after:Option<&ModuleArtifactListingState>,ctx:&mut WorldFieldOverlayContextV1<'_, '_, '_,M,R,E,C>,write:&mut MemberRecordWriteContext<'_,W,CW>,cells:&mut u64,guard:&PreparedGuard,live:&ExecutionFence)->Result<(),WorldFieldOverlayErrorV1>
where M:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool,W:ValueNodeReader+ValueNodeWriter+StatePageWriter,CW:FnMut()->bool{
 ctx.transaction.contribute(|tx|{
  tx.validate_guard(guard,live)?;before.validate(tx,ctx.values.memory,live)?;
  if !ctx.values.memory.same_pool(write.memory)||ctx.values.class!=MemoryClass::Ordinary{return Err(bad().into())}
  step(cells,&mut ctx.cancelled)?;
  if tx.read(StateNamespaceV1::Business,before.outer.as_bytes())?!=before.before{return Err(ValueTreeError::Invalid("stale module listing Current source").into())}
  if let Some(after)=after{
   let field=ModuleMarketplaceFieldV1::ArtifactListings;let descriptor=field.descriptor()?;
   let child=append_listing(descriptor,after,write,cells)?;
   let mut emitted=None;
   write_outer_root(field,descriptor,&before.key,child,write,cells,&mut |row|{
    if emitted.is_some(){return Err(bad())}emitted=Some(row);Ok(())
   })?;
   let row=emitted.ok_or_else(bad)?;
   if row.namespace!=StateNamespaceV1::Business||row.mutation.key.as_slice()!=before.outer.as_bytes(){return Err(bad().into())}
   tx.write(row.namespace,&row.mutation.key,row.mutation.after)?;
  }else{tx.write(StateNamespaceV1::Business,before.outer.as_bytes(),None)?;}
  if (ctx.cancelled)()||(write.cancelled)(){return Err(ValueTreeError::Cancelled.into())}
  Ok(())
 })
}

pub struct CurrentModuleLifecycleBidsV1{
 key:String,outer:WorldFieldKeyV1,before:Option<[u8;32]>,reference:Option<RecordRootRefV1>,
 value:Option<Vec<ModuleArtifactBidState>>,guard:PreparedGuard,parent:[u8;32],_fee:MemoryReservation,
}
impl CurrentModuleLifecycleBidsV1{pub fn value(&self)->Option<&[ModuleArtifactBidState]>{self.value.as_deref()}pub fn outer_id(&self)->&str{&self.key}}
fn owner_error(e:crate::incremental_owner_closure::WorldStateOwnerError)->WorldFieldOverlayErrorV1{
 use crate::incremental_owner_closure::WorldStateOwnerError as E;
 match e{E::Value(e)=>e.into(),E::Memory(e)=>ValueTreeError::Memory(e).into(),E::RootSet(e)=>e.into(),E::ResourceLimited(s)=>ValueTreeError::ResourceLimited(s).into(),E::Invalid(s)=>ValueTreeError::Invalid(s).into(),E::Scan(s)=>ValueTreeError::Io(std::io::Error::other(s)).into()}
}
pub fn read_current_module_lifecycle_bids_v1<M,P,R,E,C>(key:&str,parent:&ReservedValue<ModuleMarketplaceFamilyQualificationV1>,roots:&RootSetV1,state:&P,ctx:&mut WorldFieldOverlayContextV1<'_, '_, '_,M,R,E,C>,scan:&mut oasis7_formal_protocol::incremental_state::StateScanBudget,cells:&mut u64,max_text:usize,max_rows:usize,guard:&PreparedGuard,live:&ExecutionFence)->Result<CurrentModuleLifecycleBidsV1,WorldFieldOverlayErrorV1>
where M:MerkleNodeReader,P:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool{
 ctx.transaction.validate_guard(guard,live)?;let _phase=ctx.values.memory.reserve(8192,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
 if ctx.values.class!=MemoryClass::Ordinary||!parent.uses_budget(ctx.values.memory)||parent.allocation_class()!=MemoryClass::Ordinary||!ctx.transaction.uses_budget(ctx.values.memory){return Err(bad().into())}
 parent.validate_binding(roots).map_err(owner_error)?;let digest=roots.digest()?;if digest!=ctx.transaction.parent_fence().parent_roots{return Err(bad().into())}
 let field=ModuleMarketplaceFieldV1::ArtifactBids;let descriptor=field.descriptor()?;step(cells,&mut ctx.cancelled)?;
 let(outer,before,root)=child_root(field,key,ctx)?;
 let mut fee=ctx.values.memory.reserve(std::mem::size_of::<CurrentModuleLifecycleBidsV1>()+key.len()+512,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
 let mut reference=None;
 let value=if let Some(root)=root{
  let stop=std::cell::RefCell::new(&mut ctx.cancelled);let memory=ctx.values.memory;
  let checked=oasis7_formal_protocol::incremental_state::StateScanContext{source:state,memory,class:MemoryClass::Ordinary,io:scan}.with_reader(||(stop.borrow_mut())(),|reader|{
   crate::incremental_owner_closure::marketplace_bridge::full_child(descriptor,&root,reader,&mut ctx.values,ctx.key_work,ctx.record_bytes,&mut ||(stop.borrow_mut())())
  });
  checked.map_err(|e|match e{oasis7_formal_protocol::incremental_state::StateScanError::Visitor(e)=>owner_error(e),oasis7_formal_protocol::incremental_state::StateScanError::Memory(e)=>ValueTreeError::Memory(e).into(),oasis7_formal_protocol::incremental_state::StateScanError::ResourceLimited=>ValueTreeError::ResourceLimited("module bids closed source scan").into(),oasis7_formal_protocol::incremental_state::StateScanError::Cancelled=>ValueTreeError::Cancelled.into(),oasis7_formal_protocol::incremental_state::StateScanError::Invalid(s)=>ValueTreeError::Invalid(s).into(),oasis7_formal_protocol::incremental_state::StateScanError::Tree(s)=>ValueTreeError::Io(std::io::Error::other(s)).into()})?;
  drop(stop);
  step(cells,&mut ctx.cancelled)?;
  let(length,_)=bid_index::read_u64_member(&root,descriptor,"length",&mut ctx.values,ctx.key_work,ctx.semantic_bytes,ctx.record_bytes,&mut ctx.cancelled)?;
  let length=usize::try_from(length).map_err(|_|bad())?;if length>max_rows{return Err(ValueTreeError::ResourceLimited("module selected bids count").into())}
  let bytes=length.checked_mul(std::mem::size_of::<ModuleArtifactBidState>()*3).ok_or_else(bad)?;
  let mut rows_fee=ctx.values.memory.reserve(bytes,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;fee.merge(&mut rows_fee).map_err(ValueTreeError::Invalid)?;
  let mut rows=Vec::with_capacity(length);if rows.capacity()>length{return Err(ValueTreeError::ResourceLimited("module bid output capacity").into())}
  for ordinal in 0..length{
   step(cells,&mut ctx.cancelled)?;
   let id=bid_index::select_ordinal(&root,descriptor,ordinal as u64,&mut ctx.values,ctx.key_work,ctx.semantic_bytes,ctx.record_bytes,&mut ctx.cancelled)?;
   let mut unit=std::io::sink();if column(&root,field,&(("bid",id),"present"),MemberPayloadCodecV1::Unit,&mut ctx.values,ctx.key_work,ctx.semantic_bytes,ctx.record_bytes,0,&mut unit,&mut ctx.cancelled)?!=0{return Err(bad().into())}
   step(cells,&mut ctx.cancelled)?;
   let len=column(&root,field,&(("bid",id),"bidder_agent_id"),MemberPayloadCodecV1::Text,&mut ctx.values,ctx.key_work,ctx.semantic_bytes,ctx.record_bytes,max_text as u64,&mut std::io::sink(),&mut ctx.cancelled)?;
   let len=usize::try_from(len).map_err(|_|bad())?;let mut text_fee=ctx.values.memory.reserve(len,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;let mut text=Vec::with_capacity(len);if text.capacity()>len{return Err(ValueTreeError::ResourceLimited("module bidder output capacity").into())}
   step(cells,&mut ctx.cancelled)?;let actual=column(&root,field,&(("bid",id),"bidder_agent_id"),MemberPayloadCodecV1::Text,&mut ctx.values,ctx.key_work,ctx.semantic_bytes,ctx.record_bytes,max_text as u64,&mut text,&mut ctx.cancelled)?;if actual!=len as u64{return Err(bad().into())}
   let mut scalar=|name:&str,codec|->Result<[u8;8],WorldFieldOverlayErrorV1>{step(cells,&mut ctx.cancelled)?;let mut bytes=[0u8;8];if column(&root,field,&(("bid",id),name),codec,&mut ctx.values,ctx.key_work,ctx.semantic_bytes,ctx.record_bytes,8,&mut &mut bytes[..],&mut ctx.cancelled)?!=8{return Err(bad().into())}Ok(bytes)};
   let order_id=u64::from_be_bytes(scalar("order_id",MemberPayloadCodecV1::U64)?);
   let price_kind=match u64::from_be_bytes(scalar("price_kind",MemberPayloadCodecV1::U64)?){0=>ResourceKind::Electricity,1=>ResourceKind::Data,_=>return Err(bad().into())};
   let price_amount=i64::from_be_bytes(scalar("price_amount",MemberPayloadCodecV1::I64)?);let bid_at=u64::from_be_bytes(scalar("bid_at",MemberPayloadCodecV1::U64)?);
   rows.push(ModuleArtifactBidState{order_id,bidder_agent_id:String::from_utf8(text).map_err(|_|bad())?,price_kind,price_amount,bid_at});fee.merge(&mut text_fee).map_err(ValueTreeError::Invalid)?;
  }
  let owner=WorldFieldKeyV1::from_bytes(outer.as_bytes()).map_err(ValueTreeError::Invalid)?.semantic_key.ok_or_else(bad)?;
  reference=Some(RecordRootRefV1::new(field.schema(),owner,root).map_err(|_|bad())?);Some(rows)
 }else{None};
 Ok(CurrentModuleLifecycleBidsV1{key:key.to_owned(),outer,before,reference,value,guard:ctx.transaction.guard(),parent:digest,_fee:fee})
}

#[derive(Clone,Copy)]
pub enum ModuleLifecycleBidEditV1<'a>{Insert(&'a ModuleArtifactBidState),DeleteMatching{order_id:u64,bidder_agent_id:&'a str},DeleteAll}
pub fn change_current_module_lifecycle_bids_v1<M,R,E,C,W,CW>(before:&CurrentModuleLifecycleBidsV1,edit:ModuleLifecycleBidEditV1<'_>,ctx:&mut WorldFieldOverlayContextV1<'_, '_, '_,M,R,E,C>,write:&mut MemberRecordWriteContext<'_,W,CW>,cells:&mut u64,guard:&PreparedGuard,live:&ExecutionFence)->Result<(),WorldFieldOverlayErrorV1>
where M:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool,W:ValueNodeReader+ValueNodeWriter+StatePageWriter,CW:FnMut()->bool{
 let result=(||{
  ctx.transaction.validate_guard(guard,live)?;ctx.transaction.validate_transaction_identity(&before.guard)?;
  if !before._fee.uses_budget(ctx.values.memory)||!ctx.values.memory.same_pool(write.memory)||ctx.values.class!=MemoryClass::Ordinary||ctx.transaction.parent_fence().parent_roots!=before.parent{return Err(bad().into())}
  step(cells,&mut ctx.cancelled)?;
  if ctx.transaction.read(StateNamespaceV1::Business,before.outer.as_bytes())?!=before.before{return Err(ValueTreeError::Invalid("stale module bids Current source").into())}
  if matches!(edit,ModuleLifecycleBidEditV1::DeleteAll){ctx.transaction.write(StateNamespaceV1::Business,before.outer.as_bytes(),None)?;return Ok(())}
  if let Some(reference)=&before.reference{
   let change=match edit{
    ModuleLifecycleBidEditV1::Insert(bid)=>ModuleArtifactBidChangeV1::Insert{ordinal:before.value.as_ref().ok_or_else(bad)?.len()as u64,bid},
    ModuleLifecycleBidEditV1::DeleteMatching{order_id,bidder_agent_id}=>{
     let mut count=0u64;
     for value in before.value.as_ref().ok_or_else(bad)?{
      step(cells,&mut ctx.cancelled)?;
      let n=value.bidder_agent_id.len().min(bidder_agent_id.len());ctx.key_work.bytes_remaining=ctx.key_work.bytes_remaining.checked_sub(n as u64).ok_or(ValueTreeError::ResourceLimited("module bid match bytes"))?;
      if value.order_id==order_id&&value.bidder_agent_id==bidder_agent_id{count=count.checked_add(1).ok_or_else(bad)?;}
     }
     if count==0{return Err(ValueTreeError::Invalid("selected module bid match missing").into())}
     ModuleArtifactBidChangeV1::DeleteMatching{order_id,bidder_agent_id,expected_matches:count}
    },ModuleLifecycleBidEditV1::DeleteAll=>return Err(bad().into()),
   };
   change_module_bid_overlay_v1(&before.key,reference,change,cells,ctx,write,guard,live)?;
  }else{
   let ModuleLifecycleBidEditV1::Insert(bid)=edit else{return Err(ValueTreeError::Invalid("module bids absent for deletion").into())};
   let field=ModuleMarketplaceFieldV1::ArtifactBids;let descriptor=field.descriptor()?;
   let child=append_bids(descriptor,std::slice::from_ref(bid),write,cells)?;let mut emitted=None;
   write_outer_root(field,descriptor,&before.key,child,write,cells,&mut |row|{if emitted.is_some(){return Err(bad())}emitted=Some(row);Ok(())})?;
   let row=emitted.ok_or_else(bad)?;if row.namespace!=StateNamespaceV1::Business||row.mutation.key.as_slice()!=before.outer.as_bytes(){return Err(bad().into())}
   ctx.transaction.write(row.namespace,&row.mutation.key,row.mutation.after)?;
  }
  if (ctx.cancelled)()||(write.cancelled)(){return Err(ValueTreeError::Cancelled.into())}Ok(())
 })();
 if result.is_err(){let _:Result<(),oasis7_formal_protocol::incremental_execution::ExecutionError>=ctx.transaction.contribute(|_|Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted));}result
}
