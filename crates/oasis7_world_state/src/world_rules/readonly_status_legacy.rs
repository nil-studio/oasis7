//! Legacy raw rows for the single status kernel. Absence is only the actual
//! WorldState.world_rules=None branch, never a caller-provided empty map.
use super::*;
use crate::{WorldState,AllianceState,EconomicContractState};
use oasis7_agent_api::world_action::*;
use std::{convert::Infallible,ops::ControlFlow};
impl SubstitutionReadonlyFactsV1 for WorldState{
 fn substitution_metered(&self)->bool{false}
 fn substitution_rules_present(&self)->Result<bool,Infallible>{Ok(self.world_rules.is_some())}
 fn substitution_with_authority<T>(&self,id:&str,v:impl FnOnce(Option<&SubstitutionExecutionAuthorityV1>)->Result<T,Infallible>)->Result<T,Infallible>{v(self.world_rules.as_ref().and_then(|r|r.substitution.authorities.get(id)))}
 fn substitution_visit_authorities(&self,mut v:impl FnMut(&SubstitutionExecutionAuthorityV1)->Result<ControlFlow<()>,Infallible>)->Result<(),Infallible>{if let Some(r)=&self.world_rules{for row in r.substitution.authorities.values(){if v(row)?.is_break(){break}}}Ok(())}
 fn substitution_visit_roots(&self,mut v:impl FnMut(&RootOperationV1)->Result<ControlFlow<()>,Infallible>)->Result<(),Infallible>{if let Some(r)=&self.world_rules{for row in r.industrial_work.roots.values(){if v(row)?.is_break(){break}}}Ok(())}
 fn substitution_visit_lots(&self,mut v:impl FnMut(&IndustrialLotV1)->Result<ControlFlow<()>,Infallible>)->Result<(),Infallible>{if let Some(r)=&self.world_rules{for row in r.industrial_work.lots.values(){if v(row)?.is_break(){break}}}Ok(())}
}
impl OrganizationActivityReadonlyFactsV1 for WorldState{
 fn organization_time(&self)->Result<u64,Infallible>{Ok(self.time)}
 fn organization_visit_registrations(&self,mut v:impl FnMut(&OrganizationRegistrationV1)->Result<ControlFlow<()>,Infallible>)->Result<(),Infallible>{if let Some(r)=&self.world_rules{for row in r.organization_continuity.registrations.values(){if v(row)?.is_break(){break}}}Ok(())}
 fn organization_visit_cases(&self,mut v:impl FnMut(&OrganizationContinuityCaseV1)->Result<ControlFlow<()>,Infallible>)->Result<(),Infallible>{if let Some(r)=&self.world_rules{for row in r.organization_continuity.cases.values(){if v(row)?.is_break(){break}}}Ok(())}
 fn organization_with_alliance<T>(&self,id:&str,v:impl FnOnce(Option<&AllianceState>)->Result<T,Infallible>)->Result<T,Infallible>{v(self.alliances.get(id))}
 fn organization_visit_contracts(&self,mut v:impl FnMut(&EconomicContractState)->Result<ControlFlow<()>,Infallible>)->Result<(),Infallible>{for row in self.economic_contracts.values(){if v(row)?.is_break(){break}}Ok(())}
}
