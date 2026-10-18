use super::super::capability_authorization::CapabilityRevocationState;
use super::super::util::{hash_json, read_json_from_path, write_json_to_path};
use super::super::{
    Journal, JournalSegmentRef, LocalCasStore, ModuleCache, ModuleRegistry, ModuleStore,
    SegmentConfig, Snapshot, TickConsensusRecord, WorldError, WorldEvent, WorldTime,
    segment_journal, segment_snapshot,
};
use super::World;
use super::module_tick_runtime::ModuleTickRoutingMetrics;
use oasis7_distfs::{assemble_journal, assemble_snapshot};
use oasis7_proto::distributed::SnapshotManifest;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::collections::VecDeque;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
#[path = "authoritative_recovery_generation.rs"]
mod authoritative_recovery_generation;
pub use authoritative_recovery_generation::{
    AuthoritativeRecoveryCommitError, AuthoritativeRecoveryCommitStatus,
    CommittedAuthoritativeRecoveryGeneration,
};
#[cfg(not(target_arch = "wasm32"))]
#[path = "observer_load.rs"]
mod observer_load;
#[path = "persistence_support.rs"]
mod persistence_support;
#[path = "persistence_tail.rs"]
mod persistence_tail;
#[cfg(not(target_arch = "wasm32"))]
pub use observer_load::{ObserverLoadControl, ObserverLoadError, ObserverReadLimits};
#[path = "persistence_tick_consensus.rs"]
mod persistence_tick_consensus;
use self::persistence_support::{
    distfs_world_id, now_unix_ms, persist_sidecar_generation_index, write_distfs_recovery_audit,
};
#[path = "persistence_format.rs"]
mod persistence_format;
use persistence_tick_consensus::{
    TickConsensusArchiveFile, hydrate_tick_consensus_snapshot_from_archive,
    hydrate_tick_consensus_snapshot_from_archived_records,
    load_persisted_tick_consensus_snapshot_from_dir, persist_tick_consensus_archive,
    split_tick_consensus_snapshot_for_persistence,
};

const JOURNAL_FILE: &str = "journal.json";
const SNAPSHOT_FILE: &str = "snapshot.json";
const DISTFS_STATE_DIR: &str = ".distfs-state";
const DISTFS_SNAPSHOT_MANIFEST_FILE: &str = "snapshot.manifest.json";
const DISTFS_JOURNAL_SEGMENTS_FILE: &str = "journal.segments.json";
const DISTFS_RECOVERY_AUDIT_FILE: &str = "distfs.recovery.audit.json";
const DISTFS_WORLD_ID_FALLBACK: &str = "runtime-world";
const TICK_CONSENSUS_ARCHIVE_FILE: &str = "tick-consensus.archive.json";
const TICK_CONSENSUS_ARCHIVE_INDEX_FILE: &str = "tick-consensus.archive.index.json";
const TICK_CONSENSUS_ARCHIVE_SEGMENTS_DIR: &str = "tick-consensus.archive.segments";
const TICK_CONSENSUS_ARCHIVE_SEGMENT_LEN: usize = 64;
const TICK_CONSENSUS_HOT_LIMIT: usize = 128;
const SIDECAR_GENERATION_INDEX_SCHEMA_V1: u32 = 1;
const SIDECAR_GENERATION_RECORD_SCHEMA_V1: u32 = 1;
const SIDECAR_GENERATION_ROOT_DIR: &str = "sidecar-generations";
const SIDECAR_GENERATION_INDEX_FILE: &str = "index.json";
const SIDECAR_GENERATION_MANIFESTS_DIR: &str = "generations";
const SIDECAR_GENERATION_PAYLOADS_DIR: &str = "payloads";
const SIDECAR_GENERATION_STAGING_DIR: &str = "generation.tmp";
const SIDECAR_GENERATION_KEEP_LATEST: usize = 2;
const SIDECAR_GENERATION_SNAPSHOT_MANIFEST_FILE: &str = "snapshot.manifest.json";
const SIDECAR_GENERATION_JOURNAL_SEGMENTS_FILE: &str = "journal.segments.json";
const SIDECAR_GENERATION_RECOVERY_METADATA_FILE: &str = "viewer-recovery.bin";

struct PreparedModuleStoreLoad {
    registry: ModuleRegistry,
    artifacts: BTreeSet<String>,
    artifact_bytes: BTreeMap<String, Arc<[u8]>>,
}

impl PreparedModuleStoreLoad {
    fn install(self, world: &mut World) {
        world.module_registry = self.registry;
        world.module_artifacts = self.artifacts;
        world.module_artifact_bytes = self.artifact_bytes;
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct SidecarGcResult {
    status: String,
    freed_blob_count: usize,
    freed_bytes: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    updated_at_ms: i64,
}

impl SidecarGcResult {
    fn not_run() -> Self {
        Self {
            status: "not_run".to_string(),
            freed_blob_count: 0,
            freed_bytes: 0,
            error: None,
            updated_at_ms: now_unix_ms(),
        }
    }

    fn success(freed_blob_count: usize, freed_bytes: u64) -> Self {
        Self {
            status: "success".to_string(),
            freed_blob_count,
            freed_bytes,
            error: None,
            updated_at_ms: now_unix_ms(),
        }
    }

    fn failed(error: String) -> Self {
        Self {
            status: "failed".to_string(),
            freed_blob_count: 0,
            freed_bytes: 0,
            error: Some(error),
            updated_at_ms: now_unix_ms(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct SidecarGenerationRecord {
    schema_version: u32,
    generation_id: String,
    snapshot_manifest_path: String,
    journal_segments_path: String,
    snapshot_manifest_hash: String,
    manifest_hash: String,
    journal_segment_hashes: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    recovery_metadata_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    recovery_metadata_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    tick_consensus_archive_ref: Option<String>,
    pinned_blob_hashes: Vec<String>,
    created_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct SidecarGenerationIndex {
    schema_version: u32,
    latest_generation: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    rollback_safe_generation: Option<String>,
    generations: BTreeMap<String, SidecarGenerationRecord>,
    last_gc_result: SidecarGcResult,
}

#[derive(Debug, Serialize)]
struct SidecarGenerationHashPayload<'a> {
    generation_id: &'a str,
    snapshot_manifest_path: &'a str,
    journal_segments_path: &'a str,
    snapshot_manifest_hash: &'a str,
    journal_segment_hashes: &'a [String],
    recovery_metadata_path: &'a Option<String>,
    recovery_metadata_hash: &'a Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tick_consensus_archive_ref: &'a Option<String>,
    pinned_blob_hashes: &'a [String],
    created_at_ms: i64,
}

#[derive(Debug, Serialize)]
struct DistfsRecoveryAuditRecord {
    timestamp_ms: i64,
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
}
fn validate_compatible_legacy_distfs_payloads(
    dir: &Path,
    selected_manifest: &SnapshotManifest,
    selected_journal_segments: &[JournalSegmentRef],
) -> Result<(), WorldError> {
    let manifest_path = dir.join(DISTFS_SNAPSHOT_MANIFEST_FILE);
    if !manifest_path.exists() {
        return Ok(());
    }
    let legacy_manifest: SnapshotManifest = read_json_from_path(manifest_path.as_path())?;
    if hash_json(&legacy_manifest)? != hash_json(selected_manifest)? {
        // A valid but different top-level payload can belong to a newer
        // generation while an older sidecar generation is being restored.
        return Ok(());
    }
    let journal_path = dir.join(DISTFS_JOURNAL_SEGMENTS_FILE);
    if !journal_path.exists() {
        return Ok(());
    }
    let legacy_journal: Vec<JournalSegmentRef> = read_json_from_path(journal_path.as_path())?;
    if legacy_journal != selected_journal_segments {
        return Err(WorldError::DistributedValidationFailed {
            reason: "selected sidecar generation conflicts with legacy journal payload".to_string(),
        });
    }
    Ok(())
}
fn load_json_snapshot_if_newer_chain_resource_context(
    dir: &Path,
    distfs_snapshot: &Snapshot,
) -> Result<Option<Snapshot>, WorldError> {
    let snapshot_path = dir.join(SNAPSHOT_FILE);
    if !snapshot_path.exists() {
        return Ok(None);
    }
    let json_snapshot = Snapshot::load_json(snapshot_path)?;
    if !chain_resource_manifest_has_external_context(&json_snapshot.chain_resource_manifest) {
        return Ok(None);
    }
    if !chain_resource_manifest_is_bound(&distfs_snapshot.chain_resource_manifest) {
        return Ok(Some(json_snapshot));
    }
    if json_snapshot.chain_resource_manifest.world_id
        != distfs_snapshot.chain_resource_manifest.world_id
        || json_snapshot.chain_resource_manifest.chain_id
            != distfs_snapshot.chain_resource_manifest.chain_id
    {
        return Ok(Some(json_snapshot));
    }
    let json_manifest_height = json_snapshot.chain_resource_manifest.manifest_height;
    let distfs_manifest_height = distfs_snapshot.chain_resource_manifest.manifest_height;
    if json_manifest_height > distfs_manifest_height
        || (json_manifest_height == distfs_manifest_height
            && json_snapshot.state.time > distfs_snapshot.state.time)
    {
        return Ok(Some(json_snapshot));
    }
    Ok(None)
}
fn chain_resource_manifest_is_bound(manifest: &super::super::ChainResourceManifest) -> bool {
    manifest.is_schema_current() && manifest.world_id != "unbound" && manifest.chain_id != "unbound"
}
fn chain_resource_manifest_has_external_context(
    manifest: &super::super::ChainResourceManifest,
) -> bool {
    chain_resource_manifest_is_bound(manifest)
        && !(manifest.world_id == DISTFS_WORLD_ID_FALLBACK && manifest.chain_id == "runtime-chain")
}
fn verify_tick_consensus_record_slice(records: &[TickConsensusRecord]) -> Result<(), WorldError> {
    let mut previous_block_hash = None;
    let mut previous_height: Option<u64> = None;
    let mut previous_tick: Option<WorldTime> = None;
    for record in records {
        let block_hash = record.block.block_hash();
        if record.certificate.block_hash != block_hash {
            return Err(WorldError::DistributedValidationFailed {
                reason: format!(
                    "tick consensus archive block hash mismatch tick={} expected={} actual={}",
                    record.block.header.tick, block_hash, record.certificate.block_hash,
                ),
            });
        }
        if let Some(previous_block_hash) = previous_block_hash.as_ref()
            && record.block.header.parent_hash != *previous_block_hash
        {
            return Err(WorldError::DistributedValidationFailed {
                reason: format!(
                    "tick consensus archive parent hash mismatch tick={} expected={} actual={}",
                    record.block.header.tick, previous_block_hash, record.block.header.parent_hash,
                ),
            });
        }
        if let Some(previous_height) = previous_height {
            let expected_height = previous_height.saturating_add(1);
            if record.certificate.consensus_height != expected_height {
                return Err(WorldError::DistributedValidationFailed {
                    reason: format!(
                        "tick consensus archive height mismatch tick={} expected={} actual={}",
                        record.block.header.tick,
                        expected_height,
                        record.certificate.consensus_height,
                    ),
                });
            }
        }
        if let Some(previous_tick) = previous_tick
            && record.block.header.tick <= previous_tick
        {
            return Err(WorldError::DistributedValidationFailed {
                reason: format!(
                    "tick consensus archive tick ordering mismatch previous={} current={}",
                    previous_tick, record.block.header.tick,
                ),
            });
        }
        previous_block_hash = Some(record.certificate.block_hash.clone());
        previous_height = Some(record.certificate.consensus_height);
        previous_tick = Some(record.block.header.tick);
    }
    Ok(())
}
impl World {
    // ---------------------------------------------------------------------
    // Persistence
    // ---------------------------------------------------------------------

    pub fn snapshot(&self) -> Snapshot {
        let manifest_hash = super::super::util::hash_json(&self.manifest).unwrap_or_default();
        let mut snapshot = self.snapshot_with_chain_resource_context(
            super::super::ChainResourceDerivationContext {
                world_id: DISTFS_WORLD_ID_FALLBACK,
                chain_id: "runtime-chain",
                genesis_ref: None,
                created_at_height: self.journal.len() as u64,
                manifest_height: self.journal.len() as u64,
                commit_block_hash: None,
                tick: self.state.time,
            },
            manifest_hash.clone(),
            manifest_hash,
        );
        if chain_resource_manifest_has_external_context(&self.chain_resource_manifest) {
            snapshot.chain_resource_manifest = self.chain_resource_manifest.clone();
            snapshot.latest_chain_resource_delta = self.latest_chain_resource_delta.clone();
        }
        snapshot
    }

    pub fn snapshot_with_chain_resource_context(
        &self,
        chain_resource_context: super::super::ChainResourceDerivationContext<'_>,
        world_config_hash: impl Into<String>,
        generation_algorithm_hash: impl Into<String>,
    ) -> Snapshot {
        let chain_resource_manifest = self.chain_resource_manifest_snapshot_with_context(
            chain_resource_context,
            world_config_hash,
            generation_algorithm_hash,
        );
        let latest_chain_resource_delta = Some(
            crate::chain_resource::latest_from_runtime_manifest(
                super::super::ChainResourceDerivationContext {
                    world_id: chain_resource_manifest.world_id.as_str(),
                    chain_id: chain_resource_manifest.chain_id.as_str(),
                    genesis_ref: chain_resource_manifest.genesis_ref.as_deref(),
                    created_at_height: chain_resource_manifest.created_at_height,
                    manifest_height: chain_resource_manifest.manifest_height,
                    commit_block_hash: chain_resource_manifest.created_at_block_hash.as_deref(),
                    tick: self.state.time,
                },
                &chain_resource_manifest,
            ),
        );
        Snapshot {
            snapshot_catalog: self.snapshot_catalog.clone(),
            manifest: self.manifest.clone(),
            cognition: self.cognition.clone(),
            chain_resource_manifest,
            latest_chain_resource_delta,
            module_registry: self.module_registry.clone(),
            module_artifacts: self.module_artifacts.clone(),
            module_artifact_bytes: self
                .module_artifact_bytes
                .iter()
                .map(|(wasm_hash, bytes)| (wasm_hash.clone(), bytes.as_ref().to_vec()))
                .collect(),
            module_limits_max: self.module_limits_max.clone(),
            state: self.state.clone(),
            journal_len: self.journal.len(),
            last_event_id: self
                .journal
                .events
                .last()
                .map(|event| event.id)
                .unwrap_or_else(|| self.next_event_id.saturating_sub(1)),
            journal_commitment: self.journal.commitment().unwrap_or_default(),
            event_id_era: self.next_event_id_era,
            next_action_id: self.next_action_id,
            action_id_era: self.next_action_id_era,
            next_intent_id: self.next_intent_id,
            intent_id_era: self.next_intent_id_era,
            next_proposal_id: self.next_proposal_id,
            proposal_id_era: self.next_proposal_id_era,
            pending_actions: self.pending_actions.iter().cloned().collect(),
            pending_effects: self.pending_effects.iter().cloned().collect(),
            inflight_effects: self.inflight_effects.clone(),
            module_tick_schedule: self.module_tick_schedule.clone(),
            module_tick_routing_metrics: self.module_tick_routing_metrics.deterministic_snapshot(),
            capabilities: self.capabilities.clone(),
            capability_grants_v2: self.capability_grants_v2.clone(),
            capability_revocation_state: self.capability_revocation_state.clone(),
            capability_nonce_records: self.capability_nonce_records.clone(),
            capability_authorization_receipts: self.capability_authorization_receipts.clone(),
            capability_invocation_contexts: self.capability_invocation_contexts.clone(),
            capability_authorization_root: self.capability_authorization_root.clone(),
            capability_budget_accounts: self.capability_budget_accounts.clone(),
            capability_effect_receipt_links: self.capability_effect_receipt_links.clone(),
            policies: self.policies.clone(),
            proposals: self.proposals.clone(),
            scheduler_cursor: self.scheduler_cursor.clone(),
            runtime_memory_limits: self.runtime_memory_limits.clone(),
            runtime_backpressure_stats: self.runtime_backpressure_stats.clone(),
            tick_consensus_records: self.tick_consensus_records.clone(),
            tick_consensus_total_record_count: self.tick_consensus_record_count(),
            tick_consensus_archived_record_count: self.formal_archived_tick_consensus_count,
            tick_consensus_hot_from_tick: self
                .tick_consensus_records
                .first()
                .map(|record| record.block.header.tick),
            tick_consensus_hot_to_tick: self
                .tick_consensus_records
                .last()
                .map(|record| record.block.header.tick),
            tick_consensus_authority_source: self.tick_consensus_authority_source.clone(),
            tick_consensus_rejection_audit_events: self
                .tick_consensus_rejection_audit_events
                .clone(),
            governance_execution_policy: self.governance_execution_policy.clone(),
            governance_finality_epoch_snapshots: self.governance_finality_epoch_snapshots.clone(),
            governance_emergency_brake_until_tick: self.governance_emergency_brake_until_tick,
            governance_identity_penalties: self.governance_identity_penalties.clone(),
            next_governance_identity_penalty_id: self.next_governance_identity_penalty_id,
            rollback_authority_registry: self.rollback_authority_registry.clone(),
            consumed_rollback_nonces: self.consumed_rollback_nonces.clone(),
            rollback_nonce_outcomes: self.rollback_nonce_outcomes.clone(),
        }
    }

    fn chain_resource_manifest_snapshot_with_context(
        &self,
        context: super::super::ChainResourceDerivationContext<'_>,
        world_config_hash: impl Into<String>,
        generation_algorithm_hash: impl Into<String>,
    ) -> super::super::ChainResourceManifest {
        crate::chain_resource::from_runtime_state(
            context,
            world_config_hash,
            generation_algorithm_hash,
            &self.state,
        )
    }

    pub fn save_to_dir(&self, dir: impl AsRef<Path>) -> Result<(), WorldError> {
        let dir = dir.as_ref();
        fs::create_dir_all(dir)?;
        let snapshot = self.snapshot();
        self.persist_snapshot_files_to_dir(dir, snapshot)?;
        self.attach_persistence_dir(dir);
        Ok(())
    }

    pub fn save_to_dir_with_chain_resource_context(
        &self,
        dir: impl AsRef<Path>,
        chain_resource_context: super::super::ChainResourceDerivationContext<'_>,
        world_config_hash: impl Into<String>,
        generation_algorithm_hash: impl Into<String>,
    ) -> Result<(), WorldError> {
        let dir = dir.as_ref();
        fs::create_dir_all(dir)?;
        let snapshot = self.snapshot_with_chain_resource_context(
            chain_resource_context,
            world_config_hash,
            generation_algorithm_hash,
        );
        self.persist_snapshot_files_to_dir(dir, snapshot)?;
        self.attach_persistence_dir(dir);
        Ok(())
    }

    fn persist_snapshot_files_to_dir(
        &self,
        dir: &Path,
        snapshot: Snapshot,
    ) -> Result<(), WorldError> {
        // Publish the selector before touching any snapshot, journal, sidecar,
        // or module bytes. An older runtime that implements this selector
        // must fail closed before decoding data written by a newer format.
        persistence_format::write_selector_before_payloads(dir)?;
        let (persisted_snapshot, tick_consensus_archive) =
            split_tick_consensus_snapshot_for_persistence(&snapshot);
        let journal_path = dir.join(JOURNAL_FILE);
        let snapshot_path = dir.join(SNAPSHOT_FILE);
        self.journal.save_json(journal_path)?;
        persisted_snapshot.save_json(snapshot_path)?;
        persist_tick_consensus_archive(dir, &persisted_snapshot, tick_consensus_archive.as_ref())?;
        let archive_bytes = tick_consensus_archive
            .as_ref()
            .map(serde_json::to_vec)
            .transpose()
            .map_err(|err| WorldError::DistributedValidationFailed {
                reason: format!("serialize tick consensus generation archive failed: {err}"),
            })?;
        self.save_distfs_sidecar(dir, &persisted_snapshot, archive_bytes.as_deref(), None)?;
        self.save_module_store_to_dir(dir)?;
        Ok(())
    }

    fn attach_persistence_dir(&self, dir: &Path) {
        *self.persistence_dir.borrow_mut() = Some(dir.to_path_buf());
    }

    pub fn save_to_dir_with_modules(&self, dir: impl AsRef<Path>) -> Result<(), WorldError> {
        self.save_to_dir(dir)
    }

    pub fn save_module_store_to_dir(&self, dir: impl AsRef<Path>) -> Result<(), WorldError> {
        let store = ModuleStore::new(dir);
        store.save_registry(&self.module_registry)?;
        for record in self.module_registry.records.values() {
            store.write_meta(&record.manifest)?;
            let wasm_hash = &record.manifest.wasm_hash;
            let bytes = self.module_artifact_bytes.get(wasm_hash).ok_or_else(|| {
                WorldError::ModuleStoreArtifactMissing {
                    wasm_hash: wasm_hash.clone(),
                }
            })?;
            store.write_artifact(wasm_hash, bytes.as_ref())?;
        }
        Ok(())
    }

    pub fn load_from_dir(dir: impl AsRef<Path>) -> Result<Self, WorldError> {
        let dir = dir.as_ref();
        persistence_format::validate_selector_before_load(dir)?;
        if let Some((mut snapshot, journal)) = Self::try_load_from_distfs_sidecar(dir)? {
            let has_indexed_generation = Self::has_indexed_sidecar_generation(dir)?;
            if !has_indexed_generation
                && let Some(mut json_snapshot) =
                    load_json_snapshot_if_newer_chain_resource_context(dir, &snapshot)?
            {
                let _ = write_distfs_recovery_audit(
                    dir,
                    "fallback_json",
                    Some("distfs_restore_stale_chain_resource_context".to_string()),
                );
                let journal = Journal::load_json(dir.join(JOURNAL_FILE))?;
                hydrate_tick_consensus_snapshot_from_archive(dir, &mut json_snapshot)?;
                let has_inline_module_artifacts = !json_snapshot.module_artifact_bytes.is_empty();
                let world = Self::from_snapshot(json_snapshot, journal)?;
                let mut world = Self::recover_loaded_runtime_world(world, dir)?;
                if !has_inline_module_artifacts {
                    world.load_module_store_from_dir(dir)?;
                }
                return Ok(world);
            }
            if !has_indexed_generation {
                hydrate_tick_consensus_snapshot_from_archive(dir, &mut snapshot)?;
            }
            let has_inline_module_artifacts = !snapshot.module_artifact_bytes.is_empty();
            let world = Self::from_snapshot(snapshot, journal)?;
            let mut world = Self::recover_loaded_runtime_world(world, dir)?;
            if has_indexed_generation && !has_inline_module_artifacts {
                world.load_selected_generation_module_artifacts_from_dir(dir)?;
            } else if !has_inline_module_artifacts {
                world.load_module_store_from_dir(dir)?;
            }
            return Ok(world);
        }
        let journal_path = dir.join(JOURNAL_FILE);
        let snapshot_path = dir.join(SNAPSHOT_FILE);
        let journal = Journal::load_json(journal_path)?;
        let mut snapshot = Snapshot::load_json(snapshot_path)?;
        hydrate_tick_consensus_snapshot_from_archive(dir, &mut snapshot)?;
        let has_inline_module_artifacts = !snapshot.module_artifact_bytes.is_empty();
        let world = Self::from_snapshot(snapshot, journal)?;
        let mut world = Self::recover_loaded_runtime_world(world, dir)?;
        if !has_inline_module_artifacts {
            world.load_module_store_from_dir(dir)?;
        }
        Ok(world)
    }

    pub fn load_from_dir_with_modules(dir: impl AsRef<Path>) -> Result<Self, WorldError> {
        Self::load_from_dir(dir)
    }

    fn recover_loaded_runtime_world(mut world: Self, dir: &Path) -> Result<Self, WorldError> {
        *world.persistence_dir.borrow_mut() = Some(dir.to_path_buf());
        let cognition_before = world.cognition.clone();
        world.recover_cognition()?;
        // The typed economy is a separate durable projection. Validate it on
        // restore so a malformed lease/receipt prefix cannot enter runtime as
        // an apparently healthy World.
        world.cognition_economy()?;
        if world.cognition != cognition_before {
            world.persist_runtime_transaction_if_configured()?;
        }
        Ok(world)
    }

    pub fn load_module_store_from_dir(&mut self, dir: impl AsRef<Path>) -> Result<(), WorldError> {
        let store = ModuleStore::new(dir);
        if !store.registry_path().exists() {
            return Ok(());
        }
        self.prepare_module_store_load(&store)?.install(self);
        Ok(())
    }

    fn prepare_module_store_load(
        &self,
        store: &ModuleStore,
    ) -> Result<PreparedModuleStoreLoad, WorldError> {
        let registry = store.load_registry()?;
        let mut artifacts = BTreeSet::new();
        let mut artifact_bytes = BTreeMap::new();

        for record in registry.records.values() {
            let wasm_hash = &record.manifest.wasm_hash;
            let meta = store.read_meta(wasm_hash)?;
            if meta != record.manifest {
                return Err(WorldError::ModuleStoreManifestMismatch {
                    wasm_hash: wasm_hash.clone(),
                });
            }
            let bytes = store.read_artifact(wasm_hash)?;
            let actual_hash = super::super::util::sha256_hex(&bytes);
            if actual_hash != *wasm_hash {
                return Err(WorldError::ModuleStoreManifestMismatch {
                    wasm_hash: wasm_hash.clone(),
                });
            }
            self.validate_module_artifact_identity(&record.manifest)?;
            artifacts.insert(wasm_hash.clone());
            artifact_bytes.insert(wasm_hash.clone(), bytes.into());
        }
        Ok(PreparedModuleStoreLoad {
            registry,
            artifacts,
            artifact_bytes,
        })
    }

    fn load_selected_generation_module_artifacts_from_dir(
        &mut self,
        dir: impl AsRef<Path>,
    ) -> Result<(), WorldError> {
        let store = ModuleStore::new(dir);
        if !store.modules_dir().exists() {
            return Ok(());
        }
        self.module_artifact_bytes.clear();

        for record in self.module_registry.records.values() {
            let wasm_hash = &record.manifest.wasm_hash;
            let bytes = store.read_artifact(wasm_hash)?;
            let actual_hash = super::super::util::sha256_hex(&bytes);
            if actual_hash != *wasm_hash {
                return Err(WorldError::ModuleStoreManifestMismatch {
                    wasm_hash: wasm_hash.clone(),
                });
            }
            self.validate_module_artifact_identity(&record.manifest)?;
            self.module_artifact_bytes
                .insert(wasm_hash.clone(), bytes.into());
        }
        Ok(())
    }

    pub fn from_snapshot(snapshot: Snapshot, journal: Journal) -> Result<Self, WorldError> {
        Self::from_snapshot_mode(snapshot, journal, false)
    }

    fn from_snapshot_mode(
        snapshot: Snapshot,
        journal: Journal,
        observer: bool,
    ) -> Result<Self, WorldError> {
        Self::from_snapshot_mode_with_history(snapshot, journal, observer, None)
    }

    /// Restore only an exact qualified formal checkpoint. Its authenticated
    /// cold reader is attached before Tick validation; no queues/journal are
    /// trimmed and no extra journal suffix is replayed.
    pub fn from_verified_formal_snapshot(
        snapshot: Snapshot,
        journal: Journal,
        parent: crate::VerifiedFormalRetainedHistoryParentV1,
        reader:Arc<dyn oasis7_distfs::controlled_authority::formal::business_index::BusinessIndexNodeReader + Send + Sync>,
    ) -> Result<Self, WorldError> {
        Self::from_snapshot_mode_with_history(snapshot, journal, true, Some((parent, reader)))
    }

    fn from_snapshot_mode_with_history(
        snapshot: Snapshot,
        journal: Journal,
        observer: bool,
        history:Option<(crate::VerifiedFormalRetainedHistoryParentV1,Arc<dyn oasis7_distfs::controlled_authority::formal::business_index::BusinessIndexNodeReader + Send + Sync>)>,
    ) -> Result<Self, WorldError> {
        if snapshot.tick_consensus_archived_record_count > 0
            && snapshot.tick_consensus_records.is_empty()
        {
            return Err(WorldError::DistributedValidationFailed {
                reason: "Tick prefix must retain a nonempty hot tail".into(),
            });
        }
        if history.is_none() && snapshot.tick_consensus_archived_record_count > 0 {
            return Err(WorldError::DistributedValidationFailed { reason: "qualified Tick prefix requires typed history reader (legacy sidecar must hydrate first)".into() });
        }
        if (snapshot.tick_consensus_total_record_count != 0
            || snapshot.tick_consensus_archived_record_count != 0)
            && snapshot
                .tick_consensus_archived_record_count
                .checked_add(snapshot.tick_consensus_records.len() as u64)
                != Some(snapshot.tick_consensus_total_record_count)
        {
            return Err(WorldError::DistributedValidationFailed {
                reason: "Tick Snapshot count mismatch".into(),
            });
        }
        if let Some((parent, _)) = &history {
            let bytes = serde_cbor::to_vec(&snapshot).map_err(|e| {
                WorldError::DistributedValidationFailed {
                    reason: e.to_string(),
                }
            })?;
            if blake3::hash(&bytes).to_hex().as_str() != parent.after_snapshot_root()
                || snapshot.state.retained_history_root.as_ref() != Some(parent.root())
                || parent
                    .logical_tick()
                    .is_some_and(|tick| snapshot.state.time != tick)
            {
                return Err(WorldError::DistributedValidationFailed {
                    reason: "formal snapshot differs from verified retained-history parent".into(),
                });
            }
        }
        if observer && snapshot.journal_len != journal.len() {
            return Err(WorldError::DistributedValidationFailed {
                reason: "RecoveryRequired: observer needs a complete checkpoint".to_string(),
            });
        }
        if snapshot.journal_len > journal.len() {
            return Err(WorldError::JournalMismatch);
        }
        validate_journal_event_sequence(&journal)?;
        if snapshot.journal_len > 0
            && journal.events[snapshot.journal_len - 1].id != snapshot.last_event_id
        {
            return Err(WorldError::JournalMismatch);
        }
        if !snapshot.journal_commitment.is_empty() {
            let actual = hash_json(&journal.events[..snapshot.journal_len].to_vec())?;
            if actual != snapshot.journal_commitment {
                return Err(WorldError::JournalMismatch);
            }
        }
        snapshot.rollback_authority_registry.validate()?;
        if let Some(root) = snapshot.state.retained_history_root.as_ref() {
            root.validate()
                .map_err(|reason| WorldError::DistributedValidationFailed {
                    reason: format!("invalid retained-history snapshot descriptor: {reason}"),
                })?;
        }

        let mut world = Self::new_with_state(snapshot.state);
        world.rollback_authority_registry = snapshot.rollback_authority_registry;
        world.consumed_rollback_nonces = snapshot.consumed_rollback_nonces;
        world.rollback_nonce_outcomes = snapshot.rollback_nonce_outcomes;
        world.journal = journal;
        world.manifest = snapshot.manifest;
        world.cognition = snapshot.cognition;
        // Every reconstruction path, including direct checkpoint/replay
        // installation, must validate the typed economy before exposing the
        // candidate world. Directory recovery performs the same check after
        // cognition projection repair; this call closes the public restore
        // path that bypasses that post-load hook.
        world.cognition_economy()?;
        world.module_registry = snapshot.module_registry;
        world.module_artifacts = snapshot.module_artifacts;
        let module_artifact_bytes = snapshot.module_artifact_bytes;
        for (wasm_hash, bytes) in &module_artifact_bytes {
            let actual_hash = super::super::util::sha256_hex(bytes);
            if actual_hash != *wasm_hash {
                return Err(WorldError::ModuleChangeInvalid {
                    reason: format!(
                        "snapshot module artifact hash mismatch expected {wasm_hash} found {actual_hash}"
                    ),
                });
            }
        }
        if !module_artifact_bytes.is_empty() {
            for record in world.module_registry.records.values() {
                if !module_artifact_bytes.contains_key(&record.manifest.wasm_hash) {
                    return Err(WorldError::ModuleStoreArtifactMissing {
                        wasm_hash: record.manifest.wasm_hash.clone(),
                    });
                }
            }
        }
        world.module_artifact_bytes = module_artifact_bytes
            .into_iter()
            .map(|(wasm_hash, bytes)| (wasm_hash, bytes.into()))
            .collect();
        world.module_cache = ModuleCache::default();
        world.module_limits_max = snapshot.module_limits_max;
        world.snapshot_catalog = snapshot.snapshot_catalog;
        world.chain_resource_manifest = snapshot.chain_resource_manifest;
        world.latest_chain_resource_delta = snapshot.latest_chain_resource_delta;
        if snapshot.journal_len > 0 && snapshot.last_event_id == u64::MAX {
            // A checkpoint whose retained prefix ends at the rolling maximum
            // resumes in the next event-id era.  Keep the legacy synthetic
            // empty-journal fixture behavior (first allocation is MAX), but
            // make a real rollover tail replayable and deterministic.
            world.next_event_id = 1;
            world.next_event_id_era = snapshot.event_id_era;
        } else {
            world.next_event_id = snapshot.last_event_id.saturating_add(1).max(1);
            world.next_event_id_era = snapshot.event_id_era;
        }
        world.next_action_id = snapshot.next_action_id.max(1);
        world.next_action_id_era = snapshot.action_id_era;
        world.next_intent_id = snapshot.next_intent_id.max(1);
        world.next_intent_id_era = snapshot.intent_id_era;
        world.next_proposal_id = snapshot.next_proposal_id.max(1);
        world.next_proposal_id_era = snapshot.proposal_id_era;
        world.pending_actions = VecDeque::from(snapshot.pending_actions);
        world.pending_effects = VecDeque::from(snapshot.pending_effects);
        world.inflight_effects = snapshot.inflight_effects;
        world.module_tick_schedule = snapshot.module_tick_schedule;
        world.module_tick_routing_metrics = ModuleTickRoutingMetrics::from_deterministic_snapshot(
            snapshot.module_tick_routing_metrics,
        );
        world.capabilities = snapshot.capabilities;
        world.capability_grants_v2 = snapshot.capability_grants_v2;
        world.capability_revocation_state = snapshot.capability_revocation_state;
        // Result receipts are excluded from the authorization root; validate
        // their immutable signed identities before accepting a checkpoint.
        for (key, result) in &world.capability_revocation_state.world_service_results {
            crate::service_admission::correlation::validate_result(key, result)
                .map_err(|reason| super::capability_authorization::deny(&reason))?;
        }
        world.capability_nonce_records = snapshot.capability_nonce_records;
        world.capability_authorization_receipts = snapshot.capability_authorization_receipts;
        world.capability_invocation_contexts = snapshot.capability_invocation_contexts;
        world.capability_authorization_root = snapshot.capability_authorization_root;
        world.capability_budget_accounts = snapshot.capability_budget_accounts;
        world.capability_effect_receipt_links = snapshot.capability_effect_receipt_links;
        if world.capability_authorization_root.is_empty() {
            let has_v2_authorization_state = !world.capability_grants_v2.is_empty()
                || world.capability_revocation_state != CapabilityRevocationState::default()
                || !world.capability_nonce_records.is_empty()
                || !world.capability_authorization_receipts.is_empty()
                || !world.capability_invocation_contexts.is_empty()
                || !world.capability_budget_accounts.is_empty()
                || !world.capability_effect_receipt_links.is_empty();
            if has_v2_authorization_state {
                return Err(super::capability_authorization::deny(
                    "capability authorization root is required when v2 state is populated",
                ));
            }
            world.refresh_capability_authorization_root()?;
        } else {
            world.verify_capability_authorization_root()?;
        }
        world.policies = snapshot.policies;
        world.proposals = snapshot.proposals;
        world.scheduler_cursor = snapshot.scheduler_cursor;
        world.runtime_memory_limits = snapshot.runtime_memory_limits;
        world.runtime_backpressure_stats = snapshot.runtime_backpressure_stats;
        world.formal_archived_tick_consensus_count = snapshot.tick_consensus_archived_record_count;
        world.tick_consensus_records = snapshot.tick_consensus_records;
        world.tick_consensus_authority_source =
            snapshot.tick_consensus_authority_source.trim().to_string();
        if world.tick_consensus_authority_source.is_empty() {
            world.tick_consensus_authority_source =
                super::BUILTIN_MODULE_SIGNER_NODE_ID.to_string();
        }
        world.tick_consensus_rejection_audit_events =
            snapshot.tick_consensus_rejection_audit_events;
        world.governance_execution_policy = snapshot.governance_execution_policy;
        world.governance_finality_epoch_snapshots = snapshot.governance_finality_epoch_snapshots;
        Self::validate_governance_execution_policy(&world.governance_execution_policy)?;
        world.governance_emergency_brake_until_tick =
            snapshot.governance_emergency_brake_until_tick;
        world.governance_identity_penalties = snapshot.governance_identity_penalties;
        world.next_governance_identity_penalty_id =
            snapshot.next_governance_identity_penalty_id.max(1);
        for (issuer_id, record) in &world.capability_revocation_state.authority_records {
            let proof = world
                .capability_revocation_state
                .authority_finality_proofs
                .get(issuer_id)
                .ok_or_else(|| {
                    super::capability_authorization::deny(
                        "authority record is missing its replayable finality proof",
                    )
                })?;
            if proof.binding.issuer_id != *issuer_id {
                return Err(super::capability_authorization::deny(
                    "authority finality proof issuer binding is invalid",
                ));
            }
            world.verify_capability_authority_finality_proof(record, proof)?;
        }
        // Hydrate the complete checkpoint before checking the exact qualified
        // parent. Task-chain validation may need cold predecessors immediately.
        if let Some((parent, reader)) = history {
            world
                .attach_verified_formal_history(parent, reader)
                .map_err(|reason| WorldError::DistributedValidationFailed { reason })?;
        }
        crate::service_admission::game_task::validate_history(&world)
            .map_err(|reason| super::capability_authorization::deny(&reason))?;
        if observer {
            let limits = &world.runtime_memory_limits;
            if world.pending_actions.len() > limits.max_pending_actions.max(1)
                || world.pending_effects.len() > limits.max_pending_effects.max(1)
                || world.inflight_effects.len() > limits.max_inflight_effects.max(1)
                || world.journal.len() > limits.max_journal_events.max(1)
            {
                return Err(WorldError::DistributedValidationFailed {
                    reason: "ResourceLimited: observer cannot trim checkpoint queues".to_string(),
                });
            }
        } else {
            world.enforce_pending_action_limit();
            world.enforce_pending_effect_limit();
            world.enforce_inflight_effect_limit();
            let has_journal_tail = snapshot.journal_len < world.journal.len();
            world.replay_from(snapshot.journal_len)?;
            if has_journal_tail {
                // Individually valid tail receipts may still consume a missing
                // or already-consumed task predecessor. Validate the replayed
                // chain before publishing the recovered world or trimming its
                // journal. With no tail, the pre-replay history is unchanged.
                crate::service_admission::game_task::validate_history(&world)
                    .map_err(|reason| super::capability_authorization::deny(&reason))?;
            }
            world.enforce_journal_event_limit();
        }
        world.verify_tick_consensus_chain()?;
        Ok(world)
    }

    pub(super) fn save_distfs_sidecar(
        &self,
        dir: &Path,
        snapshot: &Snapshot,
        tick_consensus_archive_bytes: Option<&[u8]>,
        recovery_metadata: Option<&[u8]>,
    ) -> Result<(), WorldError> {
        let inherited_recovery_metadata = if recovery_metadata.is_none() {
            Self::load_authoritative_recovery_metadata(dir)?
        } else {
            None
        };
        let recovery_metadata = recovery_metadata.or(inherited_recovery_metadata.as_deref());
        let store_root = dir.join(DISTFS_STATE_DIR);
        fs::create_dir_all(store_root.as_path())?;
        let store = LocalCasStore::new(store_root.as_path());
        let config = SegmentConfig::default();
        let world_id = distfs_world_id(dir);
        let epoch = snapshot.state.time;
        let manifest = segment_snapshot(snapshot, world_id.as_str(), epoch, &store, config)?;
        let journal_segments = segment_journal(&self.journal, &store, config)?;

        let restored_snapshot: Snapshot = assemble_snapshot(&manifest, &store)?;
        if restored_snapshot != *snapshot {
            return Err(WorldError::DistributedValidationFailed {
                reason: "distfs snapshot assemble verification mismatch".to_string(),
            });
        }

        let restored_events: Vec<WorldEvent> =
            assemble_journal(&journal_segments, &store, |event: &WorldEvent| event.id)?;
        if restored_events != self.journal.events {
            return Err(WorldError::DistributedValidationFailed {
                reason: "distfs journal assemble verification mismatch".to_string(),
            });
        }

        persist_sidecar_generation_index(
            store_root.as_path(),
            &manifest,
            journal_segments.as_slice(),
            tick_consensus_archive_bytes,
            recovery_metadata,
        )?;
        let snapshot_manifest_path = dir.join(DISTFS_SNAPSHOT_MANIFEST_FILE);
        let journal_segments_path = dir.join(DISTFS_JOURNAL_SEGMENTS_FILE);
        write_json_to_path(&manifest, snapshot_manifest_path.as_path())?;
        write_json_to_path(&journal_segments, journal_segments_path.as_path())?;
        Ok(())
    }
}

fn validate_journal_event_sequence(journal: &Journal) -> Result<(), WorldError> {
    for pair in journal.events.windows(2) {
        let previous = pair[0].id;
        let expected = if previous == u64::MAX {
            1
        } else {
            previous.saturating_add(1)
        };
        if pair[1].id != expected {
            return Err(WorldError::JournalMismatch);
        }
    }
    Ok(())
}
