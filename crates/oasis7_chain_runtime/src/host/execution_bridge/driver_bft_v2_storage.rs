//! Durable V2 BFT execution records. This schema is intentionally disjoint
//! from the fixed-set V1 sidecar and stores raw QC material for full replay.
use super::super::storage as common_storage;
use std::collections::BTreeSet;
use std::fs::{self, File, OpenOptions};
use std::os::fd::AsRawFd;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

use oasis7_consensus::tendermint_v2::BftHeightProposerScheduleV2;
use oasis7_node::NodeBftExecutionConfigV2;
use oasis7_proto::distributed_bft::v2::{
    BFT_MAX_SIGNED_MATERIAL_BYTES_V2, BftBlockV2, BftCommitCertificateV2,
};
use oasis7_proto::distributed_bft::{
    BFT_MAX_COMMIT_HISTORY_CBOR_BYTES_V1, BFT_MAX_COMMIT_HISTORY_HEIGHTS_V1,
    BFT_MAX_COMMIT_HISTORY_PAGE_ENTRIES_V1,
};
use oasis7_runtime::{LocalCasStore, World as RuntimeWorld, blake3_hex};
use serde::{Deserialize, Serialize};

const BFT_V2_LEDGER_DIR: &str = "bft-execution-v2";
const BFT_V2_REQUIRED_MARKER: &str = "bft-execution-required-v2";
const BFT_V2_REQUIRED_BYTES: &[u8] = b"oasis7.bft.execution.required/v2\n";
const BFT_V2_LOCK_FILE: &str = "writer.lock";
const BFT_V2_GENESIS_FILE: &str = "genesis.cbor";
const BFT_V2_RECORD_PREFIX: &str = "height-";
const BFT_V2_RECORD_SUFFIX: &str = ".cbor";
const BFT_V2_LEDGER_SCHEMA: u32 = 2;
const BFT_V2_PIN_SCOPE: &str = "bft-execution-v2";
const BFT_V2_MAX_RECORD_BYTES: usize = 4 * 1024 * 1024;
const BFT_V2_INSTALL_RECEIPT_DOMAIN: &str = "oasis7.bft.execution.install-receipt.v2";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BftV2PinnedConfig {
    scope: oasis7_proto::distributed_bft::v2::BftScopeV2,
    authority_epoch: u64,
    initial_validators: Vec<oasis7_proto::distributed_bft::v2::BftValidatorV2>,
    schedule_policy: oasis7_consensus::tendermint_v2::BftSchedulePolicyV2,
    genesis_state_root: String,
    genesis_manifest_hash: String,
}

impl From<&NodeBftExecutionConfigV2> for BftV2PinnedConfig {
    fn from(config: &NodeBftExecutionConfigV2) -> Self {
        Self {
            scope: config.scope.clone(),
            authority_epoch: config.authority_epoch,
            initial_validators: config.initial_validators.clone(),
            schedule_policy: config.schedule_policy.clone(),
            genesis_state_root: config.genesis_state_root.clone(),
            genesis_manifest_hash: config.genesis_manifest_hash.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BftV2Genesis {
    schema_version: u32,
    config: BftV2PinnedConfig,
    snapshot_ref: String,
    journal_ref: String,
    snapshot_root: String,
    journal_root: String,
    execution_state_root: String,
    manifest_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct BftV2ExecutionRecord {
    schema_version: u32,
    pub(super) height: u64,
    pub(super) certificate: BftCommitCertificateV2,
    pub(super) block_id: String,
    pub(super) execution_block_hash: String,
    pub(super) execution_state_root: String,
    pub(super) state_root: String,
    pub(super) event_root: String,
    pub(super) post_manifest_hash: String,
    pub(super) snapshot_ref: String,
    pub(super) journal_ref: String,
}

pub(super) struct BftV2ExecutionLedger {
    dir: PathBuf,
    _lock: File,
    config: NodeBftExecutionConfigV2,
    genesis: BftV2Genesis,
    tip_height: u64,
    total_certificate_bytes: usize,
}

pub(super) struct BftV2CommitCandidate<'a> {
    pub(super) certificate: BftCommitCertificateV2,
    pub(super) block_id: String,
    pub(super) block: &'a BftBlockV2,
    pub(super) snapshot_bytes: &'a [u8],
    pub(super) journal_bytes: &'a [u8],
    pub(super) execution_block_hash: &'a str,
    pub(super) execution_state_root: &'a str,
    pub(super) state_root: &'a str,
    pub(super) event_root: &'a str,
}

impl BftV2ExecutionLedger {
    pub(super) fn required_material_exists(records_dir: &Path) -> Result<bool, String> {
        let marker = records_dir.join(BFT_V2_REQUIRED_MARKER);
        let ledger = records_dir.join(BFT_V2_LEDGER_DIR);
        for path in [&marker, &ledger] {
            match fs::symlink_metadata(path) {
                Ok(metadata) if metadata.file_type().is_symlink() => {
                    return Err(format!(
                        "BFT V2 execution path is a symlink: {}",
                        path.display()
                    ));
                }
                Ok(_) => return Ok(true),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(format!("inspect BFT V2 execution path: {error}")),
            }
        }
        Ok(false)
    }

    pub(super) fn open_or_initialize(
        records_dir: &Path,
        config: &NodeBftExecutionConfigV2,
        initial_schedule: &BftHeightProposerScheduleV2,
        initial_world: &RuntimeWorld,
        store: &LocalCasStore,
        allow_initialize: bool,
    ) -> Result<(Self, RuntimeWorld), String> {
        if !cfg!(unix) {
            return Err("durable BFT V2 execution is unsupported on this platform".into());
        }
        let configured_schedule = config.validate()?;
        if &configured_schedule != initial_schedule {
            return Err("BFT V2 config schedule differs from the pinned schedule".into());
        }
        let dir = records_dir.join(BFT_V2_LEDGER_DIR);
        let existed = match fs::symlink_metadata(dir.as_path()) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
                return Err("BFT V2 execution directory is not a real directory".into());
            }
            Ok(_) => true,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
            Err(error) => return Err(format!("inspect BFT V2 execution directory: {error}")),
        };
        if !existed && !allow_initialize {
            return Err(
                "BFT V2 execution directory is missing; refusing to recreate pinned state".into(),
            );
        }
        if !existed {
            validate_world_scope(initial_world, config)?;
        }
        super::super::super::super::controlled_live::private_directory(dir.as_path())?;

        let lock_path = dir.join(BFT_V2_LOCK_FILE);
        let mut lock_options = OpenOptions::new();
        lock_options
            .read(true)
            .write(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
        if existed {
            if !lock_path.is_file() {
                return Err(
                    "BFT V2 execution writer lock is missing; refusing to recreate it".into(),
                );
            }
        } else {
            lock_options.create_new(true);
        }
        let lock = lock_options
            .open(lock_path.as_path())
            .map_err(|error| format!("open BFT V2 execution lock: {error}"))?;
        crate::controlled_live_config::require_private_owned(lock_path.as_path(), false)?;
        if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
            return Err("BFT V2 execution ledger is already open by another writer".into());
        }

        let genesis_path = dir.join(BFT_V2_GENESIS_FILE);
        let genesis = match common_storage::read_optional_canonical::<BftV2Genesis>(
            genesis_path.as_path(),
            2 * 1024 * 1024,
        )? {
            Some(genesis) => genesis,
            None if allow_initialize && !existed => {
                initialize_genesis(records_dir, dir.as_path(), config, initial_world, store)?
            }
            None => {
                return Err(
                    "BFT V2 execution ledger is incomplete; refusing to reset genesis".into(),
                );
            }
        };
        if genesis.schema_version != BFT_V2_LEDGER_SCHEMA
            || genesis.config != BftV2PinnedConfig::from(config)
        {
            return Err("BFT V2 genesis is bound to a different config or schema".into());
        }
        validate_hash(&genesis.snapshot_ref)?;
        validate_hash(&genesis.journal_ref)?;
        validate_hash(&genesis.snapshot_root)?;
        validate_hash(&genesis.journal_root)?;
        validate_hash(&genesis.execution_state_root)?;
        validate_hash(&genesis.manifest_hash)?;
        let world = load_genesis_world(
            &genesis,
            store,
            initial_world.release_security_policy().clone(),
        )?;
        validate_world_scope(&world, config)?;
        let identity = world
            .execution_state_identity()
            .map_err(|error| format!("read BFT V2 genesis identity: {error:?}"))?;
        let manifest_hash = world
            .current_manifest_hash()
            .map_err(|error| format!("read BFT V2 genesis manifest: {error:?}"))?;
        if identity.state_root_hash != genesis.execution_state_root
            || identity.state_root_hash != config.genesis_state_root
            || manifest_hash != genesis.manifest_hash
            || manifest_hash != config.genesis_manifest_hash
        {
            return Err("BFT V2 genesis World differs from pinned root or manifest".into());
        }

        let mut ledger = Self {
            dir,
            _lock: lock,
            config: config.clone(),
            genesis,
            tip_height: 0,
            total_certificate_bytes: 0,
        };
        ledger.scan_records()?;
        ensure_required_marker(records_dir, ledger.dir.as_path())?;
        Ok((ledger, world))
    }

    pub(super) fn restore_genesis_world(
        &self,
        store: &LocalCasStore,
        policy: oasis7_runtime::ReleaseSecurityPolicy,
    ) -> Result<RuntimeWorld, String> {
        let world = load_genesis_world(&self.genesis, store, policy)?;
        validate_world_scope(&world, &self.config)?;
        let root = world
            .execution_state_identity()
            .map_err(|error| format!("read BFT V2 genesis identity: {error:?}"))?
            .state_root_hash;
        let manifest = world
            .current_manifest_hash()
            .map_err(|error| format!("read BFT V2 genesis manifest: {error:?}"))?;
        if root != self.config.genesis_state_root || manifest != self.config.genesis_manifest_hash {
            return Err("restored BFT V2 genesis differs from pinned root or manifest".into());
        }
        Ok(world)
    }

    pub(super) fn tip_height(&self) -> u64 {
        self.tip_height
    }

    pub(super) fn expected_parent(&self, height: u64) -> Result<String, String> {
        if height == 0 {
            return Err("BFT V2 parent request has zero height".into());
        }
        if height == 1 {
            return Ok(self.config.scope.genesis_hash.clone());
        }
        if height > self.tip_height.saturating_add(1) {
            return Err("BFT V2 parent request skips a durable height".into());
        }
        self.record(height - 1)?
            .map(|record| record.block_id)
            .ok_or_else(|| "BFT V2 predecessor record is missing".into())
    }

    pub(super) fn history_page(
        &self,
        after_height: u64,
        max_entries: usize,
    ) -> Result<Vec<BftCommitCertificateV2>, String> {
        if after_height > self.tip_height
            || max_entries == 0
            || max_entries > BFT_MAX_COMMIT_HISTORY_PAGE_ENTRIES_V1
        {
            return Err("BFT V2 history cursor or page bound is invalid".into());
        }
        let mut page = Vec::with_capacity(max_entries);
        let end = self
            .tip_height
            .min(after_height.saturating_add(max_entries as u64));
        for height in after_height.saturating_add(1)..=end {
            page.push(
                self.record(height)?
                    .ok_or("BFT V2 history has a missing height")?
                    .certificate,
            );
        }
        Ok(page)
    }

    pub(super) fn record(&self, height: u64) -> Result<Option<BftV2ExecutionRecord>, String> {
        if height == 0 || height > BFT_MAX_COMMIT_HISTORY_HEIGHTS_V1 as u64 {
            return Ok(None);
        }
        let path = self.record_path(height);
        let Some(record) = common_storage::read_optional_canonical::<BftV2ExecutionRecord>(
            path.as_path(),
            BFT_V2_MAX_RECORD_BYTES,
        )?
        else {
            return Ok(None);
        };
        File::open(path.as_path())
            .and_then(|file| file.sync_all())
            .map_err(|error| format!("sync observed BFT V2 record: {error}"))?;
        File::open(self.dir.as_path())
            .and_then(|directory| directory.sync_all())
            .map_err(|error| format!("sync BFT V2 ledger directory: {error}"))?;
        validate_record(&record, height, &self.config)?;
        Ok(Some(record))
    }

    pub(super) fn durable_install_digest(&self, height: u64) -> Result<String, String> {
        let record = self
            .record(height)?
            .ok_or("durable BFT V2 execution record is missing")?;
        let record_bytes = serde_cbor::to_vec(&record)
            .map_err(|error| format!("encode durable BFT V2 install receipt: {error}"))?;
        if record_bytes.len() > BFT_V2_MAX_RECORD_BYTES {
            return Err("durable BFT V2 install receipt exceeds record bound".into());
        }
        let pinned_config = BftV2PinnedConfig::from(&self.config);
        let receipt =
            serde_cbor::to_vec(&(BFT_V2_INSTALL_RECEIPT_DOMAIN, pinned_config, record_bytes))
                .map_err(|error| format!("encode durable BFT V2 install receipt: {error}"))?;
        Ok(blake3_hex(receipt.as_slice()))
    }

    pub(super) fn validate_record_material(
        &self,
        record: &BftV2ExecutionRecord,
        snapshot_bytes: &[u8],
        journal_bytes: &[u8],
        store: &LocalCasStore,
    ) -> Result<(), String> {
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
        if snapshot != snapshot_bytes || journal != journal_bytes {
            return Err("replayed BFT V2 World differs from exact committed CAS material".into());
        }
        Ok(())
    }

    pub(super) fn accept_existing_record(
        &mut self,
        record: &BftV2ExecutionRecord,
    ) -> Result<(), String> {
        if record.height <= self.tip_height {
            return Ok(());
        }
        let next = self
            .tip_height
            .checked_add(1)
            .ok_or("BFT V2 height overflow")?;
        if record.height != next
            || record.certificate.proposal.block.parent_commit_ref
                != self.expected_parent(record.height)?
        {
            return Err("observed BFT V2 record does not extend durable tip".into());
        }
        self.account_certificate(&record.certificate)?;
        self.tip_height = record.height;
        Ok(())
    }

    pub(super) fn commit_candidate(
        &mut self,
        candidate: BftV2CommitCandidate<'_>,
        store: &LocalCasStore,
    ) -> Result<BftV2ExecutionRecord, String> {
        let BftV2CommitCandidate {
            certificate,
            block_id,
            block,
            snapshot_bytes,
            journal_bytes,
            execution_block_hash,
            execution_state_root,
            state_root,
            event_root,
        } = candidate;
        let expected_height = self
            .tip_height
            .checked_add(1)
            .ok_or("BFT V2 height overflow")?;
        if block.height != expected_height
            || block.parent_commit_ref != self.expected_parent(block.height)?
            || block_id != block.block_id()?
        {
            return Err("BFT V2 durable commit does not extend exact current ledger head".into());
        }
        if self.tip_height >= BFT_MAX_COMMIT_HISTORY_HEIGHTS_V1 as u64 {
            return Err("BFT V2 commit history reached fixed lifetime capacity".into());
        }
        self.check_certificate_capacity(&certificate)?;

        let snapshot_ref = common_storage::put_cas_durable(
            store,
            snapshot_bytes,
            common_storage::MAX_BFT_SNAPSHOT_BYTES,
        )?;
        let journal_ref = common_storage::put_cas_durable(
            store,
            journal_bytes,
            common_storage::MAX_BFT_JOURNAL_BYTES,
        )?;
        store
            .replace_pin_scope_shard(
                BFT_V2_PIN_SCOPE,
                &format!("height-{:020}", block.height),
                &BTreeSet::from([snapshot_ref.clone(), journal_ref.clone()]),
            )
            .map_err(|error| format!("pin BFT V2 snapshot and journal: {error:?}"))?;
        let record = BftV2ExecutionRecord {
            schema_version: BFT_V2_LEDGER_SCHEMA,
            height: block.height,
            certificate,
            block_id,
            execution_block_hash: execution_block_hash.to_string(),
            execution_state_root: execution_state_root.to_string(),
            state_root: state_root.to_string(),
            event_root: event_root.to_string(),
            post_manifest_hash: block.post_manifest_hash.clone(),
            snapshot_ref,
            journal_ref,
        };
        validate_record(&record, block.height, &self.config)?;
        let bytes = serde_cbor::to_vec(&record)
            .map_err(|error| format!("encode BFT V2 record: {error}"))?;
        if bytes.len() > BFT_V2_MAX_RECORD_BYTES {
            return Err("BFT V2 durable record exceeds per-record byte bound".into());
        }
        common_storage::write_immutable_file(
            self.record_path(block.height).as_path(),
            bytes.as_slice(),
        )?;
        self.tip_height = block.height;
        self.account_certificate(&record.certificate)?;
        Ok(record)
    }

    fn account_certificate(&mut self, certificate: &BftCommitCertificateV2) -> Result<(), String> {
        let encoded = serde_cbor::to_vec(certificate).map_err(|error| error.to_string())?;
        if encoded.len() > BFT_MAX_SIGNED_MATERIAL_BYTES_V2
            || self
                .total_certificate_bytes
                .checked_add(encoded.len())
                .ok_or("BFT V2 history byte count overflow")?
                > BFT_MAX_COMMIT_HISTORY_CBOR_BYTES_V1
        {
            return Err("BFT V2 commit history exceeds encoded-byte capacity".into());
        }
        self.total_certificate_bytes += encoded.len();
        Ok(())
    }

    fn check_certificate_capacity(
        &self,
        certificate: &BftCommitCertificateV2,
    ) -> Result<(), String> {
        let encoded = serde_cbor::to_vec(certificate).map_err(|error| error.to_string())?;
        if encoded.len() > BFT_MAX_SIGNED_MATERIAL_BYTES_V2
            || self
                .total_certificate_bytes
                .checked_add(encoded.len())
                .ok_or("BFT V2 history byte count overflow")?
                > BFT_MAX_COMMIT_HISTORY_CBOR_BYTES_V1
        {
            return Err("BFT V2 commit history exceeds encoded-byte capacity".into());
        }
        Ok(())
    }

    fn record_path(&self, height: u64) -> PathBuf {
        self.dir.join(format!(
            "{BFT_V2_RECORD_PREFIX}{height:020}{BFT_V2_RECORD_SUFFIX}"
        ))
    }

    fn scan_records(&mut self) -> Result<(), String> {
        let mut heights = Vec::new();
        for entry in fs::read_dir(self.dir.as_path())
            .map_err(|error| format!("list BFT V2 ledger: {error}"))?
        {
            let entry = entry.map_err(|error| format!("read BFT V2 ledger entry: {error}"))?;
            let name = entry.file_name();
            let name = name.to_str().ok_or("BFT V2 entry name is not UTF-8")?;
            if name == BFT_V2_LOCK_FILE || name == BFT_V2_GENESIS_FILE {
                if !entry
                    .file_type()
                    .map_err(|error| error.to_string())?
                    .is_file()
                {
                    return Err("BFT V2 known ledger entry is not a regular file".into());
                }
                continue;
            }
            if let Some(height) = parse_record_name(name)? {
                if !entry
                    .file_type()
                    .map_err(|error| error.to_string())?
                    .is_file()
                {
                    return Err("BFT V2 history entry is not a regular file".into());
                }
                heights.push(height);
            } else {
                return Err(format!(
                    "unknown BFT V2 ledger entry is preserved and blocks startup: {name}"
                ));
            }
        }
        heights.sort_unstable();
        if heights.len() > BFT_MAX_COMMIT_HISTORY_HEIGHTS_V1
            || heights
                .iter()
                .enumerate()
                .any(|(index, height)| *height != index as u64 + 1)
        {
            return Err("BFT V2 history is non-contiguous or exceeds lifetime capacity".into());
        }
        let mut parent = self.config.scope.genesis_hash.clone();
        for height in heights {
            let record = common_storage::read_required_canonical::<BftV2ExecutionRecord>(
                self.record_path(height).as_path(),
                BFT_V2_MAX_RECORD_BYTES,
            )?;
            validate_record(&record, height, &self.config)?;
            if record.certificate.proposal.block.parent_commit_ref != parent {
                return Err("BFT V2 history predecessor reference is discontinuous".into());
            }
            parent.clone_from(&record.block_id);
            self.account_certificate(&record.certificate)?;
            self.tip_height = height;
        }
        Ok(())
    }
}

pub(super) fn has_bft_v2_material(records_dir: &Path) -> Result<bool, String> {
    BftV2ExecutionLedger::required_material_exists(records_dir)
}

fn initialize_genesis(
    records_dir: &Path,
    ledger_dir: &Path,
    config: &NodeBftExecutionConfigV2,
    world: &RuntimeWorld,
    store: &LocalCasStore,
) -> Result<BftV2Genesis, String> {
    validate_world_scope(world, config)?;
    let snapshot_bytes = super::super::super::super::snapshot_to_cbor(&world.snapshot())?;
    let journal_bytes = super::super::super::super::journal_to_cbor(world.journal())?;
    if snapshot_bytes.len() > common_storage::MAX_BFT_SNAPSHOT_BYTES
        || journal_bytes.len() > common_storage::MAX_BFT_JOURNAL_BYTES
    {
        return Err("BFT V2 genesis snapshot or journal exceeds its durable byte bound".into());
    }
    let snapshot_ref = common_storage::put_cas_durable(
        store,
        snapshot_bytes.as_slice(),
        common_storage::MAX_BFT_SNAPSHOT_BYTES,
    )?;
    let journal_ref = common_storage::put_cas_durable(
        store,
        journal_bytes.as_slice(),
        common_storage::MAX_BFT_JOURNAL_BYTES,
    )?;
    store
        .replace_pin_scope_shard(
            BFT_V2_PIN_SCOPE,
            "genesis",
            &BTreeSet::from([snapshot_ref.clone(), journal_ref.clone()]),
        )
        .map_err(|error| format!("pin BFT V2 genesis material: {error:?}"))?;
    let execution_state_root = world
        .execution_state_identity()
        .map_err(|error| format!("compute BFT V2 genesis root: {error:?}"))?
        .state_root_hash;
    let manifest_hash = world
        .current_manifest_hash()
        .map_err(|error| format!("compute BFT V2 genesis manifest: {error:?}"))?;
    if execution_state_root != config.genesis_state_root
        || manifest_hash != config.genesis_manifest_hash
    {
        return Err("BFT V2 config genesis root or manifest differs from World".into());
    }
    let genesis = BftV2Genesis {
        schema_version: BFT_V2_LEDGER_SCHEMA,
        config: BftV2PinnedConfig::from(config),
        snapshot_ref,
        journal_ref,
        snapshot_root: blake3_hex(snapshot_bytes.as_slice()),
        journal_root: blake3_hex(journal_bytes.as_slice()),
        execution_state_root,
        manifest_hash,
    };
    let bytes =
        serde_cbor::to_vec(&genesis).map_err(|error| format!("encode BFT V2 genesis: {error}"))?;
    common_storage::write_immutable_file(
        ledger_dir.join(BFT_V2_GENESIS_FILE).as_path(),
        bytes.as_slice(),
    )?;
    ensure_required_marker(records_dir, ledger_dir)?;
    Ok(genesis)
}

fn ensure_required_marker(records_dir: &Path, ledger_dir: &Path) -> Result<(), String> {
    let marker = records_dir.join(BFT_V2_REQUIRED_MARKER);
    match common_storage::read_optional_bytes(marker.as_path(), 128)? {
        Some(bytes) if bytes != BFT_V2_REQUIRED_BYTES => {
            Err("BFT V2 required marker is malformed; refusing fallback".into())
        }
        Some(_) => crate::controlled_live_config::require_private_owned(marker.as_path(), false),
        None if ledger_dir.join(BFT_V2_GENESIS_FILE).is_file() => {
            common_storage::write_immutable_file(marker.as_path(), BFT_V2_REQUIRED_BYTES)
        }
        None => Err("BFT V2 marker cannot precede its genesis record".into()),
    }
}

fn load_genesis_world(
    genesis: &BftV2Genesis,
    store: &LocalCasStore,
    policy: oasis7_runtime::ReleaseSecurityPolicy,
) -> Result<RuntimeWorld, String> {
    let snapshot_bytes = common_storage::read_cas_bounded(
        store,
        genesis.snapshot_ref.as_str(),
        common_storage::MAX_BFT_SNAPSHOT_BYTES,
    )?;
    let journal_bytes = common_storage::read_cas_bounded(
        store,
        genesis.journal_ref.as_str(),
        common_storage::MAX_BFT_JOURNAL_BYTES,
    )?;
    if blake3_hex(snapshot_bytes.as_slice()) != genesis.snapshot_root
        || blake3_hex(journal_bytes.as_slice()) != genesis.journal_root
    {
        return Err("BFT V2 genesis snapshot or journal digest mismatch".into());
    }
    let snapshot: oasis7_runtime::Snapshot = serde_cbor::from_slice(snapshot_bytes.as_slice())
        .map_err(|error| format!("decode BFT V2 genesis snapshot: {error}"))?;
    let journal: oasis7_runtime::Journal = serde_cbor::from_slice(journal_bytes.as_slice())
        .map_err(|error| format!("decode BFT V2 genesis journal: {error}"))?;
    RuntimeWorld::from_snapshot_with_release_security_policy(snapshot, journal, policy)
        .map_err(|error| format!("restore BFT V2 genesis World: {error:?}"))
}

fn validate_record(
    record: &BftV2ExecutionRecord,
    height: u64,
    config: &NodeBftExecutionConfigV2,
) -> Result<(), String> {
    if record.schema_version != BFT_V2_LEDGER_SCHEMA || record.height != height {
        return Err("BFT V2 execution record schema or height mismatch".into());
    }
    record.certificate.validate_wire_bounds()?;
    let block = &record.certificate.proposal.block;
    if block.height != height
        || block.scope != config.scope
        || block.authority_epoch != config.authority_epoch
        || block.execution_block_hash != record.execution_block_hash
        || block.execution_state_root != record.execution_state_root
        || block.state_root != record.state_root
        || block.event_root != record.event_root
        || block.post_manifest_hash != record.post_manifest_hash
        || block.block_id()? != record.block_id
    {
        return Err("BFT V2 execution record differs from its signed block".into());
    }
    for value in [
        record.block_id.as_str(),
        record.execution_block_hash.as_str(),
        record.execution_state_root.as_str(),
        record.state_root.as_str(),
        record.event_root.as_str(),
        record.post_manifest_hash.as_str(),
    ] {
        validate_hash(value)?;
    }
    validate_hash(&record.snapshot_ref)?;
    validate_hash(&record.journal_ref)
}

fn validate_world_scope(
    world: &RuntimeWorld,
    config: &NodeBftExecutionConfigV2,
) -> Result<(), String> {
    config.scope.validate()?;
    let manifest = world.chain_resource_manifest();
    if manifest.world_id != config.scope.world_id
        || manifest.chain_id != config.scope.chain_id
        || manifest.genesis_ref.as_deref() != Some(config.scope.genesis_hash.as_str())
    {
        return Err("BFT V2 scope differs from authenticated World identity".into());
    }
    Ok(())
}

fn validate_hash(value: &str) -> Result<(), String> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err("BFT V2 storage digest must be canonical 32-byte hex".into());
    }
    Ok(())
}

fn parse_record_name(name: &str) -> Result<Option<u64>, String> {
    let Some(number) = name
        .strip_prefix(BFT_V2_RECORD_PREFIX)
        .and_then(|value| value.strip_suffix(BFT_V2_RECORD_SUFFIX))
    else {
        return Ok(None);
    };
    if number.len() != 20 || !number.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(format!(
            "malformed BFT V2 history filename is preserved: {name}"
        ));
    }
    let height = number
        .parse::<u64>()
        .map_err(|error| format!("parse BFT V2 history height: {error}"))?;
    if height == 0 {
        return Err("BFT V2 history filename has zero height".into());
    }
    Ok(Some(height))
}
