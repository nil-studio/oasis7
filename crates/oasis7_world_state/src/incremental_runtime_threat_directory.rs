//! Complete resident original Threat keys. DATA only; consumers still point
//! read actual Current quantities and perform their original business CAS.
use crate::{
    incremental_field_overlay::WorldFieldOverlayErrorV1,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_runtime_business::{RuntimeBusinessQualificationV1, with_runtime_threat_member_v1},
};
use oasis7_formal_protocol::{
    incremental_execution::{
        ExecutionFence, ExecutionTransaction, PreparedGuard, RootSetV1, SealedExecutionBindingV1,
    },
    incremental_state::{
        MerkleNodeReader, StateEntryV1, StateNamespaceV1, StateScanBudget, StateScanContext,
        StateScanError,
    },
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use std::{
    cell::RefCell,
    cmp::Ordering,
    mem::{align_of, size_of},
    sync::Arc,
};
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("resident Threat key directory")
}
fn limited() -> ValueTreeError {
    ValueTreeError::ResourceLimited("resident Threat key directory")
}
fn root_bytes(roots: &RootSetV1) -> Result<usize, ValueTreeError> {
    roots.state.iter().try_fold(roots.world_id.len(), |n, r| {
        n.checked_add(r.node_root.len())
            .and_then(|n| n.checked_add(128))
            .ok_or_else(limited)
    })
}
fn arc_heap<T>() -> Result<usize, ValueTreeError> {
    size_of::<ReservedValue<T>>()
        .checked_add(2 * size_of::<usize>())
        .and_then(|n| n.checked_add(2 * align_of::<ReservedValue<T>>()))
        .ok_or_else(limited)
}
pub struct ThreatKeyDirectoryWorkV1 {
    pub visits_remaining: u64,
    pub comparison_bytes_remaining: u64,
    pub pending_entries_remaining: usize,
    pub max_rows: usize,
    pub max_key_bytes: usize,
}
impl ThreatKeyDirectoryWorkV1 {
    fn step(&mut self, bytes: usize, c: &mut impl FnMut() -> bool) -> Result<(), ValueTreeError> {
        if c() {
            return Err(ValueTreeError::Cancelled);
        }
        self.visits_remaining = self.visits_remaining.checked_sub(1).ok_or_else(limited)?;
        self.comparison_bytes_remaining = self
            .comparison_bytes_remaining
            .checked_sub(u64::try_from(bytes).map_err(|_| limited())?)
            .ok_or_else(limited)?;
        Ok(())
    }
}
fn compare(
    a: &[u8],
    b: &[u8],
    work: &mut ThreatKeyDirectoryWorkV1,
    c: &mut impl FnMut() -> bool,
) -> Result<Ordering, ValueTreeError> {
    work.step(a.len().min(b.len()), c)?;
    Ok(a.cmp(b))
}
struct ThreatKeyRowV1 {
    key: Vec<u8>,
    entry_key: Vec<u8>,
    member: [u8; 32],
}
#[path = "incremental_runtime_threat_directory_tree.rs"]
mod tree;
pub struct ThreatKeyDirectoryV1 {
    tree: Option<tree::NodeRef>,
    count: u64,
    roots: [u8; 32],
    memory: MemoryBudget,
}
impl ThreatKeyDirectoryV1 {
    pub fn visit<ER: From<ValueTreeError>>(
        &self,
        work: &mut ThreatKeyDirectoryWorkV1,
        mut cancelled: impl FnMut() -> bool,
        mut visit: impl FnMut(&str, &mut ThreatKeyDirectoryWorkV1) -> Result<(), ER>,
    ) -> Result<(), ER> {
        tree::visit(&self.tree, work, &mut cancelled, &mut |row, work| {
            let key = std::str::from_utf8(&row.key).map_err(|_| invalid())?;
            visit(key, work)
        })
    }

    pub fn validate_binding(
        &self,
        roots: &RootSetV1,
        memory: &MemoryBudget,
    ) -> Result<(), ValueTreeError> {
        if !self.memory.same_pool(memory)
            || self.tree.as_ref().is_some_and(|n| !n.uses_budget(memory))
            || self.roots != roots.digest().map_err(|_| invalid())?
        {
            return Err(invalid());
        }
        Ok(())
    }
}
pub struct CurrentThreatKeyDirectoryV1 {
    tree: Option<tree::NodeRef>,
    guard: PreparedGuard,
    parent: [u8; 32],
    memory: MemoryBudget,
}
impl CurrentThreatKeyDirectoryV1 {
    pub fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        live: &ExecutionFence,
    ) -> Result<(), WorldFieldOverlayErrorV1> {
        tx.validate_guard(&self.guard, live)?;
        if self.parent != tx.parent_fence().parent_roots
            || !self.memory.same_pool(memory)
            || !tx.uses_budget(memory)
        {
            return Err(invalid().into());
        }
        Ok(())
    }
    pub fn visit<ER: From<ValueTreeError>>(
        &self,
        work: &mut ThreatKeyDirectoryWorkV1,
        mut cancelled: impl FnMut() -> bool,
        mut visit: impl FnMut(&str, &mut ThreatKeyDirectoryWorkV1) -> Result<(), ER>,
    ) -> Result<(), ER> {
        tree::visit(&self.tree, work, &mut cancelled, &mut |row, work| {
            let key = std::str::from_utf8(&row.key).map_err(|_| invalid())?;
            visit(key, work)
        })
    }
}
pub struct FrozenThreatKeyDirectoryV1 {
    tree: Option<tree::NodeRef>,
    count: u64,
    guard: PreparedGuard,
    memory: MemoryBudget,
}
impl FrozenThreatKeyDirectoryV1 {
    pub fn validate_guard<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        live: &ExecutionFence,
    ) -> Result<(), WorldFieldOverlayErrorV1> {
        tx.validate_guard(&self.guard, live)?;
        if !self.memory.same_pool(memory) || !tx.uses_budget(memory) {
            return Err(invalid().into());
        }
        Ok(())
    }
    pub fn validate_sealed(
        &self,
        binding: &SealedExecutionBindingV1,
        roots: &RootSetV1,
        memory: &MemoryBudget,
    ) -> Result<(), WorldFieldOverlayErrorV1> {
        if !self.memory.same_pool(memory) {
            return Err(invalid().into());
        }
        binding.validate_source_guard(&self.guard, memory)?;
        binding.validate_after_roots(roots, memory)?;
        Ok(())
    }
    pub fn rebind_after_seal_v1(
        &self,
        binding: &SealedExecutionBindingV1,
        roots: &RootSetV1,
        memory: &MemoryBudget,
        work: &mut ThreatKeyDirectoryWorkV1,
        mut cancelled: impl FnMut() -> bool,
    ) -> Result<ReservedValue<ThreatKeyDirectoryV1>, WorldFieldOverlayErrorV1> {
        work.step(
            root_bytes(roots)?.checked_mul(2).ok_or_else(limited)?,
            &mut cancelled,
        )?;
        self.validate_sealed(binding, roots, memory)?;
        let fee = memory
            .reserve(size_of::<ThreatKeyDirectoryV1>(), MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        Ok(fee.retain(ThreatKeyDirectoryV1 {
            tree: self.tree.clone(),
            count: self.count,
            roots: roots.digest().map_err(|_| invalid())?,
            memory: memory.clone(),
        }))
    }
}
fn matches_key(bytes: &[u8]) -> Result<bool, ValueTreeError> {
    use crate::{
        incremental_projection::FieldOwner,
        incremental_world_field_key::{WorldFieldKeyV1, WorldFieldRecordKindV1},
    };
    if !bytes.starts_with(b"oasis7.iws.world-field-key.v1\0") {
        return Ok(false);
    }
    let key = WorldFieldKeyV1::from_bytes(bytes).map_err(ValueTreeError::Invalid)?;
    Ok(key.owner == FieldOwner::Runtime
        && key.name == "threat_heatmap"
        && key.kind != WorldFieldRecordKindV1::Metadata)
}
fn parents<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    q: &ReservedValue<RuntimeBusinessQualificationV1>,
    roots: &RootSetV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut ThreatKeyDirectoryWorkV1,
) -> Result<(), ValueTreeError> {
    ctx.check()?;
    let bytes = root_bytes(roots)?;
    work.step(bytes, &mut ctx.cancelled)?;
    if ctx.values.class != MemoryClass::Ordinary
        || !q.uses_budget(ctx.values.memory)
        || q.allocation_class() != MemoryClass::Ordinary
        || q.heatmap_entries() > work.max_rows as u64
    {
        return Err(invalid());
    }
    q.validate_binding(roots)?;
    Ok(())
}
fn inspect<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    entry: &StateEntryV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut ThreatKeyDirectoryWorkV1,
) -> Result<Arc<ReservedValue<ThreatKeyRowV1>>, ValueTreeError> {
    let bound = usize::try_from(ctx.semantic_bytes)
        .map_err(|_| limited())?
        .min(work.max_key_bytes.checked_add(128).ok_or_else(limited)?);
    let _buffer = ctx.values.memory.reserve(
        bound
            .checked_add(size_of::<Vec<u8>>())
            .ok_or_else(limited)?,
        MemoryClass::Ordinary,
    )?;
    let mut key = Vec::new();
    key.try_reserve_exact(bound).map_err(|_| limited())?;
    if key.capacity() > bound {
        return Err(limited());
    }
    let memory = ctx.values.memory;
    let stop = RefCell::new(&mut ctx.cancelled);
    with_runtime_threat_member_v1(
        entry,
        &mut ctx.values,
        ctx.semantic_bytes,
        ctx.record_bytes,
        &mut key,
        bound,
        &mut || (stop.borrow_mut())(),
        |original, _quantity| {
            if original.len() > work.max_key_bytes {
                return Err(limited());
            }
            work.step(
                original
                    .len()
                    .checked_add(entry.key.len())
                    .ok_or_else(limited)?,
                &mut || (stop.borrow_mut())(),
            )?;
            let bytes = arc_heap::<ThreatKeyRowV1>()?
                .checked_add(original.len())
                .and_then(|n| n.checked_add(entry.key.len()))
                .ok_or_else(limited)?;
            let fee = memory.reserve(bytes, MemoryClass::Ordinary)?;
            Ok(Arc::new(fee.retain(ThreatKeyRowV1 {
                key: original.as_bytes().to_vec(),
                entry_key: entry.key.clone(),
                member: entry.value,
            })))
        },
    )?
    .ok_or_else(invalid)
}
pub fn build_threat_key_directory_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    q: &ReservedValue<RuntimeBusinessQualificationV1>,
    roots: &RootSetV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut ThreatKeyDirectoryWorkV1,
) -> Result<ReservedValue<ThreatKeyDirectoryV1>, ValueTreeError> {
    parents(q, roots, ctx, work)?;
    let cap = work.max_rows.clamp(1, 256);
    let _page = ctx.values.memory.reserve(
        cap.checked_mul(
            size_of::<StateEntryV1>()
                + crate::incremental_world_field_key::WORLD_FIELD_KEY_MAX_BYTES,
        )
        .ok_or_else(limited)?,
        MemoryClass::Ordinary,
    )?;
    let mut entries = Vec::new();
    entries.try_reserve_exact(cap).map_err(|_| limited())?;
    if entries.capacity() > cap {
        return Err(limited());
    }
    let root = roots.state_root(StateNamespaceV1::Business);
    let mut cursor = None;
    let mut tree = None;
    let mut count = 0u64;
    loop {
        ctx.check()?;
        let stop = RefCell::new(&mut ctx.cancelled);
        let result = StateScanContext {
            source: ctx.state,
            memory: ctx.values.memory,
            class: MemoryClass::Ordinary,
            io: scan,
        }
        .visit_entries_page(
            root,
            cursor,
            cap,
            |entry| {
                work.step(entry.key.len(), &mut || (stop.borrow_mut())())?;
                if matches_key(&entry.key)? {
                    if entries.len() == cap {
                        return Err(limited());
                    }
                    entries.push(entry.clone());
                }
                Ok(())
            },
            || (stop.borrow_mut())(),
        );
        let page = match result {
            Ok(p) => p,
            Err(StateScanError::Visitor(e)) => return Err(e),
            Err(StateScanError::Memory(e)) => return Err(e.into()),
            Err(StateScanError::Cancelled) => return Err(ValueTreeError::Cancelled),
            Err(StateScanError::ResourceLimited) => return Err(limited()),
            Err(_) => return Err(invalid()),
        };
        for entry in entries.drain(..) {
            let row = inspect(&entry, ctx, work)?;
            let (next, new) = tree::put(&tree, row, ctx.values.memory, work, &mut ctx.cancelled)?;
            if !new {
                return Err(invalid());
            }
            tree = Some(next);
            count = count.checked_add(1).ok_or_else(limited)?;
        }
        if !page.has_more {
            break;
        }
        let next = page.last_key_digest.ok_or_else(invalid)?;
        if page.visited == 0 || cursor.is_some_and(|old| next <= old) {
            return Err(invalid());
        }
        cursor = Some(next);
    }
    if count != q.heatmap_entries() {
        return Err(invalid());
    }
    ctx.check()?;
    work.step(root_bytes(roots)?, &mut ctx.cancelled)?;
    let fee = ctx
        .values
        .memory
        .reserve(size_of::<ThreatKeyDirectoryV1>(), MemoryClass::Ordinary)?;
    Ok(fee.retain(ThreatKeyDirectoryV1 {
        tree,
        count,
        roots: roots.digest().map_err(|_| invalid())?,
        memory: ctx.values.memory.clone(),
    }))
}
#[path = "incremental_runtime_threat_directory_current.rs"]
mod current;
pub use current::{freeze_current_threat_key_directory_v1, read_current_threat_key_directory_v1};
