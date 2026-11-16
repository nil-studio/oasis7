//! Original current-era audience event suffix from the real auxiliary journal.
use super::*;
use oasis7_world_projection::current_commitment::{ProjectionColumnDataV1,ProjectionColumnErrorV1};
use std::{cell::RefCell,mem::size_of,ops::ControlFlow};
fn col(e:ProjectionColumnErrorV1<BodyMutationError>)->BodyMutationError{match e{ProjectionColumnErrorV1::Work(e)=>e,ProjectionColumnErrorV1::Memory(e)=>ValueTreeError::Memory(e).into(),ProjectionColumnErrorV1::Invalid(note)=>invalid(note)}}
pub(super) struct CurrentEventsFeesV1{_rows:Vec<MemoryReservation>,_slots:MemoryReservation}
pub(super) fn events_data_v1(aux:&ProjectionColumnDataV1,metadata:&crate::world::incremental_normal_business_commitment::OriginalProjectionReadMetadataV1<'_>,reader:Option<&str>,common:&mut FormalBoundedContextV1<'_>)->Result<(Vec<oasis7_world_state::WorldEvent>,CurrentEventsFeesV1),BodyMutationError>{
 let memory=common.memory().clone();if !aux.uses_budget(&memory){return Err(invalid("projection event journal pool"))}
 let node=aux.object_member_data("world_journal",&memory,&mut|s,b|common.charge(s,b).map_err(paid)).map_err(col)?.ok_or_else(||invalid("projection actual event journal missing"))?;
 let journal=node.share_data(&memory,&mut|s,b|common.charge(s,b).map_err(paid)).map_err(col)?;let count=journal.array_len_data_v1(&memory,&mut|s,b|common.charge(s,b).map_err(paid)).map_err(col)?;
 if u64::try_from(count).map_err(|_|invalid("projection journal count"))?!=metadata.journal_retained_event_count_v1(){return Err(invalid("projection journal actual count differs from metadata"))}
 let start=metadata.journal_current_era_start_v1();let capacity=count.checked_sub(start).ok_or_else(||invalid("projection journal current era starts outside retained source"))?;let n=size_of::<Vec<oasis7_world_state::WorldEvent>>().checked_add(size_of::<Vec<MemoryReservation>>()).and_then(|n|n.checked_add(capacity.checked_mul(size_of::<oasis7_world_state::WorldEvent>()+size_of::<MemoryReservation>())?)).ok_or_else(||invalid("projection event suffix layout"))?;
 common.charge(1,n as u64).map_err(paid)?;let slots=memory.reserve(n,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;let mut events=Vec::with_capacity(capacity);let mut fees=Vec::with_capacity(capacity);if events.capacity()!=capacity||fees.capacity()!=capacity{return Err(invalid("projection event suffix capacity"))}
 let shared=RefCell::new(common);journal.root_data().array_elements_data_v1(&memory,&mut|s,b|shared.borrow_mut().charge(s,b).map_err(paid),&mut|index,node,_|{
  if index<start{return Ok(ControlFlow::Continue(()))}
  let raw=super::shared_json::capture_shared_v1(&node,&shared,&RefCell::new(None),usize::MAX).map_err(ProjectionColumnErrorV1::Work)?;
  let(event,fee)=super::fact_decode::recover_world_event_parts_data_v1(&raw,&shared).map_err(ProjectionColumnErrorV1::Work)?;
  shared.borrow_mut().charge(1,raw.len()as u64).map_err(paid).map_err(ProjectionColumnErrorV1::Work)?;
  if oasis7_world_projection::current_commitment_history::original_projection_event_visible_v1(&event,reader){events.push(event);fees.push(fee)}Ok(ControlFlow::Continue(()))
 }).map_err(col)?;shared.borrow_mut().consume_steps(0).map_err(paid)?;Ok((events,CurrentEventsFeesV1{_rows:fees,_slots:slots}))
}
