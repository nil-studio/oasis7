//! Local content-addressed storage, verified blob codecs and file/pin indexes.

use oasis7_proto::world_error::WorldError;
use serde::de::{DeserializeOwned, Deserializer};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use zstd::stream::{decode_all as zstd_decode_all, encode_all as zstd_encode_all};
const BLOBS_DIR: &str = "blobs";
const PINS_FILE: &str = "pins.json";
const PIN_SCOPES_DIR: &str = "pin_scopes";
const FILES_INDEX_FILE: &str = "files_index.json";
const FILE_INDEX_VERSION: u64 = 1;
const COMPRESSED_BLOB_MAGIC: &[u8; 8] = b"O7CBLOB1";
const COMPRESSED_BLOB_HASH_HEX_LEN: usize = 64;
const COMPRESSED_BLOB_HEADER_LEN: usize = 16 + COMPRESSED_BLOB_HASH_HEX_LEN;
const COMPRESSIBLE_BLOB_MIN_BYTES: usize = 1024;
const COMPRESSED_BLOB_ZSTD_LEVEL: i32 = 3;
const COMPRESSED_RANGE_CACHE_ENTRY_MAX_BYTES: u64 = compressed_range_cache::ENTRY_MAX_BYTES;
mod local_blob_codec;
pub use local_blob_codec::{BlobReadReceipt, decode_local_blob_into};
mod challenge;
mod challenge_scheduler;
mod compressed_range_cache;
pub use oasis7_proto::memory_budget;
mod manifest;
mod pin_store;
mod retention;
pub use retention::CasRetentionGuard;
#[cfg(test)]
mod retention_tests;
mod segmentation;

pub use challenge::*;
pub use challenge_scheduler::*;
pub use manifest::*;
pub use segmentation::{assemble_journal, assemble_snapshot, segment_journal, segment_snapshot};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HashAlgorithm {
    Blake3,
    Sha256,
}

pub trait BlobStore {
    fn put(&self, content_hash: &str, bytes: &[u8]) -> Result<(), WorldError>;
    fn get(&self, content_hash: &str) -> Result<Vec<u8>, WorldError>;
    fn has(&self, content_hash: &str) -> Result<bool, WorldError>;

    fn put_bytes(&self, bytes: &[u8]) -> Result<String, WorldError> {
        let content_hash = blake3_hex(bytes);
        self.put(&content_hash, bytes)?;
        Ok(content_hash)
    }
}

pub trait FileStore {
    fn write_file(&self, path: &str, bytes: &[u8]) -> Result<FileMetadata, WorldError>;
    fn read_file(&self, path: &str) -> Result<Vec<u8>, WorldError>;
    fn delete_file(&self, path: &str) -> Result<bool, WorldError>;
    fn stat_file(&self, path: &str) -> Result<Option<FileMetadata>, WorldError>;
    fn list_files(&self) -> Result<Vec<FileMetadata>, WorldError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileMetadata {
    pub path: String,
    pub content_hash: String,
    pub size_bytes: u64,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct FileIndexAuditReport {
    pub total_indexed_files: usize,
    pub total_pins: usize,
    pub missing_file_blob_hashes: Vec<String>,
    pub dangling_pin_hashes: Vec<String>,
    pub orphan_blob_hashes: Vec<String>,
}

impl FileIndexAuditReport {
    pub fn is_clean(&self) -> bool {
        self.missing_file_blob_hashes.is_empty()
            && self.dangling_pin_hashes.is_empty()
            && self.orphan_blob_hashes.is_empty()
    }
}

struct BlobReferenceSets {
    total_indexed_files: usize,
    total_pins: usize,
    indexed_hashes: BTreeSet<String>,
    pinned_hashes: BTreeSet<String>,
    blob_hashes: BTreeSet<String>,
}

#[derive(Debug, Clone)]
pub struct LocalCasStore {
    root: PathBuf,
    blobs_dir: PathBuf,
    pins_path: PathBuf,
    hash_algorithm: HashAlgorithm,
    files_index_path: PathBuf,
}

mod store;

fn read_range(
    reader: &mut impl Read,
    offset: u64,
    limit: usize,
    raw_len: u64,
) -> Result<(Vec<u8>, bool), WorldError> {
    if offset >= raw_len {
        return Ok((Vec::new(), true));
    }
    let mut skipped = 0u64;
    let mut discard = [0u8; 8192];
    while skipped < offset {
        let remaining = usize::try_from(offset.saturating_sub(skipped)).unwrap_or(usize::MAX);
        let discard_len = discard.len().min(remaining);
        let read = reader.read(&mut discard[..discard_len])?;
        if read == 0 {
            return Ok((Vec::new(), true));
        }
        skipped = skipped.saturating_add(read as u64);
    }
    let remaining = usize::try_from(raw_len.saturating_sub(offset)).unwrap_or(usize::MAX);
    let bytes_to_read = limit.min(remaining);
    let mut bytes = vec![0; bytes_to_read];
    reader.read_exact(bytes.as_mut_slice())?;
    Ok((
        bytes,
        offset.saturating_add(bytes_to_read as u64) >= raw_len,
    ))
}

impl BlobStore for LocalCasStore {
    fn put(&self, content_hash: &str, bytes: &[u8]) -> Result<(), WorldError> {
        let _retention = self.lock_retention()?;
        self.put_without_retention_lock(content_hash, bytes)
    }

    fn put_bytes(&self, bytes: &[u8]) -> Result<String, WorldError> {
        let content_hash = self.hash_hex(bytes);
        self.put(&content_hash, bytes)?;
        Ok(content_hash)
    }

    fn get(&self, content_hash: &str) -> Result<Vec<u8>, WorldError> {
        let path = self.blob_path(content_hash)?;
        if !path.exists() {
            return Err(WorldError::BlobNotFound {
                content_hash: content_hash.to_string(),
            });
        }
        let disk_bytes = fs::read(path)?;
        Ok(self.decode_blob_from_disk(content_hash, disk_bytes))
    }

    fn has(&self, content_hash: &str) -> Result<bool, WorldError> {
        let path = self.blob_path(content_hash)?;
        Ok(path.exists())
    }
}

impl FileStore for LocalCasStore {
    fn write_file(&self, path: &str, bytes: &[u8]) -> Result<FileMetadata, WorldError> {
        let _retention = self.lock_retention()?;
        let normalized_path = normalize_file_path(path)?;
        let content_hash = self.put_bytes_without_retention_lock(bytes)?;
        let metadata = FileMetadata {
            path: normalized_path.clone(),
            content_hash,
            size_bytes: bytes.len() as u64,
            updated_at_ms: now_unix_time_ms(),
        };
        let mut file_index = self.load_file_index()?;
        let replaced_hash = file_index
            .files
            .insert(normalized_path, metadata.clone())
            .map(|previous| previous.content_hash);
        if let Some(replaced_hash) = replaced_hash.as_deref() {
            file_index.decrement_content_hash_ref_count(replaced_hash);
        }
        file_index.increment_content_hash_ref_count(metadata.content_hash.as_str());
        self.save_file_index(&file_index)?;
        if let Some(replaced_hash) = replaced_hash
            && replaced_hash != metadata.content_hash
        {
            self.remove_blob_if_unreferenced_in_index(&file_index, replaced_hash.as_str())?;
        }
        Ok(metadata)
    }

    fn read_file(&self, path: &str) -> Result<Vec<u8>, WorldError> {
        let normalized_path = normalize_file_path(path)?;
        let file_index = self.load_file_index()?;
        let metadata = file_index.files.get(&normalized_path).ok_or_else(|| {
            WorldError::DistributedValidationFailed {
                reason: format!("file not found: {normalized_path}"),
            }
        })?;
        self.get(&metadata.content_hash)
    }

    fn delete_file(&self, path: &str) -> Result<bool, WorldError> {
        let _retention = self.lock_retention()?;
        let normalized_path = normalize_file_path(path)?;
        let mut file_index = self.load_file_index()?;
        let removed_hash = file_index
            .files
            .remove(&normalized_path)
            .map(|metadata| metadata.content_hash);
        if let Some(removed_hash) = removed_hash {
            file_index.decrement_content_hash_ref_count(removed_hash.as_str());
            self.save_file_index(&file_index)?;
            self.remove_blob_if_unreferenced_in_index(&file_index, removed_hash.as_str())?;
            return Ok(true);
        }
        Ok(false)
    }

    fn stat_file(&self, path: &str) -> Result<Option<FileMetadata>, WorldError> {
        let normalized_path = normalize_file_path(path)?;
        let file_index = self.load_file_index()?;
        Ok(file_index.files.get(&normalized_path).cloned())
    }

    fn list_files(&self) -> Result<Vec<FileMetadata>, WorldError> {
        let file_index = self.load_file_index()?;
        Ok(file_index.files.values().cloned().collect())
    }
}

pub fn blake3_hex(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}

#[doc(hidden)]
pub fn validate_hash(content_hash: &str) -> Result<(), WorldError> {
    if content_hash.is_empty()
        || content_hash.contains('/')
        || content_hash.contains('\\')
        || content_hash.contains("..")
    {
        return Err(WorldError::BlobHashInvalid {
            content_hash: content_hash.to_string(),
        });
    }
    Ok(())
}

fn validate_pin_path_component(value: &str, label: &str) -> Result<(), WorldError> {
    if value.is_empty()
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(WorldError::DistributedValidationFailed {
            reason: format!("invalid {label}: {value}"),
        });
    }
    Ok(())
}

fn normalize_file_path(path: &str) -> Result<String, WorldError> {
    if path.is_empty() {
        return Err(WorldError::DistributedValidationFailed {
            reason: "invalid file path: empty path".to_string(),
        });
    }
    let raw_path = Path::new(path);
    if raw_path.is_absolute() {
        return Err(WorldError::DistributedValidationFailed {
            reason: format!("invalid file path: absolute path not allowed ({path})"),
        });
    }

    let mut normalized = Vec::new();
    for component in raw_path.components() {
        match component {
            std::path::Component::Normal(part) => {
                let segment =
                    part.to_str()
                        .ok_or_else(|| WorldError::DistributedValidationFailed {
                            reason: format!("invalid file path: non-utf8 segment ({path})"),
                        })?;
                normalized.push(segment.to_string());
            }
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                return Err(WorldError::DistributedValidationFailed {
                    reason: format!("invalid file path: parent traversal not allowed ({path})"),
                });
            }
            std::path::Component::RootDir | std::path::Component::Prefix(_) => {
                return Err(WorldError::DistributedValidationFailed {
                    reason: format!("invalid file path: absolute path not allowed ({path})"),
                });
            }
        }
    }
    if normalized.is_empty() {
        return Err(WorldError::DistributedValidationFailed {
            reason: format!("invalid file path: no path segment ({path})"),
        });
    }
    Ok(normalized.join("/"))
}

fn normalize_expected_content_hash(
    expected_content_hash: Option<&str>,
) -> Result<Option<String>, WorldError> {
    let Some(content_hash) = expected_content_hash else {
        return Ok(None);
    };
    validate_hash(content_hash)?;
    Ok(Some(content_hash.to_string()))
}

fn ensure_file_hash_precondition(
    file_index: &FileIndexFile,
    normalized_path: &str,
    expected_content_hash: Option<&str>,
) -> Result<(), WorldError> {
    let Some(expected_content_hash) = expected_content_hash else {
        return Ok(());
    };
    let current = file_index.files.get(normalized_path).ok_or_else(|| {
        WorldError::DistributedValidationFailed {
            reason: format!(
                "file precondition failed: path missing for expected hash path={} expected={}",
                normalized_path, expected_content_hash
            ),
        }
    })?;
    if current.content_hash != expected_content_hash {
        return Err(WorldError::DistributedValidationFailed {
            reason: format!(
                "file precondition failed: hash mismatch path={} expected={} actual={}",
                normalized_path, expected_content_hash, current.content_hash
            ),
        });
    }
    Ok(())
}

fn now_unix_time_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| i64::try_from(duration.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
struct PinFile {
    #[serde(default)]
    pins: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct FileIndexFile {
    #[serde(default = "default_file_index_version")]
    version: u64,
    #[serde(default)]
    files: BTreeMap<String, FileMetadata>,
    #[serde(default)]
    content_hash_ref_counts: BTreeMap<String, u64>,
}

impl Default for FileIndexFile {
    fn default() -> Self {
        Self {
            version: FILE_INDEX_VERSION,
            files: BTreeMap::new(),
            content_hash_ref_counts: BTreeMap::new(),
        }
    }
}

fn default_file_index_version() -> u64 {
    FILE_INDEX_VERSION
}

impl<'de> Deserialize<'de> for FileIndexFile {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct RawFileIndexFile {
            #[serde(default = "default_file_index_version")]
            version: u64,
            #[serde(default)]
            files: BTreeMap<String, FileMetadata>,
            #[serde(default)]
            content_hash_ref_counts: Option<BTreeMap<String, u64>>,
        }

        let raw = RawFileIndexFile::deserialize(deserializer)?;
        Ok(match raw.content_hash_ref_counts {
            Some(content_hash_ref_counts) => Self {
                version: raw.version,
                files: raw.files,
                content_hash_ref_counts,
            },
            None => Self::from_files_with_version(raw.version, raw.files),
        })
    }
}

impl FileIndexFile {
    fn from_files(files: BTreeMap<String, FileMetadata>) -> Self {
        Self::from_files_with_version(FILE_INDEX_VERSION, files)
    }

    fn from_files_with_version(version: u64, files: BTreeMap<String, FileMetadata>) -> Self {
        let mut file_index = Self {
            version,
            files,
            content_hash_ref_counts: BTreeMap::new(),
        };
        file_index.repair_content_hash_ref_counts();
        file_index
    }

    fn repair_content_hash_ref_counts(&mut self) {
        let expected = self.calculated_content_hash_ref_counts();
        if self.content_hash_ref_counts != expected {
            self.content_hash_ref_counts = expected;
        }
    }

    fn calculated_content_hash_ref_counts(&self) -> BTreeMap<String, u64> {
        let mut counts = BTreeMap::new();
        for metadata in self.files.values() {
            let count = counts.entry(metadata.content_hash.clone()).or_insert(0_u64);
            *count = count.saturating_add(1);
        }
        counts
    }

    fn content_hash_ref_count(&self, content_hash: &str) -> u64 {
        self.content_hash_ref_counts
            .get(content_hash)
            .copied()
            .unwrap_or(0)
    }

    fn increment_content_hash_ref_count(&mut self, content_hash: &str) {
        let count = self
            .content_hash_ref_counts
            .entry(content_hash.to_string())
            .or_insert(0_u64);
        *count = count.saturating_add(1);
    }

    fn decrement_content_hash_ref_count(&mut self, content_hash: &str) {
        match self.content_hash_ref_counts.get_mut(content_hash) {
            Some(count) if *count > 1 => {
                *count -= 1;
            }
            Some(_) => {
                self.content_hash_ref_counts.remove(content_hash);
            }
            None => {}
        }
    }
}

#[derive(Debug, Clone)]
struct BlobEntry {
    hash: String,
    path: PathBuf,
    size: u64,
    modified: SystemTime,
}

fn unique_atomic_temp_path(path: &Path) -> PathBuf {
    static NEXT_TEMP_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let sequence = NEXT_TEMP_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let pid = std::process::id();
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    path.with_extension(format!("tmp-{pid}-{nanos}-{sequence}"))
}

fn write_bytes_atomic(path: &Path, bytes: &[u8]) -> Result<(), WorldError> {
    for _ in 0..16 {
        let tmp = unique_atomic_temp_path(path);
        let mut file = match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)
        {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        };
        let result = file.write_all(bytes).and_then(|()| {
            drop(file);
            fs::rename(&tmp, path)
        });
        if result.is_err() {
            let _ = fs::remove_file(&tmp);
        }
        return result.map_err(WorldError::from);
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        format!(
            "cannot reserve atomic temporary file for {}",
            path.display()
        ),
    )
    .into())
}

fn write_json_atomic<T: Serialize>(value: &T, path: &Path) -> Result<(), WorldError> {
    let bytes = serde_json::to_vec(value)?;
    write_bytes_atomic(path, &bytes)
}

fn read_json_from_path<T: DeserializeOwned>(path: &Path) -> Result<T, WorldError> {
    let bytes = fs::read(path)?;
    Ok(serde_json::from_slice(&bytes)?)
}

#[doc(hidden)]
pub fn to_canonical_cbor<T: Serialize>(value: &T) -> Result<Vec<u8>, WorldError> {
    let mut buf = Vec::with_capacity(256);
    let canonical_value = serde_cbor::value::to_value(value)?;
    let mut serializer = serde_cbor::ser::Serializer::new(&mut buf);
    serializer.self_describe()?;
    canonical_value.serialize(&mut serializer)?;
    Ok(buf)
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;

#[cfg(test)]
mod segmentation_tests;
