//! Closed original authorization-root String data; no business authorization.
use super::*;
use crate::{
    incremental_field_overlay::WorldFieldOverlayErrorV1,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_record_value_read::{RecordValueV1, read_record_value_v1},
};
use oasis7_formal_protocol::{
    incremental_execution::{RootSetV1, TransitionRecordV1, TransitionVerifier},
    incremental_state::{StateScanBudget, StateScanContext, StateScanError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use std::cell::RefCell;
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("authorization root family")
}
/// Immutable, exact-RootSet structural evidence for the original String field.
/// Construction is private and confers neither hash semantics nor authority.
pub struct RuntimeAuthorizationRootQualificationV1 {
    roots: [u8; 32],
    class: MemoryClass,
    utf8_bytes: u64,
}
impl RuntimeAuthorizationRootQualificationV1 {
    pub fn allocation_class(&self) -> MemoryClass {
        self.class
    }
    pub fn utf8_bytes(&self) -> u64 {
        self.utf8_bytes
    }
    pub fn validate_binding(&self, roots: &RootSetV1) -> Result<(), ValueTreeError> {
        if roots.digest().map_err(|_| invalid())? != self.roots {
            return Err(invalid());
        }
        Ok(())
    }
}
fn mint(
    roots: &RootSetV1,
    class: MemoryClass,
    utf8_bytes: u64,
    memory: &MemoryBudget,
) -> Result<ReservedValue<RuntimeAuthorizationRootQualificationV1>, ValueTreeError> {
    let retained = memory.reserve(512, class)?;
    let digest = roots.digest().map_err(|_| invalid())?;
    Ok(retained.retain(RuntimeAuthorizationRootQualificationV1 {
        roots: digest,
        class,
        utf8_bytes,
    }))
}
fn identify(
    namespace: StateNamespaceV1,
    bytes: &[u8],
) -> Result<Option<WorldFieldRecordKindV1>, ValueTreeError> {
    if !bytes.starts_with(b"oasis7.iws.world-field-key.v1\0") {
        return Ok(None);
    }
    let binding = WorldFieldKeyV1::from_bytes(bytes).map_err(ValueTreeError::Invalid)?;
    let field = authorization_root_descriptor_v1();
    if binding.owner != field.owner || binding.name != field.name {
        return Ok(None);
    }
    let expected = WorldFieldKeyV1::new(field, binding.kind, binding.semantic_key)
        .map_err(ValueTreeError::Invalid)?;
    if namespace != StateNamespaceV1::Authorization || expected.as_bytes() != bytes {
        return Err(invalid());
    }
    match binding.kind {
        WorldFieldRecordKindV1::Metadata | WorldFieldRecordKindV1::Entry => Ok(Some(binding.kind)),
        _ => Err(invalid()),
    }
}
/// Full bounded stream validation. No output String or collection is retained.
/// All roots are scanned to reject misplaced or additional same-field records.
pub fn qualify_runtime_authorization_root_v1<M, R, E, C>(
    roots: &RootSetV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    max_text_bytes: u64,
) -> Result<ReservedValue<RuntimeAuthorizationRootQualificationV1>, ValueTreeError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    roots.validate().map_err(|_| invalid())?;
    ctx.check()?;
    if ctx.root != roots.state_root(StateNamespaceV1::Authorization) {
        return Err(invalid());
    }
    let _phase = ctx.values.memory.reserve(64 << 10, ctx.values.class)?;
    let utf8_bytes = read_authorization_root_value_v1(ctx, max_text_bytes, &mut std::io::sink())?;
    let meta = WorldFieldKeyV1::new(
        authorization_root_descriptor_v1(),
        WorldFieldRecordKindV1::Metadata,
        None,
    )
    .map_err(ValueTreeError::Invalid)?;
    let entry = ctx
        .key(
            authorization_root_descriptor_v1(),
            WorldFieldRecordKindV1::Entry,
            &(),
        )?
        .0;
    let mut seen = [false; 2];
    {
        let cancel = RefCell::new(&mut ctx.cancelled);
        let error = RefCell::new(&mut ctx.take_state_error);
        for root in &roots.state {
            let result = StateScanContext {
                source: ctx.state,
                memory: ctx.values.memory,
                class: ctx.values.class,
                io: &mut *scan,
            }
            .visit_entries(
                root,
                |row| {
                    let Some(kind) = identify(row.namespace, &row.key)? else {
                        return Ok(());
                    };
                    let (index, expected) = match kind {
                        WorldFieldRecordKindV1::Metadata => (0, &meta),
                        WorldFieldRecordKindV1::Entry => (1, &entry),
                        _ => return Err(invalid()),
                    };
                    if row.key != expected.as_bytes() || std::mem::replace(&mut seen[index], true) {
                        return Err(invalid());
                    }
                    Ok(())
                },
                || (cancel.borrow_mut())(),
            );
            let original = (error.borrow_mut())();
            if (cancel.borrow_mut())() {
                return Err(ValueTreeError::Cancelled);
            }
            if let Some(original) = original {
                return Err(original);
            }
            result.map_err(|e| match e {
                StateScanError::Visitor(e) => e,
                StateScanError::Cancelled => ValueTreeError::Cancelled,
                StateScanError::Memory(e) => ValueTreeError::Memory(e),
                StateScanError::ResourceLimited => {
                    ValueTreeError::ResourceLimited("authorization root family scan")
                }
                StateScanError::Invalid(_) | StateScanError::Tree(_) => invalid(),
            })?;
        }
    }
    ctx.check()?;
    if seen != [true; 2] {
        return Err(invalid());
    }
    let output = mint(roots, ctx.values.class, utf8_bytes, ctx.values.memory)?;
    ctx.check()?;
    Ok(output)
}
/// Re-admit only fixed immutable structural evidence. The original Text has
/// already been fully authenticated and is never cloned into the carrier.
pub fn readmit_runtime_authorization_root_ordinary_v1(
    source: &ReservedValue<RuntimeAuthorizationRootQualificationV1>,
    roots: &RootSetV1,
    memory: &MemoryBudget,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<RuntimeAuthorizationRootQualificationV1>, ValueTreeError> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    if !source.uses_budget(memory)
        || source.reserved_bytes() < 512
        || source.class != MemoryClass::Recovery
    {
        return Err(invalid());
    }
    let _phase = memory.reserve(128 << 10, MemoryClass::Ordinary)?;
    source.validate_binding(roots)?;
    let output = mint(roots, MemoryClass::Ordinary, source.utf8_bytes, memory)?;
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(output)
}
/// Independent replay precedes changed-value checking. Identical rows inherit
/// their qualified original UTF-8 without opening any Text payload pages.
pub fn accept_runtime_authorization_root_transition_v1<M, P, R, E, C>(
    parent: &ReservedValue<RuntimeAuthorizationRootQualificationV1>,
    verifier: &TransitionVerifier<'_, P>,
    record: &TransitionRecordV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    max_text_bytes: u64,
) -> Result<ReservedValue<RuntimeAuthorizationRootQualificationV1>, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
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
    let _phase = ctx
        .values
        .memory
        .reserve(64 << 10, ctx.values.class)
        .map_err(ValueTreeError::Memory)?;
    parent.validate_binding(&record.before)?;
    let replay = verifier.verify(record);
    let original = (ctx.take_state_error)();
    ctx.check()?;
    if let Some(original) = original {
        return Err(original.into());
    }
    let verified = replay?;
    let record = &verified.record;
    if ctx.root != record.after.state_root(StateNamespaceV1::Authorization) {
        return Err(invalid().into());
    }
    let mut utf8_bytes = parent.utf8_bytes;
    for write in &record.writes {
        ctx.check()?;
        let Some(kind) = identify(write.namespace, &write.mutation.key)? else {
            continue;
        };
        if write.mutation.before.is_none() || write.mutation.after.is_none() {
            return Err(invalid().into());
        }
        match kind {
            WorldFieldRecordKindV1::Metadata => {
                let expected = WorldFieldKeyV1::new(authorization_root_descriptor_v1(), kind, None)
                    .map_err(ValueTreeError::Invalid)?;
                if write.mutation.key != expected.as_bytes()
                    || write.mutation.before != write.mutation.after
                {
                    return Err(invalid().into());
                }
            }
            WorldFieldRecordKindV1::Entry => {
                let expected = ctx.key(authorization_root_descriptor_v1(), kind, &())?;
                if write.mutation.key != expected.0.as_bytes() {
                    return Err(invalid().into());
                }
                if write.mutation.before != write.mutation.after {
                    let value = write.mutation.after.ok_or_else(invalid)?;
                    let result = read_record_value_v1(
                        &value,
                        &expected,
                        &mut ctx.records(),
                        max_text_bytes,
                        &mut std::io::sink(),
                        authorization_root_read_profile_v1(),
                    )?;
                    utf8_bytes = match result {
                        RecordValueV1::Text { utf8_bytes } => utf8_bytes,
                        _ => return Err(invalid().into()),
                    };
                }
            }
            _ => return Err(invalid().into()),
        }
    }
    ctx.check()?;
    let output = mint(
        &record.after,
        ctx.values.class,
        utf8_bytes,
        ctx.values.memory,
    )?;
    ctx.check()?;
    Ok(output)
}
