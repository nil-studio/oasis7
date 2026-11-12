//! Derived exact namespace membership, preserving all original authorization
//! histories without hydrating them on each owner lookup.
use super::*;
use oasis7_agent_api::game::GameIdentityNamespaceV1;
pub(super) struct ParticipantNamespaceIndexV1 {
    roots: [Option<StateRootV1>; 3],
    entries: Vec<(u8, u64, ReservedValue<GameIdentityNamespaceV1>)>,
}
fn ns_copy(
    value: &GameIdentityNamespaceV1,
    memory: &MemoryBudget,
    class: MemoryClass,
) -> Result<ReservedValue<GameIdentityNamespaceV1>, ValueTreeError> {
    let n = value
        .service_id
        .len()
        .checked_add(value.environment_id.len())
        .and_then(|n| n.checked_add(std::mem::size_of::<GameIdentityNamespaceV1>()))
        .ok_or_else(invalid)?;
    let fee = memory.reserve(n, class)?;
    let v = value.clone();
    if v.service_id.capacity() > value.service_id.len()
        || v.environment_id.capacity() > value.environment_id.len()
    {
        return Err(invalid());
    }
    Ok(fee.retain(v))
}
impl ParticipantNamespaceIndexV1 {
    pub(super) fn readmit(
        &self,
        memory: &MemoryBudget,
        class: MemoryClass,
        mut stop: impl FnMut() -> bool,
    ) -> Result<ReservedValue<Self>, ValueTreeError> {
        let fee = memory.reserve(
            self.entries
                .len()
                .checked_mul(std::mem::size_of::<(
                    u8,
                    u64,
                    ReservedValue<GameIdentityNamespaceV1>,
                )>())
                .and_then(|n| n.checked_add(2048))
                .ok_or_else(invalid)?,
            class,
        )?;
        let mut entries = Vec::with_capacity(self.entries.len());
        if entries.capacity() > self.entries.len() {
            return Err(invalid());
        }
        for (lane, ordinal, ns) in &self.entries {
            if stop() {
                return Err(ValueTreeError::Cancelled);
            }
            entries.push((*lane, *ordinal, ns_copy(ns, memory, class)?));
        }
        if stop() {
            return Err(ValueTreeError::Cancelled);
        }
        Ok(fee.retain(Self {
            roots: self.roots.clone(),
            entries,
        }))
    }
    pub(super) fn contains(
        &self,
        value: &GameIdentityNamespaceV1,
        work: &mut RevocationReadWorkV1,
        mut stop: impl FnMut() -> bool,
    ) -> Result<bool, ValueTreeError> {
        for (_, _, ns) in &self.entries {
            if stop() {
                return Err(ValueTreeError::Cancelled);
            }
            spend(
                work,
                value
                    .service_id
                    .len()
                    .checked_add(value.environment_id.len())
                    .and_then(|n| n.checked_add(ns.service_id.len()))
                    .and_then(|n| n.checked_add(ns.environment_id.len()))
                    .ok_or_else(invalid)?,
            )?;
            if &**ns == value {
                return Ok(true);
            }
        }
        Ok(false)
    }
}
fn child<M, R, E, C>(
    root: &StateRootV1,
    key: &str,
    d: &mut ChangeDecoder<'_, '_, M, R, E, C>,
) -> Result<StateRootV1, ValueTreeError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let row = diff::point_typed_column_v1(&profile(), root, key, d, 8192)?;
    let ColumnCellV1::Root(root) = &row.cell else {
        return Err(invalid());
    };
    Ok(root.clone())
}
fn namespace<M, R, E, C>(
    root: &StateRootV1,
    lane: u8,
    d: &mut ChangeDecoder<'_, '_, M, R, E, C>,
) -> Result<ReservedValue<GameIdentityNamespaceV1>, ValueTreeError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let _phase = d.ctx.values.memory.reserve(2048, d.ctx.values.class)?;
    let mut root = root.clone();
    if lane == 2 {
        root = child(&root, "request", d)?;
    }
    if lane != 0 {
        root = child(&root, "policy", d)?;
    }
    root = child(&root, "identity_namespace", d)?;
    let value = crate::incremental_typed_record_codec::recover_typed_record_body_v1::<
        GameIdentityNamespaceV1,
        _,
        _,
        _,
        _,
    >(&root, &profile(), d.ctx, d.scan, d.work)?;
    ns_copy(value.value(), d.ctx.values.memory, d.ctx.values.class)
}
pub(super) fn advance_namespaces<M, R, E, C>(
    parent: Option<&ReservedValue<ParticipantNamespaceIndexV1>>,
    participants: &StateRootV1,
    d: &mut ChangeDecoder<'_, '_, M, R, E, C>,
) -> Result<ReservedValue<ParticipantNamespaceIndexV1>, ValueTreeError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let mut roots = std::array::from_fn(|_| None);
    let _phase = d.ctx.values.memory.reserve(2048, d.ctx.values.class)?;
    for (i, key) in ["policy", "policy_history", "devlocal_policy_history"]
        .into_iter()
        .enumerate()
    {
        let row = diff::point_typed_column_v1(&profile(), participants, key, d, 8192)?;
        roots[i] = match &row.cell {
            ColumnCellV1::Unit if i == 0 => None,
            ColumnCellV1::Root(root) => Some(root.clone()),
            _ => return Err(invalid()),
        };
    }
    let count = roots
        .iter()
        .flatten()
        .try_fold(1usize, |n, r| {
            n.checked_add(usize::try_from(r.leaf_count).ok()?)
        })
        .ok_or_else(invalid)?;
    let cap = count
        .checked_add(parent.map_or(0, |p| p.entries.len()))
        .ok_or_else(invalid)?;
    if cap > d.work.max_tree_heap / std::mem::size_of::<GameIdentityNamespaceV1>() {
        return Err(ValueTreeError::ResourceLimited("participant namespaces"));
    }
    let memory = d.ctx.values.memory;
    let class = d.ctx.values.class;
    let fee = memory.reserve(
        cap.checked_mul(std::mem::size_of::<(
            u8,
            u64,
            ReservedValue<GameIdentityNamespaceV1>,
        )>())
        .and_then(|n| n.checked_add(2048))
        .ok_or_else(invalid)?,
        class,
    )?;
    let mut entries = Vec::with_capacity(cap);
    if entries.capacity() > cap {
        return Err(invalid());
    }
    if let Some(parent) = parent {
        if !parent.uses_budget(memory) {
            return Err(invalid());
        }
        for (lane, id, ns) in &parent.entries {
            d.check()?;
            spend(
                d.work,
                ns.service_id
                    .len()
                    .checked_add(ns.environment_id.len())
                    .ok_or_else(invalid)?,
            )?;
            entries.push((*lane, *id, ns_copy(ns, memory, class)?));
        }
    }
    for lane in 0..3 {
        if parent.is_some_and(|p| p.roots[lane] == roots[lane]) {
            continue;
        }
        if lane == 0 {
            entries.retain(|(l, _, _)| *l != 0);
            if let Some(root) = &roots[0] {
                entries.push((0, 0, namespace(root, 0, d)?));
            }
            continue;
        }
        let empty = StateRootV1::empty(StateNamespaceV1::Authorization).map_err(|_| invalid())?;
        let before = parent
            .and_then(|p| p.roots[lane].as_ref())
            .unwrap_or(&empty);
        let after = roots[lane].as_ref().ok_or_else(invalid)?;
        diff::visit_entries(before, after, d, |old, new, d| {
            let cell = new.or(old).ok_or_else(invalid)?;
            let diff::TypedCellRefV1::Entry(e) = cell else {
                return Err(invalid());
            };
            let parsed = WorldFieldKeyV1::from_bytes(&e.key).map_err(ValueTreeError::Invalid)?;
            if parsed.kind == WorldFieldRecordKindV1::Metadata {
                return Ok(());
            }
            let key = diff::key(&profile(), cell, d)?;
            let ColumnKeyV1::Pair(prefix, ordinal) = &*key else {
                return Ok(());
            };
            if **prefix != ColumnKeyV1::text("entry") {
                return Err(invalid());
            }
            let ColumnKeyV1::Ordinal(id) = &**ordinal else {
                return Err(invalid());
            };
            for _ in &entries {
                d.check()?;
                spend(d.work, 16)?;
            }
            entries.retain(|(l, i, _)| *l != lane as u8 || *i != *id);
            if let Some(new) = new {
                if entries.len() >= cap {
                    return Err(invalid());
                }
                let root = diff::root(&profile(), new, d)?;
                entries.push((lane as u8, *id, namespace(&root, lane as u8, d)?));
            }
            Ok(())
        })?;
    }
    d.check()?;
    Ok(fee.retain(ParticipantNamespaceIndexV1 { roots, entries }))
}
