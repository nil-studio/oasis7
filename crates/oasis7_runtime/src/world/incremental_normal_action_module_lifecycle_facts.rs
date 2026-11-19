//! Lazy sealed marketplace observations in the original preflight order.
use super::current::*;
use super::rules::{ModuleMarketFactsV1, sealed};
use super::*;
use crate::world::incremental_module_invocation::AcceptedInvocationDirectoryV1;
use crate::world::incremental_normal_action_factory_schedule::{
    FactoryScheduleReadErrorV1, with_module_lifecycle_read_io_v1,
};
use oasis7_agent_api::world_action::ResourceKind;
use oasis7_formal_protocol::incremental_value::ValueNodeReader;
use oasis7_world_state::{
    incremental_field_overlay::WorldFieldOverlayContextV1,
    incremental_field_read::WorldFieldReadContextV1,
};

type Error = FactoryScheduleReadErrorV1;
pub(super) struct CurrentMarketFactsV1<'a, 's, 'tx, 'v, 'm, 'p, 'b, 'mw, M, P, R, V, E, C, PE, PC> {
    pub(super) sources: &'a ModuleLifecycleSourcesV1<'s>,
    pub(super) ctx: &'a mut WorldFieldOverlayContextV1<'tx, 'v, 'm, M, R, E, C>,
    pub(super) parent: &'a mut WorldFieldReadContextV1<'p, P, V, PE, PC>,
    pub(super) factory: &'a mut FactoryScheduleWorkV1,
    pub(super) module: &'a mut ModuleLifecycleWorkV1<'mw>,
    pub(super) common: &'a mut FormalBoundedContextV1<'b>,
    pub(super) live: &'a ExecutionFence,
    pub(super) directory: &'a ReservedValue<AcceptedInvocationDirectoryV1>,
    pub(super) fees: [Option<MemoryReservation>; 16],
}
impl<M, P, R, V, E, C, PE, PC> sealed::Sealed
    for CurrentMarketFactsV1<'_, '_, '_, '_, '_, '_, '_, '_, M, P, R, V, E, C, PE, PC>
{
}
impl<M, P, R, V, E, C, PE, PC>
    CurrentMarketFactsV1<'_, '_, '_, '_, '_, '_, '_, '_, M, P, R, V, E, C, PE, PC>
{
    fn hold(&mut self, fee: MemoryReservation) -> Result<(), Error> {
        let slot = self
            .fees
            .iter_mut()
            .find(|v| v.is_none())
            .ok_or_else(invalid)?;
        *slot = Some(fee);
        Ok(())
    }
    fn pay_clone(&mut self, bytes: usize) -> Result<(), Error> {
        pay(self.common, bytes)?;
        let fee = self
            .common
            .memory()
            .reserve(bytes, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        self.hold(fee)
    }
}
impl<M, P, R, V, E, C, PE, PC> ModuleMarketFactsV1
    for CurrentMarketFactsV1<'_, '_, '_, '_, '_, '_, '_, '_, M, P, R, V, E, C, PE, PC>
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
    type Error = Error;
    fn reserve(&mut self, action: &crate::Action) -> Result<(), Error> {
        let fields: &[&str] = match action {
            crate::Action::ListModuleArtifactForSale {
                seller_agent_id,
                wasm_hash,
                ..
            }
            | crate::Action::DelistModuleArtifact {
                seller_agent_id,
                wasm_hash,
                ..
            } => &[seller_agent_id, wasm_hash],
            crate::Action::BuyModuleArtifact {
                buyer_agent_id,
                wasm_hash,
                ..
            } => &[buyer_agent_id, wasm_hash],
            crate::Action::PlaceModuleArtifactBid {
                bidder_agent_id,
                wasm_hash,
                ..
            }
            | crate::Action::CancelModuleArtifactBid {
                bidder_agent_id,
                wasm_hash,
                ..
            } => &[bidder_agent_id, wasm_hash],
            crate::Action::DestroyModuleArtifact {
                owner_agent_id,
                wasm_hash,
                reason,
            } => &[owner_agent_id, wasm_hash, reason],
            _ => return Err(invalid().into()),
        };
        let bytes = fields
            .iter()
            .try_fold(0usize, |n, s| n.checked_add(s.len()).ok_or_else(invalid))?
            .checked_mul(16)
            .and_then(|n| n.checked_add(16384))
            .ok_or_else(invalid)?;
        self.pay_clone(bytes)
    }
    fn agent_exists(&mut self, id: &str) -> Result<bool, Error> {
        let sources = self.sources;
        let live = self.live;
        with_module_lifecycle_read_io_v1(
            sources.families.roots,
            self.ctx,
            self.parent,
            self.factory,
            self.common,
            self.module,
            |ctx, parent, factory, _module| {
                use oasis7_world_state::incremental_agent_subtree::{
                    CurrentAgentPresenceV1, read_current_agent_presence_v1,
                };
                let guard = ctx.transaction.guard();
                Ok(read_current_agent_presence_v1(
                    id,
                    sources.factory.lifecycle.agents,
                    sources.families.roots,
                    ctx,
                    parent,
                    &mut factory.lifecycle.pending_entries_remaining,
                    &guard,
                    live,
                )? == CurrentAgentPresenceV1::Existing)
            },
        )
    }
    fn artifact_exists(&mut self, hash: &str) -> Result<bool, Error> {
        with_module_lifecycle_read_io_v1(
            self.sources.families.roots,
            self.ctx,
            self.parent,
            self.factory,
            self.common,
            self.module,
            |ctx, _, _, _| Ok(read_artifact_registered_v1(hash, ctx)?),
        )
    }
    fn owner(&mut self, hash: &str) -> Result<Option<String>, Error> {
        let owner = with_module_lifecycle_read_io_v1(
            self.sources.families.roots,
            self.ctx,
            self.parent,
            self.factory,
            self.common,
            self.module,
            |ctx, _, _, module| Ok(read_artifact_owner_v1(hash, ctx, module.max_selected_heap)?),
        )?;
        let (value, fee) = owner.into_value_v1();
        self.hold(fee)?;
        Ok(value)
    }
    fn listing(
        &mut self,
        hash: &str,
    ) -> Result<Option<oasis7_world_state::ModuleArtifactListingState>, Error> {
        let sources = self.sources;
        let live = self.live;
        let owner = with_module_lifecycle_read_io_v1(
            sources.families.roots,
            self.ctx,
            self.parent,
            self.factory,
            self.common,
            self.module,
            |ctx, _, _, module| {
                let guard = ctx.transaction.guard();
                Ok(oasis7_world_state::incremental_module_marketplace::read_current_module_lifecycle_listing_v1(hash,sources.families.marketplace,sources.families.roots,ctx,&mut module.marketplace.membership_cells_remaining,module.max_selected_heap,&guard,live)?)
            },
        )?;
        let Some(value) = owner.value() else {
            return Ok(None);
        };
        self.pay_clone(
            value
                .seller_agent_id
                .len()
                .checked_mul(8)
                .and_then(|n| n.checked_add(4096))
                .ok_or_else(invalid)?,
        )?;
        Ok(Some(value.clone()))
    }
    fn bid_exists(&mut self, hash: &str, id: u64, bidder: &str) -> Result<Option<bool>, Error> {
        let sources = self.sources;
        let live = self.live;
        with_module_lifecycle_read_io_v1(
            sources.families.roots,
            self.ctx,
            self.parent,
            self.factory,
            self.common,
            self.module,
            |ctx, parent, _, module| {
                let guard = ctx.transaction.guard();
                let owner=oasis7_world_state::incremental_module_marketplace::read_current_module_lifecycle_bids_v1(hash,sources.families.marketplace,sources.families.roots,parent.state,ctx,module.marketplace_scan,&mut module.marketplace.membership_cells_remaining,module.max_selected_heap,usize::try_from(module.marketplace.outer_rows_remaining).unwrap_or(usize::MAX),&guard,live)?;
                let Some(bids) = owner.value() else {
                    return Ok(None);
                };
                for bid in bids {
                    if (ctx.cancelled)() {
                        return Err(ValueTreeError::Cancelled.into());
                    }
                    module.marketplace.membership_cells_remaining = module
                        .marketplace
                        .membership_cells_remaining
                        .checked_sub(1)
                        .ok_or(ValueTreeError::ResourceLimited("module bid preflight rows"))?;
                    if bid.order_id == id {
                        let bytes = u64::try_from(
                            bid.bidder_agent_id
                                .len()
                                .min(bidder.len())
                                .saturating_add(1),
                        )
                        .map_err(|_| invalid())?;
                        ctx.key_work.bytes_remaining =
                            ctx.key_work.bytes_remaining.checked_sub(bytes).ok_or(
                                ValueTreeError::ResourceLimited("module bid preflight comparison"),
                            )?;
                        if bid.bidder_agent_id == bidder {
                            return Ok(Some(true));
                        }
                    }
                }
                Ok(Some(false))
            },
        )
    }
    fn active_use(&mut self, hash: &str) -> Result<bool, Error> {
        let sources = self.sources;
        let live = self.live;
        let directory = self.directory;
        with_module_lifecycle_read_io_v1(
            sources.families.roots,
            self.ctx,
            self.parent,
            self.factory,
            self.common,
            self.module,
            |ctx, parent, factory, _| {
                let guard = ctx.transaction.guard();
                Ok(
                    crate::world::incremental_module_invocation::any_current_active_artifact_v1(
                        hash,
                        directory,
                        sources.families.instances,
                        sources.families.config,
                        sources.families.roots,
                        parent.state,
                        ctx,
                        &mut factory.invocation,
                        &mut factory.config,
                        &mut factory.config_scan,
                        &guard,
                        live,
                    )
                    .map_err(enumeration_error)?,
                )
            },
        )
    }
    fn balance(&mut self, id: &str, kind: ResourceKind) -> Result<i64, Error> {
        let sources = self.sources;
        let live = self.live;
        with_module_lifecycle_read_io_v1(
            sources.families.roots,
            self.ctx,
            self.parent,
            self.factory,
            self.common,
            self.module,
            |ctx, _, _, _| Ok(read_market_balance_v1(id, kind, sources, ctx, live)?),
        )
    }
    fn next_order(&mut self) -> Result<u64, Error> {
        with_module_lifecycle_read_io_v1(
            self.sources.families.roots,
            self.ctx,
            self.parent,
            self.factory,
            self.common,
            self.module,
            |ctx, _, _, _| Ok(read_market_sequence_v1(false, ctx)?),
        )
    }
    fn next_sale(&mut self) -> Result<u64, Error> {
        with_module_lifecycle_read_io_v1(
            self.sources.families.roots,
            self.ctx,
            self.parent,
            self.factory,
            self.common,
            self.module,
            |ctx, _, _, _| Ok(read_market_sequence_v1(true, ctx)?),
        )
    }
}

fn enumeration_error(
    e: crate::world::incremental_module_invocation::CurrentInvocationEnumerationErrorV1,
) -> BodyMutationError {
    use crate::world::incremental_module_invocation::CurrentInvocationEnumerationErrorV1 as E;
    match e {
        E::Execution(e) => e.into(),
        E::Value(e) => e.into(),
        E::Body(e) => e,
        E::Overlay(e) => overlay_error(e),
    }
}

#[path = "incremental_normal_action_module_deployment_facts.rs"]
mod deployment;
