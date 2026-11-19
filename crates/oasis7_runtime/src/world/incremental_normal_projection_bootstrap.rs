//! Actual accepted GEN/cold initialization; no arbitrary column/root constructor.
use crate::world::incremental_body::BodyMutationError;
use oasis7_formal_protocol::{controlled_authority::formal::bounded_context::{FormalBoundedContextV1,WorkIdentityV1},incremental_value::ValueTreeError};
use oasis7_proto::memory_budget::{MemoryBudget,MemoryClass,MemoryReservation,ReservedValue};
fn invalid(note:&'static str)->BodyMutationError{BodyMutationError::Invalid(note)}
fn paid(e:impl std::error::Error+Send+Sync+'static)->BodyMutationError{BodyMutationError::Original(Box::new(e))}
#[path="incremental_normal_projection_bootstrap_witness.rs"]mod witness;
pub(in crate::world) use witness::ProjectionGenesisWitnessV1;

use crate::world::incremental_normal_projection_commitment::NormalProjectionColumnSeedDataV1;
use oasis7_formal_protocol::incremental_projection_sidecar::PaidVerifiedProjectionSidecarV1;
/// Concrete physical bootstrap proof plus original side DATA. The private
/// cache constructor still computes and compares the signed projection root;
/// consuming this type alone is not permission to publish or read history.
pub struct BoundNormalProjectionBootstrapSourceV1{
 columns:NormalProjectionColumnSeedDataV1,
 record:[u8;32],body:[u8;32],roots:[u8;32],
 fees:BootstrapProjectionSourceFeesV1,
}
enum BootstrapProjectionPhysicalSourceV1{Genesis{witness:ProjectionGenesisWitnessV1,outer:MemoryReservation},Cold(cold::ColdBootstrapProjectionFeesV1)}
pub(in crate::world) struct BootstrapProjectionSourceFeesV1{
 physical:BootstrapProjectionPhysicalSourceV1,
 side:PaidVerifiedProjectionSidecarV1,
 expected_projection_root:ReservedValue<String>,
 expected_genesis_raw_state_hash:Option<ReservedValue<String>>,
 core:WorkIdentityV1,memory:MemoryBudget,_fee:MemoryReservation,
}
impl BootstrapProjectionSourceFeesV1{
 pub(in crate::world) fn expected_projection_root(&self)->&str{&self.expected_projection_root}
 pub(in crate::world) fn expected_genesis_raw_state_hash(&self)->Option<&str>{self.expected_genesis_raw_state_hash.as_deref().map(String::as_str)}
 pub(in crate::world) fn validate_context(&self,common:&mut FormalBoundedContextV1<'_>)->Result<(),BodyMutationError>{common.consume_steps(1).map_err(paid)?;if self.core!=common.work().identity()||!self.memory.same_pool(common.memory())||!self._fee.uses_budget(common.memory())||!self.expected_projection_root.uses_budget(common.memory())||self.expected_genesis_raw_state_hash.as_ref().is_some_and(|v|!v.uses_budget(common.memory())){return Err(invalid("bootstrap projection source fees"))}match &self.physical{BootstrapProjectionPhysicalSourceV1::Genesis{witness,outer}=>{witness.validate_context(common)?;if !outer.uses_budget(common.memory()){return Err(invalid("GEN projection original wrapper fee"))}},BootstrapProjectionPhysicalSourceV1::Cold(source)=>source.validate_context(common)?}Ok(())}
}
impl BoundNormalProjectionBootstrapSourceV1{
 pub(in crate::world) fn validate_context(&self,common:&mut FormalBoundedContextV1<'_>)->Result<(),BodyMutationError>{self.fees.validate_context(common)?;self.columns.validate_context(common)?;common.charge(1,96).map_err(paid)?;if self.side_record()!=self.record||self.fees.side.body_id()!=self.body||self.fees.side.roots_id()!=self.roots{return Err(invalid("bootstrap projection side/context mismatch"))}Ok(())}
 fn side_record(&self)->[u8;32]{self.fees.side.record_id()}
 pub(in crate::world) fn into_parts_v1(self)->(NormalProjectionColumnSeedDataV1,[u8;32],[u8;32],[u8;32],BootstrapProjectionSourceFeesV1){(self.columns,self.record,self.body,self.roots,self.fees)}
}
fn copy_text(value:&str,common:&mut FormalBoundedContextV1<'_>)->Result<ReservedValue<String>,BodyMutationError>{let n=value.len().checked_add(std::mem::size_of::<String>()).ok_or_else(||invalid("bootstrap projection text layout"))?;common.charge(1,n as u64).map_err(paid)?;let fee=common.memory().reserve(n,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;Ok(fee.retain(value.to_owned()))}
impl crate::world::incremental_genesis_projection::RuntimeProjectedGenesisV1{
 /// Consume only the original immutable GEN output. All emitted typed/raw
 /// records are compared with actual accepted parent points before moving any
 /// columns; no root tuple or arbitrary seed constructor is offered.
 pub fn bind_accepted_genesis_projection_source_v1<M:oasis7_formal_protocol::incremental_state::MerkleNodeReader>(self,parent:&crate::world::incremental_normal_sources::BoundNormalTickParentV1<'_>,tx:&mut oasis7_formal_protocol::incremental_execution::ExecutionTransaction<'_,M>,side:PaidVerifiedProjectionSidecarV1,common:&mut FormalBoundedContextV1<'_>,live:&oasis7_formal_protocol::incremental_execution::ExecutionFence)->Result<(BoundNormalProjectionBootstrapSourceV1,crate::world::incremental_genesis::RuntimeGenesisSummaryV1,crate::world::incremental_normal_business_commitment::PreparedNormalBusinessMetadataGenesisV1),BodyMutationError>{
  self.validate_context_bounded_v1(common)?;
  let refs=parent.refs();side.validate_context(refs.head,refs.core,common).map_err(paid)?;
  let n=std::mem::size_of::<BoundNormalProjectionBootstrapSourceV1>()+std::mem::size_of::<BootstrapProjectionSourceFeesV1>();common.charge(1,n as u64).map_err(paid)?;let fee=common.memory().reserve(n,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
  let identity=self.business_metadata().identity();let world=&refs.head.record().world;common.charge(3,(identity.0.len()+identity.1.len()+identity.2.map_or(0,str::len)+world.world_id.len()+world.chain_id.len()+world.genesis_digest.len())as u64).map_err(paid)?;if identity.0!=world.world_id||identity.1!=world.chain_id||identity.2!=Some(world.genesis_digest.as_str()){return Err(invalid("GEN projection actual World identity"))}
  let json=side.retained_history_root_json();let graph=json.len().checked_mul(128).and_then(|n|n.checked_add(32768)).ok_or_else(||invalid("projection side typed graph"))?;common.charge(1,graph as u64).map_err(paid)?;let _graph=common.memory().reserve(graph,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
  // Shape only, never FormalRetainedHistoryRoot::validate/History authority.
  let _:Option<oasis7_world_state::formal_retained_history::FormalRetainedHistoryRootV1>=serde_json::from_slice(json).map_err(paid)?;
  let mut selected=None;for field in self.columns().fields(){common.consume_steps(1).map_err(paid)?;if field.descriptor().owner==oasis7_world_state::incremental_projection::FieldOwner::WorldState&&field.descriptor().name=="retained_history_root"{if selected.is_some()||field.rows().len()!=1{return Err(invalid("GEN projection side column uniqueness"))}selected=Some(field.rows()[0].original_json());}}
  let expected=selected.ok_or_else(||invalid("GEN projection actual side column missing"))?;common.charge(1,(expected.len()+json.len())as u64).map_err(paid)?;if expected!=json{return Err(invalid("GEN projection original side differs from material"))}
  self.witness_for_bootstrap_v1().verify_actual_genesis_v1(parent,tx,common,live)?;
  let expected_projection_root=copy_text(&refs.head.record().projection_root,common)?;let raw=copy_text(self.business_metadata().raw_state_hash_for_bootstrap_v1(),common)?;
  let record=refs.head.verified_record_id();let body=refs.core.catalog().entry.commit;let roots=refs.core.body().roots;
  let(genesis,columns,metadata,witness,outer)=self.into_bootstrap_parts_v1();
  let fees=BootstrapProjectionSourceFeesV1{physical:BootstrapProjectionPhysicalSourceV1::Genesis{witness,outer},side,expected_projection_root,expected_genesis_raw_state_hash:Some(raw),core:common.work().identity(),memory:common.memory().clone(),_fee:fee};
  Ok((BoundNormalProjectionBootstrapSourceV1{columns,record,body,roots,fees},genesis,metadata))
 }
}

#[path="incremental_normal_projection_bootstrap_genesis_side.rs"]
mod genesis_side;

#[path="incremental_normal_projection_bootstrap_cold.rs"]
mod cold;
pub(in crate::world) use cold::bind_qualified_cold_projection_source_v1;

#[path = "incremental_normal_projection_bootstrap_capture.rs"]
mod capture;
