//! Resident job-to-root links derived from the authenticated industrial branch.
//! This is an index for the legacy completion predicates, not an authority.
use super::*;
use crate::{
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1, lookup},
    incremental_inventory_value_read::InventoryValueReader,
    incremental_record_columns_read::{ColumnCellV1, ColumnKeyV1, ColumnReadProfileV1},
    incremental_typed_record_diff::{
        ChangeDecoder, TypedCellRefV1, accept_body, key, point_typed_column_v1, root, visit_entries,
    },
    incremental_world_field_key::WorldFieldRecordKindV1,
};
use oasis7_agent_api::world_action::{IndustrialRefV1, IndustrialWorkStateV1, RootOperationV1};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionError, ExecutionFence, PreparedGuard, RootSetV1},
    incremental_state::{MerkleNodeReader, StateEntryV1, StateNamespaceV1, StateScanBudget},
    incremental_value::{ValueNodeReader, ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation, ReservedValue};
use std::{
    mem::{align_of, size_of},
    sync::Arc,
};

fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("industrial work root directory")
}
fn limit() -> ValueTreeError {
    ValueTreeError::ResourceLimited("industrial work root directory")
}
fn add(a: usize, b: usize) -> Result<usize, ValueTreeError> {
    a.checked_add(b).ok_or_else(limit)
}
fn arc_layout<T>() -> Result<usize, ValueTreeError> {
    add(
        size_of::<ReservedValue<T>>(),
        add(2 * size_of::<usize>(), align_of::<ReservedValue<T>>())?,
    )
}
fn spend(
    scan: &mut StateScanBudget,
    cancelled: &mut impl FnMut() -> bool,
    steps: u64,
    bytes: u64,
) -> Result<(), ValueTreeError> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    scan.nodes_remaining =
        scan.nodes_remaining
            .checked_sub(steps)
            .ok_or(ValueTreeError::ResourceLimited(
                "industrial directory steps",
            ))?;
    scan.read_bytes_remaining =
        scan.read_bytes_remaining
            .checked_sub(bytes)
            .ok_or(ValueTreeError::ResourceLimited(
                "industrial directory bytes",
            ))?;
    Ok(())
}

struct RootLinkV1 {
    outer_key: String,
    root_ref: IndustrialRefV1,
    started_job_id: Option<u64>,
}
struct DirectoryBackingV1 {
    rows: Vec<Arc<ReservedValue<RootLinkV1>>>,
    _rows_fee: MemoryReservation,
}

/// Immutable resident lookup data. Every row retains the map's outer key and
/// full embedded identity because the original conflict rule compares refs.
pub struct IndustrialWorkDirectoryV1 {
    data: Arc<ReservedValue<DirectoryBackingV1>>,
    memory: MemoryBudget,
    class: MemoryClass,
}
impl IndustrialWorkDirectoryV1 {
    pub fn len(&self) -> usize {
        self.data.rows.len()
    }
    pub fn is_empty(&self) -> bool {
        self.data.rows.is_empty()
    }
    pub fn check_budget(
        &self,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<(), ValueTreeError> {
        if self.class != class
            || !self.memory.same_pool(memory)
            || !self.data.uses_budget(memory)
            || self.data.reserved_bytes() < arc_layout::<DirectoryBackingV1>()?
        {
            return Err(invalid());
        }
        Ok(())
    }
    fn share(&self, memory: &MemoryBudget, class: MemoryClass) -> Result<Self, ValueTreeError> {
        self.check_budget(memory, class)?;
        Ok(Self {
            data: self.data.clone(),
            memory: memory.clone(),
            class,
        })
    }
    /// Implements the original `None` industrial-ref predicate: any matching
    /// started-job link makes the missing link invalid, even if the branch is
    /// otherwise empty or the matching root is not the requested map key.
    pub fn any_started_job_v1(
        &self,
        job_id: u64,
        memory: &MemoryBudget,
        class: MemoryClass,
        scan: &mut StateScanBudget,
        mut cancelled: impl FnMut() -> bool,
    ) -> Result<bool, ValueTreeError> {
        self.check_budget(memory, class)?;
        for row in &self.data.rows {
            spend(
                scan,
                &mut cancelled,
                1,
                u64::try_from(root_link_text_bytes(row)?).map_err(|_| limit())?,
            )?;
            if row.started_job_id == Some(job_id) {
                return Ok(true);
            }
        }
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
        Ok(false)
    }
    /// Implements the original `Some` conflict rule. A second link carrying
    /// the same full ref is allowed; only a distinct full ref conflicts.
    pub fn has_conflicting_started_job_v1(
        &self,
        job_id: u64,
        requested: &IndustrialRefV1,
        memory: &MemoryBudget,
        class: MemoryClass,
        scan: &mut StateScanBudget,
        mut cancelled: impl FnMut() -> bool,
    ) -> Result<bool, ValueTreeError> {
        self.check_budget(memory, class)?;
        for row in &self.data.rows {
            spend(
                scan,
                &mut cancelled,
                1,
                u64::try_from(
                    root_link_text_bytes(row)?
                        .checked_add(industrial_ref_text_bytes(requested)?)
                        .ok_or_else(limit)?,
                )
                .map_err(|_| limit())?,
            )?;
            if row.started_job_id == Some(job_id) && row.root_ref != *requested {
                return Ok(true);
            }
        }
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
        Ok(false)
    }
    pub(super) fn readmit_ordinary(
        &self,
        memory: &MemoryBudget,
        mut cancelled: impl FnMut() -> bool,
    ) -> Result<Self, ValueTreeError> {
        self.check_budget(memory, self.class)?;
        if self.class == MemoryClass::Ordinary {
            return self.share(memory, MemoryClass::Ordinary);
        }
        let mut builder = DirectoryBuilderV1::new(memory, MemoryClass::Ordinary)?;
        for row in &self.data.rows {
            if cancelled() {
                return Err(ValueTreeError::Cancelled);
            }
            builder.push_copy(&row.outer_key, &row.root_ref, row.started_job_id)?;
        }
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
        Ok(builder.finish())
    }
}
fn industrial_ref_text_bytes(value: &IndustrialRefV1) -> Result<usize, ValueTreeError> {
    [
        value.world_binding.len(),
        value.domain.len(),
        value.schema.len(),
        value.id.len(),
        value.digest.len(),
    ]
    .into_iter()
    .try_fold(0usize, |sum, part| sum.checked_add(part).ok_or_else(limit))
}
fn root_link_text_bytes(value: &RootLinkV1) -> Result<usize, ValueTreeError> {
    value
        .outer_key
        .len()
        .checked_add(industrial_ref_text_bytes(&value.root_ref)?)
        .ok_or_else(limit)
}

struct DirectoryBuilderV1 {
    rows: Vec<Arc<ReservedValue<RootLinkV1>>>,
    rows_fee: MemoryReservation,
    carrier: MemoryReservation,
    memory: MemoryBudget,
    class: MemoryClass,
}
impl DirectoryBuilderV1 {
    fn new(memory: &MemoryBudget, class: MemoryClass) -> Result<Self, ValueTreeError> {
        Ok(Self {
            rows: Vec::new(),
            rows_fee: memory.reserve(0, class)?,
            carrier: memory.reserve(arc_layout::<DirectoryBackingV1>()?, class)?,
            memory: memory.clone(),
            class,
        })
    }
    fn with_parent(
        parent: &IndustrialWorkDirectoryV1,
        memory: &MemoryBudget,
        class: MemoryClass,
        scan: &mut StateScanBudget,
        cancelled: &mut impl FnMut() -> bool,
    ) -> Result<Self, ValueTreeError> {
        parent.check_budget(memory, class)?;
        let mut out = Self::new(memory, class)?;
        out.ensure_capacity(parent.data.rows.len(), scan, cancelled)?;
        for row in &parent.data.rows {
            spend(
                scan,
                cancelled,
                1,
                size_of::<Arc<ReservedValue<RootLinkV1>>>() as u64,
            )?;
            out.rows.push(row.clone());
        }
        Ok(out)
    }
    fn ensure_capacity(
        &mut self,
        needed: usize,
        scan: &mut StateScanBudget,
        cancelled: &mut impl FnMut() -> bool,
    ) -> Result<(), ValueTreeError> {
        if needed <= self.rows.capacity() {
            return Ok(());
        }
        let cap = self
            .rows
            .capacity()
            .checked_mul(2)
            .ok_or_else(limit)?
            .max(4)
            .max(needed);
        let bytes = cap
            .checked_mul(size_of::<Arc<ReservedValue<RootLinkV1>>>())
            .ok_or_else(limit)?;
        spend(
            scan,
            cancelled,
            1,
            u64::try_from(bytes).map_err(|_| limit())?,
        )?;
        let fee = self.memory.reserve(bytes, self.class)?;
        self.rows
            .try_reserve_exact(cap - self.rows.len())
            .map_err(|_| limit())?;
        if self.rows.capacity() > cap || self.rows.capacity() < needed {
            return Err(invalid());
        }
        self.rows_fee = fee;
        Ok(())
    }
    fn find(
        &self,
        outer: &str,
        scan: &mut StateScanBudget,
        cancelled: &mut impl FnMut() -> bool,
    ) -> Result<Result<usize, usize>, ValueTreeError> {
        let mut at = 0usize;
        while at < self.rows.len() {
            let row = &self.rows[at];
            let bytes = row
                .outer_key
                .len()
                .checked_add(outer.len())
                .ok_or_else(limit)?;
            spend(
                scan,
                cancelled,
                1,
                u64::try_from(bytes).map_err(|_| limit())?,
            )?;
            match row.outer_key.as_str().cmp(outer) {
                std::cmp::Ordering::Less => at += 1,
                std::cmp::Ordering::Equal => return Ok(Ok(at)),
                std::cmp::Ordering::Greater => return Ok(Err(at)),
            }
        }
        Ok(Err(at))
    }
    fn remove(
        &mut self,
        outer: &str,
        scan: &mut StateScanBudget,
        cancelled: &mut impl FnMut() -> bool,
    ) -> Result<(), ValueTreeError> {
        let Ok(at) = self.find(outer, scan, cancelled)? else {
            return Err(invalid());
        };
        let moved = self.rows.len().saturating_sub(at + 1);
        spend(
            scan,
            cancelled,
            1,
            u64::try_from(
                moved
                    .checked_mul(size_of::<Arc<ReservedValue<RootLinkV1>>>())
                    .ok_or_else(limit)?,
            )
            .map_err(|_| limit())?,
        )?;
        self.rows.remove(at);
        Ok(())
    }
    fn push_copy(
        &mut self,
        outer: &str,
        root_ref: &IndustrialRefV1,
        started_job_id: Option<u64>,
    ) -> Result<(), ValueTreeError> {
        self.ensure_capacity_unmetered(self.rows.len().checked_add(1).ok_or_else(limit)?)?;
        let text_bytes = outer
            .len()
            .checked_add(industrial_ref_text_bytes(root_ref)?)
            .ok_or_else(limit)?;
        let row_bytes = arc_layout::<RootLinkV1>()?
            .checked_add(text_bytes)
            .ok_or_else(limit)?;
        let fee = self.memory.reserve(row_bytes, self.class)?;
        let outer_key = copy_text(outer)?;
        let root_ref = IndustrialRefV1 {
            world_binding: copy_text(&root_ref.world_binding)?,
            domain: copy_text(&root_ref.domain)?,
            schema: copy_text(&root_ref.schema)?,
            id: copy_text(&root_ref.id)?,
            revision: root_ref.revision,
            digest: copy_text(&root_ref.digest)?,
        };
        let row = fee.retain(RootLinkV1 {
            outer_key,
            root_ref,
            started_job_id,
        });
        if root_link_text_bytes(&row)? != text_bytes {
            return Err(invalid());
        }
        if self
            .rows
            .last()
            .is_some_and(|previous| previous.outer_key.as_str() >= outer)
        {
            return Err(invalid());
        }
        self.rows.push(Arc::new(row));
        Ok(())
    }
    fn ensure_capacity_unmetered(&mut self, needed: usize) -> Result<(), ValueTreeError> {
        if needed <= self.rows.capacity() {
            return Ok(());
        }
        let cap = self
            .rows
            .capacity()
            .checked_mul(2)
            .ok_or_else(limit)?
            .max(4)
            .max(needed);
        let bytes = cap
            .checked_mul(size_of::<Arc<ReservedValue<RootLinkV1>>>())
            .ok_or_else(limit)?;
        let fee = self.memory.reserve(bytes, self.class)?;
        self.rows
            .try_reserve_exact(cap - self.rows.len())
            .map_err(|_| limit())?;
        if self.rows.capacity() > cap || self.rows.capacity() < needed {
            return Err(invalid());
        }
        self.rows_fee = fee;
        Ok(())
    }
    fn insert_decoded(
        &mut self,
        outer: &str,
        root_ref: &IndustrialRefV1,
        started_job_id: Option<u64>,
        scan: &mut StateScanBudget,
        cancelled: &mut impl FnMut() -> bool,
    ) -> Result<(), ValueTreeError> {
        let text_bytes = outer
            .len()
            .checked_add(industrial_ref_text_bytes(root_ref)?)
            .ok_or_else(limit)?;
        spend(
            scan,
            cancelled,
            1,
            u64::try_from(text_bytes).map_err(|_| limit())?,
        )?;
        let position = self.find(outer, scan, cancelled)?;
        if position.is_ok() {
            return Err(invalid());
        }
        self.ensure_capacity(
            self.rows.len().checked_add(1).ok_or_else(limit)?,
            scan,
            cancelled,
        )?;
        let row_bytes = arc_layout::<RootLinkV1>()?
            .checked_add(text_bytes)
            .ok_or_else(limit)?;
        let fee = self.memory.reserve(row_bytes, self.class)?;
        let outer_key = copy_text(outer)?;
        let root_ref = IndustrialRefV1 {
            world_binding: copy_text(&root_ref.world_binding)?,
            domain: copy_text(&root_ref.domain)?,
            schema: copy_text(&root_ref.schema)?,
            id: copy_text(&root_ref.id)?,
            revision: root_ref.revision,
            digest: copy_text(&root_ref.digest)?,
        };
        let row = fee.retain(RootLinkV1 {
            outer_key,
            root_ref,
            started_job_id,
        });
        if root_link_text_bytes(&row)? != text_bytes {
            return Err(invalid());
        }
        let at = match position {
            Ok(_) => return Err(invalid()),
            Err(at) => at,
        };
        let moved = self.rows.len().saturating_sub(at);
        spend(
            scan,
            cancelled,
            1,
            u64::try_from(
                moved
                    .checked_mul(size_of::<Arc<ReservedValue<RootLinkV1>>>())
                    .ok_or_else(limit)?,
            )
            .map_err(|_| limit())?,
        )?;
        self.rows.insert(at, Arc::new(row));
        Ok(())
    }
    fn finish(self) -> IndustrialWorkDirectoryV1 {
        let Self {
            rows,
            rows_fee,
            carrier,
            memory,
            class,
        } = self;
        IndustrialWorkDirectoryV1 {
            data: Arc::new(carrier.retain(DirectoryBackingV1 {
                rows,
                _rows_fee: rows_fee,
            })),
            memory,
            class,
        }
    }
}
fn copy_text(source: &str) -> Result<String, ValueTreeError> {
    let mut text = String::new();
    text.try_reserve_exact(source.len()).map_err(|_| limit())?;
    if text.capacity() > source.len() {
        return Err(invalid());
    }
    text.push_str(source);
    Ok(text)
}

fn profile() -> TypedRecordProfileV1 {
    WorldRuleCompositeBranchV1::IndustrialWork.profile()
}
fn read_roots_map<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    body: Option<&StateRootV1>,
    d: &mut ChangeDecoder<'_, '_, M, R, E, C>,
) -> Result<Option<ReservedValue<StateRootV1>>, ValueTreeError> {
    let Some(body) = body else { return Ok(None) };
    let max_tree_heap = d.work.max_tree_heap;
    let row = point_typed_column_v1(&profile(), body, "roots", d, max_tree_heap)?;
    let ColumnCellV1::Root(root) = &row.cell else {
        return Err(invalid());
    };
    Ok(Some(
        d.ctx
            .values
            .memory
            .reserve(512, d.ctx.values.class)?
            .retain(root.clone()),
    ))
}
fn root_link_from_body<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    body: &StateRootV1,
    d: &mut ChangeDecoder<'_, '_, M, R, E, C>,
) -> Result<(RecoveredTypedRecordV1<IndustrialRefV1>, Option<u64>), ValueTreeError> {
    let p = profile();
    let max_tree_heap = d.work.max_tree_heap;
    let root_ref_body = point_typed_column_v1(&p, body, "root_ref", d, max_tree_heap)?;
    let ColumnCellV1::Root(root_ref_body) = &root_ref_body.cell else {
        return Err(invalid());
    };
    let root_ref = recover_typed_record_body_v1::<IndustrialRefV1, _, _, _, _>(
        root_ref_body,
        &p,
        d.ctx,
        d.scan,
        d.work,
    )?;
    let max_tree_heap = d.work.max_tree_heap;
    let started = point_typed_column_v1(&p, body, "started_job_id", d, max_tree_heap)?;
    let started_job_id = match started.cell {
        ColumnCellV1::Unit => None,
        ColumnCellV1::U64(value) => Some(value),
        _ => return Err(invalid()),
    };
    // Keep the typed recovery and its exact reservation alive until the
    // directory row has been separately reserved and copied.
    Ok((root_ref, started_job_id))
}

fn index_changes<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    builder: &mut DirectoryBuilderV1,
    before: Option<&StateRootV1>,
    after: Option<&StateRootV1>,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut TypedRecordReadWorkV1,
) -> Result<(), ValueTreeError> {
    let p = profile();
    let trees = work.max_retained_trees;
    let mut d = ChangeDecoder {
        ctx,
        scan,
        work,
        profile: p,
        trees_remaining: trees,
        tree_depth: 0,
    };
    let old = read_roots_map(before, &mut d)?;
    let new = read_roots_map(after, &mut d)?;
    let _empty_fee = d.ctx.values.memory.reserve(512, d.ctx.values.class)?;
    let empty = StateRootV1::empty(StateNamespaceV1::Authorization).map_err(|_| invalid())?;
    visit_entries(
        old.as_deref().unwrap_or(&empty),
        new.as_deref().unwrap_or(&empty),
        &mut d,
        |old, new, d| {
            let cell = new.or(old).ok_or_else(invalid)?;
            let TypedCellRefV1::Entry(entry) = cell else {
                return Err(invalid());
            };
            let parsed =
                WorldFieldKeyV1::from_bytes(&entry.key).map_err(ValueTreeError::Invalid)?;
            if parsed.kind == WorldFieldRecordKindV1::Metadata {
                let expected =
                    WorldFieldKeyV1::new(p.field, WorldFieldRecordKindV1::Metadata, None)
                        .map_err(ValueTreeError::Invalid)?;
                for edge in [old, new].into_iter().flatten() {
                    let TypedCellRefV1::Entry(entry) = edge else {
                        return Err(invalid());
                    };
                    let binding =
                        WorldFieldKeyV1::from_bytes(&entry.key).map_err(ValueTreeError::Invalid)?;
                    if binding.semantic_key.is_some()
                        || entry.key != expected.as_bytes()
                        || entry.namespace != p.namespace()?
                    {
                        return Err(invalid());
                    }
                    d.ctx.records().presence(&entry.value, &expected, 0)?;
                }
                return d.check();
            }
            let map_key = key(&p, cell, d)?;
            let outer = match &*map_key {
                ColumnKeyV1::Text(name) if name == "_length" => return Ok(()),
                ColumnKeyV1::Pair(kind, value) if matches!(&**kind, ColumnKeyV1::Text(name) if name == "entry") =>
                {
                    let ColumnKeyV1::Text(outer) = &**value else {
                        return Err(invalid());
                    };
                    outer
                }
                _ => return Err(invalid()),
            };
            if old.is_some() {
                builder.remove(outer, d.scan, &mut d.ctx.cancelled)?;
            }
            if let Some(new) = new {
                let body = root(&p, new, d)?;
                let (root_ref, started_job_id) = root_link_from_body(&body, d)?;
                builder.insert_decoded(
                    outer,
                    root_ref.value(),
                    started_job_id,
                    d.scan,
                    &mut d.ctx.cancelled,
                )?;
            }
            d.check()
        },
    )
}

/// Update the resident link directory from the authenticated branch delta.
/// Bootstrap traverses the original typed roots map once; transitions visit
/// only the root-map closure changed in the accepted typed branch.
pub(super) fn update<
    T: CompositeBranchCodecV1,
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    parent: Option<&IndustrialWorkDirectoryV1>,
    before: Option<&StateRootV1>,
    after: Option<&StateRootV1>,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut TypedRecordReadWorkV1,
) -> Result<Option<IndustrialWorkDirectoryV1>, ValueTreeError> {
    if T::BRANCH != WorldRuleCompositeBranchV1::IndustrialWork {
        return Ok(None);
    }
    if before == after {
        if let Some(parent) = parent {
            return Ok(Some(parent.share(ctx.values.memory, ctx.values.class)?));
        }
    }
    if before.is_some() && parent.is_none() {
        return Err(invalid());
    }
    let mut builder = if let Some(parent) = parent {
        DirectoryBuilderV1::with_parent(
            parent,
            ctx.values.memory,
            ctx.values.class,
            scan,
            &mut ctx.cancelled,
        )?
    } else {
        DirectoryBuilderV1::new(ctx.values.memory, ctx.values.class)?
    };
    index_changes(&mut builder, before, after, ctx, scan, work)?;
    Ok(Some(builder.finish()))
}

/// Current branch qualification, preserving the absent-branch versus
/// present-empty distinction while checking the actual changed closure.
pub struct CurrentIndustrialWorkV1 {
    parent: [u8; 32],
    body: Option<ReservedValue<StateRootV1>>,
    present: bool,
    directory: IndustrialWorkDirectoryV1,
    identity: PreparedGuard,
    memory: MemoryBudget,
}
impl CurrentIndustrialWorkV1 {
    pub fn branch_present(&self) -> bool {
        self.present
    }
    pub fn directory(&self) -> &IndustrialWorkDirectoryV1 {
        &self.directory
    }
    pub fn body(&self) -> Option<&StateRootV1> {
        self.body.as_deref()
    }
}

fn read_current_body<M, R, E, C>(
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
) -> Result<Option<ReservedValue<StateRootV1>>, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let mut records = InventoryValueReader {
        values: &mut context.values,
        key_work: &mut *context.key_work,
        semantic_bytes: context.semantic_bytes,
        record_bytes: context.record_bytes,
        cancelled: &mut context.cancelled,
    };
    let (field, world_rules_present) = crate::incremental_world_rule_parent::read_parent_overlay(
        context.transaction,
        &mut records,
        &mut context.take_state_error,
    )?;
    let (key, semantic_bytes) = records.key(
        field,
        WorldFieldRecordKindV1::Entry,
        &(WorldRuleCompositeBranchV1::IndustrialWork.name(), ()),
    )?;
    let id = lookup(
        context.transaction,
        &key,
        &mut records,
        &mut context.take_state_error,
    )?;
    match (world_rules_present, id) {
        (false, None) => Ok(None),
        (true, Some(id)) => Ok(Some(read_body::<IndustrialWorkStateV1, _, _>(
            &key,
            semantic_bytes,
            &id,
            &mut records,
        )?)),
        _ => Err(invalid().into()),
    }
}

#[allow(clippy::too_many_arguments)]
pub fn bind_current_industrial_work_directory_v1<M, S, R, E, C>(
    parent: &ReservedValue<WorldRuleCompositeQualificationV1<IndustrialWorkStateV1>>,
    roots: &RootSetV1,
    state: &S,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut TypedRecordReadWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<ReservedValue<CurrentIndustrialWorkV1>, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    S: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let result = (|| {
        context.transaction.validate_guard(guard, live)?;
        context.transaction.validate_transaction_identity(guard)?;
        parent.check_budget(context.values.memory, MemoryClass::Ordinary)?;
        if context.values.class != MemoryClass::Ordinary
            || !context.transaction.uses_budget(context.values.memory)
            || !parent.uses_budget(context.values.memory)
            || parent.reserved_bytes() < 512
        {
            return Err(invalid().into());
        }
        roots.validate().map_err(|_| invalid())?;
        parent.validate_binding(roots)?;
        let digest = roots.digest().map_err(|_| invalid())?;
        if context.transaction.parent_fence().parent_roots != digest {
            return Err(invalid().into());
        }
        let fee = context
            .values
            .memory
            .reserve(
                size_of::<CurrentIndustrialWorkV1>() + 512,
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        let body = read_current_body(context)?;
        let empty_fee = context
            .values
            .memory
            .reserve(512, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let empty = StateRootV1::empty(StateNamespaceV1::Authorization).map_err(|_| invalid())?;
        let anchor = body.as_deref().or_else(|| parent.body()).unwrap_or(&empty);
        let trees = work.max_retained_trees;
        {
            let mut read = WorldFieldReadContextV1 {
                root: anchor,
                state,
                values: ValueReadContext {
                    source: context.values.source,
                    memory: context.values.memory,
                    class: context.values.class,
                    io: &mut *context.values.io,
                },
                key_work: &mut *context.key_work,
                semantic_bytes: context.semantic_bytes,
                record_bytes: context.record_bytes,
                take_state_error: &mut context.take_state_error,
                cancelled: &mut context.cancelled,
            };
            accept_body::<IndustrialWorkStateV1, _, _, _, _>(
                &profile(),
                parent.body(),
                body.as_deref(),
                &mut ChangeDecoder {
                    ctx: &mut read,
                    scan,
                    work,
                    profile: profile(),
                    trees_remaining: trees,
                    tree_depth: 0,
                },
            )?;
            read.check()?;
        }
        let parent_directory = parent.industrial_work_directory().ok_or_else(invalid)?;
        let directory = update::<IndustrialWorkStateV1, _, _, _, _>(
            Some(parent_directory),
            parent.body(),
            body.as_deref(),
            &mut WorldFieldReadContextV1 {
                root: anchor,
                state,
                values: ValueReadContext {
                    source: context.values.source,
                    memory: context.values.memory,
                    class: context.values.class,
                    io: &mut *context.values.io,
                },
                key_work: &mut *context.key_work,
                semantic_bytes: context.semantic_bytes,
                record_bytes: context.record_bytes,
                take_state_error: &mut context.take_state_error,
                cancelled: &mut context.cancelled,
            },
            scan,
            work,
        )?
        .ok_or_else(invalid)?;
        context.transaction.validate_guard(guard, live)?;
        drop(empty_fee);
        Ok(fee.retain(CurrentIndustrialWorkV1 {
            parent: digest,
            present: body.is_some(),
            body,
            directory,
            identity: context.transaction.guard(),
            memory: context.values.memory.clone(),
        }))
    })();
    if result.is_err() {
        let _: Result<(), ExecutionError> = context
            .transaction
            .contribute(|_| Err(ExecutionError::Aborted));
    }
    result
}

fn validate_current<M, R, E, C>(
    source: &ReservedValue<CurrentIndustrialWorkV1>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<(), WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    context.transaction.validate_guard(guard, live)?;
    context
        .transaction
        .validate_transaction_identity(&source.identity)?;
    if !source.uses_budget(context.values.memory)
        || source.reserved_bytes() < size_of::<CurrentIndustrialWorkV1>()
        || !source.memory.same_pool(context.values.memory)
        || context.values.class != MemoryClass::Ordinary
        || !context.transaction.uses_budget(context.values.memory)
        || source.parent != context.transaction.parent_fence().parent_roots
    {
        return Err(invalid().into());
    }
    let actual = read_current_body(context)?;
    if actual.as_deref() != source.body.as_deref() {
        return Err(invalid().into());
    }
    source
        .directory
        .check_budget(context.values.memory, MemoryClass::Ordinary)?;
    context.transaction.validate_guard(guard, live)?;
    Ok(())
}

fn read_current_map_column<M, S, R, E, C>(
    body: &StateRootV1,
    name: &str,
    state: &S,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut TypedRecordReadWorkV1,
) -> Result<ReservedValue<StateRootV1>, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    S: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let p = profile();
    let fee = context
        .values
        .memory
        .reserve(512, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let trees = work.max_retained_trees;
    let max_heap = work.max_tree_heap;
    let mut read = WorldFieldReadContextV1 {
        root: body,
        state,
        values: ValueReadContext {
            source: context.values.source,
            memory: context.values.memory,
            class: context.values.class,
            io: &mut *context.values.io,
        },
        key_work: &mut *context.key_work,
        semantic_bytes: context.semantic_bytes,
        record_bytes: context.record_bytes,
        take_state_error: &mut context.take_state_error,
        cancelled: &mut context.cancelled,
    };
    let mut d = ChangeDecoder {
        ctx: &mut read,
        scan,
        work,
        profile: p,
        trees_remaining: trees,
        tree_depth: 0,
    };
    let row = point_typed_column_v1(&p, body, name, &mut d, max_heap)?;
    let ColumnCellV1::Root(root) = &row.cell else {
        return Err(invalid().into());
    };
    d.check()?;
    Ok(fee.retain(root.clone()))
}

fn read_current_map_entry<M, R, E, C>(
    map: &StateRootV1,
    outer_id: &str,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    labels: crate::incremental_record_tree::RecordTreeLabelsV1,
    max_heap: usize,
) -> Result<
    Option<ReservedValue<crate::incremental_record_columns_read::ColumnRowV1>>,
    WorldFieldOverlayErrorV1,
>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let mut records = InventoryValueReader {
        values: &mut context.values,
        key_work: &mut *context.key_work,
        semantic_bytes: context.semantic_bytes,
        record_bytes: context.record_bytes,
        cancelled: &mut context.cancelled,
    };
    let p = profile();
    let (key, _) = records.key(p.field, WorldFieldRecordKindV1::Entry, &("entry", outer_id))?;
    let Some(value) = crate::incremental_record_tree::lookup(map, &key, &mut records, labels)?
    else {
        return Ok(None);
    };
    let key_len = key.as_bytes().len();
    let bytes = key_len
        .checked_add(size_of::<StateEntryV1>())
        .ok_or_else(invalid)?;
    let _entry_fee = records
        .values
        .memory
        .reserve(bytes, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    Ok(Some(
        crate::incremental_record_columns_read::read_column_row_v1(
            ColumnReadProfileV1 {
                field: p.field,
                schema: p.schema,
                groups: &[],
                reference_schema: p.reference_schema,
            },
            &StateEntryV1 {
                namespace: map.namespace,
                key: key.as_bytes().to_vec(),
                value,
            },
            &mut records,
            max_heap,
        )?,
    ))
}

/// Read the full selected root with the original closed RootOperation codec.
#[allow(clippy::too_many_arguments)]
pub fn read_current_industrial_completion_root_v1<M, S, R, E, C>(
    source: &ReservedValue<CurrentIndustrialWorkV1>,
    outer_root_id: &str,
    state: &S,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut TypedRecordReadWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<Option<RecoveredTypedRecordV1<RootOperationV1>>, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    S: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let result = (|| {
        validate_current(source, context, guard, live)?;
        let Some(body) = source.body.as_deref() else {
            return Ok(None);
        };
        let p = profile();
        let roots = read_current_map_column(body, "roots", state, context, scan, work)?;
        let max_heap = work.max_tree_heap;
        let Some(row) = read_current_map_entry(&roots, outer_root_id, context, p.labels, max_heap)?
        else {
            return Ok(None);
        };
        let ColumnCellV1::Root(root) = &row.cell else {
            return Err(invalid().into());
        };
        let mut read = WorldFieldReadContextV1 {
            root,
            state,
            values: ValueReadContext {
                source: context.values.source,
                memory: context.values.memory,
                class: context.values.class,
                io: &mut *context.values.io,
            },
            key_work: &mut *context.key_work,
            semantic_bytes: context.semantic_bytes,
            record_bytes: context.record_bytes,
            take_state_error: &mut context.take_state_error,
            cancelled: &mut context.cancelled,
        };
        let recovered = recover_typed_record_body_v1::<RootOperationV1, _, _, _, _>(
            root, &p, &mut read, scan, work,
        )?;
        read.check()?;
        context.transaction.validate_guard(guard, live)?;
        Ok(Some(recovered))
    })();
    if result.is_err() {
        let _: Result<(), ExecutionError> = context
            .transaction
            .contribute(|_| Err(ExecutionError::Aborted));
    }
    result
}

/// Observe the actual top-level `IndustrialWorkState.dispositions` map. The
/// result does not inspect RootOperation.allowed_dispositions.
#[allow(clippy::too_many_arguments)]
pub fn current_industrial_disposition_present_v1<M, S, R, E, C>(
    source: &ReservedValue<CurrentIndustrialWorkV1>,
    outer_root_id: &str,
    state: &S,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut TypedRecordReadWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<bool, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    S: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let result = (|| {
        validate_current(source, context, guard, live)?;
        let Some(body) = source.body.as_deref() else {
            return Ok(false);
        };
        let p = profile();
        let dispositions =
            read_current_map_column(body, "dispositions", state, context, scan, work)?;
        let mut records = InventoryValueReader {
            values: &mut context.values,
            key_work: &mut *context.key_work,
            semantic_bytes: context.semantic_bytes,
            record_bytes: context.record_bytes,
            cancelled: &mut context.cancelled,
        };
        let (key, _) = records.key(
            p.field,
            WorldFieldRecordKindV1::Entry,
            &("entry", outer_root_id),
        )?;
        let present =
            crate::incremental_record_tree::lookup(&dispositions, &key, &mut records, p.labels)?
                .is_some();
        drop(records);
        context.transaction.validate_guard(guard, live)?;
        Ok(present)
    })();
    if result.is_err() {
        let _: Result<(), ExecutionError> = context
            .transaction
            .contribute(|_| Err(ExecutionError::Aborted));
    }
    result
}
