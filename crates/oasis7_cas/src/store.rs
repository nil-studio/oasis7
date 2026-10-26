use super::*;

impl LocalCasStore {
    pub(super) fn put_without_retention_lock(
        &self,
        content_hash: &str,
        bytes: &[u8],
    ) -> Result<(), WorldError> {
        self.ensure_dirs()?;
        let actual_hash = self.hash_hex(bytes);
        if actual_hash != content_hash {
            return Err(WorldError::BlobHashMismatch {
                expected: content_hash.to_string(),
                actual: actual_hash,
            });
        }
        let path = self.blob_path(content_hash)?;
        if path.exists() {
            return Ok(());
        }
        let disk_bytes = self.maybe_encode_blob_for_disk(content_hash, bytes)?;
        write_bytes_atomic(&path, disk_bytes.as_slice())?;
        Ok(())
    }

    pub(super) fn put_bytes_without_retention_lock(
        &self,
        bytes: &[u8],
    ) -> Result<String, WorldError> {
        let hash = self.hash_hex(bytes);
        self.put_without_retention_lock(&hash, bytes)?;
        Ok(hash)
    }

    pub fn new(root: impl AsRef<Path>) -> Self {
        Self::new_with_hash_algorithm(root, HashAlgorithm::Blake3)
    }

    pub fn new_with_hash_algorithm(root: impl AsRef<Path>, hash_algorithm: HashAlgorithm) -> Self {
        let root = root.as_ref().to_path_buf();
        let blobs_dir = root.join(BLOBS_DIR);
        let pins_path = root.join(PINS_FILE);
        let files_index_path = root.join(FILES_INDEX_FILE);
        Self {
            root,
            blobs_dir,
            pins_path,
            hash_algorithm,
            files_index_path,
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn blobs_dir(&self) -> &Path {
        &self.blobs_dir
    }

    pub fn pins_path(&self) -> &Path {
        &self.pins_path
    }

    pub fn hash_algorithm(&self) -> HashAlgorithm {
        self.hash_algorithm
    }

    pub fn get_verified(&self, content_hash: &str) -> Result<Vec<u8>, WorldError> {
        let bytes = self.get(content_hash)?;
        let actual_hash = self.hash_hex(&bytes);
        if actual_hash != content_hash {
            return Err(WorldError::BlobHashMismatch {
                expected: content_hash.to_string(),
                actual: actual_hash,
            });
        }
        Ok(bytes)
    }

    pub fn get_range(
        &self,
        content_hash: &str,
        offset: u64,
        limit: usize,
    ) -> Result<(Vec<u8>, bool), WorldError> {
        let path = self.blob_path(content_hash)?;
        let mut file = match fs::File::open(&path) {
            Ok(file) => file,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                return Err(WorldError::BlobNotFound {
                    content_hash: content_hash.to_string(),
                });
            }
            Err(err) => return Err(err.into()),
        };
        let mut prefix = [0u8; COMPRESSED_BLOB_HEADER_LEN];
        let prefix_len = file.read(&mut prefix)?;
        if prefix_len == COMPRESSED_BLOB_HEADER_LEN && prefix.starts_with(COMPRESSED_BLOB_MAGIC) {
            if &prefix[16..COMPRESSED_BLOB_HEADER_LEN] != content_hash.as_bytes() {
                return Err(WorldError::BlobHashInvalid {
                    content_hash: content_hash.to_string(),
                });
            }
            let raw_len = u64::from_le_bytes(prefix[8..16].try_into().unwrap_or([0; 8]));
            if raw_len <= COMPRESSED_RANGE_CACHE_ENTRY_MAX_BYTES {
                return compressed_range_cache::read_cached_range(
                    file, path, offset, limit, raw_len,
                );
            }
            let mut decoder = zstd::stream::read::Decoder::new(file)?;
            return read_range(&mut decoder, offset, limit, raw_len);
        }

        let raw_len = file.metadata()?.len();
        file.seek(SeekFrom::Start(offset))?;
        read_range(&mut file, 0, limit, raw_len.saturating_sub(offset))
    }

    pub(super) fn hash_hex(&self, bytes: &[u8]) -> String {
        match self.hash_algorithm {
            HashAlgorithm::Blake3 => blake3_hex(bytes),
            HashAlgorithm::Sha256 => {
                let mut hasher = Sha256::new();
                hasher.update(bytes);
                hex::encode(hasher.finalize())
            }
        }
    }

    pub fn files_index_path(&self) -> &Path {
        &self.files_index_path
    }

    pub(super) fn ensure_dirs(&self) -> Result<(), WorldError> {
        fs::create_dir_all(&self.root)?;
        fs::create_dir_all(&self.blobs_dir)?;
        Ok(())
    }

    pub(super) fn maybe_encode_blob_for_disk(
        &self,
        content_hash: &str,
        bytes: &[u8],
    ) -> Result<Vec<u8>, WorldError> {
        if bytes.len() < COMPRESSIBLE_BLOB_MIN_BYTES {
            return Ok(bytes.to_vec());
        }
        let compressed = zstd_encode_all(bytes, COMPRESSED_BLOB_ZSTD_LEVEL).map_err(|err| {
            WorldError::DistributedValidationFailed {
                reason: format!("compress blob for disk failed: {err}"),
            }
        })?;
        let encoded_len = COMPRESSED_BLOB_HEADER_LEN.saturating_add(compressed.len());
        if encoded_len >= bytes.len() {
            return Ok(bytes.to_vec());
        }

        let mut encoded = Vec::with_capacity(encoded_len);
        encoded.extend_from_slice(COMPRESSED_BLOB_MAGIC);
        encoded.extend_from_slice(&(bytes.len() as u64).to_le_bytes());
        encoded.extend_from_slice(content_hash.as_bytes());
        encoded.extend_from_slice(compressed.as_slice());
        Ok(encoded)
    }

    pub(super) fn decode_blob_from_disk(&self, content_hash: &str, disk_bytes: Vec<u8>) -> Vec<u8> {
        if disk_bytes.len() < COMPRESSED_BLOB_HEADER_LEN
            || !disk_bytes.starts_with(COMPRESSED_BLOB_MAGIC)
        {
            return disk_bytes;
        }

        let Some(raw_len_bytes) = disk_bytes.get(8..16) else {
            return disk_bytes;
        };
        let expected_raw_len = u64::from_le_bytes(raw_len_bytes.try_into().unwrap_or([0; 8]));
        let Some(encoded_hash_bytes) = disk_bytes.get(16..COMPRESSED_BLOB_HEADER_LEN) else {
            return disk_bytes;
        };
        if encoded_hash_bytes != content_hash.as_bytes() {
            return disk_bytes;
        }
        match zstd_decode_all(&disk_bytes[COMPRESSED_BLOB_HEADER_LEN..]) {
            Ok(decoded) if decoded.len() as u64 == expected_raw_len => decoded,
            _ => disk_bytes,
        }
    }

    pub(super) fn blob_path(&self, content_hash: &str) -> Result<PathBuf, WorldError> {
        validate_hash(content_hash)?;
        Ok(self.blobs_dir.join(format!("{content_hash}.blob")))
    }

    pub(super) fn load_file_index(&self) -> Result<FileIndexFile, WorldError> {
        if !self.files_index_path.exists() {
            return Ok(FileIndexFile::default());
        }
        let file_index: FileIndexFile = read_json_from_path(&self.files_index_path)?;
        if file_index.version != FILE_INDEX_VERSION {
            return Err(WorldError::DistributedValidationFailed {
                reason: format!(
                    "unsupported files index version: expected={}, actual={}",
                    FILE_INDEX_VERSION, file_index.version
                ),
            });
        }
        Ok(file_index)
    }

    pub(super) fn save_file_index(&self, file_index: &FileIndexFile) -> Result<(), WorldError> {
        self.ensure_dirs()?;
        write_json_atomic(file_index, &self.files_index_path)
    }

    pub(super) fn remove_blob_if_unreferenced_in_index(
        &self,
        file_index: &FileIndexFile,
        content_hash: &str,
    ) -> Result<bool, WorldError> {
        validate_hash(content_hash)?;
        if file_index.content_hash_ref_count(content_hash) > 0 {
            return Ok(false);
        }
        if self.is_effectively_pinned(content_hash)? {
            return Ok(false);
        }
        let path = self.blob_path(content_hash)?;
        if !path.exists() {
            return Ok(false);
        }
        fs::remove_file(path)?;
        Ok(true)
    }

    pub fn prune_unpinned(&self, max_bytes: u64) -> Result<u64, WorldError> {
        let _retention = self.lock_retention()?;
        self.ensure_dirs()?;
        let pins = self.load_effective_pins()?;
        let mut total_bytes = 0u64;
        let mut entries = Vec::new();

        if self.blobs_dir.exists() {
            for entry in fs::read_dir(&self.blobs_dir)? {
                let entry = entry?;
                if !entry.file_type()?.is_file() {
                    continue;
                }
                let path = entry.path();
                if path.extension().and_then(|ext| ext.to_str()) != Some("blob") {
                    continue;
                }
                let metadata = entry.metadata()?;
                let size = metadata.len();
                total_bytes = total_bytes.saturating_add(size);
                let modified = metadata.modified().unwrap_or(UNIX_EPOCH);
                let hash = path
                    .file_stem()
                    .and_then(|stem| stem.to_str())
                    .unwrap_or("")
                    .to_string();
                entries.push(BlobEntry {
                    hash,
                    path,
                    size,
                    modified,
                });
            }
        }

        if total_bytes <= max_bytes {
            return Ok(0);
        }

        entries.sort_by_key(|entry| entry.modified);
        let mut freed = 0u64;
        for entry in entries {
            if total_bytes <= max_bytes {
                break;
            }
            if pins.contains(&entry.hash) {
                continue;
            }
            fs::remove_file(&entry.path)?;
            total_bytes = total_bytes.saturating_sub(entry.size);
            freed = freed.saturating_add(entry.size);
        }
        Ok(freed)
    }

    pub fn write_file_if_match(
        &self,
        path: &str,
        expected_content_hash: Option<&str>,
        bytes: &[u8],
    ) -> Result<FileMetadata, WorldError> {
        let _retention = self.lock_retention()?;
        let normalized_path = normalize_file_path(path)?;
        let expected_content_hash = normalize_expected_content_hash(expected_content_hash)?;
        let mut file_index = self.load_file_index()?;
        ensure_file_hash_precondition(
            &file_index,
            &normalized_path,
            expected_content_hash.as_deref(),
        )?;

        let content_hash = self.put_bytes_without_retention_lock(bytes)?;
        let metadata = FileMetadata {
            path: normalized_path.clone(),
            content_hash,
            size_bytes: bytes.len() as u64,
            updated_at_ms: now_unix_time_ms(),
        };
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

    pub fn delete_file_if_match(
        &self,
        path: &str,
        expected_content_hash: Option<&str>,
    ) -> Result<bool, WorldError> {
        let _retention = self.lock_retention()?;
        let normalized_path = normalize_file_path(path)?;
        let expected_content_hash = normalize_expected_content_hash(expected_content_hash)?;
        let mut file_index = self.load_file_index()?;

        if let Some(expected_hash) = expected_content_hash.as_deref() {
            ensure_file_hash_precondition(&file_index, &normalized_path, Some(expected_hash))?;
        }

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

    pub fn audit_file_index(&self) -> Result<FileIndexAuditReport, WorldError> {
        let _retention = self.lock_retention()?;
        let references = self.load_blob_reference_sets()?;

        let missing_file_blob_hashes = references
            .indexed_hashes
            .iter()
            .filter(|hash| !references.blob_hashes.contains(*hash))
            .cloned()
            .collect::<Vec<_>>();
        let dangling_pin_hashes = references
            .pinned_hashes
            .iter()
            .filter(|hash| !references.blob_hashes.contains(*hash))
            .cloned()
            .collect::<Vec<_>>();
        let orphan_blob_hashes = references
            .blob_hashes
            .iter()
            .filter(|hash| {
                !references.indexed_hashes.contains(*hash)
                    && !references.pinned_hashes.contains(*hash)
            })
            .cloned()
            .collect::<Vec<_>>();

        Ok(FileIndexAuditReport {
            total_indexed_files: references.total_indexed_files,
            total_pins: references.total_pins,
            missing_file_blob_hashes,
            dangling_pin_hashes,
            orphan_blob_hashes,
        })
    }

    pub fn prune_orphan_blobs(&self) -> Result<u64, WorldError> {
        let _retention = self.lock_retention()?;
        let references = self.load_blob_reference_sets()?;
        let mut freed = 0_u64;
        for orphan_hash in references.blob_hashes.iter().filter(|hash| {
            !references.indexed_hashes.contains(*hash) && !references.pinned_hashes.contains(*hash)
        }) {
            let path = self.blob_path(orphan_hash.as_str())?;
            if !path.exists() {
                continue;
            }
            let size = fs::metadata(&path).map(|meta| meta.len()).unwrap_or(0);
            fs::remove_file(&path)?;
            freed = freed.saturating_add(size);
        }
        Ok(freed)
    }

    fn load_blob_reference_sets(&self) -> Result<BlobReferenceSets, WorldError> {
        self.ensure_dirs()?;
        let file_index = self.load_file_index()?;
        let pinned_hashes = self.load_effective_pins()?;
        let blob_hashes = self.scan_blob_hashes()?;

        let mut indexed_hashes = BTreeSet::new();
        for metadata in file_index.files.values() {
            validate_hash(metadata.content_hash.as_str())?;
            indexed_hashes.insert(metadata.content_hash.clone());
        }
        for pin in &pinned_hashes {
            validate_hash(pin.as_str())?;
        }

        Ok(BlobReferenceSets {
            total_indexed_files: file_index.files.len(),
            total_pins: pinned_hashes.len(),
            indexed_hashes,
            pinned_hashes,
            blob_hashes,
        })
    }

    fn scan_blob_hashes(&self) -> Result<BTreeSet<String>, WorldError> {
        self.ensure_dirs()?;
        let mut hashes = BTreeSet::new();
        if !self.blobs_dir.exists() {
            return Ok(hashes);
        }
        for entry in fs::read_dir(&self.blobs_dir)? {
            let entry = entry?;
            if !entry.file_type()?.is_file() {
                continue;
            }
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("blob") {
                continue;
            }
            let hash = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .ok_or_else(|| WorldError::DistributedValidationFailed {
                    reason: format!("invalid blob file name: {}", path.display()),
                })?;
            validate_hash(hash)?;
            hashes.insert(hash.to_string());
        }
        Ok(hashes)
    }
}
