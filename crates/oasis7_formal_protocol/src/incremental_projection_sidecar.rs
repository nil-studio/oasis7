//! Small original JSON side data in the existing declared object graph.
//! Content/context verification is DATA only. Neither graph references nor
//! this owner independently authenticate history or the side value's meaning;
//! a bootstrap consumer must compare the original signed projection commitment.
use crate::{incremental_artifacts::OriginalArtifactRefV1,incremental_execution::ExecutionError,incremental_history::wire::{Reader,Writer},incremental_material::CheckedCommitCore,controlled_authority::{formal::{FormalError,VerifiedFormalCommitV1,bounded_context::{FormalBoundedContextV1,WorkIdentityV1}},replicated_protocol::{ArtifactRole,ClosedRecord}}};
use oasis7_proto::memory_budget::{MemoryBudget,MemoryClass,MemoryReservation,ReservedValue};
use std::{cell::RefCell,mem::size_of,sync::Arc};
pub const PROJECTION_SIDECAR_DOMAIN_V1:&[u8]=b"oasis7.iws.projection-side.v1\0";
pub const PROJECTION_SIDECAR_MAX_JSON_BYTES:usize=4096;
pub const PROJECTION_SIDECAR_MAX_BYTES:usize=8192;
/// A codec DATA value, never an accepted-source constructor. JSON is the
/// original typed Option<FormalRetainedHistoryRootV1> serialization; Protocol
/// deliberately does not decode a WorldState type or infer missing as null.
pub struct ProjectionSidecarV1{pub record:[u8;32],pub commit_body:[u8;32],pub roots:[u8;32],pub retained_history_root_json:Vec<u8>}
impl ProjectionSidecarV1{
 pub fn canonical_bytes_with_work(&self,memory:&MemoryBudget,work:&mut dyn FnMut(u64,u64)->Result<(),ExecutionError>)->Result<ReservedValue<Vec<u8>>,ExecutionError>{
  if self.retained_history_root_json.is_empty()||self.retained_history_root_json.len()>PROJECTION_SIDECAR_MAX_JSON_BYTES{return Err(ExecutionError::Invalid("projection side JSON bound"))}
  let n=self.retained_history_root_json.len().checked_add(512).and_then(|n|n.checked_mul(3)).ok_or(ExecutionError::Invalid("projection side encode layout"))?;work(1,n as u64)?;let fee=memory.reserve(n,MemoryClass::Ordinary).map_err(ExecutionError::Memory)?;
  let work=RefCell::new(work);let meter=|s,b|(work.borrow_mut())(s,b);let mut writer=Writer::with_work(&meter);writer.hash(&self.record);writer.hash(&self.commit_body);writer.hash(&self.roots);writer.nested(&self.retained_history_root_json);let bytes=writer.finish(PROJECTION_SIDECAR_DOMAIN_V1,PROJECTION_SIDECAR_MAX_BYTES)?;Ok(fee.retain(bytes))
 }
 fn decode_with_work(bytes:&[u8],work:&mut dyn FnMut(u64,u64)->Result<(),ExecutionError>)->Result<Self,ExecutionError>{
  work(8,(bytes.len().checked_mul(3).and_then(|n|n.checked_add(size_of::<Self>())).ok_or(ExecutionError::Invalid("projection side decode work"))?)as u64)?;
  let mut reader=Reader::new(bytes,PROJECTION_SIDECAR_DOMAIN_V1,PROJECTION_SIDECAR_MAX_BYTES)?;let record=reader.array()?;let commit_body=reader.array()?;let roots=reader.array()?;let json=reader.nested()?;if json.is_empty()||json.len()>PROJECTION_SIDECAR_MAX_JSON_BYTES{return Err(ExecutionError::Invalid("projection side JSON bound"))}let retained_history_root_json=json.to_vec();reader.finish()?;Ok(Self{record,commit_body,roots,retained_history_root_json})
 }
}
/// Physical/logical IO only. The actual Host adapter owns continuing original
/// IO lanes from this same Common. No legacy unbounded fallback is supplied.
pub trait PaidProjectionOriginalReaderV1{
 type Error;
 fn allocation_budget(&self)->&MemoryBudget;
 fn allocation_class(&self)->MemoryClass;
 fn read_projection_original_bounded_v1(&mut self,id:&[u8;32],max:usize,common:&mut FormalBoundedContextV1<'_>)->Result<Arc<ReservedValue<Vec<u8>>>,Self::Error>;
}
#[derive(Debug)]pub enum ProjectionSidecarReadErrorV1<E>{Invalid(&'static str),Context(FormalError),Original(E),Codec(ExecutionError)}
impl<E:std::fmt::Debug>std::fmt::Display for ProjectionSidecarReadErrorV1<E>{fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result{write!(f,"projection side data: {self:?}")}}
impl<E:std::fmt::Debug>std::error::Error for ProjectionSidecarReadErrorV1<E>{}
pub struct PaidVerifiedProjectionSidecarV1{value:ProjectionSidecarV1,raw:Arc<ReservedValue<Vec<u8>>>,core:WorkIdentityV1,fee:MemoryReservation}
impl PaidVerifiedProjectionSidecarV1{
 pub fn retained_history_root_json(&self)->&[u8]{&self.value.retained_history_root_json}
 pub fn record_id(&self)->[u8;32]{self.value.record}
 pub fn body_id(&self)->[u8;32]{self.value.commit_body}
 pub fn roots_id(&self)->[u8;32]{self.value.roots}
 pub fn validate_context(&self,head:&VerifiedFormalCommitV1,core:&ReservedValue<CheckedCommitCore>,common:&mut FormalBoundedContextV1<'_>)->Result<(),FormalError>{
  common.charge(4,128)?;
  if !core.uses_budget(common.memory())||!self.raw.uses_budget(common.memory())||!self.fee.uses_budget(common.memory())||self.core!=common.work().identity()||self.value.record!=head.verified_record_id()||core.candidate_record_id()!=self.value.record||core.catalog().entry.commit!=self.value.commit_body||core.body().roots!=self.value.roots{return Err(FormalError::Invalid("projection side actual context".into()))}Ok(())
 }
}
fn raw<O:PaidProjectionOriginalReaderV1>(objects:&mut O,reference:OriginalArtifactRefV1,common:&mut FormalBoundedContextV1<'_>)->Result<Arc<ReservedValue<Vec<u8>>>,ProjectionSidecarReadErrorV1<O::Error>>{
 use ProjectionSidecarReadErrorV1 as E;
 if !objects.allocation_budget().same_pool(common.memory())||objects.allocation_class()!=MemoryClass::Ordinary{return Err(E::Invalid("projection side Original pool/class"))}
 let n=usize::try_from(reference.byte_len).map_err(|_|E::Invalid("projection Original length"))?;common.consume_steps(1).map_err(E::Context)?;
 let value=objects.read_projection_original_bounded_v1(&reference.id,n,common).map_err(E::Original)?;if !value.uses_budget(common.memory())||value.len()!=n{return Err(E::Invalid("projection Original exact length/pool"))}
 let _hash=common.memory().reserve(size_of::<blake3::Hasher>()+size_of::<blake3::Hash>(),MemoryClass::Ordinary).map_err(|_|E::Invalid("projection side hash phase"))?;common.consume_steps(1).map_err(E::Context)?;let mut hash=blake3::Hasher::new();for chunk in value.chunks(8192){common.charge(2,chunk.len()as u64).map_err(E::Context)?;hash.update(chunk);}if hash.finalize().as_bytes()!=&reference.id{return Err(E::Invalid("projection Original content hash"))}Ok(value)
}
/// Locate exactly one domain object declared by the real current body. Every
/// selected byte is re-read through the paid Original source, never trusted
/// from the caller's graph. Final semantic signed-root checks are mandatory
/// in the fixed Runtime bootstrap consumer; this function grants no authority.
pub fn read_projection_sidecar_bounded_v1<O:PaidProjectionOriginalReaderV1>(head:&VerifiedFormalCommitV1,core:&ReservedValue<CheckedCommitCore>,material:&ClosedRecord,objects:&mut O,common:&mut FormalBoundedContextV1<'_>)->Result<PaidVerifiedProjectionSidecarV1,ProjectionSidecarReadErrorV1<O::Error>>{
 use ProjectionSidecarReadErrorV1 as E;let record=head.record();common.charge(8,256).map_err(E::Context)?;
 if !core.uses_budget(common.memory())||core.candidate_record_id()!=head.verified_record_id()||!crate::controlled_authority::formal::incremental::is_iws_record(record.schema_version){return Err(E::Invalid("projection side actual IWS head/core"))}
 let compare=material.payload_digest.len()+material.before_state_root.len()+material.after_state_root.len()+record.payload_digest.len()+record.before_state_root.len()+record.after_state_root.len();common.charge(1,compare as u64).map_err(E::Context)?;
 if material.payload_digest!=record.payload_digest||material.before_state_root!=record.before_state_root||material.after_state_root!=record.after_state_root||material.roots.len()!=record.artifact_roots.len(){return Err(E::Invalid("projection side graph record binding"))}
 for(role,id)in &record.artifact_roots{common.charge(1,(id.len()+128)as u64).map_err(E::Context)?;if material.roots.get(role)!=Some(id){return Err(E::Invalid("projection side graph root binding"))}}
 let body_hex=record.artifact_roots.get(&ArtifactRole::IncrementalCommitBody).ok_or(E::Invalid("projection side body role missing"))?;let mut body_id=[0;32];common.charge(1,64).map_err(E::Context)?;hex::decode_to_slice(body_hex,&mut body_id).map_err(|_|E::Invalid("projection side body ID"))?;if body_id!=core.catalog().entry.commit{return Err(E::Invalid("projection side actual body"))}
 let mut body=None;for object in &material.objects{common.charge(1,(object.content_hash.len()+64)as u64).map_err(E::Context)?;if &object.content_hash==body_hex{if body.replace(object).is_some(){return Err(E::Invalid("projection side duplicate body object"))}}}let body=body.ok_or(E::Invalid("projection side body graph missing"))?;
 // The actual body bytes are authenticated independently of graph metadata.
 let _body_raw=raw(objects,OriginalArtifactRefV1{id:body_id,byte_len:u64::try_from(body.bytes.len()).map_err(|_|E::Invalid("projection body original length"))?},common)?;
 let mut selected=None;let mut last=None;
 for reference in &body.references{common.charge(1,reference.len()as u64).map_err(E::Context)?;if last.is_some_and(|old:&String|old>=reference){return Err(E::Invalid("projection body references order"))}last=Some(reference);
  for object in &material.objects{common.charge(1,(object.content_hash.len()+PROJECTION_SIDECAR_DOMAIN_V1.len())as u64).map_err(E::Context)?;if &object.content_hash==reference&&object.bytes.starts_with(PROJECTION_SIDECAR_DOMAIN_V1){if selected.is_some()||object.bytes.len()>PROJECTION_SIDECAR_MAX_BYTES{return Err(E::Invalid("projection side uniqueness/bound"))}let mut id=[0;32];common.charge(1,64).map_err(E::Context)?;hex::decode_to_slice(reference,&mut id).map_err(|_|E::Invalid("projection side ID"))?;selected=Some(OriginalArtifactRefV1{id,byte_len:object.bytes.len()as u64});}}
 }
 let reference=selected.ok_or(E::Invalid("projection side typed object missing"))?;let raw=raw(objects,reference,common)?;
 let n=raw.len().checked_mul(3).and_then(|n|n.checked_add(size_of::<PaidVerifiedProjectionSidecarV1>()+1024)).ok_or(E::Invalid("projection side owner layout"))?;common.charge(1,n as u64).map_err(E::Context)?;let fee=common.memory().reserve(n,MemoryClass::Ordinary).map_err(|_|E::Invalid("projection side decode phase"))?;
 let mut first=None;let result=ProjectionSidecarV1::decode_with_work(&raw,&mut|s,b|common.charge(s,b).map_err(|e|{first=Some(e);ExecutionError::Aborted}));if let Some(e)=first{return Err(E::Context(e))}let value=result.map_err(E::Codec)?;
 let result=PaidVerifiedProjectionSidecarV1{value,raw,core:common.work().identity(),fee};result.validate_context(head,core,common).map_err(E::Context)?;Ok(result)
}
