//! Actual Current selected fields. No World or full factory DTO is restored.
use super::*;
use oasis7_world_state::incremental_ledger_value::LedgerValueKindV1;
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
    sources: &FactoryLifecycleSourcesV1<'_>,
    roots: &oasis7_formal_protocol::incremental_execution::RootSetV1,
    read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    parent_read: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    work: &mut FactoryLifecycleWorkV1,
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
pub(super) fn cell<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeWriter,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    D: FnMut() -> bool,
>(
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    id: &str,
    cell: FactoryCellV1<'_>,
    read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    work: &mut FactoryLifecycleWorkV1,
    output: &mut impl std::io::Write,
) -> Result<FactoryCellReadV1, BodyMutationError> {
    work.spend(
        id.len()
            .checked_add(16)
            .ok_or(ValueTreeError::ResourceLimited("factory field work"))?,
    )?;
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
    read_factory_cell_overlay_v1(
        FactoryRecordKeyV1::Factory(id),
        cell,
        &mut ctx,
        work.max_scalar_bytes.max(8) as u64,
        output,
    )
    .map_err(overlay_error)
}
pub(super) fn text<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeWriter,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    D: FnMut() -> bool,
>(
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    id: &str,
    column: FactoryColumnV1,
    read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    work: &mut FactoryLifecycleWorkV1,
) -> Result<Option<ReservedValue<String>>, BodyMutationError> {
    let max = work.max_scalar_bytes.max(8);
    let fee = step
        .mutation
        .memory
        .reserve(
            max.checked_add(128)
                .ok_or(ValueTreeError::ResourceLimited("factory text allocation"))?,
            MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?;
    let mut bytes = Vec::with_capacity(max);
    match cell(
        step,
        id,
        FactoryCellV1::Column(column),
        read,
        work,
        &mut bytes,
    )? {
        FactoryCellReadV1::RecordAbsent => Ok(None),
        FactoryCellReadV1::Value(FactoryValueV1::Text { utf8_bytes })
            if usize::try_from(utf8_bytes).ok() == Some(bytes.len()) =>
        {
            Ok(Some(fee.retain(String::from_utf8(bytes).map_err(
                |_| BodyMutationError::Invalid("factory text UTF-8"),
            )?)))
        }
        _ => Err(BodyMutationError::Invalid("factory text shape")),
    }
}
pub(super) fn number<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeWriter,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    D: FnMut() -> bool,
>(
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    id: &str,
    column: FactoryColumnV1,
    read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    work: &mut FactoryLifecycleWorkV1,
) -> Result<i64, BodyMutationError> {
    match cell(
        step,
        id,
        FactoryCellV1::Column(column),
        read,
        work,
        &mut std::io::sink(),
    )? {
        FactoryCellReadV1::Value(FactoryValueV1::Signed(n)) => Ok(n),
        _ => Err(BodyMutationError::Invalid("factory signed scalar")),
    }
}
pub(super) fn ledger<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeWriter,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    D: FnMut() -> bool,
>(
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    id: &str,
    column: FactoryColumnV1,
    read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    work: &mut FactoryLifecycleWorkV1,
) -> Result<ReservedValue<MaterialLedgerId>, BodyMutationError> {
    let max = work.max_scalar_bytes.max(8);
    let fee = step
        .mutation
        .memory
        .reserve(
            max.checked_add(128)
                .ok_or(ValueTreeError::ResourceLimited("factory ledger allocation"))?,
            MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?;
    let mut bytes = Vec::with_capacity(max);
    let value = cell(
        step,
        id,
        FactoryCellV1::Column(column),
        read,
        work,
        &mut bytes,
    )?;
    let FactoryCellReadV1::Value(FactoryValueV1::Ledger(info)) = value else {
        return Err(BodyMutationError::Invalid("factory ledger shape"));
    };
    let id =
        String::from_utf8(bytes).map_err(|_| BodyMutationError::Invalid("factory ledger UTF-8"))?;
    if usize::try_from(info.id_bytes).ok() != Some(id.len()) {
        return Err(BodyMutationError::Invalid("factory ledger length"));
    }
    let ledger = match info.kind {
        LedgerValueKindV1::World => MaterialLedgerId::World,
        LedgerValueKindV1::Agent => MaterialLedgerId::Agent(id),
        LedgerValueKindV1::Site => MaterialLedgerId::Site(id),
        LedgerValueKindV1::Factory => MaterialLedgerId::Factory(id),
    };
    Ok(fee.retain(ledger))
}
pub(super) fn quantity<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeWriter,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    D: FnMut() -> bool,
>(
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    ledger: &MaterialLedgerId,
    kind: &str,
    read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    work: &mut FactoryLifecycleWorkV1,
) -> Result<LedgerQuantityV1, BodyMutationError> {
    work.spend(kind.len())?;
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
    ctx.ledger(ledger, kind).map_err(overlay_error)
}
pub(super) fn amount(value: LedgerQuantityV1) -> i64 {
    match value {
        LedgerQuantityV1::Value(v) => v,
        _ => 0,
    }
}
pub(super) fn retired<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeWriter,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    D: FnMut() -> bool,
>(
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    id: &str,
    read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    work: &mut FactoryLifecycleWorkV1,
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
    oasis7_world_state::incremental_world_id_sets::world_id_set_contains_overlay_v1(
        oasis7_world_state::incremental_world_id_sets::WorldIdSetFieldV1::RetiredFactories,
        oasis7_world_state::incremental_world_id_sets::WorldIdSetKeyRefV1::Text(id),
        &mut ctx,
    )
    .map_err(overlay_error)
}
