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

#[derive(Clone, Default)]
struct RecordedNodes {
    nodes: BTreeMap<String, Vec<u8>>,
    total_bytes: usize,
    accessed: BTreeSet<String>,
}

/// Exact current-layer union; no material is admitted by a capacity plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ContentNodeCapacityV1 {
    Fits { nodes: usize, bytes: usize },
    Exceeded { nodes: usize, bytes: usize },
}

/// Reads from the supplied index reader while retaining a bounded, verified
/// overlay of every dependency needed for deterministic replay.
pub(super) struct RecordingBusinessIndexReaderV1 {
    external: Arc<dyn BusinessIndexNodeReader + Send + Sync>,
    strict_overlay: bool,
    closed_parent: Option<Arc<Self>>,
    closed_depth: usize,
    recorded: Mutex<RecordedNodes>,
}

impl RecordingBusinessIndexReaderV1 {
    /// Create a reader that serves seeded nodes first, then captures verified
    /// misses from the external reader.
    pub(super) fn new(
        external: Arc<dyn BusinessIndexNodeReader + Send + Sync>,
        seed: Vec<(String, Vec<u8>)>,
    ) -> Result<Arc<Self>, String> {
        Self::with_mode(external, seed, false, None)
    }

    /// Create a replay reader that serves only the persisted dependency set.
    /// It never consults the external reader or lets replay add new nodes.
    pub(super) fn new_strict(
        external: Arc<dyn BusinessIndexNodeReader + Send + Sync>,
        seed: Vec<(String, Vec<u8>)>,
    ) -> Result<Arc<Self>, String> {
        Self::with_mode(external, seed, true, None)
    }

    /// A bounded current-segment reader over a previously closed parent.
    pub(super) fn new_closed_layer(
        parent: Arc<Self>,
        seed: Vec<(String, Vec<u8>)>,
    ) -> Result<Arc<Self>, String> {
        if !parent.strict_overlay {
            return Err("closed dependency layer requires a strict verified parent".into());
        }
        Self::with_mode(parent.clone(), seed, true, Some(parent))
    }

    /// Derive local new nodes over the same closed parent without growing the
    /// ancestry chain for every preview or retry within one segment.
    pub(super) fn live_closed_layer(&self) -> Result<Arc<Self>, String> {
        if !self.strict_overlay {
            return Err("live dependency layer requires a strict replayed parent".into());
        }
        let parent = self
            .closed_parent
            .clone()
            .ok_or("live dependency layer lacks closed parent")?;
        let layer = Self::with_mode(parent.clone(), self.recorded_nodes()?, false, Some(parent))?;
        for (digest, bytes) in self.accessed_nodes()? {
            if layer.read_node(&digest)?.as_ref() != Some(&bytes) {
                return Err("live closed layer lost observed dependency".into());
            }
        }
        Ok(layer)
    }

    pub(super) fn replay_current_layer(
        &self,
        seed: Vec<(String, Vec<u8>)>,
    ) -> Result<Arc<Self>, String> {
        match self.closed_parent.clone() {
            Some(parent) => Self::new_closed_layer(parent, seed),
            None if self.strict_overlay => Self::new_strict(self.external.clone(), seed),
            None => Err("current dependency layer lacks closed parent".into()),
        }
    }

    fn with_mode(
        external: Arc<dyn BusinessIndexNodeReader + Send + Sync>,
        seed: Vec<(String, Vec<u8>)>,
        strict_overlay: bool,
        closed_parent: Option<Arc<Self>>,
    ) -> Result<Arc<Self>, String> {
        let closed_depth = closed_parent
            .as_ref()
            .map_or(Some(0), |parent| parent.closed_depth.checked_add(1))
            .filter(|depth| *depth <= 64)
            .ok_or("closed dependency layer depth capacity")?;
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
            closed_parent,
            closed_depth,
            recorded: Mutex::new(recorded),
        }))
    }

    /// Copy the verified parent overlay and its successful accessed baseline.
    /// New preview writes remain local; a missing parent node never consults an
    /// old recorder or an external source. Failed previews cannot mutate either.
    pub(super) fn fork_live_preview(&self) -> Result<Arc<Self>, String> {
        let recorded = self
            .recorded
            .lock()
            .map_err(|_| "formal BFT dependency recorder lock poisoned".to_owned())?
            .clone();
        Ok(Arc::new(Self {
            external: Arc::new(NoAdditionalParentNodes),
            strict_overlay: false,
            closed_parent: self.closed_parent.clone(),
            closed_depth: self.closed_depth,
            recorded: Mutex::new(recorded),
        }))
    }

    /// Include derived writes before signing, using the same digest union as
    /// publication. Conflicting bytes remain an identity error, not capacity.
    pub(super) fn plan_content_nodes(
        &self,
        writes: &[(String, Vec<u8>)],
    ) -> Result<ContentNodeCapacityV1, String> {
        let recorded = self
            .recorded
            .lock()
            .map_err(|_| "formal BFT dependency recorder lock poisoned")?;
        let mut additional = BTreeMap::<&str, &[u8]>::new();
        let mut nodes = recorded.nodes.len();
        let mut bytes = recorded.total_bytes;
        for (digest, value) in writes {
            validate_node(digest, value)?;
            if let Some(original) = recorded.nodes.get(digest) {
                if original != value {
                    return Err("planned business-index bytes conflict".into());
                }
                continue;
            }
            if let Some(original) = additional.insert(digest, value) {
                if original != value {
                    return Err("planned business-index bytes conflict".into());
                }
                continue;
            }
            nodes = nodes
                .checked_add(1)
                .ok_or("planned business-index count overflow")?;
            bytes = bytes
                .checked_add(value.len())
                .ok_or("planned business-index bytes overflow")?;
        }
        Ok(
            if nodes > MAX_RETAINED_INDEX_NODES || bytes > MAX_RETAINED_INDEX_TOTAL_BYTES {
                ContentNodeCapacityV1::Exceeded { nodes, bytes }
            } else {
                ContentNodeCapacityV1::Fits { nodes, bytes }
            },
        )
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

    // A layer may inspect only an immutable strict inventory, never its raw
    // external reader. Do not mutate ancestor observations or charge it again.
    fn closed_node(&self, digest: &str) -> Result<Option<Vec<u8>>, String> {
        if !self.strict_overlay {
            return Err("closed dependency lookup reached an open reader".into());
        }
        let local = self
            .recorded
            .lock()
            .map_err(|_| "formal BFT dependency recorder lock poisoned".to_owned())?
            .nodes
            .get(digest)
            .cloned();
        match (local, &self.closed_parent) {
            (Some(bytes), _) => Ok(Some(bytes)),
            (None, Some(parent)) => parent.closed_node(digest),
            (None, None) => Ok(None),
        }
    }

    fn resolve(&self, digest: &str, cached_only: bool) -> Result<Option<Vec<u8>>, String> {
        validate_digest(digest)?;
        if let Some(bytes) = self.local_node(digest)? {
            return Ok(Some(bytes));
        }
        if self.strict_overlay {
            return self
                .closed_parent
                .as_ref()
                .map(|parent| parent.closed_node(digest))
                .transpose()?
                .flatten()
                .map(|bytes| self.record_external_node(digest, bytes))
                .transpose();
        }
        if let Some(parent) = &self.closed_parent {
            return parent
                .closed_node(digest)?
                .map(|bytes| self.record_external_node(digest, bytes))
                .transpose();
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

// Every admitted parent node is already seeded in the fork. Missing content
// must retain the historical strict-overlay refusal, without shared counters.
struct NoAdditionalParentNodes;
impl BusinessIndexNodeReader for NoAdditionalParentNodes {
    fn read_node(&self, _digest: &str) -> Result<Option<Vec<u8>>, String> {
        Ok(None)
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
            let original = self
                .closed_parent
                .as_ref()
                .map(|parent| parent.closed_node(digest))
                .transpose()?
                .flatten();
            if original.as_deref() != Some(bytes) {
                return Err("formal BFT replay tried to add an unrecorded index node".into());
            }
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
