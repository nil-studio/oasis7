//! Stopped-only full-sync grant installation and terminal-bound signed renewal.
//! Keep native admission/ownership, monotonic issuer lease and original ledger
//! readback in the same caller-visible operation. No ledger bytes are rewritten.
use super::{
    FormalFullSyncConfigV2, grant_reconcile, grant_transition, now_unix_ms, operation_snapshot,
    validate_grant_for_node,
};
use super::super::{local_io, model, process};
use oasis7_client_api::world_service_full_sync::FormalNodeReadGrantV1;
use std::path::Path;

pub(super) fn install_grant(
    data_dir: &Path,
    grant_path: &Path,
) -> Result<serde_json::Value, String> {
    let config_path = data_dir.join(model::CONFIG_FILE);
    let config_path = local_io::regular(&config_path)?;
    let node = model::NodeConfig::load(&config_path)?;
    node.verify_identity()?;
    let instance_data = node.identity.data_dir.clone();
    let _admission = process::admission_lock(&instance_data)?;
    let _ownership = process::lock(&instance_data)?;
    let current = process::read_instance(&instance_data)?;
    if current
        .as_ref()
        .is_some_and(|instance| !matches!(instance.state.as_str(), "stopped" | "failed"))
    {
        return Err("full-sync grant changes require the exact node to be stopped".into());
    }
    grant_reconcile::require_completed_start(&instance_data, current.as_ref())?;
    let node = model::NodeConfig::load(&config_path)?;
    node.verify_identity()?;
    let previous_config = node
        .identity
        .chain
        .formal_full_sync()
        .cloned()
        .or_else(|| {
            node.identity
                .chain
                .budgeted_formal_full_sync()
                .map(FormalFullSyncConfigV2::read_material_config)
        })
        .ok_or("full-sync grant can only be installed for a formal full-state sync node")?;
    let grant: FormalNodeReadGrantV1 = local_io::read(grant_path)?;
    let (keypair, _) =
        super::super::super::node_keypair_config::read_existing_node_keypair_in_secure_config_dir(
            &instance_data.join("keys"),
        )?;
    let now = now_unix_ms().ok_or("system clock before Unix epoch")?;
    validate_grant_for_node(&previous_config, &grant, &keypair.public_key_hex, now)?;
    if let Some(previous) = &previous_config.grant {
        if grant != *previous
            && (grant.body.grant_epoch <= previous.body.grant_epoch
                || grant.body.authority_epoch < previous.body.authority_epoch)
        {
            return Err(
                "replacement full-sync grant must advance its grant epoch and authority floor"
                    .into(),
            );
        }
        if grant == *previous {
            return Ok(serde_json::json!({
                "installed": true,
                "changed": false,
                "grant_epoch": grant.body.grant_epoch,
                "authority_epoch": grant.body.authority_epoch,
                "config_digest": local_io::digest(&node)?
            }));
        }
    }
    let mut renewed = node.clone();
    *renewed
        .identity
        .chain
        .formal_full_sync_grant_mut()
        .ok_or("full-sync profile disappeared during grant install")? = Some(grant.clone());
    if node.identity.chain.budgeted_formal_full_sync().is_some() {
        let path = instance_data.join("state/full-sync/operation.json");
        match std::fs::symlink_metadata(&path) {
            Ok(_) => {
                let snapshot = operation_snapshot::read_snapshot(&path)?;
                let original: serde_json::Value = serde_json::from_slice(
                    snapshot
                        .original_request_json
                        .as_deref()
                        .ok_or("grant renewal requires the original durable request")?,
                )
                .map_err(|e| e.to_string())?;
                let renewed_config = renewed
                    .identity
                    .chain
                    .budgeted_formal_full_sync()
                    .ok_or("renewed budgeted configuration missing")?
                    .read_material_config();
                if !grant_transition::permits(
                    &snapshot,
                    &original,
                    &serde_json::to_value(&renewed.identity).map_err(|e| e.to_string())?,
                    &serde_json::to_value(&renewed_config).map_err(|e| e.to_string())?,
                )? {
                    return Err("grant renewal requires a completed original operation".into());
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound && current.is_none() => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Err("grant renewal cannot discard an existing operation ledger".into());
            }
            Err(error) => return Err(error.to_string()),
        }
    }
    // The ledger remains immutable. A completed terminal plus a monotonic,
    // independently signed grant permits the next cold startup to verify both
    // old and new credentials; there is no cross-file half-written ledger.
    local_io::write(&instance_data.join(model::CONFIG_FILE), &renewed)?;
    Ok(serde_json::json!({
        "installed": true,
        "changed": true,
        "grant_epoch": grant.body.grant_epoch,
        "authority_epoch": grant.body.authority_epoch,
        "config_digest": local_io::digest(&renewed)?
    }))
}
