//! Paid original JSON value construction for the selected module transaction.
//! The first pass observes the original serializer only to admit resources; it
//! never decodes a DTO, checks grammar, hashes a root, or grants permission.
use super::*;
use std::io::Write;
pub(super) enum ModuleGovernanceJsonErrorV1{Body(BodyMutationError),Json{error:serde_json::Error,_fee:MemoryReservation}}
pub(super) struct PaidModuleJsonValueV1{pub(super) value:serde_json::Value,pub(super) fee:MemoryReservation}
#[derive(Default)]
struct Counts{wire:usize,nodes:usize,entries:usize,quoted:bool,escaped:bool,token:bool}
impl Counts{
 fn observe(&mut self,bytes:&[u8])->Result<(),BodyMutationError>{
  self.wire=self.wire.checked_add(bytes.len()).ok_or_else(invalid)?;
  for b in bytes {
   if self.quoted {
    if self.escaped{self.escaped=false}else if *b==b'\\'{self.escaped=true}else if *b==b'"'{self.quoted=false}
    continue;
   }
   match *b {
    b'"'=>{self.quoted=true;self.token=false;self.nodes=self.nodes.checked_add(1).ok_or_else(invalid)?},
    b'{'|b'['=>{self.token=false;self.nodes=self.nodes.checked_add(1).ok_or_else(invalid)?},
    b':'=>{self.token=false;self.entries=self.entries.checked_add(1).ok_or_else(invalid)?},
    b','|b'}'|b']'|b' '|b'\n'|b'\r'|b'\t'=>self.token=false,
    _=>if !self.token{self.token=true;self.nodes=self.nodes.checked_add(1).ok_or_else(invalid)?},
   }
  }Ok(())
 }
 fn output_heap(&self)->Result<usize,BodyMutationError>{
  // Strings never need more UTF8 bytes than their escaped JSON wire. Actual
  // structural node/entry counts, not the wire length, bound container slots.
  let arrays=self.nodes.checked_mul(std::mem::size_of::<serde_json::Value>()).and_then(|n|n.checked_mul(4)).ok_or_else(invalid)?;
  let maps=oasis7_world_state::incremental_collection_layout::json_object_output_heap_v1(self.entries)?;
  self.wire.checked_mul(2).and_then(|n|n.checked_add(arrays)).and_then(|n|n.checked_add(maps)).and_then(|n|n.checked_add(8192)).ok_or_else(invalid)
 }
}
struct Counter<'a,'b>{common:&'a mut FormalBoundedContextV1<'b>,counts:Counts,error:Option<BodyMutationError>}
impl Write for Counter<'_,'_>{
 fn write(&mut self,bytes:&[u8])->std::io::Result<usize>{
  if self.error.is_some(){return Err(std::io::Error::other("module JSON meter poisoned"))}
  for chunk in bytes.chunks(8192){
   let result=pay(self.common,chunk.len()).and_then(|_|self.counts.observe(chunk));
   if let Err(e)=result{self.error=Some(e);return Err(std::io::Error::other("module JSON meter"))}
  }Ok(bytes.len())
 }
 fn flush(&mut self)->std::io::Result<()>{if self.error.is_some(){Err(std::io::Error::other("module JSON meter poisoned"))}else{Ok(())}}
}
pub(super) fn module_json_value_paid_v1<T:serde::Serialize+?Sized>(source:&T,common:&mut FormalBoundedContextV1<'_>)->Result<PaidModuleJsonValueV1,ModuleGovernanceJsonErrorV1>{
 let error_fee=common.memory().reserve(8192,MemoryClass::Ordinary).map_err(ValueTreeError::Memory).map_err(|e|ModuleGovernanceJsonErrorV1::Body(e.into()))?;
 let mut counter=Counter{common,counts:Counts::default(),error:None};
 let encoded=serde_json::to_writer(&mut counter,source);
 if let Some(first)=counter.error.take(){return Err(ModuleGovernanceJsonErrorV1::Body(first))}
 if let Err(error)=encoded{return Err(ModuleGovernanceJsonErrorV1::Json{error,_fee:error_fee})}
 let heap=counter.counts.output_heap().map_err(ModuleGovernanceJsonErrorV1::Body)?;
 // The original to_value repeats the actual encoder walk; account that second
 // traversal before it can allocate the now-admitted Value graph.
 counter.common.charge(u64::try_from(counter.counts.nodes).map_err(|_|ModuleGovernanceJsonErrorV1::Body(invalid()))?,u64::try_from(counter.counts.wire).map_err(|_|ModuleGovernanceJsonErrorV1::Body(invalid()))?).map_err(|e|ModuleGovernanceJsonErrorV1::Body(BodyMutationError::Original(Box::new(e))))?;
 let fee=counter.common.memory().reserve(heap,MemoryClass::Ordinary).map_err(ValueTreeError::Memory).map_err(|e|ModuleGovernanceJsonErrorV1::Body(e.into()))?;
 match serde_json::to_value(source){Ok(value)=>Ok(PaidModuleJsonValueV1{value,fee}),Err(error)=>Err(ModuleGovernanceJsonErrorV1::Json{error,_fee:error_fee})}
}
