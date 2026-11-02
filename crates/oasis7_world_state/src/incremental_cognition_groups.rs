//! Explicit optional-cohort lifecycle. These receipts bind only overlay rows;
//! complete after-state qualification requires the lifecycle receiver.
use crate::{
    incremental_cognition_family::CognitionCurrentStateQualificationV1,
    incremental_cognition_fields::{CognitionControlFieldV1, CognitionEnvelopeRefV1},
    incremental_cognition_json::CognitionJsonFieldV1,
    incremental_field_member::*,
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1, lookup},
    incremental_inventory_value_read::InventoryValueReader,
    incremental_json_records::JsonWriteWorkV1,
    incremental_projection::FieldDescriptor,
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFence, ExecutionTransaction, PreparedGuard},
    incremental_state::{MerkleNodeReader, StatePageWriter},
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CognitionOptionalGroupV1 {
    Economy,
    Delegation,
    ContinuationContexts,
    ContinuationEvaluations,
    ReorgEpoch,
}
impl CognitionOptionalGroupV1 {
    fn contains(self, index: usize) -> bool {
        match self {
            Self::Economy => (5..10).contains(&index) || (17..26).contains(&index),
            Self::Delegation => (26..30).contains(&index),
            Self::ContinuationContexts => index == 15,
            Self::ContinuationEvaluations => index == 16,
            Self::ReorgEpoch => index == 4,
        }
    }
}
fn descriptor(index: usize) -> FieldDescriptor {
    if index < 10 {
        CognitionControlFieldV1::ALL[index].descriptor()
    } else {
        CognitionJsonFieldV1::ALL[index - 10].descriptor()
    }
}
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("cognition optional cohort lifecycle")
}
#[derive(Debug)]
pub struct CognitionGroupExpectedV1 {
    group: CognitionOptionalGroupV1,
    parent: [u8; 32],
    rows: [[Option<[u8; 32]>; 2]; 30],
    present: bool,
}
impl CognitionGroupExpectedV1 {
    pub fn present(&self) -> bool {
        self.present
    }
}
fn current<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    C: FnMut() -> bool,
    E: FnMut() -> Option<ValueTreeError>,
>(
    group: CognitionOptionalGroupV1,
    tx: &mut ExecutionTransaction<'_, M>,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    error: &mut E,
) -> Result<[[Option<[u8; 32]>; 2]; 30], WorldFieldOverlayErrorV1> {
    let mut rows = [[None; 2]; 30];
    let mut presence = None;
    for (index, row) in rows.iter_mut().enumerate() {
        if !group.contains(index) {
            continue;
        }
        for (slot, value) in row.iter_mut().enumerate() {
            if (records.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            let key = if slot == 0 {
                WorldFieldKeyV1::new(descriptor(index), WorldFieldRecordKindV1::Metadata, None)
                    .map_err(ValueTreeError::Invalid)?
            } else {
                records
                    .key(descriptor(index), WorldFieldRecordKindV1::Entry, &())?
                    .0
            };
            *value = lookup(tx, &key, records, error)?;
            let exists = value.is_some();
            if presence.is_some_and(|old| old != exists) {
                return Err(invalid().into());
            }
            presence = Some(exists);
        }
    }
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(rows)
}
pub fn read_cognition_group_overlay_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    group: CognitionOptionalGroupV1,
    parent: &ReservedValue<CognitionCurrentStateQualificationV1>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
) -> Result<ReservedValue<CognitionGroupExpectedV1>, WorldFieldOverlayErrorV1> {
    context.transaction.contribute(|tx| {
        if context.values.class != MemoryClass::Ordinary
            || !tx.uses_budget(context.values.memory)
            || !parent.uses_budget(context.values.memory)
        {
            return Err(invalid().into());
        }
        parent.validate_binding(&tx.parent_fence().parent_roots)?;
        let retained = context
            .values
            .memory
            .reserve(32 << 10, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let rows = current(
            group,
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
        let present = rows.iter().any(|row| row[0].is_some());
        Ok(retained.retain(CognitionGroupExpectedV1 {
            group,
            parent: tx.parent_fence().parent_roots,
            rows,
            present,
        }))
    })
}
pub struct CognitionGroupWriteWorkV1 {
    pub json: JsonWriteWorkV1,
    pub records_remaining: u64,
}
/// `Some(source)` creates an absent group; `None` deletes the exact group.
/// No whole-owner capability is returned before generic and typed acceptance.
pub fn change_cognition_group_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: StatePageWriter + ValueNodeReader,
    CW: FnMut() -> bool,
>(
    expected: &ReservedValue<CognitionGroupExpectedV1>,
    source: Option<CognitionEnvelopeRefV1<'_>>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    work: &mut CognitionGroupWriteWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<bool, WorldFieldOverlayErrorV1> {
    let cancel = &mut context.cancelled;
    context.transaction.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        if context.values.class != MemoryClass::Ordinary
            || !tx.uses_budget(context.values.memory)
            || !tx.uses_budget(write.memory)
            || !expected.uses_budget(context.values.memory)
            || expected.parent != tx.parent_fence().parent_roots
        {
            return Err(invalid().into());
        }
        let _operation = write
            .memory
            .reserve(32 << 10, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let rows = {
            let mut combined = || cancel() || (write.cancelled)();
            current(
                expected.group,
                tx,
                &mut InventoryValueReader {
                    values: &mut context.values,
                    key_work: &mut *context.key_work,
                    semantic_bytes: context.semantic_bytes,
                    record_bytes: context.record_bytes,
                    cancelled: &mut combined,
                },
                &mut context.take_state_error,
            )?
        };
        if rows != expected.rows {
            return Err(invalid().into());
        }
        if cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        if let Some(source) = source {
            if expected.present {
                return Err(invalid().into());
            }
            let mut failure = None;
            let mut count = 0u64;
            let mut sink = |row: ReservedValue<
                oasis7_formal_protocol::incremental_execution::ExecutionWriteV1,
            >| {
                let key = WorldFieldKeyV1::from_bytes(&row.mutation.key)
                    .map_err(ValueTreeError::Invalid)?;
                match tx.write(key.namespace, &row.mutation.key, row.mutation.after) {
                    Ok(()) => {
                        count += 1;
                        Ok(())
                    }
                    Err(error) => {
                        failure = Some(error);
                        Err(invalid())
                    }
                }
            };
            let mut combined = MemberRecordWriteContext {
                sink: &mut *write.sink,
                memory: write.memory,
                io: &mut *write.io,
                key_work: &mut *write.key_work,
                limits: write.limits,
                cancelled: || cancel() || (write.cancelled)(),
            };
            let result = crate::incremental_cognition_genesis::prepare_cognition_control_fields_v1(
                source,
                CognitionControlFieldV1::ALL
                    .into_iter()
                    .enumerate()
                    .filter(|(index, _)| expected.group.contains(*index))
                    .map(|(_, field)| field),
                &mut combined,
                &mut work.records_remaining,
                &mut sink,
            )
            .and_then(|()| {
                crate::incremental_cognition_json::prepare_cognition_json_fields_v1(
                    source,
                    CognitionJsonFieldV1::ALL
                        .into_iter()
                        .enumerate()
                        .filter(|(index, _)| expected.group.contains(10 + *index))
                        .map(|(_, field)| field),
                    &mut combined,
                    &mut work.records_remaining,
                    &mut work.json,
                    &mut sink,
                )
            });
            if let Some(error) = failure {
                return Err(error.into());
            }
            result?;
            let required = (0..30)
                .filter(|index| expected.group.contains(*index))
                .count() as u64
                * 2;
            if count != required {
                return Err(invalid().into());
            }
        } else {
            if !expected.present {
                return Ok(false);
            }
            for index in 0..30 {
                if expected.group.contains(index) {
                    for slot in 0..2 {
                        if cancel() || (write.cancelled)() {
                            return Err(ValueTreeError::Cancelled.into());
                        }
                        let key = if slot == 0 {
                            WorldFieldKeyV1::new(
                                descriptor(index),
                                WorldFieldRecordKindV1::Metadata,
                                None,
                            )
                            .map_err(ValueTreeError::Invalid)?
                        } else {
                            let mut records = InventoryValueReader {
                                values: &mut context.values,
                                key_work: &mut *context.key_work,
                                semantic_bytes: context.semantic_bytes,
                                record_bytes: context.record_bytes,
                                cancelled: &mut *cancel,
                            };
                            records
                                .key(descriptor(index), WorldFieldRecordKindV1::Entry, &())?
                                .0
                        };
                        let binding = WorldFieldKeyV1::from_bytes(key.as_bytes())
                            .map_err(ValueTreeError::Invalid)?;
                        tx.write(binding.namespace, key.as_bytes(), None)?;
                    }
                }
            }
        }
        if cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(true)
    })
}

/// Explicitly verifies one current overlay cohort body. This bounded bootstrap
/// is useful after creation; subsequent point edits consume its retained proof.
/// The returned entry proof does not qualify the remaining family or side owners.
pub fn qualify_cognition_group_json_overlay_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    expected: &ReservedValue<CognitionGroupExpectedV1>,
    field: CognitionJsonFieldV1,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    work: &mut crate::incremental_json_records::JsonReadWorkV1,
) -> Result<
    ReservedValue<crate::incremental_cognition_json_read::CognitionJsonEntryQualificationV1>,
    WorldFieldOverlayErrorV1,
> {
    context.transaction.contribute(|tx| {
        if context.values.class != MemoryClass::Ordinary
            || !tx.uses_budget(context.values.memory)
            || !expected.uses_budget(context.values.memory)
            || expected.parent != tx.parent_fence().parent_roots
            || !expected.present
        {
            return Err(invalid().into());
        }
        let index = CognitionJsonFieldV1::ALL
            .iter()
            .position(|candidate| *candidate == field)
            .ok_or_else(invalid)?;
        if !expected.group.contains(10 + index) {
            return Err(invalid().into());
        }
        let mut records = InventoryValueReader {
            values: &mut context.values,
            key_work: &mut *context.key_work,
            semantic_bytes: context.semantic_bytes,
            record_bytes: context.record_bytes,
            cancelled: &mut context.cancelled,
        };
        if current(
            expected.group,
            tx,
            &mut records,
            &mut context.take_state_error,
        )? != expected.rows
        {
            return Err(invalid().into());
        }
        let key = records.key(field.descriptor(), WorldFieldRecordKindV1::Entry, &())?;
        let value = expected.rows[10 + index][1].ok_or_else(invalid)?;
        let child =
            crate::incremental_cognition_json_read::outer(field, &key, &value, &mut records)?;
        let output = crate::incremental_cognition_json_read::qualify_body_records(
            field,
            &child,
            &mut records,
            work,
        )?;
        if (records.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(output)
    })
}
