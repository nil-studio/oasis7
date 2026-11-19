//! Genuine Current production facts shared by Advance and Deliver. No
//! Producing/deadline/world-binding predicate is silently added to delivery.
use super::incremental_industrial_eligibility_authority::CurrentProductionSiteV1;
use super::incremental_industrial_eligibility_factory::CurrentProductionFactoryV1;
use super::incremental_industrial_eligibility_owner::OwnedCurrentIndustrialStateOwnerV1;
use super::*;
use oasis7_proto::memory_budget::MemoryBudget;
use oasis7_world_state::incremental_world_native::WorldNativeRecoveredRecordV1;
pub(in crate::world) struct CurrentOrderProductionFactsV1 {
    agent_exists: bool,
    starter: Option<WorldNativeRecoveredRecordV1<oasis7_world_state::StarterOcClaimState>>,
    retired: bool,
    blocked: bool,
    policy: Option<WorldNativeRecoveredRecordV1<oasis7_world_state::GameplayPolicyState>>,
    factory: Option<ReservedValue<CurrentProductionFactoryV1>>,
    site: Option<ReservedValue<CurrentProductionSiteV1>>,
    anchor_active: Option<bool>,
    owner: Option<ReservedValue<Option<OwnedCurrentIndustrialStateOwnerV1>>>,
    order_id: String,
    guard: PreparedGuard,
    memory: MemoryBudget,
}
impl CurrentOrderProductionFactsV1 {
    pub(in crate::world) fn order_id(&self) -> &str {
        &self.order_id
    }
    pub(in crate::world) fn facts(&self) -> ProductionEligibilityFactsV1<'_> {
        ProductionEligibilityFactsV1 {
            primary: PrimaryProductionFactsV1 {
                agent_exists: self.agent_exists,
                starter_claim: self.starter.as_ref().map(|s| s.value().value()),
            },
            retired: self.retired,
            blocked: self.blocked,
            factory: self.factory.as_ref().map(|f| FactoryProductionFactsV1 {
                builder_agent_id: &f.builder_agent_id,
                builtin_spec_available: f.builtin_spec_available,
                spec_matches_builtin: f.spec_matches_builtin,
                recipe_slots: f.recipe_slots,
                durability_ppm: f.durability_ppm,
                input_ledger: &f.input_ledger,
                site_location_id: f.site_location_id.as_deref(),
            }),
            site: self.site.as_ref().map(|s| FactorySiteProductionFactsV1 {
                active: s.active,
                chunk_ready: s.chunk_ready,
                location_id: &s.location_id,
                owner_agent_id: &s.owner_agent_id,
                owner_authorized: s.owner_authorized,
            }),
            anchor_active: self.anchor_active,
            resolved_owner: self.owner.as_ref().and_then(|o| o.as_ref()).map(|o| {
                ResolvedProductionOwnerFactsV1 {
                    player_id: &o.player_id,
                    source_id: &o.source_id,
                    public_key: o.public_key.as_deref(),
                }
            }),
        }
    }
    pub(in crate::world) fn validate<M: MerkleNodeReader>(
        &self,
        tx: &oasis7_formal_protocol::incremental_execution::ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        live: &ExecutionFence,
    ) -> Result<(), IndustrialEligibilityErrorV1> {
        tx.validate_guard(&self.guard, live)?;
        if !self.memory.same_pool(memory) || !tx.uses_budget(memory) {
            return Err(ValueTreeError::Invalid("Current production facts scope").into());
        }
        Ok(())
    }
}
pub(super) fn from_order<M, R, E, C, P, V, ER, CR>(
    sources: &IndustrialEligibilitySourcesV1<'_, '_>,
    outer_id: &str,
    order: &oasis7_world_state::world_rules::AgentIndustrialOrderV1,
    state: &M,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    read: &mut WorldFieldReadContextV1<'_, P, V, ER, CR>,
    work: &mut IndustrialEligibilityWorkV1,
    live: &ExecutionFence,
) -> Result<ReservedValue<CurrentOrderProductionFactsV1>, IndustrialEligibilityErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    ER: FnMut() -> Option<ValueTreeError>,
    CR: FnMut() -> bool,
{
    from_order_with_virtual_policy(sources,outer_id,order,state,ctx,read,work,live,false)
}
fn from_order_with_virtual_policy<M, R, E, C, P, V, ER, CR>(
    sources: &IndustrialEligibilitySourcesV1<'_, '_>,
    outer_id: &str,
    order: &oasis7_world_state::world_rules::AgentIndustrialOrderV1,
    state: &M,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    read: &mut WorldFieldReadContextV1<'_, P, V, ER, CR>,
    work: &mut IndustrialEligibilityWorkV1,
    live: &ExecutionFence,
    virtual_blocked: bool,
) -> Result<ReservedValue<CurrentOrderProductionFactsV1>, IndustrialEligibilityErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    ER: FnMut() -> Option<ValueTreeError>,
    CR: FnMut() -> bool,
{
    use super::incremental_industrial_eligibility_owner::resolve_current_industrial_state_owner_v1;
    let memory = ctx.values.memory;
    let fee = memory
        .reserve(
            std::mem::size_of::<CurrentOrderProductionFactsV1>()
                .checked_add(outer_id.len())
                .ok_or(ValueTreeError::Invalid("industrial facts ID size"))?,
            MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?;
    let mut result = CurrentOrderProductionFactsV1 {
        agent_exists: false,
        starter: None,
        retired: false,
        blocked: false,
        policy: None,
        factory: None,
        site: None,
        anchor_active: None,
        owner: None,
        order_id: outer_id.to_owned(),
        guard: ctx.transaction.guard(),
        memory: memory.clone(),
    };
    let complete = |mut result: CurrentOrderProductionFactsV1,
                    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>|
     -> Result<_, IndustrialEligibilityErrorV1> {
        if (ctx.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        result.guard = ctx.transaction.guard();
        ctx.transaction.validate_guard(&result.guard, live)?;
        Ok(result)
    };
    let guard = ctx.transaction.guard();
    result.agent_exists = read_current_agent_presence_v1(
        &order.owner_id,
        sources.agents,
        sources.roots,
        ctx,
        read,
        &mut work.pending_entries_remaining,
        &guard,
        live,
    )? == CurrentAgentPresenceV1::Existing;
    if !result.agent_exists {
        return Ok(fee.retain(complete(result, ctx)?));
    }
    {
        let mut claims = WorldFieldReadContextV1 {
            root: sources.roots.state_root(StateNamespaceV1::Authorization),
            state: read.state,
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
        let guard = ctx.transaction.guard();
        result.starter = recover_current_starter_oc_claim_v1(
            &order.owner_id,
            sources.native,
            sources.roots,
            ctx.transaction,
            &mut claims,
            &mut work.scan,
            &mut work.typed,
            &guard,
            live,
        )?;
    }
    if validate_primary_production_facts_v1(result.facts().primary, &order.owner_id).is_err() {
        return Ok(fee.retain(complete(result, ctx)?));
    }
    work.read.cell()?;
    result.retired = world_id_set_contains_overlay_v1(
        WorldIdSetFieldV1::RetiredFactories,
        WorldIdSetKeyRefV1::Text(&order.factory_id),
        ctx,
    )?;
    if !result.retired {
        let mut policy_read = WorldFieldReadContextV1 {
            root: sources.roots.state_root(StateNamespaceV1::Business),
            state: read.state,
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
        result.policy = Some(recover_gameplay_policy_overlay_v1(
            ctx.transaction,
            live,
            &mut policy_read,
            &mut work.scan,
            &mut work.typed,
        )?);
    }
    if let Some(policy) = &result.policy {
        for id in &policy.value().value().blocked_agents {
            work.read.comparison_bytes_remaining = work
                .read
                .comparison_bytes_remaining
                .checked_sub(id.len() as u64)
                .ok_or(ValueTreeError::Invalid("industrial policy comparison work"))?;
            if (ctx.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            result.blocked |= id == &order.owner_id;
        }
    }
    // The original WorldRule temporary view marks the owner in the policy
    // only at this original policy gate; primary/retired short circuits stay.
    result.blocked |= !result.retired && virtual_blocked;
    if !result.retired && !result.blocked {
        let guard = ctx.transaction.guard();
        result.factory = read_current_production_factory_v1(
            &order.factory_id,
            sources.factories,
            ctx,
            &mut work.read,
            &guard,
            live,
        )?;
    }
    if result.retired
        || result.blocked
        || result.factory.as_ref().is_none_or(|f| {
            !f.builtin_spec_available
                || f.builder_agent_id != order.owner_id
                || !f.spec_matches_builtin
                || f.recipe_slots < 2
                || f.durability_ppm <= 0
        })
    {
        return Ok(fee.retain(complete(result, ctx)?));
    }
    let factory = result.factory.as_ref().ok_or(ValueTreeError::Invalid(
        "industrial working factory vanished",
    ))?;
    let guard = ctx.transaction.guard();
    result.site = read_current_production_site_v1(
        &factory.site_id,
        &order.owner_id,
        sources.sites,
        state,
        ctx,
        &mut work.scan,
        &mut work.read,
        &guard,
        live,
    )?;
    if result.site.is_some() {
        let guard = ctx.transaction.guard();
        result.anchor_active = read_current_production_anchor_v1(
            &order.quote.delivery_location_id,
            sources.anchors,
            state,
            ctx,
            &mut work.scan,
            &mut work.read,
            &guard,
            live,
        )?;
    }
    if production_eligibility_from_facts_v1(
        result.facts(),
        &order.owner_id,
        &order.factory_id,
        &order.quote.delivery_location_id,
    )
    .is_err()
        || factory.input_ledger != order.quote.input_ledger
    {
        return Ok(fee.retain(complete(result, ctx)?));
    }
    let guard = ctx.transaction.guard();
    result.owner = Some(resolve_current_industrial_state_owner_v1(
        sources,
        &order.owner_id,
        state,
        ctx,
        read,
        work,
        &guard,
        live,
    )?);
    Ok(fee.retain(complete(result, ctx)?))
}
/// Actual Current order source is reselected internally; callers cannot supply
/// an invented order or arbitrary facts. Business predicates remain consumers.
pub(in crate::world) fn read_current_order_production_facts_v1<M, R, E, C, P, V, ER, CR>(
    sources: &IndustrialEligibilitySourcesV1<'_, '_>,
    id: &str,
    state: &M,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    read: &mut WorldFieldReadContextV1<'_, P, V, ER, CR>,
    work: &mut IndustrialEligibilityWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<ReservedValue<CurrentOrderProductionFactsV1>, IndustrialEligibilityErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    ER: FnMut() -> Option<ValueTreeError>,
    CR: FnMut() -> bool,
{
    let result = (|| {
        ctx.transaction.validate_guard(guard, live)?;
        sources.orders.validate_current(ctx, guard, live)?;
        let guard = ctx.transaction.guard();
        let order = read_current_agent_order_v1(
            sources.orders.source(),
            id,
            state,
            ctx,
            &mut work.scan,
            &mut work.typed,
            &guard,
            live,
        )?
        .ok_or(ValueTreeError::Invalid(
            "selected Current industrial order absent",
        ))?;
        from_order(sources, id, order.value(), state, ctx, read, work, live)
    })();
    if result.is_err() {
        let _: Result<(), ExecutionError> =
            ctx.transaction.contribute(|_| Err(ExecutionError::Aborted));
    }
    result
}

/// Exact original refund provenance, independent of production/factory gates.
/// The private owner retains both original policy and resolved-owner fees.
pub(in crate::world) struct CurrentOrderRefundSourceV1 {
    order_id: String,
    order: oasis7_world_state::incremental_runtime_revocation::RevocationRecoveredV1<
        oasis7_world_state::world_rules::AgentIndustrialOrderV1,
    >,
    policy: WorldNativeRecoveredRecordV1<oasis7_world_state::GameplayPolicyState>,
    owner: Option<ReservedValue<Option<OwnedCurrentIndustrialStateOwnerV1>>>,
    blocked: bool,
    guard: PreparedGuard,
    memory: MemoryBudget,
}
impl CurrentOrderRefundSourceV1 {
    pub(in crate::world) fn order_id(&self) -> &str {
        &self.order_id
    }
    pub(in crate::world) fn is_current(&self) -> bool {
        !self.blocked
            && self
                .owner
                .as_ref()
                .and_then(|v| v.as_ref())
                .is_some_and(|owner| {
                    owner.player_id == self.order.value().quote.owner_player_id
                        && owner.source_id == self.order.value().quote.owner_source_id
                        && owner.public_key.as_ref().is_some_and(|k| !k.is_empty())
                })
    }
    pub(in crate::world) fn validate<M: MerkleNodeReader>(
        &self,
        tx: &oasis7_formal_protocol::incremental_execution::ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        live: &ExecutionFence,
    ) -> Result<(), IndustrialEligibilityErrorV1> {
        tx.validate_guard(&self.guard, live)?;
        if !self.memory.same_pool(memory) || !tx.uses_budget(memory) {
            return Err(ValueTreeError::Invalid("Current refund provenance scope").into());
        }
        Ok(())
    }
}
pub(in crate::world) fn read_current_order_refund_source_v1<M, R, E, C, P, V, ER, CR>(
    sources: &IndustrialEligibilitySourcesV1<'_, '_>,
    id: &str,
    state: &M,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    read: &mut WorldFieldReadContextV1<'_, P, V, ER, CR>,
    work: &mut IndustrialEligibilityWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<ReservedValue<CurrentOrderRefundSourceV1>, IndustrialEligibilityErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    ER: FnMut() -> Option<ValueTreeError>,
    CR: FnMut() -> bool,
{
    read_refund_with_virtual_policy(sources,id,state,ctx,read,work,guard,live,false)
}
fn read_refund_with_virtual_policy<M, R, E, C, P, V, ER, CR>(
    sources: &IndustrialEligibilitySourcesV1<'_, '_>,
    id: &str,
    state: &M,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    read: &mut WorldFieldReadContextV1<'_, P, V, ER, CR>,
    work: &mut IndustrialEligibilityWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
    virtual_blocked: bool,
) -> Result<ReservedValue<CurrentOrderRefundSourceV1>, IndustrialEligibilityErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    ER: FnMut() -> Option<ValueTreeError>,
    CR: FnMut() -> bool,
{
    let result = (|| {
        ctx.transaction.validate_guard(guard, live)?;
        sources.orders.validate_current(ctx, guard, live)?;
        let fee = ctx
            .values
            .memory
            .reserve(
                std::mem::size_of::<CurrentOrderRefundSourceV1>()
                    .checked_add(id.len())
                    .ok_or(ValueTreeError::Invalid("refund source ID size"))?,
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        let guard = ctx.transaction.guard();
        let order = read_current_agent_order_v1(
            sources.orders.source(),
            id,
            state,
            ctx,
            &mut work.scan,
            &mut work.typed,
            &guard,
            live,
        )?
        .ok_or(ValueTreeError::Invalid("Current refund order missing"))?;
        let policy = {
            let mut selected = WorldFieldReadContextV1 {
                root: sources.roots.state_root(StateNamespaceV1::Business),
                state: read.state,
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
            recover_gameplay_policy_overlay_v1(
                ctx.transaction,
                live,
                &mut selected,
                &mut work.scan,
                &mut work.typed,
            )?
        };
        let mut blocked = false;
        for agent in &policy.value().value().blocked_agents {
            work.read.comparison_bytes_remaining = work
                .read
                .comparison_bytes_remaining
                .checked_sub(agent.len() as u64)
                .ok_or(ValueTreeError::Invalid("refund policy comparison work"))?;
            if (ctx.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            if agent == &order.value().owner_id {
                blocked = true;
                break;
            }
        }
        blocked |= virtual_blocked;
        let owner = if blocked {
            None
        } else {
            let guard = ctx.transaction.guard();
            Some(super::incremental_industrial_eligibility_owner::resolve_current_industrial_state_owner_v1(sources,&order.value().owner_id,state,ctx,read,work,&guard,live)?)
        };
        if (ctx.cancelled)() || (read.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        let guard = ctx.transaction.guard();
        ctx.transaction.validate_guard(&guard, live)?;
        Ok(fee.retain(CurrentOrderRefundSourceV1 {
            order_id: id.to_owned(),
            order,
            policy,
            owner,
            blocked,
            guard,
            memory: ctx.values.memory.clone(),
        }))
    })();
    if result.is_err() {
        let _: Result<(), ExecutionError> =
            ctx.transaction.contribute(|_| Err(ExecutionError::Aborted));
    }
    result
}

/// Fixed original temporary WorldRule policy view; the private actual identity
/// dependency is revalidated before the original lazy facts kernel.
pub(in crate::world) fn read_current_world_rule_order_production_facts_v1<M,R,E,C,P,V,ER,CR>(
    view:&ReservedValue<super::world_rule_view::CurrentWorldRuleOrderViewV1>,
    sources:&IndustrialEligibilitySourcesV1<'_,'_>,id:&str,state:&M,
    ctx:&mut WorldFieldOverlayContextV1<'_,'_,'_,M,R,E,C>,
    read:&mut WorldFieldReadContextV1<'_,P,V,ER,CR>,work:&mut IndustrialEligibilityWorkV1,
    tail:&mut crate::world::incremental_normal_tail::NormalTailWorkV1<'_,'_>,
    guard:&PreparedGuard,live:&ExecutionFence,
) -> Result<ReservedValue<CurrentOrderProductionFactsV1>,crate::world::incremental_normal_world_rule_lifecycle::delivery::IndustrialDeliveryErrorV1>
where M:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool,
P:MerkleNodeReader,V:ValueNodeReader,ER:FnMut()->Option<ValueTreeError>,CR:FnMut()->bool {
    let result=(|| {
        ctx.transaction.validate_guard(guard,live)?;
        sources.orders.validate_current(ctx,guard,live).map_err(IndustrialEligibilityErrorV1::from)?;
        let current=ctx.transaction.guard();
        let order=read_current_agent_order_v1(sources.orders.source(),id,state,ctx,&mut work.scan,&mut work.typed,&current,live)?
            .ok_or(ValueTreeError::Invalid("Current world-rule view order missing"))?;
        let virtual_blocked=super::world_rule_view::validate_current_view(view,id,order.value(),sources,state,ctx,read,work,tail,live)?;
        Ok(from_order_with_virtual_policy(sources,id,order.value(),state,ctx,read,work,live,virtual_blocked)?)
    })();
    if result.is_err(){let _:Result<(),ExecutionError>=ctx.transaction.contribute(|_|Err(ExecutionError::Aborted));}
    result
}

/// Refund provenance under the same original temporary policy; a virtually
/// blocked owner short circuits before the original owner resolver.
pub(in crate::world) fn read_current_world_rule_order_refund_source_v1<M,R,E,C,P,V,ER,CR>(
    view:&ReservedValue<super::world_rule_view::CurrentWorldRuleOrderViewV1>,
    sources:&IndustrialEligibilitySourcesV1<'_,'_>,id:&str,state:&M,
    ctx:&mut WorldFieldOverlayContextV1<'_,'_,'_,M,R,E,C>,
    read:&mut WorldFieldReadContextV1<'_,P,V,ER,CR>,work:&mut IndustrialEligibilityWorkV1,
    tail:&mut crate::world::incremental_normal_tail::NormalTailWorkV1<'_,'_>,
    guard:&PreparedGuard,live:&ExecutionFence,
) -> Result<ReservedValue<CurrentOrderRefundSourceV1>,crate::world::incremental_normal_world_rule_lifecycle::delivery::IndustrialDeliveryErrorV1>
where M:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool,
P:MerkleNodeReader,V:ValueNodeReader,ER:FnMut()->Option<ValueTreeError>,CR:FnMut()->bool {
    let result=(|| {
        ctx.transaction.validate_guard(guard,live)?;
        sources.orders.validate_current(ctx,guard,live).map_err(IndustrialEligibilityErrorV1::from)?;
        let current=ctx.transaction.guard();
        let order=read_current_agent_order_v1(sources.orders.source(),id,state,ctx,&mut work.scan,&mut work.typed,&current,live)?
            .ok_or(ValueTreeError::Invalid("Current world-rule view order missing"))?;
        let virtual_blocked=super::world_rule_view::validate_current_view(view,id,order.value(),sources,state,ctx,read,work,tail,live)?;
        let current=ctx.transaction.guard();
        Ok(read_refund_with_virtual_policy(sources,id,state,ctx,read,work,&current,live,virtual_blocked)?)
    })();
    if result.is_err(){let _:Result<(),ExecutionError>=ctx.transaction.contribute(|_|Err(ExecutionError::Aborted));}
    result
}
