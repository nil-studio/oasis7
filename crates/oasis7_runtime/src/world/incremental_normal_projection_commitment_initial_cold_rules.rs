//! Complete qualified Rules ALL to original forty-two typed logical columns.
//! This is a private DATA constructor: a RootSet, JSON path or supplied empty
//! table cannot invoke it without the actual completed qualified source owner.
use super::*;
use oasis7_world_state::incremental_world_rule_projection_delta::{QualifiedWorldRuleProjectionColumnsV1,QualifiedWorldRuleLogicalRowsVisitorV1,WorldRuleLogicalContainerKindV1 as Kind};
use oasis7_formal_protocol::incremental_execution::RootSetV1;
fn validate_source(source:&QualifiedWorldRuleProjectionColumnsV1,roots:&RootSetV1,common:&mut FormalBoundedContextV1<'_>)->Result<(),BodyMutationError>{
 // Existing source validation hashes the original bounded RootSet encoding.
 // Admit its real envelope plus six fixed namespace temporaries before the
 // shared encoder/hash; this is not a second digest or a copied work bank.
 let fixed=oasis7_formal_protocol::incremental_state::STATE_ROOT_BYTES;
 let encoded=b"oasis7.iws.root-set.v1\0".len().checked_add(4+2+48).and_then(|n|n.checked_add(roots.world_id.len())).and_then(|n|n.checked_add(6usize.checked_mul(fixed)?)).ok_or_else(||invalid("qualified Rules root context layout"))?;
 let heap=encoded.checked_add(size_of::<Vec<u8>>()).and_then(|n|n.checked_add(6usize.checked_mul(fixed+size_of::<Vec<u8>>())?)).ok_or_else(||invalid("qualified Rules root context phase"))?;
 common.charge((encoded as u64).saturating_mul(4).saturating_add(128),(heap as u64).saturating_add(encoded as u64)).map_err(paid)?;let _phase=common.memory().reserve(heap,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;source.validate_context(roots,common.memory())?;common.consume_steps(0).map_err(paid)?;Ok(())
}
struct ColdBuilder<'a,'c>{builder:Builder<'a,'c>,active:Option<(Column,usize,usize)>,declared:usize}
impl ColdBuilder<'_, '_>{
 fn finish_column(&mut self)->Result<(),BodyMutationError>{if let Some((_,expected,seen))=self.active.take(){self.builder.common.consume_steps(1).map_err(paid)?;if expected!=seen{return Err(invalid("qualified Rules logical record count mismatch"))}}Ok(())}
 fn member_count(&mut self,column:Column)->Result<usize,BodyMutationError>{self.builder.common.consume_steps(1).map_err(paid)?;for entry in &self.builder.data.columns{self.builder.common.consume_steps(1).map_err(paid)?;if entry.column==column{return match &entry.content{Content::Rows(rows)=>Ok(rows.len()),Content::Scalar(_)=>Ok(1)}}}Err(invalid("qualified Rules logical column missing"))}
}
impl QualifiedWorldRuleLogicalRowsVisitorV1 for ColdBuilder<'_, '_>{
 type Error=BodyMutationError;
 fn work(&mut self,steps:u64,bytes:usize)->Result<(),Self::Error>{self.builder.common.charge(steps,u64::try_from(bytes).map_err(|_|invalid("qualified Rules work length"))?).map_err(paid)}
 fn top(&mut self,slot:Top,emission:OriginalStateFieldEmissionV1,emitted:bool)->Result<(),Self::Error>{self.finish_column()?;WorldRuleLogicalRowsVisitorV1::top_slot(&mut self.builder,slot,emission)?;if emitted{WorldRuleLogicalRowsVisitorV1::top_emitted(&mut self.builder,slot)?}Ok(())}
 fn column(&mut self,column:Column,projection_records:usize)->Result<(),Self::Error>{
  self.finish_column()?;self.builder.common.consume_steps(1).map_err(paid)?;
  // The qualified ALL owner emits every column scope, even when its real
  // complete enumeration is empty. These temporary empty containers are
  // filled only by that scope's original rows before this builder can finish.
  match column.kind(){
   Kind::Map=>if column==Column::SupplyShipments{WorldRuleLogicalRowsVisitorV1::map(&mut self.builder,column,&BTreeMap::<u64,()>::new())?}else{WorldRuleLogicalRowsVisitorV1::map(&mut self.builder,column,&BTreeMap::<String,()>::new())?},
   Kind::Set=>if column==Column::OverflowedWindows{WorldRuleLogicalRowsVisitorV1::set(&mut self.builder,column,&BTreeSet::<(String,u64)>::new())?}else{WorldRuleLogicalRowsVisitorV1::set(&mut self.builder,column,&BTreeSet::<String>::new())?},
   Kind::Sequence=>WorldRuleLogicalRowsVisitorV1::sequence(&mut self.builder,column,&[]as &[()])?,
   Kind::Scalar=>WorldRuleLogicalRowsVisitorV1::scalar(&mut self.builder,column,&Option::<String>::None)?,
  }
  if column.kind()!=Kind::Map&&projection_records>1{return Err(invalid("qualified Rules whole column has repeated source"))}
  self.active=Some((column,projection_records,0));self.declared=self.declared.checked_add(1).ok_or_else(||invalid("qualified Rules inventory count overflow"))?;Ok(())
 }
 fn row(&mut self,column:Column,path:&[String],_actual_semantic:&[u8],original_json:&[u8])->Result<(),Self::Error>{
  let Some((active,expected,seen))=self.active else{return Err(invalid("qualified Rules row outside logical scope"))};self.builder.common.consume_steps(1).map_err(paid)?;if active!=column||seen>=expected{return Err(invalid("qualified Rules row scope/count mismatch"))}
  let before=self.member_count(column)?;
  // The original delta writer owns typed key reconstruction (including u64
  // shipment keys and tuple set keys), raw JSON admission and row AVL order.
  self.builder.data.apply_path_v1(path,Some(original_json),self.builder.common)?;
  if column.kind()==Kind::Map{self.builder.common.consume_steps(1).map_err(paid)?;let after=self.member_count(column)?;if path.len()==column.path().len()+1{if before.checked_add(1)!=Some(after){return Err(invalid("qualified Rules repeated logical map member"))}}else if before!=0||after!=0||expected!=1{return Err(invalid("qualified Rules empty map scope mixed with members"))}}
  self.active=Some((column,expected,seen.checked_add(1).ok_or_else(||invalid("qualified Rules row count overflow"))?));Ok(())
 }
}
impl OriginalWorldRuleRowsDataV1{
 pub(in crate::world::incremental_normal_projection_commitment) fn from_qualified_v1(source:&QualifiedWorldRuleProjectionColumnsV1,roots:&RootSetV1,common:&mut FormalBoundedContextV1<'_>)->Result<Option<Self>,BodyMutationError>{
  common.consume_steps(1).map_err(paid)?;validate_source(source,roots,common)?;
  if !source.rules_present(){common.consume_steps(0).map_err(paid)?;return Ok(None)}
  let n=Column::ALL.len().checked_mul(size_of::<Entry>()).and_then(|n|n.checked_add(13*size_of::<TopSlot>()+size_of::<Self>()+2*size_of::<usize>())).ok_or_else(||invalid("qualified WorldRules row inventory layout"))?;let fee=reserve(common,n)?;
  let data=Self{columns:Vec::with_capacity(Column::ALL.len()),tops:Vec::with_capacity(13),_fee:fee};if data.columns.capacity()!=Column::ALL.len()||data.tops.capacity()!=13{return Err(invalid("qualified WorldRules inventory capacity"))}
  let mut cold=ColdBuilder{builder:Builder{data,common},active:None,declared:0};source.visit_original_logical_rows_v1(&mut cold)?;cold.finish_column()?;
  cold.builder.common.consume_steps(1).map_err(paid)?;if cold.declared!=Column::ALL.len()||cold.builder.data.columns.len()!=Column::ALL.len()||cold.builder.data.tops.len()!=13{return Err(invalid("qualified WorldRules logical inventory incomplete"))}
  validate_source(source,roots,cold.builder.common)?;cold.builder.data.validate(cold.builder.common)?;Ok(Some(cold.builder.data))
 }
}
