//! Fixed selected Current route/path recovery for recipe bindings.
//! Original outer identities and original typed cells are preserved verbatim.
use crate::{
    LogisticsPathAuthorityV1, LogisticsRouteV1, MaterialLedgerId, MaterialTransitPriority,
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
    incremental_ledger_value::LedgerValueKindV1,
    incremental_logistics_records::*,
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFence, PreparedGuard},
    incremental_state::MerkleNodeReader,
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
use std::{io::Write, mem::size_of};
pub struct FactoryScheduleLogisticsWorkV1 {
    pub cells_remaining: u64,
    pub bytes_remaining: u64,
    pub max_text_bytes: usize,
    pub max_path_routes: usize,
}
impl FactoryScheduleLogisticsWorkV1 {
    fn charge(&mut self, n: usize) -> Result<(), ValueTreeError> {
        self.cells_remaining = self
            .cells_remaining
            .checked_sub(1)
            .ok_or(ValueTreeError::ResourceLimited("recipe logistics cells"))?;
        self.bytes_remaining = self
            .bytes_remaining
            .checked_sub(u64::try_from(n).map_err(|_| invalid())?)
            .ok_or(ValueTreeError::ResourceLimited("recipe logistics bytes"))?;
        Ok(())
    }
}
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("recipe logistics Current point")
}
fn cell<M, R, E, C>(
    key: LogisticsRecordKeyV1<'_>,
    cell: LogisticsCellV1,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    w: &mut FactoryScheduleLogisticsWorkV1,
    out: &mut impl Write,
) -> Result<LogisticsCellReadV1, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    w.charge(key.id.len())?;
    read_logistics_cell_overlay_v1(key, cell, ctx, w.max_text_bytes.max(8) as u64, out)
}
fn text<M, R, E, C>(
    key: LogisticsRecordKeyV1<'_>,
    which: LogisticsCellV1,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    w: &mut FactoryScheduleLogisticsWorkV1,
) -> Result<String, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let mut bytes = Vec::with_capacity(w.max_text_bytes);
    if bytes.capacity() != w.max_text_bytes {
        return Err(invalid().into());
    }
    match cell(key, which, ctx, w, &mut bytes)? {
        LogisticsCellReadV1::Value(LogisticsValueV1::Text { utf8_bytes })
            if usize::try_from(utf8_bytes).ok() == Some(bytes.len()) =>
        {
            w.charge(bytes.len())?;
            Ok(String::from_utf8(bytes).map_err(|_| invalid())?)
        }
        _ => Err(invalid().into()),
    }
}
macro_rules! scalar {
    ($name:ident,$variant:ident,$t:ty) => {
        fn $name<M, R, E, C>(
            key: LogisticsRecordKeyV1<'_>,
            which: LogisticsCellV1,
            ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
            w: &mut FactoryScheduleLogisticsWorkV1,
        ) -> Result<$t, WorldFieldOverlayErrorV1>
        where
            M: MerkleNodeReader,
            R: ValueNodeReader,
            E: FnMut() -> Option<ValueTreeError>,
            C: FnMut() -> bool,
        {
            match cell(key, which, ctx, w, &mut std::io::sink())? {
                LogisticsCellReadV1::Value(LogisticsValueV1::$variant(n)) => Ok(n),
                _ => Err(invalid().into()),
            }
        }
    };
}
scalar!(unsigned, Unsigned, u64);
scalar!(signed, Signed, i64);
scalar!(boolean, Bool, bool);
fn ledger<M, R, E, C>(
    key: LogisticsRecordKeyV1<'_>,
    column: LogisticsColumnV1,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    w: &mut FactoryScheduleLogisticsWorkV1,
) -> Result<MaterialLedgerId, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let mut bytes = Vec::with_capacity(w.max_text_bytes);
    if bytes.capacity() != w.max_text_bytes {
        return Err(invalid().into());
    }
    let LogisticsCellReadV1::Value(LogisticsValueV1::Ledger(info)) =
        cell(key, LogisticsCellV1::Column(column), ctx, w, &mut bytes)?
    else {
        return Err(invalid().into());
    };
    if usize::try_from(info.id_bytes).ok() != Some(bytes.len()) {
        return Err(invalid().into());
    }
    w.charge(bytes.len())?;
    let id = String::from_utf8(bytes).map_err(|_| invalid())?;
    Ok(match info.kind {
        LedgerValueKindV1::World => MaterialLedgerId::World,
        LedgerValueKindV1::Agent => MaterialLedgerId::Agent(id),
        LedgerValueKindV1::Site => MaterialLedgerId::Site(id),
        LedgerValueKindV1::Factory => MaterialLedgerId::Factory(id),
    })
}
fn check<M, R, E, C>(
    key: LogisticsRecordKeyV1<'_>,
    q: &ReservedValue<LogisticsFamilyQualificationV1>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    w: &mut FactoryScheduleLogisticsWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<bool, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    ctx.transaction.validate_guard(guard, live)?;
    if ctx.values.class != MemoryClass::Ordinary
        || q.allocation_class() != MemoryClass::Ordinary
        || !q.uses_budget(ctx.values.memory)
    {
        return Err(invalid().into());
    }
    q.validate_binding(&ctx.transaction.parent_fence().parent_roots)?;
    match cell(key, LogisticsCellV1::Record, ctx, w, &mut std::io::sink())? {
        LogisticsCellReadV1::RecordAbsent => Ok(false),
        LogisticsCellReadV1::Value(LogisticsValueV1::Unit) => Ok(true),
        _ => Err(invalid().into()),
    }
}

pub fn recover_current_schedule_route_v1<M, R, E, C>(
    id: &str,
    q: &ReservedValue<LogisticsFamilyQualificationV1>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    w: &mut FactoryScheduleLogisticsWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<Option<ReservedValue<LogisticsRouteV1>>, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    use LogisticsCellV1::Column as Col;
    use LogisticsColumnV1 as F;
    let key = LogisticsRecordKeyV1 {
        kind: LogisticsRecordKindV1::Route,
        id,
    };
    if !check(key, q, ctx, w, guard, live)? {
        return Ok(None);
    }
    let bytes = w
        .max_text_bytes
        .checked_mul(5)
        .and_then(|n| n.checked_add(size_of::<LogisticsRouteV1>()))
        .ok_or_else(invalid)?;
    let fee = ctx
        .values
        .memory
        .reserve(bytes, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let route_id = text(key, Col(F::RouteId), ctx, w)?;
    let from_ledger = ledger(key, F::FromLedger, ctx, w)?;
    let to_ledger = ledger(key, F::ToLedger, ctx, w)?;
    let kind = text(key, Col(F::Kind), ctx, w)?;
    let distance_km = signed(key, Col(F::DistanceKm), ctx, w)?;
    let priority = match unsigned(key, Col(F::Priority), ctx, w)? {
        0 => MaterialTransitPriority::Standard,
        1 => MaterialTransitPriority::Urgent,
        _ => return Err(invalid().into()),
    };
    let owner_agent_id = text(key, Col(F::OwnerAgentId), ctx, w)?;
    let available = boolean(key, Col(F::Available), ctx, w)?;
    let capacity_units = signed(key, Col(F::CapacityUnits), ctx, w)?;
    let reserved_capacity_units = signed(key, Col(F::ReservedCapacityUnits), ctx, w)?;
    let tariff_electricity_per_unit = signed(key, Col(F::TariffElectricityPerUnit), ctx, w)?;
    ctx.transaction
        .validate_guard(&ctx.transaction.guard(), live)?;
    Ok(Some(fee.retain(LogisticsRouteV1 {
        route_id,
        from_ledger,
        to_ledger,
        kind,
        distance_km,
        priority,
        owner_agent_id,
        available,
        capacity_units,
        reserved_capacity_units,
        tariff_electricity_per_unit,
    })))
}
pub fn recover_current_schedule_path_v1<M, R, E, C>(
    id: &str,
    q: &ReservedValue<LogisticsFamilyQualificationV1>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    w: &mut FactoryScheduleLogisticsWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<Option<ReservedValue<LogisticsPathAuthorityV1>>, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    use LogisticsCellV1::Column as Col;
    use LogisticsColumnV1 as F;
    let key = LogisticsRecordKeyV1 {
        kind: LogisticsRecordKindV1::Path,
        id,
    };
    if !check(key, q, ctx, w, guard, live)? {
        return Ok(None);
    }
    let bytes = w
        .max_path_routes
        .checked_add(4)
        .and_then(|n| n.checked_mul(w.max_text_bytes))
        .and_then(|n| n.checked_add(w.max_path_routes.checked_mul(size_of::<String>())?))
        .and_then(|n| n.checked_add(size_of::<LogisticsPathAuthorityV1>()))
        .ok_or_else(invalid)?;
    let fee = ctx
        .values
        .memory
        .reserve(bytes, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let path_id = text(key, Col(F::PathId), ctx, w)?;
    let n = usize::try_from(unsigned(
        key,
        LogisticsCellV1::Collection(LogisticsCollectionV1::RouteIds),
        ctx,
        w,
    )?)
    .map_err(|_| invalid())?;
    if n > w.max_path_routes {
        return Err(ValueTreeError::ResourceLimited("recipe logistics path routes").into());
    }
    let mut route_ids = Vec::with_capacity(n);
    if route_ids.capacity() != n {
        return Err(invalid().into());
    }
    for i in 0..n {
        route_ids.push(text(
            key,
            LogisticsCellV1::TextItem(LogisticsCollectionV1::RouteIds, i as u64),
            ctx,
            w,
        )?);
    }
    let from_ledger = ledger(key, F::FromLedger, ctx, w)?;
    let to_ledger = ledger(key, F::ToLedger, ctx, w)?;
    let kind = text(key, Col(F::Kind), ctx, w)?;
    let settled_amount = signed(key, Col(F::SettledAmount), ctx, w)?;
    let remaining_recipe_amount = signed(key, Col(F::RemainingRecipeAmount), ctx, w)?;
    ctx.transaction
        .validate_guard(&ctx.transaction.guard(), live)?;
    Ok(Some(fee.retain(LogisticsPathAuthorityV1 {
        path_id,
        route_ids,
        from_ledger,
        to_ledger,
        kind,
        settled_amount,
        remaining_recipe_amount,
    })))
}
