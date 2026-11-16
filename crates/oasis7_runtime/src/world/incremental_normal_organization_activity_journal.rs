//! Resolve the actual Journal tail anew for each organization close.
use crate::world::{incremental_module_invocation::*, incremental_body::BodyMutationError,
    incremental_normal_industry_due::delivery::{ProductValidationDeliveryWorkV1,
        read_gameplay_journal_position_v1, read_gameplay_world_event_original_v1}};
use oasis7_formal_protocol::{controlled_authority::formal::bounded_context::FormalBoundedContextV1,
    incremental_material::OriginalObjectStreamReader, incremental_state::{MerkleNodeReader, StatePageWriter},
    incremental_schedule::SchedulePageWriter, incremental_value::{ValueNodeReader, ValueNodeWriter, ValueTreeError}};
use oasis7_proto::memory_budget::ReservedValue;
use oasis7_world_state::{incremental_field_read::WorldFieldReadContextV1,
    incremental_runtime_history_source::AcceptedRuntimeHistorySourceV1,
    incremental_runtime_history_manifest::RuntimeHistoryLaneV1,
    incremental_world_event_decode::DecodedWorldEventV1};
impl<'a,'t,'s,'v,M,R,E,C,W,F,O> RootModuleExecutionContextV1<'a,'t,'s,'v,M,R,E,C,W,F,O>
where M:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool,
 W:ValueNodeReader+ValueNodeWriter+StatePageWriter+SchedulePageWriter,
 F:FnMut(&mut dyn crate::world::incremental_body_event::AgentEventOriginalV1)->Result<(),BodyMutationError>,
 O:OriginalObjectStreamReader {
 pub(in crate::world) fn organization_activity_last_journal_v1<P,PE,PC>(
 &mut self, history:&ReservedValue<AcceptedRuntimeHistorySourceV1>,
 read:&mut WorldFieldReadContextV1<'_,P,R,PE,PC>,
 work:&mut ProductValidationDeliveryWorkV1, common:&mut FormalBoundedContextV1<'_>
 )->Result<Option<ReservedValue<DecodedWorldEventV1>>,RootModuleExecutionErrorV1<O::Error>>
 where P:MerkleNodeReader,PE:FnMut()->Option<ValueTreeError>,PC:FnMut()->bool {
 common.consume_steps(1).map_err(|e|RootModuleExecutionErrorV1::Body(BodyMutationError::Original(Box::new(e))))?;
 if !history.uses_budget(common.memory()) || history.manifest().roots!=self.ctx.transaction.parent_fence().parent_roots {
 return Err(ValueTreeError::Invalid("organization activity Journal parent").into());
 }
 let latest=self.events.world_events.latest_source(self.ctx.transaction).map_err(RootModuleExecutionErrorV1::Body)?;
 let (original,encoded)=if let Some(source)=latest {
 source.validate(self.ctx.transaction).map_err(RootModuleExecutionErrorV1::Body)?;
 (*source.original(),None)
 } else {
 let window=&history.manifest().lanes[RuntimeHistoryLaneV1::Journal.index()];
 if window.count==0{return Ok(None)}
 let position=window.end()?.checked_sub(1).ok_or(ValueTreeError::Invalid("organization activity Journal end"))?;
 let row=read_gameplay_journal_position_v1(history,position,read,work,common)
 .map_err(|e|RootModuleExecutionErrorV1::ProductDelivery(e.into()))?;
 (row.location.original,Some(row.encoded_bytes))
 };
 let (event,_)=read_gameplay_world_event_original_v1(&original,encoded,self.originals,work,common,&mut self.ctx.cancelled)
 .map_err(RootModuleExecutionErrorV1::ProductDelivery)?;
 Ok(Some(event))
 }
}
