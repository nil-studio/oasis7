//! Exact record-local path copying; unchanged columns and child roots survive.
use crate::{
    incremental_field_member::*,
    incremental_inventory_value_read::InventoryValueReader,
    incremental_record_tree::{self, RecordTreeWritePolicyV1},
    incremental_rollback_genesis::{
        LABELS, RollbackRecordRefV1, RollbackWorkV1, descriptor, empty_scope,
    },
    incremental_rollback_qualification::{RollbackRecordQualificationV1, scope},
    incremental_rollback_read::read_rollback_cell_v1,
    incremental_rollback_records::*,
    incremental_rollback_recovery::RollbackDecodeContextV1,
    incremental_world_field_key::WorldFieldRecordKindV1,
};
use oasis7_formal_protocol::{
    incremental_record_root::RecordRootRefV1,
    incremental_state::{StatePageWriter, StateRootV1},
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Write,
};
type Key = ([RollbackPathV1; 4], usize);
type Rows<'a> = BTreeMap<Key, RollbackValueRefV1<'a>>;
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("rollback exact delta")
}
fn cell(work: &mut RollbackWorkV1) -> Result<(), ValueTreeError> {
    work.cells_remaining = work
        .cells_remaining
        .checked_sub(1)
        .ok_or(ValueTreeError::ResourceLimited("rollback delta cells"))?;
    Ok(())
}
fn collect<'a>(
    source: RollbackRecordRefV1<'a>,
    work: &mut RollbackWorkV1,
) -> Result<Rows<'a>, ValueTreeError> {
    let mut rows = BTreeMap::new();
    let mut emit =
        |path: &[RollbackPathV1], value: RollbackValueRefV1<'a>| -> Result<(), ValueTreeError> {
            cell(work)?;
            if path.len() > 4 {
                return Err(invalid());
            }
            let mut key = [RollbackPathV1::Column(""); 4];
            key[..path.len()].copy_from_slice(path);
            if rows.insert((key, path.len()), value).is_some() {
                return Err(invalid());
            }
            Ok(())
        };
    let result = match source {
        RollbackRecordRefV1::Authority(v) => visit_rollback_authority_v1(v, &mut emit),
        RollbackRecordRefV1::Outcome(v) => visit_rollback_outcome_v1(v, &mut emit),
    };
    match result {
        Ok(()) => Ok(rows),
        Err(RollbackVisitErrorV1::Sink(e)) => Err(e),
        _ => Err(invalid()),
    }
}
macro_rules! with_key {
    ($part:expr,$key:ident=>$body:expr) => {
        match $part {
            RollbackPathV1::Column(name) => {
                let $key = &("column", name);
                $body
            }
            RollbackPathV1::Ordinal(ordinal) => {
                let $key = &("ordinal", ordinal);
                $body
            }
        }
    };
}
fn member_error(error: MemberRecordError) -> ValueTreeError {
    match error {
        MemberRecordError::Value(e) => e,
        _ => invalid(),
    }
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
            .ok_or_else(|| std::io::Error::other("rollback compare"))?;
        if self.expected.get(self.position..end) != Some(bytes) {
            return Err(std::io::Error::other("rollback expected differs"));
        }
        self.position = end;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
fn verify_original<R: ValueNodeReader, C: FnMut() -> bool>(
    root: &StateRootV1,
    schema: [u8; 32],
    key: &Key,
    value: RollbackValueRefV1<'_>,
    ctx: &mut RollbackDecodeContextV1<'_, '_, R, C>,
) -> Result<(), ValueTreeError> {
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
        &key.0[..key.1],
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
}
struct Differ<'a, 'b, 'v, 'w, R, W, C, CW> {
    original: &'a StateRootV1,
    schema: [u8; 32],
    read: &'a mut RollbackDecodeContextV1<'b, 'v, R, C>,
    write: &'a mut MemberRecordWriteContext<'w, W, CW>,
}
impl<
    R: ValueNodeReader,
    W: ValueNodeReader + StatePageWriter,
    C: FnMut() -> bool,
    CW: FnMut() -> bool,
> Differ<'_, '_, '_, '_, R, W, C, CW>
{
    fn edit(
        &mut self,
        prefix: &[RollbackPathV1],
        before_root: Option<StateRootV1>,
        before: &Rows<'_>,
        after: &Rows<'_>,
        changes: &[Key],
    ) -> Result<Option<StateRootV1>, ValueTreeError> {
        if (self.read.cancelled)() || (self.write.cancelled)() {
            return Err(ValueTreeError::Cancelled);
        }
        let after_present = after
            .keys()
            .any(|key| key.1 > prefix.len() && key.0[..prefix.len()] == *prefix);
        if !after_present {
            for (key, value) in before {
                if key.1 > prefix.len() && key.0[..prefix.len()] == *prefix {
                    cell(self.read.work)?;
                    verify_original(self.original, self.schema, key, *value, self.read)?;
                }
            }
            return Ok(None);
        }
        let mut root = match before_root {
            Some(root) => root,
            None => empty_scope(self.schema, self.write, self.read.work).map_err(member_error)?,
        };
        let mut parts = BTreeSet::new();
        for key in changes {
            cell(self.read.work)?;
            parts.insert(key.0[prefix.len()]);
        }
        for part in parts {
            let mut full = [RollbackPathV1::Column(""); 4];
            full[..prefix.len()].copy_from_slice(prefix);
            full[prefix.len()] = part;
            let depth = prefix.len() + 1;
            let key = (full, depth);
            let mut records = InventoryValueReader {
                values: self.read.values,
                key_work: self.read.keys,
                semantic_bytes: self.read.limits.max_record_bytes,
                record_bytes: self.read.limits.max_record_bytes,
                cancelled: self.read.cancelled,
            };
            let field_key =
                with_key!(part,key=>records.key(descriptor(),WorldFieldRecordKindV1::Entry,key))?;
            let old = incremental_record_tree::lookup(&root, &field_key.0, &mut records, LABELS)?;
            let leaf_before = before.get(&key);
            let leaf_after = after.get(&key);
            let new = if leaf_before.is_some() || leaf_after.is_some() {
                if let Some(value) = leaf_before {
                    if old.is_none() {
                        return Err(invalid());
                    }
                    verify_original(self.original, self.schema, &key, *value, self.read)?;
                } else if old.is_some() {
                    return Err(invalid());
                }
                match leaf_after {
                    None => None,
                    Some(value) => {
                        let payload = match value {
                            RollbackValueRefV1::Text(v) => MemberPayloadRefV1::Text(v),
                            RollbackValueRefV1::U64(v) => MemberPayloadRefV1::U64(*v),
                            RollbackValueRefV1::Bool(v) => MemberPayloadRefV1::Bool(*v),
                        };
                        let row=with_key!(part,key=>write_member_record_v1(descriptor(),WorldFieldRecordKindV1::Entry,key,payload,self.write)).map_err(member_error)?;
                        Some(row.value_root)
                    }
                }
            } else {
                let before_present = before
                    .keys()
                    .any(|key| key.1 > depth && key.0[..depth] == full[..depth]);
                let child = if before_present {
                    if old.is_none() {
                        return Err(invalid());
                    }
                    Some(scope(
                        self.original,
                        self.schema,
                        &full[..depth],
                        self.read,
                    )?)
                } else {
                    if old.is_some() {
                        return Err(invalid());
                    }
                    None
                };
                let child_changes: Vec<_> = changes
                    .iter()
                    .filter(|key| key.1 > depth && key.0[..depth] == full[..depth])
                    .copied()
                    .collect();
                match self.edit(&full[..depth], child, before, after, &child_changes)? {
                    None => None,
                    Some(child) => {
                        let semantic=with_key!(part,key=>crate::incremental_semantic_key::hash_semantic_key_v1(key,self.write.memory,MemoryClass::Ordinary,self.write.key_work,&mut self.write.cancelled)).map_err(|_|invalid())?;
                        let reference = RecordRootRefV1::new(self.schema, semantic.digest, child)
                            .map_err(|_| invalid())?;
                        let row=with_key!(part,key=>write_member_record_v1(descriptor(),WorldFieldRecordKindV1::Entry,key,MemberPayloadRefV1::RecordRoot(&reference),self.write)).map_err(member_error)?;
                        Some(row.value_root)
                    }
                }
            };
            let policy = RecordTreeWritePolicyV1 {
                max_pending_node_bytes: self.read.work.max_pending_node_bytes,
                labels: LABELS,
            };
            root = incremental_record_tree::edit(
                root,
                &field_key.0,
                (old, new),
                self.read.values,
                self.write,
                self.read.cancelled,
                policy,
            )?;
        }
        Ok(Some(root))
    }
}
/// Provisional owner delta only. Outer transaction must bind the original key,
/// guard/fence and exact RootSet and poison on any error before publishing.
pub fn prepare_rollback_record_delta_v1<
    'a,
    R: ValueNodeReader,
    W: ValueNodeReader + StatePageWriter,
    C: FnMut() -> bool,
    CW: FnMut() -> bool,
>(
    root: &StateRootV1,
    qualification: &ReservedValue<RollbackRecordQualificationV1>,
    before: RollbackRecordRefV1<'a>,
    after: RollbackRecordRefV1<'a>,
    read: &mut RollbackDecodeContextV1<'_, '_, R, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
) -> Result<ReservedValue<StateRootV1>, ValueTreeError> {
    if qualification.allocation_class() != MemoryClass::Ordinary
        || qualification.reserved_bytes() < std::mem::size_of::<RollbackRecordQualificationV1>()
        || read.values.class != MemoryClass::Ordinary
        || !read.values.memory.same_pool(write.memory)
        || !qualification.uses_budget(write.memory)
        || before.schema() != after.schema()
    {
        return Err(invalid());
    }
    qualification.validate_binding(root, &before.schema())?;
    crate::incremental_rollback_qualification::validate_original(qualification, before, read)?;
    let index_bytes = usize::try_from(read.work.cells_remaining)
        .map_err(|_| invalid())?
        .checked_mul(1024)
        .ok_or_else(invalid)?;
    let _index = write.memory.reserve(index_bytes, MemoryClass::Ordinary)?;
    let output_charge = write.memory.reserve(8192, MemoryClass::Ordinary)?;
    let before_rows = collect(before, read.work)?;
    let after_rows = collect(after, read.work)?;
    let mut changes = BTreeSet::new();
    for (key, value) in &before_rows {
        if after_rows.get(key) != Some(value) {
            changes.insert(*key);
        }
    }
    for (key, value) in &after_rows {
        if before_rows.get(key) != Some(value) {
            changes.insert(*key);
        }
    }
    if (read.cancelled)() || (write.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    if changes.is_empty() {
        return Ok(output_charge.retain(root.clone()));
    }
    let changes: Vec<_> = changes.into_iter().collect();
    let schema = before.schema();
    let mut differ = Differ {
        original: root,
        schema,
        read,
        write,
    };
    let root = differ
        .edit(&[], Some(root.clone()), &before_rows, &after_rows, &changes)?
        .ok_or_else(invalid)?;
    if (differ.read.cancelled)() || (differ.write.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(output_charge.retain(root))
}
