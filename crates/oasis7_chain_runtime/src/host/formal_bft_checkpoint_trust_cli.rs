//! Explicit configuration preparation against the original local Node binding.
//! Like node --print-binding this may initialize its private qualified-H origin.
//! It never contacts a provider, renews a checkpoint or starts signing/serving.
use oasis7_formal_protocol::bft::checkpoint_issuer_status::AuthenticatedBftCheckpointIssuerStatusV1;
use oasis7_game_client::checkpoint_trust::{
    BftCheckpointAnchorRefV1, load_checkpoint_trust_preparation,
};
use std::path::PathBuf;

pub(super) fn run<'a>(mut args: impl Iterator<Item = &'a str>) -> Result<(), String> {
    let mut config = None;
    let mut node_config = None;
    let mut policy_pin = None;
    let mut consumer_binding = None;
    let mut original_c0_pin = None;
    let mut issuer_status = None;
    let mut previous_issuer_status = None;
    let mut old_key_constraints = None;
    let mut stake_constraints = None;
    let mut time_observation = None;
    let mut time_nonce = None;
    let mut clock_epoch = None;
    while let Some(option) = args.next() {
        let slot = match option {
            "--config" => &mut config,
            "--node-config" => &mut node_config,
            "--policy-pin" => &mut policy_pin,
            "--consumer-binding" => &mut consumer_binding,
            "--original-c0-pin" => &mut original_c0_pin,
            "--issuer-status" => &mut issuer_status,
            "--previous-issuer-status" => &mut previous_issuer_status,
            "--old-key-constraints" => &mut old_key_constraints,
            "--stake-constraints" => &mut stake_constraints,
            "--time-observation" => &mut time_observation,
            "--time-nonce" => &mut time_nonce,
            "--clock-epoch" => &mut clock_epoch,
            _ => return Err("unsupported checkpoint preparation option".into()),
        };
        if slot.is_some() {
            return Err("duplicate checkpoint preparation option".into());
        }
        *slot = Some(
            args.next()
                .ok_or("missing checkpoint preparation value")?
                .to_owned(),
        );
    }
    if !matches!(
        (&time_observation, &time_nonce, &clock_epoch),
        (None, None, None) | (Some(_), Some(_), Some(_))
    ) {
        return Err(
            "checkpoint time diagnostics require observation, nonce and epoch together".into(),
        );
    }
    if old_key_constraints.is_some() != stake_constraints.is_some() {
        return Err("checkpoint safety constraints require both original files".into());
    }
    let config = PathBuf::from(config.ok_or("checkpoint preparation requires --config")?);
    let node_config =
        PathBuf::from(node_config.ok_or("checkpoint preparation requires --node-config")?);
    if !config.is_absolute() || !node_config.is_absolute() {
        return Err("checkpoint preparation paths must be absolute".into());
    }
    let pin = policy_pin.ok_or("checkpoint preparation requires independent --policy-pin")?;
    let consumer = consumer_binding.ok_or("checkpoint preparation requires --consumer-binding")?;
    super::super::controlled_live_config::require_private_owned(&config, false)?;
    let c0 = original_c0_pin
        .ok_or("checkpoint preparation requires independently retained --original-c0-pin")?;
    let prepared = load_checkpoint_trust_preparation(&config, &pin, &consumer, &c0)?;
    // Constraint originals are bounded as a pair before verification; these
    // local bytes are not an independent safety attestation or a proof.
    let safety_constraints_checked = match (old_key_constraints, stake_constraints) {
        (Some(old), Some(stake)) => {
            let policy = &prepared.config().policy;
            let read = |path: String, maximum: u64| -> Result<Vec<u8>, String> {
                let path = PathBuf::from(path);
                if !path.is_absolute() {
                    return Err("checkpoint safety constraint path must be absolute".into());
                }
                super::super::controlled_live_config::require_private_owned(&path, false)?;
                super::super::controlled_history_cli::read_bounded(&path, maximum)
            };
            let old = read(old, policy.max_safety_evidence_bytes)?;
            let remaining = policy
                .max_safety_evidence_bytes
                .checked_sub(old.len() as u64)
                .ok_or("checkpoint safety constraint byte bound exceeded")?;
            let stake = read(stake, remaining)?;
            policy.check_safety_constraint_originals(&pin, &old, &stake)?;
            true
        }
        (None, None) => false,
        _ => return Err("incomplete checkpoint safety constraint input".into()),
    };
    if previous_issuer_status.is_some() && issuer_status.is_none() {
        return Err("previous checkpoint issuer status requires --issuer-status".into());
    }
    let authenticate_status = |path: String| -> Result<_, String> {
        let path = PathBuf::from(path);
        if !path.is_absolute() {
            return Err("checkpoint issuer status path must be absolute".into());
        }
        super::super::controlled_live_config::require_private_owned(&path, false)?;
        let policy = &prepared.config().policy;
        let bytes =
            super::super::controlled_history_cli::read_bounded(&path, policy.max_status_bytes)?;
        AuthenticatedBftCheckpointIssuerStatusV1::decode(&bytes, policy, &pin)
    };
    let authenticated_status = issuer_status.map(&authenticate_status).transpose()?;
    let previous_status = previous_issuer_status.map(&authenticate_status).transpose()?;
    if let (Some(current), Some(previous)) = (&authenticated_status, &previous_status) {
        current.check_predecessor(previous)?;
    }
    // These explicit inputs are diagnostic pins only. Neither a local file
    // nor an operator-supplied nonce/epoch proves a protected request start.
    let authenticated_time = match (time_observation, time_nonce, clock_epoch) {
        (Some(path), Some(nonce), Some(epoch)) => {
            let path = PathBuf::from(path);
            if !path.is_absolute() {
                return Err("checkpoint time observation path must be absolute".into());
            }
            super::super::controlled_live_config::require_private_owned(&path, false)?;
            let bytes = super::super::controlled_history_cli::read_bounded(
                &path,
                prepared.config().policy.max_time_evidence_bytes,
            )?;
            Some(prepared.authenticate_time_observation(&bytes, &nonce, &epoch)?)
        }
        (None, None, None) => None,
        _ => return Err("incomplete checkpoint time diagnostic input".into()),
    };
    // This path restores actual H material and reconstructs the binding from
    // the original ledger/namespace, rather than trusting source DTO fields.
    let report = super::prepare_node_binding_report(&node_config)?;
    if report.binding_digest != consumer {
        return Err("checkpoint consumer differs from actual Node binding".into());
    }
    let BftCheckpointAnchorRefV1::QualifiedHandoffV2 { source, .. } =
        &prepared.config().initial_anchor;
    oasis7_distfs::protected_bft_v3::validate_successor_source_for_binding_v3(
        source,
        &report.binding,
    )
    .map_err(|error| error.to_string())?;
    prepared.check_replayed_handoff(
        &report.binding.material_identity,
        &report.binding.initial_schedule().digest()?,
    )?;
    let mut report = serde_json::json!({
        "schema_version": "oasis7.bft.checkpoint-trust-preparation.v1",
        "qualified_handoff_checked": true,
        "current_ready": false,
        "renewal_ready": false,
        "blocker": "C0-to-H lineage, current status, protected time and installation are not established by preparation",
    });
    if safety_constraints_checked {
        report["safety_constraint_originals"] = serde_json::json!({
            "policy_digest": pin,
            "old_key_constraints_digest": prepared.config().policy.security_policy.old_key_constraints_digest,
            "stake_constraints_digest": prepared.config().policy.security_policy.stake_constraints_digest,
            "original_bytes_checked": true,
            "target_safety_attestation_checked": false,
            "issuer_proof_signatures_checked": false,
        });
    }
    if let (Some(status), Some(time)) = (&authenticated_status, &authenticated_time) {
        let compatibility =
            status.check_signed_observation_window(&prepared.config().policy, time);
        report["signed_observation_status_diagnostic"] = serde_json::json!({
            "compatible_at_observation": compatibility.is_ok(),
            "blocker": compatibility.err(),
            "protected_continuity_checked": false,
            "protected_status_floor_checked": false,
            "issuer_proof_signatures_checked": false,
        });
    }
    if let Some(status) = authenticated_status {
        report["authenticated_issuer_status"] = serde_json::json!({
            "digest": status.digest(),
            "sequence": status.body().status_sequence,
            "policy_revoked": status.body().policy_revoked,
            "previous_snapshot_checked": previous_status.is_some(),
            "protected_status_floor_checked": false,
        });
    }
    if let Some(time) = authenticated_time {
        let initial_window = prepared.check_initial_signed_observation_window(&time);
        report["initial_h_observation_diagnostic"] = serde_json::json!({
            "within_original_window_at_observation": initial_window.is_ok(),
            "blocker": initial_window.err(),
            "protected_continuity_checked": false,
        });
        report["authenticated_time_observation"] = serde_json::json!({
            "digest": time.digest(),
            "time_generation": time.body().time_generation,
            "utc_earliest_unix_ms": time.body().utc_earliest_unix_ms,
            "utc_latest_unix_ms": time.body().utc_latest_unix_ms,
            "protected_continuity_checked": false,
        });
    }
    println!("{report}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preparation_requires_explicit_pins_and_paths() {
        assert!(run([].into_iter()).is_err());
        assert!(run(["--config", "relative", "--node-config", "/node"].into_iter()).is_err());
        assert!(run(["--config", "/one", "--config", "/two"].into_iter()).is_err());
        assert!(run(["--config", "/one", "--node-config", "/two"].into_iter()).is_err());
    }
}
