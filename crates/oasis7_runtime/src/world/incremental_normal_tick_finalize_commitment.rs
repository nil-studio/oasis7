//! Genuine IWS Current cut, in an explicitly separate digest space from legacy
//! World JSON state roots. This opaque DATA owner cannot authorize execution,
//! Node finality, a completed tick, a receipt or an accepted cache.
use crate::world::{incremental_body::BodyMutationError,incremental_body_event::AgentEventOriginalV1,incremental_module_invocation::*,incremental_normal_sources::BoundNormalTickParentV1};
use crate::world::incremental_module_execution_manifest_hash::{CurrentWorldManifestHashV1,prepare_current_world_manifest_hash_v1};
use oasis7_formal_protocol::{controlled_authority::formal::{bounded_context::{FormalBoundedContextV1,WorkIdentityV1},context_wire::encode_paid},incremental_execution::{CurrentRootsPreviewV1,CurrentRootsPreviewErrorV1,ExecutionFence,ExecutionTransaction},incremental_material::OriginalObjectStreamReader,incremental_state::{MerkleNodeReader,StatePageWriter,StateNamespaceV1},incremental_value::{ValueNodeReader,ValueNodeWriter,ValueTreeError}};
use oasis7_proto::memory_budget::{ReservedValue,MemoryClass};
use oasis7_consensus::normal_tick_input::PaidBoundNormalInputDataV1;
use sha2::{Digest,Sha256};
use oasis7_world_state::incremental_iws_tick_consensus::{IwsCurrentCommitmentFrameV1,IwsNodeCommitContextV1};
pub(in crate::world) use oasis7_world_state::incremental_iws_tick_consensus::IWS_CURRENT_COMMITMENT_DOMAIN_V1;
fn preview_error(error:CurrentRootsPreviewErrorV1)->BodyMutationError{match error{CurrentRootsPreviewErrorV1::Execution(error)=>error.into(),CurrentRootsPreviewErrorV1::Common(error)=>BodyMutationError::Original(Box::new(error))}}
pub(in crate::world) struct PaidIwsCurrentCommitmentV1{
 preview:ReservedValue<CurrentRootsPreviewV1>,manifest:ReservedValue<CurrentWorldManifestHashV1>,
 frame:ReservedValue<IwsCurrentCommitmentFrameV1>,wire:ReservedValue<Vec<u8>>,digest:ReservedValue<[u8;32]>,
 parent_record:[u8;32],input_material:[u8;32],identity:WorkIdentityV1,
}
impl PaidIwsCurrentCommitmentV1{
 pub(in crate::world) fn frame(&self)->&IwsCurrentCommitmentFrameV1{&self.frame}
 pub(in crate::world) fn digest(&self)->&[u8;32]{&self.digest}
 pub(in crate::world) fn original(&self)->&[u8]{&self.wire}
 pub(in crate::world) fn roots(&self)->&oasis7_formal_protocol::incremental_execution::RootSetV1{self.preview.roots()}
 pub(in crate::world) fn manifest_hash(&self)->&str{self.manifest.hash()}
 pub(in crate::world) fn validate<M:MerkleNodeReader>(&self,tx:&ExecutionTransaction<'_,M>,parent:&ReservedValue<BoundNormalTickParentV1<'_>>,input:&PaidBoundNormalInputDataV1<'_>,common:&mut FormalBoundedContextV1<'_>,live:&ExecutionFence)->Result<(),BodyMutationError>{
  parent.validate_context(common).map_err(|e|BodyMutationError::Original(Box::new(e)))?;input.validate_context(common).map_err(|e|BodyMutationError::Original(Box::new(e)))?;
  self.preview.validate(tx,common,live).map_err(preview_error)?;common.charge(1,128).map_err(|e|BodyMutationError::Original(Box::new(e)))?;
  if self.identity!=common.work().identity()||!self.preview.uses_budget(common.memory())||!self.manifest.uses_budget(common.memory())||!self.frame.uses_budget(common.memory())||!self.wire.uses_budget(common.memory())||!self.digest.uses_budget(common.memory())||parent.refs().core.candidate_record_id()!=self.parent_record||input.checked().parent_record_id()!=self.parent_record||input.checked().material_id()!=self.input_material||tx.parent_fence()!=parent.fence(){return Err(BodyMutationError::Invalid("IWS Current commitment source"));}Ok(())
 }
}
impl<'a,'t,'s,'v,M,R,E,C,W,F,O> RootModuleExecutionContextV1<'a,'t,'s,'v,M,R,E,C,W,F,O>
where M:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool,W:ValueNodeReader+ValueNodeWriter+StatePageWriter,F:FnMut(&mut dyn AgentEventOriginalV1)->Result<(),BodyMutationError>,O:OriginalObjectStreamReader{
 pub(in crate::world) fn prepare_iws_current_commitment_v1(&mut self,parent:&ReservedValue<BoundNormalTickParentV1<'_>>,input:&PaidBoundNormalInputDataV1<'_>,common:&mut FormalBoundedContextV1<'_>,live_owner:&mut dyn FnMut()->ExecutionFence)->Result<ReservedValue<PaidIwsCurrentCommitmentV1>,RootModuleExecutionErrorV1<O::Error>>{
 let result=(||{
  parent.validate_context(common).map_err(|e|RootModuleExecutionErrorV1::Body(BodyMutationError::Original(Box::new(e))))?;input.validate_context(common).map_err(|e|RootModuleExecutionErrorV1::Body(BodyMutationError::Original(Box::new(e))))?;
  if input.checked().parent_record_id()!=parent.refs().core.candidate_record_id(){return Err(RootModuleExecutionErrorV1::Body(BodyMutationError::Invalid("IWS input actual parent")))}
  common.charge(1,8192).map_err(|e|RootModuleExecutionErrorV1::Body(BodyMutationError::Original(Box::new(e))))?;
  let owner=common.memory().reserve(std::mem::size_of::<PaidIwsCurrentCommitmentV1>(),MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
  // Complete all Current reads before previewing its strict read-set cut.
  with_current_root_module_meter_v1(self,common,live_owner,|root,_|{
   let clock=oasis7_world_state::incremental_world_u64::read_world_u64_overlay_v1(oasis7_world_state::incremental_world_u64::WorldU64FieldV1::Time,root.ctx).map_err(|e|RootModuleExecutionErrorV1::Body(match e{oasis7_world_state::incremental_field_overlay::WorldFieldOverlayErrorV1::Execution(e)=>e.into(),oasis7_world_state::incremental_field_overlay::WorldFieldOverlayErrorV1::Value(e)=>e.into(),oasis7_world_state::incremental_field_overlay::WorldFieldOverlayErrorV1::Member(e)=>BodyMutationError::Original(Box::new(e))}))?;
   if clock!=root.now{return Err(RootModuleExecutionErrorV1::Body(BodyMutationError::Invalid("IWS commitment actual clock")))}Ok(())
  })?;
  let manifest=with_current_root_module_meter_v1(self,common,live_owner,|root,_|prepare_current_world_manifest_hash_v1(root.families.parent,root.families.head,root.families.roots,root.manifest_cache,None,root.families.config,root.state,root.ctx,root.manifest_work).map_err(RootModuleExecutionErrorV1::Manifest))?;
  let live=live_owner();let preview=self.ctx.transaction.preview_current_roots_v1(self.now,common,&live).map_err(|e|RootModuleExecutionErrorV1::Body(preview_error(e)))?;
  let node=input.context();let refs=parent.refs();let runtime=refs.runtime.occurrence().location();let cognition=refs.cognition.selected().occurrence().location();
  common.charge(1,(node.world_id.len()+preview.roots().world_id.len()+256)as u64).map_err(|e|RootModuleExecutionErrorV1::Body(BodyMutationError::Original(Box::new(e))))?;
  if node.world_id!=preview.roots().world_id||node.height!=preview.roots().height||preview.roots().simulation_tick!=self.now||self.ctx.transaction.parent_fence()!=parent.fence()||parent.fence()!=live{return Err(RootModuleExecutionErrorV1::Body(BodyMutationError::Invalid("IWS commitment Node/Current scope")))}
  // RootSet and StateRoot encoders have their original fixed, validated bound
  // (world ID <=256 and exactly six fixed-size roots). This is no World hash.
  let _root_scratch=common.memory().reserve(4096,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
  let roots=preview.roots().canonical_bytes()?;let policy=preview.roots().state_root(StateNamespaceV1::Authorization).canonical_bytes().map_err(oasis7_formal_protocol::incremental_execution::ExecutionError::Tree)?;
  let input_ref=input.checked().input();let policy_ref=input.checked().policy();let material=input.checked().material_id();
  let frame_bytes=[node.world_id.len(),node.node_id.len(),node.proposer_id.len(),node.node_block_hash.len(),node.action_root.len(),manifest.hash().len(),roots.capacity(),policy.capacity(),IWS_CURRENT_COMMITMENT_DOMAIN_V1.len()].into_iter().try_fold(std::mem::size_of::<IwsCurrentCommitmentFrameV1>(),|n,m|n.checked_add(m).ok_or(ValueTreeError::ResourceLimited("IWS Current commitment layout")))?;
  common.charge(9,frame_bytes as u64).map_err(|e|RootModuleExecutionErrorV1::Body(BodyMutationError::Original(Box::new(e))))?;let frame_fee=common.memory().reserve(frame_bytes,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
  fn copy(s:&str)->Result<String,ValueTreeError>{let mut value=String::new();value.try_reserve_exact(s.len()).map_err(|_|ValueTreeError::ResourceLimited("IWS context string"))?;if value.capacity()>s.len(){return Err(ValueTreeError::ResourceLimited("IWS context string capacity"))}value.push_str(s);Ok(value)}
  let frame=frame_fee.retain(IwsCurrentCommitmentFrameV1{domain:copy(IWS_CURRENT_COMMITMENT_DOMAIN_V1)?,version:1,parent_record:refs.core.candidate_record_id(),parent_body:parent.fence().parent_commit,parent_roots:parent.fence().parent_roots,runtime_original:runtime.original,runtime_ordinal:runtime.ordinal,cognition_original:cognition.original,cognition_ordinal:cognition.ordinal,current_roots:roots,manifest_hash:copy(manifest.hash())?,policy_namespace_root:policy,frozen_material:material,input_original:input_ref.id,input_bytes:input_ref.byte_len,policy_original:policy_ref.id,policy_bytes:policy_ref.byte_len,context:IwsNodeCommitContextV1{world_id:copy(&node.world_id)?,node_id:copy(&node.node_id)?,proposer_id:copy(&node.proposer_id)?,height:node.height,slot:node.slot,epoch:node.epoch,node_block_hash:copy(&node.node_block_hash)?,action_root:copy(&node.action_root)?,committed_at_unix_ms:node.committed_at_unix_ms}});
  let maximum=frame_bytes.checked_mul(2).and_then(|n|n.checked_add(8192)).ok_or(ValueTreeError::ResourceLimited("IWS Current commitment wire"))?;
  let wire=encode_paid(&*frame,maximum,common,"IWS Current commitment wire","IWS Current commitment encode").map_err(|e|RootModuleExecutionErrorV1::Body(BodyMutationError::Original(Box::new(e))))?;
  let digest_fee=common.memory().reserve(32,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;let mut hash=Sha256::new();for chunk in wire.chunks(4096){common.charge(1,chunk.len()as u64).map_err(|e|RootModuleExecutionErrorV1::Body(BodyMutationError::Original(Box::new(e))))?;hash.update(chunk);}
  let value=owner.retain(PaidIwsCurrentCommitmentV1{preview,manifest,frame,wire,digest:digest_fee.retain(hash.finalize().into()),parent_record:refs.core.candidate_record_id(),input_material:material,identity:common.work().identity()});
  value.validate(self.ctx.transaction,parent,input,common,&live_owner()).map_err(RootModuleExecutionErrorV1::Body)?;Ok(value)
 })();if result.is_err(){let _:Result<(),oasis7_formal_protocol::incremental_execution::ExecutionError>=self.ctx.transaction.contribute(|_|Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted));}result
 }
}
