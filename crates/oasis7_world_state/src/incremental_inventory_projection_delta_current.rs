//! Incremental Inventory JSON and group-presence observations. Private original
//! family index/record kernels remain the sole validation and ordering rules.
use super::*;
use crate::{
    MaterialLedgerId,
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
    incremental_inventory_value_read::InventoryValueReader,
    incremental_runtime_revocation::BusinessDiffProcessingWorkV1,
    incremental_semantic_key::{LedgerKeyRefV1, write_semantic_key_v1},
};
use oasis7_agent_api::world_action::ResourceKind;
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionError, ExecutionFence, ExecutionTransaction, PreparedGuard},
    incremental_value::ValueReadContext,
};
use oasis7_proto::memory_budget::MemoryReservation;
use std::{cell::RefCell, mem::size_of};
#[path = "incremental_inventory_projection_delta_io.rs"]
mod io;
use io::{PaidSource, Processing, encode, json};
fn bad() -> ValueTreeError {
    ValueTreeError::Invalid("Current Inventory typed projection")
}
fn limit() -> ValueTreeError {
    ValueTreeError::ResourceLimited("Current Inventory projection work")
}
#[derive(PartialEq, Eq, PartialOrd, Ord)]
enum Key {
    Resource(ResourceKind),
    Material(String),
    Group(MaterialLedgerId),
    Member(MaterialLedgerId, String),
}
impl Key {
    fn field(&self) -> &'static str {
        match self {
            Self::Resource(_) => "resources",
            Self::Material(_) => "materials",
            _ => "material_ledgers",
        }
    }
    fn bytes(&self) -> usize {
        match self {
            Self::Resource(_) => 32,
            Self::Material(s) => s.len(),
            Self::Group(l) => ledger_bytes(l),
            Self::Member(l, s) => ledger_bytes(l) + s.len(),
        }
    }
}
fn ledger_bytes(l: &MaterialLedgerId) -> usize {
    match l {
        MaterialLedgerId::World => 16,
        MaterialLedgerId::Agent(s) | MaterialLedgerId::Site(s) | MaterialLedgerId::Factory(s) => {
            s.len() + 16
        }
    }
}
fn ledger_copy(l: LedgerKeyRefV1<'_>) -> MaterialLedgerId {
    match l {
        LedgerKeyRefV1::World => MaterialLedgerId::World,
        LedgerKeyRefV1::Agent(s) => MaterialLedgerId::Agent(s.to_owned()),
        LedgerKeyRefV1::Site(s) => MaterialLedgerId::Site(s.to_owned()),
        LedgerKeyRefV1::Factory(s) => MaterialLedgerId::Factory(s.to_owned()),
    }
}
fn semantic_bytes(k: SemanticKeyRefV1<'_>) -> Result<usize, ValueTreeError> {
    Ok(match k {
        SemanticKeyRefV1::Resource(_) => 32,
        SemanticKeyRefV1::Text(s) => s.len(),
        SemanticKeyRefV1::Ledger(l) => match l {
            LedgerKeyRefV1::World => 16,
            LedgerKeyRefV1::Agent(s) | LedgerKeyRefV1::Site(s) | LedgerKeyRefV1::Factory(s) => {
                s.len() + 16
            }
        },
        SemanticKeyRefV1::Pair { left, right } => {
            semantic_bytes(left.decode().map_err(ValueTreeError::Invalid)?)?
                .checked_add(semantic_bytes(
                    right.decode().map_err(ValueTreeError::Invalid)?,
                )?)
                .ok_or_else(limit)?
        }
        _ => return Err(bad()),
    })
}
fn copy_key(kind: WorldFieldRecordKindV1, k: SemanticKeyRefV1<'_>) -> Result<Key, ValueTreeError> {
    Ok(match (k, kind) {
        (SemanticKeyRefV1::Resource(r), WorldFieldRecordKindV1::Entry) => Key::Resource(r),
        (SemanticKeyRefV1::Text(s), WorldFieldRecordKindV1::Entry) => Key::Material(s.to_owned()),
        (SemanticKeyRefV1::Ledger(l), WorldFieldRecordKindV1::Group) => Key::Group(ledger_copy(l)),
        (SemanticKeyRefV1::Pair { left, right }, WorldFieldRecordKindV1::Entry) => {
            let SemanticKeyRefV1::Ledger(l) = left.decode().map_err(ValueTreeError::Invalid)?
            else {
                return Err(bad());
            };
            let SemanticKeyRefV1::Text(s) = right.decode().map_err(ValueTreeError::Invalid)? else {
                return Err(bad());
            };
            Key::Member(ledger_copy(l), s.to_owned())
        }
        _ => return Err(bad()),
    })
}
struct Decoded {
    row: Row,
    key: Option<Key>,
    quantity: Option<i64>,
    fee: MemoryReservation,
}
fn decode<R: ValueNodeReader, C: FnMut() -> bool>(
    key: &WorldFieldKeyV1,
    root: &[u8; 32],
    records: &mut InventoryValueReader<'_, '_, R, C>,
    work: &mut InventoryFamilyWorkV1,
    p: &Processing<'_>,
) -> Result<Decoded, ValueTreeError> {
    let memory = records.values.memory.clone();
    let mut selected = None;
    let mut fee = memory.reserve(size_of::<Decoded>() + 128, MemoryClass::Ordinary)?;
    let row = row_from_records_with_semantic(key, root, records, work, |parsed, semantic| {
        let n = semantic_bytes(semantic)?
            .checked_mul(16)
            .and_then(|n| n.checked_add(1024))
            .ok_or_else(limit)?;
        p.spend(n)?;
        let mut f = memory.reserve(n, MemoryClass::Ordinary)?;
        fee.merge(&mut f).map_err(ValueTreeError::Invalid)?;
        selected = Some(copy_key(parsed.kind, semantic)?);
        Ok(())
    })?;
    let quantity = if matches!(row, Row::Entry(_) | Row::Member(_)) {
        let h = read_member_record_header_v1(
            root,
            key,
            records.values,
            MemberRecordLimitsV1 {
                semantic_bytes: records.semantic_bytes,
                payload_bytes: 8,
                record_bytes: records.record_bytes,
            },
            &mut *records.cancelled,
        )?;
        Some(records.quantity(root, key, h.semantic_bytes)?)
    } else {
        None
    };
    Ok(Decoded {
        row,
        key: selected,
        quantity,
        fee,
    })
}
struct Leaf {
    key: Key,
    path: Vec<String>,
    semantic: ReservedValue<Vec<u8>>,
    key_json: ReservedValue<Vec<u8>>,
    value_json: Option<ReservedValue<Vec<u8>>>,
    _fee: MemoryReservation,
}
struct Group {
    ledger: MaterialLedgerId,
    semantic: ReservedValue<Vec<u8>>,
    key_json: ReservedValue<Vec<u8>>,
    before: bool,
    after: bool,
    _fee: MemoryReservation,
}
pub struct CurrentInventoryProjectionDeltaV1 {
    leaves: Vec<Leaf>,
    groups: Vec<Group>,
    guard: PreparedGuard,
    parent: [u8; 32],
    memory: MemoryBudget,
    _index: InventoryIndexV1,
    _counts: [u64; 4],
    _slots: MemoryReservation,
    _fee: MemoryReservation,
}
impl CurrentInventoryProjectionDeltaV1 {
    pub fn current_record_counts(&self) -> [u64; 4] {
        self._counts
    }
    pub fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        live: &ExecutionFence,
    ) -> Result<(), WorldFieldOverlayErrorV1> {
        tx.validate_read_source_guard(&self.guard, live)?;
        if !tx.uses_budget(memory)
            || !self.memory.same_pool(memory)
            || !self._fee.uses_budget(memory)
            || !self._index.uses_budget(memory)
            || self._index.class() != MemoryClass::Ordinary
            || tx.parent_fence().parent_roots != self.parent
        {
            return Err(bad().into());
        }
        Ok(())
    }
    pub fn visit_with_work_v1<E>(
        &self,
        work: &mut dyn FnMut(usize) -> Result<(), E>,
        mut emit: impl FnMut(&[String], &[u8], &[u8], Option<&[u8]>) -> Result<(), E>,
    ) -> Result<(), E> {
        for row in &self.leaves {
            work(0)?;
            emit(
                &row.path,
                &row.semantic,
                &row.key_json,
                row.value_json.as_deref().map(Vec::as_slice),
            )?
        }
        Ok(())
    }
    /// These booleans are observations from real group presence records and the
    /// complete parent index plus every actual changed member, never a permission.
    pub fn visit_groups_with_work_v1<E>(
        &self,
        work: &mut dyn FnMut(usize) -> Result<(), E>,
        mut emit: impl FnMut(&MaterialLedgerId, &[u8], &[u8], bool, bool) -> Result<(), E>,
    ) -> Result<(), E> {
        for group in &self.groups {
            work(0)?;
            emit(
                &group.ledger,
                &group.semantic,
                &group.key_json,
                group.before,
                group.after,
            )?
        }
        Ok(())
    }
}
pub fn is_inventory_projection_field_v1(field: &str) -> bool {
    matches!(field, "resources" | "materials" | "material_ledgers")
}
fn with_index<T>(
    work: &mut InventoryFamilyWorkV1,
    p: &Processing<'_>,
    run: impl FnOnce(&mut impl_index::Meter<'_, '_>) -> Result<T, ValueTreeError>,
) -> Result<T, ValueTreeError> {
    struct Loan<'a> {
        slot: &'a mut u64,
        active: InventoryIndexWorkV1,
    }
    impl Drop for Loan<'_> {
        fn drop(&mut self) {
            *self.slot = self.active.steps_remaining
        }
    }
    let active = InventoryIndexWorkV1 {
        steps_remaining: std::mem::take(&mut work.index_steps_remaining),
    };
    let mut loan = Loan {
        slot: &mut work.index_steps_remaining,
        active,
    };
    let mut meter = impl_index::Meter {
        active: &mut loan.active,
        p,
    };
    run(&mut meter)
}
mod impl_index {
    use super::*;
    pub(super) struct Meter<'a, 'b> {
        pub(super) active: &'a mut InventoryIndexWorkV1,
        pub(super) p: &'a Processing<'b>,
    }
    impl index::InventoryIndexMeterV1 for Meter<'_, '_> {
        fn step(&mut self, c: &mut impl FnMut() -> bool) -> Result<(), ValueTreeError> {
            self.p.spend(index::processing_step_bytes_v1())?;
            index::InventoryIndexMeterV1::step(self.active, c)
        }
    }
}

struct Pending {
    key: Vec<u8>,
    before: Option<[u8; 32]>,
    after: Option<[u8; 32]>,
}
#[allow(clippy::too_many_arguments)]
pub fn read_current_inventory_projection_delta_v1<M, R, E, C>(
    roots: &RootSetV1,
    parent: &ReservedValue<InventoryFamilyQualificationV1>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    work: &mut InventoryFamilyWorkV1,
    processing: &mut BusinessDiffProcessingWorkV1,
    pending_entries: &mut usize,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<CurrentInventoryProjectionDeltaV1, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let stop = RefCell::new(&mut ctx.cancelled);
    let check = || (stop.borrow_mut())();
    let p = Processing {
        work: RefCell::new(processing),
        cancel: &check,
    };
    let source = PaidSource {
        source: ctx.values.source,
        p: &p,
    };
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        ctx.transaction.validate_read_source_guard(guard, live)?;
        let memory = ctx.values.memory.clone();
        if ctx.values.class != MemoryClass::Ordinary
            || !ctx.transaction.uses_budget(&memory)
            || !parent.uses_budget(&memory)
        {
            return Err(bad().into());
        }
        parent.check(&memory, MemoryClass::Ordinary)?;
        let n = roots
            .state
            .iter()
            .try_fold(roots.world_id.len() + 256, |n, r| {
                n.checked_add(r.node_root.len() + 128).ok_or_else(limit)
            })?;
        p.spend(n.checked_mul(16).ok_or_else(limit)?)?;
        parent.validate_binding(roots)?;
        let digest = roots.digest().map_err(|_| bad())?;
        if digest != ctx.transaction.parent_fence().parent_roots {
            return Err(bad().into());
        }
        let fee = memory
            .reserve(
                size_of::<CurrentInventoryProjectionDeltaV1>(),
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        let (mut n, mut key_bytes) = (0usize, 0usize);
        ctx.transaction.visit_pending_writes(
            StateNamespaceV1::Business,
            pending_entries,
            || {
                p.spend(0)?;
                Ok::<_, WorldFieldOverlayErrorV1>(())
            },
            |raw, before, after| {
                p.spend(
                    raw.len()
                        .checked_mul(16)
                        .and_then(|n| n.checked_add(256))
                        .ok_or_else(limit)?,
                )?;
                if before != after && key(raw)?.is_some() {
                    n = n.checked_add(1).ok_or_else(limit)?;
                    key_bytes = key_bytes.checked_add(raw.len()).ok_or_else(limit)?
                }
                Ok::<_, WorldFieldOverlayErrorV1>(())
            },
        )?;
        let pending_fee = memory
            .reserve(
                n.checked_mul(size_of::<Pending>() + 32)
                    .and_then(|n| n.checked_add(key_bytes))
                    .ok_or_else(limit)?,
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        let mut pending = Vec::with_capacity(n);
        let mut touched = Vec::with_capacity(n);
        if pending.capacity() != n || touched.capacity() != n {
            return Err(limit().into());
        }
        ctx.transaction.visit_pending_writes(
            StateNamespaceV1::Business,
            pending_entries,
            || {
                p.spend(0)?;
                Ok::<_, WorldFieldOverlayErrorV1>(())
            },
            |raw, before, after| {
                p.spend(
                    raw.len()
                        .checked_mul(16)
                        .and_then(|n| n.checked_add(256))
                        .ok_or_else(limit)?,
                )?;
                if before != after && key(raw)?.is_some() {
                    if pending.len() == n {
                        return Err(bad().into());
                    }
                    pending.push(Pending {
                        key: raw.to_vec(),
                        before,
                        after,
                    })
                }
                Ok::<_, WorldFieldOverlayErrorV1>(())
            },
        )?;
        if pending.len() != n {
            return Err(bad().into());
        }
        let slots = memory
            .reserve(
                n.checked_mul(size_of::<Leaf>() + size_of::<Group>())
                    .ok_or_else(limit)?,
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        let mut leaves = Vec::with_capacity(n);
        let mut group_rows = Vec::with_capacity(n);
        if leaves.capacity() != n || group_rows.capacity() != n {
            return Err(limit().into());
        }
        let mut groups = parent.groups.clone();
        let mut counts = parent.counts;
        for pending in pending {
            p.spend(
                pending
                    .key
                    .len()
                    .checked_mul(16)
                    .and_then(|n| n.checked_add(256))
                    .ok_or_else(limit)?,
            )?;
            let key = key(&pending.key)?.ok_or_else(bad)?;
            if ctx
                .transaction
                .read(StateNamespaceV1::Business, key.as_bytes())?
                != pending.after
            {
                return Err(bad().into());
            }
            let mut values = ValueReadContext {
                source: &source,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut *ctx.values.io,
            };
            let mut cancelled = || check();
            let mut records = InventoryValueReader {
                values: &mut values,
                key_work: &mut *ctx.key_work,
                semantic_bytes: ctx.semantic_bytes,
                record_bytes: ctx.record_bytes,
                cancelled: &mut cancelled,
            };
            let before = pending
                .before
                .map(|root| decode(&key, &root, &mut records, work, &p))
                .transpose()?;
            let after = pending
                .after
                .map(|root| decode(&key, &root, &mut records, work, &p))
                .transpose()?;
            if let (Some(before), Some(after)) = (&before, &after) {
                p.spend(
                    before.key.as_ref().map_or(0, Key::bytes)
                        + after.key.as_ref().map_or(0, Key::bytes),
                )?;
                if before.key != after.key {
                    return Err(bad().into());
                }
            }
            let mut touched_group = None;
            for (decoded, add) in [(before.as_ref(), false), (after.as_ref(), true)] {
                let Some(decoded) = decoded else { continue };
                if matches!(decoded.row, Row::Meta(_)) {
                    if pending.after.is_none() {
                        return Err(bad().into());
                    }
                    continue;
                }
                let changed = with_index(work, &p, |iw| {
                    adjust(
                        &mut groups,
                        decoded.row,
                        add,
                        &mut counts,
                        iw,
                        &mut cancelled,
                    )
                })?;
                if changed.is_some() {
                    touched_group = changed
                }
            }
            if let Some(group) = touched_group {
                if touched.len() == n {
                    return Err(bad().into());
                }
                touched.push(group)
            }
            let before_quantity = before.as_ref().and_then(|v| v.quantity);
            let after_quantity = after.as_ref().and_then(|v| v.quantity);
            let before_present = before.is_some();
            let after_present = after.is_some();
            let selected = after.or(before).ok_or_else(bad)?;
            let Some(selected_key) = selected.key else {
                continue;
            };
            match selected_key {
                Key::Group(ledger) => {
                    if before_present == after_present {
                        continue;
                    }
                    let semantic = encode(&memory, &p, |mut out| {
                        write_semantic_key_v1(&ledger, &mut out).map_err(|_| bad())
                    })?;
                    let key_json = json(&ledger, &memory, &p)?;
                    group_rows.push(Group {
                        ledger,
                        semantic,
                        key_json,
                        before: before_present,
                        after: after_present,
                        _fee: selected.fee,
                    });
                }
                selected_key => {
                    if before_quantity == after_quantity {
                        continue;
                    }
                    let value_json = after_quantity.map(|v| json(&v, &memory, &p)).transpose()?;
                    let path_bytes = selected_key
                        .bytes()
                        .checked_mul(4)
                        .and_then(|n| n.checked_add(2 * size_of::<String>() + 64))
                        .ok_or_else(limit)?;
                    p.spend(path_bytes)?;
                    let mut path_fee = memory
                        .reserve(path_bytes, MemoryClass::Ordinary)
                        .map_err(ValueTreeError::Memory)?;
                    let mut row_fee = selected.fee;
                    row_fee
                        .merge(&mut path_fee)
                        .map_err(ValueTreeError::Invalid)?;
                    let mut path = Vec::with_capacity(2);
                    if path.capacity() != 2 {
                        return Err(limit().into());
                    }
                    path.push(selected_key.field().to_owned());
                    let (semantic, key_json) = match &selected_key {
                        Key::Resource(r) => (
                            encode(&memory, &p, |mut out| {
                                write_semantic_key_v1(r, &mut out).map_err(|_| bad())
                            })?,
                            json(r, &memory, &p)?,
                        ),
                        Key::Material(s) => (
                            encode(&memory, &p, |mut out| {
                                write_semantic_key_v1(s, &mut out).map_err(|_| bad())
                            })?,
                            json(s, &memory, &p)?,
                        ),
                        Key::Member(l, s) => (
                            encode(&memory, &p, |mut out| {
                                write_semantic_key_v1(&(l, s), &mut out).map_err(|_| bad())
                            })?,
                            json(&(l, s), &memory, &p)?,
                        ),
                        Key::Group(_) => return Err(bad().into()),
                    };
                    leaves.push(Leaf {
                        key: selected_key,
                        path,
                        semantic,
                        key_json,
                        value_json,
                        _fee: row_fee,
                    });
                }
            }
        }
        for group in touched {
            let meta = with_index(work, &p, |iw| groups.get(group, iw, &mut || check()))?;
            if let Some(meta) = meta
                && !meta.present
            {
                if meta.members != 0 {
                    return Err(bad().into());
                }
                groups = with_index(work, &p, |iw| groups.remove(group, iw, &mut || check()))?;
            }
        }
        if u64::try_from(groups.len()).ok() != Some(counts[2]) {
            return Err(bad().into());
        }
        // Fixed insertion sort can stop at the first actual work/cancel failure;
        // it never leaves an unmetered comparator running after a quota refusal.
        for i in 1..leaves.len() {
            let mut j = i;
            while j != 0 {
                p.spend(leaves[j - 1].key.bytes() + leaves[j].key.bytes() + 16)?;
                if leaves[j - 1].key <= leaves[j].key {
                    break;
                }
                p.spend(2 * size_of::<Leaf>())?;
                leaves.swap(j - 1, j);
                j -= 1
            }
        }
        for i in 1..group_rows.len() {
            let mut j = i;
            while j != 0 {
                p.spend(
                    ledger_bytes(&group_rows[j - 1].ledger)
                        + ledger_bytes(&group_rows[j].ledger)
                        + 16,
                )?;
                if group_rows[j - 1].ledger <= group_rows[j].ledger {
                    break;
                }
                p.spend(2 * size_of::<Group>())?;
                group_rows.swap(j - 1, j);
                j -= 1
            }
        }
        ctx.transaction.validate_read_source_guard(guard, live)?;
        drop(pending_fee);
        Ok(CurrentInventoryProjectionDeltaV1 {
            leaves,
            groups: group_rows,
            guard: ctx.transaction.guard(),
            parent: digest,
            memory,
            _index: groups,
            _counts: counts,
            _slots: slots,
            _fee: fee,
        })
    }));
    match result {
        Ok(result) => {
            let result = if let Some(error) = (ctx.take_state_error)() {
                Err(error.into())
            } else {
                result
            };
            if result.is_err() {
                let _: Result<(), ExecutionError> =
                    ctx.transaction.contribute(|_| Err(ExecutionError::Aborted));
            }
            result
        }
        Err(panic) => {
            let _: Result<(), ExecutionError> =
                ctx.transaction.contribute(|_| Err(ExecutionError::Aborted));
            std::panic::resume_unwind(panic)
        }
    }
}

#[path = "incremental_inventory_projection_delta_cold.rs"]
mod cold;
pub use cold::{
    QualifiedInventoryProjectionColumnsV1, read_qualified_inventory_projection_columns_v1,
};
