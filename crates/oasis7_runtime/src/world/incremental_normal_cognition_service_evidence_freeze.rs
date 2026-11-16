//! Fixed Current → real seal → checked paired candidate → genuine acceptance.
//! Every fallible IO/hash/allocation precedes durable acceptance. Finish is the
//! existing fixed record comparison followed by moving the prepaid owner.
use super::*;
use oasis7_formal_protocol::incremental_execution::{ExecutionTransaction,PreparedGuard,SealedExecutionBindingV1,RootSetV1};
use oasis7_formal_protocol::incremental_state::StateRootV1;
use oasis7_world_state::incremental_cognition_proof_ledger::CheckedCognitionProofLedgerV1;
pub(in crate::world) struct FrozenCognitionWakeEvidenceV2{
 index:index::Index,journal_root:Option<StateRootV1>,marker_root:StateRootV1,receipt_root:StateRootV1,
 journal_records:u64,marker_rows:u64,receipt_rows:u64,guard:PreparedGuard,
 core:oasis7_formal_protocol::controlled_authority::formal::bounded_context::WorkIdentityV1,_fee:ReservedValue<()>,
}
impl current::CurrentCognitionWakeEvidenceV2<'_,'_>{
 pub(in crate::world) fn freeze<M:MerkleNodeReader>(&self,tx:&ExecutionTransaction<'_,M>,common:&mut FormalBoundedContextV1<'_>,live:&ExecutionFence)->Result<FrozenCognitionWakeEvidenceV2,BodyMutationError>{
  self.validate(tx,common,live)?;common.charge(4,4096).map_err(formal)?;
  let fee=common.memory().reserve(std::mem::size_of::<FrozenCognitionWakeEvidenceV2>()+4096,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
  let marker_rows=self.ledger.evidence_array_len_v2(CognitionProofLedgerFieldV1::CommitRecords,tx,common,live)?;let receipt_rows=self.ledger.evidence_array_len_v2(CognitionProofLedgerFieldV1::ReceiptRegistry,tx,common,live)?;
  let marker_root=self.ledger.evidence_field_root_v2(CognitionProofLedgerFieldV1::CommitRecords,tx,common,live)?.clone();let receipt_root=self.ledger.evidence_field_root_v2(CognitionProofLedgerFieldV1::ReceiptRegistry,tx,common,live)?.clone();
  Ok(FrozenCognitionWakeEvidenceV2{index:self.index.share(common)?,journal_root:Some(self.pending.index_root_data_v2().clone()),marker_root,receipt_root,journal_records:self.pending.count(),marker_rows,receipt_rows,guard:tx.guard(),core:common.work().identity(),_fee:fee.retain(())})
 }
}
impl FrozenCognitionWakeEvidenceV2{
 pub(in crate::world) fn validate_guard<M:MerkleNodeReader>(&self,tx:&ExecutionTransaction<'_,M>,common:&mut FormalBoundedContextV1<'_>,live:&ExecutionFence)->Result<(),BodyMutationError>{common.charge(1,0).map_err(formal)?;if self.core!=common.work().identity()||!self._fee.uses_budget(common.memory()){return Err(bad().into())}tx.validate_guard(&self.guard,live)?;Ok(())}
 /// A checked source authenticates actual post-seal Result/index bodies; it is
 /// not replaced by comparing caller-supplied roots or a completion boolean.
 pub(in crate::world) fn prepare_candidate(self,sealed:&ReservedValue<SealedExecutionBindingV1>,roots:&RootSetV1,source:&ReservedValue<CheckedCognitionHistorySourceV3>,ledger:&ReservedValue<CheckedCognitionProofLedgerV1>,common:&mut FormalBoundedContextV1<'_>)->Result<PreparedCognitionWakeEvidenceV2,BodyMutationError>{
  common.charge(6,8192).map_err(formal)?;sealed.validate_source_guard(&self.guard,common.memory())?;sealed.validate_after_roots(roots,common.memory())?;
  if self.core!=common.work().identity()||!self._fee.uses_budget(common.memory())||!source.uses_budget(common.memory())||!ledger.uses_budget(common.memory())||source.roots_id()!=sealed.after_roots_digest()||ledger.manifest().roots!=sealed.after_roots_digest()||source.candidate_record_id()!=ledger.occurrence().candidate_record_id()||source.body_id()!=ledger.occurrence().body_id(){return Err(bad().into())}
  let profile=match &self.journal_root{Some(root)=>{let index=source.indexes()[0].as_ref().ok_or_else(bad)?;if index.field()!=CognitionHistoryFieldV1::JournalEvents||index.root()!=root||index.record_count()!=self.journal_records||index.candidate_record_id()!=source.candidate_record_id()||index.body_id()!=source.body_id()||index.roots_id()!=source.roots_id(){return Err(bad().into())}EvidenceJournalProfileV2::Active},None=>{if source.disabled().is_none()||source.indexes()[0].is_some()||self.journal_records!=0||self.marker_rows!=0||self.receipt_rows!=0{return Err(bad().into())}EvidenceJournalProfileV2::Disabled}};
  if ledger.manifest().fields[0].as_ref()!=Some(&self.marker_root)||ledger.manifest().fields[1].as_ref()!=Some(&self.receipt_root){return Err(bad().into())}
  let fee=common.memory().reserve(std::mem::size_of::<AcceptedCognitionWakeEvidenceV2>(),MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
  Ok(PreparedCognitionWakeEvidenceV2{inner:fee.retain(AcceptedCognitionWakeEvidenceV2{head:source.candidate_record_id(),body:source.body_id(),roots:source.roots_id(),ledger_original:ledger.occurrence().location().original,profile,journal_records:self.journal_records,marker_rows:self.marker_rows,receipt_rows:self.receipt_rows,index:self.index,_scope:common.memory().reserve(0,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?.retain(())})})
 }
}

/// A real disabled parent and current empty side-ledger may have no Journal
/// frontier. The checked disabled source must confirm this again after seal.
pub(in crate::world) fn freeze_disabled_current_cognition_wake_evidence_v2<M:MerkleNodeReader>(parent:&ReservedValue<AcceptedCognitionWakeEvidenceV2>,normal_parent:&ReservedValue<BoundNormalTickParentV1<'_>>,accepted_ledger:&ReservedValue<AcceptedCognitionProofLedgerV1>,ledger:&oasis7_world_state::incremental_cognition_proof_ledger::CurrentCognitionProofLedgerV1,tx:&ExecutionTransaction<'_,M>,common:&mut FormalBoundedContextV1<'_>,live:&ExecutionFence)->Result<FrozenCognitionWakeEvidenceV2,BodyMutationError>{
 parent.validate_parent(normal_parent,accepted_ledger,common)?;ledger.validate_context(tx,common,live)?;
 if parent.profile!=EvidenceJournalProfileV2::Disabled||normal_parent.refs().cognition.disabled().is_none()||normal_parent.refs().cognition.indexes()[0].is_some()||ledger.evidence_array_len_v2(CognitionProofLedgerFieldV1::CommitRecords,tx,common,live)?!=0||ledger.evidence_array_len_v2(CognitionProofLedgerFieldV1::ReceiptRegistry,tx,common,live)?!=0{return Err(bad().into())}
 common.charge(4,4096).map_err(formal)?;let fee=common.memory().reserve(std::mem::size_of::<FrozenCognitionWakeEvidenceV2>()+4096,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
 let marker_root=ledger.evidence_field_root_v2(CognitionProofLedgerFieldV1::CommitRecords,tx,common,live)?.clone();let receipt_root=ledger.evidence_field_root_v2(CognitionProofLedgerFieldV1::ReceiptRegistry,tx,common,live)?.clone();
 Ok(FrozenCognitionWakeEvidenceV2{index:parent.index.share(common)?,journal_root:None,marker_root,receipt_root,journal_records:0,marker_rows:0,receipt_rows:0,guard:tx.guard(),core:common.work().identity(),_fee:fee.retain(())})
}
