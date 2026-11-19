use super::*;
use crate::incremental_field_overlay::{WorldFieldOverlayErrorV1, lookup};
use oasis7_formal_protocol::incremental_execution::{
    ExecutionFence, ExecutionTransaction, PreparedGuard, RootSetV1,
};
use std::cell::RefCell;
/// Data observed at one exact transaction revision; never business permission.
pub struct DiplomacyRelationObservationV1 {
    rows: Vec<HeldRow>,
    guard: PreparedGuard,
    memory: MemoryBudget,
}
impl DiplomacyRelationObservationV1 {
    /// Complete distinct outer IDs, in the original BTreeMap iteration order.
    pub fn ids(&self) -> impl ExactSizeIterator<Item = &str> {
        self.rows.iter().map(|row| row.outer.as_str())
    }
    pub fn validate<M: MerkleNodeReader>(
        &self,
        tx: &mut ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        live: &ExecutionFence,
    ) -> Result<(), WorldFieldOverlayErrorV1> {
        tx.contribute(|tx| {
            tx.validate_guard(&self.guard, live)?;
            if !self.memory.same_pool(memory) || !tx.uses_budget(memory) {
                return Err(closure::invalid().into());
            }
            Ok(())
        })
    }
}
struct Pending {
    field: DiplomacyFieldV1,
    key: WorldFieldKeyV1,
    before: Option<[u8; 32]>,
    after: Option<[u8; 32]>,
}
struct Changed {
    key: WorldFieldKeyV1,
    row: Option<HeldRow>,
}
pub(super) fn edge_kind(
    bytes: &[u8],
    namespace: StateNamespaceV1,
) -> Result<Option<(DiplomacyFieldV1, WorldFieldKeyV1, WorldFieldRecordKindV1)>, ValueTreeError> {
    if !bytes.starts_with(b"oasis7.iws.world-field-key.v1\0") {
        return Ok(None);
    }
    let parsed = WorldFieldKeyV1::from_bytes(bytes).map_err(ValueTreeError::Invalid)?;
    let Some(field) = DiplomacyFieldV1::from_name(parsed.name) else {
        return Ok(None);
    };
    let key = WorldFieldKeyV1::new(field.descriptor()?, parsed.kind, parsed.semantic_key)
        .map_err(ValueTreeError::Invalid)?;
    if key.as_bytes() != bytes || namespace != StateNamespaceV1::Business {
        return Err(closure::invalid());
    }
    Ok(Some((field, key, parsed.kind)))
}
fn range(
    relations: &[Relation],
    target: &str,
    upper: bool,
    work: &mut DiplomacyIndexWorkV1,
    mut cancel: impl FnMut() -> bool,
) -> Result<usize, ValueTreeError> {
    let (mut lo, mut hi) = (0, relations.len());
    while lo < hi {
        if cancel() {
            return Err(ValueTreeError::Cancelled);
        }
        let mid = lo + (hi - lo) / 2;
        let cmp = work.compare(&relations[mid].target, target)?;
        if cmp.is_lt() || (upper && cmp.is_eq()) {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    if cancel() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(lo)
}
#[allow(clippy::too_many_arguments)]
fn observe<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    target: &str,
    war: bool,
    parent: &ReservedValue<DiplomacyFamilyQualificationV1>,
    roots: &RootSetV1,
    tx: &mut ExecutionTransaction<'_, M>,
    ctx: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
    work: &mut DiplomacyIndexWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<ReservedValue<DiplomacyRelationObservationV1>, WorldFieldOverlayErrorV1> {
    tx.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        ctx.check()?;
        let _phase = ctx
            .values
            .memory
            .reserve(64 << 10, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        if ctx.values.class != MemoryClass::Ordinary
            || !parent.uses_budget(ctx.values.memory)
            || !parent.permits_class(MemoryClass::Ordinary)
            || !tx.uses_budget(ctx.values.memory)
        {
            return Err(closure::invalid().into());
        }
        let digest = roots.digest().map_err(|_| closure::invalid())?;
        parent.validate_binding(&digest)?;
        if digest != tx.parent_fence().parent_roots
            || ctx.root != roots.state_root(StateNamespaceV1::Business)
        {
            return Err(closure::invalid().into());
        }
        if parent.relations.rows.len() > work.max_rows || target.len() > work.max_text_bytes {
            return Err(ValueTreeError::ResourceLimited("diplomacy observation limits").into());
        }
        for field in [DiplomacyFieldV1::Alliances, DiplomacyFieldV1::Wars] {
            let key =
                WorldFieldKeyV1::new(field.descriptor()?, WorldFieldRecordKindV1::Metadata, None)
                    .map_err(ValueTreeError::Invalid)?;
            let value = current_lookup(tx, &key, ctx)?.ok_or_else(closure::invalid)?;
            ctx.records().presence(&value, &key, 0)?;
        }
        let pending_bytes = mul(
            work.max_pending_rows,
            add(
                std::mem::size_of::<Pending>(),
                std::mem::size_of::<Changed>(),
            )?,
        )?;
        let _pending_fee = ctx
            .values
            .memory
            .reserve(pending_bytes, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let mut pending = Vec::with_capacity(work.max_pending_rows);
        let mut remaining = std::mem::take(&mut work.pending_entries_remaining);
        let inspected = (|| {
            let cancel = RefCell::new(&mut ctx.cancelled);
            for root in &roots.state {
                let namespace = root.namespace;
                tx.visit_pending_writes::<WorldFieldOverlayErrorV1>(
                    namespace,
                    &mut remaining,
                    || {
                        if (cancel.borrow_mut())() {
                            Err(ValueTreeError::Cancelled.into())
                        } else {
                            Ok(())
                        }
                    },
                    |bytes, before, after| {
                        if (cancel.borrow_mut())() {
                            return Err(ValueTreeError::Cancelled.into());
                        }
                        work.step()?;
                        let Some((field, key, kind)) = edge_kind(bytes, namespace)? else {
                            return Ok(());
                        };
                        match kind {
                            WorldFieldRecordKindV1::Metadata => {
                                if before != after {
                                    return Err(closure::invalid().into());
                                }
                            }
                            WorldFieldRecordKindV1::Entry
                                if matches!(
                                    field,
                                    DiplomacyFieldV1::Alliances | DiplomacyFieldV1::Wars
                                ) =>
                            {
                                if pending.len() == pending.capacity() {
                                    return Err(ValueTreeError::ResourceLimited(
                                        "diplomacy pending rows",
                                    )
                                    .into());
                                }
                                pending.push(Pending {
                                    field,
                                    key,
                                    before,
                                    after,
                                });
                            }
                            WorldFieldRecordKindV1::Entry => {}
                            _ => return Err(closure::invalid().into()),
                        }
                        Ok(())
                    },
                )?;
            }
            Ok::<(), WorldFieldOverlayErrorV1>(())
        })();
        work.pending_entries_remaining = remaining;
        inspected?;
        ctx.check()?;
        let mut changes = Vec::with_capacity(work.max_pending_rows);
        let mut total =
            i128::try_from(parent.relations.rows.len()).map_err(|_| closure::invalid())?;
        for change in pending {
            ctx.check()?;
            work.step()?;
            let old = find(
                &parent.relations.rows,
                change.field,
                &change.key,
                work,
                &mut ctx.cancelled,
            )?;
            let old = old.map(|i| &parent.relations.rows[i]);
            match (old, change.before) {
                (Some(row), Some(value)) if row.member == value => {}
                (None, None) => {}
                _ => return Err(closure::invalid().into()),
            }
            total += i128::from(change.after.is_some()) - i128::from(change.before.is_some());
            let actual = current_lookup(tx, &change.key, ctx)?;
            if actual != change.after {
                return Err(closure::invalid().into());
            }
            let row = if let Some(value) = actual {
                let max = work.max_text_bytes as u64;
                Some(changed_row(
                    old,
                    &StateEntryV1 {
                        namespace: StateNamespaceV1::Business,
                        key: change.key.as_bytes().to_vec(),
                        value,
                    },
                    ctx,
                    work,
                    max,
                )?)
            } else {
                if change.field != DiplomacyFieldV1::Alliances || old.is_none() {
                    return Err(closure::invalid().into());
                }
                None
            };
            changes.push(Changed {
                key: change.key,
                row,
            });
        }
        if total < 0 || usize::try_from(total).map_err(|_| closure::invalid())? > work.max_rows {
            return Err(ValueTreeError::ResourceLimited("diplomacy Current rows").into());
        }
        let relations = if war {
            &parent.relations.wars
        } else {
            &parent.relations.members
        };
        let start = range(relations, target, false, work, &mut ctx.cancelled)?;
        let end = range(relations, target, true, work, &mut ctx.cancelled)?;
        let extra = changes.iter().try_fold(0usize, |n, change| {
            ctx.check()?;
            work.step()?;
            add(
                n,
                change
                    .row
                    .as_ref()
                    .map_or(0, |row| if war { 2 } else { row.members.len() }),
            )
        })?;
        let capacity = add(end - start, extra)?;
        let fee = ctx
            .values
            .memory
            .reserve(
                add(
                    std::mem::size_of::<DiplomacyRelationObservationV1>(),
                    mul(capacity, std::mem::size_of::<HeldRow>())?,
                )?,
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        let mut matches = Vec::with_capacity(capacity);
        for relation in &relations[start..end] {
            ctx.check()?;
            work.step()?;
            let mut changed = false;
            for change in &changes {
                work.step()?;
                if change.key == relation.row.key {
                    changed = true;
                    break;
                }
            }
            if !changed {
                reauthenticate(target, war, &relation.row, relation.ordinal, tx, ctx, work)?;
                matches.push(relation.row.clone());
            }
        }
        for change in &changes {
            let Some(row) = &change.row else { continue };
            if war && row.field == DiplomacyFieldV1::Wars && row.active {
                let a = row.aggressor.as_ref().ok_or_else(closure::invalid)?;
                let d = row.defender.as_ref().ok_or_else(closure::invalid)?;
                if work.compare(a, target)?.is_eq() || work.compare(d, target)?.is_eq() {
                    reauthenticate(target, true, row, None, tx, ctx, work)?;
                    matches.push(row.clone());
                }
            } else if !war && row.field == DiplomacyFieldV1::Alliances {
                for (i, member) in row.members.iter().enumerate() {
                    ctx.check()?;
                    if work.compare(member, target)?.is_eq() {
                        reauthenticate(target, false, row, Some(i as u64), tx, ctx, work)?;
                        matches.push(row.clone());
                    }
                }
            }
        }
        sort(&mut matches, work, &mut ctx.cancelled, |a, b, w| {
            w.compare(&a.outer, &b.outer)
        })?;
        // Multiple ordinals (or both war sides) are one original map record.
        let mut used = 0;
        for i in 0..matches.len() {
            ctx.check()?;
            work.step()?;
            if used == 0
                || !work
                    .compare(&matches[used - 1].outer, &matches[i].outer)?
                    .is_eq()
            {
                matches.swap(used, i);
                used += 1;
            }
        }
        matches.truncate(used);
        ctx.check()?;
        let guard = tx.guard();
        tx.validate_guard(&guard, live)?;
        Ok(fee.retain(DiplomacyRelationObservationV1 {
            rows: matches,
            guard,
            memory: ctx.values.memory.clone(),
        }))
    })
}
#[allow(clippy::too_many_arguments)]
fn reauthenticate<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    target: &str,
    war: bool,
    row: &HeldRow,
    ordinal: Option<u64>,
    tx: &mut ExecutionTransaction<'_, M>,
    ctx: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
    work: &mut DiplomacyIndexWorkV1,
) -> Result<(), WorldFieldOverlayErrorV1> {
    ctx.check()?;
    work.step()?;
    let actual = current_lookup(tx, &row.key, ctx)?;
    if actual != Some(row.member) {
        return Err(closure::invalid().into());
    }
    let mut local = WorldFieldReadContextV1 {
        root: &row.child,
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
        cancelled: &mut ctx.cancelled,
    };
    if war {
        if !active_at(&mut local, work)? {
            return Err(closure::invalid().into());
        }
        let a = text_at(
            DiplomacyFieldV1::Wars,
            profile::specs(DiplomacyFieldV1::Wars)[3].path,
            &mut local,
            work,
        )?;
        let d = text_at(
            DiplomacyFieldV1::Wars,
            profile::specs(DiplomacyFieldV1::Wars)[4].path,
            &mut local,
            work,
        )?;
        if !work.compare(&a, target)?.is_eq() && !work.compare(&d, target)?.is_eq() {
            return Err(closure::invalid().into());
        }
    } else {
        let i = ordinal.ok_or_else(closure::invalid)?;
        if i >= closure::length(DiplomacyFieldV1::Alliances, &mut local)? {
            return Err(closure::invalid().into());
        }
        let member = text_at(
            DiplomacyFieldV1::Alliances,
            Path::Ordinal("members", i),
            &mut local,
            work,
        )?;
        if !work.compare(&member, target)?.is_eq() {
            return Err(closure::invalid().into());
        }
    }
    local.check()?;
    Ok(())
}
/// Complete membership observations retain multiple distinct matching alliances.
#[allow(clippy::too_many_arguments)]
pub fn observe_current_alliance_memberships_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    agent: &str,
    parent: &ReservedValue<DiplomacyFamilyQualificationV1>,
    roots: &RootSetV1,
    tx: &mut ExecutionTransaction<'_, M>,
    ctx: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
    work: &mut DiplomacyIndexWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<ReservedValue<DiplomacyRelationObservationV1>, WorldFieldOverlayErrorV1> {
    observe(agent, false, parent, roots, tx, ctx, work, guard, live)
}
/// Reports active associations using actual aggressor/defender fields only.
#[allow(clippy::too_many_arguments)]
pub fn observe_current_active_wars_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    alliance: &str,
    parent: &ReservedValue<DiplomacyFamilyQualificationV1>,
    roots: &RootSetV1,
    tx: &mut ExecutionTransaction<'_, M>,
    ctx: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
    work: &mut DiplomacyIndexWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<ReservedValue<DiplomacyRelationObservationV1>, WorldFieldOverlayErrorV1> {
    observe(alliance, true, parent, roots, tx, ctx, work, guard, live)
}

fn current_lookup<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    tx: &mut ExecutionTransaction<'_, M>,
    key: &WorldFieldKeyV1,
    ctx: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
) -> Result<Option<[u8; 32]>, WorldFieldOverlayErrorV1> {
    let mut records = InventoryValueReader {
        values: &mut ctx.values,
        key_work: &mut *ctx.key_work,
        semantic_bytes: ctx.semantic_bytes,
        record_bytes: ctx.record_bytes,
        cancelled: &mut ctx.cancelled,
    };
    lookup(tx, key, &mut records, &mut ctx.take_state_error)
}

/// Opens a point from the genuine complete parent plus the actual Current child.
/// Existing children are checked through the original typed changed-path owner;
/// new children require that owner's complete empty-to-current closure.
#[allow(clippy::too_many_arguments)]
pub fn open_current_diplomacy_relation_record_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    field: DiplomacyFieldV1,
    id: &str,
    parent: &ReservedValue<DiplomacyFamilyQualificationV1>,
    roots: &RootSetV1,
    tx: &mut ExecutionTransaction<'_, M>,
    ctx: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
    work: &mut DiplomacyIndexWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<Option<ReservedValue<DiplomacyRecordQualificationV1>>, WorldFieldOverlayErrorV1> {
    tx.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        ctx.check()?;
        let _phase = ctx
            .values
            .memory
            .reserve(64 << 10, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        if !matches!(field, DiplomacyFieldV1::Alliances | DiplomacyFieldV1::Wars)
            || ctx.values.class != MemoryClass::Ordinary
            || !parent.uses_budget(ctx.values.memory)
            || !parent.permits_class(MemoryClass::Ordinary)
            || !tx.uses_budget(ctx.values.memory)
        {
            return Err(closure::invalid().into());
        }
        let digest = roots.digest().map_err(|_| closure::invalid())?;
        parent.validate_binding(&digest)?;
        if digest != tx.parent_fence().parent_roots
            || ctx.root != roots.state_root(StateNamespaceV1::Business)
        {
            return Err(closure::invalid().into());
        }
        let metadata =
            WorldFieldKeyV1::new(field.descriptor()?, WorldFieldRecordKindV1::Metadata, None)
                .map_err(ValueTreeError::Invalid)?;
        let value = current_lookup(tx, &metadata, ctx)?.ok_or_else(closure::invalid)?;
        ctx.records().presence(&value, &metadata, 0)?;
        let key = ctx
            .key(field.descriptor()?, WorldFieldRecordKindV1::Entry, id)?
            .0;
        let old = find(
            &parent.relations.rows,
            field,
            &key,
            work,
            &mut ctx.cancelled,
        )?;
        let old = old.map(|i| &parent.relations.rows[i]);
        let Some(value) = current_lookup(tx, &key, ctx)? else {
            if field == DiplomacyFieldV1::Wars && old.is_some() {
                return Err(closure::invalid().into());
            }
            ctx.check()?;
            return Ok(None);
        };
        let max = work.max_text_bytes as u64;
        let row = changed_row(
            old,
            &StateEntryV1 {
                namespace: StateNamespaceV1::Business,
                key: key.as_bytes().to_vec(),
                value,
            },
            ctx,
            work,
            max,
        )?;
        let proof = access::mint(
            field,
            access::digest(&key)?,
            &row.child,
            ctx.values.memory,
            MemoryClass::Ordinary,
        )?;
        ctx.check()?;
        Ok(Some(proof))
    })
}
