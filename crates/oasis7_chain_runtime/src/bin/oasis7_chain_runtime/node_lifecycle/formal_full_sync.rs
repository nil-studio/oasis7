//! Full-state read-only sync from an independently qualified cut. Public
//! observer projections do not expose private snapshots; requests need an
//! issuer grant and node signature, and the client verifies each cut.
//! Content-addressed blobs are atomic; proof history and the last verified World
//! remain pinned until a fully restored successor is ready.

use oasis7::{
    runtime::{
        BlobStore, Journal, LocalCasStore, Snapshot, VerifiedFormalRetainedHistoryParentV1, World,
    },
    world_service::{
        authority,
        client::{FormalReadTrustConfigV1, RemoteWorldServiceClient, WorldServicePort},
        verified_view::VerifiedWorldView,
        wire::SignedServiceResponse,
    },
};
use oasis7_client_api::world_service::{
    ReadWorldViewRequest, WORLD_SERVICE_CONTRACT_VERSION, WorldIdentity,
};
use oasis7_client_api::world_service_full_sync::{
    FORMAL_FULL_SYNC_MANIFEST_PATH, FORMAL_FULL_SYNC_OBJECT_PATH,
    FormalFullSyncArtifactRoleV1 as WireArtifactRole, FormalFullSyncCutV1,
    FormalFullSyncManifestRequestV1, FormalFullSyncManifestV1, FormalFullSyncObjectChunkV1,
    FormalFullSyncObjectRequestV1, FormalNodeReadGrantV1, FormalNodeReadRoleV1,
    MAX_FULL_SYNC_CHUNK_BYTES, MAX_FULL_SYNC_RECORD_BYTES,
};
use oasis7_distfs::controlled_authority::{
    formal::{
        VerifiedFormalReadAnchorV1,
        business_index::{
            BusinessIndexNodeKindV1, BusinessIndexNodeReader, BusinessIndexNodeV1,
            BusinessIndexRootV1,
        },
        verify_record_material,
    },
    replicated_protocol::{ArtifactObject, ArtifactRole, ClosedRecord},
};
use reqwest::blocking::Client as HttpClient;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{
    io::Read,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};
#[path = "formal_full_sync_cas.rs"]
pub(super) mod cas_lifecycle;
#[path = "formal_full_sync_compatibility.rs"]
mod compatibility;
#[path = "formal_full_sync_failover.rs"]
mod failover;
#[path = "formal_full_sync_grant_reconcile.rs"]
mod grant_reconcile;
#[path = "formal_full_sync_history.rs"]
pub(super) mod history_chain;
#[path = "formal_full_sync_native.rs"]
mod native_environment;
#[path = "formal_full_sync_providers.rs"]
mod providers;
use cas_lifecycle::CasBudget;
pub(super) use compatibility::probe_runtime_compatibility;
pub(crate) use providers::FormalFullSyncConfigV1;
#[cfg(test)]
pub(crate) use providers::FormalFullSyncEndpointV1;
#[cfg(test)]
use providers::MAX_SYNC_PROVIDERS;

const API_VERSION: &str = "oasis7.formal-full-state-sync/v1";
pub(crate) const PROFILE: &str = "formal_full_state_sync_read_only_v1";
pub(super) const STATUS_FILE: &str = "formal-full-state-sync.json";
const MAX_CATCHUP_PAGES_PER_SYNC: usize = 64;

pub(super) fn validate_grant_for_node(
    config: &FormalFullSyncConfigV1,
    grant: &FormalNodeReadGrantV1,
    node_public_key: &str,
    now_unix_ms: u64,
) -> Result<(), String> {
    config.validate()?;
    let external_anchor = config.formal_read_trust.verified_anchor()?;
    let authority_floor = external_anchor.authority().definition().epoch;
    if grant.body.authority_epoch < authority_floor {
        return Err("full-sync grant authority epoch is below the externally pinned floor".into());
    }
    grant.verify(
        &config.formal_read_trust.initial_trust.issuer_public_key,
        &WorldIdentity {
            world_id: config.world_id.clone(),
            genesis_digest: config.genesis_digest.clone(),
        },
        &config.chain_id,
        node_public_key,
        &config.expected_role,
        grant.body.authority_epoch,
        now_unix_ms,
    )
}

pub(super) fn install_grant(
    data_dir: &Path,
    grant_path: &Path,
) -> Result<serde_json::Value, String> {
    let config_path = data_dir.join(super::model::CONFIG_FILE);
    let config_path = super::local_io::regular(&config_path)?;
    let node = super::model::NodeConfig::load(&config_path)?;
    node.verify_identity()?;
    let instance_data = node.identity.data_dir.clone();
    let _admission = super::process::admission_lock(&instance_data)?;
    let _ownership = super::process::lock(&instance_data)?;
    let current = super::process::read_instance(&instance_data)?;
    if current
        .as_ref()
        .is_some_and(|instance| !matches!(instance.state.as_str(), "stopped" | "failed"))
    {
        return Err("full-sync grant changes require the exact node to be stopped".into());
    }
    grant_reconcile::require_completed_start(&instance_data, current.as_ref())?;
    let mut node = super::model::NodeConfig::load(&config_path)?;
    node.verify_identity()?;
    let previous_config = node
        .identity
        .chain
        .formal_full_sync()
        .cloned()
        .ok_or("full-sync grant can only be installed for a formal full-state sync node")?;
    let grant: FormalNodeReadGrantV1 = super::local_io::read(grant_path)?;
    let (keypair, _) =
        super::super::node_keypair_config::read_existing_node_keypair_in_secure_config_dir(
            &instance_data.join("keys"),
        )?;
    let now = now_unix_ms().ok_or("system clock before Unix epoch")?;
    validate_grant_for_node(&previous_config, &grant, &keypair.public_key_hex, now)?;
    if let Some(previous) = &previous_config.grant {
        if grant != *previous
            && (grant.body.grant_epoch <= previous.body.grant_epoch
                || grant.body.authority_epoch < previous.body.authority_epoch)
        {
            return Err(
                "replacement full-sync grant must advance its grant epoch and authority floor"
                    .into(),
            );
        }
        if grant == *previous {
            return Ok(serde_json::json!({
                "installed": true,
                "changed": false,
                "grant_epoch": grant.body.grant_epoch,
                "authority_epoch": grant.body.authority_epoch,
                "config_digest": super::local_io::digest(&node)?
            }));
        }
    }
    node.identity
        .chain
        .formal_full_sync_mut()
        .ok_or("full-sync profile disappeared during grant install")?
        .grant = Some(grant.clone());
    super::local_io::write(&instance_data.join(super::model::CONFIG_FILE), &node)?;
    Ok(serde_json::json!({
        "installed": true,
        "changed": true,
        "grant_epoch": grant.body.grant_epoch,
        "authority_epoch": grant.body.authority_epoch,
        "config_digest": super::local_io::digest(&node)?
    }))
}

impl PartialEq for FormalFullSyncConfigV1 {
    fn eq(&self, other: &Self) -> bool {
        self.api_version == other.api_version
            && self.endpoint == other.endpoint
            && self.trusted_service_public_key == other.trusted_service_public_key
            && self.alternate_endpoints == other.alternate_endpoints
            && self.world_id == other.world_id
            && self.chain_id == other.chain_id
            && self.genesis_digest == other.genesis_digest
            && self.formal_read_trust.initial_trust == other.formal_read_trust.initial_trust
            && self.formal_read_trust.checkpoint == other.formal_read_trust.checkpoint
            && self.formal_read_trust.checkpoint_digest == other.formal_read_trust.checkpoint_digest
            && self.grant == other.grant
            && self.expected_role == other.expected_role
            && self.poll_interval_ms == other.poll_interval_ms
            && self.request_timeout_ms == other.request_timeout_ms
            && self.max_total_sync_bytes == other.max_total_sync_bytes
    }
}
impl Eq for FormalFullSyncConfigV1 {}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FormalFullSyncStatusV1 {
    pub api_version: String,
    pub runtime_profile: String,
    pub state: String,
    pub ready: bool,
    pub can_propose: bool,
    pub state_root_verified: bool,
    pub world_loaded: bool,
    pub sync_scope: String,
    pub world_id: String,
    pub chain_id: String,
    pub node_public_key: String,
    pub incarnation: String,
    pub generation: u64,
    pub config_digest: String,
    pub grant_epoch: Option<u64>,
    /// Latest cut independently verified by the ordinary formal reader.
    pub last_verified_cut: Option<FormalFullSyncCutV1>,
    /// Exact Snapshot/Journal cut restored; it may lag the verified reader.
    #[serde(default)]
    pub loaded_cut: Option<FormalFullSyncCutV1>,
    #[serde(default)]
    pub lag_positions: Option<u64>,
    #[serde(default)]
    pub catch_up_pending: bool,
    pub last_checked_unix_ms: Option<u64>,
    pub error: Option<String>,
}

/// Reconstructed read-only World with no producer or P/R signing identity.
pub(crate) struct RestoredFormalFullSyncWorldV1 {
    pub world: World,
    pub cut: FormalFullSyncCutV1,
    retained_cas_hashes: std::collections::BTreeSet<String>,
}

pub(crate) struct FormalFullSyncSession {
    config: FormalFullSyncConfigV1,
    client: RemoteWorldServiceClient,
    http: HttpClient,
    active_provider_index: usize,
    node_private_key_hex: String,
    node_public_key_hex: String,
    cas: Arc<LocalCasStore>,
    cas_integrity_poisoned: Arc<AtomicBool>,
    history: history_chain::PersistedFormalReadHistoryV1,
    cas_publication_poisoned: bool,
    status: FormalFullSyncStatusV1,
}

#[cfg(test)]
#[path = "formal_full_sync_tests.rs"]
mod tests;

impl FormalFullSyncSession {
    /// Construct from an already loaded, privately held node identity. The
    /// grant must bind that key; local process ownership alone grants nothing.
    pub(crate) fn new(
        config: FormalFullSyncConfigV1,
        node_private_key_hex: String,
        cas_root: &Path,
        instance: &super::LocalInstance,
    ) -> Result<Self, String> {
        config.validate()?;
        let signed = authority::sign_read_request(
            "formal-full-sync-node-key-check",
            (),
            &node_private_key_hex,
        )?;
        if config
            .grant
            .as_ref()
            .is_some_and(|grant| signed.subject_public_key != grant.body.node_public_key)
        {
            return Err("full-sync node key differs from issuer grant".into());
        }
        super::local_io::private_dir(cas_root)?;
        let providers = config.providers();
        let active_provider = providers
            .first()
            .ok_or("full-sync has no configured provider")?;
        let timeout = Duration::from_millis(config.request_timeout_ms);
        let client =
            providers::build_read_client(&config, active_provider, &node_private_key_hex, timeout)?;
        let http = HttpClient::builder()
            .connect_timeout(timeout)
            .timeout(timeout)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|error| error.to_string())?;
        let node_public_key_hex = signed.subject_public_key;
        let cas = Arc::new(LocalCasStore::new(cas_root));
        let cas_integrity_poisoned = Arc::new(AtomicBool::new(false));
        let history = history_chain::PersistedFormalReadHistoryV1::open(
            &config,
            &client,
            Arc::clone(&cas),
            cas_root,
        )?;
        cas_lifecycle::stabilize_loaded_world_pins(&cas)?;
        cas_lifecycle::prune_unpinned(&cas)?;
        let status = FormalFullSyncStatusV1 {
            api_version: "oasis7.formal-full-sync-status/v1".into(),
            runtime_profile: PROFILE.into(),
            state: if config.grant.is_some() {
                "starting"
            } else {
                "blocked_missing_issuer_grant"
            }
            .into(),
            ready: false,
            can_propose: false,
            state_root_verified: false,
            world_loaded: false,
            sync_scope: "issuer_granted_full_state_read_only".into(),
            world_id: config.world_id.clone(),
            chain_id: config.chain_id.clone(),
            node_public_key: node_public_key_hex.clone(),
            incarnation: instance.incarnation.clone(),
            generation: instance.generation,
            config_digest: instance.config_digest.clone(),
            grant_epoch: config.grant.as_ref().map(|grant| grant.body.grant_epoch),
            last_verified_cut: None,
            loaded_cut: None,
            lag_positions: None,
            catch_up_pending: false,
            last_checked_unix_ms: None,
            error: None,
        };
        let mut session = Self {
            config,
            client,
            http,
            active_provider_index: 0,
            node_private_key_hex,
            node_public_key_hex,
            cas,
            cas_integrity_poisoned,
            history,
            cas_publication_poisoned: false,
            status,
        };
        session.status.last_verified_cut = session.history.last_verified_cut().cloned();
        Ok(session)
    }

    pub(crate) fn status(&self) -> &FormalFullSyncStatusV1 {
        &self.status
    }

    pub(crate) fn poll_interval(&self) -> Duration {
        Duration::from_millis(self.config.poll_interval_ms)
    }

    pub(crate) fn begin_poll(&mut self) {
        self.status.ready = false;
        self.status.can_propose = false;
        self.status.state_root_verified = false;
        self.status.world_loaded = false;
        self.status.loaded_cut = None;
        self.status.lag_positions = None;
        self.status.catch_up_pending = false;
        self.status.state = if self.config.grant.is_some() {
            "syncing"
        } else {
            "blocked_missing_issuer_grant"
        }
        .into();
        // Preserve the last observed failure while a retry is in flight. A
        // successful sync below is the only path that clears this field.
        self.status.last_checked_unix_ms = now_unix_ms();
    }

    pub(crate) fn set_world_loaded(&mut self, loaded: bool) {
        self.status.world_loaded = loaded;
        if !loaded {
            self.status.ready = false;
        }
    }

    /// Cold-restores the qualified snapshot; a failed poll withdraws readiness.
    pub(crate) fn sync_latest(&mut self) -> Result<RestoredFormalFullSyncWorldV1, String> {
        if self.config.grant.is_none() {
            self.status.ready = false;
            self.status.can_propose = false;
            self.status.state_root_verified = false;
            self.status.world_loaded = false;
            self.status.loaded_cut = None;
            self.status.lag_positions = None;
            self.status.catch_up_pending = false;
            self.status.state = "blocked_missing_issuer_grant".into();
            self.status.error = Some("issuer-signed full-state read grant is not installed".into());
            self.status.last_checked_unix_ms = now_unix_ms();
            return Err("issuer-signed full-state read grant is not installed".into());
        }
        if self.history.is_poisoned() {
            self.fail(
                "history checkpoint persistence failed; restart from the durable proof-page chain",
            );
            return Err(
                "HistoryUnavailable: full-sync verifier advanced without a durable page pointer"
                    .into(),
            );
        }
        if self.cas_publication_poisoned {
            self.fail(
                "full-sync CAS root publication had an uncertain durability outcome; restart to rebuild roots before garbage collection",
            );
            return Err(
                "HistoryUnavailable: full-sync CAS root publication is poisoned; restart required"
                    .into(),
            );
        }
        if self.cas_integrity_poisoned.load(Ordering::Acquire) {
            self.fail(
                "local full-sync CAS integrity or I/O failure; repair the local store before retrying",
            );
            return Err(
                "HistoryUnavailable: local full-sync CAS integrity/I/O failure is poisoned; provider failover is withheld".into(),
            );
        }
        let previously_verified_cut = self
            .history
            .last_verified_cut()
            .cloned()
            .or_else(|| self.status.last_verified_cut.clone());
        self.status.ready = false;
        self.status.can_propose = false;
        self.status.state_root_verified = false;
        self.status.world_loaded = false;
        self.status.loaded_cut = None;
        self.status.lag_positions = None;
        self.status.catch_up_pending = false;
        self.status.state = "syncing".into();
        self.status.last_checked_unix_ms = now_unix_ms();
        if let Err(error) = cas_lifecycle::prune_unpinned(&self.cas) {
            self.fail(&error);
            return Err(error);
        }
        let result = self.sync_across_providers();
        match result {
            Ok(restored) => {
                let latest = self
                    .history
                    .last_verified_cut()
                    .cloned()
                    .unwrap_or_else(|| restored.cut.clone());
                if previously_verified_cut
                    .as_ref()
                    .is_some_and(|previous| !nondecreasing_cut(previous, &latest))
                    || !nondecreasing_cut(&restored.cut, &latest)
                {
                    let error = "restored full-sync cut is not on the verified read history";
                    self.fail(error);
                    let _ = cas_lifecycle::cleanup_after_failed_candidate(
                        &self.cas,
                        self.history.is_poisoned()
                            || self.cas_publication_poisoned
                            || self.cas_integrity_poisoned.load(Ordering::Acquire),
                    );
                    return Err(error.into());
                }
                if let Err(error) = cas_lifecycle::replace_loaded_world_pins(
                    &self.cas,
                    &restored.retained_cas_hashes,
                ) {
                    self.cas_publication_poisoned = error.outcome_unknown();
                    let error = error.to_string();
                    self.fail(&error);
                    let cleanup = cas_lifecycle::cleanup_after_failed_candidate(
                        &self.cas,
                        self.cas_publication_poisoned
                            || self.history.is_poisoned()
                            || self.cas_integrity_poisoned.load(Ordering::Acquire),
                    );
                    return Err(match cleanup {
                        Ok(()) => error,
                        Err(cleanup_error) => format!(
                            "{error}; full-sync orphan cleanup also failed: {cleanup_error}"
                        ),
                    });
                }
                let lag = latest.position.saturating_sub(restored.cut.position);
                self.status.last_verified_cut = Some(latest);
                self.status.loaded_cut = Some(restored.cut.clone());
                self.status.lag_positions = Some(lag);
                self.status.catch_up_pending = lag > 0;
                self.status.state = "qualified_full_state_read_only".into();
                self.status.ready = true;
                self.status.state_root_verified = true;
                self.status.error = None;
                Ok(restored)
            }
            Err(error) => {
                let cleanup = cas_lifecycle::cleanup_after_failed_candidate(
                    &self.cas,
                    self.history.is_poisoned()
                        || self.cas_publication_poisoned
                        || self.cas_integrity_poisoned.load(Ordering::Acquire),
                );
                let error = match cleanup {
                    Ok(()) => error,
                    Err(cleanup_error) => {
                        format!("{error}; full-sync orphan cleanup also failed: {cleanup_error}")
                    }
                };
                self.fail(&error);
                Err(error)
            }
        }
    }

    fn sync_latest_inner(&mut self) -> Result<RestoredFormalFullSyncWorldV1, String> {
        // Try to catch up through a bounded number of ordinary proof pages,
        // but keep the last verified page if the publisher keeps advancing.
        let view = self.read_best_effort_current_view()?;
        let anchor = verified_anchor(&view, &self.config)?;
        let cut = cut_from_anchor(&anchor)?;
        self.config
            .grant
            .as_ref()
            .ok_or("full-sync issuer grant missing")?
            .verify(
                &self
                    .config
                    .formal_read_trust
                    .initial_trust
                    .issuer_public_key,
                &cut.world,
                &cut.chain_id,
                &self.node_public_key_hex,
                &self.config.expected_role,
                cut.authority_epoch,
                now_unix_ms().ok_or("system clock before Unix epoch")?,
            )?;

        let manifest = self.fetch_manifest(&cut)?;
        let mut budget = CasBudget::new(self.config.max_total_sync_bytes);
        let material = self.fetch_record_closure(&anchor, &cut, &manifest, &mut budget)?;
        let _snapshot_hash = material
            .roots
            .get(&ArtifactRole::Snapshot)
            .ok_or("verified record has no Snapshot artifact")?;
        let journal_hash = material
            .roots
            .get(&ArtifactRole::Journal)
            .ok_or("verified record has no Journal artifact")?;
        let expanded_snapshot = material.snapshot_bytes().map_err(|e| e.to_string())?;
        let snapshot_bytes = expanded_snapshot.as_slice();
        let journal_bytes = object_bytes(&material, journal_hash)?;
        if blake3::hash(snapshot_bytes).to_hex().as_str() != cut.after_state_root {
            return Err("full-sync Snapshot does not match qualified after_state_root".into());
        }
        let snapshot: Snapshot = serde_cbor::from_slice(snapshot_bytes)
            .map_err(|error| format!("full-sync Snapshot decode: {error}"))?;
        let journal: Journal = serde_cbor::from_slice(journal_bytes)
            .map_err(|error| format!("full-sync Journal decode: {error}"))?;
        if serde_cbor::to_vec(&snapshot).map_err(|error| error.to_string())? != snapshot_bytes
            || serde_cbor::to_vec(&journal).map_err(|error| error.to_string())? != journal_bytes
        {
            return Err("full-sync Snapshot or Journal is not canonical CBOR".into());
        }
        let retained = snapshot
            .state
            .retained_history_root
            .as_ref()
            .ok_or("full-state sync requires a retained-history root")?;
        if manifest.retained_history_root_digest.as_deref() != Some(&retained.root_digest) {
            return Err("full-sync manifest retained root differs from qualified Snapshot".into());
        }
        let parent = VerifiedFormalRetainedHistoryParentV1::verify(&anchor, snapshot_bytes)?;
        let tree_leaves = self.fetch_retained_tree(
            &cut,
            &retained.root_digest,
            &retained.index_root,
            &mut budget,
        )?;
        if tree_leaves != retained.index_root.leaf_count {
            return Err("full-sync retained tree leaf count mismatch".into());
        }
        let reader = Arc::new(LocalVerifiedBusinessIndexReader {
            store: Arc::clone(&self.cas),
            integrity_poisoned: Arc::clone(&self.cas_integrity_poisoned),
        });
        let world = World::from_verified_formal_snapshot(snapshot, journal, parent, reader)
            .map_err(|error| format!("full-sync cold restore: {error:?}"))?;
        let restored_bytes = serde_cbor::to_vec(&world.snapshot())
            .map_err(|error| format!("full-sync restored Snapshot encode: {error}"))?;
        if blake3::hash(&restored_bytes).to_hex().as_str() != cut.after_state_root {
            return Err("cold-restored World differs from qualified after_state_root".into());
        }
        // Read one more independently verified page after transfer. The
        // ordinary reader verifier has advanced from `cut`, so same-epoch
        // writes are a proven descendant. Keep this immutable snapshot
        // read-only and report the lag; the next poll installs a newer cut.
        let after = self.read_best_effort_current_view()?;
        let after_anchor = verified_anchor(&after, &self.config)?;
        let after_cut = cut_from_anchor(&after_anchor)?;
        if !nondecreasing_cut(&cut, &after_cut) {
            return Err(
                "verified head moved backwards or forked during full-state transfer".into(),
            );
        }
        if after_cut.authority_epoch != cut.authority_epoch
            || after_cut.active_authority_id != cut.active_authority_id
        {
            return Err(
                "formal full-sync grant epoch changed during transfer; install a current grant"
                    .into(),
            );
        }
        Ok(RestoredFormalFullSyncWorldV1 {
            world,
            cut,
            retained_cas_hashes: budget.objects,
        })
    }

    fn fetch_record_closure(
        &self,
        anchor: &VerifiedFormalReadAnchorV1,
        cut: &FormalFullSyncCutV1,
        manifest: &FormalFullSyncManifestV1,
        budget: &mut CasBudget,
    ) -> Result<ClosedRecord, String> {
        let record = anchor.head().record();
        if manifest.artifact_roots.len() != record.artifact_roots.len()
            || manifest.artifact_roots.iter().any(|root| {
                record.artifact_roots.get(&native_role(root.role)) != Some(&root.content_hash)
            })
        {
            return Err("full-sync roots differ from the qualified canonical record".into());
        }
        let declared_bytes = manifest.objects.iter().try_fold(0u64, |total, object| {
            total
                .checked_add(object.size_bytes)
                .ok_or("full-sync size overflow")
        })?;
        if declared_bytes > self.config.max_total_sync_bytes {
            return Err("full-sync record closure exceeds configured transfer bound".into());
        }
        let mut objects = Vec::with_capacity(manifest.objects.len());
        for descriptor in &manifest.objects {
            let bytes = if let Some(existing) = self.read_cached_object(&descriptor.content_hash)? {
                existing
            } else {
                self.fetch_object(
                    cut,
                    &descriptor.content_hash,
                    None,
                    None,
                    Some(descriptor.size_bytes),
                )?
            };
            if bytes.len() as u64 != descriptor.size_bytes
                || blake3::hash(&bytes).to_hex().as_str() != descriptor.content_hash
            {
                return Err("full-sync record artifact size/hash mismatch".into());
            }
            budget.add(&descriptor.content_hash, descriptor.size_bytes)?;
            objects.push(ArtifactObject {
                content_hash: descriptor.content_hash.clone(),
                bytes,
                references: descriptor.references.clone(),
            });
        }
        let material = ClosedRecord {
            payload_digest: record.payload_digest.clone(),
            before_state_root: record.before_state_root.clone(),
            after_state_root: record.after_state_root.clone(),
            roots: record.artifact_roots.clone(),
            objects,
        };
        verify_record_material(record, &material).map_err(|error| error.to_string())?;
        self.fetch_native_environment(cut, &material, budget)?;
        for object in &material.objects {
            self.persist_cas_object(&object.content_hash, &object.bytes)
                .map_err(|error| format!("persist full-sync artifact: {error}"))?;
        }
        Ok(material)
    }

    /// A missing cache entry is a normal miss and is fetched from the active
    /// provider. A present but unreadable or hash-invalid entry is local
    /// integrity evidence; it poisons the session so another provider cannot
    /// make the same damaged local state appear healthy.
    fn read_cached_object(&self, digest: &str) -> Result<Option<Vec<u8>>, String> {
        let result = verified_local_object(&self.cas, digest);
        if result.is_err() {
            self.cas_integrity_poisoned.store(true, Ordering::Release);
        }
        result
    }

    fn persist_cas_object(&self, digest: &str, bytes: &[u8]) -> Result<(), String> {
        match self.cas.put(digest, bytes) {
            Ok(()) => Ok(()),
            Err(error) => {
                self.cas_integrity_poisoned.store(true, Ordering::Release);
                Err(format!("{error:?}"))
            }
        }
    }

    fn read_current_view(&mut self) -> Result<VerifiedWorldView, String> {
        let view = self
            .client
            .read_view(ReadWorldViewRequest {
                observation_nonce: None,
                formal_anchor: None,
                contract_version: WORLD_SERVICE_CONTRACT_VERSION,
                world: WorldIdentity {
                    world_id: self.config.world_id.clone(),
                    genesis_digest: self.config.genesis_digest.clone(),
                },
                scope_id: "public".into(),
                min_commit: None,
                fixed_commit: None,
                deadline_unix_ms: None,
            })
            .map_err(|error| format!("independent current formal read: {error}"))?;
        let anchor = verified_anchor(&view, &self.config)?;
        // An endpoint-specific view can be validly rejected by the local
        // durable-cursor checks. Another provider must start from a fresh
        // verifier replayed from the unchanged durable chain. Persistence
        // uncertainty itself marks the history poisoned in the writer.
        self.history
            .persist_verified_view(&self.config, &self.client, &view, &anchor)?;
        Ok(view)
    }

    fn read_best_effort_current_view(&mut self) -> Result<VerifiedWorldView, String> {
        let mut previous_cut = None;
        let mut latest_view = None;
        for _ in 0..MAX_CATCHUP_PAGES_PER_SYNC {
            let view = self.read_current_view()?;
            let cut = cut_from_anchor(&verified_anchor(&view, &self.config)?)?;
            let unchanged = previous_cut
                .as_ref()
                .is_some_and(|previous| previous == &cut);
            previous_cut = Some(cut);
            latest_view = Some(view);
            if unchanged {
                break;
            }
        }
        latest_view.ok_or_else(|| "HistoryUnavailable: no verified full-sync view".into())
    }

    fn fetch_manifest(
        &self,
        cut: &FormalFullSyncCutV1,
    ) -> Result<FormalFullSyncManifestV1, String> {
        let request = FormalFullSyncManifestRequestV1 {
            schema_version: 1,
            grant: self
                .config
                .grant
                .clone()
                .ok_or("full-sync issuer grant missing")?,
            cut: cut.clone(),
        };
        request.validate()?;
        let manifest: FormalFullSyncManifestV1 =
            self.post_signed(FORMAL_FULL_SYNC_MANIFEST_PATH, request, 8 * 1024 * 1024)?;
        manifest.validate()?;
        if manifest.cut != *cut {
            return Err("full-sync manifest differs from independently verified cut".into());
        }
        Ok(manifest)
    }

    fn fetch_retained_tree(
        &self,
        cut: &FormalFullSyncCutV1,
        retained_root_digest: &str,
        root: &BusinessIndexRootV1,
        budget: &mut CasBudget,
    ) -> Result<u64, String> {
        root.validate()?;
        let empty = BusinessIndexRootV1::empty_subtree_hash(0)?;
        if root.node_root == empty {
            if root.leaf_count != 0 {
                return Err("empty full-sync retained tree has nonzero leaf count".into());
            }
            return Ok(0);
        }
        let mut prefix = Vec::with_capacity(256);
        let mut path = Vec::with_capacity(257);
        let mut leaves = 0u64;
        self.walk_retained_node(
            cut,
            retained_root_digest,
            root.node_root.as_str(),
            0,
            &mut prefix,
            &mut path,
            budget,
            &mut leaves,
        )?;
        Ok(leaves)
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "Merkle traversal carries its bounded proof state explicitly"
    )]
    fn walk_retained_node(
        &self,
        cut: &FormalFullSyncCutV1,
        retained_root_digest: &str,
        digest: &str,
        depth: usize,
        prefix: &mut Vec<bool>,
        path: &mut Vec<String>,
        budget: &mut CasBudget,
        leaves: &mut u64,
    ) -> Result<(), String> {
        if digest == BusinessIndexRootV1::empty_subtree_hash(depth)? {
            return Ok(());
        }
        if depth > 256 || budget.visited_paths >= cas_lifecycle::MAX_RETAINED_TREE_NODES {
            return Err("full-sync retained tree node/path bound".into());
        }
        budget.visited_paths += 1;
        path.push(digest.to_owned());
        let bytes = if let Some(existing) = self.read_cached_object(digest)? {
            existing
        } else {
            self.fetch_object(
                cut,
                digest,
                Some(retained_root_digest),
                Some(path.clone()),
                None,
            )?
        };
        if blake3::hash(&bytes).to_hex().as_str() != digest {
            path.pop();
            return Err("full-sync retained node digest mismatch".into());
        }
        budget.add(digest, bytes.len() as u64)?;
        let node = BusinessIndexNodeV1::decode(&bytes)?;
        match node.kind {
            BusinessIndexNodeKindV1::Branch {
                depth: node_depth,
                left_hash,
                right_hash,
            } if depth < 256 && node_depth as usize == depth => {
                let next_depth = depth + 1;
                for (right, child) in [(false, left_hash), (true, right_hash)] {
                    if child == BusinessIndexRootV1::empty_subtree_hash(next_depth)? {
                        continue;
                    }
                    prefix.push(right);
                    self.walk_retained_node(
                        cut,
                        retained_root_digest,
                        &child,
                        next_depth,
                        prefix,
                        path,
                        budget,
                        leaves,
                    )?;
                    prefix.pop();
                }
            }
            BusinessIndexNodeKindV1::Leaf(leaf) if depth == 256 && prefix.len() == 256 => {
                let key = hex::decode(&leaf.key_digest).map_err(|error| error.to_string())?;
                if key.len() != 32
                    || prefix.iter().enumerate().any(|(index, right)| {
                        (key[index / 8] & (0x80 >> (index % 8)) != 0) != *right
                    })
                {
                    path.pop();
                    return Err(
                        "full-sync retained leaf does not match authenticated tree path".into(),
                    );
                }
                *leaves = leaves
                    .checked_add(1)
                    .ok_or("full-sync retained leaf overflow")?;
            }
            _ => {
                path.pop();
                return Err("full-sync retained tree node has invalid depth or shape".into());
            }
        }
        self.persist_cas_object(digest, &bytes)
            .map_err(|error| format!("persist full-sync retained node: {error}"))?;
        path.pop();
        Ok(())
    }

    fn fetch_object(
        &self,
        cut: &FormalFullSyncCutV1,
        content_hash: &str,
        retained_root_digest: Option<&str>,
        business_path: Option<Vec<String>>,
        expected_size: Option<u64>,
    ) -> Result<Vec<u8>, String> {
        if expected_size.is_some_and(|size| size == 0 || size > MAX_FULL_SYNC_RECORD_BYTES as u64) {
            return Err("full-sync object exceeds protocol size bound".into());
        }
        let mut total_bytes = expected_size;
        let mut output = Vec::with_capacity(expected_size.unwrap_or(0) as usize);
        let mut offset = 0u64;
        loop {
            if total_bytes.is_some_and(|total| offset == total) {
                break;
            }
            let max_bytes = total_bytes.map_or(MAX_FULL_SYNC_CHUNK_BYTES as u64, |total| {
                (total - offset).min(MAX_FULL_SYNC_CHUNK_BYTES as u64)
            }) as u32;
            let request = FormalFullSyncObjectRequestV1 {
                schema_version: 1,
                grant: self
                    .config
                    .grant
                    .clone()
                    .ok_or("full-sync issuer grant missing")?,
                cut: cut.clone(),
                content_hash: content_hash.to_owned(),
                offset,
                max_bytes,
                retained_history_root_digest: retained_root_digest.map(str::to_owned),
                business_path: business_path.clone(),
            };
            request.validate()?;
            let chunk: FormalFullSyncObjectChunkV1 =
                self.post_signed(FORMAL_FULL_SYNC_OBJECT_PATH, request.clone(), 256 * 1024)?;
            chunk.validate_for(&request)?;
            if chunk.total_bytes == 0 || chunk.total_bytes > MAX_FULL_SYNC_RECORD_BYTES as u64 {
                return Err("full-sync object chunk total size outside protocol bound".into());
            }
            if total_bytes
                .replace(chunk.total_bytes)
                .is_some_and(|old| old != chunk.total_bytes)
                || expected_size.is_some_and(|expected| expected != chunk.total_bytes)
                || chunk.bytes.is_empty()
            {
                return Err("full-sync object chunk has inconsistent total size".into());
            }
            offset = offset
                .checked_add(chunk.bytes.len() as u64)
                .ok_or("full-sync object offset overflow")?;
            if offset > chunk.total_bytes {
                return Err("full-sync object transfer exceeded declared size".into());
            }
            output.extend_from_slice(&chunk.bytes);
        }
        let expected = total_bytes.ok_or("full-sync object response omitted total size")?;
        if output.len() as u64 != expected
            || blake3::hash(&output).to_hex().as_str() != content_hash
        {
            return Err("full-sync object transfer hash/size mismatch".into());
        }
        Ok(output)
    }

    fn post_signed<Q, P>(&self, path: &str, request: Q, response_bound: usize) -> Result<P, String>
    where
        Q: Serialize,
        P: DeserializeOwned + Serialize,
    {
        let signed = authority::sign_read_request(path, request, &self.node_private_key_hex)?;
        if signed.subject_public_key != self.node_public_key_hex {
            return Err("full-sync request signer identity changed".into());
        }
        let request_digest = authority::request_digest(path, &signed)?;
        let provider = self
            .config
            .providers()
            .get(self.active_provider_index)
            .cloned()
            .ok_or("active full-sync provider is missing")?;
        let url = format!("{}{}", provider.endpoint.trim_end_matches('/'), path);
        let response = self
            .http
            .post(url)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .json(&signed)
            .send()
            .map_err(|error| format!("full-sync transport: {error}"))?;
        let status = response.status();
        let mut bytes = Vec::new();
        response
            .take(response_bound as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| error.to_string())?;
        if bytes.len() > response_bound {
            return Err("full-sync response exceeds endpoint-specific bound".into());
        }
        if !status.is_success() {
            let envelope: StrictServiceEnvelope<FormalFullSyncServiceErrorV1> =
                serde_json::from_slice(&bytes)
                    .map_err(|_| format!("full-sync service HTTP {}", status.as_u16()))?;
            let response = envelope.into_signed();
            authority::verify_service_response(
                path,
                &request_digest,
                &response,
                &provider.trusted_service_public_key,
            )?;
            if response.payload.api_version != "oasis7.formal-live-error/v1" {
                return Err("full-sync service error schema mismatch".into());
            }
            return Err(format!(
                "full-sync service {} (HTTP {}): {}",
                response.payload.code,
                status.as_u16(),
                response
                    .payload
                    .message
                    .chars()
                    .take(512)
                    .collect::<String>()
            ));
        }
        let envelope: StrictServiceEnvelope<P> = serde_json::from_slice(&bytes)
            .map_err(|error| format!("full-sync response JSON: {error}"))?;
        let response = envelope.into_signed();
        authority::verify_service_response(
            path,
            &request_digest,
            &response,
            &provider.trusted_service_public_key,
        )?;
        Ok(response.payload)
    }

    fn fail(&mut self, error: &str) {
        self.status.ready = false;
        self.status.can_propose = false;
        self.status.state_root_verified = false;
        self.status.world_loaded = false;
        self.status.loaded_cut = None;
        // Expose only the session's durably persisted, independently verified
        // proof cursor. A provider's raw head or a view rejected after its
        // proof was persisted must never be synthesized here.
        self.status.last_verified_cut = self.history.last_verified_cut().cloned();
        self.status.lag_positions = None;
        self.status.catch_up_pending = false;
        self.status.state = "sync_unavailable".into();
        self.status.error = Some(error.chars().take(512).collect());
        self.status.last_checked_unix_ms = now_unix_ms();
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct StrictServiceEnvelope<T> {
    request_digest: String,
    payload: T,
    signature_hex: String,
}
impl<T> StrictServiceEnvelope<T> {
    fn into_signed(self) -> SignedServiceResponse<T> {
        SignedServiceResponse {
            request_digest: self.request_digest,
            payload: self.payload,
            signature_hex: self.signature_hex,
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct FormalFullSyncServiceErrorV1 {
    api_version: String,
    code: String,
    message: String,
}

struct LocalVerifiedBusinessIndexReader {
    store: Arc<LocalCasStore>,
    integrity_poisoned: Arc<AtomicBool>,
}
impl BusinessIndexNodeReader for LocalVerifiedBusinessIndexReader {
    fn read_node(&self, digest: &str) -> Result<Option<Vec<u8>>, String> {
        let result = self
            .store
            .get_verified(digest)
            .map(Some)
            .map_err(|error| format!("verified retained CAS read: {error:?}"));
        if result.is_err() {
            // The retained trie was completely fetched and hash checked just
            // before World restore. A later read failure is local CAS damage,
            // not evidence that another provider has a different valid cut.
            self.integrity_poisoned.store(true, Ordering::Release);
        }
        result
    }
}

fn verified_anchor(
    view: &VerifiedWorldView,
    config: &FormalFullSyncConfigV1,
) -> Result<VerifiedFormalReadAnchorV1, String> {
    let commit = view
        .formal_commit()
        .ok_or("full-sync source read has no independently verified commit")?;
    let anchor = view
        .formal_read_anchor()
        .ok_or("full-sync source read has no independently verified read anchor")?;
    if commit.record() != anchor.head().record()
        || commit.finalizing_authority_id() != anchor.head().finalizing_authority_id()
        || anchor.authority().definition().profile_version != 2
        || anchor.head().record().world.world_id != config.world_id
        || anchor.head().record().world.chain_id != config.chain_id
        || anchor.head().record().world.genesis_digest != config.genesis_digest
    {
        return Err("full-sync source proof differs from configured World or exact cut".into());
    }
    Ok(anchor.clone())
}

fn cut_from_anchor(anchor: &VerifiedFormalReadAnchorV1) -> Result<FormalFullSyncCutV1, String> {
    let record = anchor.head().record();
    Ok(FormalFullSyncCutV1 {
        world: WorldIdentity {
            world_id: record.world.world_id.clone(),
            genesis_digest: record.world.genesis_digest.clone(),
        },
        chain_id: record.world.chain_id.clone(),
        position: record.position,
        record_id: record.record_id().map_err(|error| error.to_string())?,
        after_state_root: record.after_state_root.clone(),
        active_authority_id: anchor
            .authority()
            .authority_id()
            .map_err(|error| error.to_string())?,
        finalizing_authority_id: anchor.head().finalizing_authority_id().to_owned(),
        authority_epoch: anchor.authority().definition().epoch,
    })
}

fn native_role(role: WireArtifactRole) -> ArtifactRole {
    match role {
        WireArtifactRole::Input => ArtifactRole::Input,
        WireArtifactRole::Result => ArtifactRole::Result,
        WireArtifactRole::Snapshot => ArtifactRole::Snapshot,
        WireArtifactRole::Journal => ArtifactRole::Journal,
        WireArtifactRole::NonceIndex => ArtifactRole::NonceIndex,
        WireArtifactRole::EffectOutbox => ArtifactRole::EffectOutbox,
        WireArtifactRole::ExecutionManifest => ArtifactRole::ExecutionManifest,
        WireArtifactRole::Wasm => ArtifactRole::Wasm,
        WireArtifactRole::Rules => ArtifactRole::Rules,
        WireArtifactRole::NativeExecutionEnvironment => ArtifactRole::NativeExecutionEnvironment,
    }
}

fn object_bytes<'a>(material: &'a ClosedRecord, digest: &str) -> Result<&'a [u8], String> {
    material
        .objects
        .iter()
        .find(|object| object.content_hash == digest)
        .map(|object| object.bytes.as_slice())
        .ok_or_else(|| "full-sync artifact object missing from checked closure".into())
}

fn verified_local_object(store: &LocalCasStore, digest: &str) -> Result<Option<Vec<u8>>, String> {
    match store.get_verified(digest) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(oasis7_proto::world_error::WorldError::BlobNotFound { .. }) => Ok(None),
        Err(error) => Err(format!(
            "local content-addressed object is invalid: {error:?}"
        )),
    }
}

fn nondecreasing_cut(previous: &FormalFullSyncCutV1, current: &FormalFullSyncCutV1) -> bool {
    previous.world == current.world
        && previous.chain_id == current.chain_id
        && current.position >= previous.position
        && (current.position != previous.position || current == previous)
}

fn valid_hex(value: &str, bytes: usize) -> bool {
    value.len() == bytes * 2
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn now_unix_ms() -> Option<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|time| u64::try_from(time.as_millis()).ok())
}
