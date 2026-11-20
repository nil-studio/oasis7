//! Explicit activation of the original Node private H namespace.
use super::*;
use oasis7_distfs::protected_bft_v3::{
    BftPrivateRetentionAttestorV1, BftPrivateRetentionCreationMarkerV1,
    BftSuccessorWindowArchiveRefV1, LocalBftSuccessorArchiveStoreV1,
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceConfig {
    schema_version: u32,
    creation_identity: String,
    attestor: Option<BftPrivateRetentionAttestorV1>,
    archive_directory: Option<PathBuf>,
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

#[derive(Clone, Copy)]
pub(super) enum Mode<'a> {
    Prepare,
    Activate,
    ArchiveFirst(&'a Path),
}

pub(super) fn run(config_path: &Path, source_path: &Path, mode: Mode<'_>) -> Result<(), String> {
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
    let protection = if matches!(mode, Mode::Prepare) {
        None
    } else {
        Some((
            source_config
                .attestor
                .ok_or("original activation requires independent provider attestor")?,
            config
                .provider_trust
                .clone()
                .ok_or("original activation requires pinned provider trust")?,
        ))
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
        config.target.clone(),
        material,
        source,
        matches!(mode, Mode::Prepare),
    )?;
    let (handoff, schedule, material) =
        ledger.covered_source_binding_parts(&source_config.creation_identity)?;
    let binding = BftSignerBindingV3::new(
        handoff,
        schedule,
        material,
        config.validator_id.clone(),
        config.provisioned_signing_keys.clone(),
        config.timeouts,
        config.operator_instance_id.clone(),
        hex::encode(requester.verifying_key().to_bytes()),
    )
    .map_err(|error| error.to_string())?;
    let marker = ledger.covered_source_marker(&binding, &source_config.creation_identity)?;
    let digest = marker.digest().map_err(|error| error.to_string())?;
    let original_private_origin_digest =
        if let Some((attestor, trust)) = protection {
            trust
                .validate_for(&binding)
                .map_err(|error| error.to_string())?;
            if attestor.provider_public_key != trust.provider_public_key {
                return Err("original source independent provider pins differ".into());
            }
            if let Mode::ArchiveFirst(actions) = mode {
                let directory = source_config
                    .archive_directory
                    .ok_or("first private input requires a separate archive directory")?;
                return archive_first(
                    &config, &ledger, &binding, &attestor, &marker, &directory, actions,
                );
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

fn archive_first(
    config: &Config,
    ledger: &FormalBftHeaderLedgerExecutionV3,
    binding: &BftSignerBindingV3,
    attestor: &BftPrivateRetentionAttestorV1,
    marker: &BftPrivateRetentionCreationMarkerV1,
    directory: &Path,
    actions: &Path,
) -> Result<(), String> {
    let directory = require_existing_directory(directory, "private admission input archive")?;
    crate::controlled_live_config::require_private_owned(&directory, true)?;
    if [
        &config.cas_directory,
        &config.records_directory,
        &config.replication_directory,
    ]
    .into_iter()
    .any(|live| super::super::super::formal_bft_read_cli::paths_overlap(&directory, live))
    {
        return Err("original input archive and Node storage must be disjoint".into());
    }
    let actions = read_private(actions, BFT_MAX_ACTIONS_CBOR_BYTES_V1 as u64)?;
    let decoded = oasis7_consensus::formal_bft_payload_v3::decode_private_actions_v3(&actions)
        .map_err(|_| "first original input requires canonical local actions")?;
    if decoded.is_empty()
        || decoded
            .iter()
            .any(|item| item.submitter_player_id != config.player_id)
    {
        return Err(
            "first original input requires nonempty actions for the configured player".into(),
        );
    }
    let archive = LocalBftSuccessorArchiveStoreV1::new(LocalCasStore::new(directory))
        .map_err(|error| error.to_string())?;
    let (source, candidate, replayed, first_reservation_bytes) =
        ledger.archive_first_original_local(&archive, binding, attestor, marker, actions)?;
    let report = serde_json::json!({
        "schema_version": "oasis7.formal_bft_original_local_input_archive.v1",
        "source": source,
        "candidate": candidate,
        "replayed": replayed,
        "first_reservation_bytes": first_reservation_bytes,
    });
    let encoded =
        serde_json::to_string(&report).map_err(|_| "encode staged first original input")?;
    output::write_line(&mut std::io::stdout().lock(), &encoded)
}
