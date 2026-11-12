//! Owner closure acceptance over authenticated changed outer/inner paths.
use super::*;
use crate::incremental_field_overlay::WorldFieldOverlayErrorV1;
use oasis7_formal_protocol::incremental_execution::{TransitionRecordV1, TransitionVerifier};
use oasis7_proto::memory_budget::ReservedValue;
use std::cell::RefCell;
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("organization transition closure")
}
fn index(owner: OrganizationRootKeyV1<'_>) -> usize {
    match owner {
        OrganizationRootKeyV1::Case(_) => 0,
        OrganizationRootKeyV1::Estate(_) => 1,
    }
}
pub(super) fn edge<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    key: &WorldFieldKeyV1,
    value: Option<[u8; 32]>,
    context: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<
    Option<(
        OrganizationRootKeyV1<'static>,
        ReservedValue<StateRootV1>,
        ReservedValue<String>,
    )>,
    ValueTreeError,
> {
    let Some(value) = value else {
        return Ok(None);
    };
    let info = crate::incremental_field_member::read_member_record_header_v1(
        &value,
        key,
        &mut context.values,
        MemberRecordLimitsV1 {
            semantic_bytes: context.semantic_bytes,
            payload_bytes: context.record_bytes,
            record_bytes: context.record_bytes,
        },
        &mut context.cancelled,
    )?;
    let Some((owner, outer)) = scope::original_outer(&value, key, context)? else {
        return Ok(None);
    };
    if info.codec != MemberPayloadCodecV1::RecordRoot
        || info.payload_bytes != RECORD_ROOT_BYTES as u64
    {
        return Err(invalid());
    }
    let retained = context
        .values
        .memory
        .reserve(512 + 4096, context.values.class)?;
    let mut bytes = [0; RECORD_ROOT_BYTES];
    read_member_record_profile_v1(
        &value,
        key,
        &mut context.values,
        MemberReadProfileV1 {
            codec: MemberPayloadCodecV1::RecordRoot,
            semantic_bytes: info.semantic_bytes,
            limits: MemberRecordLimitsV1 {
                semantic_bytes: info.semantic_bytes,
                payload_bytes: RECORD_ROOT_BYTES as u64,
                record_bytes: context.record_bytes,
            },
        },
        &mut std::io::sink(),
        &mut Cursor::new(bytes.as_mut_slice()),
        &mut context.cancelled,
    )?;
    let reference = RecordRootRefV1::from_canonical_bytes(&bytes).map_err(|_| invalid())?;
    let digest = WorldFieldKeyV1::from_bytes(key.as_bytes())
        .map_err(ValueTreeError::Invalid)?
        .semantic_key
        .ok_or_else(invalid)?;
    let child = reference
        .validate_binding(&owner.schema(), &digest, StateNamespaceV1::Authorization)
        .map_err(|_| invalid())?;
    if (context.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(Some((owner, retained.retain(child.clone()), outer)))
}
pub(super) fn child<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    owner: OrganizationRootKeyV1<'_>,
    before: Option<&StateRootV1>,
    after: &StateRootV1,
    context: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    cells: &mut u64,
    max_payload: u64,
) -> Result<(), ValueTreeError> {
    let memory = context.values.memory;
    let class = context.values.class;
    let state = context.state;
    let _fixed = memory.reserve(16 << 10, class)?;
    let empty = StateRootV1::empty(StateNamespaceV1::Authorization).map_err(|_| invalid())?;
    let old = before.unwrap_or(&empty);
    let old_length = if index(owner) == 0 && before.is_some() {
        let mut local = WorldFieldReadContextV1 {
            root: old,
            state,
            values: oasis7_formal_protocol::incremental_value::ValueReadContext {
                source: context.values.source,
                memory,
                class,
                io: &mut *context.values.io,
            },
            key_work: &mut *context.key_work,
            semantic_bytes: context.semantic_bytes,
            record_bytes: context.record_bytes,
            take_state_error: &mut context.take_state_error,
            cancelled: &mut context.cancelled,
        };
        closure::length(&mut local)?
    } else {
        0
    };
    let cancelled = RefCell::new(&mut context.cancelled);
    let mut local = WorldFieldReadContextV1 {
        root: after,
        state,
        values: oasis7_formal_protocol::incremental_value::ValueReadContext {
            source: context.values.source,
            memory,
            class,
            io: &mut *context.values.io,
        },
        key_work: &mut *context.key_work,
        semantic_bytes: context.semantic_bytes,
        record_bytes: context.record_bytes,
        take_state_error: &mut context.take_state_error,
        cancelled: || (cancelled.borrow_mut())(),
    };
    closure::relations(closure::owner_record(owner), &mut local, false)?;
    let new_length = if index(owner) == 0 {
        closure::length(&mut local)?
    } else {
        0
    };
    let mut receipt_counts = [0u64; 2];
    let mut cancel = || (cancelled.borrow_mut())();
    let result =
        OrganizationChangedTreeContextV1 {
            source: state,
            memory,
            class,
            budget: scan,
            cancelled: &mut cancel,
        }
        .visit(old, after, |old, new| {
            *cells = cells.checked_sub(1).ok_or(ValueTreeError::ResourceLimited(
                "organization changed cells",
            ))?;
            if let Some(entry) = new
                && closure::actual(owner, entry, &mut local, max_payload)?
            {
                receipt_counts[1] = receipt_counts[1].checked_add(1).ok_or_else(invalid)?;
            }
            if let Some(entry) = old {
                let (_, _, semantic) = closure::identity(entry, &mut local, context.record_bytes)?;
                let parsed = inspect_organization_relative_key_v1(owner, &semantic)?;
                if matches!(parsed, OrganizationRelativeKeyV1::Receipt(_)) {
                    receipt_counts[0] = receipt_counts[0].checked_add(1).ok_or_else(invalid)?;
                }
                if new.is_none() {
                    match parsed {
                        OrganizationRelativeKeyV1::Column {
                            record:
                                OrganizationRelativeRecordV1::Case
                                | OrganizationRelativeRecordV1::Estate,
                            ..
                        } => {
                            closure::relations(closure::owner_record(owner), &mut local, false)?;
                        }
                        OrganizationRelativeKeyV1::Column {
                            record: OrganizationRelativeRecordV1::Grant(_),
                            slot: 0,
                        } => {
                            return Err(ValueTreeError::Invalid(
                                "organization whole Grant deletion lacks relationship proof",
                            ));
                        }
                        OrganizationRelativeKeyV1::Column {
                            record: record @ OrganizationRelativeRecordV1::Grant(_),
                            ..
                        } => {
                            closure::relations(record, &mut local, false)?;
                        }
                        OrganizationRelativeKeyV1::Column {
                            record: record @ OrganizationRelativeRecordV1::Recovery(_),
                            ..
                        } => {
                            closure::relations(record, &mut local, true)?;
                        }
                        OrganizationRelativeKeyV1::Scope { grant, .. } => {
                            closure::relations(
                                OrganizationRelativeRecordV1::Grant(grant),
                                &mut local,
                                false,
                            )?;
                        }
                        OrganizationRelativeKeyV1::Receipt(ordinal) if ordinal < new_length => {
                            return Err(ValueTreeError::Invalid(
                                "organization receipt deletion leaves a gap",
                            ));
                        }
                        OrganizationRelativeKeyV1::Receipt(_)
                        | OrganizationRelativeKeyV1::Member(_) => {}
                    }
                }
            }
            if (cancelled.borrow_mut())() {
                return Err(ValueTreeError::Cancelled);
            }
            Ok(())
        });
    let original = (local.take_state_error)();
    if (cancelled.borrow_mut())() {
        return Err(ValueTreeError::Cancelled);
    }
    if let Some(error) = original {
        return Err(error);
    }
    result?;
    if index(owner) == 0
        && i128::from(old_length) + i128::from(receipt_counts[1]) - i128::from(receipt_counts[0])
            != i128::from(new_length)
    {
        return Err(ValueTreeError::Invalid("organization receipt length delta"));
    }
    if (cancelled.borrow_mut())() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(())
}
/// Integrity/schema acceptance only. The exact qualified parent supplies all
/// unchanged owner closure; neither generic verification nor a codec token does.
fn organization_preflight<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    parent: &ReservedValue<OrganizationFamilyQualificationV1>,
    record: &TransitionRecordV1,
    context: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<oasis7_proto::memory_budget::MemoryReservation, WorldFieldOverlayErrorV1> {
    context.check()?;
    if context.values.class != MemoryClass::Ordinary
        || !parent.uses_budget(context.values.memory)
        || !parent.permits_class(MemoryClass::Ordinary)
        || parent.reserved_bytes() < std::mem::size_of::<OrganizationFamilyQualificationV1>()
        || context.root != record.after.state_root(StateNamespaceV1::Authorization)
    {
        return Err(invalid().into());
    }
    let phase = context
        .values
        .memory
        .reserve(128 << 10, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    parent.validate_binding(&record.before.digest().map_err(|_| invalid())?)?;
    context.check()?;
    Ok(phase)
}
pub fn accept_organization_transition_v1<
    M: MerkleNodeReader,
    S: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    parent: &ReservedValue<OrganizationFamilyQualificationV1>,
    verifier: &TransitionVerifier<'_, S>,
    record: &TransitionRecordV1,
    context: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    cells: &mut u64,
    max_payload: u64,
) -> Result<ReservedValue<OrganizationFamilyQualificationV1>, WorldFieldOverlayErrorV1> {
    context.check()?;
    if verifier.class != MemoryClass::Ordinary || !verifier.memory.same_pool(context.values.memory)
    {
        return Err(invalid().into());
    }
    {
        let _phase = organization_preflight(parent, record, context)?;
    }
    let witness = crate::incremental_verified_world_transition::verify_world_transition_v1(
        verifier, record, context,
    )?;
    let result =
        accept_organization_verified_v1(parent, &witness, context, scan, cells, max_payload);
    context.check()?;
    result
}
#[allow(clippy::too_many_arguments)]
pub(crate) fn accept_organization_verified_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    parent: &ReservedValue<OrganizationFamilyQualificationV1>,
    witness: &crate::incremental_verified_world_transition::VerifiedWorldTransitionV1,
    context: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    cells: &mut u64,
    max_payload: u64,
) -> Result<ReservedValue<OrganizationFamilyQualificationV1>, WorldFieldOverlayErrorV1> {
    context.check()?;
    witness.check_context(context.values.memory, context.values.class, context.root)?;
    let verified = witness.prepared();
    let _phase = organization_preflight(parent, &verified.record, context)?;
    let mut counts = parent.record_counts();
    let mut scope: Option<
        crate::incremental_organization_scope_index::OrganizationScopeIndexBuilderV1,
    > = None;
    for write in &verified.record.writes {
        *cells = cells.checked_sub(1).ok_or(ValueTreeError::ResourceLimited(
            "organization outer changed cells",
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
        if binding.name != "world_rules" {
            continue;
        }
        let key = WorldFieldKeyV1::new(
            crate::incremental_world_rule_parent::descriptor()?,
            binding.kind,
            binding.semantic_key,
        )
        .map_err(ValueTreeError::Invalid)?;
        if write.namespace != StateNamespaceV1::Authorization
            || key.as_bytes() != write.mutation.key
        {
            return Err(invalid().into());
        }
        if binding.kind == WorldFieldRecordKindV1::Metadata {
            continue;
        }
        if binding.kind != WorldFieldRecordKindV1::Entry {
            return Err(invalid().into());
        }
        let old = edge(&key, write.mutation.before, context)?;
        let new = edge(&key, write.mutation.after, context)?;
        if old.is_some() || new.is_some() {
            if scope.is_none() {
                scope = Some(parent.scope.copy_for_changes(
                    context.values.memory,
                    context.values.class,
                    &mut |s, b| scope::spend(scan, &mut context.cancelled, s, b),
                )?);
            }
        }
        match (old, new) {
            (None, None) => {}
            (Some((owner, _, outer)), None) => {
                if write.mutation.after.is_some() || index(owner) == 0 {
                    return Err(ValueTreeError::Invalid(
                        "organization whole Case deletion lacks relationship proof",
                    )
                    .into());
                }
                counts[index(owner)] = counts[index(owner)].checked_sub(1).ok_or_else(invalid)?;
                scope.as_mut().ok_or_else(invalid)?.remove(match owner{OrganizationRootKeyV1::Case(_)=>crate::incremental_organization_scope_index::OrganizationScopeKindV1::Protection,OrganizationRootKeyV1::Estate(_)=>crate::incremental_organization_scope_index::OrganizationScopeKindV1::Estate},&outer,&mut|s,b|scope::spend(scan,&mut context.cancelled,s,b))?;
            }
            (old, Some((owner, new, outer))) => {
                if old
                    .as_ref()
                    .is_some_and(|(old_owner, _, _)| index(*old_owner) != index(owner))
                    || old.is_none() && write.mutation.before.is_some()
                {
                    return Err(invalid().into());
                }
                if old.is_none() {
                    counts[index(owner)] =
                        counts[index(owner)].checked_add(1).ok_or_else(invalid)?;
                }
                child(
                    owner,
                    old.as_ref().map(|(_, root, _)| &**root),
                    &new,
                    context,
                    scan,
                    cells,
                    max_payload,
                )?;
                let digest = WorldFieldKeyV1::from_bytes(key.as_bytes())
                    .map_err(ValueTreeError::Invalid)?
                    .semantic_key
                    .ok_or_else(invalid)?;
                scope::project(
                    owner,
                    &outer,
                    digest,
                    &new,
                    context,
                    scan,
                    cells,
                    max_payload,
                    scope.as_mut().ok_or_else(invalid)?,
                )?;
            }
        }
        if (context.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
    }
    let (_, present) = crate::incremental_world_rule_parent::read_parent_fixed(context)?;
    if !present && counts != [0; 2] {
        return Err(invalid().into());
    }
    let result = family::advanced(
        verified.record.after.digest().map_err(|_| invalid())?,
        counts,
        present,
        context.values.memory,
        context.values.class,
        match scope {
            Some(scope) => scope.finish()?,
            None => parent
                .scope
                .share(context.values.memory, context.values.class)?,
        },
    )?;
    if (context.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(result)
}
