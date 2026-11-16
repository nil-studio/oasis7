//! Actual Cognition current-state projection components. History, independent
//! ProofLedger and RetentionLedger are separate mandatory source scopes.
#[path="incremental_cognition_projection_aux_current.rs"]mod current;
#[path="incremental_cognition_projection_aux_controls.rs"]mod controls;
pub use current::CognitionProjectionAuxWorkV1;
pub use crate::incremental_json_records::{CognitionAuxPathStepV1,CognitionAuxJsonPatchValueV1,CognitionAuxJsonPatchV1};
use crate::{incremental_cognition_current_complete_v3::CompleteCognitionCurrentQualificationV3,incremental_field_overlay::{WorldFieldOverlayContextV1,WorldFieldOverlayErrorV1}};
use oasis7_formal_protocol::{incremental_execution::{ExecutionTransaction,ExecutionFence,ExecutionError,RootSetV1},incremental_state::MerkleNodeReader,incremental_value::{ValueNodeReader,ValueTreeError}};
use oasis7_proto::memory_budget::{MemoryBudget,MemoryClass,MemoryReservation,ReservedValue};
pub struct CurrentCognitionStateProjectionAuxV1{json:current::CognitionJsonProjectionRowsV1,controls:controls::CognitionControlProjectionRowsV1,_fee:MemoryReservation}
impl CurrentCognitionStateProjectionAuxV1{
 pub fn validate<M:MerkleNodeReader>(&self,tx:&ExecutionTransaction<'_,M>,memory:&MemoryBudget,live:&ExecutionFence)->Result<(),WorldFieldOverlayErrorV1>{self.json.validate(tx,memory,live)?;self.controls.validate(tx,memory,live)?;if !self._fee.uses_budget(memory){return Err(ValueTreeError::Invalid("Cognition state projection outer pool").into())}Ok(())}
 /// Roots are original REQUIRED/OPTIONAL cognition names. Optional economy
 /// and delegation members retain their original fixed second path component.
 pub fn visit_json_paths_with_work_v1<E>(&self,work:impl FnMut(u64,u64)->Result<(),E>,emit:impl FnMut(&'static str,Option<&'static str>,&CognitionAuxJsonPatchV1)->Result<(),E>)->Result<(),E>{self.json.visit_with_work_v1(work,emit)}
 pub fn visit_json_presence_with_work_v1<E>(&self,work:impl FnMut(u64,u64)->Result<(),E>,emit:impl FnMut(&'static str,Option<&'static str>,bool)->Result<(),E>)->Result<(),E>{self.json.visit_field_presence_with_work_v1(work,emit)}
 pub fn visit_controls_with_work_v1<E>(&self,work:impl FnMut(u64,u64)->Result<(),E>,emit:impl FnMut(&'static str,Option<&'static str>,Option<&[u8]>)->Result<(),E>)->Result<(),E>{self.controls.visit_with_work_v1(work,emit)}
}
fn capture<M:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool>(parent:&ReservedValue<CompleteCognitionCurrentQualificationV3>,roots:&RootSetV1,ctx:&mut WorldFieldOverlayContextV1<'_, '_, '_,M,R,E,C>,work:&mut CognitionProjectionAuxWorkV1<'_>,all:current::Scope,live:&ExecutionFence)->Result<CurrentCognitionStateProjectionAuxV1,WorldFieldOverlayErrorV1>{
 let result=std::panic::catch_unwind(std::panic::AssertUnwindSafe(||{
  let guard=ctx.transaction.guard();ctx.transaction.validate_read_source_guard(&guard,live)?;
  let bytes=std::mem::size_of::<CurrentCognitionStateProjectionAuxV1>();
  {let p=crate::incremental_world_rule_projection_delta::Processing{bank:std::cell::RefCell::new(&mut *work.processing)};crate::incremental_world_rule_projection_delta::spend(&p,bytes,&mut ctx.cancelled)?;}
  let fee=ctx.values.memory.reserve(bytes,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
  let json=match all{current::Scope::Current=>current::capture_current_cognition_json_projection_v1(parent,roots,ctx,work,live)?,current::Scope::QualifiedAll=>current::capture_qualified_cognition_json_projection_v1(parent,roots,ctx,work,live)?};
  let controls=controls::capture_controls(parent,roots,ctx,work,all,live)?;
  ctx.transaction.validate_read_source_guard(&guard,live)?;
  Ok(CurrentCognitionStateProjectionAuxV1{json,controls,_fee:fee})
 }));
 match result{Ok(result)=>{let result=if let Some(error)=(ctx.take_state_error)(){Err(error.into())}else{result};if result.is_err(){let _:Result<(),ExecutionError>=ctx.transaction.contribute(|_|Err(ExecutionError::Aborted));}result},Err(panic)=>{let _:Result<(),ExecutionError>=ctx.transaction.contribute(|_|Err(ExecutionError::Aborted));std::panic::resume_unwind(panic)}}
}
pub fn capture_current_cognition_state_projection_aux_v1<M:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool>(parent:&ReservedValue<CompleteCognitionCurrentQualificationV3>,roots:&RootSetV1,ctx:&mut WorldFieldOverlayContextV1<'_, '_, '_,M,R,E,C>,work:&mut CognitionProjectionAuxWorkV1<'_>,live:&ExecutionFence)->Result<CurrentCognitionStateProjectionAuxV1,WorldFieldOverlayErrorV1>{capture(parent,roots,ctx,work,current::Scope::Current,live)}
/// Explicit full authenticated parent scope, for GEN/cold initialization only.
/// The actual TX is required to have no mutations/facts/branch resolutions.
pub fn capture_qualified_cognition_state_projection_aux_v1<M:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool>(parent:&ReservedValue<CompleteCognitionCurrentQualificationV3>,roots:&RootSetV1,ctx:&mut WorldFieldOverlayContextV1<'_, '_, '_,M,R,E,C>,work:&mut CognitionProjectionAuxWorkV1<'_>,live:&ExecutionFence)->Result<CurrentCognitionStateProjectionAuxV1,WorldFieldOverlayErrorV1>{capture(parent,roots,ctx,work,current::Scope::QualifiedAll,live)}
