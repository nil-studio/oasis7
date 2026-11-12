//! Event-original/allocator contribution after the concrete module target has
//! applied the event's business owner. This helper is crate-private, grants no
//! semantic permission, and cannot be used as a substitute for owner mutation.
use super::{
    World,
    incremental_body::{BodyMutationContext, BodyMutationError, write_field_bytes},
    incremental_body_event::{AgentEventOriginalV1, original, read_fixed},
};
use crate::{WorldEvent, WorldEventBody};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFactKindV1, ExecutionFactRefV1, ExecutionTransaction},
    incremental_state::{MerkleNodeReader, StateNamespaceV1},
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueTreeError},
};
use oasis7_wasm_abi::CanonicalCborWorkV1;
use oasis7_world_state::incremental_event_fields::{
    EVENT_SEQUENCE_BYTES, EVENT_SEQUENCE_KEY_V1, EventSequenceV1,
};

pub struct ModuleOutputEventContextV1<'a, V, W, C, F> {
    pub values: &'a V,
    pub world_events: &'a super::incremental_world_event_counter::WorldEventCounterV1,
    pub mutation: BodyMutationContext<'a, W, C>,
    pub persist_original: F,
    pub max_event_bytes: usize,
    pub original_work: CanonicalCborWorkV1,
}
/// Called only by the concrete output target, after the corresponding fee,
/// effect/visual/policy owner succeeds in the SAME transaction. Any later
/// persistence/encoding failure poisons that transaction through contribute.
pub(super) fn contribute_module_output_event_original_v1<M, V, W, C, F>(
    now: u64,
    event: &WorldEvent,
    txn: &mut ExecutionTransaction<'_, M>,
    ctx: &mut ModuleOutputEventContextV1<'_, V, W, C, F>,
) -> Result<(), BodyMutationError>
where
    M: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeWriter,
    C: FnMut() -> bool,
    F: FnMut(&mut dyn AgentEventOriginalV1) -> Result<(), BodyMutationError>,
{
    persist_module_output_event_v1(now, event, txn, ctx).map(|_| ())
}

/// A borrowed event minted only after its original and ordered fact succeed.
/// Every constructor requires a private receipt from a real producer.
pub(super) struct PersistedWorldEventV1<'event> {
    owner: PersistedEventOwnerV1<'event>,
}
enum PersistedEventOwnerV1<'event> {
    CountedTail(&'event super::incremental_world_event_tail::RecoveredCountedWorldEventV1),
    Module {
        event: &'event WorldEvent,
        identity: oasis7_formal_protocol::incremental_execution::PreparedGuard,
    },
    Agent(super::incremental_step::PersistedAgentEventV1<'event>),
    ActionAudit(&'event super::incremental_action_execution::SelectedActionAuditV1),
    RuleOverride(&'event super::incremental_module_invocation::RootRuleOverrideAuditV1),
    CoreAction(&'event super::incremental_action_execution_core::CompletedCoreActionV1),
    BodyAction(&'event super::incremental_action_body::CompletedBodyActionV1),
    RouteAvailability(
        &'event super::incremental_action_route_availability::CompletedRouteAvailabilityV1,
    ),
    MetaProgress(&'event super::incremental_action_meta_progress::CompletedMetaProgressV1),
    Crisis(&'event super::incremental_action_crisis::CompletedCrisisV1),
    BodyInterface(
        &'event super::incremental_normal_action_body_interface::CompletedBodyInterfaceV1,
    ),
    LogisticsRegistration(
        &'event super::incremental_normal_action_logistics::CompletedLogisticsRegistrationV1,
    ),
    Observation(
        &'event super::incremental_normal_action_core_extension::CompletedObservationActionV1,
    ),
    ContractAccept(&'event super::incremental_action_contract_accept::CompletedContractAcceptV1),
    FactoryLifecycle(
        &'event super::incremental_action_factory_lifecycle::CompletedFactoryLifecycleV1,
    ),
    MaterialTransfer(
        &'event super::incremental_material_normalization::transfer::CompletedMaterialTransferV1,
    ),
    FactoryDepreciation(
        &'event super::incremental_factory_depreciation::publish::PublishedFactoryDepreciationV1<
            'event,
        >,
    ),
    CollectData(&'event super::incremental_action_data::CompletedCollectDataV1),
    DataAccess(&'event super::incremental_action_data::data_access::CompletedDataAccessV1),
    TransferResource(
        &'event super::incremental_action_data::transfer_resource::CompletedTransferResourceV1,
    ),
}
impl<'event> PersistedWorldEventV1<'event> {
    pub(super) fn from_counted_tail(
        owner: &'event super::incremental_world_event_tail::RecoveredCountedWorldEventV1,
    ) -> Self {
        Self {
            owner: PersistedEventOwnerV1::CountedTail(owner),
        }
    }
    pub(super) fn from_core_action(
        owner: &'event super::incremental_action_execution_core::CompletedCoreActionV1,
    ) -> Self {
        Self {
            owner: PersistedEventOwnerV1::CoreAction(owner),
        }
    }

    pub(super) fn from_route_availability(
        owner: &'event super::incremental_action_route_availability::CompletedRouteAvailabilityV1,
    ) -> Self {
        Self {
            owner: PersistedEventOwnerV1::RouteAvailability(owner),
        }
    }
    pub(super) fn from_meta_progress(
        owner: &'event super::incremental_action_meta_progress::CompletedMetaProgressV1,
    ) -> Self {
        Self {
            owner: PersistedEventOwnerV1::MetaProgress(owner),
        }
    }
    pub(super) fn from_logistics_registration(
        owner: &'event super::incremental_normal_action_logistics::CompletedLogisticsRegistrationV1,
    ) -> Self {
        Self {
            owner: PersistedEventOwnerV1::LogisticsRegistration(owner),
        }
    }
    pub(super) fn from_body_interface(
        owner: &'event super::incremental_normal_action_body_interface::CompletedBodyInterfaceV1,
    ) -> Self {
        Self {
            owner: PersistedEventOwnerV1::BodyInterface(owner),
        }
    }
    pub(super) fn from_observation(
        owner: &'event super::incremental_normal_action_core_extension::CompletedObservationActionV1,
    ) -> Self {
        Self {
            owner: PersistedEventOwnerV1::Observation(owner),
        }
    }
    pub(super) fn from_crisis(
        owner: &'event super::incremental_action_crisis::CompletedCrisisV1,
    ) -> Self {
        Self {
            owner: PersistedEventOwnerV1::Crisis(owner),
        }
    }
    pub(super) fn from_contract_accept(
        owner: &'event super::incremental_action_contract_accept::CompletedContractAcceptV1,
    ) -> Self {
        Self {
            owner: PersistedEventOwnerV1::ContractAccept(owner),
        }
    }
    pub(super) fn from_factory_lifecycle(
        owner: &'event super::incremental_action_factory_lifecycle::CompletedFactoryLifecycleV1,
    ) -> Self {
        Self {
            owner: PersistedEventOwnerV1::FactoryLifecycle(owner),
        }
    }
    pub(super) fn from_material_transfer(
        owner: &'event super::incremental_material_normalization::transfer::CompletedMaterialTransferV1,
    ) -> Self {
        Self {
            owner: PersistedEventOwnerV1::MaterialTransfer(owner),
        }
    }

    pub(super) fn from_data_access(
        owner: &'event super::incremental_action_data::data_access::CompletedDataAccessV1,
    ) -> Self {
        Self {
            owner: PersistedEventOwnerV1::DataAccess(owner),
        }
    }
    pub(super) fn from_transfer_resource(
        owner: &'event super::incremental_action_data::transfer_resource::CompletedTransferResourceV1,
    ) -> Self {
        Self {
            owner: PersistedEventOwnerV1::TransferResource(owner),
        }
    }

    pub(super) fn from_body_action(
        owner: &'event super::incremental_action_body::CompletedBodyActionV1,
    ) -> Self {
        Self {
            owner: PersistedEventOwnerV1::BodyAction(owner),
        }
    }
    pub(super) fn from_rule_override(
        owner: &'event super::incremental_module_invocation::RootRuleOverrideAuditV1,
    ) -> Self {
        Self {
            owner: PersistedEventOwnerV1::RuleOverride(owner),
        }
    }
    /// The audit owner can only be created by the real selected-action sink,
    /// after counted publication and admitted immutable DTO normalization.
    pub(super) fn from_action_audit(
        owner: &'event super::incremental_action_execution::SelectedActionAuditV1,
    ) -> Self {
        Self {
            owner: PersistedEventOwnerV1::ActionAudit(owner),
        }
    }
    pub(super) fn from_agent(
        owner: super::incremental_step::PersistedAgentEventV1<'event>,
    ) -> Self {
        Self {
            owner: PersistedEventOwnerV1::Agent(owner),
        }
    }
    pub(super) fn from_factory_depreciation<'batch>(
        owner: &'event super::incremental_factory_depreciation::publish::PublishedFactoryDepreciationV1<'batch>,
    ) -> Self
    where
        'batch: 'event,
    {
        Self {
            owner: PersistedEventOwnerV1::FactoryDepreciation(owner),
        }
    }
    /// The specialized CollectData sink mints this owner only after Current
    /// resources, mailbox, original stream and Event fact all succeeded.
    pub(super) fn from_collect_data(
        owner: &'event super::incremental_action_data::CompletedCollectDataV1,
    ) -> Self {
        Self {
            owner: PersistedEventOwnerV1::CollectData(owner),
        }
    }
    pub(super) fn event(&self) -> &WorldEvent {
        match &self.owner {
            PersistedEventOwnerV1::CountedTail(owner) => owner.event(),
            PersistedEventOwnerV1::Module { event, .. } => event,
            PersistedEventOwnerV1::Agent(owner) => owner.event(),
            PersistedEventOwnerV1::ActionAudit(owner) => owner.event(),
            PersistedEventOwnerV1::RuleOverride(owner) => owner.event(),
            PersistedEventOwnerV1::CoreAction(owner) => owner.event(),
            PersistedEventOwnerV1::BodyAction(owner) => owner.event(),
            PersistedEventOwnerV1::RouteAvailability(owner) => owner.event(),
            PersistedEventOwnerV1::MaterialTransfer(owner) => owner.event(),
            PersistedEventOwnerV1::MetaProgress(owner) => owner.event(),
            PersistedEventOwnerV1::Crisis(owner) => owner.event(),
            PersistedEventOwnerV1::BodyInterface(owner) => owner.event(),
            PersistedEventOwnerV1::LogisticsRegistration(owner) => owner.event(),
            PersistedEventOwnerV1::Observation(owner) => owner.event(),
            PersistedEventOwnerV1::ContractAccept(owner) => owner.event(),
            PersistedEventOwnerV1::FactoryLifecycle(owner) => owner.event(),
            PersistedEventOwnerV1::FactoryDepreciation(owner) => owner.event(),
            PersistedEventOwnerV1::CollectData(owner) => owner.event(),
            PersistedEventOwnerV1::TransferResource(owner) => owner.event(),
            PersistedEventOwnerV1::DataAccess(owner) => owner.event(),
        }
    }
    pub(super) fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
    ) -> Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> {
        match &self.owner {
            PersistedEventOwnerV1::CountedTail(owner) => {
                owner.validate(tx).map_err(|error| match error {
                    BodyMutationError::Execution(error) => error,
                    _ => oasis7_formal_protocol::incremental_execution::ExecutionError::Invalid(
                        "counted WorldEvent source",
                    ),
                })
            }
            PersistedEventOwnerV1::Module { identity, .. } => {
                tx.validate_transaction_identity(identity)
            }
            PersistedEventOwnerV1::Agent(owner) => owner.validate(tx),
            PersistedEventOwnerV1::ActionAudit(owner) => owner.validate(tx),
            PersistedEventOwnerV1::RuleOverride(owner) => owner.validate(tx),
            PersistedEventOwnerV1::CoreAction(owner) => owner.validate(tx),
            PersistedEventOwnerV1::BodyAction(owner) => owner.validate(tx),
            PersistedEventOwnerV1::RouteAvailability(owner) => owner.validate(tx),
            PersistedEventOwnerV1::MaterialTransfer(owner) => owner.validate(tx),
            PersistedEventOwnerV1::MetaProgress(owner) => owner.validate(tx),
            PersistedEventOwnerV1::Crisis(owner) => owner.validate(tx),
            PersistedEventOwnerV1::BodyInterface(owner) => owner.validate(tx),
            PersistedEventOwnerV1::LogisticsRegistration(owner) => owner.validate(tx),
            PersistedEventOwnerV1::Observation(owner) => owner.validate(tx),
            PersistedEventOwnerV1::ContractAccept(owner) => owner.validate(tx),
            PersistedEventOwnerV1::FactoryLifecycle(owner) => owner.validate(tx),
            PersistedEventOwnerV1::FactoryDepreciation(owner) => owner.validate(tx),
            PersistedEventOwnerV1::CollectData(owner) => owner.validate(tx),
            PersistedEventOwnerV1::TransferResource(owner) => owner.validate(tx),
            PersistedEventOwnerV1::DataAccess(owner) => owner.validate(tx),
        }
    }
}

pub(super) fn persist_module_output_event_v1<'event, M, V, W, C, F>(
    now: u64,
    event: &'event WorldEvent,
    txn: &mut ExecutionTransaction<'_, M>,
    ctx: &mut ModuleOutputEventContextV1<'_, V, W, C, F>,
) -> Result<PersistedWorldEventV1<'event>, BodyMutationError>
where
    M: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeWriter,
    C: FnMut() -> bool,
    F: FnMut(&mut dyn AgentEventOriginalV1) -> Result<(), BodyMutationError>,
{
    txn.contribute(|txn| {
        ctx.world_events.validate(txn)?;
        if !matches!(
            &event.body,
            WorldEventBody::PolicyDecisionRecorded(_)
                | WorldEventBody::EffectQueued(_)
                | WorldEventBody::ModuleEmitted(_)
                | WorldEventBody::ModuleRuntimeCharged(_)
                | WorldEventBody::ModuleCallFailed(_)
                | WorldEventBody::RuleDecisionRecorded(_)
        ) || event.time != now
            || !txn.uses_budget(ctx.mutation.memory)
        {
            return Err(BodyMutationError::Invalid(
                "module output event original scope",
            ));
        }
        if let WorldEventBody::RuleDecisionRecorded(record) = &event.body
            && event.caused_by != Some(crate::CausedBy::Action(record.action_id))
        {
            return Err(BodyMutationError::Invalid("rule audit action binding"));
        }
        persist_counted_event_original_v1(now, event, txn, ctx)
    })
}

/// A rule override requires the private candidate built from the actual
/// Selected/RuleOutcome pair; the generic module sink never accepts it.
pub(super) fn contribute_rule_override_event_v1<M, V, W, C, F>(
    now: u64,
    prepared: &super::incremental_module_invocation::PreparedRuleOverrideEventV1,
    live: &oasis7_formal_protocol::incremental_execution::ExecutionFence,
    txn: &mut ExecutionTransaction<'_, M>,
    ctx: &mut ModuleOutputEventContextV1<'_, V, W, C, F>,
) -> Result<(), BodyMutationError>
where
    M: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeWriter,
    C: FnMut() -> bool,
    F: FnMut(&mut dyn AgentEventOriginalV1) -> Result<(), BodyMutationError>,
{
    txn.contribute(|txn| {
        prepared.validate(txn, ctx.mutation.memory, now, live)?;
        persist_counted_event_original_v1(now, prepared.event(), txn, ctx).map(|_| ())
    })
}

/// Depreciation publication is accepted only from its private prepared event,
/// after the reducer has staged the actual balance change in this transaction.
pub(super) fn contribute_factory_depreciation_event_v1<M, V, W, C, F>(
    now: u64,
    prepared: &super::incremental_factory_depreciation::publish::PreparedFactoryDepreciationEventV1,
    live: &oasis7_formal_protocol::incremental_execution::ExecutionFence,
    txn: &mut ExecutionTransaction<'_, M>,
    ctx: &mut ModuleOutputEventContextV1<'_, V, W, C, F>,
) -> Result<(), BodyMutationError>
where
    M: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeWriter,
    C: FnMut() -> bool,
    F: FnMut(&mut dyn AgentEventOriginalV1) -> Result<(), BodyMutationError>,
{
    txn.contribute(|txn| {
        prepared.validate(txn, ctx.mutation.memory, now, live)?;
        persist_counted_event_original_v1(now, prepared.event(), txn, ctx).map(|_| ())
    })
}

/// Material transfer persistence is accepted only from the private prepared
/// receipt built after Current balances and its typed record were staged.
/// Counter advancement remains with the step after the mailbox sink succeeds.
pub(super) fn contribute_material_transfer_event_v1<M, V, W, C, F>(
    now: u64,
    prepared: &super::incremental_material_normalization::transfer::PreparedMaterialTransferEventV1,
    live: &oasis7_formal_protocol::incremental_execution::ExecutionFence,
    txn: &mut ExecutionTransaction<'_, M>,
    values: &V,
    mutation: &mut BodyMutationContext<'_, W, C>,
    original_work: &mut CanonicalCborWorkV1,
    max_event_bytes: usize,
    persist_original: &mut F,
) -> Result<(), BodyMutationError>
where
    M: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeWriter,
    C: FnMut() -> bool,
    F: FnMut(&mut dyn AgentEventOriginalV1) -> Result<(), BodyMutationError>,
{
    txn.contribute(|txn| {
        let guard = txn.guard();
        txn.validate_guard(&guard, live)?;
        prepared.validate(txn, mutation.memory, now)?;
        persist_event_original_and_fact_v1(
            now,
            prepared.event(),
            txn,
            values,
            mutation,
            original_work,
            max_event_bytes,
            persist_original,
        )?;
        Ok(())
    })
}

/// The two producer wrappers validate their own body/source scope first.
/// This private shared path owns clock, allocator, original and fact ordering.
fn persist_counted_event_original_v1<'event, M, V, W, C, F>(
    now: u64,
    event: &'event WorldEvent,
    txn: &mut ExecutionTransaction<'_, M>,
    ctx: &mut ModuleOutputEventContextV1<'_, V, W, C, F>,
) -> Result<PersistedWorldEventV1<'event>, BodyMutationError>
where
    M: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeWriter,
    C: FnMut() -> bool,
    F: FnMut(&mut dyn AgentEventOriginalV1) -> Result<(), BodyMutationError>,
{
    txn.contribute(|txn| {
        ctx.world_events.validate(txn)?;
        let before = txn.ordered_fact_count();
        if event.time != now || !txn.uses_budget(ctx.mutation.memory) {
            return Err(BodyMutationError::Invalid(
                "module output event original scope",
            ));
        }
        if (ctx.mutation.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        persist_event_original_and_fact_v1(
            now,
            event,
            txn,
            ctx.values,
            &mut ctx.mutation,
            &mut ctx.original_work,
            ctx.max_event_bytes,
            &mut ctx.persist_original,
        )?;
        ctx.world_events.note_success(txn, before)?;
        Ok(PersistedWorldEventV1 {
            owner: PersistedEventOwnerV1::Module {
                event,
                identity: txn.guard(),
            },
        })
    })
}

fn persist_event_original_and_fact_v1<M, V, W, C, F>(
    now: u64,
    event: &WorldEvent,
    txn: &mut ExecutionTransaction<'_, M>,
    values: &V,
    mutation: &mut super::incremental_body::BodyMutationContext<'_, W, C>,
    original_work: &mut CanonicalCborWorkV1,
    max_event_bytes: usize,
    persist_original: &mut F,
) -> Result<(), BodyMutationError>
where
    M: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeWriter,
    C: FnMut() -> bool,
    F: FnMut(&mut dyn AgentEventOriginalV1) -> Result<(), BodyMutationError>,
{
    txn.contribute(|txn| {
        if event.time != now || !txn.uses_budget(mutation.memory) {
            return Err(BodyMutationError::Invalid(
                "module output event original scope",
            ));
        }
        if (mutation.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        super::incremental_clock::check_clock(now, txn, mutation)?;
        let seq =
            EventSequenceV1::from_canonical_bytes(&read_fixed::<EVENT_SEQUENCE_BYTES, _, _, _, _>(
                txn,
                StateNamespaceV1::Allocator,
                EVENT_SEQUENCE_KEY_V1,
                values,
                mutation,
            )?)
            .map_err(BodyMutationError::Invalid)?;
        let (id, next, era) = World::preview_next_event_id(seq.next, seq.era);
        if id != event.id {
            return Err(BodyMutationError::Invalid("module output event allocator"));
        }
        let measured = original::measure(
            event,
            mutation.memory,
            original_work,
            &mut mutation.cancelled,
            max_event_bytes,
        )?;
        let original_id = measured.id();
        let next_ref =
            write_field_bytes(&EventSequenceV1 { next, era }.canonical_bytes(), mutation)?;
        txn.write(
            StateNamespaceV1::Allocator,
            EVENT_SEQUENCE_KEY_V1,
            Some(next_ref),
        )?;
        let mut producer = original::Producer::new(
            measured,
            mutation.memory,
            original_work,
            &mut mutation.cancelled,
        );
        persist_original(&mut producer)?;
        producer.check_complete()?;
        drop(producer);
        if (mutation.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        txn.append_fact(ExecutionFactRefV1 {
            kind: ExecutionFactKindV1::Event,
            id: original_id,
        })?;
        if (mutation.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(())
    })
}

pub(super) fn append_module_output_event_v1<M, V, W, C, F>(
    now: u64,
    body: WorldEventBody,
    tx: &mut ExecutionTransaction<'_, M>,
    events: &mut ModuleOutputEventContextV1<'_, V, W, C, F>,
) -> Result<(), BodyMutationError>
where
    M: MerkleNodeReader,
    V: ValueNodeReader,
    W: oasis7_formal_protocol::incremental_value::ValueNodeWriter,
    C: FnMut() -> bool,
    F: FnMut(
        &mut dyn super::incremental_body_event::AgentEventOriginalV1,
    ) -> Result<(), BodyMutationError>,
{
    let seq = World::read_iws_allocator(
        super::incremental_allocators::RuntimeAllocatorV1::Event,
        tx,
        events.values,
        &mut events.mutation,
    )?;
    let (id, _, _) = World::preview_next_event_id(seq.next, seq.era);
    contribute_module_output_event_original_v1(
        now,
        &WorldEvent {
            id,
            time: now,
            caused_by: None,
            body,
        },
        tx,
        events,
    )
}

pub(super) fn append_rule_decision_audit_v1<M, V, W, C, F>(
    now: u64,
    record: crate::RuleDecisionRecord,
    tx: &mut ExecutionTransaction<'_, M>,
    events: &mut ModuleOutputEventContextV1<'_, V, W, C, F>,
) -> Result<(), BodyMutationError>
where
    M: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeWriter,
    C: FnMut() -> bool,
    F: FnMut(&mut dyn AgentEventOriginalV1) -> Result<(), BodyMutationError>,
{
    let seq = World::read_iws_allocator(
        super::incremental_allocators::RuntimeAllocatorV1::Event,
        tx,
        events.values,
        &mut events.mutation,
    )?;
    let (id, _, _) = World::preview_next_event_id(seq.next, seq.era);
    let cause = Some(crate::CausedBy::Action(record.action_id));
    contribute_module_output_event_original_v1(
        now,
        &WorldEvent {
            id,
            time: now,
            caused_by: cause,
            body: WorldEventBody::RuleDecisionRecorded(record),
        },
        tx,
        events,
    )
}

#[path = "incremental_module_execution_factory_lifecycle_event.rs"]
mod factory_lifecycle;
pub(super) use factory_lifecycle::contribute_factory_lifecycle_event_v1;

#[path = "incremental_module_execution_meta_progress_event.rs"]
mod meta_progress;
pub(super) use meta_progress::contribute_meta_progress_event_v1;

#[path = "incremental_module_execution_crisis_event.rs"]
mod crisis;
pub(super) use crisis::contribute_crisis_event_v1;

#[path = "incremental_module_execution_contract_accept_event.rs"]
mod contract_accept;
pub(super) use contract_accept::contribute_contract_accept_event_v1;
