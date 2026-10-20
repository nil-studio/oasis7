use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use oasis7_node::{
    NodeExecutionCheckpointBundle, NodeExecutionCheckpointInstallContext, NodeExecutionCommitResult,
};
use oasis7_runtime::{
    BlobStore, ChainResourceDerivationContext, Journal as RuntimeJournal,
    Snapshot as RuntimeSnapshot, World as RuntimeWorld, blake3_hex,
};
use serde::{Deserialize, Serialize};

use super::checkpoint::{
    begin_execution_bridge_retention_transaction, complete_execution_bridge_retention_transaction,
    execution_bridge_record_path, execution_checkpoint_latest_path,
    execution_checkpoint_manifest_path, execution_checkpoint_manifest_rel_path,
    fail_execution_bridge_retention_transaction, run_execution_bridge_retention_maintenance,
};
use super::driver::NodeRuntimeExecutionDriver;
use super::driver_observability::{
    CheckpointInstallObservation, emit_checkpoint_bundle_install_complete,
    emit_checkpoint_bundle_install_start,
};
use super::driver_persistence::persist_execution_world_with_chain_resource_context;
use super::durable_transaction::{
    remove_dir_all_durable, remove_file_durable, rename_durable, sync_tree, unique_token,
    write_file_durable,
};
use super::{
    EXECUTION_BRIDGE_RECORD_SCHEMA_V3, EXECUTION_CHECKPOINT_MANIFEST_SCHEMA_V1,
    EXECUTION_CHECKPOINT_MANIFEST_SCHEMA_V2, ExecutionBridgeRecord,
    ExecutionCheckpointLatestPointer, ExecutionCheckpointManifest,
};

const CHECKPOINT_INSTALL_TRANSACTION_FILE: &str = "checkpoint-install-transaction.json";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
enum PublicationArtifact {
    State,
    Record,
    LatestRecord,
    Manifest,
    LatestManifest,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FileBackup {
    artifact: PublicationArtifact,
    bytes: Option<Vec<u8>>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
enum CheckpointInstallTransactionPhase {
    Prepared,
    Committed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct CheckpointInstallTransaction {
    phase: CheckpointInstallTransactionPhase,
    transaction_id: String,
    height: u64,
    previous_state: super::ExecutionBridgeState,
    world_was_present: bool,
    backups: Vec<FileBackup>,
}

impl CheckpointInstallTransaction {
    fn path(records_dir: &Path) -> PathBuf {
        records_dir.join(CHECKPOINT_INSTALL_TRANSACTION_FILE)
    }

    fn prepare(driver: &NodeRuntimeExecutionDriver, height: u64) -> Result<Self, String> {
        let world_was_present = world_dir_exists_as_directory(driver.world_dir.as_path())?;
        let mut backups = Vec::new();
        for artifact in [
            PublicationArtifact::State,
            PublicationArtifact::Record,
            PublicationArtifact::LatestRecord,
            PublicationArtifact::Manifest,
            PublicationArtifact::LatestManifest,
        ] {
            let path = artifact_path(driver, height, artifact);
            backups.push(FileBackup {
                artifact,
                bytes: capture_file(path.as_path())?,
            });
        }
        Ok(Self {
            phase: CheckpointInstallTransactionPhase::Prepared,
            transaction_id: unique_token(),
            height,
            previous_state: driver.state.clone(),
            world_was_present,
            backups,
        })
    }

    fn validate(&self) -> Result<(), String> {
        if self.transaction_id.is_empty()
            || !self
                .transaction_id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        {
            return Err("checkpoint install transaction has invalid transaction id".to_string());
        }
        let artifacts: BTreeSet<_> = self.backups.iter().map(|backup| backup.artifact).collect();
        if artifacts.len() != 5 || self.backups.len() != 5 {
            return Err(
                "checkpoint install transaction has invalid publication backups".to_string(),
            );
        }
        Ok(())
    }

    fn persist(&self, records_dir: &Path) -> Result<(), String> {
        self.validate()?;
        let bytes = serde_json::to_vec_pretty(self)
            .map_err(|err| format!("serialize checkpoint install transaction failed: {err}"))?;
        write_file_durable(Self::path(records_dir).as_path(), bytes.as_slice())
    }

    fn staging_world_path(&self, driver: &NodeRuntimeExecutionDriver) -> Result<PathBuf, String> {
        world_transaction_path(
            driver.world_dir.as_path(),
            self.transaction_id.as_str(),
            "stage",
        )
    }

    fn backup_world_path(&self, driver: &NodeRuntimeExecutionDriver) -> Result<PathBuf, String> {
        world_transaction_path(
            driver.world_dir.as_path(),
            self.transaction_id.as_str(),
            "backup",
        )
    }

    fn publish_world(
        &self,
        driver: &NodeRuntimeExecutionDriver,
        world: &RuntimeWorld,
        context: ChainResourceDerivationContext<'_>,
        world_config_hash: &str,
        generation_algorithm_hash: &str,
    ) -> Result<(), String> {
        let staging = self.staging_world_path(driver)?;
        let backup = self.backup_world_path(driver)?;
        if staging.exists() || backup.exists() {
            return Err(
                "checkpoint install transaction world staging path already exists".to_string(),
            );
        }
        persist_execution_world_with_chain_resource_context(
            staging.as_path(),
            world,
            context,
            world_config_hash,
            generation_algorithm_hash,
        )?;
        sync_tree(staging.as_path())?;
        if self.world_was_present {
            rename_durable(driver.world_dir.as_path(), backup.as_path())?;
        }
        rename_durable(staging.as_path(), driver.world_dir.as_path())
    }

    fn restore(&self, driver: &NodeRuntimeExecutionDriver) -> Result<(), String> {
        self.validate()?;
        self.restore_world(driver)?;
        for artifact in [
            PublicationArtifact::State,
            PublicationArtifact::Record,
            PublicationArtifact::Manifest,
            PublicationArtifact::LatestRecord,
            PublicationArtifact::LatestManifest,
        ] {
            let backup = self
                .backups
                .iter()
                .find(|backup| backup.artifact == artifact)
                .ok_or_else(|| "checkpoint install transaction backup is missing".to_string())?;
            restore_file(
                artifact_path(driver, self.height, artifact).as_path(),
                backup.bytes.as_deref(),
            )?;
        }
        Ok(())
    }

    fn restore_world(&self, driver: &NodeRuntimeExecutionDriver) -> Result<(), String> {
        let staging = self.staging_world_path(driver)?;
        let backup = self.backup_world_path(driver)?;
        if staging.exists() {
            require_directory(staging.as_path())?;
            remove_dir_all_durable(staging.as_path())?;
        }
        if backup.exists() {
            require_directory(backup.as_path())?;
            if driver.world_dir.exists() {
                require_directory(driver.world_dir.as_path())?;
                remove_dir_all_durable(driver.world_dir.as_path())?;
            }
            return rename_durable(backup.as_path(), driver.world_dir.as_path());
        }
        if self.world_was_present {
            if !world_dir_exists_as_directory(driver.world_dir.as_path())? {
                return Err(
                    "prepared checkpoint install lost its original world directory".to_string(),
                );
            }
        } else if driver.world_dir.exists() {
            require_directory(driver.world_dir.as_path())?;
            remove_dir_all_durable(driver.world_dir.as_path())?;
        }
        Ok(())
    }

    fn finalize_committed_best_effort(&self, driver: &NodeRuntimeExecutionDriver) {
        if let Err(err) = remove_checkpoint_install_transaction(driver.records_dir.as_path()) {
            oasis7::observability::emit_stderr_or_event(
                tracing::Level::ERROR,
                format!("checkpoint install committed marker cleanup failed: {err}").as_str(),
                "checkpoint install committed marker cleanup failed",
            );
            return;
        }
        for path in [
            self.staging_world_path(driver),
            self.backup_world_path(driver),
        ] {
            match path {
                Ok(path) if path.exists() => {
                    if let Err(err) = remove_dir_all_durable(path.as_path()) {
                        oasis7::observability::emit_stderr_or_event(
                            tracing::Level::WARN,
                            format!("checkpoint install post-commit world cleanup failed: {err}")
                                .as_str(),
                            "checkpoint install post-commit world cleanup failed",
                        );
                    }
                }
                Ok(_) => {}
                Err(err) => oasis7::observability::emit_stderr_or_event(
                    tracing::Level::WARN,
                    format!("checkpoint install post-commit path validation failed: {err}")
                        .as_str(),
                    "checkpoint install post-commit path validation failed",
                ),
            }
        }
    }
}

fn capture_file(path: &Path) -> Result<Option<Vec<u8>>, String> {
    if !path.exists() {
        return Ok(None);
    }
    let metadata = fs::symlink_metadata(path).map_err(|err| {
        format!(
            "read checkpoint install backup {} failed: {err}",
            path.display()
        )
    })?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(format!(
            "checkpoint install backup is not a regular file: {}",
            path.display()
        ));
    }
    fs::read(path).map(Some).map_err(|err| {
        format!(
            "read checkpoint install backup {} failed: {err}",
            path.display()
        )
    })
}

fn restore_file(path: &Path, bytes: Option<&[u8]>) -> Result<(), String> {
    match bytes {
        Some(bytes) => write_file_durable(path, bytes),
        None => remove_file_durable(path),
    }
}

fn artifact_path(
    driver: &NodeRuntimeExecutionDriver,
    height: u64,
    artifact: PublicationArtifact,
) -> PathBuf {
    match artifact {
        PublicationArtifact::State => driver.state_path.clone(),
        PublicationArtifact::Record => {
            execution_bridge_record_path(driver.records_dir.as_path(), height)
        }
        PublicationArtifact::LatestRecord => driver.records_dir.join("latest.json"),
        PublicationArtifact::Manifest => {
            execution_checkpoint_manifest_path(driver.records_dir.as_path(), height)
        }
        PublicationArtifact::LatestManifest => {
            execution_checkpoint_latest_path(driver.records_dir.as_path())
        }
    }
}

fn world_transaction_path(
    world_dir: &Path,
    transaction_id: &str,
    kind: &str,
) -> Result<PathBuf, String> {
    let name = world_dir
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .ok_or_else(|| format!("world directory {} has no file name", world_dir.display()))?;
    Ok(world_dir.with_file_name(format!(
        ".{name}.checkpoint-install-{transaction_id}.{kind}"
    )))
}

fn require_directory(path: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path).map_err(|err| {
        format!(
            "read world transaction path {} failed: {err}",
            path.display()
        )
    })?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(format!(
            "world transaction path is not a directory: {}",
            path.display()
        ));
    }
    Ok(())
}

fn world_dir_exists_as_directory(path: &Path) -> Result<bool, String> {
    if !path.exists() {
        return Ok(false);
    }
    require_directory(path)?;
    Ok(true)
}

fn remove_checkpoint_install_transaction(records_dir: &Path) -> Result<(), String> {
    remove_file_durable(CheckpointInstallTransaction::path(records_dir).as_path())
}

pub(super) fn load_checkpoint_install_transaction(
    records_dir: &Path,
) -> Result<Option<CheckpointInstallTransaction>, String> {
    #[cfg(test)]
    CHECKPOINT_INSTALL_TRANSACTION_LOAD_COUNT.with(|count| count.set(count.get() + 1));
    let path = CheckpointInstallTransaction::path(records_dir);
    let metadata = match fs::symlink_metadata(path.as_path()) {
        Ok(metadata) => metadata,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => {
            return Err(format!(
                "read checkpoint install transaction metadata failed: {err}"
            ));
        }
    };
    if metadata.file_type().is_symlink() {
        return Err(format!(
            "checkpoint install transaction marker is a symbolic link: {}",
            path.display()
        ));
    }
    if !metadata.is_file() {
        return Err(format!(
            "checkpoint install transaction marker is not a regular file: {}",
            path.display()
        ));
    }
    let transaction: CheckpointInstallTransaction = serde_json::from_slice(
        &fs::read(path.as_path())
            .map_err(|err| format!("read checkpoint install transaction failed: {err}"))?,
    )
    .map_err(|err| format!("parse checkpoint install transaction failed: {err}"))?;
    transaction.validate()?;
    Ok(Some(transaction))
}

pub(super) fn recover_checkpoint_install_transaction(
    driver: &mut NodeRuntimeExecutionDriver,
    transaction: Option<CheckpointInstallTransaction>,
) -> Result<(), String> {
    let Some(transaction) = transaction else {
        return Ok(());
    };
    if transaction.phase == CheckpointInstallTransactionPhase::Prepared {
        transaction.restore(driver)?;
        driver.state = transaction.previous_state.clone();
        let policy = driver.execution_world.release_security_policy().clone();
        driver.execution_world = super::driver_persistence::load_execution_world_with_policy(
            driver.world_dir.as_path(),
            policy,
        )?;
    }
    transaction.finalize_committed_best_effort(driver);
    Ok(())
}

fn persist_manifest_durable(
    records_dir: &Path,
    manifest: &ExecutionCheckpointManifest,
) -> Result<(), String> {
    manifest.validate()?;
    let manifest_bytes = serde_json::to_vec_pretty(manifest)
        .map_err(|err| format!("serialize execution checkpoint manifest failed: {err}"))?;
    write_file_durable(
        execution_checkpoint_manifest_path(records_dir, manifest.height).as_path(),
        manifest_bytes.as_slice(),
    )?;
    let latest = ExecutionCheckpointLatestPointer {
        schema_version: EXECUTION_CHECKPOINT_MANIFEST_SCHEMA_V1,
        checkpoint_id: manifest.checkpoint_id.clone(),
        height: manifest.height,
        manifest_hash: manifest.manifest_hash.clone(),
        manifest_rel_path: execution_checkpoint_manifest_rel_path(manifest.height),
        updated_at_ms: manifest.created_at_ms,
    };
    let latest_bytes = serde_json::to_vec_pretty(&latest)
        .map_err(|err| format!("serialize execution checkpoint latest pointer failed: {err}"))?;
    write_file_durable(
        execution_checkpoint_latest_path(records_dir).as_path(),
        latest_bytes.as_slice(),
    )
}

fn persist_record_durable(
    records_dir: &Path,
    record: &ExecutionBridgeRecord,
) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(record)
        .map_err(|err| format!("serialize execution bridge record failed: {err}"))?;
    write_file_durable(
        execution_bridge_record_path(records_dir, record.height).as_path(),
        bytes.as_slice(),
    )?;
    write_file_durable(records_dir.join("latest.json").as_path(), bytes.as_slice())
}

fn persist_state_durable(path: &Path, state: &super::ExecutionBridgeState) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(state)
        .map_err(|err| format!("serialize execution bridge state failed: {err}"))?;
    write_file_durable(path, bytes.as_slice())
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CheckpointInstallFault {
    AfterPrepare,
    AfterRecordLatestPublication,
    FinalStatePersistFailure,
    AfterFinalStatePersist,
    AfterCommittedMarkerPersist,
}

#[cfg(test)]
static CHECKPOINT_INSTALL_FAULT: std::sync::Mutex<
    Vec<(std::thread::ThreadId, CheckpointInstallFault)>,
> = std::sync::Mutex::new(Vec::new());

#[cfg(test)]
thread_local! {
    static CHECKPOINT_INSTALL_TRANSACTION_LOAD_COUNT: std::cell::Cell<usize> = const {
        std::cell::Cell::new(0)
    };
}

#[cfg(test)]
pub(super) fn reset_checkpoint_install_transaction_load_count_for_test() {
    CHECKPOINT_INSTALL_TRANSACTION_LOAD_COUNT.with(|count| count.set(0));
}

#[cfg(test)]
pub(super) fn checkpoint_install_transaction_load_count_for_test() -> usize {
    CHECKPOINT_INSTALL_TRANSACTION_LOAD_COUNT.with(std::cell::Cell::get)
}

#[cfg(test)]
pub(super) fn set_checkpoint_install_fault_for_test(fault: Option<CheckpointInstallFault>) {
    let thread = std::thread::current().id();
    let mut configured = CHECKPOINT_INSTALL_FAULT
        .lock()
        .expect("checkpoint install fault lock");
    configured.retain(|(configured_thread, _)| *configured_thread != thread);
    if let Some(fault) = fault {
        configured.push((thread, fault));
    }
}

#[cfg(test)]
fn inject_fault(fault: CheckpointInstallFault) -> Result<(), String> {
    let mut configured = CHECKPOINT_INSTALL_FAULT
        .lock()
        .expect("checkpoint install fault lock");
    if let Some(index) = configured.iter().position(|(thread, configured_fault)| {
        *thread == std::thread::current().id() && *configured_fault == fault
    }) {
        configured.swap_remove(index);
        return Err(match fault {
            CheckpointInstallFault::AfterPrepare => {
                "injected checkpoint install crash after prepare"
            }
            CheckpointInstallFault::AfterRecordLatestPublication => {
                "injected checkpoint install crash after record/latest publication"
            }
            CheckpointInstallFault::FinalStatePersistFailure => {
                "injected checkpoint install final state persistence failure"
            }
            CheckpointInstallFault::AfterFinalStatePersist => {
                "injected checkpoint install crash after final state persistence"
            }
            CheckpointInstallFault::AfterCommittedMarkerPersist => {
                "injected checkpoint install crash after committed marker persistence"
            }
        }
        .to_string());
    }
    Ok(())
}

macro_rules! inject_checkpoint_fault {
    ($fault:ident) => {{
        #[cfg(test)]
        inject_fault(CheckpointInstallFault::$fault)?;
        #[cfg(not(test))]
        inject_fault(())?;
    }};
}

pub(super) fn install_checkpoint_bundle(
    driver: &mut NodeRuntimeExecutionDriver,
    context: NodeExecutionCheckpointInstallContext,
    bundle: NodeExecutionCheckpointBundle,
) -> Result<NodeExecutionCommitResult, String> {
    let install_started_at = Instant::now();
    if bundle.height != context.height
        || bundle.execution_block_hash != context.execution_block_hash
        || bundle.execution_state_root != context.execution_state_root
    {
        return Err(format!(
            "execution checkpoint bundle does not match install context height={}",
            context.height
        ));
    }
    let manifest_decode_started_at = Instant::now();
    let manifest = serde_json::from_slice::<ExecutionCheckpointManifest>(&bundle.manifest_json)
        .map_err(|err| {
            format!(
                "decode execution checkpoint manifest failed at height {}: {err}",
                context.height
            )
        })?;
    manifest.validate()?;
    if manifest.schema_version < EXECUTION_CHECKPOINT_MANIFEST_SCHEMA_V2 {
        return Err(format!(
            "execution checkpoint v1 manifest cannot be installed at height {}: v2 predecessor anchor required",
            context.height
        ));
    }
    let manifest_decode_ms = manifest_decode_started_at.elapsed();
    if manifest.world_id != context.world_id
        || manifest.height != context.height
        || manifest.execution_block_hash != context.execution_block_hash
        || manifest.execution_state_root != context.execution_state_root
    {
        return Err(format!(
            "execution checkpoint manifest does not match install context height={}",
            context.height
        ));
    }
    let bundle_blob_count = bundle.blobs.len();
    let bundle_bytes = bundle
        .blobs
        .iter()
        .fold(bundle.manifest_json.len(), |total, blob| {
            total.saturating_add(blob.bytes.len())
        });
    emit_checkpoint_bundle_install_start(
        context.world_id.as_str(),
        context.height,
        manifest.checkpoint_id.as_str(),
        bundle_blob_count,
        bundle_bytes,
        manifest.pinned_refs.len(),
    );
    let blob_store_started_at = Instant::now();
    for blob in &bundle.blobs {
        let actual = blake3_hex(blob.bytes.as_slice());
        if actual != blob.content_hash {
            return Err(format!(
                "execution checkpoint blob hash mismatch expected={} actual={}",
                blob.content_hash, actual
            ));
        }
        driver
            .execution_store
            .put(blob.content_hash.as_str(), blob.bytes.as_slice())
            .map_err(|err| {
                format!(
                    "store execution checkpoint blob {} failed: {err:?}",
                    blob.content_hash
                )
            })?;
    }
    let mut blob_store_ms = blob_store_started_at.elapsed();
    let pin_check_started_at = Instant::now();
    for content_hash in &manifest.pinned_refs {
        if !driver
            .execution_store
            .has(content_hash.as_str())
            .map_err(|err| {
                format!("check execution checkpoint blob {content_hash} failed: {err:?}")
            })?
        {
            return Err(format!(
                "execution checkpoint missing pinned blob {content_hash} at height {}",
                context.height
            ));
        }
    }
    let pin_check_ms = pin_check_started_at.elapsed();
    let snapshot_started_at = Instant::now();
    let snapshot_bytes = driver
        .execution_store
        .get_verified(manifest.latest_state_ref.as_str())
        .map_err(|err| {
            format!(
                "load execution checkpoint snapshot {} failed: {err:?}",
                manifest.latest_state_ref
            )
        })?;
    blob_store_ms += snapshot_started_at.elapsed();
    let snapshot_bytes_len = snapshot_bytes.len();
    let snapshot_decode_started_at = Instant::now();
    let snapshot =
        serde_cbor::from_slice::<RuntimeSnapshot>(snapshot_bytes.as_slice()).map_err(|err| {
            format!(
                "decode execution checkpoint snapshot failed at height {}: {err}",
                context.height
            )
        })?;
    let mut decode_ms = manifest_decode_ms + snapshot_decode_started_at.elapsed();
    let actual_state_root = blake3_hex(snapshot_bytes.as_slice());
    if actual_state_root != context.execution_state_root {
        return Err(format!(
            "execution checkpoint snapshot root mismatch at height {}: expected={} actual={}",
            context.height, context.execution_state_root, actual_state_root
        ));
    }
    let journal_ref = manifest.journal_ref.as_deref().ok_or_else(|| {
        format!(
            "execution checkpoint manifest missing journal_ref at height {}",
            context.height
        )
    })?;
    let journal_started_at = Instant::now();
    let journal_bytes = driver
        .execution_store
        .get_verified(journal_ref)
        .map_err(|err| {
            format!("load execution checkpoint journal {journal_ref} failed: {err:?}")
        })?;
    blob_store_ms += journal_started_at.elapsed();
    let journal_bytes_len = journal_bytes.len();
    let journal_decode_started_at = Instant::now();
    let journal =
        serde_cbor::from_slice::<RuntimeJournal>(journal_bytes.as_slice()).map_err(|err| {
            format!(
                "decode execution checkpoint journal failed at height {}: {err}",
                context.height
            )
        })?;
    decode_ms += journal_decode_started_at.elapsed();
    let world_policy = driver.execution_world.release_security_policy().clone();
    let restored_resource_manifest = snapshot.chain_resource_manifest.clone();
    let restored_resource_delta = snapshot.latest_chain_resource_delta.clone();
    let rebuild_started_at = Instant::now();
    let mut restored_world = oasis7::runtime_host::world::from_snapshot(snapshot.clone(), journal)
        .map_err(|err| {
            format!(
                "rebuild execution checkpoint world failed at height {}: {err:?}",
                context.height
            )
        })?;
    restored_world.set_release_security_policy(world_policy);
    let rebuild_ms = rebuild_started_at.elapsed();
    let restored_commit_block_hash = restored_resource_delta
        .as_ref()
        .and_then(|delta| delta.commit_block_hash.as_deref())
        .or(restored_resource_manifest.created_at_block_hash.as_deref());
    let restored_resource_context = ChainResourceDerivationContext {
        world_id: restored_resource_manifest.world_id.as_str(),
        chain_id: restored_resource_manifest.chain_id.as_str(),
        genesis_ref: restored_resource_manifest.genesis_ref.as_deref(),
        created_at_height: restored_resource_manifest.created_at_height,
        manifest_height: restored_resource_manifest.manifest_height,
        commit_block_hash: restored_commit_block_hash,
        tick: restored_world.state().time,
    };
    let record = ExecutionBridgeRecord {
        schema_version: EXECUTION_BRIDGE_RECORD_SCHEMA_V3,
        world_id: context.world_id.clone(),
        height: context.height,
        node_block_hash: Some(context.node_block_hash.clone()),
        prev_node_block_hash: None,
        proposer_id: None,
        action_root: None,
        execution_block_hash: context.execution_block_hash.clone(),
        execution_state_root: context.execution_state_root.clone(),
        journal_len: snapshot.journal_len,
        latest_state_ref: Some(manifest.latest_state_ref.clone()),
        snapshot_ref: manifest.snapshot_ref.clone(),
        journal_ref: manifest.journal_ref.clone(),
        commit_log_ref: None,
        checkpoint_ref: Some(execution_checkpoint_manifest_rel_path(context.height)),
        external_effect_ref: None,
        world_head_proof_ref: None,
        world_head_proof_hash: None,
        controlled_capture_ref: None,
        simulator_mirror: None,
        timestamp_ms: context.committed_at_unix_ms,
    };
    let previous_execution_world = driver.execution_world.clone();
    let previous_state = driver.state.clone();
    let mut transaction = CheckpointInstallTransaction::prepare(driver, context.height)?;
    transaction.persist(driver.records_dir.as_path())?;
    inject_checkpoint_fault!(AfterPrepare);
    let mut persist_ms = Duration::default();
    let install_result = (|| -> Result<(), String> {
        let world_started_at = Instant::now();
        transaction.publish_world(
            driver,
            &restored_world,
            restored_resource_context,
            restored_resource_manifest.world_config_hash.as_str(),
            restored_resource_manifest
                .generation_algorithm_hash
                .as_str(),
        )?;
        persist_ms += world_started_at.elapsed();
        let manifest_started_at = Instant::now();
        persist_manifest_durable(driver.records_dir.as_path(), &manifest)?;
        persist_ms += manifest_started_at.elapsed();
        let record_started_at = Instant::now();
        persist_record_durable(driver.records_dir.as_path(), &record)?;
        persist_ms += record_started_at.elapsed();
        inject_checkpoint_fault!(AfterRecordLatestPublication);
        driver.execution_world = restored_world;
        driver.state.last_applied_committed_height = context.height;
        driver.state.last_execution_block_hash = Some(context.execution_block_hash.clone());
        driver.state.last_execution_state_root = Some(context.execution_state_root.clone());
        driver.state.last_node_block_hash = Some(context.node_block_hash.clone());
        let state_started_at = Instant::now();
        inject_checkpoint_fault!(FinalStatePersistFailure);
        persist_state_durable(driver.state_path.as_path(), &driver.state)?;
        persist_ms += state_started_at.elapsed();
        inject_checkpoint_fault!(AfterFinalStatePersist);
        transaction.phase = CheckpointInstallTransactionPhase::Committed;
        transaction.persist(driver.records_dir.as_path())?;
        inject_checkpoint_fault!(AfterCommittedMarkerPersist);
        Ok(())
    })();
    if let Err(err) = install_result {
        if err.starts_with("injected checkpoint install crash") {
            return Err(err);
        }
        driver.execution_world = previous_execution_world;
        driver.state = previous_state;
        if let Err(rollback_err) = transaction
            .restore(driver)
            .and_then(|_| remove_checkpoint_install_transaction(driver.records_dir.as_path()))
        {
            return Err(format!(
                "checkpoint install failed: {err}; transaction rollback failed: {rollback_err}"
            ));
        }
        return Err(err);
    }
    transaction.finalize_committed_best_effort(driver);
    let retention_started_at = Instant::now();
    let retention_result =
        begin_execution_bridge_retention_transaction(driver.records_dir.as_path())
            .and_then(|_| {
                super::checkpoint::refresh_live_task_read_evidence(
                    driver.records_dir.as_path(),
                    &driver.execution_store,
                    &driver.execution_world,
                    driver.hot_window_heights,
                )
            })
            .and_then(|_| {
                run_execution_bridge_retention_maintenance(
                    driver.records_dir.as_path(),
                    &driver.execution_store,
                    driver.hot_window_heights,
                )
            })
            .and_then(|_| {
                complete_execution_bridge_retention_transaction(driver.records_dir.as_path())
            });
    if let Err(err) = retention_result {
        driver.retention_reconcile_pending = true;
        driver.retention_reconcile_next_height = Some(
            context
                .height
                .saturating_add(driver.checkpoint_interval_heights.max(1)),
        );
        if let Err(marker_err) =
            fail_execution_bridge_retention_transaction(driver.records_dir.as_path())
        {
            oasis7::observability::emit_stderr_or_event(
                tracing::Level::ERROR,
                format!(
                    "checkpoint install retention degradation publication failed: {marker_err}"
                )
                .as_str(),
                "checkpoint install retention degradation publication failed",
            );
        }
        oasis7::observability::emit_stderr_or_event(
            tracing::Level::WARN,
            format!(
                "checkpoint install retention maintenance failed at height {}: {err}",
                context.height
            )
            .as_str(),
            "checkpoint install retention maintenance failed",
        );
    }
    let retention_ms = retention_started_at.elapsed();
    emit_checkpoint_bundle_install_complete(CheckpointInstallObservation {
        world_id: context.world_id.as_str(),
        height: context.height,
        checkpoint_id: manifest.checkpoint_id.as_str(),
        blob_count: bundle_blob_count,
        bundle_bytes,
        pinned_ref_count: manifest.pinned_refs.len(),
        snapshot_bytes: snapshot_bytes_len,
        journal_bytes: journal_bytes_len,
        blob_store_ms,
        pin_check_ms,
        decode_ms,
        rebuild_ms,
        persist_ms,
        retention_ms,
        total_ms: install_started_at.elapsed(),
    });
    Ok(NodeExecutionCommitResult {
        execution_height: context.height,
        execution_block_hash: context.execution_block_hash,
        execution_state_root: context.execution_state_root,
    })
}

#[cfg(not(test))]
fn inject_fault(_: ()) -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
mod transaction_tests {
    use super::*;

    #[test]
    fn checkpoint_install_marker_rejects_corrupt_path_and_artifact_fields() {
        let mut transaction = CheckpointInstallTransaction {
            phase: CheckpointInstallTransactionPhase::Prepared,
            transaction_id: "../outside".to_string(),
            height: 2,
            previous_state: super::super::ExecutionBridgeState::default(),
            world_was_present: false,
            backups: Vec::new(),
        };
        assert!(transaction.validate().is_err());
        transaction.transaction_id = "123-0".to_string();
        transaction.backups = (0..5)
            .map(|_| FileBackup {
                artifact: PublicationArtifact::State,
                bytes: None,
            })
            .collect();
        assert!(transaction.validate().is_err());
    }
}
