//! Whole typed revocation closure bound to the exact parent and memory class.
use super::*;
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionWriteV1, RootSetV1, TransitionRecordV1, TransitionVerifier},
    incremental_record_root::RECORD_ROOT_BYTES,
    incremental_state::{StateEntryV1, StateScanContext, StateScanError},
    incremental_value::ValueReadContext,
};
use oasis7_proto::memory_budget::{MemoryBudget, ReservedValue};
use std::cell::RefCell;
pub struct RuntimeRevocationQualificationV1 {
    roots: [u8; 32],
    body: StateRootV1,
    class: MemoryClass,
    safe_alternatives_len: usize,
}
impl RuntimeRevocationQualificationV1 {
    pub(super) fn validate_digest(&self, digest: &[u8; 32]) -> Result<(), ValueTreeError> {
        if &self.roots != digest {
            return Err(invalid());
        }
        Ok(())
    }
    pub fn validate_binding(&self, roots: &RootSetV1) -> Result<(), ValueTreeError> {
        if self.roots != roots.digest().map_err(|_| invalid())? {
            return Err(invalid());
        }
        Ok(())
    }
    pub fn allocation_class(&self) -> MemoryClass {
        self.class
    }
    /// Valid for this qualified body only. Current-overlay consumers must first
    /// authenticate the exact current parent with begin_runtime_revocation_current_v1.
    pub fn safe_alternatives_len(&self) -> usize {
        self.safe_alternatives_len
    }
    pub fn body(&self) -> &StateRootV1 {
        &self.body
    }
}
pub(super) fn outer() -> FieldDescriptor {
    FieldDescriptor {
        owner: FieldOwner::Runtime,
        name: "capability_revocation_state",
        shape: FieldShape::Value,
        destination: FieldDestination::State(StateNamespaceV1::Authorization),
    }
}
fn identify(entry: &StateEntryV1) -> Result<Option<WorldFieldKeyV1>, ValueTreeError> {
    if !entry.key.starts_with(b"oasis7.iws.world-field-key.v1\0") {
        return Ok(None);
    }
    let p = WorldFieldKeyV1::from_bytes(&entry.key).map_err(ValueTreeError::Invalid)?;
    if p.owner != FieldOwner::Runtime || p.name != outer().name {
        return Ok(None);
    }
    let key =
        WorldFieldKeyV1::new(outer(), p.kind, p.semantic_key).map_err(ValueTreeError::Invalid)?;
    if p.namespace != entry.namespace
        || entry.namespace != StateNamespaceV1::Authorization
        || key.as_bytes() != entry.key
        || !matches!(
            p.kind,
            WorldFieldRecordKindV1::Metadata | WorldFieldRecordKindV1::Entry
        )
    {
        return Err(invalid());
    }
    Ok(Some(key))
}
pub(super) fn child<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    key: &WorldFieldKeyV1,
    value: &[u8; 32],
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<StateRootV1, ValueTreeError> {
    child_records(key, value, &mut ctx.records())
}

pub(super) fn child_records<R: ValueNodeReader, C: FnMut() -> bool>(
    key: &WorldFieldKeyV1,
    value: &[u8; 32],
    records: &mut crate::incremental_inventory_value_read::InventoryValueReader<'_, '_, R, C>,
) -> Result<StateRootV1, ValueTreeError> {
    let (expected, _) = records.key(outer(), WorldFieldRecordKindV1::Entry, &())?;
    if *key != expected {
        return Err(invalid());
    }
    records
        .values
        .verify_closure(value, records.record_bytes, &mut *records.cancelled)?;
    let mut bytes = [0; RECORD_ROOT_BYTES];
    let info = read_member_record_v1(
        value,
        key,
        &mut *records.values,
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
    let owner = WorldFieldKeyV1::from_bytes(key.as_bytes())
        .map_err(ValueTreeError::Invalid)?
        .semantic_key
        .ok_or_else(invalid)?;
    let reference = RecordRootRefV1::from_canonical_bytes(&bytes).map_err(|_| invalid())?;
    Ok(reference
        .validate_binding(&schema(), &owner, StateNamespaceV1::Authorization)
        .map_err(|_| invalid())?
        .clone())
}
pub fn prepare_runtime_revocation_genesis_v1<
    W: ValueNodeWriter + ValueNodeReader + StatePageWriter,
    C: FnMut() -> bool,
    S: FnMut(ReservedValue<ExecutionWriteV1>) -> Result<(), ValueTreeError>,
>(
    value: &crate::CapabilityRevocationState,
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut RevocationWriteWorkV1,
    sink: &mut S,
) -> Result<(), MemberRecordError> {
    work.spend(&mut ctx.cancelled)?;
    let row = write_field_metadata_v1(
        outer(),
        &mut PresenceRecordWriteContext {
            sink: &mut *ctx.sink,
            memory: ctx.memory,
            io: &mut *ctx.io,
            key_work: &mut *ctx.key_work,
            limits: PresenceRecordLimitsV1 {
                semantic_bytes: ctx.limits.semantic_bytes,
                record_bytes: ctx.limits.record_bytes,
            },
            cancelled: &mut ctx.cancelled,
        },
    )?;
    crate::incremental_field_write::emit(row.field_key, row.value_root, ctx, sink)?;
    let body = prepare_runtime_revocation_body_v1(value, ctx, work)?;
    let owner = hash_semantic_key_v1(
        &(),
        ctx.memory,
        MemoryClass::Ordinary,
        ctx.key_work,
        &mut ctx.cancelled,
    )
    .map_err(MemberRecordError::Key)?;
    let _reference_charge = ctx
        .memory
        .reserve(512, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let reference =
        RecordRootRefV1::new(schema(), owner.digest, (*body).clone()).map_err(|_| invalid())?;
    work.spend(&mut ctx.cancelled)?;
    let row = write_member_record_v1(
        outer(),
        WorldFieldRecordKindV1::Entry,
        &(),
        MemberPayloadRefV1::RecordRoot(&reference),
        ctx,
    )?;
    crate::incremental_field_write::emit(row.field_key, row.value_root, ctx, sink)?;
    if (ctx.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(())
}
fn validate_body<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    before: Option<&StateRootV1>,
    after: &StateRootV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut RevocationReadWorkV1,
) -> Result<(), ValueTreeError> {
    let trees_remaining = work.max_retained_trees;
    let profile = profile();
    crate::incremental_typed_record_diff::accept_body::<crate::CapabilityRevocationState, _, _, _, _>(
        &profile,
        before,
        Some(after),
        &mut crate::incremental_typed_record_diff::ChangeDecoder {
            ctx,
            scan,
            work,
            profile,
            trees_remaining,
            tree_depth: 0,
        },
    )
}
pub fn qualify_runtime_revocation_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    roots: &RootSetV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut RevocationReadWorkV1,
) -> Result<ReservedValue<RuntimeRevocationQualificationV1>, ValueTreeError> {
    roots.validate().map_err(|_| invalid())?;
    ctx.check()?;
    if !roots.state.iter().any(|r| r == ctx.root) {
        return Err(invalid());
    }
    let retained = ctx.values.memory.reserve(512, ctx.values.class)?;
    let _scratch = ctx.values.memory.reserve(32 << 10, ctx.values.class)?;
    let body = {
        let stop = RefCell::new(&mut ctx.cancelled);
        let mut metadata = false;
        let mut body = None;
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
                cancelled: || (stop.borrow_mut())(),
            };
            let result = StateScanContext {
                source: ctx.state,
                memory: local.values.memory,
                class: local.values.class,
                io: &mut *scan,
            }
            .visit_entries(
                root,
                |entry| {
                    if let Some(key) = identify(entry)? {
                        let parsed = WorldFieldKeyV1::from_bytes(key.as_bytes())
                            .map_err(ValueTreeError::Invalid)?;
                        if parsed.kind == WorldFieldRecordKindV1::Metadata {
                            if metadata {
                                return Err(invalid());
                            }
                            local.values.verify_closure(
                                &entry.value,
                                local.record_bytes,
                                &mut local.cancelled,
                            )?;
                            local.records().presence(&entry.value, &key, 0)?;
                            metadata = true;
                        } else {
                            if body.is_some() {
                                return Err(invalid());
                            }
                            body = Some(child(&key, &entry.value, &mut local)?);
                        }
                    }
                    Ok(())
                },
                || (stop.borrow_mut())(),
            );
            let error = (local.take_state_error)();
            if (stop.borrow_mut())() {
                return Err(ValueTreeError::Cancelled);
            }
            if let Some(e) = error {
                return Err(e);
            }
            match result {
                Ok(_) => (),
                Err(StateScanError::Visitor(e)) => return Err(e),
                Err(StateScanError::Memory(e)) => return Err(e.into()),
                Err(StateScanError::Cancelled) => return Err(ValueTreeError::Cancelled),
                Err(StateScanError::ResourceLimited) => {
                    return Err(ValueTreeError::ResourceLimited("revocation family scan"));
                }
                Err(_) => return Err(invalid()),
            }
        }
        if !metadata {
            return Err(invalid());
        }
        body.ok_or_else(invalid)?
    };
    validate_body(None, &body, ctx, scan, work)?;
    let safe_alternatives_len = safe_alternatives_length(&body, ctx, scan, work)?;
    ctx.check()?;
    Ok(retained.retain(RuntimeRevocationQualificationV1 {
        roots: roots.digest().map_err(|_| invalid())?,
        body,
        safe_alternatives_len,
        class: ctx.values.class,
    }))
}
pub fn readmit_runtime_revocation_ordinary_v1(
    proof: &ReservedValue<RuntimeRevocationQualificationV1>,
    roots: &RootSetV1,
    memory: &MemoryBudget,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<RuntimeRevocationQualificationV1>, ValueTreeError> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    if !proof.uses_budget(memory)
        || proof.reserved_bytes() < 512
        || proof.class != MemoryClass::Recovery
    {
        return Err(invalid());
    }
    proof.validate_binding(roots)?;
    let charge = memory.reserve(512, MemoryClass::Ordinary)?;
    let result = charge.retain(RuntimeRevocationQualificationV1 {
        roots: proof.roots,
        body: proof.body.clone(),
        safe_alternatives_len: proof.safe_alternatives_len,
        class: MemoryClass::Ordinary,
    });
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(result)
}
pub fn accept_runtime_revocation_transition_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    parent: &ReservedValue<RuntimeRevocationQualificationV1>,
    verifier: &TransitionVerifier<'_, P>,
    record: &TransitionRecordV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut RevocationReadWorkV1,
) -> Result<
    ReservedValue<RuntimeRevocationQualificationV1>,
    crate::incremental_field_overlay::WorldFieldOverlayErrorV1,
> {
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
            || verifier.class != ctx.values.class
            || !verifier.memory.same_pool(ctx.values.memory)
            || ctx.root != record.after.state_root(StateNamespaceV1::Authorization)
        {
            return Err(invalid().into());
        }
        parent.validate_binding(&record.before)?;
        let _charge = ctx
            .values
            .memory
            .reserve(512, ctx.values.class)
            .map_err(ValueTreeError::Memory)?;
    }
    let witness = crate::incremental_verified_world_transition::verify_world_transition_v1(
        verifier, record, ctx,
    )?;
    accept_runtime_revocation_verified_v1(parent, &witness, ctx, scan, work)
}

pub(crate) fn accept_runtime_revocation_verified_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    parent: &ReservedValue<RuntimeRevocationQualificationV1>,
    witness: &crate::incremental_verified_world_transition::VerifiedWorldTransitionV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut RevocationReadWorkV1,
) -> Result<
    ReservedValue<RuntimeRevocationQualificationV1>,
    crate::incremental_field_overlay::WorldFieldOverlayErrorV1,
> {
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
        || ctx.root != record.after.state_root(StateNamespaceV1::Authorization)
    {
        return Err(invalid().into());
    }
    parent.validate_binding(&record.before)?;
    let charge = ctx
        .values
        .memory
        .reserve(512, ctx.values.class)
        .map_err(ValueTreeError::Memory)?;
    let checked = witness.prepared();
    let _scratch = ctx
        .values
        .memory
        .reserve(32 << 10, ctx.values.class)
        .map_err(ValueTreeError::Memory)?;
    let mut body = parent.body.clone();
    let mut safe_alternatives_len = parent.safe_alternatives_len;
    for write in &checked.record.writes {
        ctx.check()?;
        let entry = StateEntryV1 {
            namespace: write.namespace,
            key: write.mutation.key.clone(),
            value: write.mutation.after.unwrap_or([0; 32]),
        };
        if let Some(key) = identify(&entry)? {
            let kind = WorldFieldKeyV1::from_bytes(key.as_bytes())
                .map_err(ValueTreeError::Invalid)?
                .kind;
            if write.mutation.before.is_none() || write.mutation.after.is_none() {
                return Err(invalid().into());
            }
            if kind == WorldFieldRecordKindV1::Metadata {
                if write.mutation.before != write.mutation.after {
                    return Err(invalid().into());
                }
            } else {
                let next_body = child(&key, &entry.value, ctx)?;
                validate_body(Some(&body), &next_body, ctx, scan, work)?;
                safe_alternatives_len = safe_alternatives_length(&next_body, ctx, scan, work)?;
                body = next_body;
            }
        }
    }
    ctx.check()?;
    Ok(charge.retain(RuntimeRevocationQualificationV1 {
        roots: checked.record.after.digest().map_err(|_| invalid())?,
        body,
        safe_alternatives_len,
        class: ctx.values.class,
    }))
}

// Read only the authenticated map column and its length, preserving unchanged
// payload inheritance when unrelated revocation columns change.
fn safe_alternatives_length<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    body: &StateRootV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut RevocationReadWorkV1,
) -> Result<usize, ValueTreeError> {
    use crate::incremental_typed_record_diff::{ChangeDecoder, point_typed_column_v1};
    let profile = profile();
    let trees_remaining = work.max_retained_trees;
    let mut decoder = ChangeDecoder {
        ctx,
        scan,
        work,
        profile,
        trees_remaining,
        tree_depth: 0,
    };
    let row = point_typed_column_v1(&profile, body, "safe_alternatives", &mut decoder, 8192)?;
    let ColumnCellV1::Root(root) = &row.cell else {
        return Err(invalid());
    };
    let length = point_typed_column_v1(&profile, root, "_length", &mut decoder, 8192)?;
    let ColumnCellV1::U64(count) = &length.cell else {
        return Err(invalid());
    };
    usize::try_from(*count).map_err(|_| invalid())
}
