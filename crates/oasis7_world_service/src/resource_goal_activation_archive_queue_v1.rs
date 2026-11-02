//! Bounded actual publication consumer. This writes historical material only:
//! these files are never a trusted floor, paid receipt, or read-ready index.
use super::*;
#[cfg(unix)]
use cap_std::fs::Dir;
use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        mpsc::{self, SyncSender, TrySendError},
    },
};

enum ArchiveWorkV2 {
    Snapshot(Box<GoalActivationArchiveCutV1>),
    Original(Box<CapturedResourcePublicationV2>),
}
/// Explicitly installed local archive producer, never initialized by a read.
/// Exactly two queued cuts plus the worker's preceding cut are retained.
/// Full/oversized/gapped inputs make the affected Resource archive unavailable;
/// they do not block or change the ordinary formal/Stop publication.
pub struct GoalActivationArchiveQueueV1 {
    send: Mutex<Option<SyncSender<ArchiveWorkV2>>>,
    worker: Mutex<Option<std::thread::JoinHandle<()>>>,
    max_snapshot_bytes: usize,
    error: Arc<Mutex<Option<String>>>,
}
impl GoalActivationArchiveQueueV1 {
    #[cfg(not(unix))]
    pub fn open_existing(
        _directory: PathBuf,
        _max_snapshot_bytes: usize,
        _max_spool_bytes: u64,
        _max_spool_entries: usize,
    ) -> FormalReadResult<Self> {
        Err(FormalReadError::HistoryUnavailable(
            "private Resource archive storage requires Unix".into(),
        ))
    }
    /// The operator supplies an existing private directory. This is only a
    /// producer spool, not a self-attesting archive or rollback-protected ledger.
    #[cfg(unix)]
    pub fn open_existing(
        directory: PathBuf,
        max_snapshot_bytes: usize,
        max_spool_bytes: u64,
        max_spool_entries: usize,
    ) -> FormalReadResult<Self> {
        use cap_fs_ext::DirExt;
        use std::path::Component;
        let components = directory.components().collect::<Vec<_>>();
        if !directory.is_absolute()
            || components.len() < 2
            || components
                .iter()
                .skip(1)
                .any(|c| !matches!(c, Component::Normal(_)))
            || max_snapshot_bytes == 0
            || max_spool_bytes == 0
            || max_spool_entries == 0
        {
            return Err(FormalReadError::RequestRejected(
                "archive spool requires an absolute private path and byte bound".into(),
            ));
        }
        let mut directory = Dir::open_ambient_dir("/", cap_std::ambient_authority())
            .map_err(|e| FormalReadError::Internal(e.to_string()))?;
        for component in components.iter().skip(1) {
            directory = directory
                .open_dir_nofollow(component.as_os_str())
                .map_err(|e| FormalReadError::Internal(e.to_string()))?;
        }
        oasis7_client_storage::private_storage::private_directory(&directory)
            .map_err(FormalReadError::Integrity)?;
        let mut spool = Spool::open(directory, max_spool_bytes, max_spool_entries)?;
        let mut collector = super::super::archive_record_v2::ResourceArchiveCollectorV2::new(
            max_spool_bytes,
            max_spool_entries,
        )?;
        let (send, receive) = mpsc::sync_channel::<ArchiveWorkV2>(2);
        let error = Arc::new(Mutex::new(None));
        let worker_error = Arc::clone(&error);
        let worker = std::thread::Builder::new()
            .name("oasis7-resource-activation-archive".into())
            .spawn(move || {
                let mut previous: Option<LocalQualifiedPublicationV1> = None;
                while let Ok(job) = receive.recv() {
                    match job {
                        ArchiveWorkV2::Snapshot(job) => {
                            // Legacy snapshot-only capture remains unavailable
                            // without actual committed Journal; no fallback.
                            let next = match job.restore() {
                                Ok(next) => next,
                                Err(error) => {
                                    previous = None;
                                    report(&worker_error, &error.to_string());
                                    continue;
                                }
                            };
                            if let Some(before) = previous.take() {
                                let result =
                                    PreparedGoalActivationArchiveV1::from_adjacent(&before, &next)
                                        .and_then(|prepared| {
                                            spool.persist(prepared.into_material())
                                        });
                                if let Err(error) = result {
                                    report(&worker_error, &error.to_string());
                                }
                            }
                            previous = Some(next);
                        }
                        ArchiveWorkV2::Original(job) => {
                            let result = collector.accept(*job).and_then(|record| {
                                if let Some(record) = record {
                                    spool.persist_v2(record)?;
                                }
                                Ok(())
                            });
                            if let Err(error) = result {
                                report(&worker_error, &error.to_string());
                            }
                        }
                    }
                }
            })
            .map_err(|e| FormalReadError::Internal(e.to_string()))?;
        Ok(Self {
            send: Mutex::new(Some(send)),
            worker: Mutex::new(Some(worker)),
            max_snapshot_bytes,
            error,
        })
    }
    /// No serialization, filesystem I/O, history traversal, wait or retry.
    pub fn offer(&self, cut: GoalActivationArchiveCutV1) -> FormalReadResult<()> {
        if cut.snapshot.len() > self.max_snapshot_bytes {
            return Err(FormalReadError::HistoryUnavailable(
                "activation snapshot exceeds archive spool bound".into(),
            ));
        }
        let sender = self.send.try_lock().map_err(|_| {
            FormalReadError::HistoryUnavailable("activation archive queue unavailable".into())
        })?;
        let sender = sender.as_ref().ok_or_else(|| {
            FormalReadError::HistoryUnavailable("activation archive queue stopped".into())
        })?;
        sender
            .try_send(ArchiveWorkV2::Snapshot(Box::new(cut)))
            .map_err(|error| match error {
                TrySendError::Full(_) => {
                    FormalReadError::HistoryUnavailable("activation archive queue full".into())
                }
                TrySendError::Disconnected(_) => FormalReadError::HistoryUnavailable(
                    "activation archive worker unavailable".into(),
                ),
            })
    }
    /// Actual Writer-owned input only; O(1) reference/opaque metadata handoff.
    pub fn offer_original(&self, cut: CapturedResourcePublicationV2) -> FormalReadResult<()> {
        if cut.original_len() > self.max_snapshot_bytes {
            return Err(FormalReadError::HistoryUnavailable(
                "owned publication exceeds configured input bound".into(),
            ));
        }
        let sender = self
            .send
            .try_lock()
            .map_err(|_| FormalReadError::HistoryUnavailable("archive queue unavailable".into()))?;
        let sender = sender
            .as_ref()
            .ok_or_else(|| FormalReadError::HistoryUnavailable("archive queue stopped".into()))?;
        sender
            .try_send(ArchiveWorkV2::Original(Box::new(cut)))
            .map_err(|_| {
                FormalReadError::HistoryUnavailable(
                    "archive original queue full/unavailable".into(),
                )
            })
    }
    /// Explicit owner stop outside Node/Viewer/publication/Runtime locks.
    /// Default Drop only closes the channel and never joins a worker.
    pub fn request_stop(&self) -> FormalReadResult<()> {
        let sender = self
            .send
            .lock()
            .map_err(|_| FormalReadError::Internal("archive sender lock poisoned".into()))?
            .take();
        drop(sender);
        Ok(())
    }
    /// A bounded owner poll: join only after the OS worker has already exited.
    /// Slow fsync leaves the owner alive/pending; no shared-lock wait is added.
    pub fn finish_shutdown(&self) -> FormalReadResult<()> {
        let mut slot = self
            .worker
            .lock()
            .map_err(|_| FormalReadError::Internal("archive worker lock poisoned".into()))?;
        if slot.as_ref().is_some_and(|worker| !worker.is_finished()) {
            return Err(FormalReadError::HistoryUnavailable(
                "activation archive shutdown pending".into(),
            ));
        }
        let worker = slot.take();
        drop(slot);
        if let Some(worker) = worker {
            worker
                .join()
                .map_err(|_| FormalReadError::Internal("archive worker panicked".into()))?;
        }
        Ok(())
    }
    /// Diagnostic only. `None` does not attest availability, payment or a floor.
    pub fn last_error(&self) -> FormalReadResult<Option<String>> {
        self.error
            .lock()
            .map(|error| error.clone())
            .map_err(|_| FormalReadError::Internal("archive diagnostic lock poisoned".into()))
    }
}
fn report(slot: &Mutex<Option<String>>, message: &str) {
    if let Ok(mut slot) = slot.lock() {
        *slot = Some(message.chars().take(384).collect());
    }
}
#[cfg(unix)]
struct Spool {
    directory: Dir,
    _lock: std::fs::File,
    files: std::collections::BTreeMap<String, u64>,
    bytes: u64,
    maximum_bytes: u64,
    maximum_entries: usize,
    healthy: bool,
}
#[cfg(unix)]
impl Spool {
    fn open(directory: Dir, maximum_bytes: u64, maximum_entries: usize) -> FormalReadResult<Self> {
        use cap_fs_ext::{FollowSymlinks, OpenOptionsFollowExt};
        use cap_std::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
        let mut options = cap_std::fs::OpenOptions::new();
        options
            .read(true)
            .write(true)
            .create(true)
            .mode(0o600)
            .follow(FollowSymlinks::No)
            .custom_flags(libc::O_NONBLOCK);
        let lock = directory
            .open_with(".activation-spool.lock", &options)
            .map_err(|e| FormalReadError::Internal(e.to_string()))?;
        let owner = directory
            .dir_metadata()
            .map_err(|e| FormalReadError::Internal(e.to_string()))?
            .uid();
        let metadata = lock
            .metadata()
            .map_err(|e| FormalReadError::Internal(e.to_string()))?;
        if !metadata.is_file()
            || metadata.nlink() != 1
            || metadata.uid() != owner
            || metadata.permissions().mode() & 0o7777 != 0o600
        {
            return Err(FormalReadError::Integrity(
                "activation spool lock metadata".into(),
            ));
        }
        let lock = lock.into_std();
        lock.try_lock().map_err(|_| {
            FormalReadError::HistoryUnavailable("activation spool already owned".into())
        })?;
        let mut files = std::collections::BTreeMap::new();
        let mut bytes = 0u64;
        for entry in directory
            .entries()
            .map_err(|e| FormalReadError::Internal(e.to_string()))?
        {
            let entry = entry.map_err(|e| FormalReadError::Internal(e.to_string()))?;
            let name = entry.file_name();
            let name = name.to_str().ok_or_else(|| {
                FormalReadError::Integrity("activation spool filename encoding".into())
            })?;
            if name == ".activation-spool.lock" {
                continue;
            }
            let digest = name
                .strip_suffix(".activation.cbor")
                .or_else(|| name.strip_suffix(".activation-v2.cbor"))
                .ok_or_else(|| {
                    FormalReadError::Integrity("activation spool unknown entry".into())
                })?;
            if digest.len() != 64
                || !digest
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            {
                return Err(FormalReadError::Integrity(
                    "activation spool filename".into(),
                ));
            }
            let metadata = directory
                .symlink_metadata(name)
                .map_err(|e| FormalReadError::Internal(e.to_string()))?;
            if !metadata.is_file()
                || metadata.nlink() != 1
                || metadata.uid() != owner
                || metadata.permissions().mode() & 0o7777 != 0o600
            {
                return Err(FormalReadError::Integrity(
                    "activation spool entry metadata".into(),
                ));
            }
            bytes = bytes.checked_add(metadata.len()).ok_or_else(|| {
                FormalReadError::Integrity("activation spool byte overflow".into())
            })?;
            files.insert(name.to_owned(), metadata.len());
            if bytes > maximum_bytes || files.len() > maximum_entries {
                return Err(FormalReadError::HistoryUnavailable(
                    "activation spool exceeds configured storage bound".into(),
                ));
            }
        }
        Ok(Self {
            directory,
            _lock: lock,
            files,
            bytes,
            maximum_bytes,
            maximum_entries,
            healthy: true,
        })
    }
    fn persist(&mut self, material: GoalActivationArchiveMaterialV1) -> FormalReadResult<()> {
        use oasis7_client_storage::private_storage::private_directory;
        if !self.healthy {
            return Err(FormalReadError::HistoryUnavailable(
                "activation spool requires explicit recovery".into(),
            ));
        }
        if material.results.is_empty() {
            return Ok(());
        }
        private_directory(&self.directory).map_err(FormalReadError::Integrity)?;
        let bytes = oasis7_wasm_abi::encode_canonical_cbor(&material)
            .map_err(|e| FormalReadError::Internal(e.to_string()))?;
        let filename = format!("{}.activation.cbor", blake3_hex(&bytes));
        self.persist_bytes(filename, bytes)
    }
    fn persist_v2(
        &mut self,
        record: super::super::archive_record_v2::ResourceGoalArchiveRecordV2,
    ) -> FormalReadResult<()> {
        let bytes =
            serde_cbor::to_vec(&record).map_err(|e| FormalReadError::Integrity(e.to_string()))?;
        let filename = format!("{}.activation-v2.cbor", blake3_hex(&bytes));
        self.persist_bytes(filename, bytes)
    }
    fn persist_bytes(&mut self, filename: String, bytes: Vec<u8>) -> FormalReadResult<()> {
        use oasis7_client_storage::private_storage::{
            private_directory, publish_immutable, read_private,
        };
        private_directory(&self.directory).map_err(FormalReadError::Integrity)?;
        if !self.healthy {
            return Err(FormalReadError::HistoryUnavailable(
                "activation spool requires recovery".into(),
            ));
        }
        if self.files.contains_key(&filename) {
            if read_private(&self.directory, &filename, bytes.len())
                .map_err(FormalReadError::Integrity)?
                != bytes
            {
                return Err(FormalReadError::Integrity(
                    "activation spool content changed".into(),
                ));
            }
            return Ok(());
        }
        let length = u64::try_from(bytes.len())
            .map_err(|_| FormalReadError::Integrity("activation material size".into()))?;
        let next = self
            .bytes
            .checked_add(length)
            .ok_or_else(|| FormalReadError::Integrity("activation spool byte overflow".into()))?;
        if next > self.maximum_bytes || self.files.len() >= self.maximum_entries {
            // No automatic deletion: dependencies on old activation anchors may
            // still be live. Missing capacity keeps new Resource reads off.
            return Err(FormalReadError::HistoryUnavailable(
                "activation spool capacity exhausted".into(),
            ));
        }
        // Any publication/fsync ambiguity poisons this writer. Its uncounted
        // file must be inventoried on reopen before accepting another write.
        self.healthy = false;
        if !publish_immutable(&self.directory, &filename, &bytes)
            .map_err(FormalReadError::Integrity)?
        {
            return Err(FormalReadError::Integrity(
                "activation spool changed outside its lifetime lock".into(),
            ));
        }
        self.bytes = next;
        self.files.insert(filename, length);
        self.healthy = true;
        Ok(())
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    struct Directory(std::path::PathBuf);
    impl Directory {
        fn new() -> Self {
            use std::os::unix::fs::DirBuilderExt;
            let mut random = [0u8; 16];
            getrandom::fill(&mut random).unwrap();
            let path = std::env::temp_dir()
                .canonicalize()
                .unwrap()
                .join(format!("oasis7-resource-archive-{}", hex::encode(random)));
            std::fs::DirBuilder::new()
                .mode(0o700)
                .create(&path)
                .unwrap();
            Self(path)
        }
        fn open(&self) -> Dir {
            Dir::open_ambient_dir(&self.0, cap_std::ambient_authority()).unwrap()
        }
    }
    impl Drop for Directory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn activation_archive_spool_inventory_enforces_total_capacity_and_lifetime_owner() {
        let directory = Directory::new();
        let dir = directory.open();
        let bytes = b"abc";
        let name = format!("{}.activation.cbor", blake3_hex(bytes));
        oasis7_client_storage::private_storage::publish_immutable(&dir, &name, bytes).unwrap();
        // Storage-only bytes, deliberately not a qualified archive/read proof.
        assert!(Spool::open(directory.open(), 2, 1).is_err());
        let spool = Spool::open(directory.open(), 3, 1).unwrap();
        assert_eq!(spool.bytes, 3);
        assert_eq!(spool.files.len(), 1);
        assert!(Spool::open(directory.open(), 3, 1).is_err());
        drop(spool);
        assert!(Spool::open(directory.open(), 3, 1).is_ok());
    }
    #[test]
    fn activation_archive_explicit_stop_drains_without_drop_join_or_late_offer() {
        let directory = Directory::new();
        let queue = GoalActivationArchiveQueueV1::open_existing(
            directory.0.clone(),
            1024 * 1024,
            1024 * 1024,
            8,
        )
        .unwrap();
        queue.request_stop().unwrap();
        let (world, _, anchor, _) = crate::formal_read_fixtures::actual_world_initial_fixture();
        let local = LocalStopPublication::from_publication_with_archive(
            FormalReadPublication::new_with_archive(anchor, world).unwrap(),
        )
        .unwrap();
        assert!(
            queue
                .offer(local.qualified_cut().archive_cut().unwrap())
                .is_err()
        );
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
        loop {
            if queue.finish_shutdown().is_ok() {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "archive worker failed to terminate"
            );
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        queue.finish_shutdown().unwrap();
    }
}
