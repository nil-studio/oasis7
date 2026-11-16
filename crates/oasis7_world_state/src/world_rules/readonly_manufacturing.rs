//! Manufacturing quotation over original readonly raw facts, not supplied
//! eligibility/status results. Source errors remain distinct from WorldError.
use super::{industrial::*,industrial_eligibility_kernel::*,readonly_owner::*,invalid};
use crate::{WorldState,WorldError,WorldTime,FactoryState,FactorySiteAuthorityV1,LocationAnchorV1,GameplayPolicyState,AgentClaimState,RecipeJobState,MaterialLedgerId,MainTokenAccountBalance,ResolvedAgentOwnerV1};
use oasis7_agent_api::world_action::ResourceKind;
use std::{collections::{BTreeMap,BTreeSet},convert::Infallible,mem::size_of,ops::ControlFlow};
pub trait ManufacturingQuoteReadonlyFactsV1:IndustrialOwnerReadonlyFactsV1 {
    fn quote_with_factory<T>(&self,id:&str,visit:impl FnOnce(Option<&FactoryState>)->Result<T,Self::Error>)->Result<T,Self::Error>;
    fn quote_with_site<T>(&self,id:&str,visit:impl FnOnce(Option<&FactorySiteAuthorityV1>)->Result<T,Self::Error>)->Result<T,Self::Error>;
    fn quote_with_anchor<T>(&self,id:&str,visit:impl FnOnce(Option<&LocationAnchorV1>)->Result<T,Self::Error>)->Result<T,Self::Error>;
    fn quote_with_policy<T>(&self,visit:impl FnOnce(&GameplayPolicyState)->Result<T,Self::Error>)->Result<T,Self::Error>;
    fn quote_factory_retired(&self,id:&str)->Result<bool,Self::Error>;
    fn quote_visit_claims(&self,visit:impl FnMut(&AgentClaimState)->Result<(),Self::Error>)->Result<(),Self::Error>;
    fn quote_visit_jobs(&self,visit:impl FnMut(&RecipeJobState)->Result<(),Self::Error>)->Result<(),Self::Error>;
    fn quote_with_ledger<T>(&self,id:&MaterialLedgerId,visit:impl FnOnce(Option<&BTreeMap<String,i64>>)->Result<T,Self::Error>)->Result<T,Self::Error>;
    /// None means actual missing Agent; missing Electricity in an existing
    /// original ResourceStock is Some(0), as in ResourceStock::get.
    fn quote_agent_electricity(&self,id:&str)->Result<Option<i64>,Self::Error>;
    fn quote_with_account<T>(&self,id:&str,visit:impl FnOnce(Option<&MainTokenAccountBalance>)->Result<T,Self::Error>)->Result<T,Self::Error>;
    fn quote_reputation(&self,id:&str)->Result<Option<i64>,Self::Error>;
}
macro_rules! domain {($e:expr)=>{match $e {Ok(value)=>value,Err(error)=>return Ok(Err(error))}};}
fn own<S:IndustrialOwnerReadonlyFactsV1>(s:&S,text:&str)->Result<String,S::Error>{s.owner_work(1,text.len())?;s.owner_before_owned(text.len()+size_of::<String>())?;Ok(text.to_owned())}
fn insert<S:IndustrialOwnerReadonlyFactsV1>(s:&S,set:&mut BTreeSet<String>,key:&str)->Result<(),S::Error>{let levels=(set.len()+1).ilog2() as usize+2;s.owner_work(1,(key.len()+1).saturating_mul(levels*2+4)+512*levels)?;s.owner_before_owned(key.len()+size_of::<String>()+512*levels)?;set.insert(key.to_owned());Ok(())}
fn primary<S:IndustrialOwnerReadonlyFactsV1>(s:&S,owner:&str)->Result<Result<(),WorldError>,S::Error>{
    let exists=s.owner_agent_present(owner)?;
    if !exists{return Ok(validate_primary_production_facts_v1(PrimaryProductionFactsV1{agent_exists:false,starter_claim:None},owner));}
    s.owner_with_starter(owner,|claim|{s.owner_work(1,claim.map_or(0,|c|c.agent_id.len())+owner.len())?;Ok(validate_primary_production_facts_v1(PrimaryProductionFactsV1{agent_exists:true,starter_claim:claim},owner))})
}
fn existing<S:ManufacturingQuoteReadonlyFactsV1>(s:&S,owner:&str)->Result<Result<BTreeSet<String>,WorldError>,S::Error>{
    domain!(primary(s,owner)?);let mut set=BTreeSet::new();insert(s,&mut set,owner)?;
    s.quote_visit_claims(|claim|{s.owner_work(1,claim.claim_owner_id.len()+owner.len())?;if claim.claim_owner_id==owner{insert(s,&mut set,&claim.target_agent_id)?;}Ok(())})?;Ok(Ok(set))
}
fn reserved<S:IndustrialOwnerReadonlyFactsV1>(s:&S,owner:&str)->Result<BTreeSet<String>,S::Error>{let mut set=BTreeSet::new();s.owner_visit_orders(|order|{s.owner_work(1,order.owner_id.len()+owner.len())?;if order.owner_id==owner&&!order.is_terminal(){insert(s,&mut set,&order.target_agent_id)?;}Ok(ControlFlow::Continue(()))})?;Ok(set)}
fn capacity<S:ManufacturingQuoteReadonlyFactsV1>(s:&S,owner:&str,target:Option<&str>,new_order:bool)->Result<Result<(),WorldError>,S::Error>{
    let set=domain!(existing(s,owner)?);let pending=reserved(s,owner)?;
    let reputation=s.quote_reputation(owner)?.unwrap_or(0);
    manufacturing_capacity_from_targets_with_work_v1(set,pending.iter().map(String::as_str),target,new_order,reputation,&mut |bytes|{s.owner_work(1,bytes)?;s.owner_before_owned(bytes)})
}
fn production<S:ManufacturingQuoteReadonlyFactsV1>(s:&S,owner:&str,factory_id:&str,location:&str)->Result<Result<(),WorldError>,S::Error>{
    domain!(primary(s,owner)?);
    let retired=s.quote_factory_retired(factory_id)?;
    let blocked=s.quote_with_policy(|policy|{for agent in &policy.blocked_agents{s.owner_work(1,agent.len()+owner.len())?;if agent==owner{return Ok(true);}}Ok(false)})?;
    s.owner_with_starter(owner,|claim|{
        let primary=PrimaryProductionFactsV1{agent_exists:true,starter_claim:claim};
        if retired||blocked{return Ok(production_factory_eligibility_from_facts_v1(primary,retired,blocked,None,owner).map(|_|()));}
        s.quote_with_factory(factory_id,|factory|{
            let Some(f)=factory else{return Ok(production_factory_eligibility_from_facts_v1(primary,false,false,None,owner).map(|_|()))};
            // Exact closed assembler catalog branch, including the overwritten
            // original factory-id allocation, admitted before construction.
            let static_bytes=size_of::<oasis7_wasm_abi::FactoryModuleSpec>()+2*size_of::<String>()+3*size_of::<oasis7_wasm_abi::MaterialStack>()+"factory.assembler.mk1Assembler MK1assemblerprecisionstructural_frameiron_ingotcopper_wire".len()+factory_id.len();
            s.owner_work(1,static_bytes)?;s.owner_before_owned(static_bytes)?;
            let canonical=crate::builtin_factory_spec("factory.assembler.mk1",factory_id);
            s.owner_work(1,static_bytes+f.builder_agent_id.len()+owner.len())?;
            let facts=FactoryProductionFactsV1{builder_agent_id:&f.builder_agent_id,builtin_spec_available:canonical.is_some(),spec_matches_builtin:canonical.as_ref().is_some_and(|spec|&f.spec==spec),recipe_slots:u32::from(f.spec.recipe_slots),durability_ppm:f.durability_ppm,input_ledger:&f.input_ledger,site_location_id:f.site_location_id.as_deref()};
            let facts=domain!(production_factory_eligibility_from_facts_v1(primary,false,false,Some(facts),owner));
            s.quote_with_site(&f.site_id,|site|{
                let mut authorized=false;if let Some(site)=site{for actor in &site.authorized_agent_ids{s.owner_work(1,actor.len()+owner.len())?;if actor==owner{authorized=true;break;}}}
                let site_facts=site.map(|site|FactorySiteProductionFactsV1{active:site.active,chunk_ready:site.chunk_ready,location_id:&site.location_id,owner_agent_id:&site.owner_agent_id,owner_authorized:authorized});
                s.quote_with_anchor(location,|anchor|{
                    s.owner_work(1,site.map_or(0,|site|site.location_id.len()+site.owner_agent_id.len())+location.len()+owner.len()+f.site_location_id.as_ref().map_or(0,String::len))?;
                    Ok(production_site_eligibility_from_facts_v1(facts,site_facts,anchor.map(|anchor|anchor.active),owner,location))
                })
            })
        })
    })
}
fn hash<S:IndustrialOwnerReadonlyFactsV1,T:serde::Serialize>(s:&S,value:&T)->Result<Result<String,WorldError>,S::Error>{
    s.owner_before_owned(64+size_of::<String>())?;
    match crate::util::hash_json_with_work_v1(value,&mut |steps,bytes|s.owner_work(steps,usize::try_from(bytes).unwrap_or(usize::MAX))){Ok(value)=>Ok(Ok(value)),Err(crate::util::JsonHashWorkErrorV1::Codec(error))=>Ok(Err(error.into())),Err(crate::util::JsonHashWorkErrorV1::Work(error))=>Err(error)}
}
fn ledger_len(id:&MaterialLedgerId)->usize{match id{MaterialLedgerId::World=>1,MaterialLedgerId::Agent(v)|MaterialLedgerId::Site(v)|MaterialLedgerId::Factory(v)=>v.len()+1}}
pub fn order_production_eligibility_readonly_v1<S:ManufacturingQuoteReadonlyFactsV1>(s:&S,order:&AgentIndustrialOrderV1)->Result<Result<(),WorldError>,S::Error>{
    domain!(production(s,&order.owner_id,&order.factory_id,&order.quote.delivery_location_id)?);
    s.quote_with_factory(&order.factory_id,|factory|{let Some(factory)=factory else{return Ok(Err(invalid("factory_missing")))};
        s.owner_work(1,ledger_len(&factory.input_ledger)+ledger_len(&order.quote.input_ledger))?;
        domain!(order_input_eligibility_from_facts_v1(&factory.input_ledger,order));
        let owner=resolve_industrial_owner_readonly_v1(s,&order.owner_id)?;
        if let Some(owner)=&owner{s.owner_work(1,owner.player_id.len()+order.quote.owner_player_id.len()+owner.source_id.len()+order.quote.owner_source_id.len())?;}
        Ok(order_owner_eligibility_from_facts_v1(owner.as_ref().map(|o|ResolvedProductionOwnerFactsV1{player_id:&o.player_id,source_id:&o.source_id,public_key:o.public_key.as_deref()}),order))
    })
}
pub fn refund_source_is_current_readonly_v1<S:ManufacturingQuoteReadonlyFactsV1>(s:&S,order:&AgentIndustrialOrderV1)->Result<bool,S::Error>{
    let blocked=s.quote_with_policy(|policy|{for actor in &policy.blocked_agents{s.owner_work(1,actor.len()+order.owner_id.len())?;if actor==&order.owner_id{return Ok(true);}}Ok(false)})?;
    if blocked{return Ok(false);}
    let owner=resolve_industrial_owner_readonly_v1(s,&order.owner_id)?;
    let Some(owner)=owner else{return Ok(false)};
    s.owner_work(1,owner.player_id.len()+order.quote.owner_player_id.len()+owner.source_id.len()+order.quote.owner_source_id.len())?;
    Ok(owner.player_id==order.quote.owner_player_id&&owner.source_id==order.quote.owner_source_id&&owner.public_key.as_ref().is_some_and(|key|!key.is_empty()))
}
/// Sole WorldState blocker composition, including its original lazy refund
/// source read and production-before-capacity refusal order.
pub fn industrial_order_blocker_readonly_v1<S:ManufacturingQuoteReadonlyFactsV1>(s:&S,order:&AgentIndustrialOrderV1,now:WorldTime)->Result<Option<String>,S::Error>{
    let mut charge=|steps,bytes,heap|{s.owner_work(steps,bytes)?;if heap==0{Ok(())}else{s.owner_before_owned(heap)}};
    let validation=order.validate_with_work_v1(&mut charge)?;
    let refund_current=if validation.is_ok()&&order.refund_pending{Some(refund_source_is_current_readonly_v1(s,order)?)}else{None};
    if let Some(blocker)=order.delivery_prefix_after_validation_with_work_v1(now,validation,refund_current,&mut charge)?{return Ok(Some(blocker));}
    if let Err(error)=order_production_eligibility_readonly_v1(s,order)?{return Ok(Some(super::readonly_format::format(format_args!("restore_production_authority:{error:?}"),&mut charge)?));}
    if let Err(error)=capacity(s,&order.owner_id,Some(&order.target_agent_id),false)?{return Ok(Some(super::readonly_format::format(format_args!("restore_acquisition_capacity:{error:?}"),&mut charge)?));}Ok(None)
}
/// Complete original quote. The outer Err is a source/budget/cancellation
/// failure; the inner Err is the original lazy business refusal.
pub fn agent_manufacturing_quote_readonly_v1<S:ManufacturingQuoteReadonlyFactsV1>(s:&S,owner:&str,factory_id:&str,quoted_at:WorldTime,world_binding:Option<&str>)->Result<Result<AgentManufacturingQuoteV1,WorldError>,S::Error>{
    s.quote_with_factory(factory_id,|factory|{let Some(f)=factory else{return Ok(Err(invalid("factory_not_found")))};
        let Some(location)=f.site_location_id.as_deref() else{return Ok(Err(invalid("factory_location_unavailable")))};let location=own(s,location)?;
        domain!(production(s,owner,factory_id,&location)?);domain!(capacity(s,owner,None,true)?);
        let mut pending=0usize;s.owner_visit_orders(|order|{s.owner_work(1,order.owner_id.len()+owner.len())?;if order.owner_id==owner&&!order.is_terminal(){pending+=1;}Ok(ControlFlow::Continue(()))})?;
        if pending>=2{return Ok(Err(invalid("manufacturing_order_capacity")));}
        let mut occupied=false;s.owner_visit_orders(|order|{s.owner_work(1,order.factory_id.len()+factory_id.len())?;if order.factory_id==factory_id&&!order.is_terminal(){occupied=true;return Ok(ControlFlow::Break(()));}Ok(ControlFlow::Continue(()))})?;
        if occupied{return Ok(Err(invalid("manufacturing_order_capacity")));}
        let mut jobs=0usize;s.quote_visit_jobs(|job|{s.owner_work(1,job.factory_id.len()+factory_id.len())?;if job.factory_id==factory_id{jobs+=1;}Ok(())})?;let jobs=jobs.max(f.production.active_jobs as usize);
        if jobs+1>usize::from(f.spec.recipe_slots){return Ok(Err(invalid("factory_capacity_reserved")));}
        let mut inputs=BTreeMap::new();for (kind,amount) in [("alloy_plate",8),("motor_mk1",4),("control_chip",4),("sensor_pack",2),("module_rack",1)]{s.owner_work(1,kind.len()+512)?;s.owner_before_owned(kind.len()+512)?;inputs.insert(kind.to_owned(),amount);}
        if !matches!(&f.input_ledger,MaterialLedgerId::Agent(id) if id==owner){return Ok(Err(invalid("unsupported_input_authority_scope")));}
        s.quote_with_ledger(&f.input_ledger,|ledger|{let Some(ledger)=ledger else{return Ok(Err(invalid("input_ledger_missing")))};
            for (kind,amount) in &inputs{s.owner_work(1,(kind.len()+1).saturating_mul((ledger.len()+1).ilog2() as usize+2))?;if ledger.get(kind).copied().unwrap_or(0)<*amount{s.owner_before_owned(kind.len()+256)?;return Ok(Err(invalid(format!("insufficient_material:{kind}"))));}}
            {let electricity=s.quote_agent_electricity(owner)?.expect("production eligibility authenticated the existing agent");
                if electricity<AGENT_MANUFACTURING_POWER{return Ok(Err(invalid("insufficient_manufacturing_electricity")));}
                let targets=domain!(existing(s,owner)?);let scale=targets.len()+pending;let reputation=s.quote_reputation(owner)?;
                let cost=domain!(crate::agent_claim_quote(reputation.unwrap_or(0),scale).map_err(invalid));
                if cost.slot_index<2{return Ok(Err(invalid("additional_slot_required")));}
                let upfront=cost.activation_fee_amount+cost.claim_bond_amount+cost.upkeep_per_epoch;
                s.quote_with_account(owner,|account|{let default=MainTokenAccountBalance::default();let account=account.unwrap_or(&default);
                    let Some(liquid_after)=account.liquid_balance.checked_sub(upfront) else{return Ok(Err(invalid("insufficient_liquid_claim_reservation")))};
                    s.quote_with_site(&f.site_id,|site|{let site=site.expect("production eligibility authenticated the site");s.quote_with_anchor(&location,|anchor|{let anchor=anchor.expect("production eligibility authenticated the anchor");s.owner_with_starter(owner,|starter|{let starter=starter.expect("production eligibility authenticated the starter claim");
                        let targets=domain!(existing(s,owner)?);let reserved=reserved(s,owner)?;
                        let factory_dependencies=(&f.factory_id,&f.site_id,&f.builder_agent_id,&f.spec,&f.input_ledger,&f.output_ledger,f.site_authority_revision,&f.site_location_id,f.location_anchor_revision,&f.construction_power_profile_key,f.construction_power_profile_revision,f.built_at,jobs);
                        let dependency_digest=domain!(hash(s,&(factory_dependencies,site,anchor,ledger,electricity,account,starter,reputation,targets,reserved))?);
                        let world_binding=world_binding.map(|b|own(s,b)).transpose()?;let recipe_id=own(s,"recipe.assembler.agent_mk1")?;let owner_id=own(s,owner)?;let owner_player_id=own(s,&starter.player_id)?;
                        let Some(resolved)=resolve_industrial_owner_readonly_v1(s,owner)? else{return Ok(Err(invalid("manufacturing_root_owner_unavailable")))};let ResolvedAgentOwnerV1{source_id:owner_source_id,..}=resolved;
                        let factory_id=own(s,factory_id)?;
                        let Some(expires_at)=quoted_at.checked_add(5) else{return Ok(Err(invalid("time_overflow")))};
                        let input_ledger=match &f.input_ledger{MaterialLedgerId::Agent(v)=>MaterialLedgerId::Agent(own(s,v)?),_=>unreachable!("input authority checked above")};
                        s.owner_before_owned(size_of::<AgentManufacturingQuoteV1>())?;
                        let mut q=AgentManufacturingQuoteV1{policy_version:1,world_binding,recipe_id,owner_id,owner_player_id,owner_source_id,factory_id,quoted_at,expires_at,quote_digest:String::new(),dependency_digest,delivery_location_id:own(s,&location)?,inputs,input_ledger,electricity:AGENT_MANUFACTURING_POWER,claim_cost:cost,upfront_liquid:upfront,liquid_after_reservation:liquid_after,electricity_after_reservation:electricity-AGENT_MANUFACTURING_POWER,maintenance_per_tick:f.spec.maintenance_per_tick};
                        q.quote_digest=domain!(hash(s,&q)?);Ok(Ok(q))
                    })})})
                })
            }
        })
    })
}
impl ManufacturingQuoteReadonlyFactsV1 for WorldState {
    fn quote_with_factory<T>(&self,id:&str,visit:impl FnOnce(Option<&FactoryState>)->Result<T,Infallible>)->Result<T,Infallible>{visit(self.factories.get(id))}
    fn quote_with_site<T>(&self,id:&str,visit:impl FnOnce(Option<&FactorySiteAuthorityV1>)->Result<T,Infallible>)->Result<T,Infallible>{visit(self.factory_site_authorities.get(id))}
    fn quote_with_anchor<T>(&self,id:&str,visit:impl FnOnce(Option<&LocationAnchorV1>)->Result<T,Infallible>)->Result<T,Infallible>{visit(self.location_anchors.get(id))}
    fn quote_with_policy<T>(&self,visit:impl FnOnce(&GameplayPolicyState)->Result<T,Infallible>)->Result<T,Infallible>{visit(&self.gameplay_policy)}
    fn quote_factory_retired(&self,id:&str)->Result<bool,Infallible>{Ok(self.retired_factory_ids.contains(id))}
    fn quote_visit_claims(&self,mut visit:impl FnMut(&AgentClaimState)->Result<(),Infallible>)->Result<(),Infallible>{for claim in self.agent_claims.values(){visit(claim)?;}Ok(())}
    fn quote_visit_jobs(&self,mut visit:impl FnMut(&RecipeJobState)->Result<(),Infallible>)->Result<(),Infallible>{for job in self.pending_recipe_jobs.values(){visit(job)?;}Ok(())}
    fn quote_with_ledger<T>(&self,id:&MaterialLedgerId,visit:impl FnOnce(Option<&BTreeMap<String,i64>>)->Result<T,Infallible>)->Result<T,Infallible>{visit(self.material_ledgers.get(id))}
    fn quote_agent_electricity(&self,id:&str)->Result<Option<i64>,Infallible>{Ok(self.agents.get(id).map(|agent|agent.state.resources.get(ResourceKind::Electricity)))}
    fn quote_with_account<T>(&self,id:&str,visit:impl FnOnce(Option<&MainTokenAccountBalance>)->Result<T,Infallible>)->Result<T,Infallible>{visit(self.main_token_balances.get(id))}
    fn quote_reputation(&self,id:&str)->Result<Option<i64>,Infallible>{Ok(self.reputation_scores.get(id).copied())}
}
