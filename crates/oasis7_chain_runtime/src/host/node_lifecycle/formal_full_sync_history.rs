//! Durable, externally rooted proof-page replay for full-state sync.
//!
//! The mutable head file is only a local consistency pointer. Startup replays
//! every content-addressed page from the configured external V1 pin before it
//! restores a World or advertises readiness.

use super::*;
use oasis7::world_service::formal_read::{
    FormalReadHistoryPageV1, FormalReadReplayCursorV1, FormalReadStepV1,
};
use std::collections::BTreeSet;

const HISTORY_HEAD_FILE: &str = "formal-full-sync-history-head.json";
const MAX_HISTORY_CHAIN_PAGES: u64 = 65_536;
pub(super) const MAX_HISTORY_CHAIN_BYTES: u64 = 4 * 1024 * 1024 * 1024;
const MAX_HISTORY_MANIFEST_BYTES: usize = 4096;
pub(super) const MAX_HISTORY_MANIFEST_CHAIN_BYTES: usize = 64 * 1024 * 1024;
pub(super) const MAX_HISTORY_CAS_BYTES: u64 =
    MAX_HISTORY_CHAIN_BYTES + MAX_HISTORY_MANIFEST_CHAIN_BYTES as u64;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FormalFullSyncHistoryHeadV1 {
    schema_version: u32,
    world_id: String,
    chain_id: String,
    origin_checkpoint_digest: String,
    trust_identity_digest: String,
    final_cursor: FormalReadReplayCursorV1,
    manifest_digest: Option<String>,
    page_count: u64,
    total_page_bytes: u64,
    total_manifest_bytes: u64,
    last_verified_cut: Option<FormalFullSyncCutV1>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FormalFullSyncHistoryManifestV1 {
    schema_version: u32,
    previous_manifest_digest: Option<String>,
    page_digest: String,
    page_bytes: u64,
    start_cursor: FormalReadReplayCursorV1,
    end_cursor: FormalReadReplayCursorV1,
    page_count: u64,
    total_page_bytes: u64,
    previous_manifest_bytes: u64,
}

pub(super) struct PersistedFormalReadHistoryV1 {
    cas: Arc<LocalCasStore>,
    head_path: std::path::PathBuf,
    trust_identity_digest: String,
    head: FormalFullSyncHistoryHeadV1,
    history_pins: BTreeSet<String>,
    poisoned: bool,
}

impl PersistedFormalReadHistoryV1 {
    /// Replay the retained pages from external C0 and optionally require an
    /// immutable stage cut to occur in that exact authenticated lineage.
    pub(super) fn verify_read_only_at_least(
        config: &FormalFullSyncConfigV1,
        client: &RemoteWorldServiceClient,
        cas: Arc<LocalCasStore>,
        root_path: &Path,
        minimum_cut: Option<&FormalFullSyncCutV1>,
    ) -> Result<Option<serde_json::Value>, String> {
        let origin_checkpoint_digest = config
            .formal_read_trust
            .checkpoint
            .digest()
            .map_err(|error| error.to_string())?;
        if origin_checkpoint_digest != config.formal_read_trust.checkpoint_digest {
            return Err("full-sync external checkpoint digest is not canonical".into());
        }
        let origin_cursor = client
            .formal_replay_cursor()
            .map_err(|error| format!("full-sync initial formal cursor: {error}"))?;
        let trust_identity_digest = trust_identity_digest(config)?;
        let head_path = root_path.join(HISTORY_HEAD_FILE);
        super::super::local_io::reject_symlinks(&head_path)?;
        let origin_anchor = client
            .formal_current_anchor()
            .map_err(|error| format!("restore full-sync external origin anchor: {error}"))?;
        let origin_cut = cut_from_anchor(&origin_anchor)?;
        validate_minimum_cut(minimum_cut, &origin_cut, config)?;
        let head = match std::fs::symlink_metadata(&head_path) {
            Ok(metadata) if metadata.is_file() => {
                super::super::local_io::read::<FormalFullSyncHistoryHeadV1>(&head_path)?
            }
            Ok(_) => return Err("full-sync history head is not a regular file".into()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let cas_summary = super::cas_lifecycle::verify_read_only_cas(&cas)?;
                if cas_summary["object_count"].as_u64().unwrap_or_default() != 0 {
                    return Err("HistoryUnavailable: full-sync retained CAS exists without its external-origin history head".into());
                }
                if minimum_cut.is_some_and(|minimum| minimum != &origin_cut) {
                    return Err("HistoryUnavailable: empty full-sync history is below the immutable stage minimum cut".into());
                }
                return Ok(Some(serde_json::json!({
                    "state": "uninitialized",
                    "origin_cut": origin_cut,
                    "current_cut": origin_cut,
                    "last_verified_cut": serde_json::Value::Null,
                    "page_count": 0,
                    "total_page_bytes": 0,
                    "total_manifest_bytes": 0,
                    "trust_identity_digest": trust_identity_digest,
                    "cas": cas_summary,
                    "minimum_cut_verified": minimum_cut.is_some(),
                    "minimum_cut": minimum_cut
                })));
            }
            Err(error) => return Err(format!("read full-sync history head: {error}")),
        };
        let mut history = Self {
            cas: Arc::clone(&cas),
            head_path,
            trust_identity_digest: trust_identity_digest.clone(),
            head,
            history_pins: BTreeSet::new(),
            poisoned: false,
        };
        history.restore(config, client, &origin_cursor, false, minimum_cut)?;
        let cas_summary = super::cas_lifecycle::verify_read_only_cas(&cas)?;
        let current_anchor = client
            .formal_current_anchor()
            .map_err(|error| format!("restore full-sync verified anchor: {error}"))?;
        let current_cut = cut_from_anchor(&current_anchor)?;
        Ok(Some(serde_json::json!({
            "schema_version": history.head.schema_version,
            "world_id": history.head.world_id,
            "chain_id": history.head.chain_id,
            "origin_checkpoint_digest": history.head.origin_checkpoint_digest,
            "trust_identity_digest": trust_identity_digest,
            "origin_cut": origin_cut,
            "page_count": history.head.page_count,
            "total_page_bytes": history.head.total_page_bytes,
            "total_manifest_bytes": history.head.total_manifest_bytes,
            "manifest_digest": history.head.manifest_digest,
            "final_cursor": history.head.final_cursor,
            "last_verified_cut": history.head.last_verified_cut,
            "current_cut": current_cut,
            "cas": cas_summary,
            "minimum_cut_verified": minimum_cut.is_some(),
            "minimum_cut": minimum_cut
        })))
    }

    pub(super) fn open(
        config: &FormalFullSyncConfigV1,
        client: &RemoteWorldServiceClient,
        cas: Arc<LocalCasStore>,
        root_path: &Path,
    ) -> Result<Self, String> {
        let origin_checkpoint_digest = config
            .formal_read_trust
            .checkpoint
            .digest()
            .map_err(|error| error.to_string())?;
        if origin_checkpoint_digest != config.formal_read_trust.checkpoint_digest {
            return Err("full-sync external checkpoint digest is not canonical".into());
        }
        let origin_cursor = client
            .formal_replay_cursor()
            .map_err(|error| format!("full-sync initial formal cursor: {error}"))?;
        let trust_identity_digest = trust_identity_digest(config)?;
        let head_path = root_path.join(HISTORY_HEAD_FILE);
        let genesis = FormalFullSyncHistoryHeadV1 {
            schema_version: 1,
            world_id: config.world_id.clone(),
            chain_id: config.chain_id.clone(),
            origin_checkpoint_digest: config.formal_read_trust.checkpoint_digest.clone(),
            trust_identity_digest: trust_identity_digest.clone(),
            final_cursor: origin_cursor.clone(),
            manifest_digest: None,
            page_count: 0,
            total_page_bytes: 0,
            total_manifest_bytes: 0,
            last_verified_cut: None,
        };
        super::super::local_io::reject_symlinks(&head_path)?;
        let head = match std::fs::symlink_metadata(&head_path) {
            Ok(metadata) if metadata.is_file() => {
                super::super::local_io::read::<FormalFullSyncHistoryHeadV1>(&head_path)?
            }
            Ok(_) => return Err("full-sync history head is not a regular file".into()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                super::super::local_io::write(&head_path, &genesis)?;
                genesis
            }
            Err(error) => return Err(format!("read full-sync history head: {error}")),
        };
        let mut history = Self {
            cas,
            head_path,
            trust_identity_digest,
            head,
            history_pins: BTreeSet::new(),
            poisoned: false,
        };
        history.restore(config, client, &origin_cursor, true, None)?;
        Ok(history)
    }

    pub(super) fn last_verified_cut(&self) -> Option<&FormalFullSyncCutV1> {
        self.head.last_verified_cut.as_ref()
    }

    pub(super) fn is_poisoned(&self) -> bool {
        self.poisoned
    }

    pub(super) fn poison(&mut self) {
        self.poisoned = true;
    }

    /// Seed a newly constructed provider client from the exact saved proof-page
    /// chain. Provider endpoints do not become checkpoint sources: the cursor
    /// is always replayed from the configured external C0 pin.
    pub(super) fn replay_into_fresh_client(
        &mut self,
        config: &FormalFullSyncConfigV1,
        client: &RemoteWorldServiceClient,
    ) -> Result<(), String> {
        let origin_cursor = client
            .formal_replay_cursor()
            .map_err(|error| format!("full-sync provider external cursor: {error}"))?;
        self.restore(config, client, &origin_cursor, false, None)
    }

    pub(super) fn persist_verified_view(
        &mut self,
        config: &FormalFullSyncConfigV1,
        client: &RemoteWorldServiceClient,
        view: &VerifiedWorldView,
        anchor: &VerifiedFormalReadAnchorV1,
    ) -> Result<(), String> {
        let cursor = view
            .formal_replay_cursor()?
            .ok_or("full-sync view has no verified replay cursor")?;
        if cursor.anchor.position != anchor.head().record().position
            || cursor.anchor.record_id != anchor.head().head().record_id
            || cursor.anchor.active_authority_id
                != anchor
                    .authority()
                    .authority_id()
                    .map_err(|error| error.to_string())?
        {
            return Err("full-sync replay cursor differs from verified current anchor".into());
        }
        if client
            .formal_replay_cursor()
            .map_err(|error| format!("read current formal replay cursor: {error}"))?
            != cursor
        {
            return Err("full-sync view cursor differs from client verifier state".into());
        }

        let mut next = self.head.clone();
        let mut next_history_pins = self.history_pins.clone();
        if let Some(page) = view.formal_history_page() {
            page.validate()?;
            if page.start != next.final_cursor || page.end != cursor {
                return Err("full-sync history page does not continue exact durable cursor".into());
            }
            if next.page_count >= MAX_HISTORY_CHAIN_PAGES {
                return Err(
                    "HistoryUnavailable: full-sync history page chain reached its bound".into(),
                );
            }
            let bytes = page.encode_bytes()?;
            let page_digest = hex::encode(blake3::hash(&bytes).as_bytes());
            next_history_pins.insert(page_digest.clone());
            let total_page_bytes = next
                .total_page_bytes
                .checked_add(bytes.len() as u64)
                .ok_or("full-sync proof-page byte count overflow")?;
            if total_page_bytes > MAX_HISTORY_CHAIN_BYTES {
                return Err(
                    "HistoryUnavailable: full-sync proof-page chain reached its byte bound".into(),
                );
            }
            self.cas
                .put(&page_digest, &bytes)
                .map_err(|error| format!("persist full-sync proof page: {error:?}"))?;
            if verified_bounded_cas(
                &self.cas,
                &page_digest,
                oasis7_client_api::world_service::MAX_FORMAL_PROOF_BYTES,
            )? != bytes
            {
                return Err(
                    "persisted full-sync proof page differs from its verified bytes".into(),
                );
            }

            let manifest = FormalFullSyncHistoryManifestV1 {
                schema_version: 1,
                previous_manifest_digest: next.manifest_digest.clone(),
                page_digest,
                page_bytes: bytes.len() as u64,
                start_cursor: page.start.clone(),
                end_cursor: page.end.clone(),
                page_count: next.page_count + 1,
                total_page_bytes,
                previous_manifest_bytes: next.total_manifest_bytes,
            };
            let manifest_bytes = serde_cbor::to_vec(&manifest)
                .map_err(|error| format!("encode full-sync history manifest: {error}"))?;
            if manifest_bytes.len() > MAX_HISTORY_MANIFEST_BYTES {
                return Err("full-sync history manifest exceeds its size bound".into());
            }
            next.total_manifest_bytes = next
                .total_manifest_bytes
                .checked_add(manifest_bytes.len() as u64)
                .ok_or("full-sync manifest byte count overflow")?;
            if next.total_manifest_bytes > MAX_HISTORY_MANIFEST_CHAIN_BYTES as u64 {
                return Err(
                    "HistoryUnavailable: full-sync manifest chain reached its byte bound".into(),
                );
            }
            let manifest_digest = hex::encode(blake3::hash(&manifest_bytes).as_bytes());
            next_history_pins.insert(manifest_digest.clone());
            self.cas
                .put(&manifest_digest, &manifest_bytes)
                .map_err(|error| format!("persist full-sync history manifest: {error:?}"))?;
            if verified_bounded_cas(&self.cas, &manifest_digest, MAX_HISTORY_MANIFEST_BYTES)?
                != manifest_bytes
            {
                return Err("persisted full-sync manifest differs from its canonical bytes".into());
            }
            next.manifest_digest = Some(manifest_digest);
            next.page_count += 1;
            next.total_page_bytes = total_page_bytes;
            next.final_cursor = cursor;
        } else if cursor != next.final_cursor {
            return Err("full-sync read advanced without a bounded history page".into());
        }
        next.last_verified_cut = (next.page_count > 0)
            .then(|| cut_from_anchor(anchor))
            .transpose()?;
        validate_head(&next, config, &self.trust_identity_digest)?;
        if next_history_pins != self.history_pins
            && let Err(error) =
                super::cas_lifecycle::replace_history_pins(&self.cas, &next_history_pins)
        {
            if error.outcome_unknown() {
                self.poisoned = true;
            }
            return Err(error.to_string());
        }
        if let Err(error) = super::super::local_io::write(&self.head_path, &next) {
            // The head writer may have renamed the new file before its
            // directory sync failed. Do not collect either history closure
            // until startup replays whichever durable head survived.
            self.poisoned = true;
            return Err(error);
        }
        self.head = next;
        self.history_pins = next_history_pins;
        Ok(())
    }

    fn restore(
        &mut self,
        config: &FormalFullSyncConfigV1,
        client: &RemoteWorldServiceClient,
        origin_cursor: &FormalReadReplayCursorV1,
        publish_pins: bool,
        minimum_cut: Option<&FormalFullSyncCutV1>,
    ) -> Result<(), String> {
        validate_head(&self.head, config, &self.trust_identity_digest)?;
        let origin_anchor = client
            .formal_current_anchor()
            .map_err(|error| format!("restore full-sync external origin anchor: {error}"))?;
        let origin_cut = cut_from_anchor(&origin_anchor)?;
        validate_minimum_cut(minimum_cut, &origin_cut, config)?;
        let mut minimum_found = minimum_cut.is_some_and(|minimum| minimum == &origin_cut);
        if self.head.page_count == 0 {
            if self.head.manifest_digest.is_some()
                || self.head.total_page_bytes != 0
                || self.head.total_manifest_bytes != 0
                || self.head.last_verified_cut.is_some()
                || self.head.final_cursor != *origin_cursor
            {
                return Err("full-sync empty history head is not the exact external origin".into());
            }
            if minimum_cut.is_some() && !minimum_found {
                return Err("HistoryUnavailable: empty full-sync history is below the immutable stage minimum cut".into());
            }
            if publish_pins {
                super::cas_lifecycle::replace_history_pins(&self.cas, &BTreeSet::new())
                    .map_err(|error| error.to_string())?;
            }
            self.history_pins.clear();
            return Ok(());
        }
        if self.head.manifest_digest.is_none() || self.head.last_verified_cut.is_none() {
            return Err(
                "full-sync persisted history is missing its manifest or verified cut".into(),
            );
        }

        let mut manifests = Vec::new();
        let mut history_pins = BTreeSet::new();
        let mut manifest_digest = self.head.manifest_digest.clone();
        let mut expected_end = self.head.final_cursor.clone();
        let mut expected_count = self.head.page_count;
        let mut expected_total = self.head.total_page_bytes;
        let mut expected_manifest_total = self.head.total_manifest_bytes;
        while let Some(digest) = manifest_digest.take() {
            if manifests.len() >= MAX_HISTORY_CHAIN_PAGES as usize || expected_count == 0 {
                return Err("full-sync history manifest chain exceeds its page bound".into());
            }
            let bytes = verified_bounded_cas(&self.cas, &digest, MAX_HISTORY_MANIFEST_BYTES)?;
            history_pins.insert(digest.clone());
            let manifest: FormalFullSyncHistoryManifestV1 =
                serde_cbor::from_slice(&bytes).map_err(|error| error.to_string())?;
            if serde_cbor::to_vec(&manifest).map_err(|error| error.to_string())? != bytes
                || manifest.schema_version != 1
                || manifest.page_count != expected_count
                || manifest.total_page_bytes != expected_total
                || manifest
                    .previous_manifest_bytes
                    .checked_add(bytes.len() as u64)
                    != Some(expected_manifest_total)
                || manifest.end_cursor != expected_end
                || !valid_hex(&manifest.page_digest, 32)
                || manifest.page_count == 0
                || manifest.page_count > MAX_HISTORY_CHAIN_PAGES
                || manifest.page_bytes == 0
                || manifest.page_bytes
                    > oasis7_client_api::world_service::MAX_FORMAL_PROOF_BYTES as u64
                || manifest.page_bytes > expected_total
            {
                return Err("full-sync history manifest link or cursor is invalid".into());
            }
            expected_total = expected_total
                .checked_sub(manifest.page_bytes)
                .ok_or("full-sync proof-page byte total underflow")?;
            expected_manifest_total = manifest.previous_manifest_bytes;
            expected_count -= 1;
            expected_end = manifest.start_cursor.clone();
            manifest_digest = manifest.previous_manifest_digest.clone();
            manifests.push(manifest);
            if expected_count == 0 && manifest_digest.is_some() {
                return Err("full-sync history manifest chain exceeds its declared count".into());
            }
        }
        if expected_count != 0
            || expected_total != 0
            || expected_manifest_total != 0
            || expected_end != *origin_cursor
            || manifests.len() as u64 != self.head.page_count
        {
            return Err(
                "full-sync proof-page manifest chain does not reach external origin".into(),
            );
        }
        manifests.reverse();
        history_pins.extend(
            manifests
                .iter()
                .map(|manifest| manifest.page_digest.clone()),
        );

        let store = Arc::clone(&self.cas);
        let pages = manifests.into_iter().map(|manifest| {
            let page_bytes = verified_bounded_cas(
                &store,
                &manifest.page_digest,
                oasis7_client_api::world_service::MAX_FORMAL_PROOF_BYTES,
            )?;
            if page_bytes.len() as u64 != manifest.page_bytes {
                return Err("full-sync page byte count differs from immutable manifest".into());
            }
            let page = FormalReadHistoryPageV1::decode_bytes(&page_bytes)?;
            if page.start != manifest.start_cursor || page.end != manifest.end_cursor {
                return Err("full-sync manifest differs from its immutable proof page".into());
            }
            if let Some(minimum) = minimum_cut {
                for step in &page.history.steps {
                    if let FormalReadStepV1::Commit(proof) = step {
                        let record = &proof.decision.record;
                        if record.position == minimum.position {
                            let record_id = record.record_id().map_err(|error| error.to_string())?;
                            let finalizer = &proof.primary_complete.body.finalizing_authority_id;
                            if record.world.world_id != minimum.world.world_id
                                || record.world.chain_id != minimum.chain_id
                                || record.world.genesis_digest != minimum.world.genesis_digest
                                || record_id != minimum.record_id
                                || record.after_state_root != minimum.after_state_root
                                || finalizer != &minimum.finalizing_authority_id
                            {
                                return Err("HistoryUnavailable: saved formal history diverges at the immutable stage minimum cut".into());
                            }
                            minimum_found = true;
                        }
                    }
                }
            }
            Ok((manifest.page_digest, page_bytes))
        });
        client
            .resume_formal_history_pages(
                &config.formal_read_trust.checkpoint_digest,
                pages,
                &self.head.final_cursor,
            )
            .map_err(|error| format!("replay full-sync external-origin history pages: {error}"))?;
        let anchor = client
            .formal_current_anchor()
            .map_err(|error| format!("restore full-sync verified anchor: {error}"))?;
        if cut_from_anchor(&anchor)? != self.head.last_verified_cut.clone().unwrap() {
            return Err(
                "full-sync saved cut differs from replayed external-origin proof chain".into(),
            );
        }
        let current_cut = cut_from_anchor(&anchor)?;
        if let Some(minimum) = minimum_cut {
            if !minimum_found || !super::nondecreasing_cut(minimum, &current_cut) {
                return Err("HistoryUnavailable: verified full-sync history is below the immutable stage minimum cut".into());
            }
            if minimum.position == current_cut.position && minimum != &current_cut {
                return Err("HistoryUnavailable: current full-sync cut differs from the exact immutable stage minimum".into());
            }
        }
        if publish_pins {
            super::cas_lifecycle::replace_history_pins(&self.cas, &history_pins)
                .map_err(|error| error.to_string())?;
        }
        self.history_pins = history_pins;
        Ok(())
    }
}

fn validate_head(
    head: &FormalFullSyncHistoryHeadV1,
    config: &FormalFullSyncConfigV1,
    trust_identity_digest: &str,
) -> Result<(), String> {
    if head.schema_version != 1
        || head.world_id != config.world_id
        || head.chain_id != config.chain_id
        || head.origin_checkpoint_digest != config.formal_read_trust.checkpoint_digest
        || head.trust_identity_digest != trust_identity_digest
        || head.page_count > MAX_HISTORY_CHAIN_PAGES
        || head.total_page_bytes > MAX_HISTORY_CHAIN_BYTES
        || head.total_manifest_bytes > MAX_HISTORY_MANIFEST_CHAIN_BYTES as u64
        || !valid_hex(&head.origin_checkpoint_digest, 32)
        || !valid_hex(&head.trust_identity_digest, 32)
        || head
            .manifest_digest
            .as_deref()
            .is_some_and(|digest| !valid_hex(digest, 32))
    {
        return Err(
            "full-sync history head differs from configured external trust or bounds".into(),
        );
    }
    Ok(())
}

fn validate_minimum_cut(
    minimum: Option<&FormalFullSyncCutV1>,
    origin: &FormalFullSyncCutV1,
    config: &FormalFullSyncConfigV1,
) -> Result<(), String> {
    let Some(minimum) = minimum else {
        return Ok(());
    };
    minimum.validate()?;
    if minimum.world.world_id != config.world_id
        || minimum.world.genesis_digest != config.genesis_digest
        || minimum.chain_id != config.chain_id
        || minimum.world != origin.world
        || minimum.chain_id != origin.chain_id
    {
        return Err(
            "immutable stage minimum cut differs from the configured external World".into(),
        );
    }
    if minimum.position < origin.position
        || (minimum.position == origin.position && minimum != origin)
        || (minimum.position > origin.position && !super::nondecreasing_cut(origin, minimum))
    {
        return Err("immutable stage minimum cut is not on or after the exact external C0".into());
    }
    Ok(())
}

fn trust_identity_digest(config: &FormalFullSyncConfigV1) -> Result<String, String> {
    let mut bytes = b"oasis7.formal.full_sync.history_trust.v1\0".to_vec();
    bytes.extend(
        serde_cbor::to_vec(&(
            &config.world_id,
            &config.chain_id,
            &config.genesis_digest,
            &config.formal_read_trust.initial_trust,
            &config.formal_read_trust.checkpoint,
            &config.formal_read_trust.checkpoint_digest,
        ))
        .map_err(|error| error.to_string())?,
    );
    Ok(hex::encode(blake3::hash(&bytes).as_bytes()))
}

fn verified_bounded_cas(
    store: &LocalCasStore,
    digest: &str,
    max_bytes: usize,
) -> Result<Vec<u8>, String> {
    if !valid_hex(digest, 32) || max_bytes == 0 {
        return Err("invalid bounded full-sync CAS request".into());
    }
    let (bytes, complete) = store
        .get_range(digest, 0, max_bytes.saturating_add(1))
        .map_err(|error| format!("read bounded full-sync CAS object: {error:?}"))?;
    if !complete || bytes.len() > max_bytes {
        return Err("full-sync CAS object exceeds its declared bound".into());
    }
    if blake3::hash(&bytes).to_hex().as_str() != digest {
        return Err("full-sync CAS object content digest mismatch".into());
    }
    Ok(bytes)
}
