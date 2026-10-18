use std::fs;
use std::path::Path;

use oasis7_runtime::{
    ChainResourceDerivationContext, MainTokenConfig, MainTokenSupplyState, ReleaseSecurityPolicy,
    World as RuntimeWorld, production_hardened_main_token_config,
};

use super::ExecutionBridgeState;

pub(crate) fn load_execution_bridge_state(path: &Path) -> Result<ExecutionBridgeState, String> {
    if !path.exists() {
        return Ok(ExecutionBridgeState::default());
    }
    let bytes = fs::read(path).map_err(|err| {
        format!(
            "read execution bridge state {} failed: {}",
            path.display(),
            err
        )
    })?;
    serde_json::from_slice::<ExecutionBridgeState>(bytes.as_slice()).map_err(|err| {
        format!(
            "parse execution bridge state {} failed: {}",
            path.display(),
            err
        )
    })
}

pub(crate) fn persist_execution_bridge_state(
    path: &Path,
    state: &ExecutionBridgeState,
) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(state)
        .map_err(|err| format!("serialize execution bridge state failed: {}", err))?;
    super::write_bytes_atomic(path, bytes.as_slice())
}

pub(crate) fn load_execution_world(world_dir: &Path) -> Result<RuntimeWorld, String> {
    load_execution_world_with_policy(world_dir, ReleaseSecurityPolicy::production_hardened())
}

pub(crate) fn remove_partial_execution_world_persistence_files(
    world_dir: &Path,
) -> Result<(), String> {
    let snapshot_path = world_dir.join("snapshot.json");
    let journal_path = world_dir.join("journal.json");
    let snapshot_exists = snapshot_path.exists();
    let journal_exists = journal_path.exists();
    if snapshot_exists == journal_exists {
        return Ok(());
    }
    for path in [snapshot_path, journal_path] {
        match fs::remove_file(path.as_path()) {
            Ok(()) => {}
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => {
                return Err(format!(
                    "remove partial execution world persistence file {} failed: {}",
                    path.display(),
                    err
                ));
            }
        }
    }
    Ok(())
}

pub(crate) fn execution_world_persistence_files_missing(world_dir: &Path) -> bool {
    let snapshot_path = world_dir.join("snapshot.json");
    let journal_path = world_dir.join("journal.json");
    !snapshot_path.exists() || !journal_path.exists()
}

fn execution_world_has_pristine_main_token_state(world: &RuntimeWorld) -> bool {
    let state = world.state();
    state.main_token_supply == MainTokenSupplyState::default()
        && state.main_token_balances.is_empty()
        && state.main_token_genesis_buckets.is_empty()
        && state.main_token_epoch_issuance_records.is_empty()
        && state.main_token_treasury_balances.is_empty()
        && state.main_token_claim_nonces.is_empty()
        && state.main_token_transfer_nonces.is_empty()
        && state.main_token_scheduled_policy_updates.is_empty()
        && state.main_token_treasury_distribution_records.is_empty()
        && state.main_token_node_points_bridge_records.is_empty()
        && state
            .restricted_starter_claim_liveops_pool_top_up_records
            .is_empty()
}

fn normalize_execution_world_main_token_config_for_policy(
    world: &mut RuntimeWorld,
    release_security_policy: ReleaseSecurityPolicy,
) {
    if release_security_policy.is_production_hardened() {
        if world.main_token_config() == &MainTokenConfig::default() {
            world.set_main_token_config(production_hardened_main_token_config());
        }
        return;
    }

    if execution_world_has_pristine_main_token_state(world)
        && world.main_token_config() == &production_hardened_main_token_config()
    {
        world.set_main_token_config(MainTokenConfig::default());
    }
}

pub(crate) fn load_execution_world_with_policy(
    world_dir: &Path,
    release_security_policy: ReleaseSecurityPolicy,
) -> Result<RuntimeWorld, String> {
    let snapshot_path = world_dir.join("snapshot.json");
    let journal_path = world_dir.join("journal.json");
    if !snapshot_path.exists() || !journal_path.exists() {
        let mut world = oasis7::runtime_host::world::new_production_hardened()
            .with_release_security_policy(release_security_policy.clone());
        normalize_execution_world_main_token_config_for_policy(&mut world, release_security_policy);
        return Ok(world);
    }
    oasis7::runtime_host::world::load_from_dir(world_dir)
        .map_err(|err| {
            format!(
                "load execution world from {} failed: {:?}",
                world_dir.display(),
                err
            )
        })
        .map(|world| {
            let mut world = world.with_release_security_policy(release_security_policy.clone());
            normalize_execution_world_main_token_config_for_policy(
                &mut world,
                release_security_policy,
            );
            world
        })
}

pub(crate) fn persist_execution_world(
    world_dir: &Path,
    execution_world: &RuntimeWorld,
) -> Result<(), String> {
    execution_world.save_to_dir(world_dir).map_err(|err| {
        format!(
            "save execution world to {} failed: {:?}",
            world_dir.display(),
            err
        )
    })
}

pub(crate) fn persist_execution_world_with_chain_resource_context(
    world_dir: &Path,
    execution_world: &RuntimeWorld,
    chain_resource_context: ChainResourceDerivationContext<'_>,
    world_config_hash: impl Into<String>,
    generation_algorithm_hash: impl Into<String>,
) -> Result<(), String> {
    execution_world
        .save_to_dir_with_chain_resource_context(
            world_dir,
            chain_resource_context,
            world_config_hash,
            generation_algorithm_hash,
        )
        .map_err(|err| {
            format!(
                "save execution world to {} failed: {:?}",
                world_dir.display(),
                err
            )
        })
}
