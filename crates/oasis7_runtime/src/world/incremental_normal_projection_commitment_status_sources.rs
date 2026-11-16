//! Fixed Current rows and the actual reverse history suffix. Only data reads,
//! fee admission and the continuing canonical loan cross this source boundary.
use super::*;
use std::{cell::{RefCell,RefMut},ops::ControlFlow};
use oasis7_wasm_abi::{CanonicalCborAllocator,CanonicalCborStreamError as Error,CapabilityCanonicalWorkV1};
use oasis7_world_projection::current_commitment::{ProjectionColumnRefV1,ProjectionColumnDataV1,ProjectionColumnErrorV1,ProjectionDataPathRefV1 as Path};
use crate::world::industrial_control::readonly_runtime_owner::{RuntimeOwnerReadonlyFactsV1,current_agent_identity_readonly_v1,current_agent_owner_readonly_v1,owner_public_key_readonly_v1,world_rule_binding_readonly_v1};
pub(super)struct CurrentStatus<'v,'f,'a,'s,'c,'k,M>{view:&'v values::View<'f,'a,'s,'c,M>,canonical:RefCell<&'k mut oasis7_wasm_abi::CanonicalCborWorkV1>,first:RefCell<Option<BodyMutationError>>}
fn graph(e:ProjectionColumnErrorV1<BodyMutationError>)->BodyMutationError{match e{ProjectionColumnErrorV1::Work(e)=>e,ProjectionColumnErrorV1::Memory(e)=>ValueTreeError::Memory(e).into(),ProjectionColumnErrorV1::Invalid(e)=>invalid(e)}}
impl<'v,'f,'a,'s,'c,'k,M:MerkleNodeReader>CurrentStatus<'v,'f,'a,'s,'c,'k,M>{
 pub(super)fn new(view:&'v values::View<'f,'a,'s,'c,M>,canonical:&'k mut oasis7_wasm_abi::CanonicalCborWorkV1)->Self{Self{view,canonical:RefCell::new(canonical),first:RefCell::new(None)}}
 pub(super)fn finish(&self)->Result<(),BodyMutationError>{if let Some(error)=self.first.borrow_mut().take(){return Err(error)}self.view.facts.check()}
 fn share(&self,row:&ProjectionColumnRefV1<'_>)->Result<ProjectionColumnDataV1,BodyMutationError>{let memory=self.view.facts.common.borrow().memory().clone();row.share_data(&memory,&mut|s,b|self.view.facts.common.borrow_mut().charge(s,b).map_err(paid)).map_err(graph)}
 fn length(&self,row:&ProjectionColumnDataV1)->Result<usize,BodyMutationError>{let memory=self.view.facts.common.borrow().memory().clone();row.array_len_data_v1(&memory,&mut|s,b|self.view.facts.common.borrow_mut().charge(s,b).map_err(paid)).map_err(graph)}
 fn index(&self,row:&ProjectionColumnDataV1,index:usize)->Result<ProjectionColumnDataV1,BodyMutationError>{let memory=self.view.facts.common.borrow().memory().clone();row.select_path_data_v1(&[Path::Index(index)],&memory,&mut|s,b|self.view.facts.common.borrow_mut().charge(s,b).map_err(paid)).map_err(graph)?.ok_or_else(||invalid("status actual history ordinal missing"))}
 fn visit_cognition_tail(&self,field:&str,mut visit:impl FnMut(&Value)->Result<ControlFlow<()>,BodyMutationError>)->Result<(),BodyMutationError>{
  let Some(row)=self.view.optional(&["cognition",field])?else{return Ok(())};let rows=self.share(&row)?;
  // Original Value::as_array defaults to no records for a non-array field.
  if rows.root_kind()!="array"{return Ok(())}
  let length=self.length(&rows)?;
  for offset in 0..length.min(2048){let selected=self.index(&rows,length-1-offset)?;let value=match self.view.decode::<Value>(&selected.root_data())?{Ok(value)=>value,Err(_)=>return Err(invalid("actual cognition history JSON graph"))};if visit(&value)?.is_break(){break}}
  Ok(())
 }
}
impl<M>results::sealed::Sources for CurrentStatus<'_, '_, '_, '_, '_, '_,M>{}
impl<M:MerkleNodeReader>results::SourcesV1 for CurrentStatus<'_, '_, '_, '_, '_, '_,M>{
 type Error=BodyMutationError;
 type Event<'x>=ReservedValue<crate::WorldEvent>where Self:'x;
 fn identity(&self,id:&str)->Result<Option<crate::CapabilityAgentIdentity>,Self::Error>{current_agent_identity_readonly_v1(&self.view.owner_source()?,id)}
 fn public_key(&self,id:&str)->Result<Option<String>,Self::Error>{Ok(owner_public_key_readonly_v1(&self.view.owner_source()?,id)?.ok())}
 fn manifest(&self)->Result<results::ManifestDataV1,Self::Error>{self.view.owner_source()?.runtime_manifest(|world,_,genesis|Ok(results::ManifestDataV1{world_id:self.view.copy(world)?,genesis_ref:genesis.map(|v|self.view.copy(v)).transpose()?}))}
 fn binding(&self)->Result<Option<String>,Self::Error>{world_rule_binding_readonly_v1(&self.view.owner_source()?)}
 fn events(&self)->Result<Vec<Self::Event<'_>>,Self::Error>{
  let row=self.view.required(&["world_journal"])?;let journal=self.share(&row)?;let length=self.length(&journal)?;let count=length.min(2048);
  let bytes=count.checked_mul(size_of::<ReservedValue<crate::WorldEvent>>()).ok_or_else(||invalid("status event window layout"))?;self.view.facts.work(1,bytes)?;self.view.facts.before_owned(bytes)?;
  let mut events=Vec::with_capacity(count);if events.capacity()!=count{return Err(invalid("status event window capacity"))}
  for offset in 0..count{let selected=self.index(&journal,length-1-offset)?;let event=match self.view.decode::<crate::WorldEvent>(&selected.root_data())?{Ok(event)=>event,Err(_)=>return Err(invalid("actual status WorldEvent original JSON"))};self.view.facts.work(1,size_of::<crate::WorldEvent>())?;let fee=self.view.facts.common.borrow().memory().reserve(size_of::<crate::WorldEvent>(),MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;events.push(fee.retain(event));}
  Ok(events)
 }
 fn visit_commits(&self,visit:impl FnMut(&Value)->Result<ControlFlow<()>,Self::Error>)->Result<(),Self::Error>{self.visit_cognition_tail("commit_records",visit)}
 fn visit_receipts(&self,visit:impl FnMut(&Value)->Result<ControlFlow<()>,Self::Error>)->Result<(),Self::Error>{self.visit_cognition_tail("receipt_lineage_registry",visit)}
 fn with_action_digest<T>(&self,id:&str,visit:impl FnOnce(&Value)->Result<T,Self::Error>)->Result<T,Self::Error>{let Some(rows)=self.view.optional(&["cognition","action_digests"])?else{return visit(&Value::Null)};let Some(row)=self.view.member_like_json_get(&rows,id)?else{return visit(&Value::Null)};let value=match self.view.decode::<Value>(&row)?{Ok(value)=>value,Err(_)=>return Err(invalid("actual action digest JSON graph"))};visit(&value)}
 fn before_value<T:serde::Serialize+?Sized>(&self,v:&T)->Result<(),Self::Error>{self.view.before_clone(v)}
 fn work(&self,s:u64,b:usize)->Result<(),Self::Error>{self.finish()?;self.view.facts.work(s,b)}
 fn ownership(&self,n:usize)->Result<(),Self::Error>{self.view.facts.before_owned(n)}
 fn check(&self)->Result<(),Self::Error>{self.finish()}
 fn stream(&self,error:Error)->Self::Error{paid(error)}
}
impl<'v,'f,'a,'s,'c,'k,M:MerkleNodeReader>status_readonly::SourcesV1 for CurrentStatus<'v,'f,'a,'s,'c,'k,M>{
 type State=CurrentProjectionReadFactsV1<'a,'s,'c,M>;
 fn state_facts(&self)->&Self::State{self.view.facts}
 fn current_owner(&self,id:&str)->Result<Option<crate::ResolvedAgentOwnerV1>,BodyMutationError>{current_agent_owner_readonly_v1(&self.view.owner_source()?,id)}
 fn claim<T>(&self,id:&str,visit:impl FnOnce(Option<&oasis7_world_state::AgentClaimState>)->Result<T,BodyMutationError>)->Result<T,BodyMutationError>{self.view.facts.owner_with_claim_data(id,visit)}
}
impl<M:MerkleNodeReader>CanonicalCborAllocator for CurrentStatus<'_, '_, '_, '_, '_, '_,M>{type Charge=MemoryReservation;
 fn charge_work(&self,s:u64,b:u64)->Result<(),Error>{if self.first.borrow().is_some(){return Err(Error::Poisoned)}self.view.facts.common.borrow_mut().charge(s,b).map_err(|e|{*self.first.borrow_mut()=Some(paid(e));Error::Poisoned})}
 fn reserve(&self,n:usize)->Result<MemoryReservation,Error>{if self.first.borrow().is_some(){return Err(Error::Poisoned)}self.view.facts.common.borrow().memory().reserve(n,MemoryClass::Ordinary).map_err(|e|{*self.first.borrow_mut()=Some(ValueTreeError::Memory(e).into());Error::Poisoned})}
}
impl<M:MerkleNodeReader>CapabilityCanonicalWorkV1 for CurrentStatus<'_, '_, '_, '_, '_, '_,M>{type Loan<'x>=RefMut<'x,oasis7_wasm_abi::CanonicalCborWorkV1>where Self:'x;
 fn is_metered(&self)->bool{true}
 fn loan_canonical_work(&self)->Self::Loan<'_>{RefMut::map(self.canonical.borrow_mut(),|work|&mut **work)}
 fn record_stream_error(&self,error:Error)->Error{if self.first.borrow().is_none(){*self.first.borrow_mut()=Some(paid(error))}Error::Poisoned}
}
