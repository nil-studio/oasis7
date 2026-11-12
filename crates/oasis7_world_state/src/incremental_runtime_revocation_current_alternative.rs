//! Indexed Current member observation; never a signed admission permit.
use super::*;
use crate::{
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
    incremental_inventory_value_read::InventoryValueReader,
};
use oasis7_formal_protocol::incremental_execution::{
    ExecutionFence, ExecutionTransaction, PreparedGuard,
};
use oasis7_proto::memory_budget::ReservedValue;

pub struct CurrentSafeAlternativeRecordV1 {
    lineage: String,
    record: RevocationRecoveredV1<serde_json::Value>,
    member_root: oasis7_formal_protocol::incremental_state::StateRootV1,
    guard: PreparedGuard,
}
impl CurrentSafeAlternativeRecordV1 {
    pub fn lineage(&self) -> &str {
        &self.lineage
    }
    pub fn record(&self) -> &serde_json::Value {
        self.record.value()
    }
    /// Any intervening transaction revision requires a fresh Current read.
    pub fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        live: &ExecutionFence,
    ) -> Result<(), WorldFieldOverlayErrorV1> {
        tx.validate_guard(&self.guard, live)?;
        Ok(())
    }
}

#[allow(clippy::too_many_arguments)]
pub fn read_current_safe_alternative_for_action_v1<M, R, E, C, P, V, ER, CR>(
    action_id: u64,
    qualification: &ReservedValue<RuntimeRevocationQualificationV1>,
    current: &RuntimeRevocationCurrentV1,
    overlay: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    read: &mut WorldFieldReadContextV1<'_, P, V, ER, CR>,
    scan: &mut StateScanBudget,
    work: &mut RevocationReadWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<Option<ReservedValue<CurrentSafeAlternativeRecordV1>>, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    ER: FnMut() -> Option<ValueTreeError>,
    CR: FnMut() -> bool,
{
    overlay.transaction.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        if overlay.values.class != MemoryClass::Ordinary
            || read.values.class != MemoryClass::Ordinary
            || qualification.allocation_class() != MemoryClass::Ordinary
            || !qualification.uses_budget(overlay.values.memory)
            || !tx.uses_budget(overlay.values.memory)
            || !read.values.memory.same_pool(overlay.values.memory)
        {
            return Err(invalid().into());
        }
        qualification.validate_digest(&tx.parent_fence().parent_roots)?;
        read.check()?;
        if (overlay.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        if let Some(error) = (overlay.take_state_error)() {
            return Err(error.into());
        }
        // Covers the bounded actual outer reference even on an index miss.
        let scratch = overlay
            .values
            .memory
            .reserve(8192, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let body = {
            let mut records = InventoryValueReader {
                values: &mut overlay.values,
                key_work: &mut *overlay.key_work,
                semantic_bytes: overlay.semantic_bytes,
                record_bytes: overlay.record_bytes,
                cancelled: &mut overlay.cancelled,
            };
            super::current_identity::current_body(tx, &mut records, &mut overlay.take_state_error)?
        };
        current.check(tx, &body, overlay.values.memory)?;
        let selected = if let Some(lineage) = current.safe_alternative_lineage_for_action(action_id)
        {
            let bytes = lineage
                .len()
                .checked_mul(2)
                .and_then(|n| n.checked_add(2048))
                .and_then(|n| n.checked_add(std::mem::size_of::<CurrentSafeAlternativeRecordV1>()))
                .and_then(|n| n.checked_add(64))
                .ok_or_else(invalid)?;
            let fee = overlay
                .values
                .memory
                .reserve(bytes, MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?;
            let row = super::current_identity::map_entry(
                &body,
                "safe_alternatives",
                lineage,
                read,
                scan,
                work,
            )?
            .ok_or_else(invalid)?;
            let ColumnCellV1::Root(root) = &row.cell else {
                return Err(invalid().into());
            };
            let record = crate::incremental_typed_record_codec::recover_typed_record_body_v1::<
                serde_json::Value,
                _,
                _,
                _,
                _,
            >(root, &profile(), read, scan, work)?;
            if record.value()["authorization"]["action_id"].as_u64() != Some(action_id) {
                return Err(invalid().into());
            }
            Some((fee, lineage.to_owned(), record, root.clone()))
        } else {
            None
        };
        drop(scratch);
        read.check()?;
        if (overlay.cancelled)() || (read.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        if let Some(error) = (overlay.take_state_error)() {
            return Err(error.into());
        }
        if let Some(error) = (read.take_state_error)() {
            return Err(error.into());
        }
        tx.validate_guard(&tx.guard(), live)?;
        Ok(selected.map(|(fee, lineage, record, member_root)| {
            fee.retain(CurrentSafeAlternativeRecordV1 {
                lineage,
                member_root,
                record,
                guard: tx.guard(),
            })
        }))
    })
}

#[path = "incremental_runtime_revocation_safe_alternative_cas.rs"]
mod cas;
pub use cas::replace_current_safe_alternative_record_v1;
