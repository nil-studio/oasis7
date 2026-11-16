//! Path-copy observation columns from the same real typed delta rows. Raw
//! State SHA keeps its separate original-order slot/typed-key serialization.
use super::*;
use oasis7_world_state::observation_projection::current_commitment::{ProjectionColumnDataV1,ProjectionColumnErrorV1,ProjectionDataPathRefV1 as Path};
use std::{cell::RefCell,mem::size_of};
pub(super) fn column_error<E:std::error::Error+Send+Sync+'static>(error:ProjectionColumnErrorV1<E>)->BodyMutationError{match error{ProjectionColumnErrorV1::Work(e)=>paid(e),ProjectionColumnErrorV1::Memory(e)=>ValueTreeError::Memory(e).into(),ProjectionColumnErrorV1::Invalid(e)=>invalid(e)}}
#[derive(Debug)]struct JsonCause{error:serde_json::Error,_phase:MemoryReservation}
impl std::fmt::Display for JsonCause{fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result{self.error.fmt(f)}}impl std::error::Error for JsonCause{}
fn phase(common:&mut FormalBoundedContextV1<'_>,n:usize)->Result<MemoryReservation,BodyMutationError>{let n=n.checked_mul(1024).and_then(|n|n.checked_add(32768)).ok_or_else(||invalid("projection dirty JSON phase"))?;common.charge(1,n as u64).map_err(paid)?;common.memory().reserve(n,MemoryClass::Ordinary).map_err(ValueTreeError::Memory).map_err(Into::into)}
fn decode(bytes:&[u8],common:&mut FormalBoundedContextV1<'_>)->Result<(serde_json::Value,MemoryReservation),BodyMutationError>{let phase=phase(common,bytes.len())?;common.charge(bytes.len()as u64,bytes.len()as u64).map_err(paid)?;let result=serde_json::from_slice(bytes);common.consume_steps(0).map_err(paid)?;match result{Ok(value)=>Ok((value,phase)),Err(error)=>Err(paid(JsonCause{error,_phase:phase}))}}
pub(super) fn data(bytes:&[u8],common:&mut FormalBoundedContextV1<'_>)->Result<ProjectionColumnDataV1,BodyMutationError>{let(value,_phase)=decode(bytes,common)?;let memory=common.memory().clone();ProjectionColumnDataV1::from_json_data(value,&memory,&mut|s,b|common.charge(s,b)).map_err(column_error)}
fn object(common:&mut FormalBoundedContextV1<'_>)->Result<ProjectionColumnDataV1,BodyMutationError>{let memory=common.memory().clone();ProjectionColumnDataV1::from_json_data(serde_json::Value::Object(Default::default()),&memory,&mut|s,b|common.charge(s,b)).map_err(column_error)}
fn key(value:serde_json::Value,common:&mut FormalBoundedContextV1<'_>)->Result<ReservedValue<String>,BodyMutationError>{
 let bound=match &value{serde_json::Value::String(value)=>value.capacity(),serde_json::Value::Number(_)=>20,_=>return Err(invalid("original object key JSON type"))};let n=size_of::<String>().checked_add(bound).ok_or_else(||invalid("original object key layout"))?;common.charge(1,n as u64).map_err(paid)?;let fee=common.memory().reserve(n,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
 let value=match value{serde_json::Value::String(value)=>value,serde_json::Value::Number(value)=>value.to_string(),_=>unreachable!()};if value.capacity()>bound{return Err(invalid("original object key capacity"))}Ok(fee.retain(value))
}
pub(super) fn ensure_field(graph:&ProjectionColumnDataV1,field:&str,common:&mut FormalBoundedContextV1<'_>)->Result<ProjectionColumnDataV1,BodyMutationError>{
 let memory=common.memory().clone();if graph.object_member_data(field,&memory,&mut|s,b|common.charge(s,b)).map_err(column_error)?.is_some(){return graph.share_data(&memory,&mut|s,b|common.charge(s,b)).map_err(column_error)}
 // Only a real typed map-row source reaches this fixed helper. The empty
 // intermediate graph is filled before the enclosing Current owner is minted.
 let empty=object(common)?;graph.with_object_member_data(field,Some(&empty),&memory,&mut|s,b|common.charge(s,b)).map_err(column_error)
}
fn patch(graph:&ProjectionColumnDataV1,field:&str,semantic:&[u8],key_json:&[u8],json:Option<&[u8]>,common:&mut FormalBoundedContextV1<'_>)->Result<ProjectionColumnDataV1,BodyMutationError>{
 common.charge(1,semantic.len() as u64).map_err(paid)?;let kind=oasis7_world_state::incremental_semantic_key::read_semantic_key_v1(semantic).map_err(invalid)?;let after=match json{Some(json)=>Some(data(json,common)?),None=>None};let memory=common.memory().clone();
 if matches!(kind,oasis7_world_state::incremental_semantic_key::SemanticKeyRefV1::Unit){return graph.with_object_member_data(field,after.as_ref(),&memory,&mut|s,b|common.charge(s,b)).map_err(column_error)}
 let(value,_key_phase)=decode(key_json,common)?;let key=key(value,common)?;let graph=ensure_field(graph,field,common)?;graph.with_path_data_v1(&[Path::Field(field),Path::Field(&key)],after.as_ref(),&memory,&mut|s,b|common.charge(s,b)).map_err(column_error)
}
pub(super) fn fold_observation_sources_v1(before:&ProjectionColumnDataV1,source:&super::sources::ActualNormalStateSourcesV1,after_state:&super::state_columns::OriginalStateColumnsDataV1,common:&mut FormalBoundedContextV1<'_>)->Result<ProjectionColumnDataV1,BodyMutationError>{
 let memory=common.memory().clone();let mut out=before.share_data(&memory,&mut|s,b|common.charge(s,b)).map_err(column_error)?;let shared=RefCell::new(common);
 // Only the actual original formatter allowlist enters observation State.
 // Hidden actual columns remain in the separate complete raw-SHA owner.
 source.main_policy.visit_with_work_v1(|s,b|shared.borrow_mut().charge(s,b).map_err(paid),|field,semantic,key,json|{if VISIBLE_STATE_COLUMNS_V1.contains(&field){out=patch(&out,field,semantic,key,json,&mut shared.borrow_mut())?}Ok::<_,BodyMutationError>(())})?;
 source.materials.visit_with_work_v1(|s,b|shared.borrow_mut().charge(s,b).map_err(paid),|field,semantic,key,json|{if VISIBLE_STATE_COLUMNS_V1.contains(&field){out=patch(&out,field,semantic,key,json,&mut shared.borrow_mut())?}Ok::<_,BodyMutationError>(())})?;
 source.fixed.visit_with_work_v1(|s,b|shared.borrow_mut().charge(s,b).map_err(paid),|field,semantic,key,json|{if VISIBLE_STATE_COLUMNS_V1.contains(&field){out=patch(&out,field,semantic,key,json,&mut shared.borrow_mut())?}Ok::<_,BodyMutationError>(())})?;
 source.native.visit_with_work_v1(|s,b|shared.borrow_mut().charge(s,b).map_err(paid),|field,semantic,key,json|{if VISIBLE_STATE_COLUMNS_V1.contains(&field){out=patch(&out,field,semantic,key,json,&mut shared.borrow_mut())?}Ok::<_,BodyMutationError>(())})?;
 source.primitive.visit_with_work_v1(|s,b|shared.borrow_mut().charge(s,b).map_err(paid),|field,semantic,key,json|{if VISIBLE_STATE_COLUMNS_V1.contains(&field){out=patch(&out,field,semantic,key,json,&mut shared.borrow_mut())?}Ok::<_,BodyMutationError>(())})?;
 source.industry.visit_with_work_v1(&mut|b|shared.borrow_mut().charge(1,b as u64).map_err(paid),|path,semantic,key,json|{if path.len()!=1{return Err(invalid("Industry observation path"))}if VISIBLE_STATE_COLUMNS_V1.contains(&path[0]){out=patch(&out,path[0],semantic,key,json,&mut shared.borrow_mut())?}Ok::<_,BodyMutationError>(())})?;
 source.agent_authority.visit_with_work_v1(|s,b|shared.borrow_mut().charge(s,b).map_err(paid),|field,semantic,key,json|{if VISIBLE_STATE_COLUMNS_V1.contains(&field){out=patch(&out,field,semantic,key,json,&mut shared.borrow_mut())?}Ok::<_,BodyMutationError>(())})?;
 source.factory_authority.visit_with_work_v1(|s,b|shared.borrow_mut().charge(s,b).map_err(paid),|field,semantic,key,json|{if VISIBLE_STATE_COLUMNS_V1.contains(&field){out=patch(&out,field,semantic,key,json,&mut shared.borrow_mut())?}Ok::<_,BodyMutationError>(())})?;
 source.agents.visit_with_work_v1(|s,b|shared.borrow_mut().charge(s,b).map_err(paid),|field,semantic,key,json|{if VISIBLE_STATE_COLUMNS_V1.contains(&field){out=patch(&out,field,semantic,key,json,&mut shared.borrow_mut())?}Ok::<_,BodyMutationError>(())})?;
 // Inventory has genuine independent ledger-container presence. It cannot
 // be inferred from nonzero material members.
 source.inventory.visit_groups_with_work_v1(&mut|b|shared.borrow_mut().charge(1,b as u64).map_err(paid),|_ledger,_semantic,key_json,before,after|{if !before&&after{let mut common=shared.borrow_mut();let memory=common.memory().clone();let(value,_phase)=decode(key_json,&mut common)?;let key=key(value,&mut common)?;out=ensure_field(&out,"material_ledgers",&mut common)?;let empty=object(&mut common)?;out=out.with_path_data_v1(&[Path::Field("material_ledgers"),Path::Field(&key)],Some(&empty),&memory,&mut|s,b|common.charge(s,b)).map_err(column_error)?}Ok::<_,BodyMutationError>(())})?;
 source.inventory.visit_with_work_v1(&mut|b|shared.borrow_mut().charge(1,b as u64).map_err(paid),|path,semantic,key_json,json|{if path.len()!=1{return Err(invalid("Inventory observation path"))}let mut common=shared.borrow_mut();if path[0]!="material_ledgers"{out=patch(&out,&path[0],semantic,key_json,json,&mut common)?;return Ok(())}let(value,_phase)=decode(key_json,&mut common)?;let serde_json::Value::Array(mut pair)=value else{return Err(invalid("Ledger original pair JSON"))};if pair.len()!=2{return Err(invalid("Ledger original pair arity"))}let material=key(pair.pop().unwrap(),&mut common)?;let ledger=key(pair.pop().unwrap(),&mut common)?;let after=match json{Some(json)=>Some(data(json,&mut common)?),None=>None};let memory=common.memory().clone();out=out.with_path_data_v1(&[Path::Field("material_ledgers"),Path::Field(&ledger),Path::Field(&material)],after.as_ref(),&memory,&mut|s,b|common.charge(s,b)).map_err(column_error)?;Ok::<_,BodyMutationError>(())})?;
 source.inventory.visit_groups_with_work_v1(&mut|b|shared.borrow_mut().charge(1,b as u64).map_err(paid),|_ledger,_semantic,key_json,before,after|{if before&&!after{let mut common=shared.borrow_mut();let(value,_phase)=decode(key_json,&mut common)?;let key=key(value,&mut common)?;let memory=common.memory().clone();out=out.with_path_data_v1(&[Path::Field("material_ledgers"),Path::Field(&key)],None,&memory,&mut|s,b|common.charge(s,b)).map_err(column_error)?}Ok::<_,BodyMutationError>(())})?;
 let common=shared.into_inner();for field in VISIBLE_STATE_COLUMNS_V1{if *field=="module_visual_entities"||*field=="world_rules"||oasis7_world_state::incremental_diplomacy::is_diplomacy_projection_field_v1(field)||oasis7_world_state::incremental_governance_records::is_governance_projection_field_v1(field){continue}if !after_state.original_field_emitted_v1(field,common)?{out=out.with_object_member_data(field,None,&memory,&mut|s,b|common.charge(s,b)).map_err(column_error)?}}
 Ok(out)
}

pub(super) fn fold_world_rules_observation_v1<M:oasis7_formal_protocol::incremental_state::MerkleNodeReader>(before:&ProjectionColumnDataV1,after_state:&super::state_columns::OriginalStateColumnsDataV1,source:&crate::world::incremental_normal_projection_commitment_world_rules::PaidCurrentProjectionWorldRulesDeltaV1,tx:&oasis7_formal_protocol::incremental_execution::ExecutionTransaction<'_,M>,max_json:usize,common:&mut FormalBoundedContextV1<'_>,live:&oasis7_formal_protocol::incremental_execution::ExecutionFence)->Result<ProjectionColumnDataV1,BodyMutationError>{
 source.validate(tx,common.memory(),common.work().identity(),live)?;let memory=common.memory().clone();if !source.rules_present(){return before.with_object_member_data("world_rules",None,&memory,&mut|s,b|common.charge(s,b)).map_err(column_error)}
 let rows=after_state.world_rule_rows_data_v1(common)?.ok_or_else(||invalid("actual WorldRules logical columns absent"))?;
 if before.object_member_data("world_rules",&memory,&mut|s,b|common.charge(s,b)).map_err(column_error)?.is_none(){
  // First appearance has no old graph to visit. Encode this newly-created
  // actual optional container from its real post-delta typed columns once.
  let shared=RefCell::new(common);let first=RefCell::new(None);let view=rows.serialization_v1(&shared,&first);let bytes=super::shared_json::capture_shared_v1(&view,&shared,&first,max_json)?;let common=shared.into_inner();let value=data(&bytes,common)?;return before.with_object_member_data("world_rules",Some(&value),&memory,&mut|s,b|common.charge(s,b)).map_err(column_error)
 }
 let mut out=before.share_data(&memory,&mut|s,b|common.charge(s,b)).map_err(column_error)?;
 source.visit(tx,common,live,|path,_semantic,json,common|{
  if path.len()<2||path[0]!="world_rules"{return Err(invalid("actual WorldRules observation path"))}
  let memory=common.memory().clone();let root=out.object_member_data("world_rules",&memory,&mut|s,b|common.charge(s,b)).map_err(column_error)?.ok_or_else(||invalid("actual WorldRules observation root"))?;
  if root.object_member_data(&path[1],&memory,&mut|s,b|common.charge(s,b)).map_err(column_error)?.is_none(){
   use oasis7_world_state::incremental_world_rule_projection_delta::WorldRuleLogicalTopSlotV1 as Top;
   let top=match path[1].as_str(){"industrial_work"=>Top::IndustrialWork,"substitution"=>Top::Substitution,_=>return Err(invalid("actual WorldRules mandatory branch absent"))};
   let emitted=source.branch_emission();if (top==Top::IndustrialWork&&emitted[0]==Some(false))||(top==Top::Substitution&&emitted[1]==Some(false)){return Ok(())}
   let shared=RefCell::new(&mut *common);let first=RefCell::new(None);let view=rows.optional_branch_serialization_v1(top,&shared,&first)?;let bytes=super::shared_json::capture_shared_v1(&view,&shared,&first,max_json)?;let common=shared.into_inner();let value=data(&bytes,common)?;out=out.with_path_data_v1(&[Path::Field("world_rules"),Path::Field(&path[1])],Some(&value),&memory,&mut|s,b|common.charge(s,b)).map_err(column_error)?;return Ok(())
  }
  let n=path.len().checked_mul(size_of::<Path<'_>>()).and_then(|n|n.checked_add(size_of::<Vec<Path<'_>>>())).ok_or_else(||invalid("WorldRules observation path layout"))?;common.charge(1,n as u64).map_err(paid)?;let _fee=common.memory().reserve(n,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;let mut parts=Vec::with_capacity(path.len());if parts.capacity()!=path.len(){return Err(invalid("WorldRules observation path capacity"))}for part in path{common.charge(1,part.len() as u64).map_err(paid)?;parts.push(Path::Field(part))}
  let after=match json{Some(json)=>Some(data(json,common)?),None=>None};out=out.with_path_data_v1(&parts,after.as_ref(),&memory,&mut|s,b|common.charge(s,b)).map_err(column_error)?;Ok(())
 })?;
 for(index,name)in ["industrial_work","substitution"].into_iter().enumerate(){common.consume_steps(1).map_err(paid)?;if source.branch_emission()[index]==Some(false){out=out.with_path_data_v1(&[Path::Field("world_rules"),Path::Field(name)],None,&memory,&mut|s,b|common.charge(s,b)).map_err(column_error)?}}
 source.validate(tx,common.memory(),common.work().identity(),live)?;Ok(out)
}
