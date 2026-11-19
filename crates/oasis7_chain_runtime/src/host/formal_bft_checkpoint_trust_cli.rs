//! Explicit configuration preparation against the original local Node binding.
//! Like node --print-binding this may initialize its private qualified-H origin.
//! It never contacts a provider, renews a checkpoint or starts signing/serving.
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
    while let Some(option) = args.next() {
        let slot = match option {
            "--config" => &mut config,
            "--node-config" => &mut node_config,
            "--policy-pin" => &mut policy_pin,
            "--consumer-binding" => &mut consumer_binding,
            "--original-c0-pin" => &mut original_c0_pin,
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
    println!(
        "{{\"schema_version\":\"oasis7.bft.checkpoint-trust-preparation.v1\",\"qualified_handoff_checked\":true,\"current_ready\":false,\"renewal_ready\":false,\"blocker\":\"C0-to-H lineage, current status, protected time and installation are not established by preparation\"}}"
    );
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
