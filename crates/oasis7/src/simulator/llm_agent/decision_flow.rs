use super::super::agent::{AgentDecision, AgentQuery, MicroDepotQuoteRequest};
use super::LlmCompletionTurn;
use super::prompt_assembly::{PromptSectionKind, PromptSectionPriority};
use oasis7_simulator::{Action, ResourceKind, ResourceOwner};
use oasis7_simulator::{MicroDepotActionKind, MicroDepotPressureClass};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

mod action_parsers;

use action_parsers::parse_market_or_social_action;

pub(super) fn parse_limit_arg(
    value: Option<&serde_json::Value>,
    default: usize,
    max: usize,
) -> usize {
    value
        .and_then(|value| value.as_u64())
        .map(|value| value.clamp(1, max as u64) as usize)
        .unwrap_or(default)
}

#[derive(Debug, Clone, Serialize)]
pub(super) struct ModuleCallExchange {
    pub module: String,
    pub args: serde_json::Value,
    pub result: serde_json::Value,
}

pub(super) fn prompt_section_kind_name(kind: PromptSectionKind) -> &'static str {
    match kind {
        PromptSectionKind::Policy => "policy",
        PromptSectionKind::Goals => "goals",
        PromptSectionKind::Context => "context",
        PromptSectionKind::Tools => "tools",
        PromptSectionKind::Conversation => "conversation",
        PromptSectionKind::History => "history",
        PromptSectionKind::Memory => "memory",
        PromptSectionKind::OutputSchema => "output_schema",
    }
}

pub(super) fn prompt_section_priority_name(priority: PromptSectionPriority) -> &'static str {
    match priority {
        PromptSectionPriority::High => "high",
        PromptSectionPriority::Medium => "medium",
        PromptSectionPriority::Low => "low",
    }
}

pub(super) fn summarize_trace_text(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }

    let mut truncated = String::new();
    for (index, ch) in text.chars().enumerate() {
        if index >= max_chars {
            break;
        }
        truncated.push(ch);
    }
    truncated.push_str("...(truncated)");
    truncated
}

#[derive(Debug, Deserialize)]
struct LlmDecisionPayload {
    decision: String,
    action: Option<serde_json::Value>,
    ticks: Option<u64>,
    to: Option<String>,
    from: Option<String>,
    max_amount: Option<i64>,
    from_owner: Option<String>,
    to_owner: Option<String>,
    kind: Option<String>,
    amount: Option<i64>,
    buyer: Option<String>,
    seller: Option<String>,
    price_per_pu: Option<i64>,
    side: Option<String>,
    limit_price_per_pu: Option<i64>,
    order_id: Option<u64>,
    owner: Option<String>,
    compound_mass_g: Option<i64>,
    location_id: Option<String>,
    factory_id: Option<String>,
    factory_kind: Option<String>,
    facility_id: Option<String>,
    target_id: Option<String>,
    action_kind: Option<MicroDepotActionKind>,
    base_cost_class: Option<MicroDepotPressureClass>,
    base_risk_class: Option<MicroDepotPressureClass>,
    blocker_type: Option<String>,
    recipe_id: Option<String>,
    batches: Option<i64>,
    publisher: Option<String>,
    installer: Option<String>,
    bidder: Option<String>,
    module_id: Option<String>,
    module_version: Option<String>,
    install_target_type: Option<String>,
    install_target_location_id: Option<String>,
    manifest_path: Option<String>,
    source_files: Option<BTreeMap<String, String>>,
    wasm_hash: Option<String>,
    wasm_bytes_hex: Option<String>,
    price_kind: Option<String>,
    price_amount: Option<i64>,
    bid_order_id: Option<u64>,
    activate: Option<bool>,
    actor: Option<String>,
    declarer: Option<String>,
    subject: Option<String>,
    object: Option<String>,
    schema_id: Option<String>,
    claim: Option<String>,
    confidence_ppm: Option<i64>,
    evidence_event_ids: Option<Vec<u64>>,
    ttl_ticks: Option<u64>,
    stake: Option<LlmSocialStakePayload>,
    challenger: Option<String>,
    fact_id: Option<u64>,
    reason: Option<String>,
    adjudicator: Option<String>,
    #[serde(alias = "adjudication_decision", alias = "verdict")]
    adjudication: Option<String>,
    notes: Option<String>,
    relation_kind: Option<String>,
    weight_bps: Option<i64>,
    backing_fact_ids: Option<Vec<u64>>,
    proposer_agent_id: Option<String>,
    alliance_id: Option<String>,
    members: Option<Vec<String>>,
    member_agent_id: Option<String>,
    charter: Option<String>,
    initiator_agent_id: Option<String>,
    war_id: Option<String>,
    aggressor_alliance_id: Option<String>,
    defender_alliance_id: Option<String>,
    objective: Option<String>,
    intensity: Option<u32>,
    proposal_key: Option<String>,
    title: Option<String>,
    description: Option<String>,
    options: Option<Vec<String>>,
    voting_window_ticks: Option<u64>,
    quorum_weight: Option<u64>,
    pass_threshold_bps: Option<u16>,
    voter_agent_id: Option<String>,
    option: Option<String>,
    weight: Option<u32>,
    resolver_agent_id: Option<String>,
    crisis_id: Option<String>,
    strategy: Option<String>,
    success: Option<bool>,
    operator_agent_id: Option<String>,
    target_agent_id: Option<String>,
    track: Option<String>,
    points: Option<i64>,
    achievement_id: Option<String>,
    creator_agent_id: Option<String>,
    contract_id: Option<String>,
    counterparty_agent_id: Option<String>,
    settlement_kind: Option<String>,
    settlement_amount: Option<i64>,
    reputation_stake: Option<i64>,
    expires_at: Option<u64>,
    accepter_agent_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct LlmSocialStakePayload {
    kind: Option<String>,
    amount: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub(super) struct LlmModuleCallRequest {
    pub module: String,
    #[serde(default)]
    pub args: serde_json::Value,
}

#[derive(Debug, Deserialize)]
pub(super) struct LlmPlanPayload {
    #[serde(default)]
    pub missing: Vec<String>,
    #[serde(default)]
    pub next: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawLlmDecisionDraftPayload {
    decision: serde_json::Value,
    #[serde(default)]
    confidence: Option<f64>,
    #[serde(default)]
    need_verify: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct RawLlmExecuteUntilPayload {
    decision: String,
    action: serde_json::Value,
    until: RawLlmExecuteUntilUntil,
    #[serde(default)]
    max_ticks: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct RawLlmExecuteUntilUntil {
    #[serde(default)]
    event: Option<String>,
    #[serde(default)]
    event_any_of: Vec<String>,
    #[serde(default)]
    value_lte: Option<i64>,
}

#[derive(Debug, Clone)]
pub(super) struct LlmDecisionDraft {
    pub decision: AgentDecision,
    pub confidence: Option<f64>,
    pub need_verify: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ExecuteUntilEventKind {
    ActionRejected,
    NewVisibleAgent,
    NewVisibleLocation,
    ArriveTarget,
    InsufficientElectricity,
    ThermalOverload,
    HarvestYieldBelow,
    HarvestAvailableBelow,
}

impl ExecuteUntilEventKind {
    pub(super) fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "action_rejected" => Some(Self::ActionRejected),
            "new_visible_agent" => Some(Self::NewVisibleAgent),
            "new_visible_location" => Some(Self::NewVisibleLocation),
            "arrive_target" => Some(Self::ArriveTarget),
            "insufficient_electricity" => Some(Self::InsufficientElectricity),
            "thermal_overload" => Some(Self::ThermalOverload),
            "harvest_yield_below" => Some(Self::HarvestYieldBelow),
            "harvest_available_below" => Some(Self::HarvestAvailableBelow),
            _ => None,
        }
    }

    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::ActionRejected => "action_rejected",
            Self::NewVisibleAgent => "new_visible_agent",
            Self::NewVisibleLocation => "new_visible_location",
            Self::ArriveTarget => "arrive_target",
            Self::InsufficientElectricity => "insufficient_electricity",
            Self::ThermalOverload => "thermal_overload",
            Self::HarvestYieldBelow => "harvest_yield_below",
            Self::HarvestAvailableBelow => "harvest_available_below",
        }
    }

    pub(super) fn requires_value_lte(self) -> bool {
        matches!(self, Self::HarvestYieldBelow | Self::HarvestAvailableBelow)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ExecuteUntilCondition {
    pub kind: ExecuteUntilEventKind,
    pub value_lte: Option<i64>,
}

pub(super) fn summarize_execute_until_conditions(conditions: &[ExecuteUntilCondition]) -> String {
    use std::fmt::Write as _;

    let mut summary = String::new();
    for condition in conditions {
        if !summary.is_empty() {
            summary.push('|');
        }
        summary.push_str(condition.kind.as_str());
        if let Some(value_lte) = condition.value_lte {
            let _ = write!(summary, "<= {value_lte}");
        }
    }
    summary
}

#[derive(Debug, Clone)]
pub(super) struct ExecuteUntilDirective {
    pub action: Action,
    pub until_conditions: Vec<ExecuteUntilCondition>,
    pub max_ticks: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct DecisionRewriteReceipt {
    pub from: String,
    pub to: String,
    pub reason: String,
}

#[derive(Debug)]
pub(super) enum ParsedLlmTurn {
    Plan {
        payload: LlmPlanPayload,
        message_to_user: Option<String>,
    },
    DecisionDraft {
        draft: LlmDecisionDraft,
        message_to_user: Option<String>,
    },
    Decision {
        decision: AgentDecision,
        parse_error: Option<String>,
        message_to_user: Option<String>,
    },
    ExecuteUntil {
        directive: ExecuteUntilDirective,
        message_to_user: Option<String>,
        rewrite_receipt: Option<DecisionRewriteReceipt>,
    },
    ModuleCall {
        request: LlmModuleCallRequest,
        message_to_user: Option<String>,
    },
    Invalid(String),
}

#[cfg(test)]
pub(super) fn parse_llm_turn_payloads(
    turns: &[LlmCompletionTurn],
    agent_id: &str,
) -> Vec<ParsedLlmTurn> {
    parse_llm_turn_payloads_with_debug_mode(turns, agent_id, false)
}

pub(super) fn parse_llm_turn_payloads_with_debug_mode(
    turns: &[LlmCompletionTurn],
    agent_id: &str,
    llm_debug_mode: bool,
) -> Vec<ParsedLlmTurn> {
    turns
        .iter()
        .map(|turn| match turn {
            LlmCompletionTurn::Decision { payload } => {
                parse_llm_turn_value(payload.clone(), agent_id, llm_debug_mode)
            }
            LlmCompletionTurn::ModuleCall { module, args } => {
                let normalized_module = normalize_prompt_module_name(module.as_str());
                if normalized_module.trim().is_empty() {
                    ParsedLlmTurn::Invalid("module_call missing `module`".to_string())
                } else {
                    ParsedLlmTurn::ModuleCall {
                        request: LlmModuleCallRequest {
                            module: normalized_module,
                            args: args.clone(),
                        },
                        message_to_user: None,
                    }
                }
            }
        })
        .collect()
}

fn parse_llm_turn_value(
    value: serde_json::Value,
    agent_id: &str,
    llm_debug_mode: bool,
) -> ParsedLlmTurn {
    let message_to_user = parse_message_to_user(&value);

    if let Some(turn_type) = value
        .get("type")
        .and_then(|value| value.as_str())
        .map(|value| value.trim().to_ascii_lowercase())
    {
        return match turn_type.as_str() {
            "module_call" => match serde_json::from_value::<LlmModuleCallRequest>(value) {
                Ok(mut request) => {
                    request.module = normalize_prompt_module_name(request.module.as_str());
                    if request.module.trim().is_empty() {
                        ParsedLlmTurn::Invalid("module_call missing `module`".to_string())
                    } else {
                        ParsedLlmTurn::ModuleCall {
                            request,
                            message_to_user,
                        }
                    }
                }
                Err(err) => ParsedLlmTurn::Invalid(format!("module_call parse failed: {err}")),
            },
            "plan" => match serde_json::from_value::<LlmPlanPayload>(value) {
                Ok(payload) => ParsedLlmTurn::Plan {
                    payload,
                    message_to_user,
                },
                Err(err) => ParsedLlmTurn::Invalid(format!("plan parse failed: {err}")),
            },
            "decision_draft" => match parse_llm_decision_draft(value, agent_id, llm_debug_mode) {
                Ok(draft) => ParsedLlmTurn::DecisionDraft {
                    draft,
                    message_to_user,
                },
                Err(err) => ParsedLlmTurn::Invalid(err),
            },
            other => ParsedLlmTurn::Invalid(format!("unsupported turn type: {other}")),
        };
    }

    if value
        .get("decision")
        .and_then(|value| value.as_str())
        .is_some_and(|value| value.trim().eq_ignore_ascii_case("execute_until"))
    {
        return match parse_execute_until_decision(value, agent_id, llm_debug_mode) {
            Ok((directive, rewrite_receipt)) => ParsedLlmTurn::ExecuteUntil {
                directive,
                message_to_user,
                rewrite_receipt,
            },
            Err(err) => ParsedLlmTurn::Invalid(err),
        };
    }

    let (decision, parse_error) =
        parse_llm_decision_value_with_error(value, agent_id, llm_debug_mode);
    if let Some(err) = parse_error {
        ParsedLlmTurn::Invalid(err)
    } else {
        ParsedLlmTurn::Decision {
            decision,
            parse_error: None,
            message_to_user,
        }
    }
}

fn parse_message_to_user(value: &serde_json::Value) -> Option<String> {
    let message = value
        .get("message_to_user")
        .or_else(|| value.get("user_message"))
        .or_else(|| value.get("message"))
        .and_then(|value| value.as_str())?;
    let normalized = message.trim();
    if normalized.is_empty() {
        return None;
    }
    Some(normalized.to_string())
}

fn parse_llm_decision_draft(
    value: serde_json::Value,
    agent_id: &str,
    llm_debug_mode: bool,
) -> Result<LlmDecisionDraft, String> {
    let raw_value = value.clone();
    let payload = serde_json::from_value::<RawLlmDecisionDraftPayload>(value)
        .map_err(|err| format!("decision_draft parse failed: {err}"))?;

    let decision_value = if payload.decision.is_object() {
        payload.decision
    } else {
        decision_draft_shorthand_value(&raw_value).unwrap_or(payload.decision)
    };

    let (decision, parse_error) =
        parse_llm_decision_value_with_error(decision_value, agent_id, llm_debug_mode);
    if let Some(err) = parse_error {
        return Err(format!("decision_draft invalid decision: {err}"));
    }

    Ok(LlmDecisionDraft {
        decision,
        confidence: payload.confidence,
        need_verify: payload.need_verify.unwrap_or(true),
    })
}

fn decision_draft_shorthand_value(value: &serde_json::Value) -> Option<serde_json::Value> {
    let mut decision_object = value.as_object()?.clone();
    if !decision_object
        .get("decision")
        .is_some_and(|value| value.is_string())
    {
        return None;
    }

    decision_object.remove("type");
    decision_object.remove("confidence");
    decision_object.remove("need_verify");
    Some(serde_json::Value::Object(decision_object))
}

fn parse_execute_until_decision(
    value: serde_json::Value,
    agent_id: &str,
    llm_debug_mode: bool,
) -> Result<(ExecuteUntilDirective, Option<DecisionRewriteReceipt>), String> {
    const DEFAULT_MAX_TICKS: u64 = 6;
    const MAX_TICKS_CAP: u64 = 256;

    let payload = serde_json::from_value::<RawLlmExecuteUntilPayload>(value)
        .map_err(|err| format!("execute_until parse failed: {err}"))?;

    if !payload
        .decision
        .trim()
        .eq_ignore_ascii_case("execute_until")
    {
        return Err("execute_until missing decision=execute_until".to_string());
    }

    let (action_decision, action_parse_error) =
        parse_llm_decision_value_with_error(payload.action, agent_id, llm_debug_mode);
    if let Some(err) = action_parse_error {
        return Err(format!("execute_until invalid action: {err}"));
    }
    let (action, rewrite_receipt) = match action_decision {
        AgentDecision::Act(action) => (action, None),
        AgentDecision::Query(_) => {
            return Err("execute_until action must be durable; queries are read-only".to_string());
        }
        AgentDecision::Wait => (
            Action::HarvestRadiation {
                agent_id: agent_id.to_string(),
                max_amount: 1,
            },
            Some(DecisionRewriteReceipt {
                from: "wait".to_string(),
                to: "harvest_radiation".to_string(),
                reason: "execute_until.action=wait is non-actionable; rewritten to minimal harvest_radiation(max_amount=1)"
                    .to_string(),
            }),
        ),
        AgentDecision::WaitTicks(ticks) => (
            Action::HarvestRadiation {
                agent_id: agent_id.to_string(),
                max_amount: 1,
            },
            Some(DecisionRewriteReceipt {
                from: "wait_ticks".to_string(),
                to: "harvest_radiation".to_string(),
                reason: format!(
                    "execute_until.action=wait_ticks({ticks}) is non-actionable; rewritten to minimal harvest_radiation(max_amount=1)"
                ),
            }),
        ),
        AgentDecision::ModuleCommand { .. } => {
            return Err("execute_until cannot contain a trusted module command".to_string());
        }
    };

    let until_conditions = parse_execute_until_conditions(&payload.until)?;

    let max_ticks = payload
        .max_ticks
        .unwrap_or(DEFAULT_MAX_TICKS)
        .clamp(1, MAX_TICKS_CAP);

    Ok((
        ExecuteUntilDirective {
            action,
            until_conditions,
            max_ticks,
        },
        rewrite_receipt,
    ))
}

fn parse_execute_until_conditions(
    until: &RawLlmExecuteUntilUntil,
) -> Result<Vec<ExecuteUntilCondition>, String> {
    let mut values = Vec::new();
    if let Some(event) = until.event.as_ref() {
        values.push(event.as_str());
    }
    for event in until.event_any_of.iter() {
        values.push(event.as_str());
    }

    let mut conditions = Vec::new();
    for value in values {
        for token in value.split(['|', ',']) {
            let trimmed = token.trim();
            if trimmed.is_empty() {
                continue;
            }
            let kind = ExecuteUntilEventKind::parse(trimmed)
                .ok_or_else(|| format!("execute_until unsupported until.event: {trimmed}"))?;
            let value_lte = if kind.requires_value_lte() {
                let Some(value_lte) = until.value_lte else {
                    return Err(format!(
                        "execute_until event {} requires until.value_lte",
                        kind.as_str()
                    ));
                };
                if value_lte < 0 {
                    return Err(format!(
                        "execute_until until.value_lte must be non-negative for {}",
                        kind.as_str()
                    ));
                }
                Some(value_lte)
            } else {
                None
            };
            let condition = ExecuteUntilCondition { kind, value_lte };
            if !conditions.contains(&condition) {
                conditions.push(condition);
            }
        }
    }

    if conditions.is_empty() {
        return Err("execute_until missing until.event/event_any_of".to_string());
    }

    Ok(conditions)
}

fn parse_llm_decision_value_with_error(
    value: serde_json::Value,
    agent_id: &str,
    llm_debug_mode: bool,
) -> (AgentDecision, Option<String>) {
    let parsed = match serde_json::from_value::<LlmDecisionPayload>(value) {
        Ok(value) => value,
        Err(err) => {
            return (
                AgentDecision::Wait,
                Some(format!("json parse failed: {err}")),
            );
        }
    };

    let decision = match parsed.decision.trim().to_ascii_lowercase().as_str() {
        "wait" => AgentDecision::Wait,
        "wait_ticks" => AgentDecision::WaitTicks(parsed.ticks.unwrap_or(1).max(1)),
        "quote_micro_depot_install" => {
            let Some(action) = parsed.action else {
                return (
                    AgentDecision::Wait,
                    Some(
                        "quote_micro_depot_install requires an InstallMicroDepot action"
                            .to_string(),
                    ),
                );
            };
            let action = match serde_json::from_value::<Action>(action) {
                Ok(action @ Action::InstallMicroDepot { .. }) => action,
                Ok(_) | Err(_) => {
                    return (
                        AgentDecision::Wait,
                        Some(
                            "quote_micro_depot_install requires an InstallMicroDepot action"
                                .to_string(),
                        ),
                    );
                }
            };
            AgentDecision::Query(AgentQuery::QuoteMicroDepotInstall(action))
        }
        "evaluate_micro_depot_quote" => {
            let facility_id = parsed.facility_id.filter(|value| !value.trim().is_empty());
            let target_id = parsed.target_id.filter(|value| !value.trim().is_empty());
            let (
                Some(facility_id),
                Some(target_id),
                Some(action_kind),
                Some(base_cost_class),
                Some(base_risk_class),
            ) = (
                facility_id,
                target_id,
                parsed.action_kind,
                parsed.base_cost_class,
                parsed.base_risk_class,
            )
            else {
                return (AgentDecision::Wait, Some("evaluate_micro_depot_quote requires facility_id, target_id, action_kind, base_cost_class, and base_risk_class".to_string()));
            };
            AgentDecision::Query(AgentQuery::EvaluateMicroDepotQuote(
                MicroDepotQuoteRequest {
                    agent_id: agent_id.to_string(),
                    facility_id,
                    target_id,
                    action_kind,
                    base_cost_class,
                    base_risk_class,
                    blocker_type: parsed.blocker_type,
                },
            ))
        }
        "move_agent" => {
            let to = parsed.to.unwrap_or_default();
            if to.trim().is_empty() {
                return (
                    AgentDecision::Wait,
                    Some("move_agent missing `to`".to_string()),
                );
            }
            AgentDecision::Act(Action::MoveAgent {
                agent_id: agent_id.to_string(),
                to,
            })
        }
        "harvest_radiation" => {
            let max_amount = parsed.max_amount.unwrap_or(1);
            if max_amount <= 0 {
                return (
                    AgentDecision::Wait,
                    Some("harvest_radiation requires positive max_amount".to_string()),
                );
            }
            AgentDecision::Act(Action::HarvestRadiation {
                agent_id: agent_id.to_string(),
                max_amount,
            })
        }
        "transfer_resource" => {
            let from_owner = match parsed.from_owner.as_deref() {
                Some(owner) => match parse_owner_spec(owner, agent_id) {
                    Ok(owner) => owner,
                    Err(err) => return (AgentDecision::Wait, Some(err)),
                },
                None => {
                    return (
                        AgentDecision::Wait,
                        Some("transfer_resource missing `from_owner`".to_string()),
                    );
                }
            };
            let to_owner = match parsed.to_owner.as_deref() {
                Some(owner) => match parse_owner_spec(owner, agent_id) {
                    Ok(owner) => owner,
                    Err(err) => return (AgentDecision::Wait, Some(err)),
                },
                None => {
                    return (
                        AgentDecision::Wait,
                        Some("transfer_resource missing `to_owner`".to_string()),
                    );
                }
            };
            let kind = match parsed.kind.as_deref() {
                Some(raw_kind) => match parse_resource_kind(raw_kind) {
                    Some(kind) => kind,
                    None => {
                        return (
                            AgentDecision::Wait,
                            Some(format!("transfer_resource invalid kind: {raw_kind}")),
                        );
                    }
                },
                None => {
                    return (
                        AgentDecision::Wait,
                        Some("transfer_resource missing `kind`".to_string()),
                    );
                }
            };
            let amount = parsed.amount.unwrap_or(1);
            if amount <= 0 {
                return (
                    AgentDecision::Wait,
                    Some("transfer_resource requires positive amount".to_string()),
                );
            }
            AgentDecision::Act(Action::TransferResource {
                from: from_owner,
                to: to_owner,
                kind,
                amount,
            })
        }
        "debug_grant_resource" => {
            if !llm_debug_mode {
                return (
                    AgentDecision::Wait,
                    Some(
                        "debug_grant_resource is disabled; enable OASIS7_LLM_DEBUG_MODE=true"
                            .to_string(),
                    ),
                );
            }
            let owner = match parsed.owner.as_deref() {
                Some(owner) => match parse_owner_spec(owner, agent_id) {
                    Ok(owner) => owner,
                    Err(err) => return (AgentDecision::Wait, Some(err)),
                },
                None => ResourceOwner::Agent {
                    agent_id: agent_id.to_string(),
                },
            };
            let kind = match parsed.kind.as_deref() {
                Some(raw_kind) => match parse_resource_kind(raw_kind) {
                    Some(kind) => kind,
                    None => {
                        return (
                            AgentDecision::Wait,
                            Some(format!("debug_grant_resource invalid kind: {raw_kind}")),
                        );
                    }
                },
                None => {
                    return (
                        AgentDecision::Wait,
                        Some("debug_grant_resource missing `kind`".to_string()),
                    );
                }
            };
            let amount = parsed.amount.unwrap_or(1);
            if amount <= 0 {
                return (
                    AgentDecision::Wait,
                    Some("debug_grant_resource requires positive amount".to_string()),
                );
            }
            AgentDecision::Act(Action::DebugGrantResource {
                owner,
                kind,
                amount,
            })
        }
        "mine_compound" => {
            let owner = match parsed.owner.as_deref() {
                Some(owner) => match parse_owner_spec(owner, agent_id) {
                    Ok(owner) => owner,
                    Err(err) => return (AgentDecision::Wait, Some(err)),
                },
                None => ResourceOwner::Agent {
                    agent_id: agent_id.to_string(),
                },
            };
            let location_id = parsed.location_id.unwrap_or_default();
            if location_id.trim().is_empty() {
                return (
                    AgentDecision::Wait,
                    Some("mine_compound missing `location_id`".to_string()),
                );
            }
            let compound_mass_g = parsed.compound_mass_g.unwrap_or(1);
            if compound_mass_g <= 0 {
                return (
                    AgentDecision::Wait,
                    Some("mine_compound requires positive compound_mass_g".to_string()),
                );
            }
            AgentDecision::Act(Action::MineCompound {
                owner,
                location_id,
                compound_mass_g,
            })
        }
        "refine_compound" => {
            let owner = match parsed.owner.as_deref() {
                Some(owner) => match parse_owner_spec(owner, agent_id) {
                    Ok(owner) => owner,
                    Err(err) => return (AgentDecision::Wait, Some(err)),
                },
                None => ResourceOwner::Agent {
                    agent_id: agent_id.to_string(),
                },
            };
            let compound_mass_g = parsed.compound_mass_g.unwrap_or(1);
            if compound_mass_g <= 0 {
                return (
                    AgentDecision::Wait,
                    Some("refine_compound requires positive compound_mass_g".to_string()),
                );
            }
            AgentDecision::Act(Action::RefineCompound {
                owner,
                compound_mass_g,
            })
        }
        "build_factory" => {
            let owner = match parsed.owner.as_deref() {
                Some(owner) => match parse_owner_spec(owner, agent_id) {
                    Ok(owner) => owner,
                    Err(err) => return (AgentDecision::Wait, Some(err)),
                },
                None => ResourceOwner::Agent {
                    agent_id: agent_id.to_string(),
                },
            };
            let location_id = parsed.location_id.unwrap_or_default();
            if location_id.trim().is_empty() {
                return (
                    AgentDecision::Wait,
                    Some("build_factory missing `location_id`".to_string()),
                );
            }
            let factory_id = parsed.factory_id.unwrap_or_default();
            if factory_id.trim().is_empty() {
                return (
                    AgentDecision::Wait,
                    Some("build_factory missing `factory_id`".to_string()),
                );
            }
            let factory_kind = parsed
                .factory_kind
                .unwrap_or_else(|| match factory_id.trim() {
                    "factory.smelter.mk1"
                    | "factory.assembler.mk1"
                    | "factory.power.radiation.mk1" => factory_id.clone(),
                    _ => String::new(),
                });
            if factory_kind.trim().is_empty() {
                return (
                    AgentDecision::Wait,
                    Some("build_factory missing `factory_kind`".to_string()),
                );
            }
            AgentDecision::Act(Action::BuildFactory {
                owner,
                location_id,
                factory_id,
                factory_kind,
            })
        }
        "schedule_recipe" => {
            let owner = match parsed.owner.as_deref() {
                Some(owner) => match parse_owner_spec(owner, agent_id) {
                    Ok(owner) => owner,
                    Err(err) => return (AgentDecision::Wait, Some(err)),
                },
                None => ResourceOwner::Agent {
                    agent_id: agent_id.to_string(),
                },
            };
            let factory_id = parsed.factory_id.unwrap_or_default();
            if factory_id.trim().is_empty() {
                return (
                    AgentDecision::Wait,
                    Some("schedule_recipe missing `factory_id`".to_string()),
                );
            }
            let recipe_id = parsed.recipe_id.unwrap_or_default();
            if recipe_id.trim().is_empty() {
                return (
                    AgentDecision::Wait,
                    Some("schedule_recipe missing `recipe_id`".to_string()),
                );
            }
            let batches = parsed.batches.unwrap_or(1).max(1);
            AgentDecision::Act(Action::ScheduleRecipe {
                owner,
                factory_id,
                recipe_id,
                batches,
            })
        }
        other => {
            if let Some(parsed_action) = parse_market_or_social_action(other, &parsed, agent_id) {
                return match parsed_action {
                    Ok(action) => (AgentDecision::Act(action), None),
                    Err(err) => (AgentDecision::Wait, Some(err)),
                };
            }
            return (
                AgentDecision::Wait,
                Some(format!("unsupported decision: {other}")),
            );
        }
    };

    (decision, None)
}

fn parse_owner_spec(value: &str, default_agent_id: &str) -> Result<ResourceOwner, String> {
    let normalized = value.trim();
    if normalized.is_empty() {
        return Err("owner spec cannot be empty".to_string());
    }
    if normalized.eq_ignore_ascii_case("self") {
        return Ok(ResourceOwner::Agent {
            agent_id: default_agent_id.to_string(),
        });
    }
    if let Some(agent_id) = normalized.strip_prefix("agent:") {
        if agent_id.trim().is_empty() {
            return Err("owner spec agent id cannot be empty".to_string());
        }
        return Ok(ResourceOwner::Agent {
            agent_id: agent_id.trim().to_string(),
        });
    }
    if let Some(location_id) = normalized.strip_prefix("location:") {
        if location_id.trim().is_empty() {
            return Err("owner spec location id cannot be empty".to_string());
        }
        return Ok(ResourceOwner::Location {
            location_id: location_id.trim().to_string(),
        });
    }
    Err(format!(
        "invalid owner spec `{normalized}`; use self/agent:<id>/location:<id>"
    ))
}

fn parse_resource_kind(value: &str) -> Option<ResourceKind> {
    match value.trim().to_ascii_lowercase().as_str() {
        "electricity" | "power" => Some(ResourceKind::Electricity),
        "data" => Some(ResourceKind::Data),
        _ => None,
    }
}

pub(super) fn normalize_prompt_module_name(value: &str) -> String {
    let normalized = value.trim();
    if normalized.is_empty() {
        return String::new();
    }

    let canonical = match normalized.to_ascii_lowercase().as_str() {
        "agent.modules.list" | "agent_modules_list" | "agent-modules-list" => {
            Some("agent.modules.list")
        }
        "environment.current_observation"
        | "environment_current_observation"
        | "environment-current-observation" => Some("environment.current_observation"),
        "memory.short_term.recent"
        | "memory_short_term_recent"
        | "memory-short-term-recent"
        | "memory.short.term.recent" => Some("memory.short_term.recent"),
        "memory.long_term.search"
        | "memory_long_term_search"
        | "memory-long-term-search"
        | "memory.long.term.search" => Some("memory.long_term.search"),
        "world.rules.guide" | "world_rules_guide" | "world-rules-guide" | "world.rules_guide" => {
            Some("world.rules.guide")
        }
        "module.lifecycle.status"
        | "module_lifecycle_status"
        | "module-lifecycle-status"
        | "module.lifecycle_status" => Some("module.lifecycle.status"),
        "power.order_book.status"
        | "power_order_book_status"
        | "power-order-book-status"
        | "power.order_book_status"
        | "power.orderbook.status"
        | "power_orderbook_status" => Some("power.order_book.status"),
        "module.market.status"
        | "module_market_status"
        | "module-market-status"
        | "module.market_status" => Some("module.market.status"),
        "social.state.status"
        | "social_state_status"
        | "social-state-status"
        | "social.state_status" => Some("social.state.status"),
        _ => None,
    };

    canonical
        .map(str::to_string)
        .unwrap_or_else(|| normalized.to_string())
}
