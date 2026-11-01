//! Borrowed provisional control cells; the remaining Cognition cohorts and all
//! side owners must be supplied independently before any whole-owner claim.
use crate::{
    incremental_cognition_fields::{CognitionControlFieldV1, CognitionEnvelopeRefV1},
    incremental_field_member::{
        MemberPayloadRefV1, MemberRecordError, MemberRecordWriteContext, write_member_record_v1,
    },
    incremental_field_presence::{
        PresenceRecordLimitsV1, PresenceRecordWriteContext, write_field_metadata_v1,
    },
    incremental_world_field_key::WorldFieldRecordKindV1,
};
use oasis7_formal_protocol::{
    incremental_execution::ExecutionWriteV1,
    incremental_value::{ValueNodeWriter, ValueTreeError},
};
use oasis7_proto::memory_budget::ReservedValue;

pub fn prepare_cognition_controls_genesis_v1<
    W: ValueNodeWriter,
    C: FnMut() -> bool,
    S: FnMut(ReservedValue<ExecutionWriteV1>) -> Result<(), ValueTreeError>,
>(
    source: CognitionEnvelopeRefV1<'_>,
    context: &mut MemberRecordWriteContext<'_, W, C>,
    records_remaining: &mut u64,
    sink: &mut S,
) -> Result<(), MemberRecordError> {
    for field in CognitionControlFieldV1::REQUIRED
        .into_iter()
        .chain([CognitionControlFieldV1::ReorgInvalidationEpoch])
    {
        let Some(value) = field.value(source) else {
            continue;
        };
        let payload = match field {
            CognitionControlFieldV1::JournalHeadSeq
            | CognitionControlFieldV1::ReorgInvalidationEpoch => MemberPayloadRefV1::U64(
                value
                    .as_u64()
                    .ok_or(MemberRecordError::Invalid("cognition control u64"))?,
            ),
            _ => MemberPayloadRefV1::Text(
                value
                    .as_str()
                    .ok_or(MemberRecordError::Invalid("cognition control text"))?,
            ),
        };
        for metadata in [true, false] {
            if (context.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            if *records_remaining == 0 {
                return Err(ValueTreeError::ResourceLimited("cognition control records").into());
            }
            *records_remaining -= 1;
            let (key, root) = if metadata {
                let output = write_field_metadata_v1(
                    field.descriptor(),
                    &mut PresenceRecordWriteContext {
                        sink: &mut *context.sink,
                        memory: context.memory,
                        io: &mut *context.io,
                        key_work: &mut *context.key_work,
                        limits: PresenceRecordLimitsV1 {
                            semantic_bytes: context.limits.semantic_bytes,
                            record_bytes: context.limits.record_bytes,
                        },
                        cancelled: &mut context.cancelled,
                    },
                )?;
                (output.field_key, output.value_root)
            } else {
                let output = write_member_record_v1(
                    field.descriptor(),
                    WorldFieldRecordKindV1::Entry,
                    &(),
                    payload,
                    context,
                )?;
                (output.field_key, output.value_root)
            };
            crate::incremental_field_write::emit(key, root, context, sink)?;
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "incremental_cognition_genesis_tests.rs"]
mod tests;
