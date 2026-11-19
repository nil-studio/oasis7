//! Complete Runtime governance-control qualification and actual changed-write inheritance.
use super::*;
use crate::incremental_field_overlay::WorldFieldOverlayErrorV1;
use crate::incremental_field_read::WorldFieldReadContextV1;
use oasis7_formal_protocol::{
    incremental_execution::{RootSetV1, TransitionRecordV1, TransitionVerifier},
    incremental_state::{
        MerkleNodeReader, StateEntryV1, StateScanBudget, StateScanContext, StateScanError,
    },
};
use std::cell::RefCell;
pub struct RuntimeControlsQualificationV1 {
    roots: [u8; 32],
    class: oasis7_proto::memory_budget::MemoryClass,
}
impl RuntimeControlsQualificationV1 {
    pub fn allocation_class(&self) -> oasis7_proto::memory_budget::MemoryClass {
        self.class
    }
    pub fn validate_binding(&self, roots: &RootSetV1) -> Result<(), ValueTreeError> {
        if roots.digest().map_err(|_| invalid())? != self.roots {
            return Err(invalid());
        }
        Ok(())
    }
}
fn owns(name: &str) -> bool {
    RuntimeControlFieldV1::ALL.iter().any(|f| f.name() == name)
}
fn identify(
    namespace: StateNamespaceV1,
    bytes: &[u8],
) -> Result<Option<(usize, WorldFieldRecordKindV1)>, ValueTreeError> {
    if !bytes.starts_with(b"oasis7.iws.world-field-key.v1\0") {
        return Ok(None);
    }
    let binding = WorldFieldKeyV1::from_bytes(bytes).map_err(ValueTreeError::Invalid)?;
    if binding.owner != FieldOwner::Runtime || !owns(binding.name) {
        return Ok(None);
    }
    let index = RuntimeControlFieldV1::ALL
        .iter()
        .position(|field| field.name() == binding.name)
        .ok_or_else(invalid)?;
    let field = RuntimeControlFieldV1::ALL[index];
    let expected = WorldFieldKeyV1::new(field.descriptor(), binding.kind, binding.semantic_key)
        .map_err(ValueTreeError::Invalid)?;
    if namespace != field.namespace() || expected.as_bytes() != bytes {
        return Err(invalid());
    }
    if !matches!(
        binding.kind,
        WorldFieldRecordKindV1::Metadata | WorldFieldRecordKindV1::Entry
    ) {
        return Err(invalid());
    }
    Ok(Some((index, binding.kind)))
}
fn check_entry<R: ValueNodeReader, C: FnMut() -> bool>(
    index: usize,
    kind: WorldFieldRecordKindV1,
    entry: &StateEntryV1,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    max_text_bytes: u64,
) -> Result<(), ValueTreeError> {
    let field = RuntimeControlFieldV1::ALL[index];
    records
        .values
        .verify_closure(&entry.value, records.record_bytes, &mut *records.cancelled)?;
    if kind == WorldFieldRecordKindV1::Metadata {
        let key = WorldFieldKeyV1::new(field.descriptor(), kind, None)
            .map_err(ValueTreeError::Invalid)?;
        if key.as_bytes() != entry.key {
            return Err(invalid());
        }
        records.presence(&entry.value, &key, 0)?;
    } else {
        let key = records.key(field.descriptor(), kind, &())?;
        if key.0.as_bytes() != entry.key {
            return Err(invalid());
        }
        read_value(
            field,
            &entry.value,
            &key,
            records,
            max_text_bytes,
            &mut std::io::sink(),
        )?;
    }
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(())
}
pub fn qualify_runtime_controls_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    roots: &RootSetV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    max_text_bytes: u64,
) -> Result<ReservedValue<RuntimeControlsQualificationV1>, ValueTreeError> {
    roots.validate().map_err(|_| invalid())?;
    ctx.check()?;
    if !roots.state.iter().any(|r| r == ctx.root) {
        return Err(invalid());
    }
    let retained = ctx.values.memory.reserve(512, ctx.values.class)?;
    let _scratch = ctx.values.memory.reserve(8192, ctx.values.class)?;
    let cancel = RefCell::new(&mut ctx.cancelled);
    let mut metadata = 0u8;
    let mut values = 0u8;
    for root in &roots.state {
        let mut local = WorldFieldReadContextV1 {
            root,
            state: ctx.state,
            values: oasis7_formal_protocol::incremental_value::ValueReadContext {
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
            io: &mut *scan,
        }
        .visit_entries(
            root,
            |entry| {
                if let Some((index, kind)) = identify(entry.namespace, &entry.key)? {
                    check_entry(index, kind, entry, &mut local.records(), max_text_bytes)?;
                    let bits = if kind == WorldFieldRecordKindV1::Metadata {
                        &mut metadata
                    } else {
                        &mut values
                    };
                    if *bits & (1 << index) != 0 {
                        return Err(invalid());
                    }
                    *bits |= 1 << index;
                }
                Ok(())
            },
            || (cancel.borrow_mut())(),
        );
        let original = (local.take_state_error)();
        if (cancel.borrow_mut())() {
            return Err(ValueTreeError::Cancelled);
        }
        if let Some(error) = original {
            return Err(error);
        }
        match result {
            Ok(_) => (),
            Err(StateScanError::Visitor(e)) => return Err(e),
            Err(StateScanError::Memory(e)) => return Err(e.into()),
            Err(StateScanError::ResourceLimited) => {
                return Err(ValueTreeError::ResourceLimited("Runtime controls scan"));
            }
            Err(StateScanError::Cancelled) => return Err(ValueTreeError::Cancelled),
            Err(_) => return Err(invalid()),
        }
    }
    if metadata != 7 || values != 7 {
        return Err(invalid());
    }
    let roots = roots.digest().map_err(|_| invalid())?;
    if (cancel.borrow_mut())() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(retained.retain(RuntimeControlsQualificationV1 {
        roots,
        class: ctx.values.class,
    }))
}
pub fn accept_runtime_controls_transition_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    parent: &ReservedValue<RuntimeControlsQualificationV1>,
    verifier: &TransitionVerifier<'_, P>,
    record: &TransitionRecordV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    max_text_bytes: u64,
) -> Result<ReservedValue<RuntimeControlsQualificationV1>, WorldFieldOverlayErrorV1> {
    ctx.check()?;
    if verifier.class != ctx.values.class || !verifier.memory.same_pool(ctx.values.memory) {
        return Err(invalid().into());
    }
    {
        let _phase = runtime_controls_preflight_v1(parent, record, ctx)?;
    }
    let witness = crate::incremental_verified_world_transition::verify_world_transition_v1(
        verifier, record, ctx,
    )?;
    accept_runtime_controls_verified_v1(parent, &witness, ctx, max_text_bytes)
}

#[path = "incremental_runtime_controls_family_readmission.rs"]
mod readmission;
pub use readmission::readmit_runtime_controls_ordinary_v1;

fn runtime_controls_preflight_v1<M, R, E, C>(
    parent: &ReservedValue<RuntimeControlsQualificationV1>,
    record: &TransitionRecordV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<oasis7_proto::memory_budget::MemoryReservation, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    ctx.check()?;
    if ctx.values.class != oasis7_proto::memory_budget::MemoryClass::Ordinary
        || parent.allocation_class() != ctx.values.class
        || !parent.uses_budget(ctx.values.memory)
        || parent.reserved_bytes() < 512
        || !record.after.state.iter().any(|r| r == ctx.root)
    {
        return Err(invalid().into());
    }
    let phase = ctx
        .values
        .memory
        .reserve(128 << 10, ctx.values.class)
        .map_err(ValueTreeError::Memory)?;
    parent.validate_binding(&record.before)?;
    ctx.check()?;
    Ok(phase)
}
pub(crate) fn accept_runtime_controls_verified_v1<M, R, E, C>(
    parent: &ReservedValue<RuntimeControlsQualificationV1>,
    witness: &crate::incremental_verified_world_transition::VerifiedWorldTransitionV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    max_text_bytes: u64,
) -> Result<ReservedValue<RuntimeControlsQualificationV1>, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    ctx.check()?;
    witness.check_context(ctx.values.memory, ctx.values.class, ctx.root)?;
    let record = witness.record();
    let _phase = runtime_controls_preflight_v1(parent, record, ctx)?;
    let retained = ctx
        .values
        .memory
        .reserve(512, ctx.values.class)
        .map_err(ValueTreeError::Memory)?;

    if !record.after.state.iter().any(|r| r == ctx.root) {
        return Err(invalid().into());
    }
    for write in &record.writes {
        ctx.check()?;
        let Some((index, kind)) = identify(write.namespace, &write.mutation.key)? else {
            continue;
        };
        let value = write.mutation.after.ok_or_else(invalid)?;
        if write.mutation.before.is_none()
            || (kind == WorldFieldRecordKindV1::Metadata
                && write.mutation.before != write.mutation.after)
        {
            return Err(invalid().into());
        }
        let _scratch = ctx
            .values
            .memory
            .reserve(8192, ctx.values.class)
            .map_err(ValueTreeError::Memory)?;
        check_entry(
            index,
            kind,
            &StateEntryV1 {
                namespace: write.namespace,
                key: write.mutation.key.clone(),
                value,
            },
            &mut ctx.records(),
            max_text_bytes,
        )?;
    }
    let roots = record.after.digest().map_err(|_| invalid())?;
    ctx.check()?;
    Ok(retained.retain(RuntimeControlsQualificationV1 {
        roots,
        class: ctx.values.class,
    }))
}
