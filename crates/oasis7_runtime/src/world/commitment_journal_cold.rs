//! Explicit cold-only complete WorldJournal recovery from the actual accepted
//! Runtime History window. No resident cache, inferred event slice or empty
//! substitute is an input to this fixed reader.
use super::*;
use crate::world::incremental_normal_industry_due::delivery::{ProductValidationDeliveryWorkV1,DeliverySourceErrorV1,read_gameplay_journal_window_position_v1,read_gameplay_world_event_original_v1};
use oasis7_formal_protocol::{incremental_material::OriginalObjectStreamReader,incremental_value::ValueNodeReader};
use oasis7_world_state::{incremental_field_read::WorldFieldReadContextV1,incremental_runtime_history_manifest::RuntimeHistoryLaneV1};
use std::cell::RefCell;
#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn prepare_cold_journal_projection_v1<M,S,R,E,C,O,K>(
 parent:&BoundNormalTickParentV1<'_>,metadata:&NormalTerminalReadMetadataV1,
 read:&mut WorldFieldReadContextV1<'_,S,R,E,C>,originals:&mut O,
 work:&mut ProductValidationDeliveryWorkV1,max_event_json_bytes:usize,
 tx:&ExecutionTransaction<'_,M>,common:&mut FormalBoundedContextV1<'_>,
 cancelled:&mut K,live:&ExecutionFence,
)->Result<ActualCurrentJournalProjectionV1,DeliverySourceErrorV1<O::Error>>
where M:MerkleNodeReader,S:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool,O:OriginalObjectStreamReader,K:FnMut()->bool{
 parent.validate_context(common)?;tx.validate_bootstrap_read_only_v1(live)?;metadata.validate_provenance(tx,common,live)?;
 common.charge(1,64)?;
 if tx.parent_fence()!=parent.fence()||!read.values.memory.same_pool(common.memory())||read.values.class!=MemoryClass::Ordinary{return Err(invalid("cold journal actual parent/read context").into())}
 let window=&parent.refs().runtime.manifest().lanes[RuntimeHistoryLaneV1::Journal.index()];
 common.consume_steps(1)?;if metadata.journal_retained_event_count_v1()!=window.count{return Err(invalid("cold journal metadata/window count mismatch").into())}
 let end=window.start.checked_add(window.count).ok_or_else(||invalid("cold journal window end overflow"))?;
 let expected=usize::try_from(window.count).map_err(|_|invalid("cold journal count does not fit process"))?;
 let memory=common.memory().clone();let n=size_of::<ActualCurrentJournalProjectionV1>();common.charge(1,n as u64)?;let fee=memory.reserve(n,MemoryClass::Ordinary)?;
 // The empty initial array is a builder only. It is returned only after all
 // actual count rows, originals and the authentic header have been checked.
 common.charge(1,size_of::<serde_json::Value>()as u64)?;let _initial=memory.reserve(size_of::<serde_json::Value>(),MemoryClass::Ordinary)?;
 let mut events=ProjectionColumnDataV1::from_json_data(serde_json::Value::Array(Vec::new()),&memory,&mut|s,b|common.charge(s,b)).map_err(graph_error)?;
 let mut seen=0usize;let mut previous=None;let mut first_latest=None;let mut latest_start=0usize;
 for ordinal in window.start..end{
  common.consume_steps(1)?;if cancelled(){return Err(ValueTreeError::Cancelled.into())}
  let row=read_gameplay_journal_window_position_v1(window,ordinal,read,work,common)?;
  // Exact encoded length, original digest, full owned WorldEvent layout and
  // canonical original bytes are all checked by this one existing reader.
  let(decoded,encoded)=read_gameplay_world_event_original_v1(&row.location.original,Some(row.encoded_bytes),originals,work,common,cancelled)?;
  common.charge(1,24)?;if encoded!=row.encoded_bytes{return Err(invalid("cold journal original encoded length changed").into())}
  let event=decoded.event();let id=event.id;
  // Equivalent to the original reverse search for the last non-increasing
  // adjacent ID pair, without cloning any event graph to a staging Vec.
  if previous.is_none_or(|old|old>=id){first_latest=Some(id);latest_start=seen}previous=Some(id);
  let json=super::super::initial::capture_bounded_v1(common,max_event_json_bytes,|output|serde_json::to_writer(output,event).map_err(paid))?;
  let(value,_phase)={let shared=RefCell::new(&mut *common);super::super::fact_decode::recover_json_value_parts_data_v1(&json,&shared)?};
  let value=ProjectionColumnDataV1::from_json_data(value,&memory,&mut|s,b|common.charge(s,b)).map_err(graph_error)?;
  events=events.with_array_append_data_v1(&value,&memory,&mut|s,b|common.charge(s,b)).map_err(graph_error)?;
  // Both the paid original decoder and its JSON phase stay alive until the
  // persistent array owns the actual complete event DATA.
  common.consume_steps(0)?;drop(decoded);seen=seen.checked_add(1).ok_or_else(||invalid("cold journal row count overflow"))?;
 }
 common.charge(1,48)?;
 if seen!=expected||events.array_len_data_v1(&memory,&mut|s,b|common.charge(s,b)).map_err(graph_error)?!=expected{return Err(invalid("cold journal complete source count mismatch").into())}
 let high=metadata.journal_high_watermark();if previous.is_some_and(|last|last!=high){return Err(invalid("cold journal last original differs from actual high watermark").into())}
 let bounds=oasis7_world_projection::current_commitment_history::original_journal_bounds_from_actual_suffix_v1(latest_start,first_latest,high,metadata.next_event_id_era_v1());
 if bounds.start!=metadata.journal_current_era_start_v1()||bounds.floor!=metadata.journal_retention_floor()||bounds.high!=metadata.journal_high_watermark()||bounds.era!=metadata.event_id_era(){return Err(invalid("cold journal originals differ from actual header bounds").into())}
 metadata.validate_provenance(tx,common,live)?;tx.validate_bootstrap_read_only_v1(live)?;
 let source=ActualCurrentJournalProjectionV1{events,next_event_id:metadata.next_event_id_v1(),next_event_id_era:metadata.next_event_id_era_v1(),guard:tx.guard(),core:common.work().identity(),parent_record:parent.refs().head.verified_record_id(),memory,_fee:fee};
 source.validate(parent,tx,common,live)?;Ok(source)
}
