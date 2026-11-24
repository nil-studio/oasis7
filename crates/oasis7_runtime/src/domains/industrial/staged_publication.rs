//! Industrial event publication through the ordinary root staging boundary.
//!
//! Reuse the original sparse WorldState reducers and shared consensus/journal
//! preparation. This is an ordinary synchronous publication boundary, not an
//! IWS durable-acceptance receipt or the complete five-domain tick transaction.

use super::{PreparedEventPublication, PreparedEventStateDelta};
use crate::state::industry_history_transition::PreparedIndustryHistoryEvent;
use crate::state::industry_transition::PreparedIndustryEvent;
use crate::world::transition::{
    BorrowedWorldView, DurableCommitConfirmation, PreparedStagedCommit, RootInstallContext,
    StagedTransition, SubsystemDelta, TransitionContext, TransitionReject,
};
use crate::{CausedBy, DomainEvent, World, WorldError, WorldEvent, WorldEventBody, WorldEventId};

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
        let (body, state_delta) = match self {
            Self::State(event) => (
                WorldEventBody::Domain((*event).clone()),
                PreparedEventStateDelta::Industry(PreparedIndustryEvent::prepare(
                    view.state(), event, view.time(),
                )?),
            ),
            Self::History(event) => (
                WorldEventBody::Domain((*event).clone()),
                PreparedEventStateDelta::IndustryHistory(PreparedIndustryHistoryEvent::prepare(
                    view.state(), event, view.time(),
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
                PreparedEventStateDelta::for_body(view.world(), body).ok_or_else(|| {
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
        } = view.world().prepare_event_publication(
            body,
            ctx.caused_by.clone(),
            state_delta,
        )?;
        Ok(PreparedStagedCommit::new(
            view.base_head().clone(),
            Some(consensus_record),
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
        let prepared = {
            let view = BorrowedWorldView::new(self)?;
            transition
                .prepare(&view, &TransitionContext::new(view.time(), caused_by))
                .map_err(TransitionReject::into_world_error)?
        };
        let event_id = prepared.delta().event.id;
        if self.take_fail_next_append_after_publication_prepare_for_test() {
            return Err(WorldError::ResourceBalanceInvalid {
                reason: "injected append_event failure after publication preparation".into(),
            });
        }
        // Same synchronous ordinary publication boundary used by the module
        // marketplace owner; no intervening canonical writes or IWS receipt.
        prepared
            .install_confirmed_unchecked_head(self, DurableCommitConfirmation::Confirmed)
            .map_err(TransitionReject::into_world_error)?;
        Ok(event_id)
    }
}
