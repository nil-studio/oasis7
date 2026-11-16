//! Original industrial owner resolution over readonly raw source facts.
use crate::{WorldState,ResolvedAgentOwnerV1,AgentClaimState,StarterOcClaimState};
use super::{AgentIndustrialOrderV1,AgentIndustrialPhaseV1};
use std::{convert::Infallible,mem::size_of,ops::ControlFlow};
pub trait IndustrialOwnerReadonlyFactsV1 {
    type Error;
    fn owner_work(&self,steps:u64,bytes:usize)->Result<(),Self::Error>;
    fn owner_before_owned(&self,bytes:usize)->Result<(),Self::Error>;
    fn owner_agent_present(&self,agent:&str)->Result<bool,Self::Error>;
    fn owner_visit_orders(&self,visit:impl FnMut(&AgentIndustrialOrderV1)->Result<ControlFlow<()>,Self::Error>)->Result<(),Self::Error>;
    fn owner_with_claim<T>(&self,agent:&str,visit:impl FnOnce(Option<&AgentClaimState>)->Result<T,Self::Error>)->Result<T,Self::Error>;
    fn owner_with_starter<T>(&self,agent:&str,visit:impl FnOnce(Option<&StarterOcClaimState>)->Result<T,Self::Error>)->Result<T,Self::Error>;
    fn owner_acquisition_sequence(&self,agent:&str)->Result<Option<u64>,Self::Error>;
}
fn own<S:IndustrialOwnerReadonlyFactsV1>(s:&S,text:&str)->Result<String,S::Error>{s.owner_work(1,text.len())?;s.owner_before_owned(text.len()+size_of::<String>())?;Ok(text.to_owned())}
pub fn is_manufactured_agent_readonly_v1<S:IndustrialOwnerReadonlyFactsV1>(s:&S,agent:&str)->Result<bool,S::Error>{
    let mut manufactured=false;
    s.owner_visit_orders(|order|{s.owner_work(1,order.target_agent_id.len()+agent.len())?;if order.target_agent_id==agent{manufactured=true;return Ok(ControlFlow::Break(()));}Ok(ControlFlow::Continue(()))})?;
    Ok(manufactured)
}
pub fn resolve_industrial_owner_readonly_v1<S:IndustrialOwnerReadonlyFactsV1>(s:&S,agent:&str)->Result<Option<ResolvedAgentOwnerV1>,S::Error>{
    if !s.owner_agent_present(agent)?{return Ok(None);}
    let manufactured=is_manufactured_agent_readonly_v1(s,agent)?;
    if manufactured {
        let mut delivered=false;
        s.owner_visit_orders(|order|{s.owner_work(1,order.target_agent_id.len()+agent.len())?;if order.target_agent_id==agent&&order.delivered_at.is_some()&&order.phase==AgentIndustrialPhaseV1::Delivered{delivered=true;return Ok(ControlFlow::Break(()));}Ok(ControlFlow::Continue(()))})?;
        if !delivered{return Ok(None);}
        return s.owner_with_claim(agent,|claim|{
            let Some(claim)=claim else{return Ok(None)};let Some(source)=claim.control_source.as_ref() else{return Ok(None)};
            s.owner_work(1,claim.target_agent_id.len()+agent.len()+claim.claim_owner_id.len()+source.root_agent_id.len())?;
            if claim.target_agent_id!=agent||claim.claim_owner_id!=source.root_agent_id||source.acquired_event_digest.len()!=64||source.acquisition_sequence==0||s.owner_acquisition_sequence(agent)?!=Some(source.acquisition_sequence){return Ok(None);}
            s.owner_with_starter(&source.root_agent_id,|root|{let Some(root)=root else{return Ok(None)};
                s.owner_work(1,root.agent_id.len()+source.root_agent_id.len()+root.player_id.len()+source.player_id.len())?;
                if root.agent_id!=source.root_agent_id||root.player_id!=source.player_id||root.claimed_at!=source.root_claimed_at||root.player_id.is_empty()||!s.owner_agent_present(&root.agent_id)?||root.public_key.as_ref().is_none_or(|key|key.is_empty()){return Ok(None);}
                s.owner_before_owned(size_of::<ResolvedAgentOwnerV1>())?;
                let primary_agent_id=own(s,&source.root_agent_id)?;let player_id=own(s,&source.player_id)?;let public_key=root.public_key.as_deref().map(|key|own(s,key)).transpose()?;
                let n="manufactured_claim:".len()+source.acquired_event_digest.len();s.owner_work(1,n)?;s.owner_before_owned(n+size_of::<String>())?;
                Ok(Some(ResolvedAgentOwnerV1{primary_agent_id,player_id,public_key,source_id:format!("manufactured_claim:{}",source.acquired_event_digest),generation:source.acquisition_sequence}))
            })
        });
    }
    s.owner_with_starter(agent,|root|{let Some(root)=root else{return Ok(None)};
        s.owner_work(1,root.agent_id.len()+agent.len())?;if root.agent_id!=agent||root.player_id.is_empty(){return Ok(None);}
        s.owner_before_owned(size_of::<ResolvedAgentOwnerV1>())?;
        let primary_agent_id=own(s,agent)?;let player_id=own(s,&root.player_id)?;let public_key=root.public_key.as_deref().map(|key|own(s,key)).transpose()?;
        let digits=if root.claimed_at==0{1}else{root.claimed_at.ilog10() as usize+1};let n="starter_claim:".len()+agent.len()+1+digits;s.owner_work(1,n)?;s.owner_before_owned(n+size_of::<String>())?;
        Ok(Some(ResolvedAgentOwnerV1{primary_agent_id,player_id,public_key,source_id:format!("starter_claim:{agent}:{}",root.claimed_at),generation:1}))
    })
}
impl IndustrialOwnerReadonlyFactsV1 for WorldState {
    type Error=Infallible;
    fn owner_work(&self,_:u64,_:usize)->Result<(),Infallible>{Ok(())}
    fn owner_before_owned(&self,_:usize)->Result<(),Infallible>{Ok(())}
    fn owner_agent_present(&self,agent:&str)->Result<bool,Infallible>{Ok(self.agents.contains_key(agent))}
    fn owner_visit_orders(&self,mut visit:impl FnMut(&AgentIndustrialOrderV1)->Result<ControlFlow<()>,Infallible>)->Result<(),Infallible>{if let Some(rules)=&self.world_rules{for order in rules.agent_orders.values(){if visit(order)?.is_break(){break;}}}Ok(())}
    fn owner_with_claim<T>(&self,agent:&str,visit:impl FnOnce(Option<&AgentClaimState>)->Result<T,Infallible>)->Result<T,Infallible>{visit(self.agent_claims.get(agent))}
    fn owner_with_starter<T>(&self,agent:&str,visit:impl FnOnce(Option<&StarterOcClaimState>)->Result<T,Infallible>)->Result<T,Infallible>{visit(self.starter_oc_claims.get(agent))}
    fn owner_acquisition_sequence(&self,agent:&str)->Result<Option<u64>,Infallible>{Ok(self.agent_claim_acquisition_sequences.get(agent).copied())}
}
