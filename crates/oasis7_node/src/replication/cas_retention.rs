//! Local CAS retention and corruption repair for node replication.
use super::*;

impl ReplicationRuntime {
    pub(super) fn prune_store_orphans(&self) -> Result<u64, NodeError> {
        // Loading the cold index can migrate legacy CAS members and acquire
        // the retention guard itself. Finish that before this transaction.
        let cold_index = load_commit_message_cold_index_from_root(self.config.root_dir.as_path())?;
        let _retention = self
            .store
            .acquire_retention_guard()
            .map_err(distfs_error_to_node_error)?;
        let mut referenced_hashes = BTreeSet::new();
        for metadata in self
            .store
            .list_files()
            .map_err(distfs_error_to_node_error)?
        {
            referenced_hashes.insert(metadata.content_hash);
        }
        for pinned_hash in self
            .store
            .list_effective_pins()
            .map_err(distfs_error_to_node_error)?
        {
            referenced_hashes.insert(pinned_hash);
        }
        referenced_hashes.extend(
            cold_index
                .by_height
                .values()
                .filter_map(|entry| match entry {
                    CommitMessageColdEntry::LegacyContentHash(content_hash) => {
                        Some(content_hash.to_string())
                    }
                    CommitMessageColdEntry::PackRef(_) => None,
                }),
        );

        let blobs_dir = self.config.root_dir.join("store").join("blobs");
        if !blobs_dir.exists() {
            return Ok(0);
        }

        let mut freed = 0u64;
        for entry in fs::read_dir(&blobs_dir).map_err(|err| NodeError::Replication {
            reason: format!("read_dir {} failed: {}", blobs_dir.display(), err),
        })? {
            let entry = entry.map_err(|err| NodeError::Replication {
                reason: format!("read_dir {} entry failed: {}", blobs_dir.display(), err),
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
            if path.extension().and_then(|ext| ext.to_str()) != Some("blob") {
                continue;
            }
            let Some(content_hash) = path.file_stem().and_then(|stem| stem.to_str()) else {
                return Err(NodeError::Replication {
                    reason: format!("invalid blob file name: {}", path.display()),
                });
            };
            if referenced_hashes.contains(content_hash) {
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

    pub(crate) fn discard_blob_by_hash(&self, content_hash: &str) -> Result<bool, NodeError> {
        let _retention = self
            .store
            .acquire_retention_guard()
            .map_err(distfs_error_to_node_error)?;
        // A corrupt CAS member must be removed before a verified refetch: the
        // store intentionally treats an existing path as immutable, so put()
        // would otherwise leave the corrupt bytes in place.  Validate the
        // hash through the store API before constructing the exact blob path.
        if !self
            .store
            .has(content_hash)
            .map_err(distfs_error_to_node_error)?
        {
            return Ok(false);
        }
        // A different repair may have won since the caller observed invalid
        // bytes. Never discard a now-valid member, including a pinned one.
        match self.store.get_verified(content_hash) {
            Ok(_) => return Ok(false),
            Err(oasis7_proto::world_error::WorldError::BlobHashMismatch { .. }) => {}
            Err(error) => return Err(distfs_error_to_node_error(error)),
        }
        let path = self.store.blobs_dir().join(format!("{content_hash}.blob"));
        match fs::remove_file(&path) {
            Ok(()) => Ok(true),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(err) => Err(NodeError::Replication {
                reason: format!("discard corrupt blob {} failed: {}", path.display(), err),
            }),
        }
    }
}
