//! Explicit offline repair; source authority remains the trusted caller's duty.
use super::*;

impl LocalCasStore {
    /// Restore only a corrupt existing member from independently authorized bytes.
    /// Returns the preserved disk evidence path; None means it was already valid.
    /// Stop users of this store first. This does not authorize world rollback.
    pub fn restore_verified_blob(
        &self,
        content_hash: &str,
        bytes: &[u8],
    ) -> Result<Option<PathBuf>, WorldError> {
        let path = self.blob_path(content_hash)?;
        let actual = self.hash_hex(bytes);
        if actual != content_hash {
            return Err(WorldError::BlobHashMismatch {
                expected: content_hash.into(),
                actual,
            });
        }
        let _retention = self.lock_retention()?;
        let metadata = fs::symlink_metadata(&path)?;
        if !metadata.file_type().is_file() {
            return Err(WorldError::DistributedValidationFailed {
                reason: "restore requires an existing regular CAS member".into(),
            });
        }
        match self.get_verified(content_hash) {
            Ok(_) => return Ok(None),
            Err(WorldError::BlobHashMismatch { .. }) => {}
            Err(error) => return Err(error),
        }
        // Preserve exact disk bytes (possibly compressed), without moving the
        // source until the evidence copy has been synced successfully.
        let damaged = fs::read(&path)?;
        let quarantine = self.root.join("quarantine");
        let mut directory = fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            directory.mode(0o700);
        }
        match directory.create(&quarantine) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.into()),
        }
        if !fs::symlink_metadata(&quarantine)?.file_type().is_dir() {
            return Err(WorldError::DistributedValidationFailed {
                reason: "quarantine must be a regular directory".into(),
            });
        }
        let evidence = unique_atomic_temp_path(&quarantine.join(content_hash));
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&evidence)?;
        file.write_all(&damaged)?;
        file.sync_all()?;
        drop(file);
        #[cfg(unix)]
        fs::File::open(&quarantine)?.sync_all()?;
        let disk_bytes = self.maybe_encode_blob_for_disk(content_hash, bytes)?;
        write_bytes_atomic(&path, &disk_bytes)?;
        self.get_verified(content_hash)?;
        Ok(Some(evidence))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_restore_requires_exact_bytes_and_preserves_damage() {
        let root = crate::tests::temp_dir("restore-exact");
        let store = LocalCasStore::new(&root);
        let bytes = b"original committed payload";
        let hash = store.put_bytes(bytes).unwrap();
        let path = store.blob_path(&hash).unwrap();
        fs::write(&path, b"").unwrap();
        assert!(
            store
                .restore_verified_blob(&hash, b"replacement intent")
                .is_err()
        );
        assert_eq!(fs::read(&path).unwrap(), b"");
        let evidence = store.restore_verified_blob(&hash, bytes).unwrap().unwrap();
        assert_eq!(fs::read(&evidence).unwrap(), b"");
        assert_eq!(store.get_verified(&hash).unwrap(), bytes);
        assert!(store.restore_verified_blob(&hash, bytes).unwrap().is_none());
        assert_eq!(fs::read_dir(root.join("quarantine")).unwrap().count(), 1);
        fs::remove_file(&path).unwrap();
        assert!(store.restore_verified_blob(&hash, bytes).is_err());
        assert!(!path.exists());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn restore_preserves_compression_and_configured_hash_algorithm() {
        for algorithm in [HashAlgorithm::Blake3, HashAlgorithm::Sha256] {
            let root = crate::tests::temp_dir("restore-compressed");
            let store = LocalCasStore::new_with_hash_algorithm(&root, algorithm);
            let bytes = vec![11; 16384];
            let hash = store.put_bytes(&bytes).unwrap();
            let path = store.blob_path(&hash).unwrap();
            fs::write(&path, b"damaged").unwrap();
            let evidence = store.restore_verified_blob(&hash, &bytes).unwrap().unwrap();
            assert_eq!(fs::read(evidence).unwrap(), b"damaged");
            assert_eq!(store.get_verified(&hash).unwrap(), bytes);
            assert!(fs::metadata(path).unwrap().len() < bytes.len() as u64);
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[cfg(unix)]
    #[test]
    fn restore_rejects_symlink_without_touching_target() {
        let root = crate::tests::temp_dir("restore-symlink");
        let store = LocalCasStore::new(&root);
        let bytes = b"original";
        let hash = store.put_bytes(bytes).unwrap();
        let path = store.blob_path(&hash).unwrap();
        let external = root.join("external");
        fs::write(&external, b"preserve").unwrap();
        fs::remove_file(&path).unwrap();
        std::os::unix::fs::symlink(&external, &path).unwrap();
        assert!(store.restore_verified_blob(&hash, bytes).is_err());
        assert_eq!(fs::read(&external).unwrap(), b"preserve");
        assert!(!root.join("quarantine").exists());
        fs::remove_file(&path).unwrap();
        fs::write(&path, b"damaged").unwrap();
        let external_dir = root.join("external-dir");
        fs::create_dir(&external_dir).unwrap();
        std::os::unix::fs::symlink(&external_dir, root.join("quarantine")).unwrap();
        assert!(store.restore_verified_blob(&hash, bytes).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"damaged");
        assert_eq!(fs::read_dir(&external_dir).unwrap().count(), 0);
        fs::remove_dir_all(root).unwrap();
    }
}
