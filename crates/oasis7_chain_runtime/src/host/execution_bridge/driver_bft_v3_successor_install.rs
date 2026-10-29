//! Same-cut private installation after the original protected minimum commits.
//! Per-checkpoint originals are immutable; pending WALs are resolved by the
//! original writer, never interpreted as permission to sign a later window.
use super::super::super::super::super::storage as common_storage;
use super::super::super::formal_prefix_files as files;
use super::successor_archive::{MAX_METADATA, decode, encode, err, live};
use super::*;
use oasis7_distfs::protected_bft_v3::{
    BftSignerBindingV3, BftSignerLeaseV3, BftSignerStateV3, BftSuccessorWindowAdoptedEvidenceV1,
    BftSuccessorWindowCheckpointBodyV1, BftSuccessorWindowInstallIntentV1,
    BftSuccessorWindowInstallReceiptV1, BftSuccessorWindowMinimumV1, BftSuccessorWindowStatusV1,
    BftSuccessorWindowStoredRequestV1, LocalBftSuccessorArchiveStoreV1,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::time::Instant;

pub(super) const DIRECTORY: &str = "successor-installs-v1";
const DOMAIN: &str = "oasis7.formal.bft.successor-installed-pointer.v1";
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct InstalledPointerV1 {
    domain: String,
    checkpoint: BftSuccessorWindowCheckpointBodyV1,
    intent: BftSuccessorWindowInstallIntentV1,
    receipt: BftSuccessorWindowInstallReceiptV1,
    original_public_state: oasis7_distfs::protected_bft_v3::BftSuccessorWindowArchiveRefV1,
    minimum: BftSuccessorWindowMinimumV1,
    installed_revision: u64,
    // This retains the existing provider contract: the independently verified
    // InstalledPending protected state digest, not a self-referential file hash.
    readback_state_digest: String,
}
impl InstalledPointerV1 {
    pub(super) fn from_original(
        original: &BftSuccessorWindowStoredRequestV1,
        installed: &BftSignerStateV3,
    ) -> Result<Self, String> {
        let checkpoint = &original.request.intent.body.checkpoint;
        let receipt = original
            .request
            .receipt
            .as_ref()
            .ok_or("successor original receipt absent")?;
        let extension = installed
            .successor_window
            .as_ref()
            .ok_or("successor minimum absent")?;
        let minimum = extension
            .minimum
            .as_ref()
            .ok_or("successor minimum absent")?;
        if original.request.status != BftSuccessorWindowStatusV1::InstalledPending
            || minimum != &checkpoint.minimum().map_err(err)?
            || extension.active_request_id.as_deref() != Some(checkpoint.request_id.as_str())
            || extension.request(&checkpoint.request_id) != Some(original)
            || receipt.body.committed_revision != installed.revision
            || installed.binding_digest != checkpoint.source.binding_digest
            || installed.pending_driver_apply.is_some()
            || installed
                .journal
                .as_ref()
                .is_some_and(|j| j.pending_commit().is_some())
        {
            return Err("successor install differs from exact committed minimum/original".into());
        }
        Ok(Self {
            domain: DOMAIN.into(),
            checkpoint: checkpoint.clone(),
            intent: original.request.intent.clone(),
            receipt: receipt.clone(),
            original_public_state: original.original_public_state.clone(),
            minimum: minimum.clone(),
            installed_revision: installed.revision,
            readback_state_digest: installed.state_digest.clone(),
        })
    }
    pub(super) fn evidence(&self) -> BftSuccessorWindowAdoptedEvidenceV1 {
        BftSuccessorWindowAdoptedEvidenceV1 {
            checkpoint_digest: self.receipt.body.checkpoint_digest.clone(),
            driver_install_digest: self.checkpoint.cut.driver_install_digest.clone(),
            schedule_digest: self.checkpoint.successor_schedule_digest.clone(),
            journal_digest: self.checkpoint.current_journal_digest.clone(),
            readback_state_digest: self.readback_state_digest.clone(),
        }
    }
}
/// Capacity is checked before protected minimum CAS, including the largest
/// encoded future generation/revision. This DTO is sizing only, never authority.
pub(super) fn preflight_pointer(
    original: &BftSuccessorWindowStoredRequestV1,
) -> Result<(), String> {
    use oasis7_distfs::protected_bft_v3::BftSuccessorWindowInstallReceiptBodyV1;
    let body = &original.request.intent.body.checkpoint;
    let receipt = BftSuccessorWindowInstallReceiptV1 {
        body: BftSuccessorWindowInstallReceiptBodyV1 {
            schema_version: 1,
            source: body.source.clone(),
            request_id: body.request_id.clone(),
            checkpoint_digest: body.digest().map_err(err)?,
            intent_digest: original.request.intent.digest().map_err(err)?,
            minimum: body.minimum().map_err(err)?,
            lease_generation: u64::MAX,
            committed_revision: u64::MAX,
            driver_install_digest: body.cut.driver_install_digest.clone(),
        },
        provider_public_key: "00".repeat(32),
        provider_signature_hex: "00".repeat(64),
    };
    let pointer = InstalledPointerV1 {
        domain: DOMAIN.into(),
        checkpoint: body.clone(),
        intent: original.request.intent.clone(),
        receipt,
        original_public_state: original.original_public_state.clone(),
        minimum: body.minimum().map_err(err)?,
        installed_revision: u64::MAX,
        readback_state_digest: "00".repeat(32),
    };
    encode(&pointer, MAX_METADATA)?;
    Ok(())
}
pub(super) fn directory(records: &Path) -> Result<PathBuf, String> {
    let root = canonical_storage_path(records)?;
    crate::controlled_live_config::require_private_owned(&root, true)?;
    let path = root.join(DIRECTORY);
    if path.exists() {
        canonical_storage_path(&path)?;
        crate::controlled_live_config::require_private_owned(&path, true)?;
    }
    Ok(path)
}
fn paths(
    records: &Path,
    checkpoint: &BftSuccessorWindowCheckpointBodyV1,
) -> Result<(PathBuf, PathBuf), String> {
    let directory = directory(records)?;
    let digest = checkpoint.digest().map_err(err)?;
    Ok((
        directory.join(format!("{digest}.intent.cbor")),
        directory.join(format!("{digest}.installed.cbor")),
    ))
}
/// Fresh read-only installed pointer. No live ledger lock, live World, or
/// pointer DTO can replace the authenticated original request and minimum.
pub(super) fn read_installed(
    records: &Path,
    original: &BftSuccessorWindowStoredRequestV1,
    installed: &BftSignerStateV3,
    deadline: Instant,
) -> Result<InstalledPointerV1, String> {
    live(deadline)?;
    let expected = InstalledPointerV1::from_original(original, installed)?;
    let (intent, pointer) = paths(records, &expected.checkpoint)?;
    if common_storage::read_optional_bytes(&intent, MAX_METADATA)?.is_some()
        || !files::stages(&intent)?.is_empty()
        || !files::stages(&pointer)?.is_empty()
    {
        return Err("successor private install WAL is unresolved".into());
    }
    let bytes = common_storage::read_optional_bytes(&pointer, MAX_METADATA)?
        .ok_or("successor installed pointer absent")?;
    files::single_link(&pointer)?;
    let actual: InstalledPointerV1 = decode(&bytes, MAX_METADATA)?;
    if actual != expected {
        return Err("successor installed pointer is old or differs from original minimum".into());
    }
    live(deadline)?;
    Ok(actual)
}
impl FormalBftHeaderLedgerExecutionV3 {
    pub(super) fn install_successor_window(
        &mut self,
        records: &Path,
        material_identity: &str,
        cursor_height: u64,
        binding: &BftSignerBindingV3,
        provider_pin: &str,
        lease: &mut dyn BftSignerLeaseV3,
        request_id: &str,
        checkpoint_digest: &str,
        archive: Arc<LocalBftSuccessorArchiveStoreV1>,
        deadline: Instant,
    ) -> Result<BftSuccessorWindowAdoptedEvidenceV1, String> {
        live(deadline)?;
        self.ledger.verify_carriers()?;
        // Remote lease validates signatures/current lease/challenge and exact
        // backend result. A caller-supplied checkpoint is never an install API.
        let (original, checkpoint) = lease
            .get_successor_window(request_id, checkpoint_digest)
            .map_err(err)?;
        let original = original.ok_or("successor original request unresolved or absent")?;
        original.validate(binding, provider_pin).map_err(err)?;
        let body = &original.request.intent.body.checkpoint;
        if body.source.material_identity != material_identity
            || body.cut.height != cursor_height
            || body.digest().map_err(err)? != checkpoint_digest
        {
            return Err("successor private owner/cursor differs from original cut".into());
        }
        let pointer = InstalledPointerV1::from_original(&original, &checkpoint.state)?;
        let cut = self.verified_cut(body.cut.height, &body.cut.header_id)?;
        if cut.durable_record_digest() != body.cut.driver_record_digest
            || cut
                .world()
                .execution_state_identity()
                .map_err(err)?
                .state_root_hash
                != body.cut.semantic_state_root
            || cut.world().current_manifest_hash().map_err(err)? != body.cut.post_manifest_hash
        {
            return Err("successor installation differs from actual durable World".into());
        }
        let verifier = ActualBftSuccessorWindowDriverVerifierV1::new(
            self.inputs.anchor.clone(),
            self.inputs.target.clone(),
            archive,
        )?;
        verifier.verify_original_closure(&original, deadline)?;
        live(deadline)?;
        let (intent, installed) = paths(records, body)?;
        let dir = intent.parent().ok_or("successor install parent absent")?;
        if !dir.exists() {
            super::super::super::super::super::super::super::controlled_live::private_directory(
                dir,
            )?;
            files::sync(records)?;
        }
        // At most the protocol's 64 immutable request originals. Never evict a
        // completed pointer or discard an unknown/inconsistent WAL to make room.
        let mut originals = BTreeSet::from([checkpoint_digest.to_owned()]);
        let mut files_count = 0usize;
        for entry in std::fs::read_dir(dir).map_err(err)? {
            let entry = entry.map_err(err)?;
            if !entry.file_type().map_err(err)?.is_file() {
                return Err("successor install nonregular entry".into());
            }
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| "successor install filename")?;
            let target = files::stage_target(&name).unwrap_or(&name);
            let digest = target
                .strip_suffix(".intent.cbor")
                .or_else(|| target.strip_suffix(".installed.cbor"))
                .ok_or("successor install unknown original entry")?;
            if digest.len() != 64
                || !digest
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            {
                return Err("successor install filename digest".into());
            }
            originals.insert(digest.to_owned());
            files_count = files_count
                .checked_add(1)
                .ok_or("successor install count overflow")?;
            if originals.len() > 64 || files_count > 256 {
                return Err("successor install directory capacity".into());
            }
        }
        let bytes = encode(&pointer, MAX_METADATA)?;
        self.ledger.verify_carriers()?;
        if common_storage::read_optional_bytes(&installed, MAX_METADATA)?.is_none() {
            files::publish(&intent, &bytes)?;
            live(deadline)?;
            files::link_record(&installed, &intent, &bytes)?;
        }
        live(deadline)?;
        // Finish verifies equivalent bytes before deleting the original intent;
        // a conflicting old pointer is never replaced.
        if common_storage::read_optional_bytes(&intent, MAX_METADATA)?.is_some() {
            files::finish(&installed, &intent, &bytes)?;
        }
        live(deadline)?;
        files::clean_equivalent_stages(&intent, &bytes)?;
        files::clean_equivalent_stages(&installed, &bytes)?;
        files::sync(dir)?;
        self.ledger.verify_carriers()?;
        let installed = read_installed(records, &original, &checkpoint.state, deadline)?;
        Ok(installed.evidence())
    }
}
