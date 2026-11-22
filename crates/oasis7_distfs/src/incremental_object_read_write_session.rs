//! Explicit provisional original IO. Read-only sessions never acquire writes.
use super::*;
use oasis7_formal_protocol::incremental_material::{
    OriginalObjectReader, OriginalObjectStreamReader, OriginalObjectStreamWriter,
};

pub struct ObjectReadWriteSession<'s, 'pool, 'io, 'quota> {
    reader: ObjectReadSession<'s, 'pool, 'io, 'quota>,
    _layout: oasis7_proto::memory_budget::ReservedValue<()>,
}
impl<'pool> IncrementalObjectStore<'pool> {
    /// Construct only from the actual opened store and its original IO loan.
    /// This does not turn an existing read-only handle into a writable handle.
    pub fn provisional_read_write_session<'s, 'io, 'quota>(
        &'s self,
        io: &'io mut ObjectIo<'quota>,
    ) -> Result<ObjectReadWriteSession<'s, 'pool, 'io, 'quota>> {
        self.validate(io)?;
        if io.class != MemoryClass::Ordinary {
            return Err(PageStoreError::Corrupt("original writer allocation class"));
        }
        reclaim::check_publication(&self.pages.root)?;
        let layout = self.pages.memory.reserve(
            std::mem::size_of::<ObjectReadWriteSession<'_, '_, '_, '_>>(),
            MemoryClass::Ordinary,
        )?;
        Ok(ObjectReadWriteSession {
            reader: ObjectReadSession { store: self, io },
            _layout: layout.retain(()),
        })
    }
}
impl OriginalObjectReader for ObjectReadWriteSession<'_, '_, '_, '_> {
    type Error = PageStoreError;
    fn allocation_class(&self) -> MemoryClass {
        self.reader.allocation_class()
    }
    fn allocation_budget(&self) -> &MemoryBudget {
        self.reader.allocation_budget()
    }
    fn read_original(
        &mut self,
        id: &[u8; 32],
        max_bytes: usize,
        memory: &MemoryBudget,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>> {
        self.reader.read_original(id, max_bytes, memory)
    }
}
impl OriginalObjectStreamReader for ObjectReadWriteSession<'_, '_, '_, '_> {
    fn original_writer(
        &mut self,
    ) -> Option<&mut dyn OriginalObjectStreamWriter<Error = Self::Error>> {
        Some(self)
    }
    fn stream_original(
        &mut self,
        id: &[u8; 32],
        max_bytes: u64,
        memory: &MemoryBudget,
        output: &mut dyn Write,
    ) -> Result<u64> {
        self.reader.stream_original(id, max_bytes, memory, output)
    }
}
impl OriginalObjectStreamWriter for ObjectReadWriteSession<'_, '_, '_, '_> {
    type Error = PageStoreError;
    fn write_original(
        &mut self,
        mut input: &mut dyn Read,
        length: u64,
        expected_id: [u8; 32],
        memory: &MemoryBudget,
    ) -> Result<[u8; 32]> {
        if !memory.same_pool(&self.reader.store.pages.memory)
            || self.reader.io.class != MemoryClass::Ordinary
        {
            return Err(PageStoreError::Corrupt("original writer pool/class"));
        }
        // The original store kernel hashes the actual stream, verifies the
        // expected flat digest, then durably publishes its original locator.
        self.reader
            .store
            .write_object(&mut input, length, Some(expected_id), self.reader.io)
    }
}
