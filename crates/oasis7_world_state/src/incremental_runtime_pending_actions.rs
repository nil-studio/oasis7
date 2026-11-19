//! Ordered references to original ActionEnvelopes. Original persistence and
//! business validation remain caller obligations; these rows are provisional.
use crate::{
    ActionEnvelope,
    incremental_field_member::*,
    incremental_field_presence::{
        PresenceRecordLimitsV1, PresenceRecordWriteContext, write_field_metadata_v1,
    },
};
use oasis7_formal_protocol::{
    incremental_artifacts::OriginalArtifactRefV1,
    incremental_execution::ExecutionWriteV1,
    incremental_state::{StateNamespaceV1, StatePageWriter},
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::ReservedValue;
use std::collections::VecDeque;

pub const PENDING_ACTIONS_FIELD_V1: FieldDescriptor = FieldDescriptor {
    owner: FieldOwner::Runtime,
    name: "pending_actions",
    shape: FieldShape::Ordered,
    destination: FieldDestination::State(StateNamespaceV1::Scheduler),
};

/// The callback persists the exact original envelope and returns its actual
/// content ID and length. Repeated IDs retain distinct pair positions.
/// No arbitrary DTO serialization enters the field-member protocol.
pub fn prepare_runtime_pending_actions_genesis_v1<
    W: StatePageWriter + ValueNodeReader,
    C: FnMut() -> bool,
    P: FnMut(u64, &ActionEnvelope) -> Result<OriginalArtifactRefV1, ValueTreeError>,
    S: FnMut(ReservedValue<ExecutionWriteV1>) -> Result<(), ValueTreeError>,
>(
    source: &VecDeque<ActionEnvelope>,
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
    remaining: &mut u64,
    work: &mut crate::incremental_runtime_pending_action_record::PendingActionWriteWorkV1,
    persist: &mut P,
    sink: &mut S,
) -> Result<(), MemberRecordError> {
    let mut debit = || {
        *remaining = remaining
            .checked_sub(1)
            .ok_or(ValueTreeError::ResourceLimited("pending action GEN rows"))?;
        Ok::<_, ValueTreeError>(())
    };
    if (ctx.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    debit()?;
    let meta = write_field_metadata_v1(
        PENDING_ACTIONS_FIELD_V1,
        &mut PresenceRecordWriteContext {
            sink: &mut *ctx.sink,
            memory: ctx.memory,
            io: &mut *ctx.io,
            key_work: &mut *ctx.key_work,
            limits: PresenceRecordLimitsV1 {
                semantic_bytes: ctx.limits.semantic_bytes,
                record_bytes: ctx.limits.record_bytes,
            },
            cancelled: &mut ctx.cancelled,
        },
    )?;
    crate::incremental_field_write::emit(meta.field_key, meta.value_root, ctx, sink)?;
    for (ordinal, envelope) in source.iter().enumerate() {
        if (ctx.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        debit()?;
        let ordinal = u64::try_from(ordinal)
            .map_err(|_| MemberRecordError::Invalid("pending action ordinal"))?;
        let original = persist(ordinal, envelope)?;
        if (ctx.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        let row =
            crate::incremental_runtime_pending_action_record::prepare_pending_action_record_v1(
                crate::incremental_runtime_pending_action_record::PendingActionPositionV1 {
                    era: 0,
                    ordinal,
                },
                envelope,
                original,
                ctx,
                work,
            )?;
        crate::incremental_field_write::emit(row.field_key, row.value_root, ctx, sink)?;
    }
    if (ctx.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(())
}

#[cfg(test)]
#[path = "incremental_runtime_pending_actions_tests.rs"]
mod tests;
