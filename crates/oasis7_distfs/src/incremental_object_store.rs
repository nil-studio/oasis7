//! Original immutable objects on the bounded page store. Flat original IDs
//! remain unchanged; the physical locator is never an authenticated range root.
//! Full reads verify that original ID before returning an owned content value.
use super::*;
use oasis7_formal_protocol::incremental_value::{ValueIoBudget, ValueReadContext, write_value};
use std::io::{Read, Write};

const CAPABILITY: &[u8] = b"oasis7.iws.object-aliases.v1\n";
const LOCATOR: &[u8] = b"oasis7.iws.object-locator.v1\0";
const LOCATOR_BOUND: usize = 256;
const WORKSPACE: usize = 64 * 1024;

pub struct IncrementalObjectStore<'a> {
    pages: &'a IncrementalPageStore,
    _charge: MemoryReservation,
}
/// A locator is physical metadata only. Its root cannot authorize partial
/// output under the original flat ID without a full verification/valid lease.
struct ObjectLocator {
    length: u64,
    value_root: [u8; 32],
}
impl<'a> IncrementalObjectStore<'a> {
    /// Borrow the actual object IO allowances for live candidate core checks.
    pub fn original_reader<'s, 'io, 'quota>(
        &'s self,
        io: &'io mut ObjectIo<'quota>,
    ) -> ObjectReadSession<'s, 'a, 'io, 'quota> {
        ObjectReadSession { store: self, io }
    }
    /// Explicitly install this capability on a page pool. A partial creation
    /// is retained, never guessed complete by Open or silently cleared.
    pub fn create(
        pages: &'a IncrementalPageStore,
        io: &mut PageIoBudget,
        cancelled: &AtomicBool,
    ) -> Result<Self> {
        let _workspace = pages.memory.reserve(WORKSPACE, MemoryClass::Ordinary)?;
        check_cancelled(cancelled)?;
        files::validate_store_io(&pages.root, io, cancelled)?;
        files::validate_lease(&pages.root, &pages._lease)?;
        files::create_directory(&pages.root.join("objects"))?;
        files::publish(
            &pages.root,
            "",
            "objects-format",
            CAPABILITY,
            io,
            cancelled,
            true,
        )?;
        Self::open(pages, MemoryClass::Ordinary, io, cancelled)
    }
    pub fn open(
        pages: &'a IncrementalPageStore,
        class: MemoryClass,
        io: &mut PageIoBudget,
        cancelled: &AtomicBool,
    ) -> Result<Self> {
        let charge = pages.memory.reserve(4096, class)?;
        check_cancelled(cancelled)?;
        files::validate_store_io(&pages.root, io, cancelled)?;
        files::validate_lease(&pages.root, &pages._lease)?;
        files::directory(&pages.root.join("objects"))?;
        let capability = files::read(&pages.root.join("objects-format"), 64, io, cancelled)?;
        if capability != CAPABILITY {
            return Err(PageStoreError::Corrupt("object capability version"));
        }
        Ok(IncrementalObjectStore {
            pages,
            _charge: charge,
        })
    }
}

impl IncrementalObjectStore<'_> {
    /// Store the actual protocol producer's history material, one original at
    /// a time. RootSets, state pages, facts' content, artifacts and original
    /// certificates still have to be supplied/verified by their owners. This
    /// returns a content ID, not a complete-closure or acceptance receipt.
    pub fn persist_history_material(
        &self,
        prepared: &ReservedValue<oasis7_formal_protocol::incremental_history::PreparedHistoryV1>,
        io: &mut ObjectIo<'_>,
    ) -> Result<[u8; 32]> {
        if !prepared.uses_budget(&self.pages.memory) {
            return Err(PageStoreError::Corrupt("history producer memory pool"));
        }
        let _workspace = self
            .pages
            .memory
            .reserve(WORKSPACE, MemoryClass::Ordinary)?;
        let bad = || PageStoreError::Corrupt("history material binding");
        let facts = prepared
            .facts
            .canonical_bytes(&self.pages.memory)
            .map_err(from_execution)?;
        let fact_id = *blake3::hash(&facts).as_bytes();
        if prepared.body.facts != fact_id || prepared.segment.facts != fact_id {
            return Err(bad());
        }
        self.write_object(&mut facts.as_slice(), facts.len() as u64, Some(fact_id), io)?;
        match (&prepared.transition_bytes, prepared.body.transition) {
            (None, None) => {}
            (Some(bytes), Some(expected)) if bytes.uses_budget(&self.pages.memory) => {
                self.write_object(
                    &mut bytes.as_slice(),
                    bytes.len() as u64,
                    Some(expected),
                    io,
                )?;
            }
            _ => return Err(bad()),
        }
        let segment = prepared.segment.canonical_bytes().map_err(|_| bad())?;
        let segment_id =
            self.write_object(&mut segment.as_slice(), segment.len() as u64, None, io)?;
        if prepared.prefix.segment != segment_id {
            return Err(bad());
        }
        let prefix = prepared.prefix.canonical_bytes().map_err(|_| bad())?;
        let prefix_id = self.write_object(&mut prefix.as_slice(), prefix.len() as u64, None, io)?;
        if prepared.body.history != prefix_id {
            return Err(bad());
        }
        match (&prepared.checkpoint, prepared.body.checkpoint) {
            (None, None) => {}
            (Some(checkpoint), Some(expected)) => {
                let bytes = checkpoint.canonical_bytes().map_err(|_| bad())?;
                self.write_object(
                    &mut bytes.as_slice(),
                    bytes.len() as u64,
                    Some(expected),
                    io,
                )?;
            }
            _ => return Err(bad()),
        }
        let bytes = prepared.body.canonical_bytes().map_err(|_| bad())?;
        self.write_object(&mut bytes.as_slice(), bytes.len() as u64, None, io)
    }
    /// Consume exactly length bytes, streaming the original flat digest once.
    /// Supplying expected rejects a mismatched import before locator publish;
    /// provisional pages from an unsuccessful import remain unaccepted.
    pub fn write_object(
        &self,
        input: &mut impl Read,
        length: u64,
        expected: Option<[u8; 32]>,
        io: &mut ObjectIo<'_>,
    ) -> Result<[u8; 32]> {
        let _workspace = self
            .pages
            .memory
            .reserve(WORKSPACE, MemoryClass::Ordinary)?;
        self.validate(io)?;
        check_cancelled(io.cancelled)?;
        let mut input = DigestReader {
            input,
            hash: blake3::Hasher::new(),
        };
        let mut session = BorrowedPageSession {
            store: self.pages,
            io: io.physical,
            cancelled: io.cancelled,
        };
        let root = write_value(
            &mut input,
            length,
            &mut session,
            &self.pages.memory,
            io.logical,
            || io.cancelled.load(Ordering::Acquire),
        )
        .map_err(from_value)?;
        let id = *input.hash.finalize().as_bytes();
        if expected.is_some_and(|expected| expected != id) {
            return Err(PageStoreError::Corrupt("original object import hash"));
        }
        let locator = encode_locator(
            &id,
            &ObjectLocator {
                length,
                value_root: root.digest().map_err(from_value)?,
            },
        );
        check_cancelled(io.cancelled)?;
        files::publish(
            &self.pages.root,
            "objects",
            &hex::encode(id),
            &locator,
            io.physical,
            io.cancelled,
            false,
        )?;
        // Different physical policies may publish concurrently. Re-read the
        // winning locator and verify the full original, never trust CAS hits.
        self.read_object_into(&id, length, &mut std::io::sink(), io)?;
        Ok(id)
    }

    /// Output is provisional until success, including late digest/IO errors.
    /// Authoritative reducers should use read_object_owned or a private staged
    /// file, never apply this stream incrementally before final verification.
    pub fn read_object_into(
        &self,
        id: &[u8; 32],
        max_length: u64,
        output: &mut impl Write,
        io: &mut ObjectIo<'_>,
    ) -> Result<u64> {
        let _workspace = self.pages.memory.reserve(WORKSPACE, io.class)?;
        self.validate(io)?;
        let locator = self.locator(id, max_length, io)?;
        self.read_locator(id, &locator, output, io)?;
        Ok(locator.length)
    }

    /// Admit the complete bounded canonical record before allocation and return
    /// it only after original flat digest verification. Large artifacts use the
    /// streaming API and an admitted provisional sink instead of this method.
    pub fn read_object_owned(
        &self,
        id: &[u8; 32],
        max_length: usize,
        io: &mut ObjectIo<'_>,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>> {
        let _workspace = self.pages.memory.reserve(WORKSPACE, io.class)?;
        self.validate(io)?;
        let locator = self.locator(id, max_length as u64, io)?;
        let length = usize::try_from(locator.length)
            .map_err(|_| PageStoreError::ResourceLimited("object address space"))?;
        let charge = self.pages.memory.reserve(
            length
                .checked_add(128)
                .ok_or(PageStoreError::ResourceLimited(
                    "object allocation overflow",
                ))?,
            io.class,
        )?;
        let mut output = Vec::with_capacity(length);
        self.read_locator(id, &locator, &mut output, io)?;
        if output.len() != length || output.capacity() > charge.bytes() {
            return Err(PageStoreError::Corrupt("object output allocation"));
        }
        Ok(Arc::new(charge.retain(output)))
    }
    fn validate(&self, io: &mut ObjectIo<'_>) -> Result<()> {
        files::validate_store_io(&self.pages.root, io.physical, io.cancelled)?;
        files::validate_lease(&self.pages.root, &self.pages._lease)?;
        files::directory(&self.pages.root.join("objects"))?;
        if files::read(
            &self.pages.root.join("objects-format"),
            64,
            io.physical,
            io.cancelled,
        )? != CAPABILITY
        {
            return Err(PageStoreError::Corrupt("object capability version"));
        }
        Ok(())
    }
    fn locator(
        &self,
        id: &[u8; 32],
        max_length: u64,
        io: &mut ObjectIo<'_>,
    ) -> Result<ObjectLocator> {
        let bytes = files::read(
            &self.pages.root.join("objects").join(hex::encode(id)),
            LOCATOR_BOUND,
            io.physical,
            io.cancelled,
        )
        .map_err(|error| match error {
            PageStoreError::Io(error) if error.kind() == io::ErrorKind::NotFound => {
                PageStoreError::Missing(*id)
            }
            error => error,
        })?;
        let locator = decode_locator(id, &bytes)?;
        if locator.length > max_length {
            return Err(PageStoreError::ResourceLimited("original object length"));
        }
        Ok(locator)
    }
    fn read_locator(
        &self,
        id: &[u8; 32],
        locator: &ObjectLocator,
        output: &mut impl Write,
        io: &mut ObjectIo<'_>,
    ) -> Result<()> {
        let session = ReadPageSession {
            store: self.pages,
            io: Mutex::new(&mut *io.physical),
            cancelled: io.cancelled,
        };
        let mut context = ValueReadContext {
            source: &session,
            memory: &self.pages.memory,
            class: io.class,
            io: io.logical,
        };
        let root = context.load_root(&locator.value_root).map_err(from_value)?;
        if root.0.logical_len != locator.length {
            return Err(PageStoreError::Corrupt("object root length"));
        }
        let mut writer = DigestWriter {
            output,
            hash: blake3::Hasher::new(),
        };
        context
            .read_range(&root, 0, locator.length, &mut writer, || {
                io.cancelled.load(Ordering::Acquire)
            })
            .map_err(from_value)?;
        check_cancelled(io.cancelled)?;
        if writer.hash.finalize().as_bytes() != id {
            return Err(PageStoreError::Corrupt(
                "original object reconstruction hash",
            ));
        }
        Ok(())
    }
}

/// Both logical traversal and physical representation work consume explicit
/// operation allowances. Memory always comes from the pool's original budget.
pub struct ObjectIo<'a> {
    pub physical: &'a mut PageIoBudget,
    pub logical: &'a mut ValueIoBudget,
    pub class: MemoryClass,
    pub cancelled: &'a AtomicBool,
}

struct DigestReader<'a, R> {
    input: &'a mut R,
    hash: blake3::Hasher,
}
impl<R: Read> Read for DigestReader<'_, R> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        let count = self.input.read(out)?;
        self.hash.update(&out[..count]);
        Ok(count)
    }
}
struct DigestWriter<'a, W> {
    output: &'a mut W,
    hash: blake3::Hasher,
}
impl<W: Write> Write for DigestWriter<'_, W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let count = self.output.write(bytes)?;
        self.hash.update(&bytes[..count]);
        Ok(count)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.output.flush()
    }
}
struct BorrowedPageSession<'a> {
    store: &'a IncrementalPageStore,
    io: &'a mut PageIoBudget,
    cancelled: &'a AtomicBool,
}
impl ValueNodeWriter for BorrowedPageSession<'_> {
    fn put_node(
        &mut self,
        id: [u8; 32],
        bytes: ReservedValue<Vec<u8>>,
    ) -> std::result::Result<(), ValueTreeError> {
        self.store
            .put_page(id, &bytes, self.io, self.cancelled)
            .map_err(value_error)
    }
}
struct ReadPageSession<'a> {
    store: &'a IncrementalPageStore,
    io: Mutex<&'a mut PageIoBudget>,
    cancelled: &'a AtomicBool,
}
impl ValueNodeReader for ReadPageSession<'_> {
    fn read_node(
        &self,
        id: &[u8; 32],
        max_bytes: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> std::result::Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        if !memory.same_pool(&self.store.memory) {
            return Err(ValueTreeError::Invalid("object reader pool"));
        }
        let mut io = self
            .io
            .lock()
            .map_err(|_| ValueTreeError::Invalid("object reader poisoned"))?;
        self.store
            .read_page(id, max_bytes, class, &mut io, self.cancelled)
            .map_err(value_error)
    }
}
fn from_value(error: ValueTreeError) -> PageStoreError {
    match error {
        ValueTreeError::Invalid(reason) => PageStoreError::Corrupt(reason),
        ValueTreeError::ResourceLimited(reason) => PageStoreError::ResourceLimited(reason),
        ValueTreeError::Memory(error) => PageStoreError::Memory(error),
        ValueTreeError::Io(error) => PageStoreError::Io(error),
        ValueTreeError::Cancelled => PageStoreError::Cancelled,
    }
}
fn from_execution(
    error: oasis7_formal_protocol::incremental_execution::ExecutionError,
) -> PageStoreError {
    use oasis7_formal_protocol::incremental_execution::ExecutionError;
    match error {
        ExecutionError::Memory(error) => PageStoreError::Memory(error),
        ExecutionError::ResourceLimited(reason) => PageStoreError::ResourceLimited(reason),
        _ => PageStoreError::Corrupt("invalid history material"),
    }
}
fn encode_locator(id: &[u8; 32], locator: &ObjectLocator) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(LOCATOR_BOUND);
    bytes.extend_from_slice(LOCATOR);
    bytes.extend_from_slice(id);
    bytes.extend_from_slice(&locator.length.to_be_bytes());
    bytes.extend_from_slice(&locator.value_root);
    let checksum = blake3::hash(&bytes);
    bytes.extend_from_slice(checksum.as_bytes());
    bytes
}
fn decode_locator(id: &[u8; 32], bytes: &[u8]) -> Result<ObjectLocator> {
    let bad = || PageStoreError::Corrupt("object locator domain/length/checksum");
    if bytes.len() != LOCATOR.len() + 104 || !bytes.starts_with(LOCATOR) {
        return Err(bad());
    }
    let body = &bytes[LOCATOR.len()..];
    if &body[..32] != id || blake3::hash(&bytes[..bytes.len() - 32]).as_bytes() != &body[72..] {
        return Err(bad());
    }
    Ok(ObjectLocator {
        length: u64::from_be_bytes(body[32..40].try_into().map_err(|_| bad())?),
        value_root: body[40..72].try_into().map_err(|_| bad())?,
    })
}

#[cfg(test)]
#[path = "incremental_object_store_tests.rs"]
mod tests;

/// Keeps the source store, operation quotas and original charged carriers bound.
pub struct ObjectReadSession<'s, 'pool, 'io, 'quota> {
    store: &'s IncrementalObjectStore<'pool>,
    io: &'io mut ObjectIo<'quota>,
}
impl oasis7_formal_protocol::incremental_material::OriginalObjectReader
    for ObjectReadSession<'_, '_, '_, '_>
{
    type Error = PageStoreError;
    fn allocation_budget(&self) -> &MemoryBudget {
        &self.store.pages.memory
    }
    fn read_original(
        &mut self,
        id: &[u8; 32],
        max_bytes: usize,
        memory: &MemoryBudget,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>> {
        if !memory.same_pool(&self.store.pages.memory) || self.io.class != MemoryClass::Ordinary {
            return Err(PageStoreError::Corrupt("live original reader pool/class"));
        }
        self.store.read_object_owned(id, max_bytes, self.io)
    }
}
