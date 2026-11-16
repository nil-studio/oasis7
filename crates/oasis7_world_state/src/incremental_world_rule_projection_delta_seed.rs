//! GEN/cold logical row inventory over the original WorldRules source. The
//! original thirteen-field emitter owns order and skip decisions; this visitor
//! adds only closed typed collection access, including empty containers.
use crate::{world_rules::WorldRulesStateV1,incremental_semantic_key::SemanticKeyV1,OriginalStateFieldEmissionV1,OriginalStateFieldSlotEventV1};
use oasis7_formal_protocol::incremental_value::ValueTreeError;
use std::{cell::RefCell,collections::{BTreeMap,BTreeSet}};
macro_rules! tops {($($v:ident=>$name:literal),+)=>{
 #[derive(Clone,Copy,Debug,PartialEq,Eq)]pub enum WorldRuleLogicalTopSlotV1{$($v),+}
 impl WorldRuleLogicalTopSlotV1{pub const ALL:[Self;13]=[$(Self::$v),+];pub fn name(self)->&'static str{match self{$(Self::$v=>$name),+}}fn from_name(name:&str)->Option<Self>{match name{$($name=>Some(Self::$v)),+,_=>None}}}
};}
tops!(Requests=>"requests",GovernanceEffects=>"governance_effects",OrganizationCases=>"organization_cases",OrganizationEstate=>"organization_estate",SupplyAuthorizations=>"supply_authorizations",SupplyBatches=>"supply_batches",SupplyLineages=>"supply_lineages",SupplyShipments=>"supply_shipments",OrganizationContinuity=>"organization_continuity",EmergencySupply=>"emergency_supply",AgentOrders=>"agent_orders",IndustrialWork=>"industrial_work",Substitution=>"substitution");
#[derive(Clone,Copy,Debug,PartialEq,Eq)]pub enum WorldRuleLogicalContainerKindV1{Map,Set,Sequence,Scalar}
macro_rules! columns {($($v:ident:$kind:ident=>[$($path:literal),+]),+$(,)?)=>{
 #[derive(Clone,Copy,Debug,PartialEq,Eq)]pub enum WorldRuleLogicalColumnV1{$($v),+}
 impl WorldRuleLogicalColumnV1{pub const ALL:&'static[Self]=&[$(Self::$v),+];pub fn path(self)->&'static[&'static str]{match self{$(Self::$v=>&["world_rules",$($path),+]),+}}pub fn kind(self)->WorldRuleLogicalContainerKindV1{match self{$(Self::$v=>WorldRuleLogicalContainerKindV1::$kind),+}}}
};}
columns!{
 Requests:Map=>["requests"],GovernanceEffects:Map=>["governance_effects"],OrganizationCases:Map=>["organization_cases"],OrganizationEstate:Map=>["organization_estate"],SupplyAuthorizations:Map=>["supply_authorizations"],SupplyBatches:Map=>["supply_batches"],SupplyLineages:Map=>["supply_lineages"],SupplyShipments:Map=>["supply_shipments"],AgentOrders:Map=>["agent_orders"],
 OrganizationRegistrations:Map=>["organization_continuity","registrations"],OrganizationAvailability:Map=>["organization_continuity","availability"],OrganizationActivityReceipts:Map=>["organization_continuity","activity_receipts"],OrganizationMembershipHistory:Map=>["organization_continuity","membership_history"],OrganizationAssets:Map=>["organization_continuity","assets"],OrganizationContinuityCases:Map=>["organization_continuity","cases"],OrganizationConsumedEffects:Map=>["organization_continuity","consumed_effects"],
 EmergencyWorldId:Scalar=>["emergency_supply","world_id"],MinimumServices:Map=>["emergency_supply","minimum_services"],RegionLineages:Map=>["emergency_supply","region_lineages"],EmergencyObservations:Map=>["emergency_supply","observations"],ObservationIntervalCoverage:Map=>["emergency_supply","observation_interval_coverage"],OverflowedWindows:Set=>["emergency_supply","overflowed_windows"],EmergencyServices:Map=>["emergency_supply","services"],EmergencyEvidence:Map=>["emergency_supply","evidence"],EmergencyAuthorizations:Map=>["emergency_supply","authorizations"],EmergencyBatches:Map=>["emergency_supply","batches"],EmergencyNeeds:Map=>["emergency_supply","needs"],EmergencyReviews:Map=>["emergency_supply","reviews"],RestrictedPower:Map=>["emergency_supply","restricted_power"],
 IndustrialRoots:Map=>["industrial_work","roots"],IndustrialLots:Map=>["industrial_work","lots"],ImportedLotAuthorities:Map=>["industrial_work","imported_lot_authorities"],IndustrialReceipts:Map=>["industrial_work","receipts"],IndustrialSinkKeys:Set=>["industrial_work","sink_keys"],IndustrialBindings:Sequence=>["industrial_work","bindings"],IndustrialDispositions:Map=>["industrial_work","dispositions"],
 SubstitutionAuthorities:Map=>["substitution","authorities"],SubstitutionDecisions:Map=>["substitution","decisions"],SubstitutionSettlements:Map=>["substitution","settlements"],ConversionDomains:Set=>["substitution","conversion_domains"],TerminalUsed:Map=>["substitution","terminal_used"],RetainedResiduals:Map=>["substitution","retained_residuals"],
}
pub trait WorldRuleLogicalRowsVisitorV1 {
 type Error:From<ValueTreeError>;
 fn work(&mut self,steps:u64,bytes:usize)->Result<(),Self::Error>;
 fn top_slot(&mut self,slot:WorldRuleLogicalTopSlotV1,emission:OriginalStateFieldEmissionV1)->Result<(),Self::Error>;
 fn top_emitted(&mut self,slot:WorldRuleLogicalTopSlotV1)->Result<(),Self::Error>;
 /// Called once even for an empty map. K is the original nominal key type;
 /// iteration and the consumer's AVL comparison must retain its Ord.
 fn map<K:Ord+serde::Serialize+SemanticKeyV1,V:serde::Serialize>(&mut self,column:WorldRuleLogicalColumnV1,values:&BTreeMap<K,V>)->Result<(),Self::Error>;
 fn set<K:Ord+serde::Serialize+SemanticKeyV1>(&mut self,column:WorldRuleLogicalColumnV1,values:&BTreeSet<K>)->Result<(),Self::Error>;
 fn sequence<V:serde::Serialize>(&mut self,column:WorldRuleLogicalColumnV1,values:&[V])->Result<(),Self::Error>;
 fn scalar<V:serde::Serialize+?Sized>(&mut self,column:WorldRuleLogicalColumnV1,value:&V)->Result<(),Self::Error>;
}
fn branch<V:WorldRuleLogicalRowsVisitorV1>(source:&WorldRulesStateV1,slot:WorldRuleLogicalTopSlotV1,v:&mut V)->Result<(),V::Error>{
 use WorldRuleLogicalColumnV1 as C;use WorldRuleLogicalTopSlotV1 as T;
 // Exhaustive destructuring prevents silently losing a newly added source.
 let WorldRulesStateV1{requests,governance_effects,organization_cases,organization_estate,supply_authorizations,supply_batches,supply_lineages,supply_shipments,organization_continuity,emergency_supply,agent_orders,industrial_work,substitution}=source;
 let crate::world_rules::OrganizationContinuityStateV1{registrations:_,availability:_,activity_receipts:_,membership_history:_,assets:_,cases:_,consumed_effects:_}=organization_continuity;
 let crate::world_rules::EmergencySupplyStateV1{world_id:_,minimum_services:_,region_lineages:_,observations:_,observation_interval_coverage:_,overflowed_windows:_,services:_,evidence:_,authorizations:_,batches:_,needs:_,reviews:_,restricted_power:_}=emergency_supply;
 let oasis7_agent_api::world_action::IndustrialWorkStateV1{roots:_,lots:_,imported_lot_authorities:_,receipts:_,sink_keys:_,bindings:_,dispositions:_}=industrial_work;
 let oasis7_agent_api::world_action::SubstitutionStateV1{authorities:_,decisions:_,settlements:_,conversion_domains:_,terminal_used:_,retained_residuals:_}=substitution;
 macro_rules! maps {($owner:ident,{$($name:ident=>$column:ident),+})=>{$(v.map(C::$column,&$owner.$name)?;)+};}
 match slot {
 T::Requests=>v.map(C::Requests,requests)?,T::GovernanceEffects=>v.map(C::GovernanceEffects,governance_effects)?,T::OrganizationCases=>v.map(C::OrganizationCases,organization_cases)?,T::OrganizationEstate=>v.map(C::OrganizationEstate,organization_estate)?,T::SupplyAuthorizations=>v.map(C::SupplyAuthorizations,supply_authorizations)?,T::SupplyBatches=>v.map(C::SupplyBatches,supply_batches)?,T::SupplyLineages=>v.map(C::SupplyLineages,supply_lineages)?,T::SupplyShipments=>v.map(C::SupplyShipments,supply_shipments)?,T::AgentOrders=>v.map(C::AgentOrders,agent_orders)?,
 T::OrganizationContinuity=>{maps!(organization_continuity,{registrations=>OrganizationRegistrations,availability=>OrganizationAvailability,activity_receipts=>OrganizationActivityReceipts,membership_history=>OrganizationMembershipHistory,assets=>OrganizationAssets,cases=>OrganizationContinuityCases,consumed_effects=>OrganizationConsumedEffects});},
 T::EmergencySupply=>{v.scalar(C::EmergencyWorldId,&emergency_supply.world_id)?;maps!(emergency_supply,{minimum_services=>MinimumServices,region_lineages=>RegionLineages,observations=>EmergencyObservations,observation_interval_coverage=>ObservationIntervalCoverage});v.set(C::OverflowedWindows,&emergency_supply.overflowed_windows)?;maps!(emergency_supply,{services=>EmergencyServices,evidence=>EmergencyEvidence,authorizations=>EmergencyAuthorizations,batches=>EmergencyBatches,needs=>EmergencyNeeds,reviews=>EmergencyReviews,restricted_power=>RestrictedPower});},
 T::IndustrialWork=>{maps!(industrial_work,{roots=>IndustrialRoots,lots=>IndustrialLots,imported_lot_authorities=>ImportedLotAuthorities,receipts=>IndustrialReceipts});v.set(C::IndustrialSinkKeys,&industrial_work.sink_keys)?;v.sequence(C::IndustrialBindings,&industrial_work.bindings)?;v.map(C::IndustrialDispositions,&industrial_work.dispositions)?;},
 T::Substitution=>{maps!(substitution,{authorities=>SubstitutionAuthorities,decisions=>SubstitutionDecisions,settlements=>SubstitutionSettlements});v.set(C::ConversionDomains,&substitution.conversion_domains)?;maps!(substitution,{terminal_used=>TerminalUsed,retained_residuals=>RetainedResiduals});},
 }Ok(())
}
enum DriverError<E>{Visitor(E),Schema}
impl<E> std::fmt::Debug for DriverError<E>{fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result{f.write_str("WorldRules logical source visitor")}}
impl<E> std::fmt::Display for DriverError<E>{fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result{f.write_str("WorldRules logical source visitor")}}
impl<E> std::error::Error for DriverError<E>{}
impl<E> serde::ser::Error for DriverError<E>{fn custom<T:std::fmt::Display>(_:T)->Self{Self::Schema}}
struct Driver<'a,'r,'v,V>{source:&'a WorldRulesStateV1,visitor:&'r RefCell<&'v mut V>}
impl<V:WorldRuleLogicalRowsVisitorV1> serde::ser::SerializeStruct for Driver<'_, '_, '_,V>{
 type Ok=();type Error=DriverError<V::Error>;
 fn serialize_field<T:serde::Serialize+?Sized>(&mut self,name:&'static str,_:&T)->Result<(),Self::Error>{
  let slot=WorldRuleLogicalTopSlotV1::from_name(name).ok_or(DriverError::Schema)?;
  if !matches!(slot,WorldRuleLogicalTopSlotV1::IndustrialWork|WorldRuleLogicalTopSlotV1::Substitution){branch(self.source,slot,&mut **self.visitor.borrow_mut()).map_err(DriverError::Visitor)?;}Ok(())
 }
 fn end(self)->Result<(),Self::Error>{Ok(())}
}
/// Borrows the actual GEN/cold source, performs no clone/codec/qualification,
/// and delegates all allocation, encoding, loops and cancellation to visitor.
pub fn visit_original_world_rule_logical_rows_v1<V:WorldRuleLogicalRowsVisitorV1>(source:&WorldRulesStateV1,visitor:&mut V)->Result<(),V::Error>{
 // The two original default-equality checks inspect only empty-container
 // headers (seven and six fixed members); no record graph is compared.
 visitor.work(32,std::mem::size_of::<oasis7_agent_api::world_action::IndustrialWorkStateV1>()+std::mem::size_of::<oasis7_agent_api::world_action::SubstitutionStateV1>())?;
 let visitor=RefCell::new(visitor);
 let result=source.visit_original_serialized_fields_with_slots_v1(|_|Ok(Driver{source,visitor:&visitor}),&mut |event|{
  match event {
   OriginalStateFieldSlotEventV1::Declared(slot)=>{let top=WorldRuleLogicalTopSlotV1::from_name(slot.name).ok_or(DriverError::Schema)?;let mut visitor=visitor.borrow_mut();visitor.top_slot(top,slot.emission).map_err(DriverError::Visitor)?;branch(source,top,&mut **visitor).map_err(DriverError::Visitor)?;},
   OriginalStateFieldSlotEventV1::Emitted(name)=>{let top=WorldRuleLogicalTopSlotV1::from_name(name).ok_or(DriverError::Schema)?;let mut visitor=visitor.borrow_mut();if !matches!(top,WorldRuleLogicalTopSlotV1::IndustrialWork|WorldRuleLogicalTopSlotV1::Substitution){visitor.top_slot(top,OriginalStateFieldEmissionV1::Always).map_err(DriverError::Visitor)?;}visitor.top_emitted(top).map_err(DriverError::Visitor)?;},
  }Ok(())
 });
 match result{Ok(())=>Ok(()),Err(DriverError::Visitor(error))=>Err(error),Err(DriverError::Schema)=>Err(ValueTreeError::Invalid("WorldRules original logical source schema").into())}
}

/// Encoded originals from the complete qualified ALL reader. These callbacks
/// emit DATA only: neither a layout nor a row is a source qualification.
pub trait QualifiedWorldRuleLogicalRowsVisitorV1{
 type Error:From<ValueTreeError>;
 fn work(&mut self,steps:u64,bytes:usize)->Result<(),Self::Error>;
 fn top(&mut self,slot:WorldRuleLogicalTopSlotV1,emission:OriginalStateFieldEmissionV1,emitted:bool)->Result<(),Self::Error>;
 /// Number of actual projection records in this logical column's complete
 /// qualified scope. Zero is authenticated absence, not a supplied default.
 fn column(&mut self,column:WorldRuleLogicalColumnV1,projection_records:usize)->Result<(),Self::Error>;
 fn row(&mut self,column:WorldRuleLogicalColumnV1,path:&[String],semantic:&[u8],original_json:&[u8])->Result<(),Self::Error>;
}
impl WorldRuleLogicalTopSlotV1{
 pub(super) fn original_emission_v1(self)->OriginalStateFieldEmissionV1{match self{Self::IndustrialWork|Self::Substitution=>OriginalStateFieldEmissionV1::OriginalDefault,_=>OriginalStateFieldEmissionV1::Always}}
}
