//! Actual Current JSON grant selection, with the original grant/JSON codecs.
//! Recovered data conveys no capability or complete-family qualification.
use super::*;
use crate::{
    incremental_field_overlay::{WorldFieldOverlayErrorV1, lookup},
    incremental_inventory_value_read::InventoryValueReader,
    incremental_runtime_authorization_grants::qualify_authorization_grant_value_v1,
};
use oasis7_formal_protocol::incremental_execution::{
    ExecutionFence, ExecutionTransaction, PreparedGuard,
};
use oasis7_proto::memory_budget::MemoryClass;

#[allow(clippy::too_many_arguments)]
pub fn recover_current_authorization_grant_v1<M, R, E, C>(
    parent: &ReservedValue<AuthorizationGrantsQualificationV1>,
    roots: &RootSetV1,
    original_key: &str,
    transaction: &mut ExecutionTransaction<'_, M>,
    read: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut JsonReadWorkV1,
    max_selected_heap: usize,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<Option<ReservedValue<serde_json::Value>>, WorldFieldOverlayErrorV1>
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
        let profile = authorization_grant_profile_v1();
        let (key, semantic_bytes, current) = {
            let metadata =
                WorldFieldKeyV1::new(profile.field, WorldFieldRecordKindV1::Metadata, None)
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
            let (key, semantic_bytes) =
                records.key(profile.field, WorldFieldRecordKindV1::Entry, &original_key)?;
            let current = lookup(tx, &key, &mut records, &mut read.take_state_error)?;
            (key, semantic_bytes, current)
        };
        let Some(current) = current else {
            return Ok(None);
        };
        let proof =
            qualify_authorization_grant_value_v1(&key, semantic_bytes, &current, read, work)?;
        let (value, paid) = crate::incremental_json_records::materialize_records(
            profile,
            &proof,
            &mut read.records(),
            work,
            max_selected_heap,
        )?;
        read.check()?;
        tx.validate_guard(&tx.guard(), live)?;
        Ok(Some(paid.retain(value)))
    })
}
