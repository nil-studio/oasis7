//! Native local action adapter reusable by one operating domain's OperationStore.
//! Stable action identity and payload bind local instance effects, not world truth.
use super::{
    LocalActionRecord, LocalInstance, LocalRuntimeArtifactV1, local_io as io,
    model::{CONFIG_FILE, NodeConfig},
    process,
};
use std::path::Path;

fn validate(action: &LocalActionRecord) -> Result<(), String> {
    if action.api_version != "oasis7.local-node-action/v1" || action.action_id.trim().is_empty() {
        return Err("invalid local action schema/identity".into());
    }
    let expected = match action.kind.as_str() {
        "start" => {
            if action.expected_generation.checked_add(1) != Some(action.target.generation) {
                return Err("start generation binding mismatch".into());
            }
            if let Some(artifact) = &action.runtime_artifact {
                validate_runtime_artifact(artifact)?;
                io::digest(&(
                    "start_package",
                    action.target.config_digest.clone(),
                    action.expected_generation,
                    artifact,
                ))?
            } else {
                io::digest(&(
                    "start",
                    action.target.config_digest.clone(),
                    action.expected_generation,
                ))?
            }
        }
        "stop" => {
            if action.expected_generation != action.target.generation
                || action.runtime_artifact.is_some()
            {
                return Err("stop generation binding mismatch".into());
            }
            io::digest(&(
                "stop",
                action.target.incarnation.clone(),
                action.target.generation,
                action.target.config_digest.clone(),
            ))?
        }
        _ => return Err("unsupported local action kind".into()),
    };
    if expected != action.payload_digest {
        return Err("local action payload digest mismatch".into());
    }
    Ok(())
}
/// Query exactly the original receipt. Never substitutes a later incarnation.
/// Unknown is not absence and must not be turned into another action identity.
pub(crate) fn query(data: &Path, action_id: &str) -> Result<Option<LocalActionRecord>, String> {
    let config = NodeConfig::load(&data.join(CONFIG_FILE))?;
    let path = process::action_path(&config.identity.data_dir, action_id)?;
    if !path.exists() {
        io::reject_symlinks(&path)?;
        return Ok(None);
    }
    let mut original: LocalActionRecord = io::read(&path)?;
    validate(&original)?;
    if original.action_id != action_id {
        return Err("local action file identity mismatch".into());
    }
    let current = process::read_instance(&config.identity.data_dir)?;
    let matching = current.filter(|i| {
        i.incarnation == original.target.incarnation
            && i.generation == original.target.generation
            && i.config_digest == original.target.config_digest
    });
    if let Some(current) = matching {
        if matches!(current.state.as_str(), "stopped" | "failed") {
            original.target = current;
        } else {
            let live = process::status(&config.identity.data_dir)?;
            if live["blocker"].is_null() {
                original.target =
                    serde_json::from_value(live["instance"].clone()).map_err(|e| e.to_string())?;
            } else {
                original.target.state = "unknown".into();
            }
        }
    } else if !matches!(original.target.state.as_str(), "stopped" | "failed") {
        // Stop and start share the original receiver's terminal record, retained
        // under its start action identity even after instance.json advances.
        let receipt = process::action_path(&config.identity.data_dir, &original.target.action_id)?;
        if receipt.exists() {
            let started: LocalActionRecord = io::read(&receipt)?;
            validate(&started)?;
            if started.kind == "start"
                && started.target.incarnation == original.target.incarnation
                && started.target.generation == original.target.generation
                && started.target.config_digest == original.target.config_digest
                && matches!(started.target.state.as_str(), "stopped" | "failed")
            {
                original.target = started.target;
            } else {
                original.target.state = "unknown".into();
            }
        } else {
            original.target.state = "unknown".into();
        }
    }
    Ok(Some(original))
}
/// CAS admission for a new local generation. Retrying the same action returns
/// its original receipt before any new receiver or current-artifact validation.
pub(crate) fn start(
    config_path: &Path,
    action_id: &str,
    expected_generation: u64,
    expected_config_digest: &str,
) -> Result<LocalActionRecord, String> {
    start_with_runtime_artifact(
        config_path,
        action_id,
        expected_generation,
        expected_config_digest,
        None,
    )
}

pub(crate) fn start_package(
    config_path: &Path,
    action_id: &str,
    expected_generation: u64,
    expected_config_digest: &str,
    artifact: &LocalRuntimeArtifactV1,
) -> Result<LocalActionRecord, String> {
    validate_runtime_artifact(artifact)?;
    start_with_runtime_artifact(
        config_path,
        action_id,
        expected_generation,
        expected_config_digest,
        Some(artifact),
    )
}

fn start_with_runtime_artifact(
    config_path: &Path,
    action_id: &str,
    expected_generation: u64,
    expected_config_digest: &str,
    runtime_artifact: Option<&LocalRuntimeArtifactV1>,
) -> Result<LocalActionRecord, String> {
    let config = NodeConfig::load(config_path)?;
    if let Some(original) = query(&config.identity.data_dir, action_id)? {
        if original.kind != "start"
            || original.expected_generation != expected_generation
            || original.target.config_digest != expected_config_digest
            || original.runtime_artifact.as_ref() != runtime_artifact
        {
            return Err("start action identity reused with a different request".into());
        }
        if original.target.state == "unknown" {
            let stored: LocalActionRecord =
                io::read(&process::action_path(&config.identity.data_dir, action_id)?)?;
            if stored.target.state == "queued" {
                process::start_with_action_using_runtime(
                    config_path,
                    Some(action_id),
                    Some(expected_generation),
                    runtime_artifact,
                )?;
                return query(&config.identity.data_dir, action_id)?
                    .ok_or("original queued action disappeared".into());
            }
        }
        return Ok(original);
    }
    let digest = io::digest(&config)?;
    if digest != expected_config_digest {
        return Err("local config changed before start action".into());
    }
    let current = process::read_instance(&config.identity.data_dir)?;
    if current.as_ref().map_or(0, |i| i.generation) != expected_generation {
        return Err("start expected local generation changed".into());
    }
    if current
        .as_ref()
        .is_some_and(|i| !matches!(i.state.as_str(), "stopped" | "failed"))
    {
        return Err("new start action requires an observed terminal generation; reuse the live instance or reconcile its original action".into());
    }
    process::start_with_action_using_runtime(
        config_path,
        Some(action_id),
        Some(expected_generation),
        runtime_artifact,
    )?;
    query(&config.identity.data_dir, action_id)?
        .ok_or("start receiver action not durably admitted".into())
}

fn validate_runtime_artifact(artifact: &LocalRuntimeArtifactV1) -> Result<(), String> {
    if artifact.api_version != "oasis7.local-package-runtime-artifact/v1"
        || !super::package::supports_profile(&artifact.profile)
        || artifact.operation_id.is_empty()
        || !is_sha256(&artifact.operation_request_digest)
        || artifact.package_version.is_empty()
        || artifact.commit.is_empty()
        || !artifact.runtime_path.is_absolute()
        || !is_sha256(&artifact.runtime_sha256)
        || !is_sha256(&artifact.buildinfo_sha256)
    {
        return Err("invalid verified package runtime binding".into());
    }
    if io::sha256(&artifact.runtime_path)? != artifact.runtime_sha256 {
        return Err("verified package runtime changed before start admission".into());
    }
    Ok(())
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
/// An explicit stop step fixes an exact original instance and action identity.
/// A late retry returns its original result or stays unknown; it never kills a
/// process selected only by an operating-domain label or PID.
pub(crate) fn stop(
    data: &Path,
    action_id: &str,
    expected: &LocalInstance,
    timeout_ms: u64,
) -> Result<LocalActionRecord, String> {
    if !(1..=60_000).contains(&timeout_ms) {
        return Err("unsupported stop wait bound".into());
    }
    let config = NodeConfig::load(&data.join(CONFIG_FILE))?;
    if let Some(original) = query(&config.identity.data_dir, action_id)? {
        if original.kind != "stop"
            || original.target.incarnation != expected.incarnation
            || original.target.generation != expected.generation
            || original.target.config_digest != expected.config_digest
        {
            return Err("stop action identity reused with a different target".into());
        }
        if matches!(original.target.state.as_str(), "stopped" | "failed") {
            return Ok(original);
        }
    }
    let current =
        process::read_instance(&config.identity.data_dir)?.ok_or("stop target instance missing")?;
    if current.incarnation != expected.incarnation
        || current.generation != expected.generation
        || current.config_digest != expected.config_digest
    {
        return Err("stale stop target; later incarnation is untouched".into());
    }
    if matches!(current.state.as_str(), "stopped" | "failed") {
        let _admission = process::admission_lock(&config.identity.data_dir)?;
        // Recheck after taking the admission fence; a concurrent replacement
        // must not acquire a terminal receipt for another incarnation.
        let locked =
            process::read_instance(&config.identity.data_dir)?.ok_or("stop target missing")?;
        if locked.incarnation != current.incarnation || locked.generation != current.generation {
            return Err("stop target changed before locked admission".into());
        }

        let action = LocalActionRecord {
            api_version: "oasis7.local-node-action/v1".into(),
            action_id: action_id.into(),
            kind: "stop".into(),
            payload_digest: io::digest(&(
                "stop",
                current.incarnation.clone(),
                current.generation,
                current.config_digest.clone(),
            ))?,
            expected_generation: current.generation,
            target: current,
            runtime_artifact: None,
        };
        process::save_action(&config.identity.data_dir, &action)?;
    } else {
        process::stop_with_action(&config, expected, timeout_ms, Some(action_id))?;
    }
    query(&config.identity.data_dir, action_id)?
        .ok_or("stop receiver action not durably admitted".into())
}
#[cfg(test)]
pub(crate) fn inspect(data: &Path) -> Result<serde_json::Value, String> {
    process::status(data)
}
pub(crate) fn status(data: &Path) -> Result<serde_json::Value, String> {
    process::status(data)
}
#[cfg(test)]
pub(crate) fn config_digest(config_path: &Path) -> Result<String, String> {
    io::digest(&NodeConfig::load(config_path)?)
}

#[derive(Clone, Debug, serde::Serialize)]
pub(crate) struct LocalBinding {
    pub world_id: String,
    pub chain_id: String,
    pub config_digest: String,
    pub data_dir: std::path::PathBuf,
    pub current: Option<LocalInstance>,
}
pub(crate) fn binding(config_path: &Path) -> Result<LocalBinding, String> {
    let config = NodeConfig::load(config_path)?;
    Ok(LocalBinding {
        world_id: config.identity.chain.world_id().to_string(),
        chain_id: config.identity.chain.chain_id().to_string(),
        config_digest: io::digest(&config)?,
        current: process::read_instance(&config.identity.data_dir)?,
        data_dir: config.identity.data_dir,
    })
}

/// Replay only a provably queued, never-running original admission. A queued
/// intent retains its exact incarnation/generation; arbitrary unknown outcomes
/// never become a fresh action or an authenticated stop request.
pub(crate) fn reconcile_queued_start(
    config_path: &Path,
    action_id: &str,
) -> Result<Option<LocalActionRecord>, String> {
    let config = NodeConfig::load(config_path)?;
    let observed = query(&config.identity.data_dir, action_id)?;
    let Some(original) = observed else {
        return Ok(None);
    };
    if original.kind != "start" || original.target.state != "unknown" {
        return Ok(Some(original));
    }
    let stored: LocalActionRecord =
        io::read(&process::action_path(&config.identity.data_dir, action_id)?)?;
    if stored.target.state != "queued" {
        return Ok(Some(original));
    }
    start(
        config_path,
        action_id,
        stored.expected_generation,
        &stored.target.config_digest,
    )
    .map(Some)
}

/// Native effects require a durable action receipt before spawn/signal. A queued
/// start pointer also counts as admission and must be reconciled, never erased.
/// Used only with the operating-domain writer lock held, after original query.
pub(crate) fn admission_absent(config_path: &Path, action_id: &str) -> Result<bool, String> {
    let config = NodeConfig::load(config_path)?;
    if query(&config.identity.data_dir, action_id)?.is_some() {
        return Ok(false);
    }
    let pointer = config.identity.data_dir.join("start-action.json");
    if pointer.exists() {
        let intent: LocalActionRecord = io::read(&pointer)?;
        validate(&intent)?;
        if intent.action_id == action_id {
            return Ok(false);
        }
    }
    Ok(true)
}

#[cfg(test)]
pub(crate) fn test_admission_lock(data: &Path) -> Result<std::fs::File, String> {
    process::admission_lock(data)
}
