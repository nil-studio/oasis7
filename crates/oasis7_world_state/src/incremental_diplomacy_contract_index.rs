//! NEW unhooked complete Contract creator/status metadata directory.
//! Planned private child of diplomacy family::relations. GEN/recovery feeds
//! it in the original qualified outer visitor; normal paths replace only true
//! changed children. No index or count is a contract permission capability.
use super::*;
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation, ReservedValue};
use std::{
    mem::{align_of, size_of},
    sync::Arc,
};
pub(in crate::incremental_diplomacy) type Held = Arc<ReservedValue<ContractRow>>;
pub(in crate::incremental_diplomacy) struct ContractRow {
    key: WorldFieldKeyV1,
    member: [u8; 32],
    outer: Text,
    child: StateRootV1,
    creator: Text,
    status: EconomicContractStatus,
    class: MemoryClass,
}
struct Backing {
    rows: Vec<Held>,
    _vector: MemoryReservation,
    _carrier: MemoryReservation,
}
pub(in crate::incremental_diplomacy) struct ContractCreatorIndexV1 {
    data: Arc<ReservedValue<Backing>>,
    memory: MemoryBudget,
    class: MemoryClass,
}
impl std::fmt::Debug for ContractCreatorIndexV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ContractCreatorIndexV1")
            .field("records", &self.data.rows.len())
            .field("class", &self.class)
            .finish_non_exhaustive()
    }
}
fn bounded() -> ValueTreeError {
    ValueTreeError::ResourceLimited("contract creator directory layout")
}
fn plus(a: usize, b: usize) -> Result<usize, ValueTreeError> {
    a.checked_add(b).ok_or_else(bounded)
}
fn times(a: usize, b: usize) -> Result<usize, ValueTreeError> {
    a.checked_mul(b).ok_or_else(bounded)
}
fn arc_layout<T>() -> Result<usize, ValueTreeError> {
    plus(
        plus(size_of::<ReservedValue<T>>(), 2 * size_of::<usize>())?,
        2 * align_of::<ReservedValue<T>>(),
    )
}
fn same_pool(
    index: &ContractCreatorIndexV1,
    memory: &MemoryBudget,
    class: MemoryClass,
) -> Result<(), ValueTreeError> {
    if !index.memory.same_pool(memory) || index.class != class || !index.data.uses_budget(memory) {
        return Err(closure::invalid());
    }
    Ok(())
}
fn cancel(
    work: &mut DiplomacyIndexWorkV1,
    stop: &mut impl FnMut() -> bool,
) -> Result<(), ValueTreeError> {
    if stop() {
        return Err(ValueTreeError::Cancelled);
    }
    work.step()
}
pub(in crate::incremental_diplomacy) struct ContractCreatorIndexBuilderV1 {
    rows: Vec<Held>,
    vector: MemoryReservation,
    carrier: MemoryReservation,
    memory: MemoryBudget,
    class: MemoryClass,
}
impl ContractCreatorIndexBuilderV1 {
    pub(in crate::incremental_diplomacy) fn new(
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<Self, ValueTreeError> {
        Ok(Self {
            rows: Vec::new(),
            vector: memory.reserve(0, class)?,
            carrier: memory.reserve(size_of::<Self>(), class)?,
            memory: memory.clone(),
            class,
        })
    }
    fn capacity(&mut self, needed: usize) -> Result<(), ValueTreeError> {
        if needed <= self.rows.capacity() {
            return Ok(());
        }
        let cap = self
            .rows
            .capacity()
            .checked_mul(2)
            .ok_or_else(bounded)?
            .max(4)
            .max(needed);
        let fee = self
            .memory
            .reserve(times(cap, size_of::<Held>())?, self.class)?;
        // Old vector admission stays live throughout realloc; the new admission is
        // taken before requesting growth. Unexpected allocator capacity is rejected.
        self.rows
            .try_reserve_exact(cap.checked_sub(self.rows.len()).ok_or_else(bounded)?)
            .map_err(|_| bounded())?;
        if self.rows.capacity() > cap {
            return Err(bounded());
        }
        self.vector = fee;
        Ok(())
    }
    pub(in crate::incremental_diplomacy) fn put(
        &mut self,
        row: Held,
        work: &mut DiplomacyIndexWorkV1,
        mut stop: impl FnMut() -> bool,
    ) -> Result<(), ValueTreeError> {
        if !row.uses_budget(&self.memory) || row.class != self.class {
            return Err(closure::invalid());
        }
        let mut found = None;
        for (i, old) in self.rows.iter().enumerate() {
            cancel(work, &mut stop)?;
            work.compare(old.outer.as_str(), row.outer.as_str())?;
            work.comparison_bytes_remaining = work
                .comparison_bytes_remaining
                .checked_sub(plus(old.key.as_bytes().len(), row.key.as_bytes().len())? as u64)
                .ok_or(ValueTreeError::ResourceLimited(
                    "contract directory key comparison",
                ))?;
            if old.key.as_bytes() == row.key.as_bytes() {
                found = Some(i);
                break;
            }
        }
        if let Some(i) = found {
            self.rows[i] = row;
        } else {
            if self.rows.len() >= work.max_rows {
                return Err(ValueTreeError::ResourceLimited(
                    "contract creator directory rows",
                ));
            }
            self.capacity(plus(self.rows.len(), 1)?)?;
            self.rows.push(row);
        }
        Ok(())
    }
    pub(in crate::incremental_diplomacy) fn remove(
        &mut self,
        key: &WorldFieldKeyV1,
        work: &mut DiplomacyIndexWorkV1,
        mut stop: impl FnMut() -> bool,
    ) -> Result<(), ValueTreeError> {
        for i in 0..self.rows.len() {
            cancel(work, &mut stop)?;
            let bytes = plus(self.rows[i].key.as_bytes().len(), key.as_bytes().len())?;
            work.comparison_bytes_remaining = work
                .comparison_bytes_remaining
                .checked_sub(bytes as u64)
                .ok_or(ValueTreeError::ResourceLimited(
                    "contract directory key comparison",
                ))?;
            if self.rows[i].key.as_bytes() == key.as_bytes() {
                cancel(work, &mut stop)?;
                work.comparison_bytes_remaining = work
                    .comparison_bytes_remaining
                    .checked_sub(times(self.rows.len() - i, size_of::<Held>())? as u64)
                    .ok_or(ValueTreeError::ResourceLimited(
                        "contract directory row shift",
                    ))?;
                self.rows.remove(i);
                break;
            }
        }
        Ok(())
    }
    pub(in crate::incremental_diplomacy) fn finish(
        self,
    ) -> Result<ContractCreatorIndexV1, ValueTreeError> {
        let fee = self.memory.reserve(arc_layout::<Backing>()?, self.class)?;
        let data = Arc::new(fee.retain(Backing {
            rows: self.rows,
            _vector: self.vector,
            _carrier: self.carrier,
        }));
        Ok(ContractCreatorIndexV1 {
            data,
            memory: self.memory,
            class: self.class,
        })
    }
}
impl ContractCreatorIndexV1 {
    pub(in crate::incremental_diplomacy) fn len(&self) -> usize {
        self.data.rows.len()
    }
    pub(in crate::incremental_diplomacy) fn share(
        &self,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<Self, ValueTreeError> {
        same_pool(self, memory, class)?;
        Ok(Self {
            data: self.data.clone(),
            memory: memory.clone(),
            class,
        })
    }
    /// Copies only paid resident Arc metadata. No World page or Contract DTO is
    /// fetched for unchanged rows; each row's original paid backing remains live.
    pub(in crate::incremental_diplomacy) fn copy_for_changes(
        &self,
        memory: &MemoryBudget,
        class: MemoryClass,
        work: &mut DiplomacyIndexWorkV1,
        mut stop: impl FnMut() -> bool,
    ) -> Result<ContractCreatorIndexBuilderV1, ValueTreeError> {
        same_pool(self, memory, class)?;
        let mut out = ContractCreatorIndexBuilderV1::new(memory, class)?;
        out.capacity(self.data.rows.len())?;
        for row in &self.data.rows {
            cancel(work, &mut stop)?;
            work.comparison_bytes_remaining = work
                .comparison_bytes_remaining
                .checked_sub(size_of::<Held>() as u64)
                .ok_or(ValueTreeError::ResourceLimited(
                    "contract resident share bytes",
                ))?;
            out.rows.push(row.clone());
        }
        Ok(out)
    }
    /// Recovery -> Ordinary allocates actual Ordinary row/text owners; a class
    /// flag or MemoryBudget pool match alone cannot reclassify old reservations.
    pub(in crate::incremental_diplomacy) fn readmit_ordinary(
        &self,
        memory: &MemoryBudget,
        work: &mut DiplomacyIndexWorkV1,
        mut stop: impl FnMut() -> bool,
    ) -> Result<Self, ValueTreeError> {
        if self.class == MemoryClass::Ordinary {
            return self.share(memory, MemoryClass::Ordinary);
        }
        same_pool(self, memory, MemoryClass::Recovery)?;
        let mut next = ContractCreatorIndexBuilderV1::new(memory, MemoryClass::Ordinary)?;
        next.capacity(self.data.rows.len())?;
        for old in &self.data.rows {
            cancel(work, &mut stop)?;
            work.comparison_bytes_remaining = work
                .comparison_bytes_remaining
                .checked_sub(plus(old.outer.len(), old.creator.len())? as u64)
                .ok_or_else(bounded)?;
            let outer = hold_text(&old.outer, memory, MemoryClass::Ordinary, work)?;
            let creator = hold_text(&old.creator, memory, MemoryClass::Ordinary, work)?;
            let row = retain_row(
                &old.key,
                old.member,
                &old.child,
                outer,
                creator,
                old.status,
                memory,
                MemoryClass::Ordinary,
                work,
            )?;
            next.rows.push(row);
        }
        next.finish()
    }
    pub(in crate::incremental_diplomacy) fn active_for_creator(
        &self,
        creator: &str,
        memory: &MemoryBudget,
        class: MemoryClass,
        work: &mut DiplomacyIndexWorkV1,
        mut stop: impl FnMut() -> bool,
    ) -> Result<usize, ValueTreeError> {
        same_pool(self, memory, class)?;
        let mut n = 0usize;
        for row in &self.data.rows {
            cancel(work, &mut stop)?;
            if work.compare(&row.creator, creator)?.is_eq()
                && matches!(
                    row.status,
                    EconomicContractStatus::Open | EconomicContractStatus::Accepted
                )
            {
                n = n.checked_add(1).ok_or_else(bounded)?;
            }
        }
        Ok(n)
    }
}
fn retain_row(
    key: &WorldFieldKeyV1,
    member: [u8; 32],
    child: &StateRootV1,
    outer: Text,
    creator: Text,
    status: EconomicContractStatus,
    memory: &MemoryBudget,
    class: MemoryClass,
    work: &mut DiplomacyIndexWorkV1,
) -> Result<Held, ValueTreeError> {
    work.step()?;
    let n = plus(
        arc_layout::<ContractRow>()?,
        plus(key.as_bytes().len(), child.node_root.len())?,
    )?;
    let fee = memory.reserve(n, class)?;
    work.comparison_bytes_remaining = work
        .comparison_bytes_remaining
        .checked_sub(plus(key.as_bytes().len(), child.node_root.len())? as u64)
        .ok_or(ValueTreeError::ResourceLimited(
            "contract metadata copy bytes",
        ))?;
    let parsed = WorldFieldKeyV1::from_bytes(key.as_bytes()).map_err(ValueTreeError::Invalid)?;
    let copied = WorldFieldKeyV1::new(
        DiplomacyFieldV1::EconomicContracts.descriptor()?,
        parsed.kind,
        parsed.semantic_key,
    )
    .map_err(ValueTreeError::Invalid)?;
    if copied.as_bytes() != key.as_bytes() {
        return Err(closure::invalid());
    }
    let child = child.clone();
    Ok(Arc::new(fee.retain(ContractRow {
        key: copied,
        member,
        outer,
        child,
        creator,
        status,
        class,
    })))
}
/// Only the original family GEN/changed closure may call this projection after
/// authenticating this exact child. No public caller supplies a raw root here.
pub(in crate::incremental_diplomacy) fn project_contract_row<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    entry: &StateEntryV1,
    child: &StateRootV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut DiplomacyIndexWorkV1,
) -> Result<Held, ValueTreeError> {
    ctx.check()?;
    let outer = outer_text(entry, ctx, work)?;
    let parsed = WorldFieldKeyV1::from_bytes(&entry.key).map_err(ValueTreeError::Invalid)?;
    if parsed.name != DiplomacyFieldV1::EconomicContracts.name() {
        return Err(closure::invalid());
    }
    let key = WorldFieldKeyV1::new(
        DiplomacyFieldV1::EconomicContracts.descriptor()?,
        parsed.kind,
        parsed.semantic_key,
    )
    .map_err(ValueTreeError::Invalid)?;
    let mut local = WorldFieldReadContextV1 {
        root: child,
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
    let creator = text_at(
        DiplomacyFieldV1::EconomicContracts,
        Path::Field("creator_agent_id"),
        &mut local,
        work,
    )?;
    let status = text_at(
        DiplomacyFieldV1::EconomicContracts,
        Path::Field("status"),
        &mut local,
        work,
    )?;
    let status = match status.as_str() {
        "open" => EconomicContractStatus::Open,
        "accepted" => EconomicContractStatus::Accepted,
        "settled" => EconomicContractStatus::Settled,
        "expired" => EconomicContractStatus::Expired,
        _ => return Err(closure::invalid()),
    };
    drop(local);
    retain_row(
        &key,
        entry.value,
        child,
        outer,
        creator,
        status,
        ctx.values.memory,
        ctx.values.class,
        work,
    )
}

/// Finite compatibility admission derived from this private resident directory.
/// One lease step and three copy steps per row; no public Current meter resets.
pub(in crate::incremental_diplomacy) fn readmit_work(
    source: &ContractCreatorIndexV1,
) -> Result<DiplomacyIndexWorkV1, ValueTreeError> {
    let mut bytes = 0usize;
    let mut max_text = 1usize;
    for row in &source.data.rows {
        max_text = max_text.max(row.outer.len()).max(row.creator.len());
        bytes = plus(
            bytes,
            plus(
                plus(row.key.as_bytes().len(), row.child.node_root.len())?,
                plus(row.outer.len(), row.creator.len())?,
            )?,
        )?;
    }
    let steps = u64::try_from(source.len())
        .map_err(|_| bounded())?
        .checked_mul(4)
        .and_then(|n| n.checked_add(1))
        .ok_or_else(bounded)?;
    Ok(DiplomacyIndexWorkV1 {
        cells_remaining: steps,
        comparison_bytes_remaining: u64::try_from(bytes).map_err(|_| bounded())?,
        max_rows: source.len(),
        max_members: 0,
        max_text_bytes: max_text,
        max_pending_rows: 0,
        pending_entries_remaining: 0,
    })
}
/// Called only after the original receiver has authenticated every changed
/// child. Untouched rows retain their original paid immutable backing.
pub(in crate::incremental_diplomacy) fn accepted_index<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    source: &ContractCreatorIndexV1,
    writes: &[oasis7_formal_protocol::incremental_execution::ExecutionWriteV1],
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut DiplomacyIndexWorkV1,
    _max: u64,
) -> Result<ContractCreatorIndexV1, ValueTreeError> {
    ctx.check()?;
    same_pool(source, ctx.values.memory, ctx.values.class)?;
    let mut changed = None;
    for write in writes {
        ctx.check()?;
        work.step()?;
        let Some((field, key, kind)) =
            super::current::edge_kind(&write.mutation.key, write.namespace)?
        else {
            continue;
        };
        if field != DiplomacyFieldV1::EconomicContracts {
            continue;
        }
        if kind == WorldFieldRecordKindV1::Metadata {
            // Original recognizer authenticates Metadata; never call Entry readers here.
            let value = write.mutation.after.ok_or_else(closure::invalid)?;
            let _ = family::outer(
                &StateEntryV1 {
                    namespace: write.namespace,
                    key: write.mutation.key.clone(),
                    value,
                },
                ctx,
            )?;
            continue;
        }
        if kind != WorldFieldRecordKindV1::Entry {
            return Err(closure::invalid());
        }
        let mut old = None;
        for row in &source.data.rows {
            ctx.check()?;
            work.step()?;
            work.comparison_bytes_remaining = work
                .comparison_bytes_remaining
                .checked_sub(plus(row.key.as_bytes().len(), key.as_bytes().len())? as u64)
                .ok_or_else(bounded)?;
            if row.key.as_bytes() == key.as_bytes() {
                old = Some(row);
                break;
            }
        }
        match (old, write.mutation.before) {
            (Some(row), Some(value)) if row.member == value => {}
            (None, None) => {}
            _ => return Err(closure::invalid()),
        }
        if changed.is_none() {
            changed = Some(source.copy_for_changes(
                ctx.values.memory,
                ctx.values.class,
                work,
                &mut ctx.cancelled,
            )?);
        }
        let output = changed.as_mut().ok_or_else(closure::invalid)?;
        if let Some(value) = write.mutation.after {
            let entry = StateEntryV1 {
                namespace: write.namespace,
                key: write.mutation.key.clone(),
                value,
            };
            let Some((owner, _, child)) = family::outer(&entry, ctx)? else {
                return Err(closure::invalid());
            };
            if owner != DiplomacyFieldV1::EconomicContracts {
                return Err(closure::invalid());
            }
            let row = project_contract_row(&entry, &child, ctx, work)?;
            output.put(row, work, &mut ctx.cancelled)?;
        } else {
            if old.is_none() {
                return Err(closure::invalid());
            }
            output.remove(&key, work, &mut ctx.cancelled)?;
        }
    }
    match changed {
        Some(output) => output.finish(),
        None => source.share(ctx.values.memory, ctx.values.class),
    }
}
#[path = "incremental_diplomacy_contract_index_current.rs"]
mod current;
pub use current::{CurrentContractCreatorCountV1, observe_current_creator_contract_count_v1};

#[cfg(test)]
#[path = "incremental_diplomacy_contract_index_tests.rs"]
mod tests;
