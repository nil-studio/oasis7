//! Read-only observer checkpoint loading. The only ambient open is the root
//! supplied by the operator; every artifact is opened through that capability.
#[path = "observer_decode_budget.rs"]
mod decode_budget;
use super::*;
use cap_fs_ext::{DirExt, FollowSymlinks, OpenOptionsFollowExt};
#[cfg(unix)]
use cap_std::fs::OpenOptionsExt;
use cap_std::fs::{Dir, OpenOptions};
use oasis7_distfs::{BlobStore, HashAlgorithm, decode_local_blob_into};
use std::cell::Cell;
use std::io::{Read, Write};

/// Process-local cancellation control for bounded immutable checkpoint reads.
#[derive(Clone, Default)]
pub struct ObserverLoadControl {
    cancelled: Arc<std::sync::atomic::AtomicBool>,
}
impl ObserverLoadControl {
    pub fn from_shared_cancellation(cancelled: Arc<std::sync::atomic::AtomicBool>) -> Self { Self { cancelled } }
    pub fn cancel(&self) { self.cancelled.store(true, std::sync::atomic::Ordering::SeqCst); }
}
#[derive(Debug)]
pub enum ObserverLoadError {
    NotReady,
    Transient(String),
    SecurityViolation(String),
    IntegrityFailure(String),
    RecoveryRequired,
    ResourceLimited,
    Cancelled,
}

impl From<WorldError> for ObserverLoadError {
    fn from(error: WorldError) -> Self {
        let text = format!("{error:?}");
        if text.contains("Cancelled") {
            Self::Cancelled
        } else if text.contains("SecurityViolation") {
            Self::SecurityViolation(text)
        } else if text.contains("Transient") {
            Self::Transient(text)
        } else if text.contains("ResourceLimited") {
            Self::ResourceLimited
        } else if text.contains("RecoveryRequired") {
            Self::RecoveryRequired
        } else {
            Self::IntegrityFailure(text)
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ObserverReadLimits {
    pub max_file_bytes: usize,
    pub max_decoded_blob_bytes: usize,
    pub max_total_bytes: usize,
    pub max_single_allocation_bytes: usize,
    pub max_elements: usize,
}
impl Default for ObserverReadLimits {
    fn default() -> Self {
        Self {
            max_file_bytes: 64 * 1024 * 1024,
            max_decoded_blob_bytes: 128 * 1024 * 1024,
            max_total_bytes: 512 * 1024 * 1024,
            max_single_allocation_bytes: 128 * 1024 * 1024,
            max_elements: 1_000_000,
        }
    }
}

struct RelativeArtifactPath(Vec<String>);
impl RelativeArtifactPath {
    fn parse(path: &str) -> Result<Self, ObserverLoadError> {
        let components: Vec<_> = path.split('/').collect();
        if path.len() > 512
            || components.is_empty()
            || components.iter().any(|part| {
                part.is_empty()
                    || *part == "."
                    || *part == ".."
                    || part.contains(['\\', ':'])
                    || !part
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b".-_".contains(&b))
            })
        {
            return Err(ObserverLoadError::SecurityViolation(
                "invalid relative artifact path".into(),
            ));
        }
        Ok(Self(components.into_iter().map(str::to_string).collect()))
    }
}

struct RootedReader {
    root: Dir,
    limits: ObserverReadLimits,
    consumed: Cell<usize>,
    cancelled: Arc<std::sync::atomic::AtomicBool>,
}
impl RootedReader {
    fn new(
        path: &Path,
        limits: ObserverReadLimits,
        cancelled: Arc<std::sync::atomic::AtomicBool>,
    ) -> Result<Self, ObserverLoadError> {
        let root = Dir::open_ambient_dir(path, cap_std::ambient_authority()).map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                ObserverLoadError::NotReady
            } else {
                ObserverLoadError::SecurityViolation(error.to_string())
            }
        })?;
        Ok(Self {
            root,
            limits,
            consumed: Cell::new(0),
            cancelled,
        })
    }
    fn open_read(&self, name: &str) -> Result<BudgetRead<'_>, ObserverLoadError> {
        self.check_cancelled()?;
        let path = RelativeArtifactPath::parse(name)?;
        let mut dir = self.root.try_clone().map_err(io_error)?;
        for component in &path.0[..path.0.len() - 1] {
            dir = dir.open_dir_nofollow(component).map_err(io_error)?;
        }
        let mut options = OpenOptions::new();
        options.read(true).follow(FollowSymlinks::No);
        // Opening a malicious FIFO must not block before the handle's regular
        // file check. O_NONBLOCK has no effect on ordinary checkpoint files.
        #[cfg(unix)]
        options.custom_flags(libc::O_NONBLOCK);
        let file = dir
            .open_with(path.0.last().expect("nonempty"), &options)
            .map_err(io_error)?;
        let metadata = file.metadata().map_err(io_error)?;
        if !metadata.is_file() {
            return Err(ObserverLoadError::SecurityViolation(
                "artifact is not a regular file".into(),
            ));
        }
        if metadata.len() > self.limits.max_file_bytes as u64 {
            return Err(ObserverLoadError::ResourceLimited);
        }
        Ok(BudgetRead {
            file,
            reader: self,
            count: 0,
        })
    }
    fn check_cancelled(&self) -> Result<(), ObserverLoadError> {
        if self.cancelled.load(std::sync::atomic::Ordering::Relaxed) {
            Err(ObserverLoadError::Cancelled)
        } else {
            Ok(())
        }
    }
    fn read(&self, name: &str) -> Result<Vec<u8>, ObserverLoadError> {
        let mut file = self.open_read(name)?;
        let mut output = Vec::new();
        let mut buffer = [0u8; 16 * 1024];
        loop {
            let n = file
                .read(&mut buffer)
                .map_err(|error| ObserverLoadError::from(invalid(&error.to_string())))?;
            if n == 0 {
                break;
            }
            if output.len().checked_add(n).is_none_or(|size| {
                size > self
                    .limits
                    .max_file_bytes
                    .min(self.limits.max_single_allocation_bytes)
            }) {
                return Err(ObserverLoadError::ResourceLimited);
            }
            output
                .try_reserve(n)
                .map_err(|_| ObserverLoadError::ResourceLimited)?;
            output.extend_from_slice(&buffer[..n]);
        }
        Ok(output)
    }
    fn charge(&self, n: usize) -> Result<(), ObserverLoadError> {
        let total = self
            .consumed
            .get()
            .checked_add(n)
            .ok_or(ObserverLoadError::ResourceLimited)?;
        if total > self.limits.max_total_bytes {
            return Err(ObserverLoadError::ResourceLimited);
        }
        self.consumed.set(total);
        Ok(())
    }
    fn json<T: serde::de::DeserializeOwned>(&self, name: &str) -> Result<T, ObserverLoadError> {
        let bytes = self.read(name)?;
        decode_budget::json(&bytes, self.limits)?;
        serde_json::from_slice(&bytes)
            .map_err(|error| ObserverLoadError::IntegrityFailure(error.to_string()))
    }
    fn blob(&self, hash: &str) -> Result<Vec<u8>, ObserverLoadError> {
        if hash.len() != 64 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(ObserverLoadError::SecurityViolation(
                "invalid CAS hash".into(),
            ));
        }
        let input = self.open_read(&format!(".distfs-state/blobs/{hash}.blob"))?;
        let mut output = BudgetOutput {
            bytes: Vec::new(),
            reader: self,
        };
        decode_local_blob_into(
            HashAlgorithm::Blake3,
            hash,
            input,
            &mut output,
            self.limits.max_decoded_blob_bytes,
        )
        .map_err(ObserverLoadError::from)?;
        Ok(output.bytes)
    }
}
fn io_error(error: std::io::Error) -> ObserverLoadError {
    if error.kind() == std::io::ErrorKind::NotFound {
        ObserverLoadError::Transient("captured generation artifact unavailable".into())
    } else {
        ObserverLoadError::SecurityViolation(error.to_string())
    }
}
struct BudgetRead<'a> {
    file: cap_std::fs::File,
    reader: &'a RootedReader,
    count: usize,
}
impl Read for BudgetRead<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        self.reader
            .check_cancelled()
            .map_err(|_| std::io::Error::other("Cancelled"))?;
        let n = self.file.read(buffer)?;
        self.count = self
            .count
            .checked_add(n)
            .ok_or_else(|| std::io::Error::other("ResourceLimited: input overflow"))?;
        if self.count > self.reader.limits.max_file_bytes {
            return Err(std::io::Error::other("ResourceLimited: input budget"));
        }
        self.reader
            .charge(n)
            .map_err(|_| std::io::Error::other("ResourceLimited: input total budget"))?;
        Ok(n)
    }
}
struct BudgetOutput<'a> {
    bytes: Vec<u8>,
    reader: &'a RootedReader,
}
impl Write for BudgetOutput<'_> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.reader
            .charge(bytes.len())
            .map_err(|_| std::io::Error::other("ResourceLimited: decoded total budget"))?;
        if self
            .bytes
            .len()
            .checked_add(bytes.len())
            .is_none_or(|size| size > self.reader.limits.max_single_allocation_bytes)
        {
            return Err(std::io::Error::other(
                "ResourceLimited: single allocation budget",
            ));
        }
        self.bytes
            .try_reserve(bytes.len())
            .map_err(|_| std::io::Error::other("ResourceLimited: allocation"))?;
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
impl From<oasis7_proto::world_error::WorldError> for ObserverLoadError {
    fn from(error: oasis7_proto::world_error::WorldError) -> Self {
        Self::from(WorldError::from(error))
    }
}
impl BlobStore for RootedReader {
    fn get(&self, hash: &str) -> Result<Vec<u8>, oasis7_proto::world_error::WorldError> {
        self.blob(hash).map_err(|error| {
            oasis7_proto::world_error::WorldError::DistributedValidationFailed {
                reason: format!("{error:?}"),
            }
        })
    }
    fn put(&self, _: &str, _: &[u8]) -> Result<(), oasis7_proto::world_error::WorldError> {
        Err(
            oasis7_proto::world_error::WorldError::DistributedValidationFailed {
                reason: "observer writes are forbidden".into(),
            },
        )
    }
    fn has(&self, hash: &str) -> Result<bool, oasis7_proto::world_error::WorldError> {
        self.get(hash).map(|_| true)
    }
}
fn invalid(reason: &str) -> WorldError {
    WorldError::DistributedValidationFailed {
        reason: reason.into(),
    }
}

struct CapturedGeneration {
    record: SidecarGenerationRecord,
}
fn capture(reader: &RootedReader) -> Result<CapturedGeneration, ObserverLoadError> {
    let index: SidecarGenerationIndex =
        match reader.json(".distfs-state/sidecar-generations/index.json") {
            Err(ObserverLoadError::Transient(_)) => return Err(ObserverLoadError::NotReady),
            value => value?,
        };
    if index.schema_version != SIDECAR_GENERATION_INDEX_SCHEMA_V1 {
        return Err(ObserverLoadError::IntegrityFailure(
            "unsupported generation index".into(),
        ));
    }
    let record = index
        .generations
        .get(&index.latest_generation)
        .cloned()
        .ok_or_else(|| {
            ObserverLoadError::IntegrityFailure("generation missing from index".into())
        })?;
    if record.generation_id != index.latest_generation
        || record.schema_version != SIDECAR_GENERATION_RECORD_SCHEMA_V1
    {
        return Err(ObserverLoadError::IntegrityFailure(
            "generation identity mismatch".into(),
        ));
    }
    for relative in [
        &record.snapshot_manifest_path,
        &record.journal_segments_path,
    ]
    .into_iter()
    .chain(record.recovery_metadata_path.iter())
    {
        RelativeArtifactPath::parse(relative)?;
    }
    let prefix = format!("sidecar-generations/payloads/{}/", record.generation_id);
    if !record.generation_id.starts_with("gen-")
        || record.generation_id.contains('/')
        || record.snapshot_manifest_path != format!("{prefix}snapshot.manifest.json")
        || record.journal_segments_path != format!("{prefix}journal.segments.json")
        || record
            .recovery_metadata_path
            .as_ref()
            .is_some_and(|path| path != &format!("{prefix}viewer-recovery.bin"))
    {
        return Err(ObserverLoadError::SecurityViolation(
            "unexpected generation artifact name".into(),
        ));
    }
    Ok(CapturedGeneration { record })
}

impl World {
    /// Load only verifiable immutable checkpoints, without writer recovery,
    /// persistence attachment, audits, GC, or mutable legacy-file fallback.
    pub fn load_observer_from_dir(
        path: impl AsRef<Path>,
        limits: ObserverReadLimits,
    ) -> Result<Self, ObserverLoadError> {
        Self::load_observer_from_dir_cancellable(
            path,
            limits,
            Arc::new(std::sync::atomic::AtomicBool::new(false)),
        )
    }
    /// Load with an explicit cancellation control and the same immutable read limits.
    pub fn load_observer_with_control(
        path: impl AsRef<Path>,
        limits: ObserverReadLimits,
        control: ObserverLoadControl,
    ) -> Result<Self, ObserverLoadError> {
        Self::load_observer_from_dir_cancellable(path, limits, control.cancelled)
    }
    pub(crate) fn load_observer_from_dir_cancellable(
        path: impl AsRef<Path>,
        limits: ObserverReadLimits,
        cancelled: Arc<std::sync::atomic::AtomicBool>,
    ) -> Result<Self, ObserverLoadError> {
        let reader = RootedReader::new(path.as_ref(), limits, cancelled)?;
        let captured = capture(&reader)?;
        let record = captured.record;
        let manifest: SnapshotManifest =
            reader.json(&format!(".distfs-state/{}", record.snapshot_manifest_path))?;
        let segments: Vec<JournalSegmentRef> =
            reader.json(&format!(".distfs-state/{}", record.journal_segments_path))?;
        let snapshot_manifest_hash = hash_json(&manifest).map_err(ObserverLoadError::from)?;
        let hashes = segments
            .iter()
            .map(|segment| segment.content_hash.clone())
            .collect::<Vec<_>>();
        if snapshot_manifest_hash != record.snapshot_manifest_hash
            || hashes != record.journal_segment_hashes
        {
            return Err(ObserverLoadError::IntegrityFailure(
                "generation payload hash mismatch".into(),
            ));
        }
        if manifest.chunks.len() > 4096
            || segments.len() > 4096
            || record.pinned_blob_hashes.len() > 8193
        {
            return Err(ObserverLoadError::ResourceLimited);
        }
        let mut pins = manifest
            .chunks
            .iter()
            .map(|chunk| chunk.content_hash.clone())
            .collect::<BTreeSet<_>>();
        pins.extend(hashes.iter().cloned());
        pins.extend(record.tick_consensus_archive_ref.iter().cloned());
        if pins.into_iter().collect::<Vec<_>>() != record.pinned_blob_hashes {
            return Err(ObserverLoadError::IntegrityFailure(
                "generation pin set mismatch".into(),
            ));
        }
        let metadata_hash = record
            .recovery_metadata_path
            .as_ref()
            .map(|path| {
                reader
                    .read(&format!(".distfs-state/{path}"))
                    .map(|bytes| super::super::super::util::sha256_hex(&bytes))
            })
            .transpose()?;
        if metadata_hash != record.recovery_metadata_hash {
            return Err(ObserverLoadError::IntegrityFailure(
                "recovery metadata hash mismatch".into(),
            ));
        }
        let actual_hash = hash_json(&SidecarGenerationHashPayload {
            generation_id: &record.generation_id,
            snapshot_manifest_path: &record.snapshot_manifest_path,
            journal_segments_path: &record.journal_segments_path,
            snapshot_manifest_hash: &snapshot_manifest_hash,
            journal_segment_hashes: &hashes,
            recovery_metadata_path: &record.recovery_metadata_path,
            recovery_metadata_hash: &record.recovery_metadata_hash,
            tick_consensus_archive_ref: &record.tick_consensus_archive_ref,
            pinned_blob_hashes: &record.pinned_blob_hashes,
            created_at_ms: record.created_at_ms,
        })
        .map_err(ObserverLoadError::from)?;
        if actual_hash != record.manifest_hash {
            return Err(ObserverLoadError::IntegrityFailure(
                "generation manifest hash mismatch".into(),
            ));
        }
        let mut assembled = BudgetOutput {
            bytes: Vec::new(),
            reader: &reader,
        };
        for chunk in &manifest.chunks {
            let bytes = reader.blob(&chunk.content_hash)?;
            assembled
                .write_all(&bytes)
                .map_err(|error| ObserverLoadError::from(invalid(&error.to_string())))?;
        }
        if oasis7_distfs::blake3_hex(&assembled.bytes) != manifest.state_root {
            return Err(ObserverLoadError::IntegrityFailure(
                "snapshot state root mismatch".into(),
            ));
        }
        decode_budget::snapshot_cbor(&assembled.bytes, reader.limits)?;
        let mut snapshot: Snapshot = serde_cbor::from_slice(&assembled.bytes)
            .map_err(|error| ObserverLoadError::IntegrityFailure(error.to_string()))?;
        drop(assembled);
        let mut events = Vec::<WorldEvent>::new();
        for segment in &segments {
            reader.check_cancelled()?;
            let bytes = reader.blob(&segment.content_hash)?;
            decode_budget::cbor(&bytes, reader.limits)?;
            let decoded: Vec<WorldEvent> = serde_cbor::from_slice(&bytes)
                .map_err(|error| ObserverLoadError::IntegrityFailure(error.to_string()))?;
            if decoded.first().map(|event| event.id) != Some(segment.from_event_id)
                || decoded.last().map(|event| event.id) != Some(segment.to_event_id)
            {
                return Err(ObserverLoadError::IntegrityFailure(
                    "journal segment range mismatch".into(),
                ));
            }
            let new_len = events
                .len()
                .checked_add(decoded.len())
                .ok_or(ObserverLoadError::ResourceLimited)?;
            if new_len > reader.limits.max_elements
                || new_len
                    .checked_mul(std::mem::size_of::<WorldEvent>())
                    .is_none_or(|bytes| bytes > reader.limits.max_single_allocation_bytes)
            {
                return Err(ObserverLoadError::ResourceLimited);
            }
            events
                .try_reserve(decoded.len())
                .map_err(|_| ObserverLoadError::ResourceLimited)?;
            events.extend(decoded);
        }
        if let Some(hash) = &record.tick_consensus_archive_ref {
            let archive_bytes = reader.blob(hash)?;
            decode_budget::json(&archive_bytes, reader.limits)?;
            let archive: TickConsensusArchiveFile = serde_json::from_slice(&archive_bytes)
                .map_err(|error| ObserverLoadError::IntegrityFailure(error.to_string()))?;
            hydrate_tick_consensus_snapshot_from_archived_records(
                &mut snapshot,
                archive.archived_records,
            )
            .map_err(ObserverLoadError::from)?;
        } else if snapshot.tick_consensus_archived_record_count != 0 {
            return Err(ObserverLoadError::IntegrityFailure(
                "generation archive missing".into(),
            ));
        }
        if snapshot.module_registry.records.len() > 4096
            || events.len() > reader.limits.max_elements
        {
            return Err(ObserverLoadError::ResourceLimited);
        }
        let counts = [
            snapshot.pending_actions.len(),
            snapshot.pending_effects.len(),
            snapshot.inflight_effects.len(),
            snapshot.state.agents.len(),
            snapshot.state.resources.len(),
            snapshot.state.factories.len(),
            snapshot.tick_consensus_records.len(),
        ];
        if counts
            .iter()
            .any(|count| *count > reader.limits.max_elements)
        {
            return Err(ObserverLoadError::ResourceLimited);
        }
        for module in snapshot.module_registry.records.values() {
            let hash = &module.manifest.wasm_hash;
            if hash.len() != 64 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                return Err(ObserverLoadError::SecurityViolation(
                    "invalid module hash".into(),
                ));
            }
            let meta: oasis7_wasm_abi::ModuleManifest =
                reader.json(&format!("modules/{hash}.meta.json"))?;
            if meta != module.manifest {
                return Err(ObserverLoadError::IntegrityFailure(
                    "module metadata identity mismatch".into(),
                ));
            }
            if !snapshot.module_artifact_bytes.contains_key(hash) {
                snapshot
                    .module_artifact_bytes
                    .insert(hash.clone(), reader.read(&format!("modules/{hash}.wasm"))?);
            }
        }
        let mut world = Self::from_snapshot_mode(snapshot, Journal { events }, true)
            .map_err(ObserverLoadError::from)?;
        for module in world.module_registry.records.values() {
            world
                .validate_module_artifact_identity(&module.manifest)
                .map_err(ObserverLoadError::from)?;
        }
        world
            .validate_observer_cognition()
            .map_err(ObserverLoadError::from)?;
        Ok(world)
    }
}

impl World {
    /// Reuse observer structural limits for typed in-memory offline snapshots.
    /// No filesystem reads or writes and no change to default resource limits.
    pub fn validate_offline_snapshot_cbor_budget(
        bytes: &[u8],
        limits: ObserverReadLimits,
    ) -> Result<(), ObserverLoadError> {
        if bytes.len() > limits.max_file_bytes {
            return Err(ObserverLoadError::ResourceLimited);
        }
        decode_budget::snapshot_cbor(bytes, limits)
    }
    /// Preflight only ClosedRecord root objects[].bytes as u8 payloads.
    /// The caller must also apply the existing typed closed-record validator.
    pub fn validate_offline_closed_record_cbor_budget(
        bytes: &[u8],
        limits: ObserverReadLimits,
    ) -> Result<(), ObserverLoadError> {
        if bytes.len() > limits.max_file_bytes {
            return Err(ObserverLoadError::ResourceLimited);
        }
        decode_budget::closed_record_cbor(bytes, limits)
    }

    /// Byte-aware preflight for Pending/Published writer DTOs. ClosedRecord's
    /// typed 8MiB/object/hash/reachability validation remains mandatory.
    pub fn validate_offline_formal_writer_envelope_cbor_budget(
        bytes: &[u8],
        limits: ObserverReadLimits,
    ) -> Result<(), ObserverLoadError> {
        if bytes.len() > limits.max_file_bytes {
            return Err(ObserverLoadError::ResourceLimited);
        }
        decode_budget::formal_writer_envelope_cbor(bytes, limits)
    }

    /// Generic persisted-value preflight (including WorldEvent journals).
    pub fn validate_offline_cbor_budget(
        bytes: &[u8],
        limits: ObserverReadLimits,
    ) -> Result<(), ObserverLoadError> {
        if bytes.len() > limits.max_file_bytes {
            return Err(ObserverLoadError::ResourceLimited);
        }
        decode_budget::cbor(bytes, limits)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn temp_root() -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!(
            "oasis7-observer-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        root
    }
    fn tree(root: &Path) -> BTreeMap<String, Vec<u8>> {
        fn walk(base: &Path, dir: &Path, files: &mut BTreeMap<String, Vec<u8>>) {
            for entry in fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    walk(base, &path, files);
                } else {
                    files.insert(
                        path.strip_prefix(base)
                            .unwrap()
                            .to_string_lossy()
                            .into_owned(),
                        fs::read(path).unwrap(),
                    );
                }
            }
        }
        let mut files = BTreeMap::new();
        walk(root, root, &mut files);
        files
    }
    #[test]
    fn rejects_path_shapes_and_missing_root_is_not_ready() {
        for path in [
            "/snapshot.json",
            "../snapshot.json",
            "a/../b",
            "a//b",
            "C:/secret",
            "\\\\server\\share",
            "a\\b",
            "a/./b",
        ] {
            assert!(matches!(
                RelativeArtifactPath::parse(path),
                Err(ObserverLoadError::SecurityViolation(_))
            ));
        }
        assert!(matches!(
            World::load_observer_from_dir(
                temp_root().join("missing"),
                ObserverReadLimits::default()
            ),
            Err(ObserverLoadError::NotReady)
        ));
    }
    #[test]
    fn loading_valid_checkpoint_has_zero_source_writes_and_no_persistence_attachment() {
        let root = temp_root();
        World::new().save_to_dir(&root).unwrap();
        let before = tree(&root);
        let world = World::load_observer_from_dir(&root, ObserverReadLimits::default()).unwrap();
        assert!(world.persistence_dir.borrow().is_none());
        assert_eq!(tree(&root), before);
        let limits = ObserverReadLimits {
            max_file_bytes: 1,
            ..ObserverReadLimits::default()
        };
        assert!(matches!(
            World::load_observer_from_dir(&root, limits),
            Err(ObserverLoadError::ResourceLimited)
        ));
        assert_eq!(tree(&root), before);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn observer_loads_large_artifact_byte_sequence_without_source_writes() {
        let root = temp_root();
        let mut world = World::new();
        let bytes = vec![42; 203_153];
        let hash = crate::util::sha256_hex(&bytes);
        world.module_artifact_bytes.insert(hash, bytes.into());
        world.save_to_dir(&root).unwrap();
        let before = tree(&root);
        let restored = World::load_observer_from_dir(&root, ObserverReadLimits::default()).unwrap();
        assert_eq!(restored.module_artifact_bytes, world.module_artifact_bytes);
        assert!(restored.persistence_dir.borrow().is_none());
        assert_eq!(tree(&root), before);
        fs::remove_dir_all(root).unwrap();
    }
    #[cfg(unix)]
    #[test]
    fn rejects_intermediate_and_final_symlinks() {
        use std::os::unix::fs::symlink;
        let root = temp_root();
        let outside = temp_root();
        fs::write(outside.join("secret.json"), b"outside").unwrap();
        symlink(&outside, root.join("escape")).unwrap();
        symlink(outside.join("secret.json"), root.join("secret.json")).unwrap();
        let reader = RootedReader::new(
            &root,
            ObserverReadLimits::default(),
            Arc::new(std::sync::atomic::AtomicBool::new(false)),
        )
        .unwrap();
        assert!(matches!(
            reader.read("escape/secret.json"),
            Err(ObserverLoadError::SecurityViolation(_))
        ));
        assert!(matches!(
            reader.read("secret.json"),
            Err(ObserverLoadError::SecurityViolation(_))
        ));
        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(outside).unwrap();
    }
    #[cfg(unix)]
    #[test]
    fn special_file_is_rejected_without_waiting_for_a_writer() {
        let root = temp_root();
        assert!(
            std::process::Command::new("mkfifo")
                .arg(root.join("snapshot.json"))
                .status()
                .unwrap()
                .success()
        );
        let thread_root = root.clone();
        let (send, receive) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let reader = RootedReader::new(
                &thread_root,
                ObserverReadLimits::default(),
                Arc::new(std::sync::atomic::AtomicBool::new(false)),
            )
            .unwrap();
            send.send(reader.read("snapshot.json")).unwrap();
        });
        assert!(matches!(
            receive
                .recv_timeout(std::time::Duration::from_secs(3))
                .unwrap(),
            Err(ObserverLoadError::SecurityViolation(_))
        ));
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn captured_generation_survives_a_newer_publication() {
        let root = temp_root();
        let world = World::new();
        world.save_to_dir(&root).unwrap();
        let reader = RootedReader::new(
            &root,
            ObserverReadLimits::default(),
            Arc::new(std::sync::atomic::AtomicBool::new(false)),
        )
        .unwrap();
        let first = capture(&reader).unwrap().record;
        world.save_to_dir(&root).unwrap();
        let second = capture(&reader).unwrap().record;
        assert_ne!(first.generation_id, second.generation_id);
        let manifest: SnapshotManifest = reader
            .json(&format!(".distfs-state/{}", first.snapshot_manifest_path))
            .unwrap();
        let restored: Snapshot = assemble_snapshot(&manifest, &reader).unwrap();
        assert_eq!(restored.state, world.snapshot().state);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn cancellation_and_corruption_reject_without_source_writes() {
        let root = temp_root();
        World::new().save_to_dir(&root).unwrap();
        let cancelled = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let reader =
            RootedReader::new(&root, ObserverReadLimits::default(), cancelled.clone()).unwrap();
        let captured = capture(&reader).unwrap();
        let manifest: SnapshotManifest = reader
            .json(&format!(
                ".distfs-state/{}",
                captured.record.snapshot_manifest_path
            ))
            .unwrap();
        cancelled.store(true, std::sync::atomic::Ordering::Relaxed);
        assert!(matches!(
            reader.read("snapshot.json"),
            Err(ObserverLoadError::Cancelled)
        ));
        let path = root.join(format!(
            ".distfs-state/blobs/{}.blob",
            manifest.chunks[0].content_hash
        ));
        fs::write(path, b"corrupt captured immutable content").unwrap();
        let corrupted = tree(&root);
        assert!(matches!(
            World::load_observer_from_dir(&root, ObserverReadLimits::default()),
            Err(ObserverLoadError::IntegrityFailure(_))
        ));
        assert_eq!(tree(&root), corrupted);
        fs::remove_dir_all(root).unwrap();
    }
}
