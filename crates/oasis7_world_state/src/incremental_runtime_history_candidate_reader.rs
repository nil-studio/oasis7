//! Private source child: actual single-original consumers and bounded append
//! replay. No accepted authority, materialized World or all-H ordinary scan.
use super::*;
use crate::{
    incremental_agent_mutation::BodyMutationError,
    incremental_field_member::{MemberRecordError, MemberRecordWriteContext},
    incremental_runtime_history_decode::{
        DecodedRuntimeHistoryOriginalV1, RuntimeHistoryDecodeKindV1, RuntimeHistoryDecodeWorkV1,
        decode_runtime_history_original_v1,
    },
    incremental_runtime_history_original::{
        RuntimeHistoryOriginalRefV1, measure_runtime_history_original_v1,
    },
    incremental_world_event_decode::{WorldEventDecodeWorkV1, decode_world_event_original_v1},
    incremental_world_format::select_complete_world_format_v1,
};
use oasis7_formal_protocol::{
    incremental_material::{FactBudget, OriginalObjectStreamReader},
    incremental_state::StatePageWriter,
};
pub struct RuntimeHistoryCandidateReadWorkV1 {
    pub rows_remaining: u64,
    pub max_new_rows: usize,
    pub index: RuntimeHistoryIndexWorkV1,
    pub originals: FactBudget,
    pub decode: RuntimeHistoryDecodeWorkV1,
    pub max_world_event_heap_bytes: usize,
}
pub struct RuntimeHistoryCandidateReadRequestV1<'a> {
    pub candidate: &'a VerifiedCandidateContextV1,
    pub core: &'a ReservedValue<CheckedCommitCore>,
    pub originals: &'a ReservedValue<CheckedFactOriginals>,
    pub manifest: &'a RuntimeHistoryManifestV1,
    pub selected_ordinal: u64,
    pub parent: Option<&'a ReservedValue<AcceptedRuntimeHistorySourceV1>>,
    /// A real historical source plus its certified appearance at the current
    /// accepted parent. Neither a count nor an untrusted root can replace it.
    pub prefix: Option<(
        &'a ReservedValue<AcceptedRuntimeHistorySourceV1>,
        &'a ReservedValue<RuntimeHistoryPrefixBranchV1>,
    )>,
}
pub struct CheckedRuntimeHistoryCandidateV1 {
    _charge: oasis7_proto::memory_budget::MemoryReservation,
    pub source: ReservedValue<CheckedRuntimeHistorySourceV1>,
    pub indexes: [ReservedValue<RuntimeHistoryIndexQualificationV1>; 4],
}
#[derive(Debug)]
pub enum RuntimeHistoryCandidateReadErrorV1<E> {
    Value(ValueTreeError),
    Source(E),
    Original(BodyMutationError),
    Write(MemberRecordError),
    Qualification(RuntimeHistorySourceErrorV1),
}
impl<E> From<ValueTreeError> for RuntimeHistoryCandidateReadErrorV1<E> {
    fn from(e: ValueTreeError) -> Self {
        Self::Value(e)
    }
}
struct PointBinding {
    binding: ReservedValue<RuntimeHistoryOriginalBindingV1>,
    fact_ordinal: u64,
    index_ordinal: u64,
}
fn stop<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<(), ValueTreeError> {
    ctx.check()?;
    if let Some(error) = (ctx.take_state_error)() {
        return Err(error);
    }
    Ok(())
}
fn bind_point<
    O: OriginalObjectStreamReader,
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    lane: RuntimeHistoryLaneV1,
    index_ordinal: u64,
    point: RuntimeHistoryIndexRowV1,
    request: &RuntimeHistoryCandidateReadRequestV1<'_>,
    objects: &mut O,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut RuntimeHistoryCandidateReadWorkV1,
) -> Result<PointBinding, RuntimeHistoryCandidateReadErrorV1<O::Error>> {
    stop(ctx)?;
    work.rows_remaining =
        work.rows_remaining
            .checked_sub(1)
            .ok_or(ValueTreeError::ResourceLimited(
                "Runtime History candidate rows",
            ))?;
    work.originals.facts_remaining =
        work.originals
            .facts_remaining
            .checked_sub(1)
            .ok_or(ValueTreeError::ResourceLimited(
                "Runtime History original occurrences",
            ))?;
    let kind = if lane == RuntimeHistoryLaneV1::Journal {
        ExecutionFactKindV1::Event
    } else {
        ExecutionFactKindV1::Result
    };
    let fact = request
        .core
        .facts()
        .facts
        .get(usize::try_from(point.location.ordinal).map_err(|_| invalid())?)
        .ok_or_else(invalid)?;
    if point.location.height != request.core.roots().height
        || fact.kind != kind
        || fact.id != point.location.original
    {
        return Err(invalid().into());
    }
    if point.encoded_bytes > work.originals.max_fact_bytes
        || point.encoded_bytes > work.originals.original_bytes_remaining
        || point.encoded_bytes > work.decode.max_original_bytes as u64
    {
        return Err(
            ValueTreeError::ResourceLimited("Runtime History single original admission").into(),
        );
    }
    let max = usize::try_from(point.encoded_bytes).map_err(|_| invalid())?;
    let raw = objects
        .read_original(&point.location.original, max, ctx.values.memory)
        .map_err(RuntimeHistoryCandidateReadErrorV1::Source)?;
    if !raw.uses_budget(ctx.values.memory) || raw.reserved_bytes() < raw.len() || raw.len() != max {
        return Err(invalid().into());
    }
    work.originals.original_bytes_remaining = work
        .originals
        .original_bytes_remaining
        .checked_sub(raw.len() as u64)
        .ok_or(ValueTreeError::ResourceLimited(
            "Runtime History raw original bytes",
        ))?;
    let binding = if lane == RuntimeHistoryLaneV1::Journal {
        let zero = oasis7_wasm_abi::CanonicalCborWorkV1 {
            visits_remaining: 0,
            bytes_remaining: 0,
            max_depth: work.decode.canonical.max_depth,
            max_map_keys: work.decode.canonical.max_map_keys,
            max_key_bytes: work.decode.canonical.max_key_bytes,
        };
        let empty = oasis7_proto::cbor_decode_preflight::CborPreflightWorkV1 {
            bytes_remaining: 0,
            values_remaining: 0,
            max_depth: work.decode.structure.max_depth,
        };
        let mut event_work = WorldEventDecodeWorkV1 {
            structure: std::mem::replace(&mut work.decode.structure, empty),
            canonical: std::mem::replace(&mut work.decode.canonical, zero),
            max_original_bytes: work.decode.max_original_bytes,
            max_heap_bytes: work.max_world_event_heap_bytes,
        };
        let event = decode_world_event_original_v1(
            &raw,
            ctx.values.memory,
            MemoryClass::Ordinary,
            &mut event_work,
            &mut ctx.cancelled,
        );
        work.decode.structure = event_work.structure;
        work.decode.canonical = event_work.canonical;
        let event = event?;
        measure_runtime_history_original_v1(
            RuntimeHistoryOriginalRefV1::Event(event.event()),
            ctx.values.memory,
            &mut work.decode.canonical,
            &mut ctx.cancelled,
            work.decode.max_original_bytes,
        )
        .map_err(RuntimeHistoryCandidateReadErrorV1::Original)?
    } else {
        let decode_kind = match lane {
            RuntimeHistoryLaneV1::Snapshot => RuntimeHistoryDecodeKindV1::Snapshot,
            RuntimeHistoryLaneV1::Tick => RuntimeHistoryDecodeKindV1::Tick,
            RuntimeHistoryLaneV1::Audit => RuntimeHistoryDecodeKindV1::Audit,
            _ => unreachable!(),
        };
        let value = decode_runtime_history_original_v1(
            decode_kind,
            &raw,
            ctx.values.memory,
            MemoryClass::Ordinary,
            &mut work.decode,
            &mut ctx.cancelled,
        )?;
        let original = match &*value {
            DecodedRuntimeHistoryOriginalV1::Snapshot(v) => {
                RuntimeHistoryOriginalRefV1::Snapshot(v)
            }
            DecodedRuntimeHistoryOriginalV1::Tick(v) => RuntimeHistoryOriginalRefV1::Tick(v),
            DecodedRuntimeHistoryOriginalV1::Audit(v) => {
                RuntimeHistoryOriginalRefV1::RejectionAudit(v)
            }
        };
        measure_runtime_history_original_v1(
            original,
            ctx.values.memory,
            &mut work.decode.canonical,
            &mut ctx.cancelled,
            work.decode.max_original_bytes,
        )
        .map_err(RuntimeHistoryCandidateReadErrorV1::Original)?
    };
    if binding.lane() != lane
        || binding.original_id() != point.location.original
        || binding.original_bytes() != point.encoded_bytes
    {
        return Err(invalid().into());
    }
    stop(ctx)?;
    Ok(PointBinding {
        binding,
        fact_ordinal: point.location.ordinal,
        index_ordinal,
    })
}

/// GEN visits its explicitly bounded initial prefix. Ordinary processing only
/// reads the new after points, then reproduces each append from the genuine
/// accepted base with the existing typed index kernel and actual provisional
/// page writer. The final root must equal the advertised root byte-for-byte.
/// Both read and write IO/key allowances are caller-supplied finite accounts,
/// consumed continuously across all four lanes; neither is reset per row/lane.
#[allow(clippy::too_many_arguments)]
pub fn read_candidate_runtime_history_source_v1<M, R, E, C, W, CW, O>(
    request: &RuntimeHistoryCandidateReadRequestV1<'_>,
    objects: &mut O,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    work: &mut RuntimeHistoryCandidateReadWorkV1,
) -> Result<CheckedRuntimeHistoryCandidateV1, RuntimeHistoryCandidateReadErrorV1<O::Error>>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: StatePageWriter + ValueNodeReader,
    CW: FnMut() -> bool,
    O: OriginalObjectStreamReader,
{
    stop(ctx)?;
    if (write.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    if ctx.values.class != MemoryClass::Ordinary
        || objects.allocation_class() != MemoryClass::Ordinary
        || !objects.allocation_budget().same_pool(ctx.values.memory)
        || !write.memory.same_pool(ctx.values.memory)
    {
        return Err(invalid().into());
    }
    let _phase = ctx
        .values
        .memory
        .reserve(128 << 10, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    if !request.core.uses_budget(ctx.values.memory)
        || request.core.reserved_bytes() < std::mem::size_of::<CheckedCommitCore>()
        || !request.originals.uses_budget(ctx.values.memory)
        || request.originals.reserved_bytes() < std::mem::size_of::<CheckedFactOriginals>()
        || request.originals.body_id() != request.core.body().digest().map_err(|_| invalid())?
        || request.originals.fact_set_id() != request.core.body().facts
        || request
            .candidate
            .record()
            .record_id()
            .map_err(|_| invalid())?
            != hex::encode(request.core.candidate_record_id())
    {
        return Err(invalid().into());
    }
    select_complete_world_format_v1(request.core.config(), request.core.roots())
        .map_err(|_| invalid())?;
    request.manifest.validate()?;
    if !request
        .core
        .roots()
        .state
        .iter()
        .any(|root| root == ctx.root)
    {
        return Err(invalid().into());
    }
    if request.manifest.roots != request.core.roots().digest().map_err(|_| invalid())? {
        return Err(invalid().into());
    }
    let base = if let Some((historical, prefix)) = request.prefix {
        let current = request.parent.ok_or_else(invalid)?;
        if !historical.uses_budget(ctx.values.memory)
            || historical.class != MemoryClass::Ordinary
            || historical.reserved_bytes() < CARRIER
            || !prefix.uses_budget(ctx.values.memory)
            || prefix.reserved_bytes() < CARRIER
            || prefix.manifest != historical.manifest
            || prefix.current_record != current.selected.head_record_id()
            || prefix.current_roots != current.manifest.roots
        {
            return Err(invalid().into());
        }
        Some(historical)
    } else {
        request.parent
    };
    if let Some(parent) = request.parent {
        if !parent.uses_budget(ctx.values.memory)
            || parent.reserved_bytes() < CARRIER
            || parent.class != MemoryClass::Ordinary
            || request.core.parent_record_id() != Some(parent.selected.head_record_id())
            || request.core.parent_roots().and_then(|r| r.digest().ok())
                != Some(parent.manifest.roots)
            || request.manifest.previous_view != Some(parent.selected.location().original)
        {
            return Err(invalid().into());
        }
    } else if request.core.parent_record_id().is_some()
        || request.core.roots().height != 0
        || request.manifest.previous_view.is_some()
    {
        return Err(invalid().into());
    }
    let mut new_rows = 0usize;
    for lane in RuntimeHistoryLaneV1::ALL {
        let before = base
            .map(|b| b.manifest.lanes[lane.index()].total)
            .unwrap_or(0);
        let after = &request.manifest.lanes[lane.index()];
        let delta = after.total.checked_sub(before).ok_or_else(invalid)?;
        new_rows = new_rows
            .checked_add(usize::try_from(delta).map_err(|_| invalid())?)
            .ok_or_else(invalid)?;
    }
    if new_rows > work.max_new_rows
        || u64::try_from(new_rows).map_err(|_| invalid())? > work.rows_remaining
    {
        return Err(
            ValueTreeError::ResourceLimited("Runtime History candidate retained delta").into(),
        );
    }
    let metadata = new_rows
        .checked_mul(
            std::mem::size_of::<PointBinding>()
                + std::mem::size_of::<RuntimeHistoryCandidateRowV1<'_>>()
                + 64,
        )
        .and_then(|n| n.checked_add(512))
        .ok_or(ValueTreeError::ResourceLimited(
            "Runtime History candidate metadata",
        ))?;
    let _rows_charge = ctx
        .values
        .memory
        .reserve(metadata, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let output_charge = ctx
        .values
        .memory
        .reserve(
            std::mem::size_of::<CheckedRuntimeHistoryCandidateV1>() + 512,
            MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?;
    let mut bindings = Vec::with_capacity(new_rows);
    let _index_vec = ctx
        .values
        .memory
        .reserve(
            4 * std::mem::size_of::<ReservedValue<RuntimeHistoryIndexQualificationV1>>() + 128,
            MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?;
    let mut indexes = Vec::with_capacity(4);
    for lane in RuntimeHistoryLaneV1::ALL {
        stop(ctx)?;
        let after = &request.manifest.lanes[lane.index()];
        let before = base
            .map(|b| b.manifest.lanes[lane.index()].total)
            .unwrap_or(0);
        let mut index = if let Some(base) = base {
            prepare_runtime_history_index_promotion_v1(
                base.history_index(lane),
                ctx.values.memory,
                &mut ctx.cancelled,
            )?
        } else {
            qualify_runtime_history_index_v1(lane, after, ctx, &mut work.index)?
        };
        for ordinal in before..after.total {
            let point =
                read_runtime_history_index_point_v1(lane, after, ordinal, ctx, &mut work.index)?;
            let binding = bind_point(lane, ordinal, point, request, objects, ctx, work)?;
            if base.is_some() {
                index = prepare_runtime_history_index_append_v1(
                    &index,
                    point,
                    ctx,
                    write,
                    &mut work.index,
                )
                .map_err(RuntimeHistoryCandidateReadErrorV1::Write)?;
            }
            bindings.push(binding);
        }
        if base.is_some() {
            if index.window().root != after.root || index.window().total != after.total {
                return Err(invalid().into());
            }
            index = select_runtime_history_window_v1(
                &index,
                after.start,
                after.count,
                ctx.values.memory,
                &mut ctx.cancelled,
            )?;
        }
        indexes.push(index);
    }
    let indexes: [_; 4] = indexes.try_into().map_err(|_| invalid())?;
    bindings.sort_unstable_by_key(|row| row.fact_ordinal);
    let rows: Vec<_> = bindings
        .iter()
        .map(|row| RuntimeHistoryCandidateRowV1 {
            binding: &row.binding,
            fact_ordinal: row.fact_ordinal,
            index_ordinal: row.index_ordinal,
        })
        .collect();
    let index_pending = work.index.max_pending_node_bytes;
    let mut source_work = RuntimeHistorySourceWorkV1 {
        rows_remaining: work.rows_remaining,
        index: std::mem::replace(
            &mut work.index,
            RuntimeHistoryIndexWorkV1 {
                cells_remaining: 0,
                max_pending_node_bytes: index_pending,
            },
        ),
    };
    let source = check_candidate_runtime_history_source_with_prefix_v1(
        request.candidate,
        request.core,
        request.originals,
        request.manifest,
        request.selected_ordinal,
        &indexes,
        request.parent,
        request.prefix.map(|(_, prefix)| prefix),
        &rows,
        ctx,
        &mut source_work,
    );
    work.rows_remaining = source_work.rows_remaining;
    work.index = source_work.index;
    let source = source.map_err(RuntimeHistoryCandidateReadErrorV1::Qualification)?;
    stop(ctx)?;
    if (write.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(CheckedRuntimeHistoryCandidateV1 {
        _charge: output_charge,
        source,
        indexes,
    })
}
