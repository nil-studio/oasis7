//! Joint factory/industry relation data. It never grants action or publication authority.
use crate::{
    incremental_factory_records::FactoryFamilyQualificationV1,
    incremental_industry_jobs::IndustryFamilyQualificationV1,
};
use oasis7_formal_protocol::{incremental_execution::RootSetV1, incremental_value::ValueTreeError};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};

/// Caller-owned continuous allowances; lifecycle operations never refill them.
pub struct FactoryLifecycleIndexWorkV1 {
    pub cells_remaining: u64,
    pub comparison_bytes_remaining: u64,
    pub pending_entries_remaining: usize,
    pub max_factories: usize,
    pub max_recipe_jobs: usize,
    pub max_text_bytes: usize,
    pub max_pending_records: usize,
}
impl FactoryLifecycleIndexWorkV1 {
    pub(crate) fn cell(&mut self) -> Result<(), ValueTreeError> {
        self.cells_remaining =
            self.cells_remaining
                .checked_sub(1)
                .ok_or(ValueTreeError::ResourceLimited(
                    "factory lifecycle index cells",
                ))?;
        Ok(())
    }
    pub(crate) fn comparison(
        &mut self,
        a: &str,
        b: &str,
    ) -> Result<std::cmp::Ordering, ValueTreeError> {
        self.cell()?;
        let bytes = a
            .len()
            .checked_add(b.len())
            .and_then(|n| u64::try_from(n).ok())
            .ok_or(ValueTreeError::Invalid(
                "factory lifecycle comparison overflow",
            ))?;
        self.comparison_bytes_remaining =
            self.comparison_bytes_remaining.checked_sub(bytes).ok_or(
                ValueTreeError::ResourceLimited("factory lifecycle comparison bytes"),
            )?;
        Ok(a.cmp(b))
    }
}
/// Checks both complete qualifications before any joint index can be built.
/// A bare RootSet, count or caller DTO cannot satisfy either qualification.
fn validate_joint_parent(
    factories: &ReservedValue<FactoryFamilyQualificationV1>,
    industry: &ReservedValue<IndustryFamilyQualificationV1>,
    roots: &RootSetV1,
    memory: &MemoryBudget,
    class: MemoryClass,
) -> Result<[u8; 32], ValueTreeError> {
    if !factories.uses_budget(memory)
        || !industry.uses_budget(memory)
        || factories.allocation_class() != class
        || industry.allocation_class() != class
    {
        return Err(ValueTreeError::Invalid(
            "factory lifecycle joint pool/class",
        ));
    }
    // RootSet canonical encoding has a protocol maximum of 1024 bytes;
    // its six temporary canonical roots are bounded by STATE_ROOT_BYTES.
    let _phase = memory.reserve(4096, class)?;
    let digest = roots
        .digest()
        .map_err(|_| ValueTreeError::Invalid("factory lifecycle parent roots"))?;
    factories.validate_binding(&digest)?;
    industry.validate_binding(&digest)?;
    Ok(digest)
}

use crate::{
    incremental_factory_records::*, incremental_field_read::WorldFieldReadContextV1,
    incremental_industry_jobs::*,
};
use oasis7_formal_protocol::{
    incremental_state::{MerkleNodeReader, StateNamespaceV1, StateRootV1, StateScanBudget},
    incremental_value::{ValueNodeReader, ValueReadContext},
};
use std::sync::Arc;
type HeldFactory = Arc<ReservedValue<FactoryProjection>>;
type HeldJob = Arc<ReservedValue<JobProjection>>;
#[derive(Debug)]
struct FactoryProjection {
    id: String,
    value: [u8; 32],
    child: StateRootV1,
    completed: u64,
    stable: bool,
}
#[derive(Debug)]
struct JobProjection {
    id: u64,
    value: [u8; 32],
    child: StateRootV1,
    factory: String,
}
/// Complete joint data qualification, not permission to maintain or recycle.
#[derive(Debug)]
pub struct FactoryLifecycleIndexV1 {
    roots: [u8; 32],
    class: MemoryClass,
    factory_counts: [u64; 2],
    industry_counts: [u64; 5],
    factories: Vec<HeldFactory>,
    jobs: Vec<HeldJob>,
    by_factory: Vec<HeldJob>,
    completed_sum: u128,
    stable_count: u64,
    _vectors: oasis7_proto::memory_budget::MemoryReservation,
}
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("factory lifecycle index binding")
}
fn size(n: usize, width: usize) -> Result<usize, ValueTreeError> {
    n.checked_mul(width).ok_or_else(invalid)
}
fn count(n: u64, max: usize) -> Result<usize, ValueTreeError> {
    let n = usize::try_from(n).map_err(|_| invalid())?;
    if n > max {
        return Err(ValueTreeError::ResourceLimited(
            "factory lifecycle record limit",
        ));
    }
    Ok(n)
}
impl FactoryLifecycleIndexV1 {
    pub fn allocation_class(&self) -> MemoryClass {
        self.class
    }

    /// Pure consistency check for an already constructed joint aggregate.
    /// Pool/resource admission remains with its containing Reserved owners.
    pub(crate) fn validate_aggregate_binding(
        &self,
        factories: &ReservedValue<FactoryFamilyQualificationV1>,
        industry: &ReservedValue<IndustryFamilyQualificationV1>,
        digest: &[u8; 32],
        class: MemoryClass,
    ) -> Result<(), ValueTreeError> {
        factories.validate_binding(digest)?;
        industry.validate_binding(digest)?;
        if factories.allocation_class() != class
            || industry.allocation_class() != class
            || self.class != class
            || self.roots != *digest
            || self.factory_counts != factories.record_counts()
            || self.industry_counts != industry.record_counts()
            || u64::try_from(self.factories.len()).ok() != Some(self.factory_counts[0])
            || u64::try_from(self.jobs.len()).ok() != Some(self.industry_counts[0])
            || self.by_factory.len() != self.jobs.len()
        {
            return Err(invalid());
        }
        Ok(())
    }
    pub fn factory_records(&self) -> usize {
        self.factories.len()
    }
    pub fn recipe_records(&self) -> usize {
        self.jobs.len()
    }
    pub fn validate_binding(
        &self,
        factories: &ReservedValue<FactoryFamilyQualificationV1>,
        industry: &ReservedValue<IndustryFamilyQualificationV1>,
        roots: &RootSetV1,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<(), ValueTreeError> {
        let digest = validate_joint_parent(factories, industry, roots, memory, class)?;
        if self.roots != digest
            || self.class != class
            || self.factory_counts != factories.record_counts()
            || self.industry_counts != industry.record_counts()
            || self.factories.len()
                != usize::try_from(self.factory_counts[0]).map_err(|_| invalid())?
            || self.jobs.len() != usize::try_from(self.industry_counts[0]).map_err(|_| invalid())?
            || self.by_factory.len() != self.jobs.len()
        {
            return Err(invalid());
        }
        Ok(())
    }
}
fn sort<T>(
    items: &mut [T],
    work: &mut FactoryLifecycleIndexWorkV1,
    mut cancelled: impl FnMut() -> bool,
    mut compare: impl FnMut(
        &T,
        &T,
        &mut FactoryLifecycleIndexWorkV1,
    ) -> Result<std::cmp::Ordering, ValueTreeError>,
) -> Result<(), ValueTreeError> {
    // In-place heapsort: no hidden stable-sort scratch; every comparison and
    // move is admitted before use, including cancellation on empty/tail paths.
    fn down<T>(
        a: &mut [T],
        mut pos: usize,
        end: usize,
        work: &mut FactoryLifecycleIndexWorkV1,
        cancel: &mut impl FnMut() -> bool,
        compare: &mut impl FnMut(
            &T,
            &T,
            &mut FactoryLifecycleIndexWorkV1,
        ) -> Result<std::cmp::Ordering, ValueTreeError>,
    ) -> Result<(), ValueTreeError> {
        loop {
            if cancel() {
                return Err(ValueTreeError::Cancelled);
            }
            work.cell()?;
            let child = pos
                .checked_mul(2)
                .and_then(|n| n.checked_add(1))
                .ok_or_else(invalid)?;
            if child >= end {
                return Ok(());
            }
            let mut child = child;
            if child + 1 < end && compare(&a[child], &a[child + 1], work)?.is_lt() {
                child += 1;
            }
            if !compare(&a[pos], &a[child], work)?.is_lt() {
                return Ok(());
            }
            a.swap(pos, child);
            pos = child;
        }
    }
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    let len = items.len();
    for i in (0..len / 2).rev() {
        down(items, i, len, work, &mut cancelled, &mut compare)?;
    }
    for end in (1..len).rev() {
        work.cell()?;
        items.swap(0, end);
        down(items, 0, end, work, &mut cancelled, &mut compare)?;
    }
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(())
}

fn factory_scalar<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    id: &str,
    column: FactoryColumnV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut FactoryLifecycleIndexWorkV1,
    output: &mut impl std::io::Write,
) -> Result<FactoryValueV1, ValueTreeError> {
    work.cell()?;
    let max = match column {
        FactoryColumnV1::ProductionLastCompletedRecipeId | FactoryColumnV1::SnapshotRecipeId => {
            work.max_text_bytes as u64
        }
        _ => 8,
    };
    match read_factory_cell_v1(
        FactoryRecordKeyV1::Factory(id),
        FactoryCellV1::Column(column),
        ctx,
        max,
        output,
    )? {
        FactoryCellReadV1::Value(v) => Ok(v),
        _ => Err(invalid()),
    }
}
fn factory_u64<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    id: &str,
    column: FactoryColumnV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut FactoryLifecycleIndexWorkV1,
) -> Result<u64, ValueTreeError> {
    match factory_scalar(id, column, ctx, work, &mut std::io::sink())? {
        FactoryValueV1::Unsigned(n) => Ok(n),
        _ => Err(invalid()),
    }
}
fn factory_text<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    id: &str,
    column: FactoryColumnV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut FactoryLifecycleIndexWorkV1,
) -> Result<Option<ReservedValue<String>>, ValueTreeError> {
    let max = work.max_text_bytes;
    let fee = ctx
        .values
        .memory
        .reserve(max.checked_add(128).ok_or_else(invalid)?, ctx.values.class)?;
    let mut bytes = Vec::with_capacity(max);
    match factory_scalar(id, column, ctx, work, &mut bytes)? {
        FactoryValueV1::Unit => Ok(None),
        FactoryValueV1::Text { utf8_bytes }
            if usize::try_from(utf8_bytes).ok() == Some(bytes.len()) =>
        {
            Ok(Some(
                fee.retain(String::from_utf8(bytes).map_err(|_| invalid())?),
            ))
        }
        _ => Err(invalid()),
    }
}
fn project_factory<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    identity: &FactoryQualifiedOuterIdentityV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut FactoryLifecycleIndexWorkV1,
) -> Result<Option<HeldFactory>, ValueTreeError> {
    let FactoryQualifiedOuterIdentityV1::Record {
        id,
        value_root,
        record_root,
    } = identity
    else {
        return Ok(None);
    };
    if id.len() > work.max_text_bytes {
        return Err(ValueTreeError::ResourceLimited("factory lifecycle ID"));
    }
    let fee = ctx.values.memory.reserve(
        std::mem::size_of::<ReservedValue<FactoryProjection>>() + 128 + id.len(),
        ctx.values.class,
    )?;
    let completed = factory_u64(id, FactoryColumnV1::ProductionCompletedJobs, ctx, work)?;
    let repeat = factory_u64(
        id,
        FactoryColumnV1::ProductionSameRecipeRepeatCount,
        ctx,
        work,
    )?;
    let last = factory_text(
        id,
        FactoryColumnV1::ProductionLastCompletedRecipeId,
        ctx,
        work,
    )?;
    let present = match factory_scalar(
        id,
        FactoryColumnV1::ProductionSnapshotPresent,
        ctx,
        work,
        &mut std::io::sink(),
    )? {
        FactoryValueV1::Bool(v) => v,
        _ => return Err(invalid()),
    };
    let snapshot = if present {
        factory_text(id, FactoryColumnV1::SnapshotRecipeId, ctx, work)?
    } else {
        None
    };
    let stable = crate::state::industry_transition::factory_has_canonical_stable_line_parts_v1(
        repeat,
        snapshot.as_ref().map(|s| s.as_str()),
        last.as_ref().map(|s| s.as_str()),
    );
    ctx.check()?;
    Ok(Some(Arc::new(fee.retain(FactoryProjection {
        id: id.clone(),
        value: *value_root,
        child: record_root.clone(),
        completed,
        stable,
    }))))
}
fn project_job<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    identity: &IndustryQualifiedOuterIdentityV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut FactoryLifecycleIndexWorkV1,
) -> Result<Option<HeldJob>, ValueTreeError> {
    let IndustryQualifiedOuterIdentityV1::Record {
        id,
        value_root,
        record_root,
    } = identity
    else {
        return Ok(None);
    };
    work.cell()?;
    let fee = ctx.values.memory.reserve(
        std::mem::size_of::<ReservedValue<JobProjection>>() + 128 + work.max_text_bytes,
        ctx.values.class,
    )?;
    let mut bytes = Vec::with_capacity(work.max_text_bytes);
    let value = read_industry_cell_v1(
        IndustryRecordKeyV1 {
            kind: IndustryRecordKindV1::Recipe,
            id: *id,
        },
        IndustryCellV1::Column(IndustryColumnV1::RecordFactoryId),
        ctx,
        work.max_text_bytes as u64,
        &mut bytes,
    )?;
    let IndustryCellReadV1::Value(IndustryValueV1::Text { utf8_bytes }) = value else {
        return Err(invalid());
    };
    if usize::try_from(utf8_bytes).ok() != Some(bytes.len()) {
        return Err(invalid());
    }
    let factory = String::from_utf8(bytes).map_err(|_| invalid())?;
    ctx.check()?;
    Ok(Some(Arc::new(fee.retain(JobProjection {
        id: *id,
        value: *value_root,
        child: record_root.clone(),
        factory,
    }))))
}
/// The only cold construction entry requires both real complete qualifications.
/// It visits each parent directory once and decodes only the small original
/// projections; it never restores a World or a complete factory/job DTO.
#[allow(clippy::too_many_arguments)]
pub fn build_factory_lifecycle_index_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    factories: &ReservedValue<FactoryFamilyQualificationV1>,
    industry: &ReservedValue<IndustryFamilyQualificationV1>,
    roots: &RootSetV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    outer_scan: &mut StateScanBudget,
    factory_work: &mut FactoryWorkBudgetV1,
    industry_work: &mut IndustryWorkBudgetV1,
    work: &mut FactoryLifecycleIndexWorkV1,
    max_payload_bytes: u64,
) -> Result<ReservedValue<FactoryLifecycleIndexV1>, ValueTreeError> {
    ctx.check()?;
    let memory = ctx.values.memory;
    let class = ctx.values.class;
    let digest = validate_joint_parent(factories, industry, roots, memory, class)?;
    let nf = count(factories.record_counts()[0], work.max_factories)?;
    let nj = count(industry.record_counts()[0], work.max_recipe_jobs)?;
    let slots = size(nf, std::mem::size_of::<HeldFactory>())?
        .checked_add(size(nj, 2 * std::mem::size_of::<HeldJob>())?)
        .ok_or_else(invalid)?;
    let vectors = memory.reserve(slots, class)?;
    let owner = memory.reserve(std::mem::size_of::<FactoryLifecycleIndexV1>(), class)?;
    let temporary = memory.reserve(
        size(
            nf.checked_add(1).ok_or_else(invalid)?,
            std::mem::size_of::<ReservedValue<FactoryQualifiedOuterIdentityV1>>(),
        )?
        .checked_add(size(
            nj.checked_add(1).ok_or_else(invalid)?,
            std::mem::size_of::<ReservedValue<IndustryQualifiedOuterIdentityV1>>(),
        )?)
        .ok_or_else(invalid)?,
        class,
    )?;
    let mut fs = Vec::with_capacity(nf.checked_add(1).ok_or_else(invalid)?);
    let mut js = Vec::with_capacity(nj.checked_add(1).ok_or_else(invalid)?);
    {
        let mut local = FactoryFamilyReadContextV1 {
            state: ctx.state,
            values: ValueReadContext {
                source: ctx.values.source,
                memory,
                class,
                io: &mut *ctx.values.io,
            },
            key_work: &mut *ctx.key_work,
            semantic_bytes: ctx.semantic_bytes,
            record_bytes: ctx.record_bytes,
            max_payload_bytes,
            outer_scan: &mut *outer_scan,
            take_state_error: &mut ctx.take_state_error,
            cancelled: &mut ctx.cancelled,
        };
        visit_qualified_factory_parent_identities_v1(
            roots,
            factories,
            &mut local,
            factory_work,
            |id| {
                work.cell()?;
                if fs.len() == fs.capacity() {
                    return Err(invalid());
                }
                fs.push(id);
                Ok(())
            },
        )?;
    }
    {
        let mut local = IndustryFamilyReadContextV1 {
            state: ctx.state,
            values: ValueReadContext {
                source: ctx.values.source,
                memory,
                class,
                io: &mut *ctx.values.io,
            },
            key_work: &mut *ctx.key_work,
            semantic_bytes: ctx.semantic_bytes,
            record_bytes: ctx.record_bytes,
            max_payload_bytes,
            outer_scan: &mut *outer_scan,
            take_state_error: &mut ctx.take_state_error,
            cancelled: &mut ctx.cancelled,
        };
        visit_qualified_recipe_parent_identities_v1(
            roots,
            industry,
            &mut local,
            industry_work,
            |id| {
                work.cell()?;
                if js.len() == js.capacity() {
                    return Err(invalid());
                }
                js.push(id);
                Ok(())
            },
        )?;
    }
    let mut projected_factories = Vec::with_capacity(nf);
    let mut projected_jobs = Vec::with_capacity(nj);
    {
        let mut local = WorldFieldReadContextV1 {
            root: roots.state_root(StateNamespaceV1::Business),
            state: ctx.state,
            values: ValueReadContext {
                source: ctx.values.source,
                memory,
                class,
                io: &mut *ctx.values.io,
            },
            key_work: &mut *ctx.key_work,
            semantic_bytes: ctx.semantic_bytes,
            record_bytes: ctx.record_bytes,
            take_state_error: &mut ctx.take_state_error,
            cancelled: &mut ctx.cancelled,
        };
        for row in &fs {
            local.check()?;
            if let Some(row) = project_factory(row, &mut local, work)? {
                projected_factories.push(row);
            }
        }
    }
    {
        let mut local = WorldFieldReadContextV1 {
            root: roots.state_root(StateNamespaceV1::Scheduler),
            state: ctx.state,
            values: ValueReadContext {
                source: ctx.values.source,
                memory,
                class,
                io: &mut *ctx.values.io,
            },
            key_work: &mut *ctx.key_work,
            semantic_bytes: ctx.semantic_bytes,
            record_bytes: ctx.record_bytes,
            take_state_error: &mut ctx.take_state_error,
            cancelled: &mut ctx.cancelled,
        };
        for row in &js {
            local.check()?;
            if let Some(row) = project_job(row, &mut local, work)? {
                projected_jobs.push(row);
            }
        }
    }
    if projected_factories.len() != nf || projected_jobs.len() != nj {
        return Err(invalid());
    }
    sort(
        &mut projected_factories,
        work,
        &mut ctx.cancelled,
        |a, b, w| w.comparison(&a.id, &b.id),
    )?;
    sort(&mut projected_jobs, work, &mut ctx.cancelled, |a, b, w| {
        w.cell()?;
        Ok(a.id.cmp(&b.id))
    })?;
    let mut by_factory = Vec::with_capacity(nj);
    for job in &projected_jobs {
        ctx.check()?;
        work.cell()?;
        by_factory.push(job.clone());
    }
    sort(&mut by_factory, work, &mut ctx.cancelled, |a, b, w| {
        Ok(w.comparison(&a.factory, &b.factory)?.then(a.id.cmp(&b.id)))
    })?;
    let mut completed_sum = 0u128;
    let mut stable_count = 0u64;
    for row in &projected_factories {
        ctx.check()?;
        work.cell()?;
        completed_sum = completed_sum
            .checked_add(u128::from(row.completed))
            .ok_or_else(invalid)?;
        stable_count = stable_count
            .checked_add(u64::from(row.stable))
            .ok_or_else(invalid)?;
    }
    drop(temporary);
    ctx.check()?;
    Ok(owner.retain(FactoryLifecycleIndexV1 {
        roots: digest,
        class,
        factory_counts: factories.record_counts(),
        industry_counts: industry.record_counts(),
        factories: projected_factories,
        jobs: projected_jobs,
        by_factory,
        completed_sum,
        stable_count,
        _vectors: vectors,
    }))
}

/// Re-admits every cached allocation, requiring both genuinely readmitted
/// Ordinary family qualifications at the unchanged roots. No Recovery Arc is
/// retained by the result.
#[allow(clippy::too_many_arguments)]
pub fn readmit_factory_lifecycle_index_ordinary_v1(
    source: &ReservedValue<FactoryLifecycleIndexV1>,
    factories: &ReservedValue<FactoryFamilyQualificationV1>,
    industry: &ReservedValue<IndustryFamilyQualificationV1>,
    roots: &RootSetV1,
    memory: &MemoryBudget,
    work: &mut FactoryLifecycleIndexWorkV1,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<FactoryLifecycleIndexV1>, ValueTreeError> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    if !source.uses_budget(memory) || source.class != MemoryClass::Recovery {
        return Err(invalid());
    }
    let digest = validate_joint_parent(factories, industry, roots, memory, MemoryClass::Ordinary)?;
    if source.roots != digest
        || source.factory_counts != factories.record_counts()
        || source.industry_counts != industry.record_counts()
    {
        return Err(invalid());
    }
    let nf = count(factories.record_counts()[0], work.max_factories)?;
    let nj = count(industry.record_counts()[0], work.max_recipe_jobs)?;
    if source.factories.len() != nf || source.jobs.len() != nj || source.by_factory.len() != nj {
        return Err(invalid());
    }
    let bytes = size(nf, std::mem::size_of::<HeldFactory>())?
        .checked_add(size(nj, 2 * std::mem::size_of::<HeldJob>())?)
        .ok_or_else(invalid)?;
    let vectors = memory.reserve(bytes, MemoryClass::Ordinary)?;
    let owner = memory.reserve(
        std::mem::size_of::<FactoryLifecycleIndexV1>(),
        MemoryClass::Ordinary,
    )?;
    let mut fs = Vec::with_capacity(nf);
    let mut js = Vec::with_capacity(nj);
    for row in &source.factories {
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
        work.cell()?;
        if row.id.len() > work.max_text_bytes {
            return Err(ValueTreeError::ResourceLimited(
                "factory lifecycle copy text",
            ));
        }
        let fee = memory.reserve(
            std::mem::size_of::<ReservedValue<FactoryProjection>>() + 128 + row.id.len(),
            MemoryClass::Ordinary,
        )?;
        fs.push(Arc::new(fee.retain(FactoryProjection {
            id: row.id.clone(),
            value: row.value,
            child: row.child.clone(),
            completed: row.completed,
            stable: row.stable,
        })));
    }
    for row in &source.jobs {
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
        work.cell()?;
        if row.factory.len() > work.max_text_bytes {
            return Err(ValueTreeError::ResourceLimited(
                "factory lifecycle copy text",
            ));
        }
        let fee = memory.reserve(
            std::mem::size_of::<ReservedValue<JobProjection>>() + 128 + row.factory.len(),
            MemoryClass::Ordinary,
        )?;
        js.push(Arc::new(fee.retain(JobProjection {
            id: row.id,
            value: row.value,
            child: row.child.clone(),
            factory: row.factory.clone(),
        })));
    }
    let mut by_factory = Vec::with_capacity(nj);
    for row in &js {
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
        work.cell()?;
        by_factory.push(row.clone());
    }
    sort(&mut by_factory, work, &mut cancelled, |a, b, w| {
        Ok(w.comparison(&a.factory, &b.factory)?.then(a.id.cmp(&b.id)))
    })?;
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(owner.retain(FactoryLifecycleIndexV1 {
        roots: digest,
        class: MemoryClass::Ordinary,
        factory_counts: source.factory_counts,
        industry_counts: source.industry_counts,
        factories: fs,
        jobs: js,
        by_factory,
        completed_sum: source.completed_sum,
        stable_count: source.stable_count,
        _vectors: vectors,
    }))
}

#[path = "incremental_factory_lifecycle_index_verified.rs"]
mod verified;
pub(crate) use verified::accept_factory_lifecycle_index_verified_v1;

#[path = "incremental_factory_lifecycle_index_current.rs"]
mod current;
pub use current::{FactoryLifecycleObservationV1, observe_current_factory_lifecycle_v1};

#[cfg(test)]
#[path = "incremental_factory_lifecycle_index_tests.rs"]
mod tests;
