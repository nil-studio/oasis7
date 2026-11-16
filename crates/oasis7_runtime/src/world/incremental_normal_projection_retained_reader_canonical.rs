//! Original canonical ABI bytes, on the caller's actual continuing codec/Common.
use crate::world::{incremental_body::BodyMutationError,normal_bootstrap_current::hash::BootstrapCanonicalMeterV1};
use oasis7_formal_protocol::{controlled_authority::formal::bounded_context::FormalBoundedContextV1,incremental_value::ValueTreeError};
use oasis7_proto::memory_budget::{MemoryClass,ReservedValue};
use oasis7_wasm_abi::{CanonicalCborAllocator,CanonicalCborWorkV1,stream_canonical_cbor_with_work_v1};
use std::{cell::RefCell,io::{self,Write},mem::size_of};
fn original<E:std::error::Error+Send+Sync+'static>(error:E)->BodyMutationError{BodyMutationError::Original(Box::new(error))}
struct Count<'a,'b,'c,'d,'e>{meter:&'a BootstrapCanonicalMeterV1<'b,'c,'d,'e>,n:usize}
impl Write for Count<'_, '_, '_, '_, '_>{fn write(&mut self,bytes:&[u8])->io::Result<usize>{self.meter.charge_work(1,0).map_err(|_|io::ErrorKind::Other)?;self.n=self.n.checked_add(bytes.len()).ok_or(io::ErrorKind::OutOfMemory)?;Ok(bytes.len())}fn flush(&mut self)->io::Result<()>{Ok(())}}
struct Output<'a,'b,'c,'d,'e>{meter:&'a BootstrapCanonicalMeterV1<'b,'c,'d,'e>,bytes:Vec<u8>,bound:usize}
impl Write for Output<'_, '_, '_, '_, '_>{fn write(&mut self,bytes:&[u8])->io::Result<usize>{for part in bytes.chunks(8192){self.meter.charge_work(1,part.len()as u64).map_err(|_|io::ErrorKind::Other)?;let n=self.bytes.len().checked_add(part.len()).filter(|n|*n<=self.bound).ok_or(io::ErrorKind::InvalidData)?;self.bytes.extend_from_slice(part);debug_assert_eq!(self.bytes.len(),n)}Ok(bytes.len())}fn flush(&mut self)->io::Result<()>{Ok(())}}
pub(super) fn encode_original_canonical_data_v1<T:serde::Serialize+?Sized>(value:&T,common:&mut FormalBoundedContextV1<'_>,canonical:&mut CanonicalCborWorkV1)->Result<ReservedValue<Vec<u8>>,BodyMutationError>{
 let common=RefCell::new(common);let canonical=RefCell::new(canonical);let meter=BootstrapCanonicalMeterV1::new(&common,&canonical);
 let mut count=Count{meter:&meter,n:0};let encoded=stream_canonical_cbor_with_work_v1(value,&meter,&mut count);meter.check()?;encoded.map_err(original)?;
 let n=size_of::<Vec<u8>>().checked_add(count.n).ok_or(BodyMutationError::Invalid("retained canonical output layout"))?;
 common.borrow_mut().charge(1,n as u64).map_err(original)?;let fee=common.borrow().memory().reserve(n,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
 let bytes=Vec::with_capacity(count.n);if bytes.capacity()!=count.n{return Err(BodyMutationError::Invalid("retained canonical exact capacity"))}
 let mut out=Output{meter:&meter,bytes,bound:count.n};let encoded=stream_canonical_cbor_with_work_v1(value,&meter,&mut out);meter.check()?;encoded.map_err(original)?;
 if out.bytes.len()!=count.n{return Err(BodyMutationError::Invalid("retained canonical output changed"))}common.borrow_mut().consume_steps(0).map_err(original)?;Ok(fee.retain(out.bytes))
}
