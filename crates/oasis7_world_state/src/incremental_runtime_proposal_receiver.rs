//! Independent ordinary proposal replay inherits the exact qualified parent.
use super::*;
use crate::incremental_field_overlay::WorldFieldOverlayErrorV1;
use oasis7_formal_protocol::incremental_execution::{TransitionRecordV1, TransitionVerifier};
pub fn accept_runtime_proposals_transition_v1<M, P, R, E, C>(
    parent: &ReservedValue<RuntimeProposalsQualificationV1>,
    verifier: &TransitionVerifier<'_, P>,
    record: &TransitionRecordV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut RuntimeProposalReadWorkV1,
) -> Result<ReservedValue<RuntimeProposalsQualificationV1>, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    ctx.check()?;
    if ctx.values.class != oasis7_proto::memory_budget::MemoryClass::Ordinary
        || !record.after.state.iter().any(|root| root == ctx.root)
    {
        return Err(invalid().into());
    }
    {
        ctx.check()?;
        parent.validate_binding(&record.before)?;
        if !parent.uses_budget(ctx.values.memory)
            || parent.reserved_bytes() < 512
            || parent.class != MemoryClass::Ordinary
            || ctx.values.class != MemoryClass::Ordinary
            || verifier.class != MemoryClass::Ordinary
            || !verifier.memory.same_pool(ctx.values.memory)
            || ctx.root != record.after.state_root(StateNamespaceV1::Authorization)
        {
            return Err(invalid().into());
        }
        let _retained = ctx
            .values
            .memory
            .reserve(512, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let _scratch = ctx
            .values
            .memory
            .reserve(32 << 10, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
    }
    let witness = crate::incremental_verified_world_transition::verify_world_transition_v1(
        verifier, record, ctx,
    )?;
    accept_runtime_proposals_verified_v1(parent, &witness, ctx, scan, work)
}

pub(crate) fn accept_runtime_proposals_verified_v1<M, R, E, C>(
    parent: &ReservedValue<RuntimeProposalsQualificationV1>,
    witness: &crate::incremental_verified_world_transition::VerifiedWorldTransitionV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut RuntimeProposalReadWorkV1,
) -> Result<ReservedValue<RuntimeProposalsQualificationV1>, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    ctx.check()?;
    witness.check_context(ctx.values.memory, ctx.values.class, ctx.root)?;
    if ctx.values.class != oasis7_proto::memory_budget::MemoryClass::Ordinary {
        return Err(invalid().into());
    }
    let record = witness.record();

    ctx.check()?;
    parent.validate_binding(&record.before)?;
    if !parent.uses_budget(ctx.values.memory)
        || parent.reserved_bytes() < 512
        || parent.class != MemoryClass::Ordinary
        || ctx.values.class != MemoryClass::Ordinary
        || ctx.root != record.after.state_root(StateNamespaceV1::Authorization)
    {
        return Err(invalid().into());
    }
    let retained = ctx
        .values
        .memory
        .reserve(512, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let _scratch = ctx
        .values
        .memory
        .reserve(32 << 10, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let checked = witness.prepared();
    let record = &checked.record;
    let mut count = i128::from(parent.count);
    for write in &record.writes {
        ctx.check()?;
        if !write
            .mutation
            .key
            .starts_with(b"oasis7.iws.world-field-key.v1\0")
        {
            continue;
        }
        let parsed =
            WorldFieldKeyV1::from_bytes(&write.mutation.key).map_err(ValueTreeError::Invalid)?;
        if parsed.owner != FieldOwner::Runtime || parsed.name != "proposals" {
            continue;
        }
        if parsed.namespace != StateNamespaceV1::Authorization
            || write.namespace != parsed.namespace
        {
            return Err(invalid().into());
        }
        let key = WorldFieldKeyV1::new(
            runtime_proposal_descriptor_v1(),
            parsed.kind,
            parsed.semantic_key,
        )
        .map_err(ValueTreeError::Invalid)?;
        if key.as_bytes() != write.mutation.key {
            return Err(invalid().into());
        }
        match parsed.kind {
            WorldFieldRecordKindV1::Metadata => {
                if parsed.semantic_key.is_some()
                    || write.mutation.before.is_none()
                    || write.mutation.before != write.mutation.after
                {
                    return Err(invalid().into());
                }
            }
            WorldFieldRecordKindV1::Entry => {
                count += i128::from(write.mutation.after.is_some())
                    - i128::from(write.mutation.before.is_some());
                if let Some(value) = write.mutation.after {
                    let (original, after) = outer_child(&key, &value, ctx)?;
                    if let Some(before) = write.mutation.before {
                        let (old_key, before) = outer_child(&key, &before, ctx)?;
                        if old_key != original {
                            return Err(invalid().into());
                        }
                        delta::advance_body(&before, &after, ctx, scan, work)?;
                    } else {
                        stream::qualify(&after, ctx, scan, work)?;
                    }
                }
            }
            _ => return Err(invalid().into()),
        }
    }
    let count = u64::try_from(count).map_err(|_| invalid())?;
    ctx.check()?;
    Ok(retained.retain(RuntimeProposalsQualificationV1 {
        roots: record.after.digest().map_err(|_| invalid())?,
        count,
        class: MemoryClass::Ordinary,
    }))
}
