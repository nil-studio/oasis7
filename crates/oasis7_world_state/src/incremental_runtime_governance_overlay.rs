//! Same-transaction governance column batches with exact selected values.
use super::*;
use crate::{
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1, lookup},
    incremental_field_read::WorldFieldReadContextV1,
    incremental_inventory_value_read::InventoryValueReader,
    incremental_record_value_read::RecordValueV1,
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFence, PreparedGuard, RootSetV1},
    incremental_state::MerkleNodeReader,
};
use oasis7_proto::memory_budget::{MemoryBudget, ReservedValue};
use std::{cell::RefCell, io::Write};
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("Runtime governance typed overlay")
}
pub struct RuntimeGovernanceRecordQualificationV1 {
    parent: [u8; 32],
    key: RuntimeGovernanceKeyV1,
    root: StateRootV1,
    class: MemoryClass,
}
impl RuntimeGovernanceRecordQualificationV1 {
    pub fn key(&self) -> RuntimeGovernanceKeyV1 {
        self.key
    }
    pub fn root(&self) -> &StateRootV1 {
        &self.root
    }
    pub fn allocation_class(&self) -> MemoryClass {
        self.class
    }
    pub fn validate_binding(
        &self,
        roots: &RootSetV1,
        key: RuntimeGovernanceKeyV1,
        root: &StateRootV1,
    ) -> Result<(), ValueTreeError> {
        if self.parent != roots.digest().map_err(|_| invalid())?
            || self.key != key
            || self.root != *root
        {
            return Err(invalid());
        }
        Ok(())
    }
}
pub(super) fn kind(key: RuntimeGovernanceKeyV1) -> RuntimeGovernanceKindV1 {
    match key {
        RuntimeGovernanceKeyV1::Policy => RuntimeGovernanceKindV1::Policy,
        RuntimeGovernanceKeyV1::Epoch(_) => RuntimeGovernanceKindV1::EpochSnapshot,
        RuntimeGovernanceKeyV1::Penalty(_) => RuntimeGovernanceKindV1::IdentityPenalty,
    }
}
pub(super) fn outer<R: ValueNodeReader, C: FnMut() -> bool>(
    key: RuntimeGovernanceKeyV1,
    records: &mut InventoryValueReader<'_, '_, R, C>,
) -> Result<(WorldFieldKeyV1, u64), ValueTreeError> {
    let field = kind(key).descriptor();
    match key {
        RuntimeGovernanceKeyV1::Policy => records.key(field, WorldFieldRecordKindV1::Entry, &()),
        RuntimeGovernanceKeyV1::Epoch(id) | RuntimeGovernanceKeyV1::Penalty(id) => {
            records.key(field, WorldFieldRecordKindV1::Entry, &id)
        }
    }
}
fn write_outer<W: StatePageWriter, C: FnMut() -> bool>(
    key: RuntimeGovernanceKeyV1,
    root: StateRootV1,
    semantic: [u8; 32],
    write: &mut MemberRecordWriteContext<'_, W, C>,
) -> Result<MemberRecordV1, MemberRecordError> {
    let kind = kind(key);
    let reference = RecordRootRefV1::new(kind.schema(), semantic, root)
        .map_err(|_| MemberRecordError::Invalid("governance outer reference"))?;
    let payload = MemberPayloadRefV1::RecordRoot(&reference);
    match key {
        RuntimeGovernanceKeyV1::Policy => write_member_record_v1(
            kind.descriptor(),
            WorldFieldRecordKindV1::Entry,
            &(),
            payload,
            write,
        ),
        RuntimeGovernanceKeyV1::Epoch(id) | RuntimeGovernanceKeyV1::Penalty(id) => {
            write_member_record_v1(
                kind.descriptor(),
                WorldFieldRecordKindV1::Entry,
                &id,
                payload,
                write,
            )
        }
    }
}
/// Select a fully qualified parent's record by an authenticated point lookup.
/// The original family has already checked its complete closed body.
pub fn open_runtime_governance_record_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    parent: &ReservedValue<RuntimeGovernanceQualificationV1>,
    roots: &RootSetV1,
    key: RuntimeGovernanceKeyV1,
    read: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<Option<ReservedValue<RuntimeGovernanceRecordQualificationV1>>, ValueTreeError> {
    read.check()?;
    parent.validate_binding(roots)?;
    if !parent.uses_budget(read.values.memory)
        || parent.reserved_bytes() < 512
        || parent.allocation_class() != read.values.class
        || read.root != roots.state_root(StateNamespaceV1::Authorization)
    {
        return Err(invalid());
    }
    let charge = read.values.memory.reserve(512, read.values.class)?;
    let key_bytes = outer(key, &mut read.records())?;
    let Some(id) = read.lookup(&key_bytes.0)? else {
        if key == RuntimeGovernanceKeyV1::Policy {
            return Err(invalid());
        }
        read.check()?;
        return Ok(None);
    };
    let (identity, root) = super::family::child(kind(key), &key_bytes.0, &id, read)?;
    if identity != key {
        return Err(invalid());
    }
    read.check()?;
    Ok(Some(charge.retain(
        RuntimeGovernanceRecordQualificationV1 {
            parent: roots.digest().map_err(|_| invalid())?,
            key,
            root,
            class: read.values.class,
        },
    )))
}
pub fn readmit_runtime_governance_record_ordinary_v1(
    source: &ReservedValue<RuntimeGovernanceRecordQualificationV1>,
    roots: &RootSetV1,
    memory: &MemoryBudget,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<RuntimeGovernanceRecordQualificationV1>, ValueTreeError> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    if !source.uses_budget(memory)
        || source.reserved_bytes() < 512
        || source.class != MemoryClass::Recovery
        || source.parent != roots.digest().map_err(|_| invalid())?
    {
        return Err(invalid());
    }
    let mut phase = memory.reserve(128 << 10, MemoryClass::Ordinary)?;
    let charge = phase.split(512).map_err(ValueTreeError::Invalid)?;
    let proof = charge.retain(RuntimeGovernanceRecordQualificationV1 {
        parent: source.parent,
        key: source.key,
        root: source.root.clone(),
        class: MemoryClass::Ordinary,
    });
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(proof)
}
#[derive(Clone, Copy)]
pub struct RuntimeGovernanceColumnChangeV1<'a> {
    pub column: RuntimeGovernanceColumnV1<'a>,
    pub expected: Option<RuntimeGovernanceValueV1<'a>>,
    pub after: Option<RuntimeGovernanceValueV1<'a>>,
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
            .ok_or_else(|| std::io::Error::other("governance compare overflow"))?;
        if self.expected.get(self.position..end) != Some(bytes) {
            return Err(std::io::Error::other("governance exact text differs"));
        }
        self.position = end;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
fn expected<R: ValueNodeReader, C: FnMut() -> bool>(
    column: RuntimeGovernanceColumnV1<'_>,
    value: RuntimeGovernanceValueV1<'_>,
    id: &[u8; 32],
    key: &(WorldFieldKeyV1, u64),
    records: &mut InventoryValueReader<'_, '_, R, C>,
) -> Result<(), ValueTreeError> {
    column.payload(value)?;
    use RuntimeGovernanceValueV1 as V;
    let valid = match value {
        V::Text(v) => {
            let mut output = Compare {
                expected: v.as_bytes(),
                position: 0,
            };
            let actual = super::delta::value(column, id, key, records, &mut output)?;
            matches!(actual,RecordValueV1::Text{utf8_bytes} if utf8_bytes==v.len() as u64)
                && output.position == v.len()
        }
        V::Unsigned(v) => {
            super::delta::value(column, id, key, records, &mut std::io::sink())?
                == RecordValueV1::Unsigned(v)
        }
        V::Signed(v) => {
            super::delta::value(column, id, key, records, &mut std::io::sink())?
                == RecordValueV1::Signed(v)
        }
        V::Bool(v) => {
            super::delta::value(column, id, key, records, &mut std::io::sink())?
                == RecordValueV1::Bool(v)
        }
    };
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    if !valid {
        return Err(invalid());
    }
    Ok(())
}
/// One batch is complete: optional presence/value and snapshot count/membership
/// changes must close together. Intermediate candidate roots never grant proof.
pub fn change_runtime_governance_columns_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: StatePageWriter + ValueNodeReader,
    CW: FnMut() -> bool,
>(
    parent: &ReservedValue<RuntimeGovernanceQualificationV1>,
    roots: &RootSetV1,
    record: &ReservedValue<RuntimeGovernanceRecordQualificationV1>,
    changes: &[RuntimeGovernanceColumnChangeV1<'_>],
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    work: &mut RuntimeGovernanceWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<Option<ReservedValue<RuntimeGovernanceRecordQualificationV1>>, WorldFieldOverlayErrorV1>
{
    let cancel = RefCell::new(&mut context.cancelled);
    let write_cancel = RefCell::new(&mut write.cancelled);
    context.transaction.contribute(|tx|{
tx.validate_guard(guard,live)?;parent.validate_binding(roots)?;
if tx.parent_fence().parent_roots!=roots.digest().map_err(|_|invalid())?||!parent.uses_budget(write.memory)||parent.reserved_bytes()<512||parent.allocation_class()!=MemoryClass::Ordinary||!record.uses_budget(write.memory)||record.reserved_bytes()<512||record.class!=MemoryClass::Ordinary||!tx.uses_budget(write.memory)||!context.values.memory.same_pool(write.memory)||context.values.class!=MemoryClass::Ordinary{return Err(invalid().into());}
let mut phase=write.memory.reserve(32768,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;let mut combined=||(*cancel.borrow_mut())()||(*write_cancel.borrow_mut())();
let mut records=InventoryValueReader{values:&mut context.values,key_work:context.key_work,semantic_bytes:context.semantic_bytes,record_bytes:context.record_bytes,cancelled:&mut combined};let field=kind(record.key).descriptor();let meta=WorldFieldKeyV1::new(field,WorldFieldRecordKindV1::Metadata,None).map_err(ValueTreeError::Invalid)?;let meta_id=lookup(tx,&meta,&mut records,&mut context.take_state_error)?.ok_or_else(invalid)?;records.presence(&meta_id,&meta,0)?;
let outer=outer(record.key,&mut records)?;let original=lookup(tx,&outer.0,&mut records,&mut context.take_state_error)?.ok_or_else(invalid)?;let bytes=records.fixed::<{oasis7_formal_protocol::incremental_record_root::RECORD_ROOT_BYTES}>(&original,&outer.0,outer.1,MemberPayloadCodecV1::RecordRoot)?;let semantic=WorldFieldKeyV1::from_bytes(outer.0.as_bytes()).map_err(ValueTreeError::Invalid)?.semantic_key.ok_or_else(invalid)?;let reference=RecordRootRefV1::from_canonical_bytes(&bytes).map_err(|_|invalid())?;let initial=reference.validate_binding(&kind(record.key).schema(),&semantic,StateNamespaceV1::Authorization).map_err(|_|invalid())?.clone();record.validate_binding(roots,record.key,&initial)?;
let mut root=initial.clone();
for change in changes{if combined(){return Err(ValueTreeError::Cancelled.into());}work.cells_remaining=work.cells_remaining.checked_sub(1).ok_or(ValueTreeError::ResourceLimited("governance ordinary cells"))?;if change.column.kind()!=kind(record.key)||!change.column.nullable()&&(change.expected.is_none()||change.after.is_none()){return Err(invalid().into());}if let Some(value)=change.after{change.column.payload(value)?;}
let mut records=InventoryValueReader{values:&mut context.values,key_work:context.key_work,semantic_bytes:context.semantic_bytes,record_bytes:context.record_bytes,cancelled:&mut combined};let key=super::delta::key(change.column,&mut records)?;let current=incremental_record_tree::lookup(&root,&key.0,&mut records,LABELS)?;
match(change.expected,current){(None,None)=>{},(Some(value),Some(id))=>expected(change.column,value,&id,&key,&mut records)?,_=>return Err(invalid().into())}
let unchanged=change.expected==change.after;if combined(){return Err(ValueTreeError::Cancelled.into());}if unchanged{continue;}
let mut output=MemberRecordWriteContext{sink:&mut *write.sink,memory:write.memory,io:&mut *write.io,key_work:&mut *write.key_work,limits:write.limits,cancelled:||(*cancel.borrow_mut())()||(*write_cancel.borrow_mut())()};
let after=if let Some(value)=change.after{let payload=change.column.payload(value)?;let row=with_gov_key!(change.column,key=>write_member_record_v1(kind(record.key).columns(),WorldFieldRecordKindV1::Entry,key,payload,&mut output))?;if row.field_key!=key.0{return Err(invalid().into());}Some(row.value_root)}else{None};root=incremental_record_tree::edit(root,&key.0,(current,after),&mut context.values,&mut output,&mut combined,RecordTreeWritePolicyV1{max_pending_node_bytes:work.max_pending_node_bytes,labels:LABELS})?;
}
if combined(){return Err(ValueTreeError::Cancelled.into());}if root==initial{return Ok(None);}
let mut records=InventoryValueReader{values:&mut context.values,key_work:context.key_work,semantic_bytes:context.semantic_bytes,record_bytes:context.record_bytes,cancelled:&mut combined};super::delta::changed(kind(record.key),&initial,&root,&mut records,work)?;
let mut output=MemberRecordWriteContext{sink:&mut *write.sink,memory:write.memory,io:&mut *write.io,key_work:&mut *write.key_work,limits:write.limits,cancelled:||(*cancel.borrow_mut())()||(*write_cancel.borrow_mut())()};let row=write_outer(record.key,root.clone(),semantic,&mut output)?;if row.field_key!=outer.0{return Err(invalid().into());}if(output.cancelled)(){return Err(ValueTreeError::Cancelled.into());}tx.write(StateNamespaceV1::Authorization,outer.0.as_bytes(),Some(row.value_root))?;
let proof=phase.split(512).map_err(ValueTreeError::Invalid)?.retain(RuntimeGovernanceRecordQualificationV1{parent:record.parent,key:record.key,root,class:MemoryClass::Ordinary});if(output.cancelled)(){return Err(ValueTreeError::Cancelled.into());}Ok(Some(proof))
})
}

/// Create one new map record. Mandatory Policy has no create/delete operation.
pub fn create_runtime_governance_record_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: StatePageWriter + ValueNodeReader,
    CW: FnMut() -> bool,
>(
    parent: &ReservedValue<RuntimeGovernanceQualificationV1>,
    roots: &RootSetV1,
    key: RuntimeGovernanceKeyV1,
    source: RuntimeGovernanceRecordV1<'_>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    work: &mut RuntimeGovernanceWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<ReservedValue<RuntimeGovernanceRecordQualificationV1>, WorldFieldOverlayErrorV1> {
    let cancel = RefCell::new(&mut context.cancelled);
    let write_cancel = RefCell::new(&mut write.cancelled);
    context.transaction.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        parent.validate_binding(roots)?;
        if key == RuntimeGovernanceKeyV1::Policy
            || kind(key) != source.kind()
            || tx.parent_fence().parent_roots != roots.digest().map_err(|_| invalid())?
            || parent.allocation_class() != MemoryClass::Ordinary
            || parent.reserved_bytes() < 512
            || !parent.uses_budget(write.memory)
            || !tx.uses_budget(write.memory)
            || !context.values.memory.same_pool(write.memory)
            || context.values.class != MemoryClass::Ordinary
        {
            return Err(invalid().into());
        }
        let mut phase = write
            .memory
            .reserve(32768, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let mut combined = || (*cancel.borrow_mut())() || (*write_cancel.borrow_mut())();
        let mut records = InventoryValueReader {
            values: &mut context.values,
            key_work: context.key_work,
            semantic_bytes: context.semantic_bytes,
            record_bytes: context.record_bytes,
            cancelled: &mut combined,
        };
        let field = kind(key).descriptor();
        let meta = WorldFieldKeyV1::new(field, WorldFieldRecordKindV1::Metadata, None)
            .map_err(ValueTreeError::Invalid)?;
        let meta_id =
            lookup(tx, &meta, &mut records, &mut context.take_state_error)?.ok_or_else(invalid)?;
        records.presence(&meta_id, &meta, 0)?;
        let outer = outer(key, &mut records)?;
        if lookup(tx, &outer.0, &mut records, &mut context.take_state_error)?.is_some() {
            return Err(invalid().into());
        }
        let semantic = WorldFieldKeyV1::from_bytes(outer.0.as_bytes())
            .map_err(ValueTreeError::Invalid)?
            .semantic_key
            .ok_or_else(invalid)?;
        let mut output = MemberRecordWriteContext {
            sink: &mut *write.sink,
            memory: write.memory,
            io: &mut *write.io,
            key_work: &mut *write.key_work,
            limits: write.limits,
            cancelled: || (*cancel.borrow_mut())() || (*write_cancel.borrow_mut())(),
        };
        let root = prepare_runtime_governance_record_v1(source, &mut output, work)?;
        let row = write_outer(key, root.clone(), semantic, &mut output)?;
        if row.field_key != outer.0 {
            return Err(invalid().into());
        }
        if (output.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        tx.write(
            StateNamespaceV1::Authorization,
            outer.0.as_bytes(),
            Some(row.value_root),
        )?;
        let proof = phase.split(512).map_err(ValueTreeError::Invalid)?.retain(
            RuntimeGovernanceRecordQualificationV1 {
                parent: roots.digest().map_err(|_| invalid())?,
                key,
                root,
                class: MemoryClass::Ordinary,
            },
        );
        if (output.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(proof)
    })
}
pub fn delete_runtime_governance_record_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    parent: &ReservedValue<RuntimeGovernanceQualificationV1>,
    roots: &RootSetV1,
    record: &ReservedValue<RuntimeGovernanceRecordQualificationV1>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<(), WorldFieldOverlayErrorV1> {
    context.transaction.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        parent.validate_binding(roots)?;
        if record.key == RuntimeGovernanceKeyV1::Policy
            || tx.parent_fence().parent_roots != roots.digest().map_err(|_| invalid())?
            || parent.allocation_class() != MemoryClass::Ordinary
            || parent.reserved_bytes() < 512
            || !parent.uses_budget(context.values.memory)
            || record.class != MemoryClass::Ordinary
            || record.reserved_bytes() < 512
            || !record.uses_budget(context.values.memory)
            || !tx.uses_budget(context.values.memory)
            || context.values.class != MemoryClass::Ordinary
        {
            return Err(invalid().into());
        }
        let _phase = context
            .values
            .memory
            .reserve(16384, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let mut records = InventoryValueReader {
            values: &mut context.values,
            key_work: context.key_work,
            semantic_bytes: context.semantic_bytes,
            record_bytes: context.record_bytes,
            cancelled: &mut context.cancelled,
        };
        let field = kind(record.key).descriptor();
        let meta = WorldFieldKeyV1::new(field, WorldFieldRecordKindV1::Metadata, None)
            .map_err(ValueTreeError::Invalid)?;
        let meta_id =
            lookup(tx, &meta, &mut records, &mut context.take_state_error)?.ok_or_else(invalid)?;
        records.presence(&meta_id, &meta, 0)?;
        let outer = outer(record.key, &mut records)?;
        let id = lookup(tx, &outer.0, &mut records, &mut context.take_state_error)?
            .ok_or_else(invalid)?;
        let bytes =
            records
                .fixed::<{ oasis7_formal_protocol::incremental_record_root::RECORD_ROOT_BYTES }>(
                    &id,
                    &outer.0,
                    outer.1,
                    MemberPayloadCodecV1::RecordRoot,
                )?;
        let semantic = WorldFieldKeyV1::from_bytes(outer.0.as_bytes())
            .map_err(ValueTreeError::Invalid)?
            .semantic_key
            .ok_or_else(invalid)?;
        let reference = RecordRootRefV1::from_canonical_bytes(&bytes).map_err(|_| invalid())?;
        let root = reference
            .validate_binding(
                &kind(record.key).schema(),
                &semantic,
                StateNamespaceV1::Authorization,
            )
            .map_err(|_| invalid())?;
        record.validate_binding(roots, record.key, root)?;
        if (context.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        tx.write(StateNamespaceV1::Authorization, outer.0.as_bytes(), None)?;
        if (context.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(())
    })
}
