//! Full selected Current contract through the original qualified scalar codecs.
use super::*;
use crate::world::incremental_normal_gameplay_cycles::{read_current_gameplay_scalar_v1 as scalar,read_current_diplomacy_text_with_limit_v1 as text};
use oasis7_world_state::{incremental_diplomacy::*,incremental_field_overlay::WorldFieldOverlayContextV1};
#[allow(clippy::too_many_arguments)]
pub(super) fn read_contract_v1<M,R,E,C>(id:&str,proof:&ReservedValue<DiplomacyRecordQualificationV1>,state:&M,
ctx:&mut WorldFieldOverlayContextV1<'_,'_,'_,M,R,E,C>,max:usize,tail:&mut crate::world::incremental_normal_tail::NormalTailWorkV1<'_,'_>)
->Result<ReservedValue<oasis7_world_state::EconomicContractState>,crate::world::incremental_normal_gameplay_cycles::GameplayCycleErrorV1>
where M:MerkleNodeReader,R:oasis7_formal_protocol::incremental_value::ValueNodeReader,
E:FnMut()->Option<oasis7_formal_protocol::incremental_value::ValueTreeError>,C:FnMut()->bool {
 let bad=||oasis7_formal_protocol::incremental_value::ValueTreeError::Invalid("Current organization contract field");
 let field=DiplomacyFieldV1::EconomicContracts;
 let fee=tail.reserve(std::mem::size_of::<oasis7_world_state::EconomicContractState>()+max.checked_mul(8).ok_or_else(bad)?,MemoryClass::Ordinary)?;
 macro_rules! text {($col:ident)=>{text(id,field,DiplomacyScalarV1::Contract(ContractScalarV1::$col),proof,state,ctx,max,tail)?};}
 macro_rules! required {($col:ident)=>{{let s=text!($col).ok_or_else(bad)?;tail.charge(s.len())?;s.as_str().to_owned()}};}
 macro_rules! signed {($col:ident)=>{match scalar(id,field,DiplomacyScalarV1::Contract(ContractScalarV1::$col),proof,state,ctx,tail)? {Some(DiplomacyValueV1::Signed(v))=>v,_=>return Err(bad().into())}};}
 macro_rules! time {($col:ident)=>{match scalar(id,field,DiplomacyScalarV1::Contract(ContractScalarV1::$col),proof,state,ctx,tail)? {Some(DiplomacyValueV1::Unsigned(v))=>Some(v),None=>None,_=>return Err(bad().into())}};}
 let contract_id=required!(ContractId);let creator_agent_id=required!(CreatorAgentId);let counterparty_agent_id=required!(CounterpartyAgentId);
 let fulfillment_kind=match required!(FulfillmentKind).as_str(){"atomic_exchange"=>oasis7_agent_api::world_action::EconomicContractFulfillmentKind::AtomicExchange,"service"=>oasis7_agent_api::world_action::EconomicContractFulfillmentKind::Service,_=>return Err(bad().into())};
 let settlement_kind=match required!(SettlementKind).as_str(){"electricity"=>oasis7_agent_api::world_action::ResourceKind::Electricity,"data"=>oasis7_agent_api::world_action::ResourceKind::Data,_=>return Err(bad().into())};
 let settlement_amount=signed!(SettlementAmount);let reputation_stake=signed!(ReputationStake);let expires_at=time!(ExpiresAt).ok_or_else(bad)?;
 let description=required!(Description);
 let status=match required!(Status).as_str(){"open"=>crate::EconomicContractStatus::Open,"accepted"=>crate::EconomicContractStatus::Accepted,"settled"=>crate::EconomicContractStatus::Settled,"expired"=>crate::EconomicContractStatus::Expired,_=>return Err(bad().into())};
 let accepted_at=time!(AcceptedAt);let settled_at=time!(SettledAt);
 let settlement_success=match scalar(id,field,DiplomacyScalarV1::Contract(ContractScalarV1::SettlementSuccess),proof,state,ctx,tail)?{Some(DiplomacyValueV1::Bool(v))=>Some(v),None=>None,_=>return Err(bad().into())};
 let transfer_amount=signed!(TransferAmount);let tax_amount=signed!(TaxAmount);let settlement_notes=if let Some(s)=text!(SettlementNotes){tail.charge(s.len())?;Some(s.as_str().to_owned())}else{None};
 Ok(fee.retain(oasis7_world_state::EconomicContractState{contract_id,creator_agent_id,counterparty_agent_id,fulfillment_kind,settlement_kind,settlement_amount,reputation_stake,expires_at,description,status,accepted_at,settled_at,settlement_success,transfer_amount,tax_amount,settlement_notes}))
}
