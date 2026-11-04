//! Complete governance control family verification and changed-member inheritance.
use super::*;
use crate::{
    incremental_field_read::WorldFieldReadContextV1,
    incremental_semantic_key::{SemanticKeyRefV1, read_semantic_key_v1},
};
use oasis7_formal_protocol::{
    incremental_execution::{RootSetV1, TransitionRecordV1, TransitionVerifier},
    incremental_record_root::RECORD_ROOT_BYTES,
    incremental_state::{
        MerkleNodeReader, StateEntryV1, StateScanBudget, StateScanContext, StateScanError,
    },
    incremental_value::ValueReadContext,
};
use oasis7_proto::memory_budget::{MemoryBudget, ReservedValue};
use std::cell::RefCell;
pub struct RuntimeGovernanceQualificationV1 {
    roots: [u8; 32],
    counts: [u64; 3],
    class: MemoryClass,
}
impl RuntimeGovernanceQualificationV1 {
    pub fn validate_binding(&self, roots: &RootSetV1) -> Result<(), ValueTreeError> {
        if self.roots != roots.digest().map_err(|_| invalid())? {
            return Err(invalid());
        }
        Ok(())
    }
    pub fn record_counts(&self) -> [u64; 3] {
        self.counts
    }
    pub fn allocation_class(&self) -> MemoryClass {
        self.class
    }
}
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("Runtime governance family binding")
}
fn index(kind: RuntimeGovernanceKindV1) -> usize {
    match kind {
        RuntimeGovernanceKindV1::Policy => 0,
        RuntimeGovernanceKindV1::EpochSnapshot => 1,
        RuntimeGovernanceKindV1::IdentityPenalty => 2,
    }
}
fn identify(
    entry: &StateEntryV1,
) -> Result<
    Option<(
        RuntimeGovernanceKindV1,
        WorldFieldKeyV1,
        WorldFieldRecordKindV1,
    )>,
    ValueTreeError,
> {
    if !entry.key.starts_with(b"oasis7.iws.world-field-key.v1\0") {
        return Ok(None);
    }
    let parsed = WorldFieldKeyV1::from_bytes(&entry.key).map_err(ValueTreeError::Invalid)?;
    if parsed.owner != FieldOwner::Runtime {
        return Ok(None);
    }
    let Some(kind) = RuntimeGovernanceKindV1::ALL
        .into_iter()
        .find(|k| k.descriptor().name == parsed.name)
    else {
        return Ok(None);
    };
    let key = WorldFieldKeyV1::new(kind.descriptor(), parsed.kind, parsed.semantic_key)
        .map_err(ValueTreeError::Invalid)?;
    if parsed.namespace != entry.namespace
        || entry.namespace != StateNamespaceV1::Authorization
        || key.as_bytes() != entry.key
        || !matches!(
            parsed.kind,
            WorldFieldRecordKindV1::Metadata | WorldFieldRecordKindV1::Entry
        )
    {
        return Err(invalid());
    }
    Ok(Some((kind, key, parsed.kind)))
}
pub(crate) fn child<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    kind: RuntimeGovernanceKindV1,
    key: &WorldFieldKeyV1,
    value: &[u8; 32],
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<(RuntimeGovernanceKeyV1, StateRootV1), ValueTreeError> {
    ctx.check()?;
    ctx.values
        .verify_closure(value, ctx.record_bytes, &mut ctx.cancelled)?;
    let mut original = [0u8; 64];
    let mut bytes = [0u8; RECORD_ROOT_BYTES];
    let info = read_member_record_v1(
        value,
        key,
        &mut ctx.values,
        MemberRecordLimitsV1 {
            semantic_bytes: 64,
            payload_bytes: RECORD_ROOT_BYTES as u64,
            record_bytes: ctx.record_bytes,
        },
        &mut std::io::Cursor::new(original.as_mut_slice()),
        &mut &mut bytes[..],
        &mut ctx.cancelled,
    )?;
    if info.codec != MemberPayloadCodecV1::RecordRoot
        || info.payload_bytes != RECORD_ROOT_BYTES as u64
    {
        return Err(invalid());
    }
    let original_len = usize::try_from(info.semantic_bytes).map_err(|_| invalid())?;
    let parsed = read_semantic_key_v1(original.get(..original_len).ok_or_else(invalid)?)
        .map_err(ValueTreeError::Invalid)?;
    let identity = match (kind, parsed) {
        (RuntimeGovernanceKindV1::Policy, SemanticKeyRefV1::Unit) => RuntimeGovernanceKeyV1::Policy,
        (RuntimeGovernanceKindV1::EpochSnapshot, SemanticKeyRefV1::Unsigned(id)) => {
            RuntimeGovernanceKeyV1::Epoch(id)
        }
        (RuntimeGovernanceKindV1::IdentityPenalty, SemanticKeyRefV1::Unsigned(id)) => {
            RuntimeGovernanceKeyV1::Penalty(id)
        }
        _ => return Err(invalid()),
    };
    let binding = WorldFieldKeyV1::from_bytes(key.as_bytes()).map_err(ValueTreeError::Invalid)?;
    let reference = RecordRootRefV1::from_canonical_bytes(&bytes).map_err(|_| invalid())?;
    let root = reference
        .validate_binding(
            &kind.schema(),
            &binding.semantic_key.ok_or_else(invalid)?,
            StateNamespaceV1::Authorization,
        )
        .map_err(|_| invalid())?
        .clone();
    ctx.check()?;
    Ok((identity, root))
}

pub fn qualify_runtime_governance_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    roots: &RootSetV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    max_record_heap: usize,
) -> Result<ReservedValue<RuntimeGovernanceQualificationV1>, ValueTreeError> {
    roots.validate().map_err(|_| invalid())?;
    ctx.check()?;
    if !roots.state.iter().any(|r| r == ctx.root) {
        return Err(invalid());
    }
    let charge = ctx.values.memory.reserve(512, ctx.values.class)?;
    let pending_bytes = 64usize
        .checked_mul(std::mem::size_of::<(RuntimeGovernanceKindV1, StateRootV1)>())
        .and_then(|n| n.checked_add(32 << 10))
        .ok_or_else(invalid)?;
    let _workspace = ctx.values.memory.reserve(pending_bytes, ctx.values.class)?;
    let mut pending = Vec::with_capacity(64);
    if pending.capacity() * std::mem::size_of::<(RuntimeGovernanceKindV1, StateRootV1)>()
        > _workspace.bytes()
    {
        return Err(invalid());
    }
    let cancel = RefCell::new(&mut ctx.cancelled);
    let mut metadata = 0u8;
    let mut counts = [0u64; 3];
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
        let mut cursor = None;
        let mut visited = 0u64;
        loop {
            pending.clear();
            let result = StateScanContext {
                source: ctx.state,
                memory: local.values.memory,
                class: local.values.class,
                io: &mut *scan,
            }
            .visit_entries_page(
                root,
                cursor,
                64,
                |entry| {
                    let Some((kind, key, record_kind)) = identify(entry)? else {
                        return Ok(());
                    };
                    let i = index(kind);
                    if record_kind == WorldFieldRecordKindV1::Metadata {
                        if metadata & (1 << i) != 0 {
                            return Err(invalid());
                        }
                        local.values.verify_closure(
                            &entry.value,
                            local.record_bytes,
                            &mut local.cancelled,
                        )?;
                        local.records().presence(&entry.value, &key, 0)?;
                        metadata |= 1 << i;
                    } else {
                        let (_, child_root) = child(kind, &key, &entry.value, &mut local)?;
                        if pending.len() == pending.capacity() {
                            return Err(invalid());
                        }
                        pending.push((kind, child_root));
                        counts[i] = counts[i].checked_add(1).ok_or_else(invalid)?;
                    }
                    Ok(())
                },
                || (cancel.borrow_mut())(),
            );
            let error = (local.take_state_error)();
            if (cancel.borrow_mut())() {
                return Err(ValueTreeError::Cancelled);
            }
            if let Some(e) = error {
                return Err(e);
            }
            let page = match result {
                Ok(page) => page,
                Err(StateScanError::Visitor(e)) => return Err(e),
                Err(StateScanError::Memory(e)) => return Err(e.into()),
                Err(StateScanError::ResourceLimited) => {
                    return Err(ValueTreeError::ResourceLimited("Runtime governance scan"));
                }
                Err(StateScanError::Cancelled) => return Err(ValueTreeError::Cancelled),
                Err(_) => return Err(invalid()),
            };
            visited = visited.checked_add(page.visited).ok_or_else(invalid)?;
            // Outer pages and complete child scans consume one unchanged allowance.
            for (kind, child_root) in pending.drain(..) {
                let _body = recover_runtime_governance_record_v1(
                    kind,
                    &child_root,
                    &mut local,
                    &mut *scan,
                    max_record_heap,
                )?;
            }
            if !page.has_more {
                break;
            }
            if page.visited == 0 || page.last_key_digest == cursor {
                return Err(invalid());
            }
            cursor = page.last_key_digest;
        }
        if visited != root.leaf_count {
            return Err(invalid());
        }
    }
    if metadata != 7 || counts[0] != 1 {
        return Err(invalid());
    }
    if (cancel.borrow_mut())() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(charge.retain(RuntimeGovernanceQualificationV1 {
        roots: roots.digest().map_err(|_| invalid())?,
        counts,
        class: ctx.values.class,
    }))
}

/// Changed-member validation reuses the exact parent and retains its class.
/// Recovery suffix replay stays Recovery; ordinary use requires explicit readmission.
pub fn accept_runtime_governance_transition_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    parent: &ReservedValue<RuntimeGovernanceQualificationV1>,
    verifier: &TransitionVerifier<'_, P>,
    record: &TransitionRecordV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut RuntimeGovernanceWorkV1,
    max_record_heap: usize,
) -> Result<
    ReservedValue<RuntimeGovernanceQualificationV1>,
    crate::incremental_field_overlay::WorldFieldOverlayErrorV1,
> {
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
    let charge = ctx
        .values
        .memory
        .reserve(512, ctx.values.class)
        .map_err(ValueTreeError::Memory)?;
    let _scratch = ctx
        .values
        .memory
        .reserve(32 << 10, ctx.values.class)
        .map_err(ValueTreeError::Memory)?;
    let result = verifier.verify(record);
    let error = (ctx.take_state_error)();
    ctx.check()?;
    if let Some(e) = error {
        return Err(e.into());
    }
    let checked = result?;
    let mut counts = parent.counts.map(i128::from);
    for write in &checked.record.writes {
        ctx.check()?;
        work.cells_remaining =
            work.cells_remaining
                .checked_sub(1)
                .ok_or(ValueTreeError::ResourceLimited(
                    "Runtime governance outer changed cells",
                ))?;
        let entry = StateEntryV1 {
            namespace: write.namespace,
            key: write.mutation.key.clone(),
            value: write.mutation.after.unwrap_or([0; 32]),
        };
        let Some((kind, key, record_kind)) = identify(&entry)? else {
            continue;
        };
        if record_kind == WorldFieldRecordKindV1::Metadata {
            if write.mutation.before.is_none() || write.mutation.before != write.mutation.after {
                return Err(invalid().into());
            }
            continue;
        }
        if kind == RuntimeGovernanceKindV1::Policy
            && (write.mutation.before.is_none() || write.mutation.after.is_none())
        {
            return Err(invalid().into());
        }
        counts[index(kind)] += i128::from(write.mutation.after.is_some())
            - i128::from(write.mutation.before.is_some());
        if let Some(value) = write.mutation.after {
            let (identity, child_root) = child(kind, &key, &value, ctx)?;
            if let Some(before) = write.mutation.before {
                let (original, before_root) = child(kind, &key, &before, ctx)?;
                if original != identity {
                    return Err(invalid().into());
                }
                super::delta::changed(kind, &before_root, &child_root, &mut ctx.records(), work)?;
            } else {
                let _body = recover_runtime_governance_record_v1(
                    kind,
                    &child_root,
                    ctx,
                    scan,
                    max_record_heap,
                )?;
            }
        }
    }
    let counts = [
        u64::try_from(counts[0]).map_err(|_| invalid())?,
        u64::try_from(counts[1]).map_err(|_| invalid())?,
        u64::try_from(counts[2]).map_err(|_| invalid())?,
    ];
    if counts[0] != 1 {
        return Err(invalid().into());
    }
    let proof = charge.retain(RuntimeGovernanceQualificationV1 {
        roots: record.after.digest().map_err(|_| invalid())?,
        counts,
        class: ctx.values.class,
    });
    ctx.check()?;
    Ok(proof)
}
pub fn readmit_runtime_governance_ordinary_v1(
    source: &ReservedValue<RuntimeGovernanceQualificationV1>,
    roots: &RootSetV1,
    memory: &MemoryBudget,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<RuntimeGovernanceQualificationV1>, ValueTreeError> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    if !source.uses_budget(memory)
        || source.reserved_bytes() < 512
        || source.class != MemoryClass::Recovery
    {
        return Err(invalid());
    }
    let mut phase = memory.reserve(128 << 10, MemoryClass::Ordinary)?;
    source.validate_binding(roots)?;
    let charge = phase.split(512).map_err(ValueTreeError::Invalid)?;
    let proof = charge.retain(RuntimeGovernanceQualificationV1 {
        roots: source.roots,
        counts: source.counts,
        class: MemoryClass::Ordinary,
    });
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(proof)
}
