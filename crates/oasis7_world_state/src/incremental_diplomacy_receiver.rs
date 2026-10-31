//! Qualified-parent acceptance checks only actual changed inner paths.
use super::*;
use crate::{
    incremental_field_overlay::WorldFieldOverlayErrorV1,
    incremental_record_diff::{self, RecordDiffLabelsV1},
};
use oasis7_formal_protocol::{
    incremental_execution::{TransitionRecordV1, TransitionVerifier},
    incremental_state::StateEntryV1,
};
fn invalid() -> ValueTreeError {
    closure::invalid()
}
fn child<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    field: DiplomacyFieldV1,
    before: Option<&StateRootV1>,
    after: &StateRootV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    cells: &mut u64,
    max: u64,
) -> Result<(), ValueTreeError> {
    let empty = StateRootV1::empty(StateNamespaceV1::Business).map_err(|_| invalid())?;
    let old = before.unwrap_or(&empty);
    let mut local = WorldFieldReadContextV1 {
        root: old,
        state: ctx.state,
        values: oasis7_formal_protocol::incremental_value::ValueReadContext {
            source: ctx.values.source,
            memory: ctx.values.memory,
            class: ctx.values.class,
            io: &mut *ctx.values.io,
        },
        key_work: &mut *ctx.key_work,
        semantic_bytes: ctx.semantic_bytes,
        record_bytes: ctx.record_bytes,
        take_state_error: &mut ctx.take_state_error,
        cancelled: &mut ctx.cancelled,
    };
    let old_length = if before.is_some() {
        closure::length(field, &mut local)?
    } else {
        0
    };
    local.root = after;
    closure::relations(field, &mut local)?;
    let new_length = closure::length(field, &mut local)?;
    let mut delta = 0i128;
    let state = local.state;
    let error = &mut local.take_state_error;
    let labels = RecordDiffLabelsV1 {
        invalid: "diplomacy changed tree",
        cells: "diplomacy changed paths",
        stack: "diplomacy changed stack",
        tree: inner_tree::LABELS,
    };
    let mut records = InventoryValueReader {
        values: &mut local.values,
        key_work: local.key_work,
        semantic_bytes: local.semantic_bytes,
        record_bytes: local.record_bytes,
        cancelled: &mut local.cancelled,
    };
    incremental_record_diff::visit(
        old,
        after,
        &mut records,
        cells,
        labels,
        |old, new, records| {
            let mut view = WorldFieldReadContextV1 {
                root: after,
                state,
                values: oasis7_formal_protocol::incremental_value::ValueReadContext {
                    source: records.values.source,
                    memory: records.values.memory,
                    class: records.values.class,
                    io: &mut *records.values.io,
                },
                key_work: &mut *records.key_work,
                semantic_bytes: records.semantic_bytes,
                record_bytes: records.record_bytes,
                take_state_error: &mut *error,
                cancelled: &mut *records.cancelled,
            };
            if let Some(entry) = new {
                if closure::actual(field, entry, &mut view, max)? {
                    delta += 1;
                }
                let (_, semantic) = closure::identity(field, entry, &mut view, max)?;
                if let schema::Row::Outcome(i, _) = schema::inspect(field, &semantic)? {
                    closure::outcome(i, true, &mut view)?;
                }
            }
            if let Some(entry) = old {
                let (_, semantic) = closure::identity(field, entry, &mut view, max)?;
                match schema::inspect(field, &semantic)? {
                    schema::Row::Column(_) => {}
                    schema::Row::Member(i) => {
                        delta -= 1;
                        if new.is_none() && i < new_length {
                            return Err(invalid());
                        }
                    }
                    schema::Row::Outcome(i, column) => {
                        if column.is_none() {
                            delta -= 1;
                        }
                        if new.is_none() {
                            if i < new_length {
                                closure::outcome(i, true, &mut view)?;
                            } else {
                                // A removed ordinal must leave neither marker nor any old columns.
                                for path in [
                                    Path::Outcome(i),
                                    Path::OutcomeField(i, "agent_id"),
                                    Path::OutcomeField(i, "electricity_delta"),
                                    Path::OutcomeField(i, "data_delta"),
                                    Path::OutcomeField(i, "reputation_delta"),
                                ] {
                                    let key = path.read(field, &mut view)?;
                                    if view.lookup(&key.0)?.is_some() {
                                        return Err(invalid());
                                    }
                                }
                            }
                        }
                    }
                }
            }
            view.check()
        },
    )?;
    if i128::from(old_length) + delta != i128::from(new_length) {
        return Err(invalid());
    }
    local.check()
}
/// Generic replay integrity and this exact owner's closure are both required.
pub fn accept_diplomacy_transition_v1<
    M: MerkleNodeReader,
    S: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    parent: &ReservedValue<DiplomacyFamilyQualificationV1>,
    verifier: &TransitionVerifier<'_, S>,
    record: &TransitionRecordV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    cells: &mut u64,
    max: u64,
) -> Result<ReservedValue<DiplomacyFamilyQualificationV1>, WorldFieldOverlayErrorV1> {
    ctx.check()?;
    if !parent.uses_budget(ctx.values.memory)
        || !verifier.memory.same_pool(ctx.values.memory)
        || verifier.class != ctx.values.class
    {
        return Err(invalid().into());
    }
    parent.validate_binding(&record.before.digest().map_err(|_| invalid())?)?;
    let verified = verifier.verify(record);
    let source_error = (ctx.take_state_error)();
    ctx.check()?;
    if let Some(error) = source_error {
        return Err(error.into());
    }
    let verified = verified?;
    let business = verified
        .record
        .after
        .state
        .iter()
        .find(|root| root.namespace == StateNamespaceV1::Business)
        .ok_or_else(invalid)?;
    if ctx.root != business {
        return Err(invalid().into());
    }
    let mut counts = parent.record_counts();
    for write in &verified.record.writes {
        *cells = cells.checked_sub(1).ok_or(ValueTreeError::ResourceLimited(
            "diplomacy outer changed cells",
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
        let Some(field) = DiplomacyFieldV1::from_name(binding.name) else {
            continue;
        };
        let edge = |value: Option<[u8; 32]>, ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>| {
            value
                .map(|value| {
                    family::outer(
                        &StateEntryV1 {
                            namespace: write.namespace,
                            key: write.mutation.key.clone(),
                            value,
                        },
                        ctx,
                    )
                })
                .transpose()
                .map(Option::flatten)
        };
        let old = edge(write.mutation.before, ctx)?;
        let new = edge(write.mutation.after, ctx)?;
        if binding.kind == WorldFieldRecordKindV1::Metadata {
            if write.mutation.after.is_none() {
                return Err(invalid().into());
            }
            continue;
        }
        let index = field as usize;
        match (old, new) {
            (None, Some((owner, _, new))) => {
                counts[index] = counts[index].checked_add(1).ok_or_else(invalid)?;
                child(owner, None, &new, ctx, cells, max)?;
            }
            (Some((owner, old_digest, old)), Some((new_owner, new_digest, new))) => {
                if owner != new_owner || old_digest != new_digest {
                    return Err(invalid().into());
                }
                child(owner, Some(&old), &new, ctx, cells, max)?;
            }
            (Some(_), None) => {
                if matches!(field, DiplomacyFieldV1::Alliances | DiplomacyFieldV1::Wars) {
                    return Err(invalid().into());
                }
                counts[index] = counts[index].checked_sub(1).ok_or_else(invalid)?;
            }
            (None, None) => return Err(invalid().into()),
        }
        ctx.check()?;
    }
    let proof = ctx
        .values
        .memory
        .reserve(512, ctx.values.class)
        .map_err(ValueTreeError::Memory)?
        .retain(DiplomacyFamilyQualificationV1 {
            roots: verified.record.after.digest().map_err(|_| invalid())?,
            counts,
        });
    ctx.check()?;
    Ok(proof)
}
