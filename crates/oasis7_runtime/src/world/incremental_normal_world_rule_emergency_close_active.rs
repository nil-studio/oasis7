//! Fixed original Active predicate over real Current records. Policy failures
//! remain distinct from IO/resource/cancellation failures for Close's is_ok gate.
use super::*;
use oasis7_world_state::world_rules::{EmergencyObservationWorkV1,EmergencyObservationValidationErrorV1};
use oasis7_agent_api::world_action::EmergencyEvidenceV1;
fn limited()->ValueTreeError{ValueTreeError::ResourceLimited("Current emergency observation work")}
fn evaluate<M,P,V,R,E,C,PE,PC>(e:&EmergencyEvidenceV1,directory:&ReservedValue<CurrentEmergencyLifecycleDirectoryV1>,source:&ReservedValue<CurrentEmergencyLifecycleSourceV1>,sources:&EmergencyCloseSourcesV1<'_,'_>,state:&M,ctx:&mut WorldFieldOverlayContextV1<'_,'_,'_,M,R,E,C>,work:&mut IndustrialDeliveryWorkV1,tail:&mut NormalTailWorkV1<'_,'_>,now:u64,live:&ExecutionFence)->Result<Result<bool,crate::WorldError>,IndustrialDeliveryErrorV1>
where M:MerkleNodeReader,P:MerkleNodeReader,V:ValueNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool,PE:FnMut()->Option<ValueTreeError>,PC:FnMut()->bool {
 tail.charge(e.region_id.len().checked_add(e.region_budget_lineage.len()).and_then(|n|n.checked_add(e.source_ref.len())).ok_or_else(limited)?)?;
 if let Err(error)=oasis7_world_state::world_rules::validate_emergency_observation_prefix_v1(e,now){return Ok(Err(error));}
 // The complete directory is joined to this exact Current source at entry by
 // the fixed caller. Select all actual region rows, plus actual inner-ID
 // aliases used by the original services[obligation_id] coverage lookup.
 let count=directory.active_inputs().services().len();let cap=count.checked_mul(2).ok_or_else(limited)?;
 let slots=cap.checked_mul(std::mem::size_of::<ReservedValue<CurrentEmergencyServiceV1>>()).ok_or_else(limited)?;
 let index_heap=oasis7_world_state::incremental_collection_layout::string_set_output_heap_v1(cap)?;
 // A borrowed key (&str,16) plus value (&Service,8) has the same slot width
 // and alignment as String(24); the shared String-set node bound dominates.
 let _phase=tail.reserve(slots.checked_add(index_heap).ok_or_else(limited)?,MemoryClass::Ordinary)?;
 let mut selected=Vec::new();selected.try_reserve_exact(cap).map_err(|_|limited())?;if selected.capacity()>cap{return Err(limited().into());}
 for row in directory.active_inputs().services(){tail.charge(row.region_id.len().min(e.region_id.len()))?;if row.region_id!=e.region_id{continue;}
  let guard=ctx.transaction.guard();let record=read_current_emergency_service_v1(source,sources.rules.emergency(),sources.delivery.eligibility.roots,state,&row.outer_id,ctx,&mut work.eligibility.scan,&mut work.composite_read,&guard,live)?;
  if record.value().is_none(){return Err(ValueTreeError::Invalid("Current directory service absent").into());}selected.push(record);
 }
 let original_count=selected.len();let first=e.observations.first().map(|w|w.window_start);
 for i in 0..original_count {let service=selected[i].value().ok_or_else(limited)?;tail.charge_record()?;
  if !first.is_some_and(|first|service.registered_at.checked_add(600).is_some_and(|t|t<=first)){continue;}
  let id=&service.obligation_id;let mut present=false;for record in &selected {tail.charge(id.len().min(record.outer_id().len()))?;if record.outer_id()==id{present=true;break;}}
  if present{continue;}let guard=ctx.transaction.guard();let record=read_current_emergency_service_v1(source,sources.rules.emergency(),sources.delivery.eligibility.roots,state,id,ctx,&mut work.eligibility.scan,&mut work.composite_read,&guard,live)?;
  if record.value().is_none(){return Err(ValueTreeError::Invalid("original eligible service alias absent").into());}selected.push(record);
 }
 let mut borrowed=std::collections::BTreeMap::new();for record in &selected {tail.charge(record.outer_id().len().checked_mul(selected.len()+1).ok_or_else(limited)?)?;borrowed.insert(record.outer_id(),record.value().ok_or_else(limited)?);}
 let comparison_bound=e.observations.iter().try_fold(borrowed.len()+1,|n,w|n.checked_add(w.obligation_ids.len()).and_then(|n|n.checked_add(w.normal_coverage.len())).ok_or_else(limited))?;
 let mut temporary=tail.reserve(0,MemoryClass::Ordinary)?;let mut first_work_error=None;
 let result=oasis7_world_state::world_rules::validate_emergency_observations_with_work_v1(&borrowed,e,now,false,&mut |event|{
  let (bytes,heap)=match event {EmergencyObservationWorkV1::Read{bytes}=>(bytes,0),EmergencyObservationWorkV1::OwnedId{bytes}=>(bytes,oasis7_world_state::incremental_collection_layout::string_set_output_heap_v1(1)?.checked_add(bytes).ok_or_else(limited)?),EmergencyObservationWorkV1::BorrowedSetNode=>(0,oasis7_world_state::incremental_collection_layout::string_set_output_heap_v1(1)?)};
  if let Err(error)=tail.charge(bytes.checked_mul(comparison_bound).ok_or_else(limited)?){first_work_error=Some(error);return Err(limited());}
  if heap!=0{let mut paid=tail.reserve(heap,MemoryClass::Ordinary)?;temporary.merge(&mut paid).map_err(ValueTreeError::Invalid)?;}Ok(())
 });
 if let Some(error)=first_work_error{return Err(error.into());}
 match result {Ok(v)=>Ok(Ok(v)),Err(EmergencyObservationValidationErrorV1::Policy(e))=>Ok(Err(e)),Err(EmergencyObservationValidationErrorV1::Resource(e))=>Err(e.into())}
}
/// Returns only the original Active policy result. No permission cap is minted;
/// the sole Close owner must still check its header and persist the full write set.
pub(in crate::world::incremental_normal_world_rule_lifecycle) fn validate_current_active_v1<M,P,V,R,E,C,PE,PC>(lookup_id:&str,directory:&ReservedValue<CurrentEmergencyLifecycleDirectoryV1>,source:&ReservedValue<CurrentEmergencyLifecycleSourceV1>,sources:&EmergencyCloseSourcesV1<'_,'_>,state:&M,ctx:&mut WorldFieldOverlayContextV1<'_,'_,'_,M,R,E,C>,parent:&mut WorldFieldReadContextV1<'_,P,V,PE,PC>,work:&mut IndustrialDeliveryWorkV1,extra:&mut sources::EmergencyServiceSourceWorkV1<'_>,rules_work:&mut WorldRulesFamilyWorkV1,tail:&mut NormalTailWorkV1<'_,'_>,now:u64,live:&ExecutionFence)->Result<Result<(),crate::WorldError>,IndustrialDeliveryErrorV1>
where M:MerkleNodeReader,P:MerkleNodeReader,V:ValueNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool,PE:FnMut()->Option<ValueTreeError>,PC:FnMut()->bool {
 let before=rules_work.remainder.cells_remaining;let before_records=rules_work.outer_records_remaining;let guard=ctx.transaction.guard();let joined=validate_current_emergency_directory_source_v1(directory,source,sources.rules.emergency(),sources.delivery.eligibility.roots,ctx,rules_work,&guard,live);
 let delta=before.checked_sub(rules_work.remainder.cells_remaining).ok_or_else(limited)?;let records=before_records.checked_sub(rules_work.outer_records_remaining).ok_or_else(limited)?;let accounting=tail.debit(delta.checked_add(records).ok_or_else(limited)?,delta);joined?;accounting?;
 let guard=ctx.transaction.guard();let auth=read_current_emergency_authorization_v1(source,sources.rules.emergency(),sources.delivery.eligibility.roots,state,lookup_id,ctx,&mut work.eligibility.scan,&mut work.composite_read,&guard,live)?;
 let Some(auth)=auth.value() else{return Ok(Err(crate::world_rules::invalid("authorization_inactive")))};
 let guard=ctx.transaction.guard();let base=read_current_emergency_evidence_v1(source,sources.rules.emergency(),sources.delivery.eligibility.roots,state,&auth.evidence_id,ctx,&mut work.eligibility.scan,&mut work.composite_read,&guard,live)?;
 let Some(e)=base.value() else{return Ok(Err(crate::world_rules::invalid("invalid_evidence")))};
 let mut overflowed=false;for row in directory.active_inputs().overflow_rows(){tail.charge(row.region_id.len().min(auth.region_id.len()))?;if row.region_id==auth.region_id&&row.starts_at==auth.starts_at{overflowed=true;break;}}
 if let Err(e)=oasis7_world_state::world_rules::validate_emergency_active_window_v1(auth,overflowed,now){return Ok(Err(e));}
 for row in directory.active_inputs().evidence(){tail.charge(row.region_id.len().min(e.region_id.len()).checked_add(row.region_budget_lineage.len().min(e.region_budget_lineage.len())).ok_or_else(limited)?)?;
  if row.region_id!=e.region_id||row.region_budget_lineage!=e.region_budget_lineage||row.end<oasis7_world_state::world_rules::evidence_end(e){continue;}
  let guard=ctx.transaction.guard();let candidate=read_current_emergency_evidence_v1(source,sources.rules.emergency(),sources.delivery.eligibility.roots,state,&row.outer_id,ctx,&mut work.eligibility.scan,&mut work.composite_read,&guard,live)?;
  let candidate=candidate.value().ok_or_else(||ValueTreeError::Invalid("Current directory evidence absent"))?;
  if matches!(evaluate::<M,P,V,R,E,C,PE,PC>(candidate,directory,source,sources,state,ctx,work,tail,now,live)?,Ok(true)){return Ok(Err(crate::world_rules::invalid("authorization_inactive")));}
 }
 if let Some(last)=e.observations.last(){for id in &last.obligation_ids {tail.charge(id.len())?;let guard=ctx.transaction.guard();let service=read_current_emergency_service_v1(source,sources.rules.emergency(),sources.delivery.eligibility.roots,state,id,ctx,&mut work.eligibility.scan,&mut work.composite_read,&guard,live)?;
  let checked=sources::read_current_emergency_active_service_sources_v1(sources::CurrentEmergencyServiceSelectionV1::Service(service),source,sources,state,ctx,parent,work,extra,tail,now,live);
  match checked {Ok(owner)=>owner.validate(ctx.transaction,tail,now,live)?,Err(IndustrialDeliveryErrorV1::World(e))=>return Ok(Err(e)),Err(e)=>return Err(e)}
 }}
 if let Err(e)=oasis7_world_state::world_rules::validate_emergency_active_freshness_v1(e,now){return Ok(Err(e));}ctx.transaction.validate_guard(&ctx.transaction.guard(),live)?;Ok(Ok(()))
}
