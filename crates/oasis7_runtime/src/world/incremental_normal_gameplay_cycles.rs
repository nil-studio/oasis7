//! Original gameplay-cycle stage over actual qualified Current. This module's
//! event owner is private-construction and cannot authorize a completed tick.
use crate::world::incremental_body::BodyMutationError;
use crate::world::incremental_module_execution_manifest_hash::{
    CurrentWorldManifestHashV1, prepare_current_world_manifest_hash_v1,
};
use oasis7_formal_protocol::incremental_execution::ExecutionFence;
use oasis7_formal_protocol::incremental_value::ValueReadContext;
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::WorkIdentityV1,
    incremental_execution::{ExecutionTransaction, PreparedGuard, RootSetV1},
    incremental_state::MerkleNodeReader,
};
use oasis7_proto::memory_budget::ReservedValue;
use oasis7_proto::memory_budget::{MemoryBudget, MemoryReservation};
/// Created only after the fixed fallback/directive reducer has atomically
/// staged its entire actual Current write set. The common sink consumes it.
pub(in crate::world) struct PreparedGameplayCycleEventV1 {
    event: crate::WorldEvent,
    guard: PreparedGuard,
    identity: WorkIdentityV1,
    memory: MemoryBudget,
    now: u64,
    _event_fee: MemoryReservation,
    _directive_fee: Option<std::sync::Arc<ReservedValue<()>>>,
}
impl PreparedGameplayCycleEventV1 {
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
        tx.validate_guard(&self.guard, live)?;
        if self.event.time != now
            || self.event.caused_by.is_some()
            || !matches!(
                &self.event.body,
                crate::WorldEventBody::Domain(
                    crate::DomainEvent::EconomicContractExpired { .. }
                        | crate::DomainEvent::GovernanceProposalFinalized { .. }
                        | crate::DomainEvent::CrisisSpawned { .. }
                        | crate::DomainEvent::CrisisResolved { .. }
                        | crate::DomainEvent::CrisisTimedOut { .. }
                        | crate::DomainEvent::WarConcluded { .. }
                        | crate::DomainEvent::MetaProgressGranted { .. }
                )
            )
        {
            return Err(BodyMutationError::Invalid(
                "fixed gameplay-cycle event domain",
            ));
        }
        if self.identity != identity
            || self.now != now
            || !self.memory.same_pool(memory)
            || self
                ._directive_fee
                .as_ref()
                .is_some_and(|fee| !fee.uses_budget(memory))
            || !tx.uses_budget(memory)
        {
            return Err(BodyMutationError::Invalid(
                "Current gameplay applied event source",
            ));
        }
        Ok(())
    }
}

#[path = "incremental_normal_gameplay_cycles_kernel.rs"]
pub(in crate::world) mod kernel;

pub(in crate::world) struct NormalGameplayCycleSourcesV1<'a> {
    pub roots: &'a RootSetV1,
    pub directory: &'a ReservedValue<
        oasis7_world_state::incremental_normal_gameplay_cycles_directory::GameplayCycleDirectoryV1,
    >,
    pub diplomacy: &'a ReservedValue<
        oasis7_world_state::incremental_diplomacy::DiplomacyFamilyQualificationV1,
    >,
    pub governance: &'a ReservedValue<
        oasis7_world_state::incremental_governance_records::GovernanceFamilyQualificationV1,
    >,
    pub native: &'a ReservedValue<
        oasis7_world_state::incremental_world_native::WorldNativeFieldsQualificationV1,
    >,
    pub agents: &'a ReservedValue<
        oasis7_world_state::incremental_agent_subtree::AgentSubtreeStructureQualificationV1,
    >,
    pub retention:
        &'a ReservedValue<super::incremental_world_event_counter::WorldJournalRetentionV1>,
}
/// The caller borrows one continuously consumed work owner across fallback,
/// tick routing and directives. No phase constructs or replenishes a budget.
pub(in crate::world) struct NormalGameplayCycleWorkV1 {
    pub directory:
        oasis7_world_state::incremental_normal_gameplay_cycles_directory::GameplayDirectoryWorkV1,
    pub diplomacy_index: oasis7_world_state::incremental_diplomacy::DiplomacyIndexWorkV1,
    pub selected_scan: oasis7_formal_protocol::incremental_state::StateScanBudget,
    pub cells_remaining: u64,
    pub pending_entries_remaining: usize,
    pub native: oasis7_world_state::incremental_world_native::WorldNativeSelectedReadWorkV1,
    pub native_scan: oasis7_formal_protocol::incremental_state::StateScanBudget,
    pub native_write: oasis7_world_state::incremental_world_state_genesis::TypedRecordWriteWorkV1,
    pub meta: oasis7_world_state::incremental_world_native::MetaProgressMutationWorkV1,
    pub due_scan: oasis7_formal_protocol::incremental_schedule::ScheduleScanBudget,
    pub due_limits: super::incremental_scheduler::DueModuleBatchLimitsV1,
    pub max_text_bytes: usize,
    pub max_pending_node_bytes: usize,
    pub max_directive_depth: usize,
    pub journal: super::incremental_normal_industry_due::delivery::ProductValidationDeliveryWorkV1,
}

#[derive(Debug)]
pub(in crate::world) enum GameplayCycleErrorV1 {
    Value(oasis7_formal_protocol::incremental_value::ValueTreeError),
    Body(BodyMutationError),
    Overlay(oasis7_world_state::incremental_field_overlay::WorldFieldOverlayErrorV1),
    Business(ReservedValue<crate::WorldError>),
}
impl From<BodyMutationError> for GameplayCycleErrorV1 {
    fn from(e: BodyMutationError) -> Self {
        Self::Body(e)
    }
}
impl From<oasis7_formal_protocol::incremental_value::ValueTreeError> for GameplayCycleErrorV1 {
    fn from(e: oasis7_formal_protocol::incremental_value::ValueTreeError) -> Self {
        Self::Value(e)
    }
}
impl From<oasis7_world_state::incremental_field_overlay::WorldFieldOverlayErrorV1>
    for GameplayCycleErrorV1
{
    fn from(e: oasis7_world_state::incremental_field_overlay::WorldFieldOverlayErrorV1) -> Self {
        Self::Overlay(e)
    }
}
impl From<oasis7_formal_protocol::incremental_execution::ExecutionError> for GameplayCycleErrorV1 {
    fn from(e: oasis7_formal_protocol::incremental_execution::ExecutionError) -> Self {
        Self::Body(e.into())
    }
}
impl GameplayCycleErrorV1 {
    pub(super) fn into_root<OE>(
        self,
    ) -> super::incremental_module_invocation::RootModuleExecutionErrorV1<OE> {
        use super::incremental_module_invocation::RootModuleExecutionErrorV1 as R;
        match self{Self::Value(e)=>R::Value(e),Self::Body(e)=>R::Body(e),Self::Business(e)=>R::Business(e),Self::Overlay(e)=>R::Body(match e{oasis7_world_state::incremental_field_overlay::WorldFieldOverlayErrorV1::Value(e)=>e.into(),oasis7_world_state::incremental_field_overlay::WorldFieldOverlayErrorV1::Execution(e)=>e.into(),oasis7_world_state::incremental_field_overlay::WorldFieldOverlayErrorV1::Member(e)=>BodyMutationError::Original(Box::new(e))})}
    }
}

#[path = "incremental_normal_gameplay_cycles_tick_route.rs"]
mod tick_route;
pub(in crate::world) use tick_route::{CapturedGameplayWorldHashV1, DueGameplayTickSlotV1};

#[path = "incremental_normal_gameplay_cycles_failure.rs"]
mod failure;
pub(in crate::world) use failure::PreparedGameplayDirectiveFailureV1;

#[path = "incremental_normal_gameplay_cycles_commit.rs"]
mod commit;
#[path = "incremental_normal_gameplay_cycles_decode.rs"]
mod decode;
#[path = "incremental_normal_gameplay_cycles_driver.rs"]
mod driver;
#[path = "incremental_normal_gameplay_cycles_io.rs"]
mod io;
#[path = "incremental_normal_gameplay_cycles_journal.rs"]
mod journal;
#[path = "incremental_normal_gameplay_cycles_plan.rs"]
mod plan;
#[path = "incremental_normal_gameplay_cycles_select.rs"]
mod select;
#[path = "incremental_normal_gameplay_cycles_selected.rs"]
mod selected;
#[path = "incremental_normal_gameplay_cycles_war.rs"]
mod war;

pub(in crate::world) use io::run as with_gameplay_cycle_io_v1;
pub(in crate::world) use selected::{
    open_diplomacy as open_current_gameplay_diplomacy_v1,
    scalar as read_current_gameplay_scalar_v1, text as read_current_gameplay_text_v1,
};

pub(in crate::world) use selected::text_with_limit as read_current_diplomacy_text_with_limit_v1;

pub(in crate::world) use selected::{member_text_with_limit as read_current_diplomacy_member_text_with_limit_v1,member_length as read_current_diplomacy_member_length_v1};
