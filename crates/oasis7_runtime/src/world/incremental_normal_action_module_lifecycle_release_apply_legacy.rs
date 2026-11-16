//! Actual Legacy World observer; all Apply policy is in the unique shared kernel.
use super::release_apply_rules::*;
use super::release_legacy::LegacyModuleReleaseFactsV1;
use super::release_rules::{self, ModuleReleaseBusinessFactsV1, ModuleReleaseRequestSnapshotV1};
use crate::{GovernanceFinalityEpochSnapshot, ModuleManifest, ModuleProfileChanges, WorldError};
use oasis7_world_state::{
    incremental_authorization_mapsets::AuthorizationMembershipV1, state::ModuleReleaseRequestState,
};
pub(in crate::world) struct LegacyModuleReleaseApplyFactsV1<'a> {
    world: &'a crate::world::World,
    release: LegacyModuleReleaseFactsV1<'a>,
}
impl<'a> LegacyModuleReleaseApplyFactsV1<'a> {
    pub(in crate::world) fn new(world: &'a crate::world::World) -> Self {
        Self {
            world,
            release: LegacyModuleReleaseFactsV1::new(world),
        }
    }
}
impl release_rules::sealed::Sealed for LegacyModuleReleaseApplyFactsV1<'_> {}
impl ModuleReleaseBusinessFactsV1 for LegacyModuleReleaseApplyFactsV1<'_> {
    type Error = WorldError;
    fn agent_exists(&mut self, id: &str) -> Result<bool, WorldError> {
        self.release.agent_exists(id)
    }
    fn artifact_exists(&mut self, id: &str) -> Result<bool, WorldError> {
        self.release.artifact_exists(id)
    }
    fn owner(&mut self, id: &str) -> Result<Option<String>, WorldError> {
        self.release.owner(id)
    }
    fn frozen_admission(&mut self, manifest: &ModuleManifest) -> Result<(), WorldError> {
        self.release.frozen_admission(manifest)
    }
    fn request(&mut self, id: u64) -> Result<Option<ModuleReleaseRequestSnapshotV1>, WorldError> {
        self.release.request(id)
    }
    fn next_request_id(&mut self) -> Result<u64, WorldError> {
        self.release.next_request_id()
    }
    fn node_identity_present(&mut self, id: &str) -> Result<bool, WorldError> {
        self.release.node_identity_present(id)
    }
    fn role_membership(
        &mut self,
        agent: &str,
        role: &str,
    ) -> Result<AuthorizationMembershipV1, WorldError> {
        self.release.role_membership(agent, role)
    }
    fn profile_changes(
        &mut self,
        changes: &ModuleProfileChanges,
    ) -> Result<Result<(), String>, WorldError> {
        self.release.profile_changes(changes)
    }
    fn shadow_hash(
        &mut self,
        manifest: &ModuleManifest,
        activate: bool,
        changes: &ModuleProfileChanges,
    ) -> Result<Result<String, String>, WorldError> {
        self.release.shadow_hash(manifest, activate, changes)
    }
}
impl ModuleReleaseApplyFactsV1 for LegacyModuleReleaseApplyFactsV1<'_> {
    fn reserve_apply_query(&mut self, _: &str, _: u64) -> Result<(), WorldError> {
        Ok(())
    }
    fn epoch_snapshot(&mut self) -> Result<ModuleReleaseEpochSnapshotV1, WorldError> {
        let epoch = self.world.current_governance_epoch();
        Ok(ModuleReleaseEpochSnapshotV1::Legacy {
            epoch,
            snapshot: self
                .world
                .governance_finality_epoch_snapshot_for_epoch(epoch),
        })
    }
    fn reserve_attestation_evidence(
        &mut self,
        _: &ModuleReleaseRequestState,
        _: &GovernanceFinalityEpochSnapshot,
    ) -> Result<(), WorldError> {
        Ok(())
    }
}
