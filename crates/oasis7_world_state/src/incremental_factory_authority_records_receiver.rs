//! Replay integrity plus changed-path closure inherited from a qualified parent.
use super::*;
use crate::{
    incremental_field_overlay::WorldFieldOverlayErrorV1,
    incremental_field_read::WorldFieldReadContextV1,
};
use oasis7_formal_protocol::{
    incremental_execution::{TransitionRecordV1, TransitionVerifier},
    incremental_state::{MerkleNodeReader, StateEntryV1, StateScanBudget},
    incremental_value::{ValueNodeReader, ValueReadContext},
};
use oasis7_proto::memory_budget::ReservedValue;
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("factory_authority changed closure")
}
/// Requires the exact parent family token and independent generic replay.
/// Non-factory_authority owners retain responsibility for their own typed closures.
pub fn accept_factory_authority_transition_v1<
    M: MerkleNodeReader,
    S: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    parent: &ReservedValue<FactoryAuthorityFamilyQualificationV1>,
    verifier: &TransitionVerifier<'_, S>,
    record: &TransitionRecordV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    inner_scan: &mut StateScanBudget,
    cells: &mut u64,
    max: u64,
) -> Result<ReservedValue<FactoryAuthorityFamilyQualificationV1>, WorldFieldOverlayErrorV1> {
    ctx.check()?;
    if !parent.uses_budget(ctx.values.memory)
        || !verifier.memory.same_pool(ctx.values.memory)
        || verifier.class != ctx.values.class
    {
        return Err(invalid().into());
    }
    parent.validate_binding(&record.before.digest().map_err(|_| invalid())?)?;
    let verified = verifier.verify(record);
    let error = (ctx.take_state_error)();
    ctx.check()?;
    if let Some(error) = error {
        return Err(error.into());
    }
    let verified = verified?;
    if !verified.record.after.state.iter().any(|r| r == ctx.root) {
        return Err(invalid().into());
    }
    let mut counts = parent.record_counts();
    for write in &verified.record.writes {
        *cells = cells.checked_sub(1).ok_or(ValueTreeError::ResourceLimited(
            "factory_authority outer changed cells",
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
        let Some(kind) = FactoryAuthorityRecordKindV1::ALL
            .into_iter()
            .find(|k| k.field_name() == binding.name)
        else {
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
        if binding.kind == WorldFieldRecordKindV1::Metadata || old.is_none() && new.is_none() {
            // Metadata and format markers cannot be removed or freshly introduced.
            if write.mutation.before.is_none() || write.mutation.after.is_none() {
                return Err(invalid().into());
            }
            continue;
        }
        let index = match kind {
            FactoryAuthorityRecordKindV1::Site => 0,
            FactoryAuthorityRecordKindV1::Power => 1,
        };
        match (old, new) {
            (None, Some((owner_kind, owner, root))) => {
                let mut body = WorldFieldReadContextV1 {
                    root: &root,
                    state: ctx.state,
                    values: ValueReadContext {
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
                qualify_factory_authority_body_v1(
                    owner_kind, owner, &mut body, inner_scan, cells, max,
                )?;
                counts[index] = counts[index].checked_add(1).ok_or_else(invalid)?;
            }
            (Some((old_kind, old_owner, old)), Some((new_kind, new_owner, new))) => {
                if old_kind != new_kind || old_owner != new_owner {
                    return Err(invalid().into());
                }
                advance::child(kind, &old, &new, &mut ctx.records(), cells, max)?;
            }
            (Some(_), None) => {
                counts[index] = counts[index].checked_sub(1).ok_or_else(invalid)?;
            }
            (None, None) => return Err(invalid().into()),
        }
        ctx.check()?;
    }
    let result = family::mint(
        verified.record.after.digest().map_err(|_| invalid())?,
        counts,
        ctx.values.memory,
        ctx.values.class,
    )?;
    ctx.check()?;
    Ok(result)
}
