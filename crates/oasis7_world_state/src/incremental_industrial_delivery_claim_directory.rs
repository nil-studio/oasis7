//! Resident complete claim identities, decoded once from genuine qualified
//! native originals. Normal queries fold only actual pending claim writes.
use super::*;
use oasis7_formal_protocol::{
    incremental_execution::SealedExecutionBindingV1,
    incremental_state::{StateEntryV1, StateScanContext, StateScanError},
};
use std::{
    cell::RefCell,
    cmp::Ordering,
    collections::BTreeSet,
    mem::{align_of, size_of},
    sync::Arc,
};
#[path = "incremental_industrial_delivery_claim_tree.rs"]
mod tree;
pub struct IndustrialClaimDirectoryWorkV1 {
    pub visits_remaining: u64,
    pub comparison_bytes_remaining: u64,
    pub pending_entries_remaining: usize,
    pub max_identity_bytes: usize,
    pub max_output_entries: usize,
    pub max_pending_records: usize,
}
impl IndustrialClaimDirectoryWorkV1 {
    fn step(
        &mut self,
        bytes: usize,
        c: &mut (impl FnMut() -> bool + ?Sized),
    ) -> Result<(), ValueTreeError> {
        if c() {
            return Err(ValueTreeError::Cancelled);
        }
        self.visits_remaining =
            self.visits_remaining
                .checked_sub(1)
                .ok_or(ValueTreeError::ResourceLimited(
                    "industrial claim directory visits",
                ))?;
        self.comparison_bytes_remaining = self
            .comparison_bytes_remaining
            .checked_sub(bytes as u64)
            .ok_or(ValueTreeError::ResourceLimited(
                "industrial claim directory bytes",
            ))?;
        Ok(())
    }
}
fn compare(
    a: &[u8],
    b: &[u8],
    w: &mut IndustrialClaimDirectoryWorkV1,
    c: &mut impl FnMut() -> bool,
) -> Result<Ordering, ValueTreeError> {
    w.step(a.len().min(b.len()), c)?;
    Ok(a.cmp(b))
}
fn arc_heap<T>() -> Result<usize, ValueTreeError> {
    size_of::<ReservedValue<T>>()
        .checked_add(2 * size_of::<usize>())
        .and_then(|n| n.checked_add(4 * align_of::<ReservedValue<T>>()))
        .ok_or_else(invalid)
}
struct ClaimIdentity {
    key: Vec<u8>,
    root: [u8; 32],
    outer: String,
    owner: String,
    target: String,
}
pub struct IndustrialClaimDirectoryV1 {
    roots: [u8; 32],
    tree: Option<tree::NodeRef>,
}
impl IndustrialClaimDirectoryV1 {
    pub fn validate_binding(&self, roots: &RootSetV1) -> Result<(), ValueTreeError> {
        if roots.digest().map_err(|_| invalid())? != self.roots {
            return Err(invalid());
        }
        Ok(())
    }
}
fn retain(
    entry: &StateEntryV1,
    outer: &str,
    claim: &crate::AgentClaimState,
    memory: &MemoryBudget,
    w: &mut IndustrialClaimDirectoryWorkV1,
    c: &mut impl FnMut() -> bool,
) -> Result<Arc<ReservedValue<ClaimIdentity>>, ValueTreeError> {
    let payload = entry
        .key
        .len()
        .checked_add(outer.len())
        .and_then(|n| n.checked_add(claim.claim_owner_id.len()))
        .and_then(|n| n.checked_add(claim.target_agent_id.len()))
        .ok_or_else(invalid)?;
    if payload > w.max_identity_bytes {
        return Err(ValueTreeError::ResourceLimited("industrial claim identity"));
    }
    w.step(payload, c)?;
    let fee = memory.reserve(
        arc_heap::<ClaimIdentity>()?
            .checked_add(payload)
            .ok_or_else(invalid)?,
        MemoryClass::Ordinary,
    )?;
    let value = ClaimIdentity {
        key: entry.key.clone(),
        root: entry.value,
        outer: outer.to_owned(),
        owner: claim.claim_owner_id.clone(),
        target: claim.target_agent_id.clone(),
    };
    if value.outer.capacity() > outer.len()
        || value.key.capacity() > entry.key.len()
        || value.owner.capacity() > claim.claim_owner_id.len()
        || value.target.capacity() > claim.target_agent_id.len()
    {
        return Err(invalid());
    }
    Ok(Arc::new(fee.retain(value)))
}
fn decode<
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    entry: &StateEntryV1,
    ctx: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
    scan: &mut StateScanBudget,
    typed: &mut WorldNativeSelectedReadWorkV1,
    w: &mut IndustrialClaimDirectoryWorkV1,
) -> Result<Arc<ReservedValue<ClaimIdentity>>, ValueTreeError> {
    let key = family::key::canonical::<AgentClaimsFieldV1>(entry)?;
    let original = family::key::original::<AgentClaimsFieldV1>(entry, ctx, w.max_identity_bytes)?;
    let body =
        read::read_body_at_entry_v1::<AgentClaimsFieldV1, _, _, _, _>(&key, &entry.value, ctx)?;
    let record: WorldNativeRecoveredRecordV1<crate::AgentClaimState> =
        recover_typed_record_body_v1(&body, &AgentClaimsFieldV1::profile()?, ctx, scan, typed)?;
    AgentClaimsFieldV1::validate_record_binding(
        family::key::original_ref::<AgentClaimsFieldV1>(&original)?,
        &body,
        ctx,
        scan,
        typed,
    )?;
    let WorldNativeKeyRefV1::Text(outer) =
        family::key::original_ref::<AgentClaimsFieldV1>(&original)?
    else {
        return Err(invalid());
    };
    read::check(ctx)?;
    retain(
        entry,
        outer,
        record.value().value(),
        ctx.values.memory,
        w,
        &mut ctx.cancelled,
    )
}
fn validate<
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    parent: &ReservedValue<WorldNativeFieldsQualificationV1>,
    roots: &RootSetV1,
    ctx: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
) -> Result<(), ValueTreeError> {
    read::check(ctx)?;
    if ctx.values.class != MemoryClass::Ordinary
        || !parent.uses_budget(ctx.values.memory)
        || parent.class != MemoryClass::Ordinary
        || ctx.root != roots.state_root(AgentClaimsFieldV1::profile()?.namespace()?)
    {
        return Err(invalid());
    }
    parent.agent_claims.validate_binding(roots)
}
pub fn build_industrial_claim_directory_v1<
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    parent: &ReservedValue<WorldNativeFieldsQualificationV1>,
    roots: &RootSetV1,
    ctx: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
    scan: &mut StateScanBudget,
    typed: &mut WorldNativeSelectedReadWorkV1,
    w: &mut IndustrialClaimDirectoryWorkV1,
) -> Result<ReservedValue<IndustrialClaimDirectoryV1>, ValueTreeError> {
    validate(parent, roots, ctx)?;
    if w.max_pending_records == 0 || w.max_pending_records > 256 {
        return Err(invalid());
    }
    let fee = ctx.values.memory.reserve(
        size_of::<IndustrialClaimDirectoryV1>(),
        MemoryClass::Ordinary,
    )?;
    let _page = ctx.values.memory.reserve(
        w.max_pending_records
            .checked_mul(
                size_of::<StateEntryV1>()
                    + crate::incremental_world_field_key::WORLD_FIELD_KEY_MAX_BYTES,
            )
            .and_then(|n| n.checked_add(8192))
            .ok_or_else(invalid)?,
        MemoryClass::Ordinary,
    )?;
    let mut pending = Vec::with_capacity(w.max_pending_records);
    if pending.capacity() > w.max_pending_records {
        return Err(invalid());
    }
    let mut cursor = None;
    let mut tree = None;
    let mut count = 0u64;
    let mut metadata = false;
    loop {
        read::check(ctx)?;
        let stop = RefCell::new(&mut ctx.cancelled);
        let result = StateScanContext {
            source: ctx.state,
            memory: ctx.values.memory,
            class: MemoryClass::Ordinary,
            io: &mut *scan,
        }
        .visit_entries_page(
            ctx.root,
            cursor,
            w.max_pending_records,
            |entry| {
                w.step(entry.key.len(), &mut || (stop.borrow_mut())())?;
                if !entry.key.starts_with(b"oasis7.iws.world-field-key.v1\0") {
                    return Ok(());
                }
                let parsed =
                    WorldFieldKeyV1::from_bytes(&entry.key).map_err(ValueTreeError::Invalid)?;
                if parsed.owner == FieldOwner::WorldState && parsed.name == AgentClaimsFieldV1::NAME
                {
                    family::key::canonical::<AgentClaimsFieldV1>(entry)?;
                    if pending.len() == pending.capacity() {
                        return Err(invalid());
                    }
                    pending.push(entry.clone());
                }
                Ok(())
            },
            || (stop.borrow_mut())(),
        );
        let page = match result {
            Ok(v) => v,
            Err(StateScanError::Visitor(e)) => return Err(e),
            Err(StateScanError::Memory(e)) => return Err(e.into()),
            Err(StateScanError::Cancelled) => return Err(ValueTreeError::Cancelled),
            Err(StateScanError::ResourceLimited) => {
                return Err(ValueTreeError::ResourceLimited(
                    "industrial claim cold scan",
                ));
            }
            Err(_) => return Err(invalid()),
        };
        for entry in pending.drain(..) {
            let key = family::key::canonical::<AgentClaimsFieldV1>(&entry)?;
            let parsed =
                WorldFieldKeyV1::from_bytes(key.as_bytes()).map_err(ValueTreeError::Invalid)?;
            match parsed.kind {
                WorldFieldRecordKindV1::Metadata => {
                    if metadata || parsed.semantic_key.is_some() {
                        return Err(invalid());
                    }
                    ctx.records().presence(&entry.value, &key, 0)?;
                    metadata = true;
                }
                WorldFieldRecordKindV1::Entry => {
                    let id = decode(&entry, ctx, scan, typed, w)?;
                    let (next, new) =
                        tree::put(&tree, id, ctx.values.memory, w, &mut ctx.cancelled)?;
                    if !new {
                        return Err(invalid());
                    }
                    tree = Some(next);
                    count = count.checked_add(1).ok_or_else(invalid)?;
                }
                _ => return Err(invalid()),
            }
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
    if !metadata || count != parent.agent_claims.records() {
        return Err(invalid());
    }
    read::check(ctx)?;
    Ok(fee.retain(IndustrialClaimDirectoryV1 {
        roots: roots.digest().map_err(|_| invalid())?,
        tree,
    }))
}
struct PendingClaim {
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
    dir: &ReservedValue<IndustrialClaimDirectoryV1>,
    parent: &ReservedValue<WorldNativeFieldsQualificationV1>,
    roots: &RootSetV1,
    tx: &mut ExecutionTransaction<'_, M>,
    ctx: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
    scan: &mut StateScanBudget,
    typed: &mut WorldNativeSelectedReadWorkV1,
    w: &mut IndustrialClaimDirectoryWorkV1,
) -> Result<Option<tree::NodeRef>, WorldFieldOverlayErrorV1> {
    validate(parent, roots, ctx)?;
    dir.validate_binding(roots)?;
    if !dir.uses_budget(ctx.values.memory)
        || !tx.uses_budget(ctx.values.memory)
        || roots.digest().map_err(|_| invalid())? != tx.parent_fence().parent_roots
    {
        return Err(invalid().into());
    }
    let mut tree = dir.tree.clone();
    let stop = RefCell::new(&mut ctx.cancelled);
    let mut pending = Vec::new();
    // Two passes avoid allocating by the protocol maximum: only actual matching
    // net changes and exact key payloads are admitted; no caller rows enter.
    let mut count = 0usize;
    let mut bytes = 0usize;
    tx.visit_pending_writes(
        AgentClaimsFieldV1::profile()?.namespace()?,
        &mut w.pending_entries_remaining,
        || {
            if (stop.borrow_mut())() {
                return Err(ValueTreeError::Cancelled.into());
            }
            Ok::<_, WorldFieldOverlayErrorV1>(())
        },
        |key, _, _| {
            if !key.starts_with(b"oasis7.iws.world-field-key.v1\0") {
                return Ok(());
            }
            let p = WorldFieldKeyV1::from_bytes(key).map_err(ValueTreeError::Invalid)?;
            if p.owner == FieldOwner::WorldState && p.name == AgentClaimsFieldV1::NAME {
                if p.kind != WorldFieldRecordKindV1::Entry {
                    return Err(invalid().into());
                }
                count = count.checked_add(1).ok_or_else(invalid)?;
                bytes = bytes.checked_add(key.len()).ok_or_else(invalid)?;
            }
            Ok(())
        },
    )?;
    let layout = count
        .checked_mul(size_of::<PendingClaim>())
        .and_then(|n| n.checked_add(bytes))
        .ok_or_else(invalid)?;
    let _fee = ctx
        .values
        .memory
        .reserve(layout, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    pending.try_reserve_exact(count).map_err(|_| invalid())?;
    if pending.capacity() > count {
        return Err(invalid().into());
    }
    w.step(bytes, &mut || (stop.borrow_mut())())?;
    let mut second_remaining = w.pending_entries_remaining;
    tx.visit_pending_writes(
        AgentClaimsFieldV1::profile()?.namespace()?,
        &mut second_remaining,
        || {
            (stop.borrow_mut())().then_some(()).map_or(Ok(()), |_| {
                Err(WorldFieldOverlayErrorV1::from(ValueTreeError::Cancelled))
            })
        },
        |key, before, after| {
            if !key.starts_with(b"oasis7.iws.world-field-key.v1\0") {
                return Ok(());
            }
            let p = WorldFieldKeyV1::from_bytes(key).map_err(ValueTreeError::Invalid)?;
            if p.owner == FieldOwner::WorldState && p.name == AgentClaimsFieldV1::NAME {
                pending.push(PendingClaim {
                    key: key.to_vec(),
                    before,
                    after,
                });
            }
            Ok(())
        },
    )?;
    w.pending_entries_remaining = second_remaining;
    drop(stop);
    for change in pending {
        w.step(change.key.len(), &mut ctx.cancelled)?;
        let old = tree::find(&tree, &change.key, w, &mut ctx.cancelled)?;
        if old.map(|v| v.root) != change.before {
            return Err(invalid().into());
        }
        let ns = AgentClaimsFieldV1::profile()?.namespace()?;
        if tx.read(ns, &change.key)? != change.after {
            return Err(invalid().into());
        }
        if let Some(root) = change.after {
            let entry = StateEntryV1 {
                namespace: ns,
                key: change.key,
                value: root,
            };
            let id = decode(&entry, ctx, scan, typed, w)?;
            tree = Some(tree::put(&tree, id, ctx.values.memory, w, &mut ctx.cancelled)?.0);
        } else {
            let (next, removed) =
                tree::remove(&tree, &change.key, ctx.values.memory, w, &mut ctx.cancelled)?;
            if !removed {
                return Err(invalid().into());
            }
            tree = next;
        }
    }
    read::check(ctx)?;
    Ok(tree)
}
/// Complete original inner-target set for one actual owner. The private index
/// preserves duplicate inner targets, which the original BTreeSet then folds.
pub fn recover_current_industrial_claim_targets_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    owner: &str,
    dir: &ReservedValue<IndustrialClaimDirectoryV1>,
    parent: &ReservedValue<WorldNativeFieldsQualificationV1>,
    roots: &RootSetV1,
    tx: &mut ExecutionTransaction<'_, M>,
    ctx: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
    scan: &mut StateScanBudget,
    typed: &mut WorldNativeSelectedReadWorkV1,
    w: &mut IndustrialClaimDirectoryWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<ReservedValue<BTreeSet<String>>, WorldFieldOverlayErrorV1> {
    tx.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        let tree = fold(dir, parent, roots, tx, ctx, scan, typed, w)?;
        let mut count = 0usize;
        let mut payload = 0usize;
        tree::visit::<ValueTreeError>(&tree, w, &mut ctx.cancelled, &mut |id, w| {
            w.step(id.owner.len().min(owner.len()), &mut || false)?;
            if id.owner == owner {
                count = count.checked_add(1).ok_or_else(invalid)?;
                payload = payload.checked_add(id.target.len()).ok_or_else(invalid)?;
            }
            Ok(())
        })?;
        if count > w.max_output_entries {
            return Err(ValueTreeError::ResourceLimited("industrial claim targets").into());
        }
        // Rust BTree nodes have <=11 keys and <=12 edges; one maximal node per
        // element plus two spare nodes is conservative, including pointer metadata.
        let node = 11 * size_of::<String>() + 12 * size_of::<usize>() + 64;
        let fee = ctx
            .values
            .memory
            .reserve(
                node.checked_mul(count.checked_add(2).ok_or_else(invalid)?)
                    .and_then(|n| n.checked_add(payload))
                    .and_then(|n| n.checked_add(size_of::<BTreeSet<String>>()))
                    .ok_or_else(invalid)?,
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        let mut output = BTreeSet::new();
        tree::visit::<WorldFieldOverlayErrorV1>(&tree, w, &mut ctx.cancelled, &mut |id, w| {
            if id.owner == owner {
                w.step(
                    id.target
                        .len()
                        .checked_mul(usize::BITS as usize * 12)
                        .ok_or_else(invalid)?,
                    &mut || false,
                )?;
                if tx.read(AgentClaimsFieldV1::profile()?.namespace()?, &id.key)? != Some(id.root) {
                    return Err(invalid().into());
                }
                output.insert(id.target.clone());
            }
            Ok(())
        })?;
        read::check(ctx)?;
        tx.validate_guard(&tx.guard(), live)?;
        Ok(fee.retain(output))
    })
}
pub struct FrozenIndustrialClaimDirectoryV1 {
    tree: Option<tree::NodeRef>,
    guard: PreparedGuard,
    memory: MemoryBudget,
}
impl FrozenIndustrialClaimDirectoryV1 {
    pub fn validate_guard<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        live: &ExecutionFence,
    ) -> Result<(), WorldFieldOverlayErrorV1> {
        tx.validate_guard(&self.guard, live)?;
        if !self.memory.same_pool(memory)
            || !tx.uses_budget(memory)
            || self.tree.as_ref().is_some_and(|n| !n.uses_budget(memory))
        {
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
        if !self.memory.same_pool(memory)
            || self.tree.as_ref().is_some_and(|n| !n.uses_budget(memory))
        {
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
        w: &mut IndustrialClaimDirectoryWorkV1,
        mut cancelled: impl FnMut() -> bool,
    ) -> Result<ReservedValue<IndustrialClaimDirectoryV1>, WorldFieldOverlayErrorV1> {
        let bytes = roots
            .state
            .iter()
            .try_fold(roots.world_id.len() + 256, |n, r| {
                n.checked_add(r.node_root.len() + 128)
            })
            .ok_or_else(invalid)?;
        w.step(bytes.checked_mul(4).ok_or_else(invalid)?, &mut cancelled)?;
        let _phase = memory
            .reserve(
                bytes
                    .checked_mul(4)
                    .and_then(|n| n.checked_add(4096))
                    .ok_or_else(invalid)?,
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        if !self.memory.same_pool(memory) {
            return Err(invalid().into());
        }
        binding.validate_source_guard(&self.guard, memory)?;
        binding.validate_after_roots(roots, memory)?;
        let fee = memory
            .reserve(
                size_of::<IndustrialClaimDirectoryV1>(),
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        Ok(fee.retain(IndustrialClaimDirectoryV1 {
            roots: binding.after_roots_digest(),
            tree: self.tree.clone(),
        }))
    }
}
pub fn freeze_current_industrial_claim_directory_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    dir: &ReservedValue<IndustrialClaimDirectoryV1>,
    parent: &ReservedValue<WorldNativeFieldsQualificationV1>,
    roots: &RootSetV1,
    tx: &mut ExecutionTransaction<'_, M>,
    ctx: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
    scan: &mut StateScanBudget,
    typed: &mut WorldNativeSelectedReadWorkV1,
    w: &mut IndustrialClaimDirectoryWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<ReservedValue<FrozenIndustrialClaimDirectoryV1>, WorldFieldOverlayErrorV1> {
    tx.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        let tree = fold(dir, parent, roots, tx, ctx, scan, typed, w)?;
        let fee = ctx
            .values
            .memory
            .reserve(
                size_of::<FrozenIndustrialClaimDirectoryV1>(),
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        read::check(ctx)?;
        let guard = tx.guard();
        tx.validate_guard(&guard, live)?;
        Ok(fee.retain(FrozenIndustrialClaimDirectoryV1 {
            tree,
            guard,
            memory: ctx.values.memory.clone(),
        }))
    })
}

pub fn industrial_claim_namespace_v1() -> Result<StateNamespaceV1, ValueTreeError> {
    AgentClaimsFieldV1::profile()?.namespace()
}

/// Stable, paid original outer-key ordering. Identity rows are DATA; the
/// epoch driver must reopen each selected inner target from actual Current.
pub struct CurrentIndustrialClaimSnapshotV1 {
    rows: Vec<Arc<ReservedValue<ClaimIdentity>>>,
    _tree: Option<tree::NodeRef>,
}
pub struct IndustrialClaimSnapshotEntryV1<'a>(&'a ClaimIdentity);
impl IndustrialClaimSnapshotEntryV1<'_> {
    pub fn outer_id(&self) -> &str {
        &self.0.outer
    }
    pub fn target_agent_id(&self) -> &str {
        &self.0.target
    }
    pub fn owner_id(&self) -> &str {
        &self.0.owner
    }
}
impl CurrentIndustrialClaimSnapshotV1 {
    pub fn entries(&self) -> impl ExactSizeIterator<Item = IndustrialClaimSnapshotEntryV1<'_>> {
        self.rows
            .iter()
            .map(|row| IndustrialClaimSnapshotEntryV1(&***row))
    }
}
#[allow(clippy::too_many_arguments)]
pub fn recover_current_industrial_claim_snapshot_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    dir: &ReservedValue<IndustrialClaimDirectoryV1>,
    parent: &ReservedValue<WorldNativeFieldsQualificationV1>,
    roots: &RootSetV1,
    tx: &mut ExecutionTransaction<'_, M>,
    ctx: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
    scan: &mut StateScanBudget,
    typed: &mut WorldNativeSelectedReadWorkV1,
    w: &mut IndustrialClaimDirectoryWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<ReservedValue<CurrentIndustrialClaimSnapshotV1>, WorldFieldOverlayErrorV1> {
    tx.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        let current = fold(dir, parent, roots, tx, ctx, scan, typed, w)?;
        let mut count = 0usize;
        tree::visit_shared::<ValueTreeError>(&current, w, &mut ctx.cancelled, &mut |_, _| {
            count = count.checked_add(1).ok_or_else(invalid)?;
            Ok(())
        })?;
        if count > w.max_output_entries {
            return Err(ValueTreeError::ResourceLimited("claim snapshot count").into());
        }
        let temporary = ctx
            .values
            .memory
            .reserve(
                crate::incremental_collection_layout::string_set_output_heap_v1(count)?,
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        let fee = ctx
            .values
            .memory
            .reserve(
                count
                    .checked_mul(size_of::<Arc<ReservedValue<ClaimIdentity>>>())
                    .and_then(|n| n.checked_add(size_of::<CurrentIndustrialClaimSnapshotV1>()))
                    .ok_or_else(invalid)?,
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        let mut ordered = std::collections::BTreeMap::new();
        tree::visit_shared::<ValueTreeError>(&current, w, &mut ctx.cancelled, &mut |id, w| {
            // At most twelve slots per B-tree level and fewer than usize::BITS
            // levels. Debit before std's comparisons; no unmetered fallible sort.
            let comparisons = u64::from(usize::BITS).checked_mul(12).ok_or_else(invalid)?;
            w.visits_remaining = w.visits_remaining.checked_sub(comparisons).ok_or(
                ValueTreeError::ResourceLimited("claim snapshot ordering visits"),
            )?;
            w.comparison_bytes_remaining = w
                .comparison_bytes_remaining
                .checked_sub(
                    (id.outer.len() as u64)
                        .checked_mul(comparisons)
                        .ok_or_else(invalid)?,
                )
                .ok_or(ValueTreeError::ResourceLimited(
                    "claim snapshot ordering bytes",
                ))?;
            if ordered.insert(id.outer.as_str(), id).is_some() {
                return Err(invalid());
            }
            Ok(())
        })?;
        let mut rows = Vec::new();
        rows.try_reserve_exact(count).map_err(|_| invalid())?;
        if rows.capacity() > count {
            return Err(invalid().into());
        }
        for (_, id) in ordered {
            w.step(size_of::<usize>(), &mut ctx.cancelled)?;
            rows.push(Arc::clone(id));
        }
        drop(temporary);
        read::check(ctx)?;
        tx.validate_guard(&tx.guard(), live)?;
        Ok(fee.retain(CurrentIndustrialClaimSnapshotV1 {
            rows,
            _tree: current,
        }))
    })
}
