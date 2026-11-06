//! Required typed evidence for all four original Runtime sequence pairs.
//! This proves wire/state integrity, not permission to allocate business IDs.
use crate::{
    incremental_event_fields::{EVENT_SEQUENCE_BYTES, EVENT_SEQUENCE_KEY_V1},
    incremental_field_overlay::WorldFieldOverlayErrorV1,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_runtime_allocators_codec::{
        RUNTIME_SEQUENCE_BYTES_V1, RuntimeAllocatorV1 as Allocator, RuntimeSequenceV1,
    },
    incremental_runtime_registry::RUNTIME_SEQUENCE_KEY_DOMAIN_V1,
    incremental_verified_world_transition::{
        VerifiedWorldTransitionV1, verify_world_transition_v1,
    },
};
use oasis7_formal_protocol::{
    incremental_execution::{RootSetV1, TransitionRecordV1, TransitionVerifier},
    incremental_state::{MerkleNodeReader, StateNamespaceV1 as Ns},
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation, ReservedValue};
const CARRIER: usize = std::mem::size_of::<RuntimeAllocatorsQualificationV1>() + 512;
pub struct RuntimeAllocatorsWorkV1 {
    pub records_remaining: u64,
}
pub struct RuntimeAllocatorsQualificationV1 {
    roots: [u8; 32],
    class: MemoryClass,
    sequences: [RuntimeSequenceV1; 4],
}
impl RuntimeAllocatorsQualificationV1 {
    pub fn allocation_class(&self) -> MemoryClass {
        self.class
    }
    pub fn validate_binding(&self, roots: &RootSetV1) -> Result<(), ValueTreeError> {
        if self.roots != roots.digest().map_err(|_| invalid())? {
            return Err(invalid());
        }
        Ok(())
    }
    pub fn sequence(&self, allocator: Allocator) -> RuntimeSequenceV1 {
        self.sequences[slot(allocator)]
    }
}
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("complete Runtime allocator pairs")
}
fn slot(allocator: Allocator) -> usize {
    match allocator {
        Allocator::Event => 0,
        Allocator::Action => 1,
        Allocator::Intent => 2,
        Allocator::Proposal => 3,
    }
}
fn check<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<(), ValueTreeError> {
    ctx.check()?;
    if let Some(e) = (ctx.take_state_error)() {
        return Err(e);
    }
    Ok(())
}
fn spend<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut RuntimeAllocatorsWorkV1,
) -> Result<(), ValueTreeError> {
    check(ctx)?;
    work.records_remaining = work
        .records_remaining
        .checked_sub(1)
        .ok_or(ValueTreeError::ResourceLimited("Runtime allocator records"))?;
    Ok(())
}
fn read<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    allocator: Allocator,
    id: &[u8; 32],
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<RuntimeSequenceV1, ValueTreeError> {
    check(ctx)?;
    let mut bytes = [0; RUNTIME_SEQUENCE_BYTES_V1];
    let length = if allocator == Allocator::Event {
        EVENT_SEQUENCE_BYTES
    } else {
        RUNTIME_SEQUENCE_BYTES_V1
    };
    let mut output = &mut bytes[..length];
    let root = ctx.values.load_root(id)?;
    if root.0.logical_len != length as u64 {
        return Err(invalid());
    }
    let copied = ctx
        .values
        .read_range(&root, 0, length as u64, &mut output, &mut ctx.cancelled)?;
    if copied != length as u64 {
        return Err(invalid());
    }
    if !output.is_empty() {
        return Err(invalid());
    }
    let value = allocator
        .from_canonical_bytes(&bytes[..length])
        .map_err(ValueTreeError::Invalid)?;
    check(ctx)?;
    Ok(value)
}
fn identify(key: &[u8]) -> Result<Option<Allocator>, ValueTreeError> {
    if key == EVENT_SEQUENCE_KEY_V1 {
        return Ok(Some(Allocator::Event));
    }
    if key.starts_with(RUNTIME_SEQUENCE_KEY_DOMAIN_V1) {
        let tag = crate::incremental_runtime_registry::parse_runtime_sequence_key_v1(key)
            .map_err(ValueTreeError::Invalid)?;
        return Ok(Some(match tag {
            1 => Allocator::Action,
            2 => Allocator::Intent,
            3 => Allocator::Proposal,
            _ => return Err(invalid()),
        }));
    }
    Ok(None)
}
pub fn qualify_runtime_allocators_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    roots: &RootSetV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut RuntimeAllocatorsWorkV1,
) -> Result<ReservedValue<RuntimeAllocatorsQualificationV1>, ValueTreeError> {
    check(ctx)?;
    roots.validate().map_err(|_| invalid())?;
    if ctx.root != roots.state_root(Ns::Allocator) {
        return Err(invalid());
    }
    let retained = ctx.values.memory.reserve(CARRIER, ctx.values.class)?;
    let _phase = ctx.values.memory.reserve(128 << 10, ctx.values.class)?;
    let mut sequences = [RuntimeSequenceV1 { next: 0, era: 0 }; 4];
    for allocator in Allocator::ALL {
        spend(ctx, work)?;
        let (key, length) = allocator.key();
        let id = ctx
            .lookup_raw(roots.state_root(Ns::Allocator), &key[..length])?
            .ok_or_else(invalid)?;
        sequences[slot(allocator)] = read(allocator, &id, ctx)?;
    }
    let roots = roots.digest().map_err(|_| invalid())?;
    check(ctx)?;
    let output = retained.retain(RuntimeAllocatorsQualificationV1 {
        roots,
        class: ctx.values.class,
        sequences,
    });
    check(ctx)?;
    Ok(output)
}
fn preflight<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    parent: &ReservedValue<RuntimeAllocatorsQualificationV1>,
    record: &TransitionRecordV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<MemoryReservation, ValueTreeError> {
    check(ctx)?;
    if parent.class != MemoryClass::Ordinary
        || ctx.values.class != MemoryClass::Ordinary
        || !parent.uses_budget(ctx.values.memory)
        || parent.reserved_bytes() < CARRIER
        || ctx.root != record.after.state_root(Ns::Allocator)
    {
        return Err(invalid());
    }
    let phase = ctx
        .values
        .memory
        .reserve(128 << 10, MemoryClass::Ordinary)?;
    parent.validate_binding(&record.before)?;
    check(ctx)?;
    Ok(phase)
}
pub fn accept_runtime_allocators_transition_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    parent: &ReservedValue<RuntimeAllocatorsQualificationV1>,
    verifier: &TransitionVerifier<'_, P>,
    record: &TransitionRecordV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut RuntimeAllocatorsWorkV1,
) -> Result<ReservedValue<RuntimeAllocatorsQualificationV1>, WorldFieldOverlayErrorV1> {
    let result = (|| {
        check(ctx)?;
        if verifier.class != MemoryClass::Ordinary || !verifier.memory.same_pool(ctx.values.memory)
        {
            return Err(invalid().into());
        }
        {
            let _phase = preflight(parent, record, ctx)?;
        }
        let witness = verify_world_transition_v1(verifier, record, ctx)?;
        accept_runtime_allocators_verified_v1(parent, &witness, ctx, work)
    })();
    check(ctx)?;
    result
}
pub(crate) fn accept_runtime_allocators_verified_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    parent: &ReservedValue<RuntimeAllocatorsQualificationV1>,
    witness: &VerifiedWorldTransitionV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut RuntimeAllocatorsWorkV1,
) -> Result<ReservedValue<RuntimeAllocatorsQualificationV1>, WorldFieldOverlayErrorV1> {
    let result = (|| {
        check(ctx)?;
        witness.check_context(ctx.values.memory, ctx.values.class, ctx.root)?;
        let _phase = preflight(parent, witness.record(), ctx)?;
        let retained = ctx
            .values
            .memory
            .reserve(CARRIER, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let mut sequences = parent.sequences;
        for write in &witness.record().writes {
            spend(ctx, work)?;
            let Some(allocator) = identify(&write.mutation.key)? else {
                continue;
            };
            if write.namespace != Ns::Allocator || write.mutation.before.is_none() {
                return Err(invalid().into());
            }
            let id = write.mutation.after.ok_or_else(invalid)?;
            sequences[slot(allocator)] = read(allocator, &id, ctx)?;
        }
        check(ctx)?;
        Ok(retained.retain(RuntimeAllocatorsQualificationV1 {
            roots: *witness.after_digest(),
            class: MemoryClass::Ordinary,
            sequences,
        }))
    })();
    check(ctx)?;
    result
}
pub fn readmit_runtime_allocators_ordinary_v1(
    source: &ReservedValue<RuntimeAllocatorsQualificationV1>,
    roots: &RootSetV1,
    memory: &MemoryBudget,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<RuntimeAllocatorsQualificationV1>, ValueTreeError> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    if source.class != MemoryClass::Recovery
        || !source.uses_budget(memory)
        || source.reserved_bytes() < CARRIER
    {
        return Err(invalid());
    }
    let retained = memory.reserve(CARRIER, MemoryClass::Ordinary)?;
    let _phase = memory.reserve(128 << 10, MemoryClass::Ordinary)?;
    source.validate_binding(roots)?;
    let output = retained.retain(RuntimeAllocatorsQualificationV1 {
        roots: source.roots,
        class: MemoryClass::Ordinary,
        sequences: source.sequences,
    });
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(output)
}
