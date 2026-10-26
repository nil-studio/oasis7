use std::collections::BTreeMap;

use oasis7_proto::world_error::WorldError;
use serde::{Deserialize, Serialize};

use super::{
    FileIndexFile, FileMetadata, LocalCasStore, normalize_file_path, to_canonical_cbor,
    validate_hash,
};

const FILE_INDEX_MANIFEST_VERSION: u64 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileIndexManifest {
    pub version: u64,
    pub files: Vec<FileMetadata>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileIndexManifestRef {
    pub content_hash: String,
    pub size_bytes: u64,
}

impl LocalCasStore {
    pub fn export_file_index_manifest(&self) -> Result<FileIndexManifestRef, WorldError> {
        let _retention = self.lock_retention()?;
        let file_index = self.load_file_index()?;
        let files: Vec<FileMetadata> = file_index.files.into_values().collect();

        let manifest = FileIndexManifest {
            version: FILE_INDEX_MANIFEST_VERSION,
            files,
        };
        let bytes = to_canonical_cbor(&manifest)?;
        let content_hash = self.put_bytes_without_retention_lock(&bytes)?;
        Ok(FileIndexManifestRef {
            content_hash,
            size_bytes: bytes.len() as u64,
        })
    }

    pub fn import_file_index_manifest(
        &self,
        manifest_ref: &FileIndexManifestRef,
    ) -> Result<usize, WorldError> {
        let _retention = self.lock_retention()?;
        validate_hash(manifest_ref.content_hash.as_str())?;
        let bytes = self.get_verified(manifest_ref.content_hash.as_str())?;
        if bytes.len() as u64 != manifest_ref.size_bytes {
            return Err(WorldError::DistributedValidationFailed {
                reason: format!(
                    "manifest size mismatch: expected={} actual={}",
                    manifest_ref.size_bytes,
                    bytes.len()
                ),
            });
        }

        let manifest: FileIndexManifest = serde_cbor::from_slice(&bytes)?;
        if manifest.version != FILE_INDEX_MANIFEST_VERSION {
            return Err(WorldError::DistributedValidationFailed {
                reason: format!(
                    "unsupported file index manifest version: expected={} actual={}",
                    FILE_INDEX_MANIFEST_VERSION, manifest.version
                ),
            });
        }

        let mut files = BTreeMap::new();
        for metadata in manifest.files {
            let normalized_path = normalize_file_path(metadata.path.as_str())?;
            if normalized_path != metadata.path {
                return Err(WorldError::DistributedValidationFailed {
                    reason: format!(
                        "manifest path is not normalized: original={} normalized={}",
                        metadata.path, normalized_path
                    ),
                });
            }
            validate_hash(metadata.content_hash.as_str())?;
            let blob_bytes = self.get_verified(metadata.content_hash.as_str())?;
            if blob_bytes.len() as u64 != metadata.size_bytes {
                return Err(WorldError::DistributedValidationFailed {
                    reason: format!(
                        "manifest file size mismatch path={} expected={} actual={}",
                        metadata.path,
                        metadata.size_bytes,
                        blob_bytes.len()
                    ),
                });
            }
            if files.insert(metadata.path.clone(), metadata).is_some() {
                return Err(WorldError::DistributedValidationFailed {
                    reason: format!("duplicate manifest path: {}", normalized_path),
                });
            }
        }

        let file_count = files.len();
        self.save_file_index(&FileIndexFile::from_files(files))?;
        Ok(file_count)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;
    use crate::FileStore;

    fn temp_dir(prefix: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("duration")
            .as_nanos();
        std::env::temp_dir().join(format!("oasis7-distfs-manifest-{prefix}-{unique}"))
    }

    #[test]
    fn file_index_manifest_export_import_restores_index() {
        let dir = temp_dir("roundtrip");
        let store = LocalCasStore::new(&dir);

        store.write_file("docs/b.txt", b"beta").expect("write b");
        store.write_file("docs/a.txt", b"alpha").expect("write a");
        let manifest_ref = store.export_file_index_manifest().expect("export");
        let manifest_bytes = store
            .get_verified(manifest_ref.content_hash.as_str())
            .expect("manifest bytes");
        let manifest: FileIndexManifest =
            serde_cbor::from_slice(manifest_bytes.as_slice()).expect("manifest");
        let manifest_paths: Vec<_> = manifest
            .files
            .iter()
            .map(|metadata| metadata.path.as_str())
            .collect();
        assert_eq!(manifest_paths, vec!["docs/a.txt", "docs/b.txt"]);

        let removed = fs::remove_file(store.files_index_path());
        assert!(removed.is_ok());
        assert!(store.list_files().expect("list after remove").is_empty());

        let imported = store
            .import_file_index_manifest(&manifest_ref)
            .expect("import manifest");
        assert_eq!(imported, 2);

        let files = store.list_files().expect("list files");
        assert_eq!(files.len(), 2);
        assert_eq!(store.read_file("docs/a.txt").expect("read a"), b"alpha");
        assert_eq!(store.read_file("docs/b.txt").expect("read b"), b"beta");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn file_index_manifest_import_rejects_missing_blob() {
        let dir = temp_dir("missing-blob");
        let store = LocalCasStore::new(&dir);

        let metadata = store
            .write_file("docs/a.txt", b"alpha")
            .expect("write file");
        let manifest_ref = store.export_file_index_manifest().expect("export");

        let blob_path = store
            .blob_path(metadata.content_hash.as_str())
            .expect("blob path");
        fs::remove_file(blob_path).expect("remove blob");

        let imported = store.import_file_index_manifest(&manifest_ref);
        assert!(matches!(
            imported,
            Err(WorldError::BlobNotFound { .. })
                | Err(WorldError::DistributedValidationFailed { .. })
        ));

        let _ = fs::remove_dir_all(&dir);
    }
}
