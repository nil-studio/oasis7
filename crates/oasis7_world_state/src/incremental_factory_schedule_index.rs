//! Complete pending-build identity projection, DATA rather than build permission.
//!
//! Cold admission authenticates the original integer outer keys once. Normal
//! observations merge only the transaction's actual changed build children.
use crate::{incremental_factory_records::*, incremental_field_read::WorldFieldReadContextV1};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFence, PreparedGuard, RootSetV1},
    incremental_state::{MerkleNodeReader, StateNamespaceV1, StateRootV1, StateScanBudget},
    incremental_value::{ValueNodeReader, ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation, ReservedValue};
use std::mem::size_of;

pub struct FactoryScheduleIndexWorkV1 {
    pub cells_remaining: u64,
    pub comparison_bytes_remaining: u64,
    pub max_builds: usize,
    pub max_factory_id_bytes: usize,
    pub max_pending_changes: usize,
}
impl FactoryScheduleIndexWorkV1 {
    fn charge(&mut self, bytes: usize) -> Result<(), ValueTreeError> {
        self.cells_remaining = self
            .cells_remaining
            .checked_sub(1)
            .ok_or(ValueTreeError::ResourceLimited("factory schedule cells"))?;
        self.comparison_bytes_remaining = self
            .comparison_bytes_remaining
            .checked_sub(u64::try_from(bytes).map_err(|_| invalid())?)
            .ok_or(ValueTreeError::ResourceLimited(
                "factory schedule comparisons",
            ))?;
        Ok(())
    }
}
struct BuildProjection {
    id: u64,
    value: [u8; 32],
    child: StateRootV1,
    factory: String,
    class: MemoryClass,
}
pub struct FactoryScheduleIndexV1 {
    roots: [u8; 32],
    counts: [u64; 2],
    class: MemoryClass,
    rows: Vec<ReservedValue<BuildProjection>>,
    _slots: MemoryReservation,
}
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("factory schedule index binding")
}
fn vector_bytes<T>(n: usize) -> Result<usize, ValueTreeError> {
    n.checked_mul(size_of::<T>()).ok_or_else(invalid)
}
fn binding(
    q: &ReservedValue<FactoryFamilyQualificationV1>,
    roots: &RootSetV1,
    memory: &MemoryBudget,
    class: MemoryClass,
) -> Result<[u8; 32], ValueTreeError> {
    if !q.uses_budget(memory)
        || q.allocation_class() != class
        || q.reserved_bytes() < size_of::<FactoryFamilyQualificationV1>()
    {
        return Err(invalid());
    }
    let _phase = memory.reserve(4096, class)?;
    let digest = roots.digest().map_err(|_| invalid())?;
    q.validate_binding(&digest)?;
    Ok(digest)
}
impl FactoryScheduleIndexV1 {
    pub fn validate_binding(
        &self,
        q: &ReservedValue<FactoryFamilyQualificationV1>,
        roots: &RootSetV1,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<(), ValueTreeError> {
        if self.roots != binding(q, roots, memory, class)?
            || self.class != class
            || self.counts != q.record_counts()
            || u64::try_from(self.rows.len()).ok() != Some(self.counts[1])
        {
            return Err(invalid());
        }
        for row in &self.rows {
            if !row.uses_budget(memory) || row.class != class {
                return Err(invalid());
            }
        }
        Ok(())
    }
    /// Borrows the original authenticated parent bindings. These observations
    /// grant no build/completion permission and never rebind Current roots.
    pub fn iter_parent_build_bindings(
        &self,
    ) -> impl ExactSizeIterator<Item = (u64, &[u8; 32], &StateRootV1)> + '_ {
        self.rows.iter().map(|row| (row.id, &row.value, &row.child))
    }
    pub fn records(&self) -> usize {
        self.rows.len()
    }
}

/// The source is the true complete Factory qualification, never caller rows.
#[allow(clippy::too_many_arguments)]
pub fn build_factory_schedule_index_v1<M, R, E, C>(
    q: &ReservedValue<FactoryFamilyQualificationV1>,
    roots: &RootSetV1,
    read: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    factory_work: &mut FactoryWorkBudgetV1,
    work: &mut FactoryScheduleIndexWorkV1,
    max_payload_bytes: u64,
) -> Result<ReservedValue<FactoryScheduleIndexV1>, ValueTreeError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    read.check()?;
    let memory = read.values.memory;
    let class = read.values.class;
    let digest = binding(q, roots, memory, class)?;
    let n = usize::try_from(q.record_counts()[1]).map_err(|_| invalid())?;
    if n > work.max_builds {
        return Err(ValueTreeError::ResourceLimited("factory schedule builds"));
    }
    let _temporary = memory.reserve(
        vector_bytes::<ReservedValue<FactoryBuildParentIdentityV1>>(n)?,
        class,
    )?;
    let mut identities = Vec::with_capacity(n);
    if identities.capacity() != n {
        return Err(invalid());
    }
    {
        let mut local = FactoryFamilyReadContextV1 {
            state: read.state,
            values: ValueReadContext {
                source: read.values.source,
                memory,
                class,
                io: &mut *read.values.io,
            },
            key_work: &mut *read.key_work,
            semantic_bytes: read.semantic_bytes,
            record_bytes: read.record_bytes,
            max_payload_bytes,
            outer_scan: scan,
            take_state_error: &mut read.take_state_error,
            cancelled: &mut read.cancelled,
        };
        visit_qualified_build_parent_identities_v1(
            roots,
            q,
            &mut local,
            factory_work,
            |identity| {
                work.charge(0)?;
                if identities.len() == n {
                    return Err(invalid());
                }
                identities.push(identity);
                Ok(())
            },
        )?;
    }
    if identities.len() != n {
        return Err(invalid());
    }
    let slots = memory.reserve(vector_bytes::<ReservedValue<BuildProjection>>(n)?, class)?;
    let owner = memory.reserve(size_of::<FactoryScheduleIndexV1>(), class)?;
    let mut rows = Vec::with_capacity(n);
    if rows.capacity() != n {
        return Err(invalid());
    }
    let mut local = WorldFieldReadContextV1 {
        root: roots.state_root(StateNamespaceV1::Scheduler),
        state: read.state,
        values: ValueReadContext {
            source: read.values.source,
            memory,
            class,
            io: &mut *read.values.io,
        },
        key_work: &mut *read.key_work,
        semantic_bytes: read.semantic_bytes,
        record_bytes: read.record_bytes,
        take_state_error: &mut read.take_state_error,
        cancelled: &mut read.cancelled,
    };
    for identity in identities {
        local.check()?;
        work.charge(0)?;
        let cap = work.max_factory_id_bytes;
        let fee = memory.reserve(
            size_of::<BuildProjection>()
                .checked_add(cap)
                .and_then(|n| n.checked_add(identity.record_root.node_root.len()))
                .ok_or_else(invalid)?,
            class,
        )?;
        let mut bytes = Vec::with_capacity(cap);
        if bytes.capacity() != cap {
            return Err(invalid());
        }
        let cell = read_factory_cell_v1(
            FactoryRecordKeyV1::Build(identity.id),
            FactoryCellV1::Column(FactoryColumnV1::SpecFactoryId),
            &mut local,
            cap as u64,
            &mut bytes,
        )?;
        let FactoryCellReadV1::Value(FactoryValueV1::Text { utf8_bytes }) = cell else {
            return Err(invalid());
        };
        if usize::try_from(utf8_bytes).ok() != Some(bytes.len()) {
            return Err(invalid());
        }
        work.charge(bytes.len())?;
        let factory = String::from_utf8(bytes).map_err(|_| invalid())?;
        let child = identity.record_root.clone();
        if child.node_root.capacity() > identity.record_root.node_root.len() {
            return Err(invalid());
        }
        rows.push(fee.retain(BuildProjection {
            id: identity.id,
            value: identity.value_root,
            child,
            factory,
            class,
        }));
    }
    local.check()?;
    Ok(owner.retain(FactoryScheduleIndexV1 {
        roots: digest,
        counts: q.record_counts(),
        class,
        rows,
        _slots: slots,
    }))
}

/// Observe the complete Current pending-build set without scanning cold rows.
/// Every parent row affected by a real outer write is replaced or removed;
/// additions are decoded from the authenticated Current child, not a DTO.
#[allow(clippy::too_many_arguments)]
pub fn any_current_pending_factory_build_v1<M, R, E, C>(
    factory_id: &str,
    index: &ReservedValue<FactoryScheduleIndexV1>,
    q: &ReservedValue<FactoryFamilyQualificationV1>,
    roots: &RootSetV1,
    context: &mut crate::incremental_field_overlay::WorldFieldOverlayContextV1<
        '_,
        '_,
        '_,
        M,
        R,
        E,
        C,
    >,
    factory_work: &mut FactoryWorkBudgetV1,
    work: &mut FactoryScheduleIndexWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<bool, crate::incremental_field_overlay::WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    context.transaction.validate_guard(guard, live)?;
    if !index.uses_budget(context.values.memory)
        || context.values.class != MemoryClass::Ordinary
        || index.class != MemoryClass::Ordinary
    {
        return Err(invalid().into());
    }
    index.validate_binding(q, roots, context.values.memory, MemoryClass::Ordinary)?;
    let max = work.max_pending_changes;
    let _phase = context
        .values
        .memory
        .reserve(
            vector_bytes::<ReservedValue<FactoryBuildCurrentOuterChangeV1>>(max)?,
            MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?;
    let mut changes = Vec::with_capacity(max);
    if changes.capacity() != max {
        return Err(invalid().into());
    }
    {
        let mut local = FactoryTransitionReadContextV1 {
            values: ValueReadContext {
                source: context.values.source,
                memory: context.values.memory,
                class: MemoryClass::Ordinary,
                io: &mut *context.values.io,
            },
            key_work: &mut *context.key_work,
            semantic_bytes: context.semantic_bytes,
            record_bytes: context.record_bytes,
            max_payload_bytes: work.max_factory_id_bytes as u64,
            cancelled: &mut context.cancelled,
        };
        visit_build_current_outer_changes_v1(
            context.transaction,
            q,
            &mut local,
            factory_work,
            |change| {
                work.charge(0)?;
                if changes.len() == max {
                    return Err(ValueTreeError::ResourceLimited(
                        "factory schedule pending changes",
                    )
                    .into());
                }
                changes.push(change);
                Ok(())
            },
        )?;
    }
    let mut found = false;
    for row in &index.rows {
        if (context.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        let mut changed = false;
        for change in &changes {
            work.charge(size_of::<u64>())?;
            if let FactoryBuildCurrentOuterChangeV1::Record { id, before, .. } = &**change {
                if *id == row.id {
                    if before.as_ref().is_none_or(|old| {
                        old.value_root != row.value || old.record_root != row.child
                    }) {
                        return Err(invalid().into());
                    }
                    changed = true;
                    break;
                }
            }
        }
        if !changed {
            work.charge(
                row.factory
                    .len()
                    .checked_add(factory_id.len())
                    .ok_or_else(invalid)?,
            )?;
            found |= row.factory == factory_id;
        }
    }
    for change in &changes {
        let FactoryBuildCurrentOuterChangeV1::Record { id, before, after } = &**change else {
            continue;
        };
        if before.is_some() {
            let mut matched = false;
            for row in &index.rows {
                work.charge(size_of::<u64>())?;
                if row.id == *id {
                    matched = true;
                    break;
                }
            }
            if !matched {
                return Err(invalid().into());
            }
        }
        if after.is_none() {
            continue;
        }
        let cap = work.max_factory_id_bytes;
        let _text = context
            .values
            .memory
            .reserve(cap, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let mut bytes = Vec::with_capacity(cap);
        if bytes.capacity() != cap {
            return Err(invalid().into());
        }
        let cell = read_factory_cell_overlay_v1(
            FactoryRecordKeyV1::Build(*id),
            FactoryCellV1::Column(FactoryColumnV1::SpecFactoryId),
            context,
            cap as u64,
            &mut bytes,
        )?;
        let FactoryCellReadV1::Value(FactoryValueV1::Text { utf8_bytes }) = cell else {
            return Err(invalid().into());
        };
        if usize::try_from(utf8_bytes).ok() != Some(bytes.len()) {
            return Err(invalid().into());
        }
        let text = std::str::from_utf8(&bytes).map_err(|_| invalid())?;
        work.charge(
            text.len()
                .checked_add(factory_id.len())
                .ok_or_else(invalid)?,
        )?;
        found |= text == factory_id;
    }
    context
        .transaction
        .validate_guard(&context.transaction.guard(), live)?;
    Ok(found)
}
