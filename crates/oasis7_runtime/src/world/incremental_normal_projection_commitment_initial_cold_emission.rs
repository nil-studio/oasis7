//! Cold schema comes from the original emitter. Its default values are never
//! accepted as source data: only declaration/order is observed.
use super::*;
struct Schema;
impl SerializeStruct for Schema{
 type Ok=();type Error=EmissionError;
 fn serialize_field<T:serde::Serialize+?Sized>(&mut self,_:&'static str,_:&T)->Result<(),Self::Error>{Ok(())}
 fn end(self)->Result<(),Self::Error>{Ok(())}
}
impl NormalProjectionColumnInitializerV1<'_, '_>{
 pub(in crate::world) fn capture_qualified_original_schema_v1(&mut self,retained_json:&[u8])->Result<(),BodyMutationError>{
 if self.emitted_count.is_some()||!self.state_slots.is_empty(){return Err(invalid("duplicate qualified original schema"))}
 self.common.charge(256,1024*1024).map_err(paid)?;let _phase=reserve(self.common,1024*1024)?;let state=oasis7_world_state::WorldState::default();
 let this=RefCell::new(&mut*self);let result=state.visit_original_serialized_fields_with_slots_v1(|_|Ok(Schema),&mut|event|{let mut this=this.borrow_mut();this.common.consume_steps(1).map_err(paid).map_err(EmissionError::Source)?;this.capture_original_slot(event).map_err(EmissionError::Source)});
 match result{Ok(())=>(),Err(EmissionError::Source(e))=>return Err(e),Err(EmissionError::Codec(e))=>return Err(paid(RetainedEmissionCodecError{error:e,_fee:_phase}))}drop(this);drop(state);
 for slot in &mut self.state_slots{self.common.consume_steps(1).map_err(paid)?;slot.emission_index=None;}
 // Only actual unit/value columns may populate scalar fragments. All map,
 // set and sequence values are subsequently reconstructed from qualified rows.
 struct Scalar{name:&'static str,json:ReservedValue<Vec<u8>>}
 let count=self.fields.len();let _slots=reserve(self.common,count.checked_mul(size_of::<Scalar>()).ok_or_else(||invalid("qualified scalar slots"))?)?;let mut scalars=Vec::with_capacity(count);if scalars.capacity()!=count{return Err(invalid("qualified scalar capacity"))}
 for field in self.fields.values(){self.common.consume_steps(1).map_err(paid)?;if field.descriptor.owner!=FieldOwner::WorldState||field.descriptor.shape!=FieldShape::Value||field.descriptor.name=="world_rules"{continue}if field.rows.len()!=1{return Err(invalid("qualified original scalar cardinality"))}let source=field.rows[0].original_json();let json=capture(self.common,|out|std::io::Write::write_all(out,source).map_err(paid))?;scalars.push(Scalar{name:field.descriptor.name,json});}
 for scalar in scalars{self.capture_qualified_encoded_emission_v1(scalar.name,&scalar.json)?}
 let mut retained_field=None;for field in oasis7_world_state::incremental_projection::WORLD_STATE_FIELDS.iter().copied(){self.common.charge(1,(field.name.len()+21)as u64).map_err(paid)?;if field.owner==FieldOwner::WorldState&&field.name=="retained_history_root"{retained_field=Some(field)}}let retained_field=retained_field.ok_or_else(||invalid("qualified retained side descriptor"))?;
 let retained_phase=reserve(self.common,retained_json.len().checked_add(32768).ok_or_else(||invalid("qualified side codec"))?)?;self.common.charge(retained_json.len()as u64,retained_json.len()as u64).map_err(paid)?;let retained=serde_json::from_slice::<&serde_json::value::RawValue>(retained_json);self.common.consume_steps(0).map_err(paid)?;let retained=match retained{Ok(v)=>v,Err(error)=>return Err(paid(RetainedEmissionCodecError{error,_fee:retained_phase}))};self.side(retained_field,retained,None)?;
 self.capture_qualified_encoded_emission_v1("retained_history_root",retained_json)?;
 self.emitted_count=Some(self.emitted.len());Ok(())
 }
 fn capture_qualified_encoded_emission_v1(&mut self,name:&'static str,json:&[u8])->Result<(),BodyMutationError>{
 let phase=reserve(self.common,json.len().checked_add(32768).ok_or_else(||invalid("qualified scalar codec phase"))?)?;self.common.charge(json.len()as u64,json.len()as u64).map_err(paid)?;let raw=serde_json::from_slice::<&serde_json::value::RawValue>(json);self.common.consume_steps(0).map_err(paid)?;let raw=match raw{Ok(v)=>v,Err(error)=>return Err(paid(RetainedEmissionCodecError{error,_fee:phase}))};
 self.common.consume_steps(self.state_slots.len()as u64).map_err(paid)?;let index=self.state_slots.iter().position(|slot|slot.definition.name==name).ok_or_else(||invalid("qualified scalar original slot"))?;
 let emits=match self.state_slots[index].definition.emission{OriginalStateFieldEmissionV1::Always=>true,OriginalStateFieldEmissionV1::Some=>raw.get()!="null",_=>return Err(invalid("qualified scalar emission profile"))};
 if emits{self.state_slots[index].emission_index=Some(self.emitted.len());self.capture_emitted_field(name,raw)?}Ok(())
 }
}
impl NormalProjectionColumnSeedDataV1{
 pub(in crate::world) fn begin_qualified_emission_rebuild_v1(&mut self,common:&mut FormalBoundedContextV1<'_>)->Result<(),BodyMutationError>{self.validate_context(common)?;self.emitted.clear();for slot in &mut self.state_slots{common.consume_steps(1).map_err(paid)?;slot.emission_index=None;}self.emitted_count=None;Ok(())}
 pub(in crate::world) fn append_qualified_emission_v1(&mut self,name:&'static str,json:ReservedValue<Vec<u8>>,common:&mut FormalBoundedContextV1<'_>)->Result<(),BodyMutationError>{
 self.validate_context(common)?;if !json.uses_budget(common.memory()){return Err(invalid("qualified emission pool"))}common.consume_steps(self.state_slots.len()as u64).map_err(paid)?;let index=self.state_slots.iter().position(|slot|slot.definition.name==name).ok_or_else(||invalid("qualified emission original slot"))?;if self.state_slots[index].emission_index.is_some(){return Err(invalid("duplicate qualified emission"))}
 let phase=reserve(common,json.len().checked_mul(1024).and_then(|n|n.checked_add(32768)).ok_or_else(||invalid("qualified emission graph layout"))?)?;common.charge(json.len()as u64,json.len()as u64).map_err(paid)?;let decoded=serde_json::from_slice::<serde_json::Value>(&json);common.consume_steps(0).map_err(paid)?;let decoded=match decoded{Ok(v)=>v,Err(error)=>return Err(paid(RetainedEmissionCodecError{error,_fee:phase}))};let memory=common.memory().clone();let value=ProjectionColumnDataV1::from_json_data(decoded,&memory,&mut|s,b|common.charge(s,b)).map_err(|e|match e{ProjectionColumnErrorV1::Work(e)=>paid(e),ProjectionColumnErrorV1::Memory(e)=>ValueTreeError::Memory(e).into(),ProjectionColumnErrorV1::Invalid(e)=>invalid(e)})?;
 let fee=reserve(common,size_of::<ProjectionOriginalStateEmissionV1>())?;grow(&mut self.emitted,&mut self._emitted_slots,common)?;self.state_slots[index].emission_index=Some(self.emitted.len());self.emitted.push(ProjectionOriginalStateEmissionV1{name,json,value,_fee:fee});Ok(())
 }
 pub(in crate::world) fn finish_qualified_emission_rebuild_v1(&mut self,common:&mut FormalBoundedContextV1<'_>)->Result<(),BodyMutationError>{self.validate_context(common)?;self.emitted_count=Some(self.emitted.len());Ok(())}
}
