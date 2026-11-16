//! Original StartRoot request replay and outer WorldRule gates over raw facts.
//! This is rehearsal DATA and cannot produce PreparedWorldRule or execution.
use super::{IndustrialStartReadonlyFactsV1,RecipeStartedReadonlyFactsV1,WorldRuleReceiptV1,IndustrialOwnerReadonlyFactsV1,invalid};
use crate::{WorldState,WorldError};
use oasis7_agent_api::world_action::{WorldRuleCommandV1,IndustrialWorkCommandV1};
use std::{mem::size_of,ops::ControlFlow};
pub trait IndustrialStartEnvelopeReadonlyFactsV1:IndustrialStartReadonlyFactsV1+RecipeStartedReadonlyFactsV1{
 fn start_with_request<T>(&self,key:&str,visit:impl FnOnce(Option<&WorldRuleReceiptV1>)->Result<T,Self::Error>)->Result<T,Self::Error>;
}
pub(crate) enum StartEnvelopeDataV1{Replay,Fresh}
macro_rules! domain{($value:expr)=>{match $value{Ok(value)=>value,Err(error)=>return Ok(Err(error))}};}
fn refusal<S:IndustrialOwnerReadonlyFactsV1,T>(s:&S,note:&str)->Result<Result<T,WorldError>,S::Error>{s.owner_work(1,note.len())?;s.owner_before_owned(size_of::<WorldError>()+note.len())?;Ok(Err(invalid(note)))}
fn json_len<S:IndustrialOwnerReadonlyFactsV1,T:serde::Serialize+?Sized>(s:&S,value:&T)->Result<Result<usize,WorldError>,S::Error>{match crate::util::measure_json_with_work_v1(value,&mut|steps,bytes|s.owner_work(steps,bytes as usize)){Ok(n)=>Ok(Ok(n)),Err(crate::util::JsonHashWorkErrorV1::Work(error))=>Err(error),Err(crate::util::JsonHashWorkErrorV1::Codec(error))=>{let note=super::format_readonly_projection_data_v1(format_args!("{}",error),&mut|steps,bytes,heap|{s.owner_work(steps,bytes)?;s.owner_before_owned(heap)})?;Ok(Err(WorldError::Serde(note)))}}}
/// Called only by the fixed StartRoot branch. It preserves the original replay
/// short-circuit before manufactured scope, command length and policy checks.
pub(crate) fn envelope<S:IndustrialStartEnvelopeReadonlyFactsV1>(s:&S,actor:&str,request:&str,command:&WorldRuleCommandV1,world:Option<&str>)->Result<Result<StartEnvelopeDataV1,WorldError>,S::Error>{
 assert!(matches!(command,WorldRuleCommandV1::IndustrialWork{command}if matches!(&**command,IndustrialWorkCommandV1::StartRoot{..})),"fixed StartRoot envelope");
 for id in [actor,request]{s.owner_work(1,id.len())?;s.owner_before_owned(size_of::<WorldError>()+"invalid_identity".len())?;domain!(super::validate_id(id));}
 if !s.owner_agent_present(actor)?{return refusal(s,"actor_not_found")}
 // Both IDs passed the original <=128-byte validation. The one original
 // request-key function is bounded here, including output allocation and hash.
 let key_input=b"oasis7.world-rule.request.v1".len()+16+actor.len()+request.len();s.owner_work(34+(key_input as u64).div_ceil(64),key_input+69)?;s.owner_before_owned(size_of::<String>()+69+128)?;
 let key=super::request_key(actor,request);
 s.start_with_request(&key,|receipt|{
  if let Some(receipt)=receipt{s.owner_work(1,receipt.actor_id.len()+actor.len())?;if receipt.actor_id!=actor{return refusal(s,"request_content_conflict")}
   let left=domain!(json_len(s,&receipt.command)?);let right=domain!(json_len(s,command)?);s.owner_work((left+right)as u64+1,left+right)?;
   if receipt.command!=*command{return refusal(s,"request_content_conflict")}return Ok(Ok(StartEnvelopeDataV1::Replay));
  }
  let mut scope=Ok(());s.owner_visit_orders(|order|{s.owner_work(1,order.target_agent_id.len()+actor.len())?;if order.target_agent_id==actor{s.owner_before_owned(size_of::<WorldError>()+"manufactured_actor_world_binding_mismatch".len())?;s.owner_work(1,order.quote.world_binding.as_ref().map_or(0,String::len)+world.map_or(0,str::len))?;scope=super::validate_manufactured_world_scope_from_order_v1(Some(order),world);return Ok(ControlFlow::Break(()))}Ok(ControlFlow::Continue(()))})?;domain!(scope);
  if domain!(json_len(s,command)?)>65536{return refusal(s,"world_rule_command_too_large")}
  let blocked=s.quote_with_policy(|policy|{for id in &policy.blocked_agents{s.owner_work(1,id.len()+actor.len())?;if id==actor{return Ok(true)}}Ok(false)})?;
  if blocked{return refusal(s,"actor_blocked")}Ok(Ok(StartEnvelopeDataV1::Fresh))
 })
}
impl IndustrialStartEnvelopeReadonlyFactsV1 for WorldState{
 fn start_with_request<T>(&self,key:&str,visit:impl FnOnce(Option<&WorldRuleReceiptV1>)->Result<T,Self::Error>)->Result<T,Self::Error>{visit(self.world_rules.as_ref().and_then(|rules|rules.requests.get(key)))}
}

/// Query-local raw rehearsal, not a PreparedWorldRule or application permit.
/// The source borrow keeps every allocation admission alive through use.
pub struct IndustrialStartRehearsalDataV1<'source>{recipe:Option<super::IndustrialStartRecipePreviewDataV1<'source>>,_source:std::marker::PhantomData<&'source ()>}
impl IndustrialStartRehearsalDataV1<'_>{pub fn event(&self)->Option<&crate::DomainEvent>{self.recipe.as_ref().map(|r|r.event())}}
pub fn preview_new_recipe_start_root_readonly_v1<'source,S>(s:&'source S,event:&crate::DomainEvent,now:u64)->Result<Result<IndustrialStartRehearsalDataV1<'source>,WorldError>,<S as IndustrialOwnerReadonlyFactsV1>::Error>
where S:IndustrialStartEnvelopeReadonlyFactsV1+super::IndustrialReceiptReadonlyFactsV1<Error=<S as IndustrialOwnerReadonlyFactsV1>::Error>{
 use super::{readonly_industrial_data as data,readonly_industrial_start as start,readonly_industrial_receipt as receipts};
 let crate::DomainEvent::WorldRuleCommitted{action_id,actor_id,request_id,command,world_binding,..}=event else{return refusal(s,"unsupported rule event")};
 let WorldRuleCommandV1::IndustrialWork{command:inner}=command else{return refusal(s,"unsupported rule event")};
 let IndustrialWorkCommandV1::StartRoot{root_ref,expected_profile_digest,logistics_route_ids,logistics_path_ids}=&**inner else{return refusal(s,"unsupported rule event")};
 // This fixed preview is the original NewRecipe rehearsal, whose native event
 // supplies empty route/path lists. This fixed entry accepts only that
 // exact original invocation shape; other StartRoot callers use their reducer.
 assert!(logistics_route_ids.is_empty()&&logistics_path_ids.is_empty(),"fixed NewRecipe StartRoot rehearsal routes");
 match domain!(envelope(s,actor_id,request_id,command,world_binding.as_deref())?){StartEnvelopeDataV1::Replay=>return Ok(Ok(IndustrialStartRehearsalDataV1{recipe:None,_source:std::marker::PhantomData})),StartEnvelopeDataV1::Fresh=>{}}
 let Some(world)=world_binding.as_deref().filter(|v|!v.is_empty())else{return refusal(s,"industrial_world_binding_missing")};
 let owner=super::resolve_industrial_owner_readonly_v1(s,actor_id)?;
 domain!(super::industrial_work_admission::industrial_owner_binding_with_work_v1(owner.as_ref(),&mut|steps,bytes,heap|{s.owner_work(steps,bytes)?;if heap==0{Ok(())}else{s.owner_before_owned(heap)}})?);
 let selected=domain!(super::preview_industrial_start_inputs_readonly_v1(s,actor_id,root_ref,expected_profile_digest,now,world)?);
 let recipe=domain!(super::preview_industrial_start_recipe_readonly_v1(s,&selected,actor_id,*action_id,now)?);
 let event=recipe.event();
 let child_id=super::format_readonly_projection_data_v1(format_args!("start:{}",root_ref.id),&mut|steps,bytes,heap|{s.owner_work(steps,bytes)?;if heap==0{Ok(())}else{s.owner_before_owned(heap)}})?;
 let child=domain!(super::industrial_work::industrial_ref_with_work_v1(world,"industrial","industrial-start/v1",&child_id,1,&(root_ref,event),&mut|steps,bytes,heap|{s.owner_work(steps,bytes)?;if heap==0{Ok(())}else{s.owner_before_owned(heap)}})?);
 let receipt_id=super::format_readonly_projection_data_v1(format_args!("production-start:{}",root_ref.id),&mut|steps,bytes,heap|{s.owner_work(steps,bytes)?;if heap==0{Ok(())}else{s.owner_before_owned(heap)}})?;
 let receipt_ref=domain!(super::industrial_work::industrial_ref_with_work_v1(world,"industrial","industrial-receipt/v1",&receipt_id,1,event,&mut|steps,bytes,heap|{s.owner_work(steps,bytes)?;if heap==0{Ok(())}else{s.owner_before_owned(heap)}})?);
 let payload_digest=domain!(start::hash(s,event)?);
 s.owner_before_owned(size_of::<oasis7_agent_api::world_action::IndustrialReceiptV1>())?;
 let receipt=oasis7_agent_api::world_action::IndustrialReceiptV1{receipt_ref,root_ref:data::reference(s,root_ref)?,child_ref:child,effect_role:data::text(s,"production_start")?,payload_digest,input:data::parents(s,selected.parents())?,output:Vec::new()};
 domain!(receipts::validate(s,&receipt,world)?);
 s.owner_before_owned(size_of::<IndustrialStartRehearsalDataV1<'source>>())?;Ok(Ok(IndustrialStartRehearsalDataV1{recipe:Some(recipe),_source:std::marker::PhantomData}))
}
