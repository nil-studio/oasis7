//! Unique canonical owner-chat observation over fixed World/Current sources.
//! Protocol refusals are separate from source/resource failures, so a bounded
//! caller never turns a failed read or cancelled verifier into a missing goal.
use super::*;
use crate::service_admission::{CanonicalIntentResultV1,WorldServicePayloadV1};
use oasis7_proto::{viewer::{AgentChatRequest,PlayerAuthProof,CanonicalAgentChatViewV1,CanonicalAgentGoalV1,CanonicalAgentChatAuthorityV1},player_auth::VerifiedPlayerAuth};
use oasis7_agent_api::RuntimeBindingV1;
use crate::{CapabilityAgentIdentity,AgentIntentV2};
use std::{borrow::Borrow,ops::ControlFlow};
pub(crate) mod sealed{pub(crate) trait Sealed{}}
pub(crate) struct ChatProtocolDataV1<C>{note:String,_fee:Option<C>}
impl<C> ChatProtocolDataV1<C>{pub(crate) fn from_admitted_data_v1(note:String,fee:Option<C>)->Self{Self{note,_fee:fee}}pub(crate) fn from_player_refusal_v1(value:oasis7_proto::player_auth::PaidPlayerAuthRefusalV1<C>)->Self{Self{note:value.message,_fee:value.fee}}}
impl<C:std::fmt::Debug> std::fmt::Debug for ChatProtocolDataV1<C>{fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result{f.debug_tuple("CanonicalChat").field(&self.note).finish()}}
impl<C> std::fmt::Display for ChatProtocolDataV1<C>{fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result{f.write_str(&self.note)}}impl<C:std::fmt::Debug> std::error::Error for ChatProtocolDataV1<C>{}
pub(crate) trait CanonicalOwnerChatReadonlyFactsV1:sealed::Sealed{
 type Error;type Charge;type Verified:Borrow<VerifiedPlayerAuth>;
 fn chat_work(&self,steps:u64,bytes:usize)->Result<(),Self::Error>;
 fn chat_before_owned(&self,bytes:usize)->Result<(),Self::Error>;
 fn chat_refusal(&self,note:&'static str)->Result<ChatProtocolDataV1<Self::Charge>,Self::Error>;
 fn chat_identity(&self,id:&str)->Result<Option<CapabilityAgentIdentity>,Self::Error>;
 fn chat_owner_key(&self,id:&str)->Result<Result<String,ChatProtocolDataV1<Self::Charge>>,Self::Error>;
 fn chat_binding(&self)->Result<RuntimeBindingV1,Self::Error>;
 fn chat_with_intent<T>(&self,id:&str,visit:impl FnOnce(Option<&AgentIntentV2>)->Result<T,Self::Error>)->Result<T,Self::Error>;
 fn chat_visit_goal_results(&self,agent:&str,intent:&str,visit:impl FnMut(&CanonicalIntentResultV1)->Result<ControlFlow<()>,Self::Error>)->Result<(),Self::Error>;
 fn chat_verify(&self,request:&AgentChatRequest,proof:&PlayerAuthProof)->Result<Result<Self::Verified,ChatProtocolDataV1<Self::Charge>>,Self::Error>;
}
fn text<S:CanonicalOwnerChatReadonlyFactsV1>(s:&S,value:&str)->Result<String,S::Error>{s.chat_before_owned(std::mem::size_of::<String>()+value.len())?;let mut bytes=Vec::with_capacity(value.len());assert_eq!(bytes.capacity(),value.len(),"canonical chat text capacity");for part in value.as_bytes().chunks(8192){s.chat_work(1,part.len())?;bytes.extend_from_slice(part)}Ok(String::from_utf8(bytes).expect("copied original UTF-8"))}
fn equal<S:CanonicalOwnerChatReadonlyFactsV1>(s:&S,a:&str,b:&str)->Result<bool,S::Error>{s.chat_work(1,0)?;if a.len()!=b.len(){return Ok(false)}for(a,b)in a.as_bytes().chunks(8192).zip(b.as_bytes().chunks(8192)){s.chat_work(1,a.len()+b.len())?;if a!=b{return Ok(false)}}Ok(true)}
macro_rules! protocol{($value:expr)=>{match $value{Ok(value)=>value,Err(refusal)=>return Ok(Err(refusal))}};}
pub(crate) fn validate_owner_with_facts_v1<S:CanonicalOwnerChatReadonlyFactsV1>(s:&S,request:&AgentChatRequest)->Result<Result<(),ChatProtocolDataV1<S::Charge>>,S::Error>{
 let Some(proof)=request.auth.as_ref()else{return Ok(Err(s.chat_refusal("canonical chat proof required")?))};
 let verified=protocol!(s.chat_verify(request,proof)?);let verified: &VerifiedPlayerAuth=verified.borrow();
 let Some(identity)=s.chat_identity(&request.agent_id)?else{return Ok(Err(s.chat_refusal("canonical Agent identity unavailable")?))};
 let Some(extension)=request.canonical_authority.as_ref()else{return Ok(Err(s.chat_refusal("canonical chat authority required")?))};
 let binding=s.chat_binding()?;
 let mismatch="canonical chat owner, world, branch or identity fence mismatch";
 if !equal(s,&verified.player_id,&identity.owner_binding)?{return Ok(Err(s.chat_refusal(mismatch)?))}
 let key=protocol!(s.chat_owner_key(&request.agent_id)?);if !equal(s,&verified.public_key,&key)?{return Ok(Err(s.chat_refusal(mismatch)?))}
 s.chat_work(1,0)?;if extension.agent_identity_generation!=identity.generation||!equal(s,&extension.branch_id,&binding.branch_id)?{return Ok(Err(s.chat_refusal(mismatch)?))}
 let Some(world)=request.world_id.as_deref()else{return Ok(Err(s.chat_refusal(mismatch)?))};if !equal(s,world,&binding.world_id)?{return Ok(Err(s.chat_refusal(mismatch)?))}
 s.chat_work(1,0)?;if request.reorg_epoch!=Some(binding.reorg_epoch){return Ok(Err(s.chat_refusal(mismatch)?))}
 let Some(scope)=request.authority_scope.as_deref()else{return Ok(Err(s.chat_refusal(mismatch)?))};if !equal(s,scope,"player_agent_chat")?{return Ok(Err(s.chat_refusal(mismatch)?))}
 s.chat_work(1,0)?;if request.intent_seq.is_none_or(|seq|seq==0)||request.intent_tick.is_none(){return Ok(Err(s.chat_refusal(mismatch)?))}Ok(Ok(()))
}
pub(crate) fn owner_view_with_facts_v1<S:CanonicalOwnerChatReadonlyFactsV1>(s:&S,agent:&str)->Result<Result<Option<CanonicalAgentChatViewV1>,ChatProtocolDataV1<S::Charge>>,S::Error>{
 let Some(identity)=s.chat_identity(agent)?else{return Ok(Ok(None))};let Ok(public_key)=s.chat_owner_key(agent)?else{return Ok(Ok(None))};let binding=s.chat_binding()?;
 s.chat_with_intent(agent,|current|{
  let current_intent_id=match current{Some(intent)if !matches!(intent.status.as_str(),"completed"|"rejected"|"expired"|"cancelled"|"superseded")=>Some(text(s,&intent.intent_id)?),_=>None};let mut goal=None;let mut refusal=None;
  if let Some(intent)=current{s.chat_visit_goal_results(agent,&intent.intent_id,|result|{
   s.chat_work(1,0)?;if result.rejected.is_some(){return Ok(ControlFlow::Continue(()))}let count=result.receipt.as_object().map_or(0,|v|v.len());s.chat_work(((count+1).ilog2()as u64+2)*4,((count+1).ilog2()as usize+2)*32)?;
   let Some(id)=result.receipt.get("intent_id").and_then(serde_json::Value::as_str)else{return Ok(ControlFlow::Continue(()))};if !equal(s,id,&intent.intent_id)?{return Ok(ControlFlow::Continue(()))}
   let WorldServicePayloadV1::AgentChat(request)=&result.request.signed_payload else{return Ok(ControlFlow::Continue(()))};if !equal(s,&request.agent_id,agent)?{return Ok(ControlFlow::Continue(()))}
   // Source errors propagate; only the original current-owner/crypto refusal
   // is ignored by this historical-goal filter.
   if validate_owner_with_facts_v1(s,request)?.is_err(){return Ok(ControlFlow::Continue(()))}
   if goal.is_some(){refusal=Some(s.chat_refusal("ambiguous canonical Agent goal result")?);return Ok(ControlFlow::Break(()))}
   s.chat_before_owned(std::mem::size_of::<CanonicalAgentGoalV1>())?;goal=Some(CanonicalAgentGoalV1{intent_id:text(s,&intent.intent_id)?,message:text(s,&request.message)?,status:text(s,&intent.status)?,event_seq:intent.event_seq,logical_time:intent.logical_time});Ok(ControlFlow::Continue(()))
  })?;}
  if let Some(refusal)=refusal{return Ok(Err(refusal))}s.chat_before_owned(std::mem::size_of::<CanonicalAgentChatViewV1>())?;
  Ok(Ok(Some(CanonicalAgentChatViewV1{agent_id:text(s,agent)?,player_id:text(s,&identity.owner_binding)?,public_key,world_id:binding.world_id,reorg_epoch:binding.reorg_epoch,authority_scope:text(s,"player_agent_chat")?,canonical_authority:CanonicalAgentChatAuthorityV1{branch_id:binding.branch_id,agent_identity_generation:identity.generation},current_intent_id,goal})))
 })
}
pub(super) struct Legacy<'a>(pub &'a World);
impl sealed::Sealed for Legacy<'_>{}
impl CanonicalOwnerChatReadonlyFactsV1 for Legacy<'_>{
 type Error=String;type Charge=();type Verified=VerifiedPlayerAuth;
 fn chat_work(&self,_:u64,_:usize)->Result<(),String>{Ok(())}fn chat_before_owned(&self,_:usize)->Result<(),String>{Ok(())}fn chat_refusal(&self,note:&'static str)->Result<ChatProtocolDataV1<()>,String>{Ok(ChatProtocolDataV1{note:note.into(),_fee:None})}
 fn chat_identity(&self,id:&str)->Result<Option<CapabilityAgentIdentity>,String>{Ok(self.0.current_agent_identity(id))}
 fn chat_owner_key(&self,id:&str)->Result<Result<String,ChatProtocolDataV1<()>>,String>{Ok(super::super::agent_authority::owner_public_key(self.0,id).map_err(|note|ChatProtocolDataV1{note,_fee:None}))}
 fn chat_binding(&self)->Result<RuntimeBindingV1,String>{self.0.current_cognition_runtime_binding().map_err(|error|format!("{error:?}"))}
 fn chat_with_intent<T>(&self,id:&str,visit:impl FnOnce(Option<&AgentIntentV2>)->Result<T,String>)->Result<T,String>{visit(self.0.state().agents.get(id).and_then(|agent|agent.intent.as_ref()))}
 fn chat_visit_goal_results(&self,agent:&str,intent:&str,mut visit:impl FnMut(&CanonicalIntentResultV1)->Result<ControlFlow<()>,String>)->Result<(),String>{for result in super::super::retained_queries::agent_chat_goal_results(self.0,agent,intent)?.into_values(){if visit(&result)?.is_break(){break}}Ok(())}
 fn chat_verify(&self,request:&AgentChatRequest,proof:&PlayerAuthProof)->Result<Result<VerifiedPlayerAuth,ChatProtocolDataV1<()>>,String>{Ok(oasis7_proto::player_auth::verify_agent_chat_auth_proof(request,proof).map_err(|note|ChatProtocolDataV1{note,_fee:None}))}
}
pub(super) fn flatten_legacy<T>(value:Result<Result<T,ChatProtocolDataV1<()>>,String>)->Result<T,String>{value?.map_err(|error|{debug_assert!(error._fee.is_none());error.note})}
