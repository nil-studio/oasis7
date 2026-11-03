//! Current-overlay point changes in one shared transaction contribution.
use crate::{
    incremental_field_member::MemberRecordWriteContext,
    incremental_inventory_change::{
        InventoryChangeError, InventoryChangeRefV1, prepare_resolved_inventory_change_v1,
    },
    incremental_inventory_genesis::InventoryWriteSink,
    incremental_inventory_overlay_read::{
        InventoryOverlayReadContext, InventoryOverlayReadError, resolve_point,
    },
    incremental_inventory_transaction::{InventoryContributionError, apply_inventory_output_v1},
    incremental_inventory_value_read::InventoryValueReader,
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionError, ExecutionFence, PreparedGuard},
    incremental_state::{MerkleNodeReader, StateWriteV1},
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};

#[derive(Debug)]
pub enum InventoryOverlayChangeError {
    Execution(ExecutionError),
    Read(InventoryOverlayReadError),
    Change(InventoryChangeError),
    Contribution(InventoryContributionError),
}
impl From<ExecutionError> for InventoryOverlayChangeError {
    fn from(error: ExecutionError) -> Self {
        Self::Execution(error)
    }
}
impl From<InventoryOverlayReadError> for InventoryOverlayChangeError {
    fn from(error: InventoryOverlayReadError) -> Self {
        Self::Read(error)
    }
}
impl From<InventoryChangeError> for InventoryOverlayChangeError {
    fn from(error: InventoryChangeError) -> Self {
        Self::Change(error)
    }
}
impl From<InventoryContributionError> for InventoryOverlayChangeError {
    fn from(error: InventoryContributionError) -> Self {
        Self::Contribution(error)
    }
}
impl std::fmt::Display for InventoryOverlayChangeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Execution(error) => error.fmt(f),
            Self::Read(error) => error.fmt(f),
            Self::Change(error) => error.fmt(f),
            Self::Contribution(error) => error.fmt(f),
        }
    }
}
impl std::error::Error for InventoryOverlayChangeError {}

#[derive(Default)]
struct PointWrites {
    rows: [Option<ReservedValue<StateWriteV1>>; 2],
    count: usize,
}
impl InventoryWriteSink for PointWrites {
    fn put_write(&mut self, write: ReservedValue<StateWriteV1>) -> Result<(), ValueTreeError> {
        let slot = self
            .rows
            .get_mut(self.count)
            .ok_or(ValueTreeError::Invalid("inventory overlay write count"))?;
        *slot = Some(write);
        self.count += 1;
        Ok(())
    }
}

impl<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
> InventoryOverlayReadContext<'_, '_, '_, M, R, E, C>
{
    /// Apply an owner-decided point value against this transaction's current view.
    /// Every error poisons the complete transaction; no head is published here.
    pub fn change<W: ValueNodeWriter, CW: FnMut() -> bool>(
        &mut self,
        change: InventoryChangeRefV1<'_>,
        write: &mut MemberRecordWriteContext<'_, W, CW>,
        guard: &PreparedGuard,
        live: &ExecutionFence,
    ) -> Result<u8, InventoryOverlayChangeError> {
        let values = &mut self.values;
        let key_work = &mut *self.key_work;
        let semantic_bytes = self.semantic_bytes;
        let record_bytes = self.record_bytes;
        let cancelled_read = &mut self.cancelled;
        let take_state_error = &mut self.take_state_error;
        self.transaction.contribute(|transaction| {
            transaction.validate_guard(guard, live)?;
            if values.class != MemoryClass::Ordinary
                || !transaction.uses_budget(values.memory)
                || !transaction.uses_budget(write.memory)
            {
                return Err(
                    InventoryChangeError::Invalid("inventory overlay change pool/class").into(),
                );
            }
            if cancelled_read() || (write.cancelled)() {
                return Err(InventoryChangeError::Value(ValueTreeError::Cancelled).into());
            }
            // Admit the fixed resolved point, output and write slots before
            // constructing them; all remain local to this contribution.
            let _workspace = write
                .memory
                .reserve(4096, MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)
                .map_err(InventoryChangeError::from)?;
            let point = {
                let mut cancelled = || cancelled_read() || (write.cancelled)();
                let mut records = InventoryValueReader {
                    values: &mut *values,
                    key_work: &mut *key_work,
                    semantic_bytes,
                    record_bytes,
                    cancelled: &mut cancelled,
                };
                match &change {
                    InventoryChangeRefV1::Resource { key, .. } => resolve_point(
                        transaction,
                        &mut records,
                        take_state_error,
                        "resources",
                        key,
                        None,
                    )?,
                    InventoryChangeRefV1::Material { key, .. } => resolve_point(
                        transaction,
                        &mut records,
                        take_state_error,
                        "materials",
                        *key,
                        None,
                    )?,
                    InventoryChangeRefV1::LedgerMaterial {
                        ledger, material, ..
                    } => resolve_point(
                        transaction,
                        &mut records,
                        take_state_error,
                        "material_ledgers",
                        &(*ledger, *material),
                        Some(*ledger),
                    )?,
                }
            };
            let mut writes = PointWrites::default();
            let output = prepare_resolved_inventory_change_v1(
                change,
                point,
                write,
                &mut writes,
                &mut *cancelled_read,
            )?;
            let memory = write.memory;
            apply_inventory_output_v1(
                &output,
                writes.rows.iter().flatten(),
                transaction,
                memory,
                &mut || cancelled_read() || (write.cancelled)(),
            )?;
            Ok(output.records)
        })
    }
}
