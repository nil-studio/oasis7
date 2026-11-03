use super::*;
use crate::{
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
    incremental_inventory_value_read::InventoryValueReader,
    incremental_record_tree::{self, RecordTreeWritePolicyV1},
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFence, PreparedGuard},
    incremental_state::{MerkleNodeReader, StatePageWriter},
    incremental_value::ValueNodeReader,
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
pub struct FixedEconomyCellChangeV1<'a> {
    pub column: FixedEconomyColumnV1,
    pub qualification: &'a ReservedValue<FixedEconomyRecordQualificationV1>,
    pub expected: FixedEconomyValueV1,
    pub after: FixedEconomyValueV1,
    pub max_pending_node_bytes: usize,
}
pub fn change_qualified_fixed_economy_cell_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: StatePageWriter,
    CW: FnMut() -> bool,
>(
    change: FixedEconomyCellChangeV1<'_>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<Option<ReservedValue<FixedEconomyRecordQualificationV1>>, WorldFieldOverlayErrorV1> {
    let cancel = &mut context.cancelled;
    let error = &mut context.take_state_error;
    context.transaction.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        change.expected.payload(change.column)?;
        let payload = change.after.payload(change.column)?;
        if !tx.uses_budget(write.memory) {
            return Err(read::invalid().into());
        }
        let _operation = write
            .memory
            .reserve(32 << 10, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let resolved = {
            let mut combined = || cancel() || (write.cancelled)();
            let mut records = InventoryValueReader {
                values: &mut context.values,
                key_work: &mut *context.key_work,
                semantic_bytes: context.semantic_bytes,
                record_bytes: context.record_bytes,
                cancelled: &mut combined,
            };
            overlay::resolve(
                FixedEconomyCellRequestV1 {
                    column: change.column,
                    qualification: change.qualification,
                },
                tx,
                &mut records,
                error,
            )?
        };
        let expected = resolved.value == change.expected;
        let unchanged = change.expected == change.after;
        // The getter stops after shared resolution. Both comparisons precede this
        // extra update return boundary, independently testable as getter N + 1.
        if cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        if !expected {
            return Err(ValueTreeError::Invalid("fixed economy expected value").into());
        }
        if unchanged {
            return Ok(None);
        }
        let field = change.column.field();
        let row = {
            let mut combined = MemberRecordWriteContext {
                sink: &mut *write.sink,
                memory: write.memory,
                io: &mut *write.io,
                key_work: &mut *write.key_work,
                limits: write.limits,
                cancelled: || cancel() || (write.cancelled)(),
            };
            write_cell(field, Some(change.column), payload, &mut combined)?
        };
        if row.field_key != resolved.cell_key.0 {
            return Err(read::invalid().into());
        }
        let child = incremental_record_tree::edit(
            resolved.child,
            &resolved.cell_key.0,
            (Some(resolved.cell_root), Some(row.value_root)),
            &mut context.values,
            write,
            cancel,
            RecordTreeWritePolicyV1 {
                max_pending_node_bytes: change.max_pending_node_bytes,
                labels: genesis::LABELS,
            },
        )?;
        let root = {
            let mut combined = MemberRecordWriteContext {
                sink: &mut *write.sink,
                memory: write.memory,
                io: &mut *write.io,
                key_work: &mut *write.key_work,
                limits: write.limits,
                cancelled: || cancel() || (write.cancelled)(),
            };
            genesis::write_root(field, child.clone(), &mut combined)?
        };
        if root.field_key != resolved.outer {
            return Err(read::invalid().into());
        }
        if cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        tx.write(
            field.namespace(),
            resolved.outer.as_bytes(),
            Some(root.value_root),
        )?;
        let result = closure::mint(field, child, write.memory, MemoryClass::Ordinary)?;
        if cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(Some(result))
    })
}
