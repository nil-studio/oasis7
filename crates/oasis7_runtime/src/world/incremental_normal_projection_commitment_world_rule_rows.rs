//! Closed WorldRules logical columns: original typed keys, including empty
//! maps/sets, remain separate from the original thirteen-field JSON emission.
use super::*;
use super::original_rows::{OriginalProjectionRowsDataV1,OriginalProjectionRowDataV1};
use oasis7_world_state::incremental_world_rule_projection_delta::{WorldRuleLogicalColumnV1 as Column,WorldRuleLogicalTopSlotV1 as Top,WorldRuleLogicalRowsVisitorV1};
use oasis7_world_state::{OriginalStateFieldEmissionV1,incremental_semantic_key::{SemanticKeyV1,write_semantic_key_v1}};
use std::{collections::{BTreeMap,BTreeSet},mem::size_of,sync::Arc};
enum Content{Rows(Arc<OriginalProjectionRowsDataV1>),Scalar(Arc<super::json_fragment::PaidOriginalJsonFragmentDataV1>)}
struct Entry{column:Column,content:Content}
struct TopSlot{slot:Top,emission:OriginalStateFieldEmissionV1,emitted:bool}
pub(super) struct OriginalWorldRuleRowsDataV1{columns:Vec<Entry>,tops:Vec<TopSlot>,_fee:MemoryReservation}
fn reserve(common:&mut FormalBoundedContextV1<'_>,n:usize)->Result<MemoryReservation,BodyMutationError>{common.charge(1,n as u64).map_err(paid)?;common.memory().reserve(n,MemoryClass::Ordinary).map_err(ValueTreeError::Memory).map_err(Into::into)}
impl OriginalWorldRuleRowsDataV1{
 pub(super) fn from_actual_v1(source:&oasis7_world_state::world_rules::WorldRulesStateV1,common:&mut FormalBoundedContextV1<'_>)->Result<Self,BodyMutationError>{
  let n=Column::ALL.len().checked_mul(size_of::<Entry>()).and_then(|n|n.checked_add(13*size_of::<TopSlot>()+size_of::<Self>()+2*size_of::<usize>())).ok_or_else(||invalid("WorldRules row inventory layout"))?;let fee=reserve(common,n)?;
  let mut builder=Builder{data:Self{columns:Vec::with_capacity(Column::ALL.len()),tops:Vec::with_capacity(13),_fee:fee},common};
  if builder.data.columns.capacity()!=Column::ALL.len()||builder.data.tops.capacity()!=13{return Err(invalid("WorldRules inventory capacity"))}
  oasis7_world_state::incremental_world_rule_projection_delta::visit_original_world_rule_logical_rows_v1(source,&mut builder)?;
  if builder.data.columns.len()!=Column::ALL.len()||builder.data.tops.len()!=13{return Err(invalid("WorldRules logical inventory incomplete"))}Ok(builder.data)
 }
 pub(super) fn rows_v1(&self,column:Column,common:&mut FormalBoundedContextV1<'_>)->Result<&Arc<OriginalProjectionRowsDataV1>,BodyMutationError>{
  self.validate(common)?;for entry in &self.columns{common.consume_steps(1).map_err(paid)?;if entry.column==column{return match &entry.content{Content::Rows(rows)=>Ok(rows),Content::Scalar(_)=>Err(invalid("WorldRules scalar is not row map"))}}}Err(invalid("WorldRules logical column missing"))
 }
 pub(super) fn validate(&self,common:&mut FormalBoundedContextV1<'_>)->Result<(),BodyMutationError>{common.consume_steps(1).map_err(paid)?;if !self._fee.uses_budget(common.memory()){return Err(invalid("WorldRules logical pool"))}Ok(())}
}
struct Builder<'a,'c>{data:OriginalWorldRuleRowsDataV1,common:&'a mut FormalBoundedContextV1<'c>}
impl Builder<'_,'_>{
 fn add(&mut self,column:Column,content:Content)->Result<(),BodyMutationError>{self.common.consume_steps(1).map_err(paid)?;if self.data.columns.len()==self.data.columns.capacity(){return Err(invalid("WorldRules logical columns overflow"))}for old in &self.data.columns{self.common.consume_steps(1).map_err(paid)?;if old.column==column{return Err(invalid("duplicate WorldRules logical column"))}}self.data.columns.push(Entry{column,content});Ok(())}
 fn row<K:SemanticKeyV1+serde::Serialize+?Sized,V:serde::Serialize+?Sized>(&mut self,rows:&OriginalProjectionRowsDataV1,key:&K,value:&V)->Result<OriginalProjectionRowsDataV1,BodyMutationError>{
  let semantic=super::initial::capture(self.common,|out|write_semantic_key_v1(key,out).map_err(paid))?;
  let key=super::initial::capture(self.common,|out|serde_json::to_writer(out,key).map_err(paid))?;
  let value=super::initial::capture(self.common,|out|serde_json::to_writer(out,value).map_err(paid))?;
  let row=OriginalProjectionRowDataV1::from_original_encoded_data_v1(semantic,key,value,self.common)?;rows.with_row_data_v1(row,self.common)
 }
}
impl WorldRuleLogicalRowsVisitorV1 for Builder<'_,'_>{
 type Error=BodyMutationError;
 fn work(&mut self,steps:u64,bytes:usize)->Result<(),Self::Error>{self.common.charge(steps,bytes as u64).map_err(paid)}
 fn top_slot(&mut self,slot:Top,emission:OriginalStateFieldEmissionV1)->Result<(),Self::Error>{self.common.consume_steps(1).map_err(paid)?;if self.data.tops.len()==self.data.tops.capacity(){return Err(invalid("WorldRules top slot capacity"))}for old in &self.data.tops{self.common.consume_steps(1).map_err(paid)?;if old.slot==slot{return Err(invalid("duplicate WorldRules top slot"))}}self.data.tops.push(TopSlot{slot,emission,emitted:false});Ok(())}
 fn top_emitted(&mut self,slot:Top)->Result<(),Self::Error>{for old in &mut self.data.tops{self.common.consume_steps(1).map_err(paid)?;if old.slot==slot{old.emitted=true;return Ok(())}}Err(invalid("WorldRules emitted slot not declared"))}
 fn map<K:Ord+serde::Serialize+SemanticKeyV1,V:serde::Serialize>(&mut self,column:Column,values:&BTreeMap<K,V>)->Result<(),Self::Error>{let mut rows=OriginalProjectionRowsDataV1::empty_data_v1(self.common)?;for(k,v)in values{self.common.consume_steps(1).map_err(paid)?;rows=self.row(&rows,k,v)?}self.add(column,Content::Rows(Arc::new(rows)))}
 fn set<K:Ord+serde::Serialize+SemanticKeyV1>(&mut self,column:Column,values:&BTreeSet<K>)->Result<(),Self::Error>{let mut rows=OriginalProjectionRowsDataV1::empty_data_v1(self.common)?;for key in values{self.common.consume_steps(1).map_err(paid)?;rows=self.row(&rows,key,key)?}self.add(column,Content::Rows(Arc::new(rows)))}
 fn sequence<V:serde::Serialize>(&mut self,column:Column,values:&[V])->Result<(),Self::Error>{let mut rows=OriginalProjectionRowsDataV1::empty_data_v1(self.common)?;for(index,value)in values.iter().enumerate(){self.common.consume_steps(1).map_err(paid)?;let index=u64::try_from(index).map_err(|_|invalid("WorldRules sequence index"))?;rows=self.row(&rows,&index,value)?}self.add(column,Content::Rows(Arc::new(rows)))}
 fn scalar<V:serde::Serialize+?Sized>(&mut self,column:Column,value:&V)->Result<(),Self::Error>{let encoded=super::initial::capture(self.common,|out|serde_json::to_writer(out,value).map_err(paid))?;let fragment=super::json_fragment::prepare_original_json_fragment_data_v1(value,encoded.len(),self.common)?;self.add(column,Content::Scalar(fragment))}
}

use std::cell::RefCell;
use serde::{Serialize,Serializer,ser::{SerializeStruct,SerializeSeq,Error as _}};
pub(super) struct RulesSerialize<'a,'b,'c>{data:&'a OriginalWorldRuleRowsDataV1,common:&'a RefCell<&'b mut FormalBoundedContextV1<'c>>,first:&'a RefCell<Option<BodyMutationError>>}
impl OriginalWorldRuleRowsDataV1{
 pub(super) fn optional_branch_serialization_v1<'a,'b,'c>(&'a self,top:Top,common:&'a RefCell<&'b mut FormalBoundedContextV1<'c>>,first:&'a RefCell<Option<BodyMutationError>>)->Result<BranchSerialize<'a,'b,'c>,BodyMutationError>{if !matches!(top,Top::IndustrialWork|Top::Substitution){return Err(invalid("WorldRules optional branch selector"))}self.validate(&mut common.borrow_mut())?;Ok(BranchSerialize{data:self,top,common,first})}
 pub(super) fn serialization_v1<'a,'b,'c>(&'a self,common:&'a RefCell<&'b mut FormalBoundedContextV1<'c>>,first:&'a RefCell<Option<BodyMutationError>>)->RulesSerialize<'a,'b,'c>{RulesSerialize{data:self,common,first}}
}
struct ColumnSerialize<'a,'b,'c>{entry:&'a Entry,common:&'a RefCell<&'b mut FormalBoundedContextV1<'c>>,first:&'a RefCell<Option<BodyMutationError>>}
impl Serialize for ColumnSerialize<'_,'_,'_>{fn serialize<S:Serializer>(&self,s:S)->Result<S::Ok,S::Error>{
 match &self.entry.content{
 Content::Scalar(value)=>value.raw().serialize(s),
 Content::Rows(rows)=>match self.entry.column.kind(){
 oasis7_world_state::incremental_world_rule_projection_delta::WorldRuleLogicalContainerKindV1::Map=>rows.original_map_serialization_v1(self.common,self.first).serialize(s),
 oasis7_world_state::incremental_world_rule_projection_delta::WorldRuleLogicalContainerKindV1::Set=>rows.original_set_serialization_v1(self.common,self.first).serialize(s),
 oasis7_world_state::incremental_world_rule_projection_delta::WorldRuleLogicalContainerKindV1::Sequence=>{
 let mut sequence=s.serialize_seq(Some(rows.len()))?;
 let mut iter=match rows.iter_data_v1(&mut self.common.borrow_mut()){Ok(iter)=>iter,Err(error)=>{*self.first.borrow_mut()=Some(error);return Err(S::Error::custom("WorldRules sequence meter"))}};
 loop{let row=match iter.next_data_v1(&mut self.common.borrow_mut()){Ok(row)=>row,Err(error)=>{*self.first.borrow_mut()=Some(error);return Err(S::Error::custom("WorldRules sequence meter"))}};let Some(row)=row else{break};let bytes=row.original_value_json().ok_or_else(||S::Error::custom("WorldRules sequence source"))?;
 // Source values were produced by the unique original encoder. RawValue
 // borrows the retained row; no DTO or sequence graph is cloned.
 if let Err(error)=self.common.borrow_mut().charge(bytes.len() as u64,bytes.len() as u64){*self.first.borrow_mut()=Some(paid(error));return Err(S::Error::custom("WorldRules sequence work"))}
 let parsed=serde_json::from_slice::<&serde_json::value::RawValue>(bytes);if let Err(error)=self.common.borrow_mut().consume_steps(0){*self.first.borrow_mut()=Some(paid(error));return Err(S::Error::custom("WorldRules sequence cancellation"))}let raw=parsed.map_err(S::Error::custom)?;sequence.serialize_element(raw)?;
 }sequence.end()},
 _=>Err(S::Error::custom("WorldRules rows scalar profile")),
 }}
}}
pub(super) struct BranchSerialize<'a,'b,'c>{data:&'a OriginalWorldRuleRowsDataV1,top:Top,common:&'a RefCell<&'b mut FormalBoundedContextV1<'c>>,first:&'a RefCell<Option<BodyMutationError>>}
impl Serialize for BranchSerialize<'_,'_,'_>{fn serialize<S:Serializer>(&self,s:S)->Result<S::Ok,S::Error>{
 let mut count=0;for entry in &self.data.columns{if let Err(error)=self.common.borrow_mut().consume_steps(1){*self.first.borrow_mut()=Some(paid(error));return Err(S::Error::custom("WorldRules branch count work"))}let path=entry.column.path();if path[1]==self.top.name(){count+=1}}
 let mut output=s.serialize_struct("WorldRulesBranch",count)?;
 for entry in &self.data.columns{let path=entry.column.path();if let Err(error)=self.common.borrow_mut().consume_steps(1){*self.first.borrow_mut()=Some(paid(error));return Err(S::Error::custom("WorldRules branch work"))}
 if path[1]!=self.top.name(){continue}let name=*path.get(2).ok_or_else(||S::Error::custom("WorldRules branch field"))?;output.serialize_field(name,&ColumnSerialize{entry,common:self.common,first:self.first})?}output.end()
}}
impl Serialize for RulesSerialize<'_,'_,'_>{fn serialize<S:Serializer>(&self,s:S)->Result<S::Ok,S::Error>{
 if let Err(error)=self.data.validate(&mut self.common.borrow_mut()){*self.first.borrow_mut()=Some(error);return Err(S::Error::custom("WorldRules source pool"))}
 let mut count=0;for top in &self.data.tops{if let Err(error)=self.common.borrow_mut().consume_steps(1){*self.first.borrow_mut()=Some(paid(error));return Err(S::Error::custom("WorldRules count work"))}
 if top.emitted{count+=1}}
 let mut output=s.serialize_struct("WorldRulesStateV1",count)?;
 for top in &self.data.tops{if let Err(error)=self.common.borrow_mut().consume_steps(1){*self.first.borrow_mut()=Some(paid(error));return Err(S::Error::custom("WorldRules top work"))}
 if !top.emitted{continue}let mut direct=None;for entry in &self.data.columns{if let Err(error)=self.common.borrow_mut().consume_steps(1){*self.first.borrow_mut()=Some(paid(error));return Err(S::Error::custom("WorldRules top column work"))}let path=entry.column.path();if path.len()==2&&path[1]==top.slot.name(){direct=Some(entry);break}}
 if let Some(entry)=direct{output.serialize_field(top.slot.name(),&ColumnSerialize{entry,common:self.common,first:self.first})?}else{output.serialize_field(top.slot.name(),&BranchSerialize{data:self.data,top:top.slot,common:self.common,first:self.first})?}
 }output.end()
}}

impl OriginalWorldRuleRowsDataV1{
 fn fork_v1(&self,common:&mut FormalBoundedContextV1<'_>)->Result<Self,BodyMutationError>{self.validate(common)?;let n=self.columns.len().checked_mul(size_of::<Entry>()).and_then(|n|n.checked_add(self.tops.len()*size_of::<TopSlot>()+size_of::<Self>()+2*size_of::<usize>())).ok_or_else(||invalid("WorldRules fork layout"))?;let fee=reserve(common,n)?;let mut columns=Vec::with_capacity(self.columns.len());let mut tops=Vec::with_capacity(self.tops.len());if columns.capacity()!=self.columns.len()||tops.capacity()!=self.tops.len(){return Err(invalid("WorldRules fork capacity"))}for entry in &self.columns{common.consume_steps(1).map_err(paid)?;let content=match &entry.content{Content::Rows(rows)=>Content::Rows(Arc::clone(rows)),Content::Scalar(value)=>Content::Scalar(Arc::clone(value))};columns.push(Entry{column:entry.column,content})}for top in &self.tops{common.consume_steps(1).map_err(paid)?;tops.push(TopSlot{slot:top.slot,emission:top.emission,emitted:top.emitted})}Ok(Self{columns,tops,_fee:fee})}
 fn keyed_patch<K:SemanticKeyV1+serde::Serialize+?Sized>(rows:&OriginalProjectionRowsDataV1,key:&K,after:Option<&[u8]>,common:&mut FormalBoundedContextV1<'_>)->Result<OriginalProjectionRowsDataV1,BodyMutationError>{
 let semantic=super::initial::capture(common,|out|write_semantic_key_v1(key,out).map_err(paid))?;
 if let Some(value)=after{let key_json=super::initial::capture(common,|out|serde_json::to_writer(out,key).map_err(paid))?;let n=value.len().checked_add(size_of::<Vec<u8>>()).ok_or_else(||invalid("WorldRules patch value layout"))?;let fee=reserve(common,n)?;let mut bytes=Vec::with_capacity(value.len());if bytes.capacity()!=value.len(){return Err(invalid("WorldRules patch value capacity"))}for chunk in value.chunks(8192){common.charge(1,chunk.len() as u64).map_err(paid)?;bytes.extend_from_slice(chunk)}let row=OriginalProjectionRowDataV1::from_original_encoded_data_v1(semantic,key_json,fee.retain(bytes),common)?;rows.with_row_data_v1(row,common)}else{rows.without_row_data_v1(&semantic,common)}
 }
 fn apply_path_v1(&mut self,path:&[String],after:Option<&[u8]>,common:&mut FormalBoundedContextV1<'_>)->Result<(),BodyMutationError>{
 self.validate(common)?;let mut matched=None;for(i,entry)in self.columns.iter().enumerate(){let expected=entry.column.path();common.charge(1,(path.len()+expected.len())as u64*64).map_err(paid)?;if path.len()>=expected.len()&&path.iter().zip(expected.iter()).all(|(a,b)|a==b){matched=Some(i);break}}
 let index=matched.ok_or_else(||invalid("WorldRules actual path not inventoried"))?;let column=self.columns[index].column;let base=column.path().len();
 if path.len()==base+1{let Content::Rows(rows)=&self.columns[index].content else{return Err(invalid("WorldRules member source kind"))};let key=&path[base];let next=if column==Column::SupplyShipments{common.charge(1,key.len() as u64).map_err(paid)?;let id=key.parse::<u64>().map_err(|_|invalid("WorldRules original shipment path"))?;Self::keyed_patch(rows,&id,after,common)?}else{Self::keyed_patch(rows,key,after,common)?};self.columns[index].content=Content::Rows(Arc::new(next));return Ok(())}
 if path.len()!=base{return Err(invalid("WorldRules member path depth"))}
 let value=after.ok_or_else(||invalid("WorldRules container unexpectedly absent"))?;
 use oasis7_world_state::incremental_world_rule_projection_delta::WorldRuleLogicalContainerKindV1 as Kind;
 match column.kind(){
 Kind::Map=>{common.charge(1,value.len() as u64).map_err(paid)?;if value!=b"{}"{return Err(invalid("WorldRules nonempty whole map source"))}self.columns[index].content=Content::Rows(Arc::new(OriginalProjectionRowsDataV1::empty_data_v1(common)?));},
 Kind::Scalar=>{let phase=reserve(common,value.len().checked_mul(2).and_then(|n|n.checked_add(32768)).ok_or_else(||invalid("WorldRules scalar patch layout"))?)?;common.charge(value.len()as u64,value.len()as u64).map_err(paid)?;let raw=match serde_json::from_slice::<&serde_json::value::RawValue>(value){Ok(raw)=>raw,Err(error)=>return Err(paid(RuleJsonError{error,_fee:phase}))};common.consume_steps(0).map_err(paid)?;let fragment=super::json_fragment::prepare_original_json_fragment_data_v1(raw,value.len(),common)?;self.columns[index].content=Content::Scalar(fragment);},
 Kind::Set|Kind::Sequence=>{
 let phase=reserve(common,value.len().checked_mul(128).and_then(|n|n.checked_add(32768)).ok_or_else(||invalid("WorldRules sequence patch phase"))?)?;common.charge(value.len()as u64,value.len()as u64).map_err(paid)?;
 let elements=match serde_json::from_slice::<Vec<&serde_json::value::RawValue>>(value){Ok(elements)=>elements,Err(error)=>return Err(paid(RuleJsonError{error,_fee:phase}))};common.consume_steps(0).map_err(paid)?;let mut rows=OriginalProjectionRowsDataV1::empty_data_v1(common)?;
 for(index,element)in elements.iter().enumerate(){common.charge(1,element.get().len()as u64).map_err(paid)?;rows=if column.kind()==Kind::Set{if column==Column::OverflowedWindows{let key=match serde_json::from_str::<(String,u64)>(element.get()){Ok(key)=>key,Err(error)=>return Err(paid(RuleJsonError{error,_fee:phase}))};Self::keyed_patch(&rows,&key,Some(element.get().as_bytes()),common)?}else{let key=match serde_json::from_str::<String>(element.get()){Ok(key)=>key,Err(error)=>return Err(paid(RuleJsonError{error,_fee:phase}))};Self::keyed_patch(&rows,&key,Some(element.get().as_bytes()),common)?}}else{let id=u64::try_from(index).map_err(|_|invalid("WorldRules sequence index"))?;Self::keyed_patch(&rows,&id,Some(element.get().as_bytes()),common)?};}
 self.columns[index].content=Content::Rows(Arc::new(rows));
 }
 }Ok(())
 }
 pub(super) fn apply_actual_delta_v1<M:oasis7_formal_protocol::incremental_state::MerkleNodeReader>(&self,source:&crate::world::incremental_normal_projection_commitment_world_rules::PaidCurrentProjectionWorldRulesDeltaV1,tx:&oasis7_formal_protocol::incremental_execution::ExecutionTransaction<'_,M>,common:&mut FormalBoundedContextV1<'_>,live:&oasis7_formal_protocol::incremental_execution::ExecutionFence)->Result<Self,BodyMutationError>{
 source.validate(tx,common.memory(),common.work().identity(),live)?;let mut after=self.fork_v1(common)?;source.visit(tx,common,live,|path,_actual_semantic,json,common|after.apply_path_v1(path,json,common))?;
 let emission=source.branch_emission();for top in &mut after.tops{common.consume_steps(1).map_err(paid)?;let emitted=match top.slot{Top::IndustrialWork=>emission[0],Top::Substitution=>emission[1],_=>None};if let Some(emitted)=emitted{top.emitted=emitted}}
 source.validate(tx,common.memory(),common.work().identity(),live)?;Ok(after)
 }
}
#[derive(Debug)]struct RuleJsonError{error:serde_json::Error,_fee:MemoryReservation}
impl std::fmt::Display for RuleJsonError{fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result{self.error.fmt(f)}}
impl std::error::Error for RuleJsonError{fn source(&self)->Option<&(dyn std::error::Error+'static)>{Some(&self.error)}}

#[path="incremental_normal_projection_commitment_initial_cold_rules.rs"]mod initial_cold;
