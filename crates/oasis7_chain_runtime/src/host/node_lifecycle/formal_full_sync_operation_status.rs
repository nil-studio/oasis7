//! Read-only projection of a versioned full-sync ledger.
use super::super::model::NodeConfig;
use super::operation_snapshot::{Snapshot, read_snapshot};
use oasis7_client_api::world_service_full_sync::FormalFullSyncCutV1;
use serde_json::{Value, json};
use std::{fs, io::ErrorKind};

pub(in super::super) fn observe(node: &NodeConfig) -> Option<Value> {
    let configured = node.identity.chain.budgeted_formal_full_sync()?;
    let result = read_bound_snapshot(node)
        .and_then(|snapshot| snapshot.as_ref().map(project_operation).transpose());
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

fn read_bound_snapshot(node: &NodeConfig) -> Result<Option<Snapshot>, String> {
    let configured = node
        .identity
        .chain
        .budgeted_formal_full_sync()
        .ok_or("budgeted configuration missing")?;
    let ledger_path = node
        .identity
        .data_dir
        .join("state/full-sync/operation.json");
    if matches!(fs::symlink_metadata(&ledger_path), Err(error) if error.kind() == ErrorKind::NotFound)
    {
        return Ok(None);
    }
    let snapshot = read_snapshot(&ledger_path)?;
    let original: Value = serde_json::from_slice(
        snapshot
            .original_request_json
            .as_deref()
            .ok_or("ledger does not retain the original operation request")?,
    )
    .map_err(|e| e.to_string())?;
    if snapshot.policy != configured.transfer_policy
        || snapshot.material_limit != configured.max_total_sync_bytes
        || !super::grant_transition::permits(
            &snapshot,
            &original,
            &serde_json::to_value(&node.identity).map_err(|e| e.to_string())?,
            &serde_json::to_value(configured.read_material_config())
                .map_err(|e| e.to_string())?,
        )?
        || original["native_owner_binding"].as_str()
            != Some(snapshot.identity.binding_digest.as_str())
    {
        return Err("ledger differs from the node's exact operation binding".into());
    }
    Ok(Some(snapshot))
}

#[cfg(unix)]
pub(in super::super) fn require_takeover_continuity(
    node: &NodeConfig,
    owner: &super::super::process::ProcessOwnership,
) -> Result<(), String> {
    owner.require_directory(&node.identity.data_dir)?;
    let original = read_bound_snapshot(node)?
        .ok_or("operation_recovery_required: admitted instance lost its original ledger")?;
    if original.identity.binding_digest != owner.binding_digest()? {
        return Err("operation_recovery_required: original native binding changed".into());
    }
    Ok(())
}

#[cfg(unix)]
#[derive(Default)]
pub(in super::super) struct LiveObservation {
    pub base: Value,
    pub clock: Option<(String, u64, std::time::Instant, std::time::Instant)>,
    pub checkpoint_observed: Option<std::time::Instant>,
    pub history: Value,
    pub reopen_elapsed_ms: Option<u64>,
    pub reopen_result: Option<String>,
}

#[cfg(unix)]
pub(in super::super) fn running(
    node: &NodeConfig,
    live: &LiveObservation,
) -> Result<Value, String> {
    let mut status = if live.base.is_object() {
        live.base.clone()
    } else {
        observe(node).ok_or("budgeted node status missing")?
    };
    status["api_version"] = "oasis7.formal-full-sync-status/v2".into();
    status["checkpoint_age"] = match live.checkpoint_observed {
        Some(start) => {
            json!({"source": "process_monotonic_observation", "elapsed_ms": start.elapsed().as_millis().min(u64::MAX as u128) as u64})
        }
        None => json!({"source": "unknown_across_restart", "elapsed_ms": null}),
    };
    if let Some(snapshot) = read_bound_snapshot(node)? {
        let mut operation = project_operation(&snapshot)?;
        if let Some((identity, attempt, started, deadline)) = &live.clock
            && identity == &snapshot.identity.operation_id
            && *attempt == snapshot.attempts_used
            && snapshot.active_attempt_nonce.is_some()
        {
            let now = std::time::Instant::now();
            operation["attempt_state"] = if now < *deadline { "active" } else { "expired" }.into();
            operation["elapsed_ms"] = json!(
                now.saturating_duration_since(*started)
                    .as_millis()
                    .min(u64::MAX as u128) as u64
            );
            operation["remaining_ms"] = json!(
                deadline
                    .saturating_duration_since(now)
                    .as_millis()
                    .min(u64::MAX as u128) as u64
            );
        }
        for (field, source) in [
            ("history_pages", "pages"),
            ("history_proof_bytes", "proof_bytes"),
            ("history_manifest_bytes", "manifest_bytes"),
        ] {
            operation[field] = live.history[source].clone();
        }
        for (name, limit) in [
            (
                "history_pages",
                super::history_chain::MAX_HISTORY_CHAIN_PAGES,
            ),
            (
                "history_proof_bytes",
                super::history_chain::MAX_HISTORY_CHAIN_BYTES,
            ),
            (
                "history_manifest_bytes",
                super::history_chain::MAX_HISTORY_MANIFEST_CHAIN_BYTES as u64,
            ),
        ] {
            if operation[name]
                .as_u64()
                .is_some_and(|used| u128::from(used) * 5 >= u128::from(limit) * 4)
            {
                operation["near_capacity"]
                    .as_array_mut()
                    .ok_or("invalid capacity status")?
                    .push(name.into());
            }
        }
        operation["reopen_elapsed_ms"] = json!(live.reopen_elapsed_ms);
        operation["reopen_result"] = json!(live.reopen_result);
        status["operation"] = operation;
        if status["ready"] == true
            && (!snapshot.installation.as_ref().is_some_and(|i| i.completed)
                || status["loaded_cut"] != status["operation"]["target"])
        {
            status["ready"] = false.into();
            status["world_loaded"] = false.into();
            status["state"] = "blocked".into();
            status["blocker"] = "operation_recovery_required".into();
        }
    } else {
        status["operation"] = Value::Null;
        status["ready"] = false.into();
    }
    status["can_propose"] = false.into();
    Ok(status)
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
    let ledger_bytes = serde_json::to_vec(snapshot)
        .map_err(|e| e.to_string())?
        .len() as u64;
    let terminal_bytes = snapshot
        .terminal_index
        .as_ref()
        .map(serde_json::to_vec)
        .transpose()
        .map_err(|e| e.to_string())?
        .map_or(0, |bytes| bytes.len() as u64);
    let terminal_summaries = snapshot
        .terminal_index
        .as_ref()
        .map_or(0, |index| index.summaries.len() as u64);
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
        (
            "ledger_bytes",
            ledger_bytes,
            super::operation_snapshot::MAX_LEDGER_JSON,
        ),
        (
            "terminal_summaries",
            terminal_summaries,
            super::operation_terminal::MAX_TERMINALS as u64,
        ),
        (
            "terminal_bytes",
            terminal_bytes,
            super::operation_terminal::MAX_INDEX_BYTES as u64,
        ),
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
        "owner": &snapshot.attempt_owner,
        "source": "durable_operation_ledger/v1",
        "installation": if snapshot.installation.as_ref().is_some_and(|i| i.completed) { "installed_read_only" } else if snapshot.publication.is_some() { "recovery_required" } else { "not_committed" },
        "terminal": &snapshot.terminal_index,
        "ledger_bytes": ledger_bytes,
        "terminal_bytes": terminal_bytes,
        "terminal_summaries": terminal_summaries,
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
