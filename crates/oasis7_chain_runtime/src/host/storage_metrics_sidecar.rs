use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use serde::Deserialize;

#[derive(Debug, Default, Deserialize)]
struct SidecarGenerationRecordWire {
    #[serde(default)]
    pinned_blob_hashes: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
struct SidecarGcResultWire {
    #[serde(default)]
    status: String,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    updated_at_ms: i64,
}

#[derive(Debug, Default, Deserialize)]
struct SidecarGenerationIndexWire {
    #[serde(default)]
    latest_generation: String,
    #[serde(default)]
    rollback_safe_generation: Option<String>,
    #[serde(default)]
    generations: BTreeMap<String, SidecarGenerationRecordWire>,
    #[serde(default)]
    last_gc_result: SidecarGcResultWire,
}

#[derive(Debug)]
pub(super) struct SidecarMetricsSnapshot {
    pub(super) pin_count: u64,
    pub(super) last_gc_at_ms: Option<i64>,
    pub(super) last_gc_result: String,
    pub(super) last_gc_error: Option<String>,
}

pub(super) fn read_sidecar_metrics(
    sidecar_store_root: &Path,
) -> Result<SidecarMetricsSnapshot, String> {
    let index_path = sidecar_store_root.join("sidecar-generations/index.json");
    if !index_path.exists() {
        return Ok(SidecarMetricsSnapshot {
            pin_count: 0,
            last_gc_at_ms: None,
            last_gc_result: "not_available".to_string(),
            last_gc_error: None,
        });
    }
    let bytes = fs::read(index_path.as_path()).map_err(|err| {
        format!(
            "read sidecar generation index {} failed: {err}",
            index_path.display()
        )
    })?;
    let index: SidecarGenerationIndexWire =
        serde_json::from_slice(bytes.as_slice()).map_err(|err| {
            format!(
                "parse sidecar generation index {} failed: {err}",
                index_path.display()
            )
        })?;
    let mut active_generation_ids = BTreeSet::new();
    if !index.latest_generation.trim().is_empty() {
        active_generation_ids.insert(index.latest_generation.trim().to_string());
    }
    if let Some(rollback_safe_generation) = index.rollback_safe_generation.as_ref()
        && !rollback_safe_generation.trim().is_empty()
    {
        active_generation_ids.insert(rollback_safe_generation.trim().to_string());
    }
    let mut pinned_blob_hashes = BTreeSet::new();
    for generation_id in active_generation_ids {
        if let Some(record) = index.generations.get(generation_id.as_str()) {
            for hash in &record.pinned_blob_hashes {
                if !hash.trim().is_empty() {
                    pinned_blob_hashes.insert(hash.trim().to_string());
                }
            }
        }
    }
    Ok(SidecarMetricsSnapshot {
        pin_count: pinned_blob_hashes.len() as u64,
        last_gc_at_ms: Some(index.last_gc_result.updated_at_ms).filter(|value| *value > 0),
        last_gc_result: if index.last_gc_result.status.trim().is_empty() {
            "unknown".to_string()
        } else {
            index.last_gc_result.status
        },
        last_gc_error: index.last_gc_result.error,
    })
}
