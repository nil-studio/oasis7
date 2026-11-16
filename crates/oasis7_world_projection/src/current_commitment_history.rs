//! The original committed-projection event-era and visibility rules. This
//! readonly DATA kernel is shared by Legacy formatting and Current capture.
use oasis7_world_state::{WorldEvent,WorldEventBody};
use oasis7_proto::memory_budget::{MemoryBudget,MemoryClass,MemoryReservation};
use super::current_commitment::ProjectionColumnErrorV1;
use serde::{Serialize,Serializer,ser::SerializeSeq};
#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub struct ProjectionJournalBoundsV1{pub start:usize,pub floor:u64,pub high:u64,pub era:u64}
pub fn original_journal_bounds_with_work_v1<E>(events:&[WorldEvent],last_event_id:u64,event_id_era:u64,work:&mut dyn FnMut(u64,u64)->Result<(),E>)->Result<ProjectionJournalBoundsV1,E>{
 let mut start=0;for(index,pair)in events.windows(2).enumerate().rev(){work(1,2*std::mem::size_of::<u64>()as u64)?;if pair[0].id>=pair[1].id{start=index+1;break}}
 work(1,3*std::mem::size_of::<u64>()as u64)?;
 Ok(original_journal_bounds_from_actual_suffix_v1(start,events.get(start).map(|event|event.id),last_event_id,event_id_era))
}
/// Fixed header normalization from an already genuine latest-era suffix.
/// Normal capture derives these facts from its resident parent plus actual
/// retained count/new-event IDs; this function grants no execution authority.
pub fn original_journal_bounds_from_actual_suffix_v1(start:usize,first_actual_id:Option<u64>,high:u64,event_id_era:u64)->ProjectionJournalBoundsV1{
 ProjectionJournalBoundsV1{start,floor:first_actual_id.unwrap_or_else(||high.saturating_add(1)),high,era:event_id_era.saturating_sub(u64::from(high==u64::MAX))}
}

pub fn original_projection_event_visible_v1(event:&WorldEvent,agent:Option<&str>)->bool{match &event.body{WorldEventBody::Domain(domain)=>agent.is_some_and(|agent|domain.agent_id()==Some(agent)),_=>false}}
/// Selection borrows the original paid event graph. Only its index vector is
/// allocated; no WorldEvent/body/string clone or detached uncharged output.
pub struct ProjectionJournalEventsDataV1<'a>{events:&'a[WorldEvent],selected:Vec<usize>,_fee:MemoryReservation}
impl<'a> ProjectionJournalEventsDataV1<'a>{
 pub fn from_original_events_with_work_v1<E>(events:&'a[WorldEvent],start:usize,agent:Option<&str>,memory:&MemoryBudget,work:&mut dyn FnMut(u64,u64)->Result<(),E>)->Result<Self,ProjectionColumnErrorV1<E>>{
  if start>events.len(){return Err(ProjectionColumnErrorV1::Invalid("projection event suffix start"))}
  let capacity=events.len()-start;let bytes=capacity.checked_mul(std::mem::size_of::<usize>()).and_then(|n|n.checked_add(std::mem::size_of::<Self>())).ok_or(ProjectionColumnErrorV1::Invalid("projection event selection layout"))?;
  work(1,bytes as u64).map_err(ProjectionColumnErrorV1::Work)?;let fee=memory.reserve(bytes,MemoryClass::Ordinary).map_err(ProjectionColumnErrorV1::Memory)?;let mut selected=Vec::with_capacity(capacity);if selected.capacity()!=capacity{return Err(ProjectionColumnErrorV1::Invalid("projection event selection capacity"))}
  for(index,event)in events.iter().enumerate().skip(start){let event_agent=match &event.body{WorldEventBody::Domain(domain)=>domain.agent_id(),_=>None};work(1,agent.map_or(0,str::len).saturating_add(event_agent.map_or(0,str::len))as u64).map_err(ProjectionColumnErrorV1::Work)?;if original_projection_event_visible_v1(event,agent){selected.push(index)}}
  Ok(Self{events,selected,_fee:fee})
 }
 pub fn uses_budget(&self,memory:&MemoryBudget)->bool{self._fee.uses_budget(memory)}
}
impl Serialize for ProjectionJournalEventsDataV1<'_>{fn serialize<S:Serializer>(&self,serializer:S)->Result<S::Ok,S::Error>{let mut output=serializer.serialize_seq(Some(self.selected.len()))?;for index in &self.selected{output.serialize_element(&self.events[*index])?}output.end()}}

#[cfg(test)]mod tests{
 use super::*;
 #[test]fn empty_suffix_uses_high_plus_one_and_preserves_non_wrap_era(){assert_eq!(original_journal_bounds_from_actual_suffix_v1(0,None,41,7),ProjectionJournalBoundsV1{start:0,floor:42,high:41,era:7});}
 #[test]fn max_high_adjusts_exactly_one_era_and_never_wraps_floor(){assert_eq!(original_journal_bounds_from_actual_suffix_v1(5,None,u64::MAX,3),ProjectionJournalBoundsV1{start:5,floor:u64::MAX,high:u64::MAX,era:2});assert_eq!(original_journal_bounds_from_actual_suffix_v1(5,Some(9),u64::MAX,0).era,0);}
 #[test]fn original_empty_scan_polls_before_returning(){let mut calls=0;let result=original_journal_bounds_with_work_v1(&[],3,0,&mut|_,_|{calls+=1;Err("cancelled")});assert_eq!(result,Err("cancelled"));assert_eq!(calls,1);}
}
