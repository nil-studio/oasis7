//! The actual Current revoked-id set, decoded by the original closed codec.
//! This is one selected field, not a complete revocation recovery/qualification.
use super::current_identity;
use super::*;
use crate::{
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
    incremental_inventory_value_read::InventoryValueReader,
    incremental_typed_record_diff::{ChangeDecoder, point_typed_column_v1},
};
use oasis7_formal_protocol::incremental_execution::{
    ExecutionFence, ExecutionTransaction, PreparedGuard,
};
use oasis7_proto::memory_budget::{MemoryBudget, ReservedValue};
use std::collections::BTreeSet;
pub struct CurrentBootstrapRevokedSetV1 {
    value: RevocationRecoveredV1<BTreeSet<String>>,
    body: StateRootV1,
    guard: PreparedGuard,
}
impl CurrentBootstrapRevokedSetV1 {
    pub fn value(&self) -> &BTreeSet<String> {
        self.value.value()
    }
    pub fn body(&self) -> &StateRootV1 {
        &self.body
    }
    pub fn validate_current<M: MerkleNodeReader>(
        &self,
        current: &RuntimeRevocationCurrentV1,
        tx: &ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        live: &ExecutionFence,
    ) -> Result<(), WorldFieldOverlayErrorV1> {
        tx.validate_guard(&self.guard, live)?;
        current.check(tx, &self.body, memory)?;
        if current.body() != &self.body {
            return Err(invalid().into());
        }
        Ok(())
    }
}
#[allow(clippy::too_many_arguments)]
pub fn read_current_bootstrap_revoked_set_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    ER: FnMut() -> Option<ValueTreeError>,
    CR: FnMut() -> bool,
>(
    qualification: &ReservedValue<RuntimeRevocationQualificationV1>,
    current: &RuntimeRevocationCurrentV1,
    overlay: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    read: &mut WorldFieldReadContextV1<'_, P, V, ER, CR>,
    scan: &mut StateScanBudget,
    work: &mut RevocationReadWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<ReservedValue<CurrentBootstrapRevokedSetV1>, WorldFieldOverlayErrorV1> {
    overlay.transaction.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        qualification.validate_digest(&tx.parent_fence().parent_roots)?;
        current.check(tx, current.body(), overlay.values.memory)?;
        if qualification.allocation_class() != MemoryClass::Ordinary
            || !qualification.uses_budget(overlay.values.memory)
            || overlay.values.class != MemoryClass::Ordinary
            || read.values.class != MemoryClass::Ordinary
            || !read.values.memory.same_pool(overlay.values.memory)
        {
            return Err(invalid().into());
        }
        let fee = overlay
            .values
            .memory
            .reserve(
                std::mem::size_of::<CurrentBootstrapRevokedSetV1>() + 8192,
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        let body = {
            let mut records = InventoryValueReader {
                values: &mut overlay.values,
                key_work: &mut *overlay.key_work,
                semantic_bytes: overlay.semantic_bytes,
                record_bytes: overlay.record_bytes,
                cancelled: &mut overlay.cancelled,
            };
            current_identity::current_body(tx, &mut records, &mut overlay.take_state_error)?
        };
        if &body != current.body() {
            return Err(invalid().into());
        }
        let selected = {
            let profile = profile();
            let trees_remaining = work.max_retained_trees;
            let mut decoder = ChangeDecoder {
                ctx: read,
                scan,
                work,
                profile,
                trees_remaining,
                tree_depth: 0,
            };
            point_typed_column_v1(&profile, &body, "revoked_grant_ids", &mut decoder, 8192)?
        };
        let ColumnCellV1::Root(root) = &selected.cell else {
            return Err(invalid().into());
        };
        let value = crate::incremental_typed_record_codec::recover_typed_record_body_v1(
            root,
            &profile(),
            read,
            scan,
            work,
        )?;
        read.check()?;
        if (overlay.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        if let Some(error) = (overlay.take_state_error)() {
            return Err(error.into());
        }
        if let Some(error) = (read.take_state_error)() {
            return Err(error.into());
        }
        tx.validate_guard(&tx.guard(), live)?;
        Ok(fee.retain(CurrentBootstrapRevokedSetV1 {
            value,
            body,
            guard: tx.guard(),
        }))
    })
}
