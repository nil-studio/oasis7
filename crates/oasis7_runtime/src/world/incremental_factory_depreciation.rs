//! Prepare the complete original depreciation phase from authenticated Current.
//! No reducer, original publication, event routing or accepted promotion occurs here.
#[path = "incremental_factory_depreciation_kernel.rs"]
mod kernel;
#[path = "incremental_factory_depreciation_publish.rs"]
pub(in crate::world) mod publish;
#[path = "incremental_factory_depreciation_reduce.rs"]
pub(in crate::world) mod reduce;
use super::{
    incremental_body::{BodyMutationContext, BodyMutationError, Discard},
    incremental_factory_directory_orders::FactoryOrderDirectoryErrorV1,
    incremental_factory_directory_records::{
        CurrentFactoryRecipeDirectoryV1, FactoryRecipeDirectoryWorkV1,
    },
    incremental_industrial_eligibility::{
        IndustrialEligibilityErrorV1, IndustrialEligibilitySourcesV1, IndustrialEligibilityWorkV1,
        current_producing_order_blocker_v1,
    },
};
use crate::{DomainEvent, WorldEventBody};
pub(super) use kernel::{FACTORY_DEPRECIATION_REASON, depreciation_v1};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionError, ExecutionFence, ExecutionTransaction, PreparedGuard},
    incremental_state::MerkleNodeReader,
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use oasis7_world_state::{
    incremental_factory_records::*,
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
    incremental_field_read::WorldFieldReadContextV1,
    incremental_industry_jobs::*,
};
#[derive(Debug)]
pub(super) enum FactoryDepreciationErrorV1 {
    Execution(ExecutionError),
    Value(ValueTreeError),
    Overlay(WorldFieldOverlayErrorV1),
    Directory(FactoryOrderDirectoryErrorV1),
    Industrial(IndustrialEligibilityErrorV1),
    Body(BodyMutationError),
}
macro_rules! from_error {
    ($ty:ty,$variant:ident) => {
        impl From<$ty> for FactoryDepreciationErrorV1 {
            fn from(e: $ty) -> Self {
                Self::$variant(e)
            }
        }
    };
}
from_error!(ExecutionError, Execution);
from_error!(ValueTreeError, Value);
from_error!(WorldFieldOverlayErrorV1, Overlay);
from_error!(FactoryOrderDirectoryErrorV1, Directory);
from_error!(IndustrialEligibilityErrorV1, Industrial);
from_error!(BodyMutationError, Body);
pub(super) struct FactoryDepreciationWorkV1 {
    pub max_factories: usize,
    pub max_workload_entries: usize,
    pub max_text_bytes: usize,
    pub cells_remaining: u64,
    pub comparison_bytes_remaining: u64,
}
impl FactoryDepreciationWorkV1 {
    fn cell(&mut self) -> Result<(), ValueTreeError> {
        self.cells_remaining = self
            .cells_remaining
            .checked_sub(1)
            .ok_or(ValueTreeError::ResourceLimited("depreciation cells"))?;
        Ok(())
    }
    fn compare(&mut self, bytes: usize) -> Result<(), ValueTreeError> {
        self.comparison_bytes_remaining =
            self.comparison_bytes_remaining
                .checked_sub(u64::try_from(bytes).map_err(|_| {
                    ValueTreeError::ResourceLimited("depreciation comparison bound")
                })?)
                .ok_or(ValueTreeError::ResourceLimited(
                    "depreciation comparison work",
                ))?;
        Ok(())
    }
}
struct FactoryRow {
    id: String,
    built_at: u64,
    durability: i64,
    maintenance: i64,
    slots: u16,
}
/// No constructor or mutable body access. Minted only after the complete real
/// Current selection, dependent eligibility checks and clock validation succeed.
pub(super) struct PreparedFactoryDepreciationV1 {
    identity: PreparedGuard,
    memory: MemoryBudget,
    now: u64,
    bodies: Vec<WorldEventBody>,
    claimed: std::cell::Cell<bool>,
}
impl PreparedFactoryDepreciationV1 {
    pub(super) fn claim_reducer_start_v1<M, R, E, C>(
        &self,
        ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
        now: u64,
        guard: &PreparedGuard,
        live: &ExecutionFence,
    ) -> Result<(), FactoryDepreciationErrorV1>
    where
        M: MerkleNodeReader,
        R: ValueNodeReader,
        E: FnMut() -> Option<ValueTreeError>,
        C: FnMut() -> bool,
    {
        self.validate_fresh_start_v1(ctx, now, guard, live)?;
        if self.claimed.replace(true) {
            let _: Result<(), ExecutionError> =
                ctx.transaction.contribute(|_| Err(ExecutionError::Aborted));
            return Err(ValueTreeError::Invalid("depreciation batch already claimed").into());
        }
        Ok(())
    }
    pub(super) fn bodies(&self) -> &[WorldEventBody] {
        &self.bodies
    }
    pub(super) fn time(&self) -> u64 {
        self.now
    }
    /// Transaction identity/time/pool only: a caller-fresh guard does not prove
    /// the prepared observations remain fresh. First consumption must use
    /// validate_fresh_start_v1; this method is suitable only after that boundary.
    pub(super) fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        now: u64,
        guard: &PreparedGuard,
        live: &ExecutionFence,
    ) -> Result<(), FactoryDepreciationErrorV1> {
        tx.validate_guard(guard, live)?;
        tx.validate_transaction_identity(&self.identity)?;
        if !self.memory.same_pool(memory) || !tx.uses_budget(memory) || self.now != now {
            return Err(ValueTreeError::Invalid("depreciation batch binding").into());
        }
        Ok(())
    }
    /// Authenticate the first consumption against the exact preparation guard
    /// and actual registered clock. No consumption permit is minted here.
    pub(super) fn validate_fresh_start_v1<M, R, E, C>(
        &self,
        ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
        now: u64,
        guard: &PreparedGuard,
        live: &ExecutionFence,
    ) -> Result<(), FactoryDepreciationErrorV1>
    where
        M: MerkleNodeReader,
        R: ValueNodeReader,
        E: FnMut() -> Option<ValueTreeError>,
        C: FnMut() -> bool,
    {
        let result = (|| {
            self.validate(ctx.transaction, ctx.values.memory, now, guard, live)?;
            ctx.transaction.validate_guard(&self.identity, live)?;
            if ctx.values.class != MemoryClass::Ordinary {
                return Err(ValueTreeError::Invalid("depreciation start class").into());
            }
            super::incremental_clock::check_clock(
                now,
                ctx.transaction,
                &mut BodyMutationContext {
                    sink: &mut Discard,
                    memory: ctx.values.memory,
                    io: &mut *ctx.values.io,
                    cancelled: &mut ctx.cancelled,
                },
            )?;
            if (ctx.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            Ok(())
        })();
        if result.is_err() {
            let _: Result<(), ExecutionError> =
                ctx.transaction.contribute(|_| Err(ExecutionError::Aborted));
        }
        result
    }
}
#[allow(clippy::too_many_arguments)]
pub(super) fn prepare_current_factory_depreciation_v1<M, R, E, C, P, V, ER, CR, FR, FC, RR, RC>(
    directory: &ReservedValue<CurrentFactoryRecipeDirectoryV1<'_>>,
    sources: &IndustrialEligibilitySourcesV1<'_, '_>,
    now: u64,
    resume_after_product_validation_intent: bool,
    state: &M,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    read: &mut WorldFieldReadContextV1<'_, P, V, ER, CR>,
    factory: &mut FactoryTransitionReadContextV1<'_, FR, FC>,
    recipe: &mut IndustryTransitionReadContextV1<'_, RR, RC>,
    factory_work: &mut FactoryWorkBudgetV1,
    recipe_work: &mut IndustryWorkBudgetV1,
    directory_work: &mut FactoryRecipeDirectoryWorkV1,
    industrial_work: &mut IndustrialEligibilityWorkV1,
    work: &mut FactoryDepreciationWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<ReservedValue<PreparedFactoryDepreciationV1>, FactoryDepreciationErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    ER: FnMut() -> Option<ValueTreeError>,
    CR: FnMut() -> bool,
    FR: ValueNodeReader,
    FC: FnMut() -> bool,
    RR: ValueNodeReader,
    RC: FnMut() -> bool,
{
    let result = (|| {
        ctx.transaction.validate_guard(guard, live)?;
        if ctx.values.class != MemoryClass::Ordinary
            || !ctx.transaction.uses_budget(ctx.values.memory)
            || !directory.uses_budget(ctx.values.memory)
        {
            return Err(ValueTreeError::Invalid("depreciation pool/class").into());
        }
        super::incremental_clock::check_clock(
            now,
            ctx.transaction,
            &mut BodyMutationContext {
                sink: &mut Discard,
                memory: ctx.values.memory,
                io: &mut *ctx.values.io,
                cancelled: &mut ctx.cancelled,
            },
        )?;
        if (ctx.cancelled)() || (read.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        // The persisted recovery intent proves the entire prologue has already run.
        if resume_after_product_validation_intent {
            let fee = ctx
                .values
                .memory
                .reserve(
                    std::mem::size_of::<PreparedFactoryDepreciationV1>() + 512,
                    MemoryClass::Ordinary,
                )
                .map_err(ValueTreeError::Memory)?;
            return Ok(fee.retain(PreparedFactoryDepreciationV1 {
                claimed: std::cell::Cell::new(false),
                identity: ctx.transaction.guard(),
                memory: ctx.values.memory.clone(),
                now,
                bodies: Vec::new(),
            }));
        }
        let guard = ctx.transaction.guard();
        directory.validate_current(
            ctx.transaction,
            factory,
            recipe,
            factory_work,
            recipe_work,
            directory_work,
            &guard,
            live,
        )?;
        let guard = ctx.transaction.guard();
        sources.orders.validate_current(ctx, &guard, live)?;
        let n = directory.iter_factory_ids().len();
        let jobs = directory.iter_recipe_job_ids().len();
        let orders = sources.orders.iter_order_ids().len();
        let workload_capacity = jobs
            .checked_add(orders)
            .ok_or(ValueTreeError::ResourceLimited(
                "depreciation workload bound",
            ))?;
        if n > work.max_factories || workload_capacity > work.max_workload_entries {
            return Err(ValueTreeError::ResourceLimited("depreciation candidate bound").into());
        }
        let output_bytes = n
            .checked_mul(
                std::mem::size_of::<WorldEventBody>()
                    .checked_add(work.max_text_bytes)
                    .and_then(|v| v.checked_add(FACTORY_DEPRECIATION_REASON.len()))
                    .ok_or(ValueTreeError::ResourceLimited("depreciation output bound"))?,
            )
            .and_then(|v| v.checked_add(4096))
            .ok_or(ValueTreeError::ResourceLimited("depreciation output bound"))?;
        let fee = ctx
            .values
            .memory
            .reserve(output_bytes, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let phase_bytes = n
            .checked_mul(
                std::mem::size_of::<FactoryRow>()
                    .checked_mul(2)
                    .and_then(|v| v.checked_add(work.max_text_bytes))
                    .ok_or(ValueTreeError::ResourceLimited("depreciation phase bound"))?,
            )
            .and_then(|v| {
                v.checked_add(
                    workload_capacity.checked_mul(
                        std::mem::size_of::<String>().checked_add(work.max_text_bytes)?,
                    )?,
                )
            })
            .and_then(|v| v.checked_add(work.max_text_bytes))
            .and_then(|v| v.checked_add(4096))
            .ok_or(ValueTreeError::ResourceLimited("depreciation phase bound"))?;
        let _phase = ctx
            .values
            .memory
            .reserve(phase_bytes, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let mut rows = Vec::with_capacity(n);
        let mut workloads = Vec::with_capacity(workload_capacity);
        let mut bodies = Vec::with_capacity(n);
        if rows.capacity() > n || workloads.capacity() > workload_capacity || bodies.capacity() > n
        {
            return Err(ValueTreeError::ResourceLimited("depreciation actual capacity").into());
        }
        for outer_id in directory.iter_factory_ids() {
            let id = factory_text(outer_id, ctx, work)?;
            let built_at =
                match factory_scalar(outer_id, FactoryColumnV1::FactoryBuiltAt, ctx, work)? {
                    FactoryValueV1::Unsigned(n) => n,
                    _ => return Err(ValueTreeError::Invalid("depreciation built_at type").into()),
                };
            let durability =
                match factory_scalar(outer_id, FactoryColumnV1::FactoryDurabilityPpm, ctx, work)? {
                    FactoryValueV1::Signed(n) => n,
                    _ => return Err(ValueTreeError::Invalid("depreciation durability type").into()),
                };
            let maintenance =
                match factory_scalar(outer_id, FactoryColumnV1::SpecMaintenancePerTick, ctx, work)?
                {
                    FactoryValueV1::Signed(n) => n,
                    _ => {
                        return Err(ValueTreeError::Invalid("depreciation maintenance type").into());
                    }
                };
            let slots = match factory_scalar(outer_id, FactoryColumnV1::SpecRecipeSlots, ctx, work)?
            {
                FactoryValueV1::Unsigned(n) => u16::try_from(n)
                    .map_err(|_| ValueTreeError::Invalid("depreciation slots range"))?,
                _ => return Err(ValueTreeError::Invalid("depreciation slots type").into()),
            };
            rows.push(FactoryRow {
                id,
                built_at,
                durability,
                maintenance,
                slots,
            });
        }
        for id in directory.iter_recipe_job_ids() {
            work.cell()?;
            if (ctx.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            let mut text = Vec::with_capacity(work.max_text_bytes);
            let value = read_industry_cell_overlay_v1(
                IndustryRecordKeyV1 {
                    kind: IndustryRecordKindV1::Recipe,
                    id,
                },
                IndustryCellV1::Column(IndustryColumnV1::RecordFactoryId),
                ctx,
                work.max_text_bytes as u64,
                &mut text,
            )?;
            if !matches!(
                value,
                IndustryCellReadV1::Value(IndustryValueV1::Text { .. })
            ) {
                return Err(ValueTreeError::Invalid("depreciation recipe factory type").into());
            }
            let text = String::from_utf8(text)
                .map_err(|_| ValueTreeError::Invalid("depreciation recipe UTF8"))?;
            if text.capacity() > work.max_text_bytes {
                return Err(ValueTreeError::ResourceLimited("depreciation text capacity").into());
            }
            workloads.push(text);
        }
        for (id, index) in sources.orders.iter_order_indices() {
            work.cell()?;
            if (ctx.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            if index.phase() != oasis7_world_state::world_rules::AgentIndustrialPhaseV1::Producing {
                continue;
            }
            let guard = ctx.transaction.guard();
            let blocker = current_producing_order_blocker_v1(
                sources,
                id,
                now,
                state,
                ctx,
                read,
                industrial_work,
                &guard,
                live,
            )?;
            if blocker.blocker.is_none() {
                let id = index.factory_id();
                if id.len() > work.max_text_bytes {
                    return Err(ValueTreeError::ResourceLimited(
                        "depreciation order factory bytes",
                    )
                    .into());
                }
                let copy = id.to_owned();
                if copy.capacity() > work.max_text_bytes {
                    return Err(
                        ValueTreeError::ResourceLimited("depreciation text capacity").into(),
                    );
                }
                workloads.push(copy);
            }
        }
        let sort_bytes = n
            .checked_mul(n)
            .and_then(|v| v.checked_mul(work.max_text_bytes.max(1)))
            .and_then(|v| v.checked_mul(8))
            .ok_or(ValueTreeError::ResourceLimited("depreciation sort bound"))?;
        work.compare(sort_bytes)?;
        // Stable sort preserves the outer BTreeMap order for duplicate embedded IDs.
        let mut sort_cancelled = false;
        rows.sort_by(|a, b| {
            sort_cancelled |= (ctx.cancelled)();
            a.id.cmp(&b.id)
        });
        if sort_cancelled {
            return Err(ValueTreeError::Cancelled.into());
        }
        for row in rows {
            if (ctx.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            let mut active = 0i64;
            for factory in &workloads {
                work.compare(factory.len().saturating_add(row.id.len()))?;
                if (ctx.cancelled)() {
                    return Err(ValueTreeError::Cancelled.into());
                }
                if factory == &row.id {
                    active = active.saturating_add(1)
                }
            }
            if let Some((previous_durability_ppm, durability_ppm)) = depreciation_v1(
                now,
                row.built_at,
                row.durability,
                row.maintenance,
                row.slots,
                active,
            ) {
                bodies.push(WorldEventBody::Domain(
                    DomainEvent::FactoryDurabilityChanged {
                        factory_id: row.id,
                        previous_durability_ppm,
                        durability_ppm,
                        reason: FACTORY_DEPRECIATION_REASON.to_owned(),
                    },
                ));
            }
        }
        if (ctx.cancelled)() || (read.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(fee.retain(PreparedFactoryDepreciationV1 {
            claimed: std::cell::Cell::new(false),
            identity: ctx.transaction.guard(),
            memory: ctx.values.memory.clone(),
            now,
            bodies,
        }))
    })();
    if result.is_err() {
        let _: Result<(), ExecutionError> =
            ctx.transaction.contribute(|_| Err(ExecutionError::Aborted));
    }
    result
}
fn factory_scalar<M, R, E, C>(
    id: &str,
    column: FactoryColumnV1,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    work: &mut FactoryDepreciationWorkV1,
) -> Result<FactoryValueV1, FactoryDepreciationErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    work.cell()?;
    if (ctx.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    match read_factory_cell_overlay_v1(
        FactoryRecordKeyV1::Factory(id),
        FactoryCellV1::Column(column),
        ctx,
        work.max_text_bytes as u64,
        &mut std::io::sink(),
    )? {
        FactoryCellReadV1::Value(value) => Ok(value),
        _ => Err(ValueTreeError::Invalid("depreciation selected factory absent").into()),
    }
}
fn factory_text<M, R, E, C>(
    id: &str,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    work: &mut FactoryDepreciationWorkV1,
) -> Result<String, FactoryDepreciationErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    work.cell()?;
    if (ctx.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    let mut bytes = Vec::with_capacity(work.max_text_bytes);
    let value = read_factory_cell_overlay_v1(
        FactoryRecordKeyV1::Factory(id),
        FactoryCellV1::Column(FactoryColumnV1::FactoryFactoryId),
        ctx,
        work.max_text_bytes as u64,
        &mut bytes,
    )?;
    if !matches!(value, FactoryCellReadV1::Value(FactoryValueV1::Text { .. })) {
        return Err(ValueTreeError::Invalid("depreciation embedded factory id type").into());
    }
    let value = String::from_utf8(bytes)
        .map_err(|_| ValueTreeError::Invalid("depreciation factory UTF8"))?;
    if value.capacity() > work.max_text_bytes {
        return Err(ValueTreeError::ResourceLimited("depreciation text capacity").into());
    }
    Ok(value)
}
