//! Fixed economic globals from actual changed outer records and the original
//! complete body qualifier/cell decoders. This owner is projection DATA only.
use super::*;
use crate::{
    incremental_agent_mutation::BodyMutationError,
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
    incremental_field_read::WorldFieldReadContextV1,
    incremental_runtime_revocation::BusinessDiffProcessingWorkV1,
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFence, ExecutionTransaction, PreparedGuard, RootSetV1},
    incremental_state::{MerkleNodeReader, StateScanBudget},
    incremental_value::{ValueNodeReader, ValueReadContext},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation, ReservedValue};
use std::{cell::RefCell, mem::size_of};
struct Row {
    field: FixedEconomyFieldV1,
    json: ReservedValue<Vec<u8>>,
}
pub struct CurrentFixedEconomyProjectionDeltaV1 {
    rows: Vec<Row>,
    semantic: Vec<u8>,
    guard: PreparedGuard,
    parent: [u8; 32],
    memory: MemoryBudget,
    _fee: MemoryReservation,
}
fn bad() -> ValueTreeError {
    ValueTreeError::Invalid("Current fixed economy projection")
}
fn limited() -> ValueTreeError {
    ValueTreeError::ResourceLimited("Current fixed economy projection")
}
fn oe(error: WorldFieldOverlayErrorV1) -> BodyMutationError {
    match error {
        WorldFieldOverlayErrorV1::Execution(e) => e.into(),
        WorldFieldOverlayErrorV1::Value(e) => e.into(),
        e => BodyMutationError::Original(Box::new(e)),
    }
}
fn pay(
    work: &mut BusinessDiffProcessingWorkV1,
    n: usize,
    stop: &mut impl FnMut() -> bool,
) -> Result<(), BodyMutationError> {
    if stop() {
        return Err(ValueTreeError::Cancelled.into());
    }
    let n = u64::try_from(n).map_err(|_| limited())?;
    work.steps_remaining = work
        .steps_remaining
        .checked_sub(n.checked_add(1).ok_or_else(limited)?)
        .ok_or_else(limited)?;
    work.bytes_remaining = work.bytes_remaining.checked_sub(n).ok_or_else(limited)?;
    Ok(())
}
impl CurrentFixedEconomyProjectionDeltaV1 {
    pub fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        tx.validate_read_source_guard(&self.guard, live)?;
        if !self.memory.same_pool(memory)
            || !self._fee.uses_budget(memory)
            || !tx.uses_budget(memory)
            || self.parent != tx.parent_fence().parent_roots
        {
            return Err(bad().into());
        }
        Ok(())
    }
    pub fn visit_with_work_v1<E>(
        &self,
        mut work: impl FnMut(u64, u64) -> Result<(), E>,
        mut emit: impl FnMut(&'static str, &[u8], &[u8], Option<&[u8]>) -> Result<(), E>,
    ) -> Result<(), E> {
        for row in &self.rows {
            work(1, 0)?;
            emit(row.field.name(), &self.semantic, b"null", Some(&row.json))?
        }
        Ok(())
    }
}
pub fn is_fixed_economy_projection_field_v1(name: &str) -> bool {
    FixedEconomyFieldV1::ALL
        .into_iter()
        .any(|f| f.name() == name)
}
fn identify(key: &[u8]) -> Result<Option<FixedEconomyFieldV1>, BodyMutationError> {
    if !WorldFieldKeyV1::is_field_key_v1(key) {
        return Ok(None);
    }
    let parsed = WorldFieldKeyV1::from_bytes(key).map_err(ValueTreeError::Invalid)?;
    let Some(field) = FixedEconomyFieldV1::ALL
        .into_iter()
        .find(|f| f.name() == parsed.name)
    else {
        return Ok(None);
    };
    let expected = WorldFieldKeyV1::new(field.descriptor()?, parsed.kind, parsed.semantic_key)
        .map_err(ValueTreeError::Invalid)?;
    if expected.as_bytes() != key || parsed.namespace != field.namespace() {
        return Err(bad().into());
    }
    Ok(Some(field))
}
fn body_json<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    field: FixedEconomyFieldV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    cells: &mut u64,
    p: &mut BusinessDiffProcessingWorkV1,
) -> Result<ReservedValue<Vec<u8>>, BodyMutationError> {
    // Only these original scalar cells form these four closed DTOs. Reading each
    // cell uses the same original record-tree/profile kernel as the old getter.
    fn scalar<
        M: MerkleNodeReader,
        R: ValueNodeReader,
        E: FnMut() -> Option<ValueTreeError>,
        C: FnMut() -> bool,
    >(
        column: FixedEconomyColumnV1,
        ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
        cells: &mut u64,
        p: &mut BusinessDiffProcessingWorkV1,
    ) -> Result<FixedEconomyValueV1, BodyMutationError> {
        pay(p, 0, &mut ctx.cancelled)?;
        *cells = cells.checked_sub(1).ok_or_else(limited)?;
        let key = read::cell_key(column.field(), Some(column), &mut ctx.records())?;
        let child = ctx.root;
        let root = crate::incremental_record_tree::lookup(
            child,
            &key.0,
            &mut ctx.records(),
            genesis::LABELS,
        )?
        .ok_or_else(bad)?;
        Ok(FixedEconomyValueV1::from_record(read::value(
            Some(column),
            &root,
            &key,
            &mut ctx.records(),
        )?)?)
    }
    macro_rules! u {
        ($c:ident) => {
            match scalar(FixedEconomyColumnV1::$c, ctx, cells, p)? {
                FixedEconomyValueV1::U64(v) => v,
                _ => return Err(bad().into()),
            }
        };
    }
    macro_rules! i {
        ($c:ident) => {
            match scalar(FixedEconomyColumnV1::$c, ctx, cells, p)? {
                FixedEconomyValueV1::I64(v) => v,
                _ => return Err(bad().into()),
            }
        };
    }
    macro_rules! b {
        ($c:ident) => {
            match scalar(FixedEconomyColumnV1::$c, ctx, cells, p)? {
                FixedEconomyValueV1::Bool(v) => v,
                _ => return Err(bad().into()),
            }
        };
    }
    let memory = ctx.values.memory.clone();
    let dto_bytes = size_of::<MainTokenSupplyState>()
        .max(size_of::<RewardAssetConfig>())
        .max(size_of::<ProtocolPowerReserve>())
        .max(size_of::<RewardSignatureGovernancePolicy>());
    pay(p, dto_bytes, &mut ctx.cancelled)?;
    let _dto = memory
        .reserve(dto_bytes, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    macro_rules! encode {
        ($dto:expr) => {
            crate::incremental_world_rule_projection_delta::encode_original_projection_json_data_v1(
                &$dto,
                &memory,
                p,
                &mut ctx.cancelled,
            )
            .map_err(Into::into)
        };
    }
    match field {
        FixedEconomyFieldV1::MainTokenSupply => encode!(MainTokenSupplyState {
            total_supply: u!(TotalSupply),
            circulating_supply: u!(CirculatingSupply),
            total_issued: u!(TotalIssued),
            total_burned: u!(TotalBurned)
        }),
        FixedEconomyFieldV1::RewardAssetConfig => encode!(RewardAssetConfig {
            points_per_credit: u!(PointsPerCredit),
            credits_per_power_unit: u!(CreditsPerPowerUnit),
            max_redeem_power_per_epoch: i!(MaxRedeemPowerPerEpoch),
            min_redeem_power_unit: i!(MinRedeemPowerUnit)
        }),
        FixedEconomyFieldV1::ProtocolPowerReserve => encode!(ProtocolPowerReserve {
            epoch_index: u!(EpochIndex),
            available_power_units: i!(AvailablePowerUnits),
            redeemed_power_units: i!(RedeemedPowerUnits)
        }),
        FixedEconomyFieldV1::RewardSignatureGovernancePolicy => {
            encode!(RewardSignatureGovernancePolicy {
                require_mintsig_v2: b!(RequireMintsigV2),
                allow_mintsig_v1_fallback: b!(AllowMintsigV1Fallback),
                require_redeem_signature: b!(RequireRedeemSignature),
                require_redeem_signer_match_node_id: b!(RequireRedeemSignerMatchNodeId)
            })
        }
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Scope {
    Current,
    Qualified,
}
pub fn read_current_fixed_economy_projection_delta_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    family: &ReservedValue<FixedEconomyFamilyQualificationV1>,
    roots: &RootSetV1,
    state: &P,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    scan: &mut StateScanBudget,
    cells: &mut u64,
    pending: &mut usize,
    p: &mut BusinessDiffProcessingWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<CurrentFixedEconomyProjectionDeltaV1, BodyMutationError> {
    capture(
        family,
        roots,
        state,
        ctx,
        scan,
        cells,
        Some(pending),
        p,
        guard,
        live,
        Scope::Current,
    )
}
fn capture<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    family: &ReservedValue<FixedEconomyFamilyQualificationV1>,
    roots: &RootSetV1,
    state: &P,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    scan: &mut StateScanBudget,
    cells: &mut u64,
    pending: Option<&mut usize>,
    p: &mut BusinessDiffProcessingWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
    scope: Scope,
) -> Result<CurrentFixedEconomyProjectionDeltaV1, BodyMutationError> {
    let result = (|| {
        ctx.transaction.validate_guard(guard, live)?;
        if scope == Scope::Qualified {
            ctx.transaction.validate_bootstrap_read_only_v1(live)?;
        }
        let memory = ctx.values.memory.clone();
        if ctx.values.class != MemoryClass::Ordinary
            || !family.uses_budget(&memory)
            || family.allocation_class() != MemoryClass::Ordinary
            || !ctx.transaction.uses_budget(&memory)
        {
            return Err(bad().into());
        }
        family.validate_binding(&ctx.transaction.parent_fence().parent_roots)?;
        if roots.digest().map_err(|_| bad())? != ctx.transaction.parent_fence().parent_roots {
            return Err(bad().into());
        }
        let mut dirty = [scope == Scope::Qualified; 4];
        if let Some(pending) = pending {
            for namespace in [
                oasis7_formal_protocol::incremental_state::StateNamespaceV1::Authorization,
                oasis7_formal_protocol::incremental_state::StateNamespaceV1::Business,
            ] {
                let processing = RefCell::new(&mut *p);
                let stop = RefCell::new(&mut ctx.cancelled);
                ctx.transaction.visit_pending_writes(
                    namespace,
                    pending,
                    || pay(&mut processing.borrow_mut(), 0, &mut **stop.borrow_mut()),
                    |key, before, after| {
                        pay(
                            &mut processing.borrow_mut(),
                            key.len(),
                            &mut **stop.borrow_mut(),
                        )?;
                        if before != after {
                            if let Some(field) = identify(key)? {
                                if field.namespace() != namespace {
                                    return Err(bad().into());
                                }
                                dirty[FixedEconomyFieldV1::ALL
                                    .into_iter()
                                    .position(|f| f == field)
                                    .ok_or_else(bad)?] = true
                            }
                        }
                        Ok::<_, BodyMutationError>(())
                    },
                )?;
            }
        }
        let semantic_bytes = crate::incremental_semantic_key::SEMANTIC_KEY_DOMAIN
            .len()
            .checked_add(1)
            .ok_or_else(limited)?;
        let n = size_of::<CurrentFixedEconomyProjectionDeltaV1>()
            .checked_add(4 * size_of::<Row>())
            .and_then(|n| n.checked_add(semantic_bytes))
            .ok_or_else(limited)?;
        pay(p, n, &mut ctx.cancelled)?;
        let fee = memory
            .reserve(n, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let mut semantic = Vec::with_capacity(semantic_bytes);
        if semantic.capacity() != semantic_bytes {
            return Err(limited().into());
        }
        crate::incremental_semantic_key::write_semantic_key_v1(&(), &mut semantic)
            .map_err(|_| bad())?;
        if semantic.len() != semantic_bytes {
            return Err(bad().into());
        }
        let mut rows = Vec::with_capacity(4);
        if rows.capacity() != 4 {
            return Err(limited().into());
        }
        for (index, field) in FixedEconomyFieldV1::ALL.into_iter().enumerate() {
            pay(p, 0, &mut ctx.cancelled)?;
            if !dirty[index] {
                continue;
            }
            let current_guard = ctx.transaction.guard();
            let body = super::industrial_delivery::open_current_fixed_economy_projection_body_v1(
                field,
                family,
                roots,
                ctx,
                state,
                scan,
                cells,
                &current_guard,
                live,
            )
            .map_err(oe)?;
            let mut child_ctx = WorldFieldReadContextV1 {
                root: body.root(),
                state,
                values: ValueReadContext {
                    source: ctx.values.source,
                    memory: &memory,
                    class: MemoryClass::Ordinary,
                    io: &mut *ctx.values.io,
                },
                key_work: &mut *ctx.key_work,
                semantic_bytes: ctx.semantic_bytes,
                record_bytes: ctx.record_bytes,
                take_state_error: &mut ctx.take_state_error,
                cancelled: &mut ctx.cancelled,
            };
            let json = body_json(field, &mut child_ctx, cells, p)?;
            rows.push(Row { field, json });
        }
        if let Some(error) = (ctx.take_state_error)() {
            return Err(error.into());
        }
        pay(p, 0, &mut ctx.cancelled)?;
        let output = CurrentFixedEconomyProjectionDeltaV1 {
            rows,
            semantic,
            guard: ctx.transaction.guard(),
            parent: ctx.transaction.parent_fence().parent_roots,
            memory,
            _fee: fee,
        };
        output.validate(ctx.transaction, ctx.values.memory, live)?;
        Ok(output)
    })();
    let result = match (ctx.take_state_error)() {
        Some(e) => Err(e.into()),
        None => result,
    };
    if result.is_err() {
        let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
            ctx.transaction.contribute(|_| {
                Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
            });
    }
    result
}

pub struct QualifiedFixedEconomyProjectionColumnsV1 {
    data: CurrentFixedEconomyProjectionDeltaV1,
}
impl QualifiedFixedEconomyProjectionColumnsV1 {
    pub fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        tx.validate_bootstrap_read_only_v1(live)?;
        self.data.validate(tx, memory, live)
    }
    pub fn visit_with_work_v1<E>(
        &self,
        mut work: impl FnMut(u64, u64) -> Result<(), E>,
        mut emit: impl FnMut(&'static str, &[u8], &[u8], &[u8]) -> Result<(), E>,
    ) -> Result<(), E> {
        for row in &self.data.rows {
            work(1, 0)?;
            emit(row.field.name(), &self.data.semantic, b"null", &row.json)?
        }
        Ok(())
    }
}
pub fn read_qualified_fixed_economy_projection_columns_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    family: &ReservedValue<FixedEconomyFamilyQualificationV1>,
    roots: &RootSetV1,
    state: &P,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    scan: &mut StateScanBudget,
    cells: &mut u64,
    p: &mut BusinessDiffProcessingWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<QualifiedFixedEconomyProjectionColumnsV1, BodyMutationError> {
    let data = capture(
        family,
        roots,
        state,
        ctx,
        scan,
        cells,
        None,
        p,
        guard,
        live,
        Scope::Qualified,
    )?;
    let out = QualifiedFixedEconomyProjectionColumnsV1 { data };
    out.validate(ctx.transaction, ctx.values.memory, live)?;
    Ok(out)
}
