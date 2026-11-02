//! Generic replay plus one exact Cognition Unit-entry JSON scalar scope.
//! Other cohorts and side owners remain the caller's independent obligations.
use crate::{
    incremental_cognition_json::CognitionJsonFieldV1,
    incremental_cognition_json_read::{self as read, CognitionJsonEntryQualificationV1},
    incremental_field_overlay::WorldFieldOverlayErrorV1,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_json_records::{
        JsonScalarChangeWorkV1, advance_json_scalar_subtree_records_constrained_v1,
    },
    incremental_projection::FieldOwner,
    incremental_world_field_key::{WorldFieldKeyV1, WorldFieldRecordKindV1},
};
use oasis7_formal_protocol::{
    incremental_execution::{TransitionRecordV1, TransitionVerifier},
    incremental_state::MerkleNodeReader,
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("cognition JSON entry transition")
}
pub fn accept_cognition_json_entry_transition_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    field: CognitionJsonFieldV1,
    parent: &ReservedValue<CognitionJsonEntryQualificationV1>,
    verifier: &TransitionVerifier<'_, M>,
    record: &TransitionRecordV1,
    context: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut JsonScalarChangeWorkV1,
) -> Result<ReservedValue<CognitionJsonEntryQualificationV1>, WorldFieldOverlayErrorV1> {
    context.check()?;
    if context.values.class != MemoryClass::Ordinary
        || verifier.class != MemoryClass::Ordinary
        || !parent.uses_budget(context.values.memory)
        || !verifier.memory.same_pool(context.values.memory)
        || !std::ptr::eq(verifier.source, context.state)
    {
        return Err(invalid().into());
    }
    if !record.before.state.iter().any(|root| root == context.root) {
        return Err(invalid().into());
    }
    let verified = verifier.verify(record);
    let error = (context.take_state_error)();
    context.check()?;
    if let Some(error) = error {
        return Err(error.into());
    }
    let verified = verified?;
    let metadata = WorldFieldKeyV1::new(field.descriptor(), WorldFieldRecordKindV1::Metadata, None)
        .map_err(ValueTreeError::Invalid)?;
    let metadata_root = context.lookup(&metadata)?.ok_or_else(invalid)?;
    context.records().presence(&metadata_root, &metadata, 0)?;
    let key = context.key(field.descriptor(), WorldFieldRecordKindV1::Entry, &())?;
    let before = context.lookup(&key.0)?.ok_or_else(invalid)?;
    let old = read::outer(field, &key, &before, &mut context.records())?;
    parent.validate_binding(field, &old)?;
    let mut after = before;
    for write in &verified.record.writes {
        work.paths_remaining =
            work.paths_remaining
                .checked_sub(1)
                .ok_or(ValueTreeError::ResourceLimited(
                    "cognition entry outer writes",
                ))?;
        if !write
            .mutation
            .key
            .starts_with(b"oasis7.iws.world-field-key.v1\0")
        {
            continue;
        }
        let binding =
            WorldFieldKeyV1::from_bytes(&write.mutation.key).map_err(ValueTreeError::Invalid)?;
        if binding.owner != FieldOwner::Cognition || binding.name != field.name() {
            continue;
        }
        if write.namespace != context.root.namespace {
            return Err(invalid().into());
        }
        if write.mutation.key == metadata.as_bytes() {
            if write.mutation.before != Some(metadata_root)
                || write.mutation.after != Some(metadata_root)
            {
                return Err(invalid().into());
            }
        } else if write.mutation.key == key.0.as_bytes() {
            if write.mutation.before != Some(before) {
                return Err(invalid().into());
            }
            after = write.mutation.after.ok_or_else(invalid)?;
        } else {
            return Err(invalid().into());
        }
        context.check()?;
    }
    let child = read::outer(field, &key, &after, &mut context.records())?;
    let body = advance_json_scalar_subtree_records_constrained_v1(
        field.profile(),
        parent.json(),
        &child,
        &mut context.records(),
        work,
        field.scalar_constraint_depth(),
    )?;
    let output = read::mint(field, body, context.values.memory, context.values.class)?;
    context.check()?;
    Ok(output)
}
