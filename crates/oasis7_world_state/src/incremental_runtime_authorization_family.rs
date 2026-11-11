//! Complete audit and invocation families bound to one exact RootSet.
use crate::{
    incremental_field_member::*, incremental_field_read::WorldFieldReadContextV1,
    incremental_json_records::JsonReadWorkV1, incremental_runtime_authorization_decode::*,
};
use oasis7_formal_protocol::{
    incremental_execution::{RootSetV1, TransitionRecordV1, TransitionVerifier},
    incremental_record_root::{RECORD_ROOT_BYTES, RecordRootRefV1},
    incremental_state::{
        MerkleNodeReader, StateNamespaceV1, StateRootV1, StateScanBudget, StateScanContext,
        StateScanError,
    },
    incremental_value::{ValueNodeReader, ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
use std::cell::RefCell;
pub struct AuthorizationRecordFamiliesQualificationV1 {
    roots: [u8; 32],
    counts: [u64; 2],
    class: MemoryClass,
}
impl AuthorizationRecordFamiliesQualificationV1 {
    pub fn audit_receipts(&self) -> u64 {
        self.counts[0]
    }
    pub fn invocation_contexts(&self) -> u64 {
        self.counts[1]
    }
    pub fn validate_binding(&self, roots: &RootSetV1) -> Result<(), ValueTreeError> {
        if roots.digest().map_err(|_| invalid())? != self.roots {
            return Err(invalid());
        }
        Ok(())
    }
}
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("authorization record family binding")
}
fn identify(name: &str) -> Option<(usize, AuthorizationBodyKindV1)> {
    match name {
        "capability_authorization_receipts" => Some((0, AuthorizationBodyKindV1::Audit)),
        "capability_invocation_contexts" => Some((1, AuthorizationBodyKindV1::Invocation)),
        _ => None,
    }
}
fn descriptor(index: usize) -> FieldDescriptor {
    FieldDescriptor {
        owner: FieldOwner::Runtime,
        name: if index == 0 {
            "capability_authorization_receipts"
        } else {
            "capability_invocation_contexts"
        },
        shape: FieldShape::Map,
        destination: FieldDestination::State(StateNamespaceV1::Authorization),
    }
}
fn child<M, R, E, C>(
    kind: AuthorizationBodyKindV1,
    key: &WorldFieldKeyV1,
    value: &[u8; 32],
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<StateRootV1, ValueTreeError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    ctx.values
        .verify_closure(value, ctx.record_bytes, &mut ctx.cancelled)?;
    let mut bytes = [0; RECORD_ROOT_BYTES];
    let info = read_member_record_v1(
        value,
        key,
        &mut ctx.values,
        MemberRecordLimitsV1 {
            semantic_bytes: ctx.semantic_bytes,
            payload_bytes: RECORD_ROOT_BYTES as u64,
            record_bytes: ctx.record_bytes,
        },
        &mut std::io::sink(),
        &mut &mut bytes[..],
        &mut ctx.cancelled,
    )?;
    if info.codec != MemberPayloadCodecV1::RecordRoot
        || info.payload_bytes != RECORD_ROOT_BYTES as u64
    {
        return Err(invalid());
    }
    let owner = WorldFieldKeyV1::from_bytes(key.as_bytes())
        .map_err(ValueTreeError::Invalid)?
        .semantic_key
        .ok_or_else(invalid)?;
    let reference = RecordRootRefV1::from_canonical_bytes(&bytes).map_err(|_| invalid())?;
    Ok(reference
        .validate_binding(&kind.schema(), &owner, StateNamespaceV1::Authorization)
        .map_err(|_| invalid())?
        .clone())
}
fn body<M, R, E, C>(
    kind: AuthorizationBodyKindV1,
    root: &StateRootV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    json: &mut JsonReadWorkV1,
    _max_selected_heap: usize,
) -> Result<(), ValueTreeError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    stream::qualify(kind, root, ctx, scan, json)
}
pub fn qualify_authorization_record_families_v1<M, R, E, C>(
    roots: &RootSetV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    outer_scan: &mut StateScanBudget,
    body_scan: &mut StateScanBudget,
    json: &mut JsonReadWorkV1,
    max_selected_heap: usize,
) -> Result<ReservedValue<AuthorizationRecordFamiliesQualificationV1>, ValueTreeError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    roots.validate().map_err(|_| invalid())?;
    ctx.check()?;
    if !roots.state.iter().any(|r| r == ctx.root) {
        return Err(invalid());
    }
    let retained = ctx.values.memory.reserve(512, ctx.values.class)?;
    let cancel = RefCell::new(&mut ctx.cancelled);
    let mut metadata = 0u8;
    let mut counts = [0u64; 2];
    for root in &roots.state {
        let mut local = WorldFieldReadContextV1 {
            root,
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
            cancelled: || (cancel.borrow_mut())(),
        };
        let result = StateScanContext {
            source: ctx.state,
            memory: local.values.memory,
            class: local.values.class,
            io: &mut *outer_scan,
        }
        .visit_entries(
            root,
            |entry| {
                if !entry.key.starts_with(b"oasis7.iws.world-field-key.v1\0") {
                    return Ok(());
                }
                let parsed =
                    WorldFieldKeyV1::from_bytes(&entry.key).map_err(ValueTreeError::Invalid)?;
                if parsed.owner != FieldOwner::Runtime {
                    return Ok(());
                }
                let Some((index, kind)) = identify(parsed.name) else {
                    return Ok(());
                };
                if parsed.namespace != StateNamespaceV1::Authorization
                    || root.namespace != parsed.namespace
                    || entry.namespace != parsed.namespace
                {
                    return Err(invalid());
                }
                let key = WorldFieldKeyV1::new(descriptor(index), parsed.kind, parsed.semantic_key)
                    .map_err(ValueTreeError::Invalid)?;
                if key.as_bytes() != entry.key {
                    return Err(invalid());
                }
                match parsed.kind {
                    WorldFieldRecordKindV1::Metadata => {
                        if parsed.semantic_key.is_some() || metadata & (1 << index) != 0 {
                            return Err(invalid());
                        }
                        local.records().presence(&entry.value, &key, 0)?;
                        metadata |= 1 << index;
                    }
                    WorldFieldRecordKindV1::Entry => {
                        let root = child(kind, &key, &entry.value, &mut local)?;
                        body(kind, &root, &mut local, body_scan, json, max_selected_heap)?;
                        counts[index] = counts[index].checked_add(1).ok_or_else(invalid)?;
                    }
                    _ => return Err(invalid()),
                }
                Ok(())
            },
            || (cancel.borrow_mut())(),
        );
        if let Some(e) = (local.take_state_error)() {
            return Err(e);
        }
        match result {
            Ok(_) => (),
            Err(StateScanError::Visitor(e)) => return Err(e),
            Err(StateScanError::Memory(e)) => return Err(e.into()),
            Err(StateScanError::Cancelled) => return Err(ValueTreeError::Cancelled),
            Err(StateScanError::ResourceLimited) => {
                return Err(ValueTreeError::ResourceLimited(
                    "authorization record family scan",
                ));
            }
            Err(_) => return Err(invalid()),
        }
    }
    if metadata != 3 {
        return Err(invalid());
    }
    if (cancel.borrow_mut())() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(retained.retain(AuthorizationRecordFamiliesQualificationV1 {
        roots: roots.digest().map_err(|_| invalid())?,
        counts,
        class: ctx.values.class,
    }))
}
pub fn accept_authorization_record_families_transition_v1<M, P, R, E, C>(
    parent: &ReservedValue<AuthorizationRecordFamiliesQualificationV1>,
    verifier: &TransitionVerifier<'_, P>,
    record: &TransitionRecordV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    body_scan: &mut StateScanBudget,
    json: &mut JsonReadWorkV1,
    max_selected_heap: usize,
) -> Result<
    ReservedValue<AuthorizationRecordFamiliesQualificationV1>,
    crate::incremental_field_overlay::WorldFieldOverlayErrorV1,
>
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
            || parent.reserved_bytes()
                < std::mem::size_of::<AuthorizationRecordFamiliesQualificationV1>()
            || parent.class != MemoryClass::Ordinary
            || ctx.values.class != MemoryClass::Ordinary
            || verifier.class != ctx.values.class
            || !verifier.memory.same_pool(ctx.values.memory)
        {
            return Err(invalid().into());
        }
        let _retained = ctx
            .values
            .memory
            .reserve(512, ctx.values.class)
            .map_err(ValueTreeError::Memory)?;
    }
    let witness = crate::incremental_verified_world_transition::verify_world_transition_v1(
        verifier, record, ctx,
    )?;
    accept_authorization_record_families_verified_v1(
        parent,
        &witness,
        ctx,
        body_scan,
        json,
        max_selected_heap,
    )
}

pub(crate) fn accept_authorization_record_families_verified_v1<M, R, E, C>(
    parent: &ReservedValue<AuthorizationRecordFamiliesQualificationV1>,
    witness: &crate::incremental_verified_world_transition::VerifiedWorldTransitionV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    body_scan: &mut StateScanBudget,
    json: &mut JsonReadWorkV1,
    max_selected_heap: usize,
) -> Result<
    ReservedValue<AuthorizationRecordFamiliesQualificationV1>,
    crate::incremental_field_overlay::WorldFieldOverlayErrorV1,
>
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
        || parent.reserved_bytes()
            < std::mem::size_of::<AuthorizationRecordFamiliesQualificationV1>()
        || parent.class != MemoryClass::Ordinary
        || ctx.values.class != MemoryClass::Ordinary
    {
        return Err(invalid().into());
    }
    let retained = ctx
        .values
        .memory
        .reserve(512, ctx.values.class)
        .map_err(ValueTreeError::Memory)?;
    let checked = witness.prepared();
    let record = &checked.record;
    if !record.after.state.iter().any(|r| r == ctx.root) {
        return Err(invalid().into());
    }
    let mut counts = parent.counts;
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
        if parsed.owner != FieldOwner::Runtime {
            continue;
        }
        let Some((index, kind)) = identify(parsed.name) else {
            continue;
        };
        if parsed.namespace != StateNamespaceV1::Authorization
            || write.namespace != parsed.namespace
        {
            return Err(invalid().into());
        }
        let key = WorldFieldKeyV1::new(descriptor(index), parsed.kind, parsed.semantic_key)
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
                if write.mutation.before.is_none() {
                    counts[index] = counts[index].checked_add(1).ok_or_else(invalid)?;
                }
                let Some(value) = write.mutation.after else {
                    counts[index] = counts[index].checked_sub(1).ok_or_else(invalid)?;
                    continue;
                };
                let root = child(kind, &key, &value, ctx)?;
                if let Some(before) = write.mutation.before {
                    let before = child(kind, &key, &before, ctx)?;
                    delta::advance(kind, &before, &root, ctx, json)?;
                } else {
                    body(kind, &root, ctx, body_scan, json, max_selected_heap)?;
                }
            }
            _ => return Err(invalid().into()),
        }
    }
    ctx.check()?;
    Ok(retained.retain(AuthorizationRecordFamiliesQualificationV1 {
        roots: record.after.digest().map_err(|_| invalid())?,
        counts,
        class: ctx.values.class,
    }))
}

#[path = "incremental_runtime_authorization_records_readmission.rs"]
mod readmission;
pub use readmission::readmit_authorization_record_families_ordinary_v1;

#[path = "incremental_runtime_authorization_overlay.rs"]
mod overlay;
pub use overlay::{AuthorizationRecordChangeV1, change_authorization_record_overlay_v1};

#[cfg(test)]
#[path = "incremental_runtime_authorization_family_tests.rs"]
mod tests;

#[path = "incremental_runtime_authorization_delta.rs"]
mod delta;
#[path = "incremental_runtime_authorization_stream.rs"]
mod stream;
