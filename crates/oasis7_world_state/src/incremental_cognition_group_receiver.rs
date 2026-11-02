//! Complete optional-cohort presence changes from an exact closed parent.
use super::*;
use crate::{
    incremental_field_overlay::WorldFieldOverlayErrorV1,
    incremental_json_records::{JsonChangeWorkV1, advance_json_subtree_records_v1},
};
use oasis7_formal_protocol::incremental_execution::{TransitionRecordV1, TransitionVerifier};

pub fn accept_cognition_current_lifecycle_transition_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    parent: &ReservedValue<CognitionCurrentStateQualificationV1>,
    verifier: &TransitionVerifier<'_, M>,
    record: &TransitionRecordV1,
    context: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut JsonChangeWorkV1,
) -> Result<ReservedValue<CognitionCurrentStateQualificationV1>, WorldFieldOverlayErrorV1> {
    let verified = receiver::verify_parent(parent, verifier, record, context)?;
    let retained = context
        .values
        .memory
        .reserve(32 << 10, context.values.class)
        .map_err(ValueTreeError::Memory)?;
    let mut entries = parent.entries.clone();
    let old = std::array::from_fn::<_, 30, _>(|index| {
        if index < 10 {
            parent.controls & (1 << index) != 0
        } else {
            parent.entries[index - 10].is_some()
        }
    });
    let mut seen = std::array::from_fn::<_, 30, _>(|index| [old[index]; 2]);
    let cancel = RefCell::new(&mut context.cancelled);
    for write in &verified.record.writes {
        work.paths_remaining =
            work.paths_remaining
                .checked_sub(1)
                .ok_or(ValueTreeError::ResourceLimited(
                    "cognition lifecycle outer writes",
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
            .enumerate()
            .find(|(_, field)| field.descriptor().name == binding.name);
        let json = CognitionJsonFieldV1::ALL
            .into_iter()
            .enumerate()
            .find(|(_, field)| field.name() == binding.name);
        let (index, descriptor) = if let Some((index, field)) = control {
            (index, field.descriptor())
        } else if let Some((index, field)) = json {
            (10 + index, field.descriptor())
        } else {
            return Err(invalid().into());
        };
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
        let (slot, key) = match binding.kind {
            WorldFieldRecordKindV1::Metadata => (
                0,
                (
                    WorldFieldKeyV1::new(descriptor, WorldFieldRecordKindV1::Metadata, None)
                        .map_err(ValueTreeError::Invalid)?,
                    0,
                ),
            ),
            WorldFieldRecordKindV1::Entry => (
                1,
                local.key(descriptor, WorldFieldRecordKindV1::Entry, &())?,
            ),
            _ => return Err(invalid().into()),
        };
        if write.mutation.key != key.0.as_bytes() || write.mutation.before.is_some() != old[index] {
            return Err(invalid().into());
        }
        seen[index][slot] = write.mutation.after.is_some();
        if slot == 0 {
            if let Some(value) = write.mutation.after {
                local.records().presence(&value, &key.0, 0)?;
            }
        } else if let Some((_, field)) = control {
            if let Some(value) = write.mutation.after {
                read_control(field, &key, &value, &mut local.records())?;
            }
        } else if let Some((json_index, field)) = json {
            entries[json_index] = if let Some(value) = write.mutation.after {
                let child = json_read::outer(field, &key, &value, &mut local.records())?;
                let proof = if let Some(original) = &parent.entries[json_index] {
                    let before = write.mutation.before.ok_or_else(invalid)?;
                    let old_child = json_read::outer(field, &key, &before, &mut local.records())?;
                    original.validate_binding(field, &old_child)?;
                    let body = advance_json_subtree_records_v1(
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
                    json_read::mint(field, body, local.values.memory, local.values.class)?
                } else {
                    json_read::qualify_body_records(
                        field,
                        &child,
                        &mut local.records(),
                        &mut work.json,
                    )?
                };
                Some(Arc::new(proof))
            } else {
                None
            };
        }
        local.check()?;
    }
    for pair in seen {
        if pair[0] != pair[1] {
            return Err(invalid().into());
        }
    }
    let mut controls = 0u16;
    for (index, pair) in seen[..10].iter().enumerate() {
        if pair[1] {
            controls |= 1 << index;
        }
    }
    check_layout(
        controls,
        &std::array::from_fn(|index| entries[index].is_some()),
    )?;
    let roots = record.after.digest().map_err(|_| invalid())?;
    let output = retained.retain(CognitionCurrentStateQualificationV1 {
        roots,
        controls,
        entries,
    });
    if (cancel.borrow_mut())() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(output)
}
