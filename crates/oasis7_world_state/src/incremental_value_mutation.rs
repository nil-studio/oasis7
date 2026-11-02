//! Shared caller-owned ordinary value-write budgets; no schema/authority is inferred.
use oasis7_formal_protocol::incremental_value::ValueIoBudget;
use oasis7_proto::memory_budget::MemoryBudget;
pub struct ValueMutationContextV1<'a, W, C> {
    pub sink: &'a mut W,
    pub memory: &'a MemoryBudget,
    pub io: &'a mut ValueIoBudget,
    pub cancelled: C,
}
