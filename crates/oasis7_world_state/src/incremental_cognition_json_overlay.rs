//! Exact existing scalar edits through the actual shared ExecutionTransaction.
//! New field creation and container membership edits require their own closure.
use crate::{
    incremental_cognition_json::{CognitionJsonFieldV1, write_root},
    incremental_cognition_json_read::{self as read, CognitionJsonEntryQualificationV1},
    incremental_field_member::*,
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1, lookup},
    incremental_inventory_value_read::InventoryValueReader,
    incremental_json_records::{
        JsonPathSegmentV1, JsonScalarChangeWorkV1, JsonScalarRefV1, JsonWriteWorkV1,
        advance_json_scalar_subtree_records_v1, prepare_json_scalar_path_change_records_v1,
    },
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFence, PreparedGuard},
    incremental_state::{MerkleNodeReader, StatePageWriter},
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
pub struct CognitionJsonScalarChangeV1<'a> {
    pub field: CognitionJsonFieldV1,
    pub qualification: &'a ReservedValue<CognitionJsonEntryQualificationV1>,
    pub path: &'a [JsonPathSegmentV1<'a>],
    pub change: (JsonScalarRefV1<'a>, JsonScalarRefV1<'a>),
}
pub struct CognitionJsonPointWorkV1 {
    pub point: JsonWriteWorkV1,
    pub changed: JsonScalarChangeWorkV1,
}
pub fn change_cognition_json_scalar_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: StatePageWriter,
    CW: FnMut() -> bool,
>(
    change: CognitionJsonScalarChangeV1<'_>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    work: &mut CognitionJsonPointWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<Option<ReservedValue<CognitionJsonEntryQualificationV1>>, WorldFieldOverlayErrorV1> {
    let cancel = &mut context.cancelled;
    let error = &mut context.take_state_error;
    context.transaction.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        if context.values.class != MemoryClass::Ordinary
            || !tx.uses_budget(context.values.memory)
            || !tx.uses_budget(write.memory)
            || !change.qualification.uses_budget(context.values.memory)
        {
            return Err(ValueTreeError::Invalid("cognition JSON overlay pool/class").into());
        }
        let _operation = write
            .memory
            .reserve(32 << 10, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let outer = {
            let mut combined = || cancel() || (write.cancelled)();
            current(
                change.field,
                change.qualification,
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
        let prepared = prepare_json_scalar_path_change_records_v1(
            change.field.profile(),
            change.qualification.json(),
            change.path,
            change.change,
            &mut InventoryValueReader {
                values: &mut context.values,
                key_work: &mut *context.key_work,
                semantic_bytes: context.semantic_bytes,
                record_bytes: context.record_bytes,
                cancelled: cancel,
            },
            write,
            &mut work.point,
        )?;
        if !prepared.changed() {
            return Ok(None);
        }
        if cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        let body = {
            let mut combined = || cancel() || (write.cancelled)();
            advance_json_scalar_subtree_records_v1(
                change.field.profile(),
                change.qualification.json(),
                prepared.after(),
                &mut InventoryValueReader {
                    values: &mut context.values,
                    key_work: &mut *context.key_work,
                    semantic_bytes: context.semantic_bytes,
                    record_bytes: context.record_bytes,
                    cancelled: &mut combined,
                },
                &mut work.changed,
            )?
        };
        let row = write_root(
            change.field,
            prepared.after().clone(),
            &mut MemberRecordWriteContext {
                sink: &mut *write.sink,
                memory: write.memory,
                io: &mut *write.io,
                key_work: &mut *write.key_work,
                limits: write.limits,
                cancelled: || cancel() || (write.cancelled)(),
            },
        )?;
        if row.field_key != outer {
            return Err(ValueTreeError::Invalid("cognition JSON overlay output key").into());
        }
        if cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        let namespace = WorldFieldKeyV1::from_bytes(outer.as_bytes())
            .map_err(ValueTreeError::Invalid)?
            .namespace;
        tx.write(namespace, outer.as_bytes(), Some(row.value_root))?;
        let output = read::mint(change.field, body, write.memory, MemoryClass::Ordinary)?;
        if cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(Some(output))
    })
}

pub struct CognitionJsonScalarReadV1<'a> {
    pub field: CognitionJsonFieldV1,
    pub qualification: &'a ReservedValue<CognitionJsonEntryQualificationV1>,
    pub path: &'a [JsonPathSegmentV1<'a>],
    pub expected: JsonScalarRefV1<'a>,
}
pub fn check_cognition_json_scalar_overlay_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    request: CognitionJsonScalarReadV1<'_>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    work: &mut crate::incremental_json_records::JsonScalarPathReadWorkV1,
) -> Result<(), WorldFieldOverlayErrorV1> {
    context.transaction.contribute(|tx| {
        let _operation = context
            .values
            .memory
            .reserve(32 << 10, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let mut records = InventoryValueReader {
            values: &mut context.values,
            key_work: &mut *context.key_work,
            semantic_bytes: context.semantic_bytes,
            record_bytes: context.record_bytes,
            cancelled: &mut context.cancelled,
        };
        current(
            request.field,
            request.qualification,
            tx,
            &mut records,
            &mut context.take_state_error,
        )?;
        crate::incremental_json_records::check_json_scalar_path_expected_records_v1(
            request.field.profile(),
            request.qualification.json(),
            request.path,
            request.expected,
            &mut records,
            work,
        )?;
        Ok(())
    })
}
fn current<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    field: CognitionJsonFieldV1,
    proof: &ReservedValue<CognitionJsonEntryQualificationV1>,
    tx: &mut oasis7_formal_protocol::incremental_execution::ExecutionTransaction<'_, M>,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    error: &mut E,
) -> Result<WorldFieldKeyV1, WorldFieldOverlayErrorV1> {
    if records.values.class != MemoryClass::Ordinary
        || !tx.uses_budget(records.values.memory)
        || !proof.uses_budget(records.values.memory)
    {
        return Err(ValueTreeError::Invalid("cognition JSON overlay pool/class").into());
    }
    let metadata = WorldFieldKeyV1::new(field.descriptor(), WorldFieldRecordKindV1::Metadata, None)
        .map_err(ValueTreeError::Invalid)?;
    let metadata_root = lookup(tx, &metadata, records, error)?
        .ok_or(ValueTreeError::Invalid("cognition JSON overlay metadata"))?;
    records.presence(&metadata_root, &metadata, 0)?;
    let key = records.key(field.descriptor(), WorldFieldRecordKindV1::Entry, &())?;
    let value = lookup(tx, &key.0, records, error)?
        .ok_or(ValueTreeError::Invalid("cognition JSON overlay entry"))?;
    let child = read::outer(field, &key, &value, records)?;
    proof.validate_binding(field, &child)?;
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(key.0)
}
