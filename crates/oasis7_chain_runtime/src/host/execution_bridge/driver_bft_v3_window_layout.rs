//! Original install owns first window creation; Adopt and cold-open only read back.
use super::super::successor_archive::{MAX_CLOSURE, MAX_METADATA, err, live};
use super::*;
use oasis7_distfs::protected_bft_v3::{BftSuccessorWindowArchiveRefV1, BftSuccessorWindowStatusV1};

pub(in super::super) fn segment_path(
    records: &Path,
    base: &VerifiedPrivateSuccessorBaseV1,
) -> Result<PathBuf, String> {
    Ok(records.join("successor-active-v1").join(
        base.original
            .as_ref()
            .ok_or("window original absent")?
            .request
            .intent
            .body
            .checkpoint
            .digest()
            .map_err(err)?,
    ))
}
pub(in super::super) fn base_charge(
    base: &VerifiedPrivateSuccessorBaseV1,
) -> Result<usize, String> {
    let mut bytes = base_identity(base)?.len() as u64;
    for amount in [
        base.metadata.material.bytes,
        base.metadata.cut.snapshot_ref.bytes,
        base.metadata.cut.journal_ref.bytes,
    ] {
        bytes = bytes
            .checked_add(amount)
            .filter(|n| *n <= MAX_CLOSURE as u64)
            .ok_or("active private base material capacity")?;
    }
    usize::try_from(bytes).map_err(|_| "active private base capacity conversion".into())
}
pub(in super::super) fn prepare(
    records: &Path,
    store: LocalCasStore,
    base: &VerifiedPrivateSuccessorBaseV1,
    deadline: Instant,
) -> Result<BftSuccessorWindowArchiveRefV1, String> {
    let original = base.original.as_ref().ok_or("window original absent")?;
    if original.request.status != BftSuccessorWindowStatusV1::InstalledPending {
        return Err(
            "first active segment creation requires original InstalledPending transaction".into(),
        );
    }
    live(deadline)?;
    let records = canonical_storage_path(records)?;
    let directory = records.join("successor-active-v1");
    let segment = segment_path(&records, base)?;
    private_directory(&directory)?;
    private_directory(&segment)?;
    let bytes = base_identity(base)?;
    let path = segment.join("base.cbor");
    if let Some(existing) = common_storage::read_optional_bytes(&path, MAX_METADATA)? {
        files::single_link(&path)?;
        if existing != bytes {
            return Err("active segment init original base differs".into());
        }
    } else {
        // A completed/later segment whose original base was lost cannot initialize.
        for entry in std::fs::read_dir(&segment).map_err(err)? {
            let entry = entry.map_err(err)?;
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| "window init filename")?;
            if files::stage_target(&name) != Some("base.cbor") {
                return Err("active segment lost original base; initialization refused".into());
            }
            if common_storage::read_optional_bytes(&entry.path(), MAX_METADATA)?.as_deref()
                != Some(bytes.as_slice())
            {
                return Err("active base init stage differs".into());
            }
        }
        files::publish(&path, &bytes)?;
    }
    files::clean_equivalent_stages(&path, &bytes)?;
    let mut ledger = WindowLedger::open(&segment, true, base_charge(base)?)?;
    // Directory identities must exist before the immutable Origin/init intent.
    private_directory(
        &segment
            .join(DIRECTORY)
            .join(storage::NODE_PRIVATE_DIRECTORY),
    )?;
    ledger.bind(&blake3_hex(&bytes), base.execution.schedule.height())?;
    let private = node_execution::WindowPrivateStore::open(
        segment
            .join(DIRECTORY)
            .join(storage::NODE_PRIVATE_DIRECTORY),
        store,
        base,
        &mut ledger,
    )?;
    ledger.require_completed()?;
    drop(private);
    drop(ledger);
    let root = successor_carriers::InstallationRootV1::new(records)?;
    let (_, reference) = root.read_window_origin(base, deadline)?;
    Ok(reference)
}
pub(in super::super) fn verify_ready(
    records: &Path,
    store: LocalCasStore,
    base: &VerifiedPrivateSuccessorBaseV1,
    expected: &BftSuccessorWindowArchiveRefV1,
    initial: bool,
    deadline: Instant,
) -> Result<(), String> {
    let root = successor_carriers::InstallationRootV1::new(records.into())?;
    let (origin, reference) = root.read_window_origin(base, deadline)?;
    if &reference != expected {
        return Err("active Origin differs from protected adoption commitment".into());
    }
    node_execution::WindowPrivateStore::verify_ready(
        segment_path(records, base)?
            .join(DIRECTORY)
            .join(storage::NODE_PRIVATE_DIRECTORY),
        store,
        base,
        &origin,
        initial,
    )?;
    if root.read_window_origin(base, deadline)?.1 != reference {
        return Err("active Origin changed during private ready readback".into());
    }
    Ok(())
}
