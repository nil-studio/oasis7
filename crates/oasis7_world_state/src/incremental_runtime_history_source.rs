//! Candidate and accepted Runtime History window provenance. Business execution,
//! original durability, leases and legacy archive files remain separate owners.
use super::incremental_runtime_history_original::RuntimeHistoryOriginalBindingV1;
use crate::{
    incremental_field_read::WorldFieldReadContextV1, incremental_runtime_history_index::*,
    incremental_runtime_history_manifest::*,
};
use oasis7_formal_protocol::{
    controlled_authority::formal::{VerifiedCandidateContextV1, VerifiedFormalCommitV1},
    incremental_execution::{ExecutionFactKindV1, RootSetV1},
    incremental_material::{
        CandidateFactRequestV1, CheckedCandidateFactOccurrenceV1, CheckedCommitCore,
        CheckedFactOriginals, CoreError, VerifiedFactOccurrenceV1, check_candidate_fact_v1,
        promote_candidate_fact_v1,
    },
    incremental_state::MerkleNodeReader,
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use std::convert::Infallible;
#[derive(Debug)]
pub enum RuntimeHistorySourceErrorV1 {
    Value(ValueTreeError),
    Formal(CoreError<Infallible>),
}
impl From<ValueTreeError> for RuntimeHistorySourceErrorV1 {
    fn from(e: ValueTreeError) -> Self {
        Self::Value(e)
    }
}
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("Runtime History source binding")
}
const CARRIER: usize = 16 << 10;
pub struct RuntimeHistoryCandidateRowV1<'a> {
    pub binding: &'a ReservedValue<RuntimeHistoryOriginalBindingV1>,
    pub fact_ordinal: u64,
    pub index_ordinal: u64,
}
pub struct CheckedRuntimeHistorySourceV1 {
    manifest: RuntimeHistoryManifestV1,
    selected: ReservedValue<CheckedCandidateFactOccurrenceV1>,
}
impl CheckedRuntimeHistorySourceV1 {
    pub fn manifest(&self) -> &RuntimeHistoryManifestV1 {
        &self.manifest
    }
    pub fn occurrence(&self) -> &ReservedValue<CheckedCandidateFactOccurrenceV1> {
        &self.selected
    }
}
pub struct AcceptedRuntimeHistorySourceV1 {
    manifest: RuntimeHistoryManifestV1,
    selected: ReservedValue<VerifiedFactOccurrenceV1>,
    indexes: [ReservedValue<RuntimeHistoryIndexQualificationV1>; 4],
    class: MemoryClass,
}
impl AcceptedRuntimeHistorySourceV1 {
    pub fn manifest(&self) -> &RuntimeHistoryManifestV1 {
        &self.manifest
    }
    pub fn occurrence(&self) -> &ReservedValue<VerifiedFactOccurrenceV1> {
        &self.selected
    }
    pub fn index(
        &self,
        lane: RuntimeHistoryLaneV1,
    ) -> &ReservedValue<RuntimeHistoryIndexQualificationV1> {
        &self.indexes[lane.index()]
    }
    pub fn allocation_class(&self) -> MemoryClass {
        self.class
    }
}
pub struct RuntimeHistorySourceWorkV1 {
    pub rows_remaining: u64,
    pub index: RuntimeHistoryIndexWorkV1,
}
fn check<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<(), ValueTreeError> {
    ctx.check()?;
    if let Some(e) = (ctx.take_state_error)() {
        return Err(e);
    }
    Ok(())
}
/// The selected ordinal is supplied by the enclosing closed Result router.
/// Every new row is checked against actual observed candidate facts and its
/// private canonical type binding, never a source-success callback.
pub fn check_candidate_runtime_history_source_v1<M, R, E, C>(
    candidate: &VerifiedCandidateContextV1,
    core: &ReservedValue<CheckedCommitCore>,
    originals: &ReservedValue<CheckedFactOriginals>,
    manifest: &RuntimeHistoryManifestV1,
    selected_ordinal: u64,
    indexes: &[ReservedValue<RuntimeHistoryIndexQualificationV1>; 4],
    parent: Option<&ReservedValue<AcceptedRuntimeHistorySourceV1>>,
    rows: &[RuntimeHistoryCandidateRowV1<'_>],
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut RuntimeHistorySourceWorkV1,
) -> Result<ReservedValue<CheckedRuntimeHistorySourceV1>, RuntimeHistorySourceErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    check_candidate_runtime_history_source_with_prefix_v1(
        candidate,
        core,
        originals,
        manifest,
        selected_ordinal,
        indexes,
        parent,
        None,
        rows,
        ctx,
        work,
    )
}
pub fn check_candidate_runtime_history_source_with_prefix_v1<M, R, E, C>(
    candidate: &VerifiedCandidateContextV1,
    core: &ReservedValue<CheckedCommitCore>,
    originals: &ReservedValue<CheckedFactOriginals>,
    manifest: &RuntimeHistoryManifestV1,
    selected_ordinal: u64,
    indexes: &[ReservedValue<RuntimeHistoryIndexQualificationV1>; 4],
    parent: Option<&ReservedValue<AcceptedRuntimeHistorySourceV1>>,
    prefix: Option<&ReservedValue<RuntimeHistoryPrefixBranchV1>>,
    rows: &[RuntimeHistoryCandidateRowV1<'_>],
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut RuntimeHistorySourceWorkV1,
) -> Result<ReservedValue<CheckedRuntimeHistorySourceV1>, RuntimeHistorySourceErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let result = (|| {
        check(ctx)?;
        let memory = ctx.values.memory;
        if ctx.values.class != MemoryClass::Ordinary
            || !core.uses_budget(memory)
            || !originals.uses_budget(memory)
        {
            return Err(invalid().into());
        }
        let charge = memory
            .reserve(CARRIER, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let _phase = memory
            .reserve(128 << 10, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        manifest.validate()?;
        if manifest.roots != core.roots().digest().map_err(|_| invalid())? {
            return Err(invalid().into());
        }
        let mut base = [0u64; 4];
        if let Some(parent) = parent {
            if !parent.uses_budget(memory)
                || parent.reserved_bytes() < CARRIER
                || parent.class != MemoryClass::Ordinary
                || core.parent_record_id() != Some(parent.selected.head_record_id())
                || manifest.previous_view != Some(parent.selected.location().original)
                || core
                    .parent_roots()
                    .ok_or_else(invalid)?
                    .digest()
                    .map_err(|_| invalid())?
                    != parent.manifest.roots
            {
                return Err(invalid().into());
            }
            for lane in RuntimeHistoryLaneV1::ALL {
                base[lane.index()] = parent.manifest.lanes[lane.index()].total;
            }
        } else if core.parent_record_id().is_some()
            || core.roots().height != 0
            || manifest.previous_view.is_some()
        {
            return Err(invalid().into());
        }
        let base_manifest = if let Some(prefix) = prefix {
            let current = parent.ok_or_else(invalid)?;
            if !prefix.uses_budget(memory)
                || prefix.reserved_bytes() < CARRIER
                || prefix.current_record != current.selected.head_record_id()
                || prefix.current_roots != current.manifest.roots
            {
                return Err(invalid().into());
            }
            Some(&prefix.manifest)
        } else {
            parent.map(|p| &p.manifest)
        };
        if let Some(base_manifest) = base_manifest {
            for lane in RuntimeHistoryLaneV1::ALL {
                base[lane.index()] = base_manifest.lanes[lane.index()].total;
            }
        }
        for lane in RuntimeHistoryLaneV1::ALL {
            let i = lane.index();
            let proof = &indexes[i];
            let after = &manifest.lanes[i];
            if !proof.uses_budget(memory)
                || proof.reserved_bytes()
                    < std::mem::size_of::<RuntimeHistoryIndexQualificationV1>() + 512
                || proof.allocation_class() != MemoryClass::Ordinary
                || proof.lane() != lane
                || proof.window() != after
                || after.total < base[i]
            {
                return Err(invalid().into());
            }
            if let Some(base_manifest) = base_manifest {
                let before = &base_manifest.lanes[i];
                if after.total == base[i] {
                    if after.root != before.root {
                        return Err(invalid().into());
                    }
                } else if proof.lineage() != Some((&before.root, before.total)) {
                    return Err(invalid().into());
                }
                let appended = after.total.checked_sub(before.total).ok_or_else(invalid)?;
                let before_end = before.end()?;
                if after.start < before.start
                    || (appended > 0 && before_end != before.total)
                    || after.end()? > before_end.checked_add(appended).ok_or_else(invalid)?
                {
                    return Err(invalid().into());
                }
            }
        }
        let tick = &manifest.lanes[RuntimeHistoryLaneV1::Tick.index()];
        if tick.start != manifest.archived_tick_count || tick.end()? != tick.total {
            return Err(invalid().into());
        }
        let mut expected = base;
        let mut last_fact = None;
        for row in rows {
            check(ctx)?;
            work.rows_remaining =
                work.rows_remaining
                    .checked_sub(1)
                    .ok_or(ValueTreeError::ResourceLimited(
                        "Runtime History source rows",
                    ))?;
            if !row.binding.uses_budget(memory)
                || row.binding.reserved_bytes()
                    < std::mem::size_of::<RuntimeHistoryOriginalBindingV1>() + 512
                || last_fact.is_some_and(|v| row.fact_ordinal <= v)
            {
                return Err(invalid().into());
            }
            last_fact = Some(row.fact_ordinal);
            let lane = row.binding.lane();
            let i = lane.index();
            if row.index_ordinal != expected[i] || row.index_ordinal >= manifest.lanes[i].total {
                return Err(invalid().into());
            }
            let kind = if lane == RuntimeHistoryLaneV1::Journal {
                ExecutionFactKindV1::Event
            } else {
                ExecutionFactKindV1::Result
            };
            let occurrence = check_candidate_fact_v1(
                &CandidateFactRequestV1 {
                    kind,
                    candidate,
                    core,
                    originals,
                    ordinal: row.fact_ordinal,
                },
                memory,
                &mut ctx.cancelled,
            )
            .map_err(RuntimeHistorySourceErrorV1::Formal)?;
            let actual = read_runtime_history_index_point_v1(
                lane,
                &manifest.lanes[i],
                row.index_ordinal,
                ctx,
                &mut work.index,
            )?;
            if occurrence.location().original != row.binding.original_id()
                || occurrence.original_bytes() != row.binding.original_bytes()
                || actual.location != occurrence.location()
                || actual.encoded_bytes != occurrence.original_bytes()
            {
                return Err(invalid().into());
            }
            if lane == RuntimeHistoryLaneV1::Tick
                && row.binding.sequence() != row.index_ordinal.checked_add(1).ok_or_else(invalid)?
            {
                return Err(invalid().into());
            }
            expected[i] = expected[i].checked_add(1).ok_or_else(invalid)?;
        }
        for lane in RuntimeHistoryLaneV1::ALL {
            if expected[lane.index()] != manifest.lanes[lane.index()].total {
                return Err(invalid().into());
            }
        }
        let bytes = manifest.canonical_bytes()?;
        let selected = check_candidate_fact_v1(
            &CandidateFactRequestV1 {
                kind: ExecutionFactKindV1::Result,
                candidate,
                core,
                originals,
                ordinal: selected_ordinal,
            },
            memory,
            &mut ctx.cancelled,
        )
        .map_err(RuntimeHistorySourceErrorV1::Formal)?;
        if selected.location().original != *blake3::hash(&bytes).as_bytes()
            || selected.original_bytes() != bytes.len() as u64
            || selected.roots_id() != manifest.roots
        {
            return Err(invalid().into());
        }
        check(ctx)?;
        Ok(charge.retain(CheckedRuntimeHistorySourceV1 {
            manifest: manifest.clone(),
            selected,
        }))
    })();
    check(ctx)?;
    result
}
/// Only the exact checked signed candidate can become accepted provenance.
pub fn promote_runtime_history_source_v1<M, R, E, C>(
    checked: &ReservedValue<CheckedRuntimeHistorySourceV1>,
    accepted: &VerifiedFormalCommitV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut RuntimeHistoryIndexWorkV1,
) -> Result<ReservedValue<AcceptedRuntimeHistorySourceV1>, RuntimeHistorySourceErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let result = (|| {
        check(ctx)?;
        if ctx.values.class != MemoryClass::Ordinary
            || !checked.uses_budget(ctx.values.memory)
            || checked.reserved_bytes() < CARRIER
        {
            return Err(invalid().into());
        }
        let charge = ctx
            .values
            .memory
            .reserve(CARRIER, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let selected = promote_candidate_fact_v1(
            &checked.selected,
            accepted,
            ctx.values.memory,
            &mut ctx.cancelled,
        )
        .map_err(RuntimeHistorySourceErrorV1::Formal)?;
        recover(&checked.manifest, selected, ctx, work, charge)
    })();
    check(ctx)?;
    result
}
fn recover<M, R, E, C>(
    manifest: &RuntimeHistoryManifestV1,
    selected: ReservedValue<VerifiedFactOccurrenceV1>,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut RuntimeHistoryIndexWorkV1,
    charge: oasis7_proto::memory_budget::MemoryReservation,
) -> Result<ReservedValue<AcceptedRuntimeHistorySourceV1>, RuntimeHistorySourceErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let indexes = [
        recover_runtime_history_index_attested_v1(
            manifest,
            &selected,
            RuntimeHistoryLaneV1::Snapshot,
            ctx,
            work,
        )?,
        recover_runtime_history_index_attested_v1(
            manifest,
            &selected,
            RuntimeHistoryLaneV1::Journal,
            ctx,
            work,
        )?,
        recover_runtime_history_index_attested_v1(
            manifest,
            &selected,
            RuntimeHistoryLaneV1::Tick,
            ctx,
            work,
        )?,
        recover_runtime_history_index_attested_v1(
            manifest,
            &selected,
            RuntimeHistoryLaneV1::Audit,
            ctx,
            work,
        )?,
    ];
    check(ctx)?;
    Ok(charge.retain(AcceptedRuntimeHistorySourceV1 {
        manifest: manifest.clone(),
        selected,
        indexes,
        class: ctx.values.class,
    }))
}
/// Caller obtains the genuine occurrence through the Formal cold reader. This
/// checks exact accepted head/root and the complete manifest hash before reuse.
pub fn recover_runtime_history_source_v1<M, R, E, C>(
    head: &VerifiedFormalCommitV1,
    roots: &RootSetV1,
    manifest: &RuntimeHistoryManifestV1,
    selected: ReservedValue<VerifiedFactOccurrenceV1>,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut RuntimeHistoryIndexWorkV1,
) -> Result<ReservedValue<AcceptedRuntimeHistorySourceV1>, RuntimeHistorySourceErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let result = (|| {
        check(ctx)?;
        let charge = ctx
            .values
            .memory
            .reserve(CARRIER, ctx.values.class)
            .map_err(ValueTreeError::Memory)?;
        let _phase = ctx
            .values
            .memory
            .reserve(128 << 10, ctx.values.class)
            .map_err(ValueTreeError::Memory)?;
        if ctx.values.class != MemoryClass::Recovery
            || roots.digest().map_err(|_| invalid())? != manifest.roots
            || hex::encode(selected.head_record_id())
                != head.record().record_id().map_err(|_| invalid())?
        {
            return Err(invalid().into());
        }
        recover(manifest, selected, ctx, work, charge)
    })();
    check(ctx)?;
    result
}
pub fn readmit_runtime_history_source_ordinary_v1(
    source: &ReservedValue<AcceptedRuntimeHistorySourceV1>,
    head: &VerifiedFormalCommitV1,
    roots: &RootSetV1,
    memory: &MemoryBudget,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<AcceptedRuntimeHistorySourceV1>, RuntimeHistorySourceErrorV1> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled.into());
    }
    if source.class != MemoryClass::Recovery
        || !source.uses_budget(memory)
        || source.reserved_bytes() < CARRIER
        || source.manifest.roots != roots.digest().map_err(|_| invalid())?
    {
        return Err(invalid().into());
    }
    let charge = memory
        .reserve(CARRIER, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let selected =
        oasis7_formal_protocol::incremental_material::readmit_fact_occurrence_ordinary_v1(
            &source.selected,
            head,
            roots,
            memory,
            &mut cancelled,
        )
        .map_err(RuntimeHistorySourceErrorV1::Formal)?;
    let indexes = [
        readmit_runtime_history_index_ordinary_v1(
            &source.indexes[0],
            &source.manifest.lanes[0],
            memory,
            &mut cancelled,
        )?,
        readmit_runtime_history_index_ordinary_v1(
            &source.indexes[1],
            &source.manifest.lanes[1],
            memory,
            &mut cancelled,
        )?,
        readmit_runtime_history_index_ordinary_v1(
            &source.indexes[2],
            &source.manifest.lanes[2],
            memory,
            &mut cancelled,
        )?,
        readmit_runtime_history_index_ordinary_v1(
            &source.indexes[3],
            &source.manifest.lanes[3],
            memory,
            &mut cancelled,
        )?,
    ];
    if cancelled() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(charge.retain(AcceptedRuntimeHistorySourceV1 {
        manifest: source.manifest.clone(),
        selected,
        indexes,
        class: MemoryClass::Ordinary,
    }))
}

/// Historical prefix branch authenticated under the current accepted head.
/// Its construction proves ancestry/identity only, never rollback permission.
pub struct RuntimeHistoryPrefixBranchV1 {
    manifest: RuntimeHistoryManifestV1,
    current_record: [u8; 32],
    current_roots: [u8; 32],
}
pub fn qualify_runtime_history_prefix_branch_v1(
    historical: &ReservedValue<AcceptedRuntimeHistorySourceV1>,
    current: &ReservedValue<AcceptedRuntimeHistorySourceV1>,
    inherited: &ReservedValue<VerifiedFactOccurrenceV1>,
    memory: &MemoryBudget,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<RuntimeHistoryPrefixBranchV1>, ValueTreeError> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    if !historical.uses_budget(memory)
        || !current.uses_budget(memory)
        || historical.reserved_bytes() < CARRIER
        || current.reserved_bytes() < CARRIER
        || historical.class != MemoryClass::Ordinary
        || current.class != MemoryClass::Ordinary
        || !inherited.uses_budget(memory)
        || inherited.reserved_bytes() < std::mem::size_of::<VerifiedFactOccurrenceV1>()
        || inherited.allocation_class() != MemoryClass::Ordinary
        || inherited.kind() != ExecutionFactKindV1::Result
        || inherited.location().original != historical.selected.location().original
        || inherited.original_bytes() != historical.selected.original_bytes()
        || inherited.fact_body_id() != historical.selected.fact_body_id()
        || inherited.head_record_id() != current.selected.head_record_id()
        || inherited.roots_id() != current.manifest.roots
        || inherited.head_body_id() != current.selected.head_body_id()
    {
        return Err(invalid());
    }
    let charge = memory.reserve(CARRIER, MemoryClass::Ordinary)?;
    let result = RuntimeHistoryPrefixBranchV1 {
        manifest: historical.manifest.clone(),
        current_record: current.selected.head_record_id(),
        current_roots: current.manifest.roots,
    };
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(charge.retain(result))
}

#[path = "incremental_runtime_history_prepared.rs"]
mod prepared;
pub use prepared::*;

#[path = "incremental_runtime_history_candidate_reader.rs"]
mod candidate_reader;
pub use candidate_reader::*;
