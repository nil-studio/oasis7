//! Unique upgrade prelude with concrete, sealed original-record observers.
use super::*;
use super::{instance_rules::{ModuleInstallFactsV1,ModuleBusinessErrorV1},registry_rules::ModuleManifestRecordV1};
use oasis7_world_state::incremental_module_instance_fields::{ModuleInstanceFieldV1 as F,ModuleInstanceScalarRefV1 as Scalar};
pub(in crate::world) enum ModuleInstanceRecordV1<'s>{Legacy(&'s crate::ModuleInstanceState),Current(ReservedValue<crate::world::incremental_module_read::PreparedModuleInstanceFields>)}
impl ModuleInstanceRecordV1<'_>{
 pub(in crate::world) fn module_id(&self)->&str{match self{Self::Legacy(s)=>&s.module_id,Self::Current(s)=>match s.field(F::ModuleId){Scalar::Text(v)=>v,_=>unreachable!("closed module ID")}}}
 pub(in crate::world) fn version(&self)->&str{match self{Self::Legacy(s)=>&s.module_version,Self::Current(s)=>match s.field(F::Version){Scalar::Text(v)=>v,_=>unreachable!("closed module version")}}}
 pub(in crate::world) fn owner(&self)->&str{match self{Self::Legacy(s)=>&s.owner_agent_id,Self::Current(s)=>match s.field(F::Owner){Scalar::Text(v)=>v,_=>unreachable!("closed module owner")}}}
}
pub(in crate::world) trait ModuleUpgradeFactsV1<'s>:ModuleInstallFactsV1<'s>{
 fn reserve_upgrade(&mut self,manifest:&crate::ModuleManifest,actor:&str,instance_id:&str,from:&str)->Result<(),Self::Error>;
 fn instance(&mut self,id:&str)->Result<Option<ModuleInstanceRecordV1<'s>>,Self::Error>;
 fn reserve_instance(&mut self,instance:&ModuleInstanceRecordV1<'_>)->Result<(),Self::Error>;
}
pub(in crate::world) enum ModuleUpgradePreludeV1<'s>{Rejected(crate::RejectReason),Plan{instance:ModuleInstanceRecordV1<'s>,changes:crate::ModuleChangeSet,fee_kind:oasis7_agent_api::world_action::ResourceKind,fee_amount:i64}}
pub(in crate::world) fn prepare_module_upgrade_prelude_v1<'s,S:ModuleUpgradeFactsV1<'s>>(source:&mut S,actor:&str,instance_id:&str,from:&str,manifest:&crate::ModuleManifest,activate:bool)->Result<ModuleUpgradePreludeV1<'s>,S::Error>{
 use crate::RejectReason;use oasis7_agent_api::world_action::ResourceKind;
 source.reserve_upgrade(manifest,actor,instance_id,from)?;
 if let Err(error)=source.frozen_admission(manifest){match error.into_business(){Ok(e)=>return Ok(ModuleUpgradePreludeV1::Rejected(RejectReason::RuleDenied{notes:vec![format!("{e:?}")]})),Err(e)=>return Err(e)}}
 if !source.agent_exists(actor)?{return Ok(ModuleUpgradePreludeV1::Rejected(RejectReason::AgentNotFound{agent_id:actor.to_owned()}))}
 let Some(instance)=source.instance(instance_id)?else{return Ok(ModuleUpgradePreludeV1::Rejected(RejectReason::RuleDenied{notes:vec![format!("upgrade module rejected: instance not found {}",instance_id)]}))};
 source.reserve_instance(&instance)?;
 let reason=if instance.owner()!=actor{Some(format!("upgrade module rejected: upgrader {} does not own instance {} (owner {})",actor,instance_id,instance.owner()))}
 else if instance.version()!=from{Some(format!("upgrade module rejected: from_version mismatch for instance {} expected {} got {}",instance_id,instance.version(),from))}
 else if manifest.module_id!=instance.module_id(){Some(format!("upgrade module rejected: manifest module_id mismatch for instance {} expected {} got {}",instance_id,instance.module_id(),manifest.module_id))}
 else if manifest.version==instance.version(){Some(format!("upgrade module rejected: target version equals current version {}",manifest.version))}else{None};
 if let Some(reason)=reason{return Ok(ModuleUpgradePreludeV1::Rejected(RejectReason::RuleDenied{notes:vec![reason]}))}
 if let Some(owner)=source.owner(&manifest.wasm_hash)?{if owner!=actor{return Ok(ModuleUpgradePreludeV1::Rejected(RejectReason::RuleDenied{notes:vec![format!("upgrade module artifact rejected: upgrader {} does not own {} (owner {})",actor,manifest.wasm_hash,owner)]}))}}
 let key=crate::ModuleRegistry::record_key(instance.module_id(),instance.version());
 let Some(current)=source.registry_manifest(&key)?else{return Ok(ModuleUpgradePreludeV1::Rejected(RejectReason::RuleDenied{notes:vec![format!("upgrade module rejected: current module record missing {}",key)]}))};
 if let Err(reason)=validate_upgrade_interface_compatible_v1(current.manifest(),manifest){return Ok(ModuleUpgradePreludeV1::Rejected(RejectReason::RuleDenied{notes:vec![format!("upgrade module rejected: {reason}")]}))}
 let fee_kind=ResourceKind::Electricity;let fee_amount=crate::World::module_install_fee_amount(manifest);let available=source.balance(actor,fee_kind)?;
 if fee_amount>0&&available<fee_amount{return Ok(ModuleUpgradePreludeV1::Rejected(RejectReason::InsufficientResource{agent_id:actor.to_owned(),kind:fee_kind,requested:fee_amount,available}))}
 let mut changes=crate::ModuleChangeSet{upgrade:vec![crate::ModuleUpgrade{module_id:instance.module_id().to_owned(),from_version:instance.version().to_owned(),to_version:manifest.version.clone(),wasm_hash:manifest.wasm_hash.clone(),manifest:manifest.clone()}],..Default::default()};
 if activate{changes.activate.push(crate::ModuleActivation{module_id:manifest.module_id.clone(),version:manifest.version.clone()})}
 Ok(ModuleUpgradePreludeV1::Plan{instance,changes,fee_kind,fee_amount})
}
pub(in crate::world) fn validate_upgrade_interface_compatible_v1(
        current: &oasis7_wasm_abi::ModuleManifest,
        next: &oasis7_wasm_abi::ModuleManifest,
    ) -> Result<(), String> {
        if current.interface_version != next.interface_version {
            return Err(format!(
                "upgrade interface_version mismatch: from={} to={}",
                current.interface_version, next.interface_version
            ));
        }

        let missing_exports: Vec<String> = current
            .exports
            .iter()
            .filter(|export_name| !next.exports.contains(*export_name))
            .cloned()
            .collect();
        if !missing_exports.is_empty() {
            return Err(format!(
                "upgrade exports incompatible: missing {:?}",
                missing_exports
            ));
        }

        for subscription in &current.subscriptions {
            if !next.subscriptions.contains(subscription) {
                return Err(
                    "upgrade subscriptions incompatible: existing subscription removed or modified"
                        .to_string(),
                );
            }
        }

        if current.abi_contract.abi_version != next.abi_contract.abi_version {
            return Err("upgrade abi_version mismatch".to_string());
        }
        if current.abi_contract.input_schema != next.abi_contract.input_schema
            || current.abi_contract.output_schema != next.abi_contract.output_schema
        {
            return Err("upgrade abi input/output schema mismatch".to_string());
        }
        for (slot, cap_ref) in &current.abi_contract.cap_slots {
            match next.abi_contract.cap_slots.get(slot) {
                Some(next_cap_ref) if next_cap_ref == cap_ref => {}
                _ => {
                    return Err(format!("upgrade abi cap slot mismatch for slot {}", slot));
                }
            }
        }
        for hook in &current.abi_contract.policy_hooks {
            if !next.abi_contract.policy_hooks.contains(hook) {
                return Err(format!(
                    "upgrade abi policy_hooks incompatible: missing {}",
                    hook
                ));
            }
        }
        for required_cap in &current.required_caps {
            if !next.required_caps.contains(required_cap) {
                return Err(format!(
                    "upgrade required_caps incompatible: missing {}",
                    required_cap
                ));
            }
        }
        Ok(())
    }

