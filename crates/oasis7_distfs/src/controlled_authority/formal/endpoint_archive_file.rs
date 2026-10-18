//! Read-only, bounded CAS bytes. This adapter supplies no authority or absence
//! proof; only roots from a verified protected seal authorize referenced objects.
use super::*;
use std::fs;
use std::path::PathBuf;
#[cfg(test)]
const OBJECT_LIMIT: u64 = 64 * 1024 * 1024;
pub struct FileFormalArchiveSourceV1 {
    directory: PathBuf,
}
impl FileFormalArchiveSourceV1 {
    /// `directory` is an endpoint archive root, containing formal-objects-v1.
    /// This neither copies/deletes files nor signs a checkpoint or seal.
    pub fn new(directory: PathBuf) -> Result<Self, FormalError> {
        for path in [&directory, &directory.join("formal-objects-v1")] {
            let metadata =
                fs::symlink_metadata(path).map_err(|e| FormalError::Io(e.to_string()))?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt;
                // Legacy archives may be publicly readable (0755/0644), but
                // only the registered effective UID may write their tree.
                if metadata.uid() != unsafe { libc::geteuid() } || metadata.mode() & 0o022 != 0 {
                    return Err(invalid("formal archive registered owner/write mode"));
                }
            }
            if !metadata.is_dir() || metadata.file_type().is_symlink() {
                return Err(invalid("formal archive source regular directory"));
            }
        }
        Ok(Self {
            directory: directory.join("formal-objects-v1"),
        })
    }
    /// Representation-independent inventory of at most 65,536 local logical hashes.
    /// This reads no authority/head and rejects unknown namespace entries.
    pub fn logical_ids(&self) -> Result<Vec<String>, FormalError> {
        const MAX_IDS: usize = 65_536;
        let _pin = super::super::physical::pin(&self.directory, false)?;
        if super::super::physical::enabled(&self.directory)? {
            return super::super::physical::logical_ids(&self.directory)
                .map(|ids| ids.into_iter().collect());
        }
        // Legacy raw/pack archives have no physical selector namespace. Keep
        // this compatibility local to the archive reader, not generation GC.
        let mut ids = Vec::new();
        for entry in fs::read_dir(&self.directory).map_err(|e| FormalError::Io(e.to_string()))? {
            let entry = entry.map_err(|e| FormalError::Io(e.to_string()))?;
            let id = entry
                .file_name()
                .into_string()
                .map_err(|_| invalid("formal archive logical filename encoding"))?;
            let metadata =
                fs::symlink_metadata(entry.path()).map_err(|e| FormalError::Io(e.to_string()))?;
            if id == ".business-pack-staging-v1" && metadata.is_dir() {
                super::super::physical::paths::metadata(&metadata, true)?;
                continue;
            }
            super::super::physical::paths::metadata(&metadata, false)?;
            hash_hex(&id)?;
            if !metadata.is_file() || ids.len() >= MAX_IDS {
                return Err(invalid("formal archive logical inventory type/count"));
            }
            ids.push(id);
        }
        ids.sort();
        Ok(ids)
    }
}
impl FormalArchiveSourceV1 for FileFormalArchiveSourceV1 {
    fn read_range(
        &self,
        id: &str,
        offset: u64,
        limit: u32,
    ) -> Result<Option<FormalArchiveChunkV1>, FormalError> {
        chunk::file_range(&self.directory.join(id), id, offset, limit)
    }
    fn read_object(&self, id: &str) -> Result<Option<Vec<u8>>, FormalError> {
        super::super::business_pack::read_object(&self.directory.join(id), id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    #[test]
    fn read_only_cas_checks_missing_corrupt_oversize_and_symlink_objects() {
        let root = std::env::temp_dir().canonicalize().unwrap().join(format!(
            "oasis7-formal-file-cas-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let objects = root.join("formal-objects-v1");
        fs::create_dir_all(&objects).unwrap();
        let source = FileFormalArchiveSourceV1::new(root.clone()).unwrap();
        let bytes = b"archived-original";
        let id = blake3::hash(bytes).to_hex().to_string();
        assert_eq!(source.read_object(&id).unwrap(), None);
        fs::write(objects.join(&id), bytes).unwrap();
        assert_eq!(source.logical_ids().unwrap(), vec![id.clone()]);
        assert_eq!(source.read_object(&id).unwrap().unwrap(), bytes);
        fs::write(objects.join(".unknown-metadata"), b"unknown").unwrap();
        assert!(source.logical_ids().is_err());
        fs::remove_file(objects.join(".unknown-metadata")).unwrap();
        fs::write(objects.join(&id), b"corrupt").unwrap();
        assert!(source.read_object(&id).is_err());
        let file = fs::File::create(objects.join(&id)).unwrap();
        file.set_len(OBJECT_LIMIT + 1).unwrap();
        drop(file);
        assert!(source.read_object(&id).is_err());
        assert!(source.read_object("../anything").is_err());
        #[cfg(unix)]
        {
            fs::remove_file(objects.join(&id)).unwrap();
            let actual = root.join("outside");
            fs::write(&actual, bytes).unwrap();
            std::os::unix::fs::symlink(actual, objects.join(&id)).unwrap();
            assert!(source.read_object(&id).is_err());
        }
        fs::remove_dir_all(root).unwrap();
    }
    #[cfg(unix)]
    #[test]
    fn logical_inventory_reads_selector_only_and_selected_generation_objects() {
        use crate::controlled_authority::formal::{business_index, business_pack, physical};
        use std::os::unix::fs::DirBuilderExt;
        let root = std::env::temp_dir().canonicalize().unwrap().join(format!(
            "oasis7-formal-logical-inventory-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let objects = root.join("formal-objects-v1");
        fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
        fs::DirBuilder::new().mode(0o700).create(&objects).unwrap();
        physical::initialize(&objects).unwrap();
        let bytes = vec![7; 200_000];
        let id = blake3::hash(&bytes).to_hex().to_string();
        business_pack::persist(
            &objects,
            &[business_index::BusinessIndexNodeWriteV1 {
                digest: id.clone(),
                bytes: bytes.clone(),
            }],
        )
        .unwrap();
        assert!(!objects.join(&id).exists());
        let source = FileFormalArchiveSourceV1::new(root.clone()).unwrap();
        assert_eq!(source.logical_ids().unwrap(), vec![id.clone()]);
        let mut selected = false;
        for _ in 0..32 {
            if !physical::advance_generation(&objects).unwrap().building {
                selected = true;
                break;
            }
        }
        assert!(selected, "generation must finish within the fixture budget");
        assert_eq!(source.logical_ids().unwrap(), vec![id.clone()]);
        assert_eq!(source.read_object(&id).unwrap().unwrap(), bytes);
        fs::remove_dir_all(root).unwrap();
    }
    #[cfg(unix)]
    #[test]
    fn registered_archive_rejects_other_uid_writable_directory() {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().canonicalize().unwrap().join(format!(
            "oasis7-formal-directory-mode-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        fs::create_dir(root.join("formal-objects-v1")).unwrap();
        // Read visibility is legacy-compatible; writer ownership is mandatory.
        fs::set_permissions(&root, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(FileFormalArchiveSourceV1::new(root.clone()).is_ok());
        fs::set_permissions(
            root.join("formal-objects-v1"),
            fs::Permissions::from_mode(0o775),
        )
        .unwrap();
        assert!(FileFormalArchiveSourceV1::new(root.clone()).is_err());
        fs::set_permissions(
            root.join("formal-objects-v1"),
            fs::Permissions::from_mode(0o700),
        )
        .unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o777)).unwrap();
        assert!(FileFormalArchiveSourceV1::new(root.clone()).is_err());
        fs::remove_dir_all(root).unwrap();
    }
}
