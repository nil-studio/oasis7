//! Original GEN side JSON borrowed from the same immutable projection capture.
//! This returns DATA for the actual package writer, never history authority.
use super::*;
impl crate::world::incremental_genesis_projection::RuntimeProjectedGenesisV1{
 pub fn retained_history_root_json_bounded_v1(&self,common:&mut FormalBoundedContextV1<'_>)->Result<ReservedValue<Vec<u8>>,BodyMutationError>{
  self.validate_context_bounded_v1(common)?;let mut selected=None;
  for field in self.columns().fields(){common.consume_steps(1).map_err(paid)?;if field.descriptor().owner==oasis7_world_state::incremental_projection::FieldOwner::WorldState&&field.descriptor().name=="retained_history_root"{if selected.is_some()||field.rows().len()!=1{return Err(invalid("GEN projection side column uniqueness"))}selected=Some(field.rows()[0].original_json());}}
  let bytes=selected.ok_or_else(||invalid("GEN actual retained-history side missing"))?;
  if bytes.is_empty()||bytes.len()>oasis7_formal_protocol::incremental_projection_sidecar::PROJECTION_SIDECAR_MAX_JSON_BYTES{return Err(ValueTreeError::ResourceLimited("GEN projection side JSON bound").into())}
  let n=bytes.len().checked_add(std::mem::size_of::<Vec<u8>>()).ok_or_else(||invalid("GEN projection side output layout"))?;common.charge(1,n as u64).map_err(paid)?;let fee=common.memory().reserve(n,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
  let mut output=Vec::with_capacity(bytes.len());if output.capacity()!=bytes.len(){return Err(invalid("GEN projection side output capacity"))}output.extend_from_slice(bytes);Ok(fee.retain(output))
 }
}
