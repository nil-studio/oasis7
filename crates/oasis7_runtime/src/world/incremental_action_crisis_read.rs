//! Actual qualified Current agent and complete Crisis record reads.
use super::*;
#[allow(clippy::too_many_arguments)]
pub(super) fn agent<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeWriter,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
    D: FnMut() -> bool,
>(
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    id: &str,
    sources: &CrisisSourcesV1<'_>,
    roots: &oasis7_formal_protocol::incremental_execution::RootSetV1,
    read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    parent_read: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    work: &mut CrisisWorkV1,
    live: &ExecutionFence,
) -> Result<bool, BodyMutationError> {
    work.spend(id.len())?;
    let mut ctx = WorldFieldOverlayContextV1 {
        transaction: step.transaction,
        values: ValueReadContext {
            source: read.values.source,
            memory: step.mutation.memory,
            class: read.values.class,
            io: &mut *read.values.io,
        },
        key_work: &mut *read.key_work,
        semantic_bytes: read.semantic_bytes,
        record_bytes: read.record_bytes,
        take_state_error: &mut read.take_state_error,
        cancelled: || (read.cancelled)() || (step.mutation.cancelled)(),
    };
    let guard = ctx.transaction.guard();
    Ok(read_current_agent_presence_v1(
        id,
        sources.agents,
        roots,
        &mut ctx,
        parent_read,
        &mut work.pending_entries_remaining,
        &guard,
        live,
    )
    .map_err(overlay_error)?
        == CurrentAgentPresenceV1::Existing)
}

/// Decode every fixed column from the authenticated child, preserving embedded ID.
pub(super) fn crisis<P, V, E, C>(
    id: &str,
    proof: &ReservedValue<DiplomacyRecordQualificationV1>,
    read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    work: &mut CrisisWorkV1,
) -> Result<ReservedValue<oasis7_world_state::gameplay_state::CrisisState>, BodyMutationError>
where
    P: MerkleNodeReader,
    V: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let cap = usize::try_from(work.max_text_bytes)
        .map_err(|_| ValueTreeError::ResourceLimited("crisis text bound"))?;
    let bytes = cap
        .checked_mul(6)
        .and_then(|n| {
            n.checked_add(std::mem::size_of::<
                oasis7_world_state::gameplay_state::CrisisState,
            >())
        })
        .ok_or(ValueTreeError::ResourceLimited("crisis record layout"))?;
    let charge = read
        .values
        .memory
        .reserve(bytes, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let mut ctx = WorldFieldReadContextV1 {
        root: proof.root(),
        state: read.state,
        values: ValueReadContext {
            source: read.values.source,
            memory: read.values.memory,
            class: read.values.class,
            io: &mut *read.values.io,
        },
        key_work: &mut *read.key_work,
        semantic_bytes: read.semantic_bytes,
        record_bytes: read.record_bytes,
        take_state_error: &mut read.take_state_error,
        cancelled: &mut read.cancelled,
    };
    let mut get = |column| -> Result<(Option<RecordValueV1>, String), BodyMutationError> {
        work.spend(id.len())?;
        let mut output = Vec::with_capacity(cap);
        let v = read_qualified_diplomacy_scalar_v1(
            id,
            DiplomacyFieldV1::Crises,
            DiplomacyScalarV1::Crisis(column),
            proof,
            &mut ctx,
            work.max_text_bytes,
            &mut output,
        )?;
        let text = String::from_utf8(output)
            .map_err(|_| BodyMutationError::Invalid("crisis scalar utf8"))?;
        Ok((v, text))
    };
    fn uint(v: Option<RecordValueV1>) -> Result<u64, BodyMutationError> {
        match v {
            Some(RecordValueV1::Unsigned(v)) => Ok(v),
            _ => Err(BodyMutationError::Invalid("crisis unsigned scalar")),
        }
    }
    fn text(v: (Option<RecordValueV1>, String)) -> Result<String, BodyMutationError> {
        match v {
            (Some(RecordValueV1::Text { .. }), s) => Ok(s),
            _ => Err(BodyMutationError::Invalid("crisis text scalar")),
        }
    }
    fn optional(v: (Option<RecordValueV1>, String)) -> Result<Option<String>, BodyMutationError> {
        match v {
            (None, _) => Ok(None),
            (Some(RecordValueV1::Text { .. }), s) => Ok(Some(s)),
            _ => Err(BodyMutationError::Invalid("crisis optional text scalar")),
        }
    }
    let crisis_id = text(get(CrisisScalarV1::CrisisId)?)?;
    let kind = text(get(CrisisScalarV1::Kind)?)?;
    let severity = u32::try_from(uint(get(CrisisScalarV1::Severity)?.0)?)
        .map_err(|_| BodyMutationError::Invalid("crisis severity"))?;
    let status = match text(get(CrisisScalarV1::Status)?)?.as_str() {
        "active" => oasis7_world_state::gameplay_state::CrisisStatus::Active,
        "resolved" => oasis7_world_state::gameplay_state::CrisisStatus::Resolved,
        "timed_out" => oasis7_world_state::gameplay_state::CrisisStatus::TimedOut,
        _ => return Err(BodyMutationError::Invalid("crisis status")),
    };
    let opened_at = uint(get(CrisisScalarV1::OpenedAt)?.0)?;
    let expires_at = uint(get(CrisisScalarV1::ExpiresAt)?.0)?;
    let resolver_agent_id = optional(get(CrisisScalarV1::ResolverAgentId)?)?;
    let strategy = optional(get(CrisisScalarV1::Strategy)?)?;
    let success = match get(CrisisScalarV1::Success)?.0 {
        None => None,
        Some(RecordValueV1::Bool(v)) => Some(v),
        _ => return Err(BodyMutationError::Invalid("crisis success")),
    };
    let impact = match get(CrisisScalarV1::Impact)?.0 {
        Some(RecordValueV1::Signed(v)) => v,
        _ => return Err(BodyMutationError::Invalid("crisis impact")),
    };
    let resolved_at = match get(CrisisScalarV1::ResolvedAt)?.0 {
        None => None,
        v => Some(uint(v)?),
    };
    Ok(
        charge.retain(oasis7_world_state::gameplay_state::CrisisState {
            crisis_id,
            kind,
            severity,
            status,
            opened_at,
            expires_at,
            resolver_agent_id,
            strategy,
            success,
            impact,
            resolved_at,
        }),
    )
}
