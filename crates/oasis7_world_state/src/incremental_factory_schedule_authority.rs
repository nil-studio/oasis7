//! Selected Current authority DTOs for the fixed industrial evaluator.
//! These readers grant no admission or write permission.
use crate::{
    FactoryConstructionPowerMode, FactoryConstructionPowerProfileV1, FactorySiteAuthorityV1,
    incremental_factory_authority_records::*,
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFence, PreparedGuard},
    incremental_state::{MerkleNodeReader, StateScanBudget},
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
use std::{io::Write, mem::size_of};

/// Caller-owned finite allowances. A record read never creates replacement work.
pub struct FactoryScheduleAuthorityWorkV1 {
    pub cells_remaining: u64,
    pub bytes_remaining: u64,
    pub max_text_bytes: usize,
    pub max_authorized_agents: usize,
}
impl FactoryScheduleAuthorityWorkV1 {
    fn charge(&mut self, n: usize) -> Result<(), ValueTreeError> {
        self.cells_remaining =
            self.cells_remaining
                .checked_sub(1)
                .ok_or(ValueTreeError::ResourceLimited(
                    "factory schedule authority cells",
                ))?;
        self.bytes_remaining = self
            .bytes_remaining
            .checked_sub(u64::try_from(n).map_err(|_| invalid())?)
            .ok_or(ValueTreeError::ResourceLimited(
                "factory schedule authority bytes",
            ))?;
        Ok(())
    }
}
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("factory schedule authority DTO")
}
fn layout<T>(
    texts: usize,
    items: usize,
    w: &FactoryScheduleAuthorityWorkV1,
) -> Result<usize, ValueTreeError> {
    size_of::<T>()
        .checked_add(texts.checked_mul(w.max_text_bytes).ok_or_else(invalid)?)
        .and_then(|n| n.checked_add(items.checked_mul(size_of::<String>())?))
        .ok_or_else(invalid)
}
fn cell<M, R, E, C>(
    key: FactoryAuthorityRecordKeyV1<'_>,
    which: FactoryAuthorityCellV1,
    q: &ReservedValue<FactoryAuthorityQualificationV1>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    w: &mut FactoryScheduleAuthorityWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
    out: &mut impl Write,
) -> Result<Option<FactoryAuthorityValueV1>, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    w.charge(key.id.len())?;
    ctx.transaction.validate_transaction_identity(guard)?;
    let actual = ctx.transaction.guard();
    read_qualified_factory_authority_cell_overlay_v1(
        FactoryAuthorityCellRequestV1 {
            key,
            cell: which,
            qualification: q,
        },
        ctx,
        w.max_text_bytes.max(8) as u64,
        out,
        &actual,
        live,
    )
}
fn text<M, R, E, C>(
    key: FactoryAuthorityRecordKeyV1<'_>,
    which: FactoryAuthorityCellV1,
    q: &ReservedValue<FactoryAuthorityQualificationV1>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    w: &mut FactoryScheduleAuthorityWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<Option<String>, WorldFieldOverlayErrorV1>
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
    match cell(key, which, q, ctx, w, guard, live, &mut bytes)? {
        Some(FactoryAuthorityValueV1::Text { utf8_bytes })
            if usize::try_from(utf8_bytes).ok() == Some(bytes.len()) =>
        {
            w.charge(bytes.len())?;
            Ok(Some(String::from_utf8(bytes).map_err(|_| invalid())?))
        }
        None => Ok(None),
        _ => Err(invalid().into()),
    }
}
macro_rules! scalar {
    ($name:ident,$variant:ident,$ty:ty) => {
        fn $name<M, R, E, C>(
            key: FactoryAuthorityRecordKeyV1<'_>,
            which: FactoryAuthorityCellV1,
            q: &ReservedValue<FactoryAuthorityQualificationV1>,
            ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
            w: &mut FactoryScheduleAuthorityWorkV1,
            guard: &PreparedGuard,
            live: &ExecutionFence,
        ) -> Result<$ty, WorldFieldOverlayErrorV1>
        where
            M: MerkleNodeReader,
            R: ValueNodeReader,
            E: FnMut() -> Option<ValueTreeError>,
            C: FnMut() -> bool,
        {
            match cell(key, which, q, ctx, w, guard, live, &mut std::io::sink())? {
                Some(FactoryAuthorityValueV1::$variant(n)) => Ok(n),
                _ => Err(invalid().into()),
            }
        }
    };
}
scalar!(unsigned, Unsigned, u64);
scalar!(boolean, Bool, bool);
scalar!(signed, Signed, i64);

#[allow(clippy::too_many_arguments)]
pub fn recover_current_schedule_site_v1<M, P, R, E, C>(
    id: &str,
    family: &ReservedValue<FactoryAuthorityFamilyQualificationV1>,
    state: &P,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    scan: &mut StateScanBudget,
    qualify_cells: &mut u64,
    w: &mut FactoryScheduleAuthorityWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<Option<ReservedValue<FactorySiteAuthorityV1>>, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    use FactoryAuthorityCellV1 as F;
    ctx.transaction.validate_guard(guard, live)?;
    w.charge(id.len())?;
    let key = FactoryAuthorityRecordKeyV1 {
        kind: FactoryAuthorityRecordKindV1::Site,
        id,
    };
    let Some(q) = open_factory_authority_body_overlay_v1(
        key,
        family,
        state,
        ctx,
        scan,
        qualify_cells,
        w.max_text_bytes.max(8) as u64,
        guard,
        live,
    )?
    else {
        return Ok(None);
    };
    let cap = w.max_authorized_agents;
    let fee = ctx
        .values
        .memory
        .reserve(
            layout::<FactorySiteAuthorityV1>(cap.checked_add(3).ok_or_else(invalid)?, cap, w)?,
            MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?;
    let site_id = text(key, F::SiteId, &q, ctx, w, guard, live)?.ok_or_else(invalid)?;
    let location_id = text(key, F::LocationId, &q, ctx, w, guard, live)?.ok_or_else(invalid)?;
    let owner_agent_id =
        text(key, F::OwnerAgentId, &q, ctx, w, guard, live)?.ok_or_else(invalid)?;
    let count = usize::try_from(unsigned(key, F::AuthorizedCount, &q, ctx, w, guard, live)?)
        .map_err(|_| invalid())?;
    if count > cap {
        return Err(ValueTreeError::ResourceLimited("factory schedule site allowlist").into());
    }
    let mut authorized_agent_ids = Vec::with_capacity(count);
    if authorized_agent_ids.capacity() != count {
        return Err(invalid().into());
    }
    for i in 0..count {
        authorized_agent_ids.push(
            text(key, F::AuthorizedAgent(i as u64), &q, ctx, w, guard, live)?
                .ok_or_else(invalid)?,
        );
    }
    let chunk_ready = boolean(key, F::ChunkReady, &q, ctx, w, guard, live)?;
    let active = boolean(key, F::Active, &q, ctx, w, guard, live)?;
    let authority_revision = unsigned(key, F::AuthorityRevision, &q, ctx, w, guard, live)?;
    let registered_at = unsigned(key, F::RegisteredAt, &q, ctx, w, guard, live)?;
    ctx.transaction
        .validate_guard(&ctx.transaction.guard(), live)?;
    Ok(Some(fee.retain(FactorySiteAuthorityV1 {
        site_id,
        location_id,
        owner_agent_id,
        authorized_agent_ids,
        chunk_ready,
        active,
        authority_revision,
        registered_at,
    })))
}

#[allow(clippy::too_many_arguments)]
pub fn recover_current_schedule_power_profile_v1<M, P, R, E, C>(
    id: &str,
    family: &ReservedValue<FactoryAuthorityFamilyQualificationV1>,
    state: &P,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    scan: &mut StateScanBudget,
    qualify_cells: &mut u64,
    w: &mut FactoryScheduleAuthorityWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<Option<ReservedValue<FactoryConstructionPowerProfileV1>>, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    use FactoryAuthorityCellV1 as F;
    ctx.transaction.validate_guard(guard, live)?;
    w.charge(id.len())?;
    let key = FactoryAuthorityRecordKeyV1 {
        kind: FactoryAuthorityRecordKindV1::Power,
        id,
    };
    let Some(q) = open_factory_authority_body_overlay_v1(
        key,
        family,
        state,
        ctx,
        scan,
        qualify_cells,
        w.max_text_bytes.max(8) as u64,
        guard,
        live,
    )?
    else {
        return Ok(None);
    };
    let fee = ctx
        .values
        .memory
        .reserve(
            layout::<FactoryConstructionPowerProfileV1>(3, 0, w)?,
            MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?;
    let factory_id = text(key, F::FactoryId, &q, ctx, w, guard, live)?.ok_or_else(invalid)?;
    let factory_kind = text(key, F::FactoryKind, &q, ctx, w, guard, live)?.ok_or_else(invalid)?;
    // The original reader checks the paired optional flag and value.
    let source_module_id = text(key, F::SourceModuleId, &q, ctx, w, guard, live)?;
    let electricity_amount = signed(key, F::ElectricityAmount, &q, ctx, w, guard, live)?;
    let mode = match unsigned(key, F::Mode, &q, ctx, w, guard, live)? {
        0 => FactoryConstructionPowerMode::StartOnlySink,
        _ => return Err(invalid().into()),
    };
    let authority_revision = unsigned(key, F::AuthorityRevision, &q, ctx, w, guard, live)?;
    let active = boolean(key, F::Active, &q, ctx, w, guard, live)?;
    ctx.transaction
        .validate_guard(&ctx.transaction.guard(), live)?;
    Ok(Some(fee.retain(FactoryConstructionPowerProfileV1 {
        factory_id,
        factory_kind,
        source_module_id,
        electricity_amount,
        mode,
        authority_revision,
        active,
    })))
}

use crate::{AgentLocationAuthorityV1, LocationAnchorV1, incremental_agent_authority_records::*};
fn agent_cell<M, R, E, C>(
    key: AgentAuthorityRecordKeyV1<'_>,
    column: AgentAuthorityColumnV1,
    q: &ReservedValue<AgentAuthorityRecordQualificationV1>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    w: &mut FactoryScheduleAuthorityWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
    out: &mut impl Write,
) -> Result<AgentAuthorityValueV1, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    w.charge(key.id.len())?;
    ctx.transaction.validate_transaction_identity(guard)?;
    let actual = ctx.transaction.guard();
    read_qualified_agent_authority_cell_overlay_v1(
        AgentAuthorityCellRequestV1 {
            key,
            cell: AgentAuthorityCellV1::Column(column),
            qualification: q,
        },
        ctx,
        w.max_text_bytes.max(8) as u64,
        out,
        &actual,
        live,
    )
}
fn agent_text<M, R, E, C>(
    key: AgentAuthorityRecordKeyV1<'_>,
    column: AgentAuthorityColumnV1,
    q: &ReservedValue<AgentAuthorityRecordQualificationV1>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    w: &mut FactoryScheduleAuthorityWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
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
    match agent_cell(key, column, q, ctx, w, guard, live, &mut bytes)? {
        AgentAuthorityValueV1::Text { utf8_bytes }
            if usize::try_from(utf8_bytes).ok() == Some(bytes.len()) =>
        {
            w.charge(bytes.len())?;
            Ok(String::from_utf8(bytes).map_err(|_| invalid())?)
        }
        _ => Err(invalid().into()),
    }
}
macro_rules! agent_scalar {
    ($name:ident,$variant:ident,$ty:ty) => {
        fn $name<M, R, E, C>(
            key: AgentAuthorityRecordKeyV1<'_>,
            column: AgentAuthorityColumnV1,
            q: &ReservedValue<AgentAuthorityRecordQualificationV1>,
            ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
            w: &mut FactoryScheduleAuthorityWorkV1,
            guard: &PreparedGuard,
            live: &ExecutionFence,
        ) -> Result<$ty, WorldFieldOverlayErrorV1>
        where
            M: MerkleNodeReader,
            R: ValueNodeReader,
            E: FnMut() -> Option<ValueTreeError>,
            C: FnMut() -> bool,
        {
            match agent_cell(key, column, q, ctx, w, guard, live, &mut std::io::sink())? {
                AgentAuthorityValueV1::$variant(n) => Ok(n),
                _ => Err(invalid().into()),
            }
        }
    };
}
agent_scalar!(agent_unsigned, Unsigned, u64);
agent_scalar!(agent_boolean, Bool, bool);

#[allow(clippy::too_many_arguments)]
pub fn recover_current_schedule_agent_location_v1<M, P, R, E, C>(
    id: &str,
    family: &ReservedValue<AgentAuthorityFamilyQualificationV1>,
    state: &P,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    scan: &mut StateScanBudget,
    qualify_cells: &mut u64,
    w: &mut FactoryScheduleAuthorityWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<Option<ReservedValue<AgentLocationAuthorityV1>>, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    use AgentAuthorityColumnV1 as F;
    ctx.transaction.validate_guard(guard, live)?;
    w.charge(id.len())?;
    let key = AgentAuthorityRecordKeyV1 {
        kind: AgentAuthorityRecordKindV1::AgentLocation,
        id,
    };
    let Some(q) = open_agent_authority_body_overlay_v1(
        key,
        family,
        state,
        ctx,
        scan,
        qualify_cells,
        w.max_text_bytes.max(8) as u64,
    )?
    else {
        return Ok(None);
    };
    let fee = ctx
        .values
        .memory
        .reserve(
            layout::<AgentLocationAuthorityV1>(2, 0, w)?,
            MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?;
    let agent_id = agent_text(key, F::AgentId, &q, ctx, w, guard, live)?;
    let location_id = agent_text(key, F::LocationId, &q, ctx, w, guard, live)?;
    let active = agent_boolean(key, F::Active, &q, ctx, w, guard, live)?;
    let authority_revision = agent_unsigned(key, F::AuthorityRevision, &q, ctx, w, guard, live)?;
    let effective_at = agent_unsigned(key, F::EffectiveAt, &q, ctx, w, guard, live)?;
    ctx.transaction
        .validate_guard(&ctx.transaction.guard(), live)?;
    Ok(Some(fee.retain(AgentLocationAuthorityV1 {
        agent_id,
        location_id,
        active,
        authority_revision,
        effective_at,
    })))
}
#[allow(clippy::too_many_arguments)]
pub fn recover_current_schedule_location_anchor_v1<M, P, R, E, C>(
    id: &str,
    family: &ReservedValue<AgentAuthorityFamilyQualificationV1>,
    state: &P,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    scan: &mut StateScanBudget,
    qualify_cells: &mut u64,
    w: &mut FactoryScheduleAuthorityWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<Option<ReservedValue<LocationAnchorV1>>, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    use AgentAuthorityColumnV1 as F;
    ctx.transaction.validate_guard(guard, live)?;
    w.charge(id.len())?;
    let key = AgentAuthorityRecordKeyV1 {
        kind: AgentAuthorityRecordKindV1::LocationAnchor,
        id,
    };
    let Some(q) = open_agent_authority_body_overlay_v1(
        key,
        family,
        state,
        ctx,
        scan,
        qualify_cells,
        w.max_text_bytes.max(8) as u64,
    )?
    else {
        return Ok(None);
    };
    let fee = ctx
        .values
        .memory
        .reserve(layout::<LocationAnchorV1>(1, 0, w)?, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let location_id = agent_text(key, F::LocationId, &q, ctx, w, guard, live)?;
    let active = agent_boolean(key, F::Active, &q, ctx, w, guard, live)?;
    let authority_revision = agent_unsigned(key, F::AuthorityRevision, &q, ctx, w, guard, live)?;
    let effective_at = agent_unsigned(key, F::EffectiveAt, &q, ctx, w, guard, live)?;
    ctx.transaction
        .validate_guard(&ctx.transaction.guard(), live)?;
    Ok(Some(fee.retain(LocationAnchorV1 {
        location_id,
        active,
        authority_revision,
        effective_at,
    })))
}
