//! Fixed Current world-rule lifecycle stage. Selection observes one initial
//! snapshot; commands revalidate in order; event routing occurs after the batch.
//! This stage cannot mint an ExecutedTick or ordinary publication authority.
use crate::world::incremental_body::BodyMutationError;
use oasis7_agent_api::world_action::{EmergencySupplyCommandV1, WorldRuleCommandV1};
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::WorkIdentityV1,
    incremental_execution::{ExecutionFence, ExecutionTransaction, PreparedGuard},
    incremental_state::MerkleNodeReader,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryReservation, ReservedValue};

/// The five original lifecycle lanes, in their original snapshot order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LifecycleOperationV1 {
    SupplyClose,
    SupplySettle,
    Observe,
    OrderProgress,
    EmergencyClose,
}
/// Private post-write provenance. Only fixed lifecycle state committers can
/// construct this alongside their successfully applied Current mutations.
struct CommittedLifecycleMutationV1 {
    operation: LifecycleOperationV1,
    guard: PreparedGuard,
    identity: WorkIdentityV1,
    written: written::FreshWrittenLifecycleCommandV1,
    receipt: Option<publication::PaidLifecycleRequestCommitV1>,
}
/// Distinct authenticated retained-request evidence, never a fabricated fresh
/// transition. The actual request and its reservation stay live until drop.
struct RetainedLifecycleRequestNoopV1 {
    operation: LifecycleOperationV1,
    guard: PreparedGuard,
    identity: WorkIdentityV1,
    request: ReservedValue<oasis7_world_state::incremental_world_rule_requests_current::CurrentWorldRuleRequestV1>,
    _phase: MemoryReservation,
}
enum LifecycleEventSourceV1 {
    Fresh(CommittedLifecycleMutationV1),
    Retained(RetainedLifecycleRequestNoopV1),
}
impl LifecycleEventSourceV1 {
    fn binding(&self) -> (LifecycleOperationV1, &PreparedGuard, WorkIdentityV1) {
        match self {
            Self::Fresh(source) => (source.operation, &source.guard, source.identity),
            Self::Retained(source) => (source.operation, &source.guard, source.identity),
        }
    }
}
/// Actual Current changes or an authenticated exact retained request precede
/// this counted-event source. No public ctor,
/// Clone, body setter, raw command, or caller permission flag can mint it.
pub(in crate::world) struct PreparedNormalWorldRuleLifecycleEventV1 {
    event: ReservedValue<crate::WorldEvent>,
    source: LifecycleEventSourceV1,
    memory: MemoryBudget,
    now: u64,
    _phase: MemoryReservation,
}
impl PreparedNormalWorldRuleLifecycleEventV1 {
    pub(in crate::world) fn event(&self) -> &crate::WorldEvent {
        &self.event
    }
    pub(in crate::world) fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        now: u64,
        identity: WorkIdentityV1,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        let (operation, guard, source_identity) = self.source.binding();
        tx.validate_guard(guard, live)?;
        if source_identity != identity
            || !self.memory.same_pool(memory)
            || !tx.uses_budget(memory)
            || !self.event.uses_budget(memory)
            || self.now != now
            || self.event.time != now
            || self.event.caused_by.is_some()
        {
            return Err(BodyMutationError::Invalid(
                "world-rule lifecycle post-write source",
            ));
        }
        let crate::WorldEventBody::Domain(crate::DomainEvent::WorldRuleCommitted {
            action_id: 0,
            command,
            ..
        }) = &self.event.body
        else {
            return Err(BodyMutationError::Invalid(
                "world-rule lifecycle event source",
            ));
        };
        if let LifecycleEventSourceV1::Retained(source) = &self.source {
            if !source.request.uses_budget(memory) || source.request.value().is_none() {
                return Err(BodyMutationError::Invalid("retained lifecycle request owner"));
            }
        }
        match (operation, command) {
            (LifecycleOperationV1::SupplyClose, WorldRuleCommandV1::CloseSupply { .. })
            | (LifecycleOperationV1::SupplySettle, WorldRuleCommandV1::SettleSupply { .. })
            | (
                LifecycleOperationV1::Observe,
                WorldRuleCommandV1::EmergencySupply {
                    command: EmergencySupplyCommandV1::Observe { .. },
                },
            )
            | (
                LifecycleOperationV1::OrderProgress,
                WorldRuleCommandV1::AdvanceAgentOrder { .. }
                | WorldRuleCommandV1::DeliverAgentOrder { .. },
            )
            | (
                LifecycleOperationV1::EmergencyClose,
                WorldRuleCommandV1::EmergencySupply {
                    command: EmergencySupplyCommandV1::Close { .. },
                },
            ) => Ok(()),
            _ => Err(BodyMutationError::Invalid(
                "world-rule lifecycle operation binding",
            )),
        }
    }
}

#[path = "incremental_normal_world_rule_lifecycle_work.rs"]
pub(in crate::world) mod work;

#[path = "incremental_normal_world_rule_lifecycle_io.rs"]
mod io;
#[path = "incremental_normal_world_rule_lifecycle_supply.rs"]
mod supply;

#[path = "incremental_normal_world_rule_lifecycle_header.rs"]
pub(in crate::world) mod header;

#[path = "incremental_normal_world_rule_delivery.rs"]
pub(in crate::world) mod delivery;

#[path = "incremental_normal_world_rule_emergency_close.rs"]
pub(in crate::world) mod emergency_close;

#[path="incremental_normal_world_rule_lifecycle_io_record.rs"]
mod io_record;

#[path="incremental_normal_world_rule_lifecycle_supply_close.rs"]
mod supply_close;
#[path="incremental_normal_world_rule_lifecycle_order.rs"]
mod order;

#[path="incremental_normal_world_rule_lifecycle_selection.rs"]
mod selection;

#[path="incremental_normal_world_rule_observe.rs"]
pub(in crate::world) mod observe;

#[path="incremental_normal_world_rule_lifecycle_driver_written.rs"]
pub(super) mod written;
#[path="incremental_normal_world_rule_lifecycle_publication.rs"]
pub(super) mod publication;

#[path="incremental_normal_world_rule_lifecycle_driver_top.rs"]
pub(super) mod top;

#[path="incremental_normal_world_rule_lifecycle_driver_sources.rs"]
pub(in crate::world) mod driver_sources;
#[path="incremental_normal_world_rule_lifecycle_driver_select.rs"]
mod driver_select;
#[path="incremental_normal_world_rule_lifecycle_driver_execute.rs"]
mod driver_execute;
#[path="incremental_normal_world_rule_lifecycle_driver_settle.rs"]
mod driver_settle;
#[path="incremental_normal_world_rule_lifecycle_driver.rs"]
mod driver;
pub(in crate::world) use driver_sources::{NormalWorldRuleLifecycleSourcesV1,NormalWorldRuleLifecycleWorkV1};
