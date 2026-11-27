//! Fully typed, caller-paid Current Crisis data for the future single FIFO owner.
//! Data alone is never an Accepted input, Rule decision, or publication permit.
use super::*;
use crate::incremental_field_overlay::WorldFieldOverlayErrorV1;
use oasis7_formal_protocol::incremental_execution::{
    ExecutionFence, ExecutionTransaction, PreparedGuard, RootSetV1,
};

fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("current crisis typed field")
}

fn required_text(
    (value, bytes): (Option<RecordValueV1>, Vec<u8>),
) -> Result<String, ValueTreeError> {
    match value {
        Some(RecordValueV1::Text { utf8_bytes })
            if u64::try_from(bytes.len()).ok() == Some(utf8_bytes) =>
        {
            String::from_utf8(bytes).map_err(|_| invalid())
        }
        _ => Err(invalid()),
    }
}

fn optional_text(
    (value, bytes): (Option<RecordValueV1>, Vec<u8>),
) -> Result<Option<String>, ValueTreeError> {
    if value.is_none() && bytes.is_empty() {
        Ok(None)
    } else {
        required_text((value, bytes)).map(Some)
    }
}

fn required_u64(value: Option<RecordValueV1>) -> Result<u64, ValueTreeError> {
    match value {
        Some(RecordValueV1::Unsigned(n)) => Ok(n),
        _ => Err(invalid()),
    }
}

fn optional_u64(value: Option<RecordValueV1>) -> Result<Option<u64>, ValueTreeError> {
    match value {
        Some(RecordValueV1::Unsigned(n)) => Ok(Some(n)),
        None => Ok(None),
        _ => Err(invalid()),
    }
}

fn optional_bool(value: Option<RecordValueV1>) -> Result<Option<bool>, ValueTreeError> {
    match value {
        Some(RecordValueV1::Bool(v)) => Ok(Some(v)),
        None => Ok(None),
        _ => Err(invalid()),
    }
}

/// Reconstruct all eleven exact original fields from the selected authenticated
/// child. The outer map key need not equal the embedded crisis_id.
///
/// The owner retains a conservative Ordinary reservation covering the inline
/// DTO and up to five owned text buffers plus one in-flight read buffer. Every
/// scalar remains subject to the existing physical I/O, semantic-key, work,
/// cancellation, and signed parent/root checks; no World hydration or new pool.
fn read_data<P, R, E, C>(
    id: &str,
    proof: &ReservedValue<DiplomacyRecordQualificationV1>,
    ctx: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
    max_text_bytes: u64,
) -> Result<ReservedValue<CrisisState>, ValueTreeError>
where
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    ctx.check()?;
    if ctx.values.class != MemoryClass::Ordinary || !proof.uses_budget(ctx.values.memory) {
        return Err(invalid());
    }
    let cap = usize::try_from(max_text_bytes).map_err(|_| invalid())?;
    let phase = cap
        .checked_mul(6)
        .and_then(|n| n.checked_add(std::mem::size_of::<CrisisState>()))
        .ok_or_else(invalid)?;
    let fee = ctx
        .values
        .memory
        .reserve(phase, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let mut get = |column: CrisisScalarV1| {
        let is_text = matches!(
            column,
            CrisisScalarV1::CrisisId
                | CrisisScalarV1::Kind
                | CrisisScalarV1::Status
                | CrisisScalarV1::ResolverAgentId
                | CrisisScalarV1::Strategy
        );
        let mut output = Vec::with_capacity(if is_text { cap } else { 0 });
        if output.capacity() > cap {
            return Err(invalid());
        }
        let scalar = read_qualified_diplomacy_scalar_v1(
            id,
            DiplomacyFieldV1::Crises,
            DiplomacyScalarV1::Crisis(column),
            proof,
            ctx,
            max_text_bytes,
            &mut output,
        )?;
        if output.len() > cap {
            return Err(invalid());
        }
        Ok::<_, ValueTreeError>((scalar, output))
    };
    let crisis_id = required_text(get(CrisisScalarV1::CrisisId)?)?;
    let kind = required_text(get(CrisisScalarV1::Kind)?)?;
    let severity = u32::try_from(required_u64(get(CrisisScalarV1::Severity)?.0)?)
        .map_err(|_| invalid())?;
    let status = match required_text(get(CrisisScalarV1::Status)?)?.as_str() {
        "active" => CrisisStatus::Active,
        "resolved" => CrisisStatus::Resolved,
        "timed_out" => CrisisStatus::TimedOut,
        _ => return Err(invalid()),
    };
    let opened_at = required_u64(get(CrisisScalarV1::OpenedAt)?.0)?;
    let expires_at = required_u64(get(CrisisScalarV1::ExpiresAt)?.0)?;
    let resolver_agent_id = optional_text(get(CrisisScalarV1::ResolverAgentId)?)?;
    let strategy = optional_text(get(CrisisScalarV1::Strategy)?)?;
    let success = optional_bool(get(CrisisScalarV1::Success)?.0)?;
    let impact = match get(CrisisScalarV1::Impact)?.0 {
        Some(RecordValueV1::Signed(v)) => v,
        _ => return Err(invalid()),
    };
    let resolved_at = optional_u64(get(CrisisScalarV1::ResolvedAt)?.0)?;
    ctx.check()?;
    Ok(fee.retain(CrisisState {
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
    }))
}

/// Real Current parent -> typed child -> complete Crisis data in one transaction.
/// The first Current opener advances the transaction's read revision; the second
/// phase checks its *new* guard against the original live fence, and any
/// failure aborts the same transaction. No Accepted or execution token escapes.
#[allow(clippy::too_many_arguments)]
pub fn read_current_diplomacy_crisis_state_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    id: &str,
    parent: &ReservedValue<DiplomacyFamilyQualificationV1>,
    roots: &RootSetV1,
    tx: &mut ExecutionTransaction<'_, M>,
    ctx: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
    scan: &mut StateScanBudget,
    cells: &mut u64,
    max_text_bytes: u64,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<Option<ReservedValue<CrisisState>>, WorldFieldOverlayErrorV1> {
    let Some(record) = open_current_diplomacy_crisis_record_v1(
        id,
        parent,
        roots,
        tx,
        ctx,
        scan,
        cells,
        max_text_bytes,
        guard,
        live,
    )? else {
        return Ok(None);
    };
    tx.contribute(|tx| {
        tx.validate_guard(&tx.guard(), live)?;
        if !tx.uses_budget(ctx.values.memory) || ctx.values.class != MemoryClass::Ordinary {
            return Err(invalid().into());
        }
        let mut body = WorldFieldReadContextV1 {
            root: record.root(),
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
        let state = read_data(id, &record, &mut body, max_text_bytes)?;
        body.check()?;
        tx.validate_guard(&tx.guard(), live)?;
        Ok(Some(state))
    })
}
