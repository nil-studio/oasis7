//! Genuine Selected Alliance lifecycle on one Current transaction.
//! Business state, original audits and the counted event retain distinct owners.
#[path = "incremental_normal_action_diplomacy_alloc.rs"]
mod alloc;
#[path = "incremental_normal_action_diplomacy_commit.rs"]
mod commit;
#[path = "incremental_normal_action_diplomacy_controller.rs"]
mod controller;
#[path = "incremental_normal_action_diplomacy_facts.rs"]
mod facts;
#[path = "incremental_normal_action_diplomacy_hash.rs"]
mod hash;
#[path = "incremental_normal_action_diplomacy_members.rs"]
mod members;
#[path = "incremental_normal_action_diplomacy_prepare.rs"]
mod prepare;
#[path = "incremental_normal_action_diplomacy_publish.rs"]
mod publish;
#[path = "incremental_normal_action_diplomacy_read.rs"]
mod read;
use super::{
    incremental_action_execution::{SelectedActionAuditV1, SelectedPendingActionV1},
    incremental_body::BodyMutationError,
    incremental_module_invocation::{RootRulePhaseOutcomeV1, SelectedAcceptedDispositionV1},
};
pub(in crate::world) use commit::commit_prepared_alliance_action_v1;
use facts::AllianceEvaluationErrorV1;
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::{FormalBoundedContextV1, WorkIdentityV1},
    incremental_execution::{
        ExecutionError, ExecutionFence, ExecutionTransaction, PreparedGuard, RootSetV1,
    },
    incremental_state::{MerkleNodeReader, StateScanBudget},
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation, ReservedValue};
use oasis7_world_state::{
    AllianceState,
    incremental_agent_subtree::AgentSubtreeStructureQualificationV1,
    incremental_diplomacy::{
        DiplomacyFamilyQualificationV1, DiplomacyIndexWorkV1, DiplomacyRecordQualificationV1,
    },
    incremental_world_native::WorldNativeFieldsQualificationV1,
    incremental_world_rule_composite::{
        WorldRuleCompositeReadWorkV1, WorldRuleCompositeWriteWorkV1,
    },
    incremental_world_rule_continuity::OrganizationContinuityQualificationV1,
    incremental_world_rule_continuity::PreparedContinuityMembershipV1,
    incremental_world_rule_organization::root::OrganizationFamilyQualificationV1,
};
pub(in crate::world) use prepare::prepare_alliance_action_v1;
use std::mem::size_of;
pub(in crate::world) struct AllianceSourcesV1<'a, 'source> {
    pub agents: &'a ReservedValue<AgentSubtreeStructureQualificationV1>,
    pub diplomacy: &'a ReservedValue<DiplomacyFamilyQualificationV1>,
    pub continuity: &'a ReservedValue<OrganizationContinuityQualificationV1>,
    pub organizations: &'a ReservedValue<OrganizationFamilyQualificationV1>,
    pub native: &'a ReservedValue<WorldNativeFieldsQualificationV1>,
    pub orders: &'a ReservedValue<
        super::incremental_factory_directory_orders::CurrentFactoryOrderDirectoryV1<'source>,
    >,
    pub roots: &'a RootSetV1,
}
pub(in crate::world) struct AllianceWorkV1 {
    pub pending_entries_remaining: usize,
    pub index: DiplomacyIndexWorkV1,
    pub scope_scan: StateScanBudget,
    pub scope_cells: u64,
    pub max_text_bytes: usize,
    pub max_pending_node_bytes: usize,
    pub write_cells: u64,
    pub continuity_read: WorldRuleCompositeReadWorkV1,
    pub continuity_write: WorldRuleCompositeWriteWorkV1,
    pub controller: super::incremental_industrial_eligibility::IndustrialEligibilityWorkV1,
}
type Fees = [Option<ReservedValue<()>>; 4];
struct EvaluatedAllianceBodyV1 {
    body: crate::WorldEventBody,
    fees: Fees,
}
pub(in crate::world) struct PreparedAllianceActionV1<'s, 'r, 'source> {
    selected: &'s SelectedPendingActionV1,
    outcome: &'r ReservedValue<RootRulePhaseOutcomeV1>,
    sources: &'s AllianceSourcesV1<'s, 'source>,
    body: crate::WorldEventBody,
    before: Option<ReservedValue<AllianceState>>,
    proof: Option<ReservedValue<DiplomacyRecordQualificationV1>>,
    created: Option<ReservedValue<AllianceState>>,
    after_members: Option<ReservedValue<Vec<String>>>,
    membership: Option<PreparedContinuityMembershipV1>,
    fees: Fees,
    verification: Fees,
    identity: PreparedGuard,
    core: WorkIdentityV1,
    memory: MemoryBudget,
    time: u64,
    event_fee: MemoryReservation,
    layout: ReservedValue<()>,
}
/// No public constructor: minted after all real Alliance/continuity CAS writes.
pub(in crate::world) struct PreparedAllianceEventV1 {
    event: ReservedValue<crate::WorldEvent>,
    accepted: Option<SelectedActionAuditV1>,
    identity: PreparedGuard,
    _fees: Fees,
    _layout: ReservedValue<()>,
}
impl PreparedAllianceEventV1 {
    pub(in crate::world) fn event(&self) -> &crate::WorldEvent {
        &self.event
    }
    pub(in crate::world) fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        now: u64,
    ) -> Result<(), BodyMutationError> {
        tx.validate_transaction_identity(&self.identity)?;
        if !self.event.uses_budget(memory) || !tx.uses_budget(memory) || self.event.time != now {
            return Err(bad());
        }
        if let Some(accepted) = &self.accepted {
            accepted.validate(tx)?;
            if !accepted.uses_budget(memory) {
                return Err(bad());
            }
        }
        Ok(())
    }
}
pub(in crate::world) enum CompletedAllianceActionV1 {
    Rejected {
        accepted: Option<SelectedActionAuditV1>,
        event: SelectedActionAuditV1,
        _fees: Fees,
        _layout: ReservedValue<()>,
    },
    Applied(PreparedAllianceEventV1),
}
impl CompletedAllianceActionV1 {
    pub(in crate::world) fn event(&self) -> &crate::WorldEvent {
        match self {
            Self::Rejected { event, .. } => event.event(),
            Self::Applied(value) => value.event(),
        }
    }
    pub(in crate::world) fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
    ) -> Result<(), ExecutionError> {
        match self {
            Self::Rejected {
                accepted, event, ..
            } => {
                if let Some(accepted) = accepted {
                    accepted.validate(tx)?;
                }
                event.validate(tx)
            }
            Self::Applied(value) => {
                tx.validate_transaction_identity(&value.identity)?;
                if let Some(accepted) = &value.accepted {
                    accepted.validate(tx)?;
                }
                Ok(())
            }
        }
    }
}
fn bad() -> BodyMutationError {
    BodyMutationError::Invalid("Alliance prepared Current source")
}
fn limited() -> BodyMutationError {
    ValueTreeError::ResourceLimited("Alliance prepared layout").into()
}
fn charge(common: &mut FormalBoundedContextV1<'_>, n: usize) -> Result<(), BodyMutationError> {
    common
        .charge(1, u64::try_from(n).map_err(|_| limited())?)
        .map_err(|e| BodyMutationError::Original(Box::new(e)))
}
fn overlay(
    e: oasis7_world_state::incremental_field_overlay::WorldFieldOverlayErrorV1,
) -> BodyMutationError {
    match e {
        oasis7_world_state::incremental_field_overlay::WorldFieldOverlayErrorV1::Execution(e) => {
            e.into()
        }
        oasis7_world_state::incremental_field_overlay::WorldFieldOverlayErrorV1::Value(e) => {
            e.into()
        }
        oasis7_world_state::incremental_field_overlay::WorldFieldOverlayErrorV1::Member(e) => {
            BodyMutationError::Original(Box::new(e))
        }
    }
}
#[derive(Debug)]
struct AllianceOriginalErrorV1(crate::WorldError);
impl std::fmt::Display for AllianceOriginalErrorV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.0)
    }
}
impl std::error::Error for AllianceOriginalErrorV1 {}
fn original(error: crate::WorldError) -> BodyMutationError {
    BodyMutationError::Original(Box::new(AllianceOriginalErrorV1(error)))
}
fn evaluated(error: AllianceEvaluationErrorV1) -> BodyMutationError {
    match error {
        AllianceEvaluationErrorV1::Body(e) => e,
        AllianceEvaluationErrorV1::World(e) => original(e),
    }
}
fn membership_error(
    error: oasis7_world_state::incremental_world_rule_continuity::ContinuityMembershipErrorV1,
) -> BodyMutationError {
    use oasis7_world_state::incremental_world_rule_continuity::ContinuityMembershipErrorV1 as E;
    match error {
        E::World(e) => original(e),
        E::Overlay(e) => overlay(e),
        E::Work(e) => BodyMutationError::Original(Box::new(e)),
    }
}
