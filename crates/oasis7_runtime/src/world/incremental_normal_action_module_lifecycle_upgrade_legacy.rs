//! Legacy record observer for the one upgrade prelude; no Current authority.
use super::{instance_rules::{self,ModuleInstallFactsV1,LegacyModuleInstallFactsV1},upgrade_rules::*};
use super::registry_rules::{ModuleManifestRecordV1,ModuleTextRecordV1};
use crate::{ModuleManifest,WorldError};
use oasis7_agent_api::world_action::ResourceKind;
pub(in crate::world) struct LegacyModuleUpgradeFactsV1<'s>(&'s crate::world::World);
impl<'s> LegacyModuleUpgradeFactsV1<'s>{pub(in crate::world) fn new(world:&'s crate::world::World)->Self{Self(world)}}
impl instance_rules::sealed::Sealed for LegacyModuleUpgradeFactsV1<'_>{}
impl<'s> ModuleInstallFactsV1<'s> for LegacyModuleUpgradeFactsV1<'s>{
 type Error=WorldError;
 fn reserve_install(&mut self,m:&ModuleManifest,a:&str)->Result<(),WorldError>{LegacyModuleInstallFactsV1::new(self.0).reserve_install(m,a)}
 fn frozen_admission(&mut self,m:&ModuleManifest)->Result<(),WorldError>{LegacyModuleInstallFactsV1::new(self.0).frozen_admission(m)}
 fn agent_exists(&mut self,id:&str)->Result<bool,WorldError>{LegacyModuleInstallFactsV1::new(self.0).agent_exists(id)}
 fn owner(&mut self,h:&str)->Result<Option<String>,WorldError>{LegacyModuleInstallFactsV1::new(self.0).owner(h)}
 fn balance(&mut self,id:&str,k:ResourceKind)->Result<i64,WorldError>{LegacyModuleInstallFactsV1::new(self.0).balance(id,k)}
 fn registry_manifest(&mut self,key:&str)->Result<Option<ModuleManifestRecordV1<'s>>,WorldError>{LegacyModuleInstallFactsV1::new(self.0).registry_manifest(key)}
 fn active_version(&mut self,id:&str)->Result<Option<ModuleTextRecordV1<'s>>,WorldError>{LegacyModuleInstallFactsV1::new(self.0).active_version(id)}
}
impl<'s> ModuleUpgradeFactsV1<'s> for LegacyModuleUpgradeFactsV1<'s>{
 fn reserve_upgrade(&mut self,_:&ModuleManifest,_:&str,_:&str,_:&str)->Result<(),WorldError>{Ok(())}
 fn instance(&mut self,id:&str)->Result<Option<ModuleInstanceRecordV1<'s>>,WorldError>{Ok(self.0.state.module_instances.get(id).map(ModuleInstanceRecordV1::Legacy))}
 fn reserve_instance(&mut self,_:&ModuleInstanceRecordV1<'_>)->Result<(),WorldError>{Ok(())}
}
/// Full selected row copy. Current callers must retain their decoder owner and
/// admit every copied field first; this method cannot manufacture a source.
impl ModuleInstanceRecordV1<'_>{
 pub(in crate::world) fn snapshot_paid_v1(&self,common:&mut oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedContextV1<'_>)->Result<oasis7_proto::memory_budget::ReservedValue<crate::ModuleInstanceState>,crate::world::incremental_body::BodyMutationError>{
  match self{
   Self::Current(row)=>row.snapshot_paid_v1(common),
   Self::Legacy(row)=>{
    use oasis7_agent_api::world_action::ModuleInstallTarget;
    let lengths=[row.instance_id.len(),row.module_id.len(),row.module_version.len(),row.wasm_hash.len(),row.owner_agent_id.len(),match &row.install_target{ModuleInstallTarget::SelfAgent=>0,ModuleInstallTarget::LocationInfrastructure{location_id}=>location_id.len()}];
    let mut bytes=std::mem::size_of::<crate::ModuleInstanceState>().checked_add(512).ok_or_else(super::invalid)?;
    for n in lengths{bytes=bytes.checked_add(n).ok_or_else(super::invalid)?;}
    common.charge(8,u64::try_from(bytes).map_err(|_|super::invalid())?).map_err(|e|crate::world::incremental_body::BodyMutationError::Original(Box::new(e)))?;
    let fee=common.memory().reserve(bytes,oasis7_proto::memory_budget::MemoryClass::Ordinary).map_err(oasis7_formal_protocol::incremental_value::ValueTreeError::Memory)?;
    Ok(fee.retain((*row).clone()))
   }
  }
 }
}
