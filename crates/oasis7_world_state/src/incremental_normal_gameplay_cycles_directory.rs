//! Resident original gameplay selector identities. DATA only: event reducers
//! must reselect the actual Current record and complete every original write.
use crate::{
    incremental_field_overlay::WorldFieldOverlayErrorV1,
    incremental_field_read::WorldFieldReadContextV1,
};
use oasis7_formal_protocol::{
    incremental_execution::{
        ExecutionTransaction, PreparedGuard, RootSetV1, SealedExecutionBindingV1,
    },
    incremental_state::{
        MerkleNodeReader, StateEntryV1, StateNamespaceV1, StateRootV1, StateScanBudget,
        StateScanContext, StateScanError,
    },
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use std::{
    cmp::Ordering,
    mem::{align_of, size_of},
    sync::Arc,
};
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GameplayDirectoryKindV1 {
    Proposal,
    War,
    Crisis,
    Contract,
}
impl GameplayDirectoryKindV1 {
    fn slot(self) -> usize {
        match self {
            Self::Proposal => 0,
            Self::War => 1,
            Self::Crisis => 2,
            Self::Contract => 3,
        }
    }
}
pub enum GameplaySelectorV1 {
    Proposal {
        open: bool,
        closes_at: u64,
    },
    War {
        inner_id: String,
        active: bool,
        declared_at: u64,
        max_duration_ticks: u64,
    },
    Crisis {
        active: bool,
        expires_at: u64,
        severity: u32,
    },
    Contract {
        inner_id: String,
        due_status: bool,
        atomic: bool,
        expires_at: u64,
    },
}
pub struct GameplayDirectoryRowV1 {
    kind: GameplayDirectoryKindV1,
    key: Vec<u8>,
    outer: String,
    member: [u8; 32],
    selector: GameplaySelectorV1,
    class: MemoryClass,
}
impl GameplayDirectoryRowV1 {
    pub fn kind(&self) -> GameplayDirectoryKindV1 {
        self.kind
    }
    pub fn outer_id(&self) -> &str {
        &self.outer
    }
    pub fn selector(&self) -> &GameplaySelectorV1 {
        &self.selector
    }
}
pub struct GameplayDirectoryWorkV1 {
    pub visits_remaining: u64,
    pub comparison_bytes_remaining: u64,
    pub pending_entries_remaining: usize,
    pub max_rows: usize,
    pub max_text_bytes: usize,
    pub max_payload_bytes: u64,
}
impl GameplayDirectoryWorkV1 {
    pub(crate) fn step(
        &mut self,
        bytes: usize,
        c: &mut impl FnMut() -> bool,
    ) -> Result<(), ValueTreeError> {
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
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("gameplay resident selector directory")
}
fn limited() -> ValueTreeError {
    ValueTreeError::ResourceLimited("gameplay resident selector directory")
}
fn arc_heap<T>() -> Result<usize, ValueTreeError> {
    size_of::<ReservedValue<T>>()
        .checked_add(2 * size_of::<usize>())
        .and_then(|n| n.checked_add(2 * align_of::<ReservedValue<T>>()))
        .ok_or_else(limited)
}
fn compare(
    a: &[u8],
    b: &[u8],
    work: &mut GameplayDirectoryWorkV1,
    c: &mut impl FnMut() -> bool,
) -> Result<Ordering, ValueTreeError> {
    work.step(a.len().min(b.len()), c)?;
    Ok(a.cmp(b))
}
#[path = "incremental_normal_gameplay_cycles_directory_tree.rs"]
mod tree;
pub struct GameplayCycleDirectoryV1 {
    tree: Option<tree::NodeRef>,
    roots: [u8; 32],
    counts: [u64; 4],
    memory: MemoryBudget,
}
impl GameplayCycleDirectoryV1 {
    pub fn validate_binding(
        &self,
        roots: &RootSetV1,
        memory: &MemoryBudget,
    ) -> Result<(), ValueTreeError> {
        if roots.digest().map_err(|_| invalid())? != self.roots
            || !self.memory.same_pool(memory)
            || self.tree.as_ref().is_some_and(|n| !n.uses_budget(memory))
        {
            return Err(invalid());
        }
        Ok(())
    }
    pub fn visit(
        &self,
        work: &mut GameplayDirectoryWorkV1,
        mut cancelled: impl FnMut() -> bool,
        mut visit: impl FnMut(&GameplayDirectoryRowV1) -> Result<(), ValueTreeError>,
    ) -> Result<(), ValueTreeError> {
        tree::visit(&self.tree, work, &mut cancelled, &mut |row, _| visit(row))
    }
}
pub struct FrozenGameplayCycleDirectoryV1 {
    tree: Option<tree::NodeRef>,
    counts: [u64; 4],
    guard: PreparedGuard,
    memory: MemoryBudget,
}
impl FrozenGameplayCycleDirectoryV1 {
    pub fn validate_guard<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        live: &oasis7_formal_protocol::incremental_execution::ExecutionFence,
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
        work: &mut GameplayDirectoryWorkV1,
        mut cancelled: impl FnMut() -> bool,
    ) -> Result<ReservedValue<GameplayCycleDirectoryV1>, WorldFieldOverlayErrorV1> {
        self.validate_sealed(binding, roots, memory)?;
        work.step(0, &mut cancelled)?;
        let fee = memory
            .reserve(size_of::<GameplayCycleDirectoryV1>(), MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        Ok(fee.retain(GameplayCycleDirectoryV1 {
            tree: self.tree.clone(),
            counts: self.counts,
            roots: roots.digest().map_err(|_| invalid())?,
            memory: memory.clone(),
        }))
    }
}

pub(crate) enum GameplaySelectorRefV1<'a> {
    Proposal {
        open: bool,
        closes_at: u64,
    },
    War {
        inner_id: &'a str,
        active: bool,
        declared_at: u64,
        max_duration_ticks: u64,
    },
    Crisis {
        active: bool,
        expires_at: u64,
        severity: u32,
    },
    Contract {
        inner_id: &'a str,
        due_status: bool,
        atomic: bool,
        expires_at: u64,
    },
}
pub(crate) fn retain_row(
    kind: GameplayDirectoryKindV1,
    entry: &StateEntryV1,
    outer: &str,
    value: GameplaySelectorRefV1<'_>,
    memory: &MemoryBudget,
    class: MemoryClass,
    work: &mut GameplayDirectoryWorkV1,
    c: &mut impl FnMut() -> bool,
) -> Result<Arc<ReservedValue<GameplayDirectoryRowV1>>, ValueTreeError> {
    if class != MemoryClass::Ordinary || outer.len() > work.max_text_bytes {
        return Err(invalid());
    }
    let inner = match &value {
        GameplaySelectorRefV1::War { inner_id, .. }
        | GameplaySelectorRefV1::Contract { inner_id, .. } => inner_id.len(),
        _ => 0,
    };
    if inner > work.max_text_bytes {
        return Err(limited());
    }
    let bytes = arc_heap::<GameplayDirectoryRowV1>()?
        .checked_add(entry.key.len())
        .and_then(|n| n.checked_add(outer.len()))
        .and_then(|n| n.checked_add(inner))
        .ok_or_else(limited)?;
    work.step(bytes, c)?;
    let fee = memory.reserve(bytes, class)?;
    let selector = match value {
        GameplaySelectorRefV1::Proposal { open, closes_at } => {
            GameplaySelectorV1::Proposal { open, closes_at }
        }
        GameplaySelectorRefV1::War {
            inner_id,
            active,
            declared_at,
            max_duration_ticks,
        } => GameplaySelectorV1::War {
            inner_id: inner_id.to_owned(),
            active,
            declared_at,
            max_duration_ticks,
        },
        GameplaySelectorRefV1::Crisis {
            active,
            expires_at,
            severity,
        } => GameplaySelectorV1::Crisis {
            active,
            expires_at,
            severity,
        },
        GameplaySelectorRefV1::Contract {
            inner_id,
            due_status,
            atomic,
            expires_at,
        } => GameplaySelectorV1::Contract {
            inner_id: inner_id.to_owned(),
            due_status,
            atomic,
            expires_at,
        },
    };
    Ok(Arc::new(fee.retain(GameplayDirectoryRowV1 {
        kind,
        key: entry.key.clone(),
        outer: outer.to_owned(),
        member: entry.value,
        selector,
        class,
    })))
}
/// The sole original semantic-key decoder is reused. Neither an embedded DTO
/// ID nor a digest is guessed to be the actual outer map key.
pub(crate) fn original_text<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    entry: &StateEntryV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut GameplayDirectoryWorkV1,
) -> Result<ReservedValue<String>, ValueTreeError> {
    use crate::incremental_field_member::*;
    let binding = WorldFieldKeyV1::from_bytes(&entry.key).map_err(ValueTreeError::Invalid)?;
    if binding.kind != WorldFieldRecordKindV1::Entry {
        return Err(invalid());
    }
    let descriptor = crate::incremental_projection::WORLD_STATE_FIELDS
        .iter()
        .copied()
        .find(|field| {
            field.name == binding.name
                && field.destination
                    == crate::incremental_projection::FieldDestination::State(entry.namespace)
        })
        .ok_or_else(invalid)?;
    let key = WorldFieldKeyV1::new(descriptor, binding.kind, binding.semantic_key)
        .map_err(ValueTreeError::Invalid)?;
    if key.as_bytes() != entry.key {
        return Err(invalid());
    }
    let info = read_member_record_header_v1(
        &entry.value,
        &key,
        &mut ctx.values,
        MemberRecordLimitsV1 {
            semantic_bytes: ctx.semantic_bytes,
            payload_bytes: oasis7_formal_protocol::incremental_record_root::RECORD_ROOT_BYTES
                as u64,
            record_bytes: ctx.record_bytes,
        },
        &mut ctx.cancelled,
    )?;
    let n = usize::try_from(info.semantic_bytes).map_err(|_| limited())?;
    if n > work.max_text_bytes.checked_add(128).ok_or_else(limited)? {
        return Err(limited());
    }
    work.step(n, &mut ctx.cancelled)?;
    let fee = ctx.values.memory.reserve(
        n.checked_mul(2)
            .and_then(|n| n.checked_add(size_of::<String>()))
            .ok_or_else(limited)?,
        ctx.values.class,
    )?;
    let mut bytes = Vec::new();
    bytes.try_reserve_exact(n).map_err(|_| limited())?;
    if bytes.capacity() > n {
        return Err(limited());
    }
    read_member_record_key_v1(
        &entry.value,
        &key,
        &mut ctx.values,
        MemberRecordLimitsV1 {
            semantic_bytes: ctx.semantic_bytes,
            payload_bytes: oasis7_formal_protocol::incremental_record_root::RECORD_ROOT_BYTES
                as u64,
            record_bytes: ctx.record_bytes,
        },
        &mut bytes,
        &mut ctx.cancelled,
    )?;
    let crate::incremental_semantic_key::SemanticKeyRefV1::Text(id) =
        crate::incremental_semantic_key::read_semantic_key_v1(&bytes)
            .map_err(ValueTreeError::Invalid)?
    else {
        return Err(invalid());
    };
    Ok(fee.retain(id.to_owned()))
}

fn matches_key(bytes: &[u8]) -> Result<bool, ValueTreeError> {
    use crate::incremental_field_member::*;
    if !bytes.starts_with(b"oasis7.iws.world-field-key.v1\0") {
        return Ok(false);
    }
    let key = WorldFieldKeyV1::from_bytes(bytes).map_err(ValueTreeError::Invalid)?;
    Ok(key.owner == FieldOwner::WorldState
        && key.kind == WorldFieldRecordKindV1::Entry
        && matches!(
            key.name,
            "governance_proposals" | "wars" | "crises" | "economic_contracts"
        ))
}
fn matches(entry: &StateEntryV1) -> Result<bool, ValueTreeError> {
    matches_key(&entry.key)
}
fn inspect<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    entry: &StateEntryV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut GameplayDirectoryWorkV1,
) -> Result<Option<Arc<ReservedValue<GameplayDirectoryRowV1>>>, ValueTreeError> {
    if !matches(entry)? {
        return Ok(None);
    }
    if entry.namespace == StateNamespaceV1::Authorization {
        crate::incremental_governance_records::inspect_gameplay_governance_directory_row_v1(
            entry, ctx, scan, work,
        )
    } else if entry.namespace == StateNamespaceV1::Business {
        crate::incremental_diplomacy::inspect_gameplay_diplomacy_directory_row_v1(
            entry, ctx, scan, work,
        )
    } else {
        Err(invalid())
    }
}
fn validate_parents<
    R: ValueNodeReader,
    M: MerkleNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    diplomacy: &ReservedValue<crate::incremental_diplomacy::DiplomacyFamilyQualificationV1>,
    governance: &ReservedValue<
        crate::incremental_governance_records::GovernanceFamilyQualificationV1,
    >,
    roots: &RootSetV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut GameplayDirectoryWorkV1,
) -> Result<[u64; 4], ValueTreeError> {
    ctx.check()?;
    let bytes = roots.state.iter().try_fold(0usize, |n, r| {
        n.checked_add(r.node_root.len()).ok_or_else(limited)
    })?;
    work.step(bytes, &mut ctx.cancelled)?;
    let digest = roots.digest().map_err(|_| invalid())?;
    if ctx.values.class != MemoryClass::Ordinary
        || !diplomacy.uses_budget(ctx.values.memory)
        || !governance.uses_budget(ctx.values.memory)
        || diplomacy.allocation_class() != MemoryClass::Ordinary
        || governance.allocation_class() != MemoryClass::Ordinary
    {
        return Err(invalid());
    }
    diplomacy.validate_binding(&digest)?;
    governance.validate_binding(&digest)?;
    let d = diplomacy.record_counts();
    let g = governance.record_counts();
    Ok([g[1], d[1], d[2], d[3]])
}
pub fn build_gameplay_cycle_directory_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    diplomacy: &ReservedValue<crate::incremental_diplomacy::DiplomacyFamilyQualificationV1>,
    governance: &ReservedValue<
        crate::incremental_governance_records::GovernanceFamilyQualificationV1,
    >,
    roots: &RootSetV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    outer_scan: &mut StateScanBudget,
    inner_scan: &mut StateScanBudget,
    work: &mut GameplayDirectoryWorkV1,
) -> Result<ReservedValue<GameplayCycleDirectoryV1>, ValueTreeError> {
    use std::cell::RefCell;
    let expected = validate_parents(diplomacy, governance, roots, ctx, work)?;
    let total = expected
        .iter()
        .try_fold(0u64, |n, c| n.checked_add(*c).ok_or_else(limited))?;
    if total > work.max_rows as u64 {
        return Err(limited());
    }
    let page_entries = work.max_rows.clamp(1, 256);
    let _page = ctx.values.memory.reserve(
        page_entries
            .checked_mul(
                size_of::<StateEntryV1>()
                    + crate::incremental_world_field_key::WORLD_FIELD_KEY_MAX_BYTES,
            )
            .ok_or_else(limited)?,
        MemoryClass::Ordinary,
    )?;
    let mut entries = Vec::new();
    entries
        .try_reserve_exact(page_entries)
        .map_err(|_| limited())?;
    if entries.capacity() > page_entries {
        return Err(limited());
    }
    let mut tree = None;
    let mut counts = [0u64; 4];
    for state_root in roots.state.iter().filter(|r| {
        matches!(
            r.namespace,
            StateNamespaceV1::Authorization | StateNamespaceV1::Business
        )
    }) {
        let mut cursor = None;
        loop {
            ctx.check()?;
            let stop = RefCell::new(&mut ctx.cancelled);
            let result = StateScanContext {
                source: ctx.state,
                memory: ctx.values.memory,
                class: MemoryClass::Ordinary,
                io: &mut *outer_scan,
            }
            .visit_entries_page(
                state_root,
                cursor,
                page_entries,
                |entry| {
                    work.step(entry.key.len(), &mut || (stop.borrow_mut())())?;
                    if matches(entry)? {
                        if entries.len() == page_entries {
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
            drop(stop);
            for entry in entries.drain(..) {
                let mut local = WorldFieldReadContextV1 {
                    root: state_root,
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
                if let Some(row) = inspect(&entry, &mut local, inner_scan, work)? {
                    let slot = row.kind.slot();
                    counts[slot] = counts[slot].checked_add(1).ok_or_else(limited)?;
                    let (next, new) =
                        tree::put(&tree, row, local.values.memory, work, &mut local.cancelled)?;
                    if !new {
                        return Err(invalid());
                    }
                    tree = Some(next);
                }
                local.check()?;
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
    }
    if counts != expected {
        return Err(invalid());
    }
    ctx.check()?;
    let fee = ctx
        .values
        .memory
        .reserve(size_of::<GameplayCycleDirectoryV1>(), MemoryClass::Ordinary)?;
    Ok(fee.retain(GameplayCycleDirectoryV1 {
        tree,
        counts,
        roots: roots.digest().map_err(|_| invalid())?,
        memory: ctx.values.memory.clone(),
    }))
}

struct Pending {
    namespace: StateNamespaceV1,
    key: Vec<u8>,
    before: Option<[u8; 32]>,
    after: Option<[u8; 32]>,
}
fn fold<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    directory: &ReservedValue<GameplayCycleDirectoryV1>,
    diplomacy: &ReservedValue<crate::incremental_diplomacy::DiplomacyFamilyQualificationV1>,
    governance: &ReservedValue<
        crate::incremental_governance_records::GovernanceFamilyQualificationV1,
    >,
    roots: &RootSetV1,
    tx: &mut ExecutionTransaction<'_, M>,
    ctx: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut GameplayDirectoryWorkV1,
) -> Result<(Option<tree::NodeRef>, [u64; 4]), WorldFieldOverlayErrorV1> {
    use std::cell::RefCell;
    let expected = validate_parents(diplomacy, governance, roots, ctx, work)?;
    directory.validate_binding(roots, ctx.values.memory)?;
    if expected != directory.counts
        || !directory.uses_budget(ctx.values.memory)
        || !tx.uses_budget(ctx.values.memory)
        || directory.roots != tx.parent_fence().parent_roots
    {
        return Err(invalid().into());
    }
    let stop = RefCell::new(&mut ctx.cancelled);
    let mut count = 0usize;
    let mut payload = 0usize;
    let mut remaining = work.pending_entries_remaining;
    for ns in [StateNamespaceV1::Authorization, StateNamespaceV1::Business] {
        let visited = tx.visit_pending_writes(
            ns,
            &mut remaining,
            || {
                if (stop.borrow_mut())() {
                    return Err(WorldFieldOverlayErrorV1::from(ValueTreeError::Cancelled));
                }
                Ok(())
            },
            |key, before, after| {
                work.step(key.len(), &mut || (stop.borrow_mut())())?;
                if before == after {
                    return Ok(());
                }
                // The counting pass borrows the actual paid transaction key.
                if matches_key(key)? {
                    count = count.checked_add(1).ok_or_else(limited)?;
                    payload = payload.checked_add(key.len()).ok_or_else(limited)?;
                }
                Ok(())
            },
        );
        work.pending_entries_remaining = remaining;
        visited?;
    }
    let bytes = count
        .checked_mul(size_of::<Pending>())
        .and_then(|n| n.checked_add(payload))
        .ok_or_else(limited)?;
    let _phase = ctx
        .values
        .memory
        .reserve(bytes, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let mut pending = Vec::new();
    pending.try_reserve_exact(count).map_err(|_| limited())?;
    if pending.capacity() > count {
        return Err(limited().into());
    }
    for ns in [StateNamespaceV1::Authorization, StateNamespaceV1::Business] {
        let visited = tx.visit_pending_writes(
            ns,
            &mut remaining,
            || {
                if (stop.borrow_mut())() {
                    return Err(WorldFieldOverlayErrorV1::from(ValueTreeError::Cancelled));
                }
                Ok(())
            },
            |key, before, after| {
                work.step(key.len(), &mut || (stop.borrow_mut())())?;
                if before == after {
                    return Ok(());
                }
                if matches_key(key)? {
                    pending.push(Pending {
                        namespace: ns,
                        key: key.to_vec(),
                        before,
                        after,
                    });
                }
                Ok(())
            },
        );
        work.pending_entries_remaining = remaining;
        visited?;
    }
    drop(stop);
    let mut tree = directory.tree.clone();
    let mut counts = directory.counts;
    for change in pending {
        work.step(change.key.len(), &mut ctx.cancelled)?;
        let old = tree::find(&tree, &change.key, work, &mut ctx.cancelled)?;
        if old.map(|r| r.member) != change.before {
            return Err(invalid().into());
        }
        let old_kind = old.map(|r| r.kind);
        if tx.read(change.namespace, &change.key)? != change.after {
            return Err(invalid().into());
        }
        if let Some(after) = change.after {
            let row = inspect(
                &StateEntryV1 {
                    namespace: change.namespace,
                    key: change.key,
                    value: after,
                },
                ctx,
                scan,
                work,
            )?
            .ok_or_else(invalid)?;
            if old_kind.is_some_and(|kind| kind != row.kind) {
                return Err(invalid().into());
            }
            let slot = row.kind.slot();
            if old_kind.is_none() {
                counts[slot] = counts[slot].checked_add(1).ok_or_else(limited)?;
            }
            let (next, _) = tree::put(&tree, row, ctx.values.memory, work, &mut ctx.cancelled)?;
            tree = Some(next);
        } else {
            let slot = old_kind.ok_or_else(invalid)?.slot();
            counts[slot] = counts[slot].checked_sub(1).ok_or_else(invalid)?;
            let (next, removed) = tree::remove(
                &tree,
                &change.key,
                ctx.values.memory,
                work,
                &mut ctx.cancelled,
            )?;
            if !removed {
                return Err(invalid().into());
            }
            tree = next;
        }
    }
    if counts
        .iter()
        .try_fold(0u64, |n, c| n.checked_add(*c).ok_or_else(limited))?
        > work.max_rows as u64
    {
        return Err(limited().into());
    }
    ctx.check()?;
    Ok((tree, counts))
}
pub struct CurrentGameplayCycleDirectoryV1 {
    tree: Option<tree::NodeRef>,
    counts: [u64; 4],
    guard: PreparedGuard,
    parent: [u8; 32],
    memory: MemoryBudget,
}
impl CurrentGameplayCycleDirectoryV1 {
    pub fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        live: &oasis7_formal_protocol::incremental_execution::ExecutionFence,
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
        work: &mut GameplayDirectoryWorkV1,
        mut cancelled: impl FnMut() -> bool,
        mut visitor: impl FnMut(&GameplayDirectoryRowV1, &mut GameplayDirectoryWorkV1) -> Result<(), ER>,
    ) -> Result<(), ER> {
        tree::visit(&self.tree, work, &mut cancelled, &mut visitor)
    }
}
#[allow(clippy::too_many_arguments)]
pub fn open_current_gameplay_cycle_directory_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    directory: &ReservedValue<GameplayCycleDirectoryV1>,
    diplomacy: &ReservedValue<crate::incremental_diplomacy::DiplomacyFamilyQualificationV1>,
    governance: &ReservedValue<
        crate::incremental_governance_records::GovernanceFamilyQualificationV1,
    >,
    roots: &RootSetV1,
    tx: &mut ExecutionTransaction<'_, M>,
    ctx: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut GameplayDirectoryWorkV1,
    guard: &PreparedGuard,
    live: &oasis7_formal_protocol::incremental_execution::ExecutionFence,
) -> Result<ReservedValue<CurrentGameplayCycleDirectoryV1>, WorldFieldOverlayErrorV1> {
    tx.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        let (tree, counts) = fold(directory, diplomacy, governance, roots, tx, ctx, scan, work)?;
        let fee = ctx
            .values
            .memory
            .reserve(
                size_of::<CurrentGameplayCycleDirectoryV1>(),
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        ctx.check()?;
        let guard = tx.guard();
        tx.validate_guard(&guard, live)?;
        Ok(fee.retain(CurrentGameplayCycleDirectoryV1 {
            tree,
            counts,
            guard,
            parent: tx.parent_fence().parent_roots,
            memory: ctx.values.memory.clone(),
        }))
    })
}
#[allow(clippy::too_many_arguments)]
pub fn freeze_current_gameplay_cycle_directory_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    directory: &ReservedValue<GameplayCycleDirectoryV1>,
    diplomacy: &ReservedValue<crate::incremental_diplomacy::DiplomacyFamilyQualificationV1>,
    governance: &ReservedValue<
        crate::incremental_governance_records::GovernanceFamilyQualificationV1,
    >,
    roots: &RootSetV1,
    tx: &mut ExecutionTransaction<'_, M>,
    ctx: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut GameplayDirectoryWorkV1,
    guard: &PreparedGuard,
    live: &oasis7_formal_protocol::incremental_execution::ExecutionFence,
) -> Result<ReservedValue<FrozenGameplayCycleDirectoryV1>, WorldFieldOverlayErrorV1> {
    tx.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        let (tree, counts) = fold(directory, diplomacy, governance, roots, tx, ctx, scan, work)?;
        let fee = ctx
            .values
            .memory
            .reserve(
                size_of::<FrozenGameplayCycleDirectoryV1>(),
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        ctx.check()?;
        let guard = tx.guard();
        tx.validate_guard(&guard, live)?;
        Ok(fee.retain(FrozenGameplayCycleDirectoryV1 {
            tree,
            counts,
            guard,
            memory: ctx.values.memory.clone(),
        }))
    })
}
