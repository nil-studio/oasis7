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

/// Fixed scheduler controls from actual Current. Absence is permitted only
/// for the original optional reorg field after both authenticated rows miss.
/// Streamed text remains provisional until success; its caller owns the output
/// reservation. The payload bound is enforced by the original record reader.
#[allow(clippy::too_many_arguments)]
fn read_current_cognition_control_inner_v1<M, R, E, C>(
    profile: ControlReadProfileV1,
    field: CognitionControlFieldV1,
    parent: &oasis7_proto::memory_budget::ReservedValue<
        crate::incremental_cognition_current_complete_v3::CompleteCognitionCurrentQualificationV3,
    >,
    roots: &oasis7_formal_protocol::incremental_execution::RootSetV1,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    output: &mut impl std::io::Write,
    max_text: u64,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<crate::incremental_cognition_read::CognitionControlReadV1, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    use crate::incremental_cognition_read::CognitionControlReadV1;
    ctx.transaction.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        let allowed = match profile {
            ControlReadProfileV1::Service => matches!(
                field,
                CognitionControlFieldV1::JournalHeadSeq
                    | CognitionControlFieldV1::JournalHeadDigest
                    | CognitionControlFieldV1::ReorgInvalidationEpoch
            ),
            ControlReadProfileV1::Economy { .. } => economy_control(field),
        };
        if !allowed
            || ctx.values.class != MemoryClass::Ordinary
            || !parent.uses_budget(ctx.values.memory)
            || parent.allocation_class() != MemoryClass::Ordinary
            || !tx.uses_budget(ctx.values.memory)
        {
            return Err(invalid().into());
        }
        parent.validate_binding(roots)?;
        if roots.digest().map_err(|_| invalid())? != tx.parent_fence().parent_roots {
            return Err(invalid().into());
        }
        let mut records = InventoryValueReader {
            values: &mut ctx.values,
            key_work: &mut *ctx.key_work,
            semantic_bytes: ctx.semantic_bytes,
            record_bytes: ctx.record_bytes,
            cancelled: &mut ctx.cancelled,
        };
        let metadata =
            WorldFieldKeyV1::new(field.descriptor(), WorldFieldRecordKindV1::Metadata, None)
                .map_err(ValueTreeError::Invalid)?;
        let metadata_root = lookup(tx, &metadata, &mut records, &mut ctx.take_state_error)?;
        let (key, semantic) =
            records.key(field.descriptor(), WorldFieldRecordKindV1::Entry, &())?;
        let root = lookup(tx, &key, &mut records, &mut ctx.take_state_error)?;
        let Some(metadata_root) = metadata_root else {
            let absent = match profile {
                ControlReadProfileV1::Service => {
                    field == CognitionControlFieldV1::ReorgInvalidationEpoch
                }
                ControlReadProfileV1::Economy { present } => !present,
            };
            if !absent || root.is_some() {
                return Err(invalid().into());
            }
            if (records.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            return Ok(CognitionControlReadV1::Absent);
        };
        records.presence(&metadata_root, &metadata, 0)?;
        let root = root.ok_or_else(invalid)?;
        let value = if field.unsigned() {
            CognitionControlReadV1::Unsigned(records.unsigned(&root, &key, semantic)?)
        } else {
            let bytes = read_text_value(
                &root,
                &key,
                records.values,
                MemberReadProfileV1 {
                    codec: MemberPayloadCodecV1::Text,
                    semantic_bytes: semantic,
                    limits: MemberRecordLimitsV1 {
                        semantic_bytes: records.semantic_bytes,
                        payload_bytes: records.record_bytes.min(max_text),
                        record_bytes: records.record_bytes,
                    },
                },
                output,
                "Current cognition control UTF-8",
                &mut *records.cancelled,
            )?;
            CognitionControlReadV1::Text { bytes }
        };
        if (records.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(value)
    })
}

#[derive(Clone, Copy)]
enum ControlReadProfileV1 {
    Service,
    Economy { present: bool },
}
fn economy_control(field: CognitionControlFieldV1) -> bool {
    matches!(
        field,
        CognitionControlFieldV1::EconomySchema
            | CognitionControlFieldV1::EconomyProvisionHeadSeq
            | CognitionControlFieldV1::EconomyProvisionHeadDigest
            | CognitionControlFieldV1::EconomyHeadSeq
            | CognitionControlFieldV1::EconomyHeadDigest
    )
}
#[expect(
    clippy::too_many_arguments,
    reason = "Bounded inputs are independently validated"
)]
pub fn read_current_cognition_service_control_v2<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    field: CognitionControlFieldV1,
    parent: &oasis7_proto::memory_budget::ReservedValue<
        crate::incremental_cognition_current_complete_v3::CompleteCognitionCurrentQualificationV3,
    >,
    roots: &oasis7_formal_protocol::incremental_execution::RootSetV1,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    output: &mut impl std::io::Write,
    max_text: u64,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<crate::incremental_cognition_read::CognitionControlReadV1, WorldFieldOverlayErrorV1> {
    read_current_cognition_control_inner_v1(
        ControlReadProfileV1::Service,
        field,
        parent,
        roots,
        ctx,
        output,
        max_text,
        guard,
        live,
    )
}
/// Fixed Economy controls. The absence branch is derived from the original
/// complete Current Economy group reader, never from a caller presence flag.
#[expect(
    clippy::too_many_arguments,
    reason = "Bounded inputs are independently validated"
)]
pub fn read_current_cognition_economy_control_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    field: CognitionControlFieldV1,
    parent: &oasis7_proto::memory_budget::ReservedValue<
        crate::incremental_cognition_current_complete_v3::CompleteCognitionCurrentQualificationV3,
    >,
    roots: &oasis7_formal_protocol::incremental_execution::RootSetV1,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    output: &mut impl std::io::Write,
    max_text: u64,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<crate::incremental_cognition_read::CognitionControlReadV1, WorldFieldOverlayErrorV1> {
    use crate::incremental_cognition_history::CognitionHistoryCurrentSourceV1;
    if !economy_control(field) {
        return Err(invalid().into());
    }
    ctx.transaction.validate_guard(guard, live)?;
    parent.validate_binding(roots)?;
    if !parent.uses_budget(ctx.values.memory) || parent.allocation_class() != MemoryClass::Ordinary
    {
        return Err(invalid().into());
    }
    let CognitionHistoryCurrentSourceV1::Enabled(base) = parent.history_source() else {
        return Err(invalid().into());
    };
    let group = crate::incremental_cognition_groups::read_cognition_group_overlay_v1(
        crate::incremental_cognition_groups::CognitionOptionalGroupV1::Economy,
        base,
        ctx,
    )?;
    let current_guard = ctx.transaction.guard();
    read_current_cognition_control_inner_v1(
        ControlReadProfileV1::Economy {
            present: group.present(),
        },
        field,
        parent,
        roots,
        ctx,
        output,
        max_text,
        &current_guard,
        live,
    )
}
