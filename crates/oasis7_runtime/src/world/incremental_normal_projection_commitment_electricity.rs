//! Actual Agent resource fact index. This is readonly DATA, never an Agent substitute.
use super::*;
use super::original_rows::{OriginalProjectionRowDataV1,OriginalProjectionRowsDataV1};
use std::{cell::RefCell,mem::size_of};
pub(super) fn bytes(data:&[u8],common:&mut FormalBoundedContextV1<'_>)->Result<ReservedValue<Vec<u8>>,BodyMutationError>{
 let n=size_of::<Vec<u8>>().checked_add(data.len()).ok_or_else(||invalid("electricity bytes layout"))?;common.charge(1,n as u64).map_err(paid)?;let fee=common.memory().reserve(n,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
 let mut out=Vec::with_capacity(data.len());if out.capacity()!=data.len(){return Err(invalid("electricity bytes capacity"))}for chunk in data.chunks(8192){common.charge(1,chunk.len() as u64).map_err(paid)?;out.extend_from_slice(chunk)}Ok(fee.retain(out))
}
fn put(rows:&OriginalProjectionRowsDataV1,semantic:&[u8],key:&[u8],value:i64,common:&mut FormalBoundedContextV1<'_>)->Result<OriginalProjectionRowsDataV1,BodyMutationError>{
 // Decimal i64 JSON is at most 20 bytes. Reserve before the original encoder.
 common.charge(1,20).map_err(paid)?;let fee=common.memory().reserve(size_of::<Vec<u8>>()+20,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;let mut encoded=Vec::with_capacity(20);if encoded.capacity()!=20{return Err(invalid("electricity JSON capacity"))}
 serde_json::to_writer(&mut encoded,&value).map_err(paid)?;common.consume_steps(0).map_err(paid)?;let encoded=fee.retain(encoded);
 let row=OriginalProjectionRowDataV1::from_original_encoded_data_v1(bytes(semantic,common)?,bytes(key,common)?,encoded,common)?;rows.with_row_data_v1(row,common)
}
pub(super) fn from_seed_v1(seed:&NormalProjectionColumnSeedDataV1,common:&mut FormalBoundedContextV1<'_>)->Result<OriginalProjectionRowsDataV1,BodyMutationError>{
 seed.validate_context(common)?;let mut out=OriginalProjectionRowsDataV1::empty_data_v1(common)?;
 for field in seed.fields(){common.consume_steps(1).map_err(paid)?;if field.descriptor().owner!=FieldOwner::WorldState||field.descriptor().name!="agents"{continue}for row in field.rows(){common.consume_steps(1).map_err(paid)?;let value=row.agent_electricity().ok_or_else(||invalid("actual Agent electricity source missing"))?;out=put(&out,row.semantic(),row.original_key_json(),value,common)?}}
 Ok(out)
}
pub(super) fn apply_delta_v1(before:&OriginalProjectionRowsDataV1,source:&oasis7_world_state::incremental_agent_instance_projection_delta::CurrentAgentInstanceProjectionDeltaV1,common:&mut FormalBoundedContextV1<'_>)->Result<OriginalProjectionRowsDataV1,BodyMutationError>{
 let mut out=before.share_data_v1(common)?;let shared=RefCell::new(common);
 source.visit_agent_electricity_with_work_v1(|steps,bytes|shared.borrow_mut().charge(steps,bytes).map_err(paid),|semantic,key,value|{let mut common=shared.borrow_mut();out=match value{Some(value)=>put(&out,semantic,key,value,&mut common)?,None=>out.without_row_data_v1(semantic,&mut common)?};Ok::<_,BodyMutationError>(())})?;Ok(out)
}
