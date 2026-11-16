//! Paid DATA observer of the sole original canonical-CBOR parser. It never
//! reads bytes, chooses tokens or changes a schema/hash/identity predicate.
use super::*;
use crate::incremental_runtime_revocation::BusinessDiffProcessingWorkV1;
use serde_json::{Map,Value};
use std::mem::size_of;
enum Frame{Array{values:Vec<Value>,count:u64},Object{values:Map<String,Value>,count:u64,key:Option<String>}}
pub(super) struct Collector{stack:Vec<Frame>,root:Option<Value>,text:Option<(Vec<u8>,usize)>,heap_remaining:usize,fee:oasis7_proto::memory_budget::MemoryReservation}
impl Collector{
 pub(super) fn new(bytes:u64,memory:&MemoryBudget,class:MemoryClass,processing:&mut BusinessDiffProcessingWorkV1)->Result<Self,ValueTreeError>{
  let bytes=usize::try_from(bytes).map_err(|_|invalid())?;
  // A one-entry BTree node can retain eleven KV slots. The actual byte bound
  // covers those nodes, geometric array/frame capacities, text copies, and
  // the Value/observer/output headers simultaneously. It is not a new Work.
  let n=bytes.checked_mul(1024).and_then(|n|n.checked_add(32768+size_of::<Self>()+size_of::<Value>())).ok_or_else(invalid)?;
  let amount=u64::try_from(n).map_err(|_|invalid())?;let steps=amount.checked_add(1).ok_or_else(invalid)?;let before=(processing.steps_remaining,processing.bytes_remaining);processing.steps_remaining=processing.steps_remaining.saturating_sub(steps);processing.bytes_remaining=processing.bytes_remaining.saturating_sub(amount);if before.0<steps||before.1<amount{return Err(ValueTreeError::ResourceLimited("Cognition original JSON processing"))}
  let fee=memory.reserve(n,class)?;Ok(Self{stack:Vec::new(),root:None,text:None,heap_remaining:n,fee})
 }
 fn heap(&mut self,n:usize)->Result<(),ValueTreeError>{self.heap_remaining=self.heap_remaining.checked_sub(n).ok_or(ValueTreeError::ResourceLimited("Cognition original JSON graph"))?;Ok(())}
 fn frame(&mut self,value:Frame)->Result<(),ValueTreeError>{if self.text.is_some(){return Err(invalid())}if self.stack.len()==self.stack.capacity(){let cap=self.stack.capacity().checked_mul(2).ok_or_else(invalid)?.max(4);self.heap(cap.checked_mul(size_of::<Frame>()).ok_or_else(invalid)?)?;self.stack.try_reserve_exact(cap-self.stack.len()).map_err(|_|ValueTreeError::ResourceLimited("Cognition JSON frame allocation"))?;if self.stack.capacity()>cap{return Err(invalid())}}self.stack.push(value);Ok(())}
 pub(super) fn array(&mut self,count:u64)->Result<(),ValueTreeError>{self.frame(Frame::Array{values:Vec::new(),count})}
 pub(super) fn object(&mut self,count:u64)->Result<(),ValueTreeError>{self.frame(Frame::Object{values:Map::new(),count,key:None})}
 pub(super) fn key(&mut self,key:&[u8])->Result<(),ValueTreeError>{
  self.heap(key.len().checked_add(128).ok_or_else(invalid)?)?;
  let mut bytes=Vec::new();bytes.try_reserve_exact(key.len()).map_err(|_|ValueTreeError::ResourceLimited("Cognition JSON key allocation"))?;if bytes.capacity()>key.len().checked_add(128).ok_or_else(invalid)?{return Err(invalid())}bytes.extend_from_slice(key);let text=String::from_utf8(bytes).map_err(|_|invalid())?;
  let Some(Frame::Object{values,count,key})=self.stack.last_mut()else{return Err(invalid())};if key.is_some()||values.len()as u64>=*count{return Err(invalid())}*key=Some(text);Ok(())
 }
 pub(super) fn text(&mut self,length:u64)->Result<(),ValueTreeError>{if self.text.is_some(){return Err(invalid())}let n=usize::try_from(length).map_err(|_|invalid())?;self.heap(n.checked_add(128).ok_or_else(invalid)?)?;let mut text=Vec::new();text.try_reserve_exact(n).map_err(|_|ValueTreeError::ResourceLimited("Cognition JSON text allocation"))?;if text.capacity()>n.checked_add(128).ok_or_else(invalid)?{return Err(invalid())}self.text=Some((text,n));Ok(())}
 pub(super) fn feed_text(&mut self,bytes:&[u8])->Result<(),ValueTreeError>{let Some((text,n))=&mut self.text else{return Err(invalid())};if text.len().checked_add(bytes.len()).is_none_or(|len|len>*n){return Err(invalid())}text.extend_from_slice(bytes);Ok(())}
 pub(super) fn end_text(&mut self)->Result<(),ValueTreeError>{let(text,n)=self.text.take().ok_or_else(invalid)?;if text.len()!=n{return Err(invalid())}self.scalar(Value::String(String::from_utf8(text).map_err(|_|invalid())?))}
 pub(super) fn scalar(&mut self,value:Value)->Result<(),ValueTreeError>{
  if self.text.is_some(){return Err(invalid())}
  match self.stack.last(){Some(Frame::Array{values,count})=>{if values.len()as u64>=*count{return Err(invalid())}if values.len()==values.capacity(){let cap=values.capacity().checked_mul(2).ok_or_else(invalid)?.max(4);self.heap(cap.checked_mul(size_of::<Value>()).ok_or_else(invalid)?)?;let Some(Frame::Array{values,..})=self.stack.last_mut()else{return Err(invalid())};values.try_reserve_exact(cap-values.len()).map_err(|_|ValueTreeError::ResourceLimited("Cognition JSON array allocation"))?;if values.capacity()>cap{return Err(invalid())}}},Some(Frame::Object{..})=>self.heap(11*size_of::<(String,Value)>()+256)?,None=>self.heap(size_of::<Value>()+128)?}
  match self.stack.last_mut(){Some(Frame::Array{values,..})=>values.push(value),Some(Frame::Object{values,key,..})=>{let key=key.take().ok_or_else(invalid)?;if values.insert(key,value).is_some(){return Err(invalid())}},None=>{if self.root.replace(value).is_some(){return Err(invalid())}}}Ok(())
 }
 pub(super) fn end_container(&mut self)->Result<(),ValueTreeError>{let frame=self.stack.pop().ok_or_else(invalid)?;let value=match frame{Frame::Array{values,count}=>{if values.len()as u64!=count{return Err(invalid())}Value::Array(values)},Frame::Object{values,count,key}=>{if key.is_some()||values.len()as u64!=count{return Err(invalid())}Value::Object(values)}};self.scalar(value)}
 pub(super) fn finish(self)->Result<ReservedValue<Value>,ValueTreeError>{if !self.stack.is_empty()||self.text.is_some(){return Err(invalid())}let value=self.root.ok_or_else(invalid)?;Ok(self.fee.retain(value))}
}
