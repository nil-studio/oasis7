//! Exact selected Current factory fields used by direct recipe admission.
//! No synthetic FactoryState or caller-created qualification is produced.
use crate::{
    MaterialLedgerId,
    incremental_factory_records::*,
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
    incremental_ledger_value::LedgerValueKindV1,
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFence, PreparedGuard},
    incremental_state::MerkleNodeReader,
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
use std::{io::Write, mem::size_of};

pub struct FactoryScheduleFactoryV1 {
    builder: String,
    slots: u16,
    active_jobs: u16,
    tags: Vec<String>,
    input: MaterialLedgerId,
    output: MaterialLedgerId,
}
impl FactoryScheduleFactoryV1 {
    pub fn builder(&self) -> &str {
        &self.builder
    }
    pub fn slots(&self) -> u16 {
        self.slots
    }
    pub fn active_jobs(&self) -> u16 {
        self.active_jobs
    }
    pub fn tags(&self) -> &[String] {
        &self.tags
    }
    pub fn input(&self) -> &MaterialLedgerId {
        &self.input
    }
    pub fn output(&self) -> &MaterialLedgerId {
        &self.output
    }
}
pub struct FactoryScheduleFactoryWorkV1 {
    pub cells_remaining: u64,
    pub bytes_remaining: u64,
    pub max_text_bytes: usize,
    pub max_tags: usize,
    pub max_stacks: usize,
}
impl FactoryScheduleFactoryWorkV1 {
    fn charge(&mut self, n: usize) -> Result<(), ValueTreeError> {
        self.cells_remaining =
            self.cells_remaining
                .checked_sub(1)
                .ok_or(ValueTreeError::ResourceLimited(
                    "factory schedule factory cells",
                ))?;
        self.bytes_remaining = self
            .bytes_remaining
            .checked_sub(u64::try_from(n).map_err(|_| invalid())?)
            .ok_or(ValueTreeError::ResourceLimited(
                "factory schedule factory bytes",
            ))?;
        Ok(())
    }
}
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("factory schedule selected factory")
}
fn cell<M, R, E, C>(
    id: &str,
    which: FactoryCellV1<'_>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    work: &mut FactoryScheduleFactoryWorkV1,
    out: &mut impl Write,
) -> Result<FactoryCellReadV1, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    work.charge(id.len())?;
    read_factory_cell_overlay_v1(
        FactoryRecordKeyV1::Factory(id),
        which,
        ctx,
        work.max_text_bytes.max(8) as u64,
        out,
    )
}
fn unsigned<M, R, E, C>(
    id: &str,
    which: FactoryCellV1<'_>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    work: &mut FactoryScheduleFactoryWorkV1,
) -> Result<u64, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    match cell(id, which, ctx, work, &mut std::io::sink())? {
        FactoryCellReadV1::Value(FactoryValueV1::Unsigned(n)) => Ok(n),
        _ => Err(invalid().into()),
    }
}
fn text<M, R, E, C>(
    id: &str,
    which: FactoryCellV1<'_>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    work: &mut FactoryScheduleFactoryWorkV1,
) -> Result<String, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let mut bytes = Vec::with_capacity(work.max_text_bytes);
    if bytes.capacity() != work.max_text_bytes {
        return Err(invalid().into());
    }
    match cell(id, which, ctx, work, &mut bytes)? {
        FactoryCellReadV1::Value(FactoryValueV1::Text { utf8_bytes })
            if usize::try_from(utf8_bytes).ok() == Some(bytes.len()) =>
        {
            work.charge(bytes.len())?;
            Ok(String::from_utf8(bytes).map_err(|_| invalid())?)
        }
        _ => Err(invalid().into()),
    }
}
fn ledger<M, R, E, C>(
    id: &str,
    column: FactoryColumnV1,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    work: &mut FactoryScheduleFactoryWorkV1,
) -> Result<MaterialLedgerId, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let mut bytes = Vec::with_capacity(work.max_text_bytes);
    if bytes.capacity() != work.max_text_bytes {
        return Err(invalid().into());
    }
    let FactoryCellReadV1::Value(FactoryValueV1::Ledger(info)) =
        cell(id, FactoryCellV1::Column(column), ctx, work, &mut bytes)?
    else {
        return Err(invalid().into());
    };
    if usize::try_from(info.id_bytes).ok() != Some(bytes.len()) {
        return Err(invalid().into());
    }
    work.charge(bytes.len())?;
    let text = String::from_utf8(bytes).map_err(|_| invalid())?;
    Ok(match info.kind {
        LedgerValueKindV1::World => MaterialLedgerId::World,
        LedgerValueKindV1::Agent => MaterialLedgerId::Agent(text),
        LedgerValueKindV1::Site => MaterialLedgerId::Site(text),
        LedgerValueKindV1::Factory => MaterialLedgerId::Factory(text),
    })
}

#[allow(clippy::too_many_arguments)]
pub fn recover_current_schedule_factory_v1<M, R, E, C>(
    id: &str,
    parent: &ReservedValue<FactoryFamilyQualificationV1>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    work: &mut FactoryScheduleFactoryWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<Option<ReservedValue<FactoryScheduleFactoryV1>>, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    ctx.transaction.validate_guard(guard, live)?;
    if ctx.values.class != MemoryClass::Ordinary
        || !parent.uses_budget(ctx.values.memory)
        || parent.allocation_class() != MemoryClass::Ordinary
    {
        return Err(invalid().into());
    }
    parent.validate_binding(&ctx.transaction.parent_fence().parent_roots)?;
    if matches!(
        cell(id, FactoryCellV1::Record, ctx, work, &mut std::io::sink())?,
        FactoryCellReadV1::RecordAbsent
    ) {
        return Ok(None);
    }
    let text_slots = work.max_tags.checked_add(3).ok_or_else(invalid)?;
    let size = size_of::<FactoryScheduleFactoryV1>()
        .checked_add(
            text_slots
                .checked_mul(work.max_text_bytes)
                .ok_or_else(invalid)?,
        )
        .and_then(|n| n.checked_add(work.max_tags.checked_mul(size_of::<String>())?))
        .ok_or_else(invalid)?;
    let fee = ctx
        .values
        .memory
        .reserve(size, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let builder = text(
        id,
        FactoryCellV1::Column(FactoryColumnV1::FactoryBuilderAgentId),
        ctx,
        work,
    )?;
    let slots = u16::try_from(unsigned(
        id,
        FactoryCellV1::Column(FactoryColumnV1::SpecRecipeSlots),
        ctx,
        work,
    )?)
    .map_err(|_| invalid())?;
    let active_jobs = u16::try_from(unsigned(
        id,
        FactoryCellV1::Column(FactoryColumnV1::ProductionActiveJobs),
        ctx,
        work,
    )?)
    .map_err(|_| invalid())?;
    let n = usize::try_from(unsigned(
        id,
        FactoryCellV1::Collection(FactoryCollectionV1::SpecTags),
        ctx,
        work,
    )?)
    .map_err(|_| invalid())?;
    if n > work.max_tags {
        return Err(ValueTreeError::ResourceLimited("factory schedule tags").into());
    }
    let mut tags = Vec::with_capacity(n);
    if tags.capacity() != n {
        return Err(invalid().into());
    }
    for i in 0..n {
        tags.push(text(
            id,
            FactoryCellV1::TextItem(FactoryCollectionV1::SpecTags, i as u64),
            ctx,
            work,
        )?);
    }
    let input = ledger(id, FactoryColumnV1::FactoryInputLedger, ctx, work)?;
    let output = ledger(id, FactoryColumnV1::FactoryOutputLedger, ctx, work)?;
    ctx.transaction
        .validate_guard(&ctx.transaction.guard(), live)?;
    Ok(Some(fee.retain(FactoryScheduleFactoryV1 {
        builder,
        slots,
        active_jobs,
        tags,
        input,
        output,
    })))
}

#[path = "incremental_factory_schedule_factory_full.rs"]
mod full;
pub use full::recover_current_schedule_complete_factory_v1;
