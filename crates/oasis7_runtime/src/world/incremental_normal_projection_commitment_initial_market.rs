//! Fixed Cold marketplace bridge. Only an actual qualified ALL source enters;
//! no WorldState, pending-write forgery or logical-Vec index reconstruction.
use super::*;
use oasis7_formal_protocol::{incremental_execution::{ExecutionTransaction,ExecutionFence},incremental_state::MerkleNodeReader};
use oasis7_world_state::incremental_module_marketplace::ModuleMarketplaceFieldV1 as Kind;
use std::cell::RefCell;
struct Encoded{key:ReservedValue<String>,json:ReservedValue<Vec<u8>>}
#[derive(Debug)]struct RawCause{error:serde_json::Error,_fee:MemoryReservation}
impl std::fmt::Display for RawCause{fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result{self.error.fmt(f)}}impl std::error::Error for RawCause{}
impl NormalProjectionColumnInitializerV1<'_, '_>{
 pub(in crate::world) fn capture_qualified_marketplace_columns_v1<M:MerkleNodeReader>(&mut self,source:&oasis7_world_state::incremental_owner_closure::QualifiedMarketplaceProjectionColumnsV1,tx:&ExecutionTransaction<'_,M>,live:&ExecutionFence,max_json:usize)->Result<(),BodyMutationError>{
  if self.marketplace_index.is_some(){return Err(invalid("duplicate actual marketplace index initialization"))}
  source.validate(tx,self.common.memory(),live)?;
  let columns=super::super::market_fold::QualifiedMarketplaceColumnDataV1::from_actual_qualified_v1(source,tx,self.common,live)?;
  let index=super::super::market_index::ProjectionMarketplaceIndexSeedDataV1::from_actual_qualified_v1(source,&columns,tx,self.common,live)?;
  for kind in Kind::ALL{
   let rows=columns.rows_v1(kind,self.common)?;let count=rows.len();let bytes=count.checked_mul(size_of::<Encoded>()).ok_or_else(||invalid("Cold marketplace row staging"))?;let _phase=reserve(self.common,bytes)?;let mut values=Vec::with_capacity(count);if values.capacity()!=count{return Err(invalid("Cold marketplace row capacity"))}
   rows.visit_data_v1(self.common,&mut|row,common|{
    if values.len()==count{return Err(invalid("Cold marketplace row count"))}let key=row.original_key_value().as_str().ok_or_else(||invalid("Cold marketplace outer typed key"))?;
    let fee=reserve(common,key.len().checked_add(size_of::<String>()).ok_or_else(||invalid("Cold marketplace key layout"))?)?;let mut text=String::with_capacity(key.len());if text.capacity()!=key.len(){return Err(invalid("Cold marketplace key capacity"))}let mut at=0;while at<key.len(){let mut end=at.saturating_add(8192).min(key.len());while !key.is_char_boundary(end){end-=1}common.charge(1,(end-at)as u64).map_err(paid)?;text.push_str(&key[at..end]);at=end;}
    let meter=RefCell::new(&mut*common);let first=RefCell::new(None);
    let json=if kind==Kind::ArtifactBids{let record=row.market_bids_v1().ok_or_else(||invalid("Cold actual bid index DATA missing"))?;let view=record.serialization_v1(&meter,&first);super::super::shared_json::capture_shared_v1(&view,&meter,&first,max_json)?}else{let record=row.market_record_v1().ok_or_else(||invalid("Cold actual marketplace fixed DATA missing"))?;let view=record.serialization_v1(&meter,&first);super::super::shared_json::capture_shared_v1(&view,&meter,&first,max_json)?};drop(meter);
    values.push(Encoded{key:fee.retain(text),json});Ok(())
   })?;
   let field=kind.descriptor().map_err(paid)?;self.begin(field,Some(count))?;
   for value in values{self.common.charge(value.json.len()as u64,value.json.len()as u64).map_err(paid)?;let error_fee=reserve(self.common,1024)?;let decoded=serde_json::from_slice::<&serde_json::value::RawValue>(&value.json);self.common.consume_steps(0).map_err(paid)?;let decoded=match decoded{Ok(value)=>value,Err(error)=>return Err(paid(RawCause{error,_fee:error_fee}))};self.row(field,&*value.key,decoded)?;}
   self.end(field)?;
  }
  source.validate(tx,self.common.memory(),live)?;self.marketplace_index=Some(index);Ok(())
 }
}
