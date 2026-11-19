//! Explicit activation of the original Node private H namespace.
use super::*;
#[cfg(not(test))]
use crate::execution_bridge::FormalBftHeaderLedgerExecutionV3;
#[cfg(test)]
use crate::execution_bridge_real_tests::real_execution_bridge::FormalBftHeaderLedgerExecutionV3;
use oasis7_distfs::protected_bft_v3::{
    BftPrivateRetentionAttestorV1, BftPrivateRetentionCreationMarkerV1,
    BftSuccessorWindowArchiveRefV1,
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceConfig {
    schema_version: u32,
    creation_identity: String,
    attestor: Option<BftPrivateRetentionAttestorV1>,
}

#[derive(Serialize)]
struct SourceReport {
    schema_version: &'static str,
    binding: BftSignerBindingV3,
    original_creation_marker: BftPrivateRetentionCreationMarkerV1,
    original_creation_marker_digest: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    original_private_origin_digest: Option<String>,
}

pub(super) fn run(config_path: &Path, source_path: &Path, activate: bool) -> Result<(), String> {
    let config = load_config(config_path)?;
    let source_config: SourceConfig =
        serde_json::from_slice(&read_private(source_path, 64 * 1024)?)
            .map_err(|_| "invalid original private source configuration")?;
    if source_config.schema_version != 1 {
        return Err("unsupported original private source configuration schema".into());
    }
    BftSuccessorWindowArchiveRefV1 {
        digest: source_config.creation_identity.clone(),
        bytes: 1,
    }
    .validate()
    .map_err(|error| error.to_string())?;
    let protection = if activate {
        Some((
            source_config
                .attestor
                .ok_or("original activation requires independent provider attestor")?,
            config
                .provider_trust
                .clone()
                .ok_or("original activation requires pinned provider trust")?,
        ))
    } else {
        None
    };
    let PreparedH {
        anchor,
        material,
        store,
        requester,
        ..
    } = prepare_h(&config)?;
    let source = Arc::new(CasBusinessIndexReader {
        store: store.clone(),
    });
    let mut ledger = FormalBftHeaderLedgerExecutionV3::open_from_handoff(
        &config.records_directory,
        store,
        anchor,
        config.target,
        material,
        source,
        !activate,
    )?;
    let (handoff, schedule, material) =
        ledger.covered_source_binding_parts(&source_config.creation_identity)?;
    let binding = BftSignerBindingV3::new(
        handoff,
        schedule,
        material,
        config.validator_id,
        config.provisioned_signing_keys,
        config.timeouts,
        config.operator_instance_id,
        hex::encode(requester.verifying_key().to_bytes()),
    )
    .map_err(|error| error.to_string())?;
    let marker = ledger.covered_source_marker(&binding, &source_config.creation_identity)?;
    let digest = marker.digest().map_err(|error| error.to_string())?;
    let original_private_origin_digest =
        if let Some((attestor, trust)) = protection {
            if attestor.provider_public_key != trust.provider_public_key {
                return Err("original source independent provider pins differ".into());
            }
            let timeout = Duration::from_millis(config.provider_timeout_ms);
            let deadline = Instant::now()
                .checked_add(timeout)
                .ok_or("original source deadline overflow")?;
            let provider = RemoteBftSignerProviderV3::new(
                config.provider_address,
                timeout,
                binding.clone(),
                requester,
                trust,
            )
            .map_err(|error| error.to_string())?;
            Some(ledger.activate_covered_original_source(
                &binding, &attestor, &marker, &provider, deadline,
            )?)
        } else {
            None
        };
    let report = SourceReport {
        schema_version: "oasis7.formal_bft_original_private_source.v1",
        binding,
        original_creation_marker: marker,
        original_creation_marker_digest: digest,
        original_private_origin_digest,
    };
    let encoded =
        serde_json::to_string(&report).map_err(|_| "encode original source preparation report")?;
    output::write_line(&mut std::io::stdout().lock(), &encoded)
}
