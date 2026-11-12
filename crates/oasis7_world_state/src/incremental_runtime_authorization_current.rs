//! Fixed Current selectors for the original audit/invocation record codecs.
//! The returned paid child is data, not an authorization or execution permit.
use super::*;
use crate::{
    incremental_field_overlay::{WorldFieldOverlayErrorV1, lookup},
    incremental_inventory_value_read::InventoryValueReader,
};
use oasis7_formal_protocol::incremental_execution::{
    ExecutionFence, ExecutionTransaction, PreparedGuard,
};

/// Select by the original outer map key, never by an embedded receipt or
/// invocation id. The original RecordRoot schema/owner binding remains shared
/// with qualification and mutation; no second decoder is introduced.
#[allow(clippy::too_many_arguments)]
pub fn open_current_authorization_record_body_v1<M, R, E, C>(
    parent: &ReservedValue<AuthorizationRecordFamiliesQualificationV1>,
    roots: &RootSetV1,
    kind: AuthorizationBodyKindV1,
    original_key: &str,
    transaction: &mut ExecutionTransaction<'_, M>,
    read: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<Option<ReservedValue<StateRootV1>>, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    transaction.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        let _metadata = read
            .values
            .memory
            .reserve(4096, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        parent.validate_binding(roots)?;
        if parent.class != MemoryClass::Ordinary
            || parent.reserved_bytes() < 512
            || !parent.uses_budget(read.values.memory)
            || read.values.class != MemoryClass::Ordinary
            || !tx.uses_budget(read.values.memory)
            || tx.parent_fence().parent_roots != roots.digest().map_err(|_| invalid())?
            || read.root != roots.state_root(StateNamespaceV1::Authorization)
        {
            return Err(invalid().into());
        }
        read.check()?;
        let index = match kind {
            AuthorizationBodyKindV1::Audit => 0,
            AuthorizationBodyKindV1::Invocation => 1,
        };
        let field = descriptor(index);
        let (entry, value) = {
            let metadata = WorldFieldKeyV1::new(field, WorldFieldRecordKindV1::Metadata, None)
                .map_err(ValueTreeError::Invalid)?;
            let mut records = InventoryValueReader {
                values: &mut read.values,
                key_work: &mut *read.key_work,
                semantic_bytes: read.semantic_bytes,
                record_bytes: read.record_bytes,
                cancelled: &mut read.cancelled,
            };
            let meta = lookup(tx, &metadata, &mut records, &mut read.take_state_error)?
                .ok_or_else(invalid)?;
            records.presence(&meta, &metadata, 0)?;
            let (entry, _semantic) =
                records.key(field, WorldFieldRecordKindV1::Entry, &original_key)?;
            let value = lookup(tx, &entry, &mut records, &mut read.take_state_error)?;
            (entry, value)
        };
        let Some(value) = value else {
            return Ok(None);
        };
        // Keep the original child decoder's temporary root and this returned
        // root allocation admitted together. The fixed RecordRoot wire bounds
        // both, including the root's owned node digest.
        let fee = read
            .values
            .memory
            .reserve(1024, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let root = child(kind, &entry, &value, read)?;
        read.check()?;
        tx.validate_guard(&tx.guard(), live)?;
        Ok(Some(fee.retain(root)))
    })
}
