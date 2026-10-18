use super::super::agent_claim_light_lifecycle_publication::PreparedAgentClaimLightLifecycle;
use super::super::starter_oc_claim_publication::PreparedStarterOcClaimed;
use super::*;
use crate::{
    CapabilityAuthorizationEvent, EffectIntent, EffectReceipt, Manifest, ManifestUpdate,
    ModuleEvent, ModuleRegistry,
};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

#[path = "prepared_state_delta.rs"]
mod prepared_state_delta;
pub(super) use prepared_state_delta::PreparedEventStateDelta;

struct PreparedEventPublication {
    event: WorldEvent,
    next_event_id: WorldEventId,
    next_event_id_era: u64,
    journal_events: Vec<WorldEvent>,
    journal_events_evicted: u64,
    consensus_record: TickConsensusRecord,
    state_delta: PreparedEventStateDelta,
}

impl World {
    pub(in crate::world) fn append_event(
        &mut self,
        body: WorldEventBody,
        caused_by: Option<CausedBy>,
    ) -> Result<WorldEventId, WorldError> {
        match &body {
            WorldEventBody::CapabilityAuthorization(
                CapabilityAuthorizationEvent::AuthorityInstalled { .. },
            ) => {
                return Err(super::super::capability_authorization::deny(
                    "record-only capability authority event has no replayable finality certificate",
                ));
            }
            WorldEventBody::CapabilityAuthorization(
                CapabilityAuthorizationEvent::AuthorityInstalledWithFinality { .. },
            ) => {
                return Err(super::super::capability_authorization::deny(
                    "certificate-only capability authority event has no binding proof",
                ));
            }
            _ => {}
        }
        let state_delta = match &body {
            WorldEventBody::Domain(
                event @ (DomainEvent::AgentIntentProposed { .. }
                | DomainEvent::AgentIntentSubmitted { .. }
                | DomainEvent::AgentIntentAccepted { .. }
                | DomainEvent::AgentIntentReplaced { .. }
                | DomainEvent::AgentIntentTransitioned { .. }),
            ) => Some(PreparedEventStateDelta::AgentIntent(
                self.prepare_raw_agent_intent_event(event)?,
            )),
            WorldEventBody::Domain(
                event @ (DomainEvent::ModuleArtifactDeployed { .. }
                | DomainEvent::ModuleArtifactListed { .. }
                | DomainEvent::ModuleArtifactDelisted { .. }
                | DomainEvent::ModuleArtifactDestroyed { .. }
                | DomainEvent::ModuleArtifactBidPlaced { .. }
                | DomainEvent::ModuleArtifactBidCancelled { .. }
                | DomainEvent::ModuleArtifactSaleCompleted { .. }),
            ) => Some(PreparedEventStateDelta::ModuleMarketplace(
                self.state
                    .prepare_module_marketplace_event(event, self.state.time)?,
            )),
            WorldEventBody::Domain(
                event @ (DomainEvent::ModuleReleaseRequested { .. }
                | DomainEvent::ModuleReleaseAttested { .. }
                | DomainEvent::ModuleReleaseRolesBound { .. }
                | DomainEvent::ModuleReleaseShadowed { .. }
                | DomainEvent::ModuleReleaseRoleApproved { .. }
                | DomainEvent::ModuleReleaseRejected { .. }
                | DomainEvent::ProductProfileGoverned { .. }
                | DomainEvent::RecipeProfileGoverned { .. }
                | DomainEvent::FactoryProfileGoverned { .. }
                | DomainEvent::ModuleReleaseApplied { .. }),
            ) => Some(PreparedEventStateDelta::ModuleRelease(
                self.state
                    .prepare_module_release_event(event, self.state.time)?,
            )),
            WorldEventBody::Domain(
                event @ (DomainEvent::ModuleInstalled { .. }
                | DomainEvent::ModuleUpgraded { .. }
                | DomainEvent::ModuleRollbackApplied { .. }),
            ) => {
                let prepared = self
                    .state
                    .prepare_module_instance_event_with_registry(event, self.state.time, Some(&self.module_registry))?;
                let schedule = self.prepare_module_instance_schedule(event, self.state.time)?;
                Some(PreparedEventStateDelta::ModuleInstance { prepared, schedule })
            }
            WorldEventBody::Domain(
                event @ (DomainEvent::ResourceTransferred { .. }
                | DomainEvent::DataCollected { .. }
                | DomainEvent::DataCollectedAuthenticated { .. }
                | DomainEvent::DataAccessGranted { .. }
                | DomainEvent::DataAccessRevoked { .. }),
            ) => Some(PreparedEventStateDelta::EconomyData(
                super::super::economy_data_publication::PreparedEconomyDataEvent::prepare(
                    &self.state,
                    event,
                    self.state.time,
                )?,
            )),
            WorldEventBody::Domain(
                event @ (DomainEvent::PowerRedeemed { .. }
                | DomainEvent::PowerRedeemRejected { .. }),
            ) => Some(PreparedEventStateDelta::PowerRedemption(
                super::super::power_redemption_publication::PreparedPowerRedemptionEvent::prepare(
                    &self.state,
                    event,
                    self.state.time,
                )?,
            )),
            WorldEventBody::Domain(event @ DomainEvent::NodePointsSettlementApplied { .. }) => {
                Some(PreparedEventStateDelta::NodePointsSettlement(
                    super::super::node_points_settlement_publication::PreparedNodePointsSettlement::prepare(
                        &self.state,
                        event,
                    )?,
                ))
            }
            WorldEventBody::Domain(event @ (DomainEvent::MainTokenGenesisInitialized { .. }
                | DomainEvent::MainTokenVestingClaimed { .. }
                | DomainEvent::MainTokenTransferred { .. }
                | DomainEvent::MainTokenEpochIssued { .. }
                | DomainEvent::MainTokenFeeSettled { .. })) => {
                Some(PreparedEventStateDelta::MainTokenMonetary(
                    super::super::main_token_monetary_publication::PreparedMainTokenMonetaryEvent::prepare(&self.state, event, self.state.time)?,
                ))
            }
            WorldEventBody::Domain(event @ (DomainEvent::MainTokenPolicyUpdateScheduled { .. }
                | DomainEvent::MainTokenTreasuryDistributed { .. })) => {
                Some(PreparedEventStateDelta::MainTokenGovernanceMonetary(
                    super::super::main_token_governance_monetary_publication::PreparedMainTokenGovernanceMonetaryEvent::prepare(&self.state, event, self.state.time)?,
                ))
            }
            WorldEventBody::Domain(event @ (DomainEvent::RestrictedStarterClaimLiveopsPoolToppedUp { .. }
                | DomainEvent::RestrictedStarterClaimGrantIssued { .. }
                | DomainEvent::RestrictedStarterClaimGrantExpired { .. }
                | DomainEvent::RestrictedStarterClaimGrantRevoked { .. })) => {
                Some(PreparedEventStateDelta::MainTokenRestrictedClaim(
                    super::super::main_token_restricted_claim_publication::PreparedMainTokenRestrictedClaimEvent::prepare(&self.state, event)?,
                ))
            }
            WorldEventBody::Domain(event @ DomainEvent::StarterOcClaimed { .. }) => {
                Some(PreparedEventStateDelta::StarterOcClaimed(
                    PreparedStarterOcClaimed::prepare(&self.state, event, self.state.time)?,
                ))
            }
            WorldEventBody::Domain(
                event @ (DomainEvent::AgentClaimReleaseRequested { .. }
                | DomainEvent::AgentClaimEnteredGrace { .. }
                | DomainEvent::AgentClaimIdleWarning { .. }),
            ) => Some(PreparedEventStateDelta::AgentClaimLightLifecycle(
                PreparedAgentClaimLightLifecycle::prepare(&self.state, event, self.state.time)?,
            )),
            WorldEventBody::Domain(event @ (DomainEvent::AgentLocationAuthorityUpdated { .. }
                | DomainEvent::LocationAnchorUpdated { .. }
                | DomainEvent::FactorySiteAuthorityUpdated { .. }
                | DomainEvent::FactoryConstructionPowerProfileUpdated { .. }
                | DomainEvent::ProductValidationRecorded { .. }
                | DomainEvent::ProductValidationAttemptStarted { .. })) => Some(PreparedEventStateDelta::IndustryHistory(
                    crate::state::industry_history_transition::PreparedIndustryHistoryEvent::prepare(&self.state, event, self.state.time)?,
                )),
            WorldEventBody::Domain(event @ (DomainEvent::AgentClaimed { .. } | DomainEvent::AgentClaimUpkeepSettled { .. })) => Some(PreparedEventStateDelta::AgentClaimEconomic(super::super::agent_claim_economic_publication::PreparedAgentClaimEconomic::prepare(&self.state,event,self.state.time)?)),
            WorldEventBody::Domain(event @ (DomainEvent::AgentClaimReleased { .. } | DomainEvent::AgentClaimReclaimed { .. })) => Some(PreparedEventStateDelta::AgentClaimTerminal(super::super::agent_claim_terminal_publication::PreparedAgentClaimTerminal::prepare(&self.state,event,self.state.time)?)),
            WorldEventBody::Domain(event @ (DomainEvent::EconomicContractOpened { .. } | DomainEvent::EconomicContractAccepted { .. } | DomainEvent::EconomicContractSettled { .. } | DomainEvent::EconomicContractExpired { .. })) => Some(PreparedEventStateDelta::EconomicContract(super::super::economic_contract_publication::PreparedEconomicContractEvent::prepare(&self.state,event,self.state.time)?)),
            WorldEventBody::Domain(event @ (DomainEvent::AllianceFormed { .. } | DomainEvent::AllianceJoined { .. } | DomainEvent::AllianceLeft { .. } | DomainEvent::AllianceDissolved { .. } | DomainEvent::WarDeclared { .. } | DomainEvent::WarConcluded { .. })) => Some(PreparedEventStateDelta::AllianceWar(super::super::alliance_war_publication::PreparedAllianceWarEvent::prepare(&self.state,event,self.state.time)?)),
            WorldEventBody::Domain(event @ (DomainEvent::GovernanceProposalOpened { .. } | DomainEvent::GovernanceVoteCast { .. } | DomainEvent::GovernanceProposalFinalized { .. } | DomainEvent::CrisisSpawned { .. } | DomainEvent::CrisisResolved { .. } | DomainEvent::CrisisTimedOut { .. } | DomainEvent::MetaProgressGranted { .. } | DomainEvent::ProductValidated { .. })) => Some(PreparedEventStateDelta::GovernanceMeta(super::super::governance_meta_publication::PreparedGovernanceMetaEvent::prepare(&self.state,event,self.state.time)?)),
            WorldEventBody::Domain(DomainEvent::ActionRejected { .. }) => {
                Some(PreparedEventStateDelta::NoState(body.clone()))
            }
            WorldEventBody::Domain(event)
                if crate::state::core_policy_transition::PreparedCorePolicyEvent::supports(event) =>
            {
                Some(PreparedEventStateDelta::CorePolicy(
                    Box::new(crate::state::core_policy_transition::PreparedCorePolicyEvent::prepare(
                        &self.state,
                        event,
                        self.state.time,
                    )?),
                ))
            }
            WorldEventBody::Domain(
                event @ DomainEvent::LogisticsPathRerouted {
                    requester_agent_id,
                    ..
                },
            ) => Some(PreparedEventStateDelta::DomainRouteOnly {
                event: event.clone(),
                agent_id: requester_agent_id.clone(),
            }),
            WorldEventBody::Domain(
                event @ (DomainEvent::LogisticsRouteRegistered { .. }
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
                | DomainEvent::FactoryProductionPaused { .. }),
            ) => Some(PreparedEventStateDelta::Industry(
                crate::state::industry_transition::PreparedIndustryEvent::prepare(
                    &self.state,
                    event,
                    self.state.time,
                )?,
            )),
            WorldEventBody::ModuleStateUpdated(update) => {
                Some(PreparedEventStateDelta::ModuleStateUpdated {
                    module_states: BTreeMap::from([(
                        update.module_id.clone(),
                        update.state.clone(),
                    )]),
                })
            }
            WorldEventBody::ModuleEmitted(event) => {
                self.state
                    .prepare_module_visual_event(event)?
                    .map(|next| PreparedEventStateDelta::ModuleVisualEntities {
                        event: event.clone(),
                        next,
                    })
                    .or_else(|| PreparedEventStateDelta::for_body(self, &body))
            }
            WorldEventBody::ModuleRuntimeCharged(charge) => {
                Some(PreparedEventStateDelta::ModuleRuntimeCharged(
                    self.prepare_module_runtime_charge_event(charge, self.state.time)?,
                ))
            }
            WorldEventBody::EffectQueued(intent) => {
                Some(self.prepare_raw_effect_queue_delta(intent)?)
            }
            WorldEventBody::ReceiptAppended(receipt) => {
                Some(self.prepare_raw_receipt_delta(receipt)?)
            }
            WorldEventBody::ModuleEvent(event) => Some(self.prepare_raw_module_event_delta(event)?),
            WorldEventBody::ManifestUpdated(update) => {
                Some(PreparedEventStateDelta::ManifestUpdated {
                    update: update.clone(),
                    manifest: update.manifest.clone(),
                })
            }
            WorldEventBody::Governance(
                event @ (GovernanceEvent::RestrictedStarterClaimAdminRegistryUpdated { .. }
                | GovernanceEvent::ValidatorAdmissionSubmitted { .. }
                | GovernanceEvent::ValidatorAdmissionApproved { .. }
                | GovernanceEvent::ValidatorAdmissionActivated { .. }
                | GovernanceEvent::ValidatorAdmissionRevoked { .. }),
            ) => Some(PreparedEventStateDelta::GovernanceRegistry(
                self.prepare_governance_registry_event(event)?,
            )),
            WorldEventBody::CapabilityAuthorization(
                event @ (CapabilityAuthorizationEvent::AuthorityInstalledWithProof { .. }
                | CapabilityAuthorizationEvent::AgentIdentityInstalled { .. }
                | CapabilityAuthorizationEvent::AgentSignerDelegationInstalled { .. }
                | CapabilityAuthorizationEvent::WorldServiceIntentRecorded { .. }
                | CapabilityAuthorizationEvent::SystemIdentityInstalled { .. }
                | CapabilityAuthorizationEvent::InvocationContextInstalled { .. }
                | CapabilityAuthorizationEvent::BudgetAccountInstalled { .. }
                | CapabilityAuthorizationEvent::GrantRegistered { .. }),
            ) => Some(PreparedEventStateDelta::CapabilityAuthorization(
                Box::new(self.prepare_raw_capability_authorization_event(event)?),
            )),
            WorldEventBody::CapabilityAuthorization(
                event @ CapabilityAuthorizationEvent::CommandCommitted { .. },
            ) => Some(PreparedEventStateDelta::CapabilityCommandCommit(
                self.prepare_raw_command_commit(event, self.state.time)?,
            )),
            WorldEventBody::CapabilityAuthorization(
                event @ CapabilityAuthorizationEvent::EffectReceiptCommitted { .. },
            ) => Some(PreparedEventStateDelta::CapabilityEffectReceipt(
                self.prepare_raw_effect_receipt_commit(event)?,
            )),
            _ => prepared_governance_events::prepare(self, &body)?
                .or_else(|| PreparedEventStateDelta::for_body(self, &body)),
        };
        let state_delta = state_delta.ok_or_else(|| WorldError::ResourceBalanceInvalid {
            reason: format!("unclassified world event body cannot be published: {body:?}"),
        })?;
        self.append_event_internal(body, caused_by, state_delta)
    }

    pub(in crate::world) fn append_event_with_prepared_body(
        &mut self,
        body: WorldEventBody,
        caused_by: Option<CausedBy>,
        prepared: PreparedBodyAttributesUpdate,
    ) -> Result<WorldEventId, WorldError> {
        self.append_event_internal(body, caused_by, PreparedEventStateDelta::Body(prepared))
    }

    pub(in crate::world) fn append_body_attributes_rejected(
        &mut self,
        body: WorldEventBody,
        caused_by: Option<CausedBy>,
        agent_id: String,
    ) -> Result<WorldEventId, WorldError> {
        if let WorldEventBody::Domain(event @ DomainEvent::BodyAttributesRejected { .. }) = &body {
            let prepared =
                crate::state::core_policy_transition::PreparedCorePolicyEvent::prepare(
                    &self.state,
                    event,
                    self.state.time,
                )?;
            return self.append_event_internal(
                body,
                caused_by,
                PreparedEventStateDelta::CorePolicy(Box::new(prepared)),
            );
        }
        Err(WorldError::ResourceBalanceInvalid {
            reason: format!("route-only publication is unsupported for agent {agent_id}"),
        })
    }

    fn append_event_internal(
        &mut self,
        body: WorldEventBody,
        caused_by: Option<CausedBy>,
        state_delta: PreparedEventStateDelta,
    ) -> Result<WorldEventId, WorldError> {
        self.append_prepared_event(body, caused_by, state_delta)
    }

    fn prepare_raw_effect_queue_delta(
        &self,
        intent: &EffectIntent,
    ) -> Result<PreparedEventStateDelta, WorldError> {
        let (pending_effects, pending_effects_evicted) =
            self.prepare_pending_effect_queue(intent.clone())?;
        Ok(PreparedEventStateDelta::EffectQueued {
            intent_id: intent.identity_key(),
            pending_effects,
            pending_effects_evicted,
        })
    }

    fn prepare_raw_receipt_delta(
        &self,
        receipt: &EffectReceipt,
    ) -> Result<PreparedEventStateDelta, WorldError> {
        let identity_key = self.effect_receipt_key(receipt)?;
        let mut pending_effects = self.pending_effects.clone();
        let mut inflight_effects = self.inflight_effects.clone();
        let mut removed = inflight_effects.remove(&identity_key).is_some();
        let before = pending_effects.len();
        pending_effects.retain(|intent| intent.identity_key() != identity_key);
        removed |= before != pending_effects.len();
        if !removed {
            return Err(WorldError::ReceiptUnknownIntent {
                intent_id: receipt.intent_id.clone(),
            });
        }
        Ok(PreparedEventStateDelta::ReceiptAppended {
            intent_id: identity_key,
            pending_effects,
            inflight_effects,
        })
    }

    fn prepare_raw_module_event_delta(
        &self,
        event: &ModuleEvent,
    ) -> Result<PreparedEventStateDelta, WorldError> {
        let mut module_registry = self.module_registry.clone();
        let mut module_artifacts = self.module_artifacts.clone();
        let mut module_tick_schedule = self.module_tick_schedule.clone();
        let mut cache_invalidations = BTreeSet::new();
        super::super::governance_publication::project_module_event(
            self.state.time,
            event,
            &mut module_registry,
            &mut module_artifacts,
            &mut module_tick_schedule,
            &mut cache_invalidations,
        )?;
        Ok(PreparedEventStateDelta::ModuleEvent {
            event: event.clone(),
            module_registry,
            module_artifacts,
            module_tick_schedule,
            cache_invalidations,
        })
    }

    fn append_prepared_event(
        &mut self,
        body: WorldEventBody,
        caused_by: Option<CausedBy>,
        state_delta: PreparedEventStateDelta,
    ) -> Result<WorldEventId, WorldError> {
        let prepared = self.prepare_event_publication(body, caused_by, state_delta)?;
        if self.take_fail_next_append_after_publication_prepare_for_test() {
            return Err(WorldError::ResourceBalanceInvalid {
                reason: "injected append_event failure after publication preparation".to_string(),
            });
        }

        self.install_event_publication(prepared, None)
    }

    fn install_event_publication(
        &mut self,
        prepared: PreparedEventPublication,
        release: Option<
            super::super::super::state::module_release_transition::PreparedModuleRelease,
        >,
    ) -> Result<WorldEventId, WorldError> {
        let PreparedEventPublication {
            event,
            next_event_id,
            next_event_id_era,
            journal_events,
            journal_events_evicted,
            consensus_record,
            state_delta,
        } = prepared;
        self.install_prepared_tick_consensus_record(consensus_record)?;
        state_delta.install_infallible(self);
        self.state.time = event.time;
        self.next_event_id = next_event_id;
        self.next_event_id_era = next_event_id_era;
        self.journal.events = journal_events;
        self.runtime_backpressure_stats.journal_events_evicted = self
            .runtime_backpressure_stats
            .journal_events_evicted
            .saturating_add(journal_events_evicted);

        if let Some(release) = release {
            release.install_routed(&mut self.state);
        } else if let WorldEventBody::Domain(domain_event) = &event.body {
            self.state.route_domain_event(domain_event);
        }
        Ok(event.id)
    }

    pub(in crate::world) fn append_module_artifact_deployment(
        &mut self,
        event: DomainEvent,
        caused_by: Option<CausedBy>,
        registration: super::super::module_runtime::PreparedModuleArtifactRegistration,
    ) -> Result<WorldEventId, WorldError> {
        let event_wasm_hash = match &event {
            DomainEvent::ModuleArtifactDeployed { wasm_hash, .. } => wasm_hash,
            _ => {
                return Err(WorldError::ModuleChangeInvalid {
                    reason: "artifact registration requires a deployment event".to_string(),
                });
            }
        };
        if event_wasm_hash != registration.wasm_hash() {
            return Err(WorldError::ModuleChangeInvalid {
                reason: format!(
                    "artifact registration hash {} does not match deployment hash {event_wasm_hash}",
                    registration.wasm_hash()
                ),
            });
        }
        let state_delta = PreparedEventStateDelta::ModuleMarketplace(
            self.state
                .prepare_module_marketplace_event(&event, self.state.time)?,
        );
        let prepared =
            self.prepare_event_publication(WorldEventBody::Domain(event), caused_by, state_delta)?;
        if self.take_fail_next_append_after_publication_prepare_for_test() {
            return Err(WorldError::ResourceBalanceInvalid {
                reason: "injected append_event failure after publication preparation".to_string(),
            });
        }
        let event_id = self.install_event_publication(prepared, None)?;
        registration.install(self);
        Ok(event_id)
    }

    pub(in crate::world) fn append_module_artifact_retirement(
        &mut self,
        event: DomainEvent,
        caused_by: Option<CausedBy>,
        retirement: super::super::module_artifact_retirement::PreparedModuleArtifactRetirement,
    ) -> Result<WorldEventId, WorldError> {
        if !retirement.matches_event(&event) {
            return Err(WorldError::ModuleChangeInvalid {
                reason: "artifact retirement requires a matching destroyed event".to_string(),
            });
        }
        let state_delta = PreparedEventStateDelta::ModuleMarketplace(
            self.state
                .prepare_module_marketplace_event(&event, self.state.time)?,
        );
        let prepared =
            self.prepare_event_publication(WorldEventBody::Domain(event), caused_by, state_delta)?;
        if self.take_fail_next_append_after_publication_prepare_for_test() {
            return Err(WorldError::ResourceBalanceInvalid {
                reason: "injected append_event failure after publication preparation".to_string(),
            });
        }
        let event_id = self.install_event_publication(prepared, None)?;
        retirement.install(self);
        Ok(event_id)
    }

    pub(in crate::world) fn append_module_install_with_release(
        &mut self,
        event: DomainEvent,
        caused_by: Option<CausedBy>,
        completion: super::super::module_release_publication::ModuleReleaseCompletion,
    ) -> Result<(), WorldError> {
        let instance = self.state.prepare_module_instance_event_with_registry(
            &event,
            self.state.time,
            Some(&self.module_registry),
        )?;
        let schedule = self.prepare_module_instance_schedule(&event, self.state.time)?;
        let mut prepared = self.prepare_event_publication(
            WorldEventBody::Domain(event.clone()),
            caused_by.clone(),
            PreparedEventStateDelta::ModuleInstance {
                prepared: instance,
                schedule,
            },
        )?;
        if self.take_fail_next_append_after_publication_prepare_for_test() {
            return Err(WorldError::ResourceBalanceInvalid {
                reason: "injected append_event failure after publication preparation".to_string(),
            });
        }
        let PreparedEventStateDelta::ModuleInstance {
            prepared: instance, ..
        } = &prepared.state_delta
        else {
            unreachable!()
        };
        let manifest_hash = self.current_manifest_hash()?;
        let tail = self.prepare_module_release_tail(
            instance,
            &event,
            completion,
            super::super::module_release_publication::ReleasePublicationJournal {
                next_event_id: prepared.next_event_id,
                next_event_id_era: prepared.next_event_id_era,
                events: std::mem::take(&mut prepared.journal_events),
                evicted: prepared.journal_events_evicted,
            },
            &manifest_hash,
            caused_by,
        )?;
        prepared.next_event_id = tail.journal.next_event_id;
        prepared.next_event_id_era = tail.journal.next_event_id_era;
        prepared.journal_events = tail.journal.events;
        prepared.journal_events_evicted = tail.journal.evicted;
        prepared.consensus_record = tail.consensus_record;
        self.install_event_publication(prepared, Some(tail.state))?;
        Ok(())
    }

    fn prepare_event_publication(
        &self,
        body: WorldEventBody,
        caused_by: Option<CausedBy>,
        state_delta: PreparedEventStateDelta,
    ) -> Result<PreparedEventPublication, WorldError> {
        if !state_delta.matches_body(&body) {
            return Err(WorldError::ResourceBalanceInvalid {
                reason: "prepared body delta does not match body event".to_string(),
            });
        }
        let domain_event = match &body {
            WorldEventBody::Domain(domain_event) => Some(domain_event.clone()),
            _ => None,
        };
        let (event_id, next_event_id, next_event_id_era) =
            Self::preview_next_event_id(self.next_event_id, self.next_event_id_era);
        if let Some(domain_event) = domain_event.as_ref() {
            self.validate_agent_intent_receipt_reference(domain_event, Some(event_id))?;
        }
        let event = WorldEvent {
            id: event_id,
            time: self.state.time,
            caused_by,
            body,
        };

        let mut journal_events = self.journal.events.clone();
        journal_events.push(event.clone());
        let max_len = self.runtime_memory_limits.max_journal_events.max(1);
        let overflow = journal_events.len().saturating_sub(max_len);
        if overflow > 0 {
            journal_events.drain(0..overflow);
        }
        let tick_events: Vec<WorldEvent> = journal_events
            .iter()
            .filter(|journal_event| journal_event.time == event.time)
            .cloned()
            .collect();
        let state_root = match &state_delta {
            PreparedEventStateDelta::ModuleMarketplace(prepared) => {
                self.state_root_hash_with_module_marketplace_overlay(prepared)?
            }
            PreparedEventStateDelta::ModuleRelease(prepared) => self
                .state_root_hash_with_module_release_overlay(
                    None,
                    prepared,
                    &self.current_manifest_hash()?,
                )?,
            PreparedEventStateDelta::ModuleInstance { prepared, .. } => {
                self.state_root_hash_with_module_instance_overlay(prepared)?
            }
            PreparedEventStateDelta::ModuleStateUpdated { module_states } => self
                .state_root_hash_with_command_overlay(
                    super::super::super::state::CommandStateOverlay {
                        module_states,
                        resources: &BTreeMap::new(),
                        agents: &BTreeMap::new(),
                    },
                )?,
            PreparedEventStateDelta::ModuleVisualEntities { next, .. } => {
                self.state_root_hash_with_module_visual_overlay(next)?
            }
            PreparedEventStateDelta::ModuleRuntimeCharged(prepared) => self
                .state_root_hash_with_command_overlay(
                    super::super::super::state::CommandStateOverlay {
                        module_states: &BTreeMap::new(),
                        resources: &prepared.resources,
                        agents: &prepared.agents,
                    },
                )?,
            PreparedEventStateDelta::EffectQueued { .. }
            | PreparedEventStateDelta::ReceiptAppended { .. } => {
                // Effect queues are World sidecars outside the canonical WorldState root schema.
                self.current_state_root_hash()?
            }
            PreparedEventStateDelta::ModuleEvent { .. } => self.current_state_root_hash()?,
            PreparedEventStateDelta::ManifestUpdated { manifest, .. } => {
                super::super::governance_publication::state_root_hash_with_manifest(self, manifest)?
            }
            PreparedEventStateDelta::GovernanceRegistry(prepared) => {
                self.state_root_hash_with_governance_registry_overlay(prepared)?
            }
            PreparedEventStateDelta::CapabilityAuthorization(_)
            | PreparedEventStateDelta::CapabilityCommandCommit(_)
            | PreparedEventStateDelta::CapabilityEffectReceipt(_) => {
                self.current_state_root_hash()?
            }
            PreparedEventStateDelta::AgentIntent(prepared) => {
                self.state_root_hash_with_agent_intent_overlay(prepared)?
            }
            PreparedEventStateDelta::EconomyData(prepared) => {
                self.state_root_hash_with_economy_data_overlay(prepared)?
            }
            PreparedEventStateDelta::EconomicContract(prepared) => {
                self.state_root_hash_with_economic_contract_overlay(prepared)?
            }
            PreparedEventStateDelta::AllianceWar(prepared) => {
                self.state_root_hash_with_alliance_war_overlay(prepared)?
            }
            PreparedEventStateDelta::GovernanceMeta(prepared) => {
                self.state_root_hash_with_governance_meta_overlay(prepared)?
            }
            PreparedEventStateDelta::CorePolicy(prepared) => {
                self.state_root_hash_with_core_policy_overlay(prepared)?
            }
            PreparedEventStateDelta::Industry(prepared) => {
                self.state_root_hash_with_industry_overlay(prepared)?
            }
            PreparedEventStateDelta::IndustryHistory(prepared) => {
                self.state_root_hash_with_industry_history_overlay(prepared)?
            }
            PreparedEventStateDelta::PowerRedemption(prepared) => {
                self.state_root_hash_with_power_redemption_overlay(prepared)?
            }
            PreparedEventStateDelta::NodePointsSettlement(prepared) => {
                self.state_root_hash_with_node_points_settlement_overlay(prepared)?
            }
            PreparedEventStateDelta::MainTokenMonetary(prepared) => {
                self.state_root_hash_with_main_token_monetary_overlay(prepared)?
            }
            PreparedEventStateDelta::MainTokenGovernanceMonetary(prepared) => {
                self.state_root_hash_with_main_token_governance_monetary_overlay(prepared)?
            }
            PreparedEventStateDelta::MainTokenRestrictedClaim(prepared) => {
                self.state_root_hash_with_main_token_restricted_claim_overlay(prepared)?
            }
            PreparedEventStateDelta::StarterOcClaimed(prepared) => {
                self.state_root_hash_with_starter_oc_claim_overlay(prepared)?
            }
            PreparedEventStateDelta::AgentClaimLightLifecycle(prepared) => {
                self.state_root_hash_with_agent_claim_light_lifecycle_overlay(prepared)?
            }
            PreparedEventStateDelta::AgentClaimEconomic(prepared) => {
                self.state_root_hash_with_agent_claim_economic_overlay(prepared)?
            }
            PreparedEventStateDelta::AgentClaimTerminal(prepared) => {
                self.state_root_hash_with_agent_claim_terminal_overlay(prepared)?
            }
            PreparedEventStateDelta::NoState(_) => self.current_state_root_hash()?,
            PreparedEventStateDelta::ProductValidationDeliveryCursorUpdated { next, .. } => {
                self.state_root_hash_with_product_validation_delivery_cursor(next)?
            }
            PreparedEventStateDelta::Body(_) | PreparedEventStateDelta::DomainRouteOnly { .. } => {
                let Some(domain_event) = domain_event.as_ref() else {
                    return Err(WorldError::ResourceBalanceInvalid {
                        reason: "prepared body delta requires a domain event".to_string(),
                    });
                };
                let body_overlay = state_delta.state_overlay(domain_event.clone());
                self.state_root_hash_with_body_overlay(&body_overlay)?
            }
            PreparedEventStateDelta::GovernanceEmergencyBrake { .. } => {
                // The emergency-brake gate is a World sidecar and is intentionally
                // outside the canonical WorldState root schema.
                self.current_state_root_hash()?
            }
            PreparedEventStateDelta::GovernanceFinalityEpochSnapshot { .. } => {
                // Finality epoch snapshots are persisted World sidecar data and
                // intentionally remain outside the canonical WorldState root schema.
                self.current_state_root_hash()?
            }
            PreparedEventStateDelta::GovernanceEmergencyVeto { .. } => {
                // Governance proposals are persisted World sidecar data and
                // intentionally remain outside the canonical WorldState root schema.
                self.current_state_root_hash()?
            }
            PreparedEventStateDelta::GovernanceProposal { .. }
            | PreparedEventStateDelta::GovernanceProposalShadow { .. }
            | PreparedEventStateDelta::GovernanceProposalStatus { .. } => {
                // Governance proposals are persisted World sidecar data and
                // intentionally remain outside the canonical WorldState root schema.
                self.current_state_root_hash()?
            }
            PreparedEventStateDelta::GovernanceIdentityPenaltyAppeal { .. } => {
                // Identity penalty records are persisted World sidecar data and
                // intentionally remain outside the canonical WorldState root schema.
                self.current_state_root_hash()?
            }
            PreparedEventStateDelta::GovernanceIdentityPenaltyApplication {
                target_agent_id,
                next_profile,
                profile_was_present,
                ..
            } => self.state_root_hash_with_governance_identity_profile_overlay(
                target_agent_id.as_str(),
                next_profile,
                !profile_was_present,
            )?,
            PreparedEventStateDelta::GovernanceIdentityPenaltyResolution {
                target_agent_id,
                next_profile,
                ..
            } => self.state_root_hash_with_governance_identity_profile_overlay(
                target_agent_id.as_str(),
                next_profile,
                false,
            )?,
        };
        let consensus_record = self.build_tick_consensus_record_for_prepared_events(
            event.time,
            tick_events.as_slice(),
            state_root.clone(),
        )?;
        self.validate_tick_consensus_candidate_for_prepared_publication(
            &consensus_record,
            tick_events.as_slice(),
            state_root.as_str(),
        )?;

        Ok(PreparedEventPublication {
            event,
            next_event_id,
            next_event_id_era,
            journal_events,
            journal_events_evicted: overflow as u64,
            consensus_record,
            state_delta,
        })
    }
}
