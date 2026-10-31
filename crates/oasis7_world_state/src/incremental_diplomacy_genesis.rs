use super::*;
use oasis7_formal_protocol::{
    incremental_execution::ExecutionWriteV1, incremental_state::StatePageWriter,
};
pub(super) fn visit<'a>(
    source: DiplomacySourceV1<'a>,
    emit: &mut impl FnMut(Path, MemberPayloadRefV1<'a>) -> Result<(), MemberRecordError>,
) -> Result<(), MemberRecordError> {
    for (spec, payload) in profile::specs(source.field())
        .iter()
        .zip(profile::fixed(source))
    {
        if let Some(payload) = payload {
            emit(spec.path, payload)?;
        }
    }
    match source {
        DiplomacySourceV1::Alliance(v) => {
            for (i, member) in v.members.iter().enumerate() {
                emit(
                    Path::Ordinal("members", i as u64),
                    MemberPayloadRefV1::Text(member),
                )?;
            }
        }
        DiplomacySourceV1::War(v) => {
            for (i, value) in v.participant_outcomes.iter().enumerate() {
                let i = i as u64;
                let WarParticipantOutcome {
                    agent_id,
                    electricity_delta,
                    data_delta,
                    reputation_delta,
                } = value;
                emit(Path::Outcome(i), MemberPayloadRefV1::Unit)?;
                for (name, payload) in [
                    ("agent_id", MemberPayloadRefV1::Text(agent_id)),
                    (
                        "electricity_delta",
                        MemberPayloadRefV1::I64(*electricity_delta),
                    ),
                    ("data_delta", MemberPayloadRefV1::I64(*data_delta)),
                    (
                        "reputation_delta",
                        MemberPayloadRefV1::I64(*reputation_delta),
                    ),
                ] {
                    emit(Path::OutcomeField(i, name), payload)?;
                }
            }
        }
        DiplomacySourceV1::Crisis(_) | DiplomacySourceV1::Contract(_) => {}
    }
    Ok(())
}
pub(super) fn body<W: ValueNodeReader + StatePageWriter, C: FnMut() -> bool>(
    source: DiplomacySourceV1<'_>,
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
    cells: &mut u64,
    max_pending: usize,
) -> Result<StateRootV1, MemberRecordError> {
    let _operation = ctx
        .memory
        .reserve(16 << 10, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let mut root = StateRootV1::empty(StateNamespaceV1::Business)
        .map_err(|_| ValueTreeError::Invalid("diplomacy empty body"))?;
    visit(source, &mut |path, payload| {
        *cells = cells
            .checked_sub(1)
            .ok_or(ValueTreeError::ResourceLimited("diplomacy genesis cells"))?;
        let row = path.write(source.field(), payload, ctx)?;
        root = crate::incremental_record_tree::append(
            root.clone(),
            &row.field_key,
            row.value_root,
            ctx,
            crate::incremental_record_tree::RecordTreeWritePolicyV1 {
                max_pending_node_bytes: max_pending,
                labels: inner_tree::LABELS,
            },
        )?;
        if (ctx.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(())
    })?;
    Ok(root)
}
pub fn prepare_diplomacy_genesis_v1<
    W: ValueNodeReader + StatePageWriter,
    C: FnMut() -> bool,
    S: FnMut(ReservedValue<ExecutionWriteV1>) -> Result<(), ValueTreeError>,
>(
    source: &crate::state::WorldState,
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
    cells: &mut u64,
    max_pending: usize,
    sink: &mut S,
) -> Result<[u64; 4], MemberRecordError> {
    use crate::incremental_field_presence::{
        PresenceRecordLimitsV1, PresenceRecordWriteContext, write_field_metadata_v1,
    };
    for field in DiplomacyFieldV1::ALL {
        *cells = cells
            .checked_sub(1)
            .ok_or(ValueTreeError::ResourceLimited("diplomacy metadata cells"))?;
        let row = write_field_metadata_v1(
            field.descriptor()?,
            &mut PresenceRecordWriteContext {
                sink: &mut *ctx.sink,
                memory: ctx.memory,
                io: &mut *ctx.io,
                key_work: &mut *ctx.key_work,
                limits: PresenceRecordLimitsV1 {
                    semantic_bytes: ctx.limits.semantic_bytes,
                    record_bytes: ctx.limits.record_bytes,
                },
                cancelled: &mut ctx.cancelled,
            },
        )?;
        crate::incremental_field_write::emit(row.field_key, row.value_root, ctx, sink)?;
    }
    let mut counts = [0u64; 4];
    macro_rules! records {
        ($map:expr,$variant:ident,$index:expr) => {
            for (id, value) in $map {
                let record = DiplomacySourceV1::$variant(value);
                let root = body(record, ctx, cells, max_pending)?;
                *cells = cells
                    .checked_sub(1)
                    .ok_or(ValueTreeError::ResourceLimited("diplomacy outer cells"))?;
                let row = write_diplomacy_root_reference_v1(record.field(), id, &root, ctx)?;
                crate::incremental_field_write::emit(row.field_key, row.value_root, ctx, sink)?;
                counts[$index] = counts[$index]
                    .checked_add(1)
                    .ok_or(ValueTreeError::Invalid("diplomacy genesis count"))?;
            }
        };
    }
    records!(&source.alliances, Alliance, 0);
    records!(&source.wars, War, 1);
    records!(&source.crises, Crisis, 2);
    records!(&source.economic_contracts, Contract, 3);
    if (ctx.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(counts)
}
