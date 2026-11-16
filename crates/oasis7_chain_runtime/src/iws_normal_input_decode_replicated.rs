//! Reserved Node input DATA over its genuine one-String/one-byte-Vec schema.
//! The Node's unique grammar still decides recognition/version/kind/payload.
use super::*;
use oasis7_node::NodeReplicatedExecutionInputV1;
/// No Clone/constructor/detached DTO access. Classification alone is not
/// ProviderBacked bootstrap authority or applied-input permission.
pub(crate) struct PaidNodeReplicatedInputV1{
 value:NodeReplicatedExecutionInputV1,core:WorkIdentityV1,
 _graph:MemoryReservation,_kernel:MemoryReservation,
}
impl Borrow<NodeReplicatedExecutionInputV1> for PaidNodeReplicatedInputV1{fn borrow(&self)->&NodeReplicatedExecutionInputV1{&self.value}}
impl PaidNodeReplicatedInputV1{
 pub(crate) fn validate_context(&self,common:&mut FormalBoundedContextV1<'_>)->Result<(),NormalInputDecodeErrorV1>{
  common.consume_steps(1)?;
  if self.core!=common.work().identity()||!self._graph.uses_budget(common.memory())||!self._kernel.uses_budget(common.memory()){return Err(NormalInputDecodeErrorV1::Invalid("replicated input DATA context"))}Ok(())
 }
}
pub(crate) struct PaidNodeReplicatedRefusalV1{note:String,core:WorkIdentityV1,_fee:MemoryReservation}
impl PaidNodeReplicatedRefusalV1{
 pub(crate) fn note(&self)->&str{&self.note}
 pub(crate) fn validate_context(&self,common:&mut FormalBoundedContextV1<'_>)->Result<(),NormalInputDecodeErrorV1>{common.consume_steps(1)?;if self.core!=common.work().identity()||!self._fee.uses_budget(common.memory()){return Err(NormalInputDecodeErrorV1::Invalid("replicated refusal DATA context"))}Ok(())}
}
/// The callback accepted by Node::{decode_with_owned,action_with}. A genuine
/// serde codec refusal is intentionally (), since Node discards it as None.
/// Resource/IO/cancellation failures remain the separate, typed outer error.
pub(crate) fn decode_paid_replicated_input_codec_v1(raw:&[u8],limits:&NormalInputDecodeLimitsV1,common:&mut FormalBoundedContextV1<'_>)->Result<Result<PaidNodeReplicatedInputV1,()>,NormalInputDecodeErrorV1>{
 common.consume_steps(1)?;if raw.len()>limits.max_wire_bytes{return Err(limited())}
 let shape=cbor::probe_cbor_data_shape_v1(raw,limits.max_depth,common)?;
 // No tagged/untagged/flatten Content visitor exists in this Node DTO. Unknown
 // fields are skipped by .13 IgnoredAny. Only `kind` and `payload_cbor` own
 // dynamic output: the shared scanner counts every actual/declared byte hint.
 let heap=shape.strings.checked_add(shape.sequences).and_then(|n|n.checked_mul(4)).and_then(|n|usize::try_from(n).ok()).and_then(|n|n.checked_add(std::mem::size_of::<NodeReplicatedExecutionInputV1>()+1024)).ok_or_else(limited)?;
 let scratch=heap.checked_mul(3).ok_or_else(limited)?;
 if heap>limits.max_graph_bytes||scratch>limits.max_graph_bytes{return Err(limited())}
 // This independent kernel admission covers the .13 custom error message and
 // subsequent pure kind.trim/version/payload checks while the owner is alive.
 let kernel_bytes=raw.len().checked_mul(16).and_then(|n|n.checked_add(4096)).ok_or_else(limited)?;
 let byte_work=raw.len().checked_mul(8).and_then(|n|n.checked_add(heap)).ok_or_else(limited)?;
 common.charge(shape.values.checked_mul(4).and_then(|n|n.checked_add(8)).ok_or_else(limited)?,u64::try_from(byte_work).map_err(|_|limited())?)?;
 let kernel=common.memory().reserve(kernel_bytes,MemoryClass::Ordinary)?;
 let graph=common.memory().reserve(heap,MemoryClass::Ordinary)?;
 let phase=common.memory().reserve(scratch,MemoryClass::Ordinary)?;
 let decoded=serde_cbor::from_slice::<NodeReplicatedExecutionInputV1>(raw);
 common.consume_steps(0)?;
 let result=match decoded{Ok(value)=>Ok(PaidNodeReplicatedInputV1{value,core:common.work().identity(),_graph:graph,_kernel:kernel}),Err(_)=>Err(())};
 drop(phase);Ok(result)
}
/// Convenient complete Node-envelope classification for the second pass. The
/// first-pass whole-action validator still owns reserved identity/duplicate/
/// target ordering and uses the codec callback above, not a second grammar.
pub(crate) fn decode_paid_replicated_input_data_v1(raw:&[u8],limits:&NormalInputDecodeLimitsV1,common:&mut FormalBoundedContextV1<'_>)->Result<Result<Option<PaidNodeReplicatedInputV1>,PaidNodeReplicatedRefusalV1>,NormalInputDecodeErrorV1>{
 common.consume_steps(1)?;
 // Node's three predicate notes are fixed literals or a u8 version. This fee
 // remains outside the decoded owner, including when that owner is dropped
 // by Node before its original error String reaches us.
 let mut note_fee=common.memory().reserve(1024,MemoryClass::Ordinary)?;
 let result=NodeReplicatedExecutionInputV1::decode_with_owned(raw,|raw|decode_paid_replicated_input_codec_v1(raw,limits,common))?;
 common.consume_steps(0)?;
 Ok(match result{
  Ok(Some(mut value))=>{value._kernel.merge(&mut note_fee).map_err(NormalInputDecodeErrorV1::Invalid)?;Ok(Some(value))},
  Ok(None)=>Ok(None),
  Err(note)=>Err(PaidNodeReplicatedRefusalV1{note,core:common.work().identity(),_fee:note_fee}),
 })
}
