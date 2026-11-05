//! Durability completion for retained original dependencies actually consumed.
//! Candidate/Frozen bytes remain provisional until persist_checked_originals;
//! the caller must keep that final mandatory publication step before receipts.
use super::*;
use crate::incremental_page_store::{
    PageStoreError,
    objects::{IncrementalObjectStore, ObjectIo},
};
use oasis7_formal_protocol::incremental_material::{
    OriginalObjectReader, OriginalObjectStreamReader,
};
use std::{io::Write, sync::Arc};
#[derive(Debug)]
pub(crate) enum DurableOriginalErrorV1<E> {
    Original(E),
    Store(PageStoreError),
    Limit(&'static str),
}
pub(crate) struct DurableOriginalWorkV1 {
    pub(crate) occurrences_remaining: u64,
    pub(crate) bytes_remaining: u64,
}
pub(crate) struct DurableOriginalsV1<'a, 'pool, 'quota, O> {
    pub(crate) original: &'a mut O,
    pub(crate) candidate: &'a FormalCandidateMaterialV1,
    pub(crate) store: &'a IncrementalObjectStore<'pool>,
    pub(crate) io: &'a mut ObjectIo<'quota>,
    pub(crate) work: &'a mut DurableOriginalWorkV1,
}
impl<O: OriginalObjectReader> DurableOriginalsV1<'_, '_, '_, O> {
    fn supplied(&self, id: &[u8; 32]) -> bool {
        let mut encoded = [0; 64];
        if hex::encode_to_slice(id, &mut encoded).is_err() {
            return false;
        }
        self.candidate
            .material
            .objects
            .iter()
            .chain(
                self.candidate
                    .frozen_material
                    .iter()
                    .flat_map(|m| m.objects.iter()),
            )
            .any(|o| o.content_hash.as_bytes() == encoded)
    }
    fn synchronize(
        &mut self,
        id: &[u8; 32],
        bytes: u64,
    ) -> Result<(), DurableOriginalErrorV1<O::Error>> {
        if self.supplied(id) {
            return Ok(());
        }
        self.work.occurrences_remaining =
            self.work
                .occurrences_remaining
                .checked_sub(1)
                .ok_or(DurableOriginalErrorV1::Limit(
                    "retained original occurrences",
                ))?;
        self.work.bytes_remaining = self
            .work
            .bytes_remaining
            .checked_sub(bytes)
            .ok_or(DurableOriginalErrorV1::Limit("retained original bytes"))?;
        self.store
            .sync_original_durable(id, bytes, self.io)
            .map_err(DurableOriginalErrorV1::Store)
    }
}
impl<O: OriginalObjectReader> OriginalObjectReader for DurableOriginalsV1<'_, '_, '_, O> {
    type Error = DurableOriginalErrorV1<O::Error>;
    fn allocation_budget(&self) -> &MemoryBudget {
        self.original.allocation_budget()
    }
    fn allocation_class(&self) -> MemoryClass {
        MemoryClass::Ordinary
    }
    fn read_original(
        &mut self,
        id: &[u8; 32],
        max_bytes: usize,
        memory: &MemoryBudget,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, Self::Error> {
        if !self.store.uses_budget(memory)
            || !self.original.allocation_budget().same_pool(memory)
            || self.original.allocation_class() != MemoryClass::Ordinary
            || self.io.class != MemoryClass::Ordinary
        {
            return Err(DurableOriginalErrorV1::Limit("durable original pool/class"));
        }
        let bytes = self
            .original
            .read_original(id, max_bytes, memory)
            .map_err(DurableOriginalErrorV1::Original)?;
        if !bytes.uses_budget(memory)
            || bytes.reserved_bytes() < bytes.len()
            || bytes.len() > max_bytes
        {
            return Err(DurableOriginalErrorV1::Limit(
                "durable original actual carrier",
            ));
        }
        self.synchronize(id, bytes.len() as u64)?;
        Ok(bytes)
    }
}
impl<O: OriginalObjectStreamReader> OriginalObjectStreamReader
    for DurableOriginalsV1<'_, '_, '_, O>
{
    fn stream_original(
        &mut self,
        id: &[u8; 32],
        max_bytes: u64,
        memory: &MemoryBudget,
        output: &mut dyn Write,
    ) -> Result<u64, Self::Error> {
        if !self.store.uses_budget(memory)
            || !self.original.allocation_budget().same_pool(memory)
            || self.original.allocation_class() != MemoryClass::Ordinary
            || self.io.class != MemoryClass::Ordinary
        {
            return Err(DurableOriginalErrorV1::Limit("durable stream pool/class"));
        }
        let length = self
            .original
            .stream_original(id, max_bytes, memory, output)
            .map_err(DurableOriginalErrorV1::Original)?;
        if length > max_bytes {
            return Err(DurableOriginalErrorV1::Limit(
                "durable stream actual length",
            ));
        }
        self.synchronize(id, length)?;
        Ok(length)
    }
}

/// Pull-reader counterpart. A decoder result is not released until the actual
/// retained winning locator has completed its durability barrier. Supplied
/// candidate/Frozen bytes still require the final candidate publication pass.
pub(crate) struct DurableCognitionOriginalsV1<'a, 'pool, 'quota, O> {
    pub(crate) original: &'a mut O,
    pub(crate) candidate: &'a FormalCandidateMaterialV1,
    pub(crate) store: &'a IncrementalObjectStore<'pool>,
    pub(crate) memory: &'a MemoryBudget,
    pub(crate) io: &'a mut ObjectIo<'quota>,
    pub(crate) work: &'a mut DurableOriginalWorkV1,
}
impl<O: oasis7_world_state::incremental_cognition_history::CognitionHistoryOriginalReadV3>
    oasis7_world_state::incremental_cognition_history::CognitionHistoryOriginalReadV3
    for DurableCognitionOriginalsV1<'_, '_, '_, O>
{
    fn with_original<T>(
        &mut self,
        id: &[u8; 32],
        bytes: u64,
        consume: impl FnOnce(
            &mut dyn std::io::Read,
        )
            -> Result<T, oasis7_formal_protocol::incremental_value::ValueTreeError>,
    ) -> Result<T, oasis7_formal_protocol::incremental_value::ValueTreeError> {
        use oasis7_formal_protocol::incremental_value::ValueTreeError;
        if !self.store.uses_budget(self.memory) || self.io.class != MemoryClass::Ordinary {
            return Err(ValueTreeError::Invalid(
                "durable cognition original pool/class",
            ));
        }
        let result = self.original.with_original(id, bytes, consume)?;
        let mut encoded = [0; 64];
        hex::encode_to_slice(id, &mut encoded)
            .map_err(|_| ValueTreeError::Invalid("durable cognition original ID"))?;
        let supplied = self
            .candidate
            .material
            .objects
            .iter()
            .chain(
                self.candidate
                    .frozen_material
                    .iter()
                    .flat_map(|m| m.objects.iter()),
            )
            .any(|o| o.content_hash.as_bytes() == encoded);
        if !supplied {
            self.work.occurrences_remaining =
                self.work.occurrences_remaining.checked_sub(1).ok_or(
                    ValueTreeError::ResourceLimited("durable cognition original occurrences"),
                )?;
            self.work.bytes_remaining = self.work.bytes_remaining.checked_sub(bytes).ok_or(
                ValueTreeError::ResourceLimited("durable cognition original bytes"),
            )?;
            self.store
                .sync_original_durable(id, bytes, self.io)
                .map_err(|e| ValueTreeError::Io(std::io::Error::other(e)))?;
        }
        Ok(result)
    }
}
