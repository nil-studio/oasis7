//! Work-only extension of the original manufacturing order validator.
use super::{AgentIndustrialOrderV1,AgentManufacturingQuoteV1,AgentIndustrialPhaseV1 as P,AGENT_MANUFACTURING_POWER,invalid};
use crate::{WorldError,MaterialLedgerId};
use std::{collections::BTreeMap,mem::size_of};
fn text<E>(v:&str,work:&mut impl FnMut(u64,usize,usize)->Result<(),E>)->Result<String,E>{work(1,v.len(),v.len()+size_of::<String>())?;Ok(v.to_owned())}
fn ledger<E>(v:&MaterialLedgerId,work:&mut impl FnMut(u64,usize,usize)->Result<(),E>)->Result<MaterialLedgerId,E>{work(1,0,0)?;Ok(match v{MaterialLedgerId::World=>MaterialLedgerId::World,MaterialLedgerId::Agent(v)=>MaterialLedgerId::Agent(text(v,work)?),MaterialLedgerId::Site(v)=>MaterialLedgerId::Site(text(v,work)?),MaterialLedgerId::Factory(v)=>MaterialLedgerId::Factory(text(v,work)?)})}
fn clone_quote<E>(q:&AgentManufacturingQuoteV1,work:&mut impl FnMut(u64,usize,usize)->Result<(),E>)->Result<AgentManufacturingQuoteV1,E>{
    work(1,size_of::<AgentManufacturingQuoteV1>(),size_of::<AgentManufacturingQuoteV1>())?;
    let world_binding=q.world_binding.as_deref().map(|v|text(v,work)).transpose()?;
    let recipe_id=text(&q.recipe_id,work)?;let owner_id=text(&q.owner_id,work)?;let owner_player_id=text(&q.owner_player_id,work)?;let owner_source_id=text(&q.owner_source_id,work)?;let factory_id=text(&q.factory_id,work)?;
    let quote_digest=text(&q.quote_digest,work)?;let dependency_digest=text(&q.dependency_digest,work)?;let delivery_location_id=text(&q.delivery_location_id,work)?;
    let mut inputs=BTreeMap::new();for (key,value) in &q.inputs{let levels=(inputs.len()+1usize).ilog2() as usize+2;work(1,(key.len()+1).saturating_mul(levels*2+4)+512*levels,key.len()+size_of::<String>()+512*levels)?;inputs.insert(key.to_owned(),*value);}
    let input_ledger=ledger(&q.input_ledger,work)?;work(1,size_of::<crate::AgentClaimCostQuote>(),0)?;let claim_cost=q.claim_cost.clone();
    Ok(AgentManufacturingQuoteV1{policy_version:q.policy_version,world_binding,recipe_id,owner_id,owner_player_id,owner_source_id,factory_id,quoted_at:q.quoted_at,expires_at:q.expires_at,quote_digest,dependency_digest,delivery_location_id,inputs,input_ledger,electricity:q.electricity,claim_cost,upfront_liquid:q.upfront_liquid,liquid_after_reservation:q.liquid_after_reservation,electricity_after_reservation:q.electricity_after_reservation,maintenance_per_tick:q.maintenance_per_tick})
}
fn inputs_equal<E>(left:&BTreeMap<String,i64>,right:&BTreeMap<String,i64>,work:&mut impl FnMut(u64,usize,usize)->Result<(),E>)->Result<bool,E>{
    work(1,0,0)?;if left.len()!=right.len(){return Ok(false);}for ((a,av),(b,bv)) in left.iter().zip(right){work(1,a.len()+b.len()+2*size_of::<i64>(),0)?;if a!=b||av!=bv{return Ok(false);}}Ok(true)
}
fn hash<E,T:serde::Serialize>(value:&T,work:&mut impl FnMut(u64,usize,usize)->Result<(),E>)->Result<Result<String,WorldError>,E>{
    work(1,0,64+size_of::<String>())?;
    match crate::util::hash_json_with_work_v1(value,&mut |steps,bytes|work(steps,usize::try_from(bytes).unwrap_or(usize::MAX),0)){Ok(value)=>Ok(Ok(value)),Err(crate::util::JsonHashWorkErrorV1::Codec(error))=>Ok(Err(error.into())),Err(crate::util::JsonHashWorkErrorV1::Work(error))=>Err(error)}
}
fn refusal<E>(reason:&str,work:&mut impl FnMut(u64,usize,usize)->Result<(),E>)->Result<Result<(),WorldError>,E>{work(1,reason.len(),reason.len()+size_of::<WorldError>())?;Ok(Err(invalid(reason)))}
pub(super) fn validate<E>(order:&AgentIndustrialOrderV1,work:&mut impl FnMut(u64,usize,usize)->Result<(),E>)->Result<Result<(),WorldError>,E>{
    let mut quote=clone_quote(&order.quote,work)?;let digest=std::mem::take(&mut quote.quote_digest);
    work(1,order.owner_id.len()+quote.owner_id.len()+order.factory_id.len()+quote.factory_id.len()+quote.recipe_id.len()+"recipe.assembler.agent_mk1".len(),0)?;
    if order.policy_version!=1||quote.policy_version!=1||order.owner_id!=quote.owner_id||order.factory_id!=quote.factory_id||!inputs_equal(&order.inputs,&quote.inputs,work)?||quote.electricity!=AGENT_MANUFACTURING_POWER||quote.claim_cost.slot_index<2||quote.recipe_id!="recipe.assembler.agent_mk1"{return refusal("manufacturing_recovery_incomplete",work);}
    let quote_hash=match hash(&quote,work)?{Ok(value)=>value,Err(error)=>return Ok(Err(error))};work(1,quote_hash.len()+digest.len(),0)?;if quote_hash!=digest{return refusal("manufacturing_recovery_incomplete",work);}
    let id_hash=match hash(&order.order_id,work)?{Ok(value)=>value,Err(error)=>return Ok(Err(error))};let n="industrial:".len()+id_hash.len();work(1,n+order.target_agent_id.len(),n+size_of::<String>())?;
    if order.target_agent_id!=format!("industrial:{id_hash}"){return refusal("manufacturing_recovery_incomplete",work);}
    work(1,0,0)?;
    let phase_valid=match order.phase{
        P::Reserved=>order.started_at.is_none()&&order.completed_work_ticks==0&&order.produced_at.is_none()&&order.delivered_at.is_none(),
        P::Producing|P::ProductionBlocked=>order.started_at.is_some()&&order.completed_work_ticks<12&&order.produced_at.is_none()&&order.delivered_at.is_none(),
        P::AwaitingDelivery=>order.started_at.is_some()&&order.completed_work_ticks==12&&order.produced_at.is_some()&&order.delivered_at.is_none(),
        P::Delivered=>order.started_at.is_some()&&order.completed_work_ticks==12&&order.produced_at.is_some()&&order.delivered_at.is_some(),
        P::Cancelled=>order.started_at.is_none()&&order.completed_work_ticks==0,P::Disposed=>order.started_at.is_some(),
    };
    let refundable_terminal=matches!(order.phase,P::Cancelled|P::Disposed);
    if order.refund_pending&&(!refundable_terminal||order.refunded_at.is_some())||order.refunded_at.is_some_and(|at|!refundable_terminal||at<order.confirmed_at){return refusal("manufacturing_refund_recovery_incomplete",work);}
    if !phase_valid{return refusal("manufacturing_recovery_incomplete",work);}Ok(Ok(()))
}
pub(super) fn prefix<E>(order:&AgentIndustrialOrderV1,now:crate::WorldTime,validation:Result<(),WorldError>,refund_source_current:Option<bool>,work:&mut impl FnMut(u64,usize,usize)->Result<(),E>)->Result<Option<String>,E>{
    work(1,0,0)?;
    use super::readonly_format::format as fmt;
    if let Err(error)=validation{return Ok(Some(fmt(format_args!("{error:?}"),work)?));}
    if order.refund_pending{return Ok(Some(fmt(format_args!("{}",if refund_source_current.unwrap_or(false){"refund_pending_authoritative_release"}else{"restore_original_owner_source_for_frozen_refund"}),work)?));}
    if order.is_terminal(){return Ok(Some(fmt(format_args!("order_terminal:{:?}",order.phase),work)?));}
    if order.phase!=P::AwaitingDelivery{return Ok(Some(fmt(format_args!("production_phase:{:?}",order.phase),work)?));}
    let Some(produced_at)=order.produced_at else{return Ok(Some(fmt(format_args!("manufacturing_state_inconsistent"),work)?))};
    if order.completed_work_ticks!=super::AGENT_MANUFACTURING_WORK_TICKS||order.started_at.is_none(){return Ok(Some(fmt(format_args!("manufacturing_state_inconsistent"),work)?));}
    if now>=produced_at.saturating_add(24){return Ok(Some(fmt(format_args!("delivery_expired"),work)?));}Ok(None)
}
