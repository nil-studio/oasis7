//! Fixed independent ProofLedger component consumer. No RootSet/boolean
//! constructor; absence is obtained only from the actual full parent router.
use super::*;
use crate::world::incremental_normal_final_sources::NormalCognitionProjectionInputsV1;
use oasis7_world_state::incremental_cognition_current_complete_v3::CognitionCurrentPresenceV3;
enum Backing{Present(CognitionProofLedgerProjectionAuxDataV1),Absent{guard:oasis7_formal_protocol::incremental_execution::PreparedGuard}}
pub(in crate::world) struct PaidCognitionProofProjectionAuxV1{source:Backing,core:WorkIdentityV1,memory:MemoryBudget,_fee:MemoryReservation}
impl PaidCognitionProofProjectionAuxV1{
 pub(in crate::world) fn validate<M:MerkleNodeReader>(&self,tx:&ExecutionTransaction<'_,M>,common:&mut FormalBoundedContextV1<'_>,live:&ExecutionFence)->Result<(),BodyMutationError>{common.consume_steps(1).map_err(formal)?;if self.core!=common.work().identity()||!self.memory.same_pool(common.memory())||!self._fee.uses_budget(common.memory()){return Err(bad())}match &self.source{Backing::Present(source)=>source.validate(tx,common.memory(),live).map_err(overlay),Backing::Absent{guard}=>tx.validate_read_source_guard(guard,live).map_err(Into::into)}}
 pub(in crate::world) fn role_present(&self)->bool{matches!(&self.source,Backing::Present(_))}
 pub(in crate::world) fn visit_presence_with_work_v1<E>(&self,work:impl FnMut(u64,u64)->Result<(),E>,emit:impl FnMut(&'static str,bool)->Result<(),E>)->Result<(),E>{match &self.source{Backing::Present(source)=>source.visit_presence_with_work_v1(work,emit),Backing::Absent{..}=>Ok(())}}
 pub(in crate::world) fn visit_paths_with_work_v1<E>(&self,work:impl FnMut(u64,u64)->Result<(),E>,emit:impl FnMut(&'static str,&oasis7_world_state::incremental_cognition_projection_aux::CognitionAuxJsonPatchV1)->Result<(),E>)->Result<(),E>{match &self.source{Backing::Present(source)=>source.visit_paths_with_work_v1(work,emit),Backing::Absent{..}=>Ok(())}}
}
fn absent<M:MerkleNodeReader>(parent:&BoundNormalTickParentV1<'_>,tx:&ExecutionTransaction<'_,M>,common:&mut FormalBoundedContextV1<'_>,live:&ExecutionFence)->Result<PaidCognitionProofProjectionAuxV1,BodyMutationError>{
 parent.validate_context(common).map_err(formal)?;tx.validate_read_source_guard(&tx.guard(),live)?;
 if tx.parent_fence()!=parent.fence()||!tx.uses_budget(common.memory())||parent.refs().storage.results().proof_ledger().is_some()||parent.refs().storage.current().cognition().presence()!=CognitionCurrentPresenceV3::Disabled||parent.refs().cognition.selected().manifest().enabled||parent.refs().cognition.current().enabled(){return Err(bad())}
 let bytes=std::mem::size_of::<PaidCognitionProofProjectionAuxV1>();common.charge(3,bytes as u64).map_err(formal)?;let fee=common.memory().reserve(bytes,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
 Ok(PaidCognitionProofProjectionAuxV1{source:Backing::Absent{guard:tx.guard()},core:common.work().identity(),memory:common.memory().clone(),_fee:fee})
}
fn component(value:PaidComponent)->Result<PaidCognitionProofProjectionAuxV1,BodyMutationError>{let PaidComponent{source,core,memory,_fee}=value;match source{Component::Proof(source)=>Ok(PaidCognitionProofProjectionAuxV1{source:Backing::Present(source),core,memory,_fee}),Component::State(_)|Component::History(_)=>Err(bad())}}
fn accepted(parent:&BoundNormalTickParentV1<'_>,source:&ReservedValue<AcceptedCognitionProofLedgerV1>,common:&mut FormalBoundedContextV1<'_>)->Result<(),BodyMutationError>{
 parent.validate_context(common).map_err(formal)?;common.charge(4,256).map_err(formal)?;let role=parent.refs().storage.results().proof_ledger().ok_or_else(bad)?;let occurrence=source.occurrence();let selected=role.occurrence();
 if !source.uses_budget(common.memory())||occurrence.allocation_class()!=MemoryClass::Ordinary||occurrence.head_record_id()!=parent.refs().head.verified_record_id()||occurrence.head_body_id()!=parent.refs().runtime.occurrence().head_body_id()||occurrence.fact_body_id()!=occurrence.head_body_id()||occurrence.roots_id()!=parent.refs().runtime.manifest().roots||source.manifest().roots!=occurrence.roots_id()||selected.location().original!=occurrence.location().original||selected.body_id()!=occurrence.head_body_id()||selected.roots_id()!=occurrence.roots_id(){return Err(bad())}Ok(())
}
#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn capture_current_cognition_proof_projection_aux_bounded_v1<M:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool>(parent:&BoundNormalTickParentV1<'_>,inputs:&NormalCognitionProjectionInputsV1<'_>,ctx:&mut WorldFieldOverlayContextV1<'_, '_, '_,M,R,E,C>,work:&mut CognitionProjectionAuxWorkV1<'_>,common:&mut FormalBoundedContextV1<'_>,live:&ExecutionFence)->Result<PaidCognitionProofProjectionAuxV1,BodyMutationError>{
 let outcome=std::panic::catch_unwind(std::panic::AssertUnwindSafe(||{inputs.validate(ctx.transaction,common,live)?;match(inputs.parent_ledger(),inputs.ledger()){
  (Some(before),Some(after))=>{accepted(parent,before,common)?;component(capture(parent,ctx,work,common,live,Scope::CurrentProof{before,after})?)},
  (None,None)if !inputs.enabled()=>absent(parent,ctx.transaction,common,live),_=>Err(bad()),
 }}));
 match outcome{Ok(result)=>{if result.is_err(){let _:Result<(),ExecutionError>=ctx.transaction.contribute(|_|Err(ExecutionError::Aborted));}result},Err(panic)=>{let _:Result<(),ExecutionError>=ctx.transaction.contribute(|_|Err(ExecutionError::Aborted));std::panic::resume_unwind(panic)}}
}
pub(in crate::world) fn capture_qualified_cognition_proof_projection_aux_bounded_v1<M:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool>(parent:&BoundNormalTickParentV1<'_>,source:&ReservedValue<AcceptedCognitionProofLedgerV1>,ctx:&mut WorldFieldOverlayContextV1<'_, '_, '_,M,R,E,C>,work:&mut CognitionProjectionAuxWorkV1<'_>,common:&mut FormalBoundedContextV1<'_>,live:&ExecutionFence)->Result<PaidCognitionProofProjectionAuxV1,BodyMutationError>{
 let outcome=std::panic::catch_unwind(std::panic::AssertUnwindSafe(||{ctx.transaction.validate_bootstrap_read_only_v1(live)?;accepted(parent,source,common)?;component(capture(parent,ctx,work,common,live,Scope::QualifiedProof{before:source})?)}));
 match outcome{Ok(result)=>{if result.is_err(){let _:Result<(),ExecutionError>=ctx.transaction.contribute(|_|Err(ExecutionError::Aborted));}result},Err(panic)=>{let _:Result<(),ExecutionError>=ctx.transaction.contribute(|_|Err(ExecutionError::Aborted));std::panic::resume_unwind(panic)}}
}
pub(in crate::world) fn capture_qualified_absent_cognition_proof_projection_aux_bounded_v1<M:MerkleNodeReader>(parent:&BoundNormalTickParentV1<'_>,tx:&mut ExecutionTransaction<'_,M>,common:&mut FormalBoundedContextV1<'_>,live:&ExecutionFence)->Result<PaidCognitionProofProjectionAuxV1,BodyMutationError>{
 let outcome=std::panic::catch_unwind(std::panic::AssertUnwindSafe(||{tx.validate_bootstrap_read_only_v1(live)?;absent(parent,tx,common,live)}));
 match outcome{Ok(result)=>{if result.is_err(){let _:Result<(),ExecutionError>=tx.contribute(|_|Err(ExecutionError::Aborted));}result},Err(panic)=>{let _:Result<(),ExecutionError>=tx.contribute(|_|Err(ExecutionError::Aborted));std::panic::resume_unwind(panic)}}
}
