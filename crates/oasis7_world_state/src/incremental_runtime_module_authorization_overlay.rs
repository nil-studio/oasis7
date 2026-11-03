//! Original point contents/contributions inside the one ExecutionTxn.
use super::{read::*, *};
use crate::{
    incremental_expected_bytes::Compare,
    incremental_field_member::*,
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1, lookup},
    incremental_inventory_value_read::InventoryValueReader,
    incremental_record_tree::{self as tree, RecordTreeWritePolicyV1},
    incremental_world_field_key::{WorldFieldKeyV1, WorldFieldRecordKindV1},
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFence, ExecutionTransaction, PreparedGuard},
    incremental_state::{MerkleNodeReader, StatePageWriter, StateRootV1},
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
use std::{cell::RefCell, io::Write};
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ModuleAuthorizationScalarRefV1<'a> {
    Unit,
    Unsigned(u64),
    Text(&'a str),
}
impl<'a> ModuleAuthorizationScalarRefV1<'a> {
    fn payload(self) -> MemberPayloadRefV1<'a> {
        match self {
            Self::Unit => MemberPayloadRefV1::Unit,
            Self::Unsigned(n) => MemberPayloadRefV1::U64(n),
            Self::Text(s) => MemberPayloadRefV1::Text(s),
        }
    }
}
#[derive(Clone, Copy)]
pub struct ModuleAuthorizationCellRequestV1<'a> {
    pub key: ModuleAuthorizationRecordKeyV1<'a>,
    pub address: ModuleAuthorizationAddressV1,
    pub family: &'a ReservedValue<ModuleAuthorizationFamilyQualificationV1>,
    pub body: &'a ReservedValue<ModuleAuthorizationBodyQualificationV1>,
    pub max_payload_bytes: u64,
}
pub struct ModuleAuthorizationCellChangeV1<'a> {
    pub request: ModuleAuthorizationCellRequestV1<'a>,
    pub expected: ModuleAuthorizationScalarRefV1<'a>,
    pub after: ModuleAuthorizationScalarRefV1<'a>,
    pub max_pending_node_bytes: usize,
}
pub(super) fn check_parent<M: MerkleNodeReader, R: ValueNodeReader, C: FnMut() -> bool>(
    family: &ReservedValue<ModuleAuthorizationFamilyQualificationV1>,
    tx: &ExecutionTransaction<'_, M>,
    records: &mut InventoryValueReader<'_, '_, R, C>,
) -> Result<(), ValueTreeError> {
    if records.values.class != MemoryClass::Ordinary
        || !tx.uses_budget(records.values.memory)
        || !family.uses_budget(records.values.memory)
        || family.reserved_bytes() < 512
        || family.allocation_class() != MemoryClass::Ordinary
        || family.parent_digest() != &tx.parent_fence().parent_roots
    {
        return Err(invalid());
    }
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(())
}
pub(super) fn current_length<M, R, E, C>(
    tx: &mut ExecutionTransaction<'_, M>,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    error: &mut E,
) -> Result<u64, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let path = family::length_key(records)?;
    let root = lookup(tx, &path.0, records, error)?.ok_or_else(invalid)?;
    let bytes = records.fixed::<8>(&root, &path.0, path.1, MemberPayloadCodecV1::U64)?;
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(u64::from_be_bytes(bytes))
}
type CurrentRecord = (
    WorldFieldKeyV1,
    Option<([u8; 32], ReservedValue<StateRootV1>)>,
);
pub(super) fn current<M, R, E, C>(
    key: ModuleAuthorizationRecordKeyV1<'_>,
    family: &ReservedValue<ModuleAuthorizationFamilyQualificationV1>,
    tx: &mut ExecutionTransaction<'_, M>,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    error: &mut E,
) -> Result<CurrentRecord, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    check_parent(family, tx, records)?;
    let field = key.field();
    let meta = WorldFieldKeyV1::new(field.descriptor(), WorldFieldRecordKindV1::Metadata, None)
        .map_err(ValueTreeError::Invalid)?;
    let root = lookup(tx, &meta, records, error)?.ok_or_else(invalid)?;
    records.presence(&root, &meta, 0)?;
    let length = if matches!(key, ModuleAuthorizationRecordKeyV1::Policy(_)) {
        Some(current_length(tx, records, error)?)
    } else {
        None
    };
    let path = family::record_key(key, records)?;
    let id = lookup(tx, &path.0, records, error)?;
    if let (ModuleAuthorizationRecordKeyV1::Policy(index), Some(length)) = (key, length)
        && (index < length) != id.is_some()
    {
        return Err(invalid().into());
    }
    let child = id
        .map(|id| family::outer(field, &path.0, &id, records).map(|(root, _)| (id, root)))
        .transpose()?;
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok((path.0, child))
}
pub(super) struct Resolved {
    pub outer: WorldFieldKeyV1,
    pub child: StateRootV1,
    pub path: (WorldFieldKeyV1, u64),
    pub value_root: [u8; 32],
    pub value: ModuleAuthorizationValueV1,
}
pub(super) fn resolve<M, R, E, C>(
    request: ModuleAuthorizationCellRequestV1<'_>,
    tx: &mut ExecutionTransaction<'_, M>,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    error: &mut E,
    work: &mut ModuleAuthorizationReadWorkV1,
    output: &mut impl Write,
) -> Result<Resolved, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    work.cell()?;
    column(request.key.field(), request.address).ok_or_else(invalid)?;
    let (outer, child) = current(request.key, request.family, tx, records, error)?;
    let (_, child) = child.ok_or_else(invalid)?;
    let proof = request.body;
    if !proof.uses_budget(records.values.memory)
        || proof.reserved_bytes() < 512
        || proof.class != MemoryClass::Ordinary
        || proof.field != request.key.field()
        || proof.root != *child
    {
        return Err(invalid().into());
    }
    if let Some(index) = request.address.index {
        let path = read::key(
            request.key.field(),
            ModuleAuthorizationAddressV1::fixed("effect_kinds.length"),
            records,
        )?;
        let id = tree::lookup(&child, &path.0, records, genesis::LABELS)?.ok_or_else(invalid)?;
        let length = records.fixed::<8>(&id, &path.0, path.1, MemberPayloadCodecV1::U64)?;
        if index >= u64::from_be_bytes(length) {
            return Err(invalid().into());
        }
    }
    let path = read::key(request.key.field(), request.address, records)?;
    let id = tree::lookup(&child, &path.0, records, genesis::LABELS)?.ok_or_else(invalid)?;
    let value = read::budget_value(
        request.key.field(),
        request.address,
        &id,
        &path,
        records,
        work,
        request.max_payload_bytes,
        output,
    )?;
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(Resolved {
        outer,
        child: (*child).clone(),
        path,
        value_root: id,
        value,
    })
}
/// Independent readonly resolver; it never invokes a setter/write callback.
pub fn read_module_authorization_overlay_v1<M, R, E, C>(
    request: ModuleAuthorizationCellRequestV1<'_>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    work: &mut ModuleAuthorizationReadWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
    output: &mut impl Write,
) -> Result<ReservedValue<ModuleAuthorizationValueV1>, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    ctx.transaction.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        let charge = ctx
            .values
            .memory
            .reserve(512, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let _scratch = ctx
            .values
            .memory
            .reserve(32 << 10, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let mut records = InventoryValueReader {
            values: &mut ctx.values,
            key_work: &mut *ctx.key_work,
            semantic_bytes: ctx.semantic_bytes,
            record_bytes: ctx.record_bytes,
            cancelled: &mut ctx.cancelled,
        };
        Ok(charge.retain(
            resolve(
                request,
                tx,
                &mut records,
                &mut ctx.take_state_error,
                work,
                output,
            )?
            .value,
        ))
    })
}
fn expected_matches(
    expected: ModuleAuthorizationScalarRefV1<'_>,
    actual: &ModuleAuthorizationValueV1,
    bytes: &Compare<'_>,
) -> bool {
    match (expected, actual) {
        (ModuleAuthorizationScalarRefV1::Unit, ModuleAuthorizationValueV1::Unit) => true,
        (ModuleAuthorizationScalarRefV1::Unsigned(a), ModuleAuthorizationValueV1::Unsigned(b)) => {
            a == *b
        }
        (ModuleAuthorizationScalarRefV1::Text(a), ModuleAuthorizationValueV1::Label(b)) => a == *b,
        (ModuleAuthorizationScalarRefV1::Text(_), ModuleAuthorizationValueV1::Text { .. }) => {
            bytes.is_equal()
        }
        _ => false,
    }
}
/// One existing scalar. List length and decision variant changes use the
/// explicit member/complete-record operations, never half-updated relations.
pub fn change_module_authorization_cell_v1<M, R, E, C, W, CW>(
    change: ModuleAuthorizationCellChangeV1<'_>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    work: &mut ModuleAuthorizationReadWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<Option<ReservedValue<ModuleAuthorizationBodyQualificationV1>>, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: StatePageWriter,
    CW: FnMut() -> bool,
{
    let read_cancel = &mut ctx.cancelled;
    ctx.transaction.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        if !tx.uses_budget(write.memory) || !write.memory.same_pool(ctx.values.memory) {
            return Err(invalid().into());
        }
        let request = change.request;
        if matches!(
            request.address.column,
            "effect_kinds.length" | "decision.kind"
        ) {
            return Err(invalid().into());
        }
        validate(request.key.field(), request.address, change.after.payload())
            .map_err(ValueTreeError::Invalid)?;
        let _scratch = write
            .memory
            .reserve(32 << 10, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let expected_bytes = match change.expected {
            ModuleAuthorizationScalarRefV1::Text(s) => s.as_bytes(),
            _ => &[],
        };
        let mut compare = Compare::new(expected_bytes);
        let cancellation = RefCell::new((read_cancel, &mut write.cancelled));
        let mut cancel = || {
            let mut channels = cancellation.borrow_mut();
            let (read, write) = &mut *channels;
            read() || write()
        };
        let mut records = InventoryValueReader {
            values: &mut ctx.values,
            key_work: &mut *ctx.key_work,
            semantic_bytes: ctx.semantic_bytes,
            record_bytes: ctx.record_bytes,
            cancelled: &mut cancel,
        };
        let resolved = resolve(
            request,
            tx,
            &mut records,
            &mut ctx.take_state_error,
            work,
            &mut compare,
        )?;
        let matched = expected_matches(change.expected, &resolved.value, &compare);
        let comparison = match (change.expected, change.after) {
            (ModuleAuthorizationScalarRefV1::Text(a), ModuleAuthorizationScalarRefV1::Text(b)) => {
                a.len().min(b.len()) as u64
            }
            _ => 8,
        };
        work.comparison_bytes_remaining = work
            .comparison_bytes_remaining
            .checked_sub(comparison)
            .ok_or(ValueTreeError::ResourceLimited(
            "module authorization comparison",
        ))?;
        let unchanged = change.expected == change.after;
        if (records.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        if !matched {
            return Err(invalid().into());
        }
        if unchanged {
            return Ok(None);
        }
        if request.address.column == "decision.reason" {
            let path = read::key(
                request.key.field(),
                ModuleAuthorizationAddressV1::fixed("decision.kind"),
                &mut records,
            )?;
            let id = tree::lookup(&resolved.child, &path.0, &mut records, genesis::LABELS)?
                .ok_or_else(invalid)?;
            let kind = read::value(
                request.key.field(),
                ModuleAuthorizationAddressV1::fixed("decision.kind"),
                &id,
                &path,
                &mut records,
                12,
                &mut std::io::sink(),
            )?;
            if (kind == ModuleAuthorizationValueV1::Label("deny"))
                != matches!(change.after, ModuleAuthorizationScalarRefV1::Text(_))
            {
                return Err(invalid().into());
            }
        }
        let row = {
            let mut combined = MemberRecordWriteContext {
                sink: &mut *write.sink,
                memory: write.memory,
                io: &mut *write.io,
                key_work: &mut *write.key_work,
                limits: write.limits,
                cancelled: &mut *records.cancelled,
            };
            write_member_record_v1(
                genesis::body_descriptor(),
                WorldFieldRecordKindV1::Entry,
                &request.address.semantic(request.key.field()),
                change.after.payload(),
                &mut combined,
            )?
        };
        if row.field_key != resolved.path.0 {
            return Err(invalid().into());
        }
        let mut write_cancel = || {
            let mut channels = cancellation.borrow_mut();
            let (read, write) = &mut *channels;
            read() || write()
        };
        let mut combined = MemberRecordWriteContext {
            sink: &mut *write.sink,
            memory: write.memory,
            io: &mut *write.io,
            key_work: &mut *write.key_work,
            limits: write.limits,
            cancelled: &mut write_cancel,
        };
        let root = tree::edit(
            resolved.child,
            &resolved.path.0,
            (Some(resolved.value_root), Some(row.value_root)),
            records.values,
            &mut combined,
            records.cancelled,
            RecordTreeWritePolicyV1 {
                max_pending_node_bytes: change.max_pending_node_bytes,
                labels: genesis::LABELS,
            },
        )?;
        let outer = {
            let mut combined = MemberRecordWriteContext {
                sink: &mut *write.sink,
                memory: write.memory,
                io: &mut *write.io,
                key_work: &mut *write.key_work,
                limits: write.limits,
                cancelled: &mut *records.cancelled,
            };
            match request.key {
                ModuleAuthorizationRecordKeyV1::Grant(key) => {
                    genesis::root_row(request.key.field(), key, root.clone(), &mut combined)?
                }
                ModuleAuthorizationRecordKeyV1::Policy(index) => {
                    genesis::root_row(request.key.field(), &index, root.clone(), &mut combined)?
                }
            }
        };
        if outer.field_key != resolved.outer {
            return Err(invalid().into());
        }
        if (records.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        tx.write(
            StateNamespaceV1::Authorization,
            outer.field_key.as_bytes(),
            Some(outer.value_root),
        )?;
        let proof = write
            .memory
            .reserve(512, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?
            .retain(ModuleAuthorizationBodyQualificationV1 {
                root,
                field: request.key.field(),
                class: MemoryClass::Ordinary,
            });
        if (records.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(Some(proof))
    })
}
