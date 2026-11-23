//! Explicit activation of the original Node private H namespace.
use super::*;
use oasis7_distfs::protected_bft_v3::{
    BftPrivateRetentionAttestorV1, BftPrivateRetentionCreationMarkerV1,
    BftSuccessorWindowArchiveRefV1, BftSuccessorWindowSourceV1, LocalBftSuccessorArchiveStoreV1,
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceConfig {
    schema_version: u32,
    creation_identity: String,
    attestor: Option<BftPrivateRetentionAttestorV1>,
    archive_directory: Option<PathBuf>,
    reserve_input: Option<FirstReserveInputPin>,
    original_source: Option<OriginalSourceArchivePinV1>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FirstReserveInputPin {
    source: BftSuccessorWindowSourceV1,
    candidate: OriginalCandidateArchivePinV1,
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
    ArchiveReceived(&'a Path),
    ReserveFirst,
    VerifyArchive,
}

#[derive(Clone, Copy)]
enum ArchiveUse {
    LiveSource,
    ArchiveOnly,
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
    if matches!(mode, Mode::VerifyArchive) {
        return verify_archive(&config, source_config);
    }
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
    if matches!(mode, Mode::ReserveFirst) {
        let (attestor, trust) = protection.ok_or("first Reserve requires source authority")?;
        let pins = source_config
            .reserve_input
            .ok_or("first Reserve requires exact original descriptor")?;
        let directory = source_config
            .archive_directory
            .ok_or("first Reserve requires the separate original archive")?;
        let archive = open_archive(&config, &directory, ArchiveUse::LiveSource)?;
        let input = Arc::new(
            archive
                .load_original_admission_input(&pins.candidate.reference)
                .map_err(|error| error.to_string())?,
        );
        let timeout = Duration::from_millis(config.provider_timeout_ms);
        let provider = RemoteBftSignerProviderV3::new(
            config.provider_address,
            timeout,
            binding.clone(),
            requester,
            trust,
        )
        .map_err(|error| error.to_string())?;
        let deadline = Instant::now()
            .checked_add(timeout)
            .ok_or("first Reserve deadline overflow")?;
        let (wal_digest, pending_state_digest, received) = ledger.reserve_first_covered_original(
            &binding,
            &attestor,
            &source_config.creation_identity,
            &pins.source,
            &pins.candidate,
            input,
            &provider,
            deadline,
        )?;
        let report = serde_json::json!({
            "schema_version": if received { "oasis7.formal_bft_original_pending_received.v1" }
                else { "oasis7.formal_bft_original_pending_local.v1" },
            "original_candidate_wal_digest": wal_digest,
            "pending_state_digest": pending_state_digest,
        });
        let encoded =
            serde_json::to_string(&report).map_err(|_| "encode first Pending Local report")?;
        return output::write_line(&mut std::io::stdout().lock(), &encoded);
    }
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
            if let Mode::ArchiveFirst(input) | Mode::ArchiveReceived(input) = mode {
                let directory = source_config
                    .archive_directory
                    .ok_or("first private input requires a separate archive directory")?;
                return archive_first(
                    &config,
                    &ledger,
                    &binding,
                    &attestor,
                    &marker,
                    &directory,
                    input,
                    matches!(mode, Mode::ArchiveReceived(_)),
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

#[expect(
    clippy::too_many_arguments,
    reason = "The live source authority and distinct Local/Received file are explicit."
)]
fn archive_first(
    config: &Config,
    ledger: &FormalBftHeaderLedgerExecutionV3,
    binding: &BftSignerBindingV3,
    attestor: &BftPrivateRetentionAttestorV1,
    marker: &BftPrivateRetentionCreationMarkerV1,
    directory: &Path,
    actions: &Path,
    received: bool,
) -> Result<(), String> {
    let archive = open_archive(config, directory, ArchiveUse::LiveSource)?;
    let (source, candidate, replayed, first_reservation_bytes) = if received {
        let bytes = read_private(actions, 66 * 1024 * 1024)?;
        let package = oasis7_node::decode_bft_v3_private_candidate_package(&bytes)?;
        ledger.archive_first_original_received(&archive, binding, attestor, marker, &package)?
    } else {
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
        ledger.archive_first_original_local(&archive, binding, attestor, marker, actions)?
    };
    let original_source =
        ledger.archive_first_original_source(&archive, binding, attestor, marker, &source)?;
    let report = serde_json::json!({
        "schema_version": if received { "oasis7.formal_bft_original_received_input_archive.v1" }
            else { "oasis7.formal_bft_original_local_input_archive.v1" },
        "source": source,
        "candidate": candidate,
        "original_source": original_source,
        "replayed": replayed,
        "first_reservation_bytes": first_reservation_bytes,
    });
    let encoded =
        serde_json::to_string(&report).map_err(|_| "encode staged first original input")?;
    output::write_line(&mut std::io::stdout().lock(), &encoded)
}

/// This path deliberately never opens the live H file, CAS or original WAL.
/// The trusted anchor and provider authority remain independent input pins.
fn verify_archive(config: &Config, source_config: SourceConfig) -> Result<(), String> {
    let deadline = Instant::now()
        .checked_add(Duration::from_millis(config.provider_timeout_ms))
        .ok_or("archive verification deadline overflow")?;
    let attestor = source_config
        .attestor
        .ok_or("archive verification requires independent attestor")?;
    let trust = config
        .provider_trust
        .as_ref()
        .ok_or("archive verification requires provider trust")?;
    let pins = source_config
        .reserve_input
        .ok_or("archive verification requires exact original descriptor")?;
    let source = source_config
        .original_source
        .ok_or("archive verification requires complete original H archive pin")?;
    let directory = source_config
        .archive_directory
        .ok_or("archive verification requires separate original archive")?;
    let archive = Arc::new(open_archive(config, &directory, ArchiveUse::ArchiveOnly)?);
    let anchor = load_handoff_trust_config(&config.trust_config_path)?.verified_anchor()?;
    if !config.requester_signing_key_path.is_absolute() {
        return Err("header Node requester key path must be absolute".into());
    }
    let requester = crate::controlled_live_config::read_existing_signing_key(
        &config.requester_signing_key_path,
    )?;
    let (handoff, schedule, material_identity) = source.binding_parts(
        &archive,
        anchor.clone(),
        config.target.clone(),
        &source_config.creation_identity,
    )?;
    let binding = BftSignerBindingV3::new(
        handoff,
        schedule,
        material_identity,
        config.validator_id.clone(),
        config.provisioned_signing_keys.clone(),
        config.timeouts,
        config.operator_instance_id.clone(),
        hex::encode(requester.verifying_key().to_bytes()),
    )
    .map_err(|error| error.to_string())?;
    trust
        .validate_for(&binding)
        .map_err(|error| error.to_string())?;
    oasis7_distfs::protected_bft_v3::validate_successor_source_for_binding_v3(
        &pins.source,
        &binding,
    )
    .map_err(|error| error.to_string())?;
    if attestor.provider_public_key != trust.provider_public_key {
        return Err("archive verifier independent source pins differ".into());
    }
    let reference = pins.candidate.reference.clone();
    let reader = ArchivedOriginalSourceReaderV1::new(
        archive,
        source,
        anchor,
        config.target.clone(),
        binding,
        attestor,
        pins.candidate,
    );
    use oasis7_distfs::protected_bft_v3::BftFirstReserveSourceV1;
    let original = reader
        .read_authorized_original(&reference, deadline)
        .map_err(|error| error.to_string())?;
    let (replayed, first_price) = reader.verify(&original.input)?;
    if Instant::now() >= deadline {
        return Err("archive verification deadline elapsed".into());
    }
    let report = serde_json::json!({
        "schema_version": "oasis7.formal_bft_original_archive_verified.v1",
        "candidate": reference,
        "replayed": replayed,
        "first_reservation_bytes": first_price.required_input_bytes(&original.input)
            .map_err(|error| error.to_string())?,
    });
    let encoded =
        serde_json::to_string(&report).map_err(|_| "encode original archive verification")?;
    output::write_line(&mut std::io::stdout().lock(), &encoded)
}

fn open_archive(
    config: &Config,
    directory: &Path,
    usage: ArchiveUse,
) -> Result<LocalBftSuccessorArchiveStoreV1, String> {
    let declared_archive = absolute_lexical_path(directory)?;
    let directory = require_existing_directory(directory, "private admission input archive")?;
    crate::controlled_live_config::require_private_owned(&directory, true)?;
    for live in [
        &config.cas_directory,
        &config.records_directory,
        &config.replication_directory,
    ] {
        let live = match usage {
            ArchiveUse::LiveSource => std::fs::canonicalize(live)
                .map_err(|_| "original Node storage path resolution rejected")?,
            // Independent verification must not stat, open or resolve old live
            // paths. They may be inaccessible on this reader's machine.
            ArchiveUse::ArchiveOnly => absolute_lexical_path(live)?,
        };
        if super::super::super::formal_bft_read_cli::paths_overlap(&directory, &live)
            || super::super::super::formal_bft_read_cli::paths_overlap(&declared_archive, &live)
        {
            return Err("original input archive and Node storage must be disjoint".into());
        }
    }
    LocalBftSuccessorArchiveStoreV1::new(LocalCasStore::new(directory))
        .map_err(|error| error.to_string())
}

fn absolute_lexical_path(path: &Path) -> Result<PathBuf, String> {
    use std::path::Component;
    if !path.is_absolute() {
        return Err("original source storage paths must be absolute".into());
    }
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !normalized.pop() {
                    return Err("original source storage path escapes its root".into());
                }
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    Ok(normalized)
}

#[cfg(test)]
mod archive_path_tests {
    use super::*;

    #[test]
    fn archive_only_paths_normalize_without_accessing_the_old_source() {
        assert_eq!(
            absolute_lexical_path(Path::new("/dev/null/old/../cas")).unwrap(),
            Path::new("/dev/null/cas")
        );
        assert_eq!(
            absolute_lexical_path(Path::new("/archive/./source/..")).unwrap(),
            Path::new("/archive")
        );
        assert!(absolute_lexical_path(Path::new("../archive")).is_err());
        assert!(absolute_lexical_path(Path::new("/../../archive")).is_err());
    }
}
