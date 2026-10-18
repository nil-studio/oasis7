use super::super::auth::{
    VerifiedPlayerAuth, verify_gameplay_action_auth_proof,
    verify_product_validation_quote_auth_proof, verify_refine_quote_auth_proof,
};
use super::super::gameplay_actions::{
    ACTION_BUILD_ASSEMBLER_MK1, ACTION_BUILD_SMELTER_MK1, ACTION_CLAIM_FIRST_AGENT,
    ACTION_CLAIM_STARTER_OC, ACTION_RELEASE_AGENT_CLAIM, ACTION_SCHEDULE_ASSEMBLER_CONTROL_CHIP,
    ACTION_SCHEDULE_ASSEMBLER_FACTORY_CORE, ACTION_SCHEDULE_ASSEMBLER_GEAR,
    ACTION_SCHEDULE_ASSEMBLER_LOGISTICS_DRONE, ACTION_SCHEDULE_ASSEMBLER_MODULE_RACK,
    ACTION_SCHEDULE_ASSEMBLER_MOTOR_MK1, ACTION_SCHEDULE_ASSEMBLER_SENSOR_PACK,
    ACTION_SCHEDULE_SMELTER_ALLOY_PLATE, ACTION_SCHEDULE_SMELTER_COPPER_WIRE,
    ACTION_SCHEDULE_SMELTER_IRON_INGOT, ACTION_SCHEDULE_SMELTER_POLYMER_RESIN,
    FACTORY_ASSEMBLER_MK1, FACTORY_SMELTER_MK1, FIRST_AGENT_CLAIM_TARGET_AGENT_ID,
    build_runtime_action_from_gameplay_request, gameplay_action_requires_actor_agent,
};
use super::super::protocol::{
    CollectDataPreflight, GameplayActionAck, GameplayActionError, GameplayActionRequest,
    ProductValidationQuotePreflight, ProductValidationQuoteRequest, RefineQuotePreflight,
    RefineQuoteRequest,
};
use super::control_plane::{
    ensure_agent_player_access_runtime, ensure_agent_player_binding_target_runtime,
    map_auth_verify_error_code, normalize_optional_public_key,
};
use super::*;
use crate::runtime::{IndustryStage, StarterIndustrialFeasibilityResult, WorldState};
use crate::simulator::{PlayerGameplayAction, PlayerGameplayRecentFeedback, WorldKernel};
use oasis7_simulator::ResourceOwner;
use oasis7_wasm_abi::MaterialStack;
#[path = "power_survival_quote.rs"]
mod power_survival_quote;
#[path = "schedule_readiness.rs"]
mod schedule_readiness;
use schedule_readiness::{
    factory_build_disabled_reason, schedule_recipe_data_advisory, schedule_recipe_disabled_reason,
};
#[path = "smelter_actions.rs"]
mod smelter_actions;
use smelter_actions::extend_smelter_actions;

const GAMEPLAY_ACTION_PROTOCOL: &str = "gameplay_action.submit";

pub(super) enum CollectDataResult {
    Preflight(CollectDataPreflight),
    Submit(GameplayActionAck),
}

/// Quotes the exact cost and yield parameters a caller plans to submit via `CollectData`.
///
/// `CollectData` deliberately has no global default cost or yield. Callers must supply the
/// exact action parameters so this preflight remains aligned with runtime enforcement.
pub(super) fn data_collection_preflight(
    collector_agent_id: String,
    available_electricity: i64,
    electricity_cost: i64,
    data_amount: i64,
) -> CollectDataPreflight {
    let invalid_reason = if electricity_cost <= 0 {
        Some("collection electricity cost must be positive".to_string())
    } else if data_amount <= 0 {
        Some("collection data amount must be positive".to_string())
    } else {
        None
    };
    let insufficient_electricity =
        invalid_reason.is_none() && available_electricity < electricity_cost;
    let can_execute = invalid_reason.is_none() && !insufficient_electricity;
    let blocked_reason = invalid_reason.or_else(|| {
        insufficient_electricity.then(|| {
            format!(
                "insufficient electricity: need {electricity_cost}, have {available_electricity}"
            )
        })
    });

    CollectDataPreflight {
        data_owner_agent_id: collector_agent_id.clone(),
        data_recipient_agent_id: collector_agent_id.clone(),
        collector_agent_id,
        data_use: "self_collection".to_string(),
        permission_status: "self_owned_no_grant_required".to_string(),
        electricity_cost,
        data_amount,
        available_electricity,
        electricity_after: if can_execute {
            available_electricity - electricity_cost
        } else {
            available_electricity
        },
        can_execute,
        recovery_guidance: insufficient_electricity.then(|| {
            "replenish electricity before collecting data, or defer collection until electricity is available"
                .to_string()
        }),
        alternative_action: insufficient_electricity.then(|| {
            "replenish_electricity_or_defer_collection".to_string()
        }),
        blocked_reason,
    }
}

pub(super) fn supports_runtime_gameplay_actions() -> bool {
    true
}

pub(super) use super::starter_industrial_outcome::player_starter_industrial_feasibility;

fn starter_assembler_build_disabled_reason(
    feasibility: &StarterIndustrialFeasibilityResult,
) -> Option<String> {
    feasibility.disabled_reason()
}
pub(super) fn extend_available_actions(
    state: &WorldState,
    starter_industrial_feasibility: &StarterIndustrialFeasibilityResult,
    first_agent_id: Option<&str>,
    first_agent_claim_target_available: bool,
    actions: &mut Vec<PlayerGameplayAction>,
) {
    if !supports_runtime_gameplay_actions() {
        return;
    }
    let Some(agent_id) = first_agent_id else {
        if first_agent_claim_target_available {
            actions.push(PlayerGameplayAction {
                action_id: ACTION_CLAIM_FIRST_AGENT.to_string(),
                label: "Claim first Agent".to_string(),
                protocol_action: GAMEPLAY_ACTION_PROTOCOL.to_string(),
                target_agent_id: Some(FIRST_AGENT_CLAIM_TARGET_AGENT_ID.to_string()),
                disabled_reason: None,
            });
        }
        return;
    };
    let agent_exists = state.agents.contains_key(agent_id);
    let starter_oc_required = state
        .main_token_balances
        .get(agent_id)
        .map(|balance| balance.liquid_balance == 0)
        .unwrap_or(true)
        && !state.starter_oc_claims.contains_key(agent_id);
    if starter_oc_required {
        let disabled_reason =
            "claim starter OC before using LLM/agent chat for this Agent".to_string();
        for action in actions.iter_mut() {
            if action.action_id == "chat_first_agent" {
                action.disabled_reason = Some(disabled_reason.clone());
            }
        }
        if !agent_exists {
            return;
        }
        actions.push(PlayerGameplayAction {
            action_id: ACTION_CLAIM_STARTER_OC.to_string(),
            label: "Claim starter OC".to_string(),
            protocol_action: GAMEPLAY_ACTION_PROTOCOL.to_string(),
            target_agent_id: Some(agent_id.to_string()),
            disabled_reason: None,
        });
    }

    let smelter_exists = state.factories.contains_key(FACTORY_SMELTER_MK1);
    let assembler_exists = state.factories.contains_key(FACTORY_ASSEMBLER_MK1);
    let industry_stage = state.industry_progress.stage;

    if !smelter_exists {
        actions.push(PlayerGameplayAction {
            action_id: ACTION_BUILD_SMELTER_MK1.to_string(),
            label: "Queue Smelter MK1 construction".to_string(),
            protocol_action: GAMEPLAY_ACTION_PROTOCOL.to_string(),
            target_agent_id: Some(agent_id.to_string()),
            disabled_reason: factory_build_disabled_reason(
                state,
                agent_id,
                ACTION_BUILD_SMELTER_MK1,
            ),
        });
        return;
    }

    extend_smelter_actions(state, agent_id, industry_stage, actions);

    if !assembler_exists {
        actions.push(PlayerGameplayAction {
            action_id: ACTION_BUILD_ASSEMBLER_MK1.to_string(),
            label: "Queue Assembler MK1 construction".to_string(),
            protocol_action: GAMEPLAY_ACTION_PROTOCOL.to_string(),
            target_agent_id: Some(agent_id.to_string()),
            disabled_reason: starter_assembler_build_disabled_reason(
                starter_industrial_feasibility,
            )
            .or_else(|| factory_build_disabled_reason(state, agent_id, ACTION_BUILD_ASSEMBLER_MK1)),
        });
        return;
    }

    actions.extend([
        PlayerGameplayAction {
            action_id: ACTION_SCHEDULE_ASSEMBLER_GEAR.to_string(),
            label: "Queue gear run".to_string(),
            protocol_action: GAMEPLAY_ACTION_PROTOCOL.to_string(),
            target_agent_id: Some(agent_id.to_string()),
            disabled_reason: schedule_recipe_disabled_reason(
                state,
                agent_id,
                ACTION_SCHEDULE_ASSEMBLER_GEAR,
            ),
        },
        PlayerGameplayAction {
            action_id: ACTION_SCHEDULE_ASSEMBLER_CONTROL_CHIP.to_string(),
            label: "Queue control chip run".to_string(),
            protocol_action: GAMEPLAY_ACTION_PROTOCOL.to_string(),
            target_agent_id: Some(agent_id.to_string()),
            disabled_reason: schedule_recipe_disabled_reason(
                state,
                agent_id,
                ACTION_SCHEDULE_ASSEMBLER_CONTROL_CHIP,
            ),
        },
        PlayerGameplayAction {
            action_id: ACTION_SCHEDULE_ASSEMBLER_MOTOR_MK1.to_string(),
            label: "Queue motor MK1 run".to_string(),
            protocol_action: GAMEPLAY_ACTION_PROTOCOL.to_string(),
            target_agent_id: Some(agent_id.to_string()),
            disabled_reason: schedule_recipe_disabled_reason(
                state,
                agent_id,
                ACTION_SCHEDULE_ASSEMBLER_MOTOR_MK1,
            ),
        },
        PlayerGameplayAction {
            action_id: ACTION_SCHEDULE_ASSEMBLER_LOGISTICS_DRONE.to_string(),
            label: "Queue logistics drone run".to_string(),
            protocol_action: GAMEPLAY_ACTION_PROTOCOL.to_string(),
            target_agent_id: Some(agent_id.to_string()),
            disabled_reason: schedule_recipe_disabled_reason(
                state,
                agent_id,
                ACTION_SCHEDULE_ASSEMBLER_LOGISTICS_DRONE,
            ),
        },
        PlayerGameplayAction {
            action_id: ACTION_SCHEDULE_ASSEMBLER_SENSOR_PACK.to_string(),
            label: "Queue sensor pack run".to_string(),
            protocol_action: GAMEPLAY_ACTION_PROTOCOL.to_string(),
            target_agent_id: Some(agent_id.to_string()),
            disabled_reason: stage_gate_disabled_reason(industry_stage, IndustryStage::ScaleOut)
                .or_else(|| {
                    schedule_recipe_disabled_reason(
                        state,
                        agent_id,
                        ACTION_SCHEDULE_ASSEMBLER_SENSOR_PACK,
                    )
                }),
        },
        PlayerGameplayAction {
            action_id: ACTION_SCHEDULE_ASSEMBLER_MODULE_RACK.to_string(),
            label: "Queue module rack run".to_string(),
            protocol_action: GAMEPLAY_ACTION_PROTOCOL.to_string(),
            target_agent_id: Some(agent_id.to_string()),
            disabled_reason: stage_gate_disabled_reason(industry_stage, IndustryStage::Governance)
                .or_else(|| {
                    schedule_recipe_disabled_reason(
                        state,
                        agent_id,
                        ACTION_SCHEDULE_ASSEMBLER_MODULE_RACK,
                    )
                }),
        },
        PlayerGameplayAction {
            action_id: ACTION_SCHEDULE_ASSEMBLER_FACTORY_CORE.to_string(),
            label: "Queue factory core run".to_string(),
            protocol_action: GAMEPLAY_ACTION_PROTOCOL.to_string(),
            target_agent_id: Some(agent_id.to_string()),
            disabled_reason: stage_gate_disabled_reason(industry_stage, IndustryStage::Governance)
                .or_else(|| {
                    schedule_recipe_disabled_reason(
                        state,
                        agent_id,
                        ACTION_SCHEDULE_ASSEMBLER_FACTORY_CORE,
                    )
                }),
        },
    ]);
}

impl ViewerRuntimeLiveServer {
    /// Computes a signed, read-only product-validation preflight from the authoritative world.
    /// The quote does not execute a module, mutate a nonce, or submit a validation action.
    pub(super) fn handle_product_validation_quote(
        &mut self,
        request: ProductValidationQuoteRequest,
    ) -> Result<ProductValidationQuotePreflight, GameplayActionError> {
        let verified = self.verify_product_validation_quote_auth(&request)?;
        self.session_policy
            .validate_known_session_key(verified.player_id.as_str(), verified.public_key.as_str())
            .map_err(|message| GameplayActionError {
                code: map_session_policy_error_code(message.as_str()).to_string(),
                message,
                action_id: Some("quote_validate_product".to_string()),
                target_agent_id: None,
            })?;
        let agent_id = self
            .llm_sidecar
            .bound_agent_for_player(verified.player_id.as_str())
            .ok_or_else(|| GameplayActionError {
                code: "player_agent_binding_required".to_string(),
                message: "quote_validate_product requires a bound player Agent session".to_string(),
                action_id: Some("quote_validate_product".to_string()),
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
            action_id: Some("quote_validate_product".to_string()),
            target_agent_id: err.agent_id,
        })?;

        let product_id = request.product_id.trim();
        if product_id.is_empty() || request.amount <= 0 {
            return Err(GameplayActionError {
                code: "product_validation_quote_rejected".to_string(),
                message: "product validation quote requires a product id and positive amount"
                    .to_string(),
                action_id: Some("quote_validate_product".to_string()),
                target_agent_id: Some(agent_id.to_string()),
            });
        }
        let module_id = format!("m4.product.{product_id}");
        let quote = self
            .world
            .product_validation_quote(
                agent_id,
                module_id.as_str(),
                &MaterialStack::new(product_id, request.amount),
                0,
            )
            .map_err(|reason| GameplayActionError {
                code: "product_validation_quote_rejected".to_string(),
                message: format!("product validation quote rejected: {reason}"),
                action_id: Some("quote_validate_product".to_string()),
                target_agent_id: Some(agent_id.to_string()),
            })?;
        Ok(ProductValidationQuotePreflight {
            product_id: quote.product_id,
            product_role: quote.product_role,
            tradable: quote.tradable,
            stage_before: quote.stage_before,
            stage_after: quote.stage_after,
            unlock_or_value_class: quote.unlock_or_value_class,
            recommended_action: quote.recommended_action,
            submission_allowed: quote.submission_allowed,
            missing_prerequisite: quote.missing_prerequisite,
            reachable_advance_or_recovery: quote.reachable_advance_or_recovery,
        })
    }

    /// Computes the simulator-kernel quote from a fresh, read-only projection of runtime state.
    /// No runtime action, event, auth nonce, or player binding is mutated by this path.
    pub(super) fn handle_refine_quote(
        &mut self,
        request: RefineQuoteRequest,
    ) -> Result<RefineQuotePreflight, GameplayActionError> {
        let verified = self.verify_refine_quote_auth(&request)?;
        self.session_policy
            .validate_known_session_key(verified.player_id.as_str(), verified.public_key.as_str())
            .map_err(|message| GameplayActionError {
                code: map_session_policy_error_code(message.as_str()).to_string(),
                message,
                action_id: Some("quote_refine_compound".to_string()),
                target_agent_id: None,
            })?;
        let agent_id = self
            .llm_sidecar
            .bound_agent_for_player(verified.player_id.as_str())
            .ok_or_else(|| GameplayActionError {
                code: "player_agent_binding_required".to_string(),
                message: "quote_refine_compound requires a bound player Agent session".to_string(),
                action_id: Some("quote_refine_compound".to_string()),
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
            action_id: Some("quote_refine_compound".to_string()),
            target_agent_id: err.agent_id,
        })?;
        let model = super::mapping::runtime_state_to_simulator_model(
            self.world.state(),
            &self.llm_sidecar,
            self.seed_model.as_ref(),
        );
        let quote = WorldKernel::with_model(self.snapshot_config.clone(), model)
            .quote_refine_compound(
                &ResourceOwner::Agent {
                    agent_id: agent_id.to_string(),
                },
                request.compound_mass_g,
            )
            .map_err(|reason| GameplayActionError {
                code: "refine_quote_rejected".to_string(),
                message: format!("quote_refine_compound rejected: {reason:?}"),
                action_id: Some("quote_refine_compound".to_string()),
                target_agent_id: Some(agent_id.to_string()),
            })?;
        Ok(RefineQuotePreflight {
            owner_agent_id: agent_id.to_string(),
            compound_mass_g: quote.compound_mass_g,
            electricity_cost: quote.electricity_cost,
            electricity_after: quote.electricity_after,
            hardware_output: quote.hardware_output,
            target_id: "factory_build_hardware".to_string(),
            target_gap_before: quote.hardware_shortfall_before,
            target_gap_after: quote.hardware_shortfall_after,
            target_linkage: quote.first_goal_relevance,
            recommended_refine_amount: quote.recommended_refine_amount,
            value_classification: match quote.refine_value_class.as_str() {
                "enough_to_advance" | "enough_for_next_step" => "enough_to_advance".to_string(),
                "partial_progress" => "partial_progress".to_string(),
                _ => "poor_power_tradeoff".to_string(),
            },
        })
    }

    fn verify_refine_quote_auth(
        &self,
        request: &RefineQuoteRequest,
    ) -> Result<VerifiedPlayerAuth, GameplayActionError> {
        let auth = request.auth.as_ref().ok_or_else(|| GameplayActionError {
            code: "auth_proof_required".to_string(),
            message: "quote_refine_compound requires auth proof".to_string(),
            action_id: Some("quote_refine_compound".to_string()),
            target_agent_id: None,
        })?;
        verify_refine_quote_auth_proof(request, auth).map_err(|message| GameplayActionError {
            code: map_auth_verify_error_code(message.as_str()).to_string(),
            message,
            action_id: Some("quote_refine_compound".to_string()),
            target_agent_id: None,
        })
    }

    fn verify_product_validation_quote_auth(
        &self,
        request: &ProductValidationQuoteRequest,
    ) -> Result<VerifiedPlayerAuth, GameplayActionError> {
        let auth = request.auth.as_ref().ok_or_else(|| GameplayActionError {
            code: "auth_proof_required".to_string(),
            message: "quote_validate_product requires auth proof".to_string(),
            action_id: Some("quote_validate_product".to_string()),
            target_agent_id: None,
        })?;
        verify_product_validation_quote_auth_proof(request, auth).map_err(|message| {
            GameplayActionError {
                code: map_auth_verify_error_code(message.as_str()).to_string(),
                message,
                action_id: Some("quote_validate_product".to_string()),
                target_agent_id: None,
            }
        })
    }

    pub(super) fn handle_gameplay_action(
        &mut self,
        request: GameplayActionRequest,
    ) -> Result<GameplayActionAck, GameplayActionError> {
        if self.config.world_service.is_none() {
            self.ensure_gameplay_ready_for_action(
                "gameplay_action",
                Some(request.action_id.as_str()),
                Some(request.target_agent_id.as_str()),
            )
            .map_err(|(code, message)| GameplayActionError {
                code,
                message,
                action_id: Some(request.action_id.clone()),
                target_agent_id: Some(request.target_agent_id.clone()),
            })?;
        }
        let verified = self.verify_gameplay_action_auth(&request)?;
        if self.config.world_service.is_some() {
            self.session_policy
                .validate_known_session_key(&verified.player_id, &verified.public_key)
                .map_err(|message| GameplayActionError {
                    code: map_session_policy_error_code(&message).into(),
                    message,
                    action_id: Some(request.action_id.clone()),
                    target_agent_id: Some(request.target_agent_id.clone()),
                })?;
            let (runtime_action_id, consensus_action_payload_hash) =
                self.submit_world_service_gameplay(&request)?;
            if runtime_action_id != 0 {
                self.runtime_action_players
                    .insert(runtime_action_id, verified.player_id.clone());
            }
            return Ok(GameplayActionAck {
                consensus_action_payload_hash,
                action_id: request.action_id,
                target_agent_id: request.target_agent_id,
                player_id: verified.player_id,
                runtime_action_id,
                accepted_at_tick: self.world.state().time,
                message: Some("received by world service; await committed outcome".into()),
            });
        }
        self.session_policy
            .validate_known_session_key(verified.player_id.as_str(), verified.public_key.as_str())
            .map_err(|message| GameplayActionError {
                code: map_session_policy_error_code(message.as_str()).to_string(),
                message,
                action_id: Some(request.action_id.clone()),
                target_agent_id: Some(request.target_agent_id.clone()),
            })?;
        self.llm_sidecar
            .consume_player_auth_nonce(verified.player_id.as_str(), verified.nonce)
            .map_err(|message| GameplayActionError {
                code: "auth_nonce_replay".to_string(),
                message,
                action_id: Some(request.action_id.clone()),
                target_agent_id: Some(request.target_agent_id.clone()),
            })?;

        self.authorize_active_gameplay_feedback();
        let public_key = normalize_optional_public_key(request.public_key.as_deref());
        let is_first_agent_claim = request.action_id == ACTION_CLAIM_FIRST_AGENT;
        if is_first_agent_claim {
            if request.target_agent_id != FIRST_AGENT_CLAIM_TARGET_AGENT_ID {
                return Err(GameplayActionError {
                    code: "invalid_first_agent_claim_target".to_string(),
                    message: format!(
                        "gameplay_action `{}` must target {}",
                        request.action_id, FIRST_AGENT_CLAIM_TARGET_AGENT_ID
                    ),
                    action_id: Some(request.action_id.clone()),
                    target_agent_id: Some(request.target_agent_id.clone()),
                });
            }
            if self
                .world
                .state()
                .agents
                .contains_key(request.target_agent_id.as_str())
                && self
                    .llm_sidecar
                    .agent_player_bindings
                    .contains_key(request.target_agent_id.as_str())
            {
                return Err(GameplayActionError {
                    code: "first_agent_already_bound".to_string(),
                    message: format!(
                        "gameplay_action `{}` can only run before {} is bound to a player",
                        request.action_id, request.target_agent_id
                    ),
                    action_id: Some(request.action_id.clone()),
                    target_agent_id: Some(request.target_agent_id.clone()),
                });
            }
        } else if gameplay_action_requires_actor_agent(request.action_id.as_str()) {
            let bound_agent_id = self
                .llm_sidecar
                .bound_agent_for_player(verified.player_id.as_str())
                .ok_or_else(|| GameplayActionError {
                    code: "player_agent_binding_required".to_string(),
                    message: format!(
                        "gameplay_action `{}` requires a bound player agent session",
                        request.action_id
                    ),
                    action_id: Some(request.action_id.clone()),
                    target_agent_id: Some(request.target_agent_id.clone()),
                })?;
            ensure_agent_player_access_runtime(
                &self.world,
                &self.llm_sidecar,
                bound_agent_id,
                verified.player_id.as_str(),
                public_key.as_deref(),
            )
            .map_err(|err| GameplayActionError {
                code: err.code,
                message: err.message,
                action_id: Some(request.action_id.clone()),
                target_agent_id: err.agent_id,
            })?;
            let actor_agent_id = request
                .actor_agent_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| GameplayActionError {
                    code: "actor_agent_required".to_string(),
                    message: format!(
                        "gameplay_action `{}` requires non-empty actor_agent_id",
                        request.action_id
                    ),
                    action_id: Some(request.action_id.clone()),
                    target_agent_id: Some(request.target_agent_id.clone()),
                })?;
            if actor_agent_id != bound_agent_id {
                return Err(GameplayActionError {
                    code: "actor_agent_mismatch".to_string(),
                    message: format!(
                        "gameplay_action `{}` actor_agent_id {} does not match bound player agent {}",
                        request.action_id, actor_agent_id, bound_agent_id
                    ),
                    action_id: Some(request.action_id.clone()),
                    target_agent_id: Some(request.target_agent_id.clone()),
                });
            }
            ensure_agent_player_binding_target_runtime(
                &self.world,
                &self.llm_sidecar,
                request.target_agent_id.as_str(),
                verified.player_id.as_str(),
                public_key.as_deref(),
            )
            .map_err(|err| GameplayActionError {
                code: err.code,
                message: err.message,
                action_id: Some(request.action_id.clone()),
                target_agent_id: err.agent_id,
            })?;
        } else if request.action_id == ACTION_CLAIM_STARTER_OC {
            ensure_agent_player_binding_target_runtime(
                &self.world,
                &self.llm_sidecar,
                request.target_agent_id.as_str(),
                verified.player_id.as_str(),
                public_key.as_deref(),
            )
            .map_err(|err| GameplayActionError {
                code: err.code,
                message: err.message,
                action_id: Some(request.action_id.clone()),
                target_agent_id: err.agent_id,
            })?;
        } else {
            ensure_agent_player_access_runtime(
                &self.world,
                &self.llm_sidecar,
                request.target_agent_id.as_str(),
                verified.player_id.as_str(),
                public_key.as_deref(),
            )
            .map_err(|err| GameplayActionError {
                code: err.code,
                message: err.message,
                action_id: Some(request.action_id.clone()),
                target_agent_id: err.agent_id,
            })?;
            let events = self
                .llm_sidecar
                .bind_agent_player(
                    request.target_agent_id.as_str(),
                    verified.player_id.as_str(),
                    public_key.as_deref(),
                    false,
                )
                .map_err(|message| GameplayActionError {
                    code: "player_bind_failed".to_string(),
                    message,
                    action_id: Some(request.action_id.clone()),
                    target_agent_id: Some(request.target_agent_id.clone()),
                })?;
            for event in events {
                self.enqueue_virtual_event(event);
            }
        }

        let accepted_at_tick = self.world.state().time;
        if is_first_agent_claim
            && self
                .world
                .state()
                .agents
                .contains_key(request.target_agent_id.as_str())
        {
            self.bind_first_agent_claim_player(
                request.target_agent_id.as_str(),
                verified.player_id.as_str(),
                public_key.as_deref(),
                &request,
            )?;
            self.set_verified_player_gameplay_feedback(
                &verified,
                PlayerGameplayRecentFeedback {
                    action: format!("gameplay_action:{}", request.action_id),
                    stage: "completed_advanced".to_string(),
                    effect: format!(
                        "bound existing unclaimed first Agent {} to player {}",
                        request.target_agent_id, verified.player_id
                    ),
                    intent_summary: Some(format!(
                        "bind existing first Agent {} to player {}",
                        request.target_agent_id, verified.player_id
                    )),
                    target_agent_id: Some(request.target_agent_id.clone()),
                    reason: None,
                    hint: Some(
                        "refresh the snapshot; the first Agent is now bound to this player"
                            .to_string(),
                    ),
                    delta_logical_time: 0,
                    delta_event_seq: 0,
                },
            );
            return Ok(GameplayActionAck {
                consensus_action_payload_hash: None,
                action_id: request.action_id,
                target_agent_id: request.target_agent_id,
                player_id: verified.player_id,
                runtime_action_id: 0,
                accepted_at_tick,
                message: Some(
                    "bound existing unclaimed first Agent; refresh the snapshot to continue"
                        .to_string(),
                ),
            });
        }

        let chain_status_bind = self
            .config
            .chain_status_bind
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        if let Some(chain_status_bind) = chain_status_bind {
            let chain_submit_bind = self
                .config
                .chain_submit_bind
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or(chain_status_bind)
                .to_string();
            let _ = build_runtime_action_from_gameplay_request(&request)?;
            let submitted = chain_link::submit_chain_linked_gameplay_action(
                chain_submit_bind.as_str(),
                &request,
            )?;
            let submitted_action_id = submitted
                .action_id
                .expect("chain gameplay submit must include action_id after ok=true validation");
            self.runtime_action_players
                .insert(submitted_action_id, verified.player_id.clone());
            if is_first_agent_claim {
                self.bind_first_agent_claim_player(
                    request.target_agent_id.as_str(),
                    verified.player_id.as_str(),
                    public_key.as_deref(),
                    &request,
                )?;
            }
            self.set_verified_player_gameplay_feedback_for_action(&verified, Some(submitted_action_id), PlayerGameplayRecentFeedback {
                action: format!("gameplay_action:{}", request.action_id),
                stage: "submitted".to_string(),
                effect: format!(
                    "submitted gameplay action {} for {} to chain submit endpoint {} as consensus action {}",
                    request.action_id, request.target_agent_id, chain_submit_bind, submitted_action_id
                ),
                intent_summary: Some(format!(
                    "submit gameplay action {} for {}",
                    request.action_id, request.target_agent_id
                )),
                target_agent_id: Some(request.target_agent_id.clone()),
                reason: None,
                hint: Some(
                    "wait for committed world sync to observe the gameplay action outcome"
                        .to_string(),
                ),
                delta_logical_time: 0,
                delta_event_seq: 0,
            });

            return Ok(GameplayActionAck {

                action_id: request.action_id,
                target_agent_id: request.target_agent_id,
                player_id: verified.player_id,
                runtime_action_id: submitted_action_id,
                consensus_action_payload_hash: submitted.consensus_action_payload_hash,
                accepted_at_tick,
                message: Some(
                    "submitted to chain runtime; wait for committed world sync to observe the gameplay action"
                        .to_string(),
                ),
            });
        }

        let runtime_action = build_runtime_action_from_gameplay_request(&request)?;
        let runtime_action_id = self.world.submit_action(runtime_action);
        self.runtime_action_players
            .insert(runtime_action_id, verified.player_id.clone());
        if is_first_agent_claim {
            self.bind_first_agent_claim_player(
                request.target_agent_id.as_str(),
                verified.player_id.as_str(),
                public_key.as_deref(),
                &request,
            )?;
        }
        self.set_verified_player_gameplay_feedback_for_action(
            &verified,
            Some(runtime_action_id),
            PlayerGameplayRecentFeedback {
                action: format!("gameplay_action:{}", request.action_id),
                stage: "queued".to_string(),
                effect: format!(
                    "queued gameplay action {} for {} as runtime action {}",
                    request.action_id, request.target_agent_id, runtime_action_id
                ),
                intent_summary: Some(format!(
                    "queue gameplay action {} for {}",
                    request.action_id, request.target_agent_id
                )),
                target_agent_id: Some(request.target_agent_id.clone()),
                reason: None,
                hint: Some(match request.action_id.as_str() {
                    ACTION_RELEASE_AGENT_CLAIM => {
                        "advance 1-2 steps to queue the release cooldown for this claim".to_string()
                    }
                    ACTION_CLAIM_STARTER_OC => {
                        "advance 1-2 steps to credit starter OC, then send your first agent chat"
                            .to_string()
                    }
                    _ => "advance 1-2 steps to apply the queued gameplay action".to_string(),
                }),
                delta_logical_time: 0,
                delta_event_seq: 0,
            },
        );

        Ok(GameplayActionAck {
            consensus_action_payload_hash: None,
            action_id: request.action_id,
            target_agent_id: request.target_agent_id,
            player_id: verified.player_id,
            runtime_action_id,
            accepted_at_tick,
            message: Some("advance 1-2 steps to apply the queued gameplay action".to_string()),
        })
    }

    fn verify_gameplay_action_auth(
        &self,
        request: &GameplayActionRequest,
    ) -> Result<VerifiedPlayerAuth, GameplayActionError> {
        let Some(auth) = request.auth.as_ref() else {
            return Err(GameplayActionError {
                code: "auth_proof_required".to_string(),
                message: "gameplay_action requires auth proof".to_string(),
                action_id: Some(request.action_id.clone()),
                target_agent_id: Some(request.target_agent_id.clone()),
            });
        };
        verify_gameplay_action_auth_proof(request, auth).map_err(|message| GameplayActionError {
            code: map_auth_verify_error_code(message.as_str()).to_string(),
            message,
            action_id: Some(request.action_id.clone()),
            target_agent_id: Some(request.target_agent_id.clone()),
        })
    }

    fn bind_first_agent_claim_player(
        &mut self,
        agent_id: &str,
        player_id: &str,
        public_key: Option<&str>,
        request: &GameplayActionRequest,
    ) -> Result<(), GameplayActionError> {
        let events = self
            .llm_sidecar
            .bind_agent_player(agent_id, player_id, public_key, false)
            .map_err(|message| GameplayActionError {
                code: "player_bind_failed".to_string(),
                message,
                action_id: Some(request.action_id.clone()),
                target_agent_id: Some(request.target_agent_id.clone()),
            })?;
        for event in events {
            self.enqueue_virtual_event(event);
        }
        Ok(())
    }
}

fn stage_gate_disabled_reason(
    current_stage: IndustryStage,
    required_stage: IndustryStage,
) -> Option<String> {
    if industry_stage_rank(current_stage) >= industry_stage_rank(required_stage) {
        return None;
    }
    let unlock_path = match required_stage {
        IndustryStage::ScaleOut => "complete additional smelter runs to unlock scale_out",
        IndustryStage::Governance => {
            "complete scale_out progress and the governance objective to unlock governance"
        }
        IndustryStage::Bootstrap => unreachable!("bootstrap cannot be a gated stage"),
    };
    Some(format!(
        "requires industry stage {} (current: {}); {}",
        industry_stage_label(required_stage),
        industry_stage_label(current_stage),
        unlock_path
    ))
}

fn industry_stage_rank(stage: IndustryStage) -> u8 {
    match stage {
        IndustryStage::Bootstrap => 0,
        IndustryStage::ScaleOut => 1,
        IndustryStage::Governance => 2,
    }
}

fn industry_stage_label(stage: IndustryStage) -> &'static str {
    match stage {
        IndustryStage::Bootstrap => "bootstrap",
        IndustryStage::ScaleOut => "scale_out",
        IndustryStage::Governance => "governance",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_collection_preflight_reports_the_exact_executable_collection() {
        let quote = data_collection_preflight("collector".to_string(), 20, 7, 11);

        assert_eq!(quote.electricity_cost, 7);
        assert_eq!(quote.data_amount, 11);
        assert_eq!(quote.available_electricity, 20);
        assert_eq!(quote.electricity_after, 13);
        assert!(quote.can_execute);
        assert_eq!(quote.blocked_reason, None);
        assert_eq!(quote.recovery_guidance, None);
        assert_eq!(quote.collector_agent_id, "collector");
        assert_eq!(quote.data_owner_agent_id, "collector");
        assert_eq!(quote.data_recipient_agent_id, "collector");
        assert_eq!(quote.data_use, "self_collection");
        assert_eq!(quote.permission_status, "self_owned_no_grant_required");
    }

    #[test]
    fn data_collection_preflight_preserves_balance_and_guides_recovery_when_power_is_insufficient()
    {
        let quote = data_collection_preflight("collector".to_string(), 3, 5, 8);

        assert_eq!(quote.electricity_cost, 5);
        assert_eq!(quote.data_amount, 8);
        assert_eq!(quote.available_electricity, 3);
        assert_eq!(quote.electricity_after, 3);
        assert!(!quote.can_execute);
        assert_eq!(
            quote.blocked_reason.as_deref(),
            Some("insufficient electricity: need 5, have 3")
        );
        assert_eq!(
            quote.alternative_action.as_deref(),
            Some("replenish_electricity_or_defer_collection")
        );
        assert_eq!(
            quote.recovery_guidance.as_deref(),
            Some(
                "replenish electricity before collecting data, or defer collection until electricity is available"
            )
        );
    }

    #[test]
    fn data_collection_preflight_matches_runtime_rejection_for_nonpositive_parameters() {
        let quote = data_collection_preflight("collector".to_string(), 20, 0, 8);

        assert!(!quote.can_execute);
        assert_eq!(quote.electricity_after, 20);
        assert_eq!(
            quote.blocked_reason.as_deref(),
            Some("collection electricity cost must be positive")
        );
        assert_eq!(quote.recovery_guidance, None);
    }
}
