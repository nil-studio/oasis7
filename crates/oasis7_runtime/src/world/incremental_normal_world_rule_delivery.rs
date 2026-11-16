//! Complete system-lifecycle industrial delivery. A FreshHeader and genuine
//! selected Current sources precede the fixed business reducer and every CAS.
//! The final private cap exists only after the entire write set succeeds.
use super::*;
use crate::world::{
    incremental_body_event::materials::CorePolicyMaterialContextV1,
    incremental_industrial_eligibility::{
        IndustrialEligibilityErrorV1, IndustrialEligibilitySourcesV1, IndustrialEligibilityWorkV1,
        read_current_order_production_facts_v1, read_current_order_refund_source_v1,
        validate_current_order_budgeted_v1,
    },
    incremental_normal_tail::NormalTailWorkV1,
};
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::FormalBoundedContextV1,
    incremental_execution::RootSetV1,
    incremental_state::{StateNamespaceV1, StatePageWriter, StateScanBudget},
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::MemoryClass;
use oasis7_world_state::{
    incremental_field_member::MemberRecordWriteContext,
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
    incremental_field_read::WorldFieldReadContextV1,
    incremental_fixed_economy_records::FixedEconomyFamilyQualificationV1,
    incremental_world_native::*,
    incremental_world_rule_receipts::{AgentOrdersOverlayV1, AgentOrdersQualificationV1},
};
use std::mem::size_of;
#[path = "incremental_normal_world_rule_delivery_economy.rs"]
pub(in crate::world) mod economy;
#[path = "incremental_normal_world_rule_delivery_io.rs"]
pub(in crate::world) mod paid_io;
#[path="incremental_normal_world_rule_delivery_acquisition.rs"] mod acquisition;
pub(in crate::world) use acquisition::validate_current_lifecycle_delivery_acquisition_v1;
pub(in crate::world) struct IndustrialDeliverySourcesV1<'a, 'source> {
    pub eligibility: IndustrialEligibilitySourcesV1<'a, 'source>,
    pub claims: &'a ReservedValue<IndustrialClaimDirectoryV1>,
    pub fixed: &'a ReservedValue<FixedEconomyFamilyQualificationV1>,
    pub orders_parent: &'a ReservedValue<AgentOrdersQualificationV1>,
}
pub(in crate::world) struct IndustrialDeliveryWorkV1 {
    pub eligibility: IndustrialEligibilityWorkV1,
    pub claims: IndustrialClaimDirectoryWorkV1,
    pub fixed_scan: StateScanBudget,
    pub fixed_cells_remaining: u64,
    pub native_write: oasis7_world_state::incremental_world_state_genesis::TypedRecordWriteWorkV1,
    pub records_remaining: u64,
    pub composite_read:
        oasis7_world_state::incremental_world_rule_composite::WorldRuleCompositeReadWorkV1,
    pub composite_write:
        oasis7_world_state::incremental_world_rule_composite::WorldRuleCompositeWriteWorkV1,
    pub max_identity_bytes: usize,
    pub max_pending_node_bytes: usize,
}
#[derive(Debug)]
pub(in crate::world) enum IndustrialDeliveryErrorV1 {
    Body(BodyMutationError),
    World(crate::WorldError),
    Eligibility(IndustrialEligibilityErrorV1),
}
impl From<BodyMutationError> for IndustrialDeliveryErrorV1 {
    fn from(e: BodyMutationError) -> Self {
        Self::Body(e)
    }
}
impl From<crate::WorldError> for IndustrialDeliveryErrorV1 {
    fn from(e: crate::WorldError) -> Self {
        Self::World(e)
    }
}
impl From<IndustrialEligibilityErrorV1> for IndustrialDeliveryErrorV1 {
    fn from(e: IndustrialEligibilityErrorV1) -> Self {
        Self::Eligibility(e)
    }
}
impl From<ValueTreeError> for IndustrialDeliveryErrorV1 {
    fn from(e: ValueTreeError) -> Self {
        Self::Body(e.into())
    }
}
impl From<oasis7_formal_protocol::incremental_execution::ExecutionError>
    for IndustrialDeliveryErrorV1
{
    fn from(e: oasis7_formal_protocol::incremental_execution::ExecutionError) -> Self {
        Self::Body(e.into())
    }
}
impl From<WorldFieldOverlayErrorV1> for IndustrialDeliveryErrorV1 {
    fn from(e: WorldFieldOverlayErrorV1) -> Self {
        Self::Body(match e {
            WorldFieldOverlayErrorV1::Execution(e) => e.into(),
            WorldFieldOverlayErrorV1::Value(e) => e.into(),
            WorldFieldOverlayErrorV1::Member(e) => BodyMutationError::Original(Box::new(e)),
        })
    }
}
fn invalid() -> IndustrialDeliveryErrorV1 {
    BodyMutationError::Invalid("Current industrial delivery owner").into()
}
fn denied(reason: impl Into<String>) -> IndustrialDeliveryErrorV1 {
    crate::WorldError::ResourceBalanceInvalid {
        reason: reason.into(),
    }
    .into()
}
pub(in crate::world) struct CommittedWorldRuleOrderDeliveryV1 {
    target: ReservedValue<String>,
    orders: ReservedValue<AgentOrdersOverlayV1>,
    guard: PreparedGuard,
    event: crate::DomainEvent,
    identity: WorkIdentityV1,
    memory: MemoryBudget,
    now: u64,
    _phase: MemoryReservation,
}
impl CommittedWorldRuleOrderDeliveryV1 {
    pub(in crate::world) fn target_agent_id(&self) -> &str {
        &self.target
    }
    pub(in crate::world) fn orders_overlay(&self) -> &ReservedValue<AgentOrdersOverlayV1> {
        &self.orders
    }
    pub(in crate::world) fn validate<M:MerkleNodeReader>(&self,header:&header::PreparedFreshLifecycleHeaderV1,tx:&ExecutionTransaction<'_,M>,now:u64,tail:&mut NormalTailWorkV1<'_,'_>,live:&ExecutionFence)->Result<(),BodyMutationError>{
        tx.validate_guard(&self.guard,live)?;
        let memory=tail.memory();
        if self.identity!=tail.identity()||!self.memory.same_pool(memory)||!self.target.uses_budget(memory)||!self.orders.uses_budget(memory)||!tx.uses_budget(memory)||self.now!=now{return Err(BodyMutationError::Invalid("industrial Delivered complete write source"));}
        // Compare only the complete, closed Deliver header. A foreign command
        // cannot smuggle an unrelated graph comparison into this source fee.
        let (crate::DomainEvent::WorldRuleCommitted {action_id:a,actor_id:aa,request_id:ar,command:oasis7_agent_api::world_action::WorldRuleCommandV1::DeliverAgentOrder {order_id:ao},governance_epoch:ae,world_binding:ab},crate::DomainEvent::WorldRuleCommitted {action_id:b,actor_id:ba,request_id:br,command:oasis7_agent_api::world_action::WorldRuleCommandV1::DeliverAgentOrder {order_id:bo},governance_epoch:be,world_binding:bb})=(&self.event,header.event()) else{return Err(BodyMutationError::Invalid("industrial Delivered exact header kind"));};
        let comparisons=aa.len().min(ba.len()).checked_add(ar.len().min(br.len())).and_then(|n|n.checked_add(ao.len().min(bo.len()))).and_then(|n|n.checked_add(match(ab,bb){(Some(a),Some(b))=>a.len().min(b.len()),_=>0})).ok_or(BodyMutationError::Invalid("industrial header comparison bound"))?;
        tail.charge(comparisons)?;
        if a!=b||aa!=ba||ar!=br||ao!=bo||ae!=be||ab!=bb{return Err(BodyMutationError::Invalid("industrial Delivered exact header contents"));}Ok(())
    }

}
/// The outer Common adapter includes Current/member/parent IO and all typed
/// work deltas. No invocation creates or refills a source work budget.
pub(in crate::world) fn deliver_current_industrial_order_v1<M, P, V, R, W, E, C, PE, PC, CW, ME>(
    header: &header::PreparedFreshLifecycleHeaderV1,
    sources: &IndustrialDeliverySourcesV1<'_, '_>,
    state: &M,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    materials: &mut CorePolicyMaterialContextV1<'_, M, ME>,
    work: &mut IndustrialDeliveryWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    now: u64,
    live: &ExecutionFence,
) -> Result<CommittedWorldRuleOrderDeliveryV1, IndustrialDeliveryErrorV1>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    R: ValueNodeReader,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
    CW: FnMut() -> bool,
    ME: FnMut() -> Option<ValueTreeError>,
{
    header.validate(
        ctx.transaction,
        common.memory(),
        now,
        common.work().identity(),
        live,
    )?;
    let result = paid_io::run(
        ctx,
        parent,
        write,
        materials,
        work,
        common,
        |ctx, parent, write, materials, work, tail| {
            let crate::DomainEvent::WorldRuleCommitted {
                actor_id,
                command:
                    oasis7_agent_api::world_action::WorldRuleCommandV1::DeliverAgentOrder { order_id },
                governance_epoch,
                world_binding,
                ..
            } = header.event()
            else {
                return Err(invalid());
            };
            let memory = tail.memory().clone();
            tail.charge_record()?;
            let crate::DomainEvent::WorldRuleCommitted { request_id, .. } = header.event() else {
                return Err(invalid());
            };
            let header_bytes = actor_id
                .len()
                .checked_add(request_id.len())
                .and_then(|n| n.checked_add(order_id.len()))
                .and_then(|n| n.checked_add(world_binding.as_ref().map_or(0, String::len)))
                .ok_or_else(invalid)?;
            tail.charge(header_bytes)?;
            let phase = tail.reserve(
                size_of::<CommittedWorldRuleOrderDeliveryV1>()
                    .checked_add(header_bytes)
                    .ok_or_else(invalid)?,
                MemoryClass::Ordinary,
            )?;
            let captured_event = header.event().clone();
            let guard = ctx.transaction.guard();
            sources
                .eligibility
                .orders
                .validate_current(ctx, &guard, live)
                .map_err(IndustrialEligibilityErrorV1::from)?;
            let guard = ctx.transaction.guard();
            let owned=oasis7_world_state::incremental_world_rule_composite_family::read_current_agent_order_v1(sources.eligibility.orders.source(),order_id,state,ctx,&mut work.eligibility.scan,&mut work.eligibility.typed,&guard,live)?.ok_or_else(||denied("order_not_found"))?;
            validate_current_lifecycle_delivery_acquisition_v1(order_id,sources,state,ctx,parent,work,tail,live)?;
        let order = owned.value();
            let _error_phase = tail.reserve(
                actor_id
                    .len()
                    .checked_add(order.target_agent_id.len())
                    .and_then(|n| n.checked_add(4096))
                    .ok_or_else(invalid)?,
                MemoryClass::Ordinary,
            )?;
            // Preserve delivery's original outer checks before blocker validation.
            tail.charge(match (&order.quote.world_binding,world_binding) {(Some(a),Some(b))=>a.len().min(b.len()),_=>0})?;
        if order.quote.world_binding.as_deref() != world_binding.as_deref() {
                return Err(denied("manufacturing_world_binding_mismatch"));
            }
            tail.charge(order.owner_id.len().min(actor_id.len()))?;
        if order.owner_id != *actor_id {
                return Err(denied("order_owner_required"));
            }
            let validation =
                validate_current_order_budgeted_v1(order, &owned, ctx, &mut work.eligibility)?;
            let refund = if validation.is_ok() && order.refund_pending {
                let guard = ctx.transaction.guard();
                Some(read_current_world_rule_order_refund_source_v1(
                    header.world_rule_order_view().ok_or_else(invalid)?,
                    &sources.eligibility,
                    order_id,
                    state,
                    ctx,
                    parent,
                    &mut work.eligibility,
                    tail,
                    &guard,
                    live,
                )?)
            } else {
                None
            };
            if let Some(reason) = order.delivery_prefix_after_validation_v1(
                now,
                validation,
                refund.as_ref().map(|r| r.is_current()),
            ) {
                return Err(denied(reason));
            }
            let guard = ctx.transaction.guard();
            let production = read_current_world_rule_order_production_facts_v1(
                header.world_rule_order_view().ok_or_else(invalid)?,
                &sources.eligibility,
                order_id,
                state,
                ctx,
                parent,
                &mut work.eligibility,
                tail,
                &guard,
                live,
            )?;
            production.validate(ctx.transaction, tail.memory(), live)?;
            if production.order_id() != order_id {
                return Err(invalid());
            }
            if let Err(error)=oasis7_world_state::world_rules::industrial_eligibility_kernel::order_production_eligibility_from_facts_v1(production.facts(),order){return Err(denied(format!("restore_production_authority:{error:?}")));}
            economy::capacity(
                order,
                sources,
                ctx,
                parent,
                work,
                production.facts().primary,
                tail,
                live,
            )
            .map_err(|error| match error {
                IndustrialDeliveryErrorV1::World(e) => {
                    denied(format!("restore_acquisition_capacity:{e:?}"))
                }
                e => e,
            })?;
            let guard = ctx.transaction.guard();
            if oasis7_world_state::incremental_agent_subtree::read_current_agent_presence_v1(
                &order.target_agent_id,
                sources.eligibility.agents,
                sources.eligibility.roots,
                ctx,
                parent,
                &mut work.eligibility.pending_entries_remaining,
                &guard,
                live,
            )? == oasis7_world_state::incremental_agent_subtree::CurrentAgentPresenceV1::Existing
            {
                return Err(denied("target_identity_conflict"));
            }
            // Everything that can return a business denial is evaluated before writes.
            let prepared = economy::prepare(
                order,
                actor_id,
                *governance_epoch,
                production.facts().primary.starter_claim,
                sources,
                state,
                ctx,
                parent,
                work,
                tail,
                now,
                live,
            )?;
            economy::commit(
                &prepared, order, actor_id, sources, state, ctx, parent, write, materials, work,
                tail, now, live,
            )?;
            let guard = ctx.transaction.guard();
            let orders=oasis7_world_state::incremental_world_rule_composite_family::mark_current_industrial_order_delivered_v1(sources.orders_parent,sources.eligibility.orders.source(),sources.eligibility.roots,order_id,now,state,ctx,write,&mut work.eligibility.scan,&mut work.composite_read,&mut work.composite_write,&guard,live)?;
            tail.charge(order.target_agent_id.len())?;
            let fee = tail.reserve(
                order.target_agent_id.len() + size_of::<String>(),
                MemoryClass::Ordinary,
            )?;
            let target = fee.retain(order.target_agent_id.clone());
            let guard = ctx.transaction.guard();
            ctx.transaction.validate_guard(&guard, live)?;
            Ok(CommittedWorldRuleOrderDeliveryV1 {
                target,
                orders,
                guard,
                event: captured_event,
                identity: tail.identity(),
                memory,
                now,
                _phase: phase,
            })
        },
    );
    if result
        .as_ref()
        .is_err_and(|e| !matches!(e, IndustrialDeliveryErrorV1::World(_)))
    {
        let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
            ctx.transaction.contribute(|_| {
                Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
            });
    }
    result
}

pub(in crate::world) use economy::capacity as validate_current_industrial_capacity_v1;
pub(in crate::world) use crate::world::incremental_industrial_eligibility::current_lifecycle_order_delivery_blocker_v1;

pub(in crate::world) use crate::world::incremental_industrial_eligibility::{CurrentWorldRuleOrderViewV1,read_current_world_rule_order_view_v1};

pub(in crate::world) use crate::world::incremental_industrial_eligibility::{read_current_world_rule_order_production_facts_v1,read_current_world_rule_order_refund_source_v1};
