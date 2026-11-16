//! Original issuer, parent-chain and full historical finality rules over real
//! selected column records. No JSON field supplies an authorization verdict.
use super::*;
use admission::FactsV1;
use crate::governance::{GovernanceFinalityCertificate,GovernanceFinalityEpochSnapshot};
use crate::world::industrial_control::readonly_runtime_owner::{current_agent_identity_readonly_v1,current_agent_owner_readonly_v1};
use std::collections::BTreeSet;
struct Issuer<'p,'v,'f,'a,'s,'c,'k,M>(&'p Provider<'v,'f,'a,'s,'c,'k,M>);
impl<M>admission::sealed::Sealed for Issuer<'_, '_, '_, '_, '_, '_, '_,M>{}
impl<M:MerkleNodeReader>Provider<'_, '_, '_, '_, '_, '_,M>{
 fn native_map<T:serde::de::DeserializeOwned>(&self,field:&str)->Result<BTreeMap<String,T>,BodyMutationError>{let mut out=BTreeMap::new();self.view.facts.visit_rows(field,|row|{let key=row.original_key_value().as_str().ok_or_else(||invalid("provider native outer text key"))?;let key=self.view.copy(key)?;let value=self.view.decode_original(self.view.facts.raw(row)?)?;self.view.facts.before_owned(11*(size_of::<String>()+size_of::<T>())+256)?;self.view.facts.work(1+11*(out.len()+1).ilog2()as u64,key.len())?;out.insert(key,value);Ok(std::ops::ControlFlow::Continue(()))})?;Ok(out)}
 fn revoked_data(&self,id:&str)->Result<bool,BodyMutationError>{let row=self.view.required(&["capability_revocation_state","revoked_grant_ids"])?;let memory=self.view.facts.common.borrow().memory().clone();row.share_data(&memory,&mut|s,b|self.view.facts.common.borrow_mut().charge(s,b).map_err(paid)).map_err(|e|match e{ProjectionColumnErrorV1::Work(e)=>e,ProjectionColumnErrorV1::Memory(e)=>ValueTreeError::Memory(e).into(),ProjectionColumnErrorV1::Invalid(e)=>invalid(e)})?.array_text_contains_data(id,&memory,&mut|s,b|self.view.facts.common.borrow_mut().charge(s,b).map_err(paid)).map_err(|e|match e{ProjectionColumnErrorV1::Work(e)=>e,ProjectionColumnErrorV1::Memory(e)=>ValueTreeError::Memory(e).into(),ProjectionColumnErrorV1::Invalid(e)=>invalid(e)})}
 fn superseded_data(&self,id:&str)->Result<bool,BodyMutationError>{let row=self.view.required(&["capability_revocation_state","superseded_by"])?;Ok(self.view.member_like_json_get(&row,id)?.is_some())}
 fn certificate_data(&self,certificate:&GovernanceFinalityCertificate)->Result<(),Error>{
  let mut source=Issuer(self);
  let selected:Option<GovernanceFinalityEpochSnapshot>=self.selected(&["governance_finality_epoch_snapshots"],&self.note(format_args!("{}",certificate.epoch_id))?)?;
  source.record(certificate)?;
  if let Some(snapshot)=&selected{source.record(snapshot)?;crate::world::governance::module_admission_finality::ModuleFinalityRecordsV1::validate_certificate_header_v1(snapshot,certificate.proposal_id,&certificate.manifest_hash,certificate.epoch_id,certificate)?;}
  let registry=self.scalar("governance_finality_signer_registry")?;
  let admissions=self.native_map("governance_validator_admissions")?;
  let rotations=self.native_map("governance_finality_key_rotations")?;
  let bootstrap_rotations=self.native_map("governance_bootstrap_finality_key_rotations")?;
  let n=source.record(&(&registry,&admissions,&rotations,&bootstrap_rotations))?;
  let count=admissions.len().checked_add(rotations.len()).and_then(|n|n.checked_add(bootstrap_rotations.len())).and_then(|n|n.checked_add(certificate.signatures.len()+1)).ok_or_else(||invalid("provider finality graph visits"))?;
  let cycles=count.checked_mul(count).and_then(|count|count.checked_mul(n.checked_add(1)?)).and_then(|n|n.checked_mul(512)).ok_or_else(||invalid("provider finality visits"))?;
  // Match the original Current finality source's actual-record based phase.
  source.work(cycles)?;
  let policy:crate::governance::GovernanceExecutionPolicy=self.view.typed(&["governance_execution_policy"])?;
  let records=crate::world::governance::module_admission_finality::ModuleFinalityRecordsV1{registry:&registry,admissions:&admissions,rotations:&rotations,bootstrap_rotations:&bootstrap_rotations,epoch_length:policy.epoch_length_ticks};
  let fallback;
  let snapshot=match &selected{Some(value)=>value,None=>{fallback=records.fallback_epoch_snapshot_v1(certificate.epoch_id,self.view.facts.clock/policy.epoch_length_ticks.max(1));source.record(&fallback)?;&fallback}};
  records.validate_certificate_from_source_bound_v1(snapshot,certificate.proposal_id,&certificate.manifest_hash,certificate.epoch_id,certificate,&mut source)
 }
 fn verify_issuer_data(&self,g:&CapabilityGrantV2)->Result<(),Error>{admission::issuer(&mut Issuer(self),g)}
 fn verify_revocation_data(&self,g:&CapabilityGrantV2)->Result<(),Error>{admission::revocation(&mut Issuer(self),g)}
 fn verify_parents_data(&self,g:&CapabilityGrantV2,tick:u64)->Result<(),Error>{admission::parents(&mut Issuer(self),g,tick,&mut BTreeSet::new())}
}
impl<'g,M:MerkleNodeReader>FactsV1<'g>for Issuer<'_, '_, '_, '_, '_, '_, '_,M>{
 type Error=Error;
 fn tick(&mut self)->Result<u64,Error>{Ok(self.0.view.facts.clock)}
 fn grant(&mut self,id:&str)->Result<Option<admission::GrantDataV1<'g>>,Error>{match self.0.grants_data()?.get(id){None=>Ok(None),Some(value)=>{self.0.view.before_clone(value)?;Ok(Some(admission::GrantDataV1::Current(self.0.retain(value.clone())?)))}}}
 fn grant_exists(&mut self,id:&str)->Result<bool,Error>{Ok(self.0.grants_data()?.contains_key(id))}
 fn root(&mut self)->Result<(),Error>{self.0.view.facts.verify_authorization_root_data_v1()?;Ok(())}
 fn legacy(&mut self,id:&str)->Result<(),Error>{let grant:Option<oasis7_world_state::CapabilityGrant>=self.0.native("capabilities",id)?;let Some(grant)=grant else{return Err(crate::WorldError::ModuleChangeInvalid{reason:self.0.note(format_args!("module cap missing {id}"))?}.into())};if grant.is_expired(self.0.view.facts.clock){return Err(crate::WorldError::ModuleChangeInvalid{reason:self.0.note(format_args!("module cap expired {id}"))?}.into())}Ok(())}
 fn authority(&mut self,id:&str)->Result<Option<admission::AuthorityDataV1<'g>>,Error>{self.0.selected(&["capability_revocation_state","authority_records"],id)?.map(|v|self.0.retain(v).map(admission::AuthorityDataV1::Projection)).transpose().map_err(Into::into)}
 fn proof(&mut self,id:&str)->Result<Option<admission::ProofDataV1<'g>>,Error>{self.0.selected(&["capability_revocation_state","authority_finality_proofs"],id)?.map(|v|self.0.retain(v).map(admission::ProofDataV1::Projection)).transpose().map_err(Into::into)}
 fn epoch(&mut self)->Result<u64,Error>{Ok(self.0.view.typed(&["capability_revocation_state","epoch"])?) }
 fn revoked(&mut self,id:&str)->Result<bool,Error>{Ok(self.0.revoked_data(id)?)}
 fn superseded(&mut self,id:&str)->Result<bool,Error>{Ok(self.0.superseded_data(id)?)}
 fn node(&mut self,id:&str)->Result<Option<String>,Error>{Ok(self.0.native("node_identity_bindings",id)?)}
 fn snapshot_exists(&mut self,epoch:u64)->Result<bool,Error>{let rows=self.0.view.required(&["governance_finality_epoch_snapshots"])?;let key=self.0.note(format_args!("{epoch}"))?;Ok(self.0.view.member_like_json_get(&rows,&key)?.is_some())}
 fn proposal_hash(&mut self,id:u64)->Result<Option<Result<String,()>>,Error>{let rows=self.0.view.required(&["proposal_statuses"])?;let key=self.0.note(format_args!("{id}"))?;let Some(row)=self.0.view.member_like_json_get(&rows,&key)?else{return Ok(None)};if self.0.view.is_null(&row)?{return Ok(Some(Err(())))}let hash=row.text_data().ok_or_else(||invalid("actual proposal status column type"))?;Ok(Some(Ok(self.0.view.copy(hash)?)))}
 fn certificate(&mut self,c:&GovernanceFinalityCertificate)->Result<(),Error>{self.0.certificate_data(c).map_err(|e|match e{Error::World(e)=>match self.0.note(format_args!("authority finality certificate: {e:?}")){Ok(note)=>Error::World(deny(note)),Err(e)=>Error::Body(e)},Error::PaidWorld(e)=>Error::PaidWorld(e),Error::Body(e)=>Error::Body(e)})}
 fn work(&mut self,n:usize)->Result<(),Error>{self.0.view.facts.work(1+n as u64,n)?;Ok(())}
 fn record<T:serde::Serialize>(&mut self,v:&T)->Result<usize,Error>{let raw={let mut common=self.0.view.facts.common.borrow_mut();crate::world::incremental_normal_projection_commitment::capture_projection_json_data_v1(v,&mut common)?};let n=raw.len();self.0.view.before_clone(v)?;Ok(n)}
 fn scope_contains(&mut self,p:&oasis7_wasm_abi::CapabilityScope,c:&oasis7_wasm_abi::CapabilityScope)->Result<bool,Error>{Ok(p.contains_subset_with_fallible_work_v1(c,&mut|s,b|self.0.view.facts.common.borrow_mut().charge(s,b).map_err(paid))?)}
 fn validate_grant(&mut self,g:&CapabilityGrantV2)->Result<(),Error>{let result=g.validate_with_fallible_work_v1(&mut|s,b|self.0.view.facts.common.borrow_mut().charge(s,b).map_err(paid))?;match result{Ok(())=>Ok(()),Err(e)=>Err(deny(self.0.note(format_args!("parent grant validation: {e}"))?).into())}}
 fn grant_body_matches(&mut self,g:&CapabilityGrantV2)->Result<bool,Error>{let hash=oasis7_wasm_abi::canonical_hash_with_work_v1(&g.canonical_body_input_v1(),self.0);self.0.check()?;let hash=hash.map_err(|e|Error::Body(paid(e)))?;Ok(hash.as_str()==g.canonical_body_hash)}
 fn grant_id_matches(&mut self,g:&CapabilityGrantV2)->Result<bool,Error>{let hash=oasis7_wasm_abi::canonical_hash_with_work_v1(&g.canonical_body_input_v1(),self.0);self.0.check()?;let hash=hash.map_err(|e|Error::Body(paid(e)))?;Ok(hash.as_str()==g.grant_id)}
}
impl<M:MerkleNodeReader>catalog::CapabilityCatalogSourcesV1 for Provider<'_, '_, '_, '_, '_, '_,M>{
 type Error=Error;
 fn agent_live(&self,id:&str)->Result<bool,Error>{Ok(self.view.facts.owner_agent_present_data(id)?)}
 fn identity(&self,id:&str)->Result<Option<crate::CapabilityAgentIdentity>,Error>{Ok(current_agent_identity_readonly_v1(&self.view.owner_source()?,id)?)}
 fn grants(&self)->Result<&BTreeMap<String,serde_json::Value>,Error>{Ok(self.grants_data()?)}
 fn registry(&self)->Result<&crate::modules::ModuleRegistry,Error>{Ok(self.registry_data()?)}
 fn policies(&self)->Result<&oasis7_world_state::policy::PolicySet,Error>{Ok(self.policies_data()?)}
 fn tick(&self)->u64{self.view.facts.clock}
 fn world_journal_head(&self)->Result<u64,Error>{
  let row=self.view.required(&["world_journal"])?;let memory=self.view.facts.common.borrow().memory().clone();let mut pay=|s,b|self.view.facts.common.borrow_mut().charge(s,b).map_err(paid);
  fn graph(e:ProjectionColumnErrorV1<BodyMutationError>)->BodyMutationError{match e{ProjectionColumnErrorV1::Work(e)=>e,ProjectionColumnErrorV1::Memory(e)=>ValueTreeError::Memory(e).into(),ProjectionColumnErrorV1::Invalid(e)=>invalid(e)}}
  let journal=row.share_data(&memory,&mut pay).map_err(graph)?;let len=journal.array_len_data_v1(&memory,&mut pay).map_err(graph)?;if len==0{return Ok(0)}
  use oasis7_world_state::observation_projection::current_commitment::ProjectionDataPathRefV1 as Path;
  let selected=journal.select_path_data_v1(&[Path::Index(len-1),Path::Field("id")],&memory,&mut pay).map_err(graph)?.ok_or_else(||invalid("actual retained journal last id"))?;
  Ok(self.view.decode(&selected.root_data())?.map_err(|_|invalid("actual retained journal id type"))?)
 }
 fn revocation_epoch(&self)->Result<u64,Error>{Ok(self.view.typed(&["capability_revocation_state","epoch"])?) }
 fn verify_issuer(&self,g:&CapabilityGrantV2)->Result<(),Error>{self.verify_issuer_data(g)}
 fn verify_live_revocation(&self,g:&CapabilityGrantV2)->Result<(),Error>{self.verify_revocation_data(g)}
 fn verify_parent_chain(&self,g:&CapabilityGrantV2)->Result<(),Error>{self.verify_parents_data(g,self.view.facts.clock)}
 fn verify_audience(&self,g:&CapabilityGrantV2)->Result<(),Error>{crate::world::capability_catalog::audience::validate_from_sources_v1(self,g)}
}
impl<M:MerkleNodeReader>terminal::sealed::Sources for Provider<'_, '_, '_, '_, '_, '_,M>{}
impl<M:MerkleNodeReader>terminal::ProviderTerminalSourcesV1 for Provider<'_, '_, '_, '_, '_, '_,M>{
 type Error=Error;type LiveContextError=String;
 fn production_hardened(&self)->bool{self.production_hardened}
 fn allow_local_finality_signing(&self)->bool{self.allow_local}
 fn agent_live(&self,id:&str)->Result<bool,Error>{Ok(self.view.facts.owner_agent_present_data(id)?)}
 fn verify_authorization_root(&self)->Result<(),Error>{self.view.facts.verify_authorization_root_data_v1()?;Ok(())}
 fn identity(&self,id:&str)->Result<Option<crate::CapabilityAgentIdentity>,Error>{Ok(current_agent_identity_readonly_v1(&self.view.owner_source()?,id)?)}
 fn owner(&self,id:&str)->Result<Option<terminal::ProviderTerminalOwnerV1>,Error>{Ok(current_agent_owner_readonly_v1(&self.view.owner_source()?,id)?.map(terminal::ProviderTerminalOwnerV1::Legacy))}
 fn live_context(&self,id:&str)->Result<Result<CapabilityInvocationContext,String>,Error>{Ok(sources::canonical_from_sources_v1(self,id)?.map(|value|value.capability_invocation_context))}
 fn grants(&self)->Result<&BTreeMap<String,serde_json::Value>,Error>{Ok(self.grants_data()?)}
 fn contexts(&self)->Result<&BTreeMap<String,CapabilityInvocationContext>,Error>{Ok(self.contexts_data()?)}
 fn registry(&self)->Result<&crate::modules::ModuleRegistry,Error>{Ok(self.registry_data()?)}
 fn tick(&self)->u64{self.view.facts.clock}
 fn verify_issuer(&self,g:&CapabilityGrantV2)->Result<(),Error>{self.verify_issuer_data(g)}
 fn verify_live_revocation(&self,g:&CapabilityGrantV2)->Result<(),Error>{self.verify_revocation_data(g)}
 fn verify_parent_chain_at_tick(&self,g:&CapabilityGrantV2,t:u64)->Result<(),Error>{self.verify_parents_data(g,t)}
 fn verify_audience(&self,g:&CapabilityGrantV2)->Result<(),Error>{crate::world::capability_catalog::audience::validate_from_sources_v1(self,g)}
}
use crate::world::provider_terminal_authority::origin;
impl<M>origin::sealed::Sources for Provider<'_, '_, '_, '_, '_, '_,M>{}
impl<M:MerkleNodeReader>origin::LocalOriginSourcesV1 for Provider<'_, '_, '_, '_, '_, '_,M>{
 type Error=Error;
 fn marker_actor(&self)->Result<Option<&str>,Error>{Ok(self.view.optional(&["manifest","content","local_test_provider_authority","actor"])?.and_then(|v|v.text_data()))}
 fn contexts(&self)->Result<&BTreeMap<String,CapabilityInvocationContext>,Error>{Ok(self.contexts_data()?)}
 fn grants(&self)->Result<&BTreeMap<String,serde_json::Value>,Error>{Ok(self.grants_data()?)}
 fn work(&self,s:u64,b:usize)->Result<(),Error>{self.view.facts.work(s,b)?;Ok(())}
 fn before_value<T:serde::Serialize>(&self,v:&T)->Result<(),Error>{self.view.before_clone(v)?;Ok(())}
}
pub(super) fn local_origin<M:MerkleNodeReader>(source:&Provider<'_, '_, '_, '_, '_, '_,M>,agent:&str)->Result<bool,Error>{origin::has_local_origin_from_sources_v1(source,agent)}
use crate::world::capability_catalog::audience;
impl<M>audience::sealed::Sources for Provider<'_, '_, '_, '_, '_, '_,M>{}
impl<M:MerkleNodeReader>audience::SourcesV1 for Provider<'_, '_, '_, '_, '_, '_,M>{
 type Error=Error;
 fn instance<T>(&self,id:&str,visit:impl FnOnce(Option<&oasis7_world_state::ModuleInstanceState>)->Result<T,Error>)->Result<T,Error>{let row=self.native("module_instances",id)?;visit(row.as_ref())}
 fn authority<T>(&self,id:&str,visit:impl FnOnce(Option<&crate::capability_authorization::CapabilityAuthorityRecord>)->Result<T,Error>)->Result<T,Error>{let row=self.selected(&["capability_revocation_state","authority_records"],id)?;visit(row.as_ref())}
 fn chain_world<T>(&self,visit:impl FnOnce(&str)->Result<T,Error>)->Result<T,Error>{let row=self.view.required(&["chain_resource_manifest","world_id"])?;visit(row.text_data().ok_or_else(||invalid("actual chain world ID"))?)}
 fn binding<T>(&self,visit:impl FnOnce(Option<&serde_json::Value>)->Result<T,Error>)->Result<T,Error>{let value=match self.view.optional(&["cognition","runtime_binding"])?{Some(row)=>Some(self.view.decode(&row)?.map_err(|_|invalid("actual Runtime binding JSON"))?),None=>None};visit(value.as_ref())}
 fn finality(&self,epoch:u64)->Result<(),Error>{self.view.facts.verify_provider_audience_finality_v1(epoch).map_err(|e|match e{crate::world::incremental_normal_bootstrap_finality::CurrentBootstrapFinalityErrorV1::Source(e)=>Error::Body(e),crate::world::incremental_normal_bootstrap_finality::CurrentBootstrapFinalityErrorV1::World(e)=>Error::PaidWorld(e)})}
 fn work(&self,s:u64,b:usize)->Result<(),Error>{self.view.facts.work(s,b)?;Ok(())}
}
