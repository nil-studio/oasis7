//! Original selected-order refund arithmetic, shared by legacy and Current.
//! These functions only mutate supplied DATA. Source identity/revocation,
//! Current CAS, paid copies and atomic installation remain with the owner.
use super::{AgentIndustrialOrderV1,AgentIndustrialPhaseV1 as P,invalid};
use crate::{WorldError,WorldTime,MainTokenAccountBalance};
use oasis7_agent_api::{world_resource::ResourceStock,world_action::ResourceKind};
use std::collections::BTreeMap;
pub fn mark_industrial_order_terminated_v1(order:&mut AgentIndustrialOrderV1,reason:&str){
 order.phase=if order.phase==P::Reserved{P::Cancelled}else{P::Disposed};order.terminal_reason=Some(reason.into());order.refund_pending=true;
}
pub fn validate_industrial_refund_phase_v1(order:&AgentIndustrialOrderV1)->Result<(),WorldError>{
 if !order.refund_pending||!matches!(order.phase,P::Cancelled|P::Disposed)||order.refunded_at.is_some(){return Err(invalid("manufacturing_refund_already_released_or_invalid"));}Ok(())
}
pub fn add_industrial_refund_materials_v1(order:&AgentIndustrialOrderV1,refunded:&mut BTreeMap<String,i64>)->Result<(),WorldError>{
 for(kind,amount)in &order.inputs{let value=refunded.entry(kind.clone()).or_default();*value=value.checked_add(*amount).ok_or_else(||invalid("refund_overflow"))?;}Ok(())
}
pub fn add_industrial_refund_electricity_v1(order:&AgentIndustrialOrderV1,resources:&mut ResourceStock)->Result<(),WorldError>{
 resources.add(ResourceKind::Electricity,order.quote.electricity).map_err(|error|invalid(format!("electricity_refund:{error:?}")))?;Ok(())
}
pub fn add_industrial_refund_account_v1(order:&AgentIndustrialOrderV1,account:&mut MainTokenAccountBalance)->Result<(),WorldError>{
 account.liquid_balance=account.liquid_balance.checked_add(order.quote.upfront_liquid).ok_or_else(||invalid("refund_overflow"))?;Ok(())
}
pub fn finish_industrial_refund_v1(order:&mut AgentIndustrialOrderV1,now:WorldTime){order.refund_pending=false;order.refunded_at=Some(now);}
