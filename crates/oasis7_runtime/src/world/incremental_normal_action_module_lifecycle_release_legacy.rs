//! Legacy adapters read the real World and delegate every business predicate
//! to the same fixed release kernel used by Current production.
use super::release_rules::*;
use crate::{ModuleManifest, ModuleProfileChanges, WorldError};
use oasis7_world_state::incremental_authorization_mapsets::AuthorizationMembershipV1 as Membership;
pub(in crate::world) struct LegacyModuleReleaseFactsV1<'a>(&'a crate::world::World);
impl<'a> LegacyModuleReleaseFactsV1<'a> {
    pub(in crate::world) fn new(world: &'a crate::world::World) -> Self {
        Self(world)
    }
}
impl sealed::Sealed for LegacyModuleReleaseFactsV1<'_> {}
impl ModuleReleaseBusinessFactsV1 for LegacyModuleReleaseFactsV1<'_> {
    type Error = WorldError;
    fn agent_exists(&mut self, id: &str) -> Result<bool, WorldError> {
        Ok(self.0.state.agents.contains_key(id))
    }
    fn artifact_exists(&mut self, hash: &str) -> Result<bool, WorldError> {
        Ok(self.0.module_artifacts.contains(hash))
    }
    fn owner(&mut self, hash: &str) -> Result<Option<String>, WorldError> {
        Ok(self.0.state.module_artifact_owners.get(hash).cloned())
    }
    fn frozen_admission(&mut self, manifest: &ModuleManifest) -> Result<(), WorldError> {
        self.0.ensure_module_admission_allowed(manifest)
    }
    fn request(&mut self, id: u64) -> Result<Option<ModuleReleaseRequestSnapshotV1>, WorldError> {
        Ok(self
            .0
            .state
            .module_release_requests
            .get(&id)
            .cloned()
            .map(ModuleReleaseRequestSnapshotV1::Legacy))
    }
    fn next_request_id(&mut self) -> Result<u64, WorldError> {
        Ok(self.0.state.next_module_release_request_id.max(1))
    }
    fn node_identity_present(&mut self, id: &str) -> Result<bool, WorldError> {
        Ok(self.0.node_identity_public_key(id).is_some())
    }
    fn role_membership(&mut self, agent: &str, role: &str) -> Result<Membership, WorldError> {
        Ok(match self.0.state.module_release_role_bindings.get(agent) {
            None => Membership::GroupAbsent,
            Some(roles) if roles.contains(role) => Membership::Present,
            Some(_) => Membership::MemberAbsent,
        })
    }
    fn profile_changes(
        &mut self,
        changes: &ModuleProfileChanges,
    ) -> Result<Result<(), String>, WorldError> {
        Ok(self.0.validate_module_release_profile_changes(changes))
    }
    fn shadow_hash(
        &mut self,
        manifest: &ModuleManifest,
        activate: bool,
        changes: &ModuleProfileChanges,
    ) -> Result<Result<String, String>, WorldError> {
        Ok(self
            .0
            .evaluate_module_release_shadow_hash(manifest, activate, changes))
    }
}
