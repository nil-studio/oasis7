//! Complete starter receipt projections, authenticated once at GEN/cold and
//! merged from actual Current Completion changes thereafter. DATA only.
use crate::{
    MaterialLedgerId, MaterialStack,
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
    incremental_field_read::WorldFieldReadContextV1,
    incremental_industry_jobs::*,
    incremental_ledger_value::LedgerValueKindV1,
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFence, PreparedGuard, RootSetV1},
    incremental_state::{MerkleNodeReader, StateNamespaceV1, StateRootV1, StateScanBudget},
    incremental_value::{ValueNodeReader, ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation, ReservedValue};
use std::{io::Write, mem::size_of, sync::Arc};
type Held = Arc<ReservedValue<Receipt>>;
struct Receipt {
    class: MemoryClass,
    id: u64,
    value: [u8; 32],
    child: StateRootV1,
    factory: String,
    recipe: String,
    batches: u32,
    output: MaterialLedgerId,
    produce: Vec<MaterialStack>,
}
pub struct FactoryScheduleReceiptIndexV1 {
    roots: [u8; 32],
    counts: [u64; 5],
    class: MemoryClass,
    rows: Vec<Held>,
    _slots: MemoryReservation,
}
pub struct FactoryScheduleReceiptWorkV1 {
    pub cells_remaining: u64,
    pub bytes_remaining: u64,
    pub max_receipts: usize,
    pub max_text_bytes: usize,
    pub max_produce_stacks: usize,
    pub max_pending_changes: usize,
}
impl FactoryScheduleReceiptWorkV1 {
    fn charge(&mut self, n: usize) -> Result<(), ValueTreeError> {
        self.cells_remaining =
            self.cells_remaining
                .checked_sub(1)
                .ok_or(ValueTreeError::ResourceLimited(
                    "factory schedule receipt cells",
                ))?;
        self.bytes_remaining = self
            .bytes_remaining
            .checked_sub(u64::try_from(n).map_err(|_| invalid())?)
            .ok_or(ValueTreeError::ResourceLimited(
                "factory schedule receipt bytes",
            ))?;
        Ok(())
    }
}
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("factory schedule receipt directory")
}
fn bytes<T>(n: usize) -> Result<usize, ValueTreeError> {
    n.checked_mul(size_of::<T>()).ok_or_else(invalid)
}
fn binding(
    q: &ReservedValue<IndustryFamilyQualificationV1>,
    roots: &RootSetV1,
    memory: &MemoryBudget,
    class: MemoryClass,
) -> Result<[u8; 32], ValueTreeError> {
    if !q.uses_budget(memory)
        || q.allocation_class() != class
        || q.reserved_bytes() < size_of::<IndustryFamilyQualificationV1>()
    {
        return Err(invalid());
    }
    let _phase = memory.reserve(4096, class)?;
    let digest = roots.digest().map_err(|_| invalid())?;
    q.validate_binding(&digest)?;
    Ok(digest)
}
struct Output<'a>(&'a mut dyn Write);
impl Write for Output<'_> {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        self.0.write(b)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.0.flush()
    }
}
type Read<'a> = dyn FnMut(
        IndustryCellV1<'static>,
        &mut dyn Write,
    ) -> Result<IndustryCellReadV1, WorldFieldOverlayErrorV1>
    + 'a;
fn unsigned(
    cell: IndustryCellV1<'static>,
    read: &mut Read<'_>,
    work: &mut FactoryScheduleReceiptWorkV1,
) -> Result<u64, WorldFieldOverlayErrorV1> {
    work.charge(8)?;
    match read(cell, &mut std::io::sink())? {
        IndustryCellReadV1::Value(IndustryValueV1::Unsigned(n)) => Ok(n),
        _ => Err(invalid().into()),
    }
}
fn text(
    cell: IndustryCellV1<'static>,
    read: &mut Read<'_>,
    work: &mut FactoryScheduleReceiptWorkV1,
) -> Result<String, WorldFieldOverlayErrorV1> {
    let mut out = Vec::with_capacity(work.max_text_bytes);
    if out.capacity() != work.max_text_bytes {
        return Err(invalid().into());
    }
    match read(cell, &mut out)? {
        IndustryCellReadV1::Value(IndustryValueV1::Text { utf8_bytes })
            if usize::try_from(utf8_bytes).ok() == Some(out.len()) =>
        {
            work.charge(out.len())?;
            Ok(String::from_utf8(out).map_err(|_| invalid())?)
        }
        _ => Err(invalid().into()),
    }
}
fn project(
    id: u64,
    value: [u8; 32],
    child: &StateRootV1,
    read: &mut Read<'_>,
    memory: &MemoryBudget,
    class: MemoryClass,
    work: &mut FactoryScheduleReceiptWorkV1,
) -> Result<Held, WorldFieldOverlayErrorV1> {
    use IndustryCellV1::Column as Col;
    use IndustryColumnV1 as F;
    work.charge(0)?;
    let cap = work.max_produce_stacks;
    let payload = cap
        .checked_add(3)
        .and_then(|n| n.checked_mul(work.max_text_bytes))
        .and_then(|n| n.checked_add(bytes::<MaterialStack>(cap).ok()?))
        .and_then(|n| n.checked_add(size_of::<ReservedValue<Receipt>>() + 2 * size_of::<usize>()))
        .and_then(|n| n.checked_add(child.node_root.len()))
        .ok_or_else(invalid)?;
    let fee = memory
        .reserve(payload, class)
        .map_err(ValueTreeError::Memory)?;
    let factory = text(Col(F::RecordFactoryId), read, work)?;
    let recipe = text(Col(F::RecordRecipeId), read, work)?;
    let batches = u32::try_from(unsigned(Col(F::RecordAcceptedBatches), read, work)?)
        .map_err(|_| invalid())?;
    let mut out = Vec::with_capacity(work.max_text_bytes);
    if out.capacity() != work.max_text_bytes {
        return Err(invalid().into());
    }
    let IndustryCellReadV1::Value(IndustryValueV1::Ledger(info)) =
        read(Col(F::RecordOutputLedger), &mut out)?
    else {
        return Err(invalid().into());
    };
    if usize::try_from(info.id_bytes).ok() != Some(out.len()) {
        return Err(invalid().into());
    }
    work.charge(out.len())?;
    let id_text = String::from_utf8(out).map_err(|_| invalid())?;
    let output = match info.kind {
        LedgerValueKindV1::World => MaterialLedgerId::World,
        LedgerValueKindV1::Agent => MaterialLedgerId::Agent(id_text),
        LedgerValueKindV1::Site => MaterialLedgerId::Site(id_text),
        LedgerValueKindV1::Factory => MaterialLedgerId::Factory(id_text),
    };
    let n = usize::try_from(unsigned(
        IndustryCellV1::Collection(IndustryCollectionV1::Produce),
        read,
        work,
    )?)
    .map_err(|_| invalid())?;
    if n > cap {
        return Err(ValueTreeError::ResourceLimited("factory schedule receipt produce").into());
    }
    let mut produce = Vec::with_capacity(n);
    if produce.capacity() != n {
        return Err(invalid().into());
    }
    for i in 0..n {
        let kind = text(
            IndustryCellV1::StackItem(
                IndustryCollectionV1::Produce,
                i as u64,
                IndustryStackColumnV1::Kind,
            ),
            read,
            work,
        )?;
        work.charge(8)?;
        let amount = match read(
            IndustryCellV1::StackItem(
                IndustryCollectionV1::Produce,
                i as u64,
                IndustryStackColumnV1::Amount,
            ),
            &mut std::io::sink(),
        )? {
            IndustryCellReadV1::Value(IndustryValueV1::Signed(n)) => n,
            _ => return Err(invalid().into()),
        };
        produce.push(MaterialStack { kind, amount });
    }
    let copied = child.clone();
    if copied.node_root.capacity() > child.node_root.len() {
        return Err(invalid().into());
    }
    Ok(Arc::new(fee.retain(Receipt {
        class,
        id,
        value,
        child: copied,
        factory,
        recipe,
        batches,
        output,
        produce,
    })))
}
impl FactoryScheduleReceiptIndexV1 {
    pub fn validate_binding(
        &self,
        q: &ReservedValue<IndustryFamilyQualificationV1>,
        roots: &RootSetV1,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<(), ValueTreeError> {
        if self.roots != binding(q, roots, memory, class)?
            || self.counts != q.record_counts()
            || self.class != class
            || u64::try_from(self.rows.len()).ok() != Some(self.counts[2])
        {
            return Err(invalid());
        }
        for row in &self.rows {
            if !row.uses_budget(memory) || row.class != class {
                return Err(invalid());
            }
        }
        Ok(())
    }
}

#[allow(clippy::too_many_arguments)]
pub fn build_factory_schedule_receipt_index_v1<M, R, E, C>(
    q: &ReservedValue<IndustryFamilyQualificationV1>,
    roots: &RootSetV1,
    read: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    industry_work: &mut IndustryWorkBudgetV1,
    work: &mut FactoryScheduleReceiptWorkV1,
    max_payload: u64,
) -> Result<ReservedValue<FactoryScheduleReceiptIndexV1>, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    read.check()?;
    let memory = read.values.memory;
    let class = read.values.class;
    let digest = binding(q, roots, memory, class)?;
    let n = usize::try_from(q.record_counts()[2]).map_err(|_| invalid())?;
    if n > work.max_receipts {
        return Err(ValueTreeError::ResourceLimited("factory schedule completion count").into());
    }
    let cap = n.checked_add(1).ok_or_else(invalid)?;
    let _phase = memory
        .reserve(
            bytes::<ReservedValue<IndustryQualifiedOuterIdentityV1>>(cap)?,
            class,
        )
        .map_err(ValueTreeError::Memory)?;
    let mut identities = Vec::with_capacity(cap);
    if identities.capacity() != cap {
        return Err(invalid().into());
    }
    {
        let mut ctx = IndustryFamilyReadContextV1 {
            state: read.state,
            values: ValueReadContext {
                source: read.values.source,
                memory,
                class,
                io: &mut *read.values.io,
            },
            key_work: &mut *read.key_work,
            semantic_bytes: read.semantic_bytes,
            record_bytes: read.record_bytes,
            max_payload_bytes: max_payload,
            outer_scan: scan,
            take_state_error: &mut read.take_state_error,
            cancelled: &mut read.cancelled,
        };
        visit_qualified_completion_parent_identities_v1(
            roots,
            q,
            &mut ctx,
            industry_work,
            |row| {
                work.charge(0)?;
                if identities.len() == cap {
                    return Err(invalid());
                }
                identities.push(row);
                Ok(())
            },
        )?;
    }
    let slots = memory
        .reserve(bytes::<Held>(n)?, class)
        .map_err(ValueTreeError::Memory)?;
    let owner = memory
        .reserve(size_of::<FactoryScheduleReceiptIndexV1>(), class)
        .map_err(ValueTreeError::Memory)?;
    let mut rows = Vec::with_capacity(n);
    if rows.capacity() != n {
        return Err(invalid().into());
    }
    let mut ctx = WorldFieldReadContextV1 {
        root: roots.state_root(StateNamespaceV1::Authorization),
        state: read.state,
        values: ValueReadContext {
            source: read.values.source,
            memory,
            class,
            io: &mut *read.values.io,
        },
        key_work: &mut *read.key_work,
        semantic_bytes: read.semantic_bytes,
        record_bytes: read.record_bytes,
        take_state_error: &mut read.take_state_error,
        cancelled: &mut read.cancelled,
    };
    for identity in identities {
        let IndustryQualifiedOuterIdentityV1::Record {
            id,
            value_root,
            record_root,
        } = &*identity
        else {
            continue;
        };
        if rows.len() == n {
            return Err(invalid().into());
        }
        let max = work.max_text_bytes.max(8) as u64;
        let mut lookup = |cell: IndustryCellV1<'static>, out: &mut dyn Write| {
            read_industry_cell_v1(
                IndustryRecordKeyV1 {
                    kind: IndustryRecordKindV1::Completion,
                    id: *id,
                },
                cell,
                &mut ctx,
                max,
                &mut Output(out),
            )
            .map_err(Into::into)
        };
        rows.push(project(
            *id,
            *value_root,
            record_root,
            &mut lookup,
            memory,
            class,
            work,
        )?);
    }
    if rows.len() != n {
        return Err(invalid().into());
    }
    ctx.check()?;
    Ok(owner.retain(FactoryScheduleReceiptIndexV1 {
        roots: digest,
        counts: q.record_counts(),
        class,
        rows,
        _slots: slots,
    }))
}

/// Holds a complete merged receipt projection, with all shared immutable row
/// fees alive. It does not grant Current writes, recipe or starter permission.
pub struct CurrentFactoryScheduleReceiptsV1 {
    rows: Vec<Held>,
    guard: PreparedGuard,
    memory: MemoryBudget,
    _slots: MemoryReservation,
}
impl CurrentFactoryScheduleReceiptsV1 {
    pub fn facts(&self) -> impl Iterator<Item = crate::StarterIndustrialReceiptFactsV1<'_>> {
        self.rows
            .iter()
            .map(|r| crate::StarterIndustrialReceiptFactsV1 {
                factory_id: &r.factory,
                recipe_id: &r.recipe,
                output_ledger: &r.output,
                accepted_batches: r.batches,
                produce: &r.produce,
            })
    }
    pub fn validate<M: MerkleNodeReader>(
        &self,
        tx: &oasis7_formal_protocol::incremental_execution::ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        live: &ExecutionFence,
    ) -> Result<(), WorldFieldOverlayErrorV1> {
        tx.validate_guard(&self.guard, live)?;
        if !self.memory.same_pool(memory) || !tx.uses_budget(memory) {
            return Err(invalid().into());
        }
        Ok(())
    }
}

#[allow(clippy::too_many_arguments)]
pub fn observe_current_factory_schedule_receipts_v1<M, R, E, C>(
    index: &ReservedValue<FactoryScheduleReceiptIndexV1>,
    q: &ReservedValue<IndustryFamilyQualificationV1>,
    roots: &RootSetV1,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    industry_work: &mut IndustryWorkBudgetV1,
    work: &mut FactoryScheduleReceiptWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<ReservedValue<CurrentFactoryScheduleReceiptsV1>, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    ctx.transaction.validate_guard(guard, live)?;
    let memory = ctx.values.memory;
    let class = MemoryClass::Ordinary;
    if ctx.values.class != class || index.class != class || !index.uses_budget(memory) {
        return Err(invalid().into());
    }
    index.validate_binding(q, roots, memory, class)?;
    let max = work.max_pending_changes;
    let _phase = memory
        .reserve(
            bytes::<ReservedValue<IndustryCurrentOuterChangeV1>>(max)?,
            class,
        )
        .map_err(ValueTreeError::Memory)?;
    let mut changes = Vec::with_capacity(max);
    if changes.capacity() != max {
        return Err(invalid().into());
    }
    {
        let mut read = IndustryTransitionReadContextV1 {
            values: ValueReadContext {
                source: ctx.values.source,
                memory,
                class,
                io: &mut *ctx.values.io,
            },
            key_work: &mut *ctx.key_work,
            semantic_bytes: ctx.semantic_bytes,
            record_bytes: ctx.record_bytes,
            max_payload_bytes: work.max_text_bytes.max(8) as u64,
            cancelled: &mut ctx.cancelled,
        };
        visit_completion_current_outer_changes_v1(
            ctx.transaction,
            q,
            &mut read,
            industry_work,
            |row| {
                work.charge(0)?;
                if changes.len() == max {
                    return Err(ValueTreeError::ResourceLimited(
                        "factory schedule receipt changes",
                    )
                    .into());
                }
                changes.push(row);
                Ok(())
            },
        )?;
    }
    let cap = index
        .rows
        .len()
        .checked_add(changes.len())
        .ok_or_else(invalid)?;
    if cap > work.max_receipts {
        return Err(ValueTreeError::ResourceLimited("factory schedule current receipts").into());
    }
    let slots = memory
        .reserve(bytes::<Held>(cap)?, class)
        .map_err(ValueTreeError::Memory)?;
    let owner = memory
        .reserve(size_of::<CurrentFactoryScheduleReceiptsV1>(), class)
        .map_err(ValueTreeError::Memory)?;
    let mut rows = Vec::with_capacity(cap);
    if rows.capacity() != cap {
        return Err(invalid().into());
    }
    for row in &index.rows {
        if (ctx.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        let mut changed = false;
        for change in &changes {
            work.charge(size_of::<u64>())?;
            if let IndustryCurrentOuterChangeV1::Record { id, before, .. } = &**change {
                if *id == row.id {
                    let Some(before) = before else {
                        return Err(invalid().into());
                    };
                    work.charge(
                        before
                            .record_root
                            .node_root
                            .len()
                            .checked_add(row.child.node_root.len())
                            .ok_or_else(invalid)?,
                    )?;
                    if before.value_root != row.value || before.record_root != row.child {
                        return Err(invalid().into());
                    }
                    changed = true;
                    break;
                }
            }
        }
        if !changed {
            rows.push(Arc::clone(row));
        }
    }
    for change in &changes {
        let IndustryCurrentOuterChangeV1::Record { id, before, after } = &**change else {
            continue;
        };
        if before.is_some() {
            let mut found = false;
            for row in &index.rows {
                work.charge(size_of::<u64>())?;
                if row.id == *id {
                    found = true;
                    break;
                }
            }
            if !found {
                return Err(invalid().into());
            }
        }
        let Some(after) = after else {
            continue;
        };
        let max = work.max_text_bytes.max(8) as u64;
        let mut read = |cell: IndustryCellV1<'static>, out: &mut dyn Write| {
            read_industry_cell_overlay_v1(
                IndustryRecordKeyV1 {
                    kind: IndustryRecordKindV1::Completion,
                    id: *id,
                },
                cell,
                ctx,
                max,
                &mut Output(out),
            )
        };
        rows.push(project(
            *id,
            after.value_root,
            &after.record_root,
            &mut read,
            memory,
            class,
            work,
        )?);
    }
    ctx.transaction
        .validate_guard(&ctx.transaction.guard(), live)?;
    Ok(owner.retain(CurrentFactoryScheduleReceiptsV1 {
        rows,
        guard: ctx.transaction.guard(),
        memory: memory.clone(),
        _slots: slots,
    }))
}
