//! Derived lookup from authenticated safe-alternative members. GEN/cold walks
//! the map once; successors decode only changed members, never the old DTO.
use super::*;
use crate::incremental_typed_record_diff::{self as diff, ChangeDecoder};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryReservation, ReservedValue};

pub(super) struct SafeAlternativeActionIndexV1 {
    map: StateRootV1,
    entries: Vec<(u64, String)>,
    _keys: Vec<MemoryReservation>,
}
impl SafeAlternativeActionIndexV1 {
    pub(super) fn lineage(&self, action: u64) -> Option<&str> {
        self.entries
            .binary_search_by_key(&action, |v| v.0)
            .ok()
            .map(|i| self.entries[i].1.as_str())
    }
    pub(super) fn readmit(
        &self,
        memory: &MemoryBudget,
        class: MemoryClass,
        mut cancelled: impl FnMut() -> bool,
    ) -> Result<ReservedValue<Self>, ValueTreeError> {
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
        let (fee, mut entries, mut keys) = storage(self.entries.len(), memory, class)?;
        for (id, key) in &self.entries {
            if cancelled() {
                return Err(ValueTreeError::Cancelled);
            }
            keys.push(key_fee(key, memory, class)?);
            entries.push((*id, key.clone()));
        }
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
        Ok(fee.retain(Self {
            map: self.map.clone(),
            entries,
            _keys: keys,
        }))
    }
}
type IndexStorage = (
    MemoryReservation,
    Vec<(u64, String)>,
    Vec<MemoryReservation>,
);
fn storage(
    n: usize,
    memory: &MemoryBudget,
    class: MemoryClass,
) -> Result<IndexStorage, ValueTreeError> {
    let bytes = n
        .checked_mul(
            std::mem::size_of::<(u64, String)>() + std::mem::size_of::<MemoryReservation>(),
        )
        .and_then(|v| v.checked_add(4096))
        .ok_or_else(invalid)?;
    let fee = memory.reserve(bytes, class)?;
    let mut entries = Vec::new();
    let mut keys = Vec::new();
    entries
        .try_reserve_exact(n)
        .map_err(|_| ValueTreeError::ResourceLimited("safe alternative index slots"))?;
    keys.try_reserve_exact(n)
        .map_err(|_| ValueTreeError::ResourceLimited("safe alternative index charges"))?;
    if entries.capacity() > n || keys.capacity() > n {
        return Err(invalid());
    }
    Ok((fee, entries, keys))
}
fn key_fee(
    key: &str,
    memory: &MemoryBudget,
    class: MemoryClass,
) -> Result<MemoryReservation, ValueTreeError> {
    memory
        .reserve(
            key.len()
                .checked_mul(2)
                .and_then(|n| n.checked_add(128))
                .ok_or_else(invalid)?,
            class,
        )
        .map_err(ValueTreeError::Memory)
}

pub(super) fn advance<M, R, E, C>(
    parent: Option<&ReservedValue<SafeAlternativeActionIndexV1>>,
    body: &StateRootV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut RevocationReadWorkV1,
) -> Result<ReservedValue<SafeAlternativeActionIndexV1>, ValueTreeError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    ctx.check()?;
    if let Some(parent) = parent {
        if !parent.uses_budget(ctx.values.memory) {
            return Err(invalid());
        }
    }
    let profile = profile();
    let mut d = ChangeDecoder {
        trees_remaining: work.max_retained_trees,
        tree_depth: 0,
        profile,
        ctx,
        scan,
        work,
    };
    let row = diff::point_typed_column_v1(&profile, body, "safe_alternatives", &mut d, 8192)?;
    let ColumnCellV1::Root(map) = &row.cell else {
        return Err(invalid());
    };
    let length = diff::point_typed_column_v1(&profile, map, "_length", &mut d, 8192)?;
    let ColumnCellV1::U64(count) = length.cell else {
        return Err(invalid());
    };
    let count = usize::try_from(count).map_err(|_| invalid())?;
    if count.checked_add(2) != usize::try_from(map.leaf_count).ok() {
        return Err(invalid());
    }
    // The profile's retained heap bound also bounds index slots before allocation.
    if count > d.work.max_tree_heap / 128 {
        return Err(ValueTreeError::ResourceLimited(
            "safe alternative index entries",
        ));
    }
    let capacity = count
        .checked_add(parent.map_or(0, |p| p.entries.len()))
        .ok_or_else(invalid)?;
    let memory = d.ctx.values.memory;
    let class = d.ctx.values.class;
    let (fee, mut entries, mut keys) = storage(capacity, memory, class)?;
    if let Some(parent) = parent {
        for (id, key) in &parent.entries {
            d.check()?;
            keys.push(key_fee(key, memory, class)?);
            entries.push((*id, key.clone()));
        }
    }
    let empty = StateRootV1::empty(map.namespace).map_err(|_| invalid())?;
    let before = parent.map_or(&empty, |p| &p.map);
    diff::visit_entries(before, map, &mut d, |old, new, d| {
        let cell = new.or(old).ok_or_else(invalid)?;
        // Empty -> qualified map includes its metadata leaf. It is presence,
        // not a typed column key. The enclosing owner validates this leaf.
        let diff::TypedCellRefV1::Entry(entry) = cell else {
            return Err(invalid());
        };
        let parsed = WorldFieldKeyV1::from_bytes(&entry.key).map_err(ValueTreeError::Invalid)?;
        if parsed.kind == WorldFieldRecordKindV1::Metadata {
            return Ok(());
        }
        let key = diff::key(&profile, cell, d)?;
        let ColumnKeyV1::Pair(prefix, lineage) = &*key else {
            return Ok(());
        };
        if **prefix != ColumnKeyV1::text("entry") {
            return Err(invalid());
        }
        let ColumnKeyV1::Text(lineage) = &**lineage else {
            return Err(invalid());
        };
        entries.retain(|(_, key)| key != lineage);
        if let Some(new) = new {
            let root = diff::root(&profile, new, d)?;
            let value = crate::incremental_typed_record_codec::recover_typed_record_body_v1::<
                serde_json::Value,
                _,
                _,
                _,
                _,
            >(&root, &profile, d.ctx, d.scan, d.work)?;
            if let Some(id) = value.value()["authorization"]["action_id"].as_u64() {
                if entries.len() == capacity || keys.len() == capacity {
                    return Err(invalid());
                }
                keys.push(key_fee(lineage, memory, class)?);
                entries.push((id, lineage.clone()));
            }
        }
        Ok(())
    })?;
    entries.sort_unstable_by_key(|v| v.0);
    if entries.windows(2).any(|v| v[0].0 == v[1].0) {
        return Err(ValueTreeError::Invalid(
            "duplicate safe alternative action id",
        ));
    }
    d.check()?;
    Ok(fee.retain(SafeAlternativeActionIndexV1 {
        map: map.clone(),
        entries,
        _keys: keys,
    }))
}
