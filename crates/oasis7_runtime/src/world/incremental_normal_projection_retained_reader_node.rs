//! Actual Original IO adapter into the sole paid business-SMT lookup kernel.
use super::*;
use oasis7_formal_protocol::controlled_authority::formal::{FormalError,business_index::{BusinessIndexRootV1,PaidBusinessIndexNodeReaderV1,PaidBusinessIndexLeafV1,PaidBusinessIndexErrorV1,BUSINESS_INDEX_MAX_LEAF_BYTES,get_business_index_leaf_bounded_v1}};
use std::{cell::RefCell,mem::size_of};
const FAILED:&str="paid retained original IO failed";
pub(in crate::world) enum PaidRetainedNodeReadErrorV1<E>{Context(FormalError),Memory(oasis7_proto::memory_budget::MemoryLimitExceeded),Reader{cause:E,index:Option<PaidBusinessIndexErrorV1>},Index(PaidBusinessIndexErrorV1)}
impl<E:std::fmt::Debug> std::fmt::Debug for PaidRetainedNodeReadErrorV1<E>{fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result{match self{Self::Context(e)=>std::fmt::Debug::fmt(e,f),Self::Memory(e)=>std::fmt::Debug::fmt(e,f),Self::Reader{cause,..}=>f.debug_tuple("RetainedOriginal").field(cause).finish(),Self::Index(error)=>std::fmt::Debug::fmt(error,f)}}}
impl<E:std::fmt::Display> std::fmt::Display for PaidRetainedNodeReadErrorV1<E>{fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result{match self{Self::Context(e)=>std::fmt::Display::fmt(e,f),Self::Memory(e)=>std::fmt::Display::fmt(e,f),Self::Reader{cause,..}=>std::fmt::Display::fmt(cause,f),Self::Index(error)=>std::fmt::Display::fmt(error,f)}}}
impl<E:std::error::Error+'static> std::error::Error for PaidRetainedNodeReadErrorV1<E>{fn source(&self)->Option<&(dyn std::error::Error+'static)>{match self{Self::Context(e)=>Some(e),Self::Memory(e)=>Some(e),Self::Reader{cause,..}=>Some(cause),Self::Index(error)=>Some(error)}}}
struct Reader<'a,R:PaidProjectionRetainedOriginalReaderV1>{reader:RefCell<&'a mut R>,first:RefCell<Option<R::RetainedError>>,_fee:oasis7_proto::memory_budget::MemoryReservation}
fn bad(note:&str)->FormalError{FormalError::Invalid(note.into())}
impl<R:PaidProjectionRetainedOriginalReaderV1> PaidBusinessIndexNodeReaderV1 for Reader<'_,R>{
 fn read_node_bounded(&self,digest:&str,common:&mut FormalBoundedContextV1<'_>)->Result<Option<ReservedValue<Vec<u8>>>,FormalError>{
  common.charge(1,digest.len()as u64)?;if self.first.borrow().is_some(){return Err(bad(FAILED))}
  if digest.len()!=64{return Err(bad("retained business node digest length"))}
  let id_fee=common.memory().reserve(size_of::<Vec<u8>>()+32,MemoryClass::Ordinary).map_err(|_|bad("retained node ID admission"))?;
  let raw_id=hex::decode(digest).map_err(|_|bad("retained business node digest encoding"))?;let id:[u8;32]=raw_id.try_into().map_err(|_|bad("retained business node digest length"))?;drop(id_fee);
  let raw={let mut reader=self.reader.borrow_mut();
   if !reader.allocation_budget().same_pool(common.memory())||reader.allocation_class()!=MemoryClass::Ordinary{return Err(bad("retained Original actual pool/class"))}
   match reader.read_retained_original_bounded_v1(&id,BUSINESS_INDEX_MAX_LEAF_BYTES+128,common){Ok(raw)=>raw,Err(cause)=>{*self.first.borrow_mut()=Some(cause);return Err(bad(FAILED))}}
  };
  if !raw.uses_budget(common.memory())||raw.reserved_bytes()<size_of::<Vec<u8>>().saturating_add(raw.capacity())||raw.len()>BUSINESS_INDEX_MAX_LEAF_BYTES+128{return Err(bad("retained Original paid graph/byte bound"))}
  let n=size_of::<Vec<u8>>().checked_add(raw.len()).ok_or_else(||bad("retained node copy layout"))?;common.charge(1,n as u64)?;
  let fee=common.memory().reserve(n,MemoryClass::Ordinary).map_err(|_|bad("retained node exact copy admission"))?;
  let mut out=Vec::with_capacity(raw.len());if out.capacity()!=raw.len(){return Err(bad("retained node exact copy capacity"))}
  for part in raw.chunks(8192){common.charge(1,part.len()as u64)?;out.extend_from_slice(part)}common.consume_steps(0)?;Ok(Some(fee.retain(out)))
 }
}
pub(in crate::world) fn get_retained_index_leaf_bounded_v1<R:PaidProjectionRetainedOriginalReaderV1>(root:&BusinessIndexRootV1,key:&str,reader:&mut R,common:&mut FormalBoundedContextV1<'_>)->Result<Option<PaidBusinessIndexLeafV1>,PaidRetainedNodeReadErrorV1<R::RetainedError>>{
 let n=size_of::<Reader<'_,R>>().checked_add(FAILED.len()+size_of::<FormalError>()+256).ok_or_else(||PaidRetainedNodeReadErrorV1::Context(bad("retained adapter layout")))?;
 common.charge(1,n as u64).map_err(PaidRetainedNodeReadErrorV1::Context)?;
 let fee=common.memory().reserve(n,MemoryClass::Ordinary).map_err(PaidRetainedNodeReadErrorV1::Memory)?;
 let adapter=Reader{reader:RefCell::new(reader),first:RefCell::new(None),_fee:fee};let result=get_business_index_leaf_bounded_v1(root,key,&adapter,common);
 let cause=adapter.first.borrow_mut().take();match(cause,result){(Some(cause),Err(index))=>Err(PaidRetainedNodeReadErrorV1::Reader{cause,index:Some(index)}),(Some(cause),Ok(_))=>Err(PaidRetainedNodeReadErrorV1::Reader{cause,index:None}),(None,Err(error))=>Err(PaidRetainedNodeReadErrorV1::Index(error)),(None,Ok(value))=>Ok(value)}
}
