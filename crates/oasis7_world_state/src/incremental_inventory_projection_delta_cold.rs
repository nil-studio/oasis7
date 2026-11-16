//! Once-only qualified Inventory columns, distinct from a Current delta.
use super::*;
use oasis7_formal_protocol::incremental_state::{
    StateScanBudget, StateScanContext, StateScanError,
};
struct AllGroup {
    entries: usize,
    ledger: MaterialLedgerId,
    semantic: ReservedValue<Vec<u8>>,
    key_json: ReservedValue<Vec<u8>>,
    _fee: MemoryReservation,
}
pub struct QualifiedInventoryProjectionColumnsV1 {
    counts: [u64; 4],
    leaves: Vec<Leaf>,
    groups: Vec<AllGroup>,
    roots: [u8; 32],
    memory: MemoryBudget,
    _slots: MemoryReservation,
    _fee: MemoryReservation,
}
impl QualifiedInventoryProjectionColumnsV1 {
    pub fn record_counts(&self) -> [u64; 4] {
        self.counts
    }
    pub fn visit_group_counts_with_work_v1<E>(
        &self,
        work: &mut dyn FnMut(usize) -> Result<(), E>,
        mut emit: impl FnMut(&MaterialLedgerId, usize) -> Result<(), E>,
    ) -> Result<(), E> {
        for group in &self.groups {
            work(0)?;
            emit(&group.ledger, group.entries)?
        }
        Ok(())
    }
    pub fn validate_context(
        &self,
        roots: &RootSetV1,
        memory: &MemoryBudget,
    ) -> Result<(), ValueTreeError> {
        if self.roots != roots.digest().map_err(|_| bad())?
            || !self.memory.same_pool(memory)
            || !self._fee.uses_budget(memory)
            || !self._slots.uses_budget(memory)
        {
            return Err(bad());
        }
        Ok(())
    }
    pub fn visit_with_work_v1<E>(
        &self,
        work: &mut dyn FnMut(usize) -> Result<(), E>,
        mut emit: impl FnMut(&[String], &[u8], &[u8], &[u8]) -> Result<(), E>,
    ) -> Result<(), E> {
        for row in &self.leaves {
            work(0)?;
            emit(
                &row.path,
                &row.semantic,
                &row.key_json,
                row.value_json
                    .as_ref()
                    .expect("qualified Inventory quantity"),
            )?
        }
        Ok(())
    }
    pub fn visit_groups_with_work_v1<E>(
        &self,
        work: &mut dyn FnMut(usize) -> Result<(), E>,
        mut emit: impl FnMut(&MaterialLedgerId, &[u8], &[u8]) -> Result<(), E>,
    ) -> Result<(), E> {
        for group in &self.groups {
            work(0)?;
            emit(&group.ledger, &group.semantic, &group.key_json)?
        }
        Ok(())
    }
}
struct StateSource<'a, 'b, M> {
    source: &'a M,
    p: &'a Processing<'b>,
    first: RefCell<Option<ValueTreeError>>,
}
impl<M: MerkleNodeReader> MerkleNodeReader for StateSource<'_, '_, M> {
    type Node = M::Node;
    fn allocation_class(&self) -> Option<MemoryClass> {
        self.source.allocation_class()
    }
    fn allocation_budget(&self) -> Option<&MemoryBudget> {
        self.source.allocation_budget()
    }
    fn read_node(&self, id: &str, max: usize) -> Result<Option<Self::Node>, String> {
        if self.first.borrow().is_some() {
            return Err("cold Inventory processing".into());
        }
        if let Err(e) = self.p.spend(0) {
            *self.first.borrow_mut() = Some(e);
            return Err("cold Inventory processing".into());
        }
        let result = self.source.read_node(id, max);
        if let Ok(Some(raw)) = &result {
            let paid = raw
                .as_ref()
                .len()
                .checked_mul(16)
                .and_then(|n| n.checked_add(id.len() + 256))
                .ok_or_else(limit)
                .and_then(|n| self.p.spend(n));
            if let Err(e) = paid {
                *self.first.borrow_mut() = Some(e);
                return Err("cold Inventory processing".into());
            }
        }
        result
    }
}
#[allow(clippy::too_many_arguments)]
pub fn read_qualified_inventory_projection_columns_v1<M, R, E, C>(
    roots: &RootSetV1,
    parent: &ReservedValue<InventoryFamilyQualificationV1>,
    read: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut InventoryFamilyWorkV1,
    processing: &mut BusinessDiffProcessingWorkV1,
) -> Result<QualifiedInventoryProjectionColumnsV1, ValueTreeError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    read.check()?;
    let memory = read.values.memory.clone();
    if read.values.class != MemoryClass::Ordinary || !parent.uses_budget(&memory) {
        return Err(bad());
    }
    parent.check(&memory, MemoryClass::Ordinary)?;
    parent.validate_binding(roots)?;
    let digest = roots.digest().map_err(|_| bad())?;
    let stop = RefCell::new(&mut read.cancelled);
    let check = || (stop.borrow_mut())();
    let p = Processing {
        work: RefCell::new(processing),
        cancel: &check,
    };
    let source = StateSource {
        source: read.state,
        p: &p,
        first: RefCell::new(None),
    };
    let values_source = PaidSource {
        source: read.values.source,
        p: &p,
    };
    let counts = parent.record_counts();
    let capacity = usize::try_from(
        counts[0]
            .checked_add(counts[1])
            .and_then(|n| n.checked_add(counts[3]))
            .ok_or_else(limit)?,
    )
    .map_err(|_| limit())?;
    let group_capacity = usize::try_from(counts[2]).map_err(|_| limit())?;
    let n = capacity
        .checked_mul(size_of::<Leaf>())
        .and_then(|n| {
            group_capacity
                .checked_mul(size_of::<AllGroup>())
                .and_then(|g| n.checked_add(g))
        })
        .ok_or_else(limit)?;
    p.spend(n)?;
    let slots = memory.reserve(n, MemoryClass::Ordinary)?;
    let fee = memory.reserve(
        size_of::<QualifiedInventoryProjectionColumnsV1>(),
        MemoryClass::Ordinary,
    )?;
    let mut leaves = Vec::with_capacity(capacity);
    let mut groups = Vec::with_capacity(group_capacity);
    if leaves.capacity() != capacity || groups.capacity() != group_capacity {
        return Err(limit());
    }
    let mut observed = [0u64; 4];
    let mut metadata = 0u8;
    let result = StateScanContext {
        source: &source,
        memory: &memory,
        class: MemoryClass::Ordinary,
        io: scan,
    }
    .visit_entries(
        roots.state_root(StateNamespaceV1::Business),
        |entry| {
            p.spend(
                entry
                    .key
                    .len()
                    .checked_mul(16)
                    .and_then(|n| n.checked_add(256))
                    .ok_or_else(limit)?,
            )?;
            let Some(key) = key(&entry.key)? else {
                return Ok(());
            };
            let mut values = ValueReadContext {
                source: &values_source,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut *read.values.io,
            };
            let mut cancelled = || check();
            let mut records = InventoryValueReader {
                values: &mut values,
                key_work: &mut *read.key_work,
                semantic_bytes: read.semantic_bytes,
                record_bytes: read.record_bytes,
                cancelled: &mut cancelled,
            };
            let decoded = decode(&key, &entry.value, &mut records, work, &p)?;
            let mut group_entries = None;
            let slot = match decoded.row {
                Row::Meta(slot) => {
                    if metadata & (1 << slot) != 0 {
                        return Err(bad());
                    }
                    metadata |= 1 << slot;
                    return Ok(());
                }
                Row::Entry(slot) => slot,
                Row::Group(group) => {
                    let actual =
                        with_index(work, &p, |iw| parent.groups.get(group, iw, &mut cancelled))?
                            .ok_or_else(bad)?;
                    if !actual.present {
                        return Err(bad());
                    }
                    group_entries = Some(usize::try_from(actual.members).map_err(|_| limit())?);
                    2
                }
                Row::Member(group) => {
                    let actual =
                        with_index(work, &p, |iw| parent.groups.get(group, iw, &mut cancelled))?
                            .ok_or_else(bad)?;
                    if !actual.present {
                        return Err(bad());
                    }
                    3
                }
            };
            observed[slot] = observed[slot].checked_add(1).ok_or_else(limit)?;
            if observed[slot] > counts[slot] {
                return Err(bad());
            }
            let selected = decoded.key.ok_or_else(bad)?;
            match selected {
                Key::Group(ledger) => {
                    if groups.len() == group_capacity {
                        return Err(bad());
                    }
                    let semantic = encode(&memory, &p, |mut out| {
                        write_semantic_key_v1(&ledger, &mut out).map_err(|_| bad())
                    })?;
                    let key_json = json(&ledger, &memory, &p)?;
                    groups.push(AllGroup {
                        entries: group_entries.ok_or_else(bad)?,
                        ledger,
                        semantic,
                        key_json,
                        _fee: decoded.fee,
                    });
                }
                selected => {
                    if leaves.len() == capacity {
                        return Err(bad());
                    }
                    let quantity = decoded.quantity.ok_or_else(bad)?;
                    let value_json = json(&quantity, &memory, &p)?;
                    let n = selected
                        .bytes()
                        .checked_mul(4)
                        .and_then(|n| n.checked_add(2 * size_of::<String>() + 64))
                        .ok_or_else(limit)?;
                    p.spend(n)?;
                    let mut path_fee = memory.reserve(n, MemoryClass::Ordinary)?;
                    let mut fee = decoded.fee;
                    fee.merge(&mut path_fee).map_err(ValueTreeError::Invalid)?;
                    let path = vec![selected.field().to_owned()];
                    let (semantic, key_json) = match &selected {
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
                        Key::Group(_) => return Err(bad()),
                    };
                    leaves.push(Leaf {
                        key: selected,
                        path,
                        semantic,
                        key_json,
                        value_json: Some(value_json),
                        _fee: fee,
                    });
                }
            }
            Ok(())
        },
        || check(),
    );
    if let Some(error) = source.first.borrow_mut().take() {
        return Err(error);
    }
    if let Some(error) = (read.take_state_error)() {
        return Err(error);
    }
    match result {
        Ok(_) => {}
        Err(StateScanError::Memory(e)) => return Err(ValueTreeError::Memory(e)),
        Err(StateScanError::Visitor(e)) => return Err(e),
        Err(StateScanError::Cancelled) => return Err(ValueTreeError::Cancelled),
        Err(StateScanError::ResourceLimited) => return Err(limit()),
        _ => return Err(bad()),
    }
    if observed != counts || metadata != 7 {
        return Err(bad());
    }
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
    for i in 1..groups.len() {
        let mut j = i;
        while j != 0 {
            p.spend(ledger_bytes(&groups[j - 1].ledger) + ledger_bytes(&groups[j].ledger) + 16)?;
            if groups[j - 1].ledger <= groups[j].ledger {
                break;
            }
            p.spend(2 * size_of::<AllGroup>())?;
            groups.swap(j - 1, j);
            j -= 1
        }
    }
    Ok(QualifiedInventoryProjectionColumnsV1 {
        counts,
        leaves,
        groups,
        roots: digest,
        memory,
        _slots: slots,
        _fee: fee,
    })
}
