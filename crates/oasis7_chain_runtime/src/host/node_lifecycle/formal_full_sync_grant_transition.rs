//! Admission of a new signed grant only after the prior operation has an
//! exact completed installation and durable terminal accounting. The old
//! operation/request is never rewritten or repriced on lease renewal.
use super::{FormalFullSyncConfigV1, now_unix_ms, validate_grant_for_node};
use super::operation_snapshot::Snapshot;
use super::super::model::IdentityBinding;
use serde_json::Value;

/// A literal old request is always valid for its original ledger. A different
/// grant is acceptable only if it is the *sole* configuration/identity change
/// and the original operation has completely settled its signed cut.
pub(super) fn permits(
    snapshot: &Snapshot,
    original: &Value,
    current_identity: &Value,
    current_config: &Value,
) -> Result<bool, String> {
    if original["node_identity"] == *current_identity && original["config"] == *current_config {
        return Ok(true);
    }
    if !snapshot.installation.as_ref().is_some_and(|v| v.completed)
        || !snapshot.publication.as_ref().is_some_and(|v| v.completed)
        || !snapshot
            .terminal_index
            .as_ref()
            .and_then(|index| index.summaries.last())
            .is_some_and(|summary| summary.identity == snapshot.identity)
        || snapshot.active_attempt_nonce.is_some()
        || snapshot.active_request.is_some()
        || snapshot.reservation.is_some()
        || snapshot.unresolved_reserved()? != 0
    {
        return Ok(false);
    }
    let mut old_identity: IdentityBinding =
        serde_json::from_value(original["node_identity"].clone())
            .map_err(|error| format!("original grant identity: {error}"))?;
    let mut next_identity: IdentityBinding =
        serde_json::from_value(current_identity.clone())
            .map_err(|error| format!("renewed grant identity: {error}"))?;
    let old_config: FormalFullSyncConfigV1 =
        serde_json::from_value(original["config"].clone())
            .map_err(|error| format!("original grant config: {error}"))?;
    let next_config: FormalFullSyncConfigV1 =
        serde_json::from_value(current_config.clone())
            .map_err(|error| format!("renewed grant config: {error}"))?;
    let Some(old_grant) = old_config.grant.as_ref() else {
        return Ok(false);
    };
    let Some(next_grant) = next_config.grant.as_ref() else {
        return Ok(false);
    };
    let old_identity_grant = old_identity.chain.formal_full_sync_grant_mut();
    let next_identity_grant = next_identity.chain.formal_full_sync_grant_mut();
    let (Some(old_identity_grant), Some(next_identity_grant)) =
        (old_identity_grant, next_identity_grant)
    else {
        return Ok(false);
    };
    if old_identity_grant.as_ref() != Some(old_grant)
        || next_identity_grant.as_ref() != Some(next_grant)
    {
        return Ok(false);
    }
    *old_identity_grant = None;
    *next_identity_grant = None;
    let mut original_config = original["config"].clone();
    let mut renewed_config = current_config.clone();
    original_config["grant"] = Value::Null;
    renewed_config["grant"] = Value::Null;
    if old_identity != next_identity
        || original_config != renewed_config
        || old_grant.issuer_public_key != next_grant.issuer_public_key
        || old_grant.body.node_public_key != next_grant.body.node_public_key
        || old_grant.body.world != next_grant.body.world
        || old_grant.body.chain_id != next_grant.body.chain_id
        || old_grant.body.role != next_grant.body.role
        || next_grant.body.grant_epoch <= old_grant.body.grant_epoch
        || next_grant.body.authority_epoch < old_grant.body.authority_epoch
    {
        return Ok(false);
    }
    validate_grant_for_node(
        &old_config,
        old_grant,
        &old_grant.body.node_public_key,
        old_grant.body.not_before_unix_ms,
    )?;
    validate_grant_for_node(
        &next_config,
        next_grant,
        &old_grant.body.node_public_key,
        now_unix_ms().ok_or("renewed grant system clock unavailable")?,
    )?;
    Ok(true)
}
