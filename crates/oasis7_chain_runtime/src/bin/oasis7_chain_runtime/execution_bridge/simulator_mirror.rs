use std::path::Path;

use oasis7::simulator::WorldKernel;
use oasis7_runtime::ChainResourceDerivationContext;

pub(crate) fn simulator_world_dir_from_execution_world_dir(world_dir: &Path) -> std::path::PathBuf {
    match world_dir.file_name().and_then(|name| name.to_str()) {
        Some(name) if !name.is_empty() => {
            world_dir.with_file_name(format!("{name}-simulator-mirror"))
        }
        _ => world_dir.join("simulator-mirror"),
    }
}

pub(super) fn load_simulator_execution_world(world_dir: &Path) -> Result<WorldKernel, String> {
    let snapshot_path = world_dir.join("snapshot.json");
    let journal_path = world_dir.join("journal.json");
    if !snapshot_path.exists() || !journal_path.exists() {
        return Ok(WorldKernel::new());
    }
    WorldKernel::load_from_dir(world_dir).map_err(|err| {
        format!(
            "load simulator execution mirror from {} failed: {:?}",
            world_dir.display(),
            err
        )
    })
}

pub(super) fn persist_simulator_execution_world(
    world_dir: &Path,
    simulator_world: &WorldKernel,
    resource_context: Option<ChainResourceDerivationContext<'_>>,
) -> Result<(), String> {
    let result = match resource_context {
        Some(context) => {
            simulator_world.save_to_dir_with_chain_resource_context(world_dir, context)
        }
        None => simulator_world.save_to_dir(world_dir),
    };
    result.map_err(|err| {
        format!(
            "save simulator execution mirror to {} failed: {:?}",
            world_dir.display(),
            err
        )
    })
}
