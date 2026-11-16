//! Rebuild each original emitted column from its actual qualified typed rows.
//! No enclosing State is deserialized, reconstructed or serialized.
use super::*;
impl OriginalStateColumnsDataV1{
 pub(in crate::world) fn finalize_qualified_seed_emissions_v1(&mut self,seed:&mut NormalProjectionColumnSeedDataV1,common:&mut FormalBoundedContextV1<'_>,max_json:usize)->Result<(),BodyMutationError>{
 self.validate_context(common)?;seed.validate_context(common)?;seed.begin_qualified_emission_rebuild_v1(common)?;
 for slot in &mut self.slots{
  common.consume_steps(1).map_err(paid)?;
  let count=match &slot.content{None=>0,Some(Content::Map(v))|Some(Content::Set(v))|Some(Content::Sequence(v))=>v.len(),Some(Content::Rules(_))=>1,Some(Content::Fragment(v))=>usize::from(v.as_bytes()!=b"null")};
  slot.emitted=match slot.definition.emission{oasis7_world_state::OriginalStateFieldEmissionV1::Always=>true,oasis7_world_state::OriginalStateFieldEmissionV1::Some|oasis7_world_state::OriginalStateFieldEmissionV1::NonEmpty=>count>0,oasis7_world_state::OriginalStateFieldEmissionV1::OriginalDefault=>return Err(invalid("qualified State original-default needs typed witness"))};
  if !slot.emitted{continue}
  let meter=RefCell::new(&mut *common);let first=RefCell::new(None);
  let json=match &slot.content{
   None=>return Err(invalid("qualified emitted State source missing")),
   Some(Content::Fragment(v))=>super::super::initial::capture_bounded_v1(&mut meter.borrow_mut(),max_json,|out|std::io::Write::write_all(out,v.as_bytes()).map_err(paid))?,
   Some(Content::Rules(v))=>super::super::shared_json::capture_shared_v1(&v.serialization_v1(&meter,&first),&meter,&first,max_json)?,
   Some(Content::Map(v))=>super::super::shared_json::capture_shared_v1(&v.original_map_serialization_v1(&meter,&first),&meter,&first,max_json)?,
   Some(Content::Set(v))=>super::super::shared_json::capture_shared_v1(&v.original_set_serialization_v1(&meter,&first),&meter,&first,max_json)?,
   Some(Content::Sequence(v))=>super::super::shared_json::capture_shared_v1(&v.original_value_sequence_serialization_v1(&meter,&first),&meter,&first,max_json)?,
  };drop(meter);seed.append_qualified_emission_v1(slot.definition.name,json,common)?;
 }
 seed.finish_qualified_emission_rebuild_v1(common)
 }
}
