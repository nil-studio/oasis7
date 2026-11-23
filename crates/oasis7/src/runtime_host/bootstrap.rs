//! Legacy archive-backed generated-sidecar loader. Native L4 owns the actual
//! World/industrial bootstrap, but the historical simulator host still owns
//! archive snapshot + journal validation until #6149 migrates that authority.
use crate::simulator::{ChunkRuntimeConfig, WorldConfig, WorldKernel, WorldModel};
use oasis7_runtime::World as RuntimeWorld;
pub use oasis7_runtime_host::bootstrap::*;
use std::path::Path;

pub fn bootstrap_generated_sidecar_runtime_world(
    generated_world_dir: &Path,
) -> Result<(RuntimeWorld, WorldConfig, WorldModel), String> {
    let (world, config, model, _) =
        bootstrap_generated_sidecar_runtime_world_with_chunk_runtime(generated_world_dir)?;
    Ok((world, config, model))
}

pub fn bootstrap_generated_sidecar_runtime_world_with_chunk_runtime(
    generated_world_dir: &Path,
) -> Result<(RuntimeWorld, WorldConfig, WorldModel, ChunkRuntimeConfig), String> {
    let sidecar_dir = generated_world_dir.join("generated-scenario-world");
    let provenance_path = generated_world_dir.join("world-generation-provenance.json");
    if !provenance_path.is_file() {
        return Err(format!(
            "generated world provenance missing: {}",
            provenance_path.display()
        ));
    }
    let kernel = WorldKernel::load_from_dir(&sidecar_dir).map_err(|err| {
        format!(
            "runtime live generated sidecar load failed dir={} err={err:?}",
            sidecar_dir.display()
        )
    })?;
    let snapshot = kernel.snapshot();
    let chunk_runtime = snapshot.chunk_runtime.clone();
    let config = snapshot.config;
    let model = snapshot.model;
    let (world, config) = oasis7_runtime_host::bootstrap::bootstrap_runtime_world_from_model(
        config,
        &model,
        "runtime live generated sidecar",
    )?;
    Ok((world, config, model, chunk_runtime))
}

#[cfg(test)]
mod archive_loader_tests {
    use super::*;

    #[test]
    fn generated_sidecar_rejects_missing_provenance_before_archive_open() {
        let absent = Path::new("__oasis7_native_bootstrap_absent_provenance__");
        let error = match bootstrap_generated_sidecar_runtime_world_with_chunk_runtime(absent) {
            Err(error) => error,
            Ok(_) => panic!("missing generated provenance is not a trusted archive"),
        };
        assert!(error.contains("world provenance missing"));
    }
}
