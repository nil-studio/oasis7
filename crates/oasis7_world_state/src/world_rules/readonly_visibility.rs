//! Original supply/continuity/emergency observation relations. Only actual
//! typed facts and processing/allocation callbacks enter this shared kernel.
use super::*;
use oasis7_agent_api::world_action::EmergencyServiceRegistrationV1;
use std::{collections::{BTreeMap,BTreeSet},mem::size_of};
pub trait RuleVisibilityWorkV1{type Error;fn visibility_metered(&self)->bool{true}fn visibility_work(&self,steps:u64,bytes:usize)->Result<(),Self::Error>;fn visibility_owned(&self,bytes:usize)->Result<(),Self::Error>;}
struct Ids{values:BTreeSet<String>,max_len:usize}
impl Ids{
 fn new<S:RuleVisibilityWorkV1>(s:&S)->Result<Self,S::Error>{s.visibility_owned(size_of::<Self>())?;Ok(Self{values:BTreeSet::new(),max_len:0})}
 fn has<S:RuleVisibilityWorkV1>(&self,s:&S,key:&str)->Result<bool,S::Error>{let levels=(self.values.len()+1).ilog2()as usize+2;s.visibility_work((levels*11)as u64,(self.max_len+key.len()).saturating_mul(levels*11))?;Ok(self.values.contains(key))}
 fn insert<S:RuleVisibilityWorkV1>(&mut self,s:&S,key:&str)->Result<(),S::Error>{if self.has(s,key)?{return Ok(())}let node=(11*size_of::<String>()+13*size_of::<usize>()+128).saturating_mul((self.values.len()+1).ilog2()as usize+2);s.visibility_owned(node+size_of::<String>()+key.len())?;let mut bytes=Vec::with_capacity(key.len());assert_eq!(bytes.capacity(),key.len(),"visibility ID exact capacity");for chunk in key.as_bytes().chunks(8192){s.visibility_work(1,chunk.len())?;bytes.extend_from_slice(chunk)}self.values.insert(String::from_utf8(bytes).expect("original visibility UTF-8"));self.max_len=self.max_len.max(key.len());Ok(())}
}
fn equal<S:RuleVisibilityWorkV1>(s:&S,a:&str,b:&str)->Result<bool,S::Error>{s.visibility_work(1,a.len()+b.len())?;Ok(a==b)}
fn text_set_has<S:RuleVisibilityWorkV1>(s:&S,set:&BTreeSet<String>,key:&str)->Result<bool,S::Error>{if s.visibility_metered(){let mut max=0;for value in set{s.visibility_work(1,0)?;max=max.max(value.len())}let levels=(set.len()+1).ilog2()as usize+2;s.visibility_work((levels*11)as u64,(max+key.len()).saturating_mul(levels*11))?}Ok(set.contains(key))}
fn reader<S:RuleVisibilityWorkV1>(s:&S,id:&str,value:Option<&str>)->Result<bool,S::Error>{match value{None=>Ok(false),Some(value)=>equal(s,id,value)}}
pub trait SupplyVisibilityFactsV1:RuleVisibilityWorkV1{
 fn visibility_supply_authorizations(&self,visit:impl FnMut(&SupplyAuthorizationV1)->Result<(),Self::Error>)->Result<(),Self::Error>;
 fn visibility_supply_authorization<T>(&self,id:&str,visit:impl FnOnce(Option<&SupplyAuthorizationV1>)->Result<T,Self::Error>)->Result<T,Self::Error>;
}
pub struct SupplyVisibilityPlanV1{lineages:Ids}
impl SupplyVisibilityPlanV1{
 pub fn lineage<S:RuleVisibilityWorkV1>(&self,s:&S,id:&str)->Result<bool,S::Error>{self.lineages.has(s,id)}
 pub fn provider<S:SupplyVisibilityFactsV1>(&self,s:&S,id:&str,value:Option<&str>)->Result<bool,S::Error>{s.visibility_supply_authorization(id,|auth|match auth{None=>Ok(false),Some(auth)=>reader(s,&auth.provider_id,value)})}
 pub fn request<S:RuleVisibilityWorkV1>(&self,s:&S,provider:&str,beneficiary:&str,value:Option<&str>)->Result<bool,S::Error>{Ok(reader(s,provider,value)?||reader(s,beneficiary,value)?)}
 pub fn case_custodian<S:RuleVisibilityWorkV1>(&self,s:&S,case:&OrganizationCaseV1,value:Option<&str>)->Result<bool,S::Error>{if reader(s,&case.founder_id,value)?{return Ok(true)}for grant in case.grants.values(){s.visibility_work(1,0)?;if reader(s,&grant.delegate_id,value)?{return Ok(true)}}Ok(false)}
 pub fn beneficiary_request<S:RuleVisibilityWorkV1>(&self,s:&S,provider:bool,beneficiary:&str,value:Option<&str>)->Result<bool,S::Error>{s.visibility_work(1,0)?;Ok(provider||reader(s,beneficiary,value)?)}
 pub fn case_request<S:RuleVisibilityWorkV1>(&self,s:&S,custodian:bool,requester:&str,value:Option<&str>)->Result<bool,S::Error>{s.visibility_work(1,0)?;Ok(custodian||reader(s,requester,value)?)}
 pub fn shipment<S:SupplyVisibilityFactsV1>(&self,s:&S,shipment:&SupplyShipmentV1,value:Option<&str>)->Result<bool,S::Error>{Ok(reader(s,&shipment.beneficiary_id,value)?||self.provider(s,&shipment.authorization_id,value)?)}
}
pub fn supply_visibility_plan_v1<S:SupplyVisibilityFactsV1>(s:&S,value:Option<&str>)->Result<SupplyVisibilityPlanV1,S::Error>{let mut lineages=Ids::new(s)?;s.visibility_supply_authorizations(|auth|{s.visibility_work(1,0)?;for beneficiary in &auth.beneficiaries{s.visibility_work(1,0)?;if reader(s,&auth.provider_id,value)?||reader(s,beneficiary,value)?{let n=auth.crisis_id.len().saturating_add(auth.material.len()).saturating_add(beneficiary.len()).saturating_add(64);s.visibility_owned(size_of::<String>()+n)?;s.visibility_work(1,n)?;let id=supply::lineage(auth,beneficiary);lineages.insert(s,&id)?}}Ok(())})?;Ok(SupplyVisibilityPlanV1{lineages})}
pub trait OrganizationVisibilityFactsV1:RuleVisibilityWorkV1{fn visibility_organization_registrations(&self,visit:impl FnMut(&OrganizationRegistrationV1)->Result<(),Self::Error>)->Result<(),Self::Error>;}
pub struct OrganizationVisibilityPlanV1{authorized:Ids}
impl OrganizationVisibilityPlanV1{
 pub fn organization<S:RuleVisibilityWorkV1>(&self,s:&S,id:&str)->Result<bool,S::Error>{self.authorized.has(s,id)}
 pub fn asset<S:RuleVisibilityWorkV1>(&self,s:&S,organization:&str,contributor:&str,value:Option<&str>)->Result<bool,S::Error>{Ok(self.organization(s,organization)?||equal(s,contributor,value.unwrap_or(""))?)}
 pub fn candidate_body<S:RuleVisibilityWorkV1>(&self,s:&S,organization:&str,candidate:&str,value:Option<&str>)->Result<bool,S::Error>{Ok(equal(s,candidate,value.unwrap_or(""))?||self.organization(s,organization)?)}
 pub fn remedy_body<S:RuleVisibilityWorkV1>(&self,s:&S,organization:&str,requester:&str,value:Option<&str>)->Result<bool,S::Error>{Ok(equal(s,requester,value.unwrap_or(""))?||self.organization(s,organization)?)}
 pub fn case_body<S:RuleVisibilityWorkV1>(&self,s:&S,case:&OrganizationContinuityCaseV1,value:Option<&str>)->Result<bool,S::Error>{let value=value.unwrap_or("");if text_set_has(s,&case.recipients,value)?{return Ok(true)}if self.organization(s,&case.organization_id)?{return Ok(true)}if s.visibility_metered(){let levels=(case.candidates.len()+1).ilog2()as usize+2;let mut max=0;for key in case.candidates.keys(){s.visibility_work(1,0)?;max=max.max(key.len())}s.visibility_work((levels*11)as u64,(value.len()+max).saturating_mul(levels*11))?;}Ok(case.candidates.contains_key(value))}
}
pub fn organization_visibility_plan_v1<S:OrganizationVisibilityFactsV1>(s:&S,value:Option<&str>)->Result<OrganizationVisibilityPlanV1,S::Error>{let value=value.unwrap_or("");let mut authorized=Ids::new(s)?;s.visibility_organization_registrations(|registration|{let mut found=equal(s,&registration.founder_id,value)?;if !found{found=text_set_has(s,&registration.appointment_authorities,value)?}if !found{found=text_set_has(s,&registration.remedy_adjudicators,value)?}if !found{for id in &registration.designated_successors{if equal(s,id,value)?{found=true;break}}}if !found{for id in &registration.neutral_maintainers{if equal(s,id,value)?{found=true;break}}}if found{authorized.insert(s,&registration.organization_id)?}Ok(())})?;Ok(OrganizationVisibilityPlanV1{authorized})}
pub trait EmergencyVisibilityFactsV1:RuleVisibilityWorkV1{
 fn visibility_emergency_needs(&self,visit:impl FnMut(&str,&EmergencyNeedLedgerV1)->Result<(),Self::Error>)->Result<(),Self::Error>;
 fn visibility_emergency_services(&self,visit:impl FnMut(&EmergencyServiceRegistrationV1)->Result<(),Self::Error>)->Result<(),Self::Error>;
 fn visibility_emergency_authorizations(&self,visit:impl FnMut(&EmergencyAuthorizationV1)->Result<(),Self::Error>)->Result<(),Self::Error>;
 fn visibility_emergency_reviews(&self,visit:impl FnMut(&EmergencyReviewV1)->Result<(),Self::Error>)->Result<(),Self::Error>;
 fn visibility_emergency_batch<T>(&self,id:&str,visit:impl FnOnce(Option<&EmergencyBatchV1>)->Result<T,Self::Error>)->Result<T,Self::Error>;
 fn visibility_emergency_authorization<T>(&self,id:&str,visit:impl FnOnce(Option<&EmergencyAuthorizationV1>)->Result<T,Self::Error>)->Result<T,Self::Error>;
}
pub struct EmergencyVisibilityPlanV1{visible:Ids,obligations:Ids}
impl EmergencyVisibilityPlanV1{
 pub fn need<S:RuleVisibilityWorkV1>(&self,s:&S,id:&str)->Result<bool,S::Error>{self.visible.has(s,id)}
 pub fn obligation<S:RuleVisibilityWorkV1>(&self,s:&S,id:&str)->Result<bool,S::Error>{self.obligations.has(s,id)}
 pub fn review<S:RuleVisibilityWorkV1>(&self,s:&S,need:&str,requester:&str,value:Option<&str>)->Result<bool,S::Error>{Ok(self.need(s,need)?||reader(s,requester,value)?)}
 pub fn restricted<S:RuleVisibilityWorkV1>(&self,s:&S,beneficiary:&str,value:Option<&str>)->Result<bool,S::Error>{reader(s,beneficiary,value)}
}
pub fn emergency_visibility_plan_v1<S:EmergencyVisibilityFactsV1>(s:&S,value:Option<&str>)->Result<EmergencyVisibilityPlanV1,S::Error>{
 let mut visible=Ids::new(s)?;s.visibility_emergency_needs(|id,need|{if reader(s,&need.beneficiary_id,value)?{visible.insert(s,id)?}Ok(())})?;
 s.visibility_emergency_authorizations(|auth|{if reader(s,&auth.provider_id,value)?{for id in &auth.requested{s.visibility_work(1,0)?;visible.insert(s,id)?}}Ok(())})?;
 s.visibility_emergency_reviews(|review|s.visibility_emergency_batch(&review.batch_id,|batch|{let Some(batch)=batch else{return Ok(())};s.visibility_emergency_authorization(&batch.authorization_id,|auth|{if let Some(auth)=auth{if reader(s,&auth.reviewer_id,value)?{visible.insert(s,&review.need_id)?}}Ok(())})}))?;
 let mut obligations=Ids::new(s)?;s.visibility_emergency_needs(|id,need|{if visible.has(s,id)?{obligations.insert(s,&need.obligation_id)?}Ok(())})?;
 s.visibility_emergency_services(|service|{if reader(s,&service.beneficiary_id,value)?{obligations.insert(s,&service.obligation_id)?}Ok(())})?;
 Ok(EmergencyVisibilityPlanV1{visible,obligations})
}

pub(super) struct LegacySupplyVisibilityV1<'a>(pub &'a BTreeMap<String,SupplyAuthorizationV1>);
pub(super) struct LegacyOrganizationVisibilityV1<'a>(pub &'a OrganizationContinuityStateV1);
pub(super) struct LegacyEmergencyVisibilityV1<'a>(pub &'a EmergencySupplyStateV1);
macro_rules! legacy_work{($($t:ident),+)=>{$(impl RuleVisibilityWorkV1 for $t<'_>{type Error=std::convert::Infallible;fn visibility_metered(&self)->bool{false}fn visibility_work(&self,_:u64,_:usize)->Result<(),Self::Error>{Ok(())}fn visibility_owned(&self,_:usize)->Result<(),Self::Error>{Ok(())}})+}}
legacy_work!(LegacySupplyVisibilityV1,LegacyOrganizationVisibilityV1,LegacyEmergencyVisibilityV1);
impl SupplyVisibilityFactsV1 for LegacySupplyVisibilityV1<'_>{fn visibility_supply_authorizations(&self,mut visit:impl FnMut(&SupplyAuthorizationV1)->Result<(),Self::Error>)->Result<(),Self::Error>{for auth in self.0.values(){visit(auth)?}Ok(())}fn visibility_supply_authorization<T>(&self,id:&str,visit:impl FnOnce(Option<&SupplyAuthorizationV1>)->Result<T,Self::Error>)->Result<T,Self::Error>{visit(self.0.get(id))}}
impl OrganizationVisibilityFactsV1 for LegacyOrganizationVisibilityV1<'_>{fn visibility_organization_registrations(&self,mut visit:impl FnMut(&OrganizationRegistrationV1)->Result<(),Self::Error>)->Result<(),Self::Error>{for registration in self.0.registrations.values(){visit(registration)?}Ok(())}}
impl EmergencyVisibilityFactsV1 for LegacyEmergencyVisibilityV1<'_>{
 fn visibility_emergency_needs(&self,mut visit:impl FnMut(&str,&EmergencyNeedLedgerV1)->Result<(),Self::Error>)->Result<(),Self::Error>{for(id,value)in &self.0.needs{visit(id,value)?}Ok(())}
 fn visibility_emergency_services(&self,mut visit:impl FnMut(&EmergencyServiceRegistrationV1)->Result<(),Self::Error>)->Result<(),Self::Error>{for value in self.0.services.values(){visit(value)?}Ok(())}
 fn visibility_emergency_authorizations(&self,mut visit:impl FnMut(&EmergencyAuthorizationV1)->Result<(),Self::Error>)->Result<(),Self::Error>{for value in self.0.authorizations.values(){visit(value)?}Ok(())}
 fn visibility_emergency_reviews(&self,mut visit:impl FnMut(&EmergencyReviewV1)->Result<(),Self::Error>)->Result<(),Self::Error>{for value in self.0.reviews.values(){visit(value)?}Ok(())}
 fn visibility_emergency_batch<T>(&self,id:&str,visit:impl FnOnce(Option<&EmergencyBatchV1>)->Result<T,Self::Error>)->Result<T,Self::Error>{visit(self.0.batches.get(id))}
 fn visibility_emergency_authorization<T>(&self,id:&str,visit:impl FnOnce(Option<&EmergencyAuthorizationV1>)->Result<T,Self::Error>)->Result<T,Self::Error>{visit(self.0.authorizations.get(id))}
}
