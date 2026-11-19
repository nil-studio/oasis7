//! Selected marketplace Current rows. The fixed Action driver owns authority;
//! these owners retain exact storage provenance and paid selected payloads.
use super::*;
use oasis7_formal_protocol::incremental_value::ValueNodeReader;
use oasis7_world_state::{
    incremental_field_overlay::WorldFieldOverlayContextV1,
    incremental_module_marketplace::{
        CurrentModuleLifecycleBidsV1, CurrentModuleLifecycleListingV1,
        read_current_module_lifecycle_bids_v1, read_current_module_lifecycle_listing_v1,
    },
};

/// Both rows are taken from the same transaction. Later read-set revisions do
/// not upgrade them: each writer must recheck its private original outer root.
pub(super) struct CurrentModuleMarketRowsV1 {
    pub(super) listing: CurrentModuleLifecycleListingV1,
    pub(super) bids: CurrentModuleLifecycleBidsV1,
}

/// State-reducer snapshot after fixed preflight; preflight must use lazy
/// individual reads so an unrelated malformed bids row cannot precede the
/// original missing-agent/price/owner error.
/// Called inside the exclusive module IO child. The caller supplies the real
/// State reader and continuous scan/cell loans; no scan budget is constructed.
pub(super) fn read_market_rows_v1<M, P, R, E, C>(
    hash: &str,
    sources: &ModuleLifecycleSourcesV1<'_>,
    state: &P,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    work: &mut ModuleLifecycleWorkV1<'_>,
    live: &ExecutionFence,
) -> Result<CurrentModuleMarketRowsV1, BodyMutationError>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let guard = ctx.transaction.guard();
    let listing = read_current_module_lifecycle_listing_v1(
        hash,
        sources.families.marketplace,
        sources.families.roots,
        ctx,
        &mut work.marketplace.membership_cells_remaining,
        work.max_selected_heap,
        &guard,
        live,
    )
    .map_err(overlay_error)?;
    let guard = ctx.transaction.guard();
    let bids = read_current_module_lifecycle_bids_v1(
        hash,
        sources.families.marketplace,
        sources.families.roots,
        state,
        ctx,
        work.marketplace_scan,
        &mut work.marketplace.membership_cells_remaining,
        work.max_selected_heap,
        usize::try_from(work.marketplace.outer_rows_remaining).unwrap_or(usize::MAX),
        &guard,
        live,
    )
    .map_err(overlay_error)?;
    Ok(CurrentModuleMarketRowsV1 { listing, bids })
}

struct TextCountV1(usize);
impl std::io::Write for TextCountV1 {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        self.0 = self
            .0
            .checked_add(b.len())
            .ok_or_else(|| std::io::Error::other("module owner length"))?;
        Ok(b.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
/// Selected original owner text, not a boolean ownership permit. Both decoding
/// passes use the canonical Current Text codec; the full string fee stays live.
pub(super) struct CurrentModuleArtifactOwnerV1 {
    value: Option<String>,
    hash: String,
    guard: PreparedGuard,
    parent: [u8; 32],
    _fee: MemoryReservation,
}
impl CurrentModuleArtifactOwnerV1 {
    pub(super) fn value(&self) -> Option<&str> {
        self.value.as_deref()
    }
}
pub(super) fn read_artifact_owner_v1<M, R, E, C>(
    hash: &str,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    max_payload: usize,
) -> Result<CurrentModuleArtifactOwnerV1, BodyMutationError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    use oasis7_world_state::incremental_world_text_maps::{
        WorldTextMapFieldV1, read_world_text_map_overlay_v1,
    };
    let limit = u64::try_from(max_payload).map_err(|_| invalid())?;
    let mut count = TextCountV1(0);
    let first = read_world_text_map_overlay_v1(
        WorldTextMapFieldV1::ModuleArtifactOwners,
        hash,
        ctx,
        limit,
        &mut count,
    )
    .map_err(overlay_error)?;
    let bytes = count
        .0
        .checked_mul(2)
        .and_then(|n| n.checked_add(hash.len()))
        .and_then(|n| n.checked_add(2 * std::mem::size_of::<String>()))
        .ok_or_else(invalid)?;
    let fee = ctx
        .values
        .memory
        .reserve(bytes, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let value = if first.is_some() {
        let mut out = Vec::with_capacity(count.0);
        let second = read_world_text_map_overlay_v1(
            WorldTextMapFieldV1::ModuleArtifactOwners,
            hash,
            ctx,
            limit,
            &mut out,
        )
        .map_err(overlay_error)?;
        if first != second || out.len() != count.0 {
            return Err(invalid());
        }
        Some(String::from_utf8(out).map_err(|_| invalid())?)
    } else {
        None
    };
    Ok(CurrentModuleArtifactOwnerV1 {
        value,
        hash: hash.to_owned(),
        guard: ctx.transaction.guard(),
        parent: ctx.transaction.parent_fence().parent_roots,
        _fee: fee,
    })
}

pub(super) fn read_market_balance_v1<M, R, E, C>(
    id: &str,
    kind: oasis7_agent_api::world_action::ResourceKind,
    sources: &ModuleLifecycleSourcesV1<'_>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    live: &ExecutionFence,
) -> Result<i64, BodyMutationError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    use oasis7_world_state::{
        incremental_agent_fields::AgentFieldV1,
        incremental_agent_subtree::{
            AgentSubtreeQualifiedRequestV1, AgentSubtreeScalarRequestV1, AgentSubtreeValueV1,
            read_agent_subtree_scalar_overlay_v1,
        },
    };
    let ordinal = match kind {
        oasis7_agent_api::world_action::ResourceKind::Electricity => 0,
        oasis7_agent_api::world_action::ResourceKind::Data => 1,
    };
    let guard = ctx.transaction.guard();
    match read_agent_subtree_scalar_overlay_v1(
        AgentSubtreeQualifiedRequestV1 {
            cell: AgentSubtreeScalarRequestV1 {
                agent: id,
                field: AgentFieldV1::ResourceAmount,
                ordinal,
            },
            qualification: sources.factory.lifecycle.agents,
        },
        ctx,
        64,
        &mut std::io::sink(),
        &guard,
        live,
    )
    .map_err(overlay_error)?
    {
        None => Ok(0),
        Some(AgentSubtreeValueV1::I64(value)) => Ok(value),
        _ => Err(invalid()),
    }
}

/// Evaluate the original single-match policy against the actual selected rows
/// and actual Current balances. This DATA ordinal is never a sale permit.
pub(super) fn select_current_market_sale_v1<M, R, E, C>(
    rows: &CurrentModuleMarketRowsV1,
    sources: &ModuleLifecycleSourcesV1<'_>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    cells: &mut u64,
    live: &ExecutionFence,
) -> Result<Option<usize>, BodyMutationError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let Some(listing) = rows.listing.value() else {
        return Ok(None);
    };
    let Some(bids) = rows.bids.value() else {
        return Ok(None);
    };
    // Charge fixed comparisons before the original selection loop, including
    // rejected/self bids which never reach the balance-reader callback.
    let mut bytes = 0u64;
    for bid in bids {
        if (ctx.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        *cells = cells
            .checked_sub(1)
            .ok_or(ValueTreeError::ResourceLimited("module market match rows"))?;
        let n = bid
            .bidder_agent_id
            .len()
            .min(listing.seller_agent_id.len())
            .saturating_add(1);
        bytes = bytes
            .checked_add(u64::try_from(n).map_err(|_| invalid())?)
            .ok_or_else(invalid)?;
    }
    ctx.key_work.bytes_remaining =
        ctx.key_work
            .bytes_remaining
            .checked_sub(bytes)
            .ok_or(ValueTreeError::ResourceLimited(
                "module market match comparison",
            ))?;
    oasis7_world_state::select_module_market_bid_v1(listing, bids, |bid, kind| {
        read_market_balance_v1(&bid.bidder_agent_id, kind, sources, ctx, live)
    })
}

impl CurrentModuleArtifactOwnerV1 {
    pub(super) fn validate_provenance_v1<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        memory: &oasis7_proto::memory_budget::MemoryBudget,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        tx.validate_transaction_identity(&self.guard)?;
        tx.validate_guard(&tx.guard(), live)?;
        if !self._fee.uses_budget(memory)
            || !tx.uses_budget(memory)
            || tx.parent_fence().parent_roots != self.parent
        {
            return Err(invalid());
        }
        Ok(())
    }
    pub(super) fn hash(&self) -> &str {
        &self.hash
    }
}

pub(super) fn read_artifact_registered_v1<M, R, E, C>(
    hash: &str,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
) -> Result<bool, BodyMutationError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    use oasis7_world_state::incremental_runtime_artifact_content::{
        RuntimeArtifactContentFieldV1, RuntimeArtifactContentValueV1,
        read_runtime_artifact_content_overlay_v1,
    };
    match read_runtime_artifact_content_overlay_v1(
        RuntimeArtifactContentFieldV1::Registered,
        hash,
        ctx,
    )
    .map_err(overlay_error)?
    {
        RuntimeArtifactContentValueV1::Registered(value) => Ok(value),
        _ => Err(invalid()),
    }
}

pub(super) fn read_market_sequence_v1<M, R, E, C>(
    sale: bool,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
) -> Result<u64, BodyMutationError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    use oasis7_world_state::incremental_world_u64::{WorldU64FieldV1, read_world_u64_overlay_v1};
    let field = if sale {
        WorldU64FieldV1::NextModuleMarketSaleId
    } else {
        WorldU64FieldV1::NextModuleMarketOrderId
    };
    Ok(read_world_u64_overlay_v1(field, ctx)
        .map_err(overlay_error)?
        .max(1))
}

/// Complete selected stock (ResourceKind has exactly Electricity/Data) and the
/// actual fixed Agent Body. No synthetic AgentCell or mailbox is constructed.
pub(super) struct CurrentModuleMarketAgentV1 {
    pub(super) id: String,
    pub(super) stock: oasis7_agent_api::world_resource::ResourceStock,
    pub(super) before: [Option<i64>; 2],
    pub(super) body: oasis7_world_state::incremental_agent_body::AgentBodyValueV1,
    pub(super) guard: PreparedGuard,
    pub(super) parent: [u8; 32],
    pub(super) _fee: MemoryReservation,
}
pub(super) fn read_market_agent_v1<M, P, R, V, E, C, PE, PC>(
    id: &str,
    sources: &ModuleLifecycleSourcesV1<'_>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    parent: &mut oasis7_world_state::incremental_field_read::WorldFieldReadContextV1<
        '_,
        P,
        V,
        PE,
        PC,
    >,
    factory: &mut FactoryScheduleWorkV1,
    live: &ExecutionFence,
) -> Result<Option<CurrentModuleMarketAgentV1>, BodyMutationError>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    V: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
{
    use oasis7_world_state::{
        incremental_agent_body::{AGENT_BODY_VALUE_BYTES, AgentBodyKeyV1, AgentBodyValueV1},
        incremental_agent_fields::AgentFieldV1,
        incremental_agent_subtree::*,
    };
    let guard = ctx.transaction.guard();
    if read_current_agent_presence_v1(
        id,
        sources.factory.lifecycle.agents,
        sources.families.roots,
        ctx,
        parent,
        &mut factory.lifecycle.pending_entries_remaining,
        &guard,
        live,
    )
    .map_err(overlay_error)?
        == CurrentAgentPresenceV1::Absent
    {
        return Ok(None);
    }
    let bytes = id.len().checked_add(4096).ok_or_else(invalid)?;
    factory.lifecycle.bytes_remaining = factory
        .lifecycle
        .bytes_remaining
        .checked_sub(u64::try_from(bytes).map_err(|_| invalid())?)
        .ok_or(ValueTreeError::ResourceLimited(
            "module selected agent copy",
        ))?;
    let fee = ctx
        .values
        .memory
        .reserve(bytes, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let mut before = [None, None];
    let mut stock = oasis7_agent_api::world_resource::ResourceStock::new();
    for (ordinal, kind) in [
        oasis7_agent_api::world_action::ResourceKind::Electricity,
        oasis7_agent_api::world_action::ResourceKind::Data,
    ]
    .into_iter()
    .enumerate()
    {
        let guard = ctx.transaction.guard();
        let value = read_agent_subtree_scalar_overlay_v1(
            AgentSubtreeQualifiedRequestV1 {
                cell: AgentSubtreeScalarRequestV1 {
                    agent: id,
                    field: AgentFieldV1::ResourceAmount,
                    ordinal: ordinal as u64,
                },
                qualification: sources.factory.lifecycle.agents,
            },
            ctx,
            64,
            &mut std::io::sink(),
            &guard,
            live,
        )
        .map_err(overlay_error)?;
        if let Some(value) = value {
            let AgentSubtreeValueV1::I64(n) = value else {
                return Err(invalid());
            };
            before[ordinal] = Some(n);
            stock.amounts.insert(kind, n);
        }
    }
    let key = AgentBodyKeyV1::new(id).map_err(BodyMutationError::Invalid)?;
    let mut discard = crate::world::incremental_body::Discard;
    let mut mutation = crate::world::incremental_body::BodyMutationContext {
        sink: &mut discard,
        memory: ctx.values.memory,
        io: &mut *ctx.values.io,
        cancelled: &mut ctx.cancelled,
    };
    let raw = crate::world::incremental_body_event::read_fixed::<AGENT_BODY_VALUE_BYTES, _, _, _, _>(
        ctx.transaction,
        oasis7_formal_protocol::incremental_state::StateNamespaceV1::Business,
        key.as_bytes(),
        ctx.values.source,
        &mut mutation,
    )?;
    let body = AgentBodyValueV1::from_canonical_bytes(&raw).map_err(BodyMutationError::Invalid)?;
    Ok(Some(CurrentModuleMarketAgentV1 {
        id: id.to_owned(),
        stock,
        before,
        body,
        guard: ctx.transaction.guard(),
        parent: ctx.transaction.parent_fence().parent_roots,
        _fee: fee,
    }))
}

impl CurrentModuleArtifactOwnerV1 {
    pub(super) fn into_value_v1(self) -> (Option<String>, MemoryReservation) {
        (self.value, self._fee)
    }
}
