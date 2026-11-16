use std::collections::BTreeSet;

use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use oasis7_wasm_abi::validate_module_command_declarations;
use oasis7_wasm_router::{validate_subscription_filters, validate_subscription_stage};

use super::super::{
    ModuleArtifactIdentity, ModuleCommandCatalogEntry, ModuleLimits, ModuleManifest,
    ModuleRegistry, WorldError, module_command_catalog,
};
use super::World;

const IDENTITY_HASH_SIGNATURE_SCHEME: &str = "identity_hash_v1";
const IDENTITY_HASH_SIGNATURE_PREFIX: &str = "idhash:";

impl World {
    /// Return the deterministic Agent-facing catalog for active module commands.
    pub fn module_command_catalog(&self) -> Vec<ModuleCommandCatalogEntry> {
        module_command_catalog(&self.module_registry)
    }

    pub(super) fn validate_module_changes(
        &self,
        changes: &super::super::ModuleChangeSet,
    ) -> Result<(), WorldError> {
        use super::incremental_normal_action_module_lifecycle::registry_rules::{LegacyModuleRegistryFactsV1,ModuleRegistryRuleErrorV1,validate_module_changes_from_source_v1};
        match validate_module_changes_from_source_v1(&mut LegacyModuleRegistryFactsV1::new(self),changes){
            Ok(())=>Ok(()),Err(ModuleRegistryRuleErrorV1::World(e)|ModuleRegistryRuleErrorV1::Source(e))=>Err(e),
        }
    }

    pub(super) fn shadow_validate_module_changes(
        &self,
        changes: &super::super::ModuleChangeSet,
    ) -> Result<(), WorldError> {
        use super::incremental_normal_action_module_lifecycle::{manifest_rules::shadow_validate_module_changes_from_source_v1,registry_rules::ModuleRegistryRuleErrorV1};
        match shadow_validate_module_changes_from_source_v1(&mut LegacyManifestFactsV1(self),changes){
            Ok(())=>Ok(()),Err(ModuleRegistryRuleErrorV1::World(e)|ModuleRegistryRuleErrorV1::Source(e))=>Err(e),
        }
    }

    fn validate_module_manifest(&self, module: &ModuleManifest) -> Result<(), WorldError> {
        use super::incremental_normal_action_module_lifecycle::{manifest_rules::validate_module_manifest_from_source_v1,registry_rules::ModuleRegistryRuleErrorV1};
        match validate_module_manifest_from_source_v1(&mut LegacyManifestFactsV1(self),module){
            Ok(())=>Ok(()),Err(ModuleRegistryRuleErrorV1::World(e)|ModuleRegistryRuleErrorV1::Source(e))=>Err(e),
        }
    }

    pub(super) fn validate_module_artifact_identity(
        &self,
        module: &ModuleManifest,
    ) -> Result<(), WorldError> {
        super::validate_module_artifact_identity_from_legacy_v1(self,module)
    }

    pub(super) fn validate_module_artifact_identity_fields(
        &self,
        module: &ModuleManifest,
        identity: &ModuleArtifactIdentity,
    ) -> Result<(), WorldError> {
        super::validate_module_artifact_identity_fields_from_legacy_v1(self,module,identity)
    }

    fn validate_module_abi_contract(&self, module: &ModuleManifest) -> Result<(), WorldError> {
        super::incremental_normal_action_module_lifecycle::manifest_rules::validate_module_abi_shape_v1(module)?;

        self.validate_gameplay_contract_for_manifest(module)?;

        Ok(())
    }

    fn validate_module_limits(
        &self,
        module_id: &str,
        limits: &ModuleLimits,
    ) -> Result<(), WorldError> {
        super::incremental_normal_action_module_lifecycle::manifest_rules::validate_module_limits_from_records_v1(module_id,limits,&self.module_limits_max)
    }

    pub(super) fn active_module_manifest(
        &self,
        module_id: &str,
    ) -> Result<&ModuleManifest, WorldError> {
        let version = self.module_registry.active.get(module_id).ok_or_else(|| {
            WorldError::ModuleChangeInvalid {
                reason: format!("module not active {module_id}"),
            }
        })?;
        let key = ModuleRegistry::record_key(module_id, version);
        let record = self.module_registry.records.get(&key).ok_or_else(|| {
            WorldError::ModuleChangeInvalid {
                reason: format!("module record missing {key}"),
            }
        })?;
        Ok(&record.manifest)
    }
}


#[cfg(test)]
#[path = "module_upgrade_freeze_identity_regressions.rs"]
mod module_upgrade_freeze_identity_regressions;

struct LegacyManifestFactsV1<'a>(&'a World);
impl super::incremental_normal_action_module_lifecycle::manifest_rules::sealed::Sealed for LegacyManifestFactsV1<'_>{}
impl super::incremental_normal_action_module_lifecycle::manifest_rules::ModuleManifestFactsV1 for LegacyManifestFactsV1<'_>{
 type Error=WorldError;
 fn reserve_manifest(&mut self,_:&ModuleManifest)->Result<(),WorldError>{Ok(())}
 fn artifact_registered(&mut self,h:&str)->Result<bool,WorldError>{Ok(self.0.module_artifacts.contains(h))}
 fn identity(&mut self,m:&ModuleManifest)->Result<(),WorldError>{self.0.validate_module_artifact_identity(m)}
 fn abi(&mut self,m:&ModuleManifest)->Result<(),WorldError>{self.0.validate_module_abi_contract(m)}
 fn limits(&mut self,id:&str,l:&ModuleLimits)->Result<(),WorldError>{self.0.validate_module_limits(id,l)}
 fn capability(&mut self,cap:&str)->Result<(),WorldError>{self.0.validate_module_required_capability(cap).map_err(|error|WorldError::ModuleChangeInvalid{reason:format!("module cap admission failed {cap}: {error:?}")})}
}
