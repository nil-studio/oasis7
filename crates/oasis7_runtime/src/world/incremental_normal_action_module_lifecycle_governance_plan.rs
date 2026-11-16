//! Actual Current manifest update prepared for the fixed module governance
//! transaction. This is selected-record DATA, not governance permission.
use super::*;
use super::governance_manifest::CurrentModuleWorldManifestV1;
use super::governance_json::{module_json_value_paid_v1,ModuleGovernanceJsonErrorV1};
use std::fmt::Write as _;
pub(super) struct PreparedModuleManifestUpdateV1{
 pub(super) before:CurrentModuleWorldManifestV1,pub(super) manifest:crate::Manifest,
 pub(super) base_hash:ReservedValue<String>,pub(super) clone_fee:MemoryReservation,pub(super) changes_fee:MemoryReservation,
}
pub(super) enum ModuleManifestUpdateDecisionV1{
 Ready(PreparedModuleManifestUpdateV1),Rejected{reason:crate::RejectReason,_fee:MemoryReservation},
}
struct ReasonV1{value:String,limit:usize}
impl std::fmt::Write for ReasonV1{fn write_str(&mut self,s:&str)->std::fmt::Result{if self.value.len().checked_add(s.len()).is_none_or(|n|n>self.limit){return Err(std::fmt::Error)}self.value.push_str(s);Ok(())}}
pub(super) fn prepare_module_manifest_update_v1<M:MerkleNodeReader>(before:CurrentModuleWorldManifestV1,changes:&crate::ModuleChangeSet,tx:&ExecutionTransaction<'_,M>,common:&mut FormalBoundedContextV1<'_>,live:&ExecutionFence)->Result<ModuleManifestUpdateDecisionV1,BodyMutationError>{prepare_update(before,changes,UpdateKindV1::Module,tx,common,live)}
pub(super) fn prepare_module_rollback_manifest_update_v1<M:MerkleNodeReader>(before:CurrentModuleWorldManifestV1,changes:&crate::ModuleChangeSet,tx:&ExecutionTransaction<'_,M>,common:&mut FormalBoundedContextV1<'_>,live:&ExecutionFence)->Result<ModuleManifestUpdateDecisionV1,BodyMutationError>{prepare_update(before,changes,UpdateKindV1::Rollback,tx,common,live)}
enum UpdateKindV1{Module,Rollback}
fn prepare_update<M:MerkleNodeReader>(before:CurrentModuleWorldManifestV1,changes:&crate::ModuleChangeSet,kind:UpdateKindV1,tx:&ExecutionTransaction<'_,M>,common:&mut FormalBoundedContextV1<'_>,live:&ExecutionFence)->Result<ModuleManifestUpdateDecisionV1,BodyMutationError>{
 before.validate(tx,common,live)?;
 // The current hash is of the original Manifest record, not an IWS root or a
 // whole World. The update below preserves all unmodified content fields.
 let (mut manifest,clone_fee)=before.owned_v1(common)?;
 let base_hash=crate::world::incremental_normal_action_diplomacy::fixed_json_digest_v1(&manifest,common)?;
 let changes=match module_json_value_paid_v1(changes,common){
  Ok(value)=>value,
  Err(ModuleGovernanceJsonErrorV1::Body(e))=>return Err(e),
  Err(ModuleGovernanceJsonErrorV1::Json{error,_fee:error_fee})=>{
   pay(common,8192)?;let fee=common.memory().reserve(8192,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
   let mut reason=ReasonV1{value:String::with_capacity(4096),limit:4096};write!(&mut reason,"{}{error}",match kind{UpdateKindV1::Module=>"serialize module changes failed: ",UpdateKindV1::Rollback=>"serialize module rollback changes failed: "}).map_err(|_|invalid())?;
   drop(error);drop(error_fee);
   return Ok(ModuleManifestUpdateDecisionV1::Rejected{reason:crate::RejectReason::RuleDenied{notes:vec![reason.value]},_fee:fee});
  },
 };
 manifest.version=manifest.version.saturating_add(1);
 let serde_json::Value::Object(content)=&mut manifest.content else{
  pay(common,4096)?;let fee=common.memory().reserve(4096,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
  return Ok(ModuleManifestUpdateDecisionV1::Rejected{reason:crate::RejectReason::RuleDenied{notes:vec!["current manifest content must be object".to_owned()]},_fee:fee})
 };
 // Admit the new map key and at most one split path before insertion. The
 // original complete content clone and new change graph retain separate fees.
 let n=content.len().checked_add(1).ok_or_else(invalid)?;
 let levels=usize::try_from(usize::BITS-n.leading_zeros()+2).map_err(|_|invalid())?;
 let extra_bytes=oasis7_world_state::incremental_collection_layout::json_object_output_heap_v1(1)?.checked_mul(levels).and_then(|n|n.checked_add(4096)).ok_or_else(invalid)?;
 pay(common,extra_bytes)?;
 let mut extra=common.memory().reserve(extra_bytes,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
 let mut changes_fee=changes.fee;changes_fee.merge(&mut extra).map_err(BodyMutationError::Invalid)?;
 content.insert("module_changes".to_owned(),changes.value);
 Ok(ModuleManifestUpdateDecisionV1::Ready(PreparedModuleManifestUpdateV1{before,manifest,base_hash,clone_fee,changes_fee}))
}
