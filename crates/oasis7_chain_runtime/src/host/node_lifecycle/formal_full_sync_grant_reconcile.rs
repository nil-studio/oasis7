//! Grant renewal observes the original durable lifecycle receipt. The immutable
//! admission pointer is neither deleted nor mistaken for a current result.
use super::super::{LocalActionRecord, LocalInstance, actions, local_io, process};
use std::path::Path;

/// Caller holds both admission and process ownership locks. Only a terminal
/// canonical receipt for exactly the pointer's instance resolves admission.
pub(super) fn require_completed_start(
    data: &Path,
    current: Option<&LocalInstance>,
) -> Result<(), String> {
    let pointer_path = data.join("start-action.json");
    match std::fs::symlink_metadata(&pointer_path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            // Absence is initial admission only when no incarnation or indexed
            // lifecycle receipt exists. Inspect one directory entry, not history.
            let actions_path = data.join("actions");
            local_io::reject_symlinks(&actions_path)?;
            let indexed = match std::fs::symlink_metadata(&actions_path) {
                Ok(metadata) => {
                    if !metadata.is_dir() {
                        return Err("full-sync grant action index is not a directory".into());
                    }
                    super::super::super::controlled_live_config::require_private_owned(
                        &actions_path,
                        true,
                    )?;
                    std::fs::read_dir(&actions_path)
                        .map_err(|e| e.to_string())?
                        .next()
                        .transpose()
                        .map_err(|e| e.to_string())?
                        .is_some()
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
                Err(error) => return Err(error.to_string()),
            };
            let instance_path = data.join("instance.json");
            local_io::reject_symlinks(&instance_path)?;
            let instance_exists = match std::fs::symlink_metadata(&instance_path) {
                Ok(_) => true,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
                Err(error) => return Err(error.to_string()),
            };
            if current.is_some() || indexed || instance_exists {
                return Err(
                    "full-sync grant original start pointer missing for existing lifecycle".into(),
                );
            }
            return Ok(());
        }
        Err(error) => return Err(error.to_string()),
        Ok(_) => {}
    }
    let pointer: LocalActionRecord = local_io::read(&pointer_path)?;
    let receipt_path = process::action_path(data, &pointer.action_id)?;
    let receipt: LocalActionRecord = local_io::read(&receipt_path)
        .map_err(|error| format!("full-sync grant original start receipt unavailable: {error}"))?;
    if pointer.api_version != "oasis7.local-node-action/v1"
        || pointer.kind != "start"
        || receipt.api_version != pointer.api_version
        || receipt.kind != pointer.kind
        || receipt.action_id != pointer.action_id
        || receipt.payload_digest != pointer.payload_digest
        || receipt.expected_generation != pointer.expected_generation
        || pointer.target.api_version != "oasis7.local-node-instance/v1"
        || receipt.target.api_version != pointer.target.api_version
        || pointer.target.action_id != pointer.action_id
        || receipt.target.action_id != pointer.target.action_id
        || receipt.target.incarnation != pointer.target.incarnation
        || receipt.target.generation != pointer.target.generation
        || receipt.target.config_digest != pointer.target.config_digest
        || receipt.target.started_unix_ms != pointer.target.started_unix_ms
        || !matches!(receipt.target.state.as_str(), "stopped" | "failed")
        || current != Some(&receipt.target)
    {
        return Err(
            "full-sync grant changes are blocked by an unresolved original start receipt/instance"
                .into(),
        );
    }
    // The ordinary canonical query validates its immutable payload digest,
    // generation and indexed action identity. It cannot promote a nonterminal
    // receipt here because that case was rejected before this call.
    let validated = actions::query(data, &pointer.action_id)?
        .ok_or("full-sync grant original indexed start receipt missing")?;
    if validated.target != receipt.target
        || validated.payload_digest != receipt.payload_digest
        || validated.action_id != receipt.action_id
        || validated.expected_generation != receipt.expected_generation
        || validated.kind != "start"
    {
        return Err(
            "full-sync grant original start validation changed its exact terminal result".into(),
        );
    }
    Ok(())
}
