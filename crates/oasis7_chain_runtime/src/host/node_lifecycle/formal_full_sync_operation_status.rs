//! Read-only projection of a versioned full-sync ledger.
use super::super::model::NodeConfig;
use super::operation_snapshot::{Snapshot, read_snapshot};
use oasis7_client_api::world_service_full_sync::FormalFullSyncCutV1;
use serde_json::{Value, json};
use std::{fs, io::ErrorKind};

pub(in super::super) fn observe(node: &NodeConfig) -> Option<Value> {
    let configured = node.identity.chain.budgeted_formal_full_sync()?;
    let ledger_path = node
        .identity
        .data_dir
        .join("state/full-sync/operation.json");
    let result = match fs::symlink_metadata(&ledger_path) {
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
        _ => read_snapshot(&ledger_path)
            .and_then(|snapshot| {
                let original: Value = serde_json::from_slice(
                    snapshot
                        .original_request_json
                        .as_deref()
                        .ok_or("ledger does not retain the original operation request")?,
                )
                .map_err(|error| error.to_string())?;
                let identity =
                    serde_json::to_value(&node.identity).map_err(|error| error.to_string())?;
                if snapshot.policy != configured.transfer_policy
                    || snapshot.material_limit != configured.max_total_sync_bytes
                    || original["node_identity"] != identity
                    || original["native_owner_binding"].as_str()
                        != Some(snapshot.identity.binding_digest.as_str())
                {
                    return Err("ledger differs from the node's exact operation binding".into());
                }
                project_operation(&snapshot)
            })
            .map(Some),
    };
    let (operation, blocker) = match result {
        Ok(operation) => (operation, "operation_worker_inactive"),
        Err(_) => (None, "operation_recovery_required"),
    };
    Some(json!({
        "api_version": "oasis7.formal-full-sync-status/v2",
        "runtime_profile": super::PROFILE,
        "state": "blocked",
        "ready": false,
        "can_propose": false,
        "state_root_verified": false,
        "world_loaded": false,
        "sync_scope": "issuer_granted_full_state_read_only",
        "world_id": &configured.world_id,
        "chain_id": &configured.chain_id,
        "last_verified_cut": null,
        "loaded_cut": null,
        "lag_positions": null,
        "checkpoint_age": { "source": "unknown", "elapsed_ms": null },
        "blocker": blocker,
        "operation": operation
    }))
}

fn project_operation(snapshot: &Snapshot) -> Result<Value, String> {
    let target = snapshot
        .verified_target_json
        .as_ref()
        .map(|bytes| {
            let original: Value =
                serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
            let cut: FormalFullSyncCutV1 = serde_json::from_value(
                original
                    .get(0)
                    .cloned()
                    .ok_or("operation does not retain its original target cut")?,
            )
            .map_err(|error| error.to_string())?;
            cut.validate()?;
            Ok::<_, String>(cut)
        })
        .transpose()?;
    let limits = &snapshot.policy;
    let charged = snapshot.charged()?;
    let near_capacity = [
        ("encoded_io_bytes", charged, limits.max_encoded_io_bytes),
        (
            "exchanges",
            snapshot.exchanges_started,
            limits.max_exchanges,
        ),
        ("proof_pages", snapshot.proof_pages, limits.max_proof_pages),
        ("proof_bytes", snapshot.proof_bytes, limits.max_proof_bytes),
        (
            "material_bytes",
            snapshot.material_bytes,
            snapshot.material_limit,
        ),
        ("attempts", snapshot.attempts_used, limits.max_attempts),
    ]
    .into_iter()
    .filter_map(|(name, used, limit)| {
        (u128::from(used) * 5 >= u128::from(limit) * 4).then_some(name)
    })
    .collect::<Vec<_>>();
    Ok(json!({
        "operation_id": &snapshot.identity.operation_id,
        "request_digest": &snapshot.identity.request_digest,
        "policy_digest": &snapshot.identity.policy_digest,
        "owner_binding_digest": &snapshot.identity.binding_digest,
        "target": target,
        "publication": match (&snapshot.publication, snapshot.publication_journal_version) {
            (Some(publication), Some(1)) if publication.completed => "pins_acknowledged",
            (Some(_), Some(1)) => "pending",
            (None, Some(1)) => "not_started",
            _ => "unknown",
        },
        "revision": snapshot.revision,
        "attempt_state": if snapshot.active_attempt_nonce.is_some() { "interrupted" } else { "inactive" },
        "limits": limits,
        "encoded_io": {
            "tx_actual_known": snapshot.encoded_tx_actual,
            "rx_actual_known": snapshot.encoded_rx_actual,
            "unresolved_reserved": snapshot.unresolved_reserved()?,
            "unknown_final_charged": snapshot.unknown_final_charged,
            "remaining": limits.max_encoded_io_bytes - charged
        },
        "exchanges_started": snapshot.exchanges_started,
        "proof_pages": snapshot.proof_pages,
        "proof_bytes": snapshot.proof_bytes,
        "material_limit": snapshot.material_limit,
        "material_bytes": snapshot.material_bytes,
        "material_objects": snapshot.materials.len(),
        "attempts_used": snapshot.attempts_used,
        "attempts_remaining": limits.max_attempts - snapshot.attempts_used,
        "elapsed_ms": null,
        "remaining_ms": null,
        "history_pages": null,
        "history_proof_bytes": null,
        "history_manifest_bytes": null,
        "reopen_limit_ms": limits.reopen_timeout_ms,
        "reopen_elapsed_ms": null,
        "reopen_result": null,
        "near_capacity": near_capacity,
    }))
}
