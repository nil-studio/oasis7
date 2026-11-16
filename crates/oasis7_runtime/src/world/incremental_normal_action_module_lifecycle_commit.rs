//! Exact selected Current writes for the fixed module Action state reducer.
//! These functions do not publish an Event or mint a lifecycle completion cap.
use super::current::CurrentModuleArtifactOwnerV1;
use super::*;
use oasis7_formal_protocol::incremental_value::{ValueNodeReader, ValueNodeWriter};
use oasis7_world_state::{
    incremental_field_member::MemberRecordWriteContext,
    incremental_field_overlay::WorldFieldOverlayContextV1,
    incremental_world_text_maps::{
        WorldTextMapChangeV1, WorldTextMapFieldV1, change_world_text_map_overlay_v1,
    },
};

/// Compare the original actual owner at its retained outer key and replace only
/// that owner. The final prepared Action cap also retains the real transition's
/// agent/resource/marketplace owners; this storage edit alone grants nothing.
pub(super) fn write_market_owner_v1<M, R, E, C, W, CW>(
    before: &CurrentModuleArtifactOwnerV1,
    after: Option<&str>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    max_payload: usize,
    live: &ExecutionFence,
) -> Result<(), BodyMutationError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    W: ValueNodeWriter,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    CW: FnMut() -> bool,
{
    before.validate_provenance_v1(ctx.transaction, ctx.values.memory, live)?;
    let guard = ctx.transaction.guard();
    let _changed = change_world_text_map_overlay_v1(
        WorldTextMapChangeV1 {
            field: WorldTextMapFieldV1::ModuleArtifactOwners,
            key: before.hash(),
            expected: before.value(),
            after,
            max_current_payload_bytes: u64::try_from(max_payload).map_err(|_| invalid())?,
        },
        ctx,
        write,
        &guard,
        live,
    )
    .map_err(overlay_error)?;
    Ok(())
}

pub(super) fn write_market_agent_v1<M, R, E, C, W, CW>(
    before: &super::current::CurrentModuleMarketAgentV1,
    after: &oasis7_agent_api::world_resource::ResourceStock,
    last_active: u64,
    sources: &ModuleLifecycleSourcesV1<'_>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    live: &ExecutionFence,
) -> Result<(), BodyMutationError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    W: ValueNodeReader
        + ValueNodeWriter
        + oasis7_formal_protocol::incremental_state::StatePageWriter,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    CW: FnMut() -> bool,
{
    use oasis7_world_state::{
        incremental_agent_body::{AGENT_BODY_VALUE_BYTES, AgentBodyKeyV1, AgentBodyValueV1},
        incremental_agent_fields::{AgentFieldV1, AgentScalarRefV1},
        incremental_agent_subtree::*,
        incremental_value_mutation::ValueMutationContextV1,
    };
    ctx.transaction
        .validate_transaction_identity(&before.guard)?;
    if !before._fee.uses_budget(ctx.values.memory)
        || ctx.transaction.parent_fence().parent_roots != before.parent
    {
        return Err(invalid());
    }
    for (ordinal, kind) in [
        oasis7_agent_api::world_action::ResourceKind::Electricity,
        oasis7_agent_api::world_action::ResourceKind::Data,
    ]
    .into_iter()
    .enumerate()
    {
        let after = after.amounts.get(&kind).copied();
        if before.before[ordinal] == after {
            continue;
        }
        let guard = ctx.transaction.guard();
        let mut mutation = ValueMutationContextV1 {
            sink: &mut *write.sink,
            memory: write.memory,
            io: &mut *write.io,
            cancelled: &mut write.cancelled,
        };
        change_agent_subtree_scalar_v1(
            AgentSubtreeCellChangeV1 {
                query: AgentSubtreeQualifiedRequestV1 {
                    cell: AgentSubtreeScalarRequestV1 {
                        agent: &before.id,
                        field: AgentFieldV1::ResourceAmount,
                        ordinal: ordinal as u64,
                    },
                    qualification: sources.factory.lifecycle.agents,
                },
                expected: before.before[ordinal].map(AgentScalarRefV1::I64),
                after: after.map(AgentScalarRefV1::I64),
                max_scalar_bytes: 64,
            },
            ctx,
            &mut mutation,
            &guard,
            live,
        )
        .map_err(overlay_error)?;
    }
    let key = AgentBodyKeyV1::new(&before.id).map_err(BodyMutationError::Invalid)?;
    let mut mutation = crate::world::incremental_body::BodyMutationContext {
        sink: &mut *write.sink,
        memory: write.memory,
        io: &mut *write.io,
        cancelled: &mut write.cancelled,
    };
    let raw = crate::world::incremental_body_event::read_fixed::<AGENT_BODY_VALUE_BYTES, _, _, _, _>(
        ctx.transaction,
        oasis7_formal_protocol::incremental_state::StateNamespaceV1::Business,
        key.as_bytes(),
        ctx.values.source,
        &mut mutation,
    )?;
    if raw != before.body.canonical_bytes() {
        return Err(invalid());
    }
    if before.body.last_active != last_active {
        let root = crate::world::incremental_body::write_body_value(
            &AgentBodyValueV1 {
                body_view: before.body.body_view.clone(),
                last_active,
            },
            &mut mutation,
        )?;
        ctx.transaction.write(
            oasis7_formal_protocol::incremental_state::StateNamespaceV1::Business,
            key.as_bytes(),
            Some(root),
        )?;
    }
    Ok(())
}

pub(super) struct CommittedModuleMarketStateV1 {
    guard: PreparedGuard,
    core: WorkIdentityV1,
    now: u64,
    memory: oasis7_proto::memory_budget::MemoryBudget,
    _owner: super::current::CurrentModuleArtifactOwnerV1,
    _rows: super::current::CurrentModuleMarketRowsV1,
    _agents: [Option<super::current::CurrentModuleMarketAgentV1>; 2],
    _fee: MemoryReservation,
    _mutation_fee: MemoryReservation,
}
impl CommittedModuleMarketStateV1 {
    pub(super) fn validate_evidence_v1<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        memory: &oasis7_proto::memory_budget::MemoryBudget,
        now: u64,
        identity: WorkIdentityV1,
    ) -> Result<(), BodyMutationError> {
        tx.validate_transaction_identity(&self.guard)?;
        if self.core != identity
            || self.now != now
            || !self.memory.same_pool(memory)
            || !self._fee.uses_budget(memory)
            || !tx.uses_budget(memory)
        {
            return Err(invalid());
        }
        Ok(())
    }
}
fn hash(event: &crate::DomainEvent) -> Result<&str, BodyMutationError> {
    match event {
        crate::DomainEvent::ModuleArtifactDeployed { wasm_hash, .. }
        | crate::DomainEvent::ModuleArtifactListed { wasm_hash, .. }
        | crate::DomainEvent::ModuleArtifactDelisted { wasm_hash, .. }
        | crate::DomainEvent::ModuleArtifactDestroyed { wasm_hash, .. }
        | crate::DomainEvent::ModuleArtifactBidPlaced { wasm_hash, .. }
        | crate::DomainEvent::ModuleArtifactBidCancelled { wasm_hash, .. }
        | crate::DomainEvent::ModuleArtifactSaleCompleted { wasm_hash, .. } => Ok(wasm_hash),
        _ => Err(invalid()),
    }
}

#[derive(Debug)]
struct MarketStateErrorV1 {
    cause: crate::WorldError,
    _fee: MemoryReservation,
}
impl std::fmt::Display for MarketStateErrorV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.cause)
    }
}
impl std::error::Error for MarketStateErrorV1 {}
fn world(error: crate::WorldError, fee: MemoryReservation) -> BodyMutationError {
    BodyMutationError::Original(Box::new(MarketStateErrorV1 {
        cause: error,
        _fee: fee,
    }))
}

fn write_sequence<M, R, E, C, W, CW>(
    field: oasis7_world_state::incremental_world_u64::WorldU64FieldV1,
    expected: u64,
    after: u64,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    live: &ExecutionFence,
) -> Result<(), BodyMutationError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    W: ValueNodeWriter,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    CW: FnMut() -> bool,
{
    use oasis7_world_state::incremental_world_u64::change_world_u64_overlay_v1;
    let guard = ctx.transaction.guard();
    change_world_u64_overlay_v1(field, expected, after, ctx, write, &guard, live)
        .map_err(overlay_error)?;
    Ok(())
}

/// Fixed state reducer. Called only after the private selected/Rule source and
/// original selective Accepted have been validated. All state effects complete
/// in the single paid writer child before this private committed proof exists.
pub(super) fn commit_market_event_in_child_v1<M, P, R, V, E, C, PE, PC, W, CW>(
    event: &crate::DomainEvent,
    sources: &ModuleLifecycleSourcesV1<'_>,
    state: &P,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    parent: &mut oasis7_world_state::incremental_field_read::WorldFieldReadContextV1<
        '_,
        P,
        V,
        PE,
        PC,
    >,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    factory: &mut FactoryScheduleWorkV1,
    module: &mut ModuleLifecycleWorkV1<'_>,
    tail: &mut crate::world::incremental_normal_tail::NormalTailWorkV1<'_, '_>,
    now: u64,
    live: &ExecutionFence,
) -> Result<CommittedModuleMarketStateV1, BodyMutationError>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    V: ValueNodeReader,
    W: ValueNodeReader
        + ValueNodeWriter
        + oasis7_formal_protocol::incremental_state::StatePageWriter,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
    CW: FnMut() -> bool,
{
    use super::current::*;
    use oasis7_agent_api::world_action::ResourceKind;
    use oasis7_world_state::incremental_inventory_resource_overlay::{
        change_current_world_resource_v1, read_current_world_resource_v1,
    };
    use oasis7_world_state::incremental_module_marketplace::{
        ModuleLifecycleBidEditV1, change_current_module_lifecycle_bids_v1,
        replace_current_module_lifecycle_listing_v1,
    };
    use oasis7_world_state::incremental_world_u64::{WorldU64FieldV1, read_world_u64_overlay_v1};
    let hash = hash(event)?;
    let owner = read_artifact_owner_v1(hash, ctx, module.max_selected_heap)?;
    let rows = read_market_rows_v1(hash, sources, state, ctx, module, live)?;
    let strings = event_string_bytes_v1(event)?
        .checked_mul(8)
        .and_then(|n| n.checked_add(owner.value().map_or(0, str::len)))
        .and_then(|n| n.checked_add(rows.listing.value().map_or(0, |l| l.seller_agent_id.len())))
        .and_then(|n| n.checked_add(16384))
        .ok_or_else(invalid)?;
    tail.charge(strings)?;
    let mutation_fee = ctx
        .values
        .memory
        .reserve(strings, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let error_fee = ctx
        .values
        .memory
        .reserve(strings, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    oasis7_world_state::validate_module_market_event_rows_v1(
        event,
        owner.value(),
        rows.listing.value(),
    )
    .map_err(|e| world(e, error_fee))?;
    let mut actors = [None, None];
    let (actor, fee) = match event {
        crate::DomainEvent::ModuleArtifactDeployed { publisher_agent_id, fee_kind, fee_amount, .. } => (
            Some(publisher_agent_id.as_str()), Some((*fee_kind, *fee_amount)),
        ),
        crate::DomainEvent::ModuleArtifactListed {
            seller_agent_id,
            fee_kind,
            fee_amount,
            ..
        }
        | crate::DomainEvent::ModuleArtifactDelisted {
            seller_agent_id,
            fee_kind,
            fee_amount,
            ..
        } => (
            Some(seller_agent_id.as_str()),
            Some((*fee_kind, *fee_amount)),
        ),
        crate::DomainEvent::ModuleArtifactDestroyed {
            owner_agent_id,
            fee_kind,
            fee_amount,
            ..
        } => (
            Some(owner_agent_id.as_str()),
            Some((*fee_kind, *fee_amount)),
        ),
        crate::DomainEvent::ModuleArtifactBidPlaced {
            bidder_agent_id, ..
        }
        | crate::DomainEvent::ModuleArtifactBidCancelled {
            bidder_agent_id, ..
        } => (Some(bidder_agent_id.as_str()), None),
        crate::DomainEvent::ModuleArtifactSaleCompleted { .. } => (None, None),
        _ => return Err(invalid()),
    };
    if let Some(id) = actor {
        actors[0] = read_market_agent_v1(id, sources, ctx, parent, factory, live)?;
        if let Some((kind, amount)) = fee {
            let Some(agent) = actors[0].as_mut() else {
                let f = ctx
                    .values
                    .memory
                    .reserve(strings, MemoryClass::Ordinary)
                    .map_err(ValueTreeError::Memory)?;
                return Err(world(
                    crate::WorldError::AgentNotFound {
                        agent_id: id.to_owned(),
                    },
                    f,
                ));
            };
            let guard = ctx.transaction.guard();
            let world_before =
                read_current_world_resource_v1(kind, ctx, &guard, live).map_err(overlay_error)?;
            let mut last = agent.body.last_active;
            let f = ctx
                .values
                .memory
                .reserve(strings, MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?;
            let after = oasis7_world_state::settle_module_action_fee_from_records_v1(
                &mut agent.stock,
                &mut last,
                world_before.unwrap_or(0),
                id,
                kind,
                amount,
                now,
            )
            .map_err(|e| world(e, f))?;
            if amount > 0 {
                let guard = ctx.transaction.guard();
                change_current_world_resource_v1(
                    kind,
                    world_before,
                    Some(after),
                    ctx,
                    write,
                    &guard,
                    live,
                )
                .map_err(overlay_error)?
            }
        }
    }
    match event {
        crate::DomainEvent::ModuleArtifactDeployed { publisher_agent_id, .. } => {
            write_market_owner_v1(&owner, Some(publisher_agent_id), ctx, write, module.max_selected_heap, live)?;
            let guard=ctx.transaction.guard();
            replace_current_module_lifecycle_listing_v1(&rows.listing,None,ctx,write,
                &mut module.marketplace.changed_cells_remaining,&guard,live).map_err(overlay_error)?;
            let guard=ctx.transaction.guard();
            change_current_module_lifecycle_bids_v1(&rows.bids,ModuleLifecycleBidEditV1::DeleteAll,
                ctx,write,&mut module.marketplace.changed_cells_remaining,&guard,live).map_err(overlay_error)?;
        }
        crate::DomainEvent::ModuleArtifactListed {
            seller_agent_id,
            price_kind,
            price_amount,
            order_id,
            ..
        } => {
            let after = oasis7_world_state::ModuleArtifactListingState {
                order_id: *order_id,
                seller_agent_id: seller_agent_id.clone(),
                price_kind: *price_kind,
                price_amount: *price_amount,
                listed_at: now,
            };
            let guard = ctx.transaction.guard();
            replace_current_module_lifecycle_listing_v1(
                &rows.listing,
                Some(&after),
                ctx,
                write,
                &mut module.marketplace.changed_cells_remaining,
                &guard,
                live,
            )
            .map_err(overlay_error)?;
            if *order_id > 0 {
                let before =
                    read_world_u64_overlay_v1(WorldU64FieldV1::NextModuleMarketOrderId, ctx)
                        .map_err(overlay_error)?;
                write_sequence(
                    WorldU64FieldV1::NextModuleMarketOrderId,
                    before,
                    before.max(order_id.saturating_add(1)),
                    ctx,
                    write,
                    live,
                )?
            }
        }
        crate::DomainEvent::ModuleArtifactDelisted { .. } => {
            let guard = ctx.transaction.guard();
            replace_current_module_lifecycle_listing_v1(
                &rows.listing,
                None,
                ctx,
                write,
                &mut module.marketplace.changed_cells_remaining,
                &guard,
                live,
            )
            .map_err(overlay_error)?;
        }
        crate::DomainEvent::ModuleArtifactBidPlaced {
            bidder_agent_id,
            order_id,
            price_kind,
            price_amount,
            ..
        } => {
            if actors[0].is_none() {
                let f = ctx
                    .values
                    .memory
                    .reserve(strings, MemoryClass::Ordinary)
                    .map_err(ValueTreeError::Memory)?;
                return Err(world(
                    crate::WorldError::AgentNotFound {
                        agent_id: bidder_agent_id.clone(),
                    },
                    f,
                ));
            }
            let before = read_world_u64_overlay_v1(WorldU64FieldV1::NextModuleMarketOrderId, ctx)
                .map_err(overlay_error)?;
            write_sequence(
                WorldU64FieldV1::NextModuleMarketOrderId,
                before,
                before.max(order_id.saturating_add(1)),
                ctx,
                write,
                live,
            )?;
            let bid = oasis7_world_state::ModuleArtifactBidState {
                order_id: *order_id,
                bidder_agent_id: bidder_agent_id.clone(),
                price_kind: *price_kind,
                price_amount: *price_amount,
                bid_at: now,
            };
            let guard = ctx.transaction.guard();
            change_current_module_lifecycle_bids_v1(
                &rows.bids,
                ModuleLifecycleBidEditV1::Insert(&bid),
                ctx,
                write,
                &mut module.marketplace.changed_cells_remaining,
                &guard,
                live,
            )
            .map_err(overlay_error)?;
        }
        crate::DomainEvent::ModuleArtifactBidCancelled {
            bidder_agent_id,
            order_id,
            ..
        } => {
            let guard = ctx.transaction.guard();
            change_current_module_lifecycle_bids_v1(
                &rows.bids,
                ModuleLifecycleBidEditV1::DeleteMatching {
                    order_id: *order_id,
                    bidder_agent_id,
                },
                ctx,
                write,
                &mut module.marketplace.changed_cells_remaining,
                &guard,
                live,
            )
            .map_err(overlay_error)?;
        }
        crate::DomainEvent::ModuleArtifactDestroyed { .. } => {
            write_market_owner_v1(&owner, None, ctx, write, module.max_selected_heap, live)?;
            let guard = ctx.transaction.guard();
            replace_current_module_lifecycle_listing_v1(
                &rows.listing,
                None,
                ctx,
                write,
                &mut module.marketplace.changed_cells_remaining,
                &guard,
                live,
            )
            .map_err(overlay_error)?;
            let guard = ctx.transaction.guard();
            change_current_module_lifecycle_bids_v1(
                &rows.bids,
                ModuleLifecycleBidEditV1::DeleteAll,
                ctx,
                write,
                &mut module.marketplace.changed_cells_remaining,
                &guard,
                live,
            )
            .map_err(overlay_error)?;
            use oasis7_world_state::incremental_runtime_artifact_content::{
                RuntimeArtifactContentChangeV1, RuntimeArtifactContentFieldV1,
                RuntimeArtifactContentValueV1, change_runtime_artifact_content_overlay_v1,
                read_runtime_artifact_content_overlay_v1,
            };
            let registered = match read_runtime_artifact_content_overlay_v1(
                RuntimeArtifactContentFieldV1::Registered,
                hash,
                ctx,
            )
            .map_err(overlay_error)?
            {
                RuntimeArtifactContentValueV1::Registered(v) => v,
                _ => return Err(invalid()),
            };
            let original = match read_runtime_artifact_content_overlay_v1(
                RuntimeArtifactContentFieldV1::Original,
                hash,
                ctx,
            )
            .map_err(overlay_error)?
            {
                RuntimeArtifactContentValueV1::Original(v) => v,
                _ => return Err(invalid()),
            };
            let guard = ctx.transaction.guard();
            change_runtime_artifact_content_overlay_v1(
                RuntimeArtifactContentChangeV1::Registered {
                    key: hash,
                    expected: registered,
                    after: false,
                },
                ctx,
                write,
                module.content,
                &mut |_, _| {
                    Err(ValueTreeError::Invalid(
                        "retirement cannot publish artifact bytes",
                    ))
                },
                &guard,
                live,
            )
            .map_err(overlay_error)?;
            let guard = ctx.transaction.guard();
            change_runtime_artifact_content_overlay_v1(
                RuntimeArtifactContentChangeV1::Original {
                    key: hash,
                    expected: original,
                    after: None,
                },
                ctx,
                write,
                module.content,
                &mut |_, _| {
                    Err(ValueTreeError::Invalid(
                        "retirement cannot publish artifact bytes",
                    ))
                },
                &guard,
                live,
            )
            .map_err(overlay_error)?;
        }
        crate::DomainEvent::ModuleArtifactSaleCompleted {
            buyer_agent_id,
            seller_agent_id,
            price_kind,
            price_amount,
            sale_id,
            bid_order_id,
            ..
        } => {
            actors[0] = read_market_agent_v1(seller_agent_id, sources, ctx, parent, factory, live)?;
            if actors[0].is_none() {
                let f = ctx
                    .values
                    .memory
                    .reserve(strings, MemoryClass::Ordinary)
                    .map_err(ValueTreeError::Memory)?;
                return Err(world(
                    crate::WorldError::AgentNotFound {
                        agent_id: seller_agent_id.clone(),
                    },
                    f,
                ));
            }
            actors[1] = read_market_agent_v1(buyer_agent_id, sources, ctx, parent, factory, live)?;
            if actors[1].is_none() {
                let f = ctx
                    .values
                    .memory
                    .reserve(strings, MemoryClass::Ordinary)
                    .map_err(ValueTreeError::Memory)?;
                return Err(world(
                    crate::WorldError::AgentNotFound {
                        agent_id: buyer_agent_id.clone(),
                    },
                    f,
                ));
            }
            let f = ctx
                .values
                .memory
                .reserve(strings, MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?;
            actors[1]
                .as_mut()
                .ok_or_else(invalid)?
                .stock
                .remove(*price_kind, *price_amount)
                .map_err(|e| {
                    world(
                        crate::WorldError::ResourceBalanceInvalid {
                            reason: format!("module artifact sale buyer debit failed: {e:?}"),
                        },
                        f,
                    )
                })?;
            let f = ctx
                .values
                .memory
                .reserve(strings, MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?;
            actors[0]
                .as_mut()
                .ok_or_else(invalid)?
                .stock
                .add(*price_kind, *price_amount)
                .map_err(|e| {
                    world(
                        crate::WorldError::ResourceBalanceInvalid {
                            reason: format!("module artifact sale seller credit failed: {e:?}"),
                        },
                        f,
                    )
                })?;
            write_market_owner_v1(
                &owner,
                Some(buyer_agent_id),
                ctx,
                write,
                module.max_selected_heap,
                live,
            )?;
            let guard = ctx.transaction.guard();
            replace_current_module_lifecycle_listing_v1(
                &rows.listing,
                None,
                ctx,
                write,
                &mut module.marketplace.changed_cells_remaining,
                &guard,
                live,
            )
            .map_err(overlay_error)?;
            if let Some(order_id) = bid_order_id {
                let guard = ctx.transaction.guard();
                change_current_module_lifecycle_bids_v1(
                    &rows.bids,
                    ModuleLifecycleBidEditV1::DeleteMatching {
                        order_id: *order_id,
                        bidder_agent_id: buyer_agent_id,
                    },
                    ctx,
                    write,
                    &mut module.marketplace.changed_cells_remaining,
                    &guard,
                    live,
                )
                .map_err(overlay_error)?;
            }
            if *sale_id > 0 {
                let before =
                    read_world_u64_overlay_v1(WorldU64FieldV1::NextModuleMarketSaleId, ctx)
                        .map_err(overlay_error)?;
                write_sequence(
                    WorldU64FieldV1::NextModuleMarketSaleId,
                    before,
                    before.max(sale_id.saturating_add(1)),
                    ctx,
                    write,
                    live,
                )?
            }
        }
        _ => return Err(invalid()),
    }
    for agent in actors.iter().flatten() {
        write_market_agent_v1(agent, &agent.stock, now, sources, ctx, write, live)?
    }
    // Every related write and final live check precedes the private state proof.
    ctx.transaction
        .validate_guard(&ctx.transaction.guard(), live)?;
    let proof_fee = ctx
        .values
        .memory
        .reserve(
            std::mem::size_of::<CommittedModuleMarketStateV1>(),
            MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?;
    Ok(CommittedModuleMarketStateV1 {
        guard: ctx.transaction.guard(),
        core: tail.identity(),
        now,
        memory: ctx.values.memory.clone(),
        _owner: owner,
        _rows: rows,
        _agents: actors,
        _fee: proof_fee,
        _mutation_fee: mutation_fee,
    })
}

pub(super) fn event_string_bytes_v1(
    event: &crate::DomainEvent,
) -> Result<usize, BodyMutationError> {
    let fields: &[&str] = match event {
        crate::DomainEvent::ModuleArtifactDeployed { publisher_agent_id, wasm_hash, .. } => &[publisher_agent_id, wasm_hash],
        crate::DomainEvent::ModuleArtifactListed {
            seller_agent_id,
            wasm_hash,
            ..
        }
        | crate::DomainEvent::ModuleArtifactDelisted {
            seller_agent_id,
            wasm_hash,
            ..
        } => &[seller_agent_id, wasm_hash],
        crate::DomainEvent::ModuleArtifactBidPlaced {
            bidder_agent_id,
            wasm_hash,
            ..
        }
        | crate::DomainEvent::ModuleArtifactBidCancelled {
            bidder_agent_id,
            wasm_hash,
            ..
        } => &[bidder_agent_id, wasm_hash],
        crate::DomainEvent::ModuleArtifactDestroyed {
            owner_agent_id,
            wasm_hash,
            reason,
            ..
        } => &[owner_agent_id, wasm_hash, reason],
        crate::DomainEvent::ModuleArtifactSaleCompleted {
            buyer_agent_id,
            seller_agent_id,
            wasm_hash,
            ..
        } => &[buyer_agent_id, seller_agent_id, wasm_hash],
        _ => return Err(invalid()),
    };
    fields
        .iter()
        .try_fold(0usize, |n, s| n.checked_add(s.len()).ok_or_else(invalid))
}
