use super::super::types::{
    Action, ActionEnvelope, ActionId, ActionSubmitter, ResourceOwner, WorldTime,
};
use super::WorldKernel;
use super::types::{
    KernelRuleDecision, KernelRuleModuleContext, KernelRuleModuleInput, KernelRuleVerdict,
    RejectReason, WorldEvent, WorldEventKind,
};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntentConflictResolution {
    pub conflict_key: String,
    pub winner_action_id: ActionId,
    pub loser_action_ids: Vec<ActionId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntentBatchReport {
    pub tick: WorldTime,
    pub batch_hash: String,
    pub intent_count: usize,
    pub accepted_action_ids: Vec<ActionId>,
    pub rejected_action_ids: Vec<ActionId>,
    pub conflicts: Vec<IntentConflictResolution>,
}

#[derive(Debug, Clone)]
struct RejectedIntent {
    envelope: ActionEnvelope,
    reason: RejectReason,
}

impl WorldKernel {
    pub fn submit_action(&mut self, action: Action) -> ActionId {
        self.submit_action_from_system(action)
    }

    pub fn submit_action_from_system(&mut self, action: Action) -> ActionId {
        self.submit_action_with_submitter(action, ActionSubmitter::System)
    }

    pub fn submit_action_from_agent(
        &mut self,
        agent_id: impl Into<String>,
        action: Action,
    ) -> ActionId {
        self.submit_action_with_submitter(
            action,
            ActionSubmitter::Agent {
                agent_id: agent_id.into(),
            },
        )
    }

    pub fn submit_action_from_player(
        &mut self,
        player_id: impl Into<String>,
        action: Action,
    ) -> ActionId {
        self.submit_action_with_submitter(
            action,
            ActionSubmitter::Player {
                player_id: player_id.into(),
            },
        )
    }

    fn submit_action_with_submitter(
        &mut self,
        action: Action,
        submitter: ActionSubmitter,
    ) -> ActionId {
        let id = self.next_action_id;
        self.next_action_id = self.next_action_id.saturating_add(1);
        self.pending_actions.push_back(ActionEnvelope {
            id,
            action,
            submitter,
        });
        id
    }

    pub fn pending_actions(&self) -> usize {
        self.pending_actions.len()
    }

    pub fn step(&mut self) -> Option<WorldEvent> {
        let envelope = self.pending_actions.pop_front()?;
        let rule_context_time = self.time;
        let event_time = self.time.saturating_add(1);
        self.last_intent_batch_report = None;
        let event = self.execute_action_envelope_at_time(envelope, rule_context_time, event_time);
        self.maybe_replenish_fragments();
        self.maintain_social_lifecycle();
        Some(event)
    }

    pub fn step_intents_batch(&mut self) -> Vec<WorldEvent> {
        let pending: Vec<ActionEnvelope> = self.pending_actions.drain(..).collect();
        self.step_intents_batch_from_envelopes(pending)
    }

    pub fn step_intents_batch_from_envelopes(
        &mut self,
        mut intents: Vec<ActionEnvelope>,
    ) -> Vec<WorldEvent> {
        if intents.is_empty() {
            self.last_intent_batch_report = None;
            return Vec::new();
        }
        intents.sort_by_key(|envelope| envelope.id);
        let batch_hash = compute_intent_batch_hash(&intents);

        let rule_context_time = self.time;
        let tick = self.time.saturating_add(1);
        let (mut accepted, mut rejected, conflicts) = resolve_tick_intent_conflicts(intents);
        accepted.sort_by_key(|envelope| envelope.id);
        rejected.sort_by_key(|entry| entry.envelope.id);
        let accepted_action_ids: Vec<ActionId> =
            accepted.iter().map(|envelope| envelope.id).collect();
        let rejected_action_ids: Vec<ActionId> =
            rejected.iter().map(|entry| entry.envelope.id).collect();

        let mut events = Vec::new();
        self.time = tick;
        for rejected_intent in &rejected {
            let event = self.append_event_at_time(
                tick,
                WorldEventKind::ActionRejected {
                    reason: rejected_intent.reason.clone(),
                },
            );
            events.push(event);
        }
        for envelope in accepted {
            let event = self.execute_action_envelope_at_time(envelope, rule_context_time, tick);
            events.push(event);
        }
        self.time = tick;

        self.last_intent_batch_report = Some(IntentBatchReport {
            tick,
            batch_hash,
            intent_count: accepted_action_ids
                .len()
                .saturating_add(rejected_action_ids.len()),
            accepted_action_ids,
            rejected_action_ids,
            conflicts,
        });

        self.maybe_replenish_fragments();
        self.maintain_social_lifecycle();
        events
    }

    pub fn last_intent_batch_report(&self) -> Option<&IntentBatchReport> {
        self.last_intent_batch_report.as_ref()
    }

    fn merge_pre_action_rule_decisions<I>(
        &self,
        action_id: ActionId,
        decisions: I,
    ) -> KernelRuleDecision
    where
        I: IntoIterator<Item = KernelRuleDecision>,
    {
        match super::merge_kernel_rule_decisions(action_id, decisions) {
            Ok(merged) => merged,
            Err(err) => KernelRuleDecision::deny(action_id, vec![err.to_string()]),
        }
    }

    fn build_pre_action_wasm_rule_input(
        &self,
        action_id: ActionId,
        action: &Action,
    ) -> KernelRuleModuleInput {
        KernelRuleModuleInput {
            action_id,
            action: action.clone(),
            context: KernelRuleModuleContext {
                time: self.time,
                location_ids: self.model.locations.keys().cloned().collect(),
                agent_ids: self.model.agents.keys().cloned().collect(),
            },
        }
    }

    pub fn step_until_empty(&mut self) -> Vec<WorldEvent> {
        let mut events = Vec::new();
        while let Some(event) = self.step() {
            events.push(event);
        }
        events
    }

    fn execute_action_envelope_at_time(
        &mut self,
        envelope: ActionEnvelope,
        rule_context_time: WorldTime,
        event_time: WorldTime,
    ) -> WorldEvent {
        let action_id = envelope.id;
        let action = envelope.action;
        self.time = rule_context_time;

        if let Some(reason) = reject_reason_for_submitter(&envelope.submitter, &action) {
            self.time = event_time;
            return self
                .append_event_at_time(event_time, WorldEventKind::ActionRejected { reason });
        }

        let mut decisions = Vec::with_capacity(
            self.rule_hooks.pre_action.len()
                + usize::from(self.rule_hooks.pre_action_wasm.is_some()),
        );
        if let Some(evaluator) = &self.rule_hooks.pre_action_wasm {
            let input = self.build_pre_action_wasm_rule_input(action_id, &action);
            match evaluator(&input) {
                Ok(output) => decisions.push(output.decision),
                Err(err) => decisions.push(KernelRuleDecision::deny(
                    action_id,
                    vec![format!("wasm pre-action evaluator failed: {err}")],
                )),
            }
        }
        for hook in &self.rule_hooks.pre_action {
            decisions.push(hook(action_id, &action, self));
        }
        let merged_decision = self.merge_pre_action_rule_decisions(action_id, decisions);
        self.time = event_time;
        let kind = match merged_decision.verdict {
            KernelRuleVerdict::Deny => WorldEventKind::ActionRejected {
                reason: RejectReason::RuleDenied {
                    notes: merged_decision.notes,
                },
            },
            KernelRuleVerdict::Modify => match merged_decision.override_action {
                Some(override_action) => self.apply_action(action_id, override_action),
                None => WorldEventKind::ActionRejected {
                    reason: RejectReason::RuleDenied {
                        notes: vec![format!(
                            "rule decision missing override for action {action_id}"
                        )],
                    },
                },
            },
            KernelRuleVerdict::Allow => self.apply_action(action_id, action.clone()),
        };
        let event = self.append_event_at_time(event_time, kind);
        for hook in &self.rule_hooks.post_action {
            hook(action_id, &action, &event);
        }
        event
    }

    fn append_event_at_time(&mut self, event_time: WorldTime, kind: WorldEventKind) -> WorldEvent {
        let event = WorldEvent {
            id: self.next_event_id,
            time: event_time,
            kind,
        };
        self.next_event_id = self.next_event_id.saturating_add(1);
        self.journal.push(event.clone());
        event
    }
}

fn resolve_tick_intent_conflicts(
    intents: Vec<ActionEnvelope>,
) -> (
    Vec<ActionEnvelope>,
    Vec<RejectedIntent>,
    Vec<IntentConflictResolution>,
) {
    let mut grouped = BTreeMap::<String, Vec<ActionEnvelope>>::new();
    let mut accepted = Vec::new();
    for envelope in intents {
        if let Some(key) = intent_conflict_key(&envelope.action) {
            grouped.entry(key).or_default().push(envelope);
        } else {
            accepted.push(envelope);
        }
    }

    let mut rejected = Vec::new();
    let mut conflicts = Vec::new();
    for (conflict_key, mut group) in grouped {
        group.sort_by(intent_tie_break_cmp);
        let mut group = group.into_iter();
        let Some(winner) = group.next() else {
            continue;
        };
        let winner_action_id = winner.id;
        let winner_submitter = submitter_label(&winner.submitter);
        accepted.push(winner);
        let mut loser_action_ids = Vec::new();
        for loser in group {
            loser_action_ids.push(loser.id);
            rejected.push(RejectedIntent {
                envelope: loser,
                reason: RejectReason::RuleDenied {
                    notes: vec![format!(
                        "intent conflict on key={conflict_key}; winner_action_id={} winner_submitter={winner_submitter}",
                        winner_action_id
                    )],
                },
            });
        }
        if loser_action_ids.is_empty() {
            continue;
        }
        conflicts.push(IntentConflictResolution {
            conflict_key,
            winner_action_id,
            loser_action_ids,
        });
    }

    (accepted, rejected, conflicts)
}

fn intent_conflict_key(action: &Action) -> Option<String> {
    match action {
        Action::MoveAgent { to, .. } => Some(format!("move_to:{to}")),
        Action::HarvestRadiation { agent_id, .. } => Some(format!("harvest:{agent_id}")),
        Action::MineCompound { location_id, .. } => Some(format!("mine:{location_id}")),
        Action::BuildFactory { location_id, .. } => Some(format!("build_factory:{location_id}")),
        Action::ScheduleRecipe { factory_id, .. } => Some(format!("schedule_recipe:{factory_id}")),
        Action::ServiceMicroDepotRepair { facility_id, .. } => {
            Some(format!("service_micro_depot:{facility_id}"))
        }
        Action::ServiceMicroDepotLogistics { facility_id, .. } => {
            Some(format!("service_micro_depot:{facility_id}"))
        }
        Action::PayMicroDepotUpkeep { facility_id, .. }
        | Action::SuspendMicroDepot { facility_id, .. }
        | Action::ReclaimMicroDepot { facility_id, .. } => {
            Some(format!("lifecycle_micro_depot:{facility_id}"))
        }
        _ => None,
    }
}

fn intent_tie_break_cmp(left: &ActionEnvelope, right: &ActionEnvelope) -> std::cmp::Ordering {
    let left_key = submitter_sort_key(&left.submitter);
    let right_key = submitter_sort_key(&right.submitter);
    left_key
        .cmp(&right_key)
        .then_with(|| left.id.cmp(&right.id))
}

fn submitter_sort_key(submitter: &ActionSubmitter) -> (u8, &str) {
    match submitter {
        ActionSubmitter::System => (0, "system"),
        ActionSubmitter::Agent { agent_id } => (1, agent_id.as_str()),
        ActionSubmitter::Player { player_id } => (2, player_id.as_str()),
    }
}

fn submitter_label(submitter: &ActionSubmitter) -> String {
    match submitter {
        ActionSubmitter::System => "system".to_string(),
        ActionSubmitter::Agent { agent_id } => format!("agent:{agent_id}"),
        ActionSubmitter::Player { player_id } => format!("player:{player_id}"),
    }
}

fn compute_intent_batch_hash(intents: &[ActionEnvelope]) -> String {
    #[derive(Serialize)]
    struct IntentBatchHashEntry<'a> {
        id: ActionId,
        submitter: &'a ActionSubmitter,
        action: &'a Action,
    }

    let payload: Vec<IntentBatchHashEntry<'_>> = intents
        .iter()
        .map(|envelope| IntentBatchHashEntry {
            id: envelope.id,
            submitter: &envelope.submitter,
            action: &envelope.action,
        })
        .collect();
    match super::to_canonical_cbor(&payload) {
        Ok(bytes) => blake3::hash(&bytes).to_hex().to_string(),
        Err(_) => String::new(),
    }
}

fn reject_reason_for_submitter(
    submitter: &ActionSubmitter,
    action: &Action,
) -> Option<RejectReason> {
    match submitter {
        ActionSubmitter::System => None,
        ActionSubmitter::Player { player_id } => Some(RejectReason::RuleDenied {
            notes: vec![format!(
                "player {} cannot submit world actions directly; use prompt/chat indirect control",
                player_id
            )],
        }),
        ActionSubmitter::Agent { agent_id } => reject_reason_for_agent_submitter(agent_id, action),
    }
}

fn reject_reason_for_agent_submitter(agent_id: &str, action: &Action) -> Option<RejectReason> {
    let denied = |detail: &str| -> Option<RejectReason> {
        Some(RejectReason::RuleDenied {
            notes: vec![format!(
                "agent submitter {} cannot submit action: {}",
                agent_id, detail
            )],
        })
    };

    match action {
        Action::RegisterLocation { .. }
        | Action::RegisterAgent { .. }
        | Action::RegisterPowerPlant { .. }
        | Action::UpsertModuleVisualEntity { .. }
        | Action::RemoveModuleVisualEntity { .. }
        | Action::DebugGrantResource { .. } => denied("system-only action"),
        Action::MoveAgent {
            agent_id: action_agent_id,
            ..
        }
        | Action::SpeakToNearby {
            agent_id: action_agent_id,
            ..
        }
        | Action::InspectTarget {
            agent_id: action_agent_id,
            ..
        }
        | Action::SimpleInteract {
            agent_id: action_agent_id,
            ..
        }
        | Action::HarvestRadiation {
            agent_id: action_agent_id,
            ..
        }
        | Action::ServiceMicroDepotRepair {
            agent_id: action_agent_id,
            ..
        }
        | Action::ServiceMicroDepotLogistics {
            agent_id: action_agent_id,
            ..
        }
        | Action::PayMicroDepotUpkeep {
            agent_id: action_agent_id,
            ..
        }
        | Action::SuspendMicroDepot {
            agent_id: action_agent_id,
            ..
        }
        | Action::ReclaimMicroDepot {
            agent_id: action_agent_id,
            ..
        } => {
            if action_agent_id == agent_id {
                None
            } else {
                denied("action agent_id mismatch")
            }
        }
        Action::MineCompound { owner, .. }
        | Action::RefineCompound { owner, .. }
        | Action::BuildFactory { owner, .. }
        | Action::ScheduleRecipe { owner, .. }
        | Action::PlacePowerOrder { owner, .. }
        | Action::CancelPowerOrder { owner, .. }
        | Action::PublishSocialFact { actor: owner, .. }
        | Action::RevokeSocialFact { actor: owner, .. } => {
            if resource_owner_is_agent(owner, agent_id) {
                None
            } else {
                denied("owner must be the submitter agent")
            }
        }
        Action::ChallengeSocialFact { challenger, .. }
        | Action::AdjudicateSocialFact {
            adjudicator: challenger,
            ..
        }
        | Action::DeclareSocialEdge {
            declarer: challenger,
            ..
        } => {
            if resource_owner_is_agent(challenger, agent_id) {
                None
            } else {
                denied("social actor must be the submitter agent")
            }
        }
        Action::FormAlliance {
            proposer_agent_id, ..
        }
        | Action::OpenGovernanceProposal {
            proposer_agent_id, ..
        } => {
            if proposer_agent_id == agent_id {
                None
            } else {
                denied("proposer_agent_id must be the submitter agent")
            }
        }
        Action::JoinAlliance {
            operator_agent_id, ..
        }
        | Action::LeaveAlliance {
            operator_agent_id, ..
        }
        | Action::DissolveAlliance {
            operator_agent_id, ..
        } => {
            if operator_agent_id == agent_id {
                None
            } else {
                denied("operator_agent_id must be the submitter agent")
            }
        }
        Action::DeclareWar {
            initiator_agent_id, ..
        } => {
            if initiator_agent_id == agent_id {
                None
            } else {
                denied("initiator_agent_id must be the submitter agent")
            }
        }
        Action::CastGovernanceVote { voter_agent_id, .. } => {
            if voter_agent_id == agent_id {
                None
            } else {
                denied("voter_agent_id must be the submitter agent")
            }
        }
        Action::ResolveCrisis {
            resolver_agent_id, ..
        } => {
            if resolver_agent_id == agent_id {
                None
            } else {
                denied("resolver_agent_id must be the submitter agent")
            }
        }
        Action::GrantMetaProgress {
            operator_agent_id, ..
        } => {
            if operator_agent_id == agent_id {
                None
            } else {
                denied("operator_agent_id must be the submitter agent")
            }
        }
        Action::OpenEconomicContract {
            creator_agent_id, ..
        } => {
            if creator_agent_id == agent_id {
                None
            } else {
                denied("creator_agent_id must be the submitter agent")
            }
        }
        Action::AcceptEconomicContract {
            accepter_agent_id, ..
        } => {
            if accepter_agent_id == agent_id {
                None
            } else {
                denied("accepter_agent_id must be the submitter agent")
            }
        }
        Action::SettleEconomicContract {
            operator_agent_id, ..
        } => {
            if operator_agent_id == agent_id {
                None
            } else {
                denied("operator_agent_id must be the submitter agent")
            }
        }
        Action::TransferResource { from, .. } => {
            if resource_owner_is_agent(from, agent_id) {
                None
            } else {
                denied("from owner must be the submitter agent")
            }
        }
        Action::BuyPower { buyer, .. } => {
            if resource_owner_is_agent(buyer, agent_id) {
                None
            } else {
                denied("buyer must be the submitter agent")
            }
        }
        Action::SellPower { seller, .. } => {
            if resource_owner_is_agent(seller, agent_id) {
                None
            } else {
                denied("seller must be the submitter agent")
            }
        }
        Action::CompileModuleArtifactFromSource {
            publisher_agent_id, ..
        }
        | Action::DeployModuleArtifact {
            publisher_agent_id, ..
        } => {
            if publisher_agent_id == agent_id {
                None
            } else {
                denied("publisher_agent_id must be the submitter agent")
            }
        }
        Action::InstallModuleFromArtifact {
            installer_agent_id, ..
        }
        | Action::InstallModuleToTargetFromArtifact {
            installer_agent_id, ..
        }
        | Action::InstallMicroDepot {
            installer_agent_id, ..
        } => {
            if installer_agent_id == agent_id {
                None
            } else {
                denied("installer_agent_id must be the submitter agent")
            }
        }
        Action::ListModuleArtifactForSale {
            seller_agent_id, ..
        }
        | Action::DelistModuleArtifact {
            seller_agent_id, ..
        } => {
            if seller_agent_id == agent_id {
                None
            } else {
                denied("seller_agent_id must be the submitter agent")
            }
        }
        Action::BuyModuleArtifact { buyer_agent_id, .. } => {
            if buyer_agent_id == agent_id {
                None
            } else {
                denied("buyer_agent_id must be the submitter agent")
            }
        }
        Action::DestroyModuleArtifact { owner_agent_id, .. } => {
            if owner_agent_id == agent_id {
                None
            } else {
                denied("owner_agent_id must be the submitter agent")
            }
        }
        Action::PlaceModuleArtifactBid {
            bidder_agent_id, ..
        }
        | Action::CancelModuleArtifactBid {
            bidder_agent_id, ..
        } => {
            if bidder_agent_id == agent_id {
                None
            } else {
                denied("bidder_agent_id must be the submitter agent")
            }
        }
    }
}

fn resource_owner_is_agent(owner: &ResourceOwner, agent_id: &str) -> bool {
    matches!(owner, ResourceOwner::Agent { agent_id: value } if value == agent_id)
}
