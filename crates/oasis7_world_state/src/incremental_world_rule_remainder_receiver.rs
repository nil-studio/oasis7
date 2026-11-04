use super::*;
use crate::incremental_field_overlay::WorldFieldOverlayErrorV1;
fn preflight<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    parent: &ReservedValue<WorldRuleRemainderQualificationV1>,
    record: &TransitionRecordV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<oasis7_proto::memory_budget::MemoryReservation, WorldFieldOverlayErrorV1> {
    ctx.check()?;
    if ctx.values.class != MemoryClass::Ordinary
        || parent.class != MemoryClass::Ordinary
        || !parent.uses_budget(ctx.values.memory)
        || parent.reserved_bytes() < 512
        || ctx.root != record.after.state_root(StateNamespaceV1::Authorization)
    {
        return Err(invalid().into());
    }
    let phase = ctx
        .values
        .memory
        .reserve(8192, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    parent.validate_binding(&record.before)?;
    ctx.check()?;
    Ok(phase)
}
pub fn accept_world_rule_remainder_transition_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    parent: &ReservedValue<WorldRuleRemainderQualificationV1>,
    verifier: &TransitionVerifier<'_, P>,
    record: &TransitionRecordV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut WorldRuleRemainderWorkV1,
) -> Result<ReservedValue<WorldRuleRemainderQualificationV1>, WorldFieldOverlayErrorV1> {
    let result = (|| {
        ctx.check()?;
        if verifier.class != MemoryClass::Ordinary || !verifier.memory.same_pool(ctx.values.memory)
        {
            return Err(invalid().into());
        }
        {
            let _phase = preflight(parent, record, ctx)?;
        }
        let witness = crate::incremental_verified_world_transition::verify_world_transition_v1(
            verifier, record, ctx,
        )?;
        accept_world_rule_remainder_verified_v1(parent, &witness, ctx, work)
    })();
    ctx.check()?;
    result
}
pub(crate) fn accept_world_rule_remainder_verified_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    parent: &ReservedValue<WorldRuleRemainderQualificationV1>,
    witness: &crate::incremental_verified_world_transition::VerifiedWorldTransitionV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut WorldRuleRemainderWorkV1,
) -> Result<ReservedValue<WorldRuleRemainderQualificationV1>, WorldFieldOverlayErrorV1> {
    let result = (|| {
        ctx.check()?;
        witness.check_context(ctx.values.memory, ctx.values.class, ctx.root)?;
        let verified = witness.prepared();
        let memory = ctx.values.memory;
        let _phase = preflight(parent, &verified.record, ctx)?;
        let charge = memory
            .reserve(512, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let (_, present) = read_parent_fixed(ctx)?;
        let mut counts = parent.counts;
        for write in &verified.record.writes {
            work.step(&mut ctx.cancelled)?;
            ctx.check()?;
            let Some(key) = key(&write.mutation.key)? else {
                continue;
            };
            if write.namespace != StateNamespaceV1::Authorization {
                return Err(invalid().into());
            }
            for (id, add) in [(write.mutation.before, false), (write.mutation.after, true)] {
                let Some(id) = id else {
                    continue;
                };
                if let Some(i) = inspect(&key, &id, present, add, ctx, work)? {
                    counts[i] = if add {
                        counts[i].checked_add(1)
                    } else {
                        counts[i].checked_sub(1)
                    }
                    .ok_or_else(invalid)?;
                }
            }
        }
        if !present && counts != [0; 3] {
            return Err(invalid().into());
        }
        ctx.check()?;
        let roots = verified.record.after.digest().map_err(|_| invalid())?;
        ctx.check()?;
        Ok(charge.retain(WorldRuleRemainderQualificationV1 {
            roots,
            class: MemoryClass::Ordinary,
            present,
            counts,
        }))
    })();
    ctx.check()?;
    result
}
