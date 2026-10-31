//! Owner closure acceptance over authenticated changed outer/inner paths.
use super::*;
use crate::incremental_field_overlay::WorldFieldOverlayErrorV1;
use oasis7_formal_protocol::incremental_execution::{TransitionRecordV1, TransitionVerifier};
use oasis7_proto::memory_budget::ReservedValue;
use std::cell::RefCell;
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("supply transition closure")
}
fn index(owner: SupplyRootKeyV1<'_>) -> usize {
    match owner {
        SupplyRootKeyV1::Authorization(_) => 0,
        SupplyRootKeyV1::Batch(_) => 1,
    }
}
fn edge<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    key: &WorldFieldKeyV1,
    value: Option<[u8; 32]>,
    context: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<Option<(SupplyRootKeyV1<'static>, ReservedValue<StateRootV1>)>, ValueTreeError> {
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
    let root = context.values.load_root(&value)?;
    let mut sink = std::io::sink();
    let mut probe = family::outer_key::OuterProbe::new(&mut sink);
    let result = context.values.read_range(
        &root,
        (FIELD_MEMBER_HEADER_BYTES + key.as_bytes().len()) as u64,
        info.semantic_bytes,
        &mut probe,
        &mut context.cancelled,
    );
    if let Some(reason) = probe.bad {
        return Err(ValueTreeError::Invalid(reason));
    }
    result?;
    let Some(owner) = probe.finish()? else {
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
    Ok(Some((owner, retained.retain(child.clone()))))
}
fn child<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    owner: SupplyRootKeyV1<'_>,
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
    closure::relations(owner, &mut local)?;
    let mut cancel = || (cancelled.borrow_mut())();
    let result =
        crate::incremental_world_rule_organization::root::OrganizationChangedTreeContextV1 {
            source: state,
            memory,
            class,
            budget: scan,
            cancelled: &mut cancel,
        }
        .visit(old, after, |old, new| {
            *cells = cells
                .checked_sub(1)
                .ok_or(ValueTreeError::ResourceLimited("supply changed cells"))?;
            if let Some(entry) = new {
                closure::actual(owner, entry, &mut local, max_payload)?;
            }
            if new.is_none()
                && let Some(entry) = old
            {
                let (_, _, semantic) =
                    crate::incremental_world_rule_organization::root::closure::identity(
                        entry,
                        &mut local,
                        max_payload,
                    )?;
                match schema::inspect(owner, &semantic)? {
                    schema::Key::Column(_) => closure::relations(owner, &mut local)?,
                    schema::Key::Member("frozen_lineages", member)
                    | schema::Key::FrozenField(member) => {
                        closure::frozen_relation(owner, member, &mut local)?
                    }
                    schema::Key::Member(_, _) => {}
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
    if (cancelled.borrow_mut())() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(())
}
/// Integrity/schema acceptance only. The exact qualified parent supplies all
/// unchanged owner closure; neither generic verification nor a codec token does.
pub fn accept_supply_transition_v1<
    M: MerkleNodeReader,
    S: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    parent: &ReservedValue<SupplyFamilyQualificationV1>,
    verifier: &TransitionVerifier<'_, S>,
    record: &TransitionRecordV1,
    context: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    cells: &mut u64,
    max_payload: u64,
) -> Result<ReservedValue<SupplyFamilyQualificationV1>, WorldFieldOverlayErrorV1> {
    context.check()?;
    if !parent.uses_budget(context.values.memory)
        || !verifier.memory.same_pool(context.values.memory)
        || verifier.class != context.values.class
    {
        return Err(ValueTreeError::Invalid("supply receiver pool/class").into());
    }
    parent.validate_binding(&record.before.digest().map_err(|_| invalid())?)?;
    let verified = verifier.verify(record);
    let original = (context.take_state_error)();
    if (context.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    if let Some(error) = original {
        return Err(error.into());
    }
    let verified = verified?;
    let auth = verified
        .record
        .after
        .state
        .iter()
        .find(|root| root.namespace == StateNamespaceV1::Authorization)
        .ok_or_else(invalid)?;
    if context.root != auth {
        return Err(invalid().into());
    }
    let mut counts = parent.record_counts();
    for write in &verified.record.writes {
        *cells = cells.checked_sub(1).ok_or(ValueTreeError::ResourceLimited(
            "supply outer changed cells",
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
        match (old, new) {
            (None, None) => {}
            (Some((owner, _)), None) => {
                if write.mutation.after.is_some() || write.mutation.before.is_some() {
                    return Err(ValueTreeError::Invalid(
                        "supply whole record deletion lacks relationship proof",
                    )
                    .into());
                }
                counts[index(owner)] = counts[index(owner)].checked_sub(1).ok_or_else(invalid)?;
            }
            (old, Some((owner, new))) => {
                if old
                    .as_ref()
                    .is_some_and(|(old_owner, _)| index(*old_owner) != index(owner))
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
                    old.as_ref().map(|(_, root)| &**root),
                    &new,
                    context,
                    scan,
                    cells,
                    max_payload,
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
    )?;
    if (context.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(result)
}
