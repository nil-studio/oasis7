use super::*;

pub(in crate::world::event_processing) enum PreparedEventStateDelta {
    NoState(WorldEventBody),
    ProductValidationDeliveryCursorUpdated {
        event: crate::ProductValidationDeliveryCursor,
        next: crate::ProductValidationDeliveryCursor,
    },
    ModuleMarketplace(
        super::super::super::super::state::module_marketplace_transition::PreparedModuleMarketplace,
    ),
    ModuleRelease(super::super::super::super::state::module_release_transition::PreparedModuleRelease),
    ModuleInstance {
        prepared: super::super::super::super::state::module_instance_transition::PreparedModuleInstance,
        schedule: Option<(String, Option<WorldTime>)>,
    },
    ModuleStateUpdated {
        module_states: BTreeMap<String, Vec<u8>>,
    },
    ModuleVisualEntities {
        event: oasis7_wasm_abi::ModuleEmitEvent,
        next: BTreeMap<String, oasis7_agent_api::world_entity::ModuleVisualEntity>,
    },
    ModuleRuntimeCharged(super::super::super::module_runtime_metering::PreparedModuleRuntimeCharge),
    EffectQueued {
        intent_id: String,
        pending_effects: VecDeque<EffectIntent>,
        pending_effects_evicted: u64,
    },
    ReceiptAppended {
        intent_id: String,
        pending_effects: VecDeque<EffectIntent>,
        inflight_effects: BTreeMap<String, EffectIntent>,
    },
    ModuleEvent {
        event: ModuleEvent,
        module_registry: ModuleRegistry,
        module_artifacts: BTreeSet<String>,
        module_tick_schedule: BTreeMap<String, WorldTime>,
        cache_invalidations: BTreeSet<String>,
    },
    ManifestUpdated {
        update: ManifestUpdate,
        manifest: Manifest,
    },
    GovernanceRegistry(
        super::super::super::governance_registry_publication::PreparedGovernanceRegistryEvent,
    ),
    CapabilityAuthorization(
        Box<super::super::super::capability_authorization_publication::PreparedCapabilityAuthorizationEvent>,
    ),
    CapabilityCommandCommit(
        super::super::super::capability_authorization_command_projection::PreparedCapabilityCommandCommit,
    ),
    CapabilityEffectReceipt(
        super::super::super::capability_effect_receipt_projection::PreparedCapabilityEffectReceipt,
    ),
    AgentIntent(super::super::super::agent_intent_publication::PreparedAgentIntent),
    EconomyData(super::super::super::economy_data_publication::PreparedEconomyDataEvent),
    EconomicContract(super::super::super::economic_contract_publication::PreparedEconomicContractEvent),
    AllianceWar(super::super::super::alliance_war_publication::PreparedAllianceWarEvent),
    GovernanceMeta(super::super::super::governance_meta_publication::PreparedGovernanceMetaEvent),
    CorePolicy(Box<super::super::super::super::state::core_policy_transition::PreparedCorePolicyEvent>),
    Industry(super::super::super::super::state::industry_transition::PreparedIndustryEvent),
    IndustryHistory(super::super::super::super::state::industry_history_transition::PreparedIndustryHistoryEvent),
    PowerRedemption(super::super::super::power_redemption_publication::PreparedPowerRedemptionEvent),
    NodePointsSettlement(
        super::super::super::node_points_settlement_publication::PreparedNodePointsSettlement,
    ),
    MainTokenMonetary(
        super::super::super::main_token_monetary_publication::PreparedMainTokenMonetaryEvent,
    ),
    MainTokenGovernanceMonetary(
        super::super::super::main_token_governance_monetary_publication::PreparedMainTokenGovernanceMonetaryEvent,
    ),
    MainTokenRestrictedClaim(
        super::super::super::main_token_restricted_claim_publication::PreparedMainTokenRestrictedClaimEvent,
    ),
    StarterOcClaimed(PreparedStarterOcClaimed),
    AgentClaimLightLifecycle(PreparedAgentClaimLightLifecycle),
    AgentClaimEconomic(super::super::super::agent_claim_economic_publication::PreparedAgentClaimEconomic),
    AgentClaimTerminal(super::super::super::agent_claim_terminal_publication::PreparedAgentClaimTerminal),
    Body(PreparedBodyAttributesUpdate),
    DomainRouteOnly {
        event: DomainEvent,
        agent_id: String,
    },
    GovernanceEmergencyBrake {
        next_until_tick: Option<WorldTime>,
    },
    GovernanceFinalityEpochSnapshot {
        epoch_id: u64,
        next: Option<crate::GovernanceFinalityEpochSnapshot>,
    },
    GovernanceEmergencyVeto {
        proposal_id: ProposalId,
        next: crate::Proposal,
    },
    GovernanceProposal {
        proposal_id: ProposalId,
        next: crate::Proposal,
        next_proposal_id: ProposalId,
        next_proposal_id_era: u64,
    },
    GovernanceProposalShadow {
        proposal_id: ProposalId,
        next: crate::Proposal,
    },
    GovernanceProposalStatus {
        event: GovernanceEvent,
        proposal_id: ProposalId,
        next: crate::Proposal,
    },
    GovernanceIdentityPenaltyAppeal {
        penalty_id: u64,
        next: GovernanceIdentityPenaltyRecord,
    },
    GovernanceIdentityPenaltyApplication {
        penalty_id: u64,
        target_agent_id: String,
        next: GovernanceIdentityPenaltyRecord,
        next_profile: GovernanceIdentityProfileState,
        profile_was_present: bool,
        next_penalty_id: u64,
    },
    GovernanceIdentityPenaltyResolution {
        penalty_id: u64,
        target_agent_id: String,
        next: GovernanceIdentityPenaltyRecord,
        next_profile: GovernanceIdentityProfileState,
    },
}

impl PreparedEventStateDelta {
    pub(super) fn for_body(world: &World, body: &WorldEventBody) -> Option<Self> {
        match body {
            WorldEventBody::PolicyDecisionRecorded(_)
            | WorldEventBody::RuleDecisionRecorded(_)
            | WorldEventBody::ActionOverridden(_)
            | WorldEventBody::ModuleCallFailed(_)
            | WorldEventBody::ModuleEmitted(_)
            | WorldEventBody::SnapshotCreated(_)
            | WorldEventBody::RollbackApplied(_)
            | WorldEventBody::Domain(DomainEvent::ActionRejected { .. }) => {
                Some(Self::NoState(body.clone()))
            }
            WorldEventBody::ProductValidationDeliveryCursorUpdated(cursor) => {
                let mut next = world.state.product_validation_delivery_cursor.clone();
                next.advance_to(cursor.event_id_era, cursor.routed_through_event_id);
                Some(Self::ProductValidationDeliveryCursorUpdated {
                    event: cursor.clone(),
                    next,
                })
            }
            WorldEventBody::Governance(GovernanceEvent::EmergencyBrakeActivated {
                active_until_tick,
                ..
            }) => Some(Self::GovernanceEmergencyBrake {
                next_until_tick: Some(*active_until_tick),
            }),
            WorldEventBody::Governance(GovernanceEvent::EmergencyBrakeReleased { .. }) => {
                Some(Self::GovernanceEmergencyBrake {
                    next_until_tick: None,
                })
            }
            _ => None,
        }
    }

    pub(super) fn matches_body(&self, body: &WorldEventBody) -> bool {
        match self {
            Self::ModuleMarketplace(prepared) => {
                matches!(body, WorldEventBody::Domain(event) if prepared.matches_event(event))
            }
            Self::ModuleRelease(prepared) => {
                matches!(body, WorldEventBody::Domain(event) if prepared.matches_event(event))
            }
            Self::ModuleInstance { prepared, .. } => {
                matches!(body, WorldEventBody::Domain(event) if prepared.matches_event(event))
            }
            Self::ModuleStateUpdated { module_states } => matches!(body,
                WorldEventBody::ModuleStateUpdated(update)
                if module_states.len() == 1 && module_states.get(&update.module_id) == Some(&update.state)),
            Self::ModuleVisualEntities { event, .. } => matches!(
                body,
                WorldEventBody::ModuleEmitted(body_event) if body_event == event
            ),
            Self::ModuleRuntimeCharged(prepared) => matches!(body,
                WorldEventBody::ModuleRuntimeCharged(charge)
                if prepared.agents.contains_key(&charge.payer_agent_id)),
            Self::EffectQueued { intent_id, .. } => matches!(
                body,
                WorldEventBody::EffectQueued(intent) if &intent.identity_key() == intent_id
            ),
            Self::ReceiptAppended { intent_id, .. } => matches!(
                body,
                WorldEventBody::ReceiptAppended(receipt) if &receipt.identity_key() == intent_id
            ),
            Self::ModuleEvent { event, .. } => {
                matches!(body, WorldEventBody::ModuleEvent(body_event) if body_event == event)
            }
            Self::ManifestUpdated { update, .. } => {
                matches!(body, WorldEventBody::ManifestUpdated(body_update) if body_update == update)
            }
            Self::GovernanceRegistry(prepared) => {
                matches!(body, WorldEventBody::Governance(event) if prepared.matches_event(event))
            }
            Self::CapabilityAuthorization(prepared) => matches!(
                body,
                WorldEventBody::CapabilityAuthorization(event) if prepared.matches_event(event)
            ),
            Self::CapabilityCommandCommit(prepared) => matches!(
                body,
                WorldEventBody::CapabilityAuthorization(event) if prepared.matches_event(event)
            ),
            Self::CapabilityEffectReceipt(prepared) => matches!(
                body,
                WorldEventBody::CapabilityAuthorization(event) if prepared.matches_event(event)
            ),
            Self::AgentIntent(prepared) => {
                matches!(body, WorldEventBody::Domain(event) if prepared.matches_event(event))
            }
            Self::EconomyData(prepared) => {
                matches!(body, WorldEventBody::Domain(event) if prepared.matches_event(event))
            }
            Self::EconomicContract(prepared) => {
                matches!(body, WorldEventBody::Domain(event) if prepared.matches_event(event))
            }
            Self::AllianceWar(prepared) => {
                matches!(body, WorldEventBody::Domain(event) if prepared.matches_event(event))
            }
            Self::GovernanceMeta(prepared) => {
                matches!(body, WorldEventBody::Domain(event) if prepared.matches_event(event))
            }
            Self::CorePolicy(prepared) => {
                matches!(body, WorldEventBody::Domain(event) if prepared.matches_event(event))
            }
            Self::Industry(prepared) => {
                matches!(body, WorldEventBody::Domain(event) if prepared.matches(event))
            }
            Self::IndustryHistory(prepared) => {
                matches!(body, WorldEventBody::Domain(event) if prepared.matches(event))
            }
            Self::PowerRedemption(prepared) => {
                matches!(body, WorldEventBody::Domain(event) if prepared.matches_event(event))
            }
            Self::NodePointsSettlement(prepared) => {
                matches!(body, WorldEventBody::Domain(event) if prepared.matches_event(event))
            }
            Self::MainTokenMonetary(prepared) => {
                matches!(body, WorldEventBody::Domain(event) if prepared.matches_event(event))
            }
            Self::MainTokenGovernanceMonetary(prepared) => {
                matches!(body, WorldEventBody::Domain(event) if prepared.matches_event(event))
            }
            Self::MainTokenRestrictedClaim(prepared) => {
                matches!(body, WorldEventBody::Domain(event) if prepared.matches_event(event))
            }
            Self::StarterOcClaimed(prepared) => {
                matches!(body, WorldEventBody::Domain(event) if prepared.matches_event(event))
            }
            Self::AgentClaimLightLifecycle(prepared) => {
                matches!(body, WorldEventBody::Domain(event) if prepared.matches_event(event))
            }
            Self::AgentClaimEconomic(prepared) => {
                matches!(body, WorldEventBody::Domain(event) if prepared.matches_event(event))
            }
            Self::AgentClaimTerminal(prepared) => {
                matches!(body, WorldEventBody::Domain(event) if prepared.matches_event(event))
            }
            Self::NoState(prepared_body) => prepared_body == body,
            Self::ProductValidationDeliveryCursorUpdated { event, .. } => {
                matches!(body, WorldEventBody::ProductValidationDeliveryCursorUpdated(cursor) if cursor == event)
            }
            Self::Body(prepared) => {
                matches!(body, WorldEventBody::Domain(event) if prepared.matches_event(event))
            }
            Self::DomainRouteOnly { event, .. } => {
                matches!(body, WorldEventBody::Domain(body_event) if body_event == event)
            }
            Self::GovernanceEmergencyBrake { next_until_tick } => match body {
                WorldEventBody::Governance(GovernanceEvent::EmergencyBrakeActivated {
                    active_until_tick,
                    ..
                }) => next_until_tick == &Some(*active_until_tick),
                WorldEventBody::Governance(GovernanceEvent::EmergencyBrakeReleased { .. }) => {
                    next_until_tick.is_none()
                }
                _ => false,
            },
            Self::GovernanceFinalityEpochSnapshot { epoch_id, next } => match body {
                WorldEventBody::Governance(GovernanceEvent::FinalityEpochSnapshotSet {
                    snapshot,
                    ..
                }) => *epoch_id == snapshot.epoch_id && next.as_ref() == Some(snapshot),
                WorldEventBody::Governance(GovernanceEvent::FinalityEpochSnapshotRemoved {
                    epoch_id: event_epoch_id,
                    ..
                }) => *epoch_id == *event_epoch_id && next.is_none(),
                _ => false,
            },
            Self::GovernanceEmergencyVeto { proposal_id, .. } => matches!(
                body,
                WorldEventBody::Governance(GovernanceEvent::EmergencyVetoed {
                    proposal_id: event_proposal_id,
                    ..
                }) if proposal_id == event_proposal_id
            ),
            Self::GovernanceProposal { proposal_id, .. } => matches!(
                body,
                WorldEventBody::Governance(GovernanceEvent::Proposed {
                    proposal_id: event_proposal_id,
                    ..
                }) if proposal_id == event_proposal_id
            ),
            Self::GovernanceProposalShadow { proposal_id, .. } => matches!(
                body,
                WorldEventBody::Governance(GovernanceEvent::ShadowReport {
                    proposal_id: event_proposal_id,
                    ..
                }) if proposal_id == event_proposal_id
            ),
            Self::GovernanceProposalStatus { event, .. } => {
                matches!(body, WorldEventBody::Governance(body_event) if body_event == event)
            }
            Self::GovernanceIdentityPenaltyAppeal { penalty_id, .. } => matches!(
                body,
                WorldEventBody::Governance(GovernanceEvent::IdentityPenaltyAppealed {
                    penalty_id: event_penalty_id,
                    ..
                }) if penalty_id == event_penalty_id
            ),
            Self::GovernanceIdentityPenaltyApplication { penalty_id, .. } => matches!(
                body,
                WorldEventBody::Governance(GovernanceEvent::IdentityPenaltyApplied {
                    penalty_id: event_penalty_id,
                    ..
                }) if penalty_id == event_penalty_id
            ),
            Self::GovernanceIdentityPenaltyResolution { penalty_id, .. } => matches!(
                body,
                WorldEventBody::Governance(GovernanceEvent::IdentityPenaltyResolved {
                    penalty_id: event_penalty_id,
                    ..
                }) if penalty_id == event_penalty_id
            ),
        }
    }

    pub(super) fn state_overlay(
        &self,
        event: DomainEvent,
    ) -> super::super::super::super::BodyOverlay {
        match self {
            Self::ModuleMarketplace(_) => unreachable!("marketplace uses a sparse overlay"),
            Self::ModuleRelease(_) => unreachable!("release uses a sparse release overlay"),
            Self::ModuleInstance { .. }
            | Self::ModuleStateUpdated { .. }
            | Self::ModuleVisualEntities { .. }
            | Self::ModuleRuntimeCharged(_) => {
                unreachable!("module output uses a command overlay")
            }
            Self::EffectQueued { .. } | Self::ReceiptAppended { .. } => {
                unreachable!("effect sidecars do not have a state overlay")
            }
            Self::ModuleEvent { .. } | Self::ManifestUpdated { .. } => {
                unreachable!("module metadata does not have a body state overlay")
            }
            Self::GovernanceRegistry(_) => {
                unreachable!("governance registry uses a state projection")
            }
            Self::CapabilityAuthorization(_) => {
                unreachable!("capability authorization uses sidecar state")
            }
            Self::CapabilityCommandCommit(_) => {
                unreachable!("capability command commit uses sidecar state")
            }
            Self::CapabilityEffectReceipt(_) => {
                unreachable!("capability effect receipt uses sidecar state")
            }
            Self::AgentIntent(_) => unreachable!("agent intent uses a sparse state projection"),
            Self::EconomyData(_) => {
                unreachable!("economy/data events use a sparse state projection")
            }
            Self::EconomicContract(_) => {
                unreachable!("economic contract uses a sparse state projection")
            }
            Self::AllianceWar(_) => unreachable!("alliance/war uses a sparse state projection"),
            Self::GovernanceMeta(_) => {
                unreachable!("governance/meta uses a sparse state projection")
            }
            Self::CorePolicy(_) => unreachable!("core/policy uses a sparse state projection"),
            Self::Industry(_) => unreachable!("industry uses a sparse state projection"),
            Self::IndustryHistory(_) => {
                unreachable!("industry history uses a sparse state projection")
            }
            Self::PowerRedemption(_) => {
                unreachable!("power redemption uses a sparse state projection")
            }
            Self::NodePointsSettlement(_) => {
                unreachable!("node points settlement uses a sparse state projection")
            }
            Self::MainTokenMonetary(_) => {
                unreachable!("main-token monetary events use a sparse state projection")
            }
            Self::MainTokenGovernanceMonetary(_) => {
                unreachable!("main-token governance monetary events use a sparse state projection")
            }
            Self::MainTokenRestrictedClaim(_) => {
                unreachable!("restricted-claim events use a sparse state projection")
            }
            Self::StarterOcClaimed(_) => {
                unreachable!("starter OC claims use a sparse state projection")
            }
            Self::AgentClaimLightLifecycle(_) => {
                unreachable!("light claim lifecycle uses a sparse state projection")
            }
            Self::AgentClaimEconomic(_) => unreachable!("claim economic uses sparse projection"),
            Self::AgentClaimTerminal(_) => unreachable!("claim terminal uses sparse projection"),
            Self::NoState(_) => unreachable!("NoState does not have a state overlay"),
            Self::ProductValidationDeliveryCursorUpdated { .. } => {
                unreachable!("delivery cursor uses a sparse state projection")
            }
            Self::Body(prepared) => prepared.body_overlay().with_routed_domain_event(event),
            Self::DomainRouteOnly { agent_id, .. } => {
                { super::super::super::super::BodyOverlay::route_only(agent_id.clone()) }
                    .with_routed_domain_event(event)
            }
            Self::GovernanceEmergencyBrake { .. } => {
                unreachable!("governance emergency brake does not have a state overlay")
            }
            Self::GovernanceFinalityEpochSnapshot { .. } => {
                unreachable!("governance finality snapshot does not have a state overlay")
            }
            Self::GovernanceEmergencyVeto { .. } => {
                unreachable!("governance emergency veto does not have a state overlay")
            }
            Self::GovernanceProposal { .. } => {
                unreachable!("governance proposal does not have a state overlay")
            }
            Self::GovernanceProposalShadow { .. } => {
                unreachable!("governance proposal shadow does not have a state overlay")
            }
            Self::GovernanceProposalStatus { .. } => {
                unreachable!("governance proposal status does not have a state overlay")
            }
            Self::GovernanceIdentityPenaltyAppeal { .. } => {
                unreachable!("identity penalty appeal does not have a state overlay")
            }
            Self::GovernanceIdentityPenaltyApplication { .. } => {
                unreachable!("identity penalty application does not have a state overlay")
            }
            Self::GovernanceIdentityPenaltyResolution { .. } => {
                unreachable!("identity penalty resolution does not have a state overlay")
            }
        }
    }

    pub(super) fn install_infallible(self, world: &mut World) {
        match self {
            Self::ModuleMarketplace(prepared) => prepared.install_infallible(&mut world.state),
            Self::ModuleRelease(prepared) => prepared.install_infallible(&mut world.state),
            Self::ModuleInstance { prepared, schedule } => {
                prepared.install_infallible(&mut world.state);
                world.install_prepared_module_instance_schedule(schedule);
            }
            Self::ModuleStateUpdated { module_states } => {
                world.state.module_states.extend(module_states)
            }
            Self::ModuleVisualEntities { next, .. } => {
                world.state.module_visual_entities = next;
            }
            Self::ModuleRuntimeCharged(prepared) => prepared.install_infallible(world),
            Self::EffectQueued {
                pending_effects,
                pending_effects_evicted,
                ..
            } => {
                world.pending_effects = pending_effects;
                world.runtime_backpressure_stats.pending_effects_evicted = world
                    .runtime_backpressure_stats
                    .pending_effects_evicted
                    .saturating_add(pending_effects_evicted);
            }
            Self::ReceiptAppended {
                pending_effects,
                inflight_effects,
                ..
            } => {
                world.pending_effects = pending_effects;
                world.inflight_effects = inflight_effects;
            }
            Self::ModuleEvent {
                module_registry,
                module_artifacts,
                module_tick_schedule,
                cache_invalidations,
                ..
            } => {
                world.module_registry = module_registry;
                world.module_artifacts = module_artifacts;
                world.module_tick_schedule = module_tick_schedule;
                for record_key in cache_invalidations {
                    let prefix = format!("{record_key}|");
                    world
                        .prepared_subscription_cache
                        .retain(|key, _| !key.starts_with(prefix.as_str()));
                }
            }
            Self::ManifestUpdated { manifest, .. } => world.manifest = manifest,
            Self::GovernanceRegistry(prepared) => prepared.install(world),
            Self::CapabilityAuthorization(prepared) => (*prepared).install(world),
            Self::CapabilityCommandCommit(prepared) => prepared.install(world),
            Self::CapabilityEffectReceipt(prepared) => prepared.install(world),
            Self::AgentIntent(prepared) => prepared.install_infallible(&mut world.state),
            Self::EconomyData(prepared) => prepared.install_infallible(&mut world.state),
            Self::EconomicContract(prepared) => prepared.install_infallible(&mut world.state),
            Self::AllianceWar(prepared) => prepared.install_infallible(&mut world.state),
            Self::GovernanceMeta(prepared) => prepared.install_infallible(&mut world.state),
            Self::CorePolicy(prepared) => (*prepared).install_infallible(&mut world.state),
            Self::Industry(prepared) => prepared.install(&mut world.state),
            Self::IndustryHistory(prepared) => prepared.install(&mut world.state),
            Self::PowerRedemption(prepared) => prepared.install_infallible(&mut world.state),
            Self::NodePointsSettlement(prepared) => prepared.install_infallible(&mut world.state),
            Self::MainTokenMonetary(prepared) => prepared.install_infallible(&mut world.state),
            Self::MainTokenGovernanceMonetary(prepared) => {
                prepared.install_infallible(&mut world.state)
            }
            Self::MainTokenRestrictedClaim(prepared) => {
                prepared.install_infallible(&mut world.state)
            }
            Self::StarterOcClaimed(prepared) => prepared.install_infallible(&mut world.state),
            Self::AgentClaimLightLifecycle(prepared) => {
                prepared.install_infallible(&mut world.state)
            }
            Self::AgentClaimEconomic(prepared) => prepared.install_infallible(&mut world.state),
            Self::AgentClaimTerminal(prepared) => prepared.install_infallible(&mut world.state),
            Self::Body(prepared) => prepared.install_infallible(world),
            Self::GovernanceEmergencyBrake { next_until_tick } => {
                let next_until_tick = next_until_tick.map(|next| {
                    world
                        .governance_emergency_brake_until_tick
                        .map_or(next, |current| current.max(next))
                });
                world.governance_emergency_brake_until_tick = next_until_tick;
            }
            Self::GovernanceFinalityEpochSnapshot { epoch_id, next } => match next {
                Some(snapshot) => {
                    world
                        .governance_finality_epoch_snapshots
                        .insert(epoch_id, snapshot);
                }
                None => {
                    world.governance_finality_epoch_snapshots.remove(&epoch_id);
                }
            },
            Self::GovernanceEmergencyVeto { proposal_id, next } => {
                world.proposals.insert(proposal_id, next);
            }
            Self::GovernanceProposal {
                proposal_id,
                next,
                next_proposal_id,
                next_proposal_id_era,
            } => {
                world.proposals.insert(proposal_id, next);
                world.next_proposal_id = next_proposal_id;
                world.next_proposal_id_era = next_proposal_id_era;
            }
            Self::GovernanceProposalShadow { proposal_id, next } => {
                world.proposals.insert(proposal_id, next);
            }
            Self::GovernanceProposalStatus {
                proposal_id, next, ..
            } => {
                world.proposals.insert(proposal_id, next);
            }
            Self::GovernanceIdentityPenaltyAppeal { penalty_id, next } => {
                world.governance_identity_penalties.insert(penalty_id, next);
            }
            Self::GovernanceIdentityPenaltyApplication {
                penalty_id,
                target_agent_id,
                next,
                next_profile,
                next_penalty_id,
                ..
            } => {
                world.governance_identity_penalties.insert(penalty_id, next);
                world
                    .state
                    .governance_identity_profiles
                    .insert(target_agent_id, next_profile);
                world.next_governance_identity_penalty_id = next_penalty_id;
            }
            Self::GovernanceIdentityPenaltyResolution {
                penalty_id,
                target_agent_id,
                next,
                next_profile,
            } => {
                world.governance_identity_penalties.insert(penalty_id, next);
                world
                    .state
                    .governance_identity_profiles
                    .insert(target_agent_id, next_profile);
            }
            Self::ProductValidationDeliveryCursorUpdated { next, .. } => {
                world.state.product_validation_delivery_cursor = next;
            }
            Self::NoState(_) | Self::DomainRouteOnly { .. } => {}
        }
    }
}
