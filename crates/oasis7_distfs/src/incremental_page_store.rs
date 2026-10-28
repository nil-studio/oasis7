//! Bounded, durable physical representations of IWS pages. A successful put is
//! provisional storage, never world acceptance or a protected-root publication.
//! There is deliberately no deletion API until typed owner roots are installed.
use oasis7_cas::frame_codec::{self, FrameDescriptor};
use oasis7_formal_protocol::incremental_value::{
    VALUE_MAX_NODE_BYTES, ValueNodeReader, ValueNodeWriter, ValueTreeError,
};
use oasis7_proto::memory_budget::{
    MemoryBudget, MemoryClass, MemoryLimitExceeded, MemoryReservation, ReservedValue,
};
use std::{
    fs::{self, File},
    io,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

#[path = "incremental_page_store_files.rs"]
mod files;
#[path = "incremental_object_store.rs"]
pub mod objects;
#[path = "incremental_page_store_recipe.rs"]
mod recipe;
#[path = "incremental_state_store.rs"]
pub mod state;

const FORMAT: &[u8] = b"oasis7.iws.physical-pages.v1\n";
const RAW_WORKSPACE: usize = 512 * 1024;
// Conservative codec admission includes the native zstd context and window,
// encoded/read buffers and Rust metadata. RSS remains separately measurable.
const ENCODE_WORKSPACE: usize = 16 * 1024 * 1024;
const DECODE_WORKSPACE: usize = 2 * 1024 * 1024;

#[derive(Debug)]
pub enum PageStoreError {
    Missing([u8; 32]),
    Corrupt(&'static str),
    ResourceLimited(&'static str),
    Memory(MemoryLimitExceeded),
    Io(io::Error),
    Cancelled,
}
impl std::fmt::Display for PageStoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for PageStoreError {}
impl From<io::Error> for PageStoreError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}
impl From<MemoryLimitExceeded> for PageStoreError {
    fn from(e: MemoryLimitExceeded) -> Self {
        Self::Memory(e)
    }
}
type Result<T> = std::result::Result<T, PageStoreError>;

#[derive(Debug, Clone, Copy)]
pub enum PageCompression {
    Raw,
    Zstd,
}

/// Physical allowances are independent of logical ValueIoBudget. Failed work
/// remains spent. Counters include recipes, frames and duplicate readback.
#[derive(Debug, Clone)]
pub struct PageIoBudget {
    pub reads_remaining: u64,
    pub read_bytes_remaining: u64,
    pub writes_remaining: u64,
    pub write_bytes_remaining: u64,
}
impl PageIoBudget {
    fn read(&mut self, bytes: usize) -> Result<()> {
        if self.reads_remaining == 0 || self.read_bytes_remaining < bytes as u64 {
            return Err(PageStoreError::ResourceLimited("physical read allowance"));
        }
        self.reads_remaining -= 1;
        self.read_bytes_remaining -= bytes as u64;
        Ok(())
    }
    fn write(&mut self, bytes: usize) -> Result<()> {
        if self.writes_remaining == 0 || self.write_bytes_remaining < bytes as u64 {
            return Err(PageStoreError::ResourceLimited("physical write allowance"));
        }
        self.writes_remaining -= 1;
        self.write_bytes_remaining -= bytes as u64;
        Ok(())
    }
}

pub struct IncrementalPageStore {
    root: PathBuf,
    memory: MemoryBudget,
    compression: PageCompression,
    // A permanent shared kernel lease prevents later cooperative maintenance
    // from retiring representations under readers/writers. No TTL/PID lease.
    _lease: File,
    _charge: MemoryReservation,
}
impl IncrementalPageStore {
    /// Only create a fresh dedicated directory; never reinterpret an existing
    /// world or complete a partially initialized directory implicitly.
    pub fn create(root: &Path, memory: MemoryBudget, compression: PageCompression) -> Result<Self> {
        let charge = admit_path(root, &memory, MemoryClass::Ordinary)?;
        files::create_store(root)?;
        Self::finish_open(root, memory, compression, charge)
    }
    /// Missing/unknown format or fixed guard fails without creating any files.
    pub fn open(
        root: &Path,
        memory: MemoryBudget,
        compression: PageCompression,
        class: MemoryClass,
    ) -> Result<Self> {
        let charge = admit_path(root, &memory, class)?;
        Self::finish_open(root, memory, compression, charge)
    }
    fn finish_open(
        root: &Path,
        memory: MemoryBudget,
        compression: PageCompression,
        charge: MemoryReservation,
    ) -> Result<Self> {
        let lease = files::open_store(root)?;
        Ok(Self {
            root: root.to_owned(),
            memory,
            compression,
            _lease: lease,
            _charge: charge,
        })
    }
    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn put_page(
        &self,
        id: [u8; 32],
        bytes: &ReservedValue<Vec<u8>>,
        io: &mut PageIoBudget,
        cancelled: &AtomicBool,
    ) -> Result<()> {
        self.put_page_class(id, bytes, MemoryClass::Ordinary, io, cancelled)
    }
    fn put_page_class(
        &self,
        id: [u8; 32],
        bytes: &ReservedValue<Vec<u8>>,
        class: MemoryClass,
        io: &mut PageIoBudget,
        cancelled: &AtomicBool,
    ) -> Result<()> {
        check_cancelled(cancelled)?;
        if !bytes.uses_budget(&self.memory) || bytes.capacity() > bytes.reserved_bytes() {
            return Err(PageStoreError::Corrupt("unaccounted/cross-pool input"));
        }
        if bytes.is_empty()
            || bytes.len() > VALUE_MAX_NODE_BYTES
            || blake3::hash(bytes).as_bytes() != &id
        {
            return Err(PageStoreError::Corrupt("page length/hash"));
        }
        let _workspace = self.memory.reserve(
            match self.compression {
                PageCompression::Raw => RAW_WORKSPACE,
                PageCompression::Zstd => ENCODE_WORKSPACE,
            },
            class,
        )?;
        files::validate_store_io(&self.root, io, cancelled)?;
        files::validate_lease(&self.root, &self._lease)?;
        if files::exists(&self.root.join("recipes").join(hex::encode(id)))? {
            return self.check_existing(&id, bytes, class, io, cancelled);
        }
        let mut frames = Vec::with_capacity(recipe::MAX_FRAMES);
        for chunk in frame_codec::chunks(bytes) {
            check_cancelled(cancelled)?;
            let (descriptor, stored) =
                frame_codec::encode(chunk, matches!(self.compression, PageCompression::Zstd))?;
            files::publish(
                &self.root,
                "frames",
                &hex::encode(descriptor.stored),
                &stored,
                io,
                cancelled,
                true,
            )?;
            frames.push(descriptor);
        }
        let encoded = recipe::encode(&id, bytes.len(), &frames)?;
        files::publish(
            &self.root,
            "recipes",
            &hex::encode(id),
            &encoded,
            io,
            cancelled,
            false,
        )?;
        self.check_existing(&id, bytes, class, io, cancelled)
    }
    fn check_existing(
        &self,
        id: &[u8; 32],
        bytes: &[u8],
        class: MemoryClass,
        io: &mut PageIoBudget,
        cancelled: &AtomicBool,
    ) -> Result<()> {
        let stored = self.read_page(id, VALUE_MAX_NODE_BYTES, class, io, cancelled)?;
        if stored.as_slice() != bytes {
            return Err(PageStoreError::Corrupt("immutable page differs"));
        }
        // An earlier publisher may have died after link() but before its
        // directory/file barriers. Readable bytes alone do not reestablish
        // durability. Sync the actually selected representation's closure,
        // including the winner of a concurrent raw/zstd recipe publication.
        let recipe_path = self.root.join("recipes").join(hex::encode(id));
        let encoded = files::read(&recipe_path, recipe::MAX_RECIPE_BYTES, io, cancelled)?;
        let (_, frames) = recipe::decode(id, &encoded)?;
        for frame in frames {
            check_cancelled(cancelled)?;
            files::sync(&self.root.join("frames").join(hex::encode(frame.stored)))?;
        }
        files::sync(&self.root.join("frames"))?;
        files::sync(&recipe_path)?;
        files::sync(&self.root.join("recipes"))?;
        Ok(())
    }

    pub fn read_page(
        &self,
        id: &[u8; 32],
        max_bytes: usize,
        class: MemoryClass,
        io: &mut PageIoBudget,
        cancelled: &AtomicBool,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>> {
        check_cancelled(cancelled)?;
        let _workspace = self.memory.reserve(DECODE_WORKSPACE, class)?;
        files::validate_store_io(&self.root, io, cancelled)?;
        files::validate_lease(&self.root, &self._lease)?;
        let path = self.root.join("recipes").join(hex::encode(id));
        let encoded = files::read(&path, recipe::MAX_RECIPE_BYTES, io, cancelled).map_err(
            |error| match error {
                PageStoreError::Io(error) if error.kind() == io::ErrorKind::NotFound => {
                    PageStoreError::Missing(*id)
                }
                error => error,
            },
        )?;
        let (length, frames) = recipe::decode(id, &encoded)?;
        if length > max_bytes {
            return Err(PageStoreError::ResourceLimited("requested page bound"));
        }
        let charge = self.memory.reserve(length + 128, class)?;
        let mut output = Vec::with_capacity(length);
        for frame in frames {
            let path = self.root.join("frames").join(hex::encode(frame.stored));
            let stored = files::read(&path, frame_codec::FRAME_BYTES, io, cancelled)?;
            let decoded = frame_codec::decode(&frame, &stored)
                .map_err(|_| PageStoreError::Corrupt("physical frame"))?;
            output.extend_from_slice(&decoded);
        }
        check_cancelled(cancelled)?;
        if output.len() != length
            || output.capacity() > charge.bytes()
            || blake3::hash(&output).as_bytes() != id
        {
            return Err(PageStoreError::Corrupt("reconstructed page"));
        }
        Ok(Arc::new(charge.retain(output)))
    }

    /// One operation owns its physical allowance and cancellation state. It
    /// borrows this store's shared process memory pool, never creates a pool.
    pub fn value_session(&self, io: PageIoBudget) -> ValuePageSession<'_> {
        ValuePageSession {
            store: self,
            io: Mutex::new(io),
            cancelled: AtomicBool::new(false),
        }
    }
}
fn admit_path(root: &Path, memory: &MemoryBudget, class: MemoryClass) -> Result<MemoryReservation> {
    if !root.is_absolute() || root.as_os_str().len() > 4096 {
        return Err(PageStoreError::ResourceLimited(
            "absolute bounded store path",
        ));
    }
    Ok(memory.reserve(root.as_os_str().len() * 4 + 4096, class)?)
}
fn check_cancelled(cancelled: &AtomicBool) -> Result<()> {
    if cancelled.load(Ordering::Acquire) {
        Err(PageStoreError::Cancelled)
    } else {
        Ok(())
    }
}

pub struct ValuePageSession<'a> {
    store: &'a IncrementalPageStore,
    io: Mutex<PageIoBudget>,
    pub cancelled: AtomicBool,
}
impl ValuePageSession<'_> {
    pub fn remaining_io(&self) -> Result<PageIoBudget> {
        self.io
            .lock()
            .map(|io| io.clone())
            .map_err(|_| PageStoreError::Corrupt("physical session poisoned"))
    }
}
impl ValueNodeWriter for ValuePageSession<'_> {
    fn put_node(
        &mut self,
        id: [u8; 32],
        bytes: ReservedValue<Vec<u8>>,
    ) -> std::result::Result<(), ValueTreeError> {
        let io = self
            .io
            .get_mut()
            .map_err(|_| ValueTreeError::Invalid("physical session poisoned"))?;
        self.store
            .put_page(id, &bytes, io, &self.cancelled)
            .map_err(value_error)
    }
}
impl ValueNodeReader for ValuePageSession<'_> {
    fn read_node(
        &self,
        id: &[u8; 32],
        max_bytes: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> std::result::Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        if !self.store.memory.same_pool(memory) {
            return Err(ValueTreeError::Invalid("physical session memory pool"));
        }
        let mut io = self
            .io
            .lock()
            .map_err(|_| ValueTreeError::Invalid("physical session poisoned"))?;
        self.store
            .read_page(id, max_bytes, class, &mut io, &self.cancelled)
            .map_err(value_error)
    }
}
fn value_error(error: PageStoreError) -> ValueTreeError {
    match error {
        PageStoreError::Memory(error) => ValueTreeError::Memory(error),
        PageStoreError::ResourceLimited(reason) => ValueTreeError::ResourceLimited(reason),
        PageStoreError::Cancelled => ValueTreeError::Cancelled,
        PageStoreError::Corrupt(reason) => ValueTreeError::Invalid(reason),
        PageStoreError::Missing(_) => ValueTreeError::Io(io::Error::new(
            io::ErrorKind::NotFound,
            "physical page missing",
        )),
        PageStoreError::Io(error) => ValueTreeError::Io(error),
    }
}

#[cfg(test)]
#[path = "incremental_page_store_tests.rs"]
mod tests;
