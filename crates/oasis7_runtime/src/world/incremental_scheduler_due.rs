//! Bounded snapshot selection in the original instance-ID execution order.
use super::*;
use oasis7_formal_protocol::{
    incremental_execution::PreparedGuard,
    incremental_schedule::{
        SCHEDULE_ROOT_BYTES, ScheduleScanBudget, ScheduleScanContext, ScheduleScanError,
    },
};
use oasis7_proto::memory_budget::ReservedValue;

#[derive(Debug, Clone, Copy)]
pub struct DueModuleBatchLimitsV1 {
    pub entries: usize,
    pub instance_bytes: usize,
}
struct Row {
    instance: ReservedValue<String>,
    wake: u64,
}
/// Query output only; no invocation or complete-index qualification. The caller
/// validates this post-read guard against its live fence before consumption.
/// The charge follows retained instance strings and metadata until final drop.
pub struct DueModuleBatchV1 {
    rows: ReservedValue<Vec<Row>>,
    guard: PreparedGuard,
}
impl DueModuleBatchV1 {
    pub fn iter(&self) -> impl ExactSizeIterator<Item = (&str, u64)> {
        self.rows
            .iter()
            .map(|row| (row.instance.as_str(), row.wake))
    }
    pub fn guard(&self) -> &PreparedGuard {
        &self.guard
    }
}
impl World {
    /// Select from the current overlay. A separately qualified complete
    /// scalar/index bijection remains required; target checks cannot prove that
    /// an omitted scalar is absent. No execution callback or World mutation runs.
    pub fn read_iws_due_module_batch<R, V, W, C>(
        &self,
        txn: &mut ExecutionTransaction<'_, R>,
        values: &V,
        context: &mut BodyMutationContext<'_, W, C>,
        scan_budget: &mut ScheduleScanBudget,
        limits: DueModuleBatchLimitsV1,
    ) -> Result<DueModuleBatchV1, BodyMutationError>
    where
        R: MerkleNodeReader,
        V: ValueNodeReader,
        C: FnMut() -> bool,
    {
        txn.contribute(|txn| {
            check_clock(self.state.time, txn, values, context)?;
            let bytes = read_fixed::<SCHEDULE_ROOT_BYTES, _, _, _, _>(
                txn,
                Ns::Scheduler,
                MODULE_WAKE_INDEX_KEY_V1,
                values,
                context,
            )?;
            let root = ScheduleIndexV1::from_canonical_bytes(&bytes)?;
            let metadata = limits
                .entries
                .checked_mul(std::mem::size_of::<Row>())
                .and_then(|bytes| bytes.checked_add(1024))
                .ok_or(BodyMutationError::Invalid("module due metadata size"))?;
            let charge = context
                .memory
                .reserve(metadata, MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?;
            let mut rows = Vec::new();
            rows.try_reserve_exact(limits.entries)
                .map_err(|_| ValueTreeError::ResourceLimited("module due metadata allocation"))?;
            if rows
                .capacity()
                .checked_mul(std::mem::size_of::<Row>())
                .is_none_or(|bytes| bytes > metadata)
            {
                return Err(ValueTreeError::ResourceLimited("module due metadata capacity").into());
            }
            let memory = context.memory;
            let mut id_bytes = limits.instance_bytes;
            {
                let reader =
                    index::IndexReader::new(values, memory, MemoryClass::Ordinary, context.io);
                let mut scan = ScheduleScanContext {
                    source: &reader,
                    memory,
                    class: MemoryClass::Ordinary,
                    io: scan_budget,
                };
                let result = scan.visit_due(
                    &root,
                    self.state.time,
                    |wake, instance| {
                        if rows.len() == limits.entries {
                            return Err(BodyMutationError::Value(ValueTreeError::ResourceLimited(
                                "module due entries",
                            )));
                        }
                        id_bytes = id_bytes.checked_sub(instance.len()).ok_or(
                            BodyMutationError::Value(ValueTreeError::ResourceLimited(
                                "module due instance bytes",
                            )),
                        )?;
                        let bound = instance
                            .len()
                            .checked_add(1024)
                            .ok_or(BodyMutationError::Invalid("module due instance size"))?;
                        let owned = memory
                            .reserve(bound, MemoryClass::Ordinary)
                            .map_err(ValueTreeError::Memory)?;
                        let mut id = String::new();
                        id.try_reserve_exact(instance.len()).map_err(|_| {
                            ValueTreeError::ResourceLimited("module due instance allocation")
                        })?;
                        if id.capacity() > bound {
                            return Err(BodyMutationError::Value(ValueTreeError::ResourceLimited(
                                "module due instance capacity",
                            )));
                        }
                        id.push_str(instance);
                        rows.push(Row {
                            instance: owned.retain(id),
                            wake,
                        });
                        Ok(())
                    },
                    &mut context.cancelled,
                );
                match result {
                    Ok(count) if count == rows.len() as u64 => {}
                    Ok(_) => return Err(BodyMutationError::Invalid("module due output count")),
                    Err(ScheduleScanError::Tree(error)) => {
                        return reader.result(Err(ExecutionError::Tree(error)));
                    }
                    Err(ScheduleScanError::Visitor(error)) => return Err(error),
                    Err(ScheduleScanError::Invalid(reason)) => {
                        return Err(BodyMutationError::Invalid(reason));
                    }
                    Err(ScheduleScanError::Memory(error)) => {
                        return Err(ValueTreeError::Memory(error).into());
                    }
                    Err(ScheduleScanError::ResourceLimited) => {
                        return Err(ValueTreeError::ResourceLimited("module due scan").into());
                    }
                    Err(ScheduleScanError::Cancelled) => {
                        return Err(ValueTreeError::Cancelled.into());
                    }
                }
            }
            rows.sort_unstable_by(|left, right| {
                left.instance.as_str().cmp(right.instance.as_str())
            });
            if rows
                .windows(2)
                .any(|pair| pair[0].instance.as_str() == pair[1].instance.as_str())
            {
                return Err(BodyMutationError::Invalid("module due duplicate instance"));
            }
            for row in &rows {
                cancelled(context)?;
                let key =
                    ModuleWakeKeyV1::new(&row.instance).map_err(BodyMutationError::Invalid)?;
                if read_wake(key.as_bytes(), txn, values, context)? != Some(row.wake) {
                    return Err(BodyMutationError::Invalid(
                        "module due index/scalar mismatch",
                    ));
                }
            }
            cancelled(context)?;
            Ok(DueModuleBatchV1 {
                rows: charge.retain(rows),
                guard: txn.guard(),
            })
        })
    }
}
