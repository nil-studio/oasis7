use super::control_plane::{
    ensure_agent_player_access_runtime, map_auth_verify_error_code, normalize_optional_public_key,
};
use super::*;
use crate::runtime::Action as RuntimeAction;
use crate::simulator::WorldKernel;
use crate::viewer::{
    GameplayActionError, ScheduleRecipeQuotePreflight, ScheduleRecipeQuoteRequest,
    VerifiedPlayerAuth, verify_schedule_recipe_quote_auth_proof,
};
use oasis7_agent_api::world_action::ResourceKind;
use oasis7_simulator::ResourceOwner;

impl ViewerRuntimeLiveServer {
    /// Computes the canonical ScheduleRecipe quote for the authenticated player's bound Agent.
    /// This is a read-only mapping: it neither consumes the auth proof nor schedules production.
    pub(super) fn handle_schedule_recipe_quote(
        &mut self,
        request: ScheduleRecipeQuoteRequest,
    ) -> Result<ScheduleRecipeQuotePreflight, GameplayActionError> {
        let verified = self.verify_schedule_recipe_quote_auth(&request)?;
        self.session_policy
            .validate_known_session_key(verified.player_id.as_str(), verified.public_key.as_str())
            .map_err(|message| GameplayActionError {
                code: map_session_policy_error_code(message.as_str()).to_string(),
                message,
                action_id: Some("quote_schedule_recipe".to_string()),
                target_agent_id: None,
            })?;
        let agent_id = self
            .llm_sidecar
            .bound_agent_for_player(verified.player_id.as_str())
            .ok_or_else(|| GameplayActionError {
                code: "player_agent_binding_required".to_string(),
                message: "quote_schedule_recipe requires a bound player Agent session".to_string(),
                action_id: Some("quote_schedule_recipe".to_string()),
                target_agent_id: None,
            })?;
        let public_key = normalize_optional_public_key(request.public_key.as_deref());
        ensure_agent_player_access_runtime(
            &self.world,
            &self.llm_sidecar,
            agent_id,
            verified.player_id.as_str(),
            public_key.as_deref(),
        )
        .map_err(|err| GameplayActionError {
            code: err.code,
            message: err.message,
            action_id: Some("quote_schedule_recipe".to_string()),
            target_agent_id: err.agent_id,
        })?;

        // Factory electricity is held by its canonical builder.  Keep the
        // quote's requester/owner boundary explicit here instead of allowing
        // the simulator snapshot or the world pool to select a payer.
        let factory_builder_agent_id = self
            .world
            .state()
            .factories
            .get(request.factory_id.as_str())
            .ok_or_else(|| GameplayActionError {
                code: "schedule_recipe_quote_rejected".to_string(),
                message: format!(
                    "quote_schedule_recipe rejected: factory not found: {}",
                    request.factory_id
                ),
                action_id: Some("quote_schedule_recipe".to_string()),
                target_agent_id: Some(agent_id.to_string()),
            })
            .map(|factory| factory.builder_agent_id.clone())?;
        if factory_builder_agent_id != agent_id {
            return Err(GameplayActionError {
                code: "schedule_recipe_quote_rejected".to_string(),
                message: format!(
                    "quote_schedule_recipe rejected: factory owner mismatch: {}",
                    request.factory_id
                ),
                action_id: Some("quote_schedule_recipe".to_string()),
                target_agent_id: Some(agent_id.to_string()),
            });
        }

        let model = mapping::runtime_state_to_simulator_model(
            self.world.state(),
            &self.llm_sidecar,
            self.seed_model.as_ref(),
        );
        let quote = WorldKernel::with_model(self.snapshot_config.clone(), model)
            .query_schedule_recipe(
                &ResourceOwner::Agent {
                    agent_id: agent_id.to_string(),
                },
                request.factory_id.as_str(),
                request.recipe_id.as_str(),
                request.batches,
                oasis7_simulator::ScheduleResourceScope::HostLedgers,
            )
            .map_err(|reason| GameplayActionError {
                code: "schedule_recipe_quote_rejected".to_string(),
                message: format!("quote_schedule_recipe rejected: {reason:?}"),
                action_id: Some("quote_schedule_recipe".to_string()),
                target_agent_id: Some(agent_id.to_string()),
            })?;
        let canonical_plan = match crate::viewer::gameplay_actions::runtime_schedule_recipe_action(
            agent_id,
            request.factory_id.as_str(),
            request.recipe_id.as_str(),
            u32::try_from(request.batches).map_err(|_| GameplayActionError {
                code: "schedule_recipe_quote_rejected".to_string(),
                message: "quote_schedule_recipe batches exceed supported range".to_string(),
                action_id: Some("quote_schedule_recipe".to_string()),
                target_agent_id: Some(agent_id.to_string()),
            })?,
        ) {
            Some(RuntimeAction::ScheduleRecipe { plan, .. }) => plan,
            _ => {
                return Err(GameplayActionError {
                    code: "schedule_recipe_quote_rejected".to_string(),
                    message: format!(
                        "quote_schedule_recipe has no canonical runtime plan for recipe `{}`",
                        request.recipe_id
                    ),
                    action_id: Some("quote_schedule_recipe".to_string()),
                    target_agent_id: Some(agent_id.to_string()),
                });
            }
        };
        if canonical_plan.power_required < 0 {
            return Err(GameplayActionError {
                code: "schedule_recipe_quote_rejected".to_string(),
                message: "quote_schedule_recipe canonical runtime plan has negative power cost"
                    .to_string(),
                action_id: Some("quote_schedule_recipe".to_string()),
                target_agent_id: Some(agent_id.to_string()),
            });
        }
        let available_electricity = self
            .world
            .state()
            .agents
            .get(factory_builder_agent_id.as_str())
            .ok_or_else(|| GameplayActionError {
                code: "schedule_recipe_quote_rejected".to_string(),
                message: format!(
                    "quote_schedule_recipe rejected: factory builder agent is missing: {}",
                    factory_builder_agent_id
                ),
                action_id: Some("quote_schedule_recipe".to_string()),
                target_agent_id: Some(agent_id.to_string()),
            })?
            .state
            .resources
            .get(ResourceKind::Electricity);
        if available_electricity < canonical_plan.power_required {
            return Err(GameplayActionError {
                code: "schedule_recipe_quote_rejected".to_string(),
                message: format!(
                    "quote_schedule_recipe canonical runtime plan requires {} electricity, available {}",
                    canonical_plan.power_required, available_electricity
                ),
                action_id: Some("quote_schedule_recipe".to_string()),
                target_agent_id: Some(agent_id.to_string()),
            });
        }
        let Some(primary_output) = canonical_plan.produce.iter().find(|stack| stack.amount > 0)
        else {
            return Err(GameplayActionError {
                code: "schedule_recipe_quote_rejected".to_string(),
                message: "quote_schedule_recipe canonical runtime plan has no primary output"
                    .to_string(),
                action_id: Some("quote_schedule_recipe".to_string()),
                target_agent_id: Some(agent_id.to_string()),
            });
        };
        let readiness = self
            .world
            .quote_recipe_execution_readiness(
                request.factory_id.as_str(),
                request.recipe_id.as_str(),
                &canonical_plan,
            )
            .map_err(|message| GameplayActionError {
                code: "schedule_recipe_quote_rejected".to_string(),
                message,
                action_id: Some("quote_schedule_recipe".to_string()),
                target_agent_id: Some(agent_id.to_string()),
            })?;
        let local_shortage_delay_ticks = readiness.local_shortage_delay_ticks;
        let shortage_reason = readiness.shortage_reason;
        Ok(ScheduleRecipeQuotePreflight {
            owner_agent_id: agent_id.to_string(),
            factory_id: quote.factory_id,
            recipe_id: quote.recipe_id,
            batches: quote.batches,
            // The runtime action plan is the duration authority.  In particular,
            // batches scale resource/material quantities, while the built-in
            // plan's one-tick duration is the execution base duration.
            base_duration_ticks: i64::from(canonical_plan.duration_ticks.max(1)),
            electricity_cost: canonical_plan.power_required,
            electricity_after: available_electricity.saturating_sub(canonical_plan.power_required),
            // Runtime execution has no generic hardware/data accounting for this action;
            // material-ledger inputs and primary output below are the authoritative fields.
            hardware_cost: 0,
            data_output: 0,
            finished_product_id: primary_output.kind.clone(),
            finished_product_units: primary_output.amount,
            local_shortage_delay_ticks,
            shortage_reason,
            recommended_pre_step: quote.recommended_pre_step,
            runway_before_ticks: quote.runway_before_ticks,
            runway_after_ticks: quote.runway_after_ticks,
            downtime_threshold_ppm: quote.downtime_threshold_ppm,
            continue_production_risk: quote.continue_production_risk,
            maintenance_pressure_delta: quote.maintenance_pressure_delta,
            recommended_maintenance_action: quote.recommended_maintenance_action,
        })
    }

    fn verify_schedule_recipe_quote_auth(
        &self,
        request: &ScheduleRecipeQuoteRequest,
    ) -> Result<VerifiedPlayerAuth, GameplayActionError> {
        let auth = request.auth.as_ref().ok_or_else(|| GameplayActionError {
            code: "auth_proof_required".to_string(),
            message: "quote_schedule_recipe requires auth proof".to_string(),
            action_id: Some("quote_schedule_recipe".to_string()),
            target_agent_id: None,
        })?;
        verify_schedule_recipe_quote_auth_proof(request, auth).map_err(|message| {
            GameplayActionError {
                code: map_auth_verify_error_code(message.as_str()).to_string(),
                message,
                action_id: Some("quote_schedule_recipe".to_string()),
                target_agent_id: None,
            }
        })
    }
}
