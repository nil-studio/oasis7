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

pub(in crate::world) enum StagedIndustrialPublication<'a> {
    State(&'a DomainEvent),
    History(&'a DomainEvent),
    RouteOnly(&'a DomainEvent, &'a str),
    DeliveryCursor(&'a WorldEventBody),
}

impl<'a> StagedIndustrialPublication<'a> {
    pub(in crate::world) fn from_body(body: &'a WorldEventBody) -> Option<Self> {
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

pub(in crate::world) struct IndustrialPublicationDelta {
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

#[derive(Default)]
pub(in crate::world) struct IndustrialBatchPublicationDelta {
    steps: Vec<(WorldEvent, PreparedEventStateDelta)>,
    next_event_id: WorldEventId,
    next_event_id_era: u64,
    journal_events: Vec<WorldEvent>,
    journal_events_evicted: u64,
}

impl SubsystemDelta for IndustrialBatchPublicationDelta {
    fn apply_infallible(self, ctx: &mut RootInstallContext<'_>) {
        if self.steps.is_empty() {
            return;
        }
        for (event, state_delta) in self.steps {
            state_delta.install_infallible(ctx.world_mut());
            ctx.state_mut().time = event.time;
            if let WorldEventBody::Domain(domain_event) = &event.body {
                ctx.state_mut().route_domain_event(domain_event);
            }
        }
        ctx.install_journal_tail(
            self.next_event_id,
            self.next_event_id_era,
            self.journal_events,
            self.journal_events_evicted,
        );
    }
}

impl StagedIndustrialPublication<'_> {
    fn prepare_state_delta(
        &self,
        world: &World,
        state: &crate::WorldState,
    ) -> Result<(WorldEventBody, PreparedEventStateDelta), TransitionReject> {
        let (body, delta) = match self {
            Self::State(event) => (
                WorldEventBody::Domain((*event).clone()),
                PreparedEventStateDelta::Industry(PreparedIndustryEvent::prepare(
                    state, event, state.time,
                )?),
            ),
            Self::History(event) => (
                WorldEventBody::Domain((*event).clone()),
                PreparedEventStateDelta::IndustryHistory(PreparedIndustryHistoryEvent::prepare(
                    state, event, state.time,
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
        Ok((body, delta))
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
        let (body, state_delta) = self.prepare_state_delta(world, &world.state)?;
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

    #[cfg_attr(not(test), expect(dead_code, reason = "pending #6140 PR 6/6 industrial write cutover"))]
    pub(in crate::world) fn prepare_industrial_action_rejected_publication(
        &self,
        body: WorldEventBody,
        caused_by: Option<CausedBy>,
    ) -> Result<(TickConsensusRecord, IndustrialPublicationDelta), WorldError> {
        if !matches!(body, WorldEventBody::Domain(DomainEvent::ActionRejected { .. })) {
            return Err(WorldError::ResourceBalanceInvalid { reason: "expected ActionRejected event body".into() });
        }
        let state_delta = PreparedEventStateDelta::NoState(body.clone());
        let p = self.prepare_event_publication(body, caused_by, state_delta)?;
        Ok((
            p.consensus_record,
            IndustrialPublicationDelta {
                event: p.event, state_delta: p.state_delta, next_event_id: p.next_event_id,
                next_event_id_era: p.next_event_id_era, journal_events: p.journal_events,
                journal_events_evicted: p.journal_events_evicted,
            },
        ))
    }

    #[cfg_attr(not(test), expect(dead_code, reason = "pending #6140 PR 6/6 industrial write cutover"))]
    pub(in crate::world) fn prepare_industrial_batch_publication(
        &self,
        bodies: &[WorldEventBody],
        caused_by: Option<CausedBy>,
    ) -> Result<(Option<TickConsensusRecord>, IndustrialBatchPublicationDelta), WorldError> {
        if bodies.is_empty() {
            return Ok((None, IndustrialBatchPublicationDelta::default()));
        }
        if bodies.len() == 1 {
            let transition = StagedIndustrialPublication::from_body(&bodies[0]).ok_or_else(|| {
                WorldError::ResourceBalanceInvalid { reason: format!("unclassified industrial event: {:?}", bodies[0]) }
            })?;
            let ctx = TransitionContext::new(self.state.time, caused_by);
            let (_, consensus_record, single) = transition.prepare_event(self, &ctx).map_err(TransitionReject::into_world_error)?;
            return Ok((
                Some(consensus_record),
                IndustrialBatchPublicationDelta {
                    steps: vec![(single.event, single.state_delta)],
                    next_event_id: single.next_event_id, next_event_id_era: single.next_event_id_era,
                    journal_events: single.journal_events, journal_events_evicted: single.journal_events_evicted,
                },
            ));
        }

        let mut working_state = self.state.clone();
        let (mut next_event_id, mut next_event_id_era) = (self.next_event_id, self.next_event_id_era);
        let mut journal_events = self.journal.events.clone();
        let max_len = self.runtime_memory_limits.max_journal_events.max(1);
        let mut journal_events_evicted = 0_u64;
        let mut steps = Vec::with_capacity(bodies.len());
        let last_event_time = working_state.time;

        for (idx, body) in bodies.iter().enumerate() {
            let transition = StagedIndustrialPublication::from_body(body).ok_or_else(|| {
                WorldError::ResourceBalanceInvalid { reason: format!("unclassified industrial event: {body:?}") }
            })?;
            let (_, state_delta) = transition.prepare_state_delta(self, &working_state).map_err(TransitionReject::into_world_error)?;
            let (event_id, next_id, next_era) = Self::preview_next_event_id(next_event_id, next_event_id_era);
            if let WorldEventBody::Domain(domain_event) = body {
                self.validate_agent_intent_receipt_reference(domain_event, Some(event_id))?;
            }
            let event = WorldEvent { id: event_id, time: working_state.time, caused_by: caused_by.clone(), body: body.clone() };
            (next_event_id, next_event_id_era) = (next_id, next_era);

            journal_events.push(event.clone());
            let overflow = journal_events.len().saturating_sub(max_len);
            if overflow > 0 {
                journal_events.drain(0..overflow);
                journal_events_evicted = journal_events_evicted.saturating_add(overflow as u64);
            }
            if idx + 1 < bodies.len() {
                let (_, working_delta) = transition.prepare_state_delta(self, &working_state).map_err(TransitionReject::into_world_error)?;
                match working_delta {
                    PreparedEventStateDelta::Industry(p) => p.install(&mut working_state),
                    PreparedEventStateDelta::IndustryHistory(p) => p.install(&mut working_state),
                    _ => {}
                }
                if let WorldEventBody::Domain(domain_event) = &event.body {
                    working_state.route_domain_event(domain_event);
                }
            }
            steps.push((event, state_delta));
        }

        let tick_events: Vec<WorldEvent> = journal_events.iter().filter(|e| e.time == last_event_time).cloned().collect();
        let state_root = match &steps.last().expect("nonempty").1 {
            PreparedEventStateDelta::Industry(p) => self.state_root_hash_for_state_with_industry_overlay(&working_state, p)?,
            _ => self.state_root_hash_for_state(&working_state)?,
        };
        let consensus_record = self.build_tick_consensus_record_for_prepared_events(last_event_time, &tick_events, state_root.clone())?;
        self.validate_tick_consensus_candidate_for_prepared_publication(&consensus_record, &tick_events, &state_root)?;

        Ok((
            Some(consensus_record),
            IndustrialBatchPublicationDelta {
                steps, next_event_id, next_event_id_era, journal_events, journal_events_evicted,
            },
        ))
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
