//! Existing primitive controls through the original shared transaction.
//! Optional-group lifecycle and whole Cognition acceptance are separate.
use crate::{
    incremental_cognition_fields::CognitionControlFieldV1,
    incremental_expected_bytes::Compare,
    incremental_field_member::*,
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1, lookup},
    incremental_inventory_value_read::InventoryValueReader,
    incremental_utf8_output::read_text_value,
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFence, ExecutionTransaction, PreparedGuard},
    incremental_state::{MerkleNodeReader, StatePageWriter},
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::MemoryClass;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CognitionControlValueV1<'a> {
    Text(&'a str),
    Unsigned(u64),
}
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("cognition control overlay")
}
fn valid(
    field: CognitionControlFieldV1,
    value: CognitionControlValueV1<'_>,
) -> Result<(), ValueTreeError> {
    let schema = match field {
        CognitionControlFieldV1::Schema => Some("cognition-persistence.v1"),
        CognitionControlFieldV1::JournalSchema => Some("cognition-journal.v1"),
        CognitionControlFieldV1::EconomySchema => Some("cognition-economy.v1"),
        _ => None,
    };
    match value {
        CognitionControlValueV1::Unsigned(_) if field.unsigned() => Ok(()),
        CognitionControlValueV1::Text(text)
            if !field.unsigned() && schema.is_none_or(|expected| expected == text) =>
        {
            Ok(())
        }
        _ => Err(invalid()),
    }
}
fn resolve<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    C: FnMut() -> bool,
    E: FnMut() -> Option<ValueTreeError>,
>(
    field: CognitionControlFieldV1,
    expected: CognitionControlValueV1<'_>,
    tx: &mut ExecutionTransaction<'_, M>,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    error: &mut E,
) -> Result<WorldFieldKeyV1, WorldFieldOverlayErrorV1> {
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    if records.values.class != MemoryClass::Ordinary || !tx.uses_budget(records.values.memory) {
        return Err(invalid().into());
    }
    valid(field, expected)?;
    let metadata = WorldFieldKeyV1::new(field.descriptor(), WorldFieldRecordKindV1::Metadata, None)
        .map_err(ValueTreeError::Invalid)?;
    let metadata_root = lookup(tx, &metadata, records, error)?.ok_or_else(invalid)?;
    records.presence(&metadata_root, &metadata, 0)?;
    let key = records.key(field.descriptor(), WorldFieldRecordKindV1::Entry, &())?;
    let root = lookup(tx, &key.0, records, error)?.ok_or_else(invalid)?;
    match expected {
        CognitionControlValueV1::Unsigned(value) => {
            if records.unsigned(&root, &key.0, key.1)? != value {
                return Err(invalid().into());
            }
        }
        CognitionControlValueV1::Text(text) => {
            let mut output = Compare::new(text.as_bytes());
            let bytes = read_text_value(
                &root,
                &key.0,
                records.values,
                MemberReadProfileV1 {
                    codec: MemberPayloadCodecV1::Text,
                    semantic_bytes: key.1,
                    limits: MemberRecordLimitsV1 {
                        semantic_bytes: records.semantic_bytes,
                        payload_bytes: records.record_bytes,
                        record_bytes: records.record_bytes,
                    },
                },
                &mut output,
                "cognition control UTF-8",
                &mut *records.cancelled,
            )?;
            if bytes != text.len() as u64 || !output.is_equal() {
                return Err(invalid().into());
            }
        }
    }
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(key.0)
}
pub fn check_cognition_control_overlay_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    field: CognitionControlFieldV1,
    expected: CognitionControlValueV1<'_>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
) -> Result<(), WorldFieldOverlayErrorV1> {
    context.transaction.contribute(|tx| {
        resolve(
            field,
            expected,
            tx,
            &mut InventoryValueReader {
                values: &mut context.values,
                key_work: &mut *context.key_work,
                semantic_bytes: context.semantic_bytes,
                record_bytes: context.record_bytes,
                cancelled: &mut context.cancelled,
            },
            &mut context.take_state_error,
        )?;
        Ok(())
    })
}
pub fn change_cognition_control_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: StatePageWriter,
    CW: FnMut() -> bool,
>(
    field: CognitionControlFieldV1,
    change: (CognitionControlValueV1<'_>, CognitionControlValueV1<'_>),
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<bool, WorldFieldOverlayErrorV1> {
    let cancel = &mut context.cancelled;
    let error = &mut context.take_state_error;
    context.transaction.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        if !tx.uses_budget(write.memory) {
            return Err(invalid().into());
        }
        let key = {
            let mut combined = || cancel() || (write.cancelled)();
            resolve(
                field,
                change.0,
                tx,
                &mut InventoryValueReader {
                    values: &mut context.values,
                    key_work: &mut *context.key_work,
                    semantic_bytes: context.semantic_bytes,
                    record_bytes: context.record_bytes,
                    cancelled: &mut combined,
                },
                error,
            )?
        };
        valid(field, change.1)?;
        let equal = change.0 == change.1;
        // Long text equality can race cancellation after the read's last poll.
        if cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        if equal {
            return Ok(false);
        }
        let payload = match change.1 {
            CognitionControlValueV1::Text(value) => MemberPayloadRefV1::Text(value),
            CognitionControlValueV1::Unsigned(value) => MemberPayloadRefV1::U64(value),
        };
        let row = write_member_record_v1(
            field.descriptor(),
            WorldFieldRecordKindV1::Entry,
            &(),
            payload,
            &mut MemberRecordWriteContext {
                sink: &mut *write.sink,
                memory: write.memory,
                io: &mut *write.io,
                key_work: &mut *write.key_work,
                limits: write.limits,
                cancelled: || cancel() || (write.cancelled)(),
            },
        )?;
        if row.field_key != key {
            return Err(invalid().into());
        }
        if cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        let namespace = WorldFieldKeyV1::from_bytes(key.as_bytes())
            .map_err(ValueTreeError::Invalid)?
            .namespace;
        tx.write(namespace, key.as_bytes(), Some(row.value_root))?;
        if cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(true)
    })
}
