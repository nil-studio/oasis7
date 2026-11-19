//! Actual Current world-manifest record for governed module transactions. Only
//! this selected two-column record is materialized; no World is reconstructed.
use super::*;
use oasis7_formal_protocol::incremental_value::{ValueNodeReader,ValueReadContext};
use oasis7_world_state::{incremental_field_overlay::WorldFieldOverlayContextV1,incremental_field_read::WorldFieldReadContextV1,incremental_runtime_artifact_config::*};
pub(super) struct CurrentModuleWorldManifestV1{
 version:u64,content:ReservedValue<serde_json::Value>,
 pub(super) body:ReservedValue<ArtifactConfigBodyQualificationV1>,
 guard:PreparedGuard,parent:[u8;32],
}
impl CurrentModuleWorldManifestV1{
 pub(super) fn validate<M:MerkleNodeReader>(&self,tx:&ExecutionTransaction<'_,M>,common:&FormalBoundedContextV1<'_>,live:&ExecutionFence)->Result<(),BodyMutationError>{
  tx.validate_guard(&self.guard,live)?;
  if tx.parent_fence().parent_roots!=self.parent||!self.content.uses_budget(common.memory())||!self.body.uses_budget(common.memory())||!tx.uses_budget(common.memory()){return Err(invalid())}Ok(())
 }
 pub(super) fn validate_in_child_v1<M:MerkleNodeReader>(&self,tx:&ExecutionTransaction<'_,M>,tail:&crate::world::incremental_normal_tail::NormalTailWorkV1<'_, '_>,live:&ExecutionFence)->Result<(),BodyMutationError>{
  tx.validate_guard(&self.guard,live)?;
  if tx.parent_fence().parent_roots!=self.parent||!self.content.uses_budget(tail.memory())||!self.body.uses_budget(tail.memory())||!tx.uses_budget(tail.memory()){return Err(invalid())}Ok(())
 }
 pub(super) fn validate_evidence_v1<M:MerkleNodeReader>(&self,tx:&ExecutionTransaction<'_,M>,memory:&oasis7_proto::memory_budget::MemoryBudget,live:&ExecutionFence)->Result<(),BodyMutationError>{
  tx.validate_transaction_identity(&self.guard)?;tx.validate_guard(&tx.guard(),live)?;
  if tx.parent_fence().parent_roots!=self.parent||!self.content.uses_budget(memory)||!self.body.uses_budget(memory)||!tx.uses_budget(memory){return Err(invalid())}Ok(())
 }
 /// The source remains live while its clone is admitted and constructed. A
 /// clone never transfers the source's own reservation to the new allocation.
 pub(super) fn owned_v1(&self,common:&mut FormalBoundedContextV1<'_>)->Result<(crate::Manifest,MemoryReservation),BodyMutationError>{
  let bytes=self.content.reserved_bytes().checked_add(std::mem::size_of::<crate::Manifest>()).ok_or_else(invalid)?;
  pay(common,bytes)?;
  let fee=common.memory().reserve(bytes,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
  Ok((crate::Manifest{version:self.version,content:(*self.content).clone()},fee))
 }
}
pub(super) fn read_current_module_world_manifest_v1<M,P,R,V,E,C,PE,PC>(sources:&ModuleLifecycleSourcesV1<'_>,
 ctx:&mut WorldFieldOverlayContextV1<'_, '_, '_,M,R,E,C>,parent:&mut WorldFieldReadContextV1<'_,P,V,PE,PC>,
 factory:&mut FactoryScheduleWorkV1,module:&mut ModuleLifecycleWorkV1<'_>,live:&ExecutionFence,
)->Result<CurrentModuleWorldManifestV1,BodyMutationError>
where M:MerkleNodeReader,P:MerkleNodeReader,R:ValueNodeReader,V:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool,PE:FnMut()->Option<ValueTreeError>,PC:FnMut()->bool{
 ctx.transaction.validate_guard(&ctx.transaction.guard(),live)?;
 let body=open_artifact_config_body_overlay_v1(ArtifactConfigSelectedBodyRequestV1{field:ArtifactConfigFieldV1::Manifest,key:ArtifactConfigRecordKeyV1::Global,family:sources.families.config,parent_roots:sources.families.roots,current:None},parent.state,ctx,&mut factory.config_scan,&mut factory.config).map_err(overlay_error)?.ok_or_else(invalid)?;
 let guard=ctx.transaction.guard();let value=read_qualified_artifact_config_overlay_v1(ArtifactConfigCellRequestV1{field:ArtifactConfigFieldV1::Manifest,key:ArtifactConfigRecordKeyV1::Global,address:ArtifactConfigAddressV1{section:"manifest",ordinal:None,inner_ordinal:None,member:None,column:"version"},family:sources.families.config,qualification:&body,max_payload_bytes:64},ctx,&guard,live,&mut std::io::sink()).map_err(overlay_error)?;
 let ArtifactConfigValueV1::Unsigned(version)=*value else{return Err(invalid())};
 let content={
  let mut read=WorldFieldReadContextV1{root:body.root(),state:parent.state,values:ValueReadContext{source:ctx.values.source,memory:ctx.values.memory,class:ctx.values.class,io:&mut*ctx.values.io},key_work:&mut*ctx.key_work,semantic_bytes:ctx.semantic_bytes,record_bytes:ctx.record_bytes,take_state_error:&mut ctx.take_state_error,cancelled:&mut ctx.cancelled};
  let json=open_artifact_config_json_v1(&body,ArtifactConfigAddressV1{section:"manifest",ordinal:None,inner_ordinal:None,member:None,column:"content"},&mut read,&mut factory.config.json)?;
  materialize_artifact_config_json_v1(&json,&mut read,&mut factory.config.json,module.max_selected_heap)?
 };
 ctx.transaction.validate_guard(&ctx.transaction.guard(),live)?;
 Ok(CurrentModuleWorldManifestV1{version,content,body,guard:ctx.transaction.guard(),parent:ctx.transaction.parent_fence().parent_roots})
}
