//! Staged transition adapters for governance proposal state, identity penalty /
//! emergency brake sidecars, and organization continuity writes.
//!
//! These adapters implement [`StagedTransition`] and [`SubsystemDelta`] over
//! the canonical reducers and [`RootInstallContext`], preparing the #6141 PR 5/5
//! cutover without altering `WorldCommitmentEnvelope v2` wire schema or active
//! write entrypoints in this slice.

use oasis7_agent_api::world_action::WorldRuleCommandV1;

use crate::world::event_processing::prepared_governance_events;
use crate::world::event_processing::publication::{
    PreparedEventPublication, PreparedEventStateDelta,
};
use crate::world::transition::{
    BorrowedWorldView, PreparedStagedCommit, RootInstallContext, StagedTransition, SubsystemDelta,
    SynchronousEventTransition, TransitionContext, TransitionReject, publish_synchronous_event,
};
use crate::{
    CausedBy, DomainEvent, GovernanceEvent, Proposal, ProposalId, TickConsensusRecord, World,
    WorldError, WorldEvent, WorldEventBody, WorldEventId,
};

#[cfg_attr(
    not(test),
    expect(dead_code, reason = "cutover wiring deferred to #6141 PR 5/5")
)]
pub(crate) enum StagedInstitutionsGovernanceTransition<'a> {
    ProposalState(&'a GovernanceEvent),
    IdentityPenaltySidecar(&'a GovernanceEvent),
    EmergencyBrakeSidecar(&'a GovernanceEvent),
    OrganizationContinuity(&'a DomainEvent),
}

impl<'a> StagedInstitutionsGovernanceTransition<'a> {
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "cutover wiring deferred to #6141 PR 5/5")
    )]
    pub(crate) fn from_body(body: &'a WorldEventBody) -> Option<Self> {
        match body {
            WorldEventBody::Governance(event) => match event {
                GovernanceEvent::Proposed { .. }
                | GovernanceEvent::ShadowReport { .. }
                | GovernanceEvent::Approved { .. }
                | GovernanceEvent::Queued { .. }
                | GovernanceEvent::Applied { .. }
                | GovernanceEvent::EmergencyVetoed { .. }
                | GovernanceEvent::FinalityEpochSnapshotSet { .. }
                | GovernanceEvent::FinalityEpochSnapshotRemoved { .. } => {
                    Some(Self::ProposalState(event))
                }
                GovernanceEvent::IdentityPenaltyApplied { .. }
                | GovernanceEvent::IdentityPenaltyAppealed { .. }
                | GovernanceEvent::IdentityPenaltyResolved { .. } => {
                    Some(Self::IdentityPenaltySidecar(event))
                }
                GovernanceEvent::EmergencyBrakeActivated { .. }
                | GovernanceEvent::EmergencyBrakeReleased { .. } => {
                    Some(Self::EmergencyBrakeSidecar(event))
                }
                _ => None,
            },
            WorldEventBody::Domain(
                event @ DomainEvent::WorldRuleCommitted {
                    command: WorldRuleCommandV1::OrganizationContinuity { .. },
                    ..
                },
            ) => Some(Self::OrganizationContinuity(event)),
            _ => None,
        }
    }

    fn prepare_state_delta(&self, world: &World) -> Result<(WorldEventBody, PreparedEventStateDelta), WorldError> {
        match self {
            Self::ProposalState(event) | Self::IdentityPenaltySidecar(event) => {
                let body = WorldEventBody::Governance((*event).clone());
                let delta = prepared_governance_events::prepare(world, &body)?.ok_or_else(|| {
                    WorldError::GovernancePolicyInvalid {
                        reason: "unclassified governance event in institutions adapter".to_string(),
                    }
                })?;
                Ok((body, delta))
            }
            Self::EmergencyBrakeSidecar(event) => {
                let body = WorldEventBody::Governance((*event).clone());
                let delta = PreparedEventStateDelta::for_body(world, &body).ok_or_else(|| {
                    WorldError::GovernancePolicyInvalid {
                        reason: "unclassified emergency brake event in institutions adapter"
                            .to_string(),
                    }
                })?;
                Ok((body, delta))
            }
            Self::OrganizationContinuity(event) => {
                let DomainEvent::WorldRuleCommitted { world_binding, .. } = *event else {
                    return Err(crate::world_rules::invalid(
                        "expected_organization_continuity_world_rule",
                    ));
                };
                if world_binding != &world.world_rule_binding() {
                    return Err(crate::world_rules::invalid("world_rule_binding_mismatch"));
                }
                let prepared = world.prepare_world_rule_event(event, world.state.time)?;
                Ok((
                    WorldEventBody::Domain((*event).clone()),
                    PreparedEventStateDelta::WorldRule(Box::new(prepared)),
                ))
            }
        }
    }
}

pub(crate) struct InstitutionsGovernanceDelta {
    event: WorldEvent,
    state_delta: PreparedEventStateDelta,
    next_event_id: WorldEventId,
    next_event_id_era: u64,
    journal_events: Vec<WorldEvent>,
    journal_events_evicted: u64,
}

impl SubsystemDelta for InstitutionsGovernanceDelta {
    fn apply_infallible(self, ctx: &mut RootInstallContext<'_>) {
        self.state_delta.install_infallible(ctx.world_mut());
        ctx.state_mut().time = self.event.time;
        ctx.install_journal_tail(
            self.next_event_id,
            self.next_event_id_era,
            self.journal_events,
            self.journal_events_evicted,
        );
        if let WorldEventBody::Domain(event) = &self.event.body {
            ctx.state_mut().route_domain_event(event);
        }
    }
}

impl StagedTransition for StagedInstitutionsGovernanceTransition<'_> {
    type Delta = InstitutionsGovernanceDelta;

    fn prepare(
        &self,
        view: &BorrowedWorldView<'_>,
        ctx: &TransitionContext,
    ) -> Result<PreparedStagedCommit<Self::Delta>, TransitionReject> {
        let (_, consensus, delta) = self.prepare_event(view.world(), ctx)?;
        Ok(PreparedStagedCommit::new(
            view.base_head().clone(),
            Some(consensus),
            delta,
        ))
    }
}

impl SynchronousEventTransition for StagedInstitutionsGovernanceTransition<'_> {
    fn prepare_event(
        &self,
        world: &World,
        ctx: &TransitionContext,
    ) -> Result<(WorldEventId, TickConsensusRecord, Self::Delta), TransitionReject> {
        let (body, state_delta) = self.prepare_state_delta(world)?;
        let PreparedEventPublication {
            event,
            next_event_id,
            next_event_id_era,
            journal_events,
            journal_events_evicted,
            consensus_record,
            state_delta,
        } = world.prepare_event_publication(body, ctx.caused_by.clone(), state_delta)?;
        Ok((
            event.id,
            consensus_record,
            InstitutionsGovernanceDelta {
                event,
                state_delta,
                next_event_id,
                next_event_id_era,
                journal_events,
                journal_events_evicted,
            },
        ))
    }
}

/// Staged adapter for atomic `Approved` (+ optional `Queued`) governance proposal transitions.
#[cfg_attr(not(test), expect(dead_code, reason = "cutover deferred to #6141 PR 5/5"))]
pub(crate) struct StagedGovernanceApproval<'a> {
    pub(crate) approved: &'a GovernanceEvent,
    pub(crate) queued: Option<&'a GovernanceEvent>,
}

pub(crate) struct GovernanceApprovalDelta {
    proposal_id: ProposalId,
    proposal: Proposal,
    events: Vec<WorldEvent>,
    next_event_id: WorldEventId,
    next_event_id_era: u64,
    journal_events: Vec<WorldEvent>,
    journal_events_evicted: u64,
}

impl SubsystemDelta for GovernanceApprovalDelta {
    fn apply_infallible(self, ctx: &mut RootInstallContext<'_>) {
        ctx.world_mut().proposals.insert(self.proposal_id, self.proposal);
        if let Some(event) = self.events.last() {
            ctx.state_mut().time = event.time;
        }
        ctx.install_journal_tail(
            self.next_event_id,
            self.next_event_id_era,
            self.journal_events,
            self.journal_events_evicted,
        );
    }
}

impl StagedTransition for StagedGovernanceApproval<'_> {
    type Delta = GovernanceApprovalDelta;

    fn prepare(
        &self,
        view: &BorrowedWorldView<'_>,
        _ctx: &TransitionContext,
    ) -> Result<PreparedStagedCommit<Self::Delta>, TransitionReject> {
        let prepared = view
            .world()
            .prepare_governance_approval(self.approved.clone(), self.queued.cloned())?;
        Ok(PreparedStagedCommit::new(
            view.base_head().clone(),
            Some(prepared.consensus_record),
            GovernanceApprovalDelta {
                proposal_id: prepared.proposal_id,
                proposal: prepared.proposal,
                events: prepared.events,
                next_event_id: prepared.next_event_id,
                next_event_id_era: prepared.next_event_id_era,
                journal_events: prepared.journal_events,
                journal_events_evicted: prepared.journal_events_evicted,
            },
        ))
    }
}

impl World {
    #[cfg_attr(not(test), expect(dead_code, reason = "cutover deferred to #6141 PR 5/5"))]
    pub(crate) fn append_staged_institutions_governance_event(
        &mut self,
        transition: StagedInstitutionsGovernanceTransition<'_>,
        caused_by: Option<CausedBy>,
    ) -> Result<WorldEventId, WorldError> {
        let ctx = TransitionContext::new(self.state.time, caused_by);
        publish_synchronous_event(self, &transition, &ctx)
    }
}
