//! Work-only companion of the unique wake predicates. Codec fallback is only
//! for the original codec rejection, never for a caller allowance failure.
use super::*;
use oasis7_wasm_abi::{CapabilityCanonicalWorkV1,CanonicalCborStreamError as Error};
use std::io::{self,Write};
#[derive(Debug)]
pub enum WakeConditionWorkErrorV1{Business(WakeConditionError),Work(Error)}
impl From<WakeConditionError> for WakeConditionWorkErrorV1{fn from(e:WakeConditionError)->Self{Self::Business(e)}}
impl From<Error> for WakeConditionWorkErrorV1{fn from(e:Error)->Self{Self::Work(e)}}
pub(super) fn legacy<T>(result:Result<T,WakeConditionWorkErrorV1>)->Result<T,WakeConditionError>{match result{Ok(v)=>Ok(v),Err(WakeConditionWorkErrorV1::Business(e))=>Err(e),Err(WakeConditionWorkErrorV1::Work(_))=>unreachable!("legacy wake observer cannot fail")}}
pub(super) struct Bytes<C>{pub(super) value:Vec<u8>,_fee:Option<C>}
pub(super) fn fields<A:CapabilityCanonicalWorkV1>(condition:&WakeConditionV1,source:&A)->Result<(),Error>{
 for text in [Some(condition.schema_version.as_str()),Some(condition.kind.as_str()),condition.event_digest.as_deref(),condition.receipt_id.as_deref(),condition.subject.as_ref().map(|s|s.kind.as_str()),condition.subject.as_ref().map(|s|s.id.as_str()),condition.path_or_rule.as_deref(),condition.operator.as_deref()].into_iter().flatten(){for part in text.as_bytes().chunks(8192){source.charge_work(1+4*part.len()as u64,part.len()as u64)?;}}
 source.charge_work(16,0)
}
pub(super) fn before_expected<A:CapabilityCanonicalWorkV1>(bytes:&[u8],source:&A)->Result<Option<A::Charge>,Error>{
 if !source.is_metered(){return Ok(None)}
 // serde_cbor::Value's original visit_seq starts Vec::new and pushes actual
 // elements (no wire size_hint allocation). Every node consumes input; this
 // covers its geometric Vec/BTree graph plus canonical encoder's live graph.
 let n=bytes.len();let heap=n.checked_mul(2048).and_then(|n|n.checked_add(65536)).ok_or(Error::ResourceLimited("wake expected value graph"))?;
 let steps=n.checked_mul(n.max(1)).and_then(|n|n.checked_mul(32)).and_then(|n|n.checked_add(1024)).ok_or(Error::ResourceLimited("wake expected value visits"))?;
 source.charge_work(steps as u64,n as u64)?;Ok(Some(source.reserve(heap)?))
}
// Original codec-invalid inputs are wake business refusals. Only actual
// work/IO failures are registered as continuing infrastructure first causes.
struct CodecSource<'a,A>(&'a A);
impl<A:CapabilityCanonicalWorkV1>oasis7_wasm_abi::CanonicalCborAllocator for CodecSource<'_,A>{type Charge=A::Charge;fn charge_work(&self,s:u64,b:u64)->Result<(),Error>{self.0.charge_work(s,b)}fn reserve(&self,n:usize)->Result<Self::Charge,Error>{self.0.reserve(n)}}
impl<A:CapabilityCanonicalWorkV1>CapabilityCanonicalWorkV1 for CodecSource<'_,A>{type Loan<'x>=A::Loan<'x>where Self:'x;fn is_metered(&self)->bool{self.0.is_metered()}fn loan_canonical_work(&self)->Self::Loan<'_>{self.0.loan_canonical_work()}fn record_stream_error(&self,e:Error)->Error{match e{Error::Invalid(_)=>e,other=>self.0.record_stream_error(other)}}}
pub(super) fn encode_value<T:serde::Serialize+?Sized,A:CapabilityCanonicalWorkV1>(value:&T,source:&A)->Result<Bytes<A::Charge>,Error>{
 if !source.is_metered(){return oasis7_wasm_abi::encode_canonical_cbor(&value).map(|value|Bytes{value,_fee:None}).map_err(|_|Error::Invalid("wake canonical codec"))}
 struct Count<'a,A>{source:&'a A,n:usize,first:Option<Error>}
 impl<A:CapabilityCanonicalWorkV1>Write for Count<'_,A>{fn write(&mut self,b:&[u8])->io::Result<usize>{if let Err(e)=self.source.charge_work(1,b.len()as u64){self.first=Some(e);return Err(io::ErrorKind::Other.into())}self.n=self.n.checked_add(b.len()).ok_or(io::ErrorKind::Other)?;Ok(b.len())}fn flush(&mut self)->io::Result<()>{Ok(())}}
 let codec=CodecSource(source);let mut count=Count{source,n:0,first:None};let result=oasis7_wasm_abi::stream_canonical_cbor_with_work_v1(value,&codec,&mut count);if let Some(e)=count.first{return Err(e)}result?;
 let n=count.n;source.charge_work(1,n as u64)?;let fee=source.reserve(n+std::mem::size_of::<Vec<u8>>())?;
 struct Out<'a,A>{source:&'a A,bytes:Vec<u8>,limit:usize,first:Option<Error>}
 impl<A:CapabilityCanonicalWorkV1>Write for Out<'_,A>{fn write(&mut self,b:&[u8])->io::Result<usize>{if b.len()>self.limit-self.bytes.len(){return Err(io::ErrorKind::Other.into())}if let Err(e)=self.source.charge_work(1,b.len()as u64){self.first=Some(e);return Err(io::ErrorKind::Other.into())}self.bytes.extend_from_slice(b);Ok(b.len())}fn flush(&mut self)->io::Result<()>{Ok(())}}
 let mut out=Out{source,bytes:Vec::with_capacity(n),limit:n,first:None};if out.bytes.capacity()!=n{return Err(Error::ResourceLimited("wake canonical Vec capacity"))}let result=oasis7_wasm_abi::stream_canonical_cbor_with_work_v1(value,&codec,&mut out);if let Some(e)=out.first{return Err(e)}result?;if out.bytes.len()!=n{return Err(Error::Invalid("wake canonical measure changed"))}Ok(Bytes{value:out.bytes,_fee:Some(fee)})
}
pub(super) fn canonical_condition<A:CapabilityCanonicalWorkV1>(condition:&WakeConditionV1,source:&A)->Result<Bytes<A::Charge>,Error>{
 match encode_value(condition,source){Ok(value)=>Ok(value),Err(Error::Invalid(_))=>{
   // Original serde_cbor fallback, on this closed condition DTO only. Strings
   // and byte vectors are real selected values, not generic caller Serialize.
   let n=condition.schema_version.len()+condition.kind.len()+condition.event_digest.as_ref().map_or(0,String::len)+condition.receipt_id.as_ref().map_or(0,String::len)+condition.subject.as_ref().map_or(0,|s|s.kind.len()+s.id.len())+condition.path_or_rule.as_ref().map_or(0,String::len)+condition.operator.as_ref().map_or(0,String::len)+condition.expected_value_bytes.as_ref().map_or(0,Vec::len);
   let heap=n.checked_mul(16).and_then(|n|n.checked_add(65536)).ok_or(Error::ResourceLimited("wake fallback layout"))?;source.charge_work(1+heap as u64,n as u64)?;let fee=if source.is_metered(){Some(source.reserve(heap)?)}else{None};Ok(Bytes{value:condition.canonical_bytes_unchecked(),_fee:fee})
 },Err(error)=>Err(error)}
}
