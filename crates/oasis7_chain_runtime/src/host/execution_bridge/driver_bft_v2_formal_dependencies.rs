//! Bounded, content-addressed dependency capture for formal BFT replay.
//!
//! A retained H result tree may read nodes from an external CAS reader. This
//! adapter records every verified node actually used so the formal BFT ledger
//! can persist and later replay against an exact local overlay. Persisted
//! replay uses strict mode and never falls back to the external source.

use oasis7_distfs::controlled_authority::formal::business_index::{
    BUSINESS_INDEX_MAX_LEAF_BYTES, BusinessIndexNodeReader, BusinessIndexRootV1,
};
use oasis7_runtime::blake3_hex;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

pub(super) const BUSINESS_INDEX_MAX_NODE_BYTES: usize = BUSINESS_INDEX_MAX_LEAF_BYTES + 128;
pub(super) const MAX_RETAINED_INDEX_TOTAL_BYTES: usize = 64 * 1024 * 1024;
pub(super) const MAX_RETAINED_INDEX_NODES: usize = 4096;

#[derive(Default)]
struct RecordedNodes {
    nodes: BTreeMap<String, Vec<u8>>,
    total_bytes: usize,
    accessed: BTreeSet<String>,
}

/// Reads from the supplied index reader while retaining a bounded, verified
/// overlay of every dependency needed for deterministic replay.
pub(super) struct RecordingBusinessIndexReaderV1 {
    external: Arc<dyn BusinessIndexNodeReader + Send + Sync>,
    strict_overlay: bool,
    recorded: Mutex<RecordedNodes>,
}

impl RecordingBusinessIndexReaderV1 {
    /// Create a reader that serves seeded nodes first, then captures verified
    /// misses from the external reader.
    pub(super) fn new(
        external: Arc<dyn BusinessIndexNodeReader + Send + Sync>,
        seed: Vec<(String, Vec<u8>)>,
    ) -> Result<Arc<Self>, String> {
        Self::with_mode(external, seed, false)
    }

    /// Create a replay reader that serves only the persisted dependency set.
    /// It never consults the external reader or lets replay add new nodes.
    pub(super) fn new_strict(
        external: Arc<dyn BusinessIndexNodeReader + Send + Sync>,
        seed: Vec<(String, Vec<u8>)>,
    ) -> Result<Arc<Self>, String> {
        Self::with_mode(external, seed, true)
    }

    fn with_mode(
        external: Arc<dyn BusinessIndexNodeReader + Send + Sync>,
        seed: Vec<(String, Vec<u8>)>,
        strict_overlay: bool,
    ) -> Result<Arc<Self>, String> {
        let mut recorded = RecordedNodes::default();
        for (digest, bytes) in seed {
            validate_node(digest.as_str(), bytes.as_slice())?;
            if let Some(existing) = recorded.nodes.get(digest.as_str()) {
                if existing != &bytes {
                    return Err(
                        "formal BFT dependency seed has conflicting bytes for one digest".into(),
                    );
                }
                continue;
            }
            add_node(&mut recorded, digest, bytes)?;
        }
        Ok(Arc::new(Self {
            external,
            strict_overlay,
            recorded: Mutex::new(recorded),
        }))
    }

    /// Return the seed plus every subsequently observed or locally cached node
    /// in canonical digest order.
    pub(super) fn recorded_nodes(&self) -> Result<Vec<(String, Vec<u8>)>, String> {
        let recorded = self
            .recorded
            .lock()
            .map_err(|_| "formal BFT dependency recorder lock poisoned".to_owned())?;
        Ok(recorded
            .nodes
            .iter()
            .map(|(digest, bytes)| (digest.clone(), bytes.clone()))
            .collect())
    }

    pub(super) fn recorded_refs(&self) -> Result<Vec<String>, String> {
        let recorded = self
            .recorded
            .lock()
            .map_err(|_| "formal BFT dependency recorder lock poisoned".to_owned())?;
        Ok(recorded.nodes.keys().cloned().collect())
    }

    /// Nodes actually read or produced by this replay. Unvisited preload
    /// entries (including a later historical suffix) do not become dependencies.
    pub(super) fn accessed_nodes(&self) -> Result<BTreeMap<String, Vec<u8>>, String> {
        let recorded = self
            .recorded
            .lock()
            .map_err(|_| "formal BFT dependency recorder lock poisoned".to_owned())?;
        Ok(recorded
            .accessed
            .iter()
            .map(|digest| (digest.clone(), recorded.nodes[digest].clone()))
            .collect())
    }

    fn local_node(&self, digest: &str) -> Result<Option<Vec<u8>>, String> {
        let mut recorded = self
            .recorded
            .lock()
            .map_err(|_| "formal BFT dependency recorder lock poisoned".to_owned())?;
        let bytes = recorded.nodes.get(digest).cloned();
        if bytes.is_some() {
            recorded.accessed.insert(digest.to_owned());
        }
        Ok(bytes)
    }

    fn record_external_node(&self, digest: &str, bytes: Vec<u8>) -> Result<Vec<u8>, String> {
        validate_node(digest, bytes.as_slice())?;
        let mut recorded = self
            .recorded
            .lock()
            .map_err(|_| "formal BFT dependency recorder lock poisoned".to_owned())?;
        if let Some(existing) = recorded.nodes.get(digest) {
            if existing != &bytes {
                return Err("external business-index reader changed bytes for one digest".into());
            }
            let bytes = existing.clone();
            recorded.accessed.insert(digest.to_owned());
            return Ok(bytes);
        }
        ensure_capacity(&recorded, bytes.len())?;
        let returned = bytes.clone();
        add_node(&mut recorded, digest.to_owned(), bytes)?;
        recorded.accessed.insert(digest.to_owned());
        Ok(returned)
    }

    fn resolve(&self, digest: &str, cached_only: bool) -> Result<Option<Vec<u8>>, String> {
        validate_digest(digest)?;
        if let Some(bytes) = self.local_node(digest)? {
            return Ok(Some(bytes));
        }
        if self.strict_overlay {
            return Ok(None);
        }
        let external = if cached_only {
            self.external.read_cached_node(digest)?
        } else {
            self.external.read_node(digest)?
        };
        external
            .map(|bytes| self.record_external_node(digest, bytes))
            .transpose()
    }
}

impl BusinessIndexNodeReader for RecordingBusinessIndexReaderV1 {
    fn prefetch_path(&self, root: &BusinessIndexRootV1, key: &str) -> Result<(), String> {
        if self.strict_overlay {
            return Ok(());
        }
        self.external.prefetch_path(root, key)
    }

    fn read_node(&self, digest: &str) -> Result<Option<Vec<u8>>, String> {
        self.resolve(digest, false)
    }

    fn read_cached_node(&self, digest: &str) -> Result<Option<Vec<u8>>, String> {
        self.resolve(digest, true)
    }

    fn cache_content_node(&self, digest: &str, bytes: &[u8]) -> Result<(), String> {
        validate_node(digest, bytes)?;
        let mut recorded = self
            .recorded
            .lock()
            .map_err(|_| "formal BFT dependency recorder lock poisoned".to_owned())?;
        if let Some(existing) = recorded.nodes.get(digest) {
            if existing.as_slice() != bytes {
                return Err("business-index cache bytes conflict with recorded digest".into());
            }
            recorded.accessed.insert(digest.to_owned());
            return Ok(());
        }
        if self.strict_overlay {
            return Err("formal BFT replay tried to add an unrecorded index node".into());
        }
        // Keep cache writes local. This bounds accounting before accepting the
        // write and avoids an external side effect that could outlive failure.
        ensure_capacity(&recorded, bytes.len())?;
        add_node(&mut recorded, digest.to_owned(), bytes.to_vec())?;
        recorded.accessed.insert(digest.to_owned());
        Ok(())
    }
}

fn add_node(recorded: &mut RecordedNodes, digest: String, bytes: Vec<u8>) -> Result<(), String> {
    ensure_capacity(recorded, bytes.len())?;
    recorded.total_bytes = recorded
        .total_bytes
        .checked_add(bytes.len())
        .ok_or("formal BFT retained index byte count overflow")?;
    recorded.nodes.insert(digest, bytes);
    Ok(())
}

fn ensure_capacity(recorded: &RecordedNodes, additional_bytes: usize) -> Result<(), String> {
    if recorded.nodes.len() >= MAX_RETAINED_INDEX_NODES {
        return Err("formal BFT retained index node count exceeds bound".into());
    }
    let total_bytes = recorded
        .total_bytes
        .checked_add(additional_bytes)
        .ok_or("formal BFT retained index byte count overflow")?;
    if total_bytes > MAX_RETAINED_INDEX_TOTAL_BYTES {
        return Err("formal BFT retained index bytes exceed bound".into());
    }
    Ok(())
}

pub(super) fn validate_nodes(nodes: &BTreeMap<String, Vec<u8>>) -> Result<(), String> {
    if nodes.len() > MAX_RETAINED_INDEX_NODES {
        return Err("formal BFT retained index node count exceeds bound".into());
    }
    let mut total_bytes = 0usize;
    for (digest, bytes) in nodes {
        validate_node(digest, bytes.as_slice())?;
        total_bytes = total_bytes
            .checked_add(bytes.len())
            .ok_or("formal BFT retained index byte count overflow")?;
        if total_bytes > MAX_RETAINED_INDEX_TOTAL_BYTES {
            return Err("formal BFT retained index bytes exceed bound".into());
        }
    }
    Ok(())
}

pub(super) fn validate_node(digest: &str, bytes: &[u8]) -> Result<(), String> {
    validate_digest(digest)?;
    if bytes.len() > BUSINESS_INDEX_MAX_NODE_BYTES {
        return Err("formal BFT business-index node exceeds per-node bound".into());
    }
    if blake3_hex(bytes) != digest {
        return Err("formal BFT business-index node digest mismatch".into());
    }
    Ok(())
}

fn validate_digest(digest: &str) -> Result<(), String> {
    if digest.len() != 64 {
        return Err("business-index digest must be 64 lowercase hex characters".into());
    }
    let bytes = hex::decode(digest).map_err(|error| format!("business-index digest: {error}"))?;
    if bytes.len() != 32 || hex::encode(bytes) != digest {
        return Err("business-index digest must be canonical lowercase BLAKE3 hex".into());
    }
    Ok(())
}

#[cfg(test)]
#[path = "driver_bft_v2_formal_dependencies_tests.rs"]
mod tests;
