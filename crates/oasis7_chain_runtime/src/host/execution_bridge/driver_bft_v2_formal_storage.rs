//! Durable, one-height storage for the first formal BFT read cut.
//!
//! This ledger is deliberately separate from the engineering BFT V2 ledger:
//! its pin names an externally qualified controlled-profile H and the exact
//! BFT profile target committed by that H.
use super::super::super::storage as common_storage;
use oasis7::world_service::CanonicalIntentResultV1;
use oasis7_consensus::tendermint_v2::{
    BftProfileTargetV1, FormalBftExecutionCommitmentV1, VerifiedBftHandoffTargetV1,
};
use oasis7_distfs::controlled_authority::formal::FormalHeadRefV1;
use oasis7_node::{BftCommitCertificateV2, BftScopeV2};
use oasis7_proto::distributed_bft::v2::{BFT_MAX_SIGNED_MATERIAL_BYTES_V2, BftBlockV2};
use oasis7_runtime::{LocalCasStore, blake3_hex};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::os::fd::AsRawFd;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

const LEDGER_DIR: &str = "formal-bft-read-v1";
const REQUIRED_MARKER: &str = "formal-bft-read-required-v1";
const REQUIRED_MARKER_BYTES: &[u8] = b"oasis7.formal.bft.read.required/v1\n";
const LOCK_FILE: &str = "writer.lock";
const PIN_FILE: &str = "pin.cbor";
const RECORD_FILE: &str = "height-00000000000000000001.cbor";
const SCHEMA_VERSION: u32 = 2;
const PIN_DOMAIN: &str = "oasis7.formal.bft.read.pin.v1";
const MAX_PIN_BYTES: usize = 2 * 1024 * 1024;
const MAX_RECORD_BYTES: usize = 8 * 1024 * 1024;
const MAX_NEW_RESULTS: usize = 1024;
const MAX_NEW_RESULTS_BYTES: usize = 4 * 1024 * 1024;
const CAS_PIN_SCOPE: &str = "formal-bft-read-v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FormalBftReadPinV1 {
    schema_version: u32,
    target: BftProfileTargetV1,
    target_digest: String,
    activation_digest: String,
    handoff_head: FormalHeadRefV1,
    scope: BftScopeV2,
    raw_snapshot_root: String,
    parent_semantic_state_root: String,
    parent_journal_root: String,
    parent_manifest_hash: String,
    governing_manifest_digest: String,
    parent_result_root: String,
    parent_projection_root: String,
    closed_material_digest: String,
}

impl FormalBftReadPinV1 {
    pub(super) fn from_handoff(
        handoff: &VerifiedBftHandoffTargetV1,
        parent_semantic_state_root: String,
        parent_manifest_hash: String,
        parent_journal_root: String,
        parent_result_root: String,
        parent_projection_root: String,
        closed_material_digest: String,
    ) -> Result<Self, String> {
        let pin = Self {
            schema_version: SCHEMA_VERSION,
            target: handoff.target().clone(),
            target_digest: handoff.target_digest().to_owned(),
            activation_digest: handoff.activation_digest().to_owned(),
            handoff_head: handoff.handoff_head().clone(),
            scope: handoff.scope().clone(),
            raw_snapshot_root: handoff.raw_snapshot_root().to_owned(),
            parent_semantic_state_root,
            parent_journal_root,
            parent_manifest_hash,
            governing_manifest_digest: handoff.governing_manifest_digest().to_owned(),
            parent_result_root,
            parent_projection_root,
            closed_material_digest,
        };
        pin.validate()?;
        Ok(pin)
    }

    fn validate(&self) -> Result<(), String> {
        self.target.validate()?;
        self.scope.validate()?;
        for (name, value) in [
            ("target_digest", self.target_digest.as_str()),
            ("activation_digest", self.activation_digest.as_str()),
            ("raw_snapshot_root", self.raw_snapshot_root.as_str()),
            (
                "parent_semantic_state_root",
                self.parent_semantic_state_root.as_str(),
            ),
            ("parent_journal_root", self.parent_journal_root.as_str()),
            ("parent_manifest_hash", self.parent_manifest_hash.as_str()),
            (
                "governing_manifest_digest",
                self.governing_manifest_digest.as_str(),
            ),
            ("parent_result_root", self.parent_result_root.as_str()),
            (
                "parent_projection_root",
                self.parent_projection_root.as_str(),
            ),
            (
                "closed_material_digest",
                self.closed_material_digest.as_str(),
            ),
        ] {
            validate_hash(name, value)?;
        }
        if self.schema_version != SCHEMA_VERSION
            || self.target.scope() != &self.scope
            || self.target.target_digest()? != self.target_digest
            || self.handoff_head.position == 0
            || self.handoff_head.position.checked_add(1).is_none()
        {
            return Err("formal BFT read pin header or target mismatch".into());
        }
        Ok(())
    }

    pub(super) fn digest(&self) -> Result<String, String> {
        self.validate()?;
        let bytes = serde_cbor::to_vec(&(PIN_DOMAIN, self))
            .map_err(|error| format!("encode formal BFT read pin digest: {error}"))?;
        Ok(blake3_hex(&bytes))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FormalBftReadRecordV1 {
    schema_version: u32,
    pub(super) height: u64,
    pub(super) certificate: BftCommitCertificateV2,
    pub(super) commitment: FormalBftExecutionCommitmentV1,
    pub(super) block_id: String,
    pub(super) snapshot_ref: String,
    pub(super) journal_ref: String,
    pub(super) business_index_refs: Vec<String>,
    pub(super) new_results: Vec<CanonicalIntentResultV1>,
}

pub(super) struct PreloadedBusinessIndexNodes {
    pub(super) refs: BTreeSet<String>,
    pub(super) nodes: BTreeMap<String, Vec<u8>>,
    pub(super) record_present: bool,
}

pub(super) struct FormalBftReadLedgerV1 {
    dir: PathBuf,
    _lock: File,
    _format_lock: File,
    pin: FormalBftReadPinV1,
    record: Option<FormalBftReadRecordV1>,
}

pub(super) struct FormalBftReadCandidateV1<'a> {
    pub(super) certificate: BftCommitCertificateV2,
    pub(super) commitment: FormalBftExecutionCommitmentV1,
    pub(super) block: &'a BftBlockV2,
    pub(super) block_id: String,
    pub(super) snapshot_bytes: &'a [u8],
    pub(super) journal_bytes: &'a [u8],
    pub(super) business_index_nodes: &'a BTreeMap<String, Vec<u8>>,
    pub(super) new_results: &'a [CanonicalIntentResultV1],
}

impl FormalBftReadLedgerV1 {
    pub(super) fn required_material_exists(records_dir: &Path) -> Result<bool, String> {
        if common_storage::has_formal_header_prefix_material(records_dir)? {
            return Ok(true);
        }
        let dir = records_dir.join(LEDGER_DIR);
        let ledger_present = match fs::symlink_metadata(dir.as_path()) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
                return Err("formal BFT read path is not a real directory".into());
            }
            Ok(_) => true,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
            Err(error) => return Err(format!("inspect formal BFT read path: {error}")),
        };
        let marker_present = read_required_marker(records_dir.join(REQUIRED_MARKER).as_path())?;
        if marker_present && !ledger_present {
            return Err("formal BFT read marker exists without its pinned ledger".into());
        }
        Ok(marker_present || ledger_present)
    }

    pub(super) fn preload_business_index_nodes(
        records_dir: &Path,
        store: &LocalCasStore,
    ) -> Result<PreloadedBusinessIndexNodes, String> {
        let dir = records_dir.join(LEDGER_DIR);
        match fs::symlink_metadata(dir.as_path()) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
                return Err("formal BFT read path is not a real directory".into());
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(PreloadedBusinessIndexNodes {
                    refs: BTreeSet::new(),
                    nodes: BTreeMap::new(),
                    record_present: false,
                });
            }
            Err(error) => return Err(format!("inspect formal BFT read path: {error}")),
        }
        super::super::super::super::super::controlled_live::private_directory(dir.as_path())?;
        let record_path = dir.join(RECORD_FILE);
        let Some(record) = common_storage::read_optional_canonical::<FormalBftReadRecordV1>(
            record_path.as_path(),
            MAX_RECORD_BYTES,
        )?
        else {
            return Ok(PreloadedBusinessIndexNodes {
                refs: BTreeSet::new(),
                nodes: BTreeMap::new(),
                record_present: false,
            });
        };
        let refs = validate_business_index_refs(&record.business_index_refs)?;
        let nodes = read_business_index_nodes_bounded(
            &refs,
            super::formal_dependencies::BUSINESS_INDEX_MAX_NODE_BYTES,
            super::formal_dependencies::MAX_RETAINED_INDEX_TOTAL_BYTES,
            |digest, maximum| common_storage::read_cas_bounded(store, digest, maximum),
        )?;
        super::formal_dependencies::validate_nodes(&nodes)?;
        Ok(PreloadedBusinessIndexNodes {
            refs,
            nodes,
            record_present: true,
        })
    }

    pub(super) fn open(
        records_dir: &Path,
        pin: FormalBftReadPinV1,
        store: &LocalCasStore,
        allow_initialize: bool,
        preloaded_refs: &BTreeSet<String>,
        preloaded_record_present: bool,
        observed_h_refs: &BTreeSet<String>,
    ) -> Result<Self, String> {
        if !cfg!(unix) {
            return Err("durable formal BFT read ledger is unsupported on this platform".into());
        }
        pin.validate()?;
        let format_lock = common_storage::lock_format(records_dir)?;
        if common_storage::has_formal_header_prefix_material(records_dir)?
            || super::formal_prefix_storage::Ledger::exists(records_dir)?
        {
            return Err("old H+1 format refuses a multiheight prefix ledger".into());
        }
        if common_storage::BftExecutionLedger::required_material_exists(records_dir)?
            || super::super::storage::BftV2ExecutionLedger::required_material_exists(records_dir)?
        {
            return Err("formal H+1 format refuses an engineering ledger".into());
        }
        let dir = records_dir.join(LEDGER_DIR);
        let existed = match fs::symlink_metadata(dir.as_path()) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
                return Err("formal BFT read path is not a real directory".into());
            }
            Ok(_) => true,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
            Err(error) => return Err(format!("inspect formal BFT read path: {error}")),
        };
        if !existed && !allow_initialize {
            return Err("formal BFT read pin is missing; refusing to recreate it".into());
        }
        let marker_path = records_dir.join(REQUIRED_MARKER);
        let marker_present = read_required_marker(marker_path.as_path())?;
        if !existed && marker_present {
            return Err("formal BFT read marker exists without its pinned ledger".into());
        }
        if existed && !marker_present && !allow_initialize {
            return Err("formal BFT read marker is missing; refusing to recreate it".into());
        }
        super::super::super::super::super::controlled_live::private_directory(dir.as_path())?;

        let lock_path = dir.join(LOCK_FILE);
        let mut options = OpenOptions::new();
        options
            .read(true)
            .write(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
        if existed {
            if !lock_path.is_file() {
                return Err("formal BFT read writer lock is missing".into());
            }
        } else {
            options.create_new(true);
        }
        let lock = options
            .open(lock_path.as_path())
            .map_err(|error| format!("open formal BFT read writer lock: {error}"))?;
        crate::controlled_live_config::require_private_owned(lock_path.as_path(), false)?;
        if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
            return Err("formal BFT read ledger is already open by another writer".into());
        }
        let lock_metadata = lock
            .metadata()
            .map_err(|error| format!("inspect formal BFT read writer lock: {error}"))?;
        if !lock_metadata.is_file() || lock_metadata.len() != 0 {
            return Err("formal BFT read writer lock is malformed".into());
        }

        let pin_path = dir.join(PIN_FILE);
        match common_storage::read_optional_canonical::<FormalBftReadPinV1>(
            pin_path.as_path(),
            MAX_PIN_BYTES,
        )? {
            Some(stored) if stored != pin => {
                return Err("formal BFT read ledger is pinned to a different H or target".into());
            }
            Some(_) => {}
            None if !existed && allow_initialize => {
                let bytes = serde_cbor::to_vec(&pin)
                    .map_err(|error| format!("encode formal BFT read pin: {error}"))?;
                if bytes.len() > MAX_PIN_BYTES {
                    return Err("formal BFT read pin exceeds byte bound".into());
                }
                common_storage::write_immutable_file(pin_path.as_path(), bytes.as_slice())?;
            }
            None => return Err("formal BFT read pin is missing or incomplete".into()),
        }
        let recovering_interrupted_initialization = existed && !marker_present;
        if recovering_interrupted_initialization {
            validate_interrupted_initialization(dir.as_path())?;
        }

        let mut ledger = Self {
            dir,
            _lock: lock,
            _format_lock: format_lock,
            pin,
            record: None,
        };
        let record_path = ledger.record_path();
        ledger.record = common_storage::read_optional_canonical::<FormalBftReadRecordV1>(
            record_path.as_path(),
            MAX_RECORD_BYTES,
        )?;
        if ledger.record.is_some() != preloaded_record_present {
            return Err("formal BFT read record changed after overlay preload".into());
        }
        if let Some(record) = ledger.record.clone() {
            validate_record(&record, &ledger.pin)?;
            if record
                .business_index_refs
                .iter()
                .cloned()
                .collect::<BTreeSet<_>>()
                != *preloaded_refs
                || !observed_h_refs
                    .iter()
                    .all(|reference| preloaded_refs.contains(reference))
            {
                return Err(
                    "formal BFT read index-node overlay differs from its immutable record".into(),
                );
            }
            ledger.validate_cas_material(&record, store)?;
            ledger.pin_candidate_refs(&record, store)?;
        } else if !preloaded_refs.is_empty() {
            return Err("formal BFT read record disappeared after overlay preload".into());
        }
        if recovering_interrupted_initialization {
            // Initialization writes the immutable pin before this marker. A
            // crash in that narrow window is recoverable only when the exact
            // caller-supplied pin is already present and no ledger record or
            // unknown file could have been created. Never invent a missing
            // pin during recovery.
            validate_interrupted_initialization(ledger.dir.as_path())?;
            common_storage::write_immutable_file(marker_path.as_path(), REQUIRED_MARKER_BYTES)?;
        } else if !existed {
            common_storage::write_immutable_file(marker_path.as_path(), REQUIRED_MARKER_BYTES)?;
        }
        Ok(ledger)
    }

    pub(super) fn record(&self) -> Option<&FormalBftReadRecordV1> {
        self.record.as_ref()
    }

    pub(super) fn reread_record(
        &self,
        store: &LocalCasStore,
    ) -> Result<Option<FormalBftReadRecordV1>, String> {
        let reread = common_storage::read_optional_canonical::<FormalBftReadRecordV1>(
            self.record_path().as_path(),
            MAX_RECORD_BYTES,
        )?;
        if reread != self.record {
            return Err("formal BFT read record changed after it was opened".into());
        }
        if let Some(record) = reread.as_ref() {
            validate_record(record, &self.pin)?;
            self.validate_cas_material(record, store)?;
        }
        Ok(reread)
    }

    pub(super) fn validate_observed_business_index_refs(
        &self,
        observed: &BTreeSet<String>,
    ) -> Result<(), String> {
        let record = self
            .record
            .as_ref()
            .ok_or("formal BFT read record is absent")?;
        let retained: BTreeSet<_> = record.business_index_refs.iter().collect();
        if observed
            .iter()
            .any(|reference| !retained.contains(reference))
        {
            return Err("formal BFT replay consumed an unretained business-index node".into());
        }
        Ok(())
    }

    pub(super) fn commit(
        &mut self,
        candidate: FormalBftReadCandidateV1<'_>,
        store: &LocalCasStore,
    ) -> Result<&FormalBftReadRecordV1, String> {
        self.commit_inner(candidate, store, || Ok(()))
    }

    #[cfg(test)]
    pub(super) fn commit_with_before_record_hook(
        &mut self,
        candidate: FormalBftReadCandidateV1<'_>,
        store: &LocalCasStore,
        before_record: impl FnOnce() -> Result<(), String>,
    ) -> Result<&FormalBftReadRecordV1, String> {
        self.commit_inner(candidate, store, before_record)
    }

    fn commit_inner(
        &mut self,
        candidate: FormalBftReadCandidateV1<'_>,
        store: &LocalCasStore,
        before_record: impl FnOnce() -> Result<(), String>,
    ) -> Result<&FormalBftReadRecordV1, String> {
        let expected_height = self
            .pin
            .handoff_head
            .position
            .checked_add(1)
            .ok_or("formal BFT activation height overflow")?;
        if candidate.block.height != expected_height
            || candidate.block.parent_commit_ref != self.pin.handoff_head.record_id
            || candidate.block_id != candidate.block.block_id()?
            || candidate.commitment.height() != expected_height
            || candidate.commitment.parent_commit_ref() != self.pin.handoff_head.record_id
        {
            return Err("formal BFT candidate does not extend the exact H handoff".into());
        }
        candidate.certificate.validate_wire_bounds()?;
        let certificate_bytes = serde_cbor::to_vec(&candidate.certificate)
            .map_err(|error| format!("encode formal BFT certificate: {error}"))?;
        if certificate_bytes.len() > BFT_MAX_SIGNED_MATERIAL_BYTES_V2 {
            return Err("formal BFT certificate exceeds signed-material bound".into());
        }
        validate_result_bounds(candidate.new_results)?;
        super::formal_dependencies::validate_nodes(candidate.business_index_nodes)?;

        if let Some(existing) = self.record.as_ref() {
            if existing.commitment != candidate.commitment
                || existing.block_id != candidate.block_id
            {
                return Err(
                    "formal BFT H+1 already contains a different durable block or commitment"
                        .into(),
                );
            }
            self.pin_candidate_refs(existing, store)?;
            return Ok(self.record.as_ref().expect("record exists"));
        }

        let snapshot_ref = common_storage::put_cas_durable(
            store,
            candidate.snapshot_bytes,
            common_storage::MAX_BFT_SNAPSHOT_BYTES,
        )?;
        let journal_ref = common_storage::put_cas_durable(
            store,
            candidate.journal_bytes,
            common_storage::MAX_BFT_JOURNAL_BYTES,
        )?;
        let mut business_index_refs = Vec::with_capacity(candidate.business_index_nodes.len());
        for (digest, node) in candidate.business_index_nodes {
            let stored = common_storage::put_cas_durable(
                store,
                node.as_slice(),
                super::formal_dependencies::BUSINESS_INDEX_MAX_NODE_BYTES,
            )?;
            if stored != *digest {
                return Err("stored formal BFT index node digest changed".into());
            }
            business_index_refs.push(digest.clone());
        }
        let record = FormalBftReadRecordV1 {
            schema_version: SCHEMA_VERSION,
            height: expected_height,
            certificate: candidate.certificate,
            commitment: candidate.commitment,
            block_id: candidate.block_id,
            snapshot_ref,
            journal_ref,
            business_index_refs,
            new_results: candidate.new_results.to_vec(),
        };
        validate_record(&record, &self.pin)?;
        let bytes = serde_cbor::to_vec(&record)
            .map_err(|error| format!("encode formal BFT read record: {error}"))?;
        if bytes.len() > MAX_RECORD_BYTES {
            return Err("formal BFT read record exceeds byte bound".into());
        }
        // CAS material must remain protected even if a crash occurs after the
        // immutable record write but before the caller next opens this ledger.
        // A failed later step can leave an orphan pin, which is safe to retry.
        self.pin_candidate_refs(&record, store)?;
        before_record()?;
        common_storage::write_immutable_file(self.record_path().as_path(), bytes.as_slice())?;
        self.record = Some(record);
        Ok(self.record.as_ref().expect("record stored"))
    }

    pub(super) fn validate_cas_material(
        &self,
        record: &FormalBftReadRecordV1,
        store: &LocalCasStore,
    ) -> Result<(Vec<u8>, Vec<u8>), String> {
        let snapshot = common_storage::read_cas_bounded(
            store,
            record.snapshot_ref.as_str(),
            common_storage::MAX_BFT_SNAPSHOT_BYTES,
        )?;
        let journal = common_storage::read_cas_bounded(
            store,
            record.journal_ref.as_str(),
            common_storage::MAX_BFT_JOURNAL_BYTES,
        )?;
        let refs = validate_business_index_refs(&record.business_index_refs)?;
        let _business_index_nodes = read_business_index_nodes_bounded(
            &refs,
            super::formal_dependencies::BUSINESS_INDEX_MAX_NODE_BYTES,
            super::formal_dependencies::MAX_RETAINED_INDEX_TOTAL_BYTES,
            |digest, maximum| common_storage::read_cas_bounded(store, digest, maximum),
        )?;
        Ok((snapshot, journal))
    }

    fn pin_candidate_refs(
        &self,
        record: &FormalBftReadRecordV1,
        store: &LocalCasStore,
    ) -> Result<(), String> {
        let mut refs = BTreeSet::from([record.snapshot_ref.clone(), record.journal_ref.clone()]);
        refs.extend(record.business_index_refs.iter().cloned());
        self.pin_refs(&refs, store)
    }

    fn pin_refs(&self, refs: &BTreeSet<String>, store: &LocalCasStore) -> Result<(), String> {
        let shard = format!("handoff-{}", self.pin.digest()?);
        store
            .replace_pin_scope_shard(CAS_PIN_SCOPE, shard.as_str(), refs)
            .map_err(|error| format!("pin formal BFT read World material: {error:?}"))
    }

    fn record_path(&self) -> PathBuf {
        self.dir.join(RECORD_FILE)
    }
}

fn read_required_marker(path: &Path) -> Result<bool, String> {
    let Some(bytes) = common_storage::read_optional_bytes(path, REQUIRED_MARKER_BYTES.len())?
    else {
        return Ok(false);
    };
    crate::controlled_live_config::require_private_owned(path, false)?;
    if bytes.as_slice() != REQUIRED_MARKER_BYTES {
        return Err("formal BFT read required marker is malformed".into());
    }
    Ok(true)
}

fn validate_interrupted_initialization(dir: &Path) -> Result<(), String> {
    let mut names = BTreeSet::new();
    for entry in fs::read_dir(dir)
        .map_err(|error| format!("inspect interrupted formal BFT initialization: {error}"))?
    {
        let entry = entry.map_err(|error| {
            format!("read interrupted formal BFT initialization entry: {error}")
        })?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| "formal BFT read initialization has a non-UTF8 entry".to_owned())?;
        if !matches!(name.as_str(), LOCK_FILE | PIN_FILE) || !names.insert(name) {
            return Err("formal BFT read initialization contains unexpected material".into());
        }
    }
    if names != BTreeSet::from([LOCK_FILE.to_owned(), PIN_FILE.to_owned()]) {
        return Err(
            "formal BFT read initialization is missing its exact pin or writer lock".into(),
        );
    }
    Ok(())
}

fn validate_record(record: &FormalBftReadRecordV1, pin: &FormalBftReadPinV1) -> Result<(), String> {
    let height = pin
        .handoff_head
        .position
        .checked_add(1)
        .ok_or("formal BFT activation height overflow")?;
    record.commitment.validate()?;
    let block = &record.certificate.proposal.block;
    record.certificate.validate_wire_bounds()?;
    record
        .commitment
        .verify_block(block, pin.activation_digest.as_str())?;
    if record.schema_version != SCHEMA_VERSION
        || record.height != height
        || block.height != height
        || block.scope != pin.scope
        || block.parent_commit_ref != pin.handoff_head.record_id
        || record.commitment.scope() != &pin.scope
        || record.commitment.parent_commit_ref() != pin.handoff_head.record_id
        || record.commitment.height() != height
        || record.commitment.activation_digest() != pin.activation_digest
        || block.manifest_hash != pin.parent_manifest_hash
        || record.commitment.manifest_hash() != pin.parent_manifest_hash
        || record.block_id != block.block_id()?
        || record.commitment.digest()? != block.execution_block_hash
    {
        return Err("formal BFT durable record differs from its pinned H/target".into());
    }
    validate_result_bounds(&record.new_results)?;
    validate_business_index_refs(&record.business_index_refs)?;
    for (name, reference) in [
        ("snapshot_ref", record.snapshot_ref.as_str()),
        ("journal_ref", record.journal_ref.as_str()),
        ("block_id", record.block_id.as_str()),
    ] {
        validate_hash(name, reference)?;
    }
    Ok(())
}

fn validate_result_bounds(results: &[CanonicalIntentResultV1]) -> Result<(), String> {
    if results.len() > MAX_NEW_RESULTS {
        return Err("formal BFT new-result count exceeds bound".into());
    }
    let bytes = serde_cbor::to_vec(&results)
        .map_err(|error| format!("encode formal BFT new-result delta: {error}"))?;
    if bytes.len() > MAX_NEW_RESULTS_BYTES {
        return Err("formal BFT new-result delta exceeds byte bound".into());
    }
    Ok(())
}

fn validate_business_index_refs(refs: &[String]) -> Result<BTreeSet<String>, String> {
    if refs.len() > super::formal_dependencies::MAX_RETAINED_INDEX_NODES
        || refs.windows(2).any(|pair| pair[0] >= pair[1])
    {
        return Err("formal BFT business-index references are not bounded and canonical".into());
    }
    let mut unique = BTreeSet::new();
    for digest in refs {
        validate_hash("business_index_ref", digest)?;
        unique.insert(digest.clone());
    }
    Ok(unique)
}

pub(super) fn read_business_index_nodes_bounded(
    refs: &BTreeSet<String>,
    max_node_bytes: usize,
    max_total_bytes: usize,
    mut read_bounded: impl FnMut(&str, usize) -> Result<Vec<u8>, String>,
) -> Result<BTreeMap<String, Vec<u8>>, String> {
    if refs.len() > super::formal_dependencies::MAX_RETAINED_INDEX_NODES {
        return Err("formal BFT business-index references exceed node-count bound".into());
    }
    let mut total_bytes = 0usize;
    let mut nodes = BTreeMap::new();
    for digest in refs {
        let remaining = max_total_bytes
            .checked_sub(total_bytes)
            .ok_or("formal BFT retained index byte count overflow")?;
        if remaining == 0 {
            return Err("formal BFT retained index bytes exceed bound".into());
        }
        let read_limit = max_node_bytes.min(remaining);
        let bytes = read_bounded(digest, read_limit)?;
        if bytes.len() > read_limit {
            return Err("formal BFT business-index node exceeds aggregate read budget".into());
        }
        super::formal_dependencies::validate_node(digest, bytes.as_slice())?;
        total_bytes = total_bytes
            .checked_add(bytes.len())
            .ok_or("formal BFT retained index byte count overflow")?;
        if total_bytes > max_total_bytes {
            return Err("formal BFT retained index bytes exceed bound".into());
        }
        nodes.insert(digest.clone(), bytes);
    }
    Ok(nodes)
}

fn validate_hash(name: &str, value: &str) -> Result<(), String> {
    let bytes = hex::decode(value).map_err(|error| format!("{name} is not hex: {error}"))?;
    let _: [u8; 32] = bytes
        .as_slice()
        .try_into()
        .map_err(|_| format!("{name} must be 32 bytes"))?;
    if hex::encode(bytes) != value {
        return Err(format!("{name} is not canonical lowercase hex"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use oasis7_consensus::tendermint_v2::BftSchedulePolicyV2;
    use oasis7_proto::distributed_bft::v2::BftValidatorV2;
    use oasis7_runtime::BlobStore;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn hash(byte: u8) -> String {
        hex::encode([byte; 32])
    }

    fn pin(byte: u8) -> FormalBftReadPinV1 {
        let scope = BftScopeV2 {
            world_id: format!("formal-world-{byte}"),
            chain_id: format!("formal-chain-{byte}"),
            genesis_hash: hash(byte),
        };
        let target = BftProfileTargetV1::new(
            scope.clone(),
            1,
            vec![BftValidatorV2 {
                validator_id: format!("validator-{byte}"),
                public_key_hex: hex::encode(
                    ed25519_dalek::SigningKey::from_bytes(&[byte.wrapping_add(1); 32])
                        .verifying_key()
                        .to_bytes(),
                ),
                voting_power: 1,
            }],
            BftSchedulePolicyV2::new(1, "bft").unwrap(),
        )
        .unwrap();
        FormalBftReadPinV1 {
            schema_version: SCHEMA_VERSION,
            target_digest: target.target_digest().unwrap(),
            target,
            activation_digest: hash(byte.wrapping_add(2)),
            handoff_head: FormalHeadRefV1 {
                position: 1,
                record_id: hash(byte.wrapping_add(3)),
            },
            scope,
            raw_snapshot_root: hash(byte.wrapping_add(4)),
            parent_semantic_state_root: hash(byte.wrapping_add(5)),
            parent_journal_root: hash(byte.wrapping_add(6)),
            parent_manifest_hash: hash(byte.wrapping_add(7)),
            governing_manifest_digest: hash(byte.wrapping_add(8)),
            parent_result_root: hash(byte.wrapping_add(9)),
            parent_projection_root: hash(byte.wrapping_add(10)),
            closed_material_digest: hash(byte.wrapping_add(11)),
        }
    }

    #[test]
    fn formal_bft_ledgers_keep_distinct_cas_pin_shards() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "oasis7-formal-bft-pin-shards-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir(&root).unwrap();
        let store = LocalCasStore::new(&root);
        let first_ref = store.put_bytes(b"first formal ledger material").unwrap();
        let second_ref = store.put_bytes(b"second formal ledger material").unwrap();
        let first = FormalBftReadLedgerV1 {
            dir: root.join("first"),
            _lock: File::open("/dev/null").unwrap(),
            _format_lock: File::open("/dev/null").unwrap(),
            pin: pin(31),
            record: None,
        };
        let second = FormalBftReadLedgerV1 {
            dir: root.join("second"),
            _lock: File::open("/dev/null").unwrap(),
            _format_lock: File::open("/dev/null").unwrap(),
            pin: pin(32),
            record: None,
        };

        first
            .pin_refs(&BTreeSet::from([first_ref.clone()]), &store)
            .unwrap();
        second
            .pin_refs(&BTreeSet::from([second_ref.clone()]), &store)
            .unwrap();

        assert_eq!(
            store.list_pin_scope_shards(CAS_PIN_SCOPE).unwrap().len(),
            2,
            "different qualified H/target identities need independent shards"
        );
        assert!(store.is_effectively_pinned(&first_ref).unwrap());
        assert!(store.is_effectively_pinned(&second_ref).unwrap());
        store.prune_unpinned(0).unwrap();
        assert!(store.has(&first_ref).unwrap());
        assert!(store.has(&second_ref).unwrap());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn preload_business_index_nodes_caps_each_read_to_remaining_aggregate_budget() {
        let payloads = [vec![0x31; 6], vec![0x42; 6]];
        let nodes: BTreeMap<_, _> = payloads
            .into_iter()
            .map(|bytes| (blake3_hex(bytes.as_slice()), bytes))
            .collect();
        let refs: BTreeSet<_> = nodes.keys().cloned().collect();
        let mut read_limits = Vec::new();

        let result = read_business_index_nodes_bounded(&refs, 8, 10, |digest, maximum| {
            read_limits.push(maximum);
            let bytes = nodes.get(digest).unwrap().clone();
            if bytes.len() > maximum {
                return Err("bounded CAS read rejected object before full allocation".into());
            }
            Ok(bytes)
        });

        assert!(
            result
                .unwrap_err()
                .contains("bounded CAS read rejected object")
        );
        assert_eq!(read_limits, vec![8, 4]);
    }
}
