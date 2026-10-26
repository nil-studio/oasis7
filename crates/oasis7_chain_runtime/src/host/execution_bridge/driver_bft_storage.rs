use super::*;
use std::collections::BTreeSet;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

const BFT_LEDGER_DIR: &str = "bft-execution-v1";
const BFT_REQUIRED_MARKER: &str = "bft-execution-required-v1";
const BFT_REQUIRED_BYTES: &[u8] = b"oasis7.bft.execution.required/v1\n";
const BFT_LOCK_FILE: &str = "writer.lock";
const BFT_GENESIS_FILE: &str = "genesis.cbor";
const BFT_RECORD_PREFIX: &str = "height-";
const BFT_RECORD_SUFFIX: &str = ".cbor";
const BFT_LEDGER_SCHEMA_V1: u32 = 1;
pub(super) const MAX_BFT_SNAPSHOT_BYTES: usize = 512 * 1024 * 1024;
pub(super) const MAX_BFT_JOURNAL_BYTES: usize = 512 * 1024 * 1024;
const MAX_BFT_RECORD_BYTES: usize = 4 * 1024 * 1024;
const BFT_PIN_SCOPE: &str = "fixed-bft-execution-v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BftPinnedConfigV1 {
    scope: oasis7_node::BftScopeV1,
    authority_epoch: u64,
    validators: Vec<oasis7_node::BftValidatorV1>,
    validator_set_hash: String,
}

impl From<&NodeBftExecutionConfigV1> for BftPinnedConfigV1 {
    fn from(config: &NodeBftExecutionConfigV1) -> Self {
        Self {
            scope: config.scope.clone(),
            authority_epoch: config.authority_epoch,
            validators: config.validators.clone(),
            validator_set_hash: config.validator_set_hash.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BftGenesisV1 {
    schema_version: u32,
    config: BftPinnedConfigV1,
    snapshot_ref: String,
    journal_ref: String,
    snapshot_root: String,
    journal_root: String,
    execution_state_root: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct BftExecutionRecordV1 {
    schema_version: u32,
    pub(super) height: u64,
    pub(super) certificate: BftCommitCertificateV1,
    pub(super) block_id: String,
    pub(super) execution_block_hash: String,
    pub(super) execution_state_root: String,
    pub(super) state_root: String,
    event_root: String,
    pub(super) snapshot_ref: String,
    pub(super) journal_ref: String,
}

pub(super) struct BftExecutionLedger {
    dir: PathBuf,
    _lock: File,
    config: NodeBftExecutionConfigV1,
    genesis: BftGenesisV1,
    tip_height: u64,
    tip_block_id: Option<String>,
    total_certificate_bytes: usize,
}

impl BftExecutionLedger {
    pub(super) fn required_material_exists(records_dir: &Path) -> Result<bool, String> {
        let marker = records_dir.join(BFT_REQUIRED_MARKER);
        let ledger = records_dir.join(BFT_LEDGER_DIR);
        for path in [&marker, &ledger] {
            match fs::symlink_metadata(path) {
                Ok(metadata) if metadata.file_type().is_symlink() => {
                    return Err(format!(
                        "BFT execution marker path is a symlink: {}",
                        path.display()
                    ));
                }
                Ok(_) => return Ok(true),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(format!("inspect BFT execution marker: {error}")),
            }
        }
        Ok(false)
    }

    pub(super) fn open_or_initialize(
        records_dir: &Path,
        config: &NodeBftExecutionConfigV1,
        initial_world: &RuntimeWorld,
        store: &LocalCasStore,
        allow_initialize: bool,
    ) -> Result<(Self, RuntimeWorld), String> {
        if !cfg!(unix) {
            return Err("durable BFT execution is unsupported on this platform".into());
        }
        config.validate()?;
        let dir = records_dir.join(BFT_LEDGER_DIR);
        let existed = match fs::symlink_metadata(dir.as_path()) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
                return Err("BFT execution directory is not a real directory".into());
            }
            Ok(_) => true,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
            Err(error) => return Err(format!("inspect BFT execution directory: {error}")),
        };
        if !existed && !allow_initialize {
            return Err(
                "BFT execution directory is missing; refusing to recreate pinned state".into(),
            );
        }
        super::super::super::controlled_live::private_directory(dir.as_path())?;
        let lock_path = dir.join(BFT_LOCK_FILE);
        let mut lock_options = OpenOptions::new();
        lock_options
            .read(true)
            .write(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
        if existed {
            if !lock_path.is_file() {
                return Err("BFT execution writer lock is missing; refusing to recreate it".into());
            }
        } else {
            lock_options.create_new(true);
        }
        let lock = lock_options
            .open(lock_path.as_path())
            .map_err(|error| format!("open BFT execution lock: {error}"))?;
        crate::controlled_live_config::require_private_owned(lock_path.as_path(), false)?;
        if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
            return Err("BFT execution ledger is already open by another writer".into());
        }

        let genesis_path = dir.join(BFT_GENESIS_FILE);
        let genesis =
            match read_optional_canonical::<BftGenesisV1>(genesis_path.as_path(), 2 * 1024 * 1024)?
            {
                Some(genesis) => genesis,
                None if allow_initialize && !existed => {
                    initialize_genesis(records_dir, dir.as_path(), config, initial_world, store)?
                }
                None => {
                    return Err(
                        "BFT execution ledger is incomplete; refusing to reset its genesis".into(),
                    );
                }
            };
        if genesis.schema_version != BFT_LEDGER_SCHEMA_V1
            || genesis.config != BftPinnedConfigV1::from(config)
        {
            return Err("BFT execution genesis is bound to a different config or schema".into());
        }
        validate_blob_ref(&genesis.snapshot_ref)?;
        validate_blob_ref(&genesis.journal_ref)?;
        validate_hash(&genesis.snapshot_root)?;
        validate_hash(&genesis.journal_root)?;
        validate_hash(&genesis.execution_state_root)?;
        let world = load_genesis_world(
            &genesis,
            store,
            initial_world.release_security_policy().clone(),
        )?;
        validate_world_scope(&world, config)?;
        if world
            .execution_state_identity()
            .map_err(|e| format!("read BFT genesis identity: {e:?}"))?
            .state_root_hash
            != genesis.execution_state_root
        {
            return Err("BFT genesis execution state root differs from the pinned snapshot".into());
        }

        let mut ledger = Self {
            dir,
            _lock: lock,
            config: config.clone(),
            genesis,
            tip_height: 0,
            tip_block_id: None,
            total_certificate_bytes: 0,
        };
        ledger.scan_records()?;
        ensure_required_marker(records_dir, ledger.dir.as_path())?;
        Ok((ledger, world))
    }

    pub(super) fn head(&self) -> Option<NodeBftExecutionHeadV1> {
        self.tip_block_id
            .as_ref()
            .map(|block_id| NodeBftExecutionHeadV1 {
                height: self.tip_height,
                block_id: block_id.clone(),
            })
    }

    pub(super) fn restore_genesis_world(
        &self,
        store: &LocalCasStore,
        policy: oasis7_runtime::ReleaseSecurityPolicy,
    ) -> Result<RuntimeWorld, String> {
        let world = load_genesis_world(&self.genesis, store, policy)?;
        super::validate_world_scope(&world, &self.config)?;
        if world
            .execution_state_identity()
            .map_err(|error| format!("read BFT genesis execution identity: {error:?}"))?
            .state_root_hash
            != self.genesis.execution_state_root
        {
            return Err("BFT genesis execution state root differs from the pinned snapshot".into());
        }
        Ok(world)
    }

    pub(super) fn tip_height(&self) -> u64 {
        self.tip_height
    }

    pub(super) fn expected_parent(&self, height: u64) -> Result<String, String> {
        if height == 1 {
            return Ok(self.config.scope.genesis_hash.clone());
        }
        if height > self.tip_height.saturating_add(1) {
            return Err("BFT parent request skips a durable height".into());
        }
        let predecessor = self
            .record(height - 1)?
            .ok_or("BFT predecessor record is missing")?;
        Ok(predecessor.block_id)
    }

    pub(super) fn history_page(
        &self,
        after_height: u64,
        max_entries: usize,
    ) -> Result<Vec<BftCommitCertificateV1>, String> {
        if after_height > self.tip_height {
            return Err("BFT history cursor is beyond the durable tip".into());
        }
        let mut page = Vec::with_capacity(max_entries.min(BFT_MAX_COMMIT_HISTORY_PAGE_ENTRIES_V1));
        let end = self
            .tip_height
            .min(after_height.saturating_add(max_entries as u64));
        for height in after_height.saturating_add(1)..=end {
            let record = self
                .record(height)?
                .ok_or("BFT history has a missing height")?;
            page.push(record.certificate);
        }
        Ok(page)
    }

    pub(super) fn record(&self, height: u64) -> Result<Option<BftExecutionRecordV1>, String> {
        if height == 0 || height as usize > BFT_MAX_COMMIT_HISTORY_HEIGHTS_V1 {
            return Ok(None);
        }
        let path = self.record_path(height);
        let Some(record) =
            read_optional_canonical::<BftExecutionRecordV1>(path.as_path(), MAX_BFT_RECORD_BYTES)?
        else {
            return Ok(None);
        };
        File::open(path.as_path())
            .and_then(|file| file.sync_all())
            .map_err(|e| format!("sync observed BFT record: {e}"))?;
        File::open(self.dir.as_path())
            .and_then(|directory| directory.sync_all())
            .map_err(|e| format!("sync BFT ledger directory: {e}"))?;
        validate_record(&record, height, &self.config)?;
        Ok(Some(record))
    }

    pub(super) fn validate_record_material(
        &self,
        record: &BftExecutionRecordV1,
        executed: &ExecutedCandidate,
        store: &LocalCasStore,
    ) -> Result<(), String> {
        let snapshot =
            read_cas_bounded(store, record.snapshot_ref.as_str(), MAX_BFT_SNAPSHOT_BYTES)?;
        let journal = read_cas_bounded(store, record.journal_ref.as_str(), MAX_BFT_JOURNAL_BYTES)?;
        if snapshot != executed.snapshot_bytes || journal != executed.journal_bytes {
            return Err(
                "replayed BFT World does not exactly match the committed CAS material".into(),
            );
        }
        Ok(())
    }

    pub(super) fn accept_existing_record(
        &mut self,
        record: &BftExecutionRecordV1,
    ) -> Result<(), String> {
        if record.height <= self.tip_height {
            return Ok(());
        }
        let expected_height = self
            .tip_height
            .checked_add(1)
            .ok_or("BFT height overflow")?;
        if record.height != expected_height
            || record.certificate.proposal.block.parent_commit_ref
                != self.expected_parent(record.height)?
        {
            return Err(
                "observed BFT record does not extend the current in-memory ledger tip".into(),
            );
        }
        let encoded = serde_cbor::to_vec(&record.certificate).map_err(|e| e.to_string())?;
        self.total_certificate_bytes = self
            .total_certificate_bytes
            .checked_add(encoded.len())
            .ok_or("BFT history byte count overflow")?;
        if self.total_certificate_bytes > MAX_BFT_HISTORY_BYTES {
            return Err("BFT history exceeds lifetime encoded-byte capacity".into());
        }
        self.tip_height = record.height;
        self.tip_block_id = Some(record.block_id.clone());
        Ok(())
    }

    pub(super) fn commit_candidate(
        &mut self,
        certificate: BftCommitCertificateV1,
        block_id: String,
        block: &BftBlockV1,
        executed: &ExecutedCandidate,
        store: &LocalCasStore,
    ) -> Result<BftExecutionRecordV1, String> {
        let expected_height = self
            .tip_height
            .checked_add(1)
            .ok_or("BFT height overflow")?;
        if block.height != expected_height
            || block.parent_commit_ref != self.expected_parent(block.height)?
            || block_id != block.block_id()?
        {
            return Err("BFT durable commit does not extend the exact current ledger head".into());
        }
        if self.tip_height as usize >= BFT_MAX_COMMIT_HISTORY_HEIGHTS_V1 {
            return Err("BFT commit history reached its fixed lifetime capacity".into());
        }
        let certificate_bytes = serde_cbor::to_vec(&certificate).map_err(|e| e.to_string())?;
        if certificate_bytes.len() > oasis7_proto::distributed_bft::BFT_MAX_SIGNED_MATERIAL_BYTES_V1
            || self
                .total_certificate_bytes
                .saturating_add(certificate_bytes.len())
                > MAX_BFT_HISTORY_BYTES
        {
            return Err("BFT commit history reached its byte capacity".into());
        }
        let snapshot_ref = put_cas_durable(
            store,
            executed.snapshot_bytes.as_slice(),
            MAX_BFT_SNAPSHOT_BYTES,
        )?;
        let journal_ref = put_cas_durable(
            store,
            executed.journal_bytes.as_slice(),
            MAX_BFT_JOURNAL_BYTES,
        )?;
        store
            .replace_pin_scope_shard(
                BFT_PIN_SCOPE,
                &format!("height-{:020}", block.height),
                &BTreeSet::from([snapshot_ref.clone(), journal_ref.clone()]),
            )
            .map_err(|e| format!("pin BFT committed snapshot/journal: {e:?}"))?;
        let record = BftExecutionRecordV1 {
            schema_version: BFT_LEDGER_SCHEMA_V1,
            height: block.height,
            certificate,
            block_id,
            execution_block_hash: executed.execution_block_hash.clone(),
            execution_state_root: executed.execution_state_root.clone(),
            state_root: executed.state_root.clone(),
            event_root: executed.event_root.clone(),
            snapshot_ref,
            journal_ref,
        };
        validate_record(&record, block.height, &self.config)?;
        let bytes = serde_cbor::to_vec(&record).map_err(|e| format!("encode BFT record: {e}"))?;
        if bytes.len() > MAX_BFT_RECORD_BYTES {
            return Err("BFT durable record exceeds the per-record byte bound".into());
        }
        write_immutable_file(self.record_path(block.height).as_path(), bytes.as_slice())?;
        self.tip_height = block.height;
        self.tip_block_id = Some(record.block_id.clone());
        self.total_certificate_bytes = self
            .total_certificate_bytes
            .saturating_add(certificate_bytes.len());
        Ok(record)
    }

    fn record_path(&self, height: u64) -> PathBuf {
        self.dir.join(format!(
            "{BFT_RECORD_PREFIX}{height:020}{BFT_RECORD_SUFFIX}"
        ))
    }

    fn scan_records(&mut self) -> Result<(), String> {
        let mut heights = Vec::new();
        for entry in
            fs::read_dir(self.dir.as_path()).map_err(|e| format!("list BFT ledger: {e}"))?
        {
            let entry = entry.map_err(|e| format!("read BFT ledger entry: {e}"))?;
            let name = entry.file_name();
            let name = name.to_str().ok_or("BFT ledger entry name is not UTF-8")?;
            if name == BFT_LOCK_FILE || name == BFT_GENESIS_FILE {
                if !entry.file_type().map_err(|e| e.to_string())?.is_file() {
                    return Err("BFT ledger known entry is not a regular file".into());
                }
                continue;
            }
            if let Some(height) = parse_record_name(name)? {
                if !entry.file_type().map_err(|e| e.to_string())?.is_file() {
                    return Err("BFT history entry is not a regular file".into());
                }
                heights.push(height);
            } else {
                return Err(format!(
                    "unknown BFT ledger entry is preserved and blocks startup: {name}"
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
            return Err("BFT history is non-contiguous or exceeds lifetime capacity".into());
        }
        let mut parent = self.config.scope.genesis_hash.clone();
        for height in heights {
            let record = read_required_canonical::<BftExecutionRecordV1>(
                self.record_path(height).as_path(),
                MAX_BFT_RECORD_BYTES,
            )?;
            validate_record(&record, height, &self.config)?;
            let block = &record.certificate.proposal.block;
            if block.parent_commit_ref != parent {
                return Err("BFT history predecessor reference is discontinuous".into());
            }
            parent = record.block_id.clone();
            let bytes = serde_cbor::to_vec(&record.certificate).map_err(|e| e.to_string())?;
            self.total_certificate_bytes = self
                .total_certificate_bytes
                .checked_add(bytes.len())
                .ok_or("BFT history byte count overflow")?;
            if self.total_certificate_bytes > MAX_BFT_HISTORY_BYTES {
                return Err("BFT history exceeds lifetime encoded-byte capacity".into());
            }
            self.tip_height = height;
            self.tip_block_id = Some(parent.clone());
        }
        Ok(())
    }
}

pub(super) fn has_bft_material(records_dir: &Path) -> Result<bool, String> {
    BftExecutionLedger::required_material_exists(records_dir)
}

fn initialize_genesis(
    records_dir: &Path,
    ledger_dir: &Path,
    config: &NodeBftExecutionConfigV1,
    world: &RuntimeWorld,
    store: &LocalCasStore,
) -> Result<BftGenesisV1, String> {
    validate_world_scope(world, config)?;
    let snapshot_bytes = super::super::super::snapshot_to_cbor(&world.snapshot())?;
    let journal_bytes = super::super::super::journal_to_cbor(world.journal())?;
    if snapshot_bytes.len() > MAX_BFT_SNAPSHOT_BYTES || journal_bytes.len() > MAX_BFT_JOURNAL_BYTES
    {
        return Err("BFT genesis snapshot or journal exceeds its durable byte bound".into());
    }
    let snapshot_ref = put_cas_durable(store, snapshot_bytes.as_slice(), MAX_BFT_SNAPSHOT_BYTES)?;
    let journal_ref = put_cas_durable(store, journal_bytes.as_slice(), MAX_BFT_JOURNAL_BYTES)?;
    store
        .replace_pin_scope_shard(
            BFT_PIN_SCOPE,
            "genesis",
            &BTreeSet::from([snapshot_ref.clone(), journal_ref.clone()]),
        )
        .map_err(|e| format!("pin BFT genesis snapshot/journal: {e:?}"))?;
    let genesis = BftGenesisV1 {
        schema_version: BFT_LEDGER_SCHEMA_V1,
        config: BftPinnedConfigV1::from(config),
        snapshot_root: blake3_hex(snapshot_bytes.as_slice()),
        journal_root: blake3_hex(journal_bytes.as_slice()),
        execution_state_root: world
            .execution_state_identity()
            .map_err(|e| format!("compute BFT genesis state root: {e:?}"))?
            .state_root_hash,
        snapshot_ref,
        journal_ref,
    };
    let bytes = serde_cbor::to_vec(&genesis).map_err(|e| format!("encode BFT genesis: {e}"))?;
    write_immutable_file(
        ledger_dir.join(BFT_GENESIS_FILE).as_path(),
        bytes.as_slice(),
    )?;
    ensure_required_marker(records_dir, ledger_dir)?;
    Ok(genesis)
}

fn ensure_required_marker(records_dir: &Path, ledger_dir: &Path) -> Result<(), String> {
    let marker = records_dir.join(BFT_REQUIRED_MARKER);
    if let Some(bytes) = read_optional_bytes(marker.as_path(), 128)? {
        if bytes != BFT_REQUIRED_BYTES {
            return Err("BFT required marker is malformed; refusing legacy fallback".into());
        }
        crate::controlled_live_config::require_private_owned(marker.as_path(), false)?;
        return Ok(());
    }
    if !ledger_dir.join(BFT_GENESIS_FILE).is_file() {
        return Err("BFT mode marker cannot be published before its genesis record".into());
    }
    write_immutable_file(marker.as_path(), BFT_REQUIRED_BYTES)
}

fn load_genesis_world(
    genesis: &BftGenesisV1,
    store: &LocalCasStore,
    policy: oasis7_runtime::ReleaseSecurityPolicy,
) -> Result<RuntimeWorld, String> {
    let snapshot_bytes =
        read_cas_bounded(store, genesis.snapshot_ref.as_str(), MAX_BFT_SNAPSHOT_BYTES)?;
    let journal_bytes =
        read_cas_bounded(store, genesis.journal_ref.as_str(), MAX_BFT_JOURNAL_BYTES)?;
    if blake3_hex(snapshot_bytes.as_slice()) != genesis.snapshot_root
        || blake3_hex(journal_bytes.as_slice()) != genesis.journal_root
    {
        return Err("BFT genesis snapshot or journal digest mismatch".into());
    }
    let snapshot: oasis7_runtime::Snapshot = serde_cbor::from_slice(snapshot_bytes.as_slice())
        .map_err(|e| format!("decode BFT genesis snapshot: {e}"))?;
    let journal: oasis7_runtime::Journal = serde_cbor::from_slice(journal_bytes.as_slice())
        .map_err(|e| format!("decode BFT genesis journal: {e}"))?;
    RuntimeWorld::from_snapshot_with_release_security_policy(snapshot, journal, policy)
        .map_err(|e| format!("restore BFT genesis World: {e:?}"))
}

fn validate_record(
    record: &BftExecutionRecordV1,
    height: u64,
    config: &NodeBftExecutionConfigV1,
) -> Result<(), String> {
    if record.schema_version != BFT_LEDGER_SCHEMA_V1 || record.height != height {
        return Err("BFT execution record schema or height mismatch".into());
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
        || block.block_id()? != record.block_id
    {
        return Err("BFT execution record differs from its signed block".into());
    }
    for value in [
        record.block_id.as_str(),
        record.execution_block_hash.as_str(),
        record.execution_state_root.as_str(),
        record.state_root.as_str(),
        record.event_root.as_str(),
    ] {
        validate_hash(value)?;
    }
    validate_blob_ref(&record.snapshot_ref)?;
    validate_blob_ref(&record.journal_ref)
}

fn validate_blob_ref(value: &str) -> Result<(), String> {
    validate_hash(value)
}

fn validate_hash(value: &str) -> Result<(), String> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("BFT storage reference must be a 32-byte hex digest".into());
    }
    Ok(())
}

pub(super) fn put_cas_durable(
    store: &LocalCasStore,
    bytes: &[u8],
    maximum: usize,
) -> Result<String, String> {
    if bytes.is_empty() || bytes.len() > maximum {
        return Err("BFT CAS object is empty or exceeds the byte bound".into());
    }
    let reference = store
        .put_bytes(bytes)
        .map_err(|e| format!("persist BFT CAS object: {e:?}"))?;
    if reference != blake3_hex(bytes) {
        return Err("BFT CAS returned a noncanonical digest".into());
    }
    let path = store.blobs_dir().join(format!("{reference}.blob"));
    File::open(path.as_path())
        .and_then(|file| file.sync_all())
        .map_err(|e| format!("sync BFT CAS object: {e}"))?;
    File::open(store.blobs_dir())
        .and_then(|file| file.sync_all())
        .map_err(|e| format!("sync BFT CAS directory: {e}"))?;
    Ok(reference)
}

pub(super) fn read_cas_bounded(
    store: &LocalCasStore,
    reference: &str,
    maximum: usize,
) -> Result<Vec<u8>, String> {
    validate_blob_ref(reference)?;
    let mut bytes = Vec::new();
    let mut offset = 0_u64;
    loop {
        let remaining = maximum.saturating_add(1).saturating_sub(bytes.len());
        if remaining == 0 {
            return Err("BFT CAS object exceeds the byte bound".into());
        }
        let (chunk, done) = store
            .get_range(reference, offset, remaining.min(64 * 1024))
            .map_err(|e| format!("read BFT CAS object: {e:?}"))?;
        if chunk.is_empty() && !done {
            return Err("BFT CAS bounded read made no progress".into());
        }
        offset = offset
            .checked_add(chunk.len() as u64)
            .ok_or("BFT CAS offset overflow")?;
        bytes.extend_from_slice(chunk.as_slice());
        if bytes.len() > maximum {
            return Err("BFT CAS object exceeds the byte bound".into());
        }
        if done {
            break;
        }
    }
    if bytes.is_empty() || blake3_hex(bytes.as_slice()) != reference {
        return Err("BFT CAS object digest mismatch or empty content".into());
    }
    Ok(bytes)
}

pub(super) fn read_optional_canonical<T: for<'de> Deserialize<'de> + Serialize>(
    path: &Path,
    maximum: usize,
) -> Result<Option<T>, String> {
    let Some(bytes) = read_optional_bytes(path, maximum)? else {
        return Ok(None);
    };
    let value: T = serde_cbor::from_slice(bytes.as_slice())
        .map_err(|e| format!("decode {}: {e}", path.display()))?;
    let canonical =
        serde_cbor::to_vec(&value).map_err(|e| format!("encode {}: {e}", path.display()))?;
    if canonical != bytes {
        return Err(format!("{} is not canonical CBOR", path.display()));
    }
    crate::controlled_live_config::require_private_owned(path, false)?;
    Ok(Some(value))
}

pub(super) fn read_required_canonical<T: for<'de> Deserialize<'de> + Serialize>(
    path: &Path,
    maximum: usize,
) -> Result<T, String> {
    read_optional_canonical(path, maximum)?
        .ok_or_else(|| format!("required BFT record is missing: {}", path.display()))
}

pub(super) fn read_optional_bytes(path: &Path, maximum: usize) -> Result<Option<Vec<u8>>, String> {
    let mut options = OpenOptions::new();
    options
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    let mut file = match options.open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("open {}: {error}", path.display())),
    };
    let metadata = file
        .metadata()
        .map_err(|error| format!("inspect opened BFT file {}: {error}", path.display()))?;
    if !metadata.is_file() || metadata.len() > maximum as u64 {
        return Err(format!(
            "BFT file is not regular or exceeds bound: {}",
            path.display()
        ));
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    (&mut file)
        .take(maximum.saturating_add(1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("read {}: {e}", path.display()))?;
    if bytes.len() > maximum {
        return Err(format!("BFT file exceeds byte bound: {}", path.display()));
    }
    Ok(Some(bytes))
}

pub(super) fn write_immutable_file(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path.parent().ok_or("BFT file path has no parent")?;
    let mut options = OpenOptions::new();
    options
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    let mut file = options
        .open(path)
        .map_err(|e| format!("create immutable BFT file {}: {e}", path.display()))?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|e| format!("write immutable BFT file {}: {e}", path.display()))?;
    File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|e| format!("sync BFT directory {}: {e}", parent.display()))
}

fn parse_record_name(name: &str) -> Result<Option<u64>, String> {
    let Some(number) = name
        .strip_prefix(BFT_RECORD_PREFIX)
        .and_then(|v| v.strip_suffix(BFT_RECORD_SUFFIX))
    else {
        return Ok(None);
    };
    if number.len() != 20 || !number.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(format!(
            "malformed BFT history filename is preserved: {name}"
        ));
    }
    let height = number
        .parse::<u64>()
        .map_err(|e| format!("parse BFT history height: {e}"))?;
    if height == 0 {
        return Err("BFT history filename has zero height".into());
    }
    Ok(Some(height))
}
