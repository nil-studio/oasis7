//! Process-local capabilities installed by the native application after construction or recovery.
use crate::runtime::{Journal, ReleaseSecurityPolicy, Snapshot, World, WorldError, WorldState};
use std::{path::Path, sync::Arc};

/// Attach application capabilities after deterministic replay and validation have completed.
pub fn install_runtime_capabilities(mut world: World) -> World {
    world.set_module_source_compiler(Arc::new(
        crate::module_source_compiler::HostModuleSourceCompiler,
    ));
    world
}

/// Native application assembly. The returned value is the Runtime's original `World` type.
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
    pub fn from_verified_formal_snapshot(
        snapshot: Snapshot,
        journal: Journal,
        parent: crate::runtime::VerifiedFormalRetainedHistoryParentV1,
        reader: Arc<dyn oasis7_distfs::controlled_authority::formal::business_index::BusinessIndexNodeReader + Send + Sync>,
    ) -> Result<World, WorldError> {
        World::from_verified_formal_snapshot(snapshot, journal, parent, reader)
            .map(install_runtime_capabilities)
    }
    pub fn load_from_dir(path: impl AsRef<Path>) -> Result<World, WorldError> {
        World::load_from_dir(path).map(install_runtime_capabilities)
    }
    pub fn load_from_dir_with_modules(path: impl AsRef<Path>) -> Result<World, WorldError> {
        World::load_from_dir_with_modules(path).map(install_runtime_capabilities)
    }
    pub fn load_authoritative_recovery_generation(
        path: impl AsRef<Path>,
    ) -> Result<Option<crate::runtime::CommittedAuthoritativeRecoveryGeneration>, WorldError> {
        World::load_authoritative_recovery_generation(path).map(|generation| {
            generation.map(|mut generation| {
                generation.world = install_runtime_capabilities(generation.world);
                generation
            })
        })
    }
}

#[cfg(test)]
mod source_compiler_tests;

#[cfg(test)]
mod agent_chat_projection_tests;
#[cfg(test)]
mod game_task_projection_tests;

pub(crate) mod provider_context;
