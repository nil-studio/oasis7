//! Exact original typed JSON fragment, parsed once by serde_json's RawValue
//! codec and retained with its allocation. No World/State hydration or detach.
use super::*;
use std::{io::{self,Write},mem::size_of,sync::Arc};
pub(super) struct PaidOriginalJsonFragmentDataV1{value:Box<serde_json::value::RawValue>,_fee:MemoryReservation}
impl PaidOriginalJsonFragmentDataV1{
 pub(super) fn as_bytes(&self)->&[u8]{self.value.get().as_bytes()}
 pub(super) fn raw(&self)->&serde_json::value::RawValue{&self.value}
 pub(super) fn uses_budget(&self,memory:&MemoryBudget)->bool{self._fee.uses_budget(memory)}
}
struct Output<'a,'b>{common:&'a mut FormalBoundedContextV1<'b>,bytes:Option<Vec<u8>>,length:usize,limit:usize,first:Option<BodyMutationError>}
impl Write for Output<'_,'_>{
 fn write(&mut self,value:&[u8])->io::Result<usize>{if self.first.is_some(){return Err(io::ErrorKind::Other.into())}for chunk in value.chunks(8192){if let Err(error)=self.common.charge(1,chunk.len()as u64){self.first=Some(paid(error));return Err(io::ErrorKind::Other.into())}let Some(length)=self.length.checked_add(chunk.len()).filter(|n|*n<=self.limit)else{self.first=Some(ValueTreeError::ResourceLimited("original JSON fragment output").into());return Err(io::ErrorKind::Other.into())};if let Some(bytes)=&mut self.bytes{bytes.extend_from_slice(chunk)}self.length=length}Ok(value.len())}
 fn flush(&mut self)->io::Result<()>{Ok(())}
}
#[derive(Debug)]struct PaidCodecError{error:serde_json::Error,_fee:MemoryReservation}
impl std::fmt::Display for PaidCodecError{fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result{std::fmt::Display::fmt(&self.error,f)}}
impl std::error::Error for PaidCodecError{fn source(&self)->Option<&(dyn std::error::Error+'static)>{Some(&self.error)}}
fn reserve(common:&mut FormalBoundedContextV1<'_>,bytes:usize)->Result<MemoryReservation,BodyMutationError>{common.charge(1,bytes as u64).map_err(paid)?;common.memory().reserve(bytes,MemoryClass::Ordinary).map_err(|e|ValueTreeError::Memory(e).into())}
/// Used only with a true closed typed source. `max_output_bytes` is the
/// existing source/record limit, not a replacement or refillable work budget.
/// Source graph fees stay with its caller throughout both original passes.
pub(super) fn prepare_original_json_fragment_data_v1<T:serde::Serialize+?Sized>(value:&T,max_output_bytes:usize,common:&mut FormalBoundedContextV1<'_>)->Result<Arc<PaidOriginalJsonFragmentDataV1>,BodyMutationError>{
 let error_bound=max_output_bytes.checked_mul(2).and_then(|n|n.checked_add(32768+size_of::<PaidCodecError>())).ok_or_else(||invalid("original JSON refusal layout"))?;let error_fee=reserve(common,error_bound)?;
 let mut count=Output{common,bytes:None,length:0,limit:max_output_bytes,first:None};let result=serde_json::to_writer(&mut count,value);if let Some(error)=count.first{return Err(error)}
 if let Err(error)=result{return Err(paid(PaidCodecError{error,_fee:error_fee}))}count.common.consume_steps(0).map_err(paid)?;let length=count.length;
 let fee=reserve(count.common,length.checked_add(size_of::<PaidOriginalJsonFragmentDataV1>()+2*size_of::<usize>()).ok_or_else(||invalid("original JSON fragment layout"))?)?;let bytes=Vec::with_capacity(length);if bytes.capacity()!=length{return Err(invalid("original JSON fragment capacity"))}
 let mut output=Output{common:count.common,bytes:Some(bytes),length:0,limit:length,first:None};let result=serde_json::to_writer(&mut output,value);if let Some(error)=output.first{return Err(error)}
 if let Err(error)=result{return Err(paid(PaidCodecError{error,_fee:error_fee}))}output.common.consume_steps(0).map_err(paid)?;if output.length!=length{return Err(invalid("original JSON fragment changed between passes"))}
 let bytes=output.bytes.ok_or_else(||invalid("original JSON fragment output"))?;
 // The original serializer guarantees UTF8. Charge the actual validation and
 // RawValue parser input before the consuming zero-graph codec conversion.
 for chunk in bytes.chunks(8192){output.common.charge((chunk.len()as u64)*2,(chunk.len()as u64)*2).map_err(paid)?}
 let text=String::from_utf8(bytes).map_err(|_|invalid("original JSON encoder UTF8"))?;
 let raw=serde_json::value::RawValue::from_string(text).map_err(|error|paid(PaidCodecError{error,_fee:error_fee}))?;output.common.consume_steps(0).map_err(paid)?;
 Ok(Arc::new(PaidOriginalJsonFragmentDataV1{value:raw,_fee:fee}))
}
