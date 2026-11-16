//! Independent IWS tick production from the real Current cut and retained
//! Journal. The returned source proves this provisional historical occurrence;
//! it grants neither Node finality nor completed-tick/accepted-cache authority.
use crate::world::{incremental_body::BodyMutationError,incremental_body_event::AgentEventOriginalV1,incremental_module_invocation::*,incremental_normal_sources::BoundNormalTickParentV1};
use super::{commitment::PaidIwsCurrentCommitmentV1,journal::PaidIwsTickJournalV1};
use oasis7_consensus::normal_tick_input::PaidBoundNormalInputDataV1;
use oasis7_formal_protocol::{controlled_authority::formal::bounded_context::{FormalBoundedContextV1,WorkIdentityV1},incremental_execution::{ExecutionFence,ExecutionTransaction,PreparedGuard,ExecutionFactKindV1,ExecutionFactRefV1},incremental_material::{OriginalObjectStreamReader,FactLocationV1},incremental_state::{MerkleNodeReader,StatePageWriter,StateNamespaceV1},incremental_value::{ValueNodeReader,ValueNodeWriter,ValueReadContext,ValueTreeError}};
use oasis7_proto::memory_budget::{ReservedValue,MemoryClass,MemoryReservation};
use oasis7_world_state::{incremental_iws_tick_consensus::*,incremental_field_read::WorldFieldReadContextV1,incremental_runtime_history_index::*,incremental_runtime_history_manifest::RuntimeHistoryLaneV1,incremental_runtime_history_original::{with_runtime_history_original_v1,RuntimeHistoryOriginalRefV1},incremental_runtime_controls::{RuntimeControlFieldV1,RuntimeControlReadV1,read_runtime_control_overlay_v1},incremental_world_text_maps::{WorldTextMapFieldV1,read_world_text_map_overlay_v1}};
fn bad()->ValueTreeError{ValueTreeError::Invalid("IWS tick consensus actual source")}
fn formal(error:impl std::error::Error+Send+Sync+'static)->BodyMutationError{BodyMutationError::Original(Box::new(error))}
fn overlay(error:oasis7_world_state::incremental_field_overlay::WorldFieldOverlayErrorV1)->BodyMutationError{match error{oasis7_world_state::incremental_field_overlay::WorldFieldOverlayErrorV1::Value(e)=>e.into(),oasis7_world_state::incremental_field_overlay::WorldFieldOverlayErrorV1::Execution(e)=>e.into(),oasis7_world_state::incremental_field_overlay::WorldFieldOverlayErrorV1::Member(e)=>formal(e)}}
/// Private actual-Current authority and genuine parent Tick-index lineage.
/// It is produced before the strict Current preview, never by a caller ID.
struct PaidIwsConsensusSourceV1{authority:ReservedValue<String>,previous:Option<[u8;32]>,height:u64,guard:PreparedGuard,identity:WorkIdentityV1,parent_record:[u8;32]}
pub(in crate::world) struct HistoryWrittenIwsCommitmentV1{
 cut:ReservedValue<PaidIwsCurrentCommitmentV1>,journal:ReservedValue<PaidIwsTickJournalV1>,source:ReservedValue<PaidIwsConsensusSourceV1>,record:ReservedValue<IwsTickConsensusRecordV1>,index:ReservedValue<RuntimeHistoryIndexQualificationV1>,location:FactLocationV1,encoded_bytes:u64,guard:PreparedGuard,identity:WorkIdentityV1,_copy:MemoryReservation,
}
impl HistoryWrittenIwsCommitmentV1{
 pub(in crate::world) fn record(&self)->&IwsTickConsensusRecordV1{&self.record}
 pub(in crate::world) fn cut(&self)->&PaidIwsCurrentCommitmentV1{&self.cut}
 pub(in crate::world) fn location(&self)->FactLocationV1{self.location}
 pub(in crate::world) fn encoded_bytes(&self)->u64{self.encoded_bytes}
 pub(in crate::world) fn index(&self)->&ReservedValue<RuntimeHistoryIndexQualificationV1>{&self.index}
 /// Historical evidence only. The old preview's strict guard is deliberately
 /// not refreshed: a consumer needing Current must create a new real preview.
 pub(in crate::world) fn validate_evidence<M:MerkleNodeReader>(&self,tx:&ExecutionTransaction<'_,M>,common:&mut FormalBoundedContextV1<'_>,live:&ExecutionFence)->Result<(),BodyMutationError>{
  tx.validate_transaction_identity(&self.guard)?;tx.validate_guard(&tx.guard(),live)?;common.charge(2,96).map_err(formal)?;
  let ordinal=usize::try_from(self.location.ordinal).map_err(|_|bad())?;
  if self.identity!=common.work().identity()||!self.cut.uses_budget(common.memory())||!self.journal.uses_budget(common.memory())||!self.source.uses_budget(common.memory())||!self.record.uses_budget(common.memory())||!self.index.uses_budget(common.memory())||self.location.height!=self.cut.roots().height||!tx.ordered_fact(ordinal).is_some_and(|f|f.kind==ExecutionFactKindV1::Result&&f.id==self.location.original){return Err(bad().into())}Ok(())
 }
}
impl<'a,'t,'s,'v,M,R,E,C,W,F,O> RootModuleExecutionContextV1<'a,'t,'s,'v,M,R,E,C,W,F,O>
where M:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool,W:ValueNodeReader+ValueNodeWriter+StatePageWriter,F:FnMut(&mut dyn AgentEventOriginalV1)->Result<(),BodyMutationError>,O:OriginalObjectStreamReader{
 fn prepare_iws_consensus_source_v1(&mut self,parent:&ReservedValue<BoundNormalTickParentV1<'_>>,index_work:&mut RuntimeHistoryIndexWorkV1,common:&mut FormalBoundedContextV1<'_>,live_owner:&mut dyn FnMut()->ExecutionFence)->Result<ReservedValue<PaidIwsConsensusSourceV1>,RootModuleExecutionErrorV1<O::Error>>{
  parent.validate_context(common).map_err(|e|RootModuleExecutionErrorV1::Body(formal(e)))?;
  common.charge(3,8192).map_err(|e|RootModuleExecutionErrorV1::Body(formal(e)))?;
  let history=parent.refs().runtime;let roots=parent.refs().roots;let window=&history.manifest().lanes[RuntimeHistoryLaneV1::Tick.index()];
  if !history.uses_budget(common.memory())||history.allocation_class()!=MemoryClass::Ordinary||history.manifest().roots!=parent.fence().parent_roots||history.history_index(RuntimeHistoryLaneV1::Tick).window()!=window||self.ctx.transaction.parent_fence()!=parent.fence(){return Err(bad().into())}
  let owner=common.memory().reserve(std::mem::size_of::<PaidIwsConsensusSourceV1>(),MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
  let identity=common.work().identity();let record=parent.refs().core.candidate_record_id();
  with_current_root_history_index_meter_v1(self,common,live_owner,index_work,|root,live,index_work|{
   let mut discard=std::io::sink();let max=root.ctx.record_bytes;
   let RuntimeControlReadV1::Text{utf8_bytes}=read_runtime_control_overlay_v1(RuntimeControlFieldV1::TickConsensusAuthoritySource,root.ctx,max,&mut discard).map_err(|e|RootModuleExecutionErrorV1::Body(overlay(e)))?else{return Err(bad().into())};
   root.work.input.bytes_remaining=root.work.input.bytes_remaining.checked_sub(utf8_bytes.checked_mul(3).ok_or_else(bad)?).ok_or(ValueTreeError::ResourceLimited("IWS authority trim/copy bytes"))?;
   let bytes=usize::try_from(utf8_bytes).map_err(|_|bad())?;let fee=root.ctx.values.memory.reserve(std::mem::size_of::<String>().checked_add(bytes).ok_or_else(bad)?,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
   let mut raw=Vec::new();raw.try_reserve_exact(bytes).map_err(|_|bad())?;if raw.capacity()>bytes{return Err(bad().into())}
   let read=read_runtime_control_overlay_v1(RuntimeControlFieldV1::TickConsensusAuthoritySource,root.ctx,max,&mut raw).map_err(|e|RootModuleExecutionErrorV1::Body(overlay(e)))?;
   if read!=(RuntimeControlReadV1::Text{utf8_bytes})||raw.len()!=bytes{return Err(bad().into())}
   let mut authority=String::from_utf8(raw).map_err(|_|bad())?;
   // Original trim and trust lookup: empty public-key text still counts as an
   // existing binding, exactly as World's Option<&str> lookup did.
   let trimmed=authority.trim();if trimmed.is_empty(){return Err(bad().into())}let start=trimmed.as_ptr()as usize-authority.as_ptr()as usize;let len=trimmed.len();authority.drain(..start);authority.truncate(len);
   if read_world_text_map_overlay_v1(WorldTextMapFieldV1::NodeIdentityBindings,&authority,root.ctx,max,&mut discard).map_err(|e|RootModuleExecutionErrorV1::Body(overlay(e)))?.is_none(){return Err(bad().into())}
   let previous=if window.total==0{None}else{
    let mut read=WorldFieldReadContextV1{root:roots.state_root(StateNamespaceV1::Artifacts),state:root.state,values:ValueReadContext{source:root.ctx.values.source,memory:root.ctx.values.memory,class:root.ctx.values.class,io:&mut*root.ctx.values.io},key_work:&mut*root.ctx.key_work,semantic_bytes:root.ctx.semantic_bytes,record_bytes:root.ctx.record_bytes,take_state_error:&mut root.ctx.take_state_error,cancelled:&mut root.ctx.cancelled};
    let row=read_runtime_history_index_point_v1(RuntimeHistoryLaneV1::Tick,window,window.total-1,&mut read,index_work)?;Some(row.location.original)
   };
   let guard=root.ctx.transaction.guard();root.ctx.transaction.validate_guard(&guard,&live())?;
   Ok(owner.retain(PaidIwsConsensusSourceV1{authority:fee.retain(authority),previous,height:window.total.checked_add(1).ok_or_else(bad)?,guard,identity,parent_record:record}))
  })
 }
 fn persist_iws_tick_consensus_v1(&mut self,cut:ReservedValue<PaidIwsCurrentCommitmentV1>,journal:ReservedValue<PaidIwsTickJournalV1>,source:ReservedValue<PaidIwsConsensusSourceV1>,parent:&ReservedValue<BoundNormalTickParentV1<'_>>,input:&PaidBoundNormalInputDataV1<'_>,index_work:&mut RuntimeHistoryIndexWorkV1,common:&mut FormalBoundedContextV1<'_>,live_owner:&mut dyn FnMut()->ExecutionFence)->Result<ReservedValue<HistoryWrittenIwsCommitmentV1>,RootModuleExecutionErrorV1<O::Error>>{
 let result=(||{
  let live=live_owner();cut.validate(self.ctx.transaction,parent,input,common,&live).map_err(RootModuleExecutionErrorV1::Body)?;journal.validate_evidence(self.ctx.transaction,common,&live).map_err(RootModuleExecutionErrorV1::Body)?;self.ctx.transaction.validate_transaction_identity(&source.guard)?;
  if source.identity!=common.work().identity()||!source.uses_budget(common.memory())||!source.authority.uses_budget(common.memory())||source.parent_record!=parent.refs().core.candidate_record_id()||journal.tick()!=self.now{return Err(bad().into())}
  // Both graph copies coexist with their source owners; reserve the complete
  // actual graph before Clone and verify its actual allocated capacities.
  let frame=cut.frame();let strings=[&frame.domain,&frame.manifest_hash,&frame.context.world_id,&frame.context.node_id,&frame.context.proposer_id,&frame.context.node_block_hash,&frame.context.action_root];
  let n=journal.iter().len();let graph=strings.iter().try_fold(std::mem::size_of::<IwsTickConsensusRecordV1>(),|n,s|n.checked_add(s.len()).ok_or_else(bad))?.checked_add(frame.current_roots.len()).and_then(|n|n.checked_add(frame.policy_namespace_root.len())).and_then(|m|m.checked_add(n.checked_mul(std::mem::size_of::<IwsTickEventIdentityV1>()+8)?)).and_then(|n|n.checked_add(source.authority.len()+IWS_TICK_CONSENSUS_DOMAIN_V1.len())).ok_or_else(bad)?;
  common.charge(n as u64+16,graph as u64).map_err(|e|RootModuleExecutionErrorV1::Body(formal(e)))?;let copy=common.memory().reserve(graph,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
  let mut ordered_events=Vec::new();let mut ordered_action_ids=Vec::new();ordered_events.try_reserve_exact(n).map_err(|_|bad())?;ordered_action_ids.try_reserve_exact(n).map_err(|_|bad())?;if ordered_events.capacity()>n||ordered_action_ids.capacity()>n{return Err(bad().into())}
  for(original,event)in journal.iter(){ordered_events.push(IwsTickEventIdentityV1{event_id:event.id,original:*original});if let Some(crate::CausedBy::Action(id))=&event.caused_by{common.charge(ordered_action_ids.len()as u64,ordered_action_ids.len()as u64*8).map_err(|e|RootModuleExecutionErrorV1::Body(formal(e)))?;if !ordered_action_ids.contains(id){ordered_action_ids.push(*id);}}}
  let current=frame.clone();if current.current_roots.capacity()>frame.current_roots.len()||current.policy_namespace_root.capacity()>frame.policy_namespace_root.len()||[&current.domain,&current.manifest_hash,&current.context.world_id,&current.context.node_id,&current.context.proposer_id,&current.context.node_block_hash,&current.context.action_root].iter().zip(strings).any(|(a,b)|a.capacity()>b.len()){return Err(bad().into())}
  let record_fee=common.memory().reserve(std::mem::size_of::<IwsTickConsensusRecordV1>(),MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
  let record=record_fee.retain(IwsTickConsensusRecordV1{domain:IWS_TICK_CONSENSUS_DOMAIN_V1.into(),version:1,consensus_height:source.height,tick:self.now,authority_source:source.authority.to_string(),parent_tick_original:source.previous,current,current_digest:*cut.digest(),ordered_events,ordered_action_ids});let _validation_scratch=common.memory().reserve(4096,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;common.charge(8,8192).map_err(|e|RootModuleExecutionErrorV1::Body(formal(e)))?;record.validate_data()?;
  let owner=common.memory().reserve(std::mem::size_of::<HistoryWrittenIwsCommitmentV1>(),MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;let identity=common.work().identity();let memory=common.memory().clone();
  // Last strict cut check is immediately before any Original/fact/index write.
  cut.validate(self.ctx.transaction,parent,input,common,&live_owner()).map_err(RootModuleExecutionErrorV1::Body)?;
  let history=parent.refs().runtime;let roots=parent.refs().roots;
  let(location,encoded_bytes,index,guard)=with_current_root_history_index_meter_v1(self,common,live_owner,index_work,|root,live,index_work|{
   let ordinal=u64::try_from(root.ctx.transaction.ordered_fact_count()).map_err(|_|bad())?;
   let(id,bytes)=with_runtime_history_original_v1(RuntimeHistoryOriginalRefV1::IwsTick(&record),&memory,&mut root.events.original_work,&mut root.ctx.cancelled,root.events.max_event_bytes,|producer|{let id=producer.original_id();let bytes=producer.original_bytes();(root.events.persist_original)(producer)?;Ok((id,bytes))}).map_err(RootModuleExecutionErrorV1::Body)?;
   root.ctx.transaction.append_fact(ExecutionFactRefV1{kind:ExecutionFactKindV1::Result,id})?;
   let location=FactLocationV1{height:cut.roots().height,ordinal,original:id};
   let mut read=WorldFieldReadContextV1{root:roots.state_root(StateNamespaceV1::Artifacts),state:root.state,values:ValueReadContext{source:root.ctx.values.source,memory:root.ctx.values.memory,class:root.ctx.values.class,io:&mut*root.ctx.values.io},key_work:&mut*root.ctx.key_work,semantic_bytes:root.ctx.semantic_bytes,record_bytes:root.ctx.record_bytes,take_state_error:&mut root.ctx.take_state_error,cancelled:&mut root.ctx.cancelled};
   let index=prepare_runtime_history_index_append_v1(history.history_index(RuntimeHistoryLaneV1::Tick),RuntimeHistoryIndexRowV1{location,encoded_bytes:bytes},&mut read,root.members,index_work).map_err(|e|RootModuleExecutionErrorV1::Body(formal(e)))?;
   let guard=root.ctx.transaction.guard();root.ctx.transaction.validate_guard(&guard,&live())?;Ok((location,bytes,index,guard))
  })?;
  let result=owner.retain(HistoryWrittenIwsCommitmentV1{cut,journal,source,record,index,location,encoded_bytes,guard,identity,_copy:copy});result.validate_evidence(self.ctx.transaction,common,&live_owner()).map_err(RootModuleExecutionErrorV1::Body)?;Ok(result)
 })();if result.is_err(){let _:Result<(),oasis7_formal_protocol::incremental_execution::ExecutionError>=self.ctx.transaction.contribute(|_|Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted));}result
 }
}

#[path="incremental_normal_tick_finalize_consensus_driver.rs"]
mod driver;
