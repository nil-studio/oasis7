//! Native L4 process-local World construction and recovery; no lower-layer reverse dependency.
#[cfg(not(target_arch = "wasm32"))]
pub mod agent;
#[cfg(not(target_arch = "wasm32"))]
pub mod agent_api_compat;
#[path = "agent_execution/async_agent_runner_retry.rs"]
pub mod async_agent_runner_retry;
#[cfg(not(target_arch = "wasm32"))]
pub mod bootstrap;
#[cfg(not(target_arch = "wasm32"))]
pub mod cognition_policy;
pub mod cognition_response_identity;
#[cfg(not(target_arch = "wasm32"))]
pub mod decision_provider;
#[cfg(not(target_arch = "wasm32"))]
pub mod gameplay_actions;
#[cfg(not(target_arch = "wasm32"))]
pub mod gameplay_service;
pub mod llm_prompt_profile;
pub mod llm_request_budget;
#[cfg(not(target_arch = "wasm32"))]
pub mod provider_json_admission;
pub mod provider_json_shape;
#[cfg(not(target_arch = "wasm32"))]
pub mod provider_loopback_adapter;
#[cfg(not(target_arch = "wasm32"))]
pub mod provider_loopback_http;
#[cfg(not(target_arch = "wasm32"))]
#[path = "agent_execution/provider_material_capture.rs"]
pub mod provider_material_capture;
pub mod provider_transport_url;
pub mod simulator_archive;

#[path = "agent_execution/llm_agent/decision_flow/field_identity.rs"]
pub mod llm_decision_field_identity;

#[cfg(not(target_arch = "wasm32"))]
use oasis7_runtime::{Journal, ReleaseSecurityPolicy, Snapshot, World, WorldError, WorldState};
#[cfg(not(target_arch = "wasm32"))]
use std::{path::Path, sync::Arc};

/// Attach application capabilities after deterministic replay and validation have completed.
#[cfg(not(target_arch = "wasm32"))]
pub fn install_runtime_capabilities(mut world: World) -> World {
    world.set_module_source_compiler(Arc::new(
        oasis7_world_service::module_source_compiler::HostModuleSourceCompiler,
    ));
    world
}

/// Native application assembly. The returned value is the Runtime's original `World` type.
#[cfg(not(target_arch = "wasm32"))]
pub mod world {
    use super::*;
    pub fn new_production_hardened_with_initial_module_visuals(
        entities: &std::collections::BTreeMap<
            String,
            oasis7_agent_api::world_entity::ModuleVisualEntity,
        >,
    ) -> Result<World, WorldError> {
        World::new_with_initial_module_visuals(
            ReleaseSecurityPolicy::production_hardened(),
            entities,
        )
        .map(install_runtime_capabilities)
    }
    pub fn new() -> World {
        install_runtime_capabilities(World::new())
    }
    pub fn new_with_state(state: WorldState) -> World {
        install_runtime_capabilities(World::new_with_state(state))
    }
    pub fn new_with_release_security_policy(policy: ReleaseSecurityPolicy) -> World {
        install_runtime_capabilities(World::new_with_release_security_policy(policy))
    }
    pub fn new_production_hardened() -> World {
        install_runtime_capabilities(World::new_production_hardened())
    }
    pub fn new_production_hardened_with_cognition_binding(
        world_id: impl Into<String>,
        branch_id: impl Into<String>,
        finality_epoch: u64,
        finality_block_hash: Option<String>,
        finality_status: impl Into<String>,
        reorg_epoch: u64,
    ) -> Result<World, WorldError> {
        World::new_production_hardened_with_cognition_binding(
            world_id,
            branch_id,
            finality_epoch,
            finality_block_hash,
            finality_status,
            reorg_epoch,
        )
        .map(install_runtime_capabilities)
    }
    pub fn from_snapshot(snapshot: Snapshot, journal: Journal) -> Result<World, WorldError> {
        World::from_snapshot(snapshot, journal).map(install_runtime_capabilities)
    }
    pub fn from_snapshot_with_release_security_policy(
        snapshot: Snapshot,
        journal: Journal,
        policy: ReleaseSecurityPolicy,
    ) -> Result<World, WorldError> {
        World::from_snapshot_with_release_security_policy(snapshot, journal, policy)
            .map(install_runtime_capabilities)
    }
    pub fn from_snapshot_with_devlocal_participant_authority(
        snapshot: Snapshot,
        journal: Journal,
        authority: oasis7_runtime::DevLocalParticipantAuthority,
    ) -> Result<World, WorldError> {
        World::from_snapshot_with_devlocal_participant_authority(snapshot, journal, authority)
            .map(install_runtime_capabilities)
    }
    pub fn from_verified_formal_snapshot(
        snapshot: Snapshot,
        journal: Journal,
        parent: oasis7_runtime::VerifiedFormalRetainedHistoryParentV1,
        reader: Arc<dyn oasis7_formal_protocol::controlled_authority::formal::business_index::BusinessIndexNodeReader + Send + Sync>,
    ) -> Result<World, WorldError> {
        World::from_verified_formal_snapshot(snapshot, journal, parent, reader)
            .map(install_runtime_capabilities)
    }
    pub fn load_from_dir(path: impl AsRef<Path>) -> Result<World, WorldError> {
        World::load_from_dir(path).map(install_runtime_capabilities)
    }
    pub fn load_from_dir_with_release_security_policy(
        path: impl AsRef<Path>,
        policy: ReleaseSecurityPolicy,
    ) -> Result<World, WorldError> {
        World::load_from_dir_with_release_security_policy(path, policy)
            .map(install_runtime_capabilities)
    }
    pub fn load_from_dir_with_devlocal_participant_authority(
        path: impl AsRef<Path>,
        authority: oasis7_runtime::DevLocalParticipantAuthority,
    ) -> Result<World, WorldError> {
        World::load_from_dir_with_devlocal_participant_authority(path, authority)
            .map(install_runtime_capabilities)
    }
    pub fn load_from_dir_with_modules(path: impl AsRef<Path>) -> Result<World, WorldError> {
        World::load_from_dir_with_modules(path).map(install_runtime_capabilities)
    }
    pub fn load_authoritative_recovery_generation(
        path: impl AsRef<Path>,
    ) -> Result<Option<oasis7_runtime::CommittedAuthoritativeRecoveryGeneration>, WorldError> {
        World::load_authoritative_recovery_generation(path).map(install_generation_capabilities)
    }
    pub fn load_authoritative_recovery_generation_with_release_security_policy(
        path: impl AsRef<Path>,
        policy: ReleaseSecurityPolicy,
    ) -> Result<Option<oasis7_runtime::CommittedAuthoritativeRecoveryGeneration>, WorldError> {
        World::load_authoritative_recovery_generation_with_release_security_policy(path, policy)
            .map(install_generation_capabilities)
    }
    pub fn load_authoritative_recovery_generation_with_devlocal_participant_authority(
        path: impl AsRef<Path>,
        authority: oasis7_runtime::DevLocalParticipantAuthority,
    ) -> Result<Option<oasis7_runtime::CommittedAuthoritativeRecoveryGeneration>, WorldError> {
        World::load_authoritative_recovery_generation_with_devlocal_participant_authority(
            path, authority,
        )
        .map(install_generation_capabilities)
    }
    fn install_generation_capabilities(
        generation: Option<oasis7_runtime::CommittedAuthoritativeRecoveryGeneration>,
    ) -> Option<oasis7_runtime::CommittedAuthoritativeRecoveryGeneration> {
        generation.map(|mut generation| {
            generation.world = install_runtime_capabilities(generation.world);
            generation
        })
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    #[test]
    fn fresh_and_rebuilt_world_install_the_same_native_capability_owner() {
        let world = world::new();
        assert_eq!(world.state().time, 0);
        let restored = world::new_with_state(world.state().clone());
        assert_eq!(restored.state().time, 0);
        let hardened = world::new_production_hardened();
        assert_eq!(hardened.state().time, 0);
        let _: World = install_runtime_capabilities(restored);
    }
}
