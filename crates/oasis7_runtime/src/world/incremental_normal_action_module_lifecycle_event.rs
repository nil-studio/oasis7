//! Closed Action-caused publication sources for actual marketplace transitions.
use super::*;
use crate::world::incremental_action_execution::SelectedActionAuditV1;
use oasis7_proto::memory_budget::MemoryBudget;

pub(in crate::world) struct PreparedNormalModuleLifecycleEventV1 {
    pub(super) event: ReservedValue<crate::WorldEvent>,
    pub(super) accepted: Option<SelectedActionAuditV1>,
    pub(super) guard: PreparedGuard,
    pub(super) core: WorkIdentityV1,
    pub(super) action_id: u64,
    pub(super) memory: MemoryBudget,
    pub(super) state: ModuleLifecycleCommittedStateV1,
    pub(super) _layout: MemoryReservation,
    pub(super) _prepare_fees: [Option<MemoryReservation>; 16],
}
pub(super) enum ModuleLifecycleCommittedStateV1 {
    ReleaseRejection(Box<super::release_plan::PreparedCurrentReleaseRejectionV1>),
    Release(super::release_commit::CommittedCurrentModuleReleaseStateV1),
    Market(super::commit::CommittedModuleMarketStateV1),
    Proposal(super::governance_state::CommittedModuleProposalStateV1),
    Install(super::instance_tail_commit::CommittedModuleInstallStateV1),
    InstallFailure(Box<super::install_driver::CurrentSelectedInstallFailureV1>),
}
impl PreparedNormalModuleLifecycleEventV1 {
    pub(in crate::world) fn action_id(&self) -> u64 {
        self.action_id
    }
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
        tx.validate_transaction_identity(&self.guard)?;
        if let Some(audit) = &self.accepted {
            audit.validate(tx)?
        }
        match &self.state {
            ModuleLifecycleCommittedStateV1::ReleaseRejection(state) => {
                state.validate(tx, memory, identity, now, live)?
            }
            ModuleLifecycleCommittedStateV1::Release(state) => {
                state.validate(tx, memory, now, identity, live)?
            }
            ModuleLifecycleCommittedStateV1::Market(state) => {
                state.validate_evidence_v1(tx, memory, now, identity)?
            }
            ModuleLifecycleCommittedStateV1::Proposal(state) => {
                state.validate_evidence_v1(tx, memory, now, identity, live)?
            }
            ModuleLifecycleCommittedStateV1::Install(state) => {
                state.validate(tx, memory, now, identity, live)?
            }
            ModuleLifecycleCommittedStateV1::InstallFailure(state) => {
                state.validate(tx, memory, identity, now, live)?
            }
        }
        if !self.memory.same_pool(memory)
            || !self.event.uses_budget(memory)
            || !self._layout.uses_budget(memory)
            || !tx.uses_budget(memory)
            || self.core != identity
            || self.event.time != now
            || self.event.caused_by
                != match &self.state {
                    ModuleLifecycleCommittedStateV1::ReleaseRejection(_)
                    | ModuleLifecycleCommittedStateV1::Release(_)
                    | ModuleLifecycleCommittedStateV1::Market(_)
                    | ModuleLifecycleCommittedStateV1::Install(_)
                    | ModuleLifecycleCommittedStateV1::InstallFailure(_) => {
                        Some(crate::CausedBy::Action(self.action_id))
                    }
                    ModuleLifecycleCommittedStateV1::Proposal(_) => None,
                }
            || self.accepted.as_ref().is_some_and(|a| {
                a.event().caused_by != Some(crate::CausedBy::Action(self.action_id))
            })
        {
            return Err(invalid());
        }
        match (&self.state, &self.event.body) {
            (
                ModuleLifecycleCommittedStateV1::ReleaseRejection(state),
                crate::WorldEventBody::Domain(crate::DomainEvent::ActionRejected {
                    action_id,
                    reason,
                }),
            ) if *action_id == state.action_id() && reason == &*state.reason => Ok(()),
            (
                ModuleLifecycleCommittedStateV1::Release(state),
                crate::WorldEventBody::Domain(body),
            ) if body == state.event() => Ok(()),
            (
                ModuleLifecycleCommittedStateV1::Market(_),
                crate::WorldEventBody::Domain(
                    crate::DomainEvent::ModuleArtifactDeployed { .. }
                    | crate::DomainEvent::ModuleArtifactListed { .. }
                    | crate::DomainEvent::ModuleArtifactBidPlaced { .. }
                    | crate::DomainEvent::ModuleArtifactBidCancelled { .. }
                    | crate::DomainEvent::ModuleArtifactSaleCompleted { .. }
                    | crate::DomainEvent::ModuleArtifactDelisted { .. }
                    | crate::DomainEvent::ModuleArtifactDestroyed { .. },
                ),
            ) => Ok(()),
            (
                ModuleLifecycleCommittedStateV1::Install(state),
                crate::WorldEventBody::Domain(
                    body @ (crate::DomainEvent::ModuleInstalled { .. }
                    | crate::DomainEvent::ModuleUpgraded { .. }
                    | crate::DomainEvent::ModuleRollbackApplied { .. }),
                ),
            ) if body == state.event() => Ok(()),
            (
                ModuleLifecycleCommittedStateV1::InstallFailure(state),
                crate::WorldEventBody::Domain(crate::DomainEvent::ActionRejected {
                    action_id, ..
                }),
            ) if *action_id == state.action_id() => Ok(()),
            (
                ModuleLifecycleCommittedStateV1::Proposal(state),
                crate::WorldEventBody::Governance(body),
            ) => validate_proposal_body_v1(state, body),
            _ => Err(invalid()),
        }
    }
}

pub(in crate::world) enum CompletedModuleLifecycleActionV1 {
    Applied {
        first: PreparedNormalModuleLifecycleEventV1,
        second: Option<PreparedNormalModuleLifecycleEventV1>,
    },
    Rejected {
        accepted: Option<SelectedActionAuditV1>,
        event: SelectedActionAuditV1,
        core: WorkIdentityV1,
        memory: MemoryBudget,
        _layout: MemoryReservation,
        _prepare_fees: [Option<MemoryReservation>; 16],
    },
}
impl CompletedModuleLifecycleActionV1 {
    pub(in crate::world) fn event(&self) -> &crate::WorldEvent {
        match self {
            Self::Applied { first, second } => second.as_ref().unwrap_or(first).event(),
            Self::Rejected { event, .. } => event.event(),
        }
    }
    pub(in crate::world) fn validate_transaction<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
    ) -> Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> {
        match self {
            Self::Applied { first, second } => {
                tx.validate_transaction_identity(&first.guard)?;
                if let Some(second) = second {
                    tx.validate_transaction_identity(&second.guard)?
                }
                if let Some(a) = &first.accepted {
                    a.validate(tx)?
                }
                Ok(())
            }
            Self::Rejected {
                accepted, event, ..
            } => {
                if let Some(a) = accepted {
                    a.validate(tx)?
                }
                event.validate(tx)
            }
        }
    }
    pub(in crate::world) fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        now: u64,
        identity: WorkIdentityV1,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        self.validate_transaction(tx)?;
        match self {
            Self::Applied { first, second } => {
                first.validate(tx, memory, now, identity, live)?;
                if let Some(second) = second {
                    second.validate(tx, memory, now, identity, live)?
                }
                Ok(())
            }
            Self::Rejected {
                core,
                memory: pool,
                _layout,
                event,
                ..
            } => {
                if *core != identity
                    || !pool.same_pool(memory)
                    || !_layout.uses_budget(memory)
                    || event.event().time != now
                {
                    Err(invalid())
                } else {
                    Ok(())
                }
            }
        }
    }
}

#[path = "incremental_normal_action_module_lifecycle_proposal_event.rs"]
mod proposal;
pub(super) use proposal::{prepare_proposal_event_v1, validate_proposal_body_v1};

#[path = "incremental_normal_action_module_lifecycle_install_failure_event.rs"]
mod install_failure;
pub(super) use install_failure::publish_current_module_install_failure_v1;

pub(super) use super::release_rejection_event::publish_current_release_rejection_v1;
