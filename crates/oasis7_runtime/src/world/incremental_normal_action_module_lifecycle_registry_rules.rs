//! Unique module change-set ordering shared by legacy World and fixed Current.
use super::*;
use crate::{ModuleChangeSet,ModuleRegistry,WorldError};
use std::collections::BTreeSet;
pub(in crate::world) mod sealed{pub trait Sealed{}}
pub(in crate::world) enum ModuleManifestRecordV1<'a>{Legacy(&'a oasis7_wasm_abi::ModuleManifest),Current(ReservedValue<oasis7_world_state::incremental_runtime_artifact_config::SelectedModuleManifestV1>)}
impl ModuleManifestRecordV1<'_>{pub(in crate::world) fn manifest(&self)->&oasis7_wasm_abi::ModuleManifest{match self{Self::Legacy(v)=>v,Self::Current(v)=>v.manifest()}}}
pub(in crate::world) enum ModuleTextRecordV1<'a>{Legacy(&'a str),Current(ReservedValue<String>)}
impl ModuleTextRecordV1<'_>{pub(in crate::world) fn as_str(&self)->&str{match self{Self::Legacy(s)=>s,Self::Current(s)=>s}}}
pub(in crate::world) enum ModuleRegistryRuleErrorV1<E>{World(WorldError),Source(E)}
impl<E> From<WorldError> for ModuleRegistryRuleErrorV1<E>{fn from(e:WorldError)->Self{Self::World(e)}}
pub(in crate::world) trait ModuleRegistryFactsV1<'s>:sealed::Sealed{
 type Error;
 fn reserve_changes(&mut self,changes:&ModuleChangeSet)->Result<(),Self::Error>;
 fn admission(&mut self,manifest:&oasis7_wasm_abi::ModuleManifest)->Result<(),Self::Error>;
 fn storage_activation(&mut self,id:&str,version:&str)->Result<(),Self::Error>;
 fn record_exists(&mut self,key:&str)->Result<bool,Self::Error>;
 fn manifest(&mut self,key:&str)->Result<Option<ModuleManifestRecordV1<'s>>,Self::Error>;
 fn active_version(&mut self,id:&str)->Result<Option<ModuleTextRecordV1<'s>>,Self::Error>;
 fn gameplay_conflicts(&mut self,changes:&ModuleChangeSet)->Result<(),Self::Error>;
}
pub(in crate::world) fn validate_module_changes_from_source_v1<'s,S:ModuleRegistryFactsV1<'s>>(source:&mut S,changes:&ModuleChangeSet)->Result<(),ModuleRegistryRuleErrorV1<S::Error>>{
        source.reserve_changes(changes).map_err(ModuleRegistryRuleErrorV1::Source)?;
        let mut register_ids = BTreeSet::new();
        let mut activate_ids = BTreeSet::new();
        let mut deactivate_ids = BTreeSet::new();
        let mut upgrade_ids = BTreeSet::new();

        for module in &changes.register {
            source.admission(module).map_err(ModuleRegistryRuleErrorV1::Source)?;
            if !register_ids.insert(module.module_id.clone()) {
                return Err(WorldError::ModuleChangeInvalid {
                    reason: format!("duplicate register module_id {}", module.module_id),
                }.into());
            }
        }

        for activation in &changes.activate {
            source.storage_activation(&activation.module_id, &activation.version).map_err(ModuleRegistryRuleErrorV1::Source)?;
            if !activate_ids.insert(activation.module_id.clone()) {
                return Err(WorldError::ModuleChangeInvalid {
                    reason: format!("duplicate activate module_id {}", activation.module_id),
                }.into());
            }
        }

        for deactivation in &changes.deactivate {
            if !deactivate_ids.insert(deactivation.module_id.clone()) {
                return Err(WorldError::ModuleChangeInvalid {
                    reason: format!("duplicate deactivate module_id {}", deactivation.module_id),
                }.into());
            }
        }

        for upgrade in &changes.upgrade {
            source.storage_activation(&upgrade.module_id, &upgrade.to_version).map_err(ModuleRegistryRuleErrorV1::Source)?;
            if upgrade.module_id != upgrade.manifest.module_id {
                return Err(WorldError::ModuleChangeInvalid {
                    reason: format!("upgrade manifest module_id mismatch {}", upgrade.module_id),
                }.into());
            }
            if upgrade.to_version != upgrade.manifest.version {
                return Err(WorldError::ModuleChangeInvalid {
                    reason: format!(
                        "upgrade target version mismatch for {}: change_set={} manifest={}",
                        upgrade.module_id, upgrade.to_version, upgrade.manifest.version
                    ),
                }.into());
            }
            if upgrade.wasm_hash != upgrade.manifest.wasm_hash {
                return Err(WorldError::ModuleChangeInvalid {
                    reason: format!(
                        "upgrade wasm_hash mismatch for {}: change_set={} manifest={}",
                        upgrade.module_id, upgrade.wasm_hash, upgrade.manifest.wasm_hash
                    ),
                }.into());
            }
            source.admission(&upgrade.manifest).map_err(ModuleRegistryRuleErrorV1::Source)?;
            if !upgrade_ids.insert(upgrade.module_id.clone()) {
                return Err(WorldError::ModuleChangeInvalid {
                    reason: format!("duplicate upgrade module_id {}", upgrade.module_id),
                }.into());
            }
        }

        let mut planned_records = BTreeSet::new();
        for module in &changes.register {
            let key = ModuleRegistry::record_key(&module.module_id, &module.version);
            if source.record_exists(&key).map_err(ModuleRegistryRuleErrorV1::Source)?
                || !planned_records.insert(key.clone())
            {
                return Err(WorldError::ModuleChangeInvalid {
                    reason: format!("module version already registered {key}"),
                }.into());
            }
        }

        for upgrade in &changes.upgrade {
            let to_key = ModuleRegistry::record_key(&upgrade.module_id, &upgrade.to_version);
            if source.record_exists(&to_key).map_err(ModuleRegistryRuleErrorV1::Source)? {
                return Err(WorldError::ModuleChangeInvalid {
                    reason: format!("module version already registered {to_key}"),
                }.into());
            }
            if !planned_records.insert(to_key.clone()) {
                return Err(WorldError::ModuleChangeInvalid {
                    reason: format!("module version already registered {to_key}"),
                }.into());
            }

            let from_key = ModuleRegistry::record_key(&upgrade.module_id, &upgrade.from_version);
            if !source.record_exists(&from_key).map_err(ModuleRegistryRuleErrorV1::Source)? {
                return Err(WorldError::ModuleChangeInvalid {
                    reason: format!("upgrade source missing {from_key}"),
                }.into());
            }

            if let Some(active_version) = source.active_version(&upgrade.module_id).map_err(ModuleRegistryRuleErrorV1::Source)?
                && active_version.as_str() != upgrade.from_version.as_str()
            {
                return Err(WorldError::ModuleChangeInvalid {
                    reason: format!(
                        "upgrade source version mismatch for {} (active {})",
                        upgrade.module_id, active_version
                    ),
                }.into());
            }
        }

        for activation in &changes.activate {
            let key = ModuleRegistry::record_key(&activation.module_id, &activation.version);
            if let Some(manifest)=changes.register.iter().find(|m|m.module_id==activation.module_id&&m.version==activation.version)
                .or_else(||changes.upgrade.iter().map(|u|&u.manifest).find(|m|m.module_id==activation.module_id&&m.version==activation.version)) {
                source.admission(manifest).map_err(ModuleRegistryRuleErrorV1::Source)?;
            }else if let Some(record)=source.manifest(&key).map_err(ModuleRegistryRuleErrorV1::Source)? {
                source.admission(record.manifest()).map_err(ModuleRegistryRuleErrorV1::Source)?;
            }
            let exists =
                source.record_exists(&key).map_err(ModuleRegistryRuleErrorV1::Source)? || planned_records.contains(&key);
            if !exists {
                return Err(WorldError::ModuleChangeInvalid {
                    reason: format!("activate target missing {key}"),
                }.into());
            }
        }

        let mut will_activate = BTreeSet::new();
        for activation in &changes.activate {
            will_activate.insert(activation.module_id.clone());
        }
        for deactivation in &changes.deactivate {
            let has_active = source.active_version(&deactivation.module_id).map_err(ModuleRegistryRuleErrorV1::Source)?.is_some();
            if !has_active && !will_activate.contains(&deactivation.module_id) {
                return Err(WorldError::ModuleChangeInvalid {
                    reason: format!("deactivate target not active {}", deactivation.module_id),
                }.into());
            }
        }

        source.gameplay_conflicts(changes).map_err(ModuleRegistryRuleErrorV1::Source)?;

        Ok(())
}

pub(in crate::world) struct LegacyModuleRegistryFactsV1<'a>(&'a crate::world::World);
impl<'a> LegacyModuleRegistryFactsV1<'a>{pub(in crate::world) fn new(world:&'a crate::world::World)->Self{Self(world)}}
impl sealed::Sealed for LegacyModuleRegistryFactsV1<'_>{}
impl<'s> ModuleRegistryFactsV1<'s> for LegacyModuleRegistryFactsV1<'s>{
 type Error=WorldError;
 fn reserve_changes(&mut self,_:&ModuleChangeSet)->Result<(),WorldError>{Ok(())}
 fn admission(&mut self,m:&oasis7_wasm_abi::ModuleManifest)->Result<(),WorldError>{self.0.ensure_module_admission_allowed(m)}
 fn storage_activation(&mut self,id:&str,v:&str)->Result<(),WorldError>{self.0.validate_m1_storage_activation(id,v)}
 fn record_exists(&mut self,key:&str)->Result<bool,WorldError>{Ok(self.0.module_registry.records.contains_key(key))}
 fn manifest(&mut self,key:&str)->Result<Option<ModuleManifestRecordV1<'s>>,WorldError>{Ok(self.0.module_registry.records.get(key).map(|r|ModuleManifestRecordV1::Legacy(&r.manifest)))}
 fn active_version(&mut self,id:&str)->Result<Option<ModuleTextRecordV1<'s>>,WorldError>{Ok(self.0.module_registry.active.get(id).map(|s|ModuleTextRecordV1::Legacy(s)))}
 fn gameplay_conflicts(&mut self,c:&ModuleChangeSet)->Result<(),WorldError>{self.0.validate_gameplay_activation_conflicts(c)}
}

impl std::fmt::Display for ModuleTextRecordV1<'_>{fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result{f.write_str(self.as_str())}}
