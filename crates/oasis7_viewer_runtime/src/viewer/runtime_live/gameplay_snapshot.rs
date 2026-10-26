use crate::runtime::{
    FactoryProductionFailureDispositionV1, FactoryProductionStatus, FactoryState, IndustryStage,
    WorldState,
};
use crate::runtime_host::archive::{
    PlayerAgentClaimSnapshot, PlayerGameplayCausalityKind, PlayerGameplayExecutionState,
    PlayerGameplayFactoryProductionFailureDisposition, PlayerGameplayGoalKind,
    PlayerGameplayPrimaryIntent, PlayerGameplayRecentFeedback, PlayerGameplaySnapshot,
    PlayerGameplayStageId, PlayerGameplayStageStatus, ProductValidationUnlockPreview,
};
use crate::viewer::ACTION_CLAIM_FIRST_AGENT;
#[path = "gameplay_snapshot_fallback.rs"]
mod fallback;
#[path = "gameplay_snapshot_fine_grain.rs"]
mod fine_grain;
#[path = "gameplay_snapshot_industry.rs"]
mod industry;
#[path = "gameplay_snapshot_intent.rs"]
mod intent;
#[path = "gameplay_snapshot_status.rs"]
mod status;
#[path = "gameplay_snapshot_sync.rs"]
mod sync;
use super::branch_commitment::{branch_recommendations, effective_branch_stage_status};
pub(super) use super::gameplay_snapshot_feedback::player_gameplay_feedback_from_control_ack;
pub(super) use super::gameplay_snapshot_helpers::apply_runtime_snapshot_empty_entities_blocker;
use super::gameplay_snapshot_helpers::{
    base_available_actions, blocker_next_step, primary_factory_for_player_gameplay,
};
use super::gameplay_snapshot_lane::apply_small_player_lane_truth;
use super::gameplay_validation_preview::product_validation_unlock_preview;
use super::player_agency_projection::PlayerGameplayCausalitySignal;
use super::player_gameplay::{extend_available_actions, player_starter_industrial_feasibility};
use fallback::{
    fallback_tradeoff_decision_for_gameplay, player_gameplay_fallback_action,
    player_gameplay_fallback_tradeoff_preview, player_gameplay_wait_resolution_quote,
};
use fine_grain::{
    player_gameplay_fine_grain_action_translation, player_gameplay_player_facing_feedback,
};
use intent::{player_gameplay_intent_scope, player_gameplay_intent_summary};
use status::derive_player_gameplay_execution_state;
use status::{
    player_gameplay_escalation_hint, player_gameplay_last_world_change,
    player_gameplay_primary_blocker, player_gameplay_response_window_class,
    player_gameplay_stalled_reason, player_gameplay_status_reason,
};
use sync::first_session_runtime_sync_blocker;
fn derive_player_gameplay_causality(
    gameplay: &PlayerGameplaySnapshot,
    recent_feedback: Option<&PlayerGameplayRecentFeedback>,
    causality_signal: Option<&PlayerGameplayCausalitySignal>,
) -> (Option<PlayerGameplayCausalityKind>, Option<String>) {
    if let Some(signal) = causality_signal {
        return (Some(signal.kind), Some(signal.detail.clone()));
    }
    match gameplay.execution_state {
        PlayerGameplayExecutionState::Accepted => (
            Some(PlayerGameplayCausalityKind::QueuedForExecution),
            Some(
                recent_feedback
                    .and_then(|feedback| feedback.hint.clone())
                    .unwrap_or_else(|| {
                        "the latest goal-affecting command is accepted and waiting for committed world progress"
                            .to_string()
                    }),
            ),
        ),
        PlayerGameplayExecutionState::Rejected => (
            Some(PlayerGameplayCausalityKind::RequestRejected),
            Some(
                recent_feedback
                    .and_then(|feedback| feedback.reason.clone())
                    .unwrap_or_else(|| {
                        "the latest goal-affecting request was rejected before execution"
                            .to_string()
                    }),
            ),
        ),
        PlayerGameplayExecutionState::Blocked => (
            Some(PlayerGameplayCausalityKind::WorldConstraint),
            gameplay
                .blocker_detail
                .clone()
                .or_else(|| recent_feedback.and_then(|feedback| feedback.reason.clone()))
                .or_else(|| {
                    gameplay
                        .blocker_kind
                        .as_ref()
                        .map(|kind| format!("current goal is blocked by {kind}"))
                }),
        ),
        PlayerGameplayExecutionState::Completed => (
            Some(PlayerGameplayCausalityKind::GoalProgressed),
            Some(
                recent_feedback
                    .map(|feedback| feedback.effect.clone())
                    .filter(|effect| !effect.trim().is_empty())
                    .unwrap_or_else(|| gameplay.progress_detail.clone()),
            ),
        ),
        PlayerGameplayExecutionState::Executing => (None, None),
    }
}
fn finalize_player_gameplay_snapshot(
    mut gameplay: PlayerGameplaySnapshot,
    industry_stage: IndustryStage,
    validation_unlock_preview: Option<ProductValidationUnlockPreview>,
    factory_production_failure_disposition: Option<
        PlayerGameplayFactoryProductionFailureDisposition,
    >,
    recent_feedback: Option<&PlayerGameplayRecentFeedback>,
    causality_signal: Option<&PlayerGameplayCausalitySignal>,
) -> PlayerGameplaySnapshot {
    gameplay.validation_unlock_preview = validation_unlock_preview;
    gameplay.factory_production_failure_disposition = factory_production_failure_disposition;
    gameplay.can_reprioritize = gameplay
        .available_actions
        .iter()
        .any(|action| action.action_id == "reprioritize" && action.disabled_reason.is_none());
    gameplay.branch_recommendations = branch_recommendations(
        industry_stage,
        gameplay.stage_status,
        gameplay.available_actions.as_slice(),
    );
    let effective_stage_status = effective_branch_stage_status(
        gameplay.stage_status,
        gameplay.branch_recommendations.as_slice(),
        gameplay.available_actions.as_slice(),
    );
    if effective_stage_status != gameplay.stage_status {
        gameplay.stage_status = effective_stage_status;
        gameplay.blocker_kind = Some("branch_commitment_unavailable".to_string());
        gameplay.blocker_detail = Some(
            "No executable branch commitment is currently available from the published actions."
                .to_string(),
        );
        gameplay.next_step_hint =
            "Restore the inputs or capability required by a published branch action, then inspect the choices again."
                .to_string();
        gameplay.branch_hint = None;
    }
    let player_facing_feedback = player_gameplay_player_facing_feedback(recent_feedback);
    let player_facing_feedback_ref = player_facing_feedback.as_ref();
    // A guidance receipt confirms the instruction/profile update itself. It
    // is not a pending world action and cannot freeze the current world state
    // as accepted or completed while the Agent continues to act.
    let execution_feedback = player_facing_feedback_ref.filter(|feedback| {
        !matches!(
            (feedback.action.as_str(), feedback.stage.as_str()),
            ("agent_chat", "accepted")
                | (
                    "prompt_control.apply" | "prompt_control.rollback",
                    "completed_advanced"
                )
        )
    });
    gameplay.execution_state =
        derive_player_gameplay_execution_state(gameplay.stage_status, execution_feedback);
    let (causality_kind, causality_detail) =
        derive_player_gameplay_causality(&gameplay, execution_feedback, causality_signal);
    gameplay.causality_kind = causality_kind;
    gameplay.causality_detail = causality_detail;
    let status_reason = player_gameplay_status_reason(&gameplay, player_facing_feedback_ref);
    // Feedback actions are command labels, not canonical accepted AgentIntent
    // identities. The caller assigns this only from a current runtime intent.
    gameplay.accepted_intent_id = None;
    gameplay.intent_summary = player_facing_feedback_ref.and_then(player_gameplay_intent_summary);
    gameplay.intent_scope = player_facing_feedback_ref.and_then(|feedback| {
        player_gameplay_intent_scope(feedback.action.as_str()).map(str::to_string)
    });
    gameplay.intent_target =
        player_facing_feedback_ref.and_then(|feedback| feedback.target_agent_id.clone());
    gameplay.status_reason = status_reason.clone();
    gameplay.last_world_change =
        player_gameplay_last_world_change(&gameplay, player_facing_feedback_ref);
    gameplay.resume_anchor = Some(format!("{} ({})", gameplay.goal_title, gameplay.goal_id));
    gameplay.primary_blocker = player_gameplay_primary_blocker(&gameplay, status_reason.as_ref());
    gameplay.response_window_class =
        player_gameplay_response_window_class(&gameplay, execution_feedback);
    gameplay.stalled_reason = player_gameplay_stalled_reason(&gameplay, player_facing_feedback_ref);
    gameplay.escalation_hint =
        player_gameplay_escalation_hint(&gameplay, player_facing_feedback_ref);
    gameplay.wait_resolution_quote =
        player_gameplay_wait_resolution_quote(player_facing_feedback_ref);
    let fallback_action =
        player_gameplay_fallback_action(&gameplay, gameplay.response_window_class.as_deref());
    gameplay.fallback_action_id = fallback_action.as_ref().map(|(id, _)| id.clone());
    gameplay.fallback_action_label = fallback_action.map(|(_, label)| label);
    gameplay.fallback_tradeoff_preview = player_gameplay_fallback_tradeoff_preview(
        &gameplay,
        gameplay.response_window_class.as_deref(),
    );
    let fallback_decision = gameplay
        .fallback_tradeoff_preview
        .as_ref()
        .map(|_| fallback_tradeoff_decision_for_gameplay(&gameplay));
    gameplay.no_safe_fallback_reason = fallback_decision
        .and_then(|decision| decision.no_safe_fallback_reason)
        .map(str::to_string);
    gameplay.required_next_decision_action_id = fallback_decision
        .and_then(|decision| decision.required_next_decision_action_id)
        .map(str::to_string);
    gameplay.required_next_decision_class = fallback_decision
        .and_then(|decision| decision.required_next_decision_class)
        .map(str::to_string);
    gameplay.resume_next_step = Some(gameplay.next_step_hint.clone());
    apply_small_player_lane_truth(&mut gameplay);
    if gameplay.factory_production_failure_disposition.is_some() {
        gameplay.wait_resolution_quote = None;
        gameplay.repair_available = None;
        gameplay.rebuild_available = None;
        gameplay.pivot_available = None;
        gameplay.recovery_path_kind = None;
        gameplay.recovery_path_detail = None;
        gameplay.recovery_options.clear();
        gameplay.fallback_action_id = None;
        gameplay.fallback_action_label = None;
        gameplay.fallback_tradeoff_preview = None;
        gameplay.no_safe_fallback_reason = None;
        gameplay.required_next_decision_action_id = None;
        gameplay.required_next_decision_class = None;
    }
    gameplay.fine_grain_action_translation =
        player_gameplay_fine_grain_action_translation(&gameplay, recent_feedback);
    gameplay.recent_feedback = player_facing_feedback;
    gameplay
}

fn is_fresh_factory_production_failure_disposition(
    state: &WorldState,
    factory: &FactoryState,
    disposition: &FactoryProductionFailureDispositionV1,
) -> bool {
    let production = &factory.production;
    // The durable disposition and the factory's current blocker record are
    // the freshness authority. A sibling recipe can complete after this
    // product-validation loss and transition the aggregate status to Idle
    // without clearing the still-current consumed/lost inputs. Do not hide
    // that recovery card merely because the aggregate status changed.
    // `active_jobs` is an aggregate across recipe slots and may include an
    // unrelated sibling that is still running. The failed job's own identity
    // and absence from pending jobs remain the freshness checks below.
    production.current_job_id.is_none()
        && production.current_recipe_id.is_none()
        && production
            .current_blocker_action_id
            .is_none_or(|action_id| action_id == disposition.action_id)
        && production.current_blocker_kind.as_deref() == Some(disposition.blocker_kind.as_str())
        && production.current_blocker_detail.as_deref() == Some(disposition.blocker_detail.as_str())
        && !state
            .pending_recipe_jobs
            .contains_key(&disposition.action_id)
}
fn latest_fresh_requester_failure_disposition<'a>(
    state: &'a WorldState,
    requester_agent_id: &str,
) -> Option<&'a FactoryProductionFailureDispositionV1> {
    state
        .factory_production_failure_dispositions
        .iter()
        .rev()
        .find_map(|(_, disposition)| {
            let factory = state.factories.get(disposition.factory_id.as_str())?;
            (disposition.requester_agent_id == requester_agent_id
                && factory.builder_agent_id == requester_agent_id
                && is_fresh_factory_production_failure_disposition(state, factory, disposition))
            .then_some(disposition)
        })
}
#[expect(clippy::too_many_arguments, reason = "DTO mapping")]
pub(super) fn build_player_gameplay_snapshot(
    state: &WorldState,
    controlled_agent_id: Option<&str>,
    confirmed_gameplay_progress: bool,
    recent_feedback: Option<&PlayerGameplayRecentFeedback>,
    primary_intent: Option<PlayerGameplayPrimaryIntent>,
    causality_signal: Option<&PlayerGameplayCausalitySignal>,
    gameplay_enabled: bool,
    gameplay_disabled_reason: Option<&str>,
    supports_agent_chat: bool,
    first_agent_claim_target_available: bool,
    agent_claim: Option<PlayerAgentClaimSnapshot>,
    player_scoped: bool,
) -> PlayerGameplaySnapshot {
    let starter_industrial_feasibility = state.starter_industrial_feasibility();
    let mut available_actions = base_available_actions(
        controlled_agent_id,
        gameplay_enabled,
        gameplay_disabled_reason,
        supports_agent_chat,
    );
    if gameplay_enabled {
        extend_available_actions(
            state,
            &starter_industrial_feasibility,
            controlled_agent_id,
            first_agent_claim_target_available,
            &mut available_actions,
        );
    }
    let industry_stage = state.industry_progress.stage;
    let validation_unlock_preview = product_validation_unlock_preview(state);
    let primary_factory = match controlled_agent_id {
        Some(agent_id) => industry::primary_personal_factory(state, agent_id),
        None if !player_scoped => primary_factory_for_player_gameplay(state),
        None => None,
    };
    let factory_production_failure_disposition =
        controlled_agent_id.and_then(|requester_agent_id| {
            latest_fresh_requester_failure_disposition(state, requester_agent_id).map(
                |disposition| PlayerGameplayFactoryProductionFailureDisposition {
                    action_id: disposition.action_id.to_string(),
                    requester_agent_id: disposition.requester_agent_id.clone(),
                    factory_id: disposition.factory_id.clone(),
                    recipe_id: disposition.recipe_id.clone(),
                    blocker_kind: disposition.blocker_kind.clone(),
                    blocker_detail: disposition.blocker_detail.clone(),
                    disposition_kind: disposition.disposition_kind.clone(),
                    consumed_inputs: disposition.consumed_inputs.clone(),
                    lost_inputs: disposition.lost_inputs.clone(),
                    consumed_power: disposition.consumed_power,
                    lost_power: disposition.lost_power,
                    next_action: disposition.next_action.clone(),
                    next_recheck: disposition.next_recheck,
                },
            )
        });
    let finalize = |gameplay| {
        let mut gameplay = finalize_player_gameplay_snapshot(
            gameplay,
            industry_stage,
            validation_unlock_preview.clone(),
            factory_production_failure_disposition.clone(),
            recent_feedback,
            causality_signal,
        );
        gameplay.primary_intent = primary_intent.clone();
        gameplay.accepted_intent_id = primary_intent.as_ref().and_then(|intent| {
            super::player_agency_projection::canonical_accepted_runtime_intent_id(
                intent.status.as_str(),
                intent.source_class.as_deref(),
                intent.freshness.as_deref(),
                intent.intent_id.as_deref(),
            )
        });
        gameplay.starter_industrial_feasibility = Some(player_starter_industrial_feasibility(
            &starter_industrial_feasibility,
            state,
            controlled_agent_id,
        ));
        gameplay
    };
    if !gameplay_enabled {
        let disabled_reason = gameplay_disabled_reason
            .unwrap_or("gameplay requires runtime live server running with --llm");
        return finalize(PlayerGameplaySnapshot {
            canonical_agent_chat: None,
            recipe_substitution: None,
            stage_id: PlayerGameplayStageId::FirstSessionLoop,
            stage_status: PlayerGameplayStageStatus::Blocked,
            execution_state: PlayerGameplayExecutionState::Executing,
            accepted_intent_id: None,
            primary_intent: None,
            intent_summary: None,
            intent_scope: None,
            intent_target: None,
            can_reprioritize: false,
            goal_id: "first_session_loop.configure_llm_access".to_string(),
            goal_kind: PlayerGameplayGoalKind::CreateFirstWorldFeedback,
            goal_title: "Configure LLM access before entering the world".to_string(),
            objective:
                "This world requires an active LLM provider before gameplay controls are allowed."
                    .to_string(),
            progress_detail:
                "Gameplay is blocked until runtime live is running with an initialized LLM provider."
                    .to_string(),
            progress_percent: 0,
            blocker_kind: Some("llm_required".to_string()),
            blocker_detail: Some(disabled_reason.to_string()),
            next_step_hint:
                "Enable --llm and configure a reachable provider before retrying play, step, or gameplay actions."
                    .to_string(),
            status_reason: None,
            last_world_change: None,
            causality_kind: None,
            causality_detail: None,
            branch_hint: None,
            resume_anchor: None,
            primary_blocker: None,
            response_window_class: None,
            stalled_reason: None,
            escalation_hint: None,
            fallback_action_id: None,
            fallback_action_label: None,
            fallback_tradeoff_preview: None,
            no_safe_fallback_reason: None,
            required_next_decision_action_id: None,
            required_next_decision_class: None,
            resume_next_step: None,
            branch_recommendations: Vec::new(),
            available_actions,
            recent_feedback: recent_feedback.cloned(),
            wait_resolution_quote: None,
            agent_claim,
            micro_depot_facilities: Vec::new(),
            small_player_lane_id: None,
            leverage_class: None,
            same_loop_repeat_count: 0,
            grind_only_flag: false,
            major_power_dependency_status: None,
            recovery_path_kind: None,
            recovery_path_detail: None,
            requires_major_power_sponsorship: None,
            repair_available: None,
            rebuild_available: None,
            pivot_available: None,
            validation_unlock_preview: None,
            starter_industrial_feasibility: None,
            factory_production_failure_disposition: None,
            recovery_options: Vec::new(),
            fine_grain_action_translation: None,
        });
    }
    let latest_blocker = primary_factory.and_then(|factory| {
        let kind = factory.production.current_blocker_kind.as_ref()?;
        let detail = factory
            .production
            .current_blocker_detail
            .clone()
            .unwrap_or_else(|| format!("factory={}", factory.factory_id));
        Some((kind.clone(), detail))
    });
    let blocked_feedback = recent_feedback.and_then(|feedback| {
        matches!(feedback.stage.as_str(), "blocked" | "completed_no_progress").then(|| {
            (
                "no_progress".to_string(),
                feedback.reason.clone().unwrap_or_else(|| {
                    "latest command did not create forward progress".to_string()
                }),
            )
        })
    });
    let has_first_session_feedback = recent_feedback
        .is_some_and(|feedback| feedback.delta_logical_time > 0 || feedback.delta_event_seq > 0);
    let has_confirmed_world_progress = if player_scoped {
        super::gameplay_snapshot_helpers::has_player_world_effect(
            state,
            controlled_agent_id,
            primary_intent.as_ref(),
        )
    } else {
        has_first_session_feedback || confirmed_gameplay_progress
    };
    let has_material_flow =
        !player_scoped && state.industry_progress.completed_material_transits > 0;
    let has_factory_ready = primary_factory.is_some();
    let has_recipe_running = primary_factory
        .is_some_and(|factory| factory.production.status == FactoryProductionStatus::Running);
    let has_first_output = primary_factory.is_some_and(|factory| {
        factory.production.completed_jobs > 0 || factory.production.last_completed_at.is_some()
    });
    let has_blocked_history =
        primary_factory.is_some_and(|factory| factory.production.last_blocked_at.is_some());
    let has_recovery_history =
        primary_factory.is_some_and(|factory| factory.production.last_resumed_at.is_some());
    let same_loop_repeat_count = primary_factory
        .map(|factory| {
            if player_scoped {
                industry::current_repeat_count(state, factory)
            } else {
                factory.production.same_recipe_repeat_count
            }
        })
        .unwrap_or(0);
    if !has_confirmed_world_progress
        && !has_material_flow
        && !has_factory_ready
        && !has_recipe_running
        && !has_first_output
        && latest_blocker.is_none()
        && let Some((blocker_kind, blocker_detail, next_step_hint)) =
            first_session_runtime_sync_blocker(recent_feedback)
    {
        let disabled_reason =
                "committed runtime sync is unavailable; refresh the snapshot or repair runtime bootstrap first"
                    .to_string();
        for action in &mut available_actions {
            if action.protocol_action == "request_snapshot"
                || (state.agents.is_empty() && action.action_id == ACTION_CLAIM_FIRST_AGENT)
            {
                continue;
            }
            action.disabled_reason = Some(disabled_reason.clone());
        }
        return finalize(PlayerGameplaySnapshot {
            canonical_agent_chat: None,
            recipe_substitution: None,
                stage_id: PlayerGameplayStageId::FirstSessionLoop,
                stage_status: PlayerGameplayStageStatus::Blocked,
                execution_state: PlayerGameplayExecutionState::Executing,
                accepted_intent_id: None,
                primary_intent: None,
                intent_summary: None,
                intent_scope: None,
                intent_target: None,
                can_reprioritize: false,
                goal_id: "first_session_loop.recover_runtime_sync".to_string(),
                goal_kind: PlayerGameplayGoalKind::CreateFirstWorldFeedback,
                goal_title: "Recover committed runtime sync".to_string(),
                objective: "Repair the committed runtime feed before retrying the first world-feedback loop.".to_string(),
                progress_detail:
                    "The first-session loop is blocked because the viewer cannot read a committed runtime world yet."
                        .to_string(),
                progress_percent: 0,
                blocker_kind: Some(blocker_kind),
                blocker_detail: Some(blocker_detail),
                next_step_hint,
                status_reason: None,
                last_world_change: None,
                causality_kind: None,
                causality_detail: None,
                branch_hint: None,
                resume_anchor: None,
                primary_blocker: None,
                response_window_class: None,
                stalled_reason: None,
                escalation_hint: None,
                fallback_action_id: None,
                fallback_action_label: None,
                fallback_tradeoff_preview: None,
                no_safe_fallback_reason: None,
                required_next_decision_action_id: None,
                required_next_decision_class: None,
                resume_next_step: None,
                branch_recommendations: Vec::new(),
                available_actions,
                recent_feedback: recent_feedback.cloned(), wait_resolution_quote: None,
                agent_claim,
                micro_depot_facilities: Vec::new(),
                small_player_lane_id: None,
                leverage_class: None,
                same_loop_repeat_count: 0,
                grind_only_flag: false,
                major_power_dependency_status: None,
                recovery_path_kind: None,
                recovery_path_detail: None,
                requires_major_power_sponsorship: None,
                repair_available: None,
                rebuild_available: None,
                pivot_available: None,
                validation_unlock_preview: None,
                starter_industrial_feasibility: None,
                factory_production_failure_disposition: None,
                recovery_options: Vec::new(),
                fine_grain_action_translation: None,
        });
    }
    if !has_confirmed_world_progress
        && !has_material_flow
        && !has_factory_ready
        && !has_recipe_running
        && !has_first_output
        && latest_blocker.is_none()
    {
        if let Some(action) = available_actions
            .iter_mut()
            .find(|action| action.action_id == "advance_step")
        {
            action.label = "Advance 1 step to create the first world feedback".to_string();
        }
        return finalize(PlayerGameplaySnapshot {
            canonical_agent_chat: None,
            recipe_substitution: None,
            stage_id: PlayerGameplayStageId::FirstSessionLoop,
            stage_status: PlayerGameplayStageStatus::Active,
            execution_state: PlayerGameplayExecutionState::Executing,
            accepted_intent_id: None,
            primary_intent: None,
            intent_summary: None,
            intent_scope: None,
            intent_target: None,
            can_reprioritize: false,
            goal_id: "first_session_loop.create_first_world_feedback".to_string(),
            goal_kind: PlayerGameplayGoalKind::CreateFirstWorldFeedback,
            goal_title: "Create the first visible world feedback".to_string(),
            objective: "Advance the world once and confirm that your action produces a visible state or event delta.".to_string(),
            progress_detail: "You are still in the initial action loop; the first feedback has not been confirmed yet.".to_string(),
            progress_percent: 0,
            blocker_kind: None,
            blocker_detail: None,
            next_step_hint: "Request a snapshot, advance 1 step, then inspect the new delta and events.".to_string(),
            status_reason: None,
            last_world_change: None,
            causality_kind: None,
            causality_detail: None,
            branch_hint: None,
            resume_anchor: None,
            primary_blocker: None,
            response_window_class: None,
            stalled_reason: None,
            escalation_hint: None,
            fallback_action_id: None,
            fallback_action_label: None,
            fallback_tradeoff_preview: None,
            no_safe_fallback_reason: None,
            required_next_decision_action_id: None,
            required_next_decision_class: None,
            resume_next_step: None,
            branch_recommendations: Vec::new(),
            available_actions,
            recent_feedback: recent_feedback.cloned(), wait_resolution_quote: None,
            agent_claim,
            micro_depot_facilities: Vec::new(),
            small_player_lane_id: None,
            leverage_class: None,
            same_loop_repeat_count: 0,
            grind_only_flag: false,
            major_power_dependency_status: None,
            recovery_path_kind: None,
            recovery_path_detail: None,
            requires_major_power_sponsorship: None,
            repair_available: None,
            rebuild_available: None,
            pivot_available: None,
            validation_unlock_preview: None,
            starter_industrial_feasibility: None,
            factory_production_failure_disposition: None,
            recovery_options: Vec::new(),
            fine_grain_action_translation: None,
        });
    }
    let fallback_feedback_blocker = if latest_blocker.is_none()
        && primary_factory
            .is_none_or(|factory| factory.production.status == FactoryProductionStatus::Blocked)
    {
        blocked_feedback
    } else {
        None
    };
    if let Some((blocker_kind, blocker_detail)) = latest_blocker.or(fallback_feedback_blocker) {
        let (progress_detail, progress_percent) = if has_first_output {
            (
                "Stage progress: the first line already produced output, but the current stoppage still blocks resilient production."
                    .to_string(),
                84,
            )
        } else {
            (
                "Stage progress: you are in the management phase, but the primary line is blocked."
                    .to_string(),
                68,
            )
        };
        return finalize(PlayerGameplaySnapshot {
            canonical_agent_chat: None,
            recipe_substitution: None,
            stage_id: PlayerGameplayStageId::PostOnboarding,
            stage_status: PlayerGameplayStageStatus::Blocked,
            execution_state: PlayerGameplayExecutionState::Executing,
            accepted_intent_id: None,
            primary_intent: None,
            intent_summary: None,
            intent_scope: None,
            intent_target: None,
            can_reprioritize: false,
            goal_id: "post_onboarding.recover_capability".to_string(),
            goal_kind: PlayerGameplayGoalKind::RecoverCapability,
            goal_title: "Recover sustainable capability".to_string(),
            objective:
                "Recover the blocked line or capability chain instead of repeating one-off actions."
                    .to_string(),
            progress_detail,
            progress_percent,
            blocker_kind: Some(blocker_kind.clone()),
            blocker_detail: Some(blocker_detail.clone()),
            next_step_hint: blocker_next_step(blocker_kind.as_str(), blocker_detail.as_str()),
            status_reason: None,
            last_world_change: None,
            causality_kind: None,
            causality_detail: None,
            branch_hint: None,
            resume_anchor: None,
            primary_blocker: None,
            response_window_class: None,
            stalled_reason: None,
            escalation_hint: None,
            fallback_action_id: None,
            fallback_action_label: None,
            fallback_tradeoff_preview: None,
            no_safe_fallback_reason: None,
            required_next_decision_action_id: None,
            required_next_decision_class: None,
            resume_next_step: None,
            branch_recommendations: Vec::new(),
            available_actions,
            recent_feedback: recent_feedback.cloned(),
            wait_resolution_quote: None,
            agent_claim,
            micro_depot_facilities: Vec::new(),
            small_player_lane_id: None,
            leverage_class: None,
            same_loop_repeat_count: 0,
            grind_only_flag: false,
            major_power_dependency_status: None,
            recovery_path_kind: None,
            recovery_path_detail: None,
            requires_major_power_sponsorship: None,
            repair_available: None,
            rebuild_available: None,
            pivot_available: None,
            validation_unlock_preview: None,
            starter_industrial_feasibility: None,
            factory_production_failure_disposition: None,
            recovery_options: Vec::new(),
            fine_grain_action_translation: None,
        });
    }
    if has_first_output {
        // Shared world maturity is not evidence that this player stabilized a
        // line or already made an expansion decision.
        let personal_stage = if player_scoped {
            industry::personal_stage(state, primary_factory, controlled_agent_id)
        } else {
            industry_stage
        };
        match personal_stage {
            IndustryStage::Bootstrap => {
                let (progress_detail, next_step_hint, progress_percent) = if has_recovery_history {
                    (
                        "Stage progress: the first line already recovered once; keep it producing until the first expansion tradeoff is justified."
                            .to_string(),
                        "Advance again and decide whether the next gain should come from more throughput, stronger inputs, or wider logistics reach."
                            .to_string(),
                        88,
                    )
                } else if has_blocked_history {
                    (
                        "Stage progress: the first line produced output, but it still needs one clean recovery beat before expansion becomes the right call."
                            .to_string(),
                        "Keep advancing until the line recovers from the next stoppage and proves it can resume without manual babysitting."
                            .to_string(),
                        82,
                    )
                } else {
                    (
                        "Stage progress: the first output exists; now harden the line until it survives its first real stoppage or exposes a repeatable recovery loop."
                            .to_string(),
                        "Advance 1-2 more times and watch whether the line stays stable, stalls, or recovers into repeatable output."
                            .to_string(),
                        80,
                    )
                };
                return finalize(PlayerGameplaySnapshot {
            canonical_agent_chat: None,
            recipe_substitution: None,
                    stage_id: PlayerGameplayStageId::PostOnboarding,
                    stage_status: PlayerGameplayStageStatus::Active,
                    execution_state: PlayerGameplayExecutionState::Executing,
                    accepted_intent_id: None,
                    primary_intent: None,
                    intent_summary: None,
                    intent_scope: None,
                    intent_target: None,
                    can_reprioritize: false,
                    goal_id: "post_onboarding.stabilize_first_line_after_output".to_string(),
                    goal_kind: PlayerGameplayGoalKind::StabilizeFirstLine,
                    goal_title: "Harden your first output into resilient production".to_string(),
                    objective: "One visible output is not enough. Keep the first line alive until it survives interruption and resumes as a repeatable capability.".to_string(),
                    progress_detail,
                    progress_percent,
                    blocker_kind: None,
                    blocker_detail: None,
                    next_step_hint,
                    status_reason: None,
                    last_world_change: None,
                    causality_kind: None,
                    causality_detail: None,
                    branch_hint: None,
                    resume_anchor: None,
                    primary_blocker: None,
                    response_window_class: None,
                    stalled_reason: None,
                    escalation_hint: None,
                    fallback_action_id: None,
                    fallback_action_label: None,
                    fallback_tradeoff_preview: None,
                    no_safe_fallback_reason: None,
                    required_next_decision_action_id: None,
                    required_next_decision_class: None,
                    resume_next_step: None,
                    branch_recommendations: Vec::new(),
                    available_actions,
                    recent_feedback: recent_feedback.cloned(), wait_resolution_quote: None,
                    agent_claim,
                    micro_depot_facilities: Vec::new(),
                    small_player_lane_id: None,
                    leverage_class: None,
                    same_loop_repeat_count,
                    grind_only_flag: false,
                    major_power_dependency_status: None,
                    recovery_path_kind: None,
                    recovery_path_detail: None,
                    requires_major_power_sponsorship: None,
                    repair_available: None,
                    rebuild_available: None,
                    pivot_available: None,
                    validation_unlock_preview: None,
                    starter_industrial_feasibility: None,
                    factory_production_failure_disposition: None,
                    recovery_options: Vec::new(),
                    fine_grain_action_translation: None,
                });
            }
            IndustryStage::ScaleOut => {
                return finalize(PlayerGameplaySnapshot {
            canonical_agent_chat: None,
            recipe_substitution: None,
                    stage_id: PlayerGameplayStageId::PostOnboarding,
                    stage_status: PlayerGameplayStageStatus::BranchReady,
                    execution_state: PlayerGameplayExecutionState::Executing,
            accepted_intent_id: None,
            primary_intent: None,
            intent_summary: None,
                    intent_scope: None,
                    intent_target: None,
                    can_reprioritize: false,
                    goal_id: "post_onboarding.choose_first_expansion_tradeoff".to_string(),
                    goal_kind: PlayerGameplayGoalKind::ChooseFirstExpansionTradeoff,
                    goal_title: "Choose the first expansion tradeoff".to_string(),
                    objective: "The first line is stable enough to grow. Pick whether the next investment should buy more throughput, stronger resilience, or wider logistics reach.".to_string(),
                    progress_detail: "Stage progress: bootstrap is complete and the first expansion tradeoff is now unlocked.".to_string(),
                    progress_percent: 92,
                    blocker_kind: None,
                    blocker_detail: None,
                    next_step_hint: "Advance again and commit to one tradeoff: add capacity, protect upstream inputs, or widen distribution coverage.".to_string(),
                    status_reason: None,
                    last_world_change: None,
                    causality_kind: None,
                    causality_detail: None,
                    branch_hint: Some(
                        "Tradeoffs unlocked: throughput expansion / input resilience / logistics reach"
                            .to_string(),
                    ),
                    resume_anchor: None,
                    primary_blocker: None,
                    response_window_class: None,
                    stalled_reason: None,
                    escalation_hint: None,
                    fallback_action_id: None,
                    fallback_action_label: None,
                    fallback_tradeoff_preview: None,
                    no_safe_fallback_reason: None,
                    required_next_decision_action_id: None,
                    required_next_decision_class: None,
                    resume_next_step: None,
                    branch_recommendations: Vec::new(),
                    available_actions,
                    recent_feedback: recent_feedback.cloned(), wait_resolution_quote: None,
                    agent_claim,
                    micro_depot_facilities: Vec::new(),
                    small_player_lane_id: None,
                    leverage_class: None,
                    same_loop_repeat_count,
                    grind_only_flag: false,
                    major_power_dependency_status: None,
                    recovery_path_kind: None,
                    recovery_path_detail: None,
                    requires_major_power_sponsorship: None,
                    repair_available: None,
                    rebuild_available: None,
                    pivot_available: None,
                    validation_unlock_preview: None,
                    starter_industrial_feasibility: None,
                    factory_production_failure_disposition: None,
                    recovery_options: Vec::new(),
                    fine_grain_action_translation: None,
                });
            }
            IndustryStage::Governance => {
                return finalize(PlayerGameplaySnapshot {
            canonical_agent_chat: None,
            recipe_substitution: None,
                    stage_id: PlayerGameplayStageId::PostOnboarding,
                    stage_status: PlayerGameplayStageStatus::BranchReady,
                    execution_state: PlayerGameplayExecutionState::Executing,
            accepted_intent_id: None,
            primary_intent: None,
            intent_summary: None,
                    intent_scope: None,
                    intent_target: None,
                    can_reprioritize: false,
                    goal_id: "post_onboarding.choose_midloop_path".to_string(),
                    goal_kind: PlayerGameplayGoalKind::ChooseMidLoopPath,
                    goal_title: "Choose your mid-loop path".to_string(),
                    objective: "Your first sustainable industrial capability is online. Expand it into stable organizational momentum.".to_string(),
                    progress_detail: "Stage progress: your stable production and subsequent completed work now support wider mid-loop choices.".to_string(),
                    progress_percent: 100,
                    blocker_kind: None,
                    blocker_detail: None,
                    next_step_hint: "Keep advancing through a local specialization that strengthens a useful regional service or supply route.".to_string(),
                    status_reason: None,
                    last_world_change: None,
                    causality_kind: None,
                    causality_detail: None,
                    branch_hint: Some(
                        "Branches unlocked: local conversion specialist / regional service specialist / regional supply specialization"
                            .to_string(),
                    ),
                    resume_anchor: None,
                    primary_blocker: None,
                    response_window_class: None,
                    stalled_reason: None,
                    escalation_hint: None,
                    fallback_action_id: None,
                    fallback_action_label: None,
                    fallback_tradeoff_preview: None,
                    no_safe_fallback_reason: None,
                    required_next_decision_action_id: None,
                    required_next_decision_class: None,
                    resume_next_step: None,
                    branch_recommendations: Vec::new(),
                    available_actions,
                    recent_feedback: recent_feedback.cloned(), wait_resolution_quote: None,
                    agent_claim,
                    micro_depot_facilities: Vec::new(),
                    small_player_lane_id: None,
                    leverage_class: None,
                    same_loop_repeat_count: 0,
                    grind_only_flag: false,
                    major_power_dependency_status: None,
                    recovery_path_kind: None,
                    recovery_path_detail: None,
                    requires_major_power_sponsorship: None,
                    repair_available: None,
                    rebuild_available: None,
                    pivot_available: None,
                    validation_unlock_preview: None,
                    starter_industrial_feasibility: None,
                    factory_production_failure_disposition: None,
                    recovery_options: Vec::new(),
                    fine_grain_action_translation: None,
                });
            }
        }
    }
    if has_recipe_running {
        return finalize(PlayerGameplaySnapshot {
            canonical_agent_chat: None,
            recipe_substitution: None,
            stage_id: PlayerGameplayStageId::PostOnboarding,
            stage_status: PlayerGameplayStageStatus::Active,
            execution_state: PlayerGameplayExecutionState::Executing,
            accepted_intent_id: None,
            primary_intent: None,
            intent_summary: None,
            intent_scope: None,
            intent_target: None,
            can_reprioritize: false,
            goal_id: "post_onboarding.stabilize_first_line".to_string(),
            goal_kind: PlayerGameplayGoalKind::StabilizeFirstLine,
            goal_title: "Stabilize your first line".to_string(),
            objective: "Keep the first production line moving until it yields stable output or exposes a clear blocker.".to_string(),
            progress_detail: "Stage progress: the first line is running; now watch for output and stoppage reasons.".to_string(),
            progress_percent: 72,
            blocker_kind: None,
            blocker_detail: None,
            next_step_hint: "Advance 1-2 more times and watch for output, recovery, or blocker feedback.".to_string(),
            status_reason: None,
            last_world_change: None,
            causality_kind: None,
            causality_detail: None,
            branch_hint: None,
            resume_anchor: None,
            primary_blocker: None,
            response_window_class: None,
            stalled_reason: None,
            escalation_hint: None,
            fallback_action_id: None,
            fallback_action_label: None,
            fallback_tradeoff_preview: None,
            no_safe_fallback_reason: None,
            required_next_decision_action_id: None,
            required_next_decision_class: None,
            resume_next_step: None,
            branch_recommendations: Vec::new(),
            available_actions,
            recent_feedback: recent_feedback.cloned(), wait_resolution_quote: None,
            agent_claim,
            micro_depot_facilities: Vec::new(),
            small_player_lane_id: None,
            leverage_class: None,
            same_loop_repeat_count,
            grind_only_flag: false,
            major_power_dependency_status: None,
            recovery_path_kind: None,
            recovery_path_detail: None,
            requires_major_power_sponsorship: None,
            repair_available: None,
            rebuild_available: None,
            pivot_available: None,
            validation_unlock_preview: None,
            starter_industrial_feasibility: None,
            factory_production_failure_disposition: None,
            recovery_options: Vec::new(),
            fine_grain_action_translation: None,
        });
    }
    if has_factory_ready {
        return finalize(PlayerGameplaySnapshot {
            canonical_agent_chat: None,
            recipe_substitution: None,
            stage_id: PlayerGameplayStageId::PostOnboarding,
            stage_status: PlayerGameplayStageStatus::Active,
            execution_state: PlayerGameplayExecutionState::Executing,
            accepted_intent_id: None,
            primary_intent: None,
            intent_summary: None,
            intent_scope: None,
            intent_target: None,
            can_reprioritize: false,
            goal_id: "post_onboarding.start_factory_run".to_string(),
            goal_kind: PlayerGameplayGoalKind::StartFactoryRun,
            goal_title: "Start your first factory run".to_string(),
            objective: "Turn the factory you built into a running, repeatable capability.".to_string(),
            progress_detail: "Stage progress: the factory is ready; one visible production push remains.".to_string(),
            progress_percent: 54,
            blocker_kind: None,
            blocker_detail: None,
            next_step_hint: "Keep advancing until the factory starts a recipe, yields output, or returns a blocker.".to_string(),
            status_reason: None,
            last_world_change: None,
            causality_kind: None,
            causality_detail: None,
            branch_hint: None,
            resume_anchor: None,
            primary_blocker: None,
            response_window_class: None,
            stalled_reason: None,
            escalation_hint: None,
            fallback_action_id: None,
            fallback_action_label: None,
            fallback_tradeoff_preview: None,
            no_safe_fallback_reason: None,
            required_next_decision_action_id: None,
            required_next_decision_class: None,
            resume_next_step: None,
            branch_recommendations: Vec::new(),
            available_actions,
            recent_feedback: recent_feedback.cloned(), wait_resolution_quote: None,
            agent_claim,
            micro_depot_facilities: Vec::new(),
            small_player_lane_id: None,
            leverage_class: None,
            same_loop_repeat_count,
            grind_only_flag: false,
            major_power_dependency_status: None,
            recovery_path_kind: None,
            recovery_path_detail: None,
            requires_major_power_sponsorship: None,
            repair_available: None,
            rebuild_available: None,
            pivot_available: None,
                    validation_unlock_preview: None,
                    starter_industrial_feasibility: None,
                    factory_production_failure_disposition: None,
            recovery_options: Vec::new(),
            fine_grain_action_translation: None,
        });
    }
    if has_material_flow {
        return finalize(PlayerGameplaySnapshot {
            canonical_agent_chat: None,
            recipe_substitution: None,
            stage_id: PlayerGameplayStageId::PostOnboarding,
            stage_status: PlayerGameplayStageStatus::Active,
            execution_state: PlayerGameplayExecutionState::Executing,
            accepted_intent_id: None,
            primary_intent: None,
            intent_summary: None,
            intent_scope: None,
            intent_target: None,
            can_reprioritize: false,
            goal_id: "post_onboarding.turn_material_flow_into_output".to_string(),
            goal_kind: PlayerGameplayGoalKind::TurnMaterialFlowIntoOutput,
            goal_title: "Turn material flow into output".to_string(),
            objective: "Do not stop at one-off harvesting; push the resource flow into visible output.".to_string(),
            progress_detail: "Stage progress: base resources are moving; now convert them into the first sustainable capability.".to_string(),
            progress_percent: 38,
            blocker_kind: None,
            blocker_detail: None,
            next_step_hint: "Keep harvesting, refining, building, or starting the first recipe until stable output appears.".to_string(),
            status_reason: None,
            last_world_change: None,
            causality_kind: None,
            causality_detail: None,
            branch_hint: None,
            resume_anchor: None,
            primary_blocker: None,
            response_window_class: None,
            stalled_reason: None,
            escalation_hint: None,
            fallback_action_id: None,
            fallback_action_label: None,
            fallback_tradeoff_preview: None,
            no_safe_fallback_reason: None,
            required_next_decision_action_id: None,
            required_next_decision_class: None,
            resume_next_step: None,
            branch_recommendations: Vec::new(),
            available_actions,
            recent_feedback: recent_feedback.cloned(), wait_resolution_quote: None,
            agent_claim,
            micro_depot_facilities: Vec::new(),
            small_player_lane_id: None,
            leverage_class: None,
            same_loop_repeat_count: 0,
            grind_only_flag: false,
            major_power_dependency_status: None,
            recovery_path_kind: None,
            recovery_path_detail: None,
            requires_major_power_sponsorship: None,
            repair_available: None,
            rebuild_available: None,
            pivot_available: None,
                    validation_unlock_preview: None,
                    starter_industrial_feasibility: None,
                    factory_production_failure_disposition: None,
            recovery_options: Vec::new(),
            fine_grain_action_translation: None,
        });
    }
    finalize(PlayerGameplaySnapshot {
            canonical_agent_chat: None,
            recipe_substitution: None,
        stage_id: PlayerGameplayStageId::PostOnboarding,
        stage_status: PlayerGameplayStageStatus::Active,
        execution_state: PlayerGameplayExecutionState::Executing,
        accepted_intent_id: None,
        primary_intent: None,
        intent_summary: None,
        intent_scope: None,
        intent_target: None,
        can_reprioritize: false,
        goal_id: "post_onboarding.establish_first_capability".to_string(),
        goal_kind: PlayerGameplayGoalKind::EstablishFirstCapability,
        goal_title: "Establish your first sustainable capability".to_string(),
        objective: "The first-session action loop is complete. Create your first sustainable industrial result instead of repeating the tutorial.".to_string(),
        progress_detail: "Stage progress: you have moved from 'can operate' into the start of 'can manage'.".to_string(),
        progress_percent: 20,
        blocker_kind: None,
        blocker_detail: None,
        next_step_hint: "Advance 2-3 more times and prioritize the first output, the first stable line, or one clear recovery signal.".to_string(),
        status_reason: None,
        last_world_change: None,
        causality_kind: None,
        causality_detail: None,
        branch_hint: None,
        resume_anchor: None,
        primary_blocker: None,
        response_window_class: None,
        stalled_reason: None,
        escalation_hint: None,
        fallback_action_id: None,
        fallback_action_label: None,
        fallback_tradeoff_preview: None,
        no_safe_fallback_reason: None,
        required_next_decision_action_id: None,
        required_next_decision_class: None,
        resume_next_step: None,
        branch_recommendations: Vec::new(),
        available_actions,
        recent_feedback: recent_feedback.cloned(), wait_resolution_quote: None,
        agent_claim,
        micro_depot_facilities: Vec::new(),
        small_player_lane_id: None,
        leverage_class: None,
        same_loop_repeat_count: 0,
        grind_only_flag: false,
        major_power_dependency_status: None,
        recovery_path_kind: None,
        recovery_path_detail: None,
        requires_major_power_sponsorship: None,
        repair_available: None,
        rebuild_available: None,
        pivot_available: None,
        validation_unlock_preview: None,
        starter_industrial_feasibility: None,
        factory_production_failure_disposition: None,
        recovery_options: Vec::new(),
        fine_grain_action_translation: None,
    })
}
