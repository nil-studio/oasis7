//! Industrial event publication through the ordinary root staging boundary.
//!
//! Reuse the original sparse WorldState reducers and shared consensus/journal
//! preparation. This is an ordinary synchronous publication boundary, not an
//! IWS durable-acceptance receipt or the complete five-domain tick transaction.

use super::{PreparedEventPublication, PreparedEventStateDelta};
use crate::state::industry_history_transition::PreparedIndustryHistoryEvent;
use crate::state::industry_transition::PreparedIndustryEvent;
use crate::world::transition::{
    BorrowedWorldView, PreparedStagedCommit, RootInstallContext, StagedTransition,
    SubsystemDelta, SynchronousEventTransition, TransitionContext, TransitionReject,
    publish_synchronous_event,
};
use crate::{
    CausedBy, DomainEvent, TickConsensusRecord, World, WorldError, WorldEvent, WorldEventBody,
    WorldEventId,
};

pub(super) enum StagedIndustrialPublication<'a> {
    State(&'a DomainEvent),
    History(&'a DomainEvent),
    RouteOnly(&'a DomainEvent, &'a str),
    DeliveryCursor(&'a WorldEventBody),
}

impl<'a> StagedIndustrialPublication<'a> {
    pub(super) fn from_body(body: &'a WorldEventBody) -> Option<Self> {
        if matches!(body, WorldEventBody::ProductValidationDeliveryCursorUpdated(_)) {
            return Some(Self::DeliveryCursor(body));
        }
        let WorldEventBody::Domain(event) = body else {
            return None;
        };
        match event {
            DomainEvent::AgentLocationAuthorityUpdated { .. }
            | DomainEvent::LocationAnchorUpdated { .. }
            | DomainEvent::FactorySiteAuthorityUpdated { .. }
            | DomainEvent::FactoryConstructionPowerProfileUpdated { .. }
            | DomainEvent::ProductValidationRecorded { .. }
            | DomainEvent::ProductValidationAttemptStarted { .. } => Some(Self::History(event)),
            DomainEvent::LogisticsRouteRegistered { .. }
            | DomainEvent::LogisticsRouteAvailabilityChanged { .. }
            | DomainEvent::MaterialTransferred { .. }
            | DomainEvent::MaterialTransitStarted { .. }
            | DomainEvent::MaterialTransitCompleted { .. }
            | DomainEvent::FactoryBuildStarted { .. }
            | DomainEvent::FactoryBuilt { .. }
            | DomainEvent::FactoryDurabilityChanged { .. }
            | DomainEvent::FactoryMaintained { .. }
            | DomainEvent::FactoryRecycled { .. }
            | DomainEvent::RecipeStarted { .. }
            | DomainEvent::RecipeCompleted { .. }
            | DomainEvent::FactoryProductionBlocked { .. }
            | DomainEvent::FactoryProductionResumed { .. }
            | DomainEvent::FactoryProductionPaused { .. } => Some(Self::State(event)),
            DomainEvent::LogisticsPathRerouted { requester_agent_id, .. } => {
                Some(Self::RouteOnly(event, requester_agent_id))
            }
            _ => None,
        }
    }
}

pub(super) struct IndustrialPublicationDelta {
    event: WorldEvent,
    state_delta: PreparedEventStateDelta,
    next_event_id: WorldEventId,
    next_event_id_era: u64,
    journal_events: Vec<WorldEvent>,
    journal_events_evicted: u64,
}

impl SubsystemDelta for IndustrialPublicationDelta {
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

impl StagedTransition for StagedIndustrialPublication<'_> {
    type Delta = IndustrialPublicationDelta;

    fn prepare(
        &self,
        view: &BorrowedWorldView<'_>,
        ctx: &TransitionContext,
    ) -> Result<PreparedStagedCommit<Self::Delta>, TransitionReject> {
        let (_, consensus_record, delta) = self.prepare_event(view.world(), ctx)?;
        Ok(PreparedStagedCommit::new(
            view.base_head().clone(),
            Some(consensus_record),
            delta,
        ))
    }
}

impl SynchronousEventTransition for StagedIndustrialPublication<'_> {
    fn prepare_event(
        &self,
        world: &World,
        ctx: &TransitionContext,
    ) -> Result<(WorldEventId, TickConsensusRecord, Self::Delta), TransitionReject> {
        let (body, state_delta) = match self {
            Self::State(event) => (
                WorldEventBody::Domain((*event).clone()),
                PreparedEventStateDelta::Industry(PreparedIndustryEvent::prepare(
                    &world.state, event, world.state.time,
                )?),
            ),
            Self::History(event) => (
                WorldEventBody::Domain((*event).clone()),
                PreparedEventStateDelta::IndustryHistory(PreparedIndustryHistoryEvent::prepare(
                    &world.state, event, world.state.time,
                )?),
            ),
            Self::RouteOnly(event, agent_id) => (
                WorldEventBody::Domain((*event).clone()),
                PreparedEventStateDelta::DomainRouteOnly {
                    event: (*event).clone(),
                    agent_id: (*agent_id).to_owned(),
                },
            ),
            Self::DeliveryCursor(body) => (
                (*body).clone(),
                PreparedEventStateDelta::for_body(world, body).ok_or_else(|| {
                    WorldError::ResourceBalanceInvalid {
                        reason: "unclassified product validation delivery cursor".to_string(),
                    }
                })?,
            ),
        };
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
            IndustrialPublicationDelta {
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

impl World {
    pub(super) fn append_staged_industrial_event(
        &mut self,
        transition: StagedIndustrialPublication<'_>,
        caused_by: Option<CausedBy>,
    ) -> Result<WorldEventId, WorldError> {
        let ctx = TransitionContext::new(self.state.time, caused_by);
        publish_synchronous_event(self, &transition, &ctx)
    }
}

// The durable path retains its exact base-head and Unknown fence while the
// synchronous path uses only its uninterrupted exclusive borrow.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::LocationAnchorV1;
    use crate::world::transition::DurableCommitConfirmation;

    #[test]
    fn industrial_staged_history_publication_preserves_exact_cold_replay() {
        let mut staged = World::new();
        let before = staged.snapshot();
        let before_journal = staged.journal().clone();
        let body = WorldEventBody::Domain(DomainEvent::LocationAnchorUpdated {
            anchor: LocationAnchorV1 {
                location_id: "industrial-cold-replay-anchor".into(),
                active: true,
                authority_revision: 1,
                effective_at: 0,
            },
        });
        let transition = StagedIndustrialPublication::from_body(&body)
            .expect("original production industrial history classification");
        let prepared = {
            let view = BorrowedWorldView::new(&staged).unwrap();
            transition
                .prepare(&view, &TransitionContext::new(view.time(), None))
                .unwrap()
        };
        // Stage 1 must never change World memory, cursor or original journal.
        assert_eq!(staged.snapshot(), before);
        assert_eq!(staged.journal(), &before_journal);
        prepared
            .install_confirmed(&mut staged, DurableCommitConfirmation::Confirmed)
            .expect("original confirmed staged publication");

        // The already-shipping append_event path must agree exactly with
        // the original prepared delta: there is no extra event, increment or
        // post-confirmation side effect when the same record is installed.
        let mut ordinary = World::new();
        ordinary.append_event(body, None).unwrap();
        assert_eq!(staged.snapshot(), ordinary.snapshot());
        assert_eq!(staged.journal(), ordinary.journal());

        // Replay starts at the independently retained prior snapshot, not a
        // clone of the new T state. This is an ordinary journal consumer, NOT
        // an accepted IWS GEN/Frozen/Hot or separate-process Cold receipt.
        let recovered = World::from_snapshot(before, staged.journal().clone())
            .expect("original nonempty industrial event cold replay");
        assert_eq!(recovered.snapshot(), staged.snapshot());
    }

    #[test]
    fn industrial_delayed_prepare_still_rejects_unknown_and_stale_head() {
        let mut world = World::new();
        let body = WorldEventBody::Domain(DomainEvent::LocationAnchorUpdated {
            anchor: LocationAnchorV1 {
                location_id: "staged-industrial-delayed".into(),
                active: true,
                authority_revision: 1,
                effective_at: 0,
            },
        });
        let transition = StagedIndustrialPublication::from_body(&body).unwrap();
        let prepare = |world: &World| {
            let view = BorrowedWorldView::new(world).unwrap();
            transition
                .prepare(&view, &TransitionContext::new(view.time(), None))
                .unwrap()
        };
        let before = world.snapshot();
        let unknown = prepare(&world).install_confirmed(
            &mut world,
            DurableCommitConfirmation::StatusUnknown {
                reason: "lost publication acknowledgment".into(),
            },
        );
        assert!(matches!(
            unknown,
            Err(TransitionReject::StatusUnknownFenced { .. })
        ));
        assert_eq!(world.snapshot(), before);

        let pending = prepare(&world);
        world
            .set_location_anchor(LocationAnchorV1 {
                location_id: "other-staged-industrial-location".into(),
                active: true,
                authority_revision: 1,
                effective_at: 0,
            })
            .unwrap();
        let changed = world.snapshot();
        assert!(matches!(
            pending.install_confirmed(&mut world, DurableCommitConfirmation::Confirmed),
            Err(TransitionReject::BaseHeadStale)
        ));
        assert_eq!(world.snapshot(), changed);
    }
}
