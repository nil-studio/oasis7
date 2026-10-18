//! Manifest types and patch operations for configuration management.

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

use super::error::WorldError;
use super::modules::ModuleChangeSet;
use super::types::PatchPath;
use super::util::hash_json;

/// A versioned manifest containing arbitrary JSON configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    pub version: u64,
    pub content: JsonValue,
}

impl Default for Manifest {
    fn default() -> Self {
        Self {
            version: 1,
            content: JsonValue::Object(serde_json::Map::new()),
        }
    }
}

impl Manifest {
    pub fn module_changes(&self) -> Result<Option<ModuleChangeSet>, WorldError> {
        let JsonValue::Object(map) = &self.content else {
            return Ok(None);
        };
        let Some(value) = map.get("module_changes") else {
            return Ok(None);
        };
        if value.is_null() {
            return Ok(None);
        }
        let changes: ModuleChangeSet = serde_json::from_value(value.clone()).map_err(|err| {
            WorldError::ModuleChangeInvalid {
                reason: err.to_string(),
            }
        })?;
        Ok(Some(changes))
    }

    pub fn without_module_changes(&self) -> Result<Manifest, WorldError> {
        let mut content = self.content.clone();
        if let JsonValue::Object(map) = &mut content {
            map.remove("module_changes");
        }
        Ok(Manifest {
            version: self.version,
            content,
        })
    }
}

/// A patch to be applied to a manifest.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ManifestPatch {
    pub base_manifest_hash: String,
    pub ops: Vec<ManifestPatchOp>,
    pub new_version: Option<u64>,
}

/// An individual operation within a manifest patch.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", content = "data")]
pub enum ManifestPatchOp {
    Set { path: PatchPath, value: JsonValue },
    Remove { path: PatchPath },
}

/// Metadata about a manifest update event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ManifestUpdate {
    pub manifest: Manifest,
    pub manifest_hash: String,
}

// ----------------------------------------------------------------------------
// Patch application functions
// ----------------------------------------------------------------------------

pub fn apply_manifest_patch(
    manifest: &Manifest,
    patch: &ManifestPatch,
) -> Result<Manifest, WorldError> {
    apply_manifest_patch_internal(manifest, patch)
}

fn apply_manifest_patch_internal(
    manifest: &Manifest,
    patch: &ManifestPatch,
) -> Result<Manifest, WorldError> {
    let current_hash = hash_json(manifest)?;
    if patch.base_manifest_hash != current_hash {
        return Err(WorldError::PatchBaseMismatch {
            expected: current_hash,
            found: patch.base_manifest_hash.clone(),
        });
    }
    apply_manifest_patch_ops(manifest, patch)
}

pub(crate) fn apply_manifest_patch_ops(
    manifest: &Manifest,
    patch: &ManifestPatch,
) -> Result<Manifest, WorldError> {
    let mut content = manifest.content.clone();
    for op in &patch.ops {
        apply_manifest_patch_op(&mut content, op)?;
    }
    let version = patch.new_version.unwrap_or(manifest.version);
    Ok(Manifest { version, content })
}

fn apply_manifest_patch_op(root: &mut JsonValue, op: &ManifestPatchOp) -> Result<(), WorldError> {
    match op {
        ManifestPatchOp::Set { path, value } => apply_patch_set(root, path, value.clone()),
        ManifestPatchOp::Remove { path } => apply_patch_remove(root, path),
    }
}

fn apply_patch_set(
    root: &mut JsonValue,
    path: &PatchPath,
    value: JsonValue,
) -> Result<(), WorldError> {
    if path.is_empty() {
        *root = value;
        return Ok(());
    }

    let mut current = root;
    for (idx, segment) in path.iter().enumerate() {
        let is_last = idx + 1 == path.len();
        let map = current
            .as_object_mut()
            .ok_or_else(|| WorldError::PatchNonObject {
                path: path[..idx].join("."),
            })?;
        if is_last {
            map.insert(segment.clone(), value);
            return Ok(());
        }
        current = map
            .entry(segment.clone())
            .or_insert_with(|| JsonValue::Object(serde_json::Map::new()));
    }
    Ok(())
}

fn apply_patch_remove(root: &mut JsonValue, path: &PatchPath) -> Result<(), WorldError> {
    if path.is_empty() {
        return Err(WorldError::PatchInvalidPath {
            path: "".to_string(),
        });
    }

    let mut current = root;
    for (idx, segment) in path.iter().enumerate() {
        let is_last = idx + 1 == path.len();
        let map = current
            .as_object_mut()
            .ok_or_else(|| WorldError::PatchNonObject {
                path: path[..idx].join("."),
            })?;
        if is_last {
            if map.remove(segment).is_none() {
                return Err(WorldError::PatchInvalidPath {
                    path: path.join("."),
                });
            }
            return Ok(());
        }
        current = map
            .get_mut(segment)
            .ok_or_else(|| WorldError::PatchInvalidPath {
                path: path[..=idx].join("."),
            })?;
    }
    Ok(())
}

// ----------------------------------------------------------------------------
// Diff and merge functions
// ----------------------------------------------------------------------------

pub fn diff_manifest(base: &Manifest, target: &Manifest) -> Result<ManifestPatch, WorldError> {
    let base_hash = hash_json(base)?;
    let mut ops = Vec::new();
    diff_json(&base.content, &target.content, &mut Vec::new(), &mut ops);
    let new_version = if base.version == target.version {
        None
    } else {
        Some(target.version)
    };
    Ok(ManifestPatch {
        base_manifest_hash: base_hash,
        ops,
        new_version,
    })
}

pub fn merge_manifest_patches(
    base: &Manifest,
    patches: &[ManifestPatch],
) -> Result<ManifestPatch, WorldError> {
    let base_hash = hash_json(base)?;
    let mut current = base.clone();
    for patch in patches {
        if patch.base_manifest_hash != base_hash {
            return Err(WorldError::PatchBaseMismatch {
                expected: base_hash.clone(),
                found: patch.base_manifest_hash.clone(),
            });
        }
        current = apply_manifest_patch_ops(&current, patch)?;
    }
    diff_manifest(base, &current)
}

pub fn merge_manifest_patches_with_conflicts(
    base: &Manifest,
    patches: &[ManifestPatch],
) -> Result<PatchMergeResult, WorldError> {
    let conflicts = detect_patch_conflicts(patches);
    let patch = merge_manifest_patches(base, patches)?;
    Ok(PatchMergeResult { patch, conflicts })
}

fn diff_json(
    base: &JsonValue,
    target: &JsonValue,
    path: &mut Vec<String>,
    ops: &mut Vec<ManifestPatchOp>,
) {
    if base == target {
        return;
    }

    match (base, target) {
        (JsonValue::Object(base_map), JsonValue::Object(target_map)) => {
            for_each_ordered_json_object_key(base_map, target_map, |key| {
                path.push(key.clone());
                match (base_map.get(key), target_map.get(key)) {
                    (Some(base_val), Some(target_val)) => {
                        diff_json(base_val, target_val, path, ops);
                    }
                    (None, Some(target_val)) => {
                        ops.push(ManifestPatchOp::Set {
                            path: path.clone(),
                            value: target_val.clone(),
                        });
                    }
                    (Some(_), None) => {
                        ops.push(ManifestPatchOp::Remove { path: path.clone() });
                    }
                    (None, None) => {}
                }
                path.pop();
            });
        }
        _ => {
            ops.push(ManifestPatchOp::Set {
                path: path.clone(),
                value: target.clone(),
            });
        }
    }
}

fn for_each_ordered_json_object_key(
    base_map: &serde_json::Map<String, JsonValue>,
    target_map: &serde_json::Map<String, JsonValue>,
    mut visit: impl FnMut(&String),
) {
    let mut base_keys = base_map.keys().peekable();
    let mut target_keys = target_map.keys().peekable();
    loop {
        let key = match (base_keys.peek(), target_keys.peek()) {
            (Some(base_key), Some(target_key)) => match base_key.cmp(target_key) {
                std::cmp::Ordering::Less => base_keys.next().expect("peeked base key"),
                std::cmp::Ordering::Equal => {
                    target_keys.next();
                    base_keys.next().expect("peeked base key")
                }
                std::cmp::Ordering::Greater => target_keys.next().expect("peeked target key"),
            },
            (Some(_), None) => base_keys.next().expect("peeked base key"),
            (None, Some(_)) => target_keys.next().expect("peeked target key"),
            (None, None) => break,
        };
        visit(key);
    }
}

// ----------------------------------------------------------------------------
// Conflict detection
// ----------------------------------------------------------------------------

/// Result of merging multiple patches, including any detected conflicts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PatchMergeResult {
    pub patch: ManifestPatch,
    pub conflicts: Vec<PatchConflict>,
}

/// A detected conflict between patch operations.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PatchConflict {
    pub path: PatchPath,
    pub kind: ConflictKind,
    pub patches: Vec<usize>,
    pub ops: Vec<PatchOpSummary>,
}

/// The kind of conflict detected.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictKind {
    SamePath,
    PrefixOverlap,
}

/// The kind of patch operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PatchOpKind {
    Set,
    Remove,
}

/// Summary of a patch operation for conflict reporting.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PatchOpSummary {
    pub patch_index: usize,
    pub kind: PatchOpKind,
    pub path: PatchPath,
}

fn detect_patch_conflicts(patches: &[ManifestPatch]) -> Vec<PatchConflict> {
    use std::collections::BTreeMap;

    let mut entries: Vec<(PatchPath, PatchOpSummary)> = Vec::new();
    for (idx, patch) in patches.iter().enumerate() {
        for op in &patch.ops {
            let (path, kind) = match op {
                ManifestPatchOp::Set { path, .. } => (path.clone(), PatchOpKind::Set),
                ManifestPatchOp::Remove { path } => (path.clone(), PatchOpKind::Remove),
            };
            entries.push((
                path.clone(),
                PatchOpSummary {
                    patch_index: idx,
                    kind,
                    path,
                },
            ));
        }
    }

    let mut conflicts: BTreeMap<String, PatchConflict> = BTreeMap::new();
    for i in 0..entries.len() {
        for j in (i + 1)..entries.len() {
            let (path_a, summary_a) = &entries[i];
            let (path_b, summary_b) = &entries[j];
            if path_is_prefix(path_a, path_b) || path_is_prefix(path_b, path_a) {
                let conflict_path = if path_a.len() <= path_b.len() {
                    path_a.clone()
                } else {
                    path_b.clone()
                };
                let key = conflict_path.join(".");
                let kind = if path_a == path_b {
                    ConflictKind::SamePath
                } else {
                    ConflictKind::PrefixOverlap
                };
                let entry = conflicts.entry(key.clone()).or_insert(PatchConflict {
                    path: conflict_path,
                    kind: kind.clone(),
                    patches: Vec::new(),
                    ops: Vec::new(),
                });
                if kind == ConflictKind::SamePath {
                    entry.kind = ConflictKind::SamePath;
                }
                insert_patch_index(&mut entry.patches, summary_a.patch_index);
                insert_patch_index(&mut entry.patches, summary_b.patch_index);
                insert_op_summary(&mut entry.ops, summary_a.clone());
                insert_op_summary(&mut entry.ops, summary_b.clone());
            }
        }
    }

    let mut results: Vec<PatchConflict> = conflicts.into_values().collect();
    for conflict in &mut results {
        conflict.patches.sort();
        conflict.ops.sort_by(|left, right| {
            left.patch_index
                .cmp(&right.patch_index)
                .then_with(|| left.path.cmp(&right.path))
        });
    }
    results
}

fn path_is_prefix(a: &PatchPath, b: &PatchPath) -> bool {
    if a.len() > b.len() {
        return false;
    }
    a.iter().zip(b.iter()).all(|(left, right)| left == right)
}

fn insert_patch_index(target: &mut Vec<usize>, index: usize) {
    if !target.contains(&index) {
        target.push(index);
    }
}

fn insert_op_summary(target: &mut Vec<PatchOpSummary>, summary: PatchOpSummary) {
    if !target.iter().any(|existing| {
        existing.patch_index == summary.patch_index
            && existing.kind == summary.kind
            && existing.path == summary.path
    }) {
        target.push(summary);
    }
}
