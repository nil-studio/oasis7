//! One registered WorldState.time representation for Runtime and control owners.
use super::{
    World,
    incremental_body::{BodyMutationContext, BodyMutationError, Discard},
};
use oasis7_formal_protocol::{
    incremental_execution::ExecutionTransaction,
    incremental_state::{MerkleNodeReader, StateNamespaceV1 as Ns},
    incremental_value::{ValueNodeWriter, ValueTreeError},
};
use oasis7_world_state::{
    incremental_field_member::{
        MemberPayloadRefV1, MemberRecordError, MemberRecordLimitsV1, MemberRecordWriteContext,
        write_member_record_v1,
    },
    incremental_field_presence::{
        PresenceRecordLimitsV1, PresenceRecordWriteContext, write_field_metadata_v1,
    },
    incremental_projection::{FieldDestination, FieldOwner, FieldShape, WORLD_STATE_FIELDS},
    incremental_semantic_key::{SemanticKeyBudget, SemanticKeyError},
    incremental_world_field_key::WorldFieldRecordKindV1,
};
fn error(error: MemberRecordError) -> BodyMutationError {
    match error {
        MemberRecordError::Value(error) => error.into(),
        MemberRecordError::Invalid(reason) => BodyMutationError::Invalid(reason),
        MemberRecordError::Io(error) => BodyMutationError::Original(Box::new(error)),
        MemberRecordError::Key(error) => match error {
            SemanticKeyError::Cancelled => ValueTreeError::Cancelled.into(),
            SemanticKeyError::Memory(error) => ValueTreeError::Memory(error).into(),
            SemanticKeyError::ResourceLimited => {
                ValueTreeError::ResourceLimited("clock semantic key").into()
            }
            SemanticKeyError::Invalid(reason) => BodyMutationError::Invalid(reason),
        },
    }
}
fn emit_clock<
    W: ValueNodeWriter,
    C: FnMut() -> bool,
    F: FnMut(&[u8], [u8; 32]) -> Result<(), BodyMutationError>,
>(
    time: u64,
    context: &mut BodyMutationContext<'_, W, C>,
    mut emit: F,
) -> Result<(), BodyMutationError> {
    let field = WORLD_STATE_FIELDS
        .iter()
        .copied()
        .find(|field| {
            field.name == "time"
                && field.owner == FieldOwner::WorldState
                && field.shape == FieldShape::Value
                && field.destination == FieldDestination::State(Ns::Allocator)
        })
        .ok_or(BodyMutationError::Invalid("world clock registration"))?;
    // Only the fixed Unit semantic key is encoded, never a caller-owned key.
    let mut key_work = SemanticKeyBudget {
        bytes_remaining: 64,
    };
    let metadata = write_field_metadata_v1(
        field,
        &mut PresenceRecordWriteContext {
            sink: &mut *context.sink,
            memory: context.memory,
            io: &mut *context.io,
            key_work: &mut key_work,
            limits: PresenceRecordLimitsV1 {
                semantic_bytes: 64,
                record_bytes: 512,
            },
            cancelled: &mut context.cancelled,
        },
    )
    .map_err(error)?;
    emit(metadata.field_key.as_bytes(), metadata.value_root)?;
    let entry = write_member_record_v1(
        field,
        WorldFieldRecordKindV1::Entry,
        &(),
        MemberPayloadRefV1::U64(time),
        &mut MemberRecordWriteContext {
            sink: &mut *context.sink,
            memory: context.memory,
            io: &mut *context.io,
            key_work: &mut key_work,
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 64,
                payload_bytes: 8,
                record_bytes: 512,
            },
            cancelled: &mut context.cancelled,
        },
    )
    .map_err(error)?;
    emit(entry.field_key.as_bytes(), entry.value_root)?;
    if (context.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(())
}
pub(super) fn check_clock<R: MerkleNodeReader, W, C: FnMut() -> bool>(
    expected: u64,
    txn: &mut ExecutionTransaction<'_, R>,
    context: &mut BodyMutationContext<'_, W, C>,
) -> Result<(), BodyMutationError> {
    if !txn.uses_budget(context.memory) {
        return Err(BodyMutationError::Invalid("clock transaction memory pool"));
    }
    // Compare authenticated IDs to exact canonical bytes of the known expected
    // value. This reads both registered paths, never a second clock copy.
    emit_clock(
        expected,
        &mut BodyMutationContext {
            sink: &mut Discard,
            memory: context.memory,
            io: &mut *context.io,
            cancelled: &mut context.cancelled,
        },
        |key, id| {
            if txn.read(Ns::Allocator, key)? != Some(id) {
                return Err(BodyMutationError::Invalid("world clock precondition"));
            }
            Ok(())
        },
    )
}
impl World {
    /// Provisional new-world records for the same registered time field used by
    /// control setters. The receiver admits output metadata; no head is installed.
    /// Full genesis chooses one Time producer, never this helper together with
    /// the WorldState scalar-family producer, and rejects duplicate output keys.
    pub fn write_iws_clock_genesis<
        W: ValueNodeWriter,
        C: FnMut() -> bool,
        F: FnMut(&[u8], [u8; 32]) -> Result<(), BodyMutationError>,
    >(
        &self,
        context: &mut BodyMutationContext<'_, W, C>,
        emit: F,
    ) -> Result<(), BodyMutationError> {
        emit_clock(self.state.time, context, emit)
    }
}

#[cfg(test)]
#[path = "incremental_clock_tests.rs"]
mod tests;
