//! Single-record Current lifecycle observations using the original native codec.
//! Child of native qualification; neither the DTO nor this owner grants action authority.
use super::*;
use crate::incremental_world_native_industry::{
    FactoryConstructionReceiptsFieldV1, FactoryRecycleReceiptsFieldV1, IndustryProgressFieldV1,
};
use oasis7_formal_protocol::incremental_execution::{
    ExecutionFence, ExecutionTransaction, PreparedGuard,
};
use oasis7_proto::memory_budget::MemoryBudget;
#[path = "incremental_factory_lifecycle_native_write.rs"]
mod publication;
pub use publication::{
    change_current_factory_industry_stage_v1, write_current_factory_construction_receipt_v1,
    write_current_factory_recycle_receipt_v1,
};

#[cfg(test)]
#[path = "incremental_factory_lifecycle_native_current_tests.rs"]
mod tests;

/// Owns the real decoder reservations and the exact observed transaction revision.
/// Absence is obtained from the actual Current outer point, never a default DTO.
pub struct CurrentFactoryNativeRecordV1<T> {
    record: Option<WorldNativeRecoveredRecordV1<T>>,
    key: WorldFieldKeyV1,
    value_root: Option<[u8; 32]>,
    body: Option<ReservedValue<StateRootV1>>,
    guard: PreparedGuard,
    memory: MemoryBudget,
}
impl<T> CurrentFactoryNativeRecordV1<T> {
    pub fn value(&self) -> Option<&T> {
        self.record.as_ref().map(|r| r.value().value())
    }
    pub fn validate<M: MerkleNodeReader>(
        &self,
        tx: &mut ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        live: &ExecutionFence,
    ) -> Result<(), WorldFieldOverlayErrorV1> {
        tx.contribute(|tx| {
            tx.validate_guard(&self.guard, live)?;
            if !self.memory.same_pool(memory) || !tx.uses_budget(memory) {
                return Err(invalid().into());
            }
            Ok(())
        })
    }
}
#[derive(Clone, Copy)]
enum SelectedKey<'a> {
    Receipt(&'a str),
    ConstructionReceipt(&'a str),
    Progress,
}
#[allow(clippy::too_many_arguments)]
fn selected<
    F: WorldNativeFieldSpecV1,
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    selected: SelectedKey<'_>,
    parent: &ReservedValue<WorldNativeFieldQualificationV1<F>>,
    roots: &RootSetV1,
    tx: &mut ExecutionTransaction<'_, M>,
    ctx: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut TypedRecordReadWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<ReservedValue<CurrentFactoryNativeRecordV1<F::Value>>, WorldFieldOverlayErrorV1> {
    tx.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        read::check(ctx)?;
        let _phase = ctx
            .values
            .memory
            .reserve(32 << 10, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        if ctx.values.class != MemoryClass::Ordinary
            || !tx.uses_budget(ctx.values.memory)
            || !parent.uses_budget(ctx.values.memory)
            || parent.allocation_class() != MemoryClass::Ordinary
            || roots.digest().map_err(|_| invalid())? != tx.parent_fence().parent_roots
        {
            return Err(invalid().into());
        }
        parent.validate_binding(roots)?;
        let field = F::descriptor()?;
        let profile = F::profile()?;
        let ns = profile.namespace()?;
        if ctx.root != roots.state_root(ns) {
            return Err(invalid().into());
        }
        let fee = ctx
            .values
            .memory
            .reserve(
                std::mem::size_of::<CurrentFactoryNativeRecordV1<F::Value>>(),
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        let metadata = WorldFieldKeyV1::new(field, WorldFieldRecordKindV1::Metadata, None)
            .map_err(ValueTreeError::Invalid)?;
        let metadata_value = tx.read(ns, metadata.as_bytes())?.ok_or_else(invalid)?;
        read::check(ctx)?;
        ctx.records().presence(&metadata_value, &metadata, 0)?;
        let (key, source_key) = match selected {
            SelectedKey::Receipt(id) => (
                ctx.key(field, WorldFieldRecordKindV1::Entry, id)?.0,
                WorldNativeKeyRefV1::Text(id),
            ),
            SelectedKey::ConstructionReceipt(id) => (
                ctx.key(field, WorldFieldRecordKindV1::Entry, id)?.0,
                WorldNativeKeyRefV1::Text(id),
            ),
            SelectedKey::Progress => (
                ctx.key(field, WorldFieldRecordKindV1::Entry, &())?.0,
                WorldNativeKeyRefV1::Unit,
            ),
        };
        let value_root = tx.read(ns, key.as_bytes())?;
        read::check(ctx)?;
        let (body, record) = if let Some(value) = value_root {
            let body = read::read_body_at_entry_v1::<F, _, _, _, _>(&key, &value, ctx)?;
            let record = recover_typed_record_body_v1(&body, &profile, ctx, scan, work)?;
            F::validate_record_binding(source_key, &body, ctx, scan, work)?;
            (Some(body), Some(record))
        } else {
            if matches!(selected, SelectedKey::Progress) {
                return Err(
                    ValueTreeError::Invalid("Current industry progress entry missing").into(),
                );
            }
            (None, None)
        };
        read::check(ctx)?;
        let tail = tx.guard();
        tx.validate_guard(&tail, live)?;
        Ok(fee.retain(CurrentFactoryNativeRecordV1 {
            record,
            key,
            value_root,
            body,
            guard: tail,
            memory: ctx.values.memory.clone(),
        }))
    })
}
/// Reads one exact outer receipt key, preserving legitimate absence.
#[allow(clippy::too_many_arguments)]
pub fn recover_current_factory_recycle_receipt_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    id: &str,
    parent: &ReservedValue<WorldNativeFieldsQualificationV1>,
    roots: &RootSetV1,
    tx: &mut ExecutionTransaction<'_, M>,
    ctx: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut WorldNativeSelectedReadWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<
    ReservedValue<CurrentFactoryNativeRecordV1<crate::FactoryRecycleReceiptV1>>,
    WorldFieldOverlayErrorV1,
> {
    tx.contribute(|tx| {
        if !parent.uses_budget(ctx.values.memory) || parent.class != MemoryClass::Ordinary {
            return Err(invalid().into());
        }
        selected::<FactoryRecycleReceiptsFieldV1, _, _, _, _, _>(
            SelectedKey::Receipt(id),
            &parent.factory_recycle_receipts,
            roots,
            tx,
            ctx,
            scan,
            work,
            guard,
            live,
        )
    })
}
/// Reads one actual Current construction-receipt outer key, preserving
/// legitimate absence. The native field qualification and original codec
/// remain mandatory; the returned record is only a data observation.
#[allow(clippy::too_many_arguments)]
pub fn recover_current_factory_construction_receipt_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    id: &str,
    parent: &ReservedValue<WorldNativeFieldsQualificationV1>,
    roots: &RootSetV1,
    tx: &mut ExecutionTransaction<'_, M>,
    ctx: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut WorldNativeSelectedReadWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<
    ReservedValue<
        CurrentFactoryNativeRecordV1<oasis7_agent_api::world_quote::FactoryBuildPowerObligationV1>,
    >,
    WorldFieldOverlayErrorV1,
> {
    tx.contribute(|tx| {
        if !parent.uses_budget(ctx.values.memory) || parent.class != MemoryClass::Ordinary {
            return Err(invalid().into());
        }
        selected::<FactoryConstructionReceiptsFieldV1, _, _, _, _, _>(
            SelectedKey::ConstructionReceipt(id),
            &parent.factory_construction_receipts,
            roots,
            tx,
            ctx,
            scan,
            work,
            guard,
            live,
        )
    })
}
/// Progress is a required unit record. Its own reservations remain alive while
/// callers inspect the stage/counters or prepare a two-field update.
#[allow(clippy::too_many_arguments)]
pub fn recover_current_factory_industry_progress_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    parent: &ReservedValue<WorldNativeFieldsQualificationV1>,
    roots: &RootSetV1,
    tx: &mut ExecutionTransaction<'_, M>,
    ctx: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut WorldNativeSelectedReadWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<
    ReservedValue<CurrentFactoryNativeRecordV1<crate::IndustryProgressState>>,
    WorldFieldOverlayErrorV1,
> {
    tx.contribute(|tx| {
        if !parent.uses_budget(ctx.values.memory) || parent.class != MemoryClass::Ordinary {
            return Err(invalid().into());
        }
        selected::<IndustryProgressFieldV1, _, _, _, _, _>(
            SelectedKey::Progress,
            &parent.industry_progress,
            roots,
            tx,
            ctx,
            scan,
            work,
            guard,
            live,
        )
    })
}
