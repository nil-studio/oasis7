//! Explicit clean-stop DATA export, keyed by the actual accepted record.
//! These files carry no accepted-source, pending, or execution qualification.
//! The loader's selected record is untrusted until both real Cold reopens agree.
use super::*;
use oasis7_distfs::{
    controlled_authority::formal::endpoint::IwsColdParentHintsV1,
    incremental_page_store::PageStoreError,
};
use oasis7_formal_protocol::{
    controlled_authority::{
        formal::bounded_context::FormalBoundedContextV1, replicated_protocol::EndpointRole,
    },
    incremental_execution::{AuthorityBinding, ExecutionFactKindV1},
    incremental_material::FactLocationV1,
};
use oasis7_proto::memory_budget::MemoryReservation;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::atomic::AtomicU64,
};

const MAGIC: &[u8; 16] = b"O7IWSCOLD-DATA1\0";
const MANIFEST_BYTES: usize = 472;
const LOCATION_BYTES: usize = 48;
const MANIFEST: &str = "checkpoint.v1";
const AUTHORIZATION: [&str; 2] = ["primary.authorization.cbor", "replica.authorization.cbor"];
const MAILBOX: &str = "mailbox.locations.v1";
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy)]
pub(crate) struct IwsNodeCheckpointLimitsV1 {
    pub max_projection_bytes: u64,
    pub max_mailbox_entries: u64,
    pub max_path_bytes: usize,
}

pub(crate) struct IwsNodeCheckpointPublishInputV1<'a> {
    pub root: &'a Path,
    pub primary: &'a IwsHostRuntimeV1,
    pub replica: &'a IwsHostRuntimeV1,
    pub mailbox_hints: &'a BTreeMap<[u8; 32], FactLocationV1>,
    pub retained_hints_fee: &'a MemoryReservation,
    pub limits: IwsNodeCheckpointLimitsV1,
}

pub(crate) struct IwsNodeCheckpointDataV1 {
    pub record: [u8; 32],
    pub fence: ExecutionFence,
    pub height: u64,
    pub authorization_hint_paths: [PathBuf; 2],
    pub facts: [IwsColdParentHintsV1; 2],
}

pub(crate) struct LoadedIwsNodeCheckpointV1 {
    data: IwsNodeCheckpointDataV1,
    mailbox_hints: BTreeMap<[u8; 32], FactLocationV1>,
    fee: MemoryReservation,
}
impl LoadedIwsNodeCheckpointV1 {
    #[cfg(any(test, feature = "iws-host-native-test-support"))]
    pub(crate) fn data(&self) -> &IwsNodeCheckpointDataV1 {
        &self.data
    }
    #[cfg(any(test, feature = "iws-host-native-test-support"))]
    pub(crate) fn mailbox_hints(&self) -> &BTreeMap<[u8; 32], FactLocationV1> {
        &self.mailbox_hints
    }
    /// The consumer must finish its own paid service admission inside this
    /// closure. On failure it must drop the original DATA here, rather than
    /// return it in an error. The map/path reservation covers the closure,
    /// including unwinding, but does not transfer into its return value.
    pub(crate) fn with_data<T>(
        self,
        consume: impl FnOnce(IwsNodeCheckpointDataV1, BTreeMap<[u8; 32], FactLocationV1>) -> T,
    ) -> T {
        let Self {
            data,
            mailbox_hints,
            fee,
        } = self;
        let result = consume(data, mailbox_hints);
        drop(fee);
        result
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct FileDigest {
    bytes: u64,
    sha256: [u8; 32],
}
#[derive(Clone, Copy)]
struct SelectedEndpoint {
    record: [u8; 32],
    fence: ExecutionFence,
    height: u64,
    facts: IwsColdParentHintsV1,
}
struct Manifest {
    selected: SelectedEndpoint,
    replica_facts: IwsColdParentHintsV1,
    authorization: [FileDigest; 2],
    mailbox_entries: u64,
    mailbox_sha256: [u8; 32],
}

fn invalid(message: &'static str) -> IwsHostRuntimeErrorV1 {
    FormalError::Invalid(message.into()).into()
}
fn physical(error: io::Error) -> IwsHostRuntimeErrorV1 {
    IwsHostRuntimeErrorV1::Physical(PageStoreError::Io(error))
}
fn scratch_fee(
    root: &Path,
    limits: IwsNodeCheckpointLimitsV1,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<MemoryReservation, IwsHostRuntimeErrorV1> {
    common.consume_steps(1)?;
    let path = root
        .as_os_str()
        .as_encoded_bytes()
        .len()
        .checked_add(128)
        .ok_or_else(|| invalid("clean checkpoint path layout"))?;
    if limits.max_projection_bytes == 0 || path > limits.max_path_bytes {
        return Err(invalid("clean checkpoint configured bound"));
    }
    // All path copies, fixed stack buffers, writer/hash state and bounded IO
    // diagnostics remain covered until the operation or loaded owner drops.
    let bytes = path
        .checked_mul(32)
        .and_then(|n| n.checked_add(size_of::<LoadedIwsNodeCheckpointV1>() + 32 * 1024))
        .ok_or_else(|| invalid("clean checkpoint scratch layout"))?;
    common.charge(1, bytes as u64)?;
    common
        .memory()
        .reserve(bytes, MemoryClass::Ordinary)
        .map_err(|e| IwsHostRuntimeErrorV1::Move(e.into()))
}
fn charge_path(
    path: &Path,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<(), IwsHostRuntimeErrorV1> {
    common.charge(1, path.as_os_str().as_encoded_bytes().len() as u64)?;
    Ok(())
}
fn record_directory(root: &Path, record: &[u8; 32]) -> Result<PathBuf, IwsHostRuntimeErrorV1> {
    let mut name = [0u8; 64];
    hex::encode_to_slice(record, &mut name).map_err(|_| invalid("clean checkpoint record name"))?;
    Ok(root.join(std::str::from_utf8(&name).map_err(|_| invalid("clean checkpoint record name"))?))
}
fn require_directory(
    path: &Path,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<(), IwsHostRuntimeErrorV1> {
    charge_path(path, common)?;
    if !fs::symlink_metadata(path)
        .map_err(physical)?
        .file_type()
        .is_dir()
    {
        return Err(invalid(
            "clean checkpoint directory must be a real directory",
        ));
    }
    Ok(())
}
fn make_directory(
    path: &Path,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<(), IwsHostRuntimeErrorV1> {
    charge_path(path, common)?;
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(path).map_err(physical)
}
fn sync_directory(
    path: &Path,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<(), IwsHostRuntimeErrorV1> {
    charge_path(path, common)?;
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_DIRECTORY);
    }
    let directory = options.open(path).map_err(physical)?;
    common.consume_steps(1)?;
    directory.sync_all().map_err(physical)
}
fn open_file(
    path: &Path,
    bytes: u64,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<File, IwsHostRuntimeErrorV1> {
    charge_path(path, common)?;
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        // Reject a FIFO without blocking before its metadata can be checked.
        // O_NONBLOCK does not alter regular-file reads.
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let file = options.open(path).map_err(physical)?;
    common.consume_steps(1)?;
    let metadata = file.metadata().map_err(physical)?;
    if !metadata.is_file() || metadata.len() != bytes {
        return Err(invalid("clean checkpoint regular file/length"));
    }
    Ok(file)
}
fn read_exact(
    file: &mut File,
    mut bytes: &mut [u8],
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<(), IwsHostRuntimeErrorV1> {
    while !bytes.is_empty() {
        let next = bytes.len().min(8192);
        common.charge(1, next as u64)?;
        match file.read(&mut bytes[..next]) {
            Ok(0) => return Err(physical(io::ErrorKind::UnexpectedEof.into())),
            Ok(n) => bytes = &mut bytes[n..],
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(physical(e)),
        }
    }
    Ok(())
}
fn require_eof(
    file: &mut File,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<(), IwsHostRuntimeErrorV1> {
    loop {
        common.charge(1, 1)?;
        match file.read(&mut [0u8; 1]) {
            Ok(0) => return Ok(()),
            Ok(_) => return Err(invalid("clean checkpoint changed/trailing bytes")),
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(physical(e)),
        }
    }
}
fn verify_file(
    path: &Path,
    expected: FileDigest,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<(), IwsHostRuntimeErrorV1> {
    let mut file = open_file(path, expected.bytes, common)?;
    let mut hash = Sha256::new();
    let mut remaining = expected.bytes;
    let mut buffer = [0u8; 8192];
    while remaining != 0 {
        let count = remaining.min(buffer.len() as u64) as usize;
        read_exact(&mut file, &mut buffer[..count], common)?;
        hash.update(&buffer[..count]);
        remaining -= count as u64;
    }
    require_eof(&mut file, common)?;
    if <[u8; 32]>::from(hash.finalize()) != expected.sha256 {
        return Err(invalid("clean checkpoint file digest"));
    }
    Ok(())
}

struct Stream<'a> {
    file: File,
    bytes: u64,
    maximum: u64,
    hash: Sha256,
    archive: &'a FormalArchiveReadWorkV1,
    cancelled: &'a AtomicBool,
    failure: Option<IwsHostRuntimeErrorV1>,
}
impl<'a> Stream<'a> {
    fn create(
        path: &Path,
        maximum: u64,
        archive: &'a FormalArchiveReadWorkV1,
        cancelled: &'a AtomicBool,
    ) -> Result<Self, IwsHostRuntimeErrorV1> {
        archive.charge(
            1,
            path.as_os_str().as_encoded_bytes().len() as u64,
            &mut || cancelled.load(Ordering::Acquire),
        )?;
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
        }
        Ok(Self {
            file: options.open(path).map_err(physical)?,
            bytes: 0,
            maximum,
            hash: Sha256::new(),
            archive,
            cancelled,
            failure: None,
        })
    }
    fn finish(mut self) -> Result<FileDigest, IwsHostRuntimeErrorV1> {
        if let Some(error) = self.failure.take() {
            return Err(error);
        }
        self.archive
            .consume_steps(1, &mut || self.cancelled.load(Ordering::Acquire))?;
        self.file.sync_all().map_err(physical)?;
        Ok(FileDigest {
            bytes: self.bytes,
            sha256: self.hash.finalize().into(),
        })
    }
    fn fail(&mut self, error: IwsHostRuntimeErrorV1) -> io::Error {
        self.failure = Some(error);
        io::Error::other("bounded clean checkpoint write stopped")
    }
}
impl Write for Stream<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.failure.is_some() {
            return Err(io::Error::other(
                "bounded clean checkpoint write already stopped",
            ));
        }
        if bytes.is_empty() {
            return Ok(0);
        }
        let part = &bytes[..bytes.len().min(8192)];
        if self
            .bytes
            .checked_add(part.len() as u64)
            .is_none_or(|n| n > self.maximum)
        {
            return Err(self.fail(invalid("clean checkpoint file bound")));
        }
        if let Err(error) = self.archive.charge(1, part.len() as u64, &mut || {
            self.cancelled.load(Ordering::Acquire)
        }) {
            return Err(self.fail(error.into()));
        }
        match self.file.write(part) {
            Ok(0) => Err(self.fail(physical(io::ErrorKind::WriteZero.into()))),
            Ok(n) => {
                self.hash.update(&part[..n]);
                self.bytes += n as u64;
                Ok(n)
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => Err(error),
            Err(error) => Err(self.fail(physical(error))),
        }
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn write_stream(stream: &mut Stream<'_>, bytes: &[u8]) -> Result<(), IwsHostRuntimeErrorV1> {
    if let Err(error) = stream.write_all(bytes) {
        return Err(stream.failure.take().unwrap_or_else(|| physical(error)));
    }
    Ok(())
}

fn selected_endpoint(
    host: &IwsHostRuntimeV1,
    role: EndpointRole,
    hints: &BTreeMap<[u8; 32], FactLocationV1>,
    memory: &MemoryBudget,
    archive: &FormalArchiveReadWorkV1,
    cancelled: &AtomicBool,
) -> Result<SelectedEndpoint, IwsHostRuntimeErrorV1> {
    let current = host
        .current
        .current()
        .ok_or_else(|| invalid("clean checkpoint actual Host missing"))?;
    if host.continuation.is_some() || current.source.endpoint_role() != role {
        return Err(invalid("clean checkpoint endpoint continuation/role"));
    }
    current.source.with_parent_read_view_bounded(
        memory,
        archive,
        &mut || cancelled.load(Ordering::Acquire),
        |parent| {
            let mut stop = || cancelled.load(Ordering::Acquire);
            let mut common = archive.bounded_context(memory, &mut stop);
            let fence = parent.fence();
            let record = parent.head().verified_record_id();
            current
                .resident
                .as_ref()
                .ok_or_else(|| invalid("clean checkpoint actual resident missing"))?
                .validate_live_binding_v1(record, &fence, &mut common)
                .map_err(IwsHostRuntimeErrorV1::NormalSource)?;
            let height = parent.core().body().height;
            common.charge(1, 256)?;
            if record != current.source.record_id() || fence != current.source.fence() {
                return Err(invalid("clean checkpoint actual parent changed"));
            }
            for (ordinal, fact) in parent.core().facts().facts.iter().enumerate() {
                common.consume_steps(1)?;
                if fact.kind != ExecutionFactKindV1::Event {
                    continue;
                }
                let ordinal =
                    u64::try_from(ordinal).map_err(|_| invalid("clean checkpoint fact ordinal"))?;
                let steps = 12 * usize::BITS as u64 + 1;
                common.charge(steps, steps * 96)?;
                if hints.get(&fact.id)
                    != Some(&FactLocationV1 {
                        height,
                        ordinal,
                        original: fact.id,
                    })
                {
                    return Err(invalid("clean checkpoint current mailbox locator missing"));
                }
            }
            Ok(SelectedEndpoint {
                record,
                fence,
                height,
                facts: IwsColdParentHintsV1 {
                    runtime: parent.runtime().occurrence().location(),
                    cognition: parent.cognition().selected().occurrence().location(),
                },
            })
        },
    )
}
fn export_projection(
    host: &IwsHostRuntimeV1,
    path: &Path,
    limits: IwsNodeCheckpointLimitsV1,
    memory: &MemoryBudget,
    archive: &FormalArchiveReadWorkV1,
    cancelled: &AtomicBool,
) -> Result<FileDigest, IwsHostRuntimeErrorV1> {
    let current = host
        .current
        .current()
        .ok_or_else(|| invalid("clean checkpoint actual Host missing"))?;
    let resident = current
        .resident
        .as_ref()
        .ok_or_else(|| invalid("clean checkpoint actual resident missing"))?;
    let mut stream = Stream::create(path, limits.max_projection_bytes, archive, cancelled)?;
    let emitted = current.source.with_parent_read_view_bounded(
        memory,
        archive,
        &mut || cancelled.load(Ordering::Acquire),
        |parent| {
            let mut stop = || cancelled.load(Ordering::Acquire);
            let mut common = archive.bounded_context(memory, &mut stop);
            resident
                .write_accepted_authorization_hint_bounded_v1(
                    parent.head(),
                    &parent.fence(),
                    &mut stream,
                    &mut common,
                )
                .map_err(IwsHostRuntimeErrorV1::NormalSource)
        },
    );
    if let Some(error) = stream.failure.take() {
        return Err(error);
    }
    if emitted? != stream.bytes {
        return Err(invalid("clean checkpoint projection stream length"));
    }
    stream.finish()
}

fn put(out: &mut [u8], offset: &mut usize, bytes: &[u8]) {
    out[*offset..*offset + bytes.len()].copy_from_slice(bytes);
    *offset += bytes.len();
}
fn take<const N: usize>(raw: &[u8; MANIFEST_BYTES], offset: &mut usize) -> [u8; N] {
    let mut value = [0u8; N];
    value.copy_from_slice(&raw[*offset..*offset + N]);
    *offset += N;
    value
}
fn put_location(out: &mut [u8], offset: &mut usize, location: FactLocationV1) {
    put(out, offset, &location.original);
    put(out, offset, &location.height.to_le_bytes());
    put(out, offset, &location.ordinal.to_le_bytes());
}
fn take_location(raw: &[u8; MANIFEST_BYTES], offset: &mut usize) -> FactLocationV1 {
    FactLocationV1 {
        original: take(raw, offset),
        height: u64::from_le_bytes(take(raw, offset)),
        ordinal: u64::from_le_bytes(take(raw, offset)),
    }
}
impl Manifest {
    fn encode(&self) -> [u8; MANIFEST_BYTES] {
        let mut raw = [0u8; MANIFEST_BYTES];
        let mut offset = 0;
        put(&mut raw, &mut offset, MAGIC);
        put(&mut raw, &mut offset, &self.selected.record);
        put(&mut raw, &mut offset, &self.selected.fence.parent_commit);
        put(&mut raw, &mut offset, &self.selected.fence.parent_roots);
        put(&mut raw, &mut offset, &self.selected.fence.authority.token);
        put(
            &mut raw,
            &mut offset,
            &self.selected.fence.authority.epoch.to_le_bytes(),
        );
        put(&mut raw, &mut offset, &self.selected.height.to_le_bytes());
        for facts in [self.selected.facts, self.replica_facts] {
            put_location(&mut raw, &mut offset, facts.runtime);
            put_location(&mut raw, &mut offset, facts.cognition);
        }
        for value in self.authorization {
            put(&mut raw, &mut offset, &value.bytes.to_le_bytes());
            put(&mut raw, &mut offset, &value.sha256);
        }
        put(&mut raw, &mut offset, &self.mailbox_entries.to_le_bytes());
        put(&mut raw, &mut offset, &self.mailbox_sha256);
        debug_assert_eq!(offset, MANIFEST_BYTES);
        raw
    }
    fn decode(
        raw: &[u8; MANIFEST_BYTES],
        selected_record: [u8; 32],
        limits: IwsNodeCheckpointLimitsV1,
    ) -> Result<Self, IwsHostRuntimeErrorV1> {
        let mut offset = 0;
        if &take::<16>(raw, &mut offset) != MAGIC {
            return Err(invalid("clean checkpoint format"));
        }
        let record = take(raw, &mut offset);
        let parent_commit = take(raw, &mut offset);
        let parent_roots = take(raw, &mut offset);
        let token = take(raw, &mut offset);
        let epoch = u64::from_le_bytes(take(raw, &mut offset));
        let height = u64::from_le_bytes(take(raw, &mut offset));
        let facts = IwsColdParentHintsV1 {
            runtime: take_location(raw, &mut offset),
            cognition: take_location(raw, &mut offset),
        };
        let replica_facts = IwsColdParentHintsV1 {
            runtime: take_location(raw, &mut offset),
            cognition: take_location(raw, &mut offset),
        };
        let authorization = std::array::from_fn(|_| FileDigest {
            bytes: u64::from_le_bytes(take(raw, &mut offset)),
            sha256: take(raw, &mut offset),
        });
        let mailbox_entries = u64::from_le_bytes(take(raw, &mut offset));
        let mailbox_sha256 = take(raw, &mut offset);
        if record != selected_record
            || mailbox_entries > limits.max_mailbox_entries
            || mailbox_entries.checked_mul(LOCATION_BYTES as u64).is_none()
            || authorization[0] != authorization[1]
            || authorization
                .iter()
                .any(|file| file.bytes == 0 || file.bytes > limits.max_projection_bytes)
            || [
                facts.runtime,
                facts.cognition,
                replica_facts.runtime,
                replica_facts.cognition,
            ]
            .iter()
            .any(|location| location.height > height)
        {
            return Err(invalid("clean checkpoint selected record/bounds"));
        }
        Ok(Self {
            selected: SelectedEndpoint {
                record,
                fence: ExecutionFence {
                    parent_commit,
                    parent_roots,
                    authority: AuthorityBinding { token, epoch },
                },
                height,
                facts,
            },
            replica_facts,
            authorization,
            mailbox_entries,
            mailbox_sha256,
        })
    }
}

struct TempDirectory {
    path: PathBuf,
    active: bool,
}
impl Drop for TempDirectory {
    fn drop(&mut self) {
        if self.active {
            // Only these four files can be created here. Never recursively
            // remove a directory containing someone else's additional files.
            for name in [AUTHORIZATION[0], AUTHORIZATION[1], MAILBOX, MANIFEST] {
                let _ = fs::remove_file(self.path.join(name));
            }
            let _ = fs::remove_dir(&self.path);
        }
    }
}
fn verify_existing(
    directory: &Path,
    raw: &[u8; MANIFEST_BYTES],
    manifest: &Manifest,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<(), IwsHostRuntimeErrorV1> {
    require_directory(directory, common)?;
    let mut existing = [0u8; MANIFEST_BYTES];
    let mut file = open_file(&directory.join(MANIFEST), MANIFEST_BYTES as u64, common)?;
    read_exact(&mut file, &mut existing, common)?;
    require_eof(&mut file, common)?;
    if &existing != raw {
        return Err(invalid("clean checkpoint immutable record already differs"));
    }
    for (name, expected) in AUTHORIZATION.into_iter().zip(manifest.authorization) {
        verify_file(&directory.join(name), expected, common)?;
    }
    verify_file(
        &directory.join(MAILBOX),
        FileDigest {
            bytes: manifest.mailbox_entries * LOCATION_BYTES as u64,
            sha256: manifest.mailbox_sha256,
        },
        common,
    )
}

/// Caller holds the stopped owner's lock and has checked Idle/no original.
/// This routine also checks both real accepted sources and refuses continuation.
/// It is an explicit clean checkpoint, not a full-history rewrite on every tick.
/// An accepted publication without this record's completed DATA is not restartable
/// through this entry: the next process must fail closed on the missing record.
pub(crate) fn publish_clean_node_checkpoint_bounded_v1(
    input: IwsNodeCheckpointPublishInputV1<'_>,
    memory: &MemoryBudget,
    archive: &FormalArchiveReadWorkV1,
    cancelled: &AtomicBool,
) -> Result<[u8; 32], IwsHostRuntimeErrorV1> {
    let IwsNodeCheckpointPublishInputV1 {
        root,
        primary,
        replica,
        mailbox_hints,
        retained_hints_fee,
        limits,
    } = input;
    let mut stop = || cancelled.load(Ordering::Acquire);
    let mut common = archive.bounded_context(memory, &mut stop);
    let _fee = scratch_fee(root, limits, &mut common)?;
    let count = u64::try_from(mailbox_hints.len())
        .map_err(|_| invalid("clean checkpoint mailbox count"))?;
    if count > limits.max_mailbox_entries
        || !retained_hints_fee.uses_budget(memory)
        || retained_hints_fee.bytes()
            < super::node_hints::mailbox_hint_heap_bytes_v1(mailbox_hints.len())?
    {
        return Err(invalid("clean checkpoint mailbox owner/bound"));
    }
    let p = selected_endpoint(
        primary,
        EndpointRole::Primary,
        mailbox_hints,
        memory,
        archive,
        cancelled,
    )?;
    let r = selected_endpoint(
        replica,
        EndpointRole::Replica,
        mailbox_hints,
        memory,
        archive,
        cancelled,
    )?;
    common.charge(1, 256)?;
    if p.record != r.record
        || p.fence != r.fence
        || p.height != r.height
        || primary
            .current
            .current()
            .map(|c| c.source.execution_environment())
            != replica
                .current
                .current()
                .map(|c| c.source.execution_environment())
    {
        return Err(invalid("clean checkpoint dual accepted parent mismatch"));
    }
    charge_path(root, &mut common)?;
    match fs::symlink_metadata(root) {
        Ok(metadata) if metadata.file_type().is_dir() => {}
        Ok(_) => return Err(invalid("clean checkpoint root is not a directory")),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            make_directory(root, &mut common)?;
            sync_directory(
                root.parent()
                    .filter(|p| !p.as_os_str().is_empty())
                    .unwrap_or(Path::new(".")),
                &mut common,
            )?;
        }
        Err(error) => return Err(physical(error)),
    }
    // Prepay the five fixed best-effort cleanup operations; failed work stays spent.
    common.consume_steps(5)?;
    let sequence = TEMP_SEQUENCE
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
        .map_err(|_| invalid("clean checkpoint temporary sequence exhausted"))?;
    let path = root.join(format!(".checkpoint-{}-{sequence}", std::process::id()));
    make_directory(&path, &mut common)?;
    let mut temporary = TempDirectory { path, active: true };
    let authorization = [
        export_projection(
            primary,
            &temporary.path.join(AUTHORIZATION[0]),
            limits,
            memory,
            archive,
            cancelled,
        )?,
        export_projection(
            replica,
            &temporary.path.join(AUTHORIZATION[1]),
            limits,
            memory,
            archive,
            cancelled,
        )?,
    ];
    if authorization[0] != authorization[1] {
        return Err(invalid(
            "clean checkpoint dual canonical authorization differs",
        ));
    }
    let mailbox_bytes = count
        .checked_mul(LOCATION_BYTES as u64)
        .ok_or_else(|| invalid("clean checkpoint mailbox size"))?;
    let mut mailbox = Stream::create(
        &temporary.path.join(MAILBOX),
        mailbox_bytes,
        archive,
        cancelled,
    )?;
    for (key, location) in mailbox_hints {
        common.charge(1, LOCATION_BYTES as u64)?;
        if key != &location.original || location.height > p.height {
            return Err(invalid("clean checkpoint mailbox key/height"));
        }
        let mut raw = [0u8; LOCATION_BYTES];
        let mut offset = 0;
        put_location(&mut raw, &mut offset, *location);
        write_stream(&mut mailbox, &raw)?;
    }
    let mailbox = mailbox.finish()?;
    if mailbox.bytes != mailbox_bytes {
        return Err(invalid("clean checkpoint mailbox stream length"));
    }
    let manifest = Manifest {
        selected: p,
        replica_facts: r.facts,
        authorization,
        mailbox_entries: count,
        mailbox_sha256: mailbox.sha256,
    };
    common.charge(1, MANIFEST_BYTES as u64)?;
    let raw = manifest.encode();
    let mut output = Stream::create(
        &temporary.path.join(MANIFEST),
        MANIFEST_BYTES as u64,
        archive,
        cancelled,
    )?;
    write_stream(&mut output, &raw)?;
    output.finish()?;
    sync_directory(&temporary.path, &mut common)?;
    let target = record_directory(root, &p.record)?;
    charge_path(&target, &mut common)?;
    match fs::symlink_metadata(&target) {
        Ok(_) => verify_existing(&target, &raw, &manifest, &mut common)?,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            common.consume_steps(1)?;
            // A published record directory is nonempty, so rename cannot
            // replace it even if another publisher wins the existence race.
            fs::rename(&temporary.path, &target).map_err(physical)?;
            temporary.active = false;
        }
        Err(error) => return Err(physical(error)),
    }
    sync_directory(&target, &mut common)?;
    // Failure/cancellation after rename is an unknown durability outcome.
    // Retry compares every existing byte and syncs again; it never overwrites.
    sync_directory(root, &mut common)?;
    common.consume_steps(0)?;
    Ok(p.record)
}

/// Read only candidate DATA for the explicitly selected record. A caller must
/// independently reopen both endpoints and compare their actual record/fence.
/// Missing, malformed, over-budget or conflicting DATA never falls back to GEN.
pub(crate) fn load_node_checkpoint_bounded_v1(
    root: &Path,
    selected_record: [u8; 32],
    limits: IwsNodeCheckpointLimitsV1,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<LoadedIwsNodeCheckpointV1, IwsHostRuntimeErrorV1> {
    let mut fee = scratch_fee(root, limits, common)?;
    require_directory(root, common)?;
    let directory = record_directory(root, &selected_record)?;
    require_directory(&directory, common)?;
    let mut file = open_file(&directory.join(MANIFEST), MANIFEST_BYTES as u64, common)?;
    let mut raw = [0u8; MANIFEST_BYTES];
    read_exact(&mut file, &mut raw, common)?;
    require_eof(&mut file, common)?;
    let manifest = Manifest::decode(&raw, selected_record, limits)?;
    let count = usize::try_from(manifest.mailbox_entries)
        .map_err(|_| invalid("clean checkpoint mailbox count"))?;
    let mut map_fee = common
        .memory()
        .reserve(
            super::node_hints::mailbox_hint_heap_bytes_v1(count)?,
            MemoryClass::Ordinary,
        )
        .map_err(|e| IwsHostRuntimeErrorV1::Move(e.into()))?;
    fee.merge(&mut map_fee)
        .map_err(|e| FormalError::Invalid(e.into()))?;
    let authorization_hint_paths = AUTHORIZATION.map(|name| directory.join(name));
    for (path, expected) in authorization_hint_paths.iter().zip(manifest.authorization) {
        verify_file(path, expected, common)?;
    }
    let bytes = manifest
        .mailbox_entries
        .checked_mul(LOCATION_BYTES as u64)
        .ok_or_else(|| invalid("clean checkpoint mailbox size"))?;
    let mut file = open_file(&directory.join(MAILBOX), bytes, common)?;
    let mut mailbox_hints = BTreeMap::new();
    let mut hash = Sha256::new();
    let mut previous = None;
    for _ in 0..count {
        let mut raw = [0u8; LOCATION_BYTES];
        read_exact(&mut file, &mut raw, common)?;
        hash.update(raw);
        let mut key = [0u8; 32];
        key.copy_from_slice(&raw[..32]);
        let mut height = [0u8; 8];
        height.copy_from_slice(&raw[32..40]);
        let mut ordinal = [0u8; 8];
        ordinal.copy_from_slice(&raw[40..48]);
        let location = FactLocationV1 {
            original: key,
            height: u64::from_le_bytes(height),
            ordinal: u64::from_le_bytes(ordinal),
        };
        let steps = 2 * 12 * usize::BITS as u64 + 1;
        common.charge(steps, steps * 96)?;
        if previous.is_some_and(|before| before >= key)
            || location.height > manifest.selected.height
        {
            return Err(invalid("clean checkpoint mailbox order/height"));
        }
        mailbox_hints.insert(key, location);
        previous = Some(key);
    }
    require_eof(&mut file, common)?;
    if <[u8; 32]>::from(hash.finalize()) != manifest.mailbox_sha256 {
        return Err(invalid("clean checkpoint mailbox digest"));
    }
    common.consume_steps(0)?;
    Ok(LoadedIwsNodeCheckpointV1 {
        data: IwsNodeCheckpointDataV1 {
            record: manifest.selected.record,
            fence: manifest.selected.fence,
            height: manifest.selected.height,
            authorization_hint_paths,
            facts: [manifest.selected.facts, manifest.replica_facts],
        },
        mailbox_hints,
        fee,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Directory(PathBuf);
    impl Directory {
        fn new() -> Self {
            let number = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "oasis7-clean-checkpoint-data-{}-{number}",
                std::process::id()
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Directory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn limits() -> IwsNodeCheckpointLimitsV1 {
        IwsNodeCheckpointLimitsV1 {
            max_projection_bytes: 1024,
            max_mailbox_entries: 4,
            max_path_bytes: 4096,
        }
    }
    fn location(id: u8, height: u64, ordinal: u64) -> FactLocationV1 {
        FactLocationV1 {
            original: [id; 32],
            height,
            ordinal,
        }
    }
    // This seeds only untrusted DATA. It deliberately cannot construct a Host,
    // accepted Current, Node continuation, or successful Cold qualification.
    fn seed(root: &Path, record: [u8; 32]) -> (PathBuf, Manifest) {
        let directory = record_directory(root, &record).unwrap();
        fs::create_dir(&directory).unwrap();
        let projection = b"unqualified projection DATA";
        for name in AUTHORIZATION {
            fs::write(directory.join(name), projection).unwrap();
        }
        let mut mailbox = [0u8; 2 * LOCATION_BYTES];
        let mut offset = 0;
        put_location(&mut mailbox, &mut offset, location(1, 0, 7));
        put_location(&mut mailbox, &mut offset, location(2, 2, 3));
        fs::write(directory.join(MAILBOX), mailbox).unwrap();
        let manifest = Manifest {
            selected: SelectedEndpoint {
                record,
                fence: ExecutionFence {
                    parent_commit: [8; 32],
                    parent_roots: [9; 32],
                    authority: AuthorityBinding {
                        token: [10; 32],
                        epoch: 3,
                    },
                },
                height: 2,
                facts: IwsColdParentHintsV1 {
                    runtime: location(11, 2, 1),
                    cognition: location(12, 2, 2),
                },
            },
            replica_facts: IwsColdParentHintsV1 {
                runtime: location(11, 2, 1),
                cognition: location(12, 2, 2),
            },
            authorization: [FileDigest {
                bytes: projection.len() as u64,
                sha256: Sha256::digest(projection).into(),
            }; 2],
            mailbox_entries: 2,
            mailbox_sha256: Sha256::digest(mailbox).into(),
        };
        fs::write(directory.join(MANIFEST), manifest.encode()).unwrap();
        (directory, manifest)
    }
    fn read(
        root: &Path,
        record: [u8; 32],
        limits: IwsNodeCheckpointLimitsV1,
        memory: &MemoryBudget,
    ) -> Result<LoadedIwsNodeCheckpointV1, IwsHostRuntimeErrorV1> {
        let archive = FormalArchiveReadWorkV1::new(1_000_000, 16 << 20).unwrap();
        let mut stop = || false;
        let mut common = archive.bounded_context(memory, &mut stop);
        load_node_checkpoint_bounded_v1(root, record, limits, &mut common)
    }

    #[test]
    fn clean_checkpoint_manifest_pins_fixed_wire_and_selected_record() {
        let root = Directory::new();
        let (_, manifest) = seed(&root.0, [7; 32]);
        let raw = manifest.encode();
        assert_eq!(raw.len(), 472);
        assert_eq!(&raw[..16], b"O7IWSCOLD-DATA1\0");
        assert_eq!(&raw[16..48], &[7; 32]);
        assert_eq!(&raw[48..80], &[8; 32]);
        assert_eq!(&raw[80..112], &[9; 32]);
        assert_eq!(&raw[144..152], &3u64.to_le_bytes());
        assert_eq!(&raw[152..160], &2u64.to_le_bytes());
        assert_eq!(&raw[160..192], &[11; 32]);
        assert_eq!(&raw[192..200], &2u64.to_le_bytes());
        assert_eq!(&raw[200..208], &1u64.to_le_bytes());
        assert_eq!(&raw[432..440], &2u64.to_le_bytes());
        assert!(Manifest::decode(&raw, [6; 32], limits()).is_err());
    }

    #[test]
    fn clean_checkpoint_load_keeps_old_locations_and_paid_admission() {
        let root = Directory::new();
        seed(&root.0, [7; 32]);
        let memory = MemoryBudget::new(2 << 20, 0).unwrap();
        let loaded = read(&root.0, [7; 32], limits(), &memory).unwrap();
        let paid = memory.used();
        assert!(paid >= super::super::node_hints::mailbox_hint_heap_bytes_v1(2).unwrap());
        assert_eq!(
            loaded.mailbox_hints().get(&[1; 32]),
            Some(&location(1, 0, 7))
        );
        assert_eq!(
            loaded.mailbox_hints().get(&[2; 32]),
            Some(&location(2, 2, 3))
        );
        assert_eq!(loaded.data().record, [7; 32]);
        loaded.with_data(|data, hints| {
            assert_eq!(memory.used(), paid);
            assert_eq!(data.fence.authority.epoch, 3);
            assert_eq!(hints.len(), 2);
            assert_ne!(
                data.authorization_hint_paths[0],
                data.authorization_hint_paths[1]
            );
        });
        assert_eq!(memory.used(), 0);
    }

    #[test]
    fn clean_checkpoint_load_rejects_missing_truncated_and_corrupt_data() {
        let root = Directory::new();
        let (directory, _) = seed(&root.0, [7; 32]);
        let memory = MemoryBudget::new(2 << 20, 0).unwrap();
        assert!(read(&root.0, [6; 32], limits(), &memory).is_err());
        let original = fs::read(directory.join(MAILBOX)).unwrap();
        fs::write(directory.join(MAILBOX), &original[..original.len() - 1]).unwrap();
        assert!(read(&root.0, [7; 32], limits(), &memory).is_err());
        fs::write(directory.join(MAILBOX), original).unwrap();
        let mut projection = fs::read(directory.join(AUTHORIZATION[1])).unwrap();
        projection[0] ^= 1;
        fs::write(directory.join(AUTHORIZATION[1]), projection).unwrap();
        assert!(read(&root.0, [7; 32], limits(), &memory).is_err());
        assert_eq!(memory.used(), 0);
    }

    #[cfg(unix)]
    #[test]
    fn clean_checkpoint_load_rejects_fifos_without_blocking() {
        const CHILD: &str = "OASIS7_CHECKPOINT_FIFO_REJECTION_CHILD";
        if std::env::var_os(CHILD).is_some() {
            use std::{ffi::CString, os::unix::ffi::OsStrExt};
            for name in [MANIFEST, AUTHORIZATION[0], AUTHORIZATION[1], MAILBOX] {
                let root = Directory::new();
                let (directory, _) = seed(&root.0, [7; 32]);
                let path = directory.join(name);
                fs::remove_file(&path).unwrap();
                let path = CString::new(path.as_os_str().as_bytes()).unwrap();
                // SAFETY: path is a live, NUL-terminated local test path.
                assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
                let memory = MemoryBudget::new(2 << 20, 0).unwrap();
                assert!(matches!(
                    read(&root.0, [7; 32], limits(), &memory),
                    Err(IwsHostRuntimeErrorV1::Formal(FormalError::Invalid(message)))
                        if message == "clean checkpoint regular file/length"
                ));
                assert_eq!(memory.used(), 0);
                if AUTHORIZATION.contains(&name) {
                    let archive = FormalArchiveReadWorkV1::new(1_000_000, 16 << 20).unwrap();
                    let mut stop = || false;
                    let mut common = archive.bounded_context(&memory, &mut stop);
                    let projection_limits = oasis7_runtime::AuthorizationRootCacheLimitsV1 {
                        max_wire_bytes: 1024,
                        max_depth: 128,
                        max_map_keys: 8,
                        max_key_bytes: 128,
                    };
                    assert!(matches!(
                        super::super::cold_recovery::read_projection_hint_bounded_v1(
                            &directory.join(name), projection_limits, &mut common,
                        ),
                        Err(IwsHostRuntimeErrorV1::Formal(FormalError::Invalid(message)))
                            if message == "cold projection hint regular file"
                    ));
                    assert_eq!(memory.used(), 0);
                }
            }
            return;
        }
        // A regression must fail with a deadline, rather than hang this suite
        // in the blocking open that it is meant to detect.
        let full_name = concat!(
            module_path!(),
            "::clean_checkpoint_load_rejects_fifos_without_blocking"
        );
        let (_, test_name) = full_name.split_once("::").unwrap();
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([test_name, "--exact", "--nocapture", "--test-threads=1"])
            .env(CHILD, "1")
            .spawn()
            .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            if let Some(status) = child.try_wait().unwrap() {
                assert!(status.success(), "FIFO rejection child failed: {status}");
                break;
            }
            if std::time::Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                panic!("checkpoint read blocked on a non-regular file");
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    #[test]
    fn clean_checkpoint_load_rejects_duplicate_locators_even_with_matching_digest() {
        let root = Directory::new();
        let (directory, mut manifest) = seed(&root.0, [7; 32]);
        let mut mailbox = fs::read(directory.join(MAILBOX)).unwrap();
        mailbox.copy_within(..LOCATION_BYTES, LOCATION_BYTES);
        manifest.mailbox_sha256 = Sha256::digest(&mailbox).into();
        fs::write(directory.join(MAILBOX), mailbox).unwrap();
        fs::write(directory.join(MANIFEST), manifest.encode()).unwrap();
        let memory = MemoryBudget::new(2 << 20, 0).unwrap();
        assert!(read(&root.0, [7; 32], limits(), &memory).is_err());
        assert_eq!(memory.used(), 0);
    }

    #[test]
    fn clean_checkpoint_bounds_and_cancellation_do_not_refill_work() {
        let root = Directory::new();
        seed(&root.0, [7; 32]);
        let memory = MemoryBudget::new(2 << 20, 0).unwrap();
        let mut small = limits();
        small.max_mailbox_entries = 1;
        assert!(read(&root.0, [7; 32], small, &memory).is_err());
        let archive = FormalArchiveReadWorkV1::new(10, 100).unwrap();
        let before = (archive.remaining_steps(), archive.remaining_read_bytes());
        let mut stop = || true;
        let mut common = archive.bounded_context(&memory, &mut stop);
        assert!(load_node_checkpoint_bounded_v1(&root.0, [7; 32], limits(), &mut common).is_err());
        assert!(archive.remaining_steps() <= before.0);
        assert!(archive.remaining_read_bytes() <= before.1);
        assert_eq!(memory.used(), 0);
    }

    #[test]
    fn clean_checkpoint_existing_record_is_compared_without_overwrite() {
        let root = Directory::new();
        let (directory, manifest) = seed(&root.0, [7; 32]);
        let (_, other) = seed(&root.0, [6; 32]);
        let memory = MemoryBudget::new(2 << 20, 0).unwrap();
        let archive = FormalArchiveReadWorkV1::new(1_000_000, 16 << 20).unwrap();
        let mut stop = || false;
        let mut common = archive.bounded_context(&memory, &mut stop);
        verify_existing(&directory, &manifest.encode(), &manifest, &mut common).unwrap();
        assert!(verify_existing(&directory, &other.encode(), &other, &mut common).is_err());
        assert_eq!(
            fs::read(directory.join(MANIFEST)).unwrap(),
            manifest.encode()
        );
        assert!(
            record_directory(&root.0, &[6; 32])
                .unwrap()
                .join(MANIFEST)
                .is_file()
        );
    }
}
