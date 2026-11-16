//! Original Runtime credential/industrial-owner joins over actual readonly
//! records. No trait method accepts an ownership or authorization verdict.
use crate::{World,CapabilityAgentIdentity,ResolvedAgentOwnerV1};
use oasis7_world_state::world_rules::{IndustrialOwnerReadonlyFactsV1,resolve_industrial_owner_readonly_v1,is_manufactured_agent_readonly_v1};
use oasis7_agent_api::game::{GameParticipantReceiptV1,GameIdentityNamespaceV1};
use oasis7_world_state::service_admission::participant::ParticipantEntryPolicyV1;
use std::{convert::Infallible,mem::size_of,ops::ControlFlow};
pub(in crate::world) trait RuntimeOwnerReadonlyFactsV1:IndustrialOwnerReadonlyFactsV1{
 fn runtime_visit_participants(&self,visit:impl FnMut(&GameParticipantReceiptV1)->Result<ControlFlow<()>,Self::Error>)->Result<(),Self::Error>;
 fn runtime_with_participant_policy<T>(&self,visit:impl FnOnce(Option<&ParticipantEntryPolicyV1>)->Result<T,Self::Error>)->Result<T,Self::Error>;
 fn runtime_visit_namespaces(&self,visit:impl FnMut(&GameIdentityNamespaceV1)->Result<ControlFlow<()>,Self::Error>)->Result<(),Self::Error>;
 fn runtime_with_identity<T>(&self,id:&str,visit:impl FnOnce(Option<&CapabilityAgentIdentity>)->Result<T,Self::Error>)->Result<T,Self::Error>;
 fn runtime_manufactured_sequence(&self,id:&str)->Result<Option<u64>,Self::Error>;
 fn runtime_manifest<T>(&self,visit:impl FnOnce(&str,&str,Option<&str>)->Result<T,Self::Error>)->Result<T,Self::Error>;
}
fn text<S:IndustrialOwnerReadonlyFactsV1>(s:&S,v:&str)->Result<String,S::Error>{s.owner_work(1,v.len())?;s.owner_before_owned(size_of::<String>()+v.len())?;Ok(v.to_owned())}
fn format<S:IndustrialOwnerReadonlyFactsV1>(s:&S,args:std::fmt::Arguments<'_>)->Result<String,S::Error>{
 // Shared streaming original fmt work kernel is used through the WorldState
 // public DATA helper, so codec and temporary capacity do not drift.
 oasis7_world_state::world_rules::format_readonly_projection_data_v1(args,&mut|steps,bytes,heap|{s.owner_work(steps,bytes)?;if heap==0{Ok(())}else{s.owner_before_owned(heap)}})
}
pub(in crate::world) fn world_rule_binding_readonly_v1<S:RuntimeOwnerReadonlyFactsV1>(s:&S)->Result<Option<String>,S::Error>{
 s.runtime_manifest(|world,chain,genesis|{
  let Some(genesis)=genesis.filter(|s|!s.is_empty())else{return Ok(None)};if world.is_empty()||chain.is_empty(){return Ok(None)}
  s.owner_before_owned(size_of::<String>()+64)?;
  match oasis7_world_state::util::hash_json_with_work_v1(&("oasis7-world-rule-world-v1",world,chain,genesis),&mut|steps,bytes|s.owner_work(steps,bytes as usize)){Ok(value)=>Ok(Some(value)),Err(oasis7_world_state::util::JsonHashWorkErrorV1::Work(error))=>Err(error),Err(oasis7_world_state::util::JsonHashWorkErrorV1::Codec(_))=>Ok(None)}
 })
}
pub(in crate::world) fn namespace_was_authorized_readonly_v1<S:RuntimeOwnerReadonlyFactsV1>(s:&S,namespace:&GameIdentityNamespaceV1)->Result<bool,S::Error>{
 let mut found=false;s.runtime_visit_namespaces(|value|{s.owner_work(1,value.service_id.len()+namespace.service_id.len()+value.environment_id.len()+namespace.environment_id.len()+size_of::<u64>())?;if value==namespace{found=true;Ok(ControlFlow::Break(()))}else{Ok(ControlFlow::Continue(()))}})?;Ok(found)
}
pub(in crate::world) fn current_agent_identity_readonly_v1<S:RuntimeOwnerReadonlyFactsV1>(s:&S,agent:&str)->Result<Option<CapabilityAgentIdentity>,S::Error>{
 if !is_manufactured_agent_readonly_v1(s,agent)?{return s.runtime_with_identity(agent,|value|match value{None=>Ok(None),Some(v)=>Ok(Some(CapabilityAgentIdentity{owner_binding:text(s,&v.owner_binding)?,generation:v.generation}))})}
 let mut delivered=None;
 s.owner_visit_orders(|order|{s.owner_work(1,order.target_agent_id.len()+agent.len())?;if order.target_agent_id==agent&&order.delivered_at.is_some(){
  let binding=world_rule_binding_readonly_v1(s)?;s.owner_work(1,order.quote.world_binding.as_ref().map_or(0,String::len)+binding.as_ref().map_or(0,String::len))?;
  delivered=Some(order.quote.world_binding==binding);Ok(ControlFlow::Break(()))
 }else{Ok(ControlFlow::Continue(()))}})?;
 if delivered!=Some(true){return Ok(None)}let Some(owner)=resolve_industrial_owner_readonly_v1(s,agent)? else{return Ok(None)};
 let root_matches=s.runtime_with_identity(&owner.primary_agent_id,|root|{if let Some(root)=root{s.owner_work(1,root.owner_binding.len()+owner.player_id.len())?;Ok(root.owner_binding==owner.player_id)}else{Ok(false)}})?;
 if !root_matches{return Ok(None)}
 let generation=s.runtime_with_identity(agent,|target|{let sequence=s.runtime_manufactured_sequence(agent)?;Ok(oasis7_world_state::incremental_runtime_revocation::resolve_manufactured_identity_generation_v1(target,sequence,owner.generation))})?;
 Ok(generation.map(|generation|CapabilityAgentIdentity{owner_binding:owner.player_id,generation}))
}
pub(in crate::world) fn current_agent_owner_readonly_v1<S:RuntimeOwnerReadonlyFactsV1>(s:&S,agent:&str)->Result<Option<ResolvedAgentOwnerV1>,S::Error>{
 let mut participant=None;let mut selected=false;
 s.runtime_visit_participants(|receipt|{
  s.owner_work(1,receipt.agent_id.len()+agent.len())?;if receipt.agent_id!=agent{return Ok(ControlFlow::Continue(()))}selected=true;
  participant=s.runtime_with_participant_policy(|policy|{
   let Some(policy)=policy else{return Ok(None)};let Some(identity)=current_agent_identity_readonly_v1(s,agent)? else{return Ok(None)};
   if !s.owner_agent_present(agent)?||is_manufactured_agent_readonly_v1(s,agent)?{return Ok(None)}
   let manifest_matches=s.runtime_manifest(|world,_,genesis|{s.owner_work(1,receipt.world_id.len()+world.len()+receipt.genesis_digest.len()+genesis.map_or(0,str::len))?;Ok(receipt.world_id==world&&genesis==Some(receipt.genesis_digest.as_str()))})?;
   if !manifest_matches{return Ok(None)}s.owner_work(1,receipt.identity_namespace.service_id.len()+policy.identity_namespace.service_id.len()+receipt.identity_namespace.environment_id.len()+policy.identity_namespace.environment_id.len())?;
   if !receipt.identity_namespace.same_authority(&policy.identity_namespace)||!namespace_was_authorized_readonly_v1(s,&receipt.identity_namespace)?{return Ok(None)}
   s.owner_work(1,identity.owner_binding.len()+receipt.owner_binding.len())?;
   if identity.owner_binding!=receipt.owner_binding||identity.generation!=receipt.owner_generation.0{return Ok(None)}
   let claim_bad=s.owner_with_claim(agent,|claim|{if let Some(claim)=claim{s.owner_work(1,claim.claim_owner_id.len()+agent.len())?;Ok(claim.claim_owner_id!=agent||claim.control_source.is_some())}else{Ok(false)}})?;if claim_bad{return Ok(None)}
   let starter_bad=s.owner_with_starter(agent,|starter|{if let Some(starter)=starter{s.owner_work(1,starter.agent_id.len()+agent.len()+starter.player_id.len()+receipt.owner_binding.len()+starter.public_key.as_ref().map_or(0,String::len)+receipt.owner_public_key.len())?;Ok(starter.agent_id!=agent||starter.player_id!=receipt.owner_binding||starter.public_key.as_ref()!=Some(&receipt.owner_public_key))}else{Ok(false)}})?;if starter_bad{return Ok(None)}
   s.owner_before_owned(size_of::<ResolvedAgentOwnerV1>())?;
   Ok(Some(ResolvedAgentOwnerV1{primary_agent_id:text(s,agent)?,player_id:text(s,&receipt.owner_binding)?,public_key:Some(text(s,&receipt.owner_public_key)?),source_id:format(s,format_args!("participant:{}",receipt.business_digest))?,generation:identity.generation}))
  })?;Ok(ControlFlow::Break(()))
 })?;
 if selected{return Ok(participant)}let Some(mut owner)=resolve_industrial_owner_readonly_v1(s,agent)?else{return Ok(None)};
 if !is_manufactured_agent_readonly_v1(s,agent)?{return Ok(Some(owner))}
 let Some(identity)=current_agent_identity_readonly_v1(s,agent)?else{return Ok(None)};s.owner_work(1,owner.player_id.len()+identity.owner_binding.len())?;if owner.player_id!=identity.owner_binding{return Ok(None)}
 if is_manufactured_agent_readonly_v1(s,agent)?{owner.source_id=format(s,format_args!("{}:identity:{}",owner.source_id,identity.generation))?;owner.generation=identity.generation;}Ok(Some(owner))
}

impl IndustrialOwnerReadonlyFactsV1 for World{
 type Error=Infallible;
 fn owner_work(&self,_:u64,_:usize)->Result<(),Self::Error>{Ok(())}
 fn owner_before_owned(&self,_:usize)->Result<(),Self::Error>{Ok(())}
 fn owner_agent_present(&self,id:&str)->Result<bool,Self::Error>{self.state.owner_agent_present(id)}
 fn owner_visit_orders(&self,visit:impl FnMut(&oasis7_world_state::world_rules::AgentIndustrialOrderV1)->Result<ControlFlow<()>,Self::Error>)->Result<(),Self::Error>{self.state.owner_visit_orders(visit)}
 fn owner_with_claim<T>(&self,id:&str,visit:impl FnOnce(Option<&oasis7_world_state::AgentClaimState>)->Result<T,Self::Error>)->Result<T,Self::Error>{self.state.owner_with_claim(id,visit)}
 fn owner_with_starter<T>(&self,id:&str,visit:impl FnOnce(Option<&oasis7_world_state::StarterOcClaimState>)->Result<T,Self::Error>)->Result<T,Self::Error>{self.state.owner_with_starter(id,visit)}
 fn owner_acquisition_sequence(&self,id:&str)->Result<Option<u64>,Self::Error>{self.state.owner_acquisition_sequence(id)}
}
impl RuntimeOwnerReadonlyFactsV1 for World{
 fn runtime_visit_participants(&self,mut visit:impl FnMut(&GameParticipantReceiptV1)->Result<ControlFlow<()>,Self::Error>)->Result<(),Self::Error>{for receipt in self.capability_revocation_state.participants.bindings.values(){if visit(receipt)?.is_break(){break}}Ok(())}
 fn runtime_with_participant_policy<T>(&self,visit:impl FnOnce(Option<&ParticipantEntryPolicyV1>)->Result<T,Self::Error>)->Result<T,Self::Error>{visit(self.capability_revocation_state.participants.policy.as_ref())}
 fn runtime_visit_namespaces(&self,mut visit:impl FnMut(&GameIdentityNamespaceV1)->Result<ControlFlow<()>,Self::Error>)->Result<(),Self::Error>{
  let registry=&self.capability_revocation_state.participants;
  if let Some(policy)=&registry.policy{if visit(&policy.identity_namespace)?.is_break(){return Ok(())}}
  for authorization in &registry.policy_history{if visit(&authorization.policy.identity_namespace)?.is_break(){return Ok(())}}
  for authorization in &registry.devlocal_policy_history{if visit(&authorization.request.policy.identity_namespace)?.is_break(){return Ok(())}}Ok(())
 }
 fn runtime_with_identity<T>(&self,id:&str,visit:impl FnOnce(Option<&CapabilityAgentIdentity>)->Result<T,Self::Error>)->Result<T,Self::Error>{visit(self.capability_revocation_state.agent_identities.get(id))}
 fn runtime_manufactured_sequence(&self,id:&str)->Result<Option<u64>,Self::Error>{Ok(self.capability_revocation_state.manufactured_identity_acquisition_sequences.get(id).copied())}
 fn runtime_manifest<T>(&self,visit:impl FnOnce(&str,&str,Option<&str>)->Result<T,Self::Error>)->Result<T,Self::Error>{let m=&self.chain_resource_manifest;visit(&m.world_id,&m.chain_id,m.genesis_ref.as_deref())}
}
/// The same canonical owner-key join used by authenticated service reads.
pub(in crate::world) fn owner_public_key_readonly_v1<S:RuntimeOwnerReadonlyFactsV1>(s:&S,agent:&str)->Result<Result<String,&'static str>,S::Error>{
 let Some(identity)=current_agent_identity_readonly_v1(s,agent)?else{return Ok(Err("Agent identity unavailable"))};
 let Some(owner)=current_agent_owner_readonly_v1(s,agent)?else{return Ok(Err("canonical owner claim unavailable"))};
 s.owner_work(1,identity.owner_binding.len()+owner.player_id.len())?;
 if let Err(error)=crate::service_admission::agent_authority::owner_public_key_from_original_owner_v1(&identity.owner_binding,&owner){return Ok(Err(error))}
 Ok(owner.public_key.ok_or("canonical owner key unavailable"))
}
