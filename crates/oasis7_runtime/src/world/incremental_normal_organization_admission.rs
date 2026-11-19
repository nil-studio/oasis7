//! Paid actual-shape copies for the closed organization DTOs.
use super::*;
use crate::world::incremental_normal_tail::NormalTailWorkV1;
use oasis7_formal_protocol::incremental_value::ValueTreeError;
fn bad()->ValueTreeError{ValueTreeError::ResourceLimited("organization copy allocation")}
pub(super) fn add(a:usize,b:usize)->Result<usize,BodyMutationError>{a.checked_add(b).ok_or_else(||bad().into())}
/// Same conservative one-node-per-entry bound as the original typed codec's
/// btree_output_heap. It covers leaves, internal edges, and alignment padding.
pub(super) fn tree_entry<K,V>()->Result<usize,BodyMutationError>{
 let alignment=std::mem::align_of::<K>().max(std::mem::align_of::<V>()).max(std::mem::align_of::<usize>()).max(std::mem::align_of::<u16>());
 let slots=std::mem::size_of::<K>().checked_add(std::mem::size_of::<V>()).and_then(|n|n.checked_mul(11)).ok_or_else(bad)?;
 let header=13usize.checked_mul(std::mem::size_of::<usize>()).and_then(|n|n.checked_add(2*std::mem::size_of::<u16>())).ok_or_else(bad)?;
 add(add(slots,header)?,alignment.checked_sub(1).and_then(|n|n.checked_mul(7)).ok_or_else(bad)?)
}
/// Every dynamic copy is admitted before allocation; source reservations remain
/// independently live. The fee-vector capacity is itself paid before growth.
pub(super) struct AdmissionV1 {fees:Vec<MemoryReservation>,metadata:MemoryReservation,bytes:usize}
impl AdmissionV1 {
 pub(super) fn new(tail:&NormalTailWorkV1<'_,'_>)->Result<Self,BodyMutationError>{Ok(Self{fees:Vec::new(),metadata:tail.reserve(0,MemoryClass::Ordinary)?,bytes:0})}
 pub(super) fn admit(&mut self,bytes:usize,tail:&mut NormalTailWorkV1<'_,'_>)->Result<(),BodyMutationError>{
 tail.charge(bytes)?;
 if self.fees.len()==self.fees.capacity(){let cap=self.fees.capacity().checked_mul(2).ok_or_else(bad)?.max(4);
 let metadata=tail.reserve(cap.checked_mul(size_of::<MemoryReservation>()).ok_or_else(bad)?,MemoryClass::Ordinary)?;
 self.fees.try_reserve_exact(cap-self.fees.len()).map_err(|_|bad())?;if self.fees.capacity()>cap{return Err(bad().into())}self.metadata=metadata;}
 let fee=tail.reserve(bytes,MemoryClass::Ordinary)?;self.bytes=add(self.bytes,bytes)?;self.fees.push(fee);Ok(())
 }
 pub(super) fn bytes(&self)->usize{self.bytes}
}
