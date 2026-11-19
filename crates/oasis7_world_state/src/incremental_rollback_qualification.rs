//! Complete closed-record shape validation against the decoded original DTO.
use crate::{
    incremental_field_member::*,
    incremental_inventory_value_read::InventoryValueReader,
    incremental_record_tree,
    incremental_rollback_genesis::{LABELS, RollbackRecordRefV1, descriptor},
    incremental_rollback_read::read_rollback_cell_v1,
    incremental_rollback_records::*,
    incremental_rollback_recovery::RollbackDecodeContextV1,
    incremental_world_field_key::{WorldFieldKeyV1, WorldFieldRecordKindV1},
};
use oasis7_formal_protocol::{
    incremental_record_root::{RECORD_ROOT_BYTES, RecordRootRefV1},
    incremental_state::{StateNamespaceV1, StateRootV1},
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use std::io::{Cursor, Write};
pub struct RollbackRecordQualificationV1 {
    root: StateRootV1,
    schema: [u8; 32],
    original: [u8; 32],
    class: MemoryClass,
}
impl RollbackRecordQualificationV1 {
    pub fn allocation_class(&self) -> MemoryClass {
        self.class
    }

    pub fn validate_binding(
        &self,
        root: &StateRootV1,
        schema: &[u8; 32],
    ) -> Result<(), ValueTreeError> {
        if &self.root == root && &self.schema == schema {
            Ok(())
        } else {
            Err(invalid())
        }
    }
}
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("rollback closed record")
}
struct Compare<'a> {
    expected: &'a [u8],
    position: usize,
}
impl Write for Compare<'_> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let end = self
            .position
            .checked_add(bytes.len())
            .ok_or_else(|| std::io::Error::other("rollback compare overflow"))?;
        if self.expected.get(self.position..end) != Some(bytes) {
            return Err(std::io::Error::other("rollback original mismatch"));
        }
        self.position = end;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
fn markers<R: ValueNodeReader, C: FnMut() -> bool>(
    root: &StateRootV1,
    ctx: &mut RollbackDecodeContextV1<'_, '_, R, C>,
) -> Result<(), ValueTreeError> {
    let mut reader = InventoryValueReader {
        values: ctx.values,
        key_work: ctx.keys,
        semantic_bytes: ctx.limits.max_record_bytes,
        record_bytes: ctx.limits.max_record_bytes,
        cancelled: &mut *ctx.cancelled,
    };
    let key = WorldFieldKeyV1::new(descriptor(), WorldFieldRecordKindV1::Metadata, None)
        .map_err(ValueTreeError::Invalid)?;
    let id =
        incremental_record_tree::lookup(root, &key, &mut reader, LABELS)?.ok_or_else(invalid)?;
    reader.presence(&id, &key, 0)?;
    let key = reader.key(descriptor(), WorldFieldRecordKindV1::Entry, &())?;
    let id =
        incremental_record_tree::lookup(root, &key.0, &mut reader, LABELS)?.ok_or_else(invalid)?;
    reader.fixed::<0>(&id, &key.0, key.1, MemberPayloadCodecV1::Unit)?;
    Ok(())
}
pub(crate) fn scope<R: ValueNodeReader, C: FnMut() -> bool>(
    root: &StateRootV1,
    schema: [u8; 32],
    path: &[RollbackPathV1],
    ctx: &mut RollbackDecodeContextV1<'_, '_, R, C>,
) -> Result<StateRootV1, ValueTreeError> {
    if path.is_empty() {
        return Ok(root.clone());
    }
    let mut bytes = [0; RECORD_ROOT_BYTES];
    read_rollback_cell_v1(
        root,
        schema,
        path,
        MemberPayloadCodecV1::RecordRoot,
        ctx.values,
        ctx.keys,
        ctx.work,
        RECORD_ROOT_BYTES as u64,
        ctx.limits.max_record_bytes,
        &mut Cursor::new(bytes.as_mut_slice()),
        &mut *ctx.cancelled,
    )?;
    let part = path[path.len() - 1];
    let mut reader = InventoryValueReader {
        values: ctx.values,
        key_work: ctx.keys,
        semantic_bytes: ctx.limits.max_record_bytes,
        record_bytes: ctx.limits.max_record_bytes,
        cancelled: &mut *ctx.cancelled,
    };
    let key = match part {
        RollbackPathV1::Column(name) => reader.key(
            descriptor(),
            WorldFieldRecordKindV1::Entry,
            &("column", name),
        ),
        RollbackPathV1::Ordinal(i) => {
            reader.key(descriptor(), WorldFieldRecordKindV1::Entry, &("ordinal", i))
        }
    }?;
    let semantic = WorldFieldKeyV1::from_bytes(key.0.as_bytes())
        .map_err(ValueTreeError::Invalid)?
        .semantic_key
        .ok_or_else(invalid)?;
    let reference = RecordRootRefV1::from_canonical_bytes(&bytes).map_err(|_| invalid())?;
    Ok(reference
        .validate_binding(&schema, &semantic, StateNamespaceV1::Authorization)
        .map_err(|_| invalid())?
        .clone())
}
/// Every expected leaf is authenticated and compared, and each child checks its
/// exact leaf count; omitted/unknown columns and None payload remnants fail.
pub fn qualify_rollback_record_v1<R: ValueNodeReader, C: FnMut() -> bool>(
    root: &StateRootV1,
    source: RollbackRecordRefV1<'_>,
    ctx: &mut RollbackDecodeContextV1<'_, '_, R, C>,
) -> Result<ReservedValue<RollbackRecordQualificationV1>, ValueTreeError> {
    let charge = ctx.values.memory.reserve(16384, ctx.values.class)?;
    let schema = source.schema();
    markers(root, ctx)?;
    let mut roots: [Option<StateRootV1>; 4] = std::array::from_fn(|_| None);
    roots[0] = Some(root.clone());
    let mut counts = [2u64; 4];
    let mut prefix = [RollbackPathV1::Column(""); 3];
    let mut depth = 0usize;
    let mut visit =
        |path: &[RollbackPathV1], value: RollbackValueRefV1<'_>| -> Result<(), ValueTreeError> {
            let scope_path = &path[..path.len() - 1];
            let common = prefix[..depth]
                .iter()
                .zip(scope_path)
                .take_while(|(a, b)| a == b)
                .count();
            while depth > common {
                if roots[depth].as_ref().ok_or_else(invalid)?.leaf_count != counts[depth] {
                    return Err(invalid());
                }
                roots[depth] = None;
                depth -= 1;
            }
            while depth < scope_path.len() {
                prefix[depth] = scope_path[depth];
                counts[depth] = counts[depth].checked_add(1).ok_or_else(invalid)?;
                depth += 1;
                let child = scope(root, schema, &scope_path[..depth], ctx)?;
                markers(&child, ctx)?;
                roots[depth] = Some(child);
                counts[depth] = 2;
            }
            counts[depth] = counts[depth].checked_add(1).ok_or_else(invalid)?;
            let mut fixed = [0; 8];
            let (codec, expected): (MemberPayloadCodecV1, &[u8]) = match value {
                RollbackValueRefV1::Text(v) => (MemberPayloadCodecV1::Text, v.as_bytes()),
                RollbackValueRefV1::U64(v) => {
                    fixed = v.to_be_bytes();
                    (MemberPayloadCodecV1::U64, &fixed)
                }
                RollbackValueRefV1::Bool(v) => {
                    fixed[0] = u8::from(v);
                    (MemberPayloadCodecV1::Bool, &fixed[..1])
                }
            };
            let mut output = Compare {
                expected,
                position: 0,
            };
            let info = read_rollback_cell_v1(
                root,
                schema,
                path,
                codec,
                ctx.values,
                ctx.keys,
                ctx.work,
                expected.len() as u64,
                ctx.limits.max_record_bytes,
                &mut output,
                &mut *ctx.cancelled,
            )?;
            if info.payload_bytes != expected.len() as u64 || output.position != expected.len() {
                return Err(invalid());
            }
            Ok(())
        };
    let result = match source {
        RollbackRecordRefV1::Authority(v) => visit_rollback_authority_v1(v, &mut visit),
        RollbackRecordRefV1::Outcome(v) => visit_rollback_outcome_v1(v, &mut visit),
    };
    match result {
        Ok(()) => {}
        Err(RollbackVisitErrorV1::Sink(e)) => return Err(e),
        Err(RollbackVisitErrorV1::IntegerOverflow) => return Err(invalid()),
    }
    for i in 0..=depth {
        if roots[i].as_ref().ok_or_else(invalid)?.leaf_count != counts[i] {
            return Err(invalid());
        }
    }
    if (ctx.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    let original = fingerprint(source, ctx)?;
    Ok(charge.retain(RollbackRecordQualificationV1 {
        root: root.clone(),
        schema,
        original,
        class: ctx.values.class,
    }))
}

pub(crate) fn fingerprint<R: ValueNodeReader, C: FnMut() -> bool>(
    source: RollbackRecordRefV1<'_>,
    ctx: &mut RollbackDecodeContextV1<'_, '_, R, C>,
) -> Result<[u8; 32], ValueTreeError> {
    let mut hash = blake3::Hasher::new();
    hash.update(b"oasis7.iws.rollback.original-projection.v1\0");
    hash.update(&source.schema());
    let mut emit =
        |path: &[RollbackPathV1], value: RollbackValueRefV1<'_>| -> Result<(), ValueTreeError> {
            if (ctx.cancelled)() {
                return Err(ValueTreeError::Cancelled);
            }
            ctx.work.cells_remaining = ctx
                .work
                .cells_remaining
                .checked_sub(1)
                .ok_or(ValueTreeError::ResourceLimited("rollback source cells"))?;
            hash.update(&[path.len() as u8]);
            for part in path {
                match part {
                    RollbackPathV1::Column(name) => {
                        hash.update(&[0]);
                        hash.update(&(name.len() as u64).to_be_bytes());
                        hash.update(name.as_bytes());
                    }
                    RollbackPathV1::Ordinal(value) => {
                        hash.update(&[1]);
                        hash.update(&value.to_be_bytes());
                    }
                }
            }
            match value {
                RollbackValueRefV1::Text(value) => {
                    ctx.keys.bytes_remaining = ctx
                        .keys
                        .bytes_remaining
                        .checked_sub(value.len() as u64)
                        .ok_or(ValueTreeError::ResourceLimited("rollback source bytes"))?;
                    hash.update(&[0]);
                    hash.update(&(value.len() as u64).to_be_bytes());
                    hash.update(value.as_bytes());
                }
                RollbackValueRefV1::U64(value) => {
                    hash.update(&[1]);
                    hash.update(&value.to_be_bytes());
                }
                RollbackValueRefV1::Bool(value) => {
                    hash.update(&[2, u8::from(value)]);
                }
            }
            Ok(())
        };
    let result = match source {
        RollbackRecordRefV1::Authority(v) => visit_rollback_authority_v1(v, &mut emit),
        RollbackRecordRefV1::Outcome(v) => visit_rollback_outcome_v1(v, &mut emit),
    };
    match result {
        Ok(()) => {}
        Err(RollbackVisitErrorV1::Sink(e)) => return Err(e),
        _ => return Err(invalid()),
    }
    if (ctx.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(*hash.finalize().as_bytes())
}
pub(crate) fn validate_original<R: ValueNodeReader, C: FnMut() -> bool>(
    qualification: &RollbackRecordQualificationV1,
    source: RollbackRecordRefV1<'_>,
    ctx: &mut RollbackDecodeContextV1<'_, '_, R, C>,
) -> Result<(), ValueTreeError> {
    if qualification.original != fingerprint(source, ctx)? {
        return Err(invalid());
    }
    Ok(())
}
pub(crate) fn inherit_delta<R: ValueNodeReader, C: FnMut() -> bool>(
    root: StateRootV1,
    source: RollbackRecordRefV1<'_>,
    ctx: &mut RollbackDecodeContextV1<'_, '_, R, C>,
) -> Result<ReservedValue<RollbackRecordQualificationV1>, ValueTreeError> {
    let charge = ctx.values.memory.reserve(8192, ctx.values.class)?;
    let original = fingerprint(source, ctx)?;
    Ok(charge.retain(RollbackRecordQualificationV1 {
        root,
        schema: source.schema(),
        original,
        class: ctx.values.class,
    }))
}

/// Re-admit the small immutable structural proof. It owns no DTO allocation.
pub fn readmit_rollback_record_ordinary_v1(
    source: &ReservedValue<RollbackRecordQualificationV1>,
    memory: &MemoryBudget,
) -> Result<ReservedValue<RollbackRecordQualificationV1>, ValueTreeError> {
    if !source.uses_budget(memory)
        || source.reserved_bytes() < std::mem::size_of::<RollbackRecordQualificationV1>()
    {
        return Err(invalid());
    }
    let charge = memory.reserve(8192, MemoryClass::Ordinary)?;
    Ok(charge.retain(RollbackRecordQualificationV1 {
        root: source.root.clone(),
        schema: source.schema,
        original: source.original,
        class: MemoryClass::Ordinary,
    }))
}
