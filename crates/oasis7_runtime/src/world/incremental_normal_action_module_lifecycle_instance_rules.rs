//! Unique install prelude. Governance preparation/publication deliberately
//! follows this decision, so a failed shadow preserves the original Proposed.
use super::registry_rules::{ModuleManifestRecordV1,ModuleTextRecordV1};
use crate::{ModuleChangeSet,ModuleActivation,ModuleManifest,RejectReason,WorldError};
use oasis7_agent_api::world_action::ResourceKind;
pub(in crate::world) mod sealed{pub trait Sealed{}}
pub(in crate::world) trait ModuleBusinessErrorV1:Sized{fn into_business(self)->Result<WorldError,Self>;}
impl ModuleBusinessErrorV1 for WorldError{fn into_business(self)->Result<WorldError,Self>{Ok(self)}}
impl ModuleBusinessErrorV1 for crate::world::incremental_normal_action_factory_schedule::FactoryScheduleReadErrorV1{
 fn into_business(self)->Result<WorldError,Self>{match self{Self::World(e)=>Ok(e),e=>Err(e)}}
}
pub(in crate::world) trait ModuleInstallFactsV1<'s>:sealed::Sealed{
 type Error:ModuleBusinessErrorV1;
 fn reserve_install(&mut self,manifest:&ModuleManifest,actor:&str)->Result<(),Self::Error>;
 fn frozen_admission(&mut self,manifest:&ModuleManifest)->Result<(),Self::Error>;
 fn agent_exists(&mut self,id:&str)->Result<bool,Self::Error>;
 fn owner(&mut self,hash:&str)->Result<Option<String>,Self::Error>;
 fn balance(&mut self,id:&str,kind:ResourceKind)->Result<i64,Self::Error>;
 fn registry_manifest(&mut self,key:&str)->Result<Option<ModuleManifestRecordV1<'s>>,Self::Error>;
 fn active_version(&mut self,id:&str)->Result<Option<ModuleTextRecordV1<'s>>,Self::Error>;
}
pub(in crate::world) enum ModuleInstallPreludeV1{
 Rejected(RejectReason),Plan{changes:ModuleChangeSet,fee_kind:ResourceKind,fee_amount:i64},
}
pub(in crate::world) fn prepare_module_install_prelude_v1<'s,S:ModuleInstallFactsV1<'s>>(source:&mut S,actor:&str,manifest:&ModuleManifest,activate:bool)->Result<ModuleInstallPreludeV1,S::Error>{
 source.reserve_install(manifest,actor)?;
 if let Err(error)=source.frozen_admission(manifest){match error.into_business(){
  Ok(error)=>return Ok(ModuleInstallPreludeV1::Rejected(RejectReason::RuleDenied{notes:vec![format!("{error:?}")]})),Err(error)=>return Err(error),
 }}
 if !source.agent_exists(actor)?{return Ok(ModuleInstallPreludeV1::Rejected(RejectReason::AgentNotFound{agent_id:actor.to_owned()}))}
 if let Some(owner)=source.owner(&manifest.wasm_hash)? {
  if owner!=actor{return Ok(ModuleInstallPreludeV1::Rejected(RejectReason::RuleDenied{notes:vec![format!("install module artifact rejected: installer {} does not own {} (owner {})",actor,manifest.wasm_hash,owner)]}))}
 }
 let fee_kind=ResourceKind::Electricity;
 let fee_amount=crate::world::World::module_install_fee_amount(manifest);
 let available=source.balance(actor,fee_kind)?;
 if available<fee_amount{return Ok(ModuleInstallPreludeV1::Rejected(RejectReason::InsufficientResource{agent_id:actor.to_owned(),kind:fee_kind,requested:fee_amount,available}))}
 let mut changes=ModuleChangeSet::default();
 let key=crate::ModuleRegistry::record_key(&manifest.module_id,&manifest.version);
 if let Some(record)=source.registry_manifest(&key)? {
  if record.manifest()!=manifest{return Ok(ModuleInstallPreludeV1::Rejected(RejectReason::RuleDenied{notes:vec![format!("install module rejected: existing manifest mismatch for {}",key)]}))}
 }else{changes.register.push(manifest.clone())}
 if activate {
  let already=source.active_version(&manifest.module_id)?.is_some_and(|v|v.as_str()==manifest.version);
  if !already{changes.activate.push(ModuleActivation{module_id:manifest.module_id.clone(),version:manifest.version.clone()});}
 }
 Ok(ModuleInstallPreludeV1::Plan{changes,fee_kind,fee_amount})
}

pub(in crate::world) struct LegacyModuleInstallFactsV1<'s>(&'s crate::world::World);
impl<'s> LegacyModuleInstallFactsV1<'s>{pub(in crate::world) fn new(world:&'s crate::world::World)->Self{Self(world)}}
impl sealed::Sealed for LegacyModuleInstallFactsV1<'_>{}
impl<'s> ModuleInstallFactsV1<'s> for LegacyModuleInstallFactsV1<'s>{
 type Error=WorldError;
 fn reserve_install(&mut self,_:&ModuleManifest,_:&str)->Result<(),WorldError>{Ok(())}
 fn frozen_admission(&mut self,m:&ModuleManifest)->Result<(),WorldError>{self.0.ensure_module_admission_allowed(m)}
 fn agent_exists(&mut self,id:&str)->Result<bool,WorldError>{Ok(self.0.state.agents.contains_key(id))}
 fn owner(&mut self,h:&str)->Result<Option<String>,WorldError>{Ok(self.0.state.module_artifact_owners.get(h).cloned())}
 fn balance(&mut self,id:&str,k:ResourceKind)->Result<i64,WorldError>{Ok(self.0.state.agents.get(id).map(|a|a.state.resources.get(k)).unwrap_or(0))}
 fn registry_manifest(&mut self,key:&str)->Result<Option<ModuleManifestRecordV1<'s>>,WorldError>{Ok(self.0.module_registry.records.get(key).map(|r|ModuleManifestRecordV1::Legacy(&r.manifest)))}
 fn active_version(&mut self,id:&str)->Result<Option<ModuleTextRecordV1<'s>>,WorldError>{Ok(self.0.module_registry.active.get(id).map(|v|ModuleTextRecordV1::Legacy(v)))}
}
