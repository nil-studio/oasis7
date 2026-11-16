//! Original rollback prelude over exact selected records and ALL applied
//! release requests. The observer is sealed; directory metadata grants nothing.
use super::*;
use super::{instance_rules::{ModuleInstallFactsV1,ModuleBusinessErrorV1},upgrade_rules::{ModuleUpgradeFactsV1,ModuleInstanceRecordV1,validate_upgrade_interface_compatible_v1},registry_rules::ModuleManifestRecordV1};
use crate::{ModuleManifest,RejectReason,ModuleChangeSet};
use oasis7_agent_api::world_action::ResourceKind;
pub(in crate::world) trait ModuleRollbackFactsV1<'s>:ModuleUpgradeFactsV1<'s>{
 fn reserve_rollback_query(&mut self,actor:&str,id:&str,target:&str)->Result<(),Self::Error>;
 /// Original lazy filter/next/next semantics: only one matching Applied record
 /// yields its embedded request_id. Missing and multiple matches both yield None.
 fn unique_applied_release(&mut self,module_id:&str,version:&str,hash:&str)->Result<Option<u64>,Self::Error>;
}
pub(in crate::world) enum ModuleRollbackPreludeV1<'s>{
 Rejected(RejectReason),
 Plan{instance:ModuleInstanceRecordV1<'s>,target_source:ModuleManifestRecordV1<'s>,target_manifest:ModuleManifest,changes:ModuleChangeSet,from_wasm_hash:String,from_release_request_id:Option<u64>,fee_kind:ResourceKind,fee_amount:i64},
}
impl ModuleInstanceRecordV1<'_>{
 pub(in crate::world) fn active_v1(&self)->bool{match self{Self::Legacy(row)=>row.active,Self::Current(row)=>match row.field(oasis7_world_state::incremental_module_instance_fields::ModuleInstanceFieldV1::Active){oasis7_world_state::incremental_module_instance_fields::ModuleInstanceScalarRefV1::Bool(v)=>v,_=>unreachable!("closed module active")}}}
}
pub(in crate::world) fn prepare_module_rollback_prelude_v1<'s,S:ModuleRollbackFactsV1<'s>>(source:&mut S,actor:&str,id:&str,target_version:&str)->Result<ModuleRollbackPreludeV1<'s>,S::Error>{
 source.reserve_rollback_query(actor,id,target_version)?;
 if !source.agent_exists(actor)?{return Ok(ModuleRollbackPreludeV1::Rejected(RejectReason::AgentNotFound{agent_id:actor.to_owned()}))}
 let target_version=target_version.trim();
 if target_version.is_empty(){return Ok(ModuleRollbackPreludeV1::Rejected(RejectReason::RuleDenied{notes:vec!["rollback module rejected: target_module_version is empty".to_owned()]}))}
 let Some(instance)=source.instance(id)?else{return Ok(ModuleRollbackPreludeV1::Rejected(RejectReason::RuleDenied{notes:vec![format!("rollback module rejected: instance not found {}",id)]}))};
 source.reserve_instance(&instance)?;
 if instance.owner()!=actor{return Ok(ModuleRollbackPreludeV1::Rejected(RejectReason::RuleDenied{notes:vec![format!("rollback module rejected: operator {} does not own instance {} (owner {})",actor,id,instance.owner())]}))}
 if instance.version()==target_version{return Ok(ModuleRollbackPreludeV1::Rejected(RejectReason::RuleDenied{notes:vec![format!("rollback module rejected: target version equals current version {}",target_version)]}))}
 let current_key=crate::ModuleRegistry::record_key(instance.module_id(),instance.version());
 let Some(current)=source.registry_manifest(&current_key)?else{return Ok(ModuleRollbackPreludeV1::Rejected(RejectReason::RuleDenied{notes:vec![format!("rollback module rejected: current module record missing {}",current_key)]}))};
 let target_key=crate::ModuleRegistry::record_key(instance.module_id(),target_version);
 let Some(target)=source.registry_manifest(&target_key)?else{return Ok(ModuleRollbackPreludeV1::Rejected(RejectReason::RuleDenied{notes:vec![format!("rollback module rejected: target version not found {}",target_key)]}))};
 if let Err(reason)=validate_upgrade_interface_compatible_v1(current.manifest(),target.manifest()){return Ok(ModuleRollbackPreludeV1::Rejected(RejectReason::RuleDenied{notes:vec![format!("rollback module rejected: {reason}")]}))}
 // Admit both actual selected graphs before cloning either artifact identity.
 source.reserve_install(current.manifest(),actor)?;
 source.reserve_install(target.manifest(),actor)?;
 let from_wasm_hash=current.manifest().wasm_hash.clone();
 let target_manifest=target.manifest().clone();
 if let Err(error)=source.frozen_admission(&target_manifest){match error.into_business(){Ok(error)=>return Ok(ModuleRollbackPreludeV1::Rejected(RejectReason::RuleDenied{notes:vec![format!("{error:?}")]})),Err(error)=>return Err(error)}}
 let from_release_request_id=source.unique_applied_release(instance.module_id(),instance.version(),&from_wasm_hash)?;
 let fee_kind=ResourceKind::Electricity;let fee_amount=crate::World::module_install_fee_amount(&target_manifest);
 if fee_amount>0{let available=source.balance(actor,fee_kind)?;if available<fee_amount{return Ok(ModuleRollbackPreludeV1::Rejected(RejectReason::InsufficientResource{agent_id:actor.to_owned(),kind:fee_kind,requested:fee_amount,available}))}}
 let mut changes=ModuleChangeSet::default();
 if instance.active_v1(){changes.activate.push(crate::ModuleActivation{module_id:target_manifest.module_id.clone(),version:target_manifest.version.clone()});}
 Ok(ModuleRollbackPreludeV1::Plan{instance,target_source:target,target_manifest,changes,from_wasm_hash,from_release_request_id,fee_kind,fee_amount})
}
