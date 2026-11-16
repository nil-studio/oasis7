//! Original reader-filtered substitution status. Only the fields actually
//! emitted by status are assembled; no World/WorldRules snapshot is restored.
use super::readonly_status::{WorldRuleStatusReadonlyFactsV1 as Facts,before,field,format,map_insert};
use super::*;
use oasis7_agent_api::world_action::*;
use serde_json::{Value,Map};
use std::{collections::BTreeMap,mem::size_of,ops::ControlFlow};
type Error<S>=<S as IndustrialOwnerReadonlyFactsV1>::Error;
pub fn substitution_status_readonly_v1<S:Facts>(s:&S,reader:Option<&str>,world:Option<&str>)->Result<Option<Value>,Error<S>>{
 let Some(actor)=reader else{return Ok(None)};if !s.status_rules_present()?{return Ok(None)}
 let owner=super::resolve_industrial_owner_readonly_v1(s,actor)?;
 let binding=super::industrial_work_admission::industrial_owner_binding_with_work_v1(owner.as_ref(),&mut|steps,bytes,heap|{s.owner_work(steps,bytes)?;if heap>0{s.owner_before_owned(heap)?}Ok(())})?.ok();
 let mut roots=BTreeMap::new();let mut decisions=BTreeMap::new();let mut settlements=BTreeMap::new();let mut residuals=BTreeMap::new();
 if let Some(binding)=binding.as_deref(){
  s.status_visit_roots(|key,root|{
   s.owner_work(1,root.owner_id.len()+actor.len())?;if root.owner_id!=actor{return Ok(ControlFlow::Continue(()))}
   // Original industrial_work::for_reader tests all referenced parent lots
   // before the current owner-binding filter, preserving the old lazy order.
   for slot in root.slots.values(){for parent in &slot.parents{let visible=s.industrial_with_lot(&parent.lot_ref.id,|lot|{let Some(lot)=lot else{return Ok(false)};before(s,&(&lot.lot_ref,&parent.lot_ref))?;s.owner_work(1,lot.owner_id.len()+actor.len())?;Ok(lot.owner_id==actor&&lot.lot_ref==parent.lot_ref)})?;if !visible{return Ok(ControlFlow::Continue(()))}}}
   s.owner_work(1,root.owner_binding_digest.len()+binding.len())?;if root.owner_binding_digest!=binding{return Ok(ControlFlow::Continue(()))}
   let root=super::readonly_industrial_data::root(s,root)?;map_insert(s,&mut roots,key,root)?;Ok(ControlFlow::Continue(()))
  })?;
  s.status_visit_decisions(|key,decision|{
   s.owner_work(1,decision.owner_id.len()+actor.len())?;if decision.owner_id!=actor{return Ok(ControlFlow::Continue(()))}
   s.owner_work(1,decision.candidate.authority.owner_binding_digest.len()+binding.len())?;if decision.candidate.authority.owner_binding_digest!=binding{return Ok(ControlFlow::Continue(()))}
   let reference=&decision.intent.preview.request.root_ref;s.owner_work(1,reference.id.len())?;
   if let Some(root)=roots.get(&reference.id){before(s,&(&root.root_ref,reference))?;if root.root_ref==*reference{before(s,decision)?;map_insert(s,&mut decisions,key,decision.clone())?;}}
   Ok(ControlFlow::Continue(()))
  })?;
  s.status_visit_settlements(|key,settlement|{s.owner_work(1,key.len())?;if decisions.contains_key(key){before(s,settlement)?;map_insert(s,&mut settlements,key,settlement.clone())?;}Ok(ControlFlow::Continue(()))})?;
  s.status_visit_residuals(|key,residual|{s.owner_work(1,residual.owner_id.len()+actor.len())?;if residual.owner_id!=actor{return Ok(ControlFlow::Continue(()))}let mut visible=false;for settlement in settlements.values(){before(s,&(&settlement.receipt_ref,&residual.settlement_ref))?;if settlement.receipt_ref==residual.settlement_ref{visible=true;break}}if visible{before(s,residual)?;map_insert(s,&mut residuals,key,residual.clone())?;}Ok(ControlFlow::Continue(()))})?;
 }
 let now=s.status_time()?;let mut previews=Vec::new();let mut truncated=false;
 for root in roots.values(){for slot in root.slots.keys(){if previews.len()==128{truncated=true;break}
  let request=SubstitutionPreviewRequestV1{root_ref:super::readonly_industrial_data::reference(s,&root.root_ref)?,slot_id:super::readonly_industrial_data::text(s,slot)?,authority_refs:Vec::new()};
  let result=match world{None=>Err(super::invalid("substitution_world_binding_missing")),Some(world)=>super::preview_substitution_readonly_v1(s,actor,&request,world,now)?};
  let mut row=Map::new();match result{Ok(preview)=>field(s,&mut row,"preview",&preview)?,Err(error)=>{let blocker=format(s,format_args!("{error:?}"))?;field(s,&mut row,"request",&request)?;field(s,&mut row,"status",&"unknown")?;field(s,&mut row,"blocker",&blocker)?;field(s,&mut row,"next_action",&"restore_authority_and_preview_again")?}}
  if previews.len()==previews.capacity(){let cap=previews.capacity().saturating_mul(2).max(4);s.owner_before_owned(cap.checked_mul(size_of::<Value>()).expect("status preview output layout"))?;s.owner_work(1,previews.len()*size_of::<Value>())?;previews.reserve_exact(cap-previews.len());assert_eq!(previews.capacity(),cap,"status preview exact capacity")}
  previews.push(Value::Object(row));
 }if truncated{break}}
 let mut out=Map::new();field(s,&mut out,"schema",&"recipe-substitution/v1")?;field(s,&mut out,"reader_agent_id",&actor)?;field(s,&mut out,"source_tick",&now)?;field(s,&mut out,"roots",&roots)?;field(s,&mut out,"previews",&previews)?;field(s,&mut out,"decisions",&decisions)?;field(s,&mut out,"settlements",&settlements)?;field(s,&mut out,"residuals",&residuals)?;field(s,&mut out,"truncated",&truncated)?;field(s,&mut out,"decision_effect",&"selection_only")?;field(s,&mut out,"conversion_effect",&"linked_settlement_only")?;Ok(Some(Value::Object(out)))
}
