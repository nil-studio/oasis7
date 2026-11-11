//! Exact Current factory deletion. This storage owner does not approve recycling.
//! Child of the original factory facade so its original container/grammar is reused.
use super::*;
use crate::{
    incremental_field_overlay::WorldFieldOverlayErrorV1,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_inventory_value_read::InventoryValueReader,
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFence, ExecutionTransaction, PreparedGuard, RootSetV1},
    incremental_state::{MerkleNodeReader, StateNamespaceV1},
    incremental_value::{ValueNodeReader, ValueReadContext},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass};
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("Current factory deletion binding")
}
/// Private construction requires the complete parent family and actual Current
/// child closure. Its expected outer root is inseparable from that observation.
pub struct PreparedCurrentFactoryDeleteV1 {
    id: String,
    key: WorldFieldKeyV1,
    value: [u8; 32],
    guard: PreparedGuard,
    memory: MemoryBudget,
}
impl PreparedCurrentFactoryDeleteV1 {
    pub fn factory_id(&self) -> &str {
        &self.id
    }
}
#[allow(clippy::too_many_arguments)]
pub fn prepare_current_factory_delete_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    id: &str,
    parent: &ReservedValue<FactoryFamilyQualificationV1>,
    roots: &RootSetV1,
    tx: &mut ExecutionTransaction<'_, M>,
    ctx: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
    work: &mut FactoryWorkBudgetV1,
    max_payload_bytes: u64,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<ReservedValue<PreparedCurrentFactoryDeleteV1>, WorldFieldOverlayErrorV1> {
    tx.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        ctx.check()?;
        let memory = ctx.values.memory;
        let _phase = memory
            .reserve(64 << 10, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let digest = roots.digest().map_err(|_| invalid())?;
        if ctx.values.class != MemoryClass::Ordinary
            || !tx.uses_budget(memory)
            || !parent.uses_budget(memory)
            || parent.allocation_class() != MemoryClass::Ordinary
            || digest != tx.parent_fence().parent_roots
            || ctx.root != roots.state_root(StateNamespaceV1::Business)
        {
            return Err(invalid().into());
        }
        parent.validate_binding(&digest)?;
        let fee = memory
            .reserve(
                std::mem::size_of::<PreparedCurrentFactoryDeleteV1>()
                    .checked_add(id.len())
                    .ok_or_else(invalid)?,
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        let before = container::fixed(FactoryRecordKeyV1::Factory(id), ctx)?;
        let current = {
            let mut records = InventoryValueReader {
                values: &mut ctx.values,
                key_work: &mut *ctx.key_work,
                semantic_bytes: ctx.semantic_bytes,
                record_bytes: ctx.record_bytes,
                cancelled: &mut ctx.cancelled,
            };
            container::overlay(
                FactoryRecordKeyV1::Factory(id),
                tx,
                &mut records,
                &mut ctx.take_state_error,
            )?
        };
        if before.key != current.key {
            return Err(invalid().into());
        }
        let child = current.child.as_ref().ok_or_else(invalid)?;
        let value = current.value_root.ok_or_else(invalid)?;
        {
            let mut context = FactoryTransitionReadContextV1 {
                values: ValueReadContext {
                    source: ctx.values.source,
                    memory,
                    class: MemoryClass::Ordinary,
                    io: &mut *ctx.values.io,
                },
                key_work: &mut *ctx.key_work,
                semantic_bytes: ctx.semantic_bytes,
                record_bytes: ctx.record_bytes,
                max_payload_bytes,
                cancelled: &mut ctx.cancelled,
            };
            crate::incremental_factory_records::validate_factory_lifecycle_child_delta_v1(
                before.child.as_ref(),
                child,
                &mut context,
                work,
            )?;
        }
        ctx.check()?;
        let tail = tx.guard();
        tx.validate_guard(&tail, live)?;
        Ok(fee.retain(PreparedCurrentFactoryDeleteV1 {
            id: id.to_owned(),
            key: current.key,
            value,
            guard: tail,
            memory: memory.clone(),
        }))
    })
}
/// Removes exactly the observed whole outer reference. Any failure poisons the
/// same transaction, including reuse after another read/write changed revision.
pub fn delete_prepared_current_factory_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    prepared: &ReservedValue<PreparedCurrentFactoryDeleteV1>,
    tx: &mut ExecutionTransaction<'_, M>,
    ctx: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
    live: &ExecutionFence,
) -> Result<(), WorldFieldOverlayErrorV1> {
    tx.contribute(|tx| {
        tx.validate_guard(&prepared.guard, live)?;
        ctx.check()?;
        if ctx.values.class != MemoryClass::Ordinary
            || !prepared.uses_budget(ctx.values.memory)
            || !prepared.memory.same_pool(ctx.values.memory)
            || !tx.uses_budget(ctx.values.memory)
        {
            return Err(invalid().into());
        }
        let current = {
            let mut records = InventoryValueReader {
                values: &mut ctx.values,
                key_work: &mut *ctx.key_work,
                semantic_bytes: ctx.semantic_bytes,
                record_bytes: ctx.record_bytes,
                cancelled: &mut ctx.cancelled,
            };
            crate::incremental_field_overlay::lookup(
                tx,
                &prepared.key,
                &mut records,
                &mut ctx.take_state_error,
            )?
        };
        if current != Some(prepared.value) {
            return Err(invalid().into());
        }
        ctx.check()?;
        tx.write(StateNamespaceV1::Business, prepared.key.as_bytes(), None)?;
        ctx.check()?;
        Ok(())
    })
}
