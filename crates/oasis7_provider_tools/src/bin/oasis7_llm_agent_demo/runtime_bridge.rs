use oasis7::runtime::{
    Action as RuntimeAction, CrisisStatus, DomainEvent as RuntimeDomainEvent,
    EconomicContractFulfillmentKind, RejectReason as RuntimeRejectReason, World as RuntimeWorld,
    WorldEventBody as RuntimeWorldEventBody,
};
use oasis7::simulator::{ActionResult, WorldEvent, WorldKernel};
use oasis7_agent_api::world_action::ResourceKind;
use oasis7_simulator::{Action as SimulatorAction, RejectReason, ResourceOwner, WorldEventKind};

fn simulator_gameplay_action_to_runtime(action: &SimulatorAction) -> Option<RuntimeAction> {
    match action {
        SimulatorAction::FormAlliance {
            proposer_agent_id,
            alliance_id,
            members,
            charter,
        } => Some(RuntimeAction::FormAlliance {
            proposer_agent_id: proposer_agent_id.clone(),
            alliance_id: alliance_id.clone(),
            members: members.clone(),
            charter: charter.clone(),
        }),
        SimulatorAction::JoinAlliance {
            operator_agent_id,
            alliance_id,
            member_agent_id,
        } => Some(RuntimeAction::JoinAlliance {
            operator_agent_id: operator_agent_id.clone(),
            alliance_id: alliance_id.clone(),
            member_agent_id: member_agent_id.clone(),
        }),
        SimulatorAction::LeaveAlliance {
            operator_agent_id,
            alliance_id,
            member_agent_id,
        } => Some(RuntimeAction::LeaveAlliance {
            operator_agent_id: operator_agent_id.clone(),
            alliance_id: alliance_id.clone(),
            member_agent_id: member_agent_id.clone(),
        }),
        SimulatorAction::DissolveAlliance {
            operator_agent_id,
            alliance_id,
            reason,
        } => Some(RuntimeAction::DissolveAlliance {
            operator_agent_id: operator_agent_id.clone(),
            alliance_id: alliance_id.clone(),
            reason: reason.clone(),
        }),
        SimulatorAction::DeclareWar {
            initiator_agent_id,
            war_id,
            aggressor_alliance_id,
            defender_alliance_id,
            objective,
            intensity,
        } => Some(RuntimeAction::DeclareWar {
            initiator_agent_id: initiator_agent_id.clone(),
            war_id: war_id.clone(),
            aggressor_alliance_id: aggressor_alliance_id.clone(),
            defender_alliance_id: defender_alliance_id.clone(),
            objective: objective.clone(),
            intensity: *intensity,
        }),
        SimulatorAction::OpenGovernanceProposal {
            proposer_agent_id,
            proposal_key,
            title,
            description,
            options,
            voting_window_ticks,
            quorum_weight,
            pass_threshold_bps,
        } => Some(RuntimeAction::OpenGovernanceProposal {
            proposer_agent_id: proposer_agent_id.clone(),
            proposal_key: proposal_key.clone(),
            title: title.clone(),
            description: description.clone(),
            options: options.clone(),
            voting_window_ticks: *voting_window_ticks,
            quorum_weight: *quorum_weight,
            pass_threshold_bps: *pass_threshold_bps,
        }),
        SimulatorAction::CastGovernanceVote {
            voter_agent_id,
            proposal_key,
            option,
            weight,
        } => Some(RuntimeAction::CastGovernanceVote {
            voter_agent_id: voter_agent_id.clone(),
            proposal_key: proposal_key.clone(),
            option: option.clone(),
            weight: *weight,
        }),
        SimulatorAction::ResolveCrisis {
            resolver_agent_id,
            crisis_id,
            strategy,
            success,
        } => Some(RuntimeAction::ResolveCrisis {
            resolver_agent_id: resolver_agent_id.clone(),
            crisis_id: crisis_id.clone(),
            strategy: strategy.clone(),
            success: *success,
        }),
        SimulatorAction::GrantMetaProgress {
            operator_agent_id,
            target_agent_id,
            track,
            points,
            achievement_id,
        } => Some(RuntimeAction::GrantMetaProgress {
            operator_agent_id: operator_agent_id.clone(),
            target_agent_id: target_agent_id.clone(),
            track: track.clone(),
            points: *points,
            achievement_id: achievement_id.clone(),
        }),
        SimulatorAction::OpenEconomicContract {
            creator_agent_id,
            contract_id,
            counterparty_agent_id,
            settlement_kind,
            settlement_amount,
            reputation_stake,
            expires_at,
            description,
        } => Some(RuntimeAction::OpenEconomicContract {
            creator_agent_id: creator_agent_id.clone(),
            contract_id: contract_id.clone(),
            counterparty_agent_id: counterparty_agent_id.clone(),
            fulfillment_kind: EconomicContractFulfillmentKind::AtomicExchange,
            settlement_kind: *settlement_kind,
            settlement_amount: *settlement_amount,
            reputation_stake: *reputation_stake,
            expires_at: *expires_at,
            description: description.clone(),
        }),
        SimulatorAction::AcceptEconomicContract {
            accepter_agent_id,
            contract_id,
        } => Some(RuntimeAction::AcceptEconomicContract {
            accepter_agent_id: accepter_agent_id.clone(),
            contract_id: contract_id.clone(),
        }),
        SimulatorAction::SettleEconomicContract {
            operator_agent_id,
            contract_id,
            success,
            notes,
        } => Some(RuntimeAction::SettleEconomicContract {
            operator_agent_id: operator_agent_id.clone(),
            contract_id: contract_id.clone(),
            success: *success,
            notes: notes.clone(),
        }),
        _ => None,
    }
}

fn runtime_reject_reason_to_simulator(reason: &RuntimeRejectReason) -> RejectReason {
    match reason {
        RuntimeRejectReason::AgentAlreadyExists { agent_id } => RejectReason::AgentAlreadyExists {
            agent_id: agent_id.clone(),
        },
        RuntimeRejectReason::AgentNotFound { agent_id } => RejectReason::AgentNotFound {
            agent_id: agent_id.clone(),
        },
        RuntimeRejectReason::AgentsNotCoLocated {
            agent_id,
            other_agent_id,
        } => RejectReason::AgentsNotCoLocated {
            agent_id: agent_id.clone(),
            other_agent_id: other_agent_id.clone(),
        },
        RuntimeRejectReason::InvalidAmount { amount } => {
            RejectReason::InvalidAmount { amount: *amount }
        }
        RuntimeRejectReason::InsufficientResource {
            agent_id,
            kind,
            requested,
            available,
        } => RejectReason::InsufficientResource {
            owner: ResourceOwner::Agent {
                agent_id: agent_id.clone(),
            },
            kind: *kind,
            requested: *requested,
            available: *available,
        },
        RuntimeRejectReason::FactoryNotFound { factory_id } => RejectReason::FacilityNotFound {
            facility_id: factory_id.clone(),
        },
        RuntimeRejectReason::RuleDenied { notes } => RejectReason::RuleDenied {
            notes: notes.clone(),
        },
        other => RejectReason::RuleDenied {
            notes: vec![format!("runtime bridge reject: {:?}", other)],
        },
    }
}

pub(crate) fn is_bridgeable_action(action: &SimulatorAction) -> bool {
    simulator_gameplay_action_to_runtime(action).is_some()
}

pub(crate) fn execute_action_in_kernel(
    kernel: &mut WorldKernel,
    agent_id: &str,
    action: SimulatorAction,
) -> ActionResult {
    let action_id = kernel.submit_action_from_agent(agent_id.to_string(), action.clone());
    if let Some(event) = kernel.step() {
        let success = !matches!(event.kind, WorldEventKind::ActionRejected { .. });
        return ActionResult {
            action,
            action_id,
            success,
            event,
        };
    }

    ActionResult {
        action,
        action_id,
        success: false,
        event: WorldEvent {
            id: action_id,
            time: kernel.time(),
            kind: WorldEventKind::ActionRejected {
                reason: RejectReason::RuleDenied {
                    notes: vec!["kernel.step returned no event".to_string()],
                },
            },
            runtime_event: None,
        },
    }
}

pub(crate) fn execute_system_action_in_kernel(
    kernel: &mut WorldKernel,
    action: SimulatorAction,
) -> ActionResult {
    let action_id = kernel.submit_action_from_system(action.clone());
    if let Some(event) = kernel.step() {
        let success = !matches!(event.kind, WorldEventKind::ActionRejected { .. });
        return ActionResult {
            action,
            action_id,
            success,
            event,
        };
    }

    ActionResult {
        action,
        action_id,
        success: false,
        event: WorldEvent {
            id: action_id,
            time: kernel.time(),
            kind: WorldEventKind::ActionRejected {
                reason: RejectReason::RuleDenied {
                    notes: vec!["kernel.step returned no event".to_string()],
                },
            },
            runtime_event: None,
        },
    }
}

pub(crate) fn advance_kernel_time_with_noop_move(kernel: &mut WorldKernel, agent_id: &str) {
    let Some(current_location_id) = kernel
        .model()
        .agents
        .get(agent_id)
        .map(|agent| agent.location_id.clone())
    else {
        return;
    };
    kernel.submit_action_from_system(SimulatorAction::MoveAgent {
        agent_id: agent_id.to_string(),
        to: current_location_id,
    });
    let _ = kernel.step();
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RuntimeGameplayPreset {
    None,
    CivicHotspotV1,
}

impl RuntimeGameplayPreset {
    pub(crate) fn parse(raw: &str) -> Option<Self> {
        let normalized = raw.trim().to_ascii_lowercase().replace('-', "_");
        match normalized.as_str() {
            "" | "none" | "off" => Some(Self::None),
            "civic_hotspot_v1" => Some(Self::CivicHotspotV1),
            _ => None,
        }
    }

    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            Self::None => "none",
            Self::CivicHotspotV1 => "civic_hotspot_v1",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct RuntimeGameplayPresetHandles {
    pub governance_proposal_key: Option<String>,
    pub governance_vote_option: Option<String>,
    pub crisis_id: Option<String>,
    pub economic_contract_id: Option<String>,
    pub economic_contract_counterparty: Option<String>,
}

#[derive(Debug)]
pub(crate) struct RuntimeGameplayBridge {
    world: RuntimeWorld,
    next_simulator_event_id: u64,
}

impl RuntimeGameplayBridge {
    const CIVIC_HOTSPOT_PROPOSAL_KEY: &str = "preset.governance.civic_hotspot_v1";
    const CIVIC_HOTSPOT_CONTRACT_ID: &str = "preset.contract.civic_hotspot_v1";
    const CIVIC_HOTSPOT_VOTE_OPTION: &str = "approve";

    fn rejection_from_events(
        &self,
        journal_start: usize,
        action_id: u64,
    ) -> Option<RuntimeRejectReason> {
        for event in self.world.journal().events.iter().skip(journal_start) {
            if let RuntimeWorldEventBody::Domain(RuntimeDomainEvent::ActionRejected {
                action_id: rejected_action_id,
                reason,
            }) = &event.body
                && *rejected_action_id == action_id
            {
                return Some(reason.clone());
            }
        }
        None
    }

    fn submit_preset_action(&mut self, action: RuntimeAction, label: &str) -> Result<(), String> {
        let action_id = self.world.submit_action(action);
        let journal_start = self.world.journal().events.len();
        self.world
            .step()
            .map_err(|err| format!("{label} step failed: {err:?}"))?;
        if let Some(reason) = self.rejection_from_events(journal_start, action_id) {
            return Err(format!("{label} rejected: {reason:?}"));
        }
        Ok(())
    }

    fn seed_civic_hotspot_v1(&mut self) -> Result<RuntimeGameplayPresetHandles, String> {
        let mut agent_ids: Vec<String> = self.world.state().agents.keys().cloned().collect();
        agent_ids.sort();
        if agent_ids.len() < 2 {
            return Err(
                "runtime gameplay preset civic_hotspot_v1 requires at least 2 agents".to_string(),
            );
        }
        let proposer = agent_ids[0].clone();
        let counterparty = agent_ids[1].clone();

        self.submit_preset_action(
            RuntimeAction::OpenGovernanceProposal {
                proposer_agent_id: proposer.clone(),
                proposal_key: Self::CIVIC_HOTSPOT_PROPOSAL_KEY.to_string(),
                title: "civic hotspot governance proposal".to_string(),
                description: "seeded proposal for gameplay continuation tests".to_string(),
                options: vec![
                    Self::CIVIC_HOTSPOT_VOTE_OPTION.to_string(),
                    "reject".to_string(),
                ],
                voting_window_ticks: 96,
                quorum_weight: 1,
                pass_threshold_bps: 5_000,
            },
            "seed civic_hotspot_v1 open_governance_proposal",
        )?;

        let expires_at = self.world.state().time.saturating_add(64);
        self.submit_preset_action(
            RuntimeAction::OpenEconomicContract {
                creator_agent_id: proposer.clone(),
                contract_id: Self::CIVIC_HOTSPOT_CONTRACT_ID.to_string(),
                counterparty_agent_id: counterparty.clone(),
                fulfillment_kind: EconomicContractFulfillmentKind::AtomicExchange,
                settlement_kind: ResourceKind::Data,
                settlement_amount: 10,
                reputation_stake: 3,
                expires_at,
                description: "seeded contract for gameplay continuation tests".to_string(),
            },
            "seed civic_hotspot_v1 open_economic_contract",
        )?;
        self.submit_preset_action(
            RuntimeAction::AcceptEconomicContract {
                accepter_agent_id: counterparty.clone(),
                contract_id: Self::CIVIC_HOTSPOT_CONTRACT_ID.to_string(),
            },
            "seed civic_hotspot_v1 accept_economic_contract",
        )?;

        for _ in 0..8 {
            if self
                .world
                .state()
                .crises
                .values()
                .any(|crisis| crisis.status == CrisisStatus::Active)
            {
                break;
            }
            self.world
                .step()
                .map_err(|err| format!("seed civic_hotspot_v1 crisis advance failed: {err:?}"))?;
        }
        let crisis_id = self
            .world
            .state()
            .crises
            .iter()
            .find_map(|(crisis_id, crisis)| {
                if crisis.status == CrisisStatus::Active {
                    Some(crisis_id.clone())
                } else {
                    None
                }
            })
            .ok_or_else(|| {
                "seed civic_hotspot_v1 failed to spawn active crisis within expected steps"
                    .to_string()
            })?;

        Ok(RuntimeGameplayPresetHandles {
            governance_proposal_key: Some(Self::CIVIC_HOTSPOT_PROPOSAL_KEY.to_string()),
            governance_vote_option: Some(Self::CIVIC_HOTSPOT_VOTE_OPTION.to_string()),
            crisis_id: Some(crisis_id),
            economic_contract_id: Some(Self::CIVIC_HOTSPOT_CONTRACT_ID.to_string()),
            economic_contract_counterparty: Some(counterparty),
        })
    }

    pub(crate) fn from_kernel(kernel: &WorldKernel) -> Result<Self, String> {
        let mut world = oasis7::runtime_host::world::new();
        let mut agent_ids: Vec<String> = kernel.model().agents.keys().cloned().collect();
        agent_ids.sort();
        for agent_id in &agent_ids {
            let Some(agent) = kernel.model().agents.get(agent_id) else {
                continue;
            };
            world.submit_action(RuntimeAction::RegisterAgent {
                agent_id: agent_id.clone(),
                pos: agent.pos,
            });
        }

        if world.pending_actions_len() > 0 {
            world
                .step()
                .map_err(|err| format!("runtime gameplay bridge bootstrap step failed: {err:?}"))?;
        }

        Ok(Self {
            world,
            next_simulator_event_id: kernel.journal().len() as u64 + 1,
        })
    }

    #[cfg(all(test, feature = "test_tier_full"))]
    pub(crate) fn seed_agent_resource_balance(
        &mut self,
        agent_id: &str,
        kind: ResourceKind,
        amount: i64,
    ) -> Result<(), String> {
        self.world
            .set_agent_resource_balance(agent_id, kind, amount)
            .map_err(|error| format!("seed runtime bridge resource balance failed: {error:?}"))
    }

    pub(crate) fn apply_preset(
        &mut self,
        preset: RuntimeGameplayPreset,
    ) -> Result<RuntimeGameplayPresetHandles, String> {
        match preset {
            RuntimeGameplayPreset::None => Ok(RuntimeGameplayPresetHandles::default()),
            RuntimeGameplayPreset::CivicHotspotV1 => self.seed_civic_hotspot_v1(),
        }
    }

    pub(crate) fn execute(
        &mut self,
        tick: u64,
        agent_id: &str,
        action: SimulatorAction,
    ) -> Result<ActionResult, String> {
        let Some(runtime_action) = simulator_gameplay_action_to_runtime(&action) else {
            return Err("runtime gameplay bridge received non-gameplay action".to_string());
        };

        let runtime_action_id = self.world.submit_action(runtime_action);
        let previous_journal_len = self.world.journal().events.len();
        let rejection = if let Err(err) = self.world.step() {
            Some(RuntimeRejectReason::RuleDenied {
                notes: vec![format!("runtime world step failed: {err:?}")],
            })
        } else {
            self.rejection_from_events(previous_journal_len, runtime_action_id)
        };

        let simulator_event = if let Some(reason) = rejection.as_ref() {
            WorldEvent {
                id: self.next_simulator_event_id,
                time: tick,
                kind: WorldEventKind::ActionRejected {
                    reason: runtime_reject_reason_to_simulator(reason),
                },
                runtime_event: None,
            }
        } else {
            WorldEvent {
                id: self.next_simulator_event_id,
                time: tick,
                kind: WorldEventKind::ResourceTransferred {
                    from: ResourceOwner::Agent {
                        agent_id: agent_id.to_string(),
                    },
                    to: ResourceOwner::Agent {
                        agent_id: agent_id.to_string(),
                    },
                    kind: ResourceKind::Data,
                    amount: 0,
                },
                runtime_event: None,
            }
        };
        self.next_simulator_event_id = self.next_simulator_event_id.saturating_add(1);

        Ok(ActionResult {
            action,
            action_id: runtime_action_id,
            success: rejection.is_none(),
            event: simulator_event,
        })
    }

    #[cfg(all(test, feature = "test_tier_full"))]
    pub(crate) fn state(&self) -> &oasis7::runtime::WorldState {
        self.world.state()
    }
}
