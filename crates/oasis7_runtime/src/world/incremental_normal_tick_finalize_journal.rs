//! Authenticated retained Journal cut for the independent IWS tick record.
//! Event IDs may wrap: positions and original IDs, never numeric-ID filtering,
//! determine ordering. Every decoded source remains paid until this cut drops.
use crate::world::{incremental_body::BodyMutationError,incremental_body_event::AgentEventOriginalV1,incremental_module_invocation::*,incremental_normal_gameplay_cycles::{NormalGameplayCycleSourcesV1,NormalGameplayCycleWorkV1},incremental_normal_industry_due::delivery::{read_gameplay_journal_position_v1,read_gameplay_world_event_original_v1}};
use oasis7_formal_protocol::{controlled_authority::formal::bounded_context::FormalBoundedContextV1,incremental_execution::{ExecutionFence,PreparedGuard,ExecutionTransaction},incremental_material::OriginalObjectStreamReader,incremental_state::{MerkleNodeReader,StatePageWriter},incremental_schedule::SchedulePageWriter,incremental_value::{ValueNodeReader,ValueNodeWriter,ValueTreeError}};
use oasis7_proto::memory_budget::{ReservedValue,MemoryClass};
use oasis7_world_state::{incremental_field_read::WorldFieldReadContextV1,incremental_runtime_history_source::AcceptedRuntimeHistorySourceV1,incremental_runtime_history_manifest::RuntimeHistoryLaneV1,incremental_world_event_decode::DecodedWorldEventV1};
struct TickJournalRowV1{original:[u8;32],event:ReservedValue<DecodedWorldEventV1>}
pub(in crate::world) struct PaidIwsTickJournalV1{rows:Vec<TickJournalRowV1>,_heap:ReservedValue<()>,guard:PreparedGuard,tick:u64,fact_count:usize,identity:oasis7_formal_protocol::controlled_authority::formal::bounded_context::WorkIdentityV1}
impl PaidIwsTickJournalV1{
 pub(in crate::world) fn validate<M:MerkleNodeReader>(&self,tx:&ExecutionTransaction<'_,M>,common:&mut FormalBoundedContextV1<'_>,live:&ExecutionFence)->Result<(),BodyMutationError>{tx.validate_guard(&self.guard,live)?;common.charge(1,0).map_err(|e|BodyMutationError::Original(Box::new(e)))?;if self.identity!=common.work().identity()||!self._heap.uses_budget(common.memory())||!tx.uses_budget(common.memory()){return Err(BodyMutationError::Invalid("IWS Journal cut pool"))}Ok(())}
 /// This cut is historical data, not permission to read Current after mutation.
 pub(in crate::world) fn validate_evidence<M:MerkleNodeReader>(&self,tx:&ExecutionTransaction<'_,M>,common:&mut FormalBoundedContextV1<'_>,live:&ExecutionFence)->Result<(),BodyMutationError>{tx.validate_transaction_identity(&self.guard)?;tx.validate_guard(&tx.guard(),live)?;common.charge(1,0).map_err(|e|BodyMutationError::Original(Box::new(e)))?;if self.identity!=common.work().identity()||!self._heap.uses_budget(common.memory())||self.fact_count!=tx.ordered_fact_count(){return Err(BodyMutationError::Invalid("IWS Journal cut occurrence"))}Ok(())}
 pub(in crate::world) fn iter(&self)->impl ExactSizeIterator<Item=(&[u8;32],&crate::WorldEvent)>{self.rows.iter().map(|r|(&r.original,r.event.event()))}
 pub(in crate::world) fn tick(&self)->u64{self.tick}
}
fn bad()->ValueTreeError{ValueTreeError::Invalid("IWS retained Journal cut")}
impl<'a,'t,'s,'v,M,R,E,C,W,F,O> RootModuleExecutionContextV1<'a,'t,'s,'v,M,R,E,C,W,F,O>
where M:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool,W:ValueNodeReader+ValueNodeWriter+StatePageWriter+SchedulePageWriter,F:FnMut(&mut dyn AgentEventOriginalV1)->Result<(),BodyMutationError>,O:OriginalObjectStreamReader{
 pub(in crate::world) fn collect_iws_tick_journal_v1<P:MerkleNodeReader,V:ValueNodeReader,PE:FnMut()->Option<ValueTreeError>,PC:FnMut()->bool>(&mut self,history:&ReservedValue<AcceptedRuntimeHistorySourceV1>,sources:&NormalGameplayCycleSourcesV1<'_>,read:&mut WorldFieldReadContextV1<'_,P,V,PE,PC>,work:&mut NormalGameplayCycleWorkV1,common:&mut FormalBoundedContextV1<'_>,live:&ExecutionFence)->Result<ReservedValue<PaidIwsTickJournalV1>,RootModuleExecutionErrorV1<O::Error>>{
 let result=(||{
 self.validate_gameplay_journal_v1(history,sources,common)?;
 let window=&history.manifest().lanes[RuntimeHistoryLaneV1::Journal.index()];let notes=self.gameplay_journal_notes_v1(window.count,common)?;
 let total=window.count.checked_add(notes.len() as u64).ok_or_else(bad)?;let retained=total.min(sources.retention.max_events() as u64);let floor=total-retained;
 // The capacity is the actual retained slice, not a caller work limit.
 let capacity=usize::try_from(retained).map_err(|_|bad())?;let bytes=capacity.checked_mul(std::mem::size_of::<TickJournalRowV1>()).ok_or_else(bad)?;
 let heap=common.memory().reserve(bytes,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?.retain(());let owner=common.memory().reserve(std::mem::size_of::<PaidIwsTickJournalV1>(),MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
 let mut rows=Vec::new();rows.try_reserve_exact(capacity).map_err(|_|bad())?;if rows.capacity()>capacity{return Err(bad().into())}
 for position in floor..total{
 common.charge(1,40).map_err(|e|RootModuleExecutionErrorV1::Body(BodyMutationError::Original(Box::new(e))))?;
 if (self.ctx.cancelled)(){return Err(ValueTreeError::Cancelled.into())}
 let(id,encoded)=if position<window.count{let row=read_gameplay_journal_position_v1(history,window.start.checked_add(position).ok_or_else(bad)?,read,&mut work.journal,common).map_err(|e|RootModuleExecutionErrorV1::ProductDelivery(e.into()))?;(row.location.original,Some(row.encoded_bytes))}else{let ordinal=usize::try_from(position-window.count).map_err(|_|bad())?;let note=notes.get(notes.len().checked_sub(ordinal+1).ok_or_else(bad)?).ok_or_else(bad)?;note.validate(self.ctx.transaction).map_err(RootModuleExecutionErrorV1::Body)?;(*note.original(),None)};
 let(event,_)=read_gameplay_world_event_original_v1(&id,encoded,self.originals,&mut work.journal,common,&mut self.ctx.cancelled).map_err(RootModuleExecutionErrorV1::ProductDelivery)?;
 if event.event().time==self.now{rows.push(TickJournalRowV1{original:id,event});}
 }
 let guard=self.ctx.transaction.guard();self.ctx.transaction.validate_guard(&guard,live)?;
 Ok(owner.retain(PaidIwsTickJournalV1{rows,_heap:heap,guard,tick:self.now,fact_count:self.ctx.transaction.ordered_fact_count(),identity:common.work().identity()}))
 })();if result.is_err(){let _:Result<(),oasis7_formal_protocol::incremental_execution::ExecutionError>=self.ctx.transaction.contribute(|_|Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted));}result
 }
}
