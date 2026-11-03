//! Same-transaction queue membership changes. These writes remain provisional
//! until generic replay and this owner's changed-path receiver succeed.
use super::*;
use crate::{
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1, lookup},
    incremental_inventory_value_read::InventoryValueReader,
    incremental_record_value_read::RecordValueV1,
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFence, ExecutionTransaction, PreparedGuard},
    incremental_state::MerkleNodeReader,
};
use oasis7_proto::memory_budget::ReservedValue;

/// A body root is an exact expected value, never a capability or dispatch grant.
/// Handoff obtains the root from the current pending membership itself.
#[derive(Clone, Copy)]
pub enum RuntimeEffectMembershipChangeV1<'a> {
    AppendPending {
        intent: &'a EffectIntent,
    },
    RemovePending {
        ordinal: u64,
        expected_body: &'a StateRootV1,
    },
    Handoff {
        ordinal: u64,
        expected_body: &'a StateRootV1,
        inflight_key: &'a str,
        expected_inflight: Option<&'a StateRootV1>,
    },
    PutInflight {
        key: &'a str,
        expected_body: Option<&'a StateRootV1>,
        intent: &'a EffectIntent,
    },
    RemoveInflight {
        key: &'a str,
        expected_body: &'a StateRootV1,
    },
}
struct Item {
    body: StateRootV1,
    prev: Option<u64>,
    next: Option<u64>,
}
fn bad() -> WorldFieldOverlayErrorV1 {
    family::invalid().into()
}
fn outer_key<R: ValueNodeReader, C: FnMut() -> bool, K: SemanticKeyV1 + ?Sized>(
    kind: RuntimeEffectKindV1,
    key: &K,
    records: &mut InventoryValueReader<'_, '_, R, C>,
) -> Result<(WorldFieldKeyV1, u64), ValueTreeError> {
    records.key(kind.descriptor(), WorldFieldRecordKindV1::Entry, key)
}
fn pending<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    ordinal: u64,
    tx: &mut ExecutionTransaction<'_, M>,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    error: &mut E,
) -> Result<Item, WorldFieldOverlayErrorV1> {
    let key = outer_key(RuntimeEffectKindV1::Pending, &ordinal, records)?;
    let id = lookup(tx, &key.0, records, error)?.ok_or_else(bad)?;
    let child = family::reference(
        RuntimeEffectKindV1::Pending,
        &id,
        &key,
        false,
        records,
        &mut std::io::sink(),
    )?;
    if child.leaf_count != 5 {
        return Err(bad());
    }
    let descriptor = Space::PendingItem.descriptor();
    let meta = WorldFieldKeyV1::new(descriptor, WorldFieldRecordKindV1::Metadata, None)
        .map_err(ValueTreeError::Invalid)?;
    let id = tree::lookup(&child, &meta, records, LABELS)?.ok_or_else(bad)?;
    records.presence(&id, &meta, 0)?;
    let marker = records.key(descriptor, WorldFieldRecordKindV1::Entry, &())?;
    let id = tree::lookup(&child, &marker.0, records, LABELS)?.ok_or_else(bad)?;
    family::read(
        &id,
        &marker,
        MemberPayloadCodecV1::Unit,
        false,
        0,
        records,
        &mut std::io::sink(),
    )?;
    let body = records.key(descriptor, WorldFieldRecordKindV1::Entry, &((), "body"))?;
    let id = tree::lookup(&child, &body.0, records, LABELS)?.ok_or_else(bad)?;
    let body = family::reference(
        RuntimeEffectKindV1::Inflight,
        &id,
        &body,
        false,
        records,
        &mut std::io::sink(),
    )?;
    let mut links = [None; 2];
    for (i, name) in ["prev", "next"].into_iter().enumerate() {
        let key = records.key(descriptor, WorldFieldRecordKindV1::Entry, &((), name))?;
        let id = tree::lookup(&child, &key.0, records, LABELS)?.ok_or_else(bad)?;
        links[i] = family::optional(&id, &key, records)?;
    }
    if links[0].is_some_and(|v| v >= ordinal) || links[1].is_some_and(|v| v <= ordinal) {
        return Err(bad());
    }
    Ok(Item {
        body,
        prev: links[0],
        next: links[1],
    })
}
fn inflight<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    key: &str,
    tx: &mut ExecutionTransaction<'_, M>,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    error: &mut E,
) -> Result<Option<StateRootV1>, WorldFieldOverlayErrorV1> {
    let key = outer_key(RuntimeEffectKindV1::Inflight, key, records)?;
    lookup(tx, &key.0, records, error)?
        .map(|id| {
            family::reference(
                RuntimeEffectKindV1::Inflight,
                &id,
                &key,
                false,
                records,
                &mut std::io::sink(),
            )
            .map_err(Into::into)
        })
        .transpose()
}
fn frontier<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    tx: &mut ExecutionTransaction<'_, M>,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    error: &mut E,
) -> Result<RuntimeEffectQueueFrontierV1, WorldFieldOverlayErrorV1> {
    for kind in RuntimeEffectKindV1::ALL {
        let meta = WorldFieldKeyV1::new(kind.descriptor(), WorldFieldRecordKindV1::Metadata, None)
            .map_err(ValueTreeError::Invalid)?;
        let id = lookup(tx, &meta, records, error)?.ok_or_else(bad)?;
        records.presence(&id, &meta, 0)?;
        let key = records.key(
            kind.descriptor(),
            WorldFieldRecordKindV1::Entry,
            &(FORMAT, ()),
        )?;
        let id = lookup(tx, &key.0, records, error)?.ok_or_else(bad)?;
        family::read(
            &id,
            &key,
            MemberPayloadCodecV1::Unit,
            false,
            0,
            records,
            &mut std::io::sink(),
        )?;
    }
    let mut values = [None; 4];
    for (i, name) in ["head", "tail", "count", "next_ordinal"]
        .into_iter()
        .enumerate()
    {
        let key = outer_key(RuntimeEffectKindV1::Pending, &((FORMAT, ()), name), records)?;
        let id = lookup(tx, &key.0, records, error)?.ok_or_else(bad)?;
        values[i] = match family::read(
            &id,
            &key,
            MemberPayloadCodecV1::U64,
            i < 2,
            8,
            records,
            &mut std::io::sink(),
        )? {
            RecordValueV1::Unsigned(v) => Some(v),
            RecordValueV1::Unit if i < 2 => None,
            _ => return Err(bad()),
        };
    }
    let value = RuntimeEffectQueueFrontierV1 {
        head: values[0],
        tail: values[1],
        count: values[2].ok_or_else(bad)?,
        next_ordinal: values[3].ok_or_else(bad)?,
    };
    if (value.count == 0) != value.head.is_none()
        || value.head.is_none() != value.tail.is_none()
        || value.head.is_some_and(|v| v >= value.next_ordinal)
        || value.tail.is_some_and(|v| v >= value.next_ordinal)
    {
        return Err(bad());
    }
    Ok(value)
}
fn emit<M: MerkleNodeReader>(
    tx: &mut ExecutionTransaction<'_, M>,
    row: MemberRecordV1,
) -> Result<(), WorldFieldOverlayErrorV1> {
    tx.write(
        StateNamespaceV1::Effects,
        row.field_key.as_bytes(),
        Some(row.value_root),
    )?;
    Ok(())
}
fn write_pending<M: MerkleNodeReader, W: StatePageWriter + ValueNodeReader, C: FnMut() -> bool>(
    ordinal: u64,
    item: Item,
    tx: &mut ExecutionTransaction<'_, M>,
    write: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut RuntimeEffectRecordsWorkV1,
) -> Result<(), WorldFieldOverlayErrorV1> {
    let child = pending_item(item.body, item.prev, item.next, write, work)?;
    work.cell()?;
    let row = root_row(
        Space::Outer(RuntimeEffectKindV1::Pending),
        &ordinal,
        child,
        false,
        write,
    )?;
    emit(tx, row)
}
/// This operation never hashes/reads an unchanged Intent body or params subtree.
/// Append and explicit inflight replacement produce only the new borrowed body.
/// Any failure poisons the entire transaction, including an earlier prefix.
pub fn change_runtime_effect_membership_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: StatePageWriter + ValueNodeReader,
    CW: FnMut() -> bool,
>(
    change: RuntimeEffectMembershipChangeV1<'_>,
    qualification: &ReservedValue<RuntimeEffectFamilyQualificationV1>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    work: &mut RuntimeEffectRecordsWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<(), WorldFieldOverlayErrorV1> {
    let cancel = &mut context.cancelled;
    let error = &mut context.take_state_error;
    context.transaction.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        if context.values.class != MemoryClass::Ordinary
            || !tx.uses_budget(context.values.memory)
            || !tx.uses_budget(write.memory)
            || !qualification.uses_budget(write.memory)
        {
            return Err(bad());
        }
        qualification.validate_binding(&tx.parent_fence().parent_roots)?;
        let _operation = write
            .memory
            .reserve(32 << 10, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let (mut queue, mut item, mut before_neighbor, mut after_neighbor) = {
            let mut combined = || cancel() || (write.cancelled)();
            let mut records = InventoryValueReader {
                values: &mut context.values,
                key_work: &mut *context.key_work,
                semantic_bytes: context.semantic_bytes,
                record_bytes: context.record_bytes,
                cancelled: &mut combined,
            };
            let queue = frontier(tx, &mut records, error)?;
            let ordinal = match &change {
                RuntimeEffectMembershipChangeV1::RemovePending { ordinal, .. }
                | RuntimeEffectMembershipChangeV1::Handoff { ordinal, .. } => Some(*ordinal),
                _ => None,
            };
            let item = ordinal
                .map(|ordinal| pending(ordinal, tx, &mut records, error))
                .transpose()?;
            if let Some(item) = &item {
                let expected = match &change {
                    RuntimeEffectMembershipChangeV1::RemovePending { expected_body, .. }
                    | RuntimeEffectMembershipChangeV1::Handoff { expected_body, .. } => {
                        *expected_body
                    }
                    _ => unreachable!(),
                };
                if &item.body != expected || ordinal.is_some_and(|v| v >= queue.next_ordinal) {
                    return Err(bad());
                }
            }
            let before = item.as_ref().and_then(|v| v.prev).or({
                if matches!(
                    change,
                    RuntimeEffectMembershipChangeV1::AppendPending { .. }
                ) {
                    queue.tail
                } else {
                    None
                }
            });
            let after = item.as_ref().and_then(|v| v.next);
            let before_neighbor = before
                .map(|n| pending(n, tx, &mut records, error).map(|item| (n, item)))
                .transpose()?;
            let after_neighbor = after
                .map(|n| pending(n, tx, &mut records, error).map(|item| (n, item)))
                .transpose()?;
            let inflight_expected = match &change {
                RuntimeEffectMembershipChangeV1::Handoff {
                    inflight_key,
                    expected_inflight,
                    ..
                } => Some((*inflight_key, *expected_inflight)),
                RuntimeEffectMembershipChangeV1::PutInflight {
                    key, expected_body, ..
                } => Some((*key, *expected_body)),
                RuntimeEffectMembershipChangeV1::RemoveInflight { key, expected_body } => {
                    Some((*key, Some(*expected_body)))
                }
                _ => None,
            };
            if let Some((key, expected)) = inflight_expected
                && inflight(key, tx, &mut records, error)?.as_ref() != expected
            {
                return Err(bad());
            }
            (queue, item, before_neighbor, after_neighbor)
        };
        if cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        let mut combined = MemberRecordWriteContext {
            sink: &mut *write.sink,
            memory: write.memory,
            io: &mut *write.io,
            key_work: &mut *write.key_work,
            limits: write.limits,
            cancelled: || cancel() || (write.cancelled)(),
        };
        let pending_changed = matches!(
            change,
            RuntimeEffectMembershipChangeV1::AppendPending { .. }
                | RuntimeEffectMembershipChangeV1::RemovePending { .. }
                | RuntimeEffectMembershipChangeV1::Handoff { .. }
        );
        match change {
            RuntimeEffectMembershipChangeV1::AppendPending { intent } => {
                let ordinal = queue.next_ordinal;
                queue.next_ordinal = ordinal.checked_add(1).ok_or_else(bad)?;
                queue.count = queue.count.checked_add(1).ok_or_else(bad)?;
                let child = body(intent, &mut combined, work)?;
                if let Some((n, mut tail)) = before_neighbor.take() {
                    if tail.next.is_some() {
                        return Err(bad());
                    }
                    tail.next = Some(ordinal);
                    write_pending(n, tail, tx, &mut combined, work)?;
                } else {
                    queue.head = Some(ordinal);
                }
                write_pending(
                    ordinal,
                    Item {
                        body: child,
                        prev: queue.tail,
                        next: None,
                    },
                    tx,
                    &mut combined,
                    work,
                )?;
                queue.tail = Some(ordinal);
            }
            RuntimeEffectMembershipChangeV1::RemovePending { ordinal, .. }
            | RuntimeEffectMembershipChangeV1::Handoff { ordinal, .. } => {
                let item = item.take().ok_or_else(bad)?;
                if item.prev.is_none() && queue.head != Some(ordinal)
                    || item.next.is_none() && queue.tail != Some(ordinal)
                {
                    return Err(bad());
                }
                if let Some((n, mut prev)) = before_neighbor.take() {
                    if prev.next != Some(ordinal) {
                        return Err(bad());
                    }
                    prev.next = item.next;
                    write_pending(n, prev, tx, &mut combined, work)?;
                } else {
                    queue.head = item.next;
                }
                if let Some((n, mut next)) = after_neighbor.take() {
                    if next.prev != Some(ordinal) {
                        return Err(bad());
                    }
                    next.prev = item.prev;
                    write_pending(n, next, tx, &mut combined, work)?;
                } else {
                    queue.tail = item.prev;
                }
                queue.count = queue.count.checked_sub(1).ok_or_else(bad)?;
                let key = crate::incremental_semantic_key::hash_semantic_key_v1(
                    &ordinal,
                    combined.memory,
                    MemoryClass::Ordinary,
                    combined.key_work,
                    &mut combined.cancelled,
                )
                .map_err(MemberRecordError::Key)?;
                let key = WorldFieldKeyV1::new(
                    RuntimeEffectKindV1::Pending.descriptor(),
                    WorldFieldRecordKindV1::Entry,
                    Some(key.digest),
                )
                .map_err(ValueTreeError::Invalid)?;
                tx.write(StateNamespaceV1::Effects, key.as_bytes(), None)?;
                if let RuntimeEffectMembershipChangeV1::Handoff { inflight_key, .. } = change {
                    work.cell()?;
                    let row = root_row(
                        Space::Outer(RuntimeEffectKindV1::Inflight),
                        inflight_key,
                        item.body,
                        false,
                        &mut combined,
                    )?;
                    emit(tx, row)?;
                }
            }
            RuntimeEffectMembershipChangeV1::PutInflight { key, intent, .. } => {
                let child = body(intent, &mut combined, work)?;
                work.cell()?;
                let row = root_row(
                    Space::Outer(RuntimeEffectKindV1::Inflight),
                    key,
                    child,
                    false,
                    &mut combined,
                )?;
                emit(tx, row)?;
            }
            RuntimeEffectMembershipChangeV1::RemoveInflight { key, .. } => {
                let key = crate::incremental_semantic_key::hash_semantic_key_v1(
                    key,
                    combined.memory,
                    MemoryClass::Ordinary,
                    combined.key_work,
                    &mut combined.cancelled,
                )
                .map_err(MemberRecordError::Key)?;
                let key = WorldFieldKeyV1::new(
                    RuntimeEffectKindV1::Inflight.descriptor(),
                    WorldFieldRecordKindV1::Entry,
                    Some(key.digest),
                )
                .map_err(ValueTreeError::Invalid)?;
                tx.write(StateNamespaceV1::Effects, key.as_bytes(), None)?;
            }
        }
        if pending_changed {
            for (name, payload) in [
                (
                    "head",
                    queue
                        .head
                        .map_or(MemberPayloadRefV1::Unit, MemberPayloadRefV1::U64),
                ),
                (
                    "tail",
                    queue
                        .tail
                        .map_or(MemberPayloadRefV1::Unit, MemberPayloadRefV1::U64),
                ),
                ("count", MemberPayloadRefV1::U64(queue.count)),
                ("next_ordinal", MemberPayloadRefV1::U64(queue.next_ordinal)),
            ] {
                work.cell()?;
                let row = row(
                    Space::Outer(RuntimeEffectKindV1::Pending),
                    &((FORMAT, ()), name),
                    payload,
                    &mut combined,
                )?;
                emit(tx, row)?;
            }
        }
        if (combined.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(())
    })
}
