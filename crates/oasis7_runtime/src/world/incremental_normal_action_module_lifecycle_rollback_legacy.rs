//! Actual legacy World observers delegating to the shared install/upgrade views.
use super::{instance_rules::{self,ModuleInstallFactsV1,LegacyModuleInstallFactsV1},upgrade_rules::{ModuleUpgradeFactsV1,ModuleInstanceRecordV1},upgrade_legacy::LegacyModuleUpgradeFactsV1,rollback_rules::ModuleRollbackFactsV1,registry_rules::{ModuleManifestRecordV1,ModuleTextRecordV1}};
use crate::{ModuleManifest,WorldError};
use oasis7_agent_api::world_action::ResourceKind;
pub(in crate::world) struct LegacyModuleRollbackFactsV1<'s>(&'s crate::world::World);
impl<'s> LegacyModuleRollbackFactsV1<'s>{pub(in crate::world) fn new(world:&'s crate::world::World)->Self{Self(world)}}
impl instance_rules::sealed::Sealed for LegacyModuleRollbackFactsV1<'_>{}
impl<'s> ModuleInstallFactsV1<'s> for LegacyModuleRollbackFactsV1<'s>{
 type Error=WorldError;
 fn reserve_install(&mut self,m:&ModuleManifest,a:&str)->Result<(),WorldError>{LegacyModuleInstallFactsV1::new(self.0).reserve_install(m,a)}
 fn frozen_admission(&mut self,m:&ModuleManifest)->Result<(),WorldError>{LegacyModuleInstallFactsV1::new(self.0).frozen_admission(m)}
 fn agent_exists(&mut self,id:&str)->Result<bool,WorldError>{LegacyModuleInstallFactsV1::new(self.0).agent_exists(id)}
 fn owner(&mut self,h:&str)->Result<Option<String>,WorldError>{LegacyModuleInstallFactsV1::new(self.0).owner(h)}
 fn balance(&mut self,id:&str,k:ResourceKind)->Result<i64,WorldError>{LegacyModuleInstallFactsV1::new(self.0).balance(id,k)}
 fn registry_manifest(&mut self,key:&str)->Result<Option<ModuleManifestRecordV1<'s>>,WorldError>{LegacyModuleInstallFactsV1::new(self.0).registry_manifest(key)}
 fn active_version(&mut self,id:&str)->Result<Option<ModuleTextRecordV1<'s>>,WorldError>{LegacyModuleInstallFactsV1::new(self.0).active_version(id)}
}
impl<'s> ModuleUpgradeFactsV1<'s> for LegacyModuleRollbackFactsV1<'s>{
 fn reserve_upgrade(&mut self,m:&ModuleManifest,a:&str,id:&str,from:&str)->Result<(),WorldError>{LegacyModuleUpgradeFactsV1::new(self.0).reserve_upgrade(m,a,id,from)}
 fn instance(&mut self,id:&str)->Result<Option<ModuleInstanceRecordV1<'s>>,WorldError>{LegacyModuleUpgradeFactsV1::new(self.0).instance(id)}
 fn reserve_instance(&mut self,row:&ModuleInstanceRecordV1<'_>)->Result<(),WorldError>{LegacyModuleUpgradeFactsV1::new(self.0).reserve_instance(row)}
}
impl<'s> ModuleRollbackFactsV1<'s> for LegacyModuleRollbackFactsV1<'s>{
 fn reserve_rollback_query(&mut self,_:&str,_:&str,_:&str)->Result<(),WorldError>{Ok(())}
 fn unique_applied_release(&mut self,module_id:&str,version:&str,hash:&str)->Result<Option<u64>,WorldError>{
  let mut matches=self.0.state.module_release_requests.values().filter(|row|row.status==oasis7_world_state::state::ModuleReleaseRequestStatus::Applied&&row.manifest.module_id==module_id&&row.manifest.version==version&&row.manifest.wasm_hash==hash);
  let source=matches.next().map(|row|row.request_id);
  Ok(if matches.next().is_none(){source}else{None})
 }
}
