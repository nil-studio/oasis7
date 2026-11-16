//! One Common and one continuing canonical loan; codec sentinels never replace
//! the first typed Common/Memory/cancellation cause.
use super::*;
impl<M:MerkleNodeReader>CanonicalCborAllocator for Provider<'_, '_, '_, '_, '_, '_,M>{
 type Charge=MemoryReservation;
 fn charge_work(&self,s:u64,b:u64)->Result<(),CanonicalCborStreamError>{if self.first.borrow().is_some(){return Err(CanonicalCborStreamError::Poisoned)}self.view.facts.work(s,usize::try_from(b).map_err(|_|CanonicalCborStreamError::ResourceLimited("provider work bytes"))?).map_err(|e|{*self.first.borrow_mut()=Some(e);CanonicalCborStreamError::Poisoned})}
 fn reserve(&self,n:usize)->Result<Self::Charge,CanonicalCborStreamError>{if self.first.borrow().is_some(){return Err(CanonicalCborStreamError::Poisoned)}self.view.facts.common.borrow().memory().reserve(n,MemoryClass::Ordinary).map_err(|e|{*self.first.borrow_mut()=Some(ValueTreeError::Memory(e).into());CanonicalCborStreamError::Poisoned})}
}
impl<M:MerkleNodeReader>CapabilityCanonicalWorkV1 for Provider<'_, '_, '_, '_, '_, '_,M>{
 type Loan<'x>=std::cell::RefMut<'x,oasis7_wasm_abi::CanonicalCborWorkV1>where Self:'x;
 fn is_metered(&self)->bool{true}
 fn loan_canonical_work(&self)->Self::Loan<'_>{std::cell::RefMut::map(self.canonical.borrow_mut(),|work|&mut **work)}
 fn record_stream_error(&self,error:CanonicalCborStreamError)->CanonicalCborStreamError{if self.first.borrow().is_none(){*self.first.borrow_mut()=Some(paid(error))}CanonicalCborStreamError::Poisoned}
}
pub(super) struct Ops<'p,S>(pub(super)&'p S);
impl<S:CapabilityCanonicalWorkV1>CanonicalCborAllocator for Ops<'_,S>{type Charge=S::Charge;fn charge_work(&self,s:u64,b:u64)->Result<(),CanonicalCborStreamError>{self.0.charge_work(s,b)}fn reserve(&self,n:usize)->Result<Self::Charge,CanonicalCborStreamError>{self.0.reserve(n)}}
impl<S:CapabilityCanonicalWorkV1>CapabilityCanonicalWorkV1 for Ops<'_,S>{type Loan<'x>=S::Loan<'x>where Self:'x;fn is_metered(&self)->bool{self.0.is_metered()}fn loan_canonical_work(&self)->Self::Loan<'_>{self.0.loan_canonical_work()}fn record_stream_error(&self,e:CanonicalCborStreamError)->CanonicalCborStreamError{self.0.record_stream_error(e)}}
trait FixedOps:CapabilityCanonicalWorkV1{
 fn actual_work(&self,s:usize,b:usize)->Result<(),Error>;
 fn actual_value<T:serde::Serialize>(&self,v:&T)->Result<(),Error>;
 fn actual_slots<T>(&self,n:usize)->Result<(),Error>;
}
impl<M:MerkleNodeReader>FixedOps for Provider<'_, '_, '_, '_, '_, '_,M>{
 fn actual_work(&self,s:usize,b:usize)->Result<(),Error>{self.check()?;self.view.facts.work(s as u64,b)?;Ok(())}
 fn actual_value<T:serde::Serialize>(&self,v:&T)->Result<(),Error>{self.view.before_clone(v)?;Ok(())}
 fn actual_slots<T>(&self,n:usize)->Result<(),Error>{let bytes=n.max(4).checked_mul(size_of::<T>()).and_then(|n|n.checked_add(256)).ok_or_else(||invalid("provider slots layout"))?;self.view.facts.work(1,bytes)?;self.view.facts.before_owned(bytes)?;Ok(())}
}
impl<S:FixedOps>catalog::CapabilityCatalogOpsV1<Error>for Ops<'_,S>{
 fn work(&mut self,s:usize,b:usize)->Result<(),Error>{self.0.actual_work(s,b)}
 fn before_value<T:serde::Serialize>(&mut self,v:&T)->Result<(),Error>{self.0.actual_value(v)}
 fn before_slots<T>(&mut self,n:usize)->Result<(),Error>{self.0.actual_slots::<T>(n)}
 fn before_command_catalog(&mut self,v:&crate::modules::ModuleRegistry)->Result<(),Error>{self.0.actual_value(v)}
}
impl<S>terminal::sealed::Ops for Ops<'_,S>{}
impl<S:FixedOps>terminal::ProviderTerminalOpsV1<Error>for Ops<'_,S>{
 fn work(&mut self,s:usize,b:usize)->Result<(),Error>{self.0.actual_work(s,b)}
 fn before_value<T:serde::Serialize>(&mut self,v:&T)->Result<(),Error>{self.0.actual_value(v)}
 fn before_slots<T>(&mut self,n:usize)->Result<(),Error>{self.0.actual_slots::<T>(n)}
 fn before_map<K,V>(&mut self,n:usize)->Result<(),Error>{let count=n.checked_mul(11).ok_or_else(||invalid("provider map layout"))?;self.0.actual_slots::<(K,V)>(count)}
 fn before_command_catalog(&mut self,v:&crate::modules::ModuleRegistry)->Result<(),Error>{self.0.actual_value(v)}
}
