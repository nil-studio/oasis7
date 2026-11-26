//! Original alliance and governance-domain events in the shared staged publication boundary.
//! Pure WorldState reducers remain the single authority; no new durable IWS receipt is minted.

use super::{PreparedEventPublication, PreparedEventStateDelta};
use crate::world::alliance_war_publication::PreparedAllianceWarEvent;
use crate::world::governance_meta_publication::PreparedGovernanceMetaEvent;
use crate::world::transition::{
    BorrowedWorldView, PreparedStagedCommit, RootInstallContext, StagedTransition, SubsystemDelta,
    SynchronousEventTransition, TransitionContext, TransitionReject, publish_synchronous_event,
};
use crate::{
    CausedBy, DomainEvent, TickConsensusRecord, World, WorldError, WorldEvent, WorldEventBody,
    WorldEventId,
};

pub(super) enum StagedInstitutionsPublication<'a> {
    Alliance(&'a DomainEvent),
    Governance(&'a DomainEvent),
}

impl<'a> StagedInstitutionsPublication<'a> {
    pub(super) fn from_body(body: &'a WorldEventBody) -> Option<Self> {
        let WorldEventBody::Domain(event) = body else {
            return None;
        };
        match event {
            DomainEvent::AllianceFormed { .. }
            | DomainEvent::AllianceJoined { .. }
            | DomainEvent::AllianceLeft { .. }
            | DomainEvent::AllianceDissolved { .. }
            | DomainEvent::WarDeclared { .. }
            | DomainEvent::WarConcluded { .. } => Some(Self::Alliance(event)),
            DomainEvent::GovernanceProposalOpened { .. }
            | DomainEvent::GovernanceVoteCast { .. }
            | DomainEvent::GovernanceProposalFinalized { .. }
            | DomainEvent::CrisisSpawned { .. }
            | DomainEvent::CrisisResolved { .. }
            | DomainEvent::CrisisTimedOut { .. }
            | DomainEvent::MetaProgressGranted { .. }
            | DomainEvent::ProductValidated { .. } => Some(Self::Governance(event)),
            _ => None,
        }
    }
}

pub(super) struct InstitutionsDelta {
    event: WorldEvent,
    state_delta: PreparedEventStateDelta,
    next_event_id: WorldEventId,
    next_event_id_era: u64,
    journal_events: Vec<WorldEvent>,
    journal_events_evicted: u64,
}

impl SubsystemDelta for InstitutionsDelta {
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

impl StagedTransition for StagedInstitutionsPublication<'_> {
    type Delta = InstitutionsDelta;

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

impl SynchronousEventTransition for StagedInstitutionsPublication<'_> {
    fn prepare_event(
        &self,
        world: &World,
        ctx: &TransitionContext,
    ) -> Result<(WorldEventId, TickConsensusRecord, Self::Delta), TransitionReject> {
        let event = match self {
            Self::Alliance(event) | Self::Governance(event) => *event,
        };
        let state_delta = match self {
            Self::Alliance(_) => PreparedEventStateDelta::AllianceWar(
                PreparedAllianceWarEvent::prepare(&world.state, event, world.state.time)?,
            ),
            Self::Governance(_) => PreparedEventStateDelta::GovernanceMeta(
                PreparedGovernanceMetaEvent::prepare(&world.state, event, world.state.time)?,
            ),
        };
        let PreparedEventPublication {
            event, next_event_id, next_event_id_era, journal_events,
            journal_events_evicted, consensus_record, state_delta,
        } = world.prepare_event_publication(
            WorldEventBody::Domain(event.clone()),
            ctx.caused_by.clone(),
            state_delta,
        )?;
        Ok((event.id, consensus_record, InstitutionsDelta {
            event, state_delta, next_event_id, next_event_id_era,
            journal_events, journal_events_evicted,
        }))
    }
}

impl World {
    pub(super) fn append_staged_institutions_event(
        &mut self,
        transition: StagedInstitutionsPublication<'_>,
        caused_by: Option<CausedBy>,
    ) -> Result<WorldEventId, WorldError> {
        let ctx = TransitionContext::new(self.state.time, caused_by);
        publish_synchronous_event(self, &transition, &ctx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::transition::DurableCommitConfirmation;

    fn crisis(id: &str) -> WorldEventBody {
        WorldEventBody::Domain(DomainEvent::CrisisSpawned {
            crisis_id: id.into(), kind: "supply".into(), severity: 2, expires_at: 100,
        })
    }

    #[test]
    fn institutions_publication_failpoint_replay_and_delayed_fence() {
        let mut world = World::new();
        let before = world.snapshot();
        let journal = world.journal().clone();
        world.fail_next_append_after_publication_prepare_for_test();
        assert!(world.append_event(crisis("staged-crisis"), None).is_err());
        assert_eq!(world.snapshot(), before);
        assert_eq!(world.journal(), &journal);

        world.append_event(crisis("staged-crisis"), None).unwrap();
        let restored = World::from_snapshot(before, world.journal().clone()).unwrap();
        assert_eq!(restored.snapshot(), world.snapshot());

        let body = crisis("delayed-crisis");
        let transition = StagedInstitutionsPublication::from_body(&body).unwrap();
        let prepare = |world: &World| {
            let view = BorrowedWorldView::new(world).unwrap();
            transition.prepare(&view, &TransitionContext::new(view.time(), None)).unwrap()
        };
        let unchanged = world.snapshot();
        assert!(matches!(
            prepare(&world).install_confirmed(
                &mut world, DurableCommitConfirmation::StatusUnknown {
                    reason: "lost governance acknowledgment".into()
                }
            ),
            Err(TransitionReject::StatusUnknownFenced { .. })
        ));
        assert_eq!(world.snapshot(), unchanged);
        let pending = prepare(&world);
        world.append_event(crisis("other-crisis"), None).unwrap();
        let new_state = world.snapshot();
        assert!(matches!(
            pending.install_confirmed(&mut world, DurableCommitConfirmation::Confirmed),
            Err(TransitionReject::BaseHeadStale)
        ));
        assert_eq!(world.snapshot(), new_state);
    }
}
