//! Bootstrap applicability checks read the sole transaction, never parent absence.
use super::*;
use crate::incremental_json_records::JsonReadWorkV1;
pub fn require_current_delegation_grants_empty_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    work: &mut JsonReadWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<(), WorldFieldOverlayErrorV1> {
    let field = CognitionJsonFieldV1::DelegationGrants;
    context.transaction.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        if context.values.class != MemoryClass::Ordinary || !tx.uses_budget(context.values.memory) {
            return Err(ValueTreeError::Invalid("delegation overlay pool/class").into());
        }
        let _phase = context
            .values
            .memory
            .reserve(4096, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let mut records = InventoryValueReader {
            values: &mut context.values,
            key_work: &mut *context.key_work,
            semantic_bytes: context.semantic_bytes,
            record_bytes: context.record_bytes,
            cancelled: &mut context.cancelled,
        };
        let metadata =
            WorldFieldKeyV1::new(field.descriptor(), WorldFieldRecordKindV1::Metadata, None)
                .map_err(ValueTreeError::Invalid)?;
        let key = records.key(field.descriptor(), WorldFieldRecordKindV1::Entry, &())?;
        let meta = lookup(tx, &metadata, &mut records, &mut context.take_state_error)?;
        let entry = lookup(tx, &key.0, &mut records, &mut context.take_state_error)?;
        match (meta, entry) {
            (None, None) => {}
            (Some(meta), Some(entry)) => {
                records.presence(&meta, &metadata, 0)?;
                let child = read::outer(field, &key, &entry, &mut records)?;
                let proof = read::qualify_body_records(field, &child, &mut records, work)?;
                crate::incremental_json_records::require_empty_object_records_v1(
                    field.profile(),
                    proof.json(),
                    &mut records,
                    work,
                )?;
            }
            _ => return Err(ValueTreeError::Invalid("delegation partial current field").into()),
        }
        if (records.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        if let Some(error) = (context.take_state_error)() {
            return Err(error.into());
        }
        Ok(())
    })
}

#[cfg(test)]
#[path = "incremental_cognition_json_overlay_empty_tests.rs"]
mod tests;
