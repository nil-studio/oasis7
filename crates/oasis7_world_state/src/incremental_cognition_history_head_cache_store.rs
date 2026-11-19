//! Physical value-tree carriers for the bounded original-head frontier.
//! A decoded carrier is not a selected-head, lifecycle or History grant.
use crate::incremental_cognition_history_hash_outboard::*;

use oasis7_formal_protocol::incremental_value::{
    ValueIoBudget, ValueNodeReader, ValueNodeWriter, ValueReadContext, ValueTreeError, write_value,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
pub struct PersistedCognitionHeadCacheV1 {
    id: [u8; 32],
    state: ReservedValue<CognitionHeadHashStateV1>,
}
impl PersistedCognitionHeadCacheV1 {
    pub fn id(&self) -> [u8; 32] {
        self.id
    }
    pub fn state(&self) -> &ReservedValue<CognitionHeadHashStateV1> {
        &self.state
    }
}
pub fn persist_cognition_head_cache_v1(
    cache: &ReservedValue<CognitionHeadHashStateV1>,
    sink: &mut impl ValueNodeWriter,
    memory: &MemoryBudget,
    io: &mut ValueIoBudget,
    work: &mut CognitionHeadHashWorkV1,
    mut cancelled: impl FnMut() -> bool,
) -> Result<[u8; 32], ValueTreeError> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    if !cache.uses_budget(memory)
        || cache.allocation_class() != MemoryClass::Ordinary
        || cache.reserved_bytes() < CognitionHeadHashBuilderV1::admission_bytes()
    {
        return Err(ValueTreeError::Invalid("head cache write pool/class"));
    }
    let _charge = memory.reserve(COGNITION_HEAD_CACHE_BYTES_V1 + 128, MemoryClass::Ordinary)?;
    let mut bytes = Vec::with_capacity(COGNITION_HEAD_CACHE_BYTES_V1);
    cache.write_canonical_cache_v1(&mut bytes, work, &mut cancelled)?;
    let root = write_value(
        &mut bytes.as_slice(),
        bytes.len() as u64,
        sink,
        memory,
        io,
        &mut cancelled,
    )?;
    let id = root.digest()?;
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(id)
}
pub fn recover_cognition_head_cache_v1(
    id: [u8; 32],
    read: &mut ValueReadContext<'_, impl ValueNodeReader>,
    work: &mut CognitionHeadHashWorkV1,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<PersistedCognitionHeadCacheV1>, ValueTreeError> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    let root = read.load_root(&id)?;
    if root.0.logical_len != COGNITION_HEAD_CACHE_BYTES_V1 as u64 {
        return Err(ValueTreeError::Invalid("head cache value length"));
    }
    let charge = read.memory.reserve(1024, read.class)?;
    let _scratch = read
        .memory
        .reserve(COGNITION_HEAD_CACHE_BYTES_V1 + 128, read.class)?;
    let mut bytes = Vec::with_capacity(COGNITION_HEAD_CACHE_BYTES_V1);
    read.read_range(&root, 0, root.0.logical_len, &mut bytes, &mut cancelled)?;
    let state = read_cognition_head_cache_v1(
        &mut bytes.as_slice(),
        read.memory,
        read.class,
        work,
        &mut cancelled,
    )?;
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(charge.retain(PersistedCognitionHeadCacheV1 { id, state }))
}

/// Owns the three physical carriers after exact v2 references, current roots
/// and original flat-head digests have been checked. No lifecycle capability.
pub struct BoundPersistedCognitionHeadCachesV2 {
    class: MemoryClass,
    caches: [Option<ReservedValue<PersistedCognitionHeadCacheV1>>; 3],
    head: [u8; 32],
    roots: [u8; 32],
}
impl BoundPersistedCognitionHeadCachesV2 {
    pub(crate) fn class(&self) -> MemoryClass {
        self.class
    }
    pub fn cache(&self, index: usize) -> Option<&ReservedValue<PersistedCognitionHeadCacheV1>> {
        self.caches.get(index).and_then(Option::as_ref)
    }
    pub fn head_record_id(&self) -> [u8; 32] {
        self.head
    }
    pub fn roots_id(&self) -> [u8; 32] {
        self.roots
    }
}
pub fn bind_persisted_cognition_head_caches_v2(
    caches: [Option<ReservedValue<PersistedCognitionHeadCacheV1>>; 3],
    selected: &ReservedValue<super::AcceptedCognitionHistoryViewV1>,
    current: &ReservedValue<super::CognitionHistoryCurrentBindingV1>,
    memory: &MemoryBudget,
    work: &mut CognitionHeadHashWorkV1,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<BoundPersistedCognitionHeadCachesV2>, ValueTreeError> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    let class = selected.class();
    if !selected.uses_budget(memory)
        || selected.reserved_bytes() < 16 << 10
        || !current.uses_budget(memory)
        || current.reserved_bytes() < 1024
        || current.class() != class
        || current.roots_id() != selected.manifest().roots
        || current.head_record_id() != selected.occurrence().head_record_id()
    {
        return Err(ValueTreeError::Invalid(
            "persisted head caches current binding",
        ));
    }
    let refs = selected
        .derived()
        .ok_or(ValueTreeError::Invalid("persisted head caches require v2"))?;
    refs.validate(selected.manifest())?;
    let charge = memory.reserve(2048, class)?;
    for (i, kind) in CognitionHeadHashV1::ALL.into_iter().enumerate() {
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
        match (&caches[i], refs.caches[i]) {
            (None, None) => {}
            (Some(cache), Some(id)) => {
                if !cache.uses_budget(memory)
                    || cache.reserved_bytes() < 1024
                    || cache.id() != id
                    || !cache.state.uses_budget(memory)
                    || cache.state.allocation_class() != class
                    || cache.state.reserved_bytes() < CognitionHeadHashBuilderV1::admission_bytes()
                {
                    return Err(ValueTreeError::Invalid("persisted head cache source"));
                }
                let count = selected.manifest().indexes[[0, 2, 3][i]].records;
                let head = &selected.manifest().heads[i];
                match head.digest {
                    Some(digest)
                        if cache.state.digest(
                            kind,
                            count,
                            head.sequence,
                            work,
                            &mut cancelled,
                        )? == digest => {}
                    None if count == 0 && head.sequence == 0 && cache.state.body_bytes() == 0 => {}
                    _ => return Err(ValueTreeError::Invalid("persisted head cache digest")),
                }
            }
            _ => return Err(ValueTreeError::Invalid("persisted head cache cohort")),
        }
    }
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(charge.retain(BoundPersistedCognitionHeadCachesV2 {
        class,
        caches,
        head: current.head_record_id(),
        roots: current.roots_id(),
    }))
}

/// Exact-ID, zero-IO resource readmission. All returned carriers are newly
/// charged Ordinary allocations; no Recovery reservation is retained.
pub fn readmit_persisted_cognition_head_cache_ordinary_v1(
    source: &ReservedValue<PersistedCognitionHeadCacheV1>,
    expected: [u8; 32],
    memory: &MemoryBudget,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<PersistedCognitionHeadCacheV1>, ValueTreeError> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    if !source.uses_budget(memory)
        || source.reserved_bytes() < 1024
        || source.id != expected
        || source.state.allocation_class() != MemoryClass::Recovery
    {
        return Err(ValueTreeError::Invalid(
            "persisted cache readmission binding",
        ));
    }
    let charge = memory.reserve(1024, MemoryClass::Ordinary)?;
    let state = readmit_cognition_head_cache_ordinary_v1(&source.state, memory, &mut cancelled)?;
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(charge.retain(PersistedCognitionHeadCacheV1 {
        id: expected,
        state,
    }))
}
