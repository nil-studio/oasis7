//! Sync monetary DomainEvent and ModuleRuntimeCharged publication in the shared staged boundary.
//! Compatible adapter for #6143 PR 3/4; active-path cutover in `publication.rs` lands in PR 4/4.

use super::{PreparedEventPublication, PreparedEventStateDelta};
use crate::domains::monetary::MonetaryDomainDelta;
use crate::world::{
    agent_claim_economic_publication::PreparedAgentClaimEconomic,
    agent_claim_light_lifecycle_publication::PreparedAgentClaimLightLifecycle,
    agent_claim_terminal_publication::PreparedAgentClaimTerminal,
    economic_contract_publication::PreparedEconomicContractEvent,
    economy_data_publication::PreparedEconomyDataEvent,
    main_token_governance_monetary_publication::PreparedMainTokenGovernanceMonetaryEvent,
    main_token_monetary_publication::PreparedMainTokenMonetaryEvent,
    main_token_restricted_claim_publication::PreparedMainTokenRestrictedClaimEvent,
    node_points_settlement_publication::PreparedNodePointsSettlement,
    power_redemption_publication::PreparedPowerRedemptionEvent,
    starter_oc_claim_publication::PreparedStarterOcClaimed,
    transition::{
        BorrowedWorldView, PreparedStagedCommit, RootInstallContext, StagedRootTransaction,
        StagedTransition, SubsystemDelta, SynchronousEventTransition, TransitionContext,
        TransitionReject, publish_synchronous_event,
    },
};
use crate::{
    CausedBy, DomainEvent, ModuleRuntimeChargeEvent, TickConsensusRecord, World, WorldError,
    WorldEvent, WorldEventBody, WorldEventId,
};

pub(crate) enum StagedMonetaryPublication<'a> {
    EconomyData(&'a DomainEvent),
    PowerRedemption(&'a DomainEvent),
    NodePointsSettlement(&'a DomainEvent),
    MainTokenMonetary(&'a DomainEvent),
    MainTokenGovernanceMonetary(&'a DomainEvent),
    MainTokenRestrictedClaim(&'a DomainEvent),
    StarterOcClaimed(&'a DomainEvent),
    AgentClaimLightLifecycle(&'a DomainEvent),
    AgentClaimEconomic(&'a DomainEvent),
    AgentClaimTerminal(&'a DomainEvent),
    EconomicContract(&'a DomainEvent),
    ModuleRuntimeCharged(&'a ModuleRuntimeChargeEvent),
}

impl<'a> StagedMonetaryPublication<'a> {
    pub(crate) fn from_body(body: &'a WorldEventBody) -> Option<Self> {
        match body {
            WorldEventBody::ModuleRuntimeCharged(charge) => Some(Self::ModuleRuntimeCharged(charge)),
            WorldEventBody::Domain(event) => match event {
                DomainEvent::ResourceTransferred { .. }
                | DomainEvent::DataCollected { .. }
                | DomainEvent::DataCollectedAuthenticated { .. }
                | DomainEvent::DataAccessGranted { .. }
                | DomainEvent::DataAccessRevoked { .. } => Some(Self::EconomyData(event)),
                DomainEvent::PowerRedeemed { .. } | DomainEvent::PowerRedeemRejected { .. } => Some(Self::PowerRedemption(event)),
                DomainEvent::NodePointsSettlementApplied { .. } => Some(Self::NodePointsSettlement(event)),
                DomainEvent::MainTokenGenesisInitialized { .. }
                | DomainEvent::MainTokenVestingClaimed { .. }
                | DomainEvent::MainTokenTransferred { .. }
                | DomainEvent::MainTokenEpochIssued { .. }
                | DomainEvent::MainTokenFeeSettled { .. } => Some(Self::MainTokenMonetary(event)),
                DomainEvent::MainTokenPolicyUpdateScheduled { .. } | DomainEvent::MainTokenTreasuryDistributed { .. } => Some(Self::MainTokenGovernanceMonetary(event)),
                DomainEvent::RestrictedStarterClaimLiveopsPoolToppedUp { .. }
                | DomainEvent::RestrictedStarterClaimGrantIssued { .. }
                | DomainEvent::RestrictedStarterClaimGrantExpired { .. }
                | DomainEvent::RestrictedStarterClaimGrantRevoked { .. } => Some(Self::MainTokenRestrictedClaim(event)),
                DomainEvent::StarterOcClaimed { .. } => Some(Self::StarterOcClaimed(event)),
                DomainEvent::AgentClaimReleaseRequested { .. }
                | DomainEvent::AgentClaimEnteredGrace { .. }
                | DomainEvent::AgentClaimIdleWarning { .. } => Some(Self::AgentClaimLightLifecycle(event)),
                DomainEvent::AgentClaimed { .. } | DomainEvent::AgentClaimUpkeepSettled { .. } => Some(Self::AgentClaimEconomic(event)),
                DomainEvent::AgentClaimReleased { .. } | DomainEvent::AgentClaimReclaimed { .. } => Some(Self::AgentClaimTerminal(event)),
                DomainEvent::EconomicContractOpened { .. }
                | DomainEvent::EconomicContractAccepted { .. }
                | DomainEvent::EconomicContractSettled { .. }
                | DomainEvent::EconomicContractExpired { .. } => Some(Self::EconomicContract(event)),
                _ => None,
            },
            _ => None,
        }
    }

    #[cfg_attr(not(test), expect(dead_code, reason = "composed by #6150 root tick"))]
    pub(crate) fn stage_in_root(&self, root: &mut StagedRootTransaction<'_, MonetaryDomainDelta>) -> Result<WorldEventId, TransitionReject> {
        let (event_id, _consensus, delta) = self.prepare_event(root.view().world(), root.context())?;
        root.stage_delta(MonetaryDomainDelta::from_publication(delta));
        Ok(event_id)
    }
}

pub(crate) struct MonetaryPublicationDelta {
    event: WorldEvent,
    state_delta: PreparedEventStateDelta,
    next_event_id: WorldEventId,
    next_event_id_era: u64,
    journal_events: Vec<WorldEvent>,
    journal_events_evicted: u64,
}

impl SubsystemDelta for MonetaryPublicationDelta {
    fn apply_infallible(self, ctx: &mut RootInstallContext<'_>) {
        self.state_delta.install_infallible(ctx.world_mut());
        ctx.state_mut().time = self.event.time;
        ctx.install_journal_tail(self.next_event_id, self.next_event_id_era, self.journal_events, self.journal_events_evicted);
        if let WorldEventBody::Domain(event) = &self.event.body {
            ctx.state_mut().route_domain_event(event);
        }
    }
}

impl StagedTransition for StagedMonetaryPublication<'_> {
    type Delta = MonetaryPublicationDelta;

    fn prepare(&self, view: &BorrowedWorldView<'_>, ctx: &TransitionContext) -> Result<PreparedStagedCommit<Self::Delta>, TransitionReject> {
        let (_, consensus, delta) = self.prepare_event(view.world(), ctx)?;
        Ok(PreparedStagedCommit::new(view.base_head().clone(), Some(consensus), delta))
    }
}

impl SynchronousEventTransition for StagedMonetaryPublication<'_> {
    fn prepare_event(&self, world: &World, ctx: &TransitionContext) -> Result<(WorldEventId, TickConsensusRecord, Self::Delta), TransitionReject> {
        if let Self::EconomicContract(DomainEvent::EconomicContractSettled { world_binding: Some(event_binding), .. }) = self
            && world.world_rule_binding().as_deref() != Some(event_binding.as_str())
        {
            return Err(crate::world_rules::invalid("economic_contract_world_binding_mismatch").into());
        }
        let s = &world.state;
        let t = s.time;
        let (body, state_delta) = match self {
            Self::EconomyData(e) => (WorldEventBody::Domain((*e).clone()), PreparedEventStateDelta::EconomyData(PreparedEconomyDataEvent::prepare(s, e, t)?)),
            Self::PowerRedemption(e) => (WorldEventBody::Domain((*e).clone()), PreparedEventStateDelta::PowerRedemption(PreparedPowerRedemptionEvent::prepare(s, e, t)?)),
            Self::NodePointsSettlement(e) => (WorldEventBody::Domain((*e).clone()), PreparedEventStateDelta::NodePointsSettlement(PreparedNodePointsSettlement::prepare(s, e)?)),
            Self::MainTokenMonetary(e) => (WorldEventBody::Domain((*e).clone()), PreparedEventStateDelta::MainTokenMonetary(PreparedMainTokenMonetaryEvent::prepare(s, e, t)?)),
            Self::MainTokenGovernanceMonetary(e) => (WorldEventBody::Domain((*e).clone()), PreparedEventStateDelta::MainTokenGovernanceMonetary(PreparedMainTokenGovernanceMonetaryEvent::prepare(s, e, t)?)),
            Self::MainTokenRestrictedClaim(e) => (WorldEventBody::Domain((*e).clone()), PreparedEventStateDelta::MainTokenRestrictedClaim(PreparedMainTokenRestrictedClaimEvent::prepare(s, e)?)),
            Self::StarterOcClaimed(e) => (WorldEventBody::Domain((*e).clone()), PreparedEventStateDelta::StarterOcClaimed(PreparedStarterOcClaimed::prepare(s, e, t)?)),
            Self::AgentClaimLightLifecycle(e) => (WorldEventBody::Domain((*e).clone()), PreparedEventStateDelta::AgentClaimLightLifecycle(PreparedAgentClaimLightLifecycle::prepare(s, e, t)?)),
            Self::AgentClaimEconomic(e) => (WorldEventBody::Domain((*e).clone()), PreparedEventStateDelta::AgentClaimEconomic(Box::new(PreparedAgentClaimEconomic::prepare(s, e, t)?))),
            Self::AgentClaimTerminal(e) => (WorldEventBody::Domain((*e).clone()), PreparedEventStateDelta::AgentClaimTerminal(PreparedAgentClaimTerminal::prepare(s, e, t)?)),
            Self::EconomicContract(e) => (WorldEventBody::Domain((*e).clone()), PreparedEventStateDelta::EconomicContract(PreparedEconomicContractEvent::prepare(s, e, t)?)),
            Self::ModuleRuntimeCharged(c) => (WorldEventBody::ModuleRuntimeCharged((*c).clone()), PreparedEventStateDelta::ModuleRuntimeCharged(world.prepare_module_runtime_charge_event(c, t)?)),
        };
        let PreparedEventPublication { event, next_event_id, next_event_id_era, journal_events, journal_events_evicted, consensus_record, state_delta } =
            world.prepare_event_publication(body, ctx.caused_by.clone(), state_delta)?;
        Ok((event.id, consensus_record, MonetaryPublicationDelta { event, state_delta, next_event_id, next_event_id_era, journal_events, journal_events_evicted }))
    }
}

impl World {
    #[cfg_attr(not(test), expect(dead_code, reason = "cutover in #6143 PR 4/4"))]
    pub(in crate::world) fn append_staged_monetary_event(&mut self, transition: StagedMonetaryPublication<'_>, caused_by: Option<CausedBy>) -> Result<WorldEventId, WorldError> {
        publish_synchronous_event(self, &transition, &TransitionContext::new(self.state.time, caused_by))
    }
}
