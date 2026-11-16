//! Original selected-field JSON and complete typed Cognition codecs. This
//! builds no World/WorldState and invents no missing record qualification.
use super::*;
use serde::{Serialize,de::DeserializeOwned};
use crate::world::cognition_economy::{CognitionEconomyStateV1,bounded_work::CurrentEconomyWorkV1};
fn graph(error:ProjectionColumnErrorV1<BodyMutationError>)->BodyMutationError{match error{ProjectionColumnErrorV1::Work(e)=>e,ProjectionColumnErrorV1::Memory(e)=>ValueTreeError::Memory(e).into(),ProjectionColumnErrorV1::Invalid(e)=>invalid(e)}}
pub(super) struct View<'f,'a,'s,'c,M>{pub(super) facts:&'f CurrentProjectionReadFactsV1<'a,'s,'c,M>,aux:&'f ProjectionColumnDataV1}
impl<'f,'a,'s,'c,M:MerkleNodeReader> View<'f,'a,'s,'c,M>{
 pub(super) fn new(facts:&'f CurrentProjectionReadFactsV1<'a,'s,'c,M>,aux:&'f ProjectionColumnDataV1)->Result<Self,BodyMutationError>{facts.check()?;if !aux.uses_budget(facts.common.borrow().memory()){return Err(invalid("Cognition observation auxiliary pool"))}Ok(Self{facts,aux})}
 pub(super) fn optional(&self,path:&[&str])->Result<Option<ProjectionColumnRefV1<'f>>,BodyMutationError>{self.facts.check()?;let mut row=self.aux.root_data();for name in path{match self.member_like_json_get(&row,name)?{Some(next)=>row=next,None=>return Ok(None)}}Ok(Some(row))}
 pub(super) fn required(&self,path:&[&str])->Result<ProjectionColumnRefV1<'f>,BodyMutationError>{self.optional(path)?.ok_or_else(||invalid("Cognition observation required original field missing"))}
 pub(super) fn member_like_json_get<'v>(&self,parent:&ProjectionColumnRefV1<'v>,name:&str)->Result<Option<ProjectionColumnRefV1<'v>>,BodyMutationError>{let memory=self.facts.common.borrow().memory().clone();let mut work=|s,b|self.facts.common.borrow_mut().charge(s,b).map_err(paid);let shared=parent.share_data(&memory,&mut work).map_err(graph)?;if shared.root_kind()!="object"{return Ok(None)}parent.object_member_data(name,&memory,&mut work).map_err(graph)}
 pub(super) fn is_null(&self,row:&ProjectionColumnRefV1<'_>)->Result<bool,BodyMutationError>{let memory=self.facts.common.borrow().memory().clone();Ok(row.share_data(&memory,&mut|s,b|self.facts.common.borrow_mut().charge(s,b).map_err(paid)).map_err(graph)?.root_kind()=="null")}
 pub(super) fn raw(&self,row:&ProjectionColumnRefV1<'_>)->Result<ReservedValue<Vec<u8>>,BodyMutationError>{shared_json::capture_shared_v1(row,&self.facts.common,&RefCell::new(None),usize::MAX)}
 pub(super) fn before_clone<T:Serialize+?Sized>(&self,value:&T)->Result<(),BodyMutationError>{let raw={let mut common=self.facts.common.borrow_mut();crate::world::incremental_normal_projection_commitment::capture_projection_json_data_v1(value,&mut common)?};let n=raw.len().checked_mul(2048).and_then(|n|n.checked_add(65536)).ok_or_else(||invalid("Cognition observation clone layout"))?;self.facts.work(raw.len()as u64,n)?;self.facts.before_owned(n)}
 pub(super) fn decode<T:DeserializeOwned>(&self,row:&ProjectionColumnRefV1<'_>)->Result<Result<T,serde_json::Error>,BodyMutationError>{
  let raw=self.raw(row)?;let n=raw.len().checked_mul(2048).and_then(|n|n.checked_add(65536+size_of::<T>())).ok_or_else(||invalid("Cognition observation selected DTO layout"))?;
  self.facts.before_owned(n)?;self.facts.work(raw.len()as u64*16,raw.len())?;
  let value=serde_json::to_value(row).map_err(paid)?;
  self.facts.check()?;
  // Preserve from_value's exact legacy errors/default semantics. The input
  // is the actual original typed JSON graph, not a parallel wire parser.
  let result=serde_json::from_value(value);self.facts.check()?;Ok(result)
 }
 pub(super) fn copy(&self,text:&str)->Result<String,BodyMutationError>{self.facts.before_owned(size_of::<String>()+text.len())?;let mut bytes=Vec::with_capacity(text.len());if bytes.capacity()!=text.len(){return Err(invalid("Cognition observation text capacity"))}for part in text.as_bytes().chunks(8192){self.facts.work(1,part.len())?;bytes.extend_from_slice(part)}Ok(String::from_utf8(bytes).expect("actual original UTF-8"))}
 pub(super) fn owner_source(&self)->Result<super::super::aux_facts::CurrentProjectionAuxFactsV1<'_, 'a,'s,'c,M>,BodyMutationError>{super::super::aux_facts::CurrentProjectionAuxFactsV1::new(self.facts,self.aux)}
 pub(super) fn typed<T:DeserializeOwned>(&self,path:&[&str])->Result<T,BodyMutationError>{let row=self.required(path)?;self.decode(&row)?.map_err(|_|invalid("Cognition provider actual typed column"))}
 pub(super) fn decode_original<T:DeserializeOwned>(&self,raw:&[u8])->Result<T,BodyMutationError>{let n=raw.len().checked_mul(2048).and_then(|n|n.checked_add(65536+size_of::<T>())).ok_or_else(||invalid("Cognition provider selected native graph"))?;self.facts.before_owned(n)?;self.facts.work(raw.len()as u64*32,raw.len())?;serde_json::from_slice(raw).map_err(|_|invalid("Cognition provider actual native codec"))}
 pub(super) fn format(&self,args:std::fmt::Arguments<'_>)->Result<String,BodyMutationError>{oasis7_world_state::world_rules::format_readonly_projection_data_v1(args,&mut|s,b,h|{self.facts.work(s,b)?;if h>0{self.facts.before_owned(h)?}Ok(())})}
 pub(super) fn business(&self,args:std::fmt::Arguments<'_>)->BodyMutationError{
  #[derive(Debug)]struct Note{message:String,_fee:MemoryReservation}
  impl std::fmt::Display for Note{fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result{f.write_str(&self.message)}}impl std::error::Error for Note{}
  let mut fee:Option<MemoryReservation>=None;
  let result=oasis7_world_state::world_rules::format_readonly_projection_data_v1(args,&mut|steps,bytes,heap|{
   self.facts.work(steps,bytes)?;
   if heap>0{let mut next=self.facts.common.borrow().memory().reserve(heap,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;match &mut fee{Some(fee)=>fee.merge(&mut next).map_err(invalid)?,None=>fee=Some(next)}}Ok(())
  });
  let message=match result{Ok(message)=>message,Err(error)=>return error};
  if let Err(error)=self.facts.work(1,size_of::<Note>()){return error}
  let mut outer=match self.facts.common.borrow().memory().reserve(size_of::<Note>(),MemoryClass::Ordinary){Ok(fee)=>fee,Err(error)=>return ValueTreeError::Memory(error).into()};
  if let Some(mut graph)=fee{if let Err(error)=outer.merge(&mut graph){return invalid(error)}}
  paid(Note{message,_fee:outer})
 }
}
pub(super) fn push<M:MerkleNodeReader,T>(facts:&CurrentProjectionReadFactsV1<'_, '_, '_,M>,out:&mut Vec<T>,value:T)->Result<(),BodyMutationError>{if out.len()==out.capacity(){let cap=out.capacity().checked_mul(2).unwrap_or(usize::MAX).max(4);let bytes=cap.checked_mul(size_of::<T>()).ok_or_else(||invalid("Cognition observation Vec layout"))?;facts.before_owned(bytes)?;out.try_reserve_exact(cap-out.len()).map_err(|_|invalid("Cognition observation Vec allocation"))?;if out.capacity()>cap{return Err(invalid("Cognition observation Vec capacity"))}}out.push(value);Ok(())}
pub(super) fn continuations<M:MerkleNodeReader>(view:&View<'_, '_, '_, '_,M>)->Result<Vec<AgentContinuation>,BodyMutationError>{
 match view.optional(&["cognition","continuations"])?{None=>Ok(Vec::new()),Some(row)=>match view.decode(&row)?{Ok(value)=>Ok(value),Err(error)=>Err(view.business(format_args!("{error}")))}}
}
pub(super) fn economy<M:MerkleNodeReader>(view:&View<'_, '_, '_, '_,M>,canonical:&mut oasis7_wasm_abi::CanonicalCborWorkV1)->Result<(CognitionEconomyStateV1,crate::world::cognition_economy::bounded_work::CurrentEconomyWorkFeesV1),BodyMutationError>{
 let row=view.optional(&["cognition","cognition_economy"])?;
 // Capture the original JSON before borrowing the canonical source. The
 // fixed decoder below records absence separately from an explicit String;
 // no legacy serde default can hash outside this continuing source.
 let input=match row{None=>None,Some(row)=>Some(match view.decode::<serde_json::Value>(&row)?{Ok(value)=>value,Err(_)=>return Err(invalid("actual economy JSON column"))})};
 view.facts.work(1,size_of::<CognitionEconomyStateV1>()+128)?;
 view.facts.before_owned(size_of::<CognitionEconomyStateV1>()+128)?;
 let mut common=view.facts.common.borrow_mut();
 let mut ops=CurrentEconomyWorkV1::new(&mut common,canonical)?;
 let result:Result<CognitionEconomyStateV1,crate::world::cognition_economy::CognitionEconomyError>=(||{
  let state=match input{Some(value)=>CognitionEconomyStateV1::decode_snapshot_value_with_work_v1(value,&mut ops)?,None=>CognitionEconomyStateV1::default_with_work_v1(&mut ops)?};
  state.validate_with_work_v1(&mut ops)?;
  Ok(state)
 })();
 // The original typed infrastructure cause beats an EconomyError sentinel,
 // including a failure inside the formerly hidden fixed-default digest.
 let fees=ops.finish()?;drop(common);
 match result{Ok(state)=>Ok((state,fees)),Err(error)=>{let error=crate::WorldError::DistributedValidationFailed{reason:view.copy(error.code())?};Err(view.business(format_args!("{error:?}")))}}

}
