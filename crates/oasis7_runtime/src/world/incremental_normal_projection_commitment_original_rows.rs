//! Persistent original typed rows. Raw semantic ordering and original typed
//! JSON remain independent of the sorted observation JSON graph.
use super::*;
use std::{cmp::Ordering,mem::size_of,sync::Arc};
enum RowValue{MarketRecord(Arc<super::market_rows::OriginalMarketRecordDataV1>),MarketBids(Arc<super::market_index::ProjectionBidIndexDataV1>),RewardPool(Arc<super::reward_rows::OriginalRewardPoolRecordDataV1>),Diplomacy(Arc<super::diplomacy_rows::OriginalDiplomacyRecordDataV1>),Governance(Arc<super::governance_rows::OriginalGovernanceRecordDataV1>),Fragment(ReservedValue<Vec<u8>>),Map(Arc<OriginalProjectionRowsDataV1>),Set(Arc<OriginalProjectionRowsDataV1>)}
pub(super) struct OriginalProjectionRowDataV1{
 semantic:ReservedValue<Vec<u8>>,key_json:ReservedValue<Vec<u8>>,key_value:ReservedValue<serde_json::Value>,value:RowValue,_fee:MemoryReservation,
}
impl OriginalProjectionRowDataV1{
 /// DATA only. The actual typed-source producer retains the Current/parent
 /// provenance and supplies bytes from its unique original encoders.
 pub(super) fn from_original_encoded_data_v1(semantic:ReservedValue<Vec<u8>>,key_json:ReservedValue<Vec<u8>>,value_json:ReservedValue<Vec<u8>>,common:&mut FormalBoundedContextV1<'_>)->Result<Arc<Self>,BodyMutationError>{
  if !value_json.uses_budget(common.memory()){return Err(invalid("original projection value pool"))}
  Self::build(semantic,key_json,RowValue::Fragment(value_json),common)
 }
 pub(super) fn from_market_record_data_v1(semantic:ReservedValue<Vec<u8>>,key_json:ReservedValue<Vec<u8>>,value:Arc<super::market_rows::OriginalMarketRecordDataV1>,common:&mut FormalBoundedContextV1<'_>)->Result<Arc<Self>,BodyMutationError>{value.validate_context(common)?;Self::build(semantic,key_json,RowValue::MarketRecord(value),common)}
 pub(super) fn market_record_v1(&self)->Option<&Arc<super::market_rows::OriginalMarketRecordDataV1>>{match &self.value{RowValue::MarketRecord(v)=>Some(v),_=>None}}
 pub(super) fn from_market_bids_data_v1(semantic:ReservedValue<Vec<u8>>,key_json:ReservedValue<Vec<u8>>,value:Arc<super::market_index::ProjectionBidIndexDataV1>,common:&mut FormalBoundedContextV1<'_>)->Result<Arc<Self>,BodyMutationError>{value.validate_context(common)?;Self::build(semantic,key_json,RowValue::MarketBids(value),common)}
 pub(super) fn market_bids_v1(&self)->Option<&Arc<super::market_index::ProjectionBidIndexDataV1>>{match &self.value{RowValue::MarketBids(v)=>Some(v),_=>None}}
 pub(super) fn from_reward_pool_data_v1(semantic:ReservedValue<Vec<u8>>,key_json:ReservedValue<Vec<u8>>,value:Arc<super::reward_rows::OriginalRewardPoolRecordDataV1>,common:&mut FormalBoundedContextV1<'_>)->Result<Arc<Self>,BodyMutationError>{value.validate(common)?;Self::build(semantic,key_json,RowValue::RewardPool(value),common)}
 pub(super) fn reward_pool_record_v1(&self)->Option<&Arc<super::reward_rows::OriginalRewardPoolRecordDataV1>>{match &self.value{RowValue::RewardPool(v)=>Some(v),_=>None}}
 pub(super) fn from_diplomacy_data_v1(semantic:ReservedValue<Vec<u8>>,key_json:ReservedValue<Vec<u8>>,value:Arc<super::diplomacy_rows::OriginalDiplomacyRecordDataV1>,common:&mut FormalBoundedContextV1<'_>)->Result<Arc<Self>,BodyMutationError>{value.validate(common)?;Self::build(semantic,key_json,RowValue::Diplomacy(value),common)}
 pub(super) fn diplomacy_record_v1(&self)->Option<&Arc<super::diplomacy_rows::OriginalDiplomacyRecordDataV1>>{match &self.value{RowValue::Diplomacy(v)=>Some(v),_=>None}}
 pub(super) fn from_governance_data_v1(semantic:ReservedValue<Vec<u8>>,key_json:ReservedValue<Vec<u8>>,value:Arc<super::governance_rows::OriginalGovernanceRecordDataV1>,common:&mut FormalBoundedContextV1<'_>)->Result<Arc<Self>,BodyMutationError>{value.validate(common)?;Self::build(semantic,key_json,RowValue::Governance(value),common)}
 pub(super) fn governance_record_v1(&self)->Option<&Arc<super::governance_rows::OriginalGovernanceRecordDataV1>>{match &self.value{RowValue::Governance(v)=>Some(v),_=>None}}
 pub(super) fn from_group_data_v1(semantic:ReservedValue<Vec<u8>>,key_json:ReservedValue<Vec<u8>>,rows:Arc<OriginalProjectionRowsDataV1>,is_set:bool,common:&mut FormalBoundedContextV1<'_>)->Result<Arc<Self>,BodyMutationError>{rows.validate(common)?;Self::build(semantic,key_json,if is_set{RowValue::Set(rows)}else{RowValue::Map(rows)},common)}
 fn build(semantic:ReservedValue<Vec<u8>>,key_json:ReservedValue<Vec<u8>>,value:RowValue,common:&mut FormalBoundedContextV1<'_>)->Result<Arc<Self>,BodyMutationError>{
  common.consume_steps(1).map_err(paid)?;if !semantic.uses_budget(common.memory())||!key_json.uses_budget(common.memory()){return Err(invalid("original projection row pool"))}
  common.charge(1,semantic.len()as u64).map_err(paid)?;oasis7_world_state::incremental_semantic_key::read_semantic_key_v1(&semantic).map_err(invalid)?;
  let phase_bytes=key_json.len().checked_mul(128).and_then(|n|n.checked_add(32768)).ok_or_else(||invalid("original key graph layout"))?;common.charge(1,phase_bytes as u64).map_err(paid)?;let mut phase=common.memory().reserve(phase_bytes,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
  let decoded=serde_json::from_slice::<serde_json::Value>(&key_json);common.consume_steps(0).map_err(paid)?;
  let decoded_key=match decoded{Ok(value)=>value,Err(error)=>return Err(paid(PaidOriginalKeyCodecError{error,_fee:phase}))};
  fn heap(value:&serde_json::Value,common:&mut FormalBoundedContextV1<'_>)->Result<usize,BodyMutationError>{common.consume_steps(1).map_err(paid)?;match value{serde_json::Value::String(text)=>Ok(text.capacity()),serde_json::Value::Array(values)=>{let mut n=values.capacity().checked_mul(size_of::<serde_json::Value>()).ok_or_else(||invalid("original key sequence layout"))?;for value in values{n=n.checked_add(heap(value,common)?).ok_or_else(||invalid("original key sequence heap"))?}Ok(n)},serde_json::Value::Object(_)=>Err(invalid("original semantic key JSON object")),_=>Ok(0)}}
  let bytes=size_of::<serde_json::Value>().checked_add(heap(&decoded_key,common)?).ok_or_else(||invalid("original key graph heap"))?;let key_value=phase.split(bytes).map_err(invalid)?.retain(decoded_key);drop(phase);
  let n=size_of::<Self>()+2*size_of::<usize>();common.charge(1,n as u64).map_err(paid)?;let fee=common.memory().reserve(n,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;Ok(Arc::new(Self{semantic,key_json,key_value,value,_fee:fee}))
 }
 pub(super) fn semantic(&self)->&[u8]{&self.semantic}
 pub(super) fn original_key_json(&self)->&[u8]{&self.key_json}
 pub(super) fn original_value_json(&self)->Option<&[u8]>{match &self.value{RowValue::Fragment(value)=>Some(value),_=>None}}
 pub(super) fn nested_rows(&self)->Option<&Arc<OriginalProjectionRowsDataV1>>{match &self.value{RowValue::Map(rows)|RowValue::Set(rows)=>Some(rows),_=>None}}
 pub(super) fn original_key_value(&self)->&serde_json::Value{&self.key_value}
}
type Tree=Option<Arc<Node>>;
struct Node{row:Arc<OriginalProjectionRowDataV1>,left:Tree,right:Tree,height:u32,len:usize,_fee:MemoryReservation}
fn height(tree:&Tree)->u32{tree.as_ref().map_or(0,|n|n.height)}fn len(tree:&Tree)->usize{tree.as_ref().map_or(0,|n|n.len)}
fn node(row:Arc<OriginalProjectionRowDataV1>,left:Tree,right:Tree,common:&mut FormalBoundedContextV1<'_>)->Result<Arc<Node>,BodyMutationError>{let n=size_of::<Node>()+2*size_of::<usize>();common.charge(1,n as u64).map_err(paid)?;let fee=common.memory().reserve(n,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;let count=1usize.checked_add(len(&left)).and_then(|n|n.checked_add(len(&right))).ok_or_else(||invalid("original projection row count"))?;let h=1+height(&left).max(height(&right));Ok(Arc::new(Node{row,left,right,height:h,len:count,_fee:fee}))}
fn balance(row:Arc<OriginalProjectionRowDataV1>,left:Tree,right:Tree,common:&mut FormalBoundedContextV1<'_>)->Result<Arc<Node>,BodyMutationError>{
 common.consume_steps(1).map_err(paid)?;
 if height(&left)>height(&right)+1{let l=left.as_ref().ok_or_else(||invalid("original rows left rotation"))?;if height(&l.left)>=height(&l.right){let r=node(row,l.right.clone(),right,common)?;return node(Arc::clone(&l.row),l.left.clone(),Some(r),common)}let m=l.right.as_ref().ok_or_else(||invalid("original rows inner rotation"))?;let a=node(Arc::clone(&l.row),l.left.clone(),m.left.clone(),common)?;let b=node(row,m.right.clone(),right,common)?;return node(Arc::clone(&m.row),Some(a),Some(b),common)}
 if height(&right)>height(&left)+1{let r=right.as_ref().ok_or_else(||invalid("original rows right rotation"))?;if height(&r.right)>=height(&r.left){let l=node(row,left,r.left.clone(),common)?;return node(Arc::clone(&r.row),Some(l),r.right.clone(),common)}let m=r.left.as_ref().ok_or_else(||invalid("original rows inner rotation"))?;let a=node(row,left,m.left.clone(),common)?;let b=node(Arc::clone(&r.row),m.right.clone(),r.right.clone(),common)?;return node(Arc::clone(&m.row),Some(a),Some(b),common)}
 node(row,left,right,common)
}
fn put(tree:&Tree,row:Arc<OriginalProjectionRowDataV1>,common:&mut FormalBoundedContextV1<'_>)->Result<Tree,BodyMutationError>{common.consume_steps(1).map_err(paid)?;let Some(old)=tree else{return node(row,None,None,common).map(Some)};match compare_projection_original_keys_v1(row.semantic(),old.row.semantic(),common)?{Ordering::Equal=>node(row,old.left.clone(),old.right.clone(),common).map(Some),Ordering::Less=>{let left=put(&old.left,row,common)?;balance(Arc::clone(&old.row),left,old.right.clone(),common).map(Some)},Ordering::Greater=>{let right=put(&old.right,row,common)?;balance(Arc::clone(&old.row),old.left.clone(),right,common).map(Some)}}}
fn remove(tree:&Tree,key:&[u8],common:&mut FormalBoundedContextV1<'_>)->Result<Tree,BodyMutationError>{common.consume_steps(1).map_err(paid)?;let Some(old)=tree else{return Ok(None)};match compare_projection_original_keys_v1(key,old.row.semantic(),common)?{Ordering::Less=>{let left=remove(&old.left,key,common)?;balance(Arc::clone(&old.row),left,old.right.clone(),common).map(Some)},Ordering::Greater=>{let right=remove(&old.right,key,common)?;balance(Arc::clone(&old.row),old.left.clone(),right,common).map(Some)},Ordering::Equal=>match(&old.left,&old.right){(None,_)=>Ok(old.right.clone()),(_,None)=>Ok(old.left.clone()),(Some(_),Some(right))=>{let mut first=right;while let Some(next)=&first.left{common.consume_steps(1).map_err(paid)?;first=next}let after=remove(&old.right,first.row.semantic(),common)?;balance(Arc::clone(&first.row),old.left.clone(),after,common).map(Some)}}}}
pub(super) struct OriginalProjectionRowsDataV1{root:Tree,memory:MemoryBudget,_fee:MemoryReservation}
impl OriginalProjectionRowsDataV1{
 pub(super) fn empty_data_v1(common:&mut FormalBoundedContextV1<'_>)->Result<Self,BodyMutationError>{let n=size_of::<Self>()+2*size_of::<usize>();common.charge(1,n as u64).map_err(paid)?;let fee=common.memory().reserve(n,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;Ok(Self{root:None,memory:common.memory().clone(),_fee:fee})}
 pub(super) fn validate(&self,common:&mut FormalBoundedContextV1<'_>)->Result<(),BodyMutationError>{common.consume_steps(1).map_err(paid)?;if !self.memory.same_pool(common.memory())||!self._fee.uses_budget(common.memory()){return Err(invalid("original projection rows pool"))}Ok(())}
 pub(super) fn len(&self)->usize{len(&self.root)}
 pub(super) fn share_data_v1(&self,common:&mut FormalBoundedContextV1<'_>)->Result<Self,BodyMutationError>{self.validate(common)?;let mut shared=Self::empty_data_v1(common)?;shared.root=self.root.clone();Ok(shared)}
 pub(super) fn row_data_v1(&self,key:&[u8],common:&mut FormalBoundedContextV1<'_>)->Result<Option<&OriginalProjectionRowDataV1>,BodyMutationError>{self.validate(common)?;let mut node=self.root.as_ref();while let Some(current)=node{common.consume_steps(1).map_err(paid)?;match compare_projection_original_keys_v1(key,current.row.semantic(),common)?{Ordering::Equal=>return Ok(Some(&current.row)),Ordering::Less=>node=current.left.as_ref(),Ordering::Greater=>node=current.right.as_ref()}}Ok(None)}

 pub(super) fn with_row_data_v1(&self,row:Arc<OriginalProjectionRowDataV1>,common:&mut FormalBoundedContextV1<'_>)->Result<Self,BodyMutationError>{self.validate(common)?;if !row._fee.uses_budget(common.memory()){return Err(invalid("original projection row pool"))}let mut after=Self::empty_data_v1(common)?;after.root=put(&self.root,row,common)?;Ok(after)}
 pub(super) fn without_row_data_v1(&self,key:&[u8],common:&mut FormalBoundedContextV1<'_>)->Result<Self,BodyMutationError>{self.validate(common)?;let mut after=Self::empty_data_v1(common)?;after.root=remove(&self.root,key,common)?;Ok(after)}
 pub(super) fn visit_data_v1(&self,common:&mut FormalBoundedContextV1<'_>,emit:&mut impl FnMut(&OriginalProjectionRowDataV1,&mut FormalBoundedContextV1<'_>)->Result<(),BodyMutationError>)->Result<(),BodyMutationError>{self.validate(common)?;fn walk(tree:&Tree,common:&mut FormalBoundedContextV1<'_>,emit:&mut impl FnMut(&OriginalProjectionRowDataV1,&mut FormalBoundedContextV1<'_>)->Result<(),BodyMutationError>)->Result<(),BodyMutationError>{if let Some(node)=tree{common.consume_steps(1).map_err(paid)?;walk(&node.left,common,emit)?;emit(&node.row,common)?;walk(&node.right,common,emit)?}Ok(())}walk(&self.root,common,emit)}
}

/// Borrowed serialization view of the resident typed-key tree. It retains the
/// source graph and uses original typed map-key serialization and raw typed
/// value fragments; no sorted JSON DOM or reconstructed column is allocated.
pub(super) struct OriginalProjectionMapSerializeV1<'a,'b,'c>{
 rows:&'a OriginalProjectionRowsDataV1,
 common:&'a std::cell::RefCell<&'b mut FormalBoundedContextV1<'c>>,
 first:&'a std::cell::RefCell<Option<BodyMutationError>>,
}
impl OriginalProjectionRowsDataV1{
 pub(super) fn original_map_serialization_v1<'a,'b,'c>(&'a self,common:&'a std::cell::RefCell<&'b mut FormalBoundedContextV1<'c>>,first:&'a std::cell::RefCell<Option<BodyMutationError>>)->OriginalProjectionMapSerializeV1<'a,'b,'c>{OriginalProjectionMapSerializeV1{rows:self,common,first}}
}
impl serde::Serialize for OriginalProjectionMapSerializeV1<'_,'_,'_>{
 fn serialize<S:serde::Serializer>(&self,serializer:S)->Result<S::Ok,S::Error>{
  use serde::ser::{SerializeMap,Error as _};
  if self.first.borrow().is_some(){return Err(S::Error::custom("original map meter stopped"))}
  if let Err(error)=self.rows.validate(&mut self.common.borrow_mut()){
   *self.first.borrow_mut()=Some(error);return Err(S::Error::custom("original map source context"))
  }
  fn walk<S:SerializeMap>(tree:&Tree,map:&mut S,view:&OriginalProjectionMapSerializeV1<'_,'_,'_>)->Result<(),S::Error>{
   let Some(node)=tree else{return Ok(())};
   if view.first.borrow().is_some(){return Err(S::Error::custom("original map meter stopped"))}
   if let Err(error)=view.common.borrow_mut().consume_steps(1){*view.first.borrow_mut()=Some(paid(error));return Err(S::Error::custom("original map meter stopped"))}
   walk(&node.left,map,view)?;
   match &node.row.value{
    RowValue::Fragment(bytes)=>{
     if let Err(error)=view.common.borrow_mut().charge(bytes.len() as u64,bytes.len() as u64){*view.first.borrow_mut()=Some(paid(error));return Err(S::Error::custom("original map fragment meter stopped"))}
     let decoded=serde_json::from_slice::<&serde_json::value::RawValue>(bytes);
     if let Err(error)=view.common.borrow_mut().consume_steps(0){*view.first.borrow_mut()=Some(paid(error));return Err(S::Error::custom("original map fragment meter stopped"))}
     let value=decoded.map_err(S::Error::custom)?;map.serialize_entry(node.row.original_key_value(),value)?;
    },
    RowValue::MarketRecord(value)=>map.serialize_entry(node.row.original_key_value(),&value.serialization_v1(view.common,view.first))?,
    RowValue::MarketBids(value)=>map.serialize_entry(node.row.original_key_value(),&value.serialization_v1(view.common,view.first))?,
    RowValue::RewardPool(value)=>map.serialize_entry(node.row.original_key_value(),&value.serialization_v1(view.common,view.first))?,
    RowValue::Diplomacy(value)=>map.serialize_entry(node.row.original_key_value(),&value.serialization_v1(view.common,view.first))?,
    RowValue::Governance(value)=>map.serialize_entry(node.row.original_key_value(),&value.serialization_v1(view.common,view.first))?,
    RowValue::Map(rows)=>map.serialize_entry(node.row.original_key_value(),&rows.original_map_serialization_v1(view.common,view.first))?,
    RowValue::Set(rows)=>map.serialize_entry(node.row.original_key_value(),&rows.original_set_serialization_v1(view.common,view.first))?,
   }
   walk(&node.right,map,view)
  }
  let mut map=serializer.serialize_map(Some(self.rows.len()))?;
  walk(&self.rows.root,&mut map,self)?;map.end()
 }
}

#[derive(Debug)]
struct PaidOriginalKeyCodecError{error:serde_json::Error,_fee:MemoryReservation}
impl std::fmt::Display for PaidOriginalKeyCodecError{fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result{std::fmt::Display::fmt(&self.error,f)}}
impl std::error::Error for PaidOriginalKeyCodecError{fn source(&self)->Option<&(dyn std::error::Error+'static)>{Some(&self.error)}}

pub(super) struct OriginalProjectionSetSerializeV1<'a,'b,'c>{rows:&'a OriginalProjectionRowsDataV1,common:&'a std::cell::RefCell<&'b mut FormalBoundedContextV1<'c>>,first:&'a std::cell::RefCell<Option<BodyMutationError>>}
impl OriginalProjectionRowsDataV1{
 pub(super) fn original_set_serialization_v1<'a,'b,'c>(&'a self,common:&'a std::cell::RefCell<&'b mut FormalBoundedContextV1<'c>>,first:&'a std::cell::RefCell<Option<BodyMutationError>>)->OriginalProjectionSetSerializeV1<'a,'b,'c>{OriginalProjectionSetSerializeV1{rows:self,common,first}}
}
impl serde::Serialize for OriginalProjectionSetSerializeV1<'_,'_,'_>{fn serialize<S:serde::Serializer>(&self,serializer:S)->Result<S::Ok,S::Error>{
 use serde::ser::{SerializeSeq,Error as _};
 if self.first.borrow().is_some(){return Err(S::Error::custom("original set meter stopped"))}
 if let Err(error)=self.rows.validate(&mut self.common.borrow_mut()){*self.first.borrow_mut()=Some(error);return Err(S::Error::custom("original set source context"))}
 fn walk<S:SerializeSeq>(tree:&Tree,seq:&mut S,view:&OriginalProjectionSetSerializeV1<'_,'_,'_>)->Result<(),S::Error>{let Some(node)=tree else{return Ok(())};if view.first.borrow().is_some(){return Err(S::Error::custom("original set meter stopped"))}if let Err(error)=view.common.borrow_mut().consume_steps(1){*view.first.borrow_mut()=Some(paid(error));return Err(S::Error::custom("original set meter stopped"))}walk(&node.left,seq,view)?;seq.serialize_element(node.row.original_key_value())?;walk(&node.right,seq,view)}
 let mut seq=serializer.serialize_seq(Some(self.rows.len()))?;walk(&self.rows.root,&mut seq,self)?;seq.end()
}}

pub(super) struct OriginalProjectionRowIteratorV1<'a>{stack:Vec<&'a Node>,_fee:MemoryReservation}
impl OriginalProjectionRowsDataV1{
 pub(super) fn iter_data_v1(&self,common:&mut FormalBoundedContextV1<'_>)->Result<OriginalProjectionRowIteratorV1<'_>,BodyMutationError>{
  self.validate(common)?;let capacity=height(&self.root) as usize;let n=capacity.checked_mul(size_of::<&Node>()).and_then(|n|n.checked_add(size_of::<OriginalProjectionRowIteratorV1<'_>>())).ok_or_else(||invalid("original row iterator layout"))?;common.charge(1,n as u64).map_err(paid)?;let fee=common.memory().reserve(n,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
  let mut result=OriginalProjectionRowIteratorV1{stack:Vec::with_capacity(capacity),_fee:fee};if result.stack.capacity()!=capacity{return Err(invalid("original row iterator capacity"))}result.push_left(&self.root,common)?;Ok(result)
 }
}
impl<'a> OriginalProjectionRowIteratorV1<'a>{
 fn push_left(&mut self,tree:&'a Tree,common:&mut FormalBoundedContextV1<'_>)->Result<(),BodyMutationError>{let mut node=tree.as_ref();while let Some(current)=node{common.consume_steps(1).map_err(paid)?;if self.stack.len()==self.stack.capacity(){return Err(invalid("original row iterator depth"))}self.stack.push(current);node=current.left.as_ref()}Ok(())}
 pub(super) fn next_data_v1(&mut self,common:&mut FormalBoundedContextV1<'_>)->Result<Option<&'a OriginalProjectionRowDataV1>,BodyMutationError>{common.consume_steps(1).map_err(paid)?;if !self._fee.uses_budget(common.memory()){return Err(invalid("original row iterator pool"))}let Some(node)=self.stack.pop()else{return Ok(None)};self.push_left(&node.right,common)?;Ok(Some(&node.row))}
}

#[path="incremental_normal_projection_commitment_original_sequence.rs"] mod sequence;
pub(super) use sequence::OriginalProjectionValueSequenceSerializeV1;
