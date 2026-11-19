//! The fixed consensus stage. The two preliminary collectors finish all their
//! reads before the strict Current preview; only the private writer follows it.
use super::HistoryWrittenIwsCommitmentV1;
use crate::world::{incremental_body::BodyMutationError,incremental_body_event::AgentEventOriginalV1,incremental_module_invocation::*,incremental_normal_sources::BoundNormalTickParentV1,incremental_normal_tail::NormalTailSourcesV1,incremental_normal_gameplay_cycles::{NormalGameplayCycleSourcesV1,NormalGameplayCycleWorkV1},incremental_world_event_counter::WorldJournalRetentionV1};
use oasis7_consensus::normal_tick_input::PaidBoundNormalInputDataV1;
use oasis7_formal_protocol::{controlled_authority::formal::bounded_context::FormalBoundedContextV1,incremental_execution::{ExecutionFence,ExecutionError},incremental_material::OriginalObjectStreamReader,incremental_schedule::SchedulePageWriter,incremental_state::{MerkleNodeReader,StatePageWriter},incremental_value::{ValueNodeReader,ValueNodeWriter,ValueTreeError}};
use oasis7_proto::memory_budget::ReservedValue;
use oasis7_world_state::{incremental_field_read::WorldFieldReadContextV1,incremental_normal_gameplay_cycles_directory::GameplayCycleDirectoryV1,incremental_runtime_history_index::RuntimeHistoryIndexWorkV1};
impl<'a,'t,'s,'v,M,R,E,C,W,F,O> RootModuleExecutionContextV1<'a,'t,'s,'v,M,R,E,C,W,F,O>
where M:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool,W:ValueNodeReader+ValueNodeWriter+StatePageWriter+SchedulePageWriter,F:FnMut(&mut dyn AgentEventOriginalV1)->Result<(),BodyMutationError>,O:OriginalObjectStreamReader{
 /// DATA-only input remains DATA. The Host's fixed sole caller must retain its
 /// genuine fresh/restored Node source for the enclosing execution result.
 pub(in crate::world) fn execute_iws_tick_consensus_v1<P:MerkleNodeReader,PE:FnMut()->Option<ValueTreeError>,PC:FnMut()->bool>(&mut self,parent_source:&ReservedValue<BoundNormalTickParentV1<'_>>,input:&PaidBoundNormalInputDataV1<'_>,gameplay_directory:&ReservedValue<GameplayCycleDirectoryV1>,retention:&ReservedValue<WorldJournalRetentionV1>,gameplay_work:&mut NormalGameplayCycleWorkV1,index_work:&mut RuntimeHistoryIndexWorkV1,parent_read:&mut WorldFieldReadContextV1<'_,P,R,PE,PC>,common:&mut FormalBoundedContextV1<'_>,live_owner:&mut dyn FnMut()->ExecutionFence)->Result<ReservedValue<HistoryWrittenIwsCommitmentV1>,RootModuleExecutionErrorV1<O::Error>>{
  let result=std::panic::catch_unwind(std::panic::AssertUnwindSafe(||{
   parent_source.validate_context(common).map_err(|e|RootModuleExecutionErrorV1::Body(BodyMutationError::Original(Box::new(e))))?;
   input.validate_context(common).map_err(|e|RootModuleExecutionErrorV1::Body(BodyMutationError::Original(Box::new(e))))?;
   let bound=NormalTailSourcesV1::from_bound_parent(parent_source);
   let sources=NormalGameplayCycleSourcesV1{roots:bound.roots(),directory:gameplay_directory,diplomacy:bound.native().diplomacy(),governance:bound.native().governance(),native:bound.native().native(),agents:bound.raw().agents(),retention};
   let live=live_owner();let journal=self.collect_iws_tick_journal_v1(parent_source.refs().runtime,&sources,parent_read,gameplay_work,common,&live)?;
   let source=self.prepare_iws_consensus_source_v1(parent_source,index_work,common,live_owner)?;
   let cut=self.prepare_iws_current_commitment_v1(parent_source,input,common,live_owner)?;
   self.persist_iws_tick_consensus_v1(cut,journal,source,parent_source,input,index_work,common,live_owner)
  }));
  match result{Ok(Ok(value))=>Ok(value),Ok(Err(error))=>{let _:Result<(),ExecutionError>=self.ctx.transaction.contribute(|_|Err(ExecutionError::Aborted));Err(error)},Err(payload)=>{let _:Result<(),ExecutionError>=self.ctx.transaction.contribute(|_|Err(ExecutionError::Aborted));std::panic::resume_unwind(payload)}}
 }
}
