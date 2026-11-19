//! Local durable import reports are not canonical Node receipts.
use super::*;
#[path = "formal_bft_header_serve_cli.rs"]
mod serve;
#[cfg(not(test))]
use crate::execution_bridge::{
    CommittedPrefixArchiveRefV1, FormalBftHeaderLedgerExecutionV3, OriginalCandidateArchivePinV1,
    ReplayedCommittedPrefixV1, ReplayedOriginalCandidateV1,
};
#[cfg(test)]
use crate::execution_bridge_real_tests::real_execution_bridge::{
    CommittedPrefixArchiveRefV1, FormalBftHeaderLedgerExecutionV3, OriginalCandidateArchivePinV1,
    ReplayedCommittedPrefixV1, ReplayedOriginalCandidateV1,
};

use oasis7_distfs::protected_bft_v3::{
    BftPrivateRetentionCreationMarkerV1, BftSuccessorWindowSourceV1,
    LocalBftSuccessorArchiveStoreV1,
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    schema_version: u32,
    trust_config_path: PathBuf,
    target: BftProfileTargetV2,
    handoff_material_path: PathBuf,
    cas_directory: PathBuf,
    ledger_directory: PathBuf,
    exact_height: u64,
    exact_header_id: String,
    #[serde(default)]
    bind: Option<String>,
    #[serde(default)]
    service_signing_key_path: Option<PathBuf>,
}

#[derive(Serialize)]
pub(super) struct Report {
    schema_version: &'static str,
    height: u64,
    header_id: String,
    durable_record_digest: String,
    world_tick: u64,
    result_root: String,
    certified_prefix: oasis7_formal_protocol::bft::FormalBftCertifiedHeaderPrefixV3,
}

pub(super) fn run<'a>(
    action: &str,
    mut args: impl Iterator<Item = &'a str>,
    shutdown: Option<Arc<std::sync::atomic::AtomicBool>>,
) -> Result<(), String> {
    let mut config = None;
    let mut history = None;
    let mut archive = None;
    while let Some(option) = args.next() {
        let slot = match option {
            "--config" => &mut config,
            "--history" if action == "install" => &mut history,
            "--archive" if matches!(action, "archive-prefix" | "verify-first-reserve") => {
                &mut archive
            }
            _ => return Err("unsupported private header ledger option".into()),
        };
        if slot.is_some() {
            return Err("duplicate private header ledger option".into());
        }
        let path = PathBuf::from(
            args.next()
                .ok_or("ledger option requires an absolute path")?,
        );
        if !path.is_absolute() {
            return Err("private header ledger input paths must be absolute".into());
        }
        *slot = Some(path);
    }
    let config = config.ok_or("private header ledger requires --config")?;
    if action == "install" && history.is_none() {
        return Err("private header ledger install requires --history".into());
    }
    if action == "serve" {
        return serve::run(&config, shutdown);
    }
    if action == "verify-first-reserve" {
        let archive = archive.ok_or("bft-header verify-first-reserve requires --archive")?;
        let report = verify_first_reserve_from_paths(&config, &archive)?;
        println!(
            "{}",
            serde_json::to_string(&report)
                .map_err(|_| "encode local first Reserve verification")?
        );
        return Ok(());
    }
    if action == "archive-prefix" {
        let archive = archive.ok_or("bft-header archive-prefix requires --archive")?;
        let report = archive_from_paths(&config, &archive)?;
        let line = serde_json::to_string(&report).map_err(|_| "encode committed archive report")?;
        use std::io::Write;
        let mut output = std::io::stdout().lock();
        return writeln!(output, "{line}")
            .and_then(|()| output.flush())
            .map_err(|_| "write committed archive report".into());
    }
    let report = from_paths(&config, history.as_deref())?;
    println!(
        "{}",
        serde_json::to_string(&report).map_err(|_| "encode public header ledger report")?
    );
    Ok(())
}

pub(super) fn from_paths(config_path: &Path, history: Option<&Path>) -> Result<Report, String> {
    let config = load_config(config_path)?;
    // Decode all private input before creating or recovering a ledger.
    let input = history
        .map(|path| {
            read_private(path, PRIVATE_HEADER_PREFIX_MAX_BYTES_V3 as u64)
                .and_then(|bytes| PrivateHeaderPrefixV3::decode_private_canonical(&bytes))
        })
        .transpose()?;
    let LoadedH {
        anchor,
        material,
        store,
    } = load_h(&config)?;
    let source = Arc::new(CasBusinessIndexReader {
        store: store.clone(),
    });
    let mut ledger = FormalBftHeaderLedgerExecutionV3::open_from_handoff(
        &config.ledger_directory,
        store,
        anchor,
        config.target,
        material,
        source,
        input.is_some(),
    )
    .map_err(|_| "private header ledger storage or qualified H recovery rejected")?;
    let cut = match input {
        Some(input) => ledger.install_prefix(&input, config.exact_height, &config.exact_header_id),
        None => ledger.verified_cut(config.exact_height, &config.exact_header_id),
    }
    .map_err(|_| "private header ledger exact-prefix execution or durable reread rejected")?;
    Ok(Report {
        schema_version: "oasis7.formal_bft_header_local_ledger.v1",
        height: cut.proof().commitment().height,
        header_id: cut.proof().certificate().block_id().into(),
        durable_record_digest: cut.durable_record_digest().into(),
        world_tick: cut.world().state().time,
        result_root: cut
            .result_tree()
            .root()
            .map_err(|_| "read verified local result root")?,
        certified_prefix: cut.certified_prefix().clone(),
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ArchiveConfig {
    schema_version: u32,
    archive_directory: PathBuf,
    source: BftSuccessorWindowSourceV1,
    /// Absent for export. A cold verifier supplies its independently pinned root/ref.
    reference: Option<CommittedPrefixArchiveRefV1>,
    candidate: Option<OriginalCandidateArchivePinV1>,
}

#[derive(Serialize)]
struct ArchiveReport {
    schema_version: &'static str,
    reference: CommittedPrefixArchiveRefV1,
    replayed: ReplayedCommittedPrefixV1,
    #[serde(skip_serializing_if = "Option::is_none")]
    candidate: Option<ReplayedOriginalCandidateV1>,
}

fn archive_from_paths(config_path: &Path, archive_path: &Path) -> Result<ArchiveReport, String> {
    let config = load_config(config_path)?;
    let bytes = read_private(archive_path, 64 * 1024)?;
    let options: ArchiveConfig =
        serde_json::from_slice(&bytes).map_err(|_| "invalid committed archive config")?;
    if options.schema_version != 1 {
        return Err("unsupported committed archive config schema".into());
    }
    options.source.validate()?;
    if options.candidate.is_some() && options.reference.is_none() {
        return Err("candidate replay requires an independently pinned committed archive".into());
    }
    let archive_root =
        require_existing_directory(&options.archive_directory, "private committed archive CAS")?;
    crate::controlled_live_config::require_private_owned(&archive_root, true)?;
    let archive = LocalBftSuccessorArchiveStoreV1::new(LocalCasStore::new(&archive_root))
        .map_err(|_| "committed archive CAS path rejected")?;
    let anchor = load_qualified_handoff_anchor(&config.trust_config_path)
        .map_err(|_| "committed archive independent H trust rejected")?;
    let (reference, original) = if let Some(reference) = options.reference {
        (reference, None)
    } else {
        let LoadedH {
            anchor,
            material,
            store,
        } = load_h(&config)?;
        let ledger_root =
            require_existing_directory(&config.ledger_directory, "private header ledger")?;
        if [store.root(), ledger_root.as_path()]
            .into_iter()
            .any(|live| super::super::formal_bft_read_cli::paths_overlap(&archive_root, live))
        {
            return Err("committed archive and original ledger/CAS must be disjoint".into());
        }
        let source = Arc::new(CasBusinessIndexReader {
            store: store.clone(),
        });
        let mut ledger = FormalBftHeaderLedgerExecutionV3::open_from_handoff(
            &ledger_root,
            store,
            anchor,
            config.target.clone(),
            material,
            source,
            false,
        )?;
        let (reference, original) = ledger.export_committed_prefix(
            &archive,
            &options.source,
            config.exact_height,
            &config.exact_header_id,
        )?;
        (reference, Some(original))
    };
    let (replayed, candidate) = if let Some(candidate) = options.candidate {
        let (cut, replayed) = candidate.replay(
            &archive,
            &reference,
            &options.source,
            anchor,
            config.target,
            (config.exact_height, &config.exact_header_id),
        )?;
        (cut, Some(replayed))
    } else {
        (
            FormalBftHeaderLedgerExecutionV3::replay_committed_prefix(
                &archive,
                &reference,
                &options.source,
                anchor,
                config.target,
                config.exact_height,
                &config.exact_header_id,
            )?,
            None,
        )
    };
    if original.is_some_and(|original| original != replayed) {
        return Err("cold archive replay differs from original durable cut".into());
    }
    Ok(ArchiveReport {
        schema_version: "oasis7.formal_bft_committed_prefix_archive.v1",
        reference,
        replayed,
        candidate,
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FirstReserveConfig {
    schema_version: u32,
    archive_directory: PathBuf,
    source: BftSuccessorWindowSourceV1,
    creation_marker: BftPrivateRetentionCreationMarkerV1,
    expected_creation_marker_digest: String,
    candidate: OriginalCandidateArchivePinV1,
}

#[derive(Serialize)]
struct FirstReserveReport {
    schema_version: &'static str,
    candidate: ReplayedOriginalCandidateV1,
    first_reservation_bytes: u64,
    private_pin_bytes: u64,
    original_selection_bytes: u64,
    retained_h_dependency_count: usize,
}

/// Operator verification of preserved material. The protected signer obtains
/// its own authenticated source and does not consume the local report.
fn verify_first_reserve_from_paths(
    config_path: &Path,
    archive_path: &Path,
) -> Result<FirstReserveReport, String> {
    let config = load_config(config_path)?;
    let options: FirstReserveConfig =
        serde_json::from_slice(&read_private(archive_path, 64 * 1024)?)
            .map_err(|_| "invalid first Reserve verification config")?;
    if options.schema_version != 1 {
        return Err("unsupported first Reserve verification config schema".into());
    }
    let archive_root = require_existing_directory(
        &options.archive_directory,
        "private original input archive CAS",
    )?;
    crate::controlled_live_config::require_private_owned(&archive_root, true)?;
    let archive = LocalBftSuccessorArchiveStoreV1::new(LocalCasStore::new(&archive_root))
        .map_err(|_| "first Reserve archive CAS path rejected")?;
    let LoadedH {
        anchor,
        material,
        store,
    } = load_h(&config)?;
    let ledger_root =
        require_existing_directory(&config.ledger_directory, "private header ledger")?;
    if [store.root(), ledger_root.as_path()]
        .into_iter()
        .any(|live| super::super::formal_bft_read_cli::paths_overlap(&archive_root, live))
    {
        return Err("original input archive and original ledger/CAS must be disjoint".into());
    }
    let source = Arc::new(CasBusinessIndexReader {
        store: store.clone(),
    });
    let ledger = FormalBftHeaderLedgerExecutionV3::open_from_handoff(
        &ledger_root,
        store,
        anchor,
        config.target,
        material,
        source,
        false,
    )?;
    let reference = &options.candidate.reference;
    if reference.key.height != config.exact_height
        || reference.key.header_id != config.exact_header_id
    {
        return Err("first Reserve candidate differs from exact operator target".into());
    }
    let input = archive
        .load_original_admission_input(reference)
        .map_err(|error| error.to_string())?;
    let (candidate, price) = options.candidate.verify_first_price(
        &ledger,
        &input,
        &options.source,
        &options.creation_marker,
        &options.expected_creation_marker_digest,
    )?;
    Ok(FirstReserveReport {
        schema_version: "oasis7.formal_bft_local_first_reserve_verification.v1",
        candidate,
        first_reservation_bytes: input.publication_reservation_bytes,
        private_pin_bytes: price.private_pin_bytes,
        original_selection_bytes: price.original_selection_bytes,
        retained_h_dependency_count: price.retained_h_dependencies.len(),
    })
}

fn load_config(config_path: &Path) -> Result<Config, String> {
    let bytes = read_private(config_path, 2 * 1024 * 1024)?;
    let config: Config =
        serde_json::from_slice(&bytes).map_err(|_| "invalid private header ledger config")?;
    if config.schema_version != 1 {
        return Err("unsupported private header ledger config schema".into());
    }
    config.target.validate()?;
    if config.exact_height == 0
        || config.exact_header_id.len() != 64
        || !config
            .exact_header_id
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err("private header ledger requires an independently pinned exact target".into());
    }
    Ok(config)
}

struct LoadedH {
    anchor: oasis7_formal_protocol::controlled_authority::formal::VerifiedFormalReadAnchorV1,
    material: oasis7_distfs::controlled_authority::replicated_protocol::ClosedRecord,
    store: LocalCasStore,
}
fn load_h(config: &Config) -> Result<LoadedH, String> {
    let cas = require_existing_directory(&config.cas_directory, "private header ledger CAS")?;
    super::super::controlled_live_config::require_private_owned(&cas, true)?;
    let anchor = load_qualified_handoff_anchor(&config.trust_config_path)
        .map_err(|_| "private header ledger trust rejected")?;
    oasis7_formal_protocol::bft::verify_formal_bft_handoff_v2(&anchor, config.target.clone())
        .map_err(|_| "private header ledger target is not admitted by qualified H")?;
    super::super::controlled_live_config::require_private_owned(
        &config.handoff_material_path,
        false,
    )?;
    let material = read_qualified_handoff_material(&config.handoff_material_path)
        .map_err(|_| "private header ledger H material rejected")?;
    let store = LocalCasStore::new(cas);
    Ok(LoadedH {
        anchor,
        material,
        store,
    })
}
