//! Adapts the unique original State field emitter, never whole-State serde.
use super::*;
use serde::ser::SerializeStruct;
use std::cell::RefCell;
use oasis7_world_state::{OriginalStateFieldSlotV1,OriginalStateFieldSlotEventV1,OriginalStateFieldEmissionV1};
pub(in crate::world) struct ProjectionOriginalStateSlotDataV1{definition:OriginalStateFieldSlotV1,emission_index:Option<usize>}
impl ProjectionOriginalStateSlotDataV1{
 pub(in crate::world) fn definition(&self)->OriginalStateFieldSlotV1{self.definition}
 pub(in crate::world) fn emission_index(&self)->Option<usize>{self.emission_index}
}
pub(in crate::world) struct ProjectionOriginalStateEmissionV1{name:&'static str,json:ReservedValue<Vec<u8>>,value:ProjectionColumnDataV1,_fee:MemoryReservation}
impl ProjectionOriginalStateEmissionV1{
 pub(in crate::world) fn name(&self)->&'static str{self.name}
 pub(in crate::world) fn original_json(&self)->&[u8]{&self.json}
 pub(in crate::world) fn value(&self)->&ProjectionColumnDataV1{&self.value}
}
impl NormalProjectionColumnSeedDataV1{
 pub(in crate::world) fn original_state_slots(&self)->&[ProjectionOriginalStateSlotDataV1]{&self.state_slots}
 pub(in crate::world) fn original_state_emissions(&self)->&[ProjectionOriginalStateEmissionV1]{&self.emitted}
 pub(in crate::world) fn original_state_declared_fields(&self)->Option<usize>{self.emitted_count}
 /// GEN/cold readonly graph assembled from the actual original emitter.
 /// This is DATA only; the enclosing actual-source pair supplies qualification.
 pub(in crate::world) fn original_state_columns_data_v1(&self,common:&mut FormalBoundedContextV1<'_>)->Result<ProjectionColumnDataV1,BodyMutationError>{
  self.validate_context(common)?;if self.emitted_count.is_none(){return Err(invalid("original State emitter was not captured"))}let memory=common.memory().clone();
  let mut value=ProjectionColumnDataV1::from_json_data(serde_json::Value::Object(Default::default()),&memory,&mut|s,b|common.charge(s,b)).map_err(|e|match e{ProjectionColumnErrorV1::Work(e)=>paid(e),ProjectionColumnErrorV1::Memory(e)=>ValueTreeError::Memory(e).into(),ProjectionColumnErrorV1::Invalid(e)=>invalid(e)})?;
  for field in &self.emitted{value=value.with_object_member_data(field.name(),Some(field.value()),&memory,&mut|s,b|common.charge(s,b)).map_err(|e|match e{ProjectionColumnErrorV1::Work(e)=>paid(e),ProjectionColumnErrorV1::Memory(e)=>ValueTreeError::Memory(e).into(),ProjectionColumnErrorV1::Invalid(e)=>invalid(e)})?}Ok(value)
 }
}
#[derive(Debug)]enum EmissionError{Source(BodyMutationError),Codec(serde_json::Error)}
impl std::fmt::Display for EmissionError{fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result{match self{Self::Source(e)=>write!(f,"{e:?}"),Self::Codec(e)=>std::fmt::Display::fmt(e,f)}}}
impl std::error::Error for EmissionError{}
impl serde::ser::Error for EmissionError{fn custom<T:std::fmt::Display>(note:T)->Self{Self::Codec(<serde_json::Error as serde::ser::Error>::custom(note))}}
struct Emitter<'a,'b,'c,'d>{initializer:&'a RefCell<&'b mut NormalProjectionColumnInitializerV1<'c,'d>>}
impl SerializeStruct for Emitter<'_,'_,'_,'_>{
 type Ok=();type Error=EmissionError;
 fn serialize_field<T:serde::Serialize+?Sized>(&mut self,key:&'static str,value:&T)->Result<(),Self::Error>{self.initializer.borrow_mut().capture_emitted_field(key,value).map_err(EmissionError::Source)}
 fn end(self)->Result<(),Self::Error>{Ok(())}
}
#[derive(Debug)]struct RetainedEmissionCodecError{error:serde_json::Error,_fee:MemoryReservation}
impl std::fmt::Display for RetainedEmissionCodecError{fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result{std::fmt::Display::fmt(&self.error,f)}}
impl std::error::Error for RetainedEmissionCodecError{fn source(&self)->Option<&(dyn std::error::Error+'static)>{Some(&self.error)}}
impl NormalProjectionColumnInitializerV1<'_,'_>{
 /// Only actual immutable GEN/cold State may be supplied by the original
 /// initializer. Each callback serializes one original typed column; this
 /// method never calls Serialize/to_value/to_vec on the enclosing State.
 pub(in crate::world) fn capture_original_state_fields_v1(&mut self,state:&oasis7_world_state::WorldState)->Result<(),BodyMutationError>{
  if self.emitted_count.is_some(){return Err(invalid("duplicate original State emission"))}
  // Base-state emission's control errors are closed serializer messages;
  // retain their admitted phase on failure. Typed field/IO/Common errors
  // already use capture's first-cause lane and are returned unchanged.
  let phase=reserve(self.common,65536+size_of::<RetainedEmissionCodecError>())?;
  let shared=RefCell::new(self);
  let result=state.visit_original_serialized_fields_with_slots_v1(|count|{shared.borrow_mut().emitted_count=Some(count);Ok(Emitter{initializer:&shared})},&mut|event|shared.borrow_mut().capture_original_slot(event).map_err(EmissionError::Source));
  match result{Ok(())=>(),Err(EmissionError::Source(error))=>return Err(error),Err(EmissionError::Codec(error))=>return Err(paid(RetainedEmissionCodecError{error,_fee:phase}))}
  let initializer=shared.into_inner();initializer.capture_dedicated_parent_columns_v1(state)?;
  initializer.marketplace_index=Some(std::sync::Arc::new(super::super::market_index::ProjectionMarketplaceIndexSeedDataV1::from_genesis_actual_v1(state,initializer.common)?));
  initializer.world_rule_rows=match state.world_rules.as_ref(){Some(rules)=>Some(std::sync::Arc::new(super::super::world_rule_rows::OriginalWorldRuleRowsDataV1::from_actual_v1(rules,initializer.common)?)),None=>None};Ok(())
 }
 fn capture_original_slot(&mut self,event:OriginalStateFieldSlotEventV1)->Result<(),BodyMutationError>{
  let name=match event{OriginalStateFieldSlotEventV1::Declared(definition)=>definition.name,OriginalStateFieldSlotEventV1::Emitted(name)=>name};
  let mut found=None;for(index,slot)in self.state_slots.iter().enumerate(){self.common.charge(1,(name.len()+slot.definition.name.len())as u64).map_err(paid)?;if slot.definition.name==name{found=Some(index);break}}
  if let Some(index)=found{let slot=&mut self.state_slots[index];match event{OriginalStateFieldSlotEventV1::Declared(definition)=>{if slot.definition!=definition{return Err(invalid("original State slot policy changed"))}},OriginalStateFieldSlotEventV1::Emitted(_)=>{if slot.emission_index.is_some(){return Err(invalid("duplicate original State emission"))}slot.emission_index=Some(self.emitted.len())}}return Ok(())}
  grow(&mut self.state_slots,&mut self.state_slot_capacity,self.common)?;let(definition,emission_index)=match event{OriginalStateFieldSlotEventV1::Declared(definition)=>(definition,None),OriginalStateFieldSlotEventV1::Emitted(name)=>(OriginalStateFieldSlotV1{name,emission:OriginalStateFieldEmissionV1::Always},Some(self.emitted.len()))};self.state_slots.push(ProjectionOriginalStateSlotDataV1{definition,emission_index});Ok(())
 }
 fn capture_emitted_field<T:serde::Serialize+?Sized>(&mut self,name:&'static str,value:&T)->Result<(),BodyMutationError>{
  self.common.consume_steps(1).map_err(paid)?;
  let json=capture(self.common,|out|serde_json::to_writer(out,value).map_err(paid))?;
  let n=json.len().checked_mul(1024).and_then(|n|n.checked_add(32768)).ok_or_else(||invalid("projection emitted field graph layout"))?;let _phase=reserve(self.common,n)?;self.common.charge(json.len()as u64,json.len()as u64).map_err(paid)?;
  let decoded:serde_json::Value=serde_json::from_slice(&json).map_err(paid)?;self.common.consume_steps(0).map_err(paid)?;let memory=self.common.memory().clone();
  let value=ProjectionColumnDataV1::from_json_data(decoded,&memory,&mut|s,b|self.common.charge(s,b)).map_err(|e|match e{ProjectionColumnErrorV1::Work(e)=>paid(e),ProjectionColumnErrorV1::Memory(e)=>ValueTreeError::Memory(e).into(),ProjectionColumnErrorV1::Invalid(e)=>invalid(e)})?;
  let fee=reserve(self.common,size_of::<ProjectionOriginalStateEmissionV1>())?;grow(&mut self.emitted,&mut self.emitted_slots,self.common)?;self.emitted.push(ProjectionOriginalStateEmissionV1{name,json,value,_fee:fee});Ok(())
 }
}

#[path="incremental_normal_projection_commitment_initial_cold_emission.rs"] mod qualified;
