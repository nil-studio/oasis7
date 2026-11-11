//! Incremental current-state scalar acceptance from an exact qualified parent.
//! Structural CRUD and external history/proof/retention acceptance are separate.
use super::*;
use crate::{
    incremental_field_overlay::WorldFieldOverlayErrorV1,
    incremental_json_records::{
        JsonScalarChangeWorkV1, advance_json_scalar_subtree_records_constrained_v1,
    },
};
use oasis7_formal_protocol::incremental_execution::{TransitionRecordV1, TransitionVerifier};
use oasis7_proto::memory_budget::MemoryClass;

pub fn accept_cognition_current_scalar_transition_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    parent: &ReservedValue<CognitionCurrentStateQualificationV1>,
    verifier: &TransitionVerifier<'_, M>,
    record: &TransitionRecordV1,
    context: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut JsonScalarChangeWorkV1,
) -> Result<ReservedValue<CognitionCurrentStateQualificationV1>, WorldFieldOverlayErrorV1> {
    accept_current(parent, verifier, record, context, work, false)
}
/// Existing field membership is fixed; JSON member/array relations may change.
pub fn accept_cognition_current_transition_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    parent: &ReservedValue<CognitionCurrentStateQualificationV1>,
    verifier: &TransitionVerifier<'_, M>,
    record: &TransitionRecordV1,
    context: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut JsonScalarChangeWorkV1,
) -> Result<ReservedValue<CognitionCurrentStateQualificationV1>, WorldFieldOverlayErrorV1> {
    accept_current(parent, verifier, record, context, work, true)
}
fn accept_current<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    parent: &ReservedValue<CognitionCurrentStateQualificationV1>,
    verifier: &TransitionVerifier<'_, M>,
    record: &TransitionRecordV1,
    context: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut JsonScalarChangeWorkV1,
    structural: bool,
) -> Result<ReservedValue<CognitionCurrentStateQualificationV1>, WorldFieldOverlayErrorV1> {
    let witness = verify_parent(parent, verifier, record, context)?;
    accept_cognition_current_verified_v1(parent, &witness, context, work, structural)
}

pub(crate) fn accept_cognition_current_verified_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    parent: &ReservedValue<CognitionCurrentStateQualificationV1>,
    witness: &crate::incremental_verified_world_transition::VerifiedWorldTransitionV1,
    context: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut JsonScalarChangeWorkV1,
    structural: bool,
) -> Result<ReservedValue<CognitionCurrentStateQualificationV1>, WorldFieldOverlayErrorV1> {
    checked_parent_v1(parent, witness, context)?;
    let record = witness.record();
    let verified = witness.prepared();
    // Arc cloning shares admitted immutable proofs; this charge covers the new
    // token's metadata and all replacement Arc allocations until its last use.
    let retained = context
        .values
        .memory
        .reserve(32 << 10, context.values.class)
        .map_err(ValueTreeError::Memory)?;
    let mut entries = parent.entries.clone();
    let cancel = RefCell::new(&mut context.cancelled);
    for write in &verified.record.writes {
        work.paths_remaining =
            work.paths_remaining
                .checked_sub(1)
                .ok_or(ValueTreeError::ResourceLimited(
                    "cognition family outer writes",
                ))?;
        if (cancel.borrow_mut())() {
            return Err(ValueTreeError::Cancelled.into());
        }
        if !write
            .mutation
            .key
            .starts_with(b"oasis7.iws.world-field-key.v1\0")
        {
            continue;
        }
        let binding =
            WorldFieldKeyV1::from_bytes(&write.mutation.key).map_err(ValueTreeError::Invalid)?;
        if binding.owner != FieldOwner::Cognition {
            continue;
        }
        let control = CognitionControlFieldV1::ALL
            .into_iter()
            .find(|field| field.descriptor().name == binding.name);
        let json = CognitionJsonFieldV1::ALL
            .into_iter()
            .enumerate()
            .find(|(_, field)| field.name() == binding.name);
        let descriptor = control
            .map(|field| field.descriptor())
            .or_else(|| json.map(|(_, field)| field.descriptor()))
            .ok_or_else(invalid)?;
        if descriptor.destination != FieldDestination::State(write.namespace) {
            return Err(invalid().into());
        }
        let root = record
            .before
            .state
            .iter()
            .find(|root| root.namespace == write.namespace)
            .ok_or_else(invalid)?;
        let mut local = WorldFieldReadContextV1 {
            root,
            state: context.state,
            values: ValueReadContext {
                source: context.values.source,
                memory: context.values.memory,
                class: context.values.class,
                io: &mut *context.values.io,
            },
            key_work: &mut *context.key_work,
            semantic_bytes: context.semantic_bytes,
            record_bytes: context.record_bytes,
            take_state_error: &mut context.take_state_error,
            cancelled: || (cancel.borrow_mut())(),
        };
        if binding.kind == WorldFieldRecordKindV1::Metadata {
            let key = WorldFieldKeyV1::new(descriptor, WorldFieldRecordKindV1::Metadata, None)
                .map_err(ValueTreeError::Invalid)?;
            if write.mutation.key != key.as_bytes()
                || write.mutation.before.is_none()
                || write.mutation.before != write.mutation.after
            {
                return Err(invalid().into());
            }
            local
                .records()
                .presence(&write.mutation.after.ok_or_else(invalid)?, &key, 0)?;
        } else {
            let key = local.key(descriptor, WorldFieldRecordKindV1::Entry, &())?;
            if binding.kind != WorldFieldRecordKindV1::Entry
                || write.mutation.key != key.0.as_bytes()
            {
                return Err(invalid().into());
            }
            let before = write.mutation.before.ok_or_else(invalid)?;
            let after = write.mutation.after.ok_or_else(invalid)?;
            if let Some(field) = control {
                if !parent.control_present(field) {
                    return Err(invalid().into());
                }
                read_control(field, &key, &after, &mut local.records())?;
            } else if let Some((index, field)) = json {
                let original = parent.entries[index].as_ref().ok_or_else(invalid)?;
                let old = json_read::outer(field, &key, &before, &mut local.records())?;
                original.validate_binding(field, &old)?;
                let child = json_read::outer(field, &key, &after, &mut local.records())?;
                let body = if structural {
                    let body = crate::incremental_json_records::advance_json_subtree_records_v1(
                        field.profile(),
                        original.json(),
                        &child,
                        &mut local.records(),
                        work,
                        field.immediate(),
                    )?;
                    json_read::require_root_shape(
                        field,
                        &body,
                        &mut local.records(),
                        &mut work.json,
                    )?;
                    body
                } else {
                    advance_json_scalar_subtree_records_constrained_v1(
                        field.profile(),
                        original.json(),
                        &child,
                        &mut local.records(),
                        work,
                        field.scalar_constraint_depth(),
                    )?
                };
                entries[index] = Some(Arc::new(json_read::mint(
                    field,
                    body,
                    local.values.memory,
                    local.values.class,
                )?));
            }
        }
        local.check()?;
    }
    let roots = record.after.digest().map_err(|_| invalid())?;
    if (cancel.borrow_mut())() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(retained.retain(CognitionCurrentStateQualificationV1 {
        class: context.values.class,
        roots,
        controls: parent.controls,
        entries,
    }))
}

pub(super) fn verify_parent<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    parent: &ReservedValue<CognitionCurrentStateQualificationV1>,
    verifier: &TransitionVerifier<'_, M>,
    record: &TransitionRecordV1,
    context: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<
    crate::incremental_verified_world_transition::VerifiedWorldTransitionV1,
    WorldFieldOverlayErrorV1,
> {
    context.check()?;
    if context.values.class != MemoryClass::Ordinary
        || parent.class() != MemoryClass::Ordinary
        || verifier.class != MemoryClass::Ordinary
        || !parent.uses_budget(context.values.memory)
        || !verifier.memory.same_pool(context.values.memory)
        || !std::ptr::eq(verifier.source, context.state)
        || !record.before.state.iter().any(|root| root == context.root)
    {
        return Err(invalid().into());
    }
    parent.validate_binding(&record.before.digest().map_err(|_| invalid())?)?;
    let after_root = record.after.state_root(context.root.namespace);
    let mut verify =
        crate::incremental_verified_world_transition::WorldTransitionVerificationContextV1 {
            after_root,
            memory: context.values.memory,
            class: context.values.class,
            take_state_error: &mut context.take_state_error,
            cancelled: &mut context.cancelled,
        };
    Ok(
        crate::incremental_verified_world_transition::verify_world_transition_in_v1(
            verifier,
            record,
            &mut verify,
        )?,
    )
}

pub(super) fn checked_parent_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    parent: &ReservedValue<CognitionCurrentStateQualificationV1>,
    witness: &crate::incremental_verified_world_transition::VerifiedWorldTransitionV1,
    context: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<(), WorldFieldOverlayErrorV1> {
    context.check()?;
    witness.check_pool_class(context.values.memory, context.values.class)?;
    if context.values.class != MemoryClass::Ordinary
        || parent.class() != MemoryClass::Ordinary
        || !parent.uses_budget(context.values.memory)
        || !witness
            .record()
            .before
            .state
            .iter()
            .any(|r| r == context.root)
    {
        return Err(invalid().into());
    }
    parent.validate_binding(witness.before_digest())?;
    Ok(())
}
