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

#[path = "incremental_normal_factory_built_event.rs"]
mod normal_factory_built;
pub(in crate::world) use normal_factory_built::contribute_normal_factory_built_event_v1;

#[path = "incremental_normal_recipe_completed_event.rs"]
mod normal_recipe_completed;
pub(in crate::world) use normal_recipe_completed::contribute_normal_recipe_completed_event_v1;

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
    MainTokenTransfer(&'event super::incremental_action_data::main_token_transfer::CompletedMainTokenTransferV1),
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
    Alliance(&'event super::incremental_normal_action_diplomacy::CompletedAllianceActionV1),
    FactorySchedule(
        &'event super::incremental_normal_action_factory_schedule::CompletedFactoryScheduleActionV1,
    ),
    Observation(
        &'event super::incremental_normal_action_core_extension::CompletedObservationActionV1,
    ),
    ContractAccept(&'event super::incremental_action_contract_accept::CompletedContractAcceptV1),
    ProfileGovernance(
        &'event super::incremental_normal_action_profile::CompletedProfileGovernanceV1,
    ),
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
    pub(in crate::world) fn from_deferred_due<M: MerkleNodeReader>(
        owner:&'event crate::world::incremental_normal_industry_due::deferred::PaidDeferredDueEventV1,
        tx: &ExecutionTransaction<'_, M>,
        common:&mut oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedContextV1<'_>,
        now: u64,
        live: &oasis7_formal_protocol::incremental_execution::ExecutionFence,
    ) -> Result<Self, BodyMutationError> {
        owner.validate(tx, common, now, live)?;
        Ok(Self {
            owner: PersistedEventOwnerV1::Module {
                event: owner.event(),
                identity: tx.guard(),
            },
        })
    }
    /// Route the exact authenticated historical Product event without applying
    /// its Domain mutation or appending a second Original/WorldEvent fact.
    pub(in crate::world) fn from_exact_product_replay<M: MerkleNodeReader>(
        owner: &'event crate::world::incremental_normal_industry_due::delivery::PreparedProductValidationReplayV1,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedContextV1<'_>,
        live: &oasis7_formal_protocol::incremental_execution::ExecutionFence,
    ) -> Result<Self, crate::world::incremental_normal_industry_due::delivery::DeliveryDataErrorV1>
    {
        owner.validate(tx, common, live)?;
        Ok(Self {
            owner: PersistedEventOwnerV1::Module {
                event: owner.event(),
                identity: tx.guard(),
            },
        })
    }
    pub(in crate::world) fn from_module_lifecycle_event<M: MerkleNodeReader>(
        owner: &'event super::incremental_normal_action_module_lifecycle::PreparedNormalModuleLifecycleEventV1,
        tx: &ExecutionTransaction<'_, M>,
        now: u64,
        common: &mut oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedContextV1<'_>,
        live: &oasis7_formal_protocol::incremental_execution::ExecutionFence,
    ) -> Result<Self, BodyMutationError> {
        common
            .consume_steps(1)
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        tx.validate_guard(&tx.guard(), live)?;
        owner.validate(tx, common.memory(), now, common.work().identity(), live)?;
        Ok(Self {
            owner: PersistedEventOwnerV1::Module {
                event: owner.event(),
                identity: tx.guard(),
            },
        })
    }

    pub(in crate::world) fn from_module_lifecycle<M: MerkleNodeReader>(
        owner: &'event super::incremental_normal_action_module_lifecycle::CompletedModuleLifecycleActionV1,
        tx: &ExecutionTransaction<'_, M>,
        now: u64,
        common: &mut oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedContextV1<'_>,
        live: &oasis7_formal_protocol::incremental_execution::ExecutionFence,
    ) -> Result<Self, BodyMutationError> {
        common
            .consume_steps(1)
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        tx.validate_guard(&tx.guard(), live)?;
        owner.validate(tx, common.memory(), now, common.work().identity(), live)?;
        Ok(Self {
            owner: PersistedEventOwnerV1::Module {
                event: owner.event(),
                identity: tx.guard(),
            },
        })
    }

    pub(in crate::world) fn from_factory_schedule(
        owner:&'event super::incremental_normal_action_factory_schedule::CompletedFactoryScheduleActionV1,
    ) -> Self {
        Self {
            owner: PersistedEventOwnerV1::FactorySchedule(owner),
        }
    }

    pub(in crate::world) fn from_safe_alternative_rejection<M: MerkleNodeReader>(
        owner: &'event crate::world::incremental_safe_alternative_matching::CompletedSafeAlternativeRejectionV1<'_, '_>,
        tx: &ExecutionTransaction<'_, M>,
    ) -> Result<Self, oasis7_formal_protocol::incremental_execution::ExecutionError> {
        owner.validate(tx)?;
        Ok(Self {
            owner: PersistedEventOwnerV1::Module {
                event: owner.event(),
                identity: tx.guard(),
            },
        })
    }

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
    pub(super) fn from_alliance(
        owner: &'event super::incremental_normal_action_diplomacy::CompletedAllianceActionV1,
    ) -> Self {
        Self {
            owner: PersistedEventOwnerV1::Alliance(owner),
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
    pub(super) fn from_profile_governance(
        owner: &'event super::incremental_normal_action_profile::CompletedProfileGovernanceV1,
    ) -> Self {
        Self {
            owner: PersistedEventOwnerV1::ProfileGovernance(owner),
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
    pub(super) fn from_main_token_transfer(owner: &'event super::incremental_action_data::main_token_transfer::CompletedMainTokenTransferV1) -> Self {
        Self { owner: PersistedEventOwnerV1::MainTokenTransfer(owner) }
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
            PersistedEventOwnerV1::Alliance(owner) => owner.event(),
            PersistedEventOwnerV1::FactorySchedule(owner) => owner.event(),
            PersistedEventOwnerV1::Observation(owner) => owner.event(),
            PersistedEventOwnerV1::ContractAccept(owner) => owner.event(),
            PersistedEventOwnerV1::FactoryLifecycle(owner) => owner.event(),
            PersistedEventOwnerV1::ProfileGovernance(owner) => owner.event(),
            PersistedEventOwnerV1::FactoryDepreciation(owner) => owner.event(),
            PersistedEventOwnerV1::CollectData(owner) => owner.event(),
            PersistedEventOwnerV1::TransferResource(owner) => owner.event(),
            PersistedEventOwnerV1::MainTokenTransfer(owner) => owner.event(),
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
            PersistedEventOwnerV1::Alliance(owner) => owner.validate(tx),
            PersistedEventOwnerV1::FactorySchedule(owner) => owner.validate_transaction(tx),
            PersistedEventOwnerV1::Observation(owner) => owner.validate(tx),
            PersistedEventOwnerV1::ContractAccept(owner) => owner.validate(tx),
            PersistedEventOwnerV1::FactoryLifecycle(owner) => owner.validate(tx),
            PersistedEventOwnerV1::ProfileGovernance(owner) => owner.validate(tx),
            PersistedEventOwnerV1::FactoryDepreciation(owner) => owner.validate(tx),
            PersistedEventOwnerV1::CollectData(owner) => owner.validate(tx),
            PersistedEventOwnerV1::TransferResource(owner) => owner.validate(tx),
            PersistedEventOwnerV1::MainTokenTransfer(owner) => owner.validate(tx),
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

/// Publication requires the actual private main-token post-write owner.
pub(super) fn contribute_main_token_transfer_event_v1<M, V, W, C, F>(
    now: u64,
    prepared: &super::incremental_action_data::main_token_transfer::PreparedMainTokenTransferEventV1,
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
        prepared.validate(txn, mutation.memory, now, live)?;
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

#[path = "incremental_normal_material_transit_event.rs"]
mod material_transit_event;
pub(in crate::world) use material_transit_event::contribute_normal_material_transit_event_v1;

#[path = "incremental_safe_alternative_outcome_event.rs"]
mod safe_alternative_outcome;
pub(in crate::world) use safe_alternative_outcome::publish_safe_alternative_outcome_event_v1;

#[path = "incremental_normal_due_product_event.rs"]
mod due_product_event;
pub(in crate::world) use due_product_event::{
    contribute_due_product_attempt_event_v1, contribute_due_product_cursor_event_v1,
    contribute_due_product_module_failure_event_v1, contribute_due_product_recorded_event_v1,
    contribute_due_product_result_event_v1, contribute_due_recipe_failure_event_v1,
};

/// A due product rejection has no actor in its original DomainEvent. Require
/// the completed private system source and preserve its global journal form.
pub(super) fn contribute_normal_system_original_only_v1<M, V, W, C, F>(
    now: u64,
    source: &super::incremental_normal_system_event::PreparedNormalSystemEventV1<'_>,
    tx: &mut ExecutionTransaction<'_, M>,
    context: &mut super::incremental_body_event::AgentEventContext<'_, V, W, C, F>,
    live: &oasis7_formal_protocol::incremental_execution::ExecutionFence,
) -> Result<(), BodyMutationError>
where
    M: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeWriter,
    C: FnMut() -> bool,
    F: FnMut(&mut dyn AgentEventOriginalV1) -> Result<(), BodyMutationError>,
{
    tx.contribute(|tx| {
        source.validate(tx, context.mutation.memory, now)?;
        tx.validate_guard(&tx.guard(), live)?;
        let WorldEventBody::Domain(domain) = &source.event().body else {
            return Err(BodyMutationError::Invalid(
                "normal original-only source kind",
            ));
        };
        if !matches!(domain, crate::DomainEvent::ActionRejected { .. })
            && domain.agent_id().is_some()
        {
            return Err(BodyMutationError::Invalid("normal original-only source"));
        }
        persist_event_original_and_fact_v1(
            now,
            source.event(),
            tx,
            context.values,
            &mut context.mutation,
            &mut context.original_work,
            context.max_event_bytes,
            &mut context.persist_original,
        )
    })
}

#[path = "incremental_normal_world_rule_lifecycle_event.rs"]
mod world_rule_lifecycle;
pub(in crate::world) use world_rule_lifecycle::contribute_normal_world_rule_lifecycle_event_v1;

#[path = "incremental_normal_claim_epoch_event.rs"]
mod claim_epoch;
pub(in crate::world) use claim_epoch::contribute_normal_claim_epoch_event_v1;

#[path = "incremental_normal_gameplay_cycle_event.rs"]
mod gameplay_cycle;
pub(in crate::world) use gameplay_cycle::contribute_normal_gameplay_cycle_event_v1;

#[path = "incremental_normal_organization_tail_event.rs"]
mod organization_tail;
pub(in crate::world) use organization_tail::contribute_normal_organization_tail_event_v1;

#[path = "incremental_normal_gameplay_directive_failure_event.rs"]
mod gameplay_directive_failure;
pub(in crate::world) use gameplay_directive_failure::contribute_normal_gameplay_directive_failure_v1;

// A non-Domain failure still requires the actual private system producer cap.
pub(super) fn contribute_normal_system_failure_original_only_v1<M, V, W, C, F>(
    now: u64,
    source: &super::incremental_normal_system_event::PreparedNormalSystemEventV1<'_>,
    tx: &mut ExecutionTransaction<'_, M>,
    context: &mut super::incremental_body_event::AgentEventContext<'_, V, W, C, F>,
    live: &oasis7_formal_protocol::incremental_execution::ExecutionFence,
) -> Result<(), BodyMutationError>
where
    M: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeWriter,
    C: FnMut() -> bool,
    F: FnMut(&mut dyn AgentEventOriginalV1) -> Result<(), BodyMutationError>,
{
    tx.contribute(|tx| {
        source.validate(tx, context.mutation.memory, now)?;
        tx.validate_guard(&tx.guard(), live)?;
        if !matches!(&source.event().body, WorldEventBody::ModuleCallFailed(_)) {
            return Err(BodyMutationError::Invalid(
                "normal failure original-only source kind",
            ));
        }
        persist_event_original_and_fact_v1(
            now,
            source.event(),
            tx,
            context.values,
            &mut context.mutation,
            &mut context.original_work,
            context.max_event_bytes,
            &mut context.persist_original,
        )
    })
}

#[path = "incremental_module_execution_profile_event.rs"]
mod profile_governance;
pub(super) use profile_governance::contribute_profile_governance_event_v1;

/// Action-correlated factory followup after its concrete Current transition.
/// The system gate has a distinct private scope; ordinary uncaused sources
/// cannot publish an action-correlated Domain event.
pub(in crate::world) fn contribute_factory_module_followup_event_v1<'event, M, V, W, C, F, ME>(
    now: u64,
    source: &'event crate::world::incremental_module_invocation::PreparedFactoryModuleFollowupV1,
    common:&mut oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedContextV1<'_>,
    live: &oasis7_formal_protocol::incremental_execution::ExecutionFence,
    transaction: &mut ExecutionTransaction<'_, M>,
    events: &mut ModuleOutputEventContextV1<'_, V, W, C, F>,
    materials: &mut crate::world::incremental_body_event::materials::CorePolicyMaterialContextV1<
        '_,
        M,
        ME,
    >,
) -> Result<PersistedWorldEventV1<'event>, BodyMutationError>
where
    M: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeWriter,
    C: FnMut() -> bool,
    F: FnMut(&mut dyn AgentEventOriginalV1) -> Result<(), BodyMutationError>,
    ME: FnMut() -> Option<ValueTreeError>,
{
    transaction.contribute(|transaction|{
  let cap=crate::world::incremental_normal_system_event::PreparedNormalSystemEventV1::from_factory_module_followup(source,transaction,now,common,live)?;
  let guard=transaction.guard();
  let mut step=crate::world::incremental_step::IwsStepContext::borrow_counted_at(transaction,now,&guard,live,events.values,&mut events.mutation,&mut events.original_work,events.world_events)?;
  step.append_prepared_normal_system_event_v1(&cap,materials,common,live,events.max_event_bytes,&mut events.persist_original)?;
  Ok(PersistedWorldEventV1{owner:PersistedEventOwnerV1::Module{event:source.event(),identity:step.transaction.guard()}})
 })
}

/// The proposal writer already applied the business transition. Only persist
/// the exact private source's original, allocator and ordered WorldEvent fact.
pub(super) fn contribute_normal_module_governance_original_only_v1<M, V, W, C, F>(
    now: u64,
    source: &super::incremental_normal_system_event::PreparedNormalSystemEventV1<'_>,
    tx: &mut ExecutionTransaction<'_, M>,
    context: &mut super::incremental_body_event::AgentEventContext<'_, V, W, C, F>,
    live: &oasis7_formal_protocol::incremental_execution::ExecutionFence,
) -> Result<(), BodyMutationError>
where
    M: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeWriter,
    C: FnMut() -> bool,
    F: FnMut(&mut dyn AgentEventOriginalV1) -> Result<(), BodyMutationError>,
{
    tx.contribute(|tx| {
        source.validate(tx, context.mutation.memory, now)?;
        tx.validate_guard(&tx.guard(), live)?;
        if source.event().caused_by.is_some()
            || !matches!(
                &source.event().body,
                WorldEventBody::Governance(
                    crate::GovernanceEvent::Proposed { .. }
                        | crate::GovernanceEvent::ShadowReport { .. }
                        | crate::GovernanceEvent::Approved { .. }
                        | crate::GovernanceEvent::Queued { .. }
                        | crate::GovernanceEvent::Applied { .. }
                ) | WorldEventBody::ModuleEvent(_)
                    | WorldEventBody::ManifestUpdated(_)
                    | WorldEventBody::CapabilityAuthorization(_)
            )
        {
            return Err(BodyMutationError::Invalid(
                "normal module governance source kind",
            ));
        }
        persist_event_original_and_fact_v1(
            now,
            source.event(),
            tx,
            context.values,
            &mut context.mutation,
            &mut context.original_work,
            context.max_event_bytes,
            &mut context.persist_original,
        )
    })
}
