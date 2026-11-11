//! Paid immutable Index data staging. This owner supplies no index, retention or execution authority.
use super::super::FormalArchiveReadWorkV1;
use super::*;
use crate::controlled_authority::formal::physical;
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};

pub(in crate::controlled_authority::formal) struct StagedIndexBlobV1 {
    id: String,
    bytes: ReservedValue<Vec<u8>>,
    layout: ReservedValue<()>,
    _guard: ReservedValue<physical::PublicationGuard>,
}
impl std::fmt::Debug for StagedIndexBlobV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StagedIndexBlobV1")
            .field("id", &self.id)
            .finish_non_exhaustive()
    }
}
impl StagedIndexBlobV1 {
    /// Borrow only the exact paid staged object and its still-held real guards.
    /// This supplies data retention input, never Index/Hot/execution authority.
    pub(in crate::controlled_authority::formal) fn retention_source(
        &self,
        memory: &MemoryBudget,
    ) -> Result<(&physical::PublicationGuard, &str), FormalError> {
        if !self.uses_budget(memory) {
            return Err(invalid("retention staged object pool mismatch"));
        }
        Ok((&self._guard, &self.id))
    }
    pub(in crate::controlled_authority::formal::endpoint) fn id(&self) -> &str {
        &self.id
    }
    pub(in crate::controlled_authority::formal::endpoint) fn bytes(&self) -> &[u8] {
        self.bytes.as_slice()
    }
    pub(in crate::controlled_authority::formal::endpoint) fn uses_budget(
        &self,
        memory: &MemoryBudget,
    ) -> bool {
        self.bytes.uses_budget(memory)
            && self.layout.uses_budget(memory)
            && self._guard.uses_budget(memory)
    }
    pub(in crate::controlled_authority::formal::endpoint) fn reserved_bytes(&self) -> usize {
        self.bytes.reserved_bytes() + self.layout.reserved_bytes() + self._guard.reserved_bytes()
    }
}
// Default CBOR: outer map(1), "Leaf"(5), inner map(1),
// "request"(8), 64-byte text(66), "entry"(6), 64-byte text(66).
const INDEX_LEAF_WIRE_MAX: usize = 1 + 5 + 1 + 8 + 66 + 6 + 66;

use super::stage_core::join_paid;
#[cfg(test)]
use super::stage_core::write_payload_bounded;
/// Only a pre-existing initialized physical namespace is admitted. No retention
/// registration occurs; keep the returned guard alive until a later genuine
/// publication/retention protocol finishes, or accept that GC may reclaim it.
pub(super) fn stage_index_leaf_bounded_v1(
    store: &Store,
    request: &str,
    entry: &str,
    memory: &MemoryBudget,
    work: &FormalArchiveReadWorkV1,
    cancelled: &mut dyn FnMut() -> bool,
) -> Result<StagedIndexBlobV1, FormalError> {
    work.consume_steps(1, cancelled)?;
    hash_hex(request)?;
    hash_hex(entry)?;
    let base = store.path.as_os_str().as_encoded_bytes().len();
    let directory_cap = base
        .checked_add(1 + "formal-objects-v1".len())
        .ok_or_else(|| invalid("bounded stage directory layout"))?;
    // Two live child paths: 64-byte digest and a temporary component containing
    // fixed punctuation + u32 pid (10 digits) + u64 monotonic counter (20).
    let temp_name_cap = ".formal-stage-"
        .len()
        .checked_add(10)
        .and_then(|n| n.checked_add(1))
        .and_then(|n| n.checked_add(20))
        .ok_or_else(|| invalid("bounded stage temporary name layout"))?;
    let child_cap = directory_cap
        .checked_add(1 + 64usize.max(temp_name_cap))
        .ok_or_else(|| invalid("bounded stage child layout"))?;
    let layout_bytes = directory_cap
        .checked_add(
            child_cap
                .checked_mul(2)
                .ok_or_else(|| invalid("bounded stage paths layout"))?,
        )
        .and_then(|n| {
            n.checked_add(
                temp_name_cap
                    + 64
                    + 3 * std::mem::size_of::<PathBuf>()
                    + 2 * std::mem::size_of::<String>(),
            )
        })
        .ok_or_else(|| invalid("bounded stage retained layout"))?;
    let layout = memory
        .reserve(layout_bytes, MemoryClass::Ordinary)
        .map_err(|_| invalid("bounded stage path admission"))?
        .retain(());
    let directory = join_paid(&store.path, "formal-objects-v1", directory_cap)?;
    let guard = super::stage_io::guard_initialized(&directory, memory, work, cancelled)?;
    let leaf_fee = memory
        .reserve(128 + std::mem::size_of::<Index>(), MemoryClass::Ordinary)
        .map_err(|_| invalid("bounded stage leaf admission"))?;
    let leaf = Index::Leaf {
        request: request.to_owned(),
        entry: entry.to_owned(),
    };
    if let Index::Leaf { request, entry } = &leaf {
        if request.capacity() > 64 || entry.capacity() > 64 {
            return Err(invalid("bounded stage leaf capacity"));
        }
    }
    // This is an encoder ceiling for this closed two-hash Leaf, never a decoder
    // grammar restriction or a claim about arbitrary Serialize heap graphs.
    let bytes = super::stage_io::encode_wire(&leaf, memory, work, INDEX_LEAF_WIRE_MAX, cancelled)?;
    drop(leaf);
    drop(leaf_fee);
    let id = super::stage_core::stage_wire_with_layout(
        &guard,
        bytes.as_slice(),
        INDEX_LEAF_WIRE_MAX,
        child_cap,
        temp_name_cap,
        memory,
        work,
        cancelled,
    )?;
    Ok(StagedIndexBlobV1 {
        id,
        bytes,
        layout,
        _guard: guard,
    })
}

#[cfg(test)]
#[path = "endpoint_store_bounded_stage_tests.rs"]
mod tests;
