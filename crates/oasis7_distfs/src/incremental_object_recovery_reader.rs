//! Explicit historical read adapter. The live owned-reader's Ordinary-only
//! contract is unchanged; this adapter never provides a write or acceptance.
use super::*;
use oasis7_formal_protocol::incremental_material::{
    OriginalObjectReader, OriginalObjectStreamReader,
};

pub struct RecoveryObjectReadSession<'s, 'pool, 'io, 'quota> {
    store: &'s IncrementalObjectStore<'pool>,
    io: &'io mut ObjectIo<'quota>,
}
impl<'pool> IncrementalObjectStore<'pool> {
    /// Borrow the same continuous IO under an explicit Recovery class. This
    /// only enables bounded original header reads for accepted history checks.
    pub fn recovery_original_reader<'s, 'io, 'quota>(
        &'s self,
        io: &'io mut ObjectIo<'quota>,
    ) -> Result<RecoveryObjectReadSession<'s, 'pool, 'io, 'quota>> {
        if io.class != MemoryClass::Recovery {
            return Err(PageStoreError::Corrupt("recovery original reader class"));
        }
        Ok(RecoveryObjectReadSession { store: self, io })
    }
}
impl OriginalObjectReader for RecoveryObjectReadSession<'_, '_, '_, '_> {
    type Error = PageStoreError;
    fn allocation_class(&self) -> MemoryClass {
        MemoryClass::Recovery
    }
    fn allocation_budget(&self) -> &MemoryBudget {
        &self.store.pages.memory
    }
    fn read_original(
        &mut self,
        id: &[u8; 32],
        max_bytes: usize,
        memory: &MemoryBudget,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>> {
        if !memory.same_pool(&self.store.pages.memory) || self.io.class != MemoryClass::Recovery {
            return Err(PageStoreError::Corrupt(
                "recovery original reader pool/class",
            ));
        }
        self.store.read_object_owned(id, max_bytes, self.io)
    }
}
impl OriginalObjectStreamReader for RecoveryObjectReadSession<'_, '_, '_, '_> {
    fn stream_original(
        &mut self,
        id: &[u8; 32],
        max_bytes: u64,
        memory: &MemoryBudget,
        mut output: &mut dyn Write,
    ) -> Result<u64> {
        if !memory.same_pool(&self.store.pages.memory) || self.io.class != MemoryClass::Recovery {
            return Err(PageStoreError::Corrupt(
                "recovery original stream pool/class",
            ));
        }
        self.store
            .read_object_into(id, max_bytes, &mut output, self.io)
    }
}
