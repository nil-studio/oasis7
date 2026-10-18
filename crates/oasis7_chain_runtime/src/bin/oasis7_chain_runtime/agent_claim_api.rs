use std::net::TcpStream;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use oasis7::consensus_action_payload::{
    ConsensusActionPayloadEnvelope, encode_consensus_action_payload,
};
use oasis7_node::NodeRuntime;
use oasis7_runtime::{Action, DomainEvent, RejectReason, World, WorldEventBody};
use serde::{Deserialize, Serialize};

const AGENT_CLAIM_SUBMIT_PATH: &str = "/v1/chain/agent-claim/submit";
const AGENT_CLAIM_ERROR_INVALID_REQUEST: &str = "invalid_request";
const AGENT_CLAIM_ERROR_ACTION_REJECTED: &str = "action_rejected";
const AGENT_CLAIM_ERROR_INTERNAL: &str = "internal_error";
const AGENT_CLAIM_ERROR_SUBMIT_FAILED: &str = "submit_failed";

static NEXT_AGENT_CLAIM_ACTION_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ChainAgentClaimSubmitRequest {
    pub(super) claimer_agent_id: String,
    pub(super) target_agent_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub(super) struct ChainAgentClaimActionPreview {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) claimer_agent_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) target_agent_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) slot_index: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) reputation_tier: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) requested_total_upfront_amount: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) auto_issued_restricted_amount: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct ChainAgentClaimActionResponse {
    pub(super) ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) action_id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) submitted_at_unix_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) preview: Option<ChainAgentClaimActionPreview>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) error_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) error: Option<String>,
}

impl ChainAgentClaimActionResponse {
    fn success(
        action_id: u64,
        submitted_at_unix_ms: i64,
        preview: ChainAgentClaimActionPreview,
    ) -> Self {
        Self {
            ok: true,
            action_id: Some(action_id),
            submitted_at_unix_ms: Some(submitted_at_unix_ms),
            preview: Some(preview),
            error_code: None,
            error: None,
        }
    }

    fn error(error_code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            ok: false,
            action_id: None,
            submitted_at_unix_ms: None,
            preview: None,
            error_code: Some(error_code.into()),
            error: Some(message.into()),
        }
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "Stable protocol and runtime seam keeps independently validated inputs explicit."
)]
pub(super) fn maybe_handle_agent_claim_request(
    stream: &mut TcpStream,
    request_bytes: &[u8],
    runtime: &Arc<Mutex<NodeRuntime>>,
    method: &str,
    _target: &str,
    path: &str,
    _node_id: &str,
    _world_id: &str,
    execution_world_dir: &Path,
) -> Result<bool, String> {
    match path {
        AGENT_CLAIM_SUBMIT_PATH => {
            if !method.eq_ignore_ascii_case("POST") {
                write_agent_claim_error(
                    stream,
                    405,
                    AGENT_CLAIM_ERROR_INVALID_REQUEST,
                    format!("method {method} is not allowed for {AGENT_CLAIM_SUBMIT_PATH}")
                        .as_str(),
                )?;
                return Ok(true);
            }
            handle_agent_claim_submit(stream, request_bytes, runtime, execution_world_dir)?;
            Ok(true)
        }
        _ => Ok(false),
    }
}

fn parse_json_request<T: for<'de> Deserialize<'de>>(request_bytes: &[u8]) -> Result<T, String> {
    let body = super::feedback_submit_api::extract_http_json_body(request_bytes)
        .map_err(|err| format!("invalid agent claim request body: {err}"))?;
    serde_json::from_slice(body).map_err(|err| format!("invalid agent claim request: {err}"))
}

fn handle_agent_claim_submit(
    stream: &mut TcpStream,
    request_bytes: &[u8],
    runtime: &Arc<Mutex<NodeRuntime>>,
    execution_world_dir: &Path,
) -> Result<(), String> {
    let request = parse_json_request::<ChainAgentClaimSubmitRequest>(request_bytes)?;
    let action = Action::ClaimAgent {
        claimer_agent_id: request.claimer_agent_id,
        target_agent_id: request.target_agent_id,
    };
    handle_agent_claim_action(stream, runtime, execution_world_dir, action)
}

fn handle_agent_claim_action(
    stream: &mut TcpStream,
    runtime: &Arc<Mutex<NodeRuntime>>,
    execution_world_dir: &Path,
    action: Action,
) -> Result<(), String> {
    let preview = match preflight_agent_claim_action(execution_world_dir, action.clone()) {
        Ok(preview) => preview,
        Err((error_code, error)) => {
            write_agent_claim_error(stream, 409, error_code.as_str(), error.as_str())?;
            return Ok(());
        }
    };
    let action_id = match submit_runtime_action(runtime, action) {
        Ok(action_id) => action_id,
        Err(err) => {
            write_agent_claim_error(stream, 502, AGENT_CLAIM_ERROR_SUBMIT_FAILED, err.as_str())?;
            return Ok(());
        }
    };
    let response = ChainAgentClaimActionResponse::success(action_id, super::now_unix_ms(), preview);
    write_agent_claim_json_response(stream, 200, &response)
}

fn preflight_agent_claim_action(
    execution_world_dir: &Path,
    action: Action,
) -> Result<ChainAgentClaimActionPreview, (String, String)> {
    let mut world =
        super::execution_bridge::load_execution_world(execution_world_dir).map_err(|err| {
            (
                AGENT_CLAIM_ERROR_INTERNAL.to_string(),
                format!("load execution world failed: {err}"),
            )
        })?;
    let journal_len_before = world.journal().events.len();
    world.submit_action(action);
    world.step().map_err(|err| {
        (
            AGENT_CLAIM_ERROR_INTERNAL.to_string(),
            format!("agent claim preflight step failed: {err:?}"),
        )
    })?;
    extract_agent_claim_preview(&world, journal_len_before)
}

fn extract_agent_claim_preview(
    world: &World,
    journal_len_before: usize,
) -> Result<ChainAgentClaimActionPreview, (String, String)> {
    for event in &world.journal().events[journal_len_before..] {
        let WorldEventBody::Domain(domain_event) = &event.body else {
            continue;
        };
        match domain_event {
            DomainEvent::ActionRejected { reason, .. } => {
                return Err(reject_reason_to_api_error(reason));
            }
            DomainEvent::AgentClaimed {
                claimer_agent_id,
                target_agent_id,
                slot_index,
                reputation_tier,
                activation_fee_amount,
                claim_bond_amount,
                upkeep_per_epoch,
                auto_issued_restricted_amount,
                ..
            } => {
                return Ok(ChainAgentClaimActionPreview {
                    claimer_agent_id: Some(claimer_agent_id.clone()),
                    target_agent_id: Some(target_agent_id.clone()),
                    slot_index: Some(*slot_index),
                    reputation_tier: Some(*reputation_tier),
                    requested_total_upfront_amount: Some(
                        activation_fee_amount
                            .saturating_add(*claim_bond_amount)
                            .saturating_add(*upkeep_per_epoch),
                    ),
                    auto_issued_restricted_amount: (*auto_issued_restricted_amount > 0)
                        .then_some(*auto_issued_restricted_amount),
                });
            }
            _ => {}
        }
    }
    Err((
        AGENT_CLAIM_ERROR_INTERNAL.to_string(),
        "agent claim preflight produced no terminal event".to_string(),
    ))
}

fn reject_reason_to_api_error(reason: &RejectReason) -> (String, String) {
    match reason {
        RejectReason::AgentAlreadyExists { agent_id } => (
            AGENT_CLAIM_ERROR_ACTION_REJECTED.to_string(),
            format!("agent already exists: {agent_id}"),
        ),
        RejectReason::AgentNotFound { agent_id } => (
            AGENT_CLAIM_ERROR_ACTION_REJECTED.to_string(),
            format!("agent not found: {agent_id}"),
        ),
        RejectReason::AgentsNotCoLocated {
            agent_id,
            other_agent_id,
        } => (
            AGENT_CLAIM_ERROR_ACTION_REJECTED.to_string(),
            format!("agents not co-located: agent_id={agent_id} other_agent_id={other_agent_id}"),
        ),
        RejectReason::InvalidAmount { amount } => (
            AGENT_CLAIM_ERROR_ACTION_REJECTED.to_string(),
            format!("invalid amount: {amount}"),
        ),
        RejectReason::InsufficientResource {
            agent_id,
            kind,
            requested,
            available,
        } => (
            AGENT_CLAIM_ERROR_ACTION_REJECTED.to_string(),
            format!(
                "insufficient resource: agent_id={agent_id} kind={kind:?} requested={requested} available={available}"
            ),
        ),
        RejectReason::InsufficientResources { deficits } => (
            AGENT_CLAIM_ERROR_ACTION_REJECTED.to_string(),
            format!("insufficient resources: {deficits:?}"),
        ),
        RejectReason::InsufficientMaterial {
            material_kind,
            requested,
            available,
        } => (
            AGENT_CLAIM_ERROR_ACTION_REJECTED.to_string(),
            format!(
                "insufficient material: material_kind={material_kind} requested={requested} available={available}"
            ),
        ),
        RejectReason::MaterialTransferDistanceExceeded {
            distance_km,
            max_distance_km,
        } => (
            AGENT_CLAIM_ERROR_ACTION_REJECTED.to_string(),
            format!(
                "material transfer distance exceeded: distance_km={distance_km} max_distance_km={max_distance_km}"
            ),
        ),
        RejectReason::MaterialTransitCapacityExceeded {
            in_flight,
            max_in_flight,
        } => (
            AGENT_CLAIM_ERROR_ACTION_REJECTED.to_string(),
            format!(
                "material transit capacity exceeded: in_flight={in_flight} max_in_flight={max_in_flight}"
            ),
        ),
        RejectReason::FactoryNotFound { factory_id } => (
            AGENT_CLAIM_ERROR_ACTION_REJECTED.to_string(),
            format!("factory not found: {factory_id}"),
        ),
        RejectReason::FactoryBusy {
            factory_id,
            active_jobs,
            recipe_slots,
        } => (
            AGENT_CLAIM_ERROR_ACTION_REJECTED.to_string(),
            format!(
                "factory busy: factory_id={factory_id} active_jobs={active_jobs} recipe_slots={recipe_slots}"
            ),
        ),
        RejectReason::RuleDenied { notes } => (
            AGENT_CLAIM_ERROR_ACTION_REJECTED.to_string(),
            notes.join(" | "),
        ),
    }
}

fn submit_runtime_action(runtime: &Arc<Mutex<NodeRuntime>>, action: Action) -> Result<u64, String> {
    let payload = build_runtime_action_payload(action)?;
    let action_id = next_agent_claim_action_id()?;
    runtime
        .lock()
        .map_err(|_| "failed to lock node runtime for agent claim submit".to_string())?
        .submit_consensus_action_payload(action_id, payload)
        .map_err(|err| format!("agent claim submit failed: {err}"))?;
    Ok(action_id)
}

fn build_runtime_action_payload(action: Action) -> Result<Vec<u8>, String> {
    let envelope = ConsensusActionPayloadEnvelope::from_runtime_action(action);
    encode_consensus_action_payload(&envelope)
        .map_err(|err| format!("encode agent claim consensus action payload failed: {err}"))
}

fn next_agent_claim_action_id() -> Result<u64, String> {
    let action_id = NEXT_AGENT_CLAIM_ACTION_ID.fetch_add(1, Ordering::Relaxed);
    if action_id == 0 {
        return Err("agent claim action id allocator exhausted".to_string());
    }
    Ok(action_id)
}

fn write_agent_claim_error(
    stream: &mut TcpStream,
    status_code: u16,
    error_code: &str,
    error: &str,
) -> Result<(), String> {
    let payload = ChainAgentClaimActionResponse::error(error_code, error);
    write_agent_claim_json_response(stream, status_code, &payload)
}

fn write_agent_claim_json_response(
    stream: &mut TcpStream,
    status_code: u16,
    payload: &ChainAgentClaimActionResponse,
) -> Result<(), String> {
    let body = serde_json::to_vec_pretty(payload)
        .map_err(|err| format!("failed to encode agent claim response: {err}"))?;
    super::write_json_response(stream, status_code, body.as_slice(), false)
        .map_err(|err| format!("failed to write agent claim response: {err}"))
}

#[cfg(test)]
pub(super) fn reset_agent_claim_api_state_for_tests() {
    NEXT_AGENT_CLAIM_ACTION_ID.store(1, Ordering::Relaxed);
}

#[cfg(test)]
#[path = "agent_claim_api_tests.rs"]
mod tests;
