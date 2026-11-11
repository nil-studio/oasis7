//! Private actual Current site and location-anchor observations.
use super::incremental_industrial_eligibility_factory::IndustrialEligibilityReadWorkV1;
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionError, ExecutionFence, PreparedGuard},
    incremental_state::{MerkleNodeReader, StateScanBudget},
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
use oasis7_world_state::{
    incremental_agent_authority_records::*,
    incremental_factory_authority_records::*,
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
};
use std::io::{self, Write};

pub(super) struct CurrentProductionSiteV1 {
    pub location_id: String,
    pub owner_agent_id: String,
    pub active: bool,
    pub chunk_ready: bool,
    pub owner_authorized: bool,
}
struct MatchText<'a> {
    text: &'a [u8],
    offset: usize,
    equal: bool,
    remaining: &'a mut u64,
}
impl Write for MatchText<'_> {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        *self.remaining = self
            .remaining
            .checked_sub(b.len() as u64)
            .ok_or_else(|| io::Error::other("industrial authorization comparison work"))?;
        let end = self
            .offset
            .checked_add(b.len())
            .ok_or_else(|| io::Error::other("industrial authorization offset"))?;
        self.equal &= self.text.get(self.offset..end) == Some(b);
        self.offset = end;
        Ok(b.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
#[allow(clippy::too_many_arguments)]
pub(super) fn read_current_production_site_v1<M, R, E, C>(
    id: &str,
    owner: &str,
    family: &ReservedValue<FactoryAuthorityFamilyQualificationV1>,
    state: &M,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut IndustrialEligibilityReadWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<Option<ReservedValue<CurrentProductionSiteV1>>, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let result = (|| {
        let key = FactoryAuthorityRecordKeyV1 {
            kind: FactoryAuthorityRecordKindV1::Site,
            id,
        };
        let Some(body) = open_factory_authority_body_overlay_v1(
            key,
            family,
            state,
            ctx,
            scan,
            &mut work.cells_remaining,
            work.max_text_bytes,
            guard,
            live,
        )?
        else {
            return Ok(None);
        };
        let cap = usize::try_from(work.max_text_bytes)
            .map_err(|_| ValueTreeError::Invalid("industrial site text limit"))?;
        let bytes = cap
            .checked_mul(2)
            .and_then(|n| n.checked_add(std::mem::size_of::<CurrentProductionSiteV1>()))
            .ok_or(ValueTreeError::Invalid("industrial site allocation bound"))?;
        let fee = ctx
            .values
            .memory
            .reserve(bytes, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let mut location = Vec::with_capacity(cap);
        work.cell()?;
        let guard = &ctx.transaction.guard();
        let location_type = read_qualified_factory_authority_cell_overlay_v1(
            FactoryAuthorityCellRequestV1 {
                key,
                cell: FactoryAuthorityCellV1::LocationId,
                qualification: &body,
            },
            ctx,
            work.max_text_bytes,
            &mut location,
            guard,
            live,
        )?;
        if !matches!(location_type, Some(FactoryAuthorityValueV1::Text { .. })) {
            return Err(ValueTreeError::Invalid("industrial site location type").into());
        }
        let mut site_owner = Vec::with_capacity(cap);
        work.cell()?;
        let guard = &ctx.transaction.guard();
        let owner_type = read_qualified_factory_authority_cell_overlay_v1(
            FactoryAuthorityCellRequestV1 {
                key,
                cell: FactoryAuthorityCellV1::OwnerAgentId,
                qualification: &body,
            },
            ctx,
            work.max_text_bytes,
            &mut site_owner,
            guard,
            live,
        )?;
        if !matches!(owner_type, Some(FactoryAuthorityValueV1::Text { .. })) {
            return Err(ValueTreeError::Invalid("industrial site owner type").into());
        }
        let mut scalar = |cell,
                          work: &mut IndustrialEligibilityReadWorkV1|
         -> Result<_, WorldFieldOverlayErrorV1> {
            work.cell()?;
            let guard = &ctx.transaction.guard();
            read_qualified_factory_authority_cell_overlay_v1(
                FactoryAuthorityCellRequestV1 {
                    key,
                    cell,
                    qualification: &body,
                },
                ctx,
                work.max_text_bytes,
                &mut io::sink(),
                guard,
                live,
            )
        };
        let active = match scalar(FactoryAuthorityCellV1::Active, work)? {
            Some(FactoryAuthorityValueV1::Bool(v)) => v,
            _ => return Err(ValueTreeError::Invalid("industrial site active type").into()),
        };
        let chunk_ready = match scalar(FactoryAuthorityCellV1::ChunkReady, work)? {
            Some(FactoryAuthorityValueV1::Bool(v)) => v,
            _ => return Err(ValueTreeError::Invalid("industrial site chunk type").into()),
        };
        let count = match scalar(FactoryAuthorityCellV1::AuthorizedCount, work)? {
            Some(FactoryAuthorityValueV1::Unsigned(v)) => v,
            _ => return Err(ValueTreeError::Invalid("industrial site authorization count").into()),
        };
        let mut authorized = false;
        for index in 0..count {
            work.cell()?;
            let mut output = MatchText {
                text: owner.as_bytes(),
                offset: 0,
                equal: true,
                remaining: &mut work.comparison_bytes_remaining,
            };
            let guard = &ctx.transaction.guard();
            let value = read_qualified_factory_authority_cell_overlay_v1(
                FactoryAuthorityCellRequestV1 {
                    key,
                    cell: FactoryAuthorityCellV1::AuthorizedAgent(index),
                    qualification: &body,
                },
                ctx,
                work.max_text_bytes,
                &mut output,
                guard,
                live,
            )?;
            if !matches!(value, Some(FactoryAuthorityValueV1::Text { .. })) {
                return Err(ValueTreeError::Invalid("industrial authorized agent type").into());
            }
            authorized |= output.equal && output.offset == owner.len();
        }
        let location_id = String::from_utf8(location)
            .map_err(|_| ValueTreeError::Invalid("industrial site location UTF-8"))?;
        let owner_agent_id = String::from_utf8(site_owner)
            .map_err(|_| ValueTreeError::Invalid("industrial site owner UTF-8"))?;
        if (ctx.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(Some(fee.retain(CurrentProductionSiteV1 {
            location_id,
            owner_agent_id,
            active,
            chunk_ready,
            owner_authorized: authorized,
        })))
    })();
    if result.is_err() {
        let _: Result<(), ExecutionError> =
            ctx.transaction.contribute(|_| Err(ExecutionError::Aborted));
    }
    result
}
#[allow(clippy::too_many_arguments)]
pub(super) fn read_current_production_anchor_v1<M, R, E, C>(
    id: &str,
    family: &ReservedValue<AgentAuthorityFamilyQualificationV1>,
    state: &M,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut IndustrialEligibilityReadWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<Option<bool>, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let result = (|| {
        ctx.transaction.validate_guard(guard, live)?;
        let key = AgentAuthorityRecordKeyV1 {
            kind: AgentAuthorityRecordKindV1::LocationAnchor,
            id,
        };
        let Some(body) = open_agent_authority_body_overlay_v1(
            key,
            family,
            state,
            ctx,
            scan,
            &mut work.cells_remaining,
            work.max_text_bytes,
        )?
        else {
            return Ok(None);
        };
        work.cell()?;
        let guard = &ctx.transaction.guard();
        let value = read_qualified_agent_authority_cell_overlay_v1(
            AgentAuthorityCellRequestV1 {
                key,
                cell: AgentAuthorityCellV1::Column(AgentAuthorityColumnV1::Active),
                qualification: &body,
            },
            ctx,
            work.max_text_bytes,
            &mut io::sink(),
            guard,
            live,
        )?;
        match value {
            AgentAuthorityValueV1::Bool(active) => Ok(Some(active)),
            _ => Err(ValueTreeError::Invalid("industrial location active type").into()),
        }
    })();
    if result.is_err() {
        let _: Result<(), ExecutionError> =
            ctx.transaction.contribute(|_| Err(ExecutionError::Aborted));
    }
    result
}
