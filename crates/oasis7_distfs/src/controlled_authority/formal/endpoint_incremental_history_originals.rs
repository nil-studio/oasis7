//! Borrow one candidate original or an authenticated persisted original cursor.
//! No whole-History materialization and no provenance/acceptance is minted.
use crate::incremental_page_store::objects::{IncrementalObjectStore, ObjectIo};
use oasis7_formal_protocol::{
    controlled_authority::replicated_protocol::ClosedRecord, incremental_value::ValueTreeError,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass};
use std::io::Cursor;
#[allow(clippy::too_many_arguments)]
pub(super) fn with_original<T>(
    candidate: &ClosedRecord,
    retained: &IncrementalObjectStore<'_>,
    io: &mut ObjectIo<'_>,
    memory: &MemoryBudget,
    occurrences: &mut u64,
    hash_bytes: &mut u64,
    id: &[u8; 32],
    length: u64,
    consume: impl FnOnce(&mut dyn std::io::Read) -> Result<T, ValueTreeError>,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<T, ValueTreeError> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    *occurrences = occurrences
        .checked_sub(1)
        .ok_or(ValueTreeError::ResourceLimited(
            "endpoint History original occurrences",
        ))?;
    let mut encoded = [0u8; 64];
    hex::encode_to_slice(id, &mut encoded)
        .map_err(|_| ValueTreeError::Invalid("History original ID"))?;
    if let Some(object) = candidate
        .objects
        .iter()
        .find(|o| o.content_hash.as_bytes() == encoded)
    {
        let actual = u64::try_from(object.bytes.len())
            .map_err(|_| ValueTreeError::Invalid("History original length"))?;
        if actual != length {
            return Err(ValueTreeError::Invalid("History original exact length"));
        }
        *hash_bytes = hash_bytes
            .checked_sub(actual)
            .ok_or(ValueTreeError::ResourceLimited(
                "endpoint History raw hash bytes",
            ))?;
        let _phase = memory.reserve(4096, MemoryClass::Ordinary)?;
        let mut hash = blake3::Hasher::new();
        for bytes in object.bytes.chunks(4096) {
            if cancelled() {
                return Err(ValueTreeError::Cancelled);
            }
            hash.update(bytes);
        }
        if hash.finalize().as_bytes() != id {
            return Err(ValueTreeError::Invalid(
                "History candidate original checksum",
            ));
        }
        let mut cursor = Cursor::new(object.bytes.as_slice());
        let result = consume(&mut cursor)?;
        if cursor.position() != length {
            return Err(ValueTreeError::Invalid(
                "History candidate original partially consumed",
            ));
        }
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
        Ok(result)
    } else {
        let mut cursor = retained
            .verified_original_cursor(id, length, io)
            .map_err(|e| ValueTreeError::Io(std::io::Error::other(e)))?;
        if cursor.length() != length {
            return Err(ValueTreeError::Invalid(
                "History retained original exact length",
            ));
        }
        let result = consume(&mut cursor);
        if let Some(error) = cursor.take_error() {
            return Err(ValueTreeError::Io(std::io::Error::other(error)));
        }
        let result = result?;
        cursor
            .finish()
            .map_err(|e| ValueTreeError::Io(std::io::Error::other(e)))?;
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
        Ok(result)
    }
}

pub(super) struct HistoryOriginalReader<'a, 'pool, 'quota, C> {
    pub(super) candidate: &'a ClosedRecord,
    pub(super) retained: &'a IncrementalObjectStore<'pool>,
    pub(super) io: &'a mut ObjectIo<'quota>,
    pub(super) memory: &'a MemoryBudget,
    pub(super) occurrences: &'a mut u64,
    pub(super) hash_bytes: &'a mut u64,
    pub(super) cancelled: &'a mut C,
}
impl<C: FnMut() -> bool>
    oasis7_world_state::incremental_cognition_history::CognitionHistoryOriginalReadV3
    for HistoryOriginalReader<'_, '_, '_, C>
{
    fn with_original<T>(
        &mut self,
        id: &[u8; 32],
        bytes: u64,
        consume: impl FnOnce(&mut dyn std::io::Read) -> Result<T, ValueTreeError>,
    ) -> Result<T, ValueTreeError> {
        with_original(
            self.candidate,
            self.retained,
            self.io,
            self.memory,
            self.occurrences,
            self.hash_bytes,
            id,
            bytes,
            consume,
            self.cancelled,
        )
    }
}
