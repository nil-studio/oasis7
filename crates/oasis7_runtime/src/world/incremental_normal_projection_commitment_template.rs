//! One-time static default schema DATA, never actual execution State.
use super::*;
use oasis7_world_projection::current_commitment::{ProjectionColumnDataV1,ProjectionColumnErrorV1};
pub(in crate::world) struct StaticProjectionSchemaDataV1{value:ProjectionColumnDataV1}
impl StaticProjectionSchemaDataV1{
 pub(in crate::world) fn data(&self)->&ProjectionColumnDataV1{&self.value}
 pub(super) fn share_data_v1(&self,common:&mut FormalBoundedContextV1<'_>)->Result<Self,BodyMutationError>{let memory=common.memory().clone();let value=self.value.share_data(&memory,&mut|s,b|common.charge(s,b)).map_err(err)?;Ok(Self{value})}
 pub(in crate::world) fn uses_budget(&self,memory:&MemoryBudget)->bool{self.value.uses_budget(memory)}
}
fn err<E:std::error::Error+Send+Sync+'static>(e:ProjectionColumnErrorV1<E>)->BodyMutationError{match e{ProjectionColumnErrorV1::Work(e)=>paid(e),ProjectionColumnErrorV1::Memory(e)=>ValueTreeError::Memory(e).into(),ProjectionColumnErrorV1::Invalid(e)=>invalid(e)}}
/// GEN/cold only. Default is the original fixed, input-independent schema:
/// empty containers, one World ledger and the original static config defaults.
/// The admitted 1MiB phase conservatively covers that closed default graph;
/// no actual World/State, accepted root or execution source is reconstructed.
pub(in crate::world) fn build_static_projection_schema_data_v1(common:&mut FormalBoundedContextV1<'_>)->Result<StaticProjectionSchemaDataV1,BodyMutationError>{
 common.charge(256,1024*1024).map_err(paid)?;let _phase=common.memory().reserve(1024*1024,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
 let state=oasis7_world_state::WorldState::default();let mut visitor=NormalProjectionColumnInitializerV1::new(common)?;visitor.capture_original_state_fields_v1(&state)?;let seed=visitor.finish()?;drop(state);
 let memory=common.memory().clone();let mut value=ProjectionColumnDataV1::from_json_data(serde_json::Value::Object(Default::default()),&memory,&mut|s,b|common.charge(s,b)).map_err(err)?;
 for field in seed.original_state_emissions(){value=value.with_object_member_data(field.name(),Some(field.value()),&memory,&mut|s,b|common.charge(s,b)).map_err(err)?}
 Ok(StaticProjectionSchemaDataV1{value})
}
