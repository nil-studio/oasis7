//! Business reduction of real prepared depreciation items; no event publication.
use super::{FactoryDepreciationErrorV1, PreparedFactoryDepreciationV1};
use crate::world::incremental_material_normalization::{
    MaterialNormalizationErrorV1, MaterialNormalizationWorkV1, normalize_current_materials_v1,
};
use crate::{DomainEvent, WorldEventBody};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionError, ExecutionFence, PreparedGuard},
    incremental_state::{MerkleNodeReader, StatePageWriter},
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation, ReservedValue};
use oasis7_world_state::{
    incremental_factory_records::*,
    incremental_field_member::MemberRecordWriteContext,
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
    incremental_inventory::{CurrentInventoryMaterialScopeV1, InventoryWorldGroupWorkV1},
    incremental_inventory_overlay_change::InventoryOverlayChangeError,
};
#[derive(Debug)]
pub(in crate::world) enum FactoryDepreciationReduceErrorV1 {
    Execution(ExecutionError),
    Value(ValueTreeError),
    Batch(FactoryDepreciationErrorV1),
    Overlay(WorldFieldOverlayErrorV1),
    Inventory(InventoryOverlayChangeError),
}
macro_rules! from_error {
    ($ty:ty,$variant:ident) => {
        impl From<$ty> for FactoryDepreciationReduceErrorV1 {
            fn from(e: $ty) -> Self {
                Self::$variant(e)
            }
        }
    };
}
from_error!(ExecutionError, Execution);
from_error!(ValueTreeError, Value);
from_error!(FactoryDepreciationErrorV1, Batch);
from_error!(WorldFieldOverlayErrorV1, Overlay);
from_error!(InventoryOverlayChangeError, Inventory);
impl From<MaterialNormalizationErrorV1> for FactoryDepreciationReduceErrorV1 {
    fn from(e: MaterialNormalizationErrorV1) -> Self {
        match e {
            MaterialNormalizationErrorV1::Execution(e) => Self::Execution(e),
            MaterialNormalizationErrorV1::Value(e) => Self::Value(e),
            MaterialNormalizationErrorV1::Overlay(e) => Self::Overlay(e),
            MaterialNormalizationErrorV1::Inventory(e) => Self::Inventory(e),
        }
    }
}
pub(in crate::world) struct FactoryDepreciationReduceWorkV1 {
    pub max_material_entries: usize,
    pub comparison_bytes_remaining: u64,
    pub cells_remaining: u64,
}
impl FactoryDepreciationReduceWorkV1 {
    fn cell(&mut self) -> Result<(), ValueTreeError> {
        self.cells_remaining =
            self.cells_remaining
                .checked_sub(1)
                .ok_or(ValueTreeError::ResourceLimited(
                    "depreciation reducer cells",
                ))?;
        Ok(())
    }
}
/// Not Clone and no public index setter. Only a successful exact-start check
/// creates progress; only successful owner writes advance its next item/guard.
pub(in crate::world) struct FactoryDepreciationReducerV1<'batch> {
    batch: &'batch ReservedValue<PreparedFactoryDepreciationV1>,
    next: usize,
    pending: Option<usize>,
    guard: PreparedGuard,
    memory: MemoryBudget,
    _fee: MemoryReservation,
}
pub(in crate::world) struct AppliedFactoryDepreciationV1<'batch> {
    batch: &'batch ReservedValue<PreparedFactoryDepreciationV1>,
    ordinal: usize,
    guard: PreparedGuard,
    memory: MemoryBudget,
    _fee: MemoryReservation,
}
impl<'batch> AppliedFactoryDepreciationV1<'batch> {
    pub(in crate::world) fn body(&self) -> &WorldEventBody {
        &self.batch.bodies()[self.ordinal]
    }
    pub(in crate::world) fn time(&self) -> u64 {
        self.batch.time()
    }
    pub(in crate::world) fn ordinal(&self) -> usize {
        self.ordinal
    }
    pub(in crate::world) fn batch(&self) -> &'batch ReservedValue<PreparedFactoryDepreciationV1> {
        self.batch
    }
    pub(in crate::world) fn validate<M: MerkleNodeReader>(
        &self,
        tx: &oasis7_formal_protocol::incremental_execution::ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        now: u64,
        live: &ExecutionFence,
    ) -> Result<(), FactoryDepreciationReduceErrorV1> {
        tx.validate_guard(&self.guard, live)?;
        self.batch.validate(tx, memory, now, &tx.guard(), live)?;
        if !self.memory.same_pool(memory) {
            return Err(ValueTreeError::Invalid("applied depreciation pool").into());
        }
        Ok(())
    }
}
impl<'batch> FactoryDepreciationReducerV1<'batch> {
    pub(in crate::world) fn applied_items(&self) -> usize {
        self.next
    }
    pub(in crate::world) fn remaining_items(&self) -> usize {
        self.batch.bodies().len().saturating_sub(self.next)
    }
    #[allow(clippy::too_many_arguments)]
    pub(in crate::world) fn apply_next_v1<M, R, E, C, W, CW>(
        &mut self,
        materials: &ReservedValue<CurrentInventoryMaterialScopeV1>,
        ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
        write: &mut MemberRecordWriteContext<'_, W, CW>,
        factory_work: &mut FactoryWorkBudgetV1,
        group_work: &mut InventoryWorldGroupWorkV1,
        work: &mut FactoryDepreciationReduceWorkV1,
        guard: &PreparedGuard,
        live: &ExecutionFence,
    ) -> Result<Option<AppliedFactoryDepreciationV1<'batch>>, FactoryDepreciationReduceErrorV1>
    where
        M: MerkleNodeReader,
        R: ValueNodeReader,
        E: FnMut() -> Option<ValueTreeError>,
        C: FnMut() -> bool,
        W: StatePageWriter,
        CW: FnMut() -> bool,
    {
        let result = (|| {
            ctx.transaction.validate_guard(guard, live)?;
            ctx.transaction.validate_guard(&self.guard, live)?;
            self.batch.validate(
                ctx.transaction,
                ctx.values.memory,
                self.batch.time(),
                guard,
                live,
            )?;
            if !self.memory.same_pool(ctx.values.memory)
                || ctx.values.class != MemoryClass::Ordinary
                || !write.memory.same_pool(ctx.values.memory)
                || !materials.uses_budget(ctx.values.memory)
            {
                return Err(ValueTreeError::Invalid("depreciation reducer pool/class").into());
            }
            work.cell()?;
            if (ctx.cancelled)() || (write.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            if self.pending.is_some() {
                return Err(ValueTreeError::Invalid(
                    "depreciation item awaits publication and route",
                )
                .into());
            }
            let Some(body) = self.batch.bodies().get(self.next) else {
                return Ok(None);
            };
            let WorldEventBody::Domain(DomainEvent::FactoryDurabilityChanged {
                factory_id,
                durability_ppm,
                ..
            }) = body
            else {
                return Err(ValueTreeError::Invalid("depreciation prepared body kind").into());
            };
            let applied_fee = ctx
                .values
                .memory
                .reserve(
                    std::mem::size_of::<AppliedFactoryDepreciationV1<'_>>() + 128,
                    MemoryClass::Ordinary,
                )
                .map_err(ValueTreeError::Memory)?;
            materials.validate_current(ctx, guard, live)?;
            normalize_current_materials_v1(
                materials,
                ctx,
                write,
                group_work,
                &mut MaterialNormalizationWorkV1 {
                    max_union_entries: work.max_material_entries,
                    cells_remaining: &mut work.cells_remaining,
                    comparison_bytes_remaining: &mut work.comparison_bytes_remaining,
                },
                guard,
                live,
            )?;
            // The original reducer looks up the EVENT's embedded ID as an outer map
            // key. Missing factory still commits the complete material normalization.
            work.cell()?;
            if (ctx.cancelled)() || (write.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            let marker = read_factory_cell_overlay_v1(
                FactoryRecordKeyV1::Factory(factory_id),
                FactoryCellV1::Record,
                ctx,
                8,
                &mut std::io::sink(),
            )?;
            match marker {
                FactoryCellReadV1::RecordAbsent => {}
                FactoryCellReadV1::Value(FactoryValueV1::Unit) => {
                    work.cell()?;
                    let current = match read_factory_cell_overlay_v1(
                        FactoryRecordKeyV1::Factory(factory_id),
                        FactoryCellV1::Column(FactoryColumnV1::FactoryDurabilityPpm),
                        ctx,
                        8,
                        &mut std::io::sink(),
                    )? {
                        FactoryCellReadV1::Value(FactoryValueV1::Signed(value)) => value,
                        _ => {
                            return Err(ValueTreeError::Invalid(
                                "depreciation reducer durability type",
                            )
                            .into());
                        }
                    };
                    let guard = ctx.transaction.guard();
                    change_factory_cell_overlay_v1(
                        FactoryCellChangeV1 {
                            key: FactoryRecordKeyV1::Factory(factory_id),
                            cell: FactoryCellV1::Column(FactoryColumnV1::FactoryDurabilityPpm),
                            expected: Some(FactoryValueRefV1::Signed(current)),
                            after: Some(FactoryValueRefV1::Signed(
                                (*durability_ppm).clamp(0, 1_000_000),
                            )),
                            max_current_payload_bytes: 8,
                        },
                        ctx,
                        write,
                        factory_work,
                        &guard,
                        live,
                    )?;
                }
                _ => {
                    return Err(
                        ValueTreeError::Invalid("depreciation reducer factory marker").into(),
                    );
                }
            }
            if (ctx.cancelled)() || (write.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            let ordinal = self.next;
            self.pending = Some(ordinal);
            self.guard = ctx.transaction.guard();
            Ok(Some(AppliedFactoryDepreciationV1 {
                batch: self.batch,
                ordinal,
                guard: ctx.transaction.guard(),
                memory: ctx.values.memory.clone(),
                _fee: applied_fee,
            }))
        })();
        if result.is_err() {
            let _: Result<(), ExecutionError> =
                ctx.transaction.contribute(|_| Err(ExecutionError::Aborted));
        }
        result
    }
    pub(super) fn finish_routed_v1<M: MerkleNodeReader>(
        &mut self,
        routed: &super::publish::RoutedFactoryDepreciationV1<'batch>,
        tx: &mut oasis7_formal_protocol::incremental_execution::ExecutionTransaction<'_, M>,
        guard: &PreparedGuard,
        live: &ExecutionFence,
    ) -> Result<(), FactoryDepreciationReduceErrorV1> {
        let result = (|| {
            tx.validate_guard(guard, live)?;
            routed.validate(tx, live)?;
            if self.pending != Some(self.next)
                || routed.ordinal() != self.next
                || !std::ptr::eq(self.batch, routed.batch())
            {
                return Err(ValueTreeError::Invalid("depreciation routed receipt progress").into());
            }
            tx.validate_transaction_identity(&self.guard)?;
            self.next = self
                .next
                .checked_add(1)
                .ok_or(ValueTreeError::ResourceLimited(
                    "depreciation reducer progress",
                ))?;
            self.pending = None;
            self.guard = tx.guard();
            Ok(())
        })();
        if result.is_err() {
            let _: Result<(), ExecutionError> = tx.contribute(|_| Err(ExecutionError::Aborted));
        }
        result
    }
}
pub(in crate::world) fn start_factory_depreciation_reducer_v1<'batch, M, R, E, C>(
    batch: &'batch ReservedValue<PreparedFactoryDepreciationV1>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    now: u64,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<FactoryDepreciationReducerV1<'batch>, FactoryDepreciationReduceErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let result = (|| {
        if !batch.uses_budget(ctx.values.memory) {
            return Err(ValueTreeError::Invalid("depreciation reducer batch pool").into());
        }
        batch.claim_reducer_start_v1(ctx, now, guard, live)?;
        let fee = ctx
            .values
            .memory
            .reserve(
                std::mem::size_of::<FactoryDepreciationReducerV1<'_>>() + 128,
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        Ok(FactoryDepreciationReducerV1 {
            batch,
            next: 0,
            pending: None,
            guard: ctx.transaction.guard(),
            memory: ctx.values.memory.clone(),
            _fee: fee,
        })
    })();
    if result.is_err() {
        let _: Result<(), ExecutionError> =
            ctx.transaction.contribute(|_| Err(ExecutionError::Aborted));
    }
    result
}
