//! Complete Current lifecycle facts, built only from authenticated selections.
use super::*;
use super::invalid;
use std::collections::{BTreeMap,BTreeSet};
use oasis7_formal_protocol::incremental_value::{ValueNodeReader,ValueTreeError};
use oasis7_world_state::{incremental_field_overlay::WorldFieldOverlayContextV1,incremental_field_read::WorldFieldReadContextV1,
 incremental_diplomacy::*,world_rules::*,incremental_world_rule_composite_family::{bind_current_agent_orders_v1,observe_current_requests_result_sources_v1,read_current_requests_result_receipt_v1},
 incremental_world_rule_organization::root::{observe_current_organization_risk_v1,read_current_organization_estate_v1}};
use crate::world::{incremental_factory_directory_orders::{AcceptedFactoryOrderDirectoryV1,open_current_factory_order_directory_v1},
 incremental_industrial_eligibility::{CurrentStateOwnerSourcesV1,resolve_current_state_owner_v1},
 incremental_normal_gameplay_cycles::{read_current_diplomacy_text_with_limit_v1 as text,read_current_diplomacy_member_text_with_limit_v1 as member_text,read_current_diplomacy_member_length_v1 as member_length},
 incremental_normal_tail::NormalTailWorkV1};
#[derive(Debug)]struct Infra<T:std::fmt::Debug>(T);
impl<T:std::fmt::Debug>std::fmt::Display for Infra<T>{fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result{write!(f,"organization Current fact: {:?}",self.0)}}
impl<T:std::fmt::Debug>std::error::Error for Infra<T>{}
fn infra<T:std::fmt::Debug+Send+Sync+'static>(e:T)->BodyMutationError{BodyMutationError::Original(Box::new(Infra(e)))}
#[derive(Clone,Copy)]
pub(super) enum LifecycleFactScopeV1 { ActivityCatalog, Lifecycle }
pub(super) struct CurrentOrganizationFactsV1 {value:OrganizationCurrentLifecycleFactsV1,guard:PreparedGuard,scope:LifecycleFactScopeV1,_allocations:super::admission::AdmissionV1}
impl CurrentOrganizationFactsV1 {
 pub(super) fn value_for_activity(&self)->&OrganizationCurrentLifecycleFactsV1{&self.value}
 pub(super) fn value_for_lifecycle(&self)->Result<&OrganizationCurrentLifecycleFactsV1,BodyMutationError>{if !matches!(self.scope,LifecycleFactScopeV1::Lifecycle){return Err(super::invalid("incomplete organization lifecycle facts"))}Ok(&self.value)}
 pub(super) fn clone_allocation_bytes(&self)->usize{self._allocations.bytes()}
 pub(super) fn validate<M:MerkleNodeReader>(&self,tx:&ExecutionTransaction<'_,M>,live:&ExecutionFence)->Result<(),BodyMutationError>{tx.validate_guard(&self.guard,live)?;Ok(())}
}
#[allow(clippy::too_many_arguments)]
pub(super) fn read_complete_facts_v1<M,P,R,E,C,PE,PC>(registration:&OrganizationRegistrationV1,
 sources:&NormalTailSourcesV1<'_,'_>,accepted_orders:&ReservedValue<AcceptedFactoryOrderDirectoryV1>,state:&M,
 ctx:&mut WorldFieldOverlayContextV1<'_,'_,'_,M,R,E,C>,parent:&mut WorldFieldReadContextV1<'_,P,R,PE,PC>,
 work:&mut NormalOrganizationTailWorkV1,scope:LifecycleFactScopeV1,tail:&mut NormalTailWorkV1<'_,'_>,live:&ExecutionFence)
 ->Result<CurrentOrganizationFactsV1,BodyMutationError>
where M:MerkleNodeReader,P:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool,PE:FnMut()->Option<ValueTreeError>,PC:FnMut()->bool {
 let mut allocations=super::admission::AdmissionV1::new(tail)?;
 allocations.admit(size_of::<OrganizationCurrentLifecycleFactsV1>().checked_add(registration.organization_id.len()).ok_or_else(||invalid("organization facts header"))?,tail)?;
 let id=&registration.organization_id;let roots=sources.roots();let guard=ctx.transaction.guard();
 let guard=ctx.transaction.guard();
 let orders=bind_current_agent_orders_v1(sources.native().rules().orders(),roots,state,ctx,&mut work.scan,&mut work.read,&guard,live).map_err(infra)?;
 let guard=ctx.transaction.guard();
 let directory=open_current_factory_order_directory_v1(accepted_orders,&orders,state,ctx,&mut work.scan,&mut work.read,&mut work.orders,&guard,live).map_err(infra)?;
 let owner_sources=CurrentStateOwnerSourcesV1{roots,orders:&directory,agents:sources.raw().agents(),native:sources.native().native()};
 let guard=ctx.transaction.guard();
 let alliance_proof=open_current_diplomacy_relation_record_v1(DiplomacyFieldV1::Alliances,id,sources.native().diplomacy(),roots,ctx.transaction,parent,&mut work.diplomacy_index,&guard,live).map_err(infra)?;
 let alliance=if let Some(proof)=alliance_proof.as_ref(){
 let formed_by=text(id,DiplomacyFieldV1::Alliances,DiplomacyScalarV1::Alliance(AllianceScalarV1::FormedByAgentId),proof,state,ctx,work.max_text_bytes,tail).map_err(infra)?.ok_or_else(||invalid("alliance founder absent"))?;
 let length=member_length(id,DiplomacyFieldV1::Alliances,proof,state,ctx,tail).map_err(infra)?;
 let count=usize::try_from(length).map_err(|_|invalid("alliance member count"))?;
 allocations.admit(size_of::<OrganizationCurrentAllianceFactV1>().checked_add(id.len()).and_then(|n|n.checked_add(formed_by.len())).and_then(|n|n.checked_add(count.checked_mul(size_of::<String>())?)).ok_or_else(||invalid("alliance facts allocation"))?,tail)?;
 let mut members=Vec::with_capacity(count);if members.capacity()>count{return Err(invalid("alliance facts capacity"))}
 for ordinal in 0..length {let member=member_text(id,DiplomacyFieldV1::Alliances,ordinal,DiplomacyMemberColumnV1::AllianceMember,proof,state,ctx,work.max_text_bytes,tail).map_err(infra)?.ok_or_else(||invalid("alliance member absent"))?;allocations.admit(member.len(),tail)?;members.push(member.as_str().to_owned());}
 tail.charge(formed_by.len())?;
 Some(OrganizationCurrentAllianceFactV1{organization_id:id.clone(),formed_by_agent_id:formed_by.as_str().to_owned(),members})
 }else{None};
 let mut agent_ids=BTreeSet::new();
 insert_agent_id(&mut agent_ids,&registration.founder_id,&mut allocations,tail)?;
 for id in registration.controller_bindings.keys().chain(registration.membership_controller_bindings.keys()){insert_agent_id(&mut agent_ids,id,&mut allocations,tail)?;}
 if let Some(alliance)=&alliance{insert_agent_id(&mut agent_ids,&alliance.formed_by_agent_id,&mut allocations,tail)?;for id in &alliance.members{insert_agent_id(&mut agent_ids,id,&mut allocations,tail)?;}}
 let mut contracts=BTreeMap::new();
 for source in registration.activity_sources.values(){
 let guard=ctx.transaction.guard();
 let proof=open_current_diplomacy_contract_record_v1(&source.contract_id,sources.native().diplomacy(),roots,ctx.transaction,parent,&mut work.scan,&mut work.cells_remaining,work.max_text_bytes as u64,&guard,live).map_err(infra)?;
 if let Some(proof)=proof {let contract=super::continuity_contract::read_contract_v1(&source.contract_id,&proof,state,ctx,work.max_text_bytes,tail).map_err(infra)?;
 insert_agent_id(&mut agent_ids,&contract.creator_agent_id,&mut allocations,tail)?;insert_agent_id(&mut agent_ids,&contract.counterparty_agent_id,&mut allocations,tail)?;
 allocations.admit(super::admission::add(super::admission::tree_entry::<String,OrganizationCurrentContractFactV1>()?,super::admission::add(source.contract_id.len(),contract.reserved_bytes())?)?,tail)?;
 contracts.insert(source.contract_id.clone(),OrganizationCurrentContractFactV1{contract:(*contract).clone()});}
 }
 let mut estate=BTreeMap::new();let mut requests=BTreeMap::new();let mut requests_by_outer=BTreeMap::new();
 if matches!(scope,LifecycleFactScopeV1::Lifecycle) {
 let guard=ctx.transaction.guard();
 let risk=observe_current_organization_risk_v1(id,sources.native().rules().organization(),roots,ctx.transaction,parent,&mut work.scan,&mut work.cells_remaining,&mut work.pending_entries_remaining,work.max_text_bytes as u64,&guard,live).map_err(infra)?;
 let metadata_memory=tail.memory().clone();let mut count=0usize;let mut text_bytes=0usize;
 risk.visit_current_estate_outers_v1(id,&metadata_memory,|s,b|tail.debit(s,b).map_err(|_|ValueTreeError::ResourceLimited("organization estate metadata")),|id|{count=count.checked_add(1).ok_or(ValueTreeError::ResourceLimited("estate count"))?;text_bytes=text_bytes.checked_add(id.len()).ok_or(ValueTreeError::ResourceLimited("estate IDs"))?;Ok(())}).map_err(infra)?;
 allocations.admit(count.checked_mul(size_of::<String>()).and_then(|n|n.checked_add(text_bytes)).ok_or_else(||invalid("estate outers allocation"))?,tail)?;
 let mut outers=Vec::with_capacity(count);if outers.capacity()>count{return Err(invalid("estate outers capacity"))}
 risk.visit_current_estate_outers_v1(id,&metadata_memory,|s,b|tail.debit(s,b).map_err(|_|ValueTreeError::ResourceLimited("organization estate metadata")),|id|{outers.push(id.to_owned());Ok(())}).map_err(infra)?;
 for outer in outers {
 // Other selected reads may extend the read-set. Reopen the real risk
 // selection for this row; never refresh an older source owner's guard.
 let guard=ctx.transaction.guard();
 let risk=observe_current_organization_risk_v1(id,sources.native().rules().organization(),roots,ctx.transaction,parent,&mut work.scan,&mut work.cells_remaining,&mut work.pending_entries_remaining,work.max_text_bytes as u64,&guard,live).map_err(infra)?;
 let guard=ctx.transaction.guard();
 let deposit=read_current_organization_estate_v1(&outer,id,sources.native().rules().organization(),&risk,roots,state,ctx,&mut work.scan,&mut work.cells_remaining,work.max_text_bytes as u64,&guard,live).map_err(infra)?.ok_or_else(||invalid("indexed estate absent"))?;
 insert_agent_id(&mut agent_ids,&deposit.deposit().contributor_id,&mut allocations,tail)?;
 let guard=ctx.transaction.guard();
 let selected=observe_current_requests_result_sources_v1(&deposit.deposit().id,sources.native().rules().requests(),roots,state,ctx,&mut work.scan,&mut work.read,&guard,live).map_err(infra)?;
 for source in selected.requests(){let guard=ctx.transaction.guard();let receipt=read_current_requests_result_receipt_v1(&source,sources.native().rules().requests(),roots,state,ctx,&mut work.scan,&mut work.read,&guard,live).map_err(infra)?;
 let value=receipt.value();
 if value.result_id!=deposit.deposit().id{return Err(invalid("organization result index mismatch"))}
 let receipt_bytes=receipt.retained_allocation_bytes_v1(&mut work.cells_remaining,&mut ctx.cancelled)?;
 allocations.admit(super::admission::add(super::admission::tree_entry::<String,WorldRuleReceiptV1>()?,super::admission::add(receipt_bytes,source.outer_key().len())?)?,tail)?;
 let outer=source.outer_key().to_owned();if outer.capacity()>source.outer_key().len(){return Err(invalid("organization receipt outer capacity"))}
 requests_by_outer.insert(outer,value.clone());}
 let value=deposit.deposit();
 let deposit_bytes=value.id.len().checked_mul(2).and_then(|n|n.checked_add(value.organization_id.len())).and_then(|n|n.checked_add(value.contributor_id.len())).and_then(|n|n.checked_add(value.material.len())).and_then(|n|n.checked_add(value.effect_receipt.as_ref().map_or(0,String::len))).ok_or_else(||invalid("estate copy allocation"))?;
 allocations.admit(super::admission::add(super::admission::tree_entry::<String,OrganizationCurrentEstateFactV1>()?,deposit_bytes)?,tail)?;
 estate.insert(deposit.deposit().id.clone(),OrganizationCurrentEstateFactV1{deposit:deposit.deposit().clone()});
 }

 }
 for receipt in requests_by_outer.into_values(){allocations.admit(super::admission::add(super::admission::tree_entry::<String,OrganizationCurrentRequestFactV1>()?,receipt.request_id.len())?,tail)?;requests.insert(receipt.request_id.clone(),OrganizationCurrentRequestFactV1{receipt});}
 let mut agents=BTreeMap::new();
 for agent in agent_ids {
 let guard=ctx.transaction.guard();
 let present=oasis7_world_state::incremental_agent_subtree::read_current_agent_presence_v1(&agent,sources.raw().agents(),roots,ctx,parent,&mut work.pending_entries_remaining,&guard,live).map_err(infra)?==oasis7_world_state::incremental_agent_subtree::CurrentAgentPresenceV1::Existing;
 let owner=if present {let guard=ctx.transaction.guard();let owner=resolve_current_state_owner_v1(&owner_sources,&agent,ctx,parent,&mut work.eligibility,&guard,live).map_err(infra)?;if let Some(value)=owner.as_ref(){allocations.admit(value.primary_agent_id().len().checked_add(value.player_id().len()).and_then(|n|n.checked_add(value.source_id().len())).ok_or_else(||invalid("controller copy allocation"))?,tail)?;}owner.as_ref().filter(|owner|!owner.primary_agent_id().is_empty()&&!owner.player_id().is_empty()&&!owner.source_id().is_empty()&&owner.generation()>0).map(|owner|OrganizationControllerBindingV1{root_agent_id:owner.primary_agent_id().to_owned(),player_id:owner.player_id().to_owned(),source_id:owner.source_id().to_owned(),generation:owner.generation()})}else{None};
 allocations.admit(super::admission::add(super::admission::tree_entry::<String,OrganizationCurrentAgentFactV1>()?,agent.len())?,tail)?;
 agents.insert(agent.clone(),OrganizationCurrentAgentFactV1{agent_id:agent,present,owner});
 }
 Ok(CurrentOrganizationFactsV1{value:OrganizationCurrentLifecycleFactsV1{organization_id:id.clone(),alliance,agents,contracts,estate,requests},guard:ctx.transaction.guard(),scope,_allocations:allocations})
}

fn insert_agent_id(ids:&mut BTreeSet<String>,id:&str,allocations:&mut super::admission::AdmissionV1,tail:&mut NormalTailWorkV1<'_,'_>)->Result<(),BodyMutationError>{
 for existing in ids.iter(){tail.charge(existing.len().min(id.len()))?;if existing==id{return Ok(())}}
 allocations.admit(super::admission::add(super::admission::tree_entry::<String,()>()?,id.len())?,tail)?;ids.insert(id.to_owned());Ok(())
}
