//! Sole original AdvanceAgentOrder phase reducer. Facts are borrowed DATA;
//! neither the decision nor this mutation grants Current/publication authority.
use super::{AgentIndustrialOrderV1, AgentIndustrialPhaseV1 as P, invalid};
use crate::{WorldError, WorldTime, WorldState};
use super::super::industrial_eligibility_kernel::{ProductionEligibilityFactsV1,order_production_eligibility_from_facts_v1};
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum IndustrialAdvancePhaseV1 { Start, Progress, Terminate(&'static str) }
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum IndustrialAdvanceOutcomeV1 { Updated, Terminate(&'static str) }
/// Called only after original order validation/owner/world/terminal gates.
/// It preserves the original deadline branch order before eligibility reads.
pub fn industrial_advance_phase_v1(order:&AgentIndustrialOrderV1,now:WorldTime)->Result<IndustrialAdvancePhaseV1,WorldError>{
 if order.phase==P::Reserved {
  return Ok(if now>=order.confirmed_at.saturating_add(10){IndustrialAdvancePhaseV1::Terminate("start_expired")}else{IndustrialAdvancePhaseV1::Start});
 }
 if order.phase==P::AwaitingDelivery {
  if now<order.produced_at.expect("produced").saturating_add(24){return Err(invalid("delivery_pending"));}
  return Ok(IndustrialAdvancePhaseV1::Terminate("delivery_expired"));
 }
 if now>=order.production_deadline().expect("started"){return Ok(IndustrialAdvancePhaseV1::Terminate("production_expired"));}
 Ok(IndustrialAdvancePhaseV1::Progress)
}
/// Closed data sources; there is no arbitrary predicate implementation. The
/// legacy arm calls its sole lazy resolver, while Current uses the sole fixed
/// production predicate. Current source authentication remains with its owner.
pub enum IndustrialAdvanceEligibilityV1<'a>{LegacyWorld(&'a WorldState),CurrentFacts(ProductionEligibilityFactsV1<'a>)}
impl IndustrialAdvanceEligibilityV1<'_>{
 fn check(&self,order:&AgentIndustrialOrderV1)->Result<(),WorldError>{match self{
  Self::LegacyWorld(state)=>super::order_production_eligibility(state,order),
  Self::CurrentFacts(facts)=>order_production_eligibility_from_facts_v1(*facts,order),
 }}
}
/// Original phase reducer; DATA only, never a Current transition capability.
pub fn advance_industrial_order_from_source_v1(order:&mut AgentIndustrialOrderV1,now:WorldTime,source:IndustrialAdvanceEligibilityV1<'_>)->Result<IndustrialAdvanceOutcomeV1,WorldError>{
 let phase=industrial_advance_phase_v1(order,now)?;
 if let IndustrialAdvancePhaseV1::Terminate(reason)=phase{return Ok(IndustrialAdvanceOutcomeV1::Terminate(reason));}
 match phase{
  IndustrialAdvancePhaseV1::Start=>{
   source.check(order)?;
   order.phase=P::Producing;order.started_at=Some(now);order.ready_at=now.saturating_add(12);order.last_progress_tick=Some(now);
  },
  IndustrialAdvancePhaseV1::Progress=>{
   if let Err(error)=source.check(order){
    order.phase=P::ProductionBlocked;order.blocked_at.get_or_insert(now);order.terminal_reason=Some(format!("production_blocked:{error:?}"));
   }else{
    if order.last_progress_tick.is_some_and(|tick|tick>=now){return Err(invalid("production_tick_already_counted"));}
    order.completed_work_ticks+=1;order.last_progress_tick=Some(now);order.ready_at=now.saturating_add(u64::from(12-order.completed_work_ticks));order.phase=P::Producing;order.terminal_reason=None;
    if order.completed_work_ticks==12{order.phase=P::AwaitingDelivery;order.produced_at=Some(now);}
   }
  },
  IndustrialAdvancePhaseV1::Terminate(_)=>unreachable!("fixed terminating phase returned before facts"),
 }
 Ok(IndustrialAdvanceOutcomeV1::Updated)
}
