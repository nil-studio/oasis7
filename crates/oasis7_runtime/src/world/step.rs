use oasis7_wasm_abi::ModuleSandbox;

use super::super::{
    Action, ActionEnvelope, CausedBy, DomainEvent, FactoryProductionStatus,
    ModuleSubscriptionStage, RejectReason, RuleVerdict, WorldError, WorldEvent, WorldEventBody,
    WorldTime,
};
use super::World;
use super::economy::EconomyActionResolution;
use super::logistics::logistics_path_id;
use crate::EconomicContractFulfillmentKind;
use crate::RuntimeCommittedTickContext;
use crate::events::LogisticsOwnerPayout;
use oasis7_agent_api::world_action::ResourceKind;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default)]
struct FactoryProductionFollowupContext {
    active_jobs: u16,
    current_status: Option<FactoryProductionStatus>,
    blocked_at: Option<WorldTime>,
    blocker_kind: Option<String>,
    blocker_detail: Option<String>,
}

impl World {
    fn append_logistics_reroute_receipt(
        &mut self,
        action: &Action,
        event_body: &WorldEventBody,
    ) -> Result<Option<WorldEvent>, WorldError> {
        let Action::TransferMaterial {
            requester_agent_id,
            route_id,
            route_ids,
            ..
        } = action
        else {
            return Ok(None);
        };
        let WorldEventBody::Domain(DomainEvent::MaterialTransitStarted {
            job_id,
            amount,
            path_id: effective_path_id,
            route_ids: effective_route_ids,
            tariff_electricity_total,
            reroute_count,
            ..
        }) = event_body
        else {
            return Ok(None);
        };
        if *reroute_count == 0 {
            return Ok(None);
        }
        let mut original_route_ids = route_ids.clone();
        if original_route_ids.is_empty()
            && let Some(route_id) = route_id
        {
            original_route_ids.push(route_id.clone());
        }
        let original_path_id = logistics_path_id(&original_route_ids);
        let mut payout_map = BTreeMap::<String, i64>::new();
        for route_id in effective_route_ids {
            if let Some(route) = self.state.logistics_routes.get(route_id) {
                let payout = route
                    .tariff_electricity_per_unit
                    .saturating_mul(*amount)
                    .max(0);
                if payout > 0 {
                    let owner_payout = payout_map.entry(route.owner_agent_id.clone()).or_default();
                    *owner_payout = owner_payout.saturating_add(payout);
                }
            }
        }
        let owner_payouts = payout_map
            .into_iter()
            .map(|(owner_agent_id, electricity)| LogisticsOwnerPayout {
                owner_agent_id,
                electricity,
            })
            .collect();
        let receipt = WorldEventBody::Domain(DomainEvent::LogisticsPathRerouted {
            requester_agent_id: requester_agent_id.clone(),
            job_id: *job_id,
            original_path_id,
            original_route_ids,
            effective_path_id: effective_path_id.clone(),
            effective_route_ids: effective_route_ids.clone(),
            reason: "requested_path_unavailable_or_capacity_conflicted".to_string(),
            reroute_count: *reroute_count,
            tariff_electricity_total: *tariff_electricity_total,
            owner_payouts,
            governance_tax_electricity: 0,
        });
        self.append_event(receipt, Some(CausedBy::Action(*job_id)))?;
        Ok(self.journal.events.last().cloned())
    }

    fn should_emit_action_accepted(action: &Action) -> bool {
        if matches!(
            action,
            Action::OpenEconomicContract {
                fulfillment_kind: EconomicContractFulfillmentKind::Service,
                ..
            } | Action::SettleEconomicContract { success: false, .. }
        ) {
            return false;
        }
        matches!(
            action,
            Action::FormAlliance { .. }
                | Action::JoinAlliance { .. }
                | Action::LeaveAlliance { .. }
                | Action::DissolveAlliance { .. }
                | Action::DeclareWar { .. }
                | Action::OpenGovernanceProposal { .. }
                | Action::CastGovernanceVote { .. }
                | Action::ResolveCrisis { .. }
                | Action::GrantMetaProgress { .. }
                | Action::UpdateGameplayPolicy { .. }
                | Action::UpdateRestrictedStarterClaimAdminRegistry { .. }
                | Action::SubmitGovernanceValidatorAdmission { .. }
                | Action::ApproveGovernanceValidatorAdmission { .. }
                | Action::ActivateGovernanceValidatorAdmission { .. }
                | Action::RevokeGovernanceValidatorAdmission { .. }
                | Action::OpenEconomicContract { .. }
                | Action::AcceptEconomicContract { .. }
                | Action::SettleEconomicContract { .. }
                | Action::ClaimAgent { .. }
                | Action::ReleaseAgentClaim { .. }
                | Action::IssueRestrictedStarterClaimGrant { .. }
                | Action::RevokeRestrictedStarterClaimGrant { .. }
        )
    }

    fn append_action_accepted_event(
        &mut self,
        envelope: &ActionEnvelope,
    ) -> Result<(), WorldError> {
        if !Self::should_emit_action_accepted(&envelope.action) {
            return Ok(());
        }
        let actor_id = envelope.action.actor_id().unwrap_or("system");
        self.append_event(
            WorldEventBody::Domain(DomainEvent::ActionAccepted {
                action_id: envelope.id,
                action_kind: super::module_runtime_labels::action_kind_label(&envelope.action)
                    .to_string(),
                actor_id: actor_id.to_string(),
                eta_ticks: 0,
                notes: vec!["accepted_for_gameplay_processing".to_string()],
            }),
            Some(CausedBy::Action(envelope.id)),
        )?;
        Ok(())
    }

    pub(super) fn preflight_domain_event(&self, body: &WorldEventBody) -> Result<(), WorldError> {
        let WorldEventBody::Domain(event) = body else {
            return Ok(());
        };
        if matches!(
            event,
            DomainEvent::ActionAccepted { .. }
                | DomainEvent::ActionRejected { .. }
                | DomainEvent::Observation { .. }
        ) {
            return Ok(());
        }
        if matches!(
            event,
            DomainEvent::GovernanceProposalOpened { .. }
                | DomainEvent::GovernanceVoteCast { .. }
                | DomainEvent::GovernanceProposalFinalized { .. }
                | DomainEvent::CrisisSpawned { .. }
                | DomainEvent::CrisisResolved { .. }
                | DomainEvent::CrisisTimedOut { .. }
                | DomainEvent::MetaProgressGranted { .. }
                | DomainEvent::ProductValidated { .. }
        ) {
            super::governance_meta_publication::PreparedGovernanceMetaEvent::prepare(
                &self.state,
                event,
                self.state.time,
            )?;
            return Ok(());
        }
        if matches!(
            event,
            DomainEvent::AllianceFormed { .. }
                | DomainEvent::AllianceJoined { .. }
                | DomainEvent::AllianceLeft { .. }
                | DomainEvent::AllianceDissolved { .. }
                | DomainEvent::WarDeclared { .. }
                | DomainEvent::WarConcluded { .. }
        ) {
            super::alliance_war_publication::PreparedAllianceWarEvent::prepare(
                &self.state,
                event,
                self.state.time,
            )?;
            return Ok(());
        }
        if matches!(
            event,
            DomainEvent::EconomicContractOpened { .. }
                | DomainEvent::EconomicContractAccepted { .. }
                | DomainEvent::EconomicContractSettled { .. }
                | DomainEvent::EconomicContractExpired { .. }
        ) {
            super::economic_contract_publication::PreparedEconomicContractEvent::prepare(
                &self.state,
                event,
                self.state.time,
            )?;
            return Ok(());
        }
        let mut preview_state = self.state.clone();
        preview_state.apply_domain_event(event, self.state.time)
    }

    fn factory_production_followup_context(
        &self,
        envelope: &ActionEnvelope,
    ) -> Option<FactoryProductionFollowupContext> {
        let factory_id = match &envelope.action {
            Action::ScheduleRecipe { factory_id, .. }
            | Action::ScheduleRecipeWithModule { factory_id, .. } => factory_id,
            _ => return None,
        };
        self.state
            .factories
            .get(factory_id)
            .map(|factory| FactoryProductionFollowupContext {
                active_jobs: factory.production.active_jobs,
                current_status: Some(factory.production.status),
                blocked_at: factory.production.last_blocked_at,
                blocker_kind: factory.production.current_blocker_kind.clone(),
                blocker_detail: factory.production.current_blocker_detail.clone(),
            })
    }

    fn classify_factory_production_block(reason: &RejectReason) -> Option<(String, String)> {
        match reason {
            RejectReason::InsufficientMaterial { material_kind, .. } => Some((
                "material_shortage".to_string(),
                format!("material_shortage:{material_kind}"),
            )),
            RejectReason::InsufficientResource { kind, .. }
                if *kind == ResourceKind::Electricity =>
            {
                Some((
                    "power_shortage".to_string(),
                    "power_shortage:electricity".to_string(),
                ))
            }
            RejectReason::InsufficientResources { deficits }
                if deficits.contains_key(&ResourceKind::Electricity) =>
            {
                Some((
                    "power_shortage".to_string(),
                    "power_shortage:electricity".to_string(),
                ))
            }
            RejectReason::RuleDenied { notes } => {
                let joined = notes.join(" | ");
                let lowercase = joined.to_ascii_lowercase();
                if lowercase.contains("economy module evaluation failed") {
                    return Some(("module_failure".to_string(), joined));
                }
                if lowercase.contains("stage gate denied")
                    || lowercase.contains("unlock_stage denied")
                    || lowercase.contains("preferred_factory_tags mismatch")
                    || lowercase.contains("recipe module denied")
                    || lowercase.contains("recipe plan rejected")
                {
                    return Some(("governance_gate".to_string(), joined));
                }
                None
            }
            _ => None,
        }
    }

    fn derive_factory_production_followup_event(
        &self,
        envelope: &ActionEnvelope,
        event_body: &WorldEventBody,
        context: Option<&FactoryProductionFollowupContext>,
    ) -> Option<DomainEvent> {
        match (&envelope.action, event_body) {
            (
                Action::ScheduleRecipe {
                    requester_agent_id,
                    factory_id,
                    recipe_id,
                    ..
                }
                | Action::ScheduleRecipeWithModule {
                    requester_agent_id,
                    factory_id,
                    recipe_id,
                    ..
                },
                WorldEventBody::Domain(DomainEvent::ActionRejected { action_id, reason }),
            ) => {
                let context = context?;
                if context.active_jobs > 0 {
                    return None;
                }
                let (blocker_kind, blocker_detail) =
                    Self::classify_factory_production_block(reason)?;
                Some(DomainEvent::FactoryProductionBlocked {
                    action_id: *action_id,
                    requester_agent_id: requester_agent_id.clone(),
                    factory_id: factory_id.clone(),
                    recipe_id: recipe_id.clone(),
                    blocker_kind,
                    blocker_detail,
                })
            }
            (
                Action::ScheduleRecipe {
                    requester_agent_id, ..
                }
                | Action::ScheduleRecipeWithModule {
                    requester_agent_id, ..
                },
                WorldEventBody::Domain(DomainEvent::RecipeStarted {
                    job_id,
                    factory_id,
                    recipe_id,
                    ..
                }),
            ) => {
                let context = context?;
                if !matches!(
                    context.current_status,
                    Some(FactoryProductionStatus::Blocked | FactoryProductionStatus::Paused)
                ) {
                    return None;
                }
                Some(DomainEvent::FactoryProductionResumed {
                    job_id: *job_id,
                    requester_agent_id: requester_agent_id.clone(),
                    factory_id: factory_id.clone(),
                    recipe_id: recipe_id.clone(),
                    previous_blocked_at: context.blocked_at,
                    previous_blocker_kind: context.blocker_kind.clone(),
                    previous_blocker_detail: context.blocker_detail.clone(),
                })
            }
            _ => None,
        }
    }

    fn append_factory_production_followup_event(
        &mut self,
        envelope: &ActionEnvelope,
        event_body: &WorldEventBody,
        context: Option<&FactoryProductionFollowupContext>,
    ) -> Result<Option<WorldEvent>, WorldError> {
        let Some(event) =
            self.derive_factory_production_followup_event(envelope, event_body, context)
        else {
            return Ok(None);
        };
        let body = WorldEventBody::Domain(event);
        self.append_event(body, Some(CausedBy::Action(envelope.id)))?;
        Ok(self.journal.events.last().cloned())
    }

    // ---------------------------------------------------------------------
    // Simulation step
    // ---------------------------------------------------------------------

    pub fn step(&mut self) -> Result<(), WorldError> {
        self.reject_ordinary_step_during_formal_execution()?;
        let mut staged = self.clone();
        staged.step_inner()?;
        *self = staged;
        Ok(())
    }

    fn step_inner(&mut self) -> Result<(), WorldError> {
        self.state.time = self.state.time.saturating_add(1);
        let _ = self.process_factory_depreciation()?;
        while let Some(envelope) = self.pending_actions.pop_front() {
            if self.try_apply_runtime_module_action(&envelope)? {
                continue;
            }
            let followup_context = self.factory_production_followup_context(&envelope);
            let event_body = self.action_to_event(&envelope)?;
            self.preflight_domain_event(&event_body)?;
            self.append_action_accepted_event(&envelope)?;
            self.append_event(event_body.clone(), Some(CausedBy::Action(envelope.id)))?;
            let _ = self.append_logistics_reroute_receipt(&envelope.action, &event_body)?;
            let _ = self.append_factory_production_followup_event(
                &envelope,
                &event_body,
                followup_context.as_ref(),
            )?;
        }
        let _ = self.process_due_economy_jobs()?;
        let _ = self.process_due_material_transits()?;
        let _ = self.process_gameplay_cycles()?;
        let _ = self.process_restricted_starter_claim_grant_epochs()?;
        let _ = self.process_agent_claim_epochs()?;
        self.refresh_threat_heatmap();
        self.record_tick_consensus()?;
        let _ = self.service_cognition_scheduler_tick(self.state.time)?;
        Ok(())
    }

    pub fn step_with_modules(&mut self, sandbox: &mut dyn ModuleSandbox) -> Result<(), WorldError> {
        self.reject_ordinary_step_during_formal_execution()?;
        let mut staged = self.clone();
        let mut no_product_validation_checkpoint = |_world: &World| Ok(());
        staged.step_with_modules_inner(sandbox, &mut no_product_validation_checkpoint)?;
        *self = staged;
        Ok(())
    }

    fn step_with_modules_inner(
        &mut self,
        sandbox: &mut dyn ModuleSandbox,
        product_validation_checkpoint: &mut dyn FnMut(&World) -> Result<(), WorldError>,
    ) -> Result<(), WorldError> {
        self.state.time = self.state.time.saturating_add(1);
        self.run_modules_for_current_tick(sandbox, None, false, product_validation_checkpoint)
    }

    pub fn step_with_modules_for_committed_height(
        &mut self,
        sandbox: &mut dyn ModuleSandbox,
        committed_height: u64,
    ) -> Result<(), WorldError> {
        let context = RuntimeCommittedTickContext {
            height: committed_height,
            slot: committed_height.saturating_sub(1),
            epoch: 0,
            node_block_hash: String::new(),
            action_root: String::new(),
            authority_node_id: self.tick_consensus_authority_source().to_string(),
            committed_at_unix_ms: 0,
        };
        self.step_with_modules_for_committed_context(sandbox, &context)
    }

    pub fn step_with_modules_for_committed_context(
        &mut self,
        sandbox: &mut dyn ModuleSandbox,
        context: &RuntimeCommittedTickContext,
    ) -> Result<(), WorldError> {
        let mut no_product_validation_checkpoint = |_world: &World| Ok(());
        self.step_with_modules_for_committed_context_with_product_validation_checkpoint(
            sandbox,
            context,
            &mut no_product_validation_checkpoint,
        )
    }

    /// Execute a committed tick while giving the host a durable checkpoint
    /// opportunity immediately after each product-validation intent is
    /// appended and before the validator is invoked.  The callback receives
    /// the staged world; it must publish that world before returning.
    pub fn step_with_modules_for_committed_context_with_product_validation_checkpoint<F>(
        &mut self,
        sandbox: &mut dyn ModuleSandbox,
        context: &RuntimeCommittedTickContext,
        product_validation_checkpoint: &mut F,
    ) -> Result<(), WorldError>
    where
        F: FnMut(&World) -> Result<(), WorldError>,
    {
        let mut staged = self.clone();
        staged.step_with_modules_for_committed_context_inner(
            sandbox,
            context,
            false,
            product_validation_checkpoint,
        )?;
        *self = staged;
        Ok(())
    }

    /// Resume a committed tick from a host-published product-validation
    /// intent.  A crash/retry path intentionally does not invoke the module:
    /// the due-job reducer observes the persisted attempt and fails closed,
    /// producing the correlated rejection/disposition exactly once.
    pub fn step_with_modules_for_committed_context_after_product_validation_intent(
        &mut self,
        sandbox: &mut dyn ModuleSandbox,
        context: &RuntimeCommittedTickContext,
    ) -> Result<(), WorldError> {
        let mut no_product_validation_checkpoint = |_world: &World| Ok(());
        let mut staged = self.clone();
        staged.step_with_modules_for_committed_context_inner(
            sandbox,
            context,
            true,
            &mut no_product_validation_checkpoint,
        )?;
        *self = staged;
        Ok(())
    }

    fn step_with_modules_for_committed_context_inner(
        &mut self,
        sandbox: &mut dyn ModuleSandbox,
        context: &RuntimeCommittedTickContext,
        resume_after_product_validation_intent: bool,
        product_validation_checkpoint: &mut dyn FnMut(&World) -> Result<(), WorldError>,
    ) -> Result<(), WorldError> {
        let committed_height = context.height;
        let formal_started = self.formal_execution_tick.as_ref() == Some(&(context.clone(), false));
        if self.formal_execution_tick.is_some()
            && (!formal_started || resume_after_product_validation_intent)
        {
            return Err(WorldError::DistributedValidationFailed {
                reason: "formal successor Tick context/prologue mismatch".into(),
            });
        }
        if (!resume_after_product_validation_intent
            && !formal_started
            && committed_height <= self.state.time)
            || (resume_after_product_validation_intent && committed_height != self.state.time)
        {
            return Err(WorldError::DistributedValidationFailed {
                reason: format!(
                    "committed execution height must {}runtime time: current={} incoming={}",
                    if resume_after_product_validation_intent {
                        "match "
                    } else {
                        "advance "
                    },
                    self.state.time,
                    committed_height
                ),
            });
        }
        if !resume_after_product_validation_intent {
            self.state.time = committed_height;
        }
        self.run_modules_for_current_tick(
            sandbox,
            Some(context),
            resume_after_product_validation_intent,
            product_validation_checkpoint,
        )?;
        if formal_started {
            self.formal_execution_tick = Some((context.clone(), true));
        }
        Ok(())
    }

    fn run_modules_for_current_tick(
        &mut self,
        sandbox: &mut dyn ModuleSandbox,
        committed_context: Option<&RuntimeCommittedTickContext>,
        resume_after_product_validation_intent: bool,
        product_validation_checkpoint: &mut dyn FnMut(&World) -> Result<(), WorldError>,
    ) -> Result<(), WorldError> {
        // A product-validation intent is persisted only after the tick
        // prologue (depreciation and committed action application) has
        // completed. Re-running that prologue on recovery would append a
        // second depreciation event and apply the same work against a later
        // in-memory generation. Continue at due-job processing instead.
        if !resume_after_product_validation_intent {
            for event in self.process_factory_depreciation()? {
                self.route_event_to_modules(&event, sandbox)?;
            }
        }
        if !resume_after_product_validation_intent {
            while let Some(envelope) = self.pending_actions.pop_front() {
                let mut action_envelope = envelope.clone();
                let mut post_action_result_event: Option<WorldEvent> = None;
                let module_resolution =
                    match self.resolve_module_backed_economy_action(&envelope, sandbox) {
                        Ok(resolution) => resolution,
                        Err(error @ WorldError::ModuleCallFailed { .. }) => {
                            // Module traps and malformed outputs are action-scoped
                            // failures. The module runtime has already journaled the
                            // failure; convert it to the same durable correlated
                            // ActionRejected path as a declared module denial.
                            EconomyActionResolution::Rejected(RejectReason::RuleDenied {
                                notes: vec![format!(
                                    "economy module evaluation failed; action rejected: {error:?}"
                                )],
                            })
                        }
                        Err(error) => return Err(error),
                    };
                match module_resolution {
                    EconomyActionResolution::Resolved(action) => {
                        action_envelope.action = action;
                    }
                    EconomyActionResolution::Rejected(reason) => {
                        let followup_context = self.factory_production_followup_context(&envelope);
                        self.append_action_accepted_event(&envelope)?;
                        let rejection_body =
                            WorldEventBody::Domain(super::super::DomainEvent::ActionRejected {
                                action_id: envelope.id,
                                reason,
                            });
                        self.append_event(
                            rejection_body.clone(),
                            Some(CausedBy::Action(envelope.id)),
                        )?;
                        post_action_result_event = self.journal.events.last().cloned();
                        self.route_action_to_modules_with_stage_and_event(
                            &envelope,
                            ModuleSubscriptionStage::PostAction,
                            post_action_result_event.as_ref(),
                            sandbox,
                        )?;
                        if let Some(event) = self.journal.events.last() {
                            let event = event.clone();
                            self.route_event_to_modules(&event, sandbox)?;
                        }
                        if let Some(event) = self.append_factory_production_followup_event(
                            &envelope,
                            &rejection_body,
                            followup_context.as_ref(),
                        )? {
                            self.route_event_to_modules(&event, sandbox)?;
                        }
                        continue;
                    }
                }

                let decision = self.evaluate_rule_decisions(&action_envelope, sandbox)?;
                if decision.verdict == RuleVerdict::Modify
                    && let Some(override_action) = decision.override_action.clone()
                {
                    self.record_action_override(
                        super::super::ActionOverrideRecord {
                            action_id: envelope.id,
                            original_action: envelope.action.clone(),
                            override_action: override_action.clone(),
                        },
                        Some(CausedBy::Action(envelope.id)),
                    )?;
                    action_envelope = ActionEnvelope {
                        committed_recipe_origin: None,
                        id: envelope.id,
                        action: override_action,
                    };
                }

                if decision.verdict == RuleVerdict::Deny {
                    self.append_action_accepted_event(&envelope)?;
                    self.append_event(
                        WorldEventBody::Domain(super::super::DomainEvent::ActionRejected {
                            action_id: envelope.id,
                            reason: RejectReason::RuleDenied {
                                notes: decision.notes.clone(),
                            },
                        }),
                        Some(CausedBy::Action(envelope.id)),
                    )?;
                    post_action_result_event = self.journal.events.last().cloned();
                } else {
                    let deficits = decision.cost.deficits(&self.state.resources);
                    if !deficits.is_empty() {
                        self.append_action_accepted_event(&envelope)?;
                        self.append_event(
                            WorldEventBody::Domain(super::super::DomainEvent::ActionRejected {
                                action_id: envelope.id,
                                reason: RejectReason::InsufficientResources { deficits },
                            }),
                            Some(CausedBy::Action(envelope.id)),
                        )?;
                        post_action_result_event = self.journal.events.last().cloned();
                    } else {
                        match self.apply_resource_delta(&decision.cost) {
                            Ok(()) => {
                                if !self.try_apply_runtime_module_action(&action_envelope)? {
                                    let followup_context =
                                        self.factory_production_followup_context(&envelope);
                                    let event_body = self.action_to_event(&action_envelope)?;
                                    self.preflight_domain_event(&event_body)?;
                                    self.append_action_accepted_event(&envelope)?;
                                    self.append_event(
                                        event_body.clone(),
                                        Some(CausedBy::Action(envelope.id)),
                                    )?;
                                    post_action_result_event = self.journal.events.last().cloned();
                                    if let Some(event) = self.append_logistics_reroute_receipt(
                                        &action_envelope.action,
                                        &event_body,
                                    )? {
                                        self.route_event_to_modules(&event, sandbox)?;
                                    }
                                    if let Some(event) = self.journal.events.last() {
                                        let event = event.clone();
                                        self.route_event_to_modules(&event, sandbox)?;
                                    }
                                    if let Some(event) = self
                                        .append_factory_production_followup_event(
                                            &envelope,
                                            &event_body,
                                            followup_context.as_ref(),
                                        )?
                                    {
                                        self.route_event_to_modules(&event, sandbox)?;
                                    }
                                }
                            }
                            Err(err) => {
                                self.append_action_accepted_event(&envelope)?;
                                self.append_event(
                                    WorldEventBody::Domain(
                                        super::super::DomainEvent::ActionRejected {
                                            action_id: envelope.id,
                                            reason: RejectReason::RuleDenied {
                                                notes: vec![format!(
                                                    "rule decision cost apply rejected: {err:?}"
                                                )],
                                            },
                                        },
                                    ),
                                    Some(CausedBy::Action(envelope.id)),
                                )?;
                                post_action_result_event = self.journal.events.last().cloned();
                            }
                        }
                    }
                }

                self.route_action_to_modules_with_stage_and_event(
                    &action_envelope,
                    ModuleSubscriptionStage::PostAction,
                    post_action_result_event.as_ref(),
                    sandbox,
                )?;
                if let Some(event) = self.journal.events.last() {
                    let event = event.clone();
                    self.route_event_to_modules(&event, sandbox)?;
                }
            }
        }
        for event in
            self.process_due_economy_jobs_with_modules(sandbox, product_validation_checkpoint)?
        {
            self.route_event_to_modules(&event, sandbox)?;
        }
        for event in self.process_due_material_transits()? {
            self.route_event_to_modules(&event, sandbox)?;
        }
        for event in self.process_gameplay_cycles_with_modules(sandbox)? {
            self.route_event_to_modules(&event, sandbox)?;
        }
        for event in self.process_restricted_starter_claim_grant_epochs()? {
            self.route_event_to_modules(&event, sandbox)?;
        }
        for event in self.process_agent_claim_epochs()? {
            self.route_event_to_modules(&event, sandbox)?;
        }
        self.refresh_threat_heatmap();
        if let Some(context) = committed_context {
            self.record_tick_consensus_for_committed_context(context)?;
        } else {
            self.record_tick_consensus()?;
        }
        let _ = self.service_cognition_scheduler_tick(self.state.time)?;
        Ok(())
    }
}
