//! Preaccept construction. Genuine checked sources are consumed as DATA; the
//! result remains private until its exact candidate is really accepted.
use super::*;
use oasis7_world_state::incremental_cognition_proof_ledger::CheckedCognitionProofLedgerV1;
use oasis7_formal_protocol::controlled_authority::formal::VerifiedFormalCommitV1;
#[derive(Debug)]
pub(in crate::world) enum EvidenceBuildErrorV2<OE>{Body(BodyMutationError),Original(OE),Business(ReservedValue<crate::WorldError>)}
impl<OE> From<BodyMutationError> for EvidenceBuildErrorV2<OE>{fn from(e:BodyMutationError)->Self{Self::Body(e)}}
impl<OE> From<ValueTreeError> for EvidenceBuildErrorV2<OE>{fn from(e:ValueTreeError)->Self{Self::Body(e.into())}}
pub(in crate::world) struct PreparedCognitionWakeEvidenceV2{pub(super) inner:ReservedValue<AcceptedCognitionWakeEvidenceV2>}
/// All allocation/hash/IO has completed before this move. A mismatched genuine
/// commit is rejected without changing the prepared source or installed cache.
pub(in crate::world) fn finish_prepared_cognition_wake_evidence_v2(prepared:PreparedCognitionWakeEvidenceV2,accepted:&VerifiedFormalCommitV1)->Result<ReservedValue<AcceptedCognitionWakeEvidenceV2>,ValueTreeError>{if accepted.verified_record_id()!=prepared.inner.head{return Err(bad())}Ok(prepared.inner)}
fn paid<M:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool,T>(read:&mut WorldFieldReadContextV1<'_,M,R,E,C>,work:&mut CurrentCognitionExpiryWorkV1,common:&mut FormalBoundedContextV1<'_>,run:impl FnOnce(&mut WorldFieldReadContextV1<'_,M,R,&mut E,&mut dyn FnMut()->bool>,&mut CurrentCognitionExpiryWorkV1)->Result<T,BodyMutationError>)->Result<T,BodyMutationError>{
 common.charge(1,0).map_err(formal)?;let (_,meter,stop)=common.parts();if stop(){return Err(ValueTreeError::Cancelled.into())}let mut lease=meter.begin_child().map_err(formal)?;
 fn lanes<M,R,E,C>(read:&mut WorldFieldReadContextV1<'_,M,R,E,C>,work:&mut CurrentCognitionExpiryWorkV1,f:&mut impl FnMut(&mut u64,bool)){for n in [&mut read.values.io.nodes_remaining,&mut work.index.cells_remaining,&mut work.cbor.nodes_remaining,&mut work.cbor.source_reads_remaining]{f(n,false)}for n in [&mut read.values.io.read_bytes_remaining,&mut read.key_work.bytes_remaining,&mut work.cbor.bytes_remaining,&mut work.cbor.key_bytes_remaining]{f(n,true)}}
 let mut original=[0;8];let mut caps=[0;8];let mut i=0;lanes(read,work,&mut|n,b|{original[i] = *n;*n=(*n).min(if b{lease.remaining_read_bytes()/4}else{lease.remaining_steps()/4});caps[i] = *n;i+=1;});
 let result={let mut cancelled=||stop()||(read.cancelled)();let mut actual=WorldFieldReadContextV1{root:read.root,state:read.state,values:ValueReadContext{source:read.values.source,memory:read.values.memory,class:read.values.class,io:&mut *read.values.io},key_work:&mut *read.key_work,semantic_bytes:read.semantic_bytes,record_bytes:read.record_bytes,take_state_error:&mut read.take_state_error,cancelled:&mut cancelled as &mut dyn FnMut()->bool};run(&mut actual,work)};
 let mut steps=0u64;let mut bytes=0u64;let mut invalid=false;let mut i=0;lanes(read,work,&mut|n,b|{invalid|=*n>caps[i];let delta=caps[i].saturating_sub(*n);*n=original[i]-delta;i+=1;let total=if b{&mut bytes}else{&mut steps};if let Some(sum)=total.checked_add(delta){*total=sum}else{invalid=true}});let debit=lease.consume(steps,bytes).map_err(formal);lease.finish();match result{Err(e)=>Err(e),Ok(value)=>{debit?;if invalid{return Err(bad().into())}
 if stop(){return Err(ValueTreeError::Cancelled.into())}Ok(value)}}
}
/// Fixed GEN/candidate cache producer. No caller-supplied roots, marker result,
/// digest Vec or success callback can stand in for either checked source.
pub(in crate::world) fn prepare_checked_cognition_wake_evidence_v2<M,R,E,C,O>(source:&ReservedValue<CheckedCognitionHistorySourceV3>,ledger:&ReservedValue<CheckedCognitionProofLedgerV1>,read:&mut WorldFieldReadContextV1<'_,M,R,E,C>,originals:&mut O,input:&mut oasis7_wasm_abi::CanonicalCborWorkV1,work:&mut CurrentCognitionExpiryWorkV1,common:&mut FormalBoundedContextV1<'_>)->Result<PreparedCognitionWakeEvidenceV2,EvidenceBuildErrorV2<O::Error>>
where M:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool,O:OriginalObjectReader{
 common.charge(1,256).map_err(|e|EvidenceBuildErrorV2::Body(formal(e)))?;
 let selected=source.selected();let index=source.indexes()[0].as_ref();let profile=if index.is_some(){EvidenceJournalProfileV2::Active}else if source.disabled().is_some(){EvidenceJournalProfileV2::Disabled}else{return Err(bad().into())};
 if !source.uses_budget(common.memory())||!ledger.uses_budget(common.memory())||!selected.uses_budget(common.memory())||read.values.class!=MemoryClass::Ordinary||!read.values.memory.same_pool(common.memory())||!originals.allocation_budget().same_pool(common.memory())||originals.allocation_class()!=MemoryClass::Ordinary||source.candidate_record_id()!=ledger.occurrence().candidate_record_id()||source.body_id()!=ledger.occurrence().body_id()||source.roots_id()!=ledger.manifest().roots||source.roots_id()!=ledger.occurrence().roots_id()||index.is_some_and(|index|index.field()!=CognitionHistoryFieldV1::JournalEvents){return Err(bad().into())}
 let count=usize::try_from(index.map_or(0,|index|index.record_count())).map_err(|_|bad())?;let bytes=count.checked_mul(32).ok_or_else(bad)?;let _events=common.memory().reserve(bytes,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;common.charge(1,bytes as u64).map_err(|e|EvidenceBuildErrorV2::Body(formal(e)))?;
 let mut events=Vec::new();events.try_reserve_exact(count).map_err(|_|ValueTreeError::ResourceLimited("checked evidence events"))?;if events.capacity()>count{return Err(ValueTreeError::ResourceLimited("checked evidence event capacity").into())}let mut previous=None;
 for ordinal in 0..count as u64{let index=index.ok_or_else(bad)?;
  let point=paid(read,work,common,|read,work|read_checked_cognition_history_index_point_v3(index,selected,ordinal,read,&mut work.index).map_err(Into::into))?;let row=point.row();
  input.visits_remaining=input.visits_remaining.checked_sub(1).ok_or(ValueTreeError::ResourceLimited("evidence Original visits"))?;input.bytes_remaining=input.bytes_remaining.checked_sub(row.encoded_bytes).ok_or(ValueTreeError::ResourceLimited("evidence Original bytes"))?;
  common.charge(1,row.encoded_bytes).map_err(|e|EvidenceBuildErrorV2::Body(formal(e)))?;
  let raw=originals.read_original(&row.location.original,usize::try_from(row.encoded_bytes).map_err(|_|bad())?,common.memory()).map_err(EvidenceBuildErrorV2::Original)?;if !raw.uses_budget(common.memory())||raw.len()as u64!=row.encoded_bytes{return Err(bad().into())}
  let schema=paid(read,work,common,|read,work|verify_cognition_history_original_record_schema_v1(CognitionOriginalCborRequestV1{field:CognitionHistoryFieldV1::JournalEvents,id:row.location.original,bytes:row.encoded_bytes},&mut raw.as_slice(),read.values.memory,MemoryClass::Ordinary,&mut work.cbor,&mut read.cancelled).map_err(Into::into))?;
  let CognitionOriginalIdentityStatusV1::DigestMatched(digest)=schema.semantic().identity_status()else{return Err(bad().into())};if schema.semantic().sequence()!=ordinal.checked_add(1)||schema.semantic().parent()!=Some(previous){return Err(bad().into())}previous=Some(digest);events.push(digest);
 }
 let fields=read_accepted_ledger_fields(LedgerSource::Checked(ledger),read,work,common)?;
 let (markers,_marker_fee)=decode_rows::<WorldCommitRecordV1>(fields.0.as_ref().ok_or_else(bad)?.value(),work.max_selected_heap,common)?;let (receipts,_receipt_fee)=decode_rows::<CognitionReceiptViewV1>(fields.1.as_ref().ok_or_else(bad)?.value(),work.max_selected_heap,common)?;
 for marker in markers.iter().filter(|row|row.status=="committed"){
  let mut count=Count{common,bytes:0,error:None};let encoded=serde_json::to_writer(&mut count,marker);if let Some(e)=count.error.take(){return Err(EvidenceBuildErrorV2::Body(formal(e)))}encoded.map_err(|e|EvidenceBuildErrorV2::Body(formal(e)))?;let bytes=count.bytes.checked_mul(32).ok_or_else(bad)?;count.common.charge(32,bytes as u64).map_err(|e|EvidenceBuildErrorV2::Body(formal(e)))?;let phase=count.bytes.checked_mul(8).and_then(|n|n.checked_add(64<<10)).ok_or_else(bad)?;let _phase=count.common.memory().reserve(phase,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
  if let Err(error)=crate::world::cognition_persistence::validate_cognition_commit_marker_v1(marker){let fee=common.memory().reserve(1024,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;return Err(EvidenceBuildErrorV2::Business(fee.retain(error)))}
 }
 if profile==EvidenceJournalProfileV2::Disabled&&(!markers.is_empty()||!receipts.is_empty()){return Err(bad().into())}let index=index::Index::from_rows(&events,&markers,&receipts,common)?;let fee=common.memory().reserve(std::mem::size_of::<AcceptedCognitionWakeEvidenceV2>(),MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
 let inner=fee.retain(AcceptedCognitionWakeEvidenceV2{head:source.candidate_record_id(),body:source.body_id(),roots:source.roots_id(),ledger_original:ledger.occurrence().location().original,profile,journal_records:count as u64,marker_rows:markers.len()as u64,receipt_rows:receipts.len()as u64,index,_scope:common.memory().reserve(0,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?.retain(())});Ok(PreparedCognitionWakeEvidenceV2{inner})
}
