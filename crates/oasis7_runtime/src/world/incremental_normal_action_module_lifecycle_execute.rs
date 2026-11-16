//! Complete selected marketplace business lane, after genuine Accepted.
use super::event::{CompletedModuleLifecycleActionV1, PreparedNormalModuleLifecycleEventV1};
use super::*;
use crate::world::{
    incremental_body_event::{AgentEventOriginalV1, materials::CorePolicyMaterialContextV1},
    incremental_module_invocation::AcceptedInvocationDirectoryV1,
    incremental_module_invocation::SelectedAcceptedDispositionV1,
    incremental_step::IwsStepContext,
};
use oasis7_formal_protocol::{
    incremental_state::StatePageWriter,
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueReadContext},
};
use oasis7_world_state::{
    incremental_field_member::MemberRecordWriteContext,
    incremental_field_overlay::WorldFieldOverlayContextV1,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_world_event_decode::WorldEventDecodeWorkV1,
};

#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn execute_market_after_accepted_v1<M, P, V, W, D, E, C, PE, PC, CW, ME, F>(
    selected: &SelectedPendingActionV1,
    outcome: &ReservedValue<RootRulePhaseOutcomeV1>,
    accepted: SelectedAcceptedDispositionV1,
    sources: &ModuleLifecycleSourcesV1<'_>,
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    materials: &mut CorePolicyMaterialContextV1<'_, M, ME>,
    module: &mut ModuleLifecycleWorkV1<'_>,
    factory: &mut FactoryScheduleWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    directory: &ReservedValue<AcceptedInvocationDirectoryV1>,
    max_event_bytes: usize,
    audit: &mut WorldEventDecodeWorkV1,
    persist: &mut F,
    live: &ExecutionFence,
) -> Result<CompletedModuleLifecycleActionV1, BodyMutationError>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    D: FnMut() -> bool,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
    CW: FnMut() -> bool,
    ME: FnMut() -> Option<ValueTreeError>,
    F: FnMut(&mut dyn AgentEventOriginalV1) -> Result<(), BodyMutationError>,
{
    let result = (|| {
        accepted.validate(selected, step.transaction, step.mutation.memory)?;
        let effective = outcome
            .override_action_for_selected_v1(selected, step.transaction)?
            .unwrap_or(&selected.envelope().action);
        if !rules::is_market_action_v1(effective)
            || !outcome.uses_budget(common.memory())
            || !step.mutation.memory.same_pool(common.memory())
        {
            return Err(invalid());
        }
        let (decision, fees) = {
            let mut ctx = WorldFieldOverlayContextV1 {
                transaction: &mut *step.transaction,
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
            let mut source = super::facts::CurrentMarketFactsV1 {
                sources,
                ctx: &mut ctx,
                parent,
                factory,
                module,
                common,
                live,
                directory,
                fees: std::array::from_fn(|_| None),
            };
            let evaluated = rules::evaluate_market_action_v1(&mut source, effective);
            let fees = source.fees;
            let decision=match evaluated {
    Ok(value)=>value,
    Err(crate::world::incremental_normal_action_factory_schedule::FactoryScheduleReadErrorV1::Body(e))=>return Err(e),
    Err(crate::world::incremental_normal_action_factory_schedule::FactoryScheduleReadErrorV1::World(cause))=>{
     return Err(BodyMutationError::Original(Box::new(MarketEvaluationCauseV1{cause,_fees:fees})))
    },
   };
            (decision, fees)
        };
        match decision {
            rules::ModuleMarketDecisionV1::Rejected(reason) => {
                let layout = common
                    .memory()
                    .reserve(
                        std::mem::size_of::<CompletedModuleLifecycleActionV1>(),
                        MemoryClass::Ordinary,
                    )
                    .map_err(ValueTreeError::Memory)?;
                let event = step.append_selected_rejection_owned(
                    materials,
                    live,
                    selected,
                    reason,
                    max_event_bytes,
                    audit,
                    persist,
                )?;
                Ok(CompletedModuleLifecycleActionV1::Rejected {
                    accepted: accepted.into_audit(),
                    event,
                    core: common.work().identity(),
                    memory: common.memory().clone(),
                    _layout: layout,
                    _prepare_fees: fees,
                })
            }
            rules::ModuleMarketDecisionV1::Event(domain) => {
                let matching = matches!(
                    domain,
                    crate::DomainEvent::ModuleArtifactListed { .. }
                        | crate::DomainEvent::ModuleArtifactBidPlaced { .. }
                );
                let first = persist_market_event_v1(
                    selected.envelope().id,
                    domain,
                    fees,
                    accepted.into_audit(),
                    sources,
                    step,
                    read,
                    parent,
                    write,
                    materials,
                    module,
                    factory,
                    common,
                    max_event_bytes,
                    persist,
                    live,
                )?;
                let second = if matching {
                    let sale = {
                        let mut ctx = WorldFieldOverlayContextV1 {
                            transaction: &mut *step.transaction,
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
                        let hash = match &first.event().body {
                            crate::WorldEventBody::Domain(
                                crate::DomainEvent::ModuleArtifactListed { wasm_hash, .. }
                                | crate::DomainEvent::ModuleArtifactBidPlaced { wasm_hash, .. },
                            ) => wasm_hash.as_str(),
                            _ => return Err(invalid()),
                        };
                        crate::world::incremental_normal_action_factory_schedule::with_module_lifecycle_read_io_v1(sources.families.roots,&mut ctx,parent,factory,common,module,|ctx,parent,factory,module|{
       let rows=current::read_market_rows_v1(hash,sources,parent.state,ctx,module,live)?;
       let Some(index)=current::select_current_market_sale_v1(&rows,sources,ctx,&mut module.marketplace.membership_cells_remaining,live)? else{return Ok(None)};
       let listing=rows.listing.value().ok_or_else(invalid)?;let bid=&rows.bids.value().ok_or_else(invalid)?[index];
       let bytes=hash.len().checked_add(listing.seller_agent_id.len()).and_then(|n|n.checked_add(bid.bidder_agent_id.len())).and_then(|n|n.checked_add(std::mem::size_of::<crate::DomainEvent>())).ok_or_else(invalid)?;
       factory.lifecycle.bytes_remaining=factory.lifecycle.bytes_remaining.checked_sub(bytes as u64).ok_or(ValueTreeError::ResourceLimited("module sale output clone"))?;
       let fee=ctx.values.memory.reserve(bytes,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
       let sale=crate::DomainEvent::ModuleArtifactSaleCompleted{buyer_agent_id:bid.bidder_agent_id.clone(),seller_agent_id:listing.seller_agent_id.clone(),wasm_hash:hash.to_owned(),price_kind:listing.price_kind,price_amount:listing.price_amount,sale_id:current::read_market_sequence_v1(true,ctx)?,listing_order_id:(listing.order_id>0).then_some(listing.order_id),bid_order_id:Some(bid.order_id)};
       Ok(Some((sale,fee)))
      }).map_err(|e|match e{crate::world::incremental_normal_action_factory_schedule::FactoryScheduleReadErrorV1::Body(e)=>e,crate::world::incremental_normal_action_factory_schedule::FactoryScheduleReadErrorV1::World(_)=>invalid()})?
                    };
                    if let Some((sale, fee)) = sale {
                        let mut fees = std::array::from_fn(|_| None);
                        fees[0] = Some(fee);
                        Some(persist_market_event_v1(
                            selected.envelope().id,
                            sale,
                            fees,
                            None,
                            sources,
                            step,
                            read,
                            parent,
                            write,
                            materials,
                            module,
                            factory,
                            common,
                            max_event_bytes,
                            persist,
                            live,
                        )?)
                    } else {
                        None
                    }
                } else {
                    None
                };
                Ok(CompletedModuleLifecycleActionV1::Applied { first, second })
            }
        }
    })();
    if result.is_err() {
        let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
            step.transaction.contribute(|_| {
                Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
            });
    }
    result
}

#[allow(clippy::too_many_arguments)]
pub(super) fn persist_market_event_v1<M, P, V, W, D, E, C, PE, PC, CW, ME, F>(
    action_id: u64,
    domain: crate::DomainEvent,
    fees: [Option<MemoryReservation>; 16],
    accepted: Option<crate::world::incremental_action_execution::SelectedActionAuditV1>,
    sources: &ModuleLifecycleSourcesV1<'_>,
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    materials: &mut CorePolicyMaterialContextV1<'_, M, ME>,
    module: &mut ModuleLifecycleWorkV1<'_>,
    factory: &mut FactoryScheduleWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    max_event_bytes: usize,
    persist: &mut F,
    live: &ExecutionFence,
) -> Result<PreparedNormalModuleLifecycleEventV1, BodyMutationError>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    D: FnMut() -> bool,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
    CW: FnMut() -> bool,
    ME: FnMut() -> Option<ValueTreeError>,
    F: FnMut(&mut dyn AgentEventOriginalV1) -> Result<(), BodyMutationError>,
{
    let now = step.time();
    let layout = common
        .memory()
        .reserve(
            std::mem::size_of::<PreparedNormalModuleLifecycleEventV1>(),
            MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?;
    let event_fee = common
        .memory()
        .reserve(
            std::mem::size_of::<crate::WorldEvent>(),
            MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?;
    let state = crate::world::incremental_step::factory_schedule_write_io::paid_with_module(
        step,
        read,
        parent,
        write,
        materials,
        factory,
        module,
        common,
        live,
        |step, read, parent, write, _materials, factory, module, tail| {
            let pages = parent.state;
            let mut ctx = WorldFieldOverlayContextV1 {
                transaction: &mut *step.transaction,
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
            commit::commit_market_event_in_child_v1(
                &domain, sources, pages, &mut ctx, parent, write, factory, module, tail, now, live,
            )
        },
    )?;
    let id = {
        let mut ctx = WorldFieldOverlayContextV1 {
            transaction: &mut *step.transaction,
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
        crate::world::incremental_normal_tail::event_preview::preview_current_normal_event_id_v1(
            &mut ctx, common,
        )?
    };
    let cap = PreparedNormalModuleLifecycleEventV1 {
        event: event_fee.retain(crate::WorldEvent {
            id,
            time: now,
            caused_by: Some(crate::CausedBy::Action(action_id)),
            body: crate::WorldEventBody::Domain(domain),
        }),
        accepted,
        guard: step.transaction.guard(),
        core: common.work().identity(),
        action_id,
        memory: common.memory().clone(),
        state: super::event::ModuleLifecycleCommittedStateV1::Market(state),
        _layout: layout,
        _prepare_fees: fees,
    };
    step.append_prepared_module_lifecycle_event_v1(
        &cap,
        materials,
        common,
        live,
        max_event_bytes,
        persist,
    )?;
    Ok(cap)
}

struct MarketEvaluationCauseV1 {
    cause: crate::WorldError,
    _fees: [Option<MemoryReservation>; 16],
}
impl std::fmt::Debug for MarketEvaluationCauseV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(&self.cause, f)
    }
}
impl std::fmt::Display for MarketEvaluationCauseV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.cause)
    }
}
impl std::error::Error for MarketEvaluationCauseV1 {}

pub(super) fn evaluation_error_v1(cause:crate::WorldError,fees:[Option<MemoryReservation>;16])->BodyMutationError {
    BodyMutationError::Original(Box::new(MarketEvaluationCauseV1{cause,_fees:fees}))
}
