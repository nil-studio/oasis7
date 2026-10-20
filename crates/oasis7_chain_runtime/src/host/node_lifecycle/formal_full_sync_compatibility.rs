//! Read-only runtime-compatibility probing for full-state sync.
//!
//! The descriptor is derived from the exact executable, fixed external trust,
//! and locally retained proof/CAS closure. It never changes node state.

use super::{
    FormalFullSyncConfigV1, FormalFullSyncStatusV1, PROFILE, history_chain, nondecreasing_cut,
    providers,
};
use oasis7::{runtime::LocalCasStore, world_service::authority};
use oasis7_client_api::world_service_full_sync::FormalFullSyncCutV1;
use std::{path::Path, sync::Arc, time::Duration};

/// Re-run the exact read-only decoder against the fixed external C0 and the
/// locally retained proof/CAS closure. This is executable-derived upgrade
/// evidence; it never repairs pins, restores a World, or changes readiness.
pub(crate) fn probe_runtime_compatibility(
    config: &FormalFullSyncConfigV1,
    node: &super::super::model::NodeConfig,
    status_path: &Path,
    data_dir: &Path,
    minimum_cut: Option<&FormalFullSyncCutV1>,
) -> Result<serde_json::Value, String> {
    config.validate()?;
    let configured = node
        .identity
        .chain
        .formal_full_sync()
        .ok_or("full-sync compatibility probe received another node profile")?;
    if configured != config {
        return Err("full-sync probe config differs from the exact NodeConfig profile".into());
    }
    let config_digest = super::super::local_io::digest(node)?;
    let instance = super::super::process::read_instance(data_dir)?;
    let status_loaded = match std::fs::symlink_metadata(status_path) {
        Ok(metadata) if metadata.is_file() => {
            let raw: serde_json::Value = super::super::local_io::read(status_path)?;
            let status: FormalFullSyncStatusV1 = serde_json::from_value(raw.clone())
                .map_err(|error| format!("decode full-sync persisted status: {error}"))?;
            let canonical = serde_json::to_value(&status)
                .map_err(|error| format!("re-encode full-sync persisted status: {error}"))?;
            let expected_grant_epoch = config.grant.as_ref().map(|grant| grant.body.grant_epoch);
            if raw != canonical
                || status.api_version != "oasis7.formal-full-sync-status/v1"
                || status.runtime_profile != PROFILE
                || status.world_id != config.world_id
                || status.chain_id != config.chain_id
                || status.sync_scope != "issuer_granted_full_state_read_only"
                || status.can_propose
                || status.grant_epoch != expected_grant_epoch
            {
                return Err(
                    "full-sync persisted status has an unsupported profile, grant, or capability"
                        .into(),
                );
            }
            for cut in [
                status.last_verified_cut.as_ref(),
                status.loaded_cut.as_ref(),
            ]
            .into_iter()
            .flatten()
            {
                cut.validate()?;
                if cut.world.world_id != config.world_id
                    || cut.world.genesis_digest != config.genesis_digest
                    || cut.chain_id != config.chain_id
                {
                    return Err("full-sync status cut differs from the configured World".into());
                }
            }
            let is_bound = instance.as_ref().is_some_and(|current| {
                status.config_digest == config_digest
                    && status.config_digest == current.config_digest
                    && status.incarnation == current.incarnation
                    && status.generation == current.generation
            });
            if is_bound {
                if let (Some(loaded), Some(verified)) =
                    (&status.loaded_cut, &status.last_verified_cut)
                    && !nondecreasing_cut(loaded, verified)
                {
                    return Err("full-sync loaded cut is not on the persisted verified head".into());
                }
                if status.ready
                    && (!status.state_root_verified
                        || !status.world_loaded
                        || status.loaded_cut.is_none()
                        || status.last_verified_cut.is_none())
                {
                    return Err(
                        "full-sync status claims readiness without a verified loaded cut".into(),
                    );
                }
                Some(status)
            } else {
                // A stale status from the other side of an interrupted config
                // promotion is diagnostic only. The exact proof/CAS closure
                // below is the compatibility authority.
                None
            }
        }
        Ok(_) => return Err("full-sync status path is not a regular file".into()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(format!("inspect full-sync status: {error}")),
    };

    let (keypair, _) =
        super::super::super::node_keypair_config::read_existing_node_keypair_in_secure_config_dir(
            &data_dir.join("keys"),
        )?;
    let signed = authority::sign_read_request(
        "formal-full-sync-compatibility-probe",
        (),
        &keypair.private_key_hex,
    )?;
    if let Some(status) = &status_loaded
        && status.node_public_key != signed.subject_public_key
    {
        return Err(
            "full-sync status node key differs from the existing provisioned node key".into(),
        );
    }
    let cas_root = data_dir.join("state").join("cas");
    let cas = Arc::new(LocalCasStore::new(&cas_root));
    let history = verify_local_history_from_external_c0(
        config,
        &keypair.private_key_hex,
        cas,
        &cas_root,
        minimum_cut,
    )?;
    Ok(serde_json::json!({
        "state": history["state"].as_str().unwrap_or("verified"),
        "status_loaded": status_loaded.is_some(),
        "status_ready": status_loaded.as_ref().is_some_and(|status| status.ready),
        "history_source": "local_qualified_history",
        "history_head_verified": true,
        "origin_cut": history["origin_cut"],
        "current_cut": history["current_cut"],
        "minimum_cut": history["minimum_cut"],
        "minimum_cut_verified": history["minimum_cut_verified"],
        "history": history
    }))
}

/// Replay local qualified proof pages from the fixed external C0. The first
/// configured endpoint is used only to construct the ordinary verifier client;
/// this path reads retained history and CAS locally and makes no HTTP request.
pub(super) fn verify_local_history_from_external_c0(
    config: &FormalFullSyncConfigV1,
    node_private_key_hex: &str,
    cas: Arc<LocalCasStore>,
    cas_root: &Path,
    minimum_cut: Option<&FormalFullSyncCutV1>,
) -> Result<serde_json::Value, String> {
    config.validate()?;
    let configured_providers = config.providers();
    let first_provider = configured_providers
        .first()
        .ok_or("full-sync has no configured provider")?;
    let timeout = Duration::from_millis(config.request_timeout_ms);
    let client =
        providers::build_read_client(config, first_provider, node_private_key_hex, timeout)?;
    history_chain::PersistedFormalReadHistoryV1::verify_read_only_at_least(
        config,
        &client,
        cas,
        cas_root,
        minimum_cut,
    )?
    .ok_or_else(|| {
        "HistoryUnavailable: full-sync external-origin history probe returned no verification result"
            .into()
    })
}
