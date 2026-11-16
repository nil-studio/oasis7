//! Sole original artifact-market action preflight. Implementations are sealed
//! actual observations; the returned event is DATA and cannot publish itself.
use super::*;
use oasis7_agent_api::world_action::ResourceKind;
use oasis7_world_state::{ModuleArtifactListingState,ModuleArtifactBidState};
pub(super) mod sealed{pub trait Sealed{}}
pub(in crate::world) trait ModuleMarketFactsV1:sealed::Sealed{
 type Error:From<crate::WorldError>;
 fn reserve(&mut self,action:&crate::Action)->Result<(),Self::Error>;
 fn agent_exists(&mut self,id:&str)->Result<bool,Self::Error>;
 fn artifact_exists(&mut self,hash:&str)->Result<bool,Self::Error>;
 fn owner(&mut self,hash:&str)->Result<Option<String>,Self::Error>;
 fn listing(&mut self,hash:&str)->Result<Option<ModuleArtifactListingState>,Self::Error>;
 fn bid_exists(&mut self,hash:&str,id:u64,bidder:&str)->Result<Option<bool>,Self::Error>;
 fn active_use(&mut self,hash:&str)->Result<bool,Self::Error>;
 fn balance(&mut self,agent:&str,kind:ResourceKind)->Result<i64,Self::Error>;
 fn next_order(&mut self)->Result<u64,Self::Error>;
 fn next_sale(&mut self)->Result<u64,Self::Error>;
}
pub(in crate::world) enum ModuleMarketDecisionV1{Rejected(crate::RejectReason),Event(crate::DomainEvent)}
fn denied(note:String)->ModuleMarketDecisionV1{ModuleMarketDecisionV1::Rejected(crate::RejectReason::RuleDenied{notes:vec![note]})}
fn missing(agent:&str)->ModuleMarketDecisionV1{ModuleMarketDecisionV1::Rejected(crate::RejectReason::AgentNotFound{agent_id:agent.into()})}
fn affordable<S:ModuleMarketFactsV1>(source:&mut S,agent:&str,kind:ResourceKind,amount:i64)->Result<Option<ModuleMarketDecisionV1>,S::Error>{
 let available=source.balance(agent,kind)?;
 Ok((available<amount).then(||ModuleMarketDecisionV1::Rejected(crate::RejectReason::InsufficientResource{agent_id:agent.into(),kind,requested:amount,available})))
}
pub(in crate::world) fn is_market_action_v1(action:&crate::Action)->bool{matches!(action,crate::Action::ListModuleArtifactForSale{..}|crate::Action::BuyModuleArtifact{..}|crate::Action::DelistModuleArtifact{..}|crate::Action::DestroyModuleArtifact{..}|crate::Action::PlaceModuleArtifactBid{..}|crate::Action::CancelModuleArtifactBid{..})}
pub(in crate::world) fn evaluate_market_action_v1<S:ModuleMarketFactsV1>(source:&mut S,action:&crate::Action)->Result<ModuleMarketDecisionV1,S::Error>{
 use crate::{Action,DomainEvent,RejectReason};use crate::world::module_actions::{MODULE_LIST_FEE_AMOUNT,MODULE_DELIST_FEE_AMOUNT,MODULE_DESTROY_FEE_AMOUNT};
 source.reserve(action)?;
 Ok(match action{
  Action::ListModuleArtifactForSale{seller_agent_id,wasm_hash,price_kind,price_amount}=>{
   if !source.agent_exists(seller_agent_id)?{return Ok(missing(seller_agent_id))}if *price_amount<=0{return Ok(ModuleMarketDecisionV1::Rejected(RejectReason::InvalidAmount{amount:*price_amount}))}
   if !source.artifact_exists(wasm_hash)?{return Ok(denied(format!("list module artifact rejected: missing artifact {}",wasm_hash)))}
   let Some(owner)=source.owner(wasm_hash)?else{return Ok(denied(format!("list module artifact rejected: owner missing for {}",wasm_hash)))};
   if owner!=*seller_agent_id{return Ok(denied(format!("list module artifact rejected: seller {} does not own {} (owner {})",seller_agent_id,wasm_hash,owner)))}
   if let Some(rejected)=affordable(source,seller_agent_id,ResourceKind::Data,MODULE_LIST_FEE_AMOUNT)?{return Ok(rejected)}
   ModuleMarketDecisionV1::Event(DomainEvent::ModuleArtifactListed{seller_agent_id:seller_agent_id.clone(),wasm_hash:wasm_hash.clone(),price_kind:*price_kind,price_amount:*price_amount,order_id:source.next_order()?,fee_kind:ResourceKind::Data,fee_amount:MODULE_LIST_FEE_AMOUNT})
  }
  Action::BuyModuleArtifact{buyer_agent_id,wasm_hash}=>{
   if !source.agent_exists(buyer_agent_id)?{return Ok(missing(buyer_agent_id))}
   let Some(listing)=source.listing(wasm_hash)?else{return Ok(denied(format!("buy module artifact rejected: listing missing for {}",wasm_hash)))};
   if listing.seller_agent_id==*buyer_agent_id{return Ok(denied(format!("buy module artifact rejected: buyer {} already owns listing {}",buyer_agent_id,wasm_hash)))}
   if !source.agent_exists(&listing.seller_agent_id)?{return Ok(missing(&listing.seller_agent_id))}
   if let Some(rejected)=affordable(source,buyer_agent_id,listing.price_kind,listing.price_amount)?{return Ok(rejected)}
   ModuleMarketDecisionV1::Event(DomainEvent::ModuleArtifactSaleCompleted{buyer_agent_id:buyer_agent_id.clone(),seller_agent_id:listing.seller_agent_id,wasm_hash:wasm_hash.clone(),price_kind:listing.price_kind,price_amount:listing.price_amount,sale_id:source.next_sale()?,listing_order_id:(listing.order_id>0).then_some(listing.order_id),bid_order_id:None})
  }
  Action::DelistModuleArtifact{seller_agent_id,wasm_hash}=>{
   if !source.agent_exists(seller_agent_id)?{return Ok(missing(seller_agent_id))}
   let Some(listing)=source.listing(wasm_hash)?else{return Ok(denied(format!("delist module artifact rejected: listing missing for {}",wasm_hash)))};
   if listing.seller_agent_id!=*seller_agent_id{return Ok(denied(format!("delist module artifact rejected: seller {} does not own listing {}",seller_agent_id,wasm_hash)))}
   let Some(owner)=source.owner(wasm_hash)?else{return Ok(denied(format!("delist module artifact rejected: owner missing for {}",wasm_hash)))};
   if owner!=*seller_agent_id{return Ok(denied(format!("delist module artifact rejected: seller {} does not own {} (owner {})",seller_agent_id,wasm_hash,owner)))}
   if let Some(rejected)=affordable(source,seller_agent_id,ResourceKind::Data,MODULE_DELIST_FEE_AMOUNT)?{return Ok(rejected)}
   ModuleMarketDecisionV1::Event(DomainEvent::ModuleArtifactDelisted{seller_agent_id:seller_agent_id.clone(),wasm_hash:wasm_hash.clone(),order_id:(listing.order_id>0).then_some(listing.order_id),fee_kind:ResourceKind::Data,fee_amount:MODULE_DELIST_FEE_AMOUNT})
  }
  Action::PlaceModuleArtifactBid{bidder_agent_id,wasm_hash,price_kind,price_amount}=>{
   if !source.agent_exists(bidder_agent_id)?{return Ok(missing(bidder_agent_id))}if *price_amount<=0{return Ok(ModuleMarketDecisionV1::Rejected(RejectReason::InvalidAmount{amount:*price_amount}))}
   if !source.artifact_exists(wasm_hash)?{return Ok(denied(format!("place module artifact bid rejected: missing artifact {}",wasm_hash)))}
   let Some(owner)=source.owner(wasm_hash)?else{return Ok(denied(format!("place module artifact bid rejected: owner missing for {}",wasm_hash)))};
   if owner==*bidder_agent_id{return Ok(denied(format!("place module artifact bid rejected: bidder {} already owns {}",bidder_agent_id,wasm_hash)))}
   if let Some(rejected)=affordable(source,bidder_agent_id,*price_kind,*price_amount)?{return Ok(rejected)}
   ModuleMarketDecisionV1::Event(DomainEvent::ModuleArtifactBidPlaced{bidder_agent_id:bidder_agent_id.clone(),wasm_hash:wasm_hash.clone(),order_id:source.next_order()?,price_kind:*price_kind,price_amount:*price_amount})
  }
  Action::CancelModuleArtifactBid{bidder_agent_id,wasm_hash,bid_order_id}=>{
   if !source.agent_exists(bidder_agent_id)?{return Ok(missing(bidder_agent_id))}
   match source.bid_exists(wasm_hash,*bid_order_id,bidder_agent_id)?{None=>return Ok(denied(format!("cancel module artifact bid rejected: no bids for {}",wasm_hash))),Some(false)=>return Ok(denied(format!("cancel module artifact bid rejected: bid {} not found for {}",bid_order_id,bidder_agent_id))),Some(true)=>{}}
   ModuleMarketDecisionV1::Event(DomainEvent::ModuleArtifactBidCancelled{bidder_agent_id:bidder_agent_id.clone(),wasm_hash:wasm_hash.clone(),order_id:*bid_order_id,reason:"cancelled_by_bidder".into()})
  }
  Action::DestroyModuleArtifact{owner_agent_id,wasm_hash,reason}=>{
   if !source.agent_exists(owner_agent_id)?{return Ok(missing(owner_agent_id))}if reason.trim().is_empty(){return Ok(denied("destroy module artifact rejected: reason is empty".into()))}
   if !source.artifact_exists(wasm_hash)?{return Ok(denied(format!("destroy module artifact rejected: missing artifact {}",wasm_hash)))}
   let Some(owner)=source.owner(wasm_hash)?else{return Ok(denied(format!("destroy module artifact rejected: owner missing for {}",wasm_hash)))};
   if owner!=*owner_agent_id{return Ok(denied(format!("destroy module artifact rejected: owner {} does not own {} (owner {})",owner_agent_id,wasm_hash,owner)))}
   if source.active_use(wasm_hash)?{return Ok(denied(format!("destroy module artifact rejected: artifact {} is used by active module",wasm_hash)))}
   if let Some(rejected)=affordable(source,owner_agent_id,ResourceKind::Electricity,MODULE_DESTROY_FEE_AMOUNT)?{return Ok(rejected)}
   ModuleMarketDecisionV1::Event(DomainEvent::ModuleArtifactDestroyed{owner_agent_id:owner_agent_id.clone(),wasm_hash:wasm_hash.clone(),reason:reason.clone(),fee_kind:ResourceKind::Electricity,fee_amount:MODULE_DESTROY_FEE_AMOUNT})
  }
  _=>return Err(crate::WorldError::ModuleChangeInvalid{reason:"not a module marketplace action".into()}.into()),
 })
}

/// Legacy wrapper uses the exact old World fields. It exists only to make the
/// original public Action path delegate to the same fixed business predicates.
pub(in crate::world) struct LegacyModuleMarketFactsV1<'a>(pub(in crate::world) &'a crate::world::World);
impl sealed::Sealed for LegacyModuleMarketFactsV1<'_>{}
impl ModuleMarketFactsV1 for LegacyModuleMarketFactsV1<'_>{
 type Error=crate::WorldError;
 fn reserve(&mut self,_:&crate::Action)->Result<(),Self::Error>{Ok(())}
 fn agent_exists(&mut self,id:&str)->Result<bool,Self::Error>{Ok(self.0.state.agents.contains_key(id))}
 fn artifact_exists(&mut self,hash:&str)->Result<bool,Self::Error>{Ok(self.0.module_artifacts.contains(hash))}
 fn owner(&mut self,hash:&str)->Result<Option<String>,Self::Error>{Ok(self.0.state.module_artifact_owners.get(hash).cloned())}
 fn listing(&mut self,hash:&str)->Result<Option<ModuleArtifactListingState>,Self::Error>{Ok(self.0.state.module_artifact_listings.get(hash).cloned())}
 fn bid_exists(&mut self,hash:&str,id:u64,bidder:&str)->Result<Option<bool>,Self::Error>{Ok(self.0.state.module_artifact_bids.get(hash).map(|v|v.iter().any(|b|b.order_id==id&&b.bidder_agent_id==bidder)))}
 fn active_use(&mut self,hash:&str)->Result<bool,Self::Error>{Ok(self.0.has_active_module_using_artifact(hash))}
 fn balance(&mut self,id:&str,kind:ResourceKind)->Result<i64,Self::Error>{Ok(self.0.state.agents.get(id).map(|c|c.state.resources.get(kind)).unwrap_or(0))}
 fn next_order(&mut self)->Result<u64,Self::Error>{Ok(self.0.state.next_module_market_order_id.max(1))}
 fn next_sale(&mut self)->Result<u64,Self::Error>{Ok(self.0.state.next_module_market_sale_id.max(1))}
}
