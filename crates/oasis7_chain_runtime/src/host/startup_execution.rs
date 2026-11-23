//! Native startup alternatives. Keep platform and test/production selection at
//! named function boundaries rather than interleaving conditional blocks in
//! the orchestration function. Production operations and test stubs retain the
//! original selection and failure propagation.

use super::{CliOptions, NodeRuntimeExecutionDriver, RuntimePaths};
use crate::execution_config::DevLocalParticipantConfig;
use oasis7_node::{NodeExecutionBootstrap, NodeRuntime};
use oasis7_proto::storage_profile::StorageProfileConfig;
use oasis7_runtime::ReleaseSecurityPolicy;
use std::path::Path;

#[cfg(unix)]
pub(super) fn run_formal_live(config_path: &Path) -> Result<(), String> {
    super::formal_live_runtime::run(config_path)
}

#[cfg(not(unix))]
pub(super) fn run_formal_live(_config_path: &Path) -> Result<(), String> {
    Err("formal live runtime currently requires Unix native storage".into())
}

#[cfg(not(test))]
pub(super) fn resolve_local_execution_bootstrap(
    options: &CliOptions,
    paths: &RuntimePaths,
    release_security_policy: &ReleaseSecurityPolicy,
    participant_control: &DevLocalParticipantConfig,
) -> Result<Option<NodeExecutionBootstrap>, String> {
    let local_execution_bootstrap = options
        .local_test_provider_authority_path
        .as_ref()
        .filter(|_| {
            !(paths
                .execution_world_dir
                .join("world-service-identity.json")
                .exists()
                && paths.execution_records_dir.join("latest.json").exists())
        })
        .map(|_| {
            let world = super::execution_bridge::load_execution_world_with_policy(
                paths.execution_world_dir.as_path(),
                release_security_policy.clone(),
            )?;
            let finality_block_hash = options
                .local_test_provider_finality_block_hash
                .as_deref()
                .ok_or_else(|| "local test finality marker is missing".to_string())?;
            super::execution_bridge::derive_local_execution_bootstrap(
                paths.execution_world_dir.as_path(),
                paths.execution_records_dir.as_path(),
                options.world_id.as_str(),
                world.state().time,
                finality_block_hash,
                release_security_policy,
            )
        })
        .transpose()?;
    let local_execution_bootstrap = if paths
        .execution_world_dir
        .join("world-service-identity.json")
        .exists()
        && paths.execution_records_dir.join("latest.json").exists()
    {
        Some(
            super::execution_bridge::derive_service_execution_bootstrap_with_context(
                &paths.execution_world_dir,
                &paths.execution_records_dir,
                &paths.storage_root,
                &options.world_id,
                &super::execution_bridge::world_service_read::RestoreContext::new(
                    release_security_policy.clone(),
                    participant_control.authority.clone(),
                )?,
            )?,
        )
    } else {
        local_execution_bootstrap
    };
    Ok(local_execution_bootstrap)
}

#[cfg(test)]
pub(super) fn resolve_local_execution_bootstrap(
    _options: &CliOptions,
    _paths: &RuntimePaths,
    _release_security_policy: &ReleaseSecurityPolicy,
    _participant_control: &DevLocalParticipantConfig,
) -> Result<Option<NodeExecutionBootstrap>, String> {
    Ok(None)
}

#[cfg(not(test))]
pub(super) fn driver_with_local_bootstrap(
    paths: &RuntimePaths,
    storage_profile_config: &StorageProfileConfig,
    baseline: NodeExecutionBootstrap,
) -> Result<NodeRuntimeExecutionDriver, String> {
    NodeRuntimeExecutionDriver::new_with_local_bootstrap(
        paths.execution_bridge_state_path.clone(),
        paths.execution_world_dir.clone(),
        paths.execution_records_dir.clone(),
        paths.storage_root.clone(),
        storage_profile_config,
        baseline,
    )
}

#[cfg(test)]
pub(super) fn driver_with_local_bootstrap(
    paths: &RuntimePaths,
    storage_profile_config: &StorageProfileConfig,
    _baseline: NodeExecutionBootstrap,
) -> Result<NodeRuntimeExecutionDriver, String> {
    NodeRuntimeExecutionDriver::new_with_storage_profile(
        paths.execution_bridge_state_path.clone(),
        paths.execution_world_dir.clone(),
        paths.execution_records_dir.clone(),
        paths.storage_root.clone(),
        storage_profile_config,
    )
}

#[cfg(not(test))]
pub(super) fn publish_provider_bootstrap(
    runtime: &NodeRuntime,
    options: &CliOptions,
    paths: &RuntimePaths,
) -> Result<(), String> {
    let mut provider_bootstrap_authority_paths =
        options.provider_backed_bootstrap_authority_paths.clone();
    if let Some(local_authority_path) = options.local_test_provider_authority_path.as_ref()
        && !provider_bootstrap_authority_paths.contains(local_authority_path)
    {
        provider_bootstrap_authority_paths.push(local_authority_path.clone());
    }
    super::execution_bridge::publish_provider_backed_bootstrap_from_paths(
        runtime,
        paths.execution_world_dir.as_path(),
        provider_bootstrap_authority_paths.as_slice(),
    )?;
    Ok(())
}

#[cfg(test)]
pub(super) fn publish_provider_bootstrap(
    _runtime: &NodeRuntime,
    _options: &CliOptions,
    _paths: &RuntimePaths,
) -> Result<(), String> {
    Ok(())
}
