//! Read-only, executable-derived persistence compatibility evidence.
//!
//! This command validates the same NodeConfig and World decoder used by the
//! process entrypoint. Its output describes software capability and the
//! exact local data cut it could read; it is not chain authority or an
//! operator-supplied compatibility claim.

use super::{local_io as io, model::NodeConfig};
use oasis7_runtime::World;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Read,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
};

#[cfg(not(test))]
use super::super::execution_bridge as execution_bridge_runtime;
#[cfg(test)]
use super::super::execution_bridge_real_tests::real_execution_bridge as execution_bridge_runtime;

const DESCRIPTOR_API: &str = "oasis7.node-runtime-compatibility/v1";

pub(super) fn describe_with_minimum(
    config_path: &Path,
    requested_data_dir: Option<&Path>,
    minimum_cut_path: Option<&Path>,
    minimum_cut_sha256: Option<&str>,
) -> Result<Value, String> {
    let (minimum_cut, minimum_cut_sha256) = read_minimum_cut(minimum_cut_path, minimum_cut_sha256)?;
    let config_path = io::regular(config_path)?;
    let config_file_sha256 = io::sha256(&config_path)?;
    let config = NodeConfig::load(&config_path)?;
    let config_digest = io::digest(&config)?;
    config.verify_identity()?;
    config.identity.chain.verify()?;

    let bound_data_dir = &config.identity.data_dir;
    let canonical_data_dir = canonical_directory(bound_data_dir)?;
    if &canonical_data_dir != bound_data_dir {
        return Err("node data directory is not canonical".into());
    }
    if let Some(requested) = requested_data_dir {
        let requested = canonical_directory(requested)?;
        if requested != canonical_data_dir {
            return Err(
                "requested data directory differs from the exact config-bound directory".into(),
            );
        }
    }

    // Cargo and package entry points may name the loaded image through a
    // symlink. Resolve only this kernel-reported executable path; configured
    // data and configuration paths retain their no-symlink admission.
    let executable_identity = loaded_executable_identity()?;
    let executable = executable_identity.path.clone();
    let executable_sha256 = executable_identity.sha256.clone();
    let runtime_profile = config.identity.chain.runtime_profile().to_string();
    let (software_formats, stored_probe) = match runtime_profile.as_str() {
        super::model::PROFILE => {
            let probe = probe_compatibility_state(&config)?;
            (compatibility_formats(), probe)
        }
        super::formal_observer::PROFILE => {
            let status_path = canonical_data_dir.join(super::formal_observer::STATUS_FILE);
            let probe = probe_optional_status(
                &status_path,
                "oasis7.formal-public-observer-status/v1",
                &config,
            )?;
            (formal_observer_formats(), probe)
        }
        super::formal_full_sync::PROFILE => {
            let status_path = canonical_data_dir.join(super::formal_full_sync::STATUS_FILE);
            let probe = probe_full_sync_state(
                &status_path,
                &canonical_data_dir,
                &config,
                minimum_cut.as_ref(),
            )?;
            (formal_full_sync_formats(), probe)
        }
        _ => {
            return Err(format!(
                "unsupported local runtime profile: {runtime_profile}"
            ));
        }
    };

    if minimum_cut.is_some() && runtime_profile != super::formal_full_sync::PROFILE {
        return Err("minimum full-sync cut is only valid for the formal full-sync profile".into());
    }

    if io::sha256(&config_path)? != config_file_sha256 {
        return Err(
            "node configuration changed while the compatibility probe was reading it".into(),
        );
    }
    if loaded_executable_identity()? != executable_identity {
        return Err(
            "loaded executable changed while the compatibility descriptor was being built".into(),
        );
    }

    Ok(json!({
        "api_version": DESCRIPTOR_API,
        "descriptor_source": "this executable's compiled persistence decoders",
        "executable_path": executable,
        "executable_sha256": executable_sha256,
        "config_path": config_path,
        "config_digest": config_digest,
        "config_sha256": config_file_sha256,
        "runtime_profile": runtime_profile,
        "world_id": config.identity.chain.world_id(),
        "chain_id": config.identity.chain.chain_id(),
        "data_dir": canonical_data_dir,
        "software_formats": software_formats,
        "stored_state_probe": stored_probe,
        "minimum_full_sync_cut_sha256": minimum_cut_sha256,
        "process_started": false,
        "world_or_authority_created": false,
        "chain_authority_claim": false
    }))
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct LoadedExecutableIdentity {
    path: PathBuf,
    sha256: String,
    device: u64,
    inode: u64,
}

fn loaded_executable_identity() -> Result<LoadedExecutableIdentity, String> {
    let mut image = crate::executable_image::open_loaded_image()?;
    crate::executable_image::validate_loaded_file(&image)?;
    let before = image.metadata().map_err(|error| error.to_string())?;
    let path = fs::canonicalize(std::env::current_exe().map_err(|error| error.to_string())?)
        .map_err(|error| error.to_string())?;
    let named = fs::File::open(&path).map_err(|error| error.to_string())?;
    let named_before = named.metadata().map_err(|error| error.to_string())?;
    if !named_before.is_file()
        || named_before.dev() != before.dev()
        || named_before.ino() != before.ino()
    {
        return Err("loaded executable pathname differs from the actual process image".into());
    }
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let count = image.read(&mut buffer).map_err(|error| error.to_string())?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    let sha256 = hex::encode(hasher.finalize());
    let after = image.metadata().map_err(|error| error.to_string())?;
    let named_after = named.metadata().map_err(|error| error.to_string())?;
    crate::executable_image::validate_loaded_file(&image)?;
    if !crate::executable_image::unchanged(&before, &after)
        || !crate::executable_image::unchanged(&named_before, &named_after)
        || named_after.dev() != before.dev()
        || named_after.ino() != before.ino()
    {
        return Err("loaded executable pathname or bytes changed while being verified".into());
    }
    Ok(LoadedExecutableIdentity {
        path,
        sha256,
        device: before.dev(),
        inode: before.ino(),
    })
}

fn compatibility_formats() -> Value {
    json!({
        "world_persistence": {
            "selector_api": World::persistence_selector_api(),
            "read_versions": [
                World::legacy_persistence_format_version(),
                World::current_persistence_format_version()
            ],
            "write_version": World::current_persistence_format_version(),
            "snapshot_decoder": World::SNAPSHOT_PERSISTENCE_DECODER,
            "journal_decoder": World::JOURNAL_PERSISTENCE_DECODER,
            "distfs_sidecar_decoder": World::SIDECAR_PERSISTENCE_DECODER,
            "module_store_decoder": World::MODULE_STORE_PERSISTENCE_DECODER
        },
        "execution_bridge_record": {
            "read_versions": [
                execution_bridge_runtime::EXECUTION_BRIDGE_RECORD_SCHEMA_V1,
                execution_bridge_runtime::EXECUTION_BRIDGE_RECORD_SCHEMA_V2,
                execution_bridge_runtime::EXECUTION_BRIDGE_RECORD_SCHEMA_V3
            ],
            "write_version": execution_bridge_runtime::EXECUTION_BRIDGE_RECORD_SCHEMA_V3
        },
        "execution_checkpoint_manifest": {
            "read_versions": [
                execution_bridge_runtime::EXECUTION_CHECKPOINT_MANIFEST_SCHEMA_V1,
                execution_bridge_runtime::EXECUTION_CHECKPOINT_MANIFEST_SCHEMA_V2
            ],
            "write_version": execution_bridge_runtime::EXECUTION_CHECKPOINT_MANIFEST_SCHEMA_V2
        }
    })
}

fn formal_observer_formats() -> Value {
    json!({
        "local_status": {
            "api_version": "oasis7.formal-public-observer-status/v1",
            "read_versions": [1],
            "write_version": 1
        },
        "local_world_and_execution_history": "not_materialized_by_this_profile"
    })
}

fn formal_full_sync_formats() -> Value {
    json!({
        "local_status": {
            "api_version": "oasis7.formal-full-sync-status/v1",
            "read_versions": [1],
            "write_version": 1
        },
        "external_checkpoint_and_replay_history": {
            "history_head_schema": 1,
            "history_manifest_schema": 1,
            "page_api": "oasis7.formal-world-read-history-page/v1"
        }
    })
}

fn probe_compatibility_state(config: &NodeConfig) -> Result<Value, String> {
    let state_root = config.identity.data_dir.join("state");
    let world_dir = state_root.join("world");
    let records_dir = state_root.join("records");
    let storage_dir = state_root.join("cas");
    let world_exists = path_exists_no_symlink(&world_dir)?;
    let world_has_entries = world_exists && directory_has_entries(&world_dir)?;
    let identity_path = world_dir.join("world-service-identity.json");
    let latest_path = records_dir.join("latest.json");
    let identity_exists = path_exists_no_symlink(&identity_path)?;
    let latest_exists = path_exists_no_symlink(&latest_path)?;
    let records_have_entries =
        path_exists_no_symlink(&records_dir)? && directory_has_entries(&records_dir)?;

    if !world_has_entries && !identity_exists && !latest_exists && !records_have_entries {
        return Ok(json!({
            "state": "uninitialized",
            "world_loaded": false,
            "execution_history_verified": false
        }));
    }

    if !world_has_entries {
        return Err("persisted execution metadata exists without a materialized World".into());
    }
    let world = super::super::runtime_authority::load_persisted_world_exact(&world_dir)?;
    let selector_path = world_dir.join("world-format.json");
    let selector_sha256 = if path_exists_no_symlink(&selector_path)? {
        Some(io::sha256(&selector_path)?)
    } else {
        None
    };

    let has_any_execution_head = identity_exists || latest_exists || records_have_entries;
    let execution_head = if has_any_execution_head {
        if !identity_exists || !latest_exists {
            return Err(
                "execution history has no exact paired world identity and latest record".into(),
            );
        }
        let latest: execution_bridge_runtime::ExecutionBridgeRecord = io::read(&latest_path)?;
        if !(execution_bridge_runtime::EXECUTION_BRIDGE_RECORD_SCHEMA_V1
            ..=execution_bridge_runtime::EXECUTION_BRIDGE_RECORD_SCHEMA_V3)
            .contains(&latest.schema_version)
        {
            return Err(format!(
                "unsupported execution record schema {}",
                latest.schema_version
            ));
        }
        if latest.world_id != config.identity.chain.world_id() {
            return Err("execution latest record belongs to another world".into());
        }
        let highest_record_height = highest_declared_record_height(&records_dir)?;
        if highest_record_height != Some(latest.height) {
            return Err(format!(
                "execution latest pointer height {} is not the highest declared numbered record {:?}",
                latest.height, highest_record_height
            ));
        }
        let numbered_path = records_dir.join(format!("{:020}.json", latest.height));
        let numbered: execution_bridge_runtime::ExecutionBridgeRecord = io::read(&numbered_path)?;
        if numbered != latest {
            return Err("latest execution record differs from the exact numbered record".into());
        }
        let world_identity = execution_bridge_runtime::world_service_read::identity(
            &world_dir,
            config.identity.chain.world_id(),
        )?;
        // Pin one exact current record by height. This reads and verifies its
        // Snapshot, Journal, and proof CAS closure without scanning old heights.
        let pinned = execution_bridge_runtime::world_service_read::pin_at_height(
            &records_dir,
            &storage_dir,
            &world_identity,
            latest.height,
        )?;
        // `world/` is a materialized startup cache. Normal commits may leave
        // it behind the authoritative execution head; startup first decodes
        // this supported World format, then reconstructs the current head
        // from the exact record and its Snapshot/Journal CAS closure.
        let loaded_world_root = decoded_world_snapshot_root(&world)?;
        let pinned_world_root = decoded_world_snapshot_root(&pinned.world)?;
        if pinned_world_root != latest.execution_state_root {
            return Err(format!(
                "pinned execution World does not reproduce the exact latest execution snapshot root: record={} pinned={}",
                latest.execution_state_root, pinned_world_root
            ));
        }
        Some(json!({
            "schema_version": latest.schema_version,
            "height": latest.height,
            "execution_block_hash": latest.execution_block_hash,
            "execution_state_root": latest.execution_state_root,
            "head_source": "verified_record_and_snapshot_journal_cas",
            "decoded_world_snapshot_root": loaded_world_root,
            "world_cache_matches_execution_head": loaded_world_root == latest.execution_state_root,
            "pinned_world_snapshot_root": pinned_world_root
        }))
    } else {
        return Err(
            "materialized World has no exact latest execution record and CAS root; compatibility is unproven"
                .into(),
        );
    };

    Ok(json!({
        "state": "verified",
        "world_loaded": true,
        "world_time": world.state().time,
        "world_format_version": if selector_sha256.is_some() {
            World::current_persistence_format_version()
        } else {
            World::legacy_persistence_format_version()
        },
        "world_selector_sha256": selector_sha256,
        "execution_history_verified": execution_head.is_some(),
        "execution_head": execution_head
    }))
}

fn decoded_world_snapshot_root(world: &World) -> Result<String, String> {
    let snapshot = serde_cbor::to_vec(&world.snapshot())
        .map_err(|error| format!("serialize decoded World snapshot: {error}"))?;
    Ok(oasis7_runtime::blake3_hex(&snapshot))
}

/// Return the maximum height declared by a numbered execution-record file.
/// A malformed higher filename is an error instead of an invitation to fall
/// back to a lower, parseable record.
fn highest_declared_record_height(records_dir: &Path) -> Result<Option<u64>, String> {
    let mut highest = None;
    for entry in fs::read_dir(records_dir)
        .map_err(|error| format!("read execution records directory: {error}"))?
    {
        let entry = entry.map_err(|error| format!("read execution records entry: {error}"))?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| "execution records contain a non-UTF8 filename".to_string())?;
        if name == "latest.json" || !name.ends_with(".json") {
            continue;
        }
        let Some(stem) = name.strip_suffix(".json") else {
            return Err(format!(
                "numbered execution record filename is malformed: {name}"
            ));
        };
        if !stem.as_bytes().first().is_some_and(u8::is_ascii_digit) {
            continue;
        }
        let file_type = entry
            .file_type()
            .map_err(|error| format!("inspect execution record {name}: {error}"))?;
        if file_type.is_symlink() || !file_type.is_file() {
            return Err(format!(
                "numbered execution record is not a regular file: {name}"
            ));
        }
        let height = stem.parse::<u64>().map_err(|error| {
            format!("malformed numbered execution record filename {name}: {error}")
        })?;
        if name != format!("{height:020}.json") {
            return Err(format!(
                "numbered execution record filename is not canonical: {name}"
            ));
        }
        highest = Some(highest.map_or(height, |current: u64| current.max(height)));
    }
    Ok(highest)
}

fn probe_optional_status(
    path: &Path,
    expected_api: &str,
    config: &NodeConfig,
) -> Result<Value, String> {
    if !path_exists_no_symlink(path)? {
        return Ok(json!({ "state": "uninitialized", "status_loaded": false }));
    }
    let raw: Value = io::read(path)?;
    let status: super::formal_observer::FormalObserverStatusV1 =
        serde_json::from_value(raw.clone())
            .map_err(|error| format!("decode formal observer persisted status: {error}"))?;
    let canonical = serde_json::to_value(&status)
        .map_err(|error| format!("re-encode formal observer status: {error}"))?;
    let config_digest = io::digest(config)?;
    if status.api_version != expected_api
        || status.runtime_profile != super::formal_observer::PROFILE
        || status.config_digest != config_digest
        || status.can_propose
        || status.state_sync_scope != "public_projection_only"
        || canonical != raw
    {
        return Err("formal observer status has an unsupported persisted format".into());
    }
    Ok(json!({
        "state": "verified",
        "status_loaded": true,
        "status_sha256": io::sha256(path)?,
        "read_ready": status.read_ready,
        "last_verified_commit": status.last_verified_commit,
        "public_projection_sha256": status.public_projection.as_ref().map(io::digest).transpose()?
    }))
}

fn probe_full_sync_state(
    status_path: &Path,
    data_dir: &Path,
    config: &NodeConfig,
    minimum_cut: Option<&oasis7_client_api::world_service_full_sync::FormalFullSyncCutV1>,
) -> Result<Value, String> {
    let formal = config
        .identity
        .chain
        .formal_full_sync()
        .ok_or("full-sync runtime compatibility probe received another node profile")?;
    super::formal_full_sync::probe_runtime_compatibility(
        formal,
        config,
        status_path,
        data_dir,
        minimum_cut,
    )
}

fn read_minimum_cut(
    path: Option<&Path>,
    expected_sha256: Option<&str>,
) -> Result<
    (
        Option<oasis7_client_api::world_service_full_sync::FormalFullSyncCutV1>,
        Option<String>,
    ),
    String,
> {
    match (path, expected_sha256) {
        (None, None) => Ok((None, None)),
        (Some(path), Some(expected)) => {
            if expected.len() != 64
                || !expected
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            {
                return Err("minimum full-sync cut SHA-256 must be 64 lowercase hex digits".into());
            }
            let canonical = io::regular(path)?;
            let file = fs::File::open(&canonical).map_err(|error| error.to_string())?;
            if !file
                .metadata()
                .map_err(|error| error.to_string())?
                .is_file()
            {
                return Err("minimum full-sync cut is not a regular file".into());
            }
            let mut bytes = Vec::new();
            file.take(16 * 1024 + 1)
                .read_to_end(&mut bytes)
                .map_err(|error| error.to_string())?;
            if bytes.len() > 16 * 1024 {
                return Err("minimum full-sync cut exceeds its size bound".into());
            }
            let digest = hex::encode(Sha256::digest(&bytes));
            if digest != expected {
                return Err(
                    "minimum full-sync cut bytes differ from the immutable stage digest".into(),
                );
            }
            let cut: oasis7_client_api::world_service_full_sync::FormalFullSyncCutV1 =
                serde_json::from_slice(&bytes)
                    .map_err(|error| format!("decode minimum full-sync cut: {error}"))?;
            cut.validate()?;
            if serde_json::to_vec_pretty(&cut).map_err(|error| error.to_string())? != bytes {
                return Err("minimum full-sync cut is not canonical JSON".into());
            }
            Ok((Some(cut), Some(digest)))
        }
        _ => Err("minimum full-sync cut path and SHA-256 must be supplied together".into()),
    }
}

fn canonical_directory(path: &Path) -> Result<std::path::PathBuf, String> {
    io::reject_symlinks(path)?;
    if !fs::metadata(path)
        .map_err(|error| error.to_string())?
        .is_dir()
    {
        return Err(format!("not a directory: {}", path.display()));
    }
    fs::canonicalize(path).map_err(|error| error.to_string())
}

fn path_exists_no_symlink(path: &Path) -> Result<bool, String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(format!(
            "symlink refused in format probe: {}",
            path.display()
        )),
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(format!("inspect {}: {error}", path.display())),
    }
}

fn directory_has_entries(path: &Path) -> Result<bool, String> {
    let mut entries = fs::read_dir(path).map_err(|error| error.to_string())?;
    let Some(entry) = entries.next() else {
        return Ok(false);
    };
    let entry = entry.map_err(|error| error.to_string())?;
    if entry
        .file_type()
        .map_err(|error| error.to_string())?
        .is_symlink()
    {
        return Err(format!(
            "symlink refused in format probe: {}",
            entry.path().display()
        ));
    }
    Ok(true)
}

#[cfg(test)]
#[path = "runtime_compatibility_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "runtime_compatibility_integration_tests.rs"]
mod integration_tests;
