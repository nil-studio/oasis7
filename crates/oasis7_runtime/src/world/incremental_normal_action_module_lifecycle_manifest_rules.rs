//! Unique manifest admission ordering. A source implementation is sealed and
//! performs the original concrete identity/ABI/capability checks over real data.
use super::*;
use super::registry_rules::ModuleRegistryRuleErrorV1;
use crate::{WorldError,ModuleManifest,ModuleLimits};
use oasis7_wasm_router::{validate_subscription_filters,validate_subscription_stage};
pub(in crate::world) mod sealed{pub trait Sealed{}}
pub(in crate::world) trait ModuleManifestFactsV1:sealed::Sealed{
 type Error;
 fn reserve_manifest(&mut self,module:&ModuleManifest)->Result<(),Self::Error>;
 fn artifact_registered(&mut self,hash:&str)->Result<bool,Self::Error>;
 fn identity(&mut self,module:&ModuleManifest)->Result<(),Self::Error>;
 fn abi(&mut self,module:&ModuleManifest)->Result<(),Self::Error>;
 fn limits(&mut self,id:&str,limits:&ModuleLimits)->Result<(),Self::Error>;
 /// Includes the exact original cap-specific error wrapper. Infrastructure
 /// failures remain the source error type and must never become a denial note.
 fn capability(&mut self,cap:&str)->Result<(),Self::Error>;
}
pub(in crate::world) fn validate_module_manifest_from_source_v1<S:ModuleManifestFactsV1>(source:&mut S,module:&ModuleManifest)->Result<(),ModuleRegistryRuleErrorV1<S::Error>>{
        source.reserve_manifest(module).map_err(ModuleRegistryRuleErrorV1::Source)?;
        if module.module_id.trim().is_empty() {
            return Err(WorldError::ModuleChangeInvalid {
                reason: "module_id is empty".to_string(),
            }.into());
        }
        if module.version.trim().is_empty() {
            return Err(WorldError::ModuleChangeInvalid {
                reason: format!("module version missing for {}", module.module_id),
            }.into());
        }
        if module.wasm_hash.trim().is_empty() {
            return Err(WorldError::ModuleChangeInvalid {
                reason: format!("module wasm_hash missing for {}", module.module_id),
            }.into());
        }
        if module.interface_version.trim().is_empty() {
            return Err(WorldError::ModuleChangeInvalid {
                reason: format!("module interface_version missing for {}", module.module_id),
            }.into());
        }
        if !source.artifact_registered(&module.wasm_hash).map_err(ModuleRegistryRuleErrorV1::Source)? {
            return Err(WorldError::ModuleChangeInvalid {
                reason: format!("module artifact missing {}", module.wasm_hash),
            }.into());
        }

        source.identity(module).map_err(ModuleRegistryRuleErrorV1::Source)?;

        if module.interface_version != "wasm-1" {
            return Err(WorldError::ModuleChangeInvalid {
                reason: format!(
                    "module interface_version unsupported {}",
                    module.interface_version
                ),
            }.into());
        }
        source.abi(module).map_err(ModuleRegistryRuleErrorV1::Source)?;

        let expected_export = module.kind.entrypoint();
        if !module.exports.iter().any(|name| name == expected_export) {
            return Err(WorldError::ModuleChangeInvalid {
                reason: format!(
                    "module exports missing {} for {}",
                    expected_export, module.module_id
                ),
            }.into());
        }

        for subscription in &module.subscriptions {
            validate_subscription_stage(subscription, &module.module_id)
                .map_err(|reason| ModuleRegistryRuleErrorV1::World(WorldError::ModuleChangeInvalid { reason }))?;
            validate_subscription_filters(&subscription.filters, &module.module_id)
                .map_err(|reason| ModuleRegistryRuleErrorV1::World(WorldError::ModuleChangeInvalid { reason }))?;
        }

        source.limits(&module.module_id,&module.limits).map_err(ModuleRegistryRuleErrorV1::Source)?;

        for cap in &module.required_caps {
            source.capability(cap).map_err(ModuleRegistryRuleErrorV1::Source)?;
        }

        Ok(())
}

/// Fixed ABI declaration predicates. The gameplay contract predicate remains
/// the next original check and uses its own unique pure kernel.
pub(in crate::world) fn validate_module_abi_shape_v1(module:&ModuleManifest)->Result<(),WorldError>{
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

        oasis7_wasm_abi::validate_module_command_declarations(&contract.declarations).map_err(|error| {
            WorldError::ModuleChangeInvalid {
                reason: format!(
                    "module command declarations invalid for {}: {error}",
                    module.module_id
                ),
            }
        })?;

        Ok(())

}
/// Six original resource-policy comparisons, with no policy/source authority.
pub(in crate::world) fn validate_module_limits_from_records_v1(module_id:&str,limits:&ModuleLimits,max:&ModuleLimits)->Result<(),WorldError>{
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

/// The original shadow stage repeats manifest admission in register/upgrade
/// order after the complete registry-change validation has succeeded.
pub(in crate::world) fn shadow_validate_module_changes_from_source_v1<S:ModuleManifestFactsV1>(source:&mut S,changes:&crate::ModuleChangeSet)->Result<(),ModuleRegistryRuleErrorV1<S::Error>>{
 for module in &changes.register{validate_module_manifest_from_source_v1(source,module)?;}
 for upgrade in &changes.upgrade{validate_module_manifest_from_source_v1(source,&upgrade.manifest)?;}
 Ok(())
}
