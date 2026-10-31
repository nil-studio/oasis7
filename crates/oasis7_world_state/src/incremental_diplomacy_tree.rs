use super::*;
pub(super) use crate::incremental_record_tree::Source;
use crate::incremental_record_tree::{RecordTreeLabelsV1, emit_nodes};
use oasis7_proto::memory_budget::MemoryBudget;
use std::cell::RefCell;
pub(super) const LABELS: RecordTreeLabelsV1 = RecordTreeLabelsV1 {
    read: "diplomacy state read",
    pending: "diplomacy pending",
    tree: "diplomacy tree",
    digest: "diplomacy digest",
    read_allowance: "diplomacy tree read allowance",
    admission: "diplomacy page allocation",
    bound: "diplomacy page bound",
    producer_pool: "diplomacy producer pool",
    page: "diplomacy page",
    write_allowance: "diplomacy page allowance",
};
pub(super) fn persist<
    W: oasis7_formal_protocol::incremental_state::StatePageWriter,
    C: FnMut() -> bool,
>(
    delta: &ReservedValue<oasis7_formal_protocol::incremental_state::StateDeltaV1>,
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
) -> Result<(), ValueTreeError> {
    emit_nodes(
        delta,
        ctx.sink,
        ctx.memory,
        ctx.io,
        &mut ctx.cancelled,
        LABELS,
    )
}
/// Match the formal transition producer's conservative per-write admission:
/// encoded pending bytes alone omit Vec capacity, digest strings, pending/final
/// maps and tracked parent paths. The separate pending-byte work quota is not a
/// heap bound and remains unchanged at StateDeltaV1::build.
pub(crate) fn workspace(
    memory: &MemoryBudget,
    writes: usize,
) -> Result<oasis7_proto::memory_budget::MemoryReservation, ValueTreeError> {
    let bytes = writes
        .checked_mul(512 << 10)
        .and_then(|n| n.checked_add(128 << 10))
        .ok_or(ValueTreeError::ResourceLimited(
            "diplomacy inner workspace overflow",
        ))?;
    memory
        .reserve(bytes, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)
}

pub(crate) fn lookup<R: ValueNodeReader, C: FnMut() -> bool>(
    root: &StateRootV1,
    key: &WorldFieldKeyV1,
    records: &mut crate::incremental_inventory_value_read::InventoryValueReader<'_, '_, R, C>,
) -> Result<Option<[u8; 32]>, ValueTreeError> {
    let _point = records
        .values
        .memory
        .reserve(8 << 10, records.values.class)?;
    let source = Source {
        source: records.values.source,
        memory: records.values.memory,
        class: records.values.class,
        io: &RefCell::new(&mut *records.values.io),
        cancelled: &RefCell::new(&mut *records.cancelled),
        error: &RefCell::new(None),
        labels: LABELS,
    };
    source.finish(root.get(key.as_bytes(), &source))
}
pub(crate) fn build<R: ValueNodeReader, C: FnMut() -> bool>(
    root: &StateRootV1,
    writes: Vec<oasis7_formal_protocol::incremental_state::StateWriteV1>,
    records: &mut crate::incremental_inventory_value_read::InventoryValueReader<'_, '_, R, C>,
    max_pending: usize,
) -> Result<ReservedValue<oasis7_formal_protocol::incremental_state::StateDeltaV1>, ValueTreeError>
{
    if records.values.class != MemoryClass::Ordinary {
        return Err(ValueTreeError::Invalid("diplomacy inner mutation class"));
    }
    let admission = workspace(records.values.memory, writes.len())?;
    let source = Source {
        source: records.values.source,
        memory: records.values.memory,
        class: records.values.class,
        io: &RefCell::new(&mut *records.values.io),
        cancelled: &RefCell::new(&mut *records.cancelled),
        error: &RefCell::new(None),
        labels: LABELS,
    };
    let delta = source.finish(
        oasis7_formal_protocol::incremental_state::StateDeltaV1::build(
            root.clone(),
            writes,
            &source,
            max_pending,
        ),
    )?;
    Ok(admission.retain(delta))
}
