//! Fixed-memory binding of three original flat-hash caches to current roots.
//! This verifies cache identity, not record lifecycle or full History readiness.
use super::*;
use crate::incremental_cognition_history_hash_outboard::*;
use oasis7_formal_protocol::incremental_value::ValueTreeError;
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};

pub struct BoundCognitionHeadCachesV1 {
    states: [ReservedValue<CognitionHeadHashStateV1>; 3],
    roots: [u8; 32],
    head: [u8; 32],
    class: MemoryClass,
}
impl BoundCognitionHeadCachesV1 {
    pub fn state(&self, kind: CognitionHeadHashV1) -> &ReservedValue<CognitionHeadHashStateV1> {
        &self.states[match kind {
            CognitionHeadHashV1::Journal => 0,
            CognitionHeadHashV1::Economy => 1,
            CognitionHeadHashV1::Provision => 2,
        }]
    }
    pub fn roots_id(&self) -> [u8; 32] {
        self.roots
    }
    pub fn head_record_id(&self) -> [u8; 32] {
        self.head
    }
    pub fn allocation_class(&self) -> MemoryClass {
        self.class
    }
}
pub fn bind_cognition_head_caches_v1(
    states: [ReservedValue<CognitionHeadHashStateV1>; 3],
    selected: &ReservedValue<AcceptedCognitionHistoryViewV1>,
    current: &ReservedValue<CognitionHistoryCurrentBindingV1>,
    memory: &MemoryBudget,
    work: &mut CognitionHeadHashWorkV1,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<BoundCognitionHeadCachesV1>, ValueTreeError> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    let class = selected.class();
    if !selected.uses_budget(memory)
        || !current.uses_budget(memory)
        || selected.reserved_bytes() < 16 << 10
        || current.reserved_bytes() < 1024
        || current.class() != class
        || !current.enabled()
        || !selected.manifest().enabled
        || current.roots_id() != selected.manifest().roots
        || current.head_record_id() != selected.occurrence().head_record_id()
    {
        return Err(ValueTreeError::Invalid(
            "Cognition head cache current binding",
        ));
    }
    let charge = memory.reserve(2048, class)?;
    for (i, kind) in CognitionHeadHashV1::ALL.into_iter().enumerate() {
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
        let state = &states[i];
        if !state.uses_budget(memory)
            || state.allocation_class() != class
            || state.reserved_bytes() < CognitionHeadHashBuilderV1::admission_bytes()
        {
            return Err(ValueTreeError::Invalid("Cognition head cache source"));
        }
        let field = CognitionHistoryFieldV1::ALL[[0, 2, 3][i]];
        let count = selected.manifest().index_ref(field).records;
        let head = &selected.manifest().heads[i];
        match head.digest {
            Some(expected)
                if state.digest(kind, count, head.sequence, work, &mut cancelled)? == expected => {}
            None if count == 0 && head.sequence == 0 && state.body_bytes() == 0 => {}
            _ => {
                return Err(ValueTreeError::Invalid(
                    "Cognition head cache original digest",
                ));
            }
        }
    }
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(charge.retain(BoundCognitionHeadCachesV1 {
        states,
        roots: current.roots_id(),
        head: current.head_record_id(),
        class,
    }))
}
