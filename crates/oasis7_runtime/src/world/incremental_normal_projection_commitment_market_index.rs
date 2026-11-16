//! Marketplace's internal stable-ID/rank graph, retained only as readonly
//! projection DATA. A logical Vec alone is never a Cold index source.
use super::*;
use super::original_rows::{OriginalProjectionRowsDataV1,OriginalProjectionRowDataV1};
use std::{sync::Arc,cell::RefCell,mem::size_of};
pub(super) struct ProjectionBidIndexDataV1{length:u64,root_id:u64,next_id:u64,nodes:OriginalProjectionRowsDataV1,bids:OriginalProjectionRowsDataV1,_fee:MemoryReservation}
pub(in crate::world) struct ProjectionMarketplaceIndexSeedDataV1{modules:OriginalProjectionRowsDataV1,memory:MemoryBudget,_fee:MemoryReservation}
#[derive(Debug)]enum BuildError{Body(BodyMutationError),Member(oasis7_world_state::incremental_field_member::MemberRecordError)}
impl From<oasis7_world_state::incremental_field_member::MemberRecordError> for BuildError{fn from(v:oasis7_world_state::incremental_field_member::MemberRecordError)->Self{Self::Member(v)}}
impl BuildError{fn into_body(self)->BodyMutationError{match self{Self::Body(v)=>v,Self::Member(v)=>paid(v)}}}
fn reserve(common:&mut FormalBoundedContextV1<'_>,n:usize)->Result<MemoryReservation,BodyMutationError>{common.charge(1,n as u64).map_err(paid)?;common.memory().reserve(n,MemoryClass::Ordinary).map_err(ValueTreeError::Memory).map_err(Into::into)}
fn row<K:oasis7_world_state::incremental_semantic_key::SemanticKeyV1+?Sized,V:serde::Serialize+?Sized>(key:&K,value:&V,common:&mut FormalBoundedContextV1<'_>)->Result<Arc<OriginalProjectionRowDataV1>,BodyMutationError>{let semantic=super::initial::capture(common,|out|oasis7_world_state::incremental_semantic_key::write_semantic_key_v1(key,out).map_err(paid))?;let key=super::capture_projection_json_data_v1(key,common)?;let value=super::capture_projection_json_data_v1(value,common)?;OriginalProjectionRowDataV1::from_original_encoded_data_v1(semantic,key,value,common)}
impl ProjectionMarketplaceIndexSeedDataV1{
 pub(super) fn from_genesis_actual_v1(state:&oasis7_world_state::WorldState,common:&mut FormalBoundedContextV1<'_>)->Result<Self,BodyMutationError>{
  let fee=reserve(common,size_of::<Self>()+2*size_of::<usize>())?;
  let mut modules=OriginalProjectionRowsDataV1::empty_data_v1(common)?;
  for(module,values)in &state.module_artifact_bids{
   common.consume_steps(1).map_err(paid)?;
   let header=reserve(common,size_of::<ProjectionBidIndexDataV1>()+2*size_of::<usize>())?;
   let mut nodes=OriginalProjectionRowsDataV1::empty_data_v1(common)?;
   let mut bids=OriginalProjectionRowsDataV1::empty_data_v1(common)?;
   let shared=RefCell::new(&mut *common);
   let length=u64::try_from(values.len()).map_err(|_|invalid("marketplace GEN bid length"))?;
   let meta=oasis7_world_state::incremental_module_marketplace::visit_original_genesis_bid_index_data_v1(length,&mut|n|shared.borrow_mut().charge(1,n as u64).map_err(paid).map_err(BuildError::Body),&mut|id,node|{let mut common=shared.borrow_mut();let value=row(&id,&node.fields(),&mut common).map_err(BuildError::Body)?;nodes=nodes.with_row_data_v1(value,&mut common).map_err(BuildError::Body)?;Ok::<_,BuildError>(())}).map_err(BuildError::into_body)?;
   let common=shared.into_inner();
   for(index,value)in values.iter().enumerate(){common.consume_steps(1).map_err(paid)?;let index=u64::try_from(index).map_err(|_|invalid("marketplace GEN bid ordinal"))?;let bytes=super::capture_projection_json_data_v1(value,common)?;let value=super::market_rows::OriginalMarketRecordDataV1::from_original_qualified_json_v1(oasis7_world_state::incremental_module_marketplace::ModuleMarketplaceFieldV1::ArtifactBids,&bytes,common)?;let semantic=super::initial::capture(common,|out|oasis7_world_state::incremental_semantic_key::write_semantic_key_v1(&index,out).map_err(paid))?;let key=super::capture_projection_json_data_v1(&index,common)?;let value=OriginalProjectionRowDataV1::from_market_record_data_v1(semantic,key,value,common)?;bids=bids.with_row_data_v1(value,common)?;}
   let data=Arc::new(ProjectionBidIndexDataV1{length:meta.length(),root_id:meta.root_id(),next_id:meta.next_id(),nodes,bids,_fee:header});
   let semantic=super::initial::capture(common,|out|oasis7_world_state::incremental_semantic_key::write_semantic_key_v1(module,out).map_err(paid))?;
   let key=super::capture_projection_json_data_v1(module,common)?;
   let value=OriginalProjectionRowDataV1::from_market_bids_data_v1(semantic,key,data,common)?;
   modules=modules.with_row_data_v1(value,common)?;
  }
  Ok(Self{modules,memory:common.memory().clone(),_fee:fee})
 }
 pub(super) fn validate_context(&self,common:&mut FormalBoundedContextV1<'_>)->Result<(),BodyMutationError>{common.consume_steps(1).map_err(paid)?;if !self.memory.same_pool(common.memory())||!self._fee.uses_budget(common.memory()){return Err(invalid("marketplace index source pool"))}self.modules.validate(common)}
 pub(super) fn original_rows_v1(&self,common:&mut FormalBoundedContextV1<'_>)->Result<OriginalProjectionRowsDataV1,BodyMutationError>{self.validate_context(common)?;self.modules.share_data_v1(common)}
}

impl ProjectionBidIndexDataV1 {
 fn node_v1(&self,id:u64,common:&mut FormalBoundedContextV1<'_>)->Result<[u64;7],BodyMutationError>{
  let semantic=super::initial::capture(common,|out|oasis7_world_state::incremental_semantic_key::write_semantic_key_v1(&id,out).map_err(paid))?;
  let row=self.nodes.row_data_v1(&semantic,common)?.ok_or_else(||invalid("actual marketplace cached index node missing"))?;
  let bytes=row.original_value_json().ok_or_else(||invalid("marketplace index node DATA kind"))?;
  // Fixed seven-u64 array; the original encoder owns the bytes and the
  // decoder has no heap-bearing output or visitor-controlled allocation.
  common.charge(8,bytes.len() as u64).map_err(paid)?;
  let error_fee=reserve(common,1024)?;
  let result=serde_json::from_slice::<[u64;7]>(bytes);
  common.consume_steps(0).map_err(paid)?;
  result.map_err(|error|paid(IndexCodecError{error,_fee:error_fee}))
 }
 pub(super) fn validate_context(&self,common:&mut FormalBoundedContextV1<'_>)->Result<(),BodyMutationError>{
  common.consume_steps(1).map_err(paid)?;
  if !self._fee.uses_budget(common.memory()){return Err(invalid("marketplace cached index pool"))}
  self.nodes.validate(common)?;self.bids.validate(common)?;
  Ok(())
 }
 pub(super) fn validate_complete_v1(&self,common:&mut FormalBoundedContextV1<'_>)->Result<(),BodyMutationError>{self.validate_context(common)?;
  let n=usize::try_from(self.length).map_err(|_|invalid("marketplace cached length"))?;
  if self.nodes.len()!=n||self.bids.len()!=n||self.next_id<self.length||(self.length==0)!=(self.root_id==u64::MAX){return Err(invalid("marketplace cached index cardinality"))}
  Ok(())
 }
 pub(super) fn serialization_v1<'a,'b,'c>(&'a self,common:&'a RefCell<&'b mut FormalBoundedContextV1<'c>>,first:&'a RefCell<Option<BodyMutationError>>)->MarketBidSequenceDataV1<'a,'b,'c>{MarketBidSequenceDataV1{index:self,common,first}}
}
pub(super) struct MarketBidSequenceDataV1<'a,'b,'c>{index:&'a ProjectionBidIndexDataV1,common:&'a RefCell<&'b mut FormalBoundedContextV1<'c>>,first:&'a RefCell<Option<BodyMutationError>>}
impl serde::Serialize for MarketBidSequenceDataV1<'_,'_,'_>{
 fn serialize<S:serde::Serializer>(&self,serializer:S)->Result<S::Ok,S::Error>{
  use serde::ser::{SerializeSeq,Error as _};
  fn fail<E:serde::ser::Error>(view:&MarketBidSequenceDataV1<'_,'_,'_>,error:BodyMutationError)->E{if view.first.borrow().is_none(){*view.first.borrow_mut()=Some(error)}E::custom("marketplace original index source")}
  self.index.validate_complete_v1(&mut self.common.borrow_mut()).map_err(|e|fail::<S::Error>(self,e))?;
  fn walk<Q:SerializeSeq>(view:&MarketBidSequenceDataV1<'_,'_,'_>,seq:&mut Q,id:u64,parent:u64,depth:usize,count:&mut u64)->Result<(),Q::Error>{
   if id==u64::MAX{return Ok(())}
   if depth>=128||*count>=view.index.length{return Err(Q::Error::custom("marketplace original index traversal bound"))}
   let fields=view.index.node_v1(id,&mut view.common.borrow_mut()).map_err(|e|fail::<Q::Error>(view,e))?;
   if fields[6]!=parent||id>=view.index.next_id{return Err(Q::Error::custom("marketplace original index parent"))}
   walk(view,seq,fields[0],id,depth+1,count)?;
   *count=count.checked_add(1).ok_or_else(||Q::Error::custom("marketplace original index count"))?;
   if *count>view.index.length{return Err(Q::Error::custom("marketplace original index length"))}
   {
    let mut common=view.common.borrow_mut();
    let semantic=super::initial::capture(&mut common,|out|oasis7_world_state::incremental_semantic_key::write_semantic_key_v1(&id,out).map_err(paid)).map_err(|e|fail::<Q::Error>(view,e))?;
    let row=view.index.bids.row_data_v1(&semantic,&mut common).map_err(|e|fail::<Q::Error>(view,e))?.ok_or_else(||Q::Error::custom("marketplace actual bid missing"))?;
    let value=row.market_record_v1().ok_or_else(||Q::Error::custom("marketplace actual bid DATA kind"))?;
    // Release the work loan before the shared fixed-field serializer borrows
    // that same lane. The selected DATA row remains borrowed from the index.
    drop(common);
    seq.serialize_element(&value.serialization_v1(view.common,view.first))?;
   }
   walk(view,seq,fields[1],id,depth+1,count)
  }
  let mut seq=serializer.serialize_seq(Some(self.index.length as usize))?;let mut count=0;
  walk(self,&mut seq,self.index.root_id,u64::MAX,0,&mut count)?;
  if count!=self.index.length{return Err(S::Error::custom("marketplace original index incomplete"))}seq.end()
 }
}

#[derive(Debug)]struct IndexCodecError{error:serde_json::Error,_fee:MemoryReservation}
impl std::fmt::Display for IndexCodecError{fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result{std::fmt::Display::fmt(&self.error,f)}}
impl std::error::Error for IndexCodecError{}

impl ProjectionBidIndexDataV1{
 pub(super) fn empty_for_new_v1(meta:[u64;3],common:&mut FormalBoundedContextV1<'_>)->Result<Arc<Self>,BodyMutationError>{let fee=reserve(common,size_of::<Self>()+2*size_of::<usize>())?;Ok(Arc::new(Self{length:meta[0],root_id:meta[1],next_id:meta[2],nodes:OriginalProjectionRowsDataV1::empty_data_v1(common)?,bids:OriginalProjectionRowsDataV1::empty_data_v1(common)?,_fee:fee}))}
 fn fork_v1(&self,common:&mut FormalBoundedContextV1<'_>)->Result<Self,BodyMutationError>{self.validate_context(common)?;let fee=reserve(common,size_of::<Self>()+2*size_of::<usize>())?;Ok(Self{length:self.length,root_id:self.root_id,next_id:self.next_id,nodes:self.nodes.share_data_v1(common)?,bids:self.bids.share_data_v1(common)?,_fee:fee})}
 pub(super) fn with_actual_meta_v1(&self,meta:[u64;3],common:&mut FormalBoundedContextV1<'_>)->Result<Arc<Self>,BodyMutationError>{let mut out=self.fork_v1(common)?;out.length=meta[0];out.root_id=meta[1];out.next_id=meta[2];Ok(Arc::new(out))}
 pub(super) fn with_actual_node_v1(&self,id:u64,value:Option<[u64;7]>,common:&mut FormalBoundedContextV1<'_>)->Result<Arc<Self>,BodyMutationError>{let mut out=self.fork_v1(common)?;let semantic=super::initial::capture(common,|out|oasis7_world_state::incremental_semantic_key::write_semantic_key_v1(&id,out).map_err(paid))?;if let Some(value)=value{if id>=self.next_id{return Err(invalid("actual marketplace node ID"))}let node=row(&id,&value,common)?;out.nodes=out.nodes.with_row_data_v1(node,common)?;if out.bids.row_data_v1(&semantic,common)?.is_none(){let record=super::market_rows::OriginalMarketRecordDataV1::empty_for_new_v1(oasis7_world_state::incremental_module_marketplace::ModuleMarketplaceFieldV1::ArtifactBids,common)?;let key=super::capture_projection_json_data_v1(&id,common)?;let row=OriginalProjectionRowDataV1::from_market_record_data_v1(semantic,key,record,common)?;out.bids=out.bids.with_row_data_v1(row,common)?;}}else{out.nodes=out.nodes.without_row_data_v1(&semantic,common)?;out.bids=out.bids.without_row_data_v1(&semantic,common)?;}Ok(Arc::new(out))}
 pub(super) fn with_actual_bid_field_v1(&self,id:u64,name:&str,value:Option<&[u8]>,common:&mut FormalBoundedContextV1<'_>)->Result<Arc<Self>,BodyMutationError>{let mut out=self.fork_v1(common)?;if name=="present"{return Ok(Arc::new(out))}let semantic=super::initial::capture(common,|out|oasis7_world_state::incremental_semantic_key::write_semantic_key_v1(&id,out).map_err(paid))?;let Some(row)=out.bids.row_data_v1(&semantic,common)?else{if value.is_some(){return Err(invalid("actual marketplace bid index missing"))}return Ok(Arc::new(out))};let record=row.market_record_v1().ok_or_else(||invalid("actual marketplace bid payload kind"))?;let record=record.with_actual_patch_v1(name,value,common)?;let key=super::capture_projection_json_data_v1(&id,common)?;let row=OriginalProjectionRowDataV1::from_market_record_data_v1(semantic,key,record,common)?;out.bids=out.bids.with_row_data_v1(row,common)?;Ok(Arc::new(out))}
 pub(super) fn validate_actual_bid_v1(&self,id:u64,common:&mut FormalBoundedContextV1<'_>)->Result<(),BodyMutationError>{let semantic=super::initial::capture(common,|out|oasis7_world_state::incremental_semantic_key::write_semantic_key_v1(&id,out).map_err(paid))?;if let Some(row)=self.bids.row_data_v1(&semantic,common)?{row.market_record_v1().ok_or_else(||invalid("actual marketplace payload kind"))?.validate_complete_v1(common)?;}Ok(())}
}
impl ProjectionMarketplaceIndexSeedDataV1{
 pub(super) fn from_actual_qualified_v1<M:oasis7_formal_protocol::incremental_state::MerkleNodeReader>(source:&oasis7_world_state::incremental_owner_closure::QualifiedMarketplaceProjectionColumnsV1,columns:&super::market_fold::QualifiedMarketplaceColumnDataV1,tx:&oasis7_formal_protocol::incremental_execution::ExecutionTransaction<'_,M>,common:&mut FormalBoundedContextV1<'_>,live:&oasis7_formal_protocol::incremental_execution::ExecutionFence)->Result<Arc<Self>,BodyMutationError>{source.validate(tx,common.memory(),live)?;let fee=reserve(common,size_of::<Self>()+2*size_of::<usize>())?;let modules=columns.rows_v1(oasis7_world_state::incremental_module_marketplace::ModuleMarketplaceFieldV1::ArtifactBids,common)?.share_data_v1(common)?;Ok(Arc::new(Self{modules,memory:common.memory().clone(),_fee:fee}))}
}
