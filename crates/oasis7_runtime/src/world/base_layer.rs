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
        let mut register_ids = BTreeSet::new();
        let mut activate_ids = BTreeSet::new();
        let mut deactivate_ids = BTreeSet::new();
        let mut upgrade_ids = BTreeSet::new();

        for module in &changes.register {
            self.ensure_module_admission_allowed(module)?;
            if !register_ids.insert(module.module_id.clone()) {
                return Err(WorldError::ModuleChangeInvalid {
                    reason: format!("duplicate register module_id {}", module.module_id),
                });
            }
        }

        for activation in &changes.activate {
            if !activate_ids.insert(activation.module_id.clone()) {
                return Err(WorldError::ModuleChangeInvalid {
                    reason: format!("duplicate activate module_id {}", activation.module_id),
                });
            }
        }

        for deactivation in &changes.deactivate {
            if !deactivate_ids.insert(deactivation.module_id.clone()) {
                return Err(WorldError::ModuleChangeInvalid {
                    reason: format!("duplicate deactivate module_id {}", deactivation.module_id),
                });
            }
        }

        for upgrade in &changes.upgrade {
            if upgrade.module_id != upgrade.manifest.module_id {
                return Err(WorldError::ModuleChangeInvalid {
                    reason: format!("upgrade manifest module_id mismatch {}", upgrade.module_id),
                });
            }
            if upgrade.to_version != upgrade.manifest.version {
                return Err(WorldError::ModuleChangeInvalid {
                    reason: format!(
                        "upgrade target version mismatch for {}: change_set={} manifest={}",
                        upgrade.module_id, upgrade.to_version, upgrade.manifest.version
                    ),
                });
            }
            if upgrade.wasm_hash != upgrade.manifest.wasm_hash {
                return Err(WorldError::ModuleChangeInvalid {
                    reason: format!(
                        "upgrade wasm_hash mismatch for {}: change_set={} manifest={}",
                        upgrade.module_id, upgrade.wasm_hash, upgrade.manifest.wasm_hash
                    ),
                });
            }
            self.ensure_module_admission_allowed(&upgrade.manifest)?;
            if !upgrade_ids.insert(upgrade.module_id.clone()) {
                return Err(WorldError::ModuleChangeInvalid {
                    reason: format!("duplicate upgrade module_id {}", upgrade.module_id),
                });
            }
        }

        let mut planned_records = BTreeSet::new();
        for module in &changes.register {
            let key = ModuleRegistry::record_key(&module.module_id, &module.version);
            if self.module_registry.records.contains_key(&key)
                || !planned_records.insert(key.clone())
            {
                return Err(WorldError::ModuleChangeInvalid {
                    reason: format!("module version already registered {key}"),
                });
            }
        }

        for upgrade in &changes.upgrade {
            let to_key = ModuleRegistry::record_key(&upgrade.module_id, &upgrade.to_version);
            if self.module_registry.records.contains_key(&to_key) {
                return Err(WorldError::ModuleChangeInvalid {
                    reason: format!("module version already registered {to_key}"),
                });
            }
            if !planned_records.insert(to_key.clone()) {
                return Err(WorldError::ModuleChangeInvalid {
                    reason: format!("module version already registered {to_key}"),
                });
            }

            let from_key = ModuleRegistry::record_key(&upgrade.module_id, &upgrade.from_version);
            if !self.module_registry.records.contains_key(&from_key) {
                return Err(WorldError::ModuleChangeInvalid {
                    reason: format!("upgrade source missing {from_key}"),
                });
            }

            if let Some(active_version) = self.module_registry.active.get(&upgrade.module_id)
                && active_version != &upgrade.from_version
            {
                return Err(WorldError::ModuleChangeInvalid {
                    reason: format!(
                        "upgrade source version mismatch for {} (active {})",
                        upgrade.module_id, active_version
                    ),
                });
            }
        }

        for activation in &changes.activate {
            let key = ModuleRegistry::record_key(&activation.module_id, &activation.version);
            if let Some(manifest) = changes
                .register
                .iter()
                .find(|m| m.module_id == activation.module_id && m.version == activation.version)
                .or_else(|| {
                    changes.upgrade.iter().map(|u| &u.manifest).find(|m| {
                        m.module_id == activation.module_id && m.version == activation.version
                    })
                })
                .or_else(|| self.module_registry.records.get(&key).map(|r| &r.manifest))
            {
                self.ensure_module_admission_allowed(manifest)?;
            }
            let exists =
                self.module_registry.records.contains_key(&key) || planned_records.contains(&key);
            if !exists {
                return Err(WorldError::ModuleChangeInvalid {
                    reason: format!("activate target missing {key}"),
                });
            }
        }

        let mut will_activate = BTreeSet::new();
        for activation in &changes.activate {
            will_activate.insert(activation.module_id.clone());
        }
        for deactivation in &changes.deactivate {
            let has_active = self
                .module_registry
                .active
                .contains_key(&deactivation.module_id);
            if !has_active && !will_activate.contains(&deactivation.module_id) {
                return Err(WorldError::ModuleChangeInvalid {
                    reason: format!("deactivate target not active {}", deactivation.module_id),
                });
            }
        }

        self.validate_gameplay_activation_conflicts(changes)?;

        Ok(())
    }

    pub(super) fn shadow_validate_module_changes(
        &self,
        changes: &super::super::ModuleChangeSet,
    ) -> Result<(), WorldError> {
        for module in &changes.register {
            self.validate_module_manifest(module)?;
        }
        for upgrade in &changes.upgrade {
            self.validate_module_manifest(&upgrade.manifest)?;
        }
        Ok(())
    }

    fn validate_module_manifest(&self, module: &ModuleManifest) -> Result<(), WorldError> {
        if module.module_id.trim().is_empty() {
            return Err(WorldError::ModuleChangeInvalid {
                reason: "module_id is empty".to_string(),
            });
        }
        if module.version.trim().is_empty() {
            return Err(WorldError::ModuleChangeInvalid {
                reason: format!("module version missing for {}", module.module_id),
            });
        }
        if module.wasm_hash.trim().is_empty() {
            return Err(WorldError::ModuleChangeInvalid {
                reason: format!("module wasm_hash missing for {}", module.module_id),
            });
        }
        if module.interface_version.trim().is_empty() {
            return Err(WorldError::ModuleChangeInvalid {
                reason: format!("module interface_version missing for {}", module.module_id),
            });
        }
        if !self.module_artifacts.contains(&module.wasm_hash) {
            return Err(WorldError::ModuleChangeInvalid {
                reason: format!("module artifact missing {}", module.wasm_hash),
            });
        }

        self.validate_module_artifact_identity(module)?;

        if module.interface_version != "wasm-1" {
            return Err(WorldError::ModuleChangeInvalid {
                reason: format!(
                    "module interface_version unsupported {}",
                    module.interface_version
                ),
            });
        }
        self.validate_module_abi_contract(module)?;

        let expected_export = module.kind.entrypoint();
        if !module.exports.iter().any(|name| name == expected_export) {
            return Err(WorldError::ModuleChangeInvalid {
                reason: format!(
                    "module exports missing {} for {}",
                    expected_export, module.module_id
                ),
            });
        }

        for subscription in &module.subscriptions {
            validate_subscription_stage(subscription, &module.module_id)
                .map_err(|reason| WorldError::ModuleChangeInvalid { reason })?;
            validate_subscription_filters(&subscription.filters, &module.module_id)
                .map_err(|reason| WorldError::ModuleChangeInvalid { reason })?;
        }

        self.validate_module_limits(&module.module_id, &module.limits)?;

        for cap in &module.required_caps {
            self.validate_module_required_capability(cap)
                .map_err(|error| WorldError::ModuleChangeInvalid {
                    reason: format!("module cap admission failed {cap}: {error:?}"),
                })?;
        }

        Ok(())
    }

    pub(super) fn validate_module_artifact_identity(
        &self,
        module: &ModuleManifest,
    ) -> Result<(), WorldError> {
        let identity =
            module
                .artifact_identity
                .as_ref()
                .ok_or_else(|| WorldError::ModuleChangeInvalid {
                    reason: format!(
                        "module artifact_identity is required for {}",
                        module.module_id
                    ),
                })?;
        self.validate_module_artifact_identity_fields(module, identity)
    }

    pub(super) fn validate_module_artifact_identity_fields(
        &self,
        module: &ModuleManifest,
        identity: &ModuleArtifactIdentity,
    ) -> Result<(), WorldError> {
        if !identity.is_complete() {
            return Err(WorldError::ModuleChangeInvalid {
                reason: format!(
                    "module artifact_identity is incomplete for {}",
                    module.module_id
                ),
            });
        }

        if identity.signature_scheme == IDENTITY_HASH_SIGNATURE_SCHEME {
            if !self.release_security_policy.allow_identity_hash_signature {
                return Err(WorldError::ModuleChangeInvalid {
                    reason: format!(
                        "module artifact_identity signature_scheme {} is disabled by release policy for {}",
                        IDENTITY_HASH_SIGNATURE_SCHEME, module.module_id
                    ),
                });
            }
            let expected_identity_hash = super::super::util::sha256_hex(
                format!(
                    "{}:{}:{}",
                    module.module_id, identity.source_hash, identity.build_manifest_hash
                )
                .as_bytes(),
            );
            let expected_signature =
                format!("{IDENTITY_HASH_SIGNATURE_PREFIX}{expected_identity_hash}");
            if identity.artifact_signature != expected_signature {
                return Err(WorldError::ModuleChangeInvalid {
                    reason: format!(
                        "module artifact_identity identity_hash signature mismatch for {}: expected={} actual={}",
                        module.module_id, expected_signature, identity.artifact_signature
                    ),
                });
            }
            return Ok(());
        }

        if identity.has_unsigned_prefix() {
            return Err(WorldError::ModuleChangeInvalid {
                reason: format!(
                    "module artifact_identity unsigned signature is forbidden for {}",
                    module.module_id
                ),
            });
        }

        let Some(signature_prefix) = identity.expected_signature_prefix() else {
            return Err(WorldError::ModuleChangeInvalid {
                reason: format!(
                    "module artifact_identity signature_scheme unsupported for {}: {}",
                    module.module_id, identity.signature_scheme
                ),
            });
        };
        let signature_hex = identity
            .artifact_signature
            .strip_prefix(signature_prefix)
            .ok_or_else(|| WorldError::ModuleChangeInvalid {
                reason: format!(
                    "module artifact_identity signature prefix mismatch for {}",
                    module.module_id
                ),
            })?;

        let signer_public_key = self
            .node_identity_public_key(identity.signer_node_id.as_str())
            .ok_or_else(|| WorldError::ModuleChangeInvalid {
                reason: format!(
                    "module artifact_identity signer is not trusted for {}: {}",
                    module.module_id, identity.signer_node_id
                ),
            })?;
        let public_key_bytes =
            decode_hex_array::<32>(signer_public_key, "module artifact signer public key")?;
        let signature_bytes = decode_hex_array::<64>(signature_hex, "module artifact signature")?;
        let verifying_key = VerifyingKey::from_bytes(&public_key_bytes).map_err(|error| {
            WorldError::ModuleChangeInvalid {
                reason: format!(
                    "module artifact_identity signer public key parse failed for {}: {}",
                    module.module_id, error
                ),
            }
        })?;

        let payload = ModuleArtifactIdentity::signing_payload_v1(
            module.wasm_hash.as_str(),
            identity.source_hash.as_str(),
            identity.build_manifest_hash.as_str(),
            identity.signer_node_id.as_str(),
        );
        let signature = Signature::from_bytes(&signature_bytes);
        verifying_key
            .verify(payload.as_slice(), &signature)
            .map_err(|error| WorldError::ModuleChangeInvalid {
                reason: format!(
                    "module artifact_identity signature mismatch for {}: {}",
                    module.module_id, error
                ),
            })?;
        Ok(())
    }

    fn validate_module_abi_contract(&self, module: &ModuleManifest) -> Result<(), WorldError> {
        let contract = &module.abi_contract;
        if let Some(version) = contract.abi_version
            && version != 1
        {
            return Err(WorldError::ModuleChangeInvalid {
                reason: format!(
                    "module abi_version unsupported {} for {}",
                    version, module.module_id
                ),
            });
        }

        if let Some(schema) = &contract.input_schema
            && schema.trim().is_empty()
        {
            return Err(WorldError::ModuleChangeInvalid {
                reason: format!("module input_schema is empty for {}", module.module_id),
            });
        }
        if let Some(schema) = &contract.output_schema
            && schema.trim().is_empty()
        {
            return Err(WorldError::ModuleChangeInvalid {
                reason: format!("module output_schema is empty for {}", module.module_id),
            });
        }
        if contract.input_schema.is_some() ^ contract.output_schema.is_some() {
            return Err(WorldError::ModuleChangeInvalid {
                reason: format!(
                    "module abi_contract requires input_schema/output_schema pair for {}",
                    module.module_id
                ),
            });
        }
        for (slot, cap_ref) in &contract.cap_slots {
            if slot.trim().is_empty() {
                return Err(WorldError::ModuleChangeInvalid {
                    reason: format!(
                        "module abi_contract cap slot is empty for {}",
                        module.module_id
                    ),
                });
            }
            if cap_ref.trim().is_empty() {
                return Err(WorldError::ModuleChangeInvalid {
                    reason: format!(
                        "module abi_contract cap slot {slot} has empty cap_ref for {}",
                        module.module_id
                    ),
                });
            }
            if !module
                .required_caps
                .iter()
                .any(|required| required == cap_ref)
            {
                return Err(WorldError::ModuleChangeInvalid {
                    reason: format!(
                        "module abi_contract cap slot {slot} binds unknown cap_ref {cap_ref} for {}",
                        module.module_id
                    ),
                });
            }
        }
        for policy_hook in &contract.policy_hooks {
            if policy_hook.trim().is_empty() {
                return Err(WorldError::ModuleChangeInvalid {
                    reason: format!(
                        "module abi_contract policy hook is empty for {}",
                        module.module_id
                    ),
                });
            }
            if policy_hook == &module.module_id {
                return Err(WorldError::ModuleChangeInvalid {
                    reason: format!(
                        "module abi_contract policy hook self-reference {}",
                        module.module_id
                    ),
                });
            }
        }

        validate_module_command_declarations(&contract.declarations).map_err(|error| {
            WorldError::ModuleChangeInvalid {
                reason: format!(
                    "module command declarations invalid for {}: {error}",
                    module.module_id
                ),
            }
        })?;

        self.validate_gameplay_contract_for_manifest(module)?;

        Ok(())
    }

    fn validate_module_limits(
        &self,
        module_id: &str,
        limits: &ModuleLimits,
    ) -> Result<(), WorldError> {
        let max = &self.module_limits_max;
        if limits.max_mem_bytes > max.max_mem_bytes {
            return Err(WorldError::ModuleChangeInvalid {
                reason: format!("module limits max_mem_bytes exceeded for {module_id}"),
            });
        }
        if limits.max_gas > max.max_gas {
            return Err(WorldError::ModuleChangeInvalid {
                reason: format!("module limits max_gas exceeded for {module_id}"),
            });
        }
        if limits.max_call_rate > max.max_call_rate {
            return Err(WorldError::ModuleChangeInvalid {
                reason: format!("module limits max_call_rate exceeded for {module_id}"),
            });
        }
        if limits.max_output_bytes > max.max_output_bytes {
            return Err(WorldError::ModuleChangeInvalid {
                reason: format!("module limits max_output_bytes exceeded for {module_id}"),
            });
        }
        if limits.max_effects > max.max_effects {
            return Err(WorldError::ModuleChangeInvalid {
                reason: format!("module limits max_effects exceeded for {module_id}"),
            });
        }
        if limits.max_emits > max.max_emits {
            return Err(WorldError::ModuleChangeInvalid {
                reason: format!("module limits max_emits exceeded for {module_id}"),
            });
        }
        Ok(())
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

fn decode_hex_array<const N: usize>(raw: &str, label: &str) -> Result<[u8; N], WorldError> {
    let bytes = hex::decode(raw).map_err(|_| WorldError::ModuleChangeInvalid {
        reason: format!("{label} must be valid hex"),
    })?;
    bytes
        .try_into()
        .map_err(|_| WorldError::ModuleChangeInvalid {
            reason: format!("{label} must be {N}-byte hex"),
        })
}

#[cfg(test)]
#[path = "module_upgrade_freeze_identity_regressions.rs"]
mod module_upgrade_freeze_identity_regressions;
