use oasis7_wasm_abi::{
    ModuleCallErrorCode, ModuleCallFailure, ModuleEmitEvent, ModuleSandbox, ModuleSubscriptionStage,
};
use serde::Deserialize;
use std::cmp::Ordering;

use super::super::{
    CrisisStatus, DomainEvent, EconomicContractFulfillmentKind, EconomicContractStatus,
    GovernanceProposalStatus, ModuleRole, WarParticipantOutcome, WorldError, WorldEvent,
    WorldEventBody,
};
use super::World;
use oasis7_agent_api::world_action::ResourceKind;

const CRISIS_AUTO_INTERVAL_TICKS: u64 = 8;
const CRISIS_DEFAULT_DURATION_TICKS: u64 = 6;
const CRISIS_TIMEOUT_PENALTY_PER_SEVERITY: i64 = 10;
const WAR_SCORE_PER_MEMBER: i64 = 10;
const WAR_SCORE_REPUTATION_DIVISOR: i64 = 10;
const WAR_WINNER_REPUTATION_PER_INTENSITY: i64 = 2;
const WAR_LOSER_REPUTATION_PER_INTENSITY: i64 = 3;
const WAR_LOSER_ELECTRICITY_PENALTY_PER_INTENSITY: i64 = 6;
const WAR_LOSER_DATA_PENALTY_PER_INTENSITY: i64 = 4;
const CONTRACT_EXPIRY_COUNTERPARTY_PENALTY_DIVISOR: i64 = 2;
const GAMEPLAY_LIFECYCLE_EMIT_KIND: &str = "gameplay.lifecycle.directives";

#[derive(Debug, Deserialize)]
struct GameplayDirectiveEnvelope {
    #[serde(default)]
    directives: Vec<GameplayLifecycleDirective>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum GameplayLifecycleDirective {
    GovernanceFinalize {
        proposal_key: String,
        #[serde(default)]
        winning_option: Option<String>,
        winning_weight: u64,
        total_weight: u64,
        passed: bool,
    },
    CrisisSpawn {
        crisis_id: String,
        kind: String,
        severity: u32,
        expires_at: u64,
    },
    CrisisTimeout {
        crisis_id: String,
        penalty_impact: i64,
    },
    WarConclude {
        war_id: String,
        winner_alliance_id: String,
        #[serde(default)]
        loser_alliance_id: Option<String>,
        aggressor_score: i64,
        defender_score: i64,
        summary: String,
        #[serde(default)]
        participant_outcomes: Vec<WarParticipantOutcome>,
    },
    MetaGrant {
        operator_agent_id: String,
        target_agent_id: String,
        track: String,
        points: i64,
        #[serde(default)]
        achievement_id: Option<String>,
    },
}

impl World {
    pub(super) fn process_gameplay_cycles_with_modules(
        &mut self,
        sandbox: &mut dyn ModuleSandbox,
    ) -> Result<Vec<WorldEvent>, WorldError> {
        let has_gameplay_tick_modules = self.has_active_gameplay_tick_modules()?;

        let journal_start_event_id = self
            .journal
            .events
            .last()
            .map(|event| event.id)
            .unwrap_or(0);
        let _ = self.route_tick_to_modules(sandbox)?;
        if !has_gameplay_tick_modules {
            return self.process_gameplay_cycles();
        }

        let mut emitted = Vec::new();
        for module_emit in self.collect_gameplay_tick_emits(journal_start_event_id) {
            let directives = self.parse_gameplay_directives(&module_emit)?;
            for directive in directives {
                self.apply_gameplay_directive(directive, &mut emitted)?;
            }
        }

        Ok(emitted)
    }

    fn has_active_gameplay_tick_modules(&self) -> Result<bool, WorldError> {
        let invocations = self.collect_active_module_invocations()?;
        Ok(invocations.into_iter().any(|invocation| {
            invocation.manifest.role == ModuleRole::Gameplay
                && invocation
                    .manifest
                    .subscriptions
                    .iter()
                    .any(|subscription| {
                        subscription.resolved_stage() == ModuleSubscriptionStage::Tick
                    })
        }))
    }

    fn is_active_gameplay_module(&self, module_id: &str) -> bool {
        self.active_module_manifest(module_id)
            .map(|manifest| manifest.role == ModuleRole::Gameplay)
            .unwrap_or(false)
    }

    fn collect_gameplay_tick_emits(&self, journal_start_event_id: u64) -> Vec<ModuleEmitEvent> {
        self.journal
            .events
            .iter()
            .filter(|event| event.id > journal_start_event_id)
            .filter_map(|event| match &event.body {
                WorldEventBody::ModuleEmitted(module_emit)
                    if module_emit.trace_id.starts_with("tick-")
                        || module_emit.trace_id.starts_with("infra-tick-") =>
                {
                    if module_emit.kind == GAMEPLAY_LIFECYCLE_EMIT_KIND
                        && self.is_active_gameplay_module(module_emit.module_id.as_str())
                    {
                        Some(module_emit.clone())
                    } else {
                        None
                    }
                }
                _ => None,
            })
            .collect()
    }

    fn parse_gameplay_directives(
        &mut self,
        module_emit: &ModuleEmitEvent,
    ) -> Result<Vec<GameplayLifecycleDirective>, WorldError> {
        match serde_json::from_value::<GameplayDirectiveEnvelope>(module_emit.payload.clone()) {
            Ok(payload) => Ok(payload.directives),
            Err(err) => self.gameplay_module_output_invalid(
                module_emit.module_id.as_str(),
                module_emit.trace_id.as_str(),
                format!(
                    "decode {} payload failed: {}",
                    GAMEPLAY_LIFECYCLE_EMIT_KIND, err
                ),
            ),
        }
    }

    fn apply_gameplay_directive(
        &mut self,
        directive: GameplayLifecycleDirective,
        emitted: &mut Vec<WorldEvent>,
    ) -> Result<(), WorldError> {
        if let Some(event) = self.prepare_gameplay_directive_event(directive) {
            self.append_gameplay_domain_event(event, emitted)?;
        }
        Ok(())
    }

    fn prepare_gameplay_directive_event(
        &self,
        directive: GameplayLifecycleDirective,
    ) -> Option<DomainEvent> {
        match directive {
            GameplayLifecycleDirective::GovernanceFinalize {
                proposal_key,
                winning_option,
                winning_weight,
                total_weight,
                passed,
            } => Some(DomainEvent::GovernanceProposalFinalized {
                proposal_key,
                winning_option,
                winning_weight,
                total_weight,
                passed,
            }),
            GameplayLifecycleDirective::CrisisSpawn {
                crisis_id,
                kind,
                severity,
                expires_at,
            } => Some(DomainEvent::CrisisSpawned {
                crisis_id,
                kind,
                severity,
                expires_at,
            }),
            GameplayLifecycleDirective::CrisisTimeout {
                crisis_id,
                penalty_impact,
            } => Some(DomainEvent::CrisisTimedOut {
                crisis_id,
                penalty_impact,
            }),
            GameplayLifecycleDirective::WarConclude {
                war_id,
                winner_alliance_id,
                loser_alliance_id,
                aggressor_score,
                defender_score,
                summary,
                participant_outcomes,
            } => Some(DomainEvent::WarConcluded {
                loser_alliance_id: loser_alliance_id.unwrap_or_else(|| {
                    self.state
                        .wars
                        .get(war_id.as_str())
                        .map(|war| {
                            if war.aggressor_alliance_id == winner_alliance_id {
                                war.defender_alliance_id.clone()
                            } else {
                                war.aggressor_alliance_id.clone()
                            }
                        })
                        .unwrap_or_default()
                }),
                war_id,
                winner_alliance_id,
                aggressor_score,
                defender_score,
                summary,
                participant_outcomes,
            }),
            GameplayLifecycleDirective::MetaGrant {
                operator_agent_id,
                target_agent_id,
                track,
                points,
                achievement_id,
            } => {
                if points == 0 {
                    None
                } else {
                    Some(DomainEvent::MetaProgressGranted {
                        operator_agent_id,
                        target_agent_id,
                        track,
                        points,
                        achievement_id,
                    })
                }
            }
        }
    }

    fn gameplay_module_output_invalid<T>(
        &mut self,
        module_id: &str,
        trace_id: &str,
        detail: impl Into<String>,
    ) -> Result<T, WorldError> {
        let failure = ModuleCallFailure {
            module_id: module_id.to_string(),
            trace_id: trace_id.to_string(),
            code: ModuleCallErrorCode::InvalidOutput,
            detail: detail.into(),
        };
        self.append_event(WorldEventBody::ModuleCallFailed(failure.clone()), None)?;
        Err(WorldError::ModuleCallFailed {
            module_id: failure.module_id,
            trace_id: failure.trace_id,
            code: failure.code,
            detail: failure.detail,
        })
    }

    pub(super) fn process_gameplay_cycles(&mut self) -> Result<Vec<WorldEvent>, WorldError> {
        let mut emitted = Vec::new();
        self.process_economic_contract_lifecycle(&mut emitted)?;
        self.finalize_due_governance_proposals(&mut emitted)?;
        self.process_crisis_lifecycle(&mut emitted)?;
        self.process_war_lifecycle(&mut emitted)?;
        Ok(emitted)
    }

    fn process_economic_contract_lifecycle(
        &mut self,
        emitted: &mut Vec<WorldEvent>,
    ) -> Result<(), WorldError> {
        for event in self.prepare_economic_contract_expiry_events_at(self.state.time) {
            self.append_gameplay_domain_event(event, emitted)?;
        }
        Ok(())
    }

    fn prepare_economic_contract_expiry_events_at(&self, now: u64) -> Vec<DomainEvent> {
        let mut due_contracts = self
            .state
            .economic_contracts
            .values()
            .filter(|contract| {
                matches!(
                    contract.status,
                    EconomicContractStatus::Open | EconomicContractStatus::Accepted
                ) && contract.fulfillment_kind == EconomicContractFulfillmentKind::AtomicExchange
                    && contract.expires_at <= now
            })
            .map(|contract| {
                (
                    contract.contract_id.clone(),
                    contract.creator_agent_id.clone(),
                    contract.counterparty_agent_id.clone(),
                    contract.status,
                    contract.reputation_stake.max(1),
                )
            })
            .collect::<Vec<_>>();
        due_contracts.sort_by(|left, right| left.0.cmp(&right.0));

        due_contracts
            .into_iter()
            .map(
                |(
                    contract_id,
                    creator_agent_id,
                    counterparty_agent_id,
                    status,
                    reputation_stake,
                )| {
                    let (creator_reputation_delta, counterparty_reputation_delta) = match status {
                        EconomicContractStatus::Open => (-reputation_stake, 0),
                        EconomicContractStatus::Accepted => (
                            -reputation_stake,
                            -reputation_stake
                                .saturating_div(CONTRACT_EXPIRY_COUNTERPARTY_PENALTY_DIVISOR)
                                .max(1),
                        ),
                        EconomicContractStatus::Settled | EconomicContractStatus::Expired => (0, 0),
                    };
                    DomainEvent::EconomicContractExpired {
                        contract_id,
                        creator_agent_id,
                        counterparty_agent_id,
                        creator_reputation_delta,
                        counterparty_reputation_delta,
                    }
                },
            )
            .collect()
    }

    #[cfg(test)]
    pub(crate) fn prepared_economic_contract_expiry_events_for_test(
        &self,
        now: u64,
    ) -> Vec<DomainEvent> {
        self.prepare_economic_contract_expiry_events_at(now)
    }

    fn finalize_due_governance_proposals(
        &mut self,
        emitted: &mut Vec<WorldEvent>,
    ) -> Result<(), WorldError> {
        for event in self.prepare_governance_finalization_events_at(self.state.time) {
            self.append_gameplay_domain_event(event, emitted)?;
        }
        Ok(())
    }

    fn prepare_governance_finalization_events_at(&self, now: u64) -> Vec<DomainEvent> {
        let mut due_keys: Vec<_> = self
            .state
            .governance_proposals
            .iter()
            .filter(|(_, proposal)| {
                proposal.status == GovernanceProposalStatus::Open && proposal.closes_at <= now
            })
            .map(|(key, _)| key.clone())
            .collect();
        due_keys.sort();

        due_keys
            .into_iter()
            .filter_map(|proposal_key| {
                let proposal = self
                    .state
                    .governance_proposals
                    .get(&proposal_key)
                    .cloned()?;
                let vote_state = self.state.governance_votes.get(&proposal_key);
                let total_weight = vote_state.map(|value| value.total_weight).unwrap_or(0);
                let (winning_option, winning_weight) = vote_state
                    .and_then(|value| {
                        value
                            .tallies
                            .iter()
                            .max_by(|(left_option, left_weight), (right_option, right_weight)| {
                                left_weight
                                    .cmp(right_weight)
                                    .then_with(|| right_option.cmp(left_option))
                            })
                            .map(|(option, weight)| (Some(option.clone()), *weight))
                    })
                    .unwrap_or((None, 0));
                let reached_quorum = total_weight >= proposal.quorum_weight;
                let reached_threshold = if total_weight == 0 {
                    false
                } else {
                    (u128::from(winning_weight) * 10_000_u128)
                        >= (u128::from(total_weight) * u128::from(proposal.pass_threshold_bps))
                };
                let passed = reached_quorum && reached_threshold && winning_option.is_some();
                Some(DomainEvent::GovernanceProposalFinalized {
                    proposal_key: proposal_key.clone(),
                    winning_option,
                    winning_weight,
                    total_weight,
                    passed,
                })
            })
            .collect()
    }

    #[cfg(test)]
    pub(crate) fn prepared_governance_finalization_events_for_test(
        &self,
        now: u64,
    ) -> Vec<DomainEvent> {
        self.prepare_governance_finalization_events_at(now)
    }

    fn process_crisis_lifecycle(
        &mut self,
        emitted: &mut Vec<WorldEvent>,
    ) -> Result<(), WorldError> {
        if let Some(event) = self.prepare_crisis_auto_spawn_event_at(self.state.time) {
            self.append_gameplay_domain_event(event, emitted)?;
        }
        for event in self.prepare_crisis_timeout_events_at(self.state.time) {
            self.append_gameplay_domain_event(event, emitted)?;
        }
        Ok(())
    }

    fn prepare_crisis_auto_spawn_event_at(&self, now: u64) -> Option<DomainEvent> {
        let has_active_crisis = self
            .state
            .crises
            .values()
            .any(|crisis| crisis.status == CrisisStatus::Active);
        if !has_active_crisis && now > 0 && now.is_multiple_of(CRISIS_AUTO_INTERVAL_TICKS) {
            let sequence = now / CRISIS_AUTO_INTERVAL_TICKS;
            let severity = ((sequence % 3) + 1) as u32;
            let kind = match severity {
                1 => "supply_shock",
                2 => "solar_storm",
                _ => "network_outage",
            }
            .to_string();
            let crisis_id = format!("crisis.auto.{now}");
            let expires_at = now
                .saturating_add(CRISIS_DEFAULT_DURATION_TICKS)
                .saturating_add(u64::from(severity));
            return Some(DomainEvent::CrisisSpawned {
                crisis_id,
                kind,
                severity,
                expires_at,
            });
        }
        None
    }

    fn prepare_crisis_timeout_events_at(&self, now: u64) -> Vec<DomainEvent> {
        let mut due_timeouts: Vec<_> = self
            .state
            .crises
            .iter()
            .filter(|(_, crisis)| crisis.status == CrisisStatus::Active && crisis.expires_at <= now)
            .map(|(crisis_id, crisis)| (crisis_id.clone(), crisis.severity.max(1)))
            .collect();
        due_timeouts.sort_by(|left, right| left.0.cmp(&right.0));
        due_timeouts
            .into_iter()
            .map(|(crisis_id, severity)| {
                let penalty_impact =
                    -i64::from(severity).saturating_mul(CRISIS_TIMEOUT_PENALTY_PER_SEVERITY);
                DomainEvent::CrisisTimedOut {
                    crisis_id,
                    penalty_impact,
                }
            })
            .collect()
    }

    #[cfg(test)]
    pub(crate) fn prepared_crisis_auto_spawn_event_for_test(
        &self,
        now: u64,
    ) -> Option<DomainEvent> {
        self.prepare_crisis_auto_spawn_event_at(now)
    }

    #[cfg(test)]
    pub(crate) fn prepared_crisis_timeout_events_for_test(&self, now: u64) -> Vec<DomainEvent> {
        self.prepare_crisis_timeout_events_at(now)
    }

    fn process_war_lifecycle(&mut self, emitted: &mut Vec<WorldEvent>) -> Result<(), WorldError> {
        while let Some(event) = self.prepare_next_due_war_event_at(self.state.time) {
            self.append_gameplay_domain_event(event, emitted)?;
        }
        Ok(())
    }

    fn prepare_next_due_war_event_at(&self, now: u64) -> Option<DomainEvent> {
        let war = self
            .state
            .wars
            .values()
            .filter(|war| {
                war.active
                    && now
                        >= war
                            .declared_at
                            .saturating_add(war.max_duration_ticks.max(1))
            })
            .min_by(|left, right| left.war_id.cmp(&right.war_id))
            .cloned()?;
        let aggressor_members = self
            .state
            .alliances
            .get(&war.aggressor_alliance_id)
            .map(|alliance| alliance.members.len() as i64)
            .unwrap_or(0);
        let defender_members = self
            .state
            .alliances
            .get(&war.defender_alliance_id)
            .map(|alliance| alliance.members.len() as i64)
            .unwrap_or(0);
        let aggressor_reputation =
            self.alliance_reputation_total(war.aggressor_alliance_id.as_str());
        let defender_reputation = self.alliance_reputation_total(war.defender_alliance_id.as_str());
        let aggressor_score = aggressor_members
            .saturating_mul(WAR_SCORE_PER_MEMBER)
            .saturating_add(i64::from(war.intensity))
            .saturating_add(aggressor_reputation.saturating_div(WAR_SCORE_REPUTATION_DIVISOR));
        let defender_score = defender_members
            .saturating_mul(WAR_SCORE_PER_MEMBER)
            .saturating_add(defender_reputation.saturating_div(WAR_SCORE_REPUTATION_DIVISOR));
        let (winner_alliance_id, loser_alliance_id) = match aggressor_score.cmp(&defender_score) {
            Ordering::Greater | Ordering::Equal => (
                war.aggressor_alliance_id.clone(),
                war.defender_alliance_id.clone(),
            ),
            Ordering::Less => (
                war.defender_alliance_id.clone(),
                war.aggressor_alliance_id.clone(),
            ),
        };
        let participant_outcomes = self.build_war_participant_outcomes(
            winner_alliance_id.as_str(),
            loser_alliance_id.as_str(),
            war.intensity,
        );
        let summary = format!(
            "auto settlement: aggressor_score={} defender_score={} aggressor_reputation={} defender_reputation={} outcome_count={}",
            aggressor_score,
            defender_score,
            aggressor_reputation,
            defender_reputation,
            participant_outcomes.len()
        );
        Some(DomainEvent::WarConcluded {
            war_id: war.war_id,
            winner_alliance_id,
            loser_alliance_id,
            aggressor_score,
            defender_score,
            summary,
            participant_outcomes,
        })
    }

    #[cfg(test)]
    pub(crate) fn prepared_next_due_war_event_for_test(&self, now: u64) -> Option<DomainEvent> {
        self.prepare_next_due_war_event_at(now)
    }

    pub(super) fn alliance_reputation_total(&self, alliance_id: &str) -> i64 {
        self.state
            .alliances
            .get(alliance_id)
            .map(|alliance| {
                alliance
                    .members
                    .iter()
                    .map(|member| {
                        self.state
                            .reputation_scores
                            .get(member)
                            .copied()
                            .unwrap_or(0)
                    })
                    .sum()
            })
            .unwrap_or(0)
    }

    fn alliance_members_sorted(&self, alliance_id: &str) -> Vec<String> {
        let mut members = self
            .state
            .alliances
            .get(alliance_id)
            .map(|alliance| alliance.members.clone())
            .unwrap_or_default();
        members.sort();
        members
    }

    fn build_war_participant_outcomes(
        &self,
        winner_alliance_id: &str,
        loser_alliance_id: &str,
        intensity: u32,
    ) -> Vec<WarParticipantOutcome> {
        let intensity = i64::from(intensity.max(1));
        let loser_electricity_penalty = intensity
            .saturating_mul(WAR_LOSER_ELECTRICITY_PENALTY_PER_INTENSITY)
            .max(1);
        let loser_data_penalty = intensity
            .saturating_mul(WAR_LOSER_DATA_PENALTY_PER_INTENSITY)
            .max(1);
        let loser_reputation_delta = intensity
            .saturating_mul(WAR_LOSER_REPUTATION_PER_INTENSITY)
            .saturating_neg();
        let winner_reputation_delta = intensity.saturating_mul(WAR_WINNER_REPUTATION_PER_INTENSITY);

        let loser_members = self.alliance_members_sorted(loser_alliance_id);
        let winner_members = self.alliance_members_sorted(winner_alliance_id);
        let mut outcomes = Vec::new();
        let mut total_electricity_spoils = 0_i64;
        let mut total_data_spoils = 0_i64;

        for member in loser_members {
            let available_electricity = self
                .state
                .agents
                .get(member.as_str())
                .map(|cell| cell.state.resources.get(ResourceKind::Electricity))
                .unwrap_or(0)
                .max(0);
            let available_data = self
                .state
                .agents
                .get(member.as_str())
                .map(|cell| cell.state.resources.get(ResourceKind::Data))
                .unwrap_or(0)
                .max(0);
            let electricity_loss = available_electricity.min(loser_electricity_penalty);
            let data_loss = available_data.min(loser_data_penalty);
            total_electricity_spoils = total_electricity_spoils.saturating_add(electricity_loss);
            total_data_spoils = total_data_spoils.saturating_add(data_loss);
            outcomes.push(WarParticipantOutcome {
                agent_id: member,
                electricity_delta: electricity_loss.saturating_neg(),
                data_delta: data_loss.saturating_neg(),
                reputation_delta: loser_reputation_delta,
            });
        }

        if !winner_members.is_empty() {
            let winner_count = i64::try_from(winner_members.len()).unwrap_or(1).max(1);
            let base_electricity_gain = total_electricity_spoils.saturating_div(winner_count);
            let base_data_gain = total_data_spoils.saturating_div(winner_count);
            let mut electricity_remainder = total_electricity_spoils
                .saturating_sub(base_electricity_gain.saturating_mul(winner_count));
            let mut data_remainder =
                total_data_spoils.saturating_sub(base_data_gain.saturating_mul(winner_count));

            for member in winner_members {
                let mut electricity_gain = base_electricity_gain;
                let mut data_gain = base_data_gain;
                if electricity_remainder > 0 {
                    electricity_gain = electricity_gain.saturating_add(1);
                    electricity_remainder = electricity_remainder.saturating_sub(1);
                }
                if data_remainder > 0 {
                    data_gain = data_gain.saturating_add(1);
                    data_remainder = data_remainder.saturating_sub(1);
                }
                outcomes.push(WarParticipantOutcome {
                    agent_id: member,
                    electricity_delta: electricity_gain,
                    data_delta: data_gain,
                    reputation_delta: winner_reputation_delta,
                });
            }
        }

        outcomes.sort_by(|left, right| left.agent_id.cmp(&right.agent_id));
        outcomes
    }

    fn append_gameplay_domain_event(
        &mut self,
        event: DomainEvent,
        emitted: &mut Vec<WorldEvent>,
    ) -> Result<(), WorldError> {
        self.append_event(WorldEventBody::Domain(event), None)?;
        if let Some(event) = self.journal.events.last() {
            emitted.push(event.clone());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        EconomicContractState, ModuleAbiContract, ModuleKind, ModuleLimits, ModuleManifest,
        ModuleRecord, ModuleRegistry, ModuleRole,
    };
    use serde_json::json;

    fn manifest(module_id: &str, role: ModuleRole) -> ModuleManifest {
        ModuleManifest {
            module_id: module_id.to_string(),
            name: module_id.to_string(),
            version: "0.1.0".to_string(),
            kind: ModuleKind::Reducer,
            role,
            wasm_hash: format!("sha256:{module_id}"),
            interface_version: "wasm-1".to_string(),
            exports: vec!["reduce".to_string()],
            subscriptions: Vec::new(),
            required_caps: Vec::new(),
            abi_contract: ModuleAbiContract::default(),
            artifact_identity: None,
            limits: ModuleLimits::unbounded(),
        }
    }

    fn activate_module(world: &mut World, manifest: ModuleManifest) {
        let key = ModuleRegistry::record_key(&manifest.module_id, &manifest.version);
        world
            .module_registry
            .active
            .insert(manifest.module_id.clone(), manifest.version.clone());
        world.module_registry.records.insert(
            key,
            ModuleRecord {
                manifest,
                registered_at: 0,
                registered_by: "test".to_string(),
                audit_event_id: None,
            },
        );
    }

    fn module_emit(module_id: &str, trace_id: &str, kind: &str) -> ModuleEmitEvent {
        ModuleEmitEvent {
            module_id: module_id.to_string(),
            trace_id: trace_id.to_string(),
            kind: kind.to_string(),
            payload: json!({"directives": []}),
        }
    }

    #[test]
    fn persisted_service_contract_is_quarantined_from_expiry_lifecycle() {
        let mut world = World::new();
        world.state.time = 10;
        world
            .state
            .reputation_scores
            .insert("creator".to_string(), 7);
        world.state.economic_contracts.insert(
            "contract.service".to_string(),
            EconomicContractState {
                contract_id: "contract.service".to_string(),
                creator_agent_id: "creator".to_string(),
                counterparty_agent_id: "counterparty".to_string(),
                fulfillment_kind: EconomicContractFulfillmentKind::Service,
                settlement_kind: ResourceKind::Data,
                settlement_amount: 5,
                reputation_stake: 4,
                expires_at: 9,
                description: "persisted unsupported service".to_string(),
                status: EconomicContractStatus::Accepted,
                accepted_at: Some(2),
                settled_at: None,
                settlement_success: None,
                transfer_amount: 0,
                tax_amount: 0,
                settlement_notes: None,
            },
        );
        let journal_len = world.journal.events.len();
        let mut emitted = Vec::new();

        world
            .process_economic_contract_lifecycle(&mut emitted)
            .expect("quarantine service contract");

        let contract = world
            .state
            .economic_contracts
            .get("contract.service")
            .expect("service contract retained");
        assert_eq!(contract.status, EconomicContractStatus::Accepted);
        assert_eq!(world.state.reputation_scores.get("creator"), Some(&7));
        assert!(emitted.is_empty());
        assert_eq!(world.journal.events.len(), journal_len);
    }

    #[test]
    fn collect_gameplay_tick_emits_filters_before_returning_events() {
        let mut world = World::new();
        activate_module(&mut world, manifest("m.gameplay", ModuleRole::Gameplay));
        activate_module(&mut world, manifest("m.domain", ModuleRole::Domain));

        world
            .append_event(
                WorldEventBody::ModuleEmitted(module_emit(
                    "m.gameplay",
                    "tick-1",
                    GAMEPLAY_LIFECYCLE_EMIT_KIND,
                )),
                None,
            )
            .expect("append gameplay lifecycle emit");
        world
            .append_event(
                WorldEventBody::ModuleEmitted(module_emit("m.gameplay", "tick-1", "telemetry")),
                None,
            )
            .expect("append non-lifecycle emit");
        world
            .append_event(
                WorldEventBody::ModuleEmitted(module_emit(
                    "m.domain",
                    "tick-1",
                    GAMEPLAY_LIFECYCLE_EMIT_KIND,
                )),
                None,
            )
            .expect("append domain lifecycle emit");
        world
            .append_event(
                WorldEventBody::ModuleEmitted(module_emit(
                    "m.gameplay",
                    "module-call-1",
                    GAMEPLAY_LIFECYCLE_EMIT_KIND,
                )),
                None,
            )
            .expect("append non-tick lifecycle emit");

        let emits = world.collect_gameplay_tick_emits(0);

        assert_eq!(emits.len(), 1);
        assert_eq!(emits[0].module_id, "m.gameplay");
        assert_eq!(emits[0].trace_id, "tick-1");
        assert_eq!(emits[0].kind, GAMEPLAY_LIFECYCLE_EMIT_KIND);
    }

    #[test]
    fn gameplay_directive_preparation_is_non_mutating_and_preserves_zero_meta_grant() {
        let world = World::new();
        let snapshot_before_prepare = world.snapshot();
        let journal_before_prepare = world.journal().clone();

        let prepared =
            world.prepare_gameplay_directive_event(GameplayLifecycleDirective::MetaGrant {
                operator_agent_id: "operator".to_string(),
                target_agent_id: "target".to_string(),
                track: "campaign".to_string(),
                points: 7,
                achievement_id: Some("achievement.first".to_string()),
            });
        let prepared_zero =
            world.prepare_gameplay_directive_event(GameplayLifecycleDirective::MetaGrant {
                operator_agent_id: "operator".to_string(),
                target_agent_id: "target".to_string(),
                track: "campaign".to_string(),
                points: 0,
                achievement_id: None,
            });

        assert_eq!(world.snapshot(), snapshot_before_prepare);
        assert_eq!(world.journal(), &journal_before_prepare);
        assert!(matches!(
            prepared,
            Some(DomainEvent::MetaProgressGranted {
                operator_agent_id,
                target_agent_id,
                track,
                points: 7,
                achievement_id,
            }) if operator_agent_id == "operator"
                && target_agent_id == "target"
                && track == "campaign"
                && achievement_id.as_deref() == Some("achievement.first")
        ));
        assert!(prepared_zero.is_none());
    }
}
