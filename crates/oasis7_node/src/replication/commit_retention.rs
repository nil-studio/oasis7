use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::fs::OpenOptions;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use oasis7_distfs::{FileStore as _, LocalCasStore, blake3_hex};
use oasis7_proto::storage_cold_index::{
    STORAGE_COLD_INDEX_KEY_KIND_HEIGHT, STORAGE_COLD_INDEX_MANIFEST_FILE,
    STORAGE_COLD_INDEX_SEGMENTS_DIR, STORAGE_COLD_INDEX_VALUE_KIND_COMMIT_PACK_REF,
    STORAGE_COLD_INDEX_VALUE_KIND_CONTENT_HASH, StorageColdIndexManifest, StorageColdIndexRange,
    StorageColdIndexRangeAnchor, storage_cold_index_dir_name,
};
use serde::{Deserialize, Serialize};

use crate::NodeError;

use super::{COMMIT_MESSAGE_DIR, write_json_compact};
use super::{GossipReplicationMessage, ReplicatedCommitPayload};

const COMMIT_MESSAGE_PACK_HEIGHT_SPAN: u64 = 256;
const COMMIT_MESSAGE_PACK_ENTRY_LEN_BYTES: u64 = 8;
const MAX_COMMIT_MESSAGE_PACK_ENTRY_BYTES: u64 = 256 * 1024;
const LATEST_COMMIT_HEAD_INDEX_SCHEMA: u8 = 1;
const LATEST_COMMIT_HEAD_INDEX_DIR: &str = "replication_commit_heads";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub(super) struct LatestCommitHeadIndex {
    pub(super) schema: u8,
    pub(super) world_id: String,
    pub(super) height: u64,
    pub(super) record_content_hash: String,
    pub(super) message_hash: String,
}

pub(super) fn latest_commit_head_index_path_from_root(root_dir: &Path, world_id: &str) -> PathBuf {
    latest_commit_head_index_directory_from_root(root_dir)
        .join(format!("{}.json", blake3_hex(world_id.as_bytes())))
}

pub(super) fn latest_commit_head_index_directory_from_root(root_dir: &Path) -> PathBuf {
    root_dir.join(LATEST_COMMIT_HEAD_INDEX_DIR)
}

pub(super) fn load_latest_commit_head_index_from_root(
    root_dir: &Path,
    world_id: &str,
) -> Result<Option<LatestCommitHeadIndex>, NodeError> {
    let path = latest_commit_head_index_path_from_root(root_dir, world_id);
    if !path.exists() {
        return Ok(None);
    }
    let index = load_json_or_default::<LatestCommitHeadIndex>(path.as_path())?;
    if index.schema != LATEST_COMMIT_HEAD_INDEX_SCHEMA
        || index.world_id != world_id
        || index.height == 0
        || index.record_content_hash.is_empty()
        || index.message_hash.is_empty()
    {
        return Err(NodeError::Replication {
            reason: format!(
                "invalid latest commit head index for world={world_id}: {}",
                path.display()
            ),
        });
    }
    Ok(Some(index))
}

pub(super) fn write_latest_commit_head_index_to_root(
    root_dir: &Path,
    index: &LatestCommitHeadIndex,
) -> Result<(), NodeError> {
    if index.schema != LATEST_COMMIT_HEAD_INDEX_SCHEMA
        || index.world_id.is_empty()
        || index.height == 0
        || index.record_content_hash.is_empty()
        || index.message_hash.is_empty()
    {
        return Err(NodeError::Replication {
            reason: "refusing to persist invalid latest commit head index".to_string(),
        });
    }
    write_json_compact(
        latest_commit_head_index_path_from_root(root_dir, index.world_id.as_str()).as_path(),
        index,
    )
}

pub(super) fn latest_hot_commit_message_height_from_root(
    root_dir: &Path,
) -> Result<Option<u64>, NodeError> {
    Ok(list_hot_commit_message_files(root_dir)?
        .last()
        .map(|(height, _)| *height))
}

pub(super) fn hot_commit_message_heights_from_root(root_dir: &Path) -> Result<Vec<u64>, NodeError> {
    Ok(list_hot_commit_message_files(root_dir)?
        .into_iter()
        .map(|(height, _)| height)
        .collect())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct CommitMessagePackRef {
    pub(super) segment_id: String,
    pub(super) offset: u64,
    pub(super) len: u64,
    pub(super) content_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub(super) enum CommitMessageColdEntry {
    LegacyContentHash(String),
    PackRef(CommitMessagePackRef),
}

impl CommitMessageColdEntry {
    pub(super) fn content_hash(&self) -> &str {
        match self {
            Self::LegacyContentHash(content_hash) => content_hash.as_str(),
            Self::PackRef(pack_ref) => pack_ref.content_hash.as_str(),
        }
    }

    fn legacy_content_hash(&self) -> Option<&str> {
        match self {
            Self::LegacyContentHash(content_hash) => Some(content_hash.as_str()),
            Self::PackRef(_) => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct CommitMessageColdIndex {
    #[serde(flatten, default)]
    pub(super) manifest: StorageColdIndexManifest,
    #[serde(default)]
    pub(super) by_height: BTreeMap<u64, CommitMessageColdEntry>,
}

impl Default for CommitMessageColdIndex {
    fn default() -> Self {
        Self {
            manifest: StorageColdIndexManifest::new(
                COMMIT_MESSAGE_DIR,
                STORAGE_COLD_INDEX_KEY_KIND_HEIGHT,
                STORAGE_COLD_INDEX_VALUE_KIND_COMMIT_PACK_REF,
            ),
            by_height: BTreeMap::new(),
        }
    }
}

impl CommitMessageColdIndex {
    pub(super) fn refresh_metadata(&mut self, hot_window: &CommitMessageHotWindow) {
        self.manifest.namespace = COMMIT_MESSAGE_DIR.to_string();
        self.manifest.key_kind = STORAGE_COLD_INDEX_KEY_KIND_HEIGHT.to_string();
        self.manifest.value_kind =
            infer_commit_message_cold_index_value_kind(&self.by_height).to_string();
        self.manifest.hot_range =
            match (hot_window.hot_window_start_height, hot_window.latest_height) {
                (Some(from_key), Some(to_key)) if from_key <= to_key => {
                    Some(StorageColdIndexRange { from_key, to_key })
                }
                _ => None,
            };
        self.manifest.cold_range_anchor = build_cold_range_anchor(&self.by_height);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CommitMessageHotWindow {
    pub(super) latest_height: Option<u64>,
    pub(super) hot_window_start_height: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CommitMessageRetentionPlan {
    pub(super) hot_window: CommitMessageHotWindow,
    pub(super) offload_candidates: Vec<CommitMessageOffloadCandidate>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CommitMessageOffloadCandidate {
    pub(super) height: u64,
    pub(super) path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum CommitMessageReadbackSource {
    HotMirror { path: PathBuf },
    ColdCasArchive { content_hash: String },
    ColdPackArchive { pack_ref: CommitMessagePackRef },
}

pub(super) fn build_commit_message_retention_plan(
    root_dir: &Path,
    hot_window_heights: usize,
) -> Result<CommitMessageRetentionPlan, NodeError> {
    let hot_commit_files = list_hot_commit_message_files(root_dir)?;
    let Some(latest_height) = hot_commit_files.last().map(|(height, _)| *height) else {
        return Ok(CommitMessageRetentionPlan {
            hot_window: CommitMessageHotWindow {
                latest_height: None,
                hot_window_start_height: None,
            },
            offload_candidates: Vec::new(),
        });
    };

    let retained_hot_window = u64::try_from(hot_window_heights.max(1)).unwrap_or(u64::MAX);
    let hot_window_start_height =
        latest_height.saturating_sub(retained_hot_window.saturating_sub(1));
    let retained_checkpoint_heights =
        retained_checkpoint_message_heights(&hot_commit_files, hot_window_heights)?;
    let offload_candidates = hot_commit_files
        .into_iter()
        .filter(|(height, _)| {
            *height < hot_window_start_height && !retained_checkpoint_heights.contains(height)
        })
        .map(|(height, path)| CommitMessageOffloadCandidate { height, path })
        .collect();

    Ok(CommitMessageRetentionPlan {
        hot_window: CommitMessageHotWindow {
            latest_height: Some(latest_height),
            hot_window_start_height: Some(hot_window_start_height),
        },
        offload_candidates,
    })
}

fn retained_checkpoint_message_heights(
    hot_commit_files: &[(u64, PathBuf)],
    hot_window_heights: usize,
) -> Result<BTreeSet<u64>, NodeError> {
    let mut checkpoint_heights = BTreeSet::new();
    for (height, path) in hot_commit_files {
        let bytes = fs::read(path).map_err(|err| NodeError::Replication {
            reason: format!("read {} failed: {err}", path.display()),
        })?;
        let Ok(message) = serde_json::from_slice::<GossipReplicationMessage>(&bytes) else {
            continue;
        };
        let Ok(payload) = serde_json::from_slice::<ReplicatedCommitPayload>(&message.payload)
        else {
            continue;
        };
        if payload.height == *height && payload.execution_checkpoint.is_some() {
            checkpoint_heights.insert(*height);
        }
    }
    Ok(checkpoint_heights
        .into_iter()
        .rev()
        .take(hot_window_heights.max(1))
        .collect())
}

pub(super) fn resolve_commit_message_readback_source(
    root_dir: &Path,
    height: u64,
) -> Result<Option<CommitMessageReadbackSource>, NodeError> {
    let hot_path = commit_message_path_from_root(root_dir, height);
    if hot_path.exists() {
        return Ok(Some(CommitMessageReadbackSource::HotMirror {
            path: hot_path,
        }));
    }

    let Some(entry) = load_commit_message_cold_index_from_root(root_dir)?
        .by_height
        .get(&height)
        .cloned()
    else {
        return Ok(None);
    };

    Ok(Some(match entry {
        CommitMessageColdEntry::LegacyContentHash(content_hash) => {
            CommitMessageReadbackSource::ColdCasArchive { content_hash }
        }
        CommitMessageColdEntry::PackRef(pack_ref) => {
            CommitMessageReadbackSource::ColdPackArchive { pack_ref }
        }
    }))
}

pub(super) fn load_commit_message_cold_index_from_root(
    root_dir: &Path,
) -> Result<CommitMessageColdIndex, NodeError> {
    let canonical_path = commit_message_cold_index_manifest_path_from_root(root_dir);
    let compat_alias_path = commit_message_cold_index_compat_alias_path_from_root(root_dir);
    if canonical_path.exists() {
        let mut recovered_from_compat_alias = false;
        let loaded = match load_json_or_default::<CommitMessageColdIndex>(canonical_path.as_path())
        {
            Ok(loaded) => loaded,
            Err(canonical_err) if compat_alias_path.exists() => {
                recovered_from_compat_alias = true;
                load_json_or_default::<CommitMessageColdIndex>(compat_alias_path.as_path())
                    .map_err(|alias_err| NodeError::Replication {
                        reason: format!(
                            "load commit message cold index failed: canonical={} error={:?}; compat_alias={} error={:?}",
                            canonical_path.display(),
                            canonical_err,
                            compat_alias_path.display(),
                            alias_err
                        ),
                    })?
            }
            Err(err) => return Err(err),
        };
        let mut cold_index = normalize_commit_message_cold_index(loaded.clone());
        let migrated_hashes = migrate_legacy_cold_entries_to_packs(root_dir, &mut cold_index)?;
        if recovered_from_compat_alias {
            write_commit_message_cold_index_to_root(root_dir, &cold_index)?;
        } else if !compat_alias_path.exists() || cold_index != loaded {
            write_commit_message_cold_index_to_root(root_dir, &cold_index)?;
            delete_legacy_cold_commit_blobs_if_unreferenced(root_dir, &migrated_hashes)?;
            prune_unreferenced_commit_message_pack_files(root_dir, &cold_index)?;
        }
        return Ok(cold_index);
    }

    if compat_alias_path.exists() {
        let mut cold_index =
            normalize_commit_message_cold_index(load_json_or_default::<CommitMessageColdIndex>(
                compat_alias_path.as_path(),
            )?);
        let migrated_hashes = migrate_legacy_cold_entries_to_packs(root_dir, &mut cold_index)?;
        write_commit_message_cold_index_to_root(root_dir, &cold_index)?;
        delete_legacy_cold_commit_blobs_if_unreferenced(root_dir, &migrated_hashes)?;
        prune_unreferenced_commit_message_pack_files(root_dir, &cold_index)?;
        return Ok(cold_index);
    }

    Ok(CommitMessageColdIndex::default())
}

pub(super) fn has_commit_message_cold_index(root_dir: &Path) -> bool {
    commit_message_cold_index_manifest_path_from_root(root_dir).exists()
        || commit_message_cold_index_compat_alias_path_from_root(root_dir).exists()
}

pub(super) fn write_commit_message_cold_index_to_root(
    root_dir: &Path,
    cold_index: &CommitMessageColdIndex,
) -> Result<(), NodeError> {
    write_json_compact(
        commit_message_cold_index_manifest_path_from_root(root_dir).as_path(),
        cold_index,
    )?;
    write_json_compact(
        commit_message_cold_index_compat_alias_path_from_root(root_dir).as_path(),
        cold_index,
    )
}

pub(super) fn write_commit_message_pack_entry(
    root_dir: &Path,
    height: u64,
    bytes: &[u8],
    content_hash: &str,
) -> Result<CommitMessagePackRef, NodeError> {
    let segment_id = commit_message_pack_segment_id(height);
    let path = commit_message_pack_segment_path_from_root(root_dir, segment_id.as_str());
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| NodeError::Replication {
            reason: format!("create dir {} failed: {}", parent.display(), err),
        })?;
    }
    let entry_len = u64::try_from(bytes.len()).map_err(|_| NodeError::Replication {
        reason: format!(
            "commit message pack entry length {} exceeds u64 capacity",
            bytes.len()
        ),
    })?;
    if entry_len > MAX_COMMIT_MESSAGE_PACK_ENTRY_BYTES {
        return Err(NodeError::Replication {
            reason: format!(
                "commit message pack entry too large for {}: {} > {}",
                path.display(),
                entry_len,
                MAX_COMMIT_MESSAGE_PACK_ENTRY_BYTES
            ),
        });
    }
    let mut file = OpenOptions::new()
        .create(true)
        .read(true)
        .append(true)
        .open(&path)
        .map_err(|err| NodeError::Replication {
            reason: format!("open {} failed: {}", path.display(), err),
        })?;
    let offset = file
        .seek(SeekFrom::End(0))
        .map_err(|err| NodeError::Replication {
            reason: format!("seek {} to end failed: {}", path.display(), err),
        })?;
    file.write_all(&(bytes.len() as u64).to_le_bytes())
        .map_err(|err| NodeError::Replication {
            reason: format!("write {} length prefix failed: {}", path.display(), err),
        })?;
    file.write_all(bytes)
        .map_err(|err| NodeError::Replication {
            reason: format!("append {} failed: {}", path.display(), err),
        })?;
    Ok(CommitMessagePackRef {
        segment_id,
        offset,
        len: bytes.len() as u64,
        content_hash: content_hash.to_string(),
    })
}

pub(super) fn load_commit_message_pack_entry(
    root_dir: &Path,
    pack_ref: &CommitMessagePackRef,
) -> Result<Vec<u8>, NodeError> {
    if !is_valid_commit_message_pack_segment_id(pack_ref.segment_id.as_str()) {
        return Err(NodeError::Replication {
            reason: format!(
                "invalid commit message pack segment id: {}",
                pack_ref.segment_id
            ),
        });
    }
    let path = commit_message_pack_segment_path_from_root(root_dir, pack_ref.segment_id.as_str());
    if pack_ref.len > MAX_COMMIT_MESSAGE_PACK_ENTRY_BYTES {
        return Err(NodeError::Replication {
            reason: format!(
                "pack entry length {} exceeds max allowed {} for {}",
                pack_ref.len,
                MAX_COMMIT_MESSAGE_PACK_ENTRY_BYTES,
                path.display()
            ),
        });
    }
    let file_size = fs::metadata(&path)
        .map_err(|err| NodeError::Replication {
            reason: format!("metadata {} failed: {}", path.display(), err),
        })?
        .len();
    let entry_end = pack_ref
        .offset
        .checked_add(COMMIT_MESSAGE_PACK_ENTRY_LEN_BYTES)
        .and_then(|value| value.checked_add(pack_ref.len))
        .ok_or_else(|| NodeError::Replication {
            reason: format!(
                "pack entry bounds overflow for {} at offset {} with len {}",
                path.display(),
                pack_ref.offset,
                pack_ref.len
            ),
        })?;
    if entry_end > file_size {
        return Err(NodeError::Replication {
            reason: format!(
                "pack entry out of bounds for {} at offset {} with len {} (file size {})",
                path.display(),
                pack_ref.offset,
                pack_ref.len,
                file_size
            ),
        });
    }
    let mut file = fs::File::open(&path).map_err(|err| NodeError::Replication {
        reason: format!("open {} failed: {}", path.display(), err),
    })?;
    file.seek(SeekFrom::Start(pack_ref.offset))
        .map_err(|err| NodeError::Replication {
            reason: format!("seek {} failed: {}", path.display(), err),
        })?;
    let mut len_bytes = [0u8; COMMIT_MESSAGE_PACK_ENTRY_LEN_BYTES as usize];
    file.read_exact(&mut len_bytes)
        .map_err(|err| NodeError::Replication {
            reason: format!("read {} length prefix failed: {}", path.display(), err),
        })?;
    let stored_len = u64::from_le_bytes(len_bytes);
    if stored_len != pack_ref.len {
        return Err(NodeError::Replication {
            reason: format!(
                "pack entry length mismatch for {} at offset {}: index={}, stored={}",
                path.display(),
                pack_ref.offset,
                pack_ref.len,
                stored_len
            ),
        });
    }
    let mut bytes = vec![
        0u8;
        usize::try_from(pack_ref.len).map_err(|_| NodeError::Replication {
            reason: format!(
                "pack entry length {} exceeds local addressable capacity",
                pack_ref.len
            ),
        })?
    ];
    file.read_exact(bytes.as_mut_slice())
        .map_err(|err| NodeError::Replication {
            reason: format!("read {} payload failed: {}", path.display(), err),
        })?;
    Ok(bytes)
}

pub(super) fn prune_unreferenced_commit_message_pack_files(
    root_dir: &Path,
    cold_index: &CommitMessageColdIndex,
) -> Result<u64, NodeError> {
    let segments_dir = commit_message_cold_index_segments_dir(root_dir);
    if !segments_dir.exists() {
        return Ok(0);
    }
    let referenced_segments = cold_index
        .by_height
        .values()
        .filter_map(|entry| match entry {
            CommitMessageColdEntry::LegacyContentHash(_) => None,
            CommitMessageColdEntry::PackRef(pack_ref) => Some(pack_ref.segment_id.clone()),
        })
        .collect::<BTreeSet<_>>();
    let mut freed = 0u64;
    for entry in fs::read_dir(&segments_dir).map_err(|err| NodeError::Replication {
        reason: format!("read_dir {} failed: {}", segments_dir.display(), err),
    })? {
        let entry = entry.map_err(|err| NodeError::Replication {
            reason: format!("read_dir {} entry failed: {}", segments_dir.display(), err),
        })?;
        if !entry
            .file_type()
            .map_err(|err| NodeError::Replication {
                reason: format!("stat {} failed: {}", entry.path().display(), err),
            })?
            .is_file()
        {
            continue;
        }
        let path = entry.path();
        let Some(segment_id) = path.file_stem().and_then(|stem| stem.to_str()) else {
            continue;
        };
        if referenced_segments.contains(segment_id) {
            continue;
        }
        let size = entry.metadata().map(|meta| meta.len()).unwrap_or(0);
        fs::remove_file(&path).map_err(|err| NodeError::Replication {
            reason: format!("remove {} failed: {}", path.display(), err),
        })?;
        freed = freed.saturating_add(size);
    }
    Ok(freed)
}

fn list_hot_commit_message_files(root_dir: &Path) -> Result<Vec<(u64, PathBuf)>, NodeError> {
    let commit_dir = root_dir.join(COMMIT_MESSAGE_DIR);
    if !commit_dir.exists() {
        return Ok(Vec::new());
    }

    let mut commit_files = Vec::new();
    let entries = fs::read_dir(&commit_dir).map_err(|err| NodeError::Replication {
        reason: format!("list commit dir {} failed: {}", commit_dir.display(), err),
    })?;
    for entry in entries {
        let entry = entry.map_err(|err| NodeError::Replication {
            reason: format!("read commit dir entry failed: {}", err),
        })?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let Some(height_text) = file_name.strip_suffix(".json") else {
            continue;
        };
        let Ok(height) = height_text.parse::<u64>() else {
            continue;
        };
        if height == 0 {
            continue;
        }
        commit_files.push((height, path));
    }

    commit_files.sort_by_key(|(height, _)| *height);
    Ok(commit_files)
}

fn commit_message_path_from_root(root_dir: &Path, height: u64) -> PathBuf {
    root_dir
        .join(COMMIT_MESSAGE_DIR)
        .join(format!("{:020}.json", height))
}

fn commit_message_cold_index_root_dir(root_dir: &Path) -> PathBuf {
    root_dir.join(storage_cold_index_dir_name(COMMIT_MESSAGE_DIR))
}

fn commit_message_cold_index_manifest_path_from_root(root_dir: &Path) -> PathBuf {
    commit_message_cold_index_root_dir(root_dir).join(STORAGE_COLD_INDEX_MANIFEST_FILE)
}

fn commit_message_cold_index_segments_dir(root_dir: &Path) -> PathBuf {
    commit_message_cold_index_root_dir(root_dir).join(STORAGE_COLD_INDEX_SEGMENTS_DIR)
}

fn commit_message_cold_index_compat_alias_path_from_root(root_dir: &Path) -> PathBuf {
    root_dir.join("replication_commit_messages_cold_index.json")
}

fn normalize_commit_message_cold_index(
    mut cold_index: CommitMessageColdIndex,
) -> CommitMessageColdIndex {
    if cold_index.manifest.namespace.trim().is_empty() {
        cold_index.manifest.namespace = COMMIT_MESSAGE_DIR.to_string();
    }
    if cold_index.manifest.key_kind.trim().is_empty() {
        cold_index.manifest.key_kind = STORAGE_COLD_INDEX_KEY_KIND_HEIGHT.to_string();
    }
    if cold_index.manifest.value_kind.trim().is_empty()
        || cold_index.manifest.value_kind == STORAGE_COLD_INDEX_VALUE_KIND_CONTENT_HASH
    {
        cold_index.manifest.value_kind =
            infer_commit_message_cold_index_value_kind(&cold_index.by_height).to_string();
    }
    if cold_index.manifest.cold_range_anchor.is_none() {
        cold_index.manifest.cold_range_anchor = build_cold_range_anchor(&cold_index.by_height);
    }
    cold_index
}

fn build_cold_range_anchor(
    by_height: &BTreeMap<u64, CommitMessageColdEntry>,
) -> Option<StorageColdIndexRangeAnchor> {
    let ((from_key, first_entry), (to_key, last_entry)) =
        (by_height.iter().next()?, by_height.iter().next_back()?);
    Some(StorageColdIndexRangeAnchor {
        from_key: *from_key,
        to_key: *to_key,
        first_content_hash: first_entry.content_hash().to_string(),
        last_content_hash: last_entry.content_hash().to_string(),
        entry_count: by_height.len(),
    })
}

fn infer_commit_message_cold_index_value_kind(
    by_height: &BTreeMap<u64, CommitMessageColdEntry>,
) -> &'static str {
    if by_height
        .values()
        .all(|entry| matches!(entry, CommitMessageColdEntry::LegacyContentHash(_)))
    {
        STORAGE_COLD_INDEX_VALUE_KIND_CONTENT_HASH
    } else {
        STORAGE_COLD_INDEX_VALUE_KIND_COMMIT_PACK_REF
    }
}

fn migrate_legacy_cold_entries_to_packs(
    root_dir: &Path,
    cold_index: &mut CommitMessageColdIndex,
) -> Result<Vec<String>, NodeError> {
    let legacy_entries = cold_index
        .by_height
        .iter()
        .filter_map(|(height, entry)| {
            entry
                .legacy_content_hash()
                .map(|content_hash| (*height, content_hash.to_string()))
        })
        .collect::<Vec<_>>();
    if legacy_entries.is_empty() {
        return Ok(Vec::new());
    }

    let store = LocalCasStore::new(root_dir.join("store"));
    let mut migrated_hashes = Vec::with_capacity(legacy_entries.len());
    for (height, content_hash) in legacy_entries {
        let bytes =
            store
                .get_verified(content_hash.as_str())
                .map_err(|err| NodeError::Replication {
                    reason: format!(
                        "load legacy cold commit blob {} for height {} failed: {:?}",
                        content_hash, height, err
                    ),
                })?;
        let pack_ref = write_commit_message_pack_entry(
            root_dir,
            height,
            bytes.as_slice(),
            content_hash.as_str(),
        )?;
        cold_index
            .by_height
            .insert(height, CommitMessageColdEntry::PackRef(pack_ref));
        migrated_hashes.push(content_hash);
    }
    Ok(migrated_hashes)
}

fn delete_legacy_cold_commit_blobs_if_unreferenced(
    root_dir: &Path,
    migrated_hashes: &[String],
) -> Result<(), NodeError> {
    if migrated_hashes.is_empty() {
        return Ok(());
    }
    let store = LocalCasStore::new(root_dir.join("store"));
    let _retention = store
        .acquire_retention_guard()
        .map_err(super::distfs_error_to_node_error)?;
    let file_hashes = store
        .list_files()
        .map_err(|err| NodeError::Replication {
            reason: format!(
                "list store files for legacy cold blob cleanup failed: {:?}",
                err
            ),
        })?
        .into_iter()
        .map(|metadata| metadata.content_hash)
        .collect::<BTreeSet<_>>();
    let pin_hashes = store
        .list_effective_pins()
        .map_err(|err| NodeError::Replication {
            reason: format!(
                "list store pins for legacy cold blob cleanup failed: {:?}",
                err
            ),
        })?
        .into_iter()
        .collect::<BTreeSet<_>>();

    for content_hash in migrated_hashes {
        if file_hashes.contains(content_hash) || pin_hashes.contains(content_hash) {
            continue;
        }
        let path = store.blobs_dir().join(format!("{content_hash}.blob"));
        if !path.exists() {
            continue;
        }
        fs::remove_file(&path).map_err(|err| NodeError::Replication {
            reason: format!(
                "remove migrated legacy cold blob {} failed: {}",
                path.display(),
                err
            ),
        })?;
    }
    Ok(())
}

fn commit_message_pack_segment_id(height: u64) -> String {
    let normalized_height = height.max(1);
    let segment_start = ((normalized_height - 1) / COMMIT_MESSAGE_PACK_HEIGHT_SPAN)
        * COMMIT_MESSAGE_PACK_HEIGHT_SPAN
        + 1;
    let segment_end =
        segment_start.saturating_add(COMMIT_MESSAGE_PACK_HEIGHT_SPAN.saturating_sub(1));
    format!("{segment_start:020}-{segment_end:020}")
}

fn is_valid_commit_message_pack_segment_id(segment_id: &str) -> bool {
    if segment_id.len() != 41 {
        return false;
    }
    for (idx, ch) in segment_id.chars().enumerate() {
        if idx == 20 {
            if ch != '-' {
                return false;
            }
        } else if !ch.is_ascii_digit() {
            return false;
        }
    }
    true
}

fn commit_message_pack_segment_path_from_root(root_dir: &Path, segment_id: &str) -> PathBuf {
    commit_message_cold_index_segments_dir(root_dir).join(format!("{segment_id}.pack"))
}

fn load_json_or_default<T>(path: &Path) -> Result<T, NodeError>
where
    T: for<'de> Deserialize<'de> + Default,
{
    if !path.exists() {
        return Ok(T::default());
    }
    let bytes = fs::read(path).map_err(|err| NodeError::Replication {
        reason: format!("read {} failed: {}", path.display(), err),
    })?;
    serde_json::from_slice::<T>(&bytes).map_err(|err| NodeError::Replication {
        reason: format!("parse {} failed: {}", path.display(), err),
    })
}
