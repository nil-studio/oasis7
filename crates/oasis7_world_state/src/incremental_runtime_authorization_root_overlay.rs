//! Exact Current CAS for the original authorization-root Text field.
//! Inputs are data, never evidence that the seven-source digest is correct.
use super::*;
use crate::{
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1, lookup},
    incremental_inventory_value_read::InventoryValueReader,
    incremental_record_value_read::{RecordValueV1, read_record_value_v1},
};
use oasis7_formal_protocol::incremental_execution::{ExecutionFence, PreparedGuard};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};

#[allow(clippy::too_many_arguments)]
pub fn replace_current_authorization_root_v1<M, R, E, C, W, D>(
    expected: &[u8; 64],
    after: &[u8; 64],
    qualified: &ReservedValue<RuntimeAuthorizationRootQualificationV1>,
    overlay: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, D>,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<bool, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeWriter,
    D: FnMut() -> bool,
{
    let error = &mut overlay.take_state_error;
    let cancel = &mut overlay.cancelled;
    overlay.transaction.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        if overlay.values.class != MemoryClass::Ordinary
            || qualified.allocation_class() != MemoryClass::Ordinary
            || !qualified.uses_budget(write.memory)
            || !overlay.values.memory.same_pool(write.memory)
            || !tx.uses_budget(write.memory)
        {
            return Err(ValueTreeError::Invalid("authorization root CAS pool/class").into());
        }
        qualified.validate_digest(&tx.parent_fence().parent_roots)?;
        if !expected
            .iter()
            .chain(after)
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(b))
        {
            return Err(ValueTreeError::Invalid("authorization root CAS SHA256 text").into());
        }
        let _phase = write
            .memory
            .reserve(16 << 10, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let key = {
            let mut stopped = || cancel() || (write.cancelled)();
            let mut records = InventoryValueReader {
                values: &mut overlay.values,
                key_work: &mut *overlay.key_work,
                semantic_bytes: overlay.semantic_bytes,
                record_bytes: overlay.record_bytes,
                cancelled: &mut stopped,
            };
            let descriptor = authorization_root_descriptor_v1();
            let meta = WorldFieldKeyV1::new(descriptor, WorldFieldRecordKindV1::Metadata, None)
                .map_err(ValueTreeError::Invalid)?;
            let meta_root = lookup(tx, &meta, &mut records, error)?.ok_or(
                ValueTreeError::Invalid("authorization root metadata missing"),
            )?;
            records.presence(&meta_root, &meta, 0)?;
            let key = records.key(descriptor, WorldFieldRecordKindV1::Entry, &())?;
            let value = lookup(tx, &key.0, &mut records, error)?.ok_or(ValueTreeError::Invalid(
                "authorization root Current missing",
            ))?;
            let mut actual = [0u8; 64];
            let decoded = read_record_value_v1(
                &value,
                &key,
                &mut records,
                64,
                &mut &mut actual[..],
                authorization_root_read_profile_v1(),
            )?;
            if !matches!(decoded, RecordValueV1::Text { utf8_bytes: 64 }) || actual != *expected {
                return Err(
                    ValueTreeError::Invalid("authorization root Current CAS mismatch").into(),
                );
            }
            key.0
        };
        if cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        if let Some(e) = error() {
            return Err(e.into());
        }
        if expected == after {
            return Ok(false);
        }
        // Exact original Text/member codec. The fixed kernel above proves the
        // expected Current value, not merely the qualified parent value.
        let text = std::str::from_utf8(after)
            .map_err(|_| ValueTreeError::Invalid("authorization root UTF8"))?;
        let row = prepare_authorization_root_value_v1(text, write)?;
        if row.field_key != key {
            return Err(ValueTreeError::Invalid("authorization root CAS key").into());
        }
        if cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        tx.write(
            StateNamespaceV1::Authorization,
            key.as_bytes(),
            Some(row.value_root),
        )?;
        if cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(true)
    })
}
