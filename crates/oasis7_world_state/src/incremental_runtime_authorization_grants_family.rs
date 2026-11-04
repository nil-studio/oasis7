//! Complete root-bound original JSON grant family qualification.
use crate::{
    incremental_field_member::*,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_json_records::{JsonReadWorkV1, qualify_json_subtree_records_v1},
    incremental_runtime_authorization_grants::authorization_grant_profile_v1,
};
use oasis7_formal_protocol::{
    incremental_execution::RootSetV1,
    incremental_record_root::{RECORD_ROOT_BYTES, RecordRootRefV1},
    incremental_state::{
        MerkleNodeReader, StateNamespaceV1, StateScanBudget, StateScanContext, StateScanError,
    },
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::ReservedValue;
use std::cell::RefCell;
pub struct AuthorizationGrantsQualificationV1 {
    roots: [u8; 32],
    entries: u64,
    class: oasis7_proto::memory_budget::MemoryClass,
}
impl AuthorizationGrantsQualificationV1 {
    pub fn entries(&self) -> u64 {
        self.entries
    }
    pub fn validate_binding(&self, roots: &RootSetV1) -> Result<(), ValueTreeError> {
        if roots.digest().map_err(|_| invalid())? != self.roots {
            return Err(invalid());
        }
        Ok(())
    }
}
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("authorization grant family binding")
}
pub fn qualify_authorization_grants_family_v1<M, R, E, C>(
    roots: &RootSetV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut JsonReadWorkV1,
) -> Result<ReservedValue<AuthorizationGrantsQualificationV1>, ValueTreeError>
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
    let profile = authorization_grant_profile_v1();
    let cancel = RefCell::new(&mut ctx.cancelled);
    let mut metadata = 0u64;
    let mut entries = 0u64;
    for root in &roots.state {
        let mut records = crate::incremental_inventory_value_read::InventoryValueReader {
            values: &mut ctx.values,
            key_work: &mut *ctx.key_work,
            semantic_bytes: ctx.semantic_bytes,
            record_bytes: ctx.record_bytes,
            cancelled: &mut || (cancel.borrow_mut())(),
        };
        let result = StateScanContext {
            source: ctx.state,
            memory: records.values.memory,
            class: records.values.class,
            io: &mut *scan,
        }
        .visit_entries(
            root,
            |entry| {
                if !entry.key.starts_with(b"oasis7.iws.world-field-key.v1\0") {
                    return Ok(());
                }
                let parsed =
                    WorldFieldKeyV1::from_bytes(&entry.key).map_err(ValueTreeError::Invalid)?;
                if parsed.owner != FieldOwner::Runtime || parsed.name != profile.field.name {
                    return Ok(());
                }
                if root.namespace != StateNamespaceV1::Authorization
                    || entry.namespace != root.namespace
                    || parsed.namespace != root.namespace
                {
                    return Err(invalid());
                }
                let key = WorldFieldKeyV1::new(profile.field, parsed.kind, parsed.semantic_key)
                    .map_err(ValueTreeError::Invalid)?;
                if key.as_bytes() != entry.key {
                    return Err(invalid());
                }
                records.values.verify_closure(
                    &entry.value,
                    records.record_bytes,
                    &mut *records.cancelled,
                )?;
                match parsed.kind {
                    WorldFieldRecordKindV1::Metadata => {
                        if parsed.semantic_key.is_some() {
                            return Err(invalid());
                        }
                        records.presence(&entry.value, &key, 0)?;
                        metadata = metadata.checked_add(1).ok_or_else(invalid)?;
                        if metadata > 1 {
                            return Err(invalid());
                        }
                    }
                    WorldFieldRecordKindV1::Entry => {
                        let owner = parsed.semantic_key.ok_or_else(invalid)?;
                        let mut bytes = [0; RECORD_ROOT_BYTES];
                        let info = read_member_record_v1(
                            &entry.value,
                            &key,
                            records.values,
                            MemberRecordLimitsV1 {
                                semantic_bytes: records.semantic_bytes,
                                payload_bytes: RECORD_ROOT_BYTES as u64,
                                record_bytes: records.record_bytes,
                            },
                            &mut std::io::sink(),
                            &mut &mut bytes[..],
                            &mut *records.cancelled,
                        )?;
                        if info.codec != MemberPayloadCodecV1::RecordRoot
                            || info.payload_bytes != RECORD_ROOT_BYTES as u64
                        {
                            return Err(invalid());
                        }
                        let reference =
                            RecordRootRefV1::from_canonical_bytes(&bytes).map_err(|_| invalid())?;
                        let child = reference
                            .validate_binding(
                                &profile.schema,
                                &owner,
                                StateNamespaceV1::Authorization,
                            )
                            .map_err(|_| invalid())?;
                        qualify_json_subtree_records_v1(profile, child, &mut records, work)?;
                        entries = entries.checked_add(1).ok_or_else(invalid)?;
                    }
                    _ => return Err(invalid()),
                }
                if (records.cancelled)() {
                    return Err(ValueTreeError::Cancelled);
                }
                Ok(())
            },
            || (cancel.borrow_mut())(),
        );
        if let Some(error) = (ctx.take_state_error)() {
            return Err(error);
        }
        match result {
            Ok(_) => (),
            Err(StateScanError::Visitor(e)) => return Err(e),
            Err(StateScanError::Memory(e)) => return Err(e.into()),
            Err(StateScanError::ResourceLimited) => {
                return Err(ValueTreeError::ResourceLimited(
                    "authorization grant family scan",
                ));
            }
            Err(StateScanError::Cancelled) => return Err(ValueTreeError::Cancelled),
            Err(_) => return Err(invalid()),
        }
    }
    if metadata != 1 {
        return Err(invalid());
    }
    if (cancel.borrow_mut())() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(retained.retain(AuthorizationGrantsQualificationV1 {
        roots: roots.digest().map_err(|_| invalid())?,
        entries,
        class: ctx.values.class,
    }))
}

/// Inherit the exact verified parent family and inspect only changed members.
/// Each replacement graph is fully validated; fine JSON edits may separately
/// carry the neutral engine's authenticated path-delta proof.
pub fn accept_authorization_grants_transition_v1<M, P, R, E, C>(
    parent: &ReservedValue<AuthorizationGrantsQualificationV1>,
    verifier: &oasis7_formal_protocol::incremental_execution::TransitionVerifier<'_, P>,
    record: &oasis7_formal_protocol::incremental_execution::TransitionRecordV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut JsonReadWorkV1,
) -> Result<
    ReservedValue<AuthorizationGrantsQualificationV1>,
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
        use oasis7_proto::memory_budget::MemoryClass;
        ctx.check()?;
        if !parent.uses_budget(ctx.values.memory)
            || parent.reserved_bytes() < std::mem::size_of::<AuthorizationGrantsQualificationV1>()
            || !verifier.memory.same_pool(ctx.values.memory)
            || verifier.class != ctx.values.class
            || parent.class != MemoryClass::Ordinary
            || ctx.values.class != MemoryClass::Ordinary
        {
            return Err(invalid().into());
        }
        parent.validate_binding(&record.before)?;
        let _retained = ctx
            .values
            .memory
            .reserve(512, ctx.values.class)
            .map_err(ValueTreeError::Memory)?;
    }
    let witness = crate::incremental_verified_world_transition::verify_world_transition_v1(
        verifier, record, ctx,
    )?;
    accept_authorization_grants_verified_v1(parent, &witness, ctx, work)
}

pub(crate) fn accept_authorization_grants_verified_v1<M, R, E, C>(
    parent: &ReservedValue<AuthorizationGrantsQualificationV1>,
    witness: &crate::incremental_verified_world_transition::VerifiedWorldTransitionV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut JsonReadWorkV1,
) -> Result<
    ReservedValue<AuthorizationGrantsQualificationV1>,
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

    use oasis7_proto::memory_budget::MemoryClass;
    ctx.check()?;
    if !parent.uses_budget(ctx.values.memory)
        || parent.reserved_bytes() < std::mem::size_of::<AuthorizationGrantsQualificationV1>()
        || parent.class != MemoryClass::Ordinary
        || ctx.values.class != MemoryClass::Ordinary
    {
        return Err(invalid().into());
    }
    parent.validate_binding(&record.before)?;
    let retained = ctx
        .values
        .memory
        .reserve(512, ctx.values.class)
        .map_err(ValueTreeError::Memory)?;
    let checked = witness.prepared();
    let record = &checked.record;
    if !record.after.state.iter().any(|root| root == ctx.root) {
        return Err(invalid().into());
    }
    let profile = authorization_grant_profile_v1();
    let mut entries = parent.entries;
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
        if parsed.owner != FieldOwner::Runtime || parsed.name != profile.field.name {
            continue;
        }
        if parsed.namespace != StateNamespaceV1::Authorization
            || write.namespace != parsed.namespace
        {
            return Err(invalid().into());
        }
        let key = WorldFieldKeyV1::new(profile.field, parsed.kind, parsed.semantic_key)
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
                let owner = parsed.semantic_key.ok_or_else(invalid)?;
                if write.mutation.before.is_none() {
                    entries = entries.checked_add(1).ok_or_else(invalid)?;
                }
                let Some(value) = write.mutation.after else {
                    entries = entries.checked_sub(1).ok_or_else(invalid)?;
                    continue;
                };
                let mut bytes = [0; RECORD_ROOT_BYTES];
                let info = read_member_record_v1(
                    &value,
                    &key,
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
                    return Err(invalid().into());
                }
                let reference =
                    RecordRootRefV1::from_canonical_bytes(&bytes).map_err(|_| invalid())?;
                let child = reference
                    .validate_binding(&profile.schema, &owner, StateNamespaceV1::Authorization)
                    .map_err(|_| invalid())?;
                qualify_json_subtree_records_v1(profile, child, &mut ctx.records(), work)?;
            }
            _ => return Err(invalid().into()),
        }
    }
    ctx.check()?;
    Ok(retained.retain(AuthorizationGrantsQualificationV1 {
        roots: record.after.digest().map_err(|_| invalid())?,
        entries,
        class: ctx.values.class,
    }))
}

#[cfg(test)]
#[path = "incremental_runtime_authorization_grants_tests.rs"]
mod tests;

#[path = "incremental_runtime_authorization_grants_readmission.rs"]
mod readmission;
pub use readmission::readmit_authorization_grants_ordinary_v1;

#[path = "incremental_runtime_authorization_grants_overlay.rs"]
mod overlay;
pub use overlay::{AuthorizationGrantChangeV1, change_authorization_grant_overlay_v1};
