use super::ViewerLiveDecisionMode;
use super::control_plane::{RuntimeLlmSidecar, runtime_provider_settings_from_env};
use super::location_id_for_pos;
use super::power_projection::runtime_storage_power_statuses;
use super::support::{
    FORMAL_RELEASE_DEFAULT_BOOTSTRAP_AGENT_ID, formal_release_default_seed_location_for_pos,
};
use crate::geometry::space_distance_cm;
#[cfg(test)]
use crate::runtime::RejectReason as RuntimeRejectReason;
use crate::runtime::{
    AgentActivityStatus, DomainEvent as RuntimeDomainEvent, WorldEvent as RuntimeWorldEvent,
    WorldEventBody as RuntimeWorldEventBody,
};
use crate::simulator::{
    DEFAULT_PROVIDER_ACTION_SCHEMA_VERSION, DEFAULT_PROVIDER_OBSERVATION_SCHEMA_VERSION,
    WorldEvent, provider_phase1_required_actions, provider_phase1_required_capabilities,
};
#[cfg(test)]
use oasis7_simulator::RejectReason as SimulatorRejectReason;
use oasis7_simulator::{
    Agent, AgentActivityProjectionV1, AgentExecutionDebugContext, Factory, Location, ResourceOwner,
    WorldConfig, WorldEventKind, WorldModel,
};
use std::collections::BTreeMap;

#[path = "mapping_events.rs"]
mod mapping_events;
#[path = "mapping_helpers.rs"]
mod mapping_helpers;
pub(super) use mapping_events::{
    action_accepted_player_feedback, fallback_non_empty, material_stack_summary,
    runtime_fallback_event_kind, runtime_reject_reason_to_simulator, runtime_structured_event,
    seed_location_id_for_agent_or_pos, seed_location_id_for_pos,
};
#[cfg(test)]
#[path = "mapping_module_visual_tests.rs"]
mod module_visual_tests;

pub(super) fn runtime_state_to_simulator_model(
    state: &crate::runtime::WorldState,
    sidecar: &RuntimeLlmSidecar,
    seed_model: Option<&WorldModel>,
) -> WorldModel {
    let mut model = seed_model.cloned().unwrap_or_default();
    // Seed module visuals are absorbed into the runtime state during
    // bootstrap.  Leaving them in this fallback model would resurrect an
    // entity after an authoritative removal.
    model.module_visual_entities.clear();
    model.agents.clear();
    let runtime_power_statuses = runtime_storage_power_statuses(state);

    for (agent_id, cell) in &state.agents {
        let seeded_agent = seed_model.and_then(|seed| seed.agents.get(agent_id));
        let seeded_location = seeded_agent
            .and_then(|agent| model.locations.get(&agent.location_id))
            .filter(|location| location.pos == cell.state.pos)
            .cloned()
            .or_else(|| seed_location_for_runtime_agent(sidecar, agent_id, cell.state.pos));
        let location_id = seeded_location
            .as_ref()
            .map(|location| location.id.clone())
            .unwrap_or_else(|| location_id_for_pos(cell.state.pos));
        if let Some(location) = seeded_location {
            model
                .locations
                .entry(location_id.clone())
                .or_insert(location);
        } else {
            model
                .locations
                .entry(location_id.clone())
                .or_insert_with(|| {
                    Location::new(
                        location_id.clone(),
                        format!("runtime-{location_id}"),
                        cell.state.pos,
                    )
                });
        }

        let mut agent = Agent::new(agent_id.clone(), location_id, cell.state.pos);
        agent.body = cell.state.body.clone();
        agent.resources = cell.state.resources.clone();
        agent.activity = cell
            .activity
            .as_ref()
            .map(runtime_agent_activity_to_simulator_projection);
        if let Some(power) = runtime_power_statuses.get(agent_id) {
            agent.power = power.clone();
        }
        model.agents.insert(agent_id.clone(), agent);
    }

    // Runtime factories supersede seed factories at their builder's canonical location.
    model.factories.clear();
    for factory in state.factories.values() {
        let location_id = model
            .agents
            .get(&factory.builder_agent_id)
            .map(|agent| agent.location_id.clone())
            .unwrap_or_else(|| factory.site_id.clone());
        model.factories.insert(
            factory.factory_id.clone(),
            Factory {
                id: factory.factory_id.clone(),
                owner: ResourceOwner::Agent {
                    agent_id: factory.builder_agent_id.clone(),
                },
                location_id,
                kind: factory.spec.factory_id.clone(),
            },
        );
    }

    // Module visual entities are part of the authoritative runtime snapshot;
    // project them after the seed model so production events can upsert the
    // same entity without mutating the seed model.
    for (entity_id, entity) in &state.module_visual_entities {
        model
            .module_visual_entities
            .insert(entity_id.clone(), entity.clone());
    }

    model.agent_prompt_profiles = sidecar.prompt_profiles.clone();
    model.agent_player_bindings = sidecar.agent_player_bindings.clone();
    model.agent_player_public_key_bindings = sidecar.agent_public_key_bindings.clone();
    model.agent_execution_debug_contexts = collect_agent_execution_debug_contexts(state, sidecar);
    model.player_auth_last_nonce = sidecar.player_auth_last_nonce.clone();
    model
}

fn runtime_agent_activity_to_simulator_projection(
    activity: &crate::runtime::AgentActivityV1,
) -> AgentActivityProjectionV1 {
    AgentActivityProjectionV1 {
        status: match activity.status {
            AgentActivityStatus::Idle => "idle",
            AgentActivityStatus::Executing => "executing",
            AgentActivityStatus::Blocked => "blocked",
            AgentActivityStatus::Waiting => "waiting",
            AgentActivityStatus::Unavailable => "unavailable",
        }
        .to_string(),
        operation: activity.operation_kind.clone(),
        target: activity.target_id.clone(),
        reason: activity
            .reason_summary
            .clone()
            .or_else(|| activity.reason_code.clone()),
        updated_at: activity.updated_at,
    }
}

fn seed_location_for_runtime_agent(
    sidecar: &RuntimeLlmSidecar,
    agent_id: &str,
    pos: crate::geometry::GeoPos,
) -> Option<Location> {
    if let Some(location) = sidecar.seed_location_for_pos(pos) {
        return Some(location);
    }
    if agent_id != FORMAL_RELEASE_DEFAULT_BOOTSTRAP_AGENT_ID {
        return None;
    }
    formal_release_default_seed_location_for_pos(pos)
}

fn collect_agent_execution_debug_contexts(
    state: &crate::runtime::WorldState,
    sidecar: &RuntimeLlmSidecar,
) -> BTreeMap<String, AgentExecutionDebugContext> {
    if !matches!(sidecar.decision_mode, ViewerLiveDecisionMode::Llm) {
        return BTreeMap::new();
    }

    let Ok(Some(settings)) = runtime_provider_settings_from_env() else {
        return BTreeMap::new();
    };

    state
        .agents
        .keys()
        .map(|agent_id| {
            let fallback_reason = settings.fallback_reason.clone();
            let provider_check_snapshot = sidecar.provider_check_snapshot().cloned();
            (
                agent_id.clone(),
                AgentExecutionDebugContext {
                    provider_mode: Some("provider_loopback_http".to_string()),
                    compatibility_status: Some(if fallback_reason.is_some() {
                        "degraded".to_string()
                    } else {
                        "ready".to_string()
                    }),
                    provider_check_source: provider_check_snapshot
                        .as_ref()
                        .map(|snapshot| snapshot.source.clone()),
                    provider_check_status: provider_check_snapshot
                        .as_ref()
                        .map(|snapshot| snapshot.status.clone()),
                    execution_mode: Some(settings.execution_mode.as_str().to_string()),
                    observation_schema_version: Some(
                        DEFAULT_PROVIDER_OBSERVATION_SCHEMA_VERSION.to_string(),
                    ),
                    action_schema_version: Some(DEFAULT_PROVIDER_ACTION_SCHEMA_VERSION.to_string()),
                    environment_class: Some("runtime_live".to_string()),
                    capabilities: provider_phase1_required_capabilities()
                        .iter()
                        .map(|value| (*value).to_string())
                        .collect(),
                    supported_action_sets: provider_phase1_required_actions()
                        .iter()
                        .map(|value| (*value).to_string())
                        .collect(),
                    provider_reported_capabilities: provider_check_snapshot
                        .as_ref()
                        .map(|snapshot| snapshot.capabilities.clone())
                        .unwrap_or_default(),
                    provider_reported_supported_action_sets: provider_check_snapshot
                        .as_ref()
                        .map(|snapshot| snapshot.supported_action_sets.clone())
                        .unwrap_or_default(),
                    fallback_reason,
                    provider_check_fallback_reason: provider_check_snapshot
                        .as_ref()
                        .and_then(|snapshot| snapshot.fallback_reason.clone()),
                    provider_check_error: provider_check_snapshot
                        .as_ref()
                        .and_then(|snapshot| snapshot.error.clone()),
                    provider_config_ref: Some(format!(
                        "provider://loopback-http/runtime-live/{}",
                        agent_id
                    )),
                    agent_profile: Some(settings.agent_profile.clone()),
                },
            )
        })
        .collect()
}

pub(super) fn map_runtime_event(
    runtime_event: &RuntimeWorldEvent,
    config: &WorldConfig,
    seed_model: Option<&WorldModel>,
) -> WorldEvent {
    // Runtime-live compat keeps unmapped runtime events visible to the viewer
    // while preserving the canonical runtime payload for later inspection.
    let kind = match &runtime_event.body {
        RuntimeWorldEventBody::Domain(domain) => {
            map_runtime_domain_event(domain, config, seed_model)
                .unwrap_or_else(|| runtime_fallback_event_kind(runtime_event))
        }
        _ => runtime_fallback_event_kind(runtime_event),
    };

    WorldEvent {
        id: runtime_event.id,
        time: runtime_event.time,
        kind,
        runtime_event: Some(runtime_event.clone()),
    }
}

pub(super) fn map_runtime_domain_event(
    event: &RuntimeDomainEvent,
    config: &WorldConfig,
    seed_model: Option<&WorldModel>,
) -> Option<WorldEventKind> {
    match event {
        RuntimeDomainEvent::AgentRegistered { agent_id, pos } => {
            Some(WorldEventKind::AgentRegistered {
                agent_id: agent_id.clone(),
                location_id: seed_location_id_for_agent_or_pos(seed_model, agent_id, *pos)
                    .unwrap_or_else(|| location_id_for_pos(*pos)),
                pos: *pos,
            })
        }
        RuntimeDomainEvent::AgentMoved { agent_id, from, to } => {
            let distance_cm = space_distance_cm(*from, *to);
            Some(WorldEventKind::AgentMoved {
                agent_id: agent_id.clone(),
                from: seed_location_id_for_pos(seed_model, *from)
                    .unwrap_or_else(|| location_id_for_pos(*from)),
                to: seed_location_id_for_pos(seed_model, *to)
                    .unwrap_or_else(|| location_id_for_pos(*to)),
                distance_cm,
                electricity_cost: config.movement_cost(distance_cm),
            })
        }
        RuntimeDomainEvent::ResourceTransferred {
            from_agent_id,
            to_agent_id,
            kind,
            amount,
        } => Some(WorldEventKind::ResourceTransferred {
            from: ResourceOwner::Agent {
                agent_id: from_agent_id.clone(),
            },
            to: ResourceOwner::Agent {
                agent_id: to_agent_id.clone(),
            },
            kind: *kind,
            amount: *amount,
        }),
        RuntimeDomainEvent::ActionRejected { reason, .. } => Some(WorldEventKind::ActionRejected {
            reason: runtime_reject_reason_to_simulator(reason),
        }),
        RuntimeDomainEvent::ActionAccepted {
            action_id,
            action_kind,
            actor_id,
            eta_ticks,
            notes,
        } => Some(runtime_structured_event(
            "runtime.action_accepted",
            format!(
                "action_id={action_id} action_kind={} actor_id={} eta_ticks={eta_ticks} player_feedback={}",
                fallback_non_empty(action_kind, "unknown_action"),
                fallback_non_empty(actor_id, "system"),
                action_accepted_player_feedback(notes),
            ),
        )),
        RuntimeDomainEvent::WarDeclared {
            war_id,
            objective,
            intensity,
            ..
        } => Some(runtime_structured_event(
            "runtime.gameplay.war_declared",
            format!(
                "war_id={} objective={} intensity={intensity}",
                fallback_non_empty(war_id, "unknown_war"),
                fallback_non_empty(objective, "unknown_objective"),
            ),
        )),
        RuntimeDomainEvent::WarConcluded {
            war_id,
            winner_alliance_id,
            summary,
            ..
        } => Some(runtime_structured_event(
            "runtime.gameplay.war_concluded",
            format!(
                "war_id={} winner={} summary={}",
                fallback_non_empty(war_id, "unknown_war"),
                fallback_non_empty(winner_alliance_id, "unknown_winner"),
                fallback_non_empty(summary, "none"),
            ),
        )),
        RuntimeDomainEvent::GovernanceProposalOpened {
            proposal_key,
            title,
            closes_at,
            ..
        } => Some(runtime_structured_event(
            "runtime.gameplay.governance_proposal_opened",
            format!(
                "proposal_key={} title={} closes_at={closes_at}",
                fallback_non_empty(proposal_key, "unknown_proposal"),
                fallback_non_empty(title, "untitled"),
            ),
        )),
        RuntimeDomainEvent::GovernanceVoteCast {
            proposal_key,
            option,
            weight,
            ..
        } => Some(runtime_structured_event(
            "runtime.gameplay.governance_vote_cast",
            format!(
                "proposal_key={} option={} weight={weight}",
                fallback_non_empty(proposal_key, "unknown_proposal"),
                fallback_non_empty(option, "unknown_option"),
            ),
        )),
        RuntimeDomainEvent::GovernanceProposalFinalized {
            proposal_key,
            winning_option,
            passed,
            ..
        } => Some(runtime_structured_event(
            "runtime.gameplay.governance_proposal_finalized",
            format!(
                "proposal_key={} winning_option={} passed={passed}",
                fallback_non_empty(proposal_key, "unknown_proposal"),
                winning_option.as_deref().unwrap_or("none"),
            ),
        )),
        RuntimeDomainEvent::CrisisSpawned {
            crisis_id,
            kind,
            severity,
            expires_at,
        } => Some(runtime_structured_event(
            "runtime.gameplay.crisis_spawned",
            format!(
                "crisis_id={} kind={} severity={severity} expires_at={expires_at}",
                fallback_non_empty(crisis_id, "unknown_crisis"),
                fallback_non_empty(kind, "unknown_kind"),
            ),
        )),
        RuntimeDomainEvent::CrisisResolved {
            crisis_id,
            strategy,
            success,
            impact,
            ..
        } => Some(runtime_structured_event(
            "runtime.gameplay.crisis_resolved",
            format!(
                "crisis_id={} strategy={} success={success} impact={impact}",
                fallback_non_empty(crisis_id, "unknown_crisis"),
                fallback_non_empty(strategy, "unknown_strategy"),
            ),
        )),
        RuntimeDomainEvent::CrisisTimedOut {
            crisis_id,
            penalty_impact,
        } => Some(runtime_structured_event(
            "runtime.gameplay.crisis_timed_out",
            format!(
                "crisis_id={} penalty_impact={penalty_impact}",
                fallback_non_empty(crisis_id, "unknown_crisis"),
            ),
        )),
        RuntimeDomainEvent::EconomicContractOpened {
            contract_id,
            counterparty_agent_id,
            settlement_amount,
            expires_at,
            ..
        } => Some(runtime_structured_event(
            "runtime.gameplay.economic_contract_opened",
            format!(
                "contract_id={} counterparty={} settlement_amount={settlement_amount} expires_at={expires_at}",
                fallback_non_empty(contract_id, "unknown_contract"),
                fallback_non_empty(counterparty_agent_id, "unknown_counterparty"),
            ),
        )),
        RuntimeDomainEvent::EconomicContractAccepted {
            contract_id,
            accepter_agent_id,
        } => Some(runtime_structured_event(
            "runtime.gameplay.economic_contract_accepted",
            format!(
                "contract_id={} accepter={}",
                fallback_non_empty(contract_id, "unknown_contract"),
                fallback_non_empty(accepter_agent_id, "unknown_accepter"),
            ),
        )),
        RuntimeDomainEvent::EconomicContractSettled {
            contract_id,
            success,
            transfer_amount,
            tax_amount,
            ..
        } => Some(runtime_structured_event(
            "runtime.gameplay.economic_contract_settled",
            format!(
                "contract_id={} success={success} transfer_amount={transfer_amount} tax_amount={tax_amount}",
                fallback_non_empty(contract_id, "unknown_contract"),
            ),
        )),
        RuntimeDomainEvent::EconomicContractExpired {
            contract_id,
            creator_agent_id,
            counterparty_agent_id,
            ..
        } => Some(runtime_structured_event(
            "runtime.gameplay.economic_contract_expired",
            format!(
                "contract_id={} creator={} counterparty={}",
                fallback_non_empty(contract_id, "unknown_contract"),
                fallback_non_empty(creator_agent_id, "unknown_creator"),
                fallback_non_empty(counterparty_agent_id, "unknown_counterparty"),
            ),
        )),
        RuntimeDomainEvent::MetaProgressGranted {
            target_agent_id,
            track,
            points,
            achievement_id,
            ..
        } => Some(runtime_structured_event(
            "runtime.gameplay.meta_progress_granted",
            format!(
                "target={} track={} points={points} achievement_id={}",
                fallback_non_empty(target_agent_id, "unknown_target"),
                fallback_non_empty(track, "unknown_track"),
                achievement_id.as_deref().unwrap_or("none"),
            ),
        )),
        RuntimeDomainEvent::FactoryBuilt {
            builder_agent_id,
            site_id,
            spec,
            ..
        } => Some(runtime_structured_event(
            "runtime.economy.factory_built",
            format!(
                "factory={} builder={} site={}",
                fallback_non_empty(&spec.factory_id, "unknown_factory"),
                fallback_non_empty(builder_agent_id, "unknown_builder"),
                fallback_non_empty(site_id, "unknown_site"),
            ),
        )),
        RuntimeDomainEvent::RecipeStarted {
            requester_agent_id,
            factory_id,
            recipe_id,
            accepted_batches,
            produce,
            ..
        } => Some(runtime_structured_event(
            "runtime.economy.recipe_started",
            format!(
                "factory={} recipe={} requester={} batches={accepted_batches} outputs={}",
                fallback_non_empty(factory_id, "unknown_factory"),
                fallback_non_empty(recipe_id, "unknown_recipe"),
                fallback_non_empty(requester_agent_id, "unknown_requester"),
                material_stack_summary(produce),
            ),
        )),
        RuntimeDomainEvent::RecipeCompleted {
            requester_agent_id,
            factory_id,
            recipe_id,
            accepted_batches,
            produce,
            ..
        } => Some(runtime_structured_event(
            "runtime.economy.recipe_completed",
            format!(
                "factory={} recipe={} requester={} batches={accepted_batches} outputs={}",
                fallback_non_empty(factory_id, "unknown_factory"),
                fallback_non_empty(recipe_id, "unknown_recipe"),
                fallback_non_empty(requester_agent_id, "unknown_requester"),
                material_stack_summary(produce),
            ),
        )),
        RuntimeDomainEvent::FactoryProductionBlocked {
            requester_agent_id,
            factory_id,
            recipe_id,
            blocker_kind,
            blocker_detail,
            ..
        } => Some(runtime_structured_event(
            "runtime.economy.factory_production_blocked",
            format!(
                "factory={} recipe={} requester={} reason={} detail={}",
                fallback_non_empty(factory_id, "unknown_factory"),
                fallback_non_empty(recipe_id, "unknown_recipe"),
                fallback_non_empty(requester_agent_id, "unknown_requester"),
                fallback_non_empty(blocker_kind, "unknown_reason"),
                fallback_non_empty(blocker_detail, "none"),
            ),
        )),
        RuntimeDomainEvent::FactoryProductionResumed {
            requester_agent_id,
            factory_id,
            recipe_id,
            previous_blocker_kind,
            previous_blocker_detail,
            ..
        } => Some(runtime_structured_event(
            "runtime.economy.factory_production_resumed",
            format!(
                "factory={} recipe={} requester={} previous_reason={} previous_detail={}",
                fallback_non_empty(factory_id, "unknown_factory"),
                fallback_non_empty(recipe_id, "unknown_recipe"),
                fallback_non_empty(requester_agent_id, "unknown_requester"),
                previous_blocker_kind.as_deref().unwrap_or("none"),
                previous_blocker_detail.as_deref().unwrap_or("none"),
            ),
        )),
        _ => None,
    }
}

#[cfg(test)]
#[path = "mapping_tests.rs"]
mod tests;
