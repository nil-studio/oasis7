//! Count-first original JSON with the same Common shared by nested readonly
//! serializers. Resource callbacks never supply values or select predicates.
use super::*;
use std::{cell::RefCell,io::{self,Write},mem::size_of};
struct Out<'a,'b,'c>{common:&'a RefCell<&'b mut FormalBoundedContextV1<'c>>,first:&'a RefCell<Option<BodyMutationError>>,bytes:Option<Vec<u8>>,length:usize,bound:usize}
impl Write for Out<'_,'_,'_>{fn write(&mut self,bytes:&[u8])->io::Result<usize>{if self.first.borrow().is_some(){return Err(io::ErrorKind::Other.into())}for chunk in bytes.chunks(8192){if let Err(error)=self.common.borrow_mut().charge(1,chunk.len()as u64){*self.first.borrow_mut()=Some(paid(error));return Err(io::ErrorKind::Other.into())}let Some(length)=self.length.checked_add(chunk.len()).filter(|n|*n<=self.bound)else{*self.first.borrow_mut()=Some(invalid("original shared JSON output bound"));return Err(io::ErrorKind::Other.into())};if let Some(bytes)=&mut self.bytes{bytes.extend_from_slice(chunk)}self.length=length}Ok(bytes.len())}fn flush(&mut self)->io::Result<()>{Ok(())}}
#[derive(Debug)]struct CodecError{error:serde_json::Error,_phase:MemoryReservation}
impl std::fmt::Display for CodecError{fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result{self.error.fmt(f)}}impl std::error::Error for CodecError{}
pub(super) fn capture_shared_v1<T:serde::Serialize+?Sized>(value:&T,common:&RefCell<&mut FormalBoundedContextV1<'_>>,first:&RefCell<Option<BodyMutationError>>,limit:usize)->Result<ReservedValue<Vec<u8>>,BodyMutationError>{
 // All live callers are the closed original-column serializers; their codec messages are fixed, while typed resource errors retain their own owners.
 let phase_bytes=32768;common.borrow_mut().charge(1,phase_bytes as u64).map_err(paid)?;let error_phase=common.borrow_mut().memory().reserve(phase_bytes,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
 let mut count=Out{common,first,bytes:None,length:0,bound:limit};let result=serde_json::to_writer(&mut count,value);if let Some(error)=first.borrow_mut().take(){return Err(error)}
 if let Err(error)=result{return Err(paid(CodecError{error,_phase:error_phase}))}let n=count.length;
 let admission=size_of::<Vec<u8>>().checked_add(n).ok_or_else(||invalid("original shared JSON allocation"))?;common.borrow_mut().charge(1,admission as u64).map_err(paid)?;let fee=common.borrow_mut().memory().reserve(admission,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;let bytes=Vec::with_capacity(n);if bytes.capacity()!=n{return Err(invalid("original shared JSON capacity"))}
 let mut output=Out{common,first,bytes:Some(bytes),length:0,bound:n};let result=serde_json::to_writer(&mut output,value);if let Some(error)=first.borrow_mut().take(){return Err(error)}common.borrow_mut().consume_steps(0).map_err(paid)?;if let Err(error)=result{return Err(paid(CodecError{error,_phase:error_phase}))}
 if output.length!=n{return Err(invalid("original shared JSON unstable size"))}Ok(fee.retain(output.bytes.ok_or_else(||invalid("original shared JSON bytes missing"))?))
}
