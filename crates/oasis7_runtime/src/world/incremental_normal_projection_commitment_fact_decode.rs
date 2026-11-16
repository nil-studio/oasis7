//! Single-row original JSON DATA recovery for fixed readonly fact kernels.
//! The source is a retained original typed row, never caller JSON or a State.
use super::*;
use std::{cell::RefCell,io::{self,Read},mem::size_of};
use serde::de::DeserializeOwned;
trait ClosedFact:DeserializeOwned{fn largest_header()->usize;}
macro_rules! facts{($($t:ty),+$(,)?)=>{$(impl ClosedFact for $t{fn largest_header()->usize{size_of::<Self>().max(size_of::<String>()).max(size_of::<oasis7_wasm_abi::MaterialStack>()).max(size_of::<oasis7_world_state::MaterialMarketQuote>())}})+};}
facts!(oasis7_agent_api::world_action::SubstitutionDecisionV1,oasis7_agent_api::world_action::ConversionSettlementV1,oasis7_agent_api::world_action::SubstitutionResidualBalanceV1,oasis7_agent_api::world_action::ParentBindingRevisionV1,oasis7_agent_api::world_action::SubstitutionExecutionAuthorityV1,oasis7_world_state::FactoryState,oasis7_world_state::RecipeJobState,oasis7_world_state::IndustryProgressState,oasis7_world_state::GameplayPolicyState,oasis7_world_state::FactorySiteAuthorityV1,
 oasis7_world_state::AgentClaimState,oasis7_world_state::StarterOcClaimState,oasis7_world_state::LocationAnchorV1,oasis7_world_state::MainTokenAccountBalance,
 oasis7_world_state::AllianceState,oasis7_world_state::EconomicContractState,
 oasis7_wasm_abi::ProductProfileV1,oasis7_wasm_abi::RecipeProfileV1,
 oasis7_world_state::world_rules::AgentIndustrialOrderV1,oasis7_world_state::world_rules::OrganizationRegistrationV1,oasis7_world_state::world_rules::OrganizationContinuityCaseV1,
 oasis7_agent_api::world_action::RootOperationV1,oasis7_agent_api::world_action::IndustrialLotV1,oasis7_agent_api::world_action::IndustrialLotAuthorityV1,oasis7_agent_api::world_action::IndustrialReceiptV1,oasis7_agent_api::world_action::DispositionPlanV1,
 oasis7_world_state::CapabilityAgentIdentity,oasis7_agent_api::game::GameParticipantReceiptV1,Option<oasis7_world_state::service_admission::participant::ParticipantEntryPolicyV1>,oasis7_agent_api::game::GameIdentityNamespaceV1,Option<String>,
 oasis7_world_state::LogisticsPathAuthorityV1,Option<oasis7_world_state::FormalRetainedHistoryRootV1>,
 Option<oasis7_world_state::AgentIntentV2>,oasis7_world_state::world_rules::WorldRuleReceiptV1,crate::service_admission::AgentSignerDelegationChangeV1,std::collections::BTreeMap<String,i64>,u64,i64);
struct Input<'a,'b,'c>{bytes:&'a[u8],position:usize,common:&'a RefCell<&'b mut FormalBoundedContextV1<'c>>,first:Option<BodyMutationError>}
impl Read for Input<'_, '_, '_>{fn read(&mut self,out:&mut[u8])->io::Result<usize>{
 if self.first.is_some(){return Err(io::ErrorKind::Other.into())}let remaining=self.bytes.len()-self.position;let n=out.len().min(remaining).min(8192);
 if let Err(error)=self.common.borrow_mut().charge(1,n as u64){self.first=Some(paid(error));return Err(io::ErrorKind::Other.into())}
 out[..n].copy_from_slice(&self.bytes[self.position..self.position+n]);self.position+=n;Ok(n)
}}
#[derive(Debug)]struct PaidFactCodecError{error:serde_json::Error,_fee:MemoryReservation}
impl std::fmt::Display for PaidFactCodecError{fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result{std::fmt::Display::fmt(&self.error,f)}}
impl std::error::Error for PaidFactCodecError{fn source(&self)->Option<&(dyn std::error::Error+'static)>{Some(&self.error)}}
fn owned_parts<T:ClosedFact>(bytes:&[u8],common:&RefCell<&mut FormalBoundedContextV1<'_>>)->Result<(T,MemoryReservation),BodyMutationError>{
 // The closed registry above contains only fixed original structs, strings,
 // vectors and their original maps. Eight header widths per actual wire
 // byte cover collection capacity growth, BTree nodes and decoder scratch;
 // defaults are empty containers/None/scalars and create no hidden graph.
 let n=bytes.len().checked_add(1).and_then(|n|n.checked_mul(T::largest_header())).and_then(|n|n.checked_mul(8)).and_then(|n|n.checked_add(bytes.len().checked_mul(2)?)).and_then(|n|n.checked_add(32768+size_of::<T>())).ok_or_else(||invalid("projection fact graph layout"))?;
 common.borrow_mut().charge(1,n as u64).map_err(paid)?;let phase=common.borrow().memory().reserve(n,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
 let mut input=Input{bytes,position:0,common,first:None};let result=serde_json::from_reader::<_,T>(&mut input);
 if let Some(error)=input.first{return Err(error)}common.borrow_mut().consume_steps(0).map_err(paid)?;
 let value=match result{Ok(value)=>value,Err(error)=>return Err(paid(PaidFactCodecError{error,_fee:phase}))};
 Ok((value,phase))
}
fn owned<T:ClosedFact>(bytes:&[u8],common:&RefCell<&mut FormalBoundedContextV1<'_>>)->Result<ReservedValue<T>,BodyMutationError>{let(value,fee)=owned_parts(bytes,common)?;Ok(fee.retain(value))}

fn recover<T:ClosedFact,R>(bytes:&[u8],common:&RefCell<&mut FormalBoundedContextV1<'_>>,visit:impl FnOnce(&T)->Result<R,BodyMutationError>)->Result<R,BodyMutationError>{
 let value=owned::<T>(bytes,common)?;
 // No Common RefMut survives into nested fixed source queries. Value drops
 // before its retained phase, including when the callback fails or unwinds.
 visit(&value)
}
macro_rules! entry{($name:ident,$t:ty)=>{pub(super) fn $name<R>(bytes:&[u8],common:&RefCell<&mut FormalBoundedContextV1<'_>>,visit:impl FnOnce(&$t)->Result<R,BodyMutationError>)->Result<R,BodyMutationError>{recover::<$t,R>(bytes,common,visit)}};}
entry!(with_factory_data_v1,oasis7_world_state::FactoryState);
entry!(with_recipe_job_data_v1,oasis7_world_state::RecipeJobState);
entry!(with_industry_progress_data_v1,oasis7_world_state::IndustryProgressState);
entry!(with_gameplay_policy_data_v1,oasis7_world_state::GameplayPolicyState);
entry!(with_site_authority_data_v1,oasis7_world_state::FactorySiteAuthorityV1);
entry!(with_agent_claim_data_v1,oasis7_world_state::AgentClaimState);
entry!(with_starter_claim_data_v1,oasis7_world_state::StarterOcClaimState);
entry!(with_location_anchor_data_v1,oasis7_world_state::LocationAnchorV1);
entry!(with_token_account_data_v1,oasis7_world_state::MainTokenAccountBalance);
entry!(with_material_ledger_data_v1,std::collections::BTreeMap<String,i64>);
entry!(with_u64_data_v1,u64);
entry!(with_i64_data_v1,i64);
entry!(with_alliance_data_v1,oasis7_world_state::AllianceState);
entry!(with_economic_contract_data_v1,oasis7_world_state::EconomicContractState);
entry!(with_product_profile_data_v1,oasis7_wasm_abi::ProductProfileV1);
entry!(with_recipe_profile_data_v1,oasis7_wasm_abi::RecipeProfileV1);

entry!(with_industrial_order_data_v1,oasis7_world_state::world_rules::AgentIndustrialOrderV1);
entry!(with_organization_registration_data_v1,oasis7_world_state::world_rules::OrganizationRegistrationV1);
entry!(with_organization_case_data_v1,oasis7_world_state::world_rules::OrganizationContinuityCaseV1);
entry!(with_industrial_root_data_v1,oasis7_agent_api::world_action::RootOperationV1);
entry!(with_industrial_lot_data_v1,oasis7_agent_api::world_action::IndustrialLotV1);
entry!(with_industrial_lot_authority_data_v1,oasis7_agent_api::world_action::IndustrialLotAuthorityV1);
entry!(with_industrial_receipt_data_v1,oasis7_agent_api::world_action::IndustrialReceiptV1);
entry!(with_industrial_disposition_data_v1,oasis7_agent_api::world_action::DispositionPlanV1);

macro_rules! owned_entry{($name:ident,$t:ty)=>{pub(super) fn $name(bytes:&[u8],common:&RefCell<&mut FormalBoundedContextV1<'_>>)->Result<ReservedValue<$t>,BodyMutationError>{owned::<$t>(bytes,common)}};}
owned_entry!(owned_factory_data_v1,oasis7_world_state::FactoryState);
owned_entry!(owned_recipe_profile_data_v1,oasis7_wasm_abi::RecipeProfileV1);
owned_entry!(owned_product_profile_data_v1,oasis7_wasm_abi::ProductProfileV1);
entry!(with_runtime_identity_data_v1,oasis7_world_state::CapabilityAgentIdentity);
entry!(with_participant_receipt_data_v1,oasis7_agent_api::game::GameParticipantReceiptV1);
entry!(with_participant_policy_data_v1,Option<oasis7_world_state::service_admission::participant::ParticipantEntryPolicyV1>);
entry!(with_identity_namespace_data_v1,oasis7_agent_api::game::GameIdentityNamespaceV1);
entry!(with_option_string_data_v1,Option<String>);

entry!(with_recipe_path_data_v1,oasis7_world_state::LogisticsPathAuthorityV1);
owned_entry!(owned_retained_root_data_v1,Option<oasis7_world_state::FormalRetainedHistoryRootV1>);

entry!(with_signer_delegation_data_v1,crate::service_admission::AgentSignerDelegationChangeV1);

entry!(with_world_rule_receipt_data_v1,oasis7_world_state::world_rules::WorldRuleReceiptV1);

entry!(with_optional_agent_intent_data_v1,Option<oasis7_world_state::AgentIntentV2>);

entry!(with_substitution_authority_data_v1,oasis7_agent_api::world_action::SubstitutionExecutionAuthorityV1);

// Fixed native recipe observations: original DTOs have only closed scalar,
// String, enum and ordinary collection fields; defaults add no nonempty graph.
facts!(oasis7_world_state::LogisticsRouteV1,oasis7_wasm_abi::MaterialProfileV1);
owned_entry!(owned_route_data_v1,oasis7_world_state::LogisticsRouteV1);
entry!(with_material_profile_data_v1,oasis7_wasm_abi::MaterialProfileV1);

/// Extract only position DATA from the retained original AgentCell row. The
/// producer has already decoded/bound the complete typed AgentCell. Ignored
/// mailbox fields are streamed through the same paid reader, not cloned into
/// an executable Agent or a second WorldState.
pub(super) fn with_agent_position_data_v1<R>(bytes:&[u8],common:&RefCell<&mut FormalBoundedContextV1<'_>>,visit:impl FnOnce(&oasis7_agent_api::world_action::GeoPos)->Result<R,BodyMutationError>)->Result<R,BodyMutationError>{
 #[derive(serde::Deserialize)]struct State{pos:oasis7_agent_api::world_action::GeoPos}
 #[derive(serde::Deserialize)]struct Agent{state:State}
 let n=bytes.len().checked_mul(16).and_then(|n|n.checked_add(65536)).ok_or_else(||invalid("selected Agent position scratch"))?;
 common.borrow_mut().charge(1,n as u64).map_err(paid)?;
 let phase=common.borrow().memory().reserve(n,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
 let mut input=Input{bytes,position:0,common,first:None};
 let result=serde_json::from_reader::<_,Agent>(&mut input);
 if let Some(error)=input.first{return Err(error)}
 common.borrow_mut().consume_steps(0).map_err(paid)?;
 let agent=match result{Ok(value)=>value,Err(error)=>return Err(paid(PaidFactCodecError{error,_fee:phase}))};
 visit(&agent.state.pos)
}

entry!(with_industrial_binding_data_v1,oasis7_agent_api::world_action::ParentBindingRevisionV1);

entry!(with_substitution_decision_data_v1,oasis7_agent_api::world_action::SubstitutionDecisionV1);
entry!(with_conversion_settlement_data_v1,oasis7_agent_api::world_action::ConversionSettlementV1);
entry!(with_substitution_residual_data_v1,oasis7_agent_api::world_action::SubstitutionResidualBalanceV1);

entry!(with_status_substitution_decision_data_v1,oasis7_agent_api::world_action::SubstitutionDecisionV1);
entry!(with_status_conversion_settlement_data_v1,oasis7_agent_api::world_action::ConversionSettlementV1);
entry!(with_status_substitution_residual_data_v1,oasis7_agent_api::world_action::SubstitutionResidualBalanceV1);

impl ClosedFact for oasis7_world_state::WorldEvent{fn largest_header()->usize{oasis7_world_state::incremental_world_event_decode::world_event_json_owned_header_max_v1()}}
entry!(with_world_event_data_v1,oasis7_world_state::WorldEvent);

pub(super) fn recover_world_event_parts_data_v1(bytes:&[u8],common:&RefCell<&mut FormalBoundedContextV1<'_>>)->Result<(oasis7_world_state::WorldEvent,MemoryReservation),BodyMutationError>{owned_parts(bytes,common)}

impl ClosedFact for serde_json::Value{fn largest_header()->usize{size_of::<serde_json::Value>().max(size_of::<String>()).max(size_of::<(String,serde_json::Value)>())}}
pub(super) fn recover_json_value_parts_data_v1(bytes:&[u8],common:&RefCell<&mut FormalBoundedContextV1<'_>>)->Result<(serde_json::Value,MemoryReservation),BodyMutationError>{owned_parts(bytes,common)}

facts!(oasis7_world_state::world_rules::SupplyAuthorizationV1,oasis7_world_state::world_rules::SupplyShipmentV1,oasis7_world_state::world_rules::OrganizationCaseV1,oasis7_world_state::world_rules::EmergencyNeedLedgerV1,oasis7_world_state::world_rules::EmergencyAuthorizationV1,oasis7_world_state::world_rules::EmergencyBatchV1,oasis7_world_state::world_rules::EmergencyReviewV1,oasis7_agent_api::world_action::EmergencyServiceRegistrationV1);
entry!(with_supply_authorization_data_v1,oasis7_world_state::world_rules::SupplyAuthorizationV1);
entry!(with_supply_shipment_data_v1,oasis7_world_state::world_rules::SupplyShipmentV1);
entry!(with_organization_legacy_case_data_v1,oasis7_world_state::world_rules::OrganizationCaseV1);
entry!(with_emergency_need_data_v1,oasis7_world_state::world_rules::EmergencyNeedLedgerV1);
entry!(with_emergency_authorization_data_v1,oasis7_world_state::world_rules::EmergencyAuthorizationV1);
entry!(with_emergency_batch_data_v1,oasis7_world_state::world_rules::EmergencyBatchV1);
entry!(with_emergency_review_data_v1,oasis7_world_state::world_rules::EmergencyReviewV1);
entry!(with_emergency_service_data_v1,oasis7_agent_api::world_action::EmergencyServiceRegistrationV1);
