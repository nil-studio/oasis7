//! Resident original State serialization slots. This is readonly DATA; the
//! actual parent/Current transition owner alone may qualify its use at seal.
use super::*;
use super::json_fragment::{PaidOriginalJsonFragmentDataV1,prepare_original_json_fragment_data_v1};
use super::original_rows::OriginalProjectionRowsDataV1;
use oasis7_world_state::OriginalStateFieldSlotV1;
use std::{cell::RefCell,mem::size_of,sync::Arc};
use serde::{Serialize,Serializer,ser::{SerializeStruct,Error as _}};

enum Content { Sequence(Arc<OriginalProjectionRowsDataV1>),Rules(Arc<super::world_rule_rows::OriginalWorldRuleRowsDataV1>), Fragment(Arc<PaidOriginalJsonFragmentDataV1>), Map(Arc<OriginalProjectionRowsDataV1>), Set(Arc<OriginalProjectionRowsDataV1>) }
struct Slot { definition:OriginalStateFieldSlotV1,content:Option<Content>,emitted:bool }
pub(super) struct OriginalStateColumnsDataV1{slots:Vec<Slot>,_fee:MemoryReservation}
fn reserve(common:&mut FormalBoundedContextV1<'_>,bytes:usize)->Result<MemoryReservation,BodyMutationError>{common.charge(1,bytes as u64).map_err(paid)?;common.memory().reserve(bytes,MemoryClass::Ordinary).map_err(|e|ValueTreeError::Memory(e).into())}
impl OriginalStateColumnsDataV1{
 /// Exactly the original emitter's complete slot order and actual emitted
 /// fragments, captured once from immutable GEN/cold State.
 pub(super) fn from_original_seed_data_v1(seed:&NormalProjectionColumnSeedDataV1,common:&mut FormalBoundedContextV1<'_>)->Result<Self,BodyMutationError>{
  seed.validate_context(common)?;let n=seed.original_state_slots().len();
  let size=n.checked_mul(size_of::<Slot>()).and_then(|v|v.checked_add(size_of::<Self>())).ok_or_else(||invalid("original State slot layout"))?;
  let fee=reserve(common,size)?;let mut slots=Vec::with_capacity(n);if slots.capacity()!=n{return Err(invalid("original State slot capacity"))}
  for slot in seed.original_state_slots(){
   common.consume_steps(1).map_err(paid)?;
   let content=if let Some(index)=slot.emission_index(){
    let emission=seed.original_state_emissions().get(index).ok_or_else(||invalid("original State emission index"))?;
    if emission.name()!=slot.definition().name{return Err(invalid("original State emission slot"))}
    Some(Content::Fragment(fragment_from_encoded(emission.original_json(),common)?))
   }else{None};
   slots.push(Slot{definition:slot.definition(),content,emitted:slot.emission_index().is_some()});
  }
  let mut result=Self{slots,_fee:fee};
  for field in seed.fields(){
   common.consume_steps(1).map_err(paid)?;
   let descriptor=field.descriptor();
   common.charge(semantic_state_columns_v1().len() as u64,(semantic_state_columns_v1().len()*128) as u64).map_err(paid)?;
   if !semantic_state_columns_v1().iter().any(|original|*original==descriptor){continue}
   if matches!(descriptor.shape,oasis7_world_state::incremental_projection::FieldShape::NestedMap|oasis7_world_state::incremental_projection::FieldShape::MapSet){
    let rows=super::nested_rows::initialize_original_nested_rows_data_v1(field,common)?;let mut found=false;
    for slot in &mut result.slots{common.charge(1,(descriptor.name.len()+slot.definition.name.len()) as u64).map_err(paid)?;if slot.definition.name==descriptor.name{slot.content=Some(Content::Map(Arc::new(rows)));found=true;break}}
    if !found{return Err(invalid("nested original State slot missing"))}continue
   }
   if descriptor.owner==FieldOwner::WorldState&&descriptor.name==oasis7_world_state::incremental_reward_budget_records::RewardRecordKindV1::Mint.name(){
    if descriptor.shape!=oasis7_world_state::incremental_projection::FieldShape::Ordered{return Err(invalid("reward original sequence registration"))}let mut rows=OriginalProjectionRowsDataV1::empty_data_v1(common)?;for row in field.rows(){common.consume_steps(1).map_err(paid)?;let row=super::original_rows::OriginalProjectionRowDataV1::from_original_encoded_data_v1(copy_bytes(row.semantic(),common)?,copy_bytes(row.original_key_json(),common)?,copy_bytes(row.original_json(),common)?,common)?;rows=rows.with_row_data_v1(row,common)?;}result.replace_original_sequence_data_v1(descriptor.name,rows,common)?;continue
   }
   if descriptor.owner!=FieldOwner::WorldState||!matches!(descriptor.shape,oasis7_world_state::incremental_projection::FieldShape::Map|oasis7_world_state::incremental_projection::FieldShape::Set|oasis7_world_state::incremental_projection::FieldShape::Children|oasis7_world_state::incremental_projection::FieldShape::ByteMap){continue}
   let mut rows=OriginalProjectionRowsDataV1::empty_data_v1(common)?;
   for row in field.rows(){
    let semantic=copy_bytes(row.semantic(),common)?;let key=copy_bytes(row.original_key_json(),common)?;let value=copy_bytes(row.original_json(),common)?;
    let kind=oasis7_world_state::incremental_governance_records::GovernanceRecordKindV1::ALL.into_iter().find(|kind|kind.field_name()==descriptor.name);
    let row=if matches!(descriptor.name,"module_artifact_listings"|"module_visual_entities"){let field=oasis7_world_state::incremental_module_marketplace::ModuleMarketplaceFieldV1::from_name(descriptor.name).ok_or_else(||invalid("marketplace source field"))?;let value=super::market_rows::OriginalMarketRecordDataV1::from_original_qualified_json_v1(field,&value,common)?;super::original_rows::OriginalProjectionRowDataV1::from_market_record_data_v1(semantic,key,value,common)?}else if let Some(kind)=kind{let value=super::governance_rows::OriginalGovernanceRecordDataV1::from_original_qualified_json_v1(kind,&value,common)?;super::original_rows::OriginalProjectionRowDataV1::from_governance_data_v1(semantic,key,value,common)?}else if descriptor.name==oasis7_world_state::incremental_reward_budget_records::RewardRecordKindV1::Pool.name(){let value=super::reward_rows::OriginalRewardPoolRecordDataV1::from_original_qualified_json_v1(&value,common)?;super::original_rows::OriginalProjectionRowDataV1::from_reward_pool_data_v1(semantic,key,value,common)?}else if let Some(field)=oasis7_world_state::incremental_diplomacy::DiplomacyFieldV1::ALL.into_iter().find(|field|field.name()==descriptor.name){let value=super::diplomacy_rows::OriginalDiplomacyRecordDataV1::from_original_qualified_json_v1(field,&value,common)?;super::original_rows::OriginalProjectionRowDataV1::from_diplomacy_data_v1(semantic,key,value,common)?}else{super::original_rows::OriginalProjectionRowDataV1::from_original_encoded_data_v1(semantic,key,value,common)?};
    rows=rows.with_row_data_v1(row,common)?;
   }
   let mut found=false;
   for slot in &mut result.slots{common.charge(1,(descriptor.name.len()+slot.definition.name.len()) as u64).map_err(paid)?;if slot.definition.name==descriptor.name{slot.content=Some(if matches!(descriptor.shape,oasis7_world_state::incremental_projection::FieldShape::Set){Content::Set(Arc::new(rows))}else{Content::Map(Arc::new(rows))});found=true;break}}
   if !found{return Err(invalid("native original State map slot missing"))}
  }
  let marketplace=seed.marketplace_index_v1(common)?;
  let market_rows=marketplace.original_rows_v1(common)?;
  result.replace_original_map_data_v1("module_artifact_bids",market_rows,common)?;
  if let Some(rules)=seed.world_rule_rows_v1(){for slot in &mut result.slots{common.consume_steps(1).map_err(paid)?;if slot.definition.name=="world_rules"{slot.content=Some(Content::Rules(Arc::clone(rules)));break}}}
  Ok(result)
 }
 pub(super) fn validate_context(&self,common:&mut FormalBoundedContextV1<'_>)->Result<(),BodyMutationError>{common.consume_steps(1).map_err(paid)?;if !self._fee.uses_budget(common.memory()){return Err(invalid("original State columns context"))}Ok(())}
 fn copy_slots(&self,common:&mut FormalBoundedContextV1<'_>)->Result<Self,BodyMutationError>{
  self.validate_context(common)?;let n=self.slots.len();let bytes=n.checked_mul(size_of::<Slot>()).and_then(|v|v.checked_add(size_of::<Self>())).ok_or_else(||invalid("original State slot copy layout"))?;
  let fee=reserve(common,bytes)?;let mut slots=Vec::with_capacity(n);if slots.capacity()!=n{return Err(invalid("original State slot copy capacity"))}
  for slot in &self.slots{common.consume_steps(1).map_err(paid)?;let content=match &slot.content{None=>None,Some(Content::Rules(v))=>Some(Content::Rules(Arc::clone(v))),Some(Content::Fragment(v))=>Some(Content::Fragment(Arc::clone(v))),Some(Content::Sequence(v))=>Some(Content::Sequence(Arc::clone(v))),Some(Content::Map(v))=>Some(Content::Map(Arc::clone(v))),Some(Content::Set(v))=>Some(Content::Set(Arc::clone(v)))};slots.push(Slot{definition:slot.definition,content,emitted:slot.emitted})}
  Ok(Self{slots,_fee:fee})
 }
 /// Internal DATA transformation. `actual_emitted` must be supplied by the
 /// fixed source's original predicate; it is never an execution permission.
 pub(super) fn with_map_data_v1(&self,name:&str,rows:Arc<OriginalProjectionRowsDataV1>,actual_emitted:bool,common:&mut FormalBoundedContextV1<'_>)->Result<Self,BodyMutationError>{
  rows.validate(common)?;let mut after=self.copy_slots(common)?;let mut found=false;
  for slot in &mut after.slots{common.charge(1,(name.len()+slot.definition.name.len()) as u64).map_err(paid)?;if slot.definition.name==name{slot.content=Some(match slot.content.as_ref(){Some(Content::Set(_))=>Content::Set(rows),_=>Content::Map(rows)});slot.emitted=actual_emitted;found=true;break}}
  if !found{return Err(invalid("original State map slot missing"))}Ok(after)
 }
 pub(super) fn world_rule_rows_data_v1(&self,common:&mut FormalBoundedContextV1<'_>)->Result<Option<&Arc<super::world_rule_rows::OriginalWorldRuleRowsDataV1>>,BodyMutationError>{self.validate_context(common)?;for slot in &self.slots{common.consume_steps(1).map_err(paid)?;if slot.definition.name=="world_rules"{return match &slot.content{Some(Content::Rules(rows))=>Ok(Some(rows)),None if !slot.emitted=>Ok(None),Some(Content::Fragment(raw)) if raw.as_bytes()==b"null"=>Ok(None),_=>Err(invalid("WorldRules logical source missing"))}}}Err(invalid("WorldRules state slot missing"))}
 pub(super) fn apply_world_rule_delta_data_v1<M:oasis7_formal_protocol::incremental_state::MerkleNodeReader>(&mut self,source:&crate::world::incremental_normal_projection_commitment_world_rules::PaidCurrentProjectionWorldRulesDeltaV1,tx:&oasis7_formal_protocol::incremental_execution::ExecutionTransaction<'_,M>,common:&mut FormalBoundedContextV1<'_>,live:&oasis7_formal_protocol::incremental_execution::ExecutionFence)->Result<(),BodyMutationError>{
  source.validate(tx,common.memory(),common.work().identity(),live)?;
  if !source.rules_present(){for slot in &mut self.slots{common.consume_steps(1).map_err(paid)?;if slot.definition.name=="world_rules"{slot.content=None;slot.emitted=false;return Ok(())}}return Err(invalid("WorldRules target State slot missing"))}
  let existing=self.world_rule_rows_data_v1(common)?;let rows=match existing{Some(rows)=>rows.apply_actual_delta_v1(source,tx,common,live)?,None=>{
   // This baseline is the original new optional-container initializer, never
   // inherited actual state. The genuine Current source has independently
   // proved parent absence/current presence and all changed typed branches.
   let schema=oasis7_world_state::world_rules::WorldRulesStateV1::default();
   let before=super::world_rule_rows::OriginalWorldRuleRowsDataV1::from_actual_v1(&schema,common)?;before.apply_actual_delta_v1(source,tx,common,live)?
  }};
  for slot in &mut self.slots{common.consume_steps(1).map_err(paid)?;if slot.definition.name=="world_rules"{slot.content=Some(Content::Rules(Arc::new(rows)));slot.emitted=true;return Ok(())}}Err(invalid("WorldRules target State slot missing"))
 }
 pub(super) fn replace_original_map_data_v1(&mut self,name:&str,rows:OriginalProjectionRowsDataV1,common:&mut FormalBoundedContextV1<'_>)->Result<(),BodyMutationError>{self.validate_context(common)?;rows.validate(common)?;for slot in &mut self.slots{common.charge(1,(name.len()+slot.definition.name.len())as u64).map_err(paid)?;if slot.definition.name==name{let emitted=match slot.definition.emission{oasis7_world_state::OriginalStateFieldEmissionV1::Always|oasis7_world_state::OriginalStateFieldEmissionV1::NonEmpty=>slot.definition.emission.emits_count(rows.len()),_=>return Err(invalid("original nested map emission profile"))};slot.content=Some(Content::Map(Arc::new(rows)));slot.emitted=emitted;return Ok(())}}Err(invalid("original nested map target missing"))}
 pub(super) fn sequence_data_v1(&self,name:&str,common:&mut FormalBoundedContextV1<'_>)->Result<&Arc<OriginalProjectionRowsDataV1>,BodyMutationError>{self.validate_context(common)?;for slot in &self.slots{common.charge(1,(name.len()+slot.definition.name.len())as u64).map_err(paid)?;if slot.definition.name==name{return match &slot.content{Some(Content::Sequence(rows))=>Ok(rows),_=>Err(invalid("original sequence source kind"))}}}Err(invalid("original sequence slot missing"))}
 pub(super) fn replace_original_sequence_data_v1(&mut self,name:&str,rows:OriginalProjectionRowsDataV1,common:&mut FormalBoundedContextV1<'_>)->Result<(),BodyMutationError>{self.validate_context(common)?;rows.validate(common)?;for slot in &mut self.slots{common.charge(1,(name.len()+slot.definition.name.len())as u64).map_err(paid)?;if slot.definition.name==name{let emitted=match slot.definition.emission{oasis7_world_state::OriginalStateFieldEmissionV1::Always|oasis7_world_state::OriginalStateFieldEmissionV1::NonEmpty=>slot.definition.emission.emits_count(rows.len()),_=>return Err(invalid("original sequence emission profile"))};slot.content=Some(Content::Sequence(Arc::new(rows)));slot.emitted=emitted;return Ok(())}}Err(invalid("original sequence slot missing"))}
 pub(super) fn map_data_v1(&self,name:&str,common:&mut FormalBoundedContextV1<'_>)->Result<Option<&Arc<OriginalProjectionRowsDataV1>>,BodyMutationError>{
  self.validate_context(common)?;for slot in &self.slots{common.charge(1,(name.len()+slot.definition.name.len()) as u64).map_err(paid)?;if slot.definition.name==name{return Ok(match &slot.content{Some(Content::Map(rows))|Some(Content::Set(rows))=>Some(rows),_=>None})}}Err(invalid("original State source slot missing"))
 }
 pub(super) fn original_field_emitted_v1(&self,name:&str,common:&mut FormalBoundedContextV1<'_>)->Result<bool,BodyMutationError>{self.validate_context(common)?;for slot in &self.slots{common.charge(1,(name.len()+slot.definition.name.len())as u64).map_err(paid)?;if slot.definition.name==name{return Ok(slot.emitted)}}Err(invalid("original State emission slot missing"))}
 pub(super) fn scalar_original_json_data_v1(&self,name:&str,common:&mut FormalBoundedContextV1<'_>)->Result<&[u8],BodyMutationError>{
  self.validate_context(common)?;for slot in &self.slots{common.charge(1,(name.len()+slot.definition.name.len()) as u64).map_err(paid)?;if slot.definition.name==name{return match &slot.content{Some(Content::Fragment(value))=>Ok(value.as_bytes()),None if !slot.emitted&&slot.definition.emission==oasis7_world_state::OriginalStateFieldEmissionV1::Some=>Ok(b"null"),_=>Err(invalid("original scalar source kind"))}}}Err(invalid("original scalar source slot missing"))
 }
 pub(super) fn with_fragment_data_v1(&self,name:&str,value:Arc<PaidOriginalJsonFragmentDataV1>,common:&mut FormalBoundedContextV1<'_>)->Result<Self,BodyMutationError>{
  if !value.uses_budget(common.memory()){return Err(invalid("original State fragment pool"))}let mut after=self.copy_slots(common)?;
  for slot in &mut after.slots{common.charge(1,(name.len()+slot.definition.name.len()) as u64).map_err(paid)?;if slot.definition.name==name{
   slot.emitted=match slot.definition.emission{oasis7_world_state::OriginalStateFieldEmissionV1::Always=>true,oasis7_world_state::OriginalStateFieldEmissionV1::Some=>value.as_bytes()!=b"null",_=>return Err(invalid("original State non-scalar emission source"))};slot.content=Some(Content::Fragment(value));return Ok(after)
  }}Err(invalid("original State fragment slot missing"))
 }
 pub(super) fn apply_original_patch_data_v1(&mut self,name:&str,semantic:&[u8],key_json:&[u8],value_json:Option<&[u8]>,common:&mut FormalBoundedContextV1<'_>)->Result<(),BodyMutationError>{
  self.validate_context(common)?;common.charge(1,semantic.len().checked_mul(32).ok_or_else(||invalid("original patch semantic work"))? as u64).map_err(paid)?;
  let key=oasis7_world_state::incremental_semantic_key::read_semantic_key_v1(semantic).map_err(invalid)?;
  let mut index=None;for(i,slot)in self.slots.iter().enumerate(){common.charge(1,(name.len()+slot.definition.name.len()) as u64).map_err(paid)?;if slot.definition.name==name{index=Some(i);break}}
  let index=index.ok_or_else(||invalid("original patch State slot missing"))?;
  if matches!(key,oasis7_world_state::incremental_semantic_key::SemanticKeyRefV1::Unit){
   let value=value_json.ok_or_else(||invalid("original State scalar deleted"))?;let value=fragment_from_encoded(value,common)?;
   let emitted=match self.slots[index].definition.emission{oasis7_world_state::OriginalStateFieldEmissionV1::Always=>true,oasis7_world_state::OriginalStateFieldEmissionV1::Some=>value.as_bytes()!=b"null",_=>return Err(invalid("original scalar patch emission profile"))};
   self.slots[index].content=Some(Content::Fragment(value));self.slots[index].emitted=emitted;return Ok(())
  }
  let (before,is_set)=match &self.slots[index].content{Some(Content::Map(rows))=>(rows,false),Some(Content::Set(rows))=>(rows,true),_=>return Err(invalid("original patch map source missing"))};
  let rows=if let Some(value)=value_json{let row=super::original_rows::OriginalProjectionRowDataV1::from_original_encoded_data_v1(copy_bytes(semantic,common)?,copy_bytes(key_json,common)?,copy_bytes(value,common)?,common)?;before.with_row_data_v1(row,common)?}else{before.without_row_data_v1(semantic,common)?};
  let emitted=match self.slots[index].definition.emission{oasis7_world_state::OriginalStateFieldEmissionV1::Always|oasis7_world_state::OriginalStateFieldEmissionV1::NonEmpty=>self.slots[index].definition.emission.emits_count(rows.len()),_=>return Err(invalid("original map patch emission profile"))};
  self.slots[index].content=Some(if is_set{Content::Set(Arc::new(rows))}else{Content::Map(Arc::new(rows))});self.slots[index].emitted=emitted;Ok(())
 }
 pub(super) fn fork_data_v1(&self,common:&mut FormalBoundedContextV1<'_>)->Result<Self,BodyMutationError>{self.copy_slots(common)}
 pub(super) fn serialization_v1<'a,'b,'c>(&'a self,common:&'a RefCell<&'b mut FormalBoundedContextV1<'c>>,first:&'a RefCell<Option<BodyMutationError>>)->StateColumnsSerializeV1<'a,'b,'c>{StateColumnsSerializeV1{columns:self,common,first}}
}
pub(super) struct StateColumnsSerializeV1<'a,'b,'c>{columns:&'a OriginalStateColumnsDataV1,common:&'a RefCell<&'b mut FormalBoundedContextV1<'c>>,first:&'a RefCell<Option<BodyMutationError>>}
impl Serialize for StateColumnsSerializeV1<'_,'_,'_>{fn serialize<S:Serializer>(&self,serializer:S)->Result<S::Ok,S::Error>{
 if self.first.borrow().is_some(){return Err(S::Error::custom("original State meter stopped"))}
 if let Err(error)=self.columns.validate_context(&mut self.common.borrow_mut()){*self.first.borrow_mut()=Some(error);return Err(S::Error::custom("original State columns context"))}
 if let Err(error)=self.common.borrow_mut().consume_steps(self.columns.slots.len() as u64){*self.first.borrow_mut()=Some(paid(error));return Err(S::Error::custom("original State count meter stopped"))}
 let count=self.columns.slots.iter().filter(|slot|slot.emitted).count();let mut output=serializer.serialize_struct("WorldState",count)?;
 for slot in &self.columns.slots{
  if let Err(error)=self.common.borrow_mut().consume_steps(1){*self.first.borrow_mut()=Some(paid(error));return Err(S::Error::custom("original State meter stopped"))}
  if !slot.emitted{continue}
  match &slot.content{Some(Content::Rules(rows))=>output.serialize_field(slot.definition.name,&rows.serialization_v1(self.common,self.first))?,None=>return Err(S::Error::custom("emitted original State source missing")),Some(Content::Fragment(fragment))=>output.serialize_field(slot.definition.name,fragment.raw())?,Some(Content::Sequence(rows))=>output.serialize_field(slot.definition.name,&rows.original_value_sequence_serialization_v1(self.common,self.first))?,Some(Content::Map(rows))=>output.serialize_field(slot.definition.name,&rows.original_map_serialization_v1(self.common,self.first))?,Some(Content::Set(rows))=>output.serialize_field(slot.definition.name,&rows.original_set_serialization_v1(self.common,self.first))?}
 }
 output.end()
}}

fn copy_bytes(bytes:&[u8],common:&mut FormalBoundedContextV1<'_>)->Result<ReservedValue<Vec<u8>>,BodyMutationError>{
 let n=bytes.len().checked_add(size_of::<Vec<u8>>()).ok_or_else(||invalid("original State bytes layout"))?;let fee=reserve(common,n)?;
 let mut output=Vec::with_capacity(bytes.len());if output.capacity()!=bytes.len(){return Err(invalid("original State bytes capacity"))}
 for chunk in bytes.chunks(8192){common.charge(1,chunk.len() as u64).map_err(paid)?;output.extend_from_slice(chunk)}common.consume_steps(0).map_err(paid)?;Ok(fee.retain(output))
}

#[derive(Debug)]struct PaidStateInputCodecError{error:serde_json::Error,_fee:MemoryReservation}
impl std::fmt::Display for PaidStateInputCodecError{fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result{std::fmt::Display::fmt(&self.error,f)}}
impl std::error::Error for PaidStateInputCodecError{fn source(&self)->Option<&(dyn std::error::Error+'static)>{Some(&self.error)}}
fn fragment_from_encoded(bytes:&[u8],common:&mut FormalBoundedContextV1<'_>)->Result<Arc<PaidOriginalJsonFragmentDataV1>,BodyMutationError>{
 let phase=reserve(common,bytes.len().checked_mul(2).and_then(|n|n.checked_add(32768)).ok_or_else(||invalid("native State JSON refusal layout"))?)?;
 common.charge(bytes.len() as u64,bytes.len() as u64).map_err(paid)?;let decoded=serde_json::from_slice::<&serde_json::value::RawValue>(bytes);common.consume_steps(0).map_err(paid)?;
 let raw=match decoded{Ok(raw)=>raw,Err(error)=>return Err(paid(PaidStateInputCodecError{error,_fee:phase}))};
 prepare_original_json_fragment_data_v1(raw,bytes.len(),common)
}

#[path="incremental_normal_projection_commitment_state_columns_cold.rs"] mod qualified;
