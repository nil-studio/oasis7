#[cfg(any(not(test), feature = "test_tier_required"))]
use std::fs;
#[cfg(any(not(test), feature = "test_tier_required"))]
use std::path::{Path, PathBuf};

#[cfg(any(not(test), feature = "test_tier_required"))]
use oasis7_node::NodeRuntime;
use oasis7_node::{NodeReplicatedExecutionInputV1, PROVIDER_BACKED_BOOTSTRAP_EXECUTION_INPUT_KIND};
use oasis7_runtime::{ProviderBackedBootstrapAuthorityV1, World as RuntimeWorld};

use super::driver::NodeRuntimeExecutionDriver;

impl NodeRuntimeExecutionDriver {
    fn execution_world_without_persistence(&self) -> Result<RuntimeWorld, String> {
        let snapshot = self.execution_world.snapshot();
        let has_inline_module_artifacts = !snapshot.module_artifact_bytes.is_empty();
        let mut world = oasis7::runtime_host::world::from_snapshot(
            snapshot,
            self.execution_world.journal().clone(),
        )
        .map_err(|error| {
            format!(
                "clone execution world for non-persistent ProviderBacked bootstrap failed: {error:?}"
            )
        })?
        .with_release_security_policy(self.execution_world.release_security_policy().clone());
        if !has_inline_module_artifacts {
            world
                .load_module_store_from_dir(self.world_dir.as_path())
                .map_err(|error| {
                    format!(
                        "load execution module store for ProviderBacked bootstrap failed: {error:?}"
                    )
                })?;
        }
        Ok(world)
    }

    pub(super) fn apply_provider_backed_bootstrap(
        &mut self,
        authorities: &[ProviderBackedBootstrapAuthorityV1],
    ) -> Result<(), String> {
        if authorities.is_empty() {
            return Err("ProviderBacked authority bootstrap input cannot be empty".to_string());
        }
        let mut validation_world = self.execution_world_without_persistence()?;
        validation_world
            .bootstrap_provider_backed_authorities(authorities)
            .map_err(|error| {
                format!(
                    "apply ProviderBacked authority bootstrap for canonical execution commit failed: {error:?}"
                )
            })?;
        self.execution_world = validation_world;
        Ok(())
    }
}

pub(super) fn decode_provider_backed_bootstrap_execution_input(
    input: &NodeReplicatedExecutionInputV1,
) -> Result<Vec<ProviderBackedBootstrapAuthorityV1>, String> {
    if input.kind != PROVIDER_BACKED_BOOTSTRAP_EXECUTION_INPUT_KIND {
        return Err(format!(
            "unsupported replicated execution input kind {}",
            input.kind
        ));
    }
    if input.target_height == 0 {
        return Err(
            "ProviderBacked authority bootstrap input must be bound to a commit height".to_string(),
        );
    }
    let authorities = serde_cbor::from_slice::<Vec<ProviderBackedBootstrapAuthorityV1>>(
        input.payload_cbor.as_slice(),
    )
    .map_err(|error| {
        format!("decode ProviderBacked authority bootstrap replicated input failed: {error}")
    })?;
    if authorities.is_empty() {
        return Err("ProviderBacked authority bootstrap input cannot be empty".to_string());
    }
    Ok(authorities)
}

/// Load explicit authority bundles before entering the chain runtime loop.
/// The input is entirely caller supplied; an empty list leaves the writer
/// unchanged and no authority or allowance is synthesized.
#[cfg(any(not(test), feature = "test_tier_required"))]
fn load_provider_backed_bootstrap_authorities(
    paths: &[PathBuf],
) -> Result<Vec<ProviderBackedBootstrapAuthorityV1>, String> {
    paths
        .iter()
        .map(|path| {
            let bytes = fs::read(path).map_err(|error| {
                format!(
                    "failed to read ProviderBacked authority bundle {}: {error}",
                    path.display()
                )
            })?;
            serde_json::from_slice::<ProviderBackedBootstrapAuthorityV1>(&bytes).map_err(|error| {
                format!(
                    "failed to decode ProviderBacked authority bundle {}: {error}",
                    path.display()
                )
            })
        })
        .collect()
}

/// Load the current execution world into a detached validation copy.
///
/// Provider bootstrap is an explicit producer input, so semantic validation
/// must happen before it enters the consensus queue. The validation copy must
/// retain the exact current snapshot/journal/module state while preventing a
/// successful preflight from publishing any runtime or cognition changes.
#[cfg(any(not(test), feature = "test_tier_required"))]
fn detached_execution_world_for_bootstrap(world_dir: &Path) -> Result<RuntimeWorld, String> {
    // Load the durable world exactly as it was persisted. The execution
    // driver's `load_execution_world` helper applies a storage-profile
    // normalization (including production main-token defaults) after load;
    // applying that policy here would change the state root before preflight
    // and reject an otherwise valid current world as stale.
    let current = oasis7::runtime_host::world::load_from_dir(world_dir).map_err(|error| {
        format!(
            "load current execution world for ProviderBacked bootstrap preflight failed: {error:?}"
        )
    })?;
    let snapshot = current.snapshot();
    let has_inline_module_artifacts = !snapshot.module_artifact_bytes.is_empty();
    let journal = current.journal().clone();
    let release_security_policy = current.release_security_policy().clone();
    let mut detached = oasis7::runtime_host::world::from_snapshot(snapshot, journal)
        .map_err(|error| {
            format!(
            "clone current execution world for ProviderBacked bootstrap preflight failed: {error:?}"
        )
        })?
        .with_release_security_policy(release_security_policy);
    if !has_inline_module_artifacts {
        detached
            .load_module_store_from_dir(world_dir)
            .map_err(|error| {
                format!(
                    "load execution module store for ProviderBacked bootstrap preflight failed: {error:?}"
                )
            })?;
    }
    Ok(detached)
}

#[cfg(any(not(test), feature = "test_tier_required"))]
fn validate_provider_backed_bootstrap_before_admission(
    world_dir: &Path,
    authorities: &[ProviderBackedBootstrapAuthorityV1],
) -> Result<(), String> {
    let mut detached_world = detached_execution_world_for_bootstrap(world_dir)?;
    detached_world
        .bootstrap_provider_backed_authorities(authorities)
        .map_err(|error| {
            format!(
                "ProviderBacked authority bootstrap preflight rejected before consensus admission: {error:?}"
            )
        })?;
    Ok(())
}

/// Translate explicit ProviderBacked authority bundles into one producer-side
/// replicated execution input. The input is queued in the node consensus
/// engine and is committed only when the authoritative proposer includes it;
/// materializing nodes receive the exact ordered bytes through the block.
#[cfg(any(not(test), feature = "test_tier_required"))]
pub(crate) fn publish_provider_backed_bootstrap_from_paths(
    runtime: &NodeRuntime,
    world_dir: &Path,
    paths: &[PathBuf],
) -> Result<(), String> {
    if paths.is_empty() {
        return Ok(());
    }
    let authorities = load_provider_backed_bootstrap_authorities(paths)?;
    validate_provider_backed_bootstrap_before_admission(world_dir, authorities.as_slice())?;
    let payload_cbor = serde_cbor::to_vec(&authorities).map_err(|error| {
        format!("encode ProviderBacked authority bootstrap replicated input failed: {error}")
    })?;
    runtime
        .submit_replicated_execution_input(NodeReplicatedExecutionInputV1::new(
            PROVIDER_BACKED_BOOTSTRAP_EXECUTION_INPUT_KIND,
            0,
            payload_cbor,
        ))
        .map_err(|error| {
            format!("queue ProviderBacked authority bootstrap replicated input failed: {error}")
        })
}
