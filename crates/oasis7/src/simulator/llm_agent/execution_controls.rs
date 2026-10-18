use super::super::agent::{ActionResult, AgentDecision};
use super::decision_flow::{
    ExecuteUntilCondition, ExecuteUntilDirective, ExecuteUntilEventKind,
    summarize_execute_until_conditions,
};
use oasis7_simulator::Action;
use oasis7_simulator::{Observation, RejectReason, WorldEventKind};

#[derive(Debug, Clone, Default)]
pub(super) struct ActionReplanGuardState {
    last_action_signature: Option<String>,
    consecutive_same_action: usize,
}

impl ActionReplanGuardState {
    pub(super) fn record_decision(&mut self, decision: &AgentDecision) {
        let signature = decision_action_signature(decision);
        match signature {
            Some(signature) => {
                if self.last_action_signature.as_deref() == Some(signature.as_str()) {
                    self.consecutive_same_action = self.consecutive_same_action.saturating_add(1);
                } else {
                    self.last_action_signature = Some(signature);
                    self.consecutive_same_action = 1;
                }
            }
            None => {
                self.last_action_signature = None;
                self.consecutive_same_action = 0;
            }
        }
    }

    pub(super) fn should_force_replan(&self, threshold: usize) -> bool {
        threshold > 0 && self.consecutive_same_action >= threshold
    }

    pub(super) fn guard_summary(&self, threshold: usize) -> Option<String> {
        let action = self.last_action_signature.as_ref()?;
        Some(format!(
            "consecutive_same_action={}; threshold={}; last_action={action}",
            self.consecutive_same_action, threshold,
        ))
    }

    pub(super) fn is_same_action_as_last(&self, action: &Action) -> bool {
        let signature = action_signature(action);
        self.last_action_signature.as_deref() == Some(signature.as_str())
    }

    pub(super) fn projected_consecutive_same_action(&self, action: &Action) -> usize {
        if self.is_same_action_as_last(action) {
            self.consecutive_same_action.saturating_add(1)
        } else {
            1
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct ActiveExecuteUntil {
    action: Action,
    until_conditions: Vec<ExecuteUntilCondition>,
    remaining_ticks: u64,
    baseline_visible_agents: usize,
    baseline_visible_locations: usize,
    target_location_id: Option<String>,
    last_action_failed: bool,
    last_action_rejected: bool,
    last_reject_reason: Option<RejectReason>,
    last_harvest_amount: Option<i64>,
    last_harvest_available: Option<i64>,
}

impl ActiveExecuteUntil {
    pub(super) fn from_auto_reentry(
        action: Action,
        observation: &Observation,
        max_ticks: u64,
    ) -> Self {
        let until_conditions = auto_reentry_until_conditions(&action);
        Self::from_directive(
            ExecuteUntilDirective {
                action,
                until_conditions,
                max_ticks: max_ticks.max(1),
            },
            observation,
        )
    }

    pub(super) fn from_directive(
        directive: ExecuteUntilDirective,
        observation: &Observation,
    ) -> Self {
        let target_location_id = match &directive.action {
            Action::MoveAgent { to, .. } => Some(to.clone()),
            _ => None,
        };

        Self {
            action: directive.action,
            until_conditions: directive.until_conditions,
            remaining_ticks: directive.max_ticks,
            baseline_visible_agents: observation.visible_agents.len(),
            baseline_visible_locations: observation.visible_locations.len(),
            target_location_id,
            last_action_failed: false,
            last_action_rejected: false,
            last_reject_reason: None,
            last_harvest_amount: None,
            last_harvest_available: None,
        }
    }

    pub(super) fn action(&self) -> &Action {
        &self.action
    }

    pub(super) fn until_events_summary(&self) -> String {
        summarize_execute_until_conditions(&self.until_conditions)
    }

    pub(super) fn remaining_ticks(&self) -> u64 {
        self.remaining_ticks
    }

    pub(super) fn update_from_action_result(&mut self, result: &ActionResult) {
        if !actions_same(self.action(), &result.action) {
            return;
        }

        self.last_action_failed = !result.success;
        self.last_action_rejected = false;
        self.last_reject_reason = None;
        self.last_harvest_amount = None;
        self.last_harvest_available = None;

        match &result.event.kind {
            WorldEventKind::ActionRejected { reason } => {
                self.last_action_rejected = true;
                self.last_reject_reason = Some(reason.clone());
            }
            WorldEventKind::RadiationHarvested {
                amount, available, ..
            } => {
                self.last_harvest_amount = Some(*amount);
                self.last_harvest_available = Some(*available);
            }
            _ => {}
        }
    }

    pub(super) fn evaluate_next_step(&mut self, observation: &Observation) -> Result<(), String> {
        for condition in &self.until_conditions {
            match condition.kind {
                ExecuteUntilEventKind::ActionRejected => {
                    if self.last_action_rejected {
                        return Err("until.event action_rejected matched".to_string());
                    }
                }
                ExecuteUntilEventKind::NewVisibleAgent => {
                    if observation.visible_agents.len() > self.baseline_visible_agents {
                        return Err(format!(
                            "until.event new_visible_agent matched: baseline={}, current={}",
                            self.baseline_visible_agents,
                            observation.visible_agents.len()
                        ));
                    }
                }
                ExecuteUntilEventKind::NewVisibleLocation => {
                    if observation.visible_locations.len() > self.baseline_visible_locations {
                        return Err(format!(
                            "until.event new_visible_location matched: baseline={}, current={}",
                            self.baseline_visible_locations,
                            observation.visible_locations.len()
                        ));
                    }
                }
                ExecuteUntilEventKind::ArriveTarget => {
                    if let Some(target_location_id) = self.target_location_id.as_ref() {
                        let arrived = observation.visible_locations.iter().any(|location| {
                            location.location_id == *target_location_id && location.distance_cm <= 0
                        });
                        if arrived {
                            return Err(format!(
                                "until.event arrive_target matched: {target_location_id}"
                            ));
                        }
                    }
                }
                ExecuteUntilEventKind::InsufficientElectricity => {
                    if self
                        .last_reject_reason
                        .as_ref()
                        .is_some_and(reject_reason_is_insufficient_electricity)
                    {
                        return Err("until.event insufficient_electricity matched".to_string());
                    }
                }
                ExecuteUntilEventKind::ThermalOverload => {
                    if self
                        .last_reject_reason
                        .as_ref()
                        .is_some_and(reject_reason_is_thermal_overload)
                    {
                        return Err("until.event thermal_overload matched".to_string());
                    }
                }
                ExecuteUntilEventKind::HarvestYieldBelow => {
                    if let (Some(amount), Some(value_lte)) =
                        (self.last_harvest_amount, condition.value_lte)
                        && amount <= value_lte
                    {
                        return Err(format!(
                            "until.event harvest_yield_below matched: amount={}, threshold={}",
                            amount, value_lte
                        ));
                    }
                }
                ExecuteUntilEventKind::HarvestAvailableBelow => {
                    if let (Some(available), Some(value_lte)) =
                        (self.last_harvest_available, condition.value_lte)
                        && available <= value_lte
                    {
                        return Err(format!(
                            "until.event harvest_available_below matched: available={}, threshold={}",
                            available, value_lte
                        ));
                    }
                }
            }
        }

        if self.last_action_failed {
            return Err("until plan stop: previous action failed".to_string());
        }

        if self.remaining_ticks == 0 {
            return Err("until plan stop: max_ticks reached".to_string());
        }

        self.remaining_ticks = self.remaining_ticks.saturating_sub(1);
        self.last_action_failed = false;
        self.last_action_rejected = false;
        self.last_reject_reason = None;
        self.last_harvest_amount = None;
        self.last_harvest_available = None;
        Ok(())
    }
}

fn auto_reentry_until_conditions(action: &Action) -> Vec<ExecuteUntilCondition> {
    match action {
        Action::MoveAgent { .. } => vec![
            ExecuteUntilCondition {
                kind: ExecuteUntilEventKind::ArriveTarget,
                value_lte: None,
            },
            ExecuteUntilCondition {
                kind: ExecuteUntilEventKind::ActionRejected,
                value_lte: None,
            },
            ExecuteUntilCondition {
                kind: ExecuteUntilEventKind::NewVisibleAgent,
                value_lte: None,
            },
            ExecuteUntilCondition {
                kind: ExecuteUntilEventKind::NewVisibleLocation,
                value_lte: None,
            },
        ],
        Action::HarvestRadiation { .. } => vec![
            ExecuteUntilCondition {
                kind: ExecuteUntilEventKind::ActionRejected,
                value_lte: None,
            },
            ExecuteUntilCondition {
                kind: ExecuteUntilEventKind::InsufficientElectricity,
                value_lte: None,
            },
            ExecuteUntilCondition {
                kind: ExecuteUntilEventKind::ThermalOverload,
                value_lte: None,
            },
            ExecuteUntilCondition {
                kind: ExecuteUntilEventKind::NewVisibleAgent,
                value_lte: None,
            },
            ExecuteUntilCondition {
                kind: ExecuteUntilEventKind::NewVisibleLocation,
                value_lte: None,
            },
        ],
        _ => vec![
            ExecuteUntilCondition {
                kind: ExecuteUntilEventKind::ActionRejected,
                value_lte: None,
            },
            ExecuteUntilCondition {
                kind: ExecuteUntilEventKind::NewVisibleAgent,
                value_lte: None,
            },
            ExecuteUntilCondition {
                kind: ExecuteUntilEventKind::NewVisibleLocation,
                value_lte: None,
            },
        ],
    }
}

pub(super) fn default_execute_until_conditions_for_action(
    action: &Action,
) -> Vec<ExecuteUntilCondition> {
    auto_reentry_until_conditions(action)
}

fn reject_reason_is_insufficient_electricity(reason: &RejectReason) -> bool {
    matches!(
        reason,
        RejectReason::InsufficientResource { kind, .. }
            if matches!(kind, oasis7_simulator::ResourceKind::Electricity)
    ) || matches!(reason, RejectReason::AgentShutdown { .. })
}

fn reject_reason_is_thermal_overload(reason: &RejectReason) -> bool {
    matches!(reason, RejectReason::ThermalOverload { .. })
}

fn decision_action_signature(decision: &AgentDecision) -> Option<String> {
    match decision {
        AgentDecision::Act(action) => Some(action_signature(action)),
        _ => None,
    }
}

fn action_signature(action: &Action) -> String {
    match action {
        Action::MoveAgent { to, .. } => format!("move_agent:{to}"),
        Action::HarvestRadiation { max_amount, .. } => {
            format!("harvest_radiation:{max_amount}")
        }
        Action::BuyPower { buyer, seller, .. } => {
            format!("buy_power:{buyer:?}:{seller:?}")
        }
        Action::SellPower { seller, buyer, .. } => {
            format!("sell_power:{seller:?}:{buyer:?}")
        }
        Action::PlacePowerOrder { owner, side, .. } => {
            format!("place_power_order:{owner:?}:{side:?}")
        }
        Action::CancelPowerOrder { owner, order_id } => {
            format!("cancel_power_order:{owner:?}:{order_id}")
        }
        Action::TransferResource { from, to, kind, .. } => {
            format!("transfer_resource:{from:?}:{to:?}:{kind:?}")
        }
        Action::BuildFactory {
            location_id,
            factory_id,
            factory_kind,
            ..
        } => format!("build_factory:{location_id}:{factory_id}:{factory_kind}"),
        Action::ScheduleRecipe {
            factory_id,
            recipe_id,
            batches,
            ..
        } => format!("schedule_recipe:{factory_id}:{recipe_id}:{batches}"),
        Action::CompileModuleArtifactFromSource {
            publisher_agent_id,
            module_id,
            manifest_path,
            source_files,
        } => format!(
            "compile_module_artifact_from_source:{publisher_agent_id}:{module_id}:{manifest_path}:files={}",
            source_files.len()
        ),
        Action::DeployModuleArtifact {
            publisher_agent_id,
            wasm_hash,
            module_id_hint,
            ..
        } => format!("deploy_module_artifact:{publisher_agent_id}:{wasm_hash}:{module_id_hint:?}"),
        Action::InstallModuleFromArtifact {
            installer_agent_id,
            module_id,
            module_version,
            wasm_hash,
            activate,
        } => format!(
            "install_module_from_artifact:{installer_agent_id}:{module_id}:{module_version}:{wasm_hash}:{activate}"
        ),
        Action::ListModuleArtifactForSale {
            seller_agent_id,
            wasm_hash,
            price_kind,
            price_amount,
        } => format!(
            "list_module_artifact_for_sale:{seller_agent_id}:{wasm_hash}:{price_kind:?}:{price_amount}"
        ),
        Action::BuyModuleArtifact {
            buyer_agent_id,
            wasm_hash,
        } => format!("buy_module_artifact:{buyer_agent_id}:{wasm_hash}"),
        Action::DelistModuleArtifact {
            seller_agent_id,
            wasm_hash,
        } => format!("delist_module_artifact:{seller_agent_id}:{wasm_hash}"),
        Action::DestroyModuleArtifact {
            owner_agent_id,
            wasm_hash,
            reason,
        } => format!("destroy_module_artifact:{owner_agent_id}:{wasm_hash}:{reason}"),
        Action::PlaceModuleArtifactBid {
            bidder_agent_id,
            wasm_hash,
            price_kind,
            price_amount,
        } => format!(
            "place_module_artifact_bid:{bidder_agent_id}:{wasm_hash}:{price_kind:?}:{price_amount}"
        ),
        Action::CancelModuleArtifactBid {
            bidder_agent_id,
            wasm_hash,
            bid_order_id,
        } => format!("cancel_module_artifact_bid:{bidder_agent_id}:{wasm_hash}:{bid_order_id}"),
        Action::PublishSocialFact {
            actor,
            schema_id,
            subject,
            object,
            claim,
            confidence_ppm,
            evidence_event_ids,
            ttl_ticks,
            stake,
        } => format!(
            "publish_social_fact:{actor:?}:{schema_id}:{subject:?}:{object:?}:{claim}:{confidence_ppm}:{evidence_event_ids:?}:{ttl_ticks:?}:{stake:?}"
        ),
        Action::ChallengeSocialFact {
            challenger,
            fact_id,
            reason,
            stake,
        } => format!("challenge_social_fact:{challenger:?}:{fact_id}:{reason}:{stake:?}"),
        Action::AdjudicateSocialFact {
            adjudicator,
            fact_id,
            decision,
            notes,
        } => format!("adjudicate_social_fact:{adjudicator:?}:{fact_id}:{decision:?}:{notes}"),
        Action::RevokeSocialFact {
            actor,
            fact_id,
            reason,
        } => format!("revoke_social_fact:{actor:?}:{fact_id}:{reason}"),
        Action::DeclareSocialEdge {
            declarer,
            schema_id,
            relation_kind,
            from,
            to,
            weight_bps,
            backing_fact_ids,
            ttl_ticks,
        } => format!(
            "declare_social_edge:{declarer:?}:{schema_id}:{relation_kind}:{from:?}:{to:?}:{weight_bps}:{backing_fact_ids:?}:{ttl_ticks:?}"
        ),
        Action::FormAlliance {
            proposer_agent_id,
            alliance_id,
            members,
            charter,
        } => format!("form_alliance:{proposer_agent_id}:{alliance_id}:{members:?}:{charter}"),
        Action::JoinAlliance {
            operator_agent_id,
            alliance_id,
            member_agent_id,
        } => format!("join_alliance:{operator_agent_id}:{alliance_id}:{member_agent_id}"),
        Action::LeaveAlliance {
            operator_agent_id,
            alliance_id,
            member_agent_id,
        } => format!("leave_alliance:{operator_agent_id}:{alliance_id}:{member_agent_id}"),
        Action::DissolveAlliance {
            operator_agent_id,
            alliance_id,
            reason,
        } => format!("dissolve_alliance:{operator_agent_id}:{alliance_id}:{reason}"),
        Action::DeclareWar {
            initiator_agent_id,
            war_id,
            aggressor_alliance_id,
            defender_alliance_id,
            objective,
            intensity,
        } => format!(
            "declare_war:{initiator_agent_id}:{war_id}:{aggressor_alliance_id}:{defender_alliance_id}:{objective}:{intensity}"
        ),
        Action::OpenGovernanceProposal {
            proposer_agent_id,
            proposal_key,
            title,
            description,
            options,
            voting_window_ticks,
            quorum_weight,
            pass_threshold_bps,
        } => format!(
            "open_governance_proposal:{proposer_agent_id}:{proposal_key}:{title}:{description}:{options:?}:{voting_window_ticks}:{quorum_weight}:{pass_threshold_bps}"
        ),
        Action::CastGovernanceVote {
            voter_agent_id,
            proposal_key,
            option,
            weight,
        } => format!("cast_governance_vote:{voter_agent_id}:{proposal_key}:{option}:{weight}"),
        Action::ResolveCrisis {
            resolver_agent_id,
            crisis_id,
            strategy,
            success,
        } => format!("resolve_crisis:{resolver_agent_id}:{crisis_id}:{strategy}:{success}"),
        Action::GrantMetaProgress {
            operator_agent_id,
            target_agent_id,
            track,
            points,
            achievement_id,
        } => format!(
            "grant_meta_progress:{operator_agent_id}:{target_agent_id}:{track}:{points}:{achievement_id:?}"
        ),
        Action::OpenEconomicContract {
            creator_agent_id,
            contract_id,
            counterparty_agent_id,
            settlement_kind,
            settlement_amount,
            reputation_stake,
            expires_at,
            description,
        } => format!(
            "open_economic_contract:{creator_agent_id}:{contract_id}:{counterparty_agent_id}:{settlement_kind:?}:{settlement_amount}:{reputation_stake}:{expires_at}:{description}"
        ),
        Action::AcceptEconomicContract {
            accepter_agent_id,
            contract_id,
        } => format!("accept_economic_contract:{accepter_agent_id}:{contract_id}"),
        Action::SettleEconomicContract {
            operator_agent_id,
            contract_id,
            success,
            notes,
        } => {
            format!("settle_economic_contract:{operator_agent_id}:{contract_id}:{success}:{notes}")
        }
        other => format!("other:{other:?}"),
    }
}

fn actions_same(left: &Action, right: &Action) -> bool {
    match (left, right) {
        (Action::MoveAgent { to: left_to, .. }, Action::MoveAgent { to: right_to, .. }) => {
            left_to == right_to
        }
        (Action::HarvestRadiation { .. }, Action::HarvestRadiation { .. }) => true,
        (
            Action::BuyPower {
                buyer: left_buyer,
                seller: left_seller,
                ..
            },
            Action::BuyPower {
                buyer: right_buyer,
                seller: right_seller,
                ..
            },
        ) => left_buyer == right_buyer && left_seller == right_seller,
        (
            Action::SellPower {
                seller: left_seller,
                buyer: left_buyer,
                ..
            },
            Action::SellPower {
                seller: right_seller,
                buyer: right_buyer,
                ..
            },
        ) => left_seller == right_seller && left_buyer == right_buyer,
        (
            Action::PlacePowerOrder {
                owner: left_owner,
                side: left_side,
                ..
            },
            Action::PlacePowerOrder {
                owner: right_owner,
                side: right_side,
                ..
            },
        ) => left_owner == right_owner && left_side == right_side,
        (
            Action::CancelPowerOrder {
                owner: left_owner,
                order_id: left_order_id,
            },
            Action::CancelPowerOrder {
                owner: right_owner,
                order_id: right_order_id,
            },
        ) => left_owner == right_owner && left_order_id == right_order_id,
        (
            Action::TransferResource {
                from: left_from,
                to: left_to,
                kind: left_kind,
                ..
            },
            Action::TransferResource {
                from: right_from,
                to: right_to,
                kind: right_kind,
                ..
            },
        ) => left_from == right_from && left_to == right_to && left_kind == right_kind,
        (
            Action::BuildFactory {
                location_id: left_location_id,
                factory_id: left_factory_id,
                factory_kind: left_factory_kind,
                ..
            },
            Action::BuildFactory {
                location_id: right_location_id,
                factory_id: right_factory_id,
                factory_kind: right_factory_kind,
                ..
            },
        ) => {
            left_location_id == right_location_id
                && left_factory_id == right_factory_id
                && left_factory_kind == right_factory_kind
        }
        (
            Action::ScheduleRecipe {
                factory_id: left_factory_id,
                recipe_id: left_recipe_id,
                ..
            },
            Action::ScheduleRecipe {
                factory_id: right_factory_id,
                recipe_id: right_recipe_id,
                ..
            },
        ) => left_factory_id == right_factory_id && left_recipe_id == right_recipe_id,
        (
            Action::CompileModuleArtifactFromSource {
                publisher_agent_id: left_publisher,
                module_id: left_module_id,
                manifest_path: left_manifest_path,
                ..
            },
            Action::CompileModuleArtifactFromSource {
                publisher_agent_id: right_publisher,
                module_id: right_module_id,
                manifest_path: right_manifest_path,
                ..
            },
        ) => {
            left_publisher == right_publisher
                && left_module_id == right_module_id
                && left_manifest_path == right_manifest_path
        }
        (
            Action::DeployModuleArtifact {
                publisher_agent_id: left_publisher,
                wasm_hash: left_wasm_hash,
                ..
            },
            Action::DeployModuleArtifact {
                publisher_agent_id: right_publisher,
                wasm_hash: right_wasm_hash,
                ..
            },
        ) => left_publisher == right_publisher && left_wasm_hash == right_wasm_hash,
        (
            Action::InstallModuleFromArtifact {
                installer_agent_id: left_installer,
                module_id: left_module_id,
                module_version: left_module_version,
                wasm_hash: left_wasm_hash,
                ..
            },
            Action::InstallModuleFromArtifact {
                installer_agent_id: right_installer,
                module_id: right_module_id,
                module_version: right_module_version,
                wasm_hash: right_wasm_hash,
                ..
            },
        ) => {
            left_installer == right_installer
                && left_module_id == right_module_id
                && left_module_version == right_module_version
                && left_wasm_hash == right_wasm_hash
        }
        (
            Action::ListModuleArtifactForSale {
                seller_agent_id: left_seller,
                wasm_hash: left_wasm_hash,
                price_kind: left_price_kind,
                ..
            },
            Action::ListModuleArtifactForSale {
                seller_agent_id: right_seller,
                wasm_hash: right_wasm_hash,
                price_kind: right_price_kind,
                ..
            },
        ) => {
            left_seller == right_seller
                && left_wasm_hash == right_wasm_hash
                && left_price_kind == right_price_kind
        }
        (
            Action::BuyModuleArtifact {
                buyer_agent_id: left_buyer,
                wasm_hash: left_wasm_hash,
            },
            Action::BuyModuleArtifact {
                buyer_agent_id: right_buyer,
                wasm_hash: right_wasm_hash,
            },
        ) => left_buyer == right_buyer && left_wasm_hash == right_wasm_hash,
        (
            Action::DelistModuleArtifact {
                seller_agent_id: left_seller,
                wasm_hash: left_wasm_hash,
            },
            Action::DelistModuleArtifact {
                seller_agent_id: right_seller,
                wasm_hash: right_wasm_hash,
            },
        ) => left_seller == right_seller && left_wasm_hash == right_wasm_hash,
        (
            Action::DestroyModuleArtifact {
                owner_agent_id: left_owner,
                wasm_hash: left_wasm_hash,
                ..
            },
            Action::DestroyModuleArtifact {
                owner_agent_id: right_owner,
                wasm_hash: right_wasm_hash,
                ..
            },
        ) => left_owner == right_owner && left_wasm_hash == right_wasm_hash,
        (
            Action::PlaceModuleArtifactBid {
                bidder_agent_id: left_bidder,
                wasm_hash: left_wasm_hash,
                price_kind: left_price_kind,
                ..
            },
            Action::PlaceModuleArtifactBid {
                bidder_agent_id: right_bidder,
                wasm_hash: right_wasm_hash,
                price_kind: right_price_kind,
                ..
            },
        ) => {
            left_bidder == right_bidder
                && left_wasm_hash == right_wasm_hash
                && left_price_kind == right_price_kind
        }
        (
            Action::CancelModuleArtifactBid {
                bidder_agent_id: left_bidder,
                wasm_hash: left_wasm_hash,
                bid_order_id: left_order_id,
            },
            Action::CancelModuleArtifactBid {
                bidder_agent_id: right_bidder,
                wasm_hash: right_wasm_hash,
                bid_order_id: right_order_id,
            },
        ) => {
            left_bidder == right_bidder
                && left_wasm_hash == right_wasm_hash
                && left_order_id == right_order_id
        }
        (
            Action::MineCompound {
                owner: left_owner,
                location_id: left_location_id,
                ..
            },
            Action::MineCompound {
                owner: right_owner,
                location_id: right_location_id,
                ..
            },
        ) => left_owner == right_owner && left_location_id == right_location_id,
        (
            Action::RefineCompound {
                owner: left_owner, ..
            },
            Action::RefineCompound {
                owner: right_owner, ..
            },
        ) => left_owner == right_owner,
        (
            Action::PublishSocialFact {
                actor: left_actor,
                schema_id: left_schema_id,
                subject: left_subject,
                object: left_object,
                claim: left_claim,
                ..
            },
            Action::PublishSocialFact {
                actor: right_actor,
                schema_id: right_schema_id,
                subject: right_subject,
                object: right_object,
                claim: right_claim,
                ..
            },
        ) => {
            left_actor == right_actor
                && left_schema_id == right_schema_id
                && left_subject == right_subject
                && left_object == right_object
                && left_claim == right_claim
        }
        (
            Action::ChallengeSocialFact {
                challenger: left_challenger,
                fact_id: left_fact_id,
                ..
            },
            Action::ChallengeSocialFact {
                challenger: right_challenger,
                fact_id: right_fact_id,
                ..
            },
        ) => left_challenger == right_challenger && left_fact_id == right_fact_id,
        (
            Action::AdjudicateSocialFact {
                adjudicator: left_adjudicator,
                fact_id: left_fact_id,
                decision: left_decision,
                ..
            },
            Action::AdjudicateSocialFact {
                adjudicator: right_adjudicator,
                fact_id: right_fact_id,
                decision: right_decision,
                ..
            },
        ) => {
            left_adjudicator == right_adjudicator
                && left_fact_id == right_fact_id
                && left_decision == right_decision
        }
        (
            Action::RevokeSocialFact {
                actor: left_actor,
                fact_id: left_fact_id,
                ..
            },
            Action::RevokeSocialFact {
                actor: right_actor,
                fact_id: right_fact_id,
                ..
            },
        ) => left_actor == right_actor && left_fact_id == right_fact_id,
        (
            Action::DeclareSocialEdge {
                declarer: left_declarer,
                schema_id: left_schema_id,
                relation_kind: left_relation_kind,
                from: left_from,
                to: left_to,
                ..
            },
            Action::DeclareSocialEdge {
                declarer: right_declarer,
                schema_id: right_schema_id,
                relation_kind: right_relation_kind,
                from: right_from,
                to: right_to,
                ..
            },
        ) => {
            left_declarer == right_declarer
                && left_schema_id == right_schema_id
                && left_relation_kind == right_relation_kind
                && left_from == right_from
                && left_to == right_to
        }
        (
            Action::FormAlliance {
                proposer_agent_id: left_proposer,
                alliance_id: left_alliance_id,
                members: left_members,
                ..
            },
            Action::FormAlliance {
                proposer_agent_id: right_proposer,
                alliance_id: right_alliance_id,
                members: right_members,
                ..
            },
        ) => {
            left_proposer == right_proposer
                && left_alliance_id == right_alliance_id
                && left_members == right_members
        }
        (
            Action::JoinAlliance {
                operator_agent_id: left_operator,
                alliance_id: left_alliance_id,
                member_agent_id: left_member,
            },
            Action::JoinAlliance {
                operator_agent_id: right_operator,
                alliance_id: right_alliance_id,
                member_agent_id: right_member,
            },
        ) => {
            left_operator == right_operator
                && left_alliance_id == right_alliance_id
                && left_member == right_member
        }
        (
            Action::LeaveAlliance {
                operator_agent_id: left_operator,
                alliance_id: left_alliance_id,
                member_agent_id: left_member,
            },
            Action::LeaveAlliance {
                operator_agent_id: right_operator,
                alliance_id: right_alliance_id,
                member_agent_id: right_member,
            },
        ) => {
            left_operator == right_operator
                && left_alliance_id == right_alliance_id
                && left_member == right_member
        }
        (
            Action::DissolveAlliance {
                operator_agent_id: left_operator,
                alliance_id: left_alliance_id,
                ..
            },
            Action::DissolveAlliance {
                operator_agent_id: right_operator,
                alliance_id: right_alliance_id,
                ..
            },
        ) => left_operator == right_operator && left_alliance_id == right_alliance_id,
        (
            Action::DeclareWar {
                initiator_agent_id: left_initiator,
                war_id: left_war_id,
                aggressor_alliance_id: left_aggressor,
                defender_alliance_id: left_defender,
                ..
            },
            Action::DeclareWar {
                initiator_agent_id: right_initiator,
                war_id: right_war_id,
                aggressor_alliance_id: right_aggressor,
                defender_alliance_id: right_defender,
                ..
            },
        ) => {
            left_initiator == right_initiator
                && left_war_id == right_war_id
                && left_aggressor == right_aggressor
                && left_defender == right_defender
        }
        (
            Action::OpenGovernanceProposal {
                proposer_agent_id: left_proposer,
                proposal_key: left_key,
                ..
            },
            Action::OpenGovernanceProposal {
                proposer_agent_id: right_proposer,
                proposal_key: right_key,
                ..
            },
        ) => left_proposer == right_proposer && left_key == right_key,
        (
            Action::CastGovernanceVote {
                voter_agent_id: left_voter,
                proposal_key: left_key,
                option: left_option,
                ..
            },
            Action::CastGovernanceVote {
                voter_agent_id: right_voter,
                proposal_key: right_key,
                option: right_option,
                ..
            },
        ) => left_voter == right_voter && left_key == right_key && left_option == right_option,
        (
            Action::ResolveCrisis {
                resolver_agent_id: left_resolver,
                crisis_id: left_crisis_id,
                strategy: left_strategy,
                ..
            },
            Action::ResolveCrisis {
                resolver_agent_id: right_resolver,
                crisis_id: right_crisis_id,
                strategy: right_strategy,
                ..
            },
        ) => {
            left_resolver == right_resolver
                && left_crisis_id == right_crisis_id
                && left_strategy == right_strategy
        }
        (
            Action::GrantMetaProgress {
                operator_agent_id: left_operator,
                target_agent_id: left_target,
                track: left_track,
                ..
            },
            Action::GrantMetaProgress {
                operator_agent_id: right_operator,
                target_agent_id: right_target,
                track: right_track,
                ..
            },
        ) => {
            left_operator == right_operator
                && left_target == right_target
                && left_track == right_track
        }
        (
            Action::OpenEconomicContract {
                creator_agent_id: left_creator,
                contract_id: left_contract,
                counterparty_agent_id: left_counterparty,
                settlement_kind: left_kind,
                ..
            },
            Action::OpenEconomicContract {
                creator_agent_id: right_creator,
                contract_id: right_contract,
                counterparty_agent_id: right_counterparty,
                settlement_kind: right_kind,
                ..
            },
        ) => {
            left_creator == right_creator
                && left_contract == right_contract
                && left_counterparty == right_counterparty
                && left_kind == right_kind
        }
        (
            Action::AcceptEconomicContract {
                accepter_agent_id: left_accepter,
                contract_id: left_contract,
            },
            Action::AcceptEconomicContract {
                accepter_agent_id: right_accepter,
                contract_id: right_contract,
            },
        ) => left_accepter == right_accepter && left_contract == right_contract,
        (
            Action::SettleEconomicContract {
                operator_agent_id: left_operator,
                contract_id: left_contract,
                success: left_success,
                ..
            },
            Action::SettleEconomicContract {
                operator_agent_id: right_operator,
                contract_id: right_contract,
                success: right_success,
                ..
            },
        ) => {
            left_operator == right_operator
                && left_contract == right_contract
                && left_success == right_success
        }
        _ => false,
    }
}
