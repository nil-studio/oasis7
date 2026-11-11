//! Independent replay and changed-original-record closure for module grants/rules.
use super::*;
use crate::incremental_field_overlay::WorldFieldOverlayErrorV1;
use oasis7_formal_protocol::incremental_execution::{TransitionRecordV1, TransitionVerifier};

pub fn accept_module_authorization_transition_v1<M, P, R, E, C>(
    parent: &ReservedValue<ModuleAuthorizationFamilyQualificationV1>,
    verifier: &TransitionVerifier<'_, P>,
    record: &TransitionRecordV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    body_scan: &mut StateScanBudget,
    work: &mut ModuleAuthorizationReadWorkV1,
) -> Result<ReservedValue<ModuleAuthorizationFamilyQualificationV1>, WorldFieldOverlayErrorV1>
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
        if !parent.uses_budget(ctx.values.memory)
            || parent.reserved_bytes() < 512
            || parent.class != ctx.values.class
            || !verifier.memory.same_pool(ctx.values.memory)
            || verifier.class != ctx.values.class
        {
            return Err(invalid().into());
        }
        parent.validate_binding(&record.before)?;
        let _phase = ctx
            .values
            .memory
            .reserve(64 << 10, ctx.values.class)
            .map_err(ValueTreeError::Memory)?;
    }
    let witness = crate::incremental_verified_world_transition::verify_world_transition_v1(
        verifier, record, ctx,
    )?;
    accept_module_authorization_verified_v1(parent, &witness, ctx, body_scan, work)
}

pub(crate) fn accept_module_authorization_verified_v1<M, R, E, C>(
    parent: &ReservedValue<ModuleAuthorizationFamilyQualificationV1>,
    witness: &crate::incremental_verified_world_transition::VerifiedWorldTransitionV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    body_scan: &mut StateScanBudget,
    work: &mut ModuleAuthorizationReadWorkV1,
) -> Result<ReservedValue<ModuleAuthorizationFamilyQualificationV1>, WorldFieldOverlayErrorV1>
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
    if !parent.uses_budget(ctx.values.memory)
        || parent.reserved_bytes() < 512
        || parent.class != ctx.values.class
    {
        return Err(invalid().into());
    }
    parent.validate_binding(&record.before)?;
    let _phase = ctx
        .values
        .memory
        .reserve(64 << 10, ctx.values.class)
        .map_err(ValueTreeError::Memory)?;
    let verified = witness.prepared();
    let record = &verified.record;
    if ctx.root != record.after.state_root(StateNamespaceV1::Authorization) {
        return Err(invalid().into());
    }
    let mut length = parent.rules;
    // Read the final mandatory length first; policy insert/delete checks below
    // then prove the ordinal interval without a collection-sized index list.
    for write in &record.writes {
        ctx.check()?;
        let Some((owner, key)) = field(write.namespace, &write.mutation.key)? else {
            continue;
        };
        if owner == ModuleAuthorizationFieldV1::PolicyRules {
            let path = length_key(&mut ctx.records())?;
            if path.0 == key {
                if write.mutation.before.is_none() {
                    return Err(invalid().into());
                }
                let value = write.mutation.after.ok_or_else(invalid)?;
                let bytes =
                    ctx.records()
                        .fixed::<8>(&value, &key, path.1, MemberPayloadCodecV1::U64)?;
                length = u64::from_be_bytes(bytes);
            }
        }
    }
    let mut grants = parent.grants;
    let mut rules = parent.rules;
    for write in &record.writes {
        ctx.check()?;
        let Some((owner, key)) = field(write.namespace, &write.mutation.key)? else {
            continue;
        };
        work.cell()?;
        let binding =
            WorldFieldKeyV1::from_bytes(key.as_bytes()).map_err(ValueTreeError::Invalid)?;
        match binding.kind {
            WorldFieldRecordKindV1::Metadata => {
                if write.mutation.before.is_none() || write.mutation.before != write.mutation.after
                {
                    return Err(invalid().into());
                }
                ctx.records()
                    .presence(&write.mutation.after.ok_or_else(invalid)?, &key, 0)?;
                continue;
            }
            WorldFieldRecordKindV1::Entry => {}
            _ => return Err(invalid().into()),
        }
        if owner == ModuleAuthorizationFieldV1::PolicyRules
            && length_key(&mut ctx.records())?.0 == key
        {
            continue;
        }
        if write.mutation.before == write.mutation.after {
            continue;
        }
        let old = write.mutation.before;
        let after = write.mutation.after;
        let mut index = None;
        // Old body qualification is inherited; only its bounded outer binding
        // is needed to authenticate a removed ordinal.
        if after.is_none() {
            let old = old.ok_or_else(invalid)?;
            index = outer(owner, &key, &old, &mut ctx.records())?.1;
        }
        if let Some(value) = after {
            let (child, ordinal) = outer(owner, &key, &value, &mut ctx.records())?;
            index = ordinal;
            let mut local = WorldFieldReadContextV1 {
                root: &child,
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
            if let Some(old) = old {
                let (before_child, _) = outer(owner, &key, &old, &mut local.records())?;
                super::super::closure::advance_module_authorization_body_v1(
                    owner,
                    &before_child,
                    &mut local,
                    body_scan,
                    work,
                )?;
            } else {
                qualify_module_authorization_body_v1(owner, &mut local, body_scan, work)?;
            }
        }
        if owner == ModuleAuthorizationFieldV1::PolicyRules {
            let index = index.ok_or_else(invalid)?;
            match (old.is_some(), after.is_some()) {
                (false, true) if index >= parent.rules && index < length => {}
                (true, false) if index >= length => {}
                (true, true) if index < length => {}
                _ => return Err(invalid().into()),
            }
        }
        let count = if owner == ModuleAuthorizationFieldV1::Capabilities {
            &mut grants
        } else {
            &mut rules
        };
        match (old.is_some(), after.is_some()) {
            (false, true) => *count = count.checked_add(1).ok_or_else(invalid)?,
            (true, false) => *count = count.checked_sub(1).ok_or_else(invalid)?,
            (true, true) => {}
            _ => return Err(invalid().into()),
        }
    }
    if rules != length {
        return Err(invalid().into());
    }
    ctx.check()?;
    let result = mint(
        record.after.digest().map_err(|_| invalid())?,
        ctx.values.class,
        grants,
        length,
        ctx.values.memory,
    )?;
    ctx.check()?;
    Ok(result)
}
