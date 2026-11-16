//! Private original row builder shared by GEN and fixed qualified Cold sources.
//! This carries DATA only; every Cold entry point validates its opaque sources.
use super::*;
#[derive(Debug)]struct OriginalColumnCodecCause{error:serde_json::Error,_phase:MemoryReservation}
impl std::fmt::Display for OriginalColumnCodecCause{fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result{self.error.fmt(f)}}impl std::error::Error for OriginalColumnCodecCause{}
impl NormalProjectionColumnInitializerV1<'_, '_>{
 pub(super) fn append_original_encoded_row_v1(&mut self,field:FieldDescriptor,semantic:ReservedValue<Vec<u8>>,key_json:ReservedValue<Vec<u8>>,json:ReservedValue<Vec<u8>>)->Result<(),BodyMutationError>{
  if !self.active.contains(&(field.owner,field.name)){return Err(invalid("projection column field framing"))}
  if !semantic.uses_budget(self.common.memory())||!key_json.uses_budget(self.common.memory())||!json.uses_budget(self.common.memory()){return Err(invalid("original qualified column pool"))}
  self.common.charge(1,semantic.len()as u64).map_err(paid)?;oasis7_world_state::incremental_semantic_key::read_semantic_key_v1(&semantic).map_err(invalid)?;
  let n=json.len().checked_mul(1024).and_then(|v|v.checked_add(32768)).ok_or_else(||invalid("projection individual column graph layout"))?;
  let phase=reserve(self.common,n)?;self.common.charge(json.len()as u64,json.len()as u64).map_err(paid)?;
  let decoded=serde_json::from_slice::<serde_json::Value>(&json);self.common.consume_steps(0).map_err(paid)?;
  let decoded=match decoded{Ok(value)=>value,Err(error)=>return Err(paid(OriginalColumnCodecCause{error,_phase:phase}))};
  let memory=self.common.memory().clone();let value=ProjectionColumnDataV1::from_json_data(decoded,&memory,&mut|s,b|self.common.charge(s,b)).map_err(|e|match e{ProjectionColumnErrorV1::Work(e)=>paid(e),ProjectionColumnErrorV1::Memory(e)=>ValueTreeError::Memory(e).into(),ProjectionColumnErrorV1::Invalid(e)=>invalid(e)})?;
  let fee=reserve(self.common,size_of::<ProjectionOriginalColumnRowV1>())?;let target=self.fields.get_mut(&(field.owner,field.name)).ok_or_else(||invalid("projection column field missing"))?;
  grow(&mut target.rows,&mut target._row_slots,self.common)?;target.rows.push(ProjectionOriginalColumnRowV1{semantic,key_json,json,value,agent_electricity:None,agent_intent:None,_fee:fee});Ok(())
 }
 pub(super) fn row_from_qualified_json_v1(&mut self,field:FieldDescriptor,semantic:&[u8],key:&[u8],json:&[u8])->Result<(),BodyMutationError>{
  self.common.consume_steps(1).map_err(paid)?;if !self.active.contains(&(field.owner,field.name)){return Err(invalid("qualified column field framing"))}if !Self::needed(field){return Ok(())}
  let copy=|input:&[u8],common:&mut FormalBoundedContextV1<'_>|capture(common,|out|std::io::Write::write_all(out,input).map_err(paid));
  let semantic=copy(semantic,self.common)?;let key=copy(key,self.common)?;let json=copy(json,self.common)?;
  self.append_original_encoded_row_v1(field,semantic,key,json)
 }
}
