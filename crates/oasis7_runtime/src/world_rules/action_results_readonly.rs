//! Original bounded recent-business-result correlation over real source rows.
//! Signature success repeats the historical proof; it does not authorize work.
use super::*;
use oasis7_wasm_abi::{CapabilityCanonicalWorkV1,CanonicalCborStreamError};
use std::{ops::{ControlFlow,Deref},collections::BTreeMap};
pub(crate)mod sealed{pub trait Sources{}}
pub(crate)struct ManifestDataV1{pub world_id:String,pub genesis_ref:Option<String>}
pub(crate)trait SourcesV1:sealed::Sources+CapabilityCanonicalWorkV1{
 type Error;
 type Event<'a>:Deref<Target=crate::WorldEvent>where Self:'a;
 fn identity(&self,reader:&str)->Result<Option<crate::CapabilityAgentIdentity>,Self::Error>;
 fn public_key(&self,reader:&str)->Result<Option<String>,Self::Error>;
 fn manifest(&self)->Result<ManifestDataV1,Self::Error>;
 fn binding(&self)->Result<Option<String>,Self::Error>;
 /// Exact reverse retained suffix, at most the original 2048-event window.
 fn events(&self)->Result<Vec<Self::Event<'_>>,Self::Error>;
 fn visit_commits(&self,visit:impl FnMut(&Value)->Result<ControlFlow<()>,Self::Error>)->Result<(),Self::Error>;
 fn visit_receipts(&self,visit:impl FnMut(&Value)->Result<ControlFlow<()>,Self::Error>)->Result<(),Self::Error>;
 fn with_action_digest<T>(&self,id:&str,visit:impl FnOnce(&Value)->Result<T,Self::Error>)->Result<T,Self::Error>;
 fn before_value<T:serde::Serialize+?Sized>(&self,value:&T)->Result<(),Self::Error>;
 fn work(&self,steps:u64,bytes:usize)->Result<(),Self::Error>;
 fn ownership(&self,bytes:usize)->Result<(),Self::Error>;
 fn check(&self)->Result<(),Self::Error>;
 fn stream(&self,error:CanonicalCborStreamError)->Self::Error;
}
fn verify<T:serde::Serialize,S:SourcesV1>(s:&S,domain:&str,signed:&crate::service_admission::SignedReadRequest<T>)->Result<bool,S::Error>{
 let result=oasis7_client_api::world_service::verify_read_request_with_work_v1(domain,signed,s);s.check()?;
 match result{Ok(())=>Ok(true),Err(oasis7_client_api::world_service::ServiceAuthorityWorkErrorV1::Protocol(_))=>Ok(false),Err(oasis7_client_api::world_service::ServiceAuthorityWorkErrorV1::Work(e))=>Err(s.stream(e))}
}
fn eq_text<S:SourcesV1>(s:&S,a:&str,b:&str)->Result<bool,S::Error>{s.work(1,a.len()+b.len())?;Ok(a==b)}
fn eq_string<S:SourcesV1>(s:&S,a:&Value,b:&str)->Result<bool,S::Error>{s.work(1,0)?;match a.as_str(){None=>Ok(false),Some(a)=>eq_text(s,a,b)}}
fn eq_value<S:SourcesV1>(s:&S,a:&Value,b:&Value)->Result<bool,S::Error>{s.work(1,0)?;if std::mem::discriminant(a)!=std::mem::discriminant(b){return Ok(false)}match(a,b){(Value::String(a),Value::String(b))=>eq_text(s,a,b),(Value::Array(_),Value::Array(_))|(Value::Object(_),Value::Object(_))=>{s.before_value(&(a,b))?;Ok(a==b)},_=>Ok(a==b)}}
fn find_commit<S:SourcesV1>(s:&S,lineage:&Value)->Result<Option<Value>,S::Error>{let mut selected=None;s.visit_commits(|row|{if eq_string(s,&row["status"],"committed")?&&eq_value(s,&row["receipt_id"],&lineage["receipt_id"])?&&eq_value(s,&row["action_id"],&lineage["action_id"])?&&eq_value(s,&row["envelope_digest"],&lineage["envelope_digest"])?{s.before_value(row)?;selected=Some(row.clone());return Ok(ControlFlow::Break(()))}Ok(ControlFlow::Continue(()))})?;Ok(selected)}
pub(crate)fn recent_from_sources_v1<S:SourcesV1>(s:&S,reader:Option<&str>)->Result<Value,S::Error>{
 let Some(reader)=reader else{return Ok(json!([]))};let Some(identity)=s.identity(reader)?else{return Ok(json!([]))};let Some(public_key)=s.public_key(reader)?else{return Ok(json!([]))};
 let manifest=s.manifest()?;let events=s.events()?;let mut results:Vec<Value>=Vec::new();
 for event in &events{
  s.work(1,0)?;
  let WorldEventBody::CapabilityAuthorization(CapabilityAuthorizationEvent::WorldServiceIntentRecorded{record,..})=&event.body else{continue};
  let intent=&record["request"]["signed_payload"];let request=&intent["payload"]["request"];let action=&request["action"];let fence=&request["task_fence"];let lineage=&record["receipt"]["lineage"];
  if !eq_string(s,&intent["kind"],"cognition")?||!eq_string(s,&action["type"],"WorldRule")?||!eq_string(s,&action["data"]["actor_id"],reader)?||!eq_string(s,&lineage["agent_id"],reader)?||!eq_string(s,&lineage["status"],"committed")?||!eq_string(s,&fence["owner_binding"],&identity.owner_binding)?{continue}
  let generation=match fence["owner_generation"].as_str(){Some(text)=>{s.work(1,text.len())?;text.parse::<u64>().ok()},None=>None};
  if generation!=Some(identity.generation)||!eq_string(s,&fence["owner_public_key"],&public_key)?||!eq_string(s,&fence["world_id"],&manifest.world_id)?{continue}
  s.work(1,fence["genesis_digest"].as_str().map_or(0,str::len)+manifest.genesis_ref.as_ref().map_or(0,String::len))?;
  if fence["genesis_digest"].as_str()!=manifest.genesis_ref.as_deref(){continue}
  s.before_value(&intent["payload"])?;
  let Ok(signed)=serde_json::from_value::<crate::service_admission::SignedReadRequest<crate::service_admission::CognitionIntentV1>>(intent["payload"].clone())else{continue};
  if !verify(s,"cognition",&signed)?{continue}
  let action_digest=oasis7_agent_api::world_cognition::cognition_digest_with_work_v1("oasis7.cognition.action.v1",&request["action"],s);s.check()?;let action_digest=action_digest.map_err(|e|s.stream(e))?;
  let committed_action=find_commit(s,lineage)?;
  let mut prefix_valid=false;
  for prefix in &events{
   s.work(1,0)?;
   if prefix.id>=event.id||committed_action.as_ref().is_none_or(|commit|prefix.time>commit["parent_tick"].as_u64().unwrap_or(0)){continue}
   let WorldEventBody::CapabilityAuthorization(CapabilityAuthorizationEvent::WorldServiceIntentRecorded{record:prefix_record,..})=&prefix.body else{continue};
   let payload=&prefix_record["request"]["signed_payload"];
   s.work(1,0)?;
   if prefix_record["rejected"].is_string()||!eq_string(s,&payload["kind"],"scheduler")?{continue}
   s.before_value(&payload["payload"])?;
   let Ok(prefix_signed)=serde_json::from_value::<crate::service_admission::SignedReadRequest<crate::service_admission::SchedulerIntentV1>>(payload["payload"].clone())else{continue};
   let crate::service_admission::SchedulerOperationV1::TaskProviderPrefix{context}=&prefix_signed.request.operation else{continue};
   if !eq_text(s,&prefix_signed.subject_public_key,&signed.subject_public_key)?{continue}
   if !verify(s,"scheduler",&prefix_signed)?{continue}
   s.before_value(&context.fence)?;
   let fence_json=serde_json::to_value(&context.fence).ok();
   let fence_matches=match fence_json.as_ref(){Some(value)=>eq_value(s,value,fence)?,None=>false};
   if !fence_matches||!eq_text(s,&context.agent_session_id,&signed.request.request.agent_session_id)?||!eq_text(s,&context.agent_turn_id,&signed.request.request.agent_turn_id)?||!eq_text(s,&context.decision_request_id,&signed.request.request.decision_request_id)?{continue}
   s.before_value(&(&context.request_commitment,&signed.request.request.request_digest))?;
   if context.request_commitment!=signed.request.request.request_digest{continue}
   s.before_value(&(&context.context_commitment,&signed.request.request.context_digest))?;
   if context.context_commitment!=signed.request.request.context_digest{continue}
   s.work(2,0)?;
   if context.retry_seq==signed.request.request.retry_seq&&context.transport_attempt==signed.request.request.transport_attempt{prefix_valid=true;break}

  }
  if !prefix_valid{continue}
  let Some(commit_id)=committed_action.as_ref().and_then(|row|row["commit_id"].as_str())else{continue};
  let digest_matches=s.with_action_digest(commit_id,|value|eq_string(s,value,action_digest.as_str()))?;
  if !digest_matches{continue}
  if !eq_value(s,&lineage["request_digest"],&request["request"]["request_digest"])?||!eq_value(s,&lineage["decision_request_id"],&request["request"]["decision_request_id"])?||!eq_value(s,&lineage["agent_turn_id"],&request["request"]["agent_turn_id"])?||!eq_value(s,&lineage["agent_session_id"],&request["request"]["agent_session_id"])?{continue}
  let mut receipt_exists=false;s.visit_receipts(|row|{if eq_value(s,row,lineage)?{receipt_exists=true;return Ok(ControlFlow::Break(()))}Ok(ControlFlow::Continue(()))})?;
  if !receipt_exists{continue}
  let Some(action_id)=lineage["action_id"].as_str().and_then(|v|v.strip_prefix("action:")).and_then(|v|v.parse::<u64>().ok())else{continue};
  let mut duplicate=false;for row in &results{s.work(1,0)?;if row["action_id"]==action_id{duplicate=true;break}}if duplicate{continue}
  s.before_value(&(&action["data"]["request_id"],&lineage["receipt_id"]))?;s.ownership(8192)?;
  let mut result=json!({"action_id":action_id,"business_request_id":action["data"]["request_id"],"receipt_id":lineage["receipt_id"],"status":"unknown","reason":null,"event_id":null,"tick":null,"next_step":"read_current_business_state_before_retry"});
  for business in &events{
   s.work(1,0)?;if business.caused_by!=Some(CausedBy::Action(action_id)){continue}
   match &business.body{
    WorldEventBody::Domain(DomainEvent::ActionRejected{action_id:rejected,reason})if *rejected==action_id=>{s.before_value(reason)?;result["status"]=json!("rejected");result["reason"]=json!(reason);result["next_step"]=json!(if matches!(reason,crate::RejectReason::RuleDenied{notes}if notes.iter().any(|v|v=="ResourceBalanceInvalid { reason: \"manufacturing_quote_expired\" }")){"refresh_quote_and_submit_new_request"}else{"resolve_rejection_before_new_request"});},
    WorldEventBody::Domain(DomainEvent::WorldRuleCommitted{action_id:committed,actor_id,request_id,command,world_binding,..})if *committed==action_id&&actor_id==reader&&action["data"]["request_id"]==*request_id=>{if world_binding!=&s.binding()?{continue}s.before_value(command)?;if serde_json::to_value(command).ok().as_ref()!=Some(&action["data"]["command"]){continue}result["status"]=json!("committed");result["next_step"]=json!("read_current_business_state");},
    _=>continue
   }
   result["event_id"]=json!(business.id);result["tick"]=json!(business.time);break
  }
  if results.len()==results.capacity(){let cap=results.capacity().saturating_mul(2).max(4);s.ownership(cap*std::mem::size_of::<Value>())?;s.work(1,results.len()*std::mem::size_of::<Value>())?;results.reserve_exact(cap-results.len());assert_eq!(results.capacity(),cap,"recent result exact capacity")}
  results.push(result);if results.len()==RESULT_LIMIT{break}
 }
 s.before_value(&results)?;Ok(json!(results))
}
