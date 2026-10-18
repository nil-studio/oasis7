use std::collections::BTreeMap;
use std::net::TcpStream;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use oasis7::consensus_action_payload::{
    ConsensusActionPayloadEnvelope, encode_consensus_action_payload,
};
use oasis7::viewer::{
    CollectDataCommand, GameplayActionRequest, build_runtime_action_from_gameplay_request,
    verify_collect_data_auth_proof, verify_gameplay_action_auth_proof,
};
use oasis7_node::NodeRuntime;
use serde::{Deserialize, Serialize};

const GAMEPLAY_SUBMIT_PATH: &str = "/v1/chain/gameplay/submit";
const GAMEPLAY_SUBMIT_ERROR_INVALID_REQUEST: &str = "invalid_request";
const GAMEPLAY_SUBMIT_ERROR_INVALID_AUTH: &str = "invalid_auth";
const GAMEPLAY_SUBMIT_ERROR_INTERNAL: &str = "internal_error";
const GAMEPLAY_SUBMIT_ERROR_SUBMIT_FAILED: &str = "submit_failed";
const GAMEPLAY_NONCE_LEDGER_FILE: &str = "gameplay-auth-nonces.json";
static NEXT_GAMEPLAY_ACTION_ID: AtomicU64 = AtomicU64::new(1);
static GAMEPLAY_NONCE_LEDGER_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

#[derive(Debug, Default, Serialize, Deserialize)]
struct GameplayNonceLedger {
    #[serde(default)]
    last_nonce_by_player_key: BTreeMap<String, BTreeMap<String, u64>>,
    #[serde(default)]
    accepted_requests: BTreeMap<String, serde_json::Value>,
}

enum LegacyNonceError {
    Replay(String),
    Internal(String),
}

impl GameplayNonceLedger {
    fn record(&mut self, player_id: &str, public_key: &str, nonce: u64) -> Result<(), String> {
        let last = self
            .last_nonce_by_player_key
            .entry(player_id.to_string())
            .or_default()
            .entry(public_key.to_string())
            .or_default();
        if nonce == 0 || nonce <= *last {
            return Err(format!(
                "auth nonce replay: expected nonce > {last}, received {nonce}"
            ));
        }
        *last = nonce;
        Ok(())
    }
}

struct AuthorizedGameplaySubmit {
    action: oasis7_runtime::Action,
    legacy_auth: Option<oasis7::viewer::VerifiedPlayerAuth>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ChainGameplaySubmitRequest {
    CollectData(CollectDataCommand),
    Gameplay(GameplayActionRequest),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct ChainGameplaySubmitResponse {
    pub(super) ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) action_id: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) consensus_action_payload_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) submitted_at_unix_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) error_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) error: Option<String>,
}

impl ChainGameplaySubmitResponse {
    fn success(action_id: u64, submitted_at_unix_ms: i64) -> Self {
        Self {
            ok: true,
            action_id: Some(action_id),
            consensus_action_payload_hash: None,
            submitted_at_unix_ms: Some(submitted_at_unix_ms),
            error_code: None,
            error: None,
        }
    }

    fn error(error_code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            ok: false,
            action_id: None,
            consensus_action_payload_hash: None,
            submitted_at_unix_ms: None,
            error_code: Some(error_code.into()),
            error: Some(message.into()),
        }
    }
}

pub(super) fn maybe_handle_gameplay_submit_request(
    stream: &mut TcpStream,
    request_bytes: &[u8],
    runtime: &Arc<Mutex<NodeRuntime>>,
    method: &str,
    path: &str,
    execution_world_dir: &Path,
) -> Result<bool, String> {
    if path == "/internal/world/v1/outcomes/query" {
        legacy_query(stream, request_bytes, runtime, method, execution_world_dir)?;
        return Ok(true);
    }
    if path != GAMEPLAY_SUBMIT_PATH {
        return Ok(false);
    }
    if !method.eq_ignore_ascii_case("POST") {
        write_gameplay_submit_error(
            stream,
            405,
            GAMEPLAY_SUBMIT_ERROR_INVALID_REQUEST,
            format!("method {method} is not allowed for {GAMEPLAY_SUBMIT_PATH}").as_str(),
        )?;
        return Ok(true);
    }
    handle_gameplay_submit(stream, request_bytes, runtime, execution_world_dir)?;
    Ok(true)
}

fn handle_gameplay_submit(
    stream: &mut TcpStream,
    request_bytes: &[u8],
    runtime: &Arc<Mutex<NodeRuntime>>,
    execution_world_dir: &Path,
) -> Result<(), String> {
    let body = match super::feedback_submit_api::extract_http_json_body(request_bytes) {
        Ok(body) => body,
        Err(err) => {
            write_gameplay_submit_error(
                stream,
                400,
                GAMEPLAY_SUBMIT_ERROR_INVALID_REQUEST,
                err.as_str(),
            )?;
            return Ok(());
        }
    };
    let request = match parse_chain_gameplay_submit_request(body) {
        Ok(request) => request,
        Err(err) => {
            write_gameplay_submit_error(
                stream,
                400,
                GAMEPLAY_SUBMIT_ERROR_INVALID_REQUEST,
                err.as_str(),
            )?;
            return Ok(());
        }
    };
    let authorized = match authorize_chain_gameplay_submit(request, execution_world_dir) {
        Ok(authorized) => authorized,
        Err((status, code, message)) => {
            write_gameplay_submit_error(stream, status, code.as_str(), message.as_str())?;
            return Ok(());
        }
    };
    if let Some(auth) = authorized.legacy_auth.as_ref()
        && let Err(error) = record_legacy_gameplay_nonce(execution_world_dir, auth)
    {
        let (status, code, message) = match error {
            LegacyNonceError::Replay(message) => (409, "auth_nonce_replay", message),
            LegacyNonceError::Internal(message) => (503, GAMEPLAY_SUBMIT_ERROR_INTERNAL, message),
        };
        write_gameplay_submit_error(stream, status, code, message.as_str())?;
        return Ok(());
    }
    let runtime_action = authorized.action;

    let submission_origin = match (&runtime_action, authorized.legacy_auth.as_ref()) {
        (
            oasis7_runtime::Action::ScheduleRecipe {
                requester_agent_id,
                factory_id,
                recipe_id,
                ..
            },
            Some(auth),
        ) => Some(oasis7_runtime::GameplaySubmissionOrigin {
            verified_player_id: auth.player_id.clone(),
            public_key: auth.public_key.clone(),
            auth_nonce: auth.nonce,
            hosted_registration_nonce: auth.hosted_registration_nonce.clone(),
            requester_agent_id: requester_agent_id.clone(),
            factory_id: factory_id.clone(),
            recipe_id: recipe_id.clone(),
        }),
        _ => None,
    };
    let world_id = runtime
        .lock()
        .map_err(|_| "node runtime lock poisoned")?
        .snapshot()
        .world_id;
    let payload = match if execution_world_dir
        .join("world-service-identity.json")
        .exists()
    {
        let identity =
            super::execution_bridge::world_service_read::identity(execution_world_dir, &world_id)?;
        let signed_payload =
            oasis7::world_service::WorldServicePayloadV1::GameplayJson(body.to_vec());
        let correlation = oasis7::world_service::derive_correlation(identity, &signed_payload)?;
        oasis7::world_service::correlation::encode_consensus_intent(
            &oasis7::world_service::SubmitIntentRequest {
                contract_version: 1,
                correlation,
                deadline_unix_ms: None,
                signed_payload,
            },
        )
    } else {
        build_gameplay_submit_action_payload(runtime_action, submission_origin)
    } {
        Ok(payload) => payload,
        Err(err) => {
            write_gameplay_submit_error(stream, 502, GAMEPLAY_SUBMIT_ERROR_INTERNAL, err.as_str())?;
            return Ok(());
        }
    };
    let payload_hash = oasis7_runtime::blake3_hex(&payload);
    let action_id = match next_gameplay_action_id() {
        Ok(action_id) => action_id,
        Err(err) => {
            write_gameplay_submit_error(stream, 502, GAMEPLAY_SUBMIT_ERROR_INTERNAL, err.as_str())?;
            return Ok(());
        }
    };
    if let Err(err) = runtime
        .lock()
        .map_err(|_| "failed to lock node runtime for gameplay submit".to_string())?
        // The HTTP endpoint has already verified the browser player's gameplay proof.
        // The node still submits the consensus envelope on its own transport lane.
        .submit_consensus_action_payload(action_id, payload)
    {
        write_gameplay_submit_error(
            stream,
            502,
            GAMEPLAY_SUBMIT_ERROR_SUBMIT_FAILED,
            format!("gameplay submit failed: {err}").as_str(),
        )?;
        return Ok(());
    }
    record_accepted_request(execution_world_dir, &world_id, body, action_id)?;

    let mut response = ChainGameplaySubmitResponse::success(action_id, super::now_unix_ms());
    response.consensus_action_payload_hash = Some(payload_hash);
    write_gameplay_submit_json_response(stream, 200, &response)
}

#[cfg(test)]
pub(super) fn parse_gameplay_submit_request(body: &[u8]) -> Result<GameplayActionRequest, String> {
    serde_json::from_slice(body).map_err(|err| format!("invalid gameplay submit request: {err}"))
}

fn parse_chain_gameplay_submit_request(body: &[u8]) -> Result<ChainGameplaySubmitRequest, String> {
    serde_json::from_slice(body).map_err(|err| format!("invalid gameplay submit request: {err}"))
}

fn authorize_chain_gameplay_submit(
    request: ChainGameplaySubmitRequest,
    execution_world_dir: &Path,
) -> Result<AuthorizedGameplaySubmit, (u16, String, String)> {
    match request {
        ChainGameplaySubmitRequest::Gameplay(request) => {
            let auth = request.auth.as_ref().ok_or_else(|| {
                (
                    401,
                    GAMEPLAY_SUBMIT_ERROR_INVALID_AUTH.to_string(),
                    "gameplay submit requires auth proof".to_string(),
                )
            })?;
            let verified = verify_gameplay_action_auth_proof(&request, auth)
                .map_err(|err| (401, GAMEPLAY_SUBMIT_ERROR_INVALID_AUTH.to_string(), err))?;
            let action = build_runtime_action_from_gameplay_request(&request)
                .map_err(|err| (400, err.code, err.message))?;
            Ok(AuthorizedGameplaySubmit {
                action,
                legacy_auth: Some(verified),
            })
        }
        ChainGameplaySubmitRequest::CollectData(command) => {
            let CollectDataCommand::Submit { request } = &command else {
                return Err((
                    400,
                    GAMEPLAY_SUBMIT_ERROR_INVALID_REQUEST.to_string(),
                    "chain gameplay submit accepts only collect_data mode=submit".to_string(),
                ));
            };
            let auth = request.auth.as_ref().ok_or_else(|| {
                (
                    401,
                    GAMEPLAY_SUBMIT_ERROR_INVALID_AUTH.to_string(),
                    "collect_data submit requires auth proof".to_string(),
                )
            })?;
            let verified = verify_collect_data_auth_proof(&command, auth)
                .map_err(|err| (401, GAMEPLAY_SUBMIT_ERROR_INVALID_AUTH.to_string(), err))?;
            let world = super::execution_bridge::load_execution_world(execution_world_dir)
                .map_err(|err| (503, GAMEPLAY_SUBMIT_ERROR_INTERNAL.to_string(), err))?;
            let collector_agent_id =
                oasis7::world_service::gameplay::resolve_collect_data_collector(
                    world.state(),
                    &verified.player_id,
                    &verified.public_key,
                )
                .map_err(|error| {
                    (
                        403,
                        GAMEPLAY_SUBMIT_ERROR_INVALID_AUTH.to_string(),
                        error.message().to_owned(),
                    )
                })?;
            Ok(AuthorizedGameplaySubmit {
                action: oasis7_runtime::Action::CollectDataAuthenticated {
                    collector_agent_id: collector_agent_id.to_owned(),
                    electricity_cost: request.electricity_cost,
                    data_amount: request.data_amount,
                    player_id: verified.player_id.clone(),
                    public_key: verified.public_key.clone(),
                    nonce: verified.nonce,
                    signature: auth.signature.clone(),
                },
                legacy_auth: None,
            })
        }
    }
}

fn build_gameplay_submit_action_payload(
    action: oasis7_runtime::Action,
    origin: Option<oasis7_runtime::GameplaySubmissionOrigin>,
) -> Result<Vec<u8>, String> {
    let envelope = match origin {
        Some(origin) => ConsensusActionPayloadEnvelope::from_recipe_submission(action, origin),
        None => ConsensusActionPayloadEnvelope::from_runtime_action(action),
    };
    encode_consensus_action_payload(&envelope)
}

fn next_gameplay_action_id() -> Result<u64, String> {
    let action_id = NEXT_GAMEPLAY_ACTION_ID.fetch_add(1, Ordering::Relaxed);
    if action_id == 0 {
        return Err("gameplay action id allocator exhausted".to_string());
    }
    Ok(action_id)
}

fn write_gameplay_submit_error(
    stream: &mut TcpStream,
    status_code: u16,
    error_code: &str,
    error: &str,
) -> Result<(), String> {
    let payload = ChainGameplaySubmitResponse::error(error_code, error);
    write_gameplay_submit_json_response(stream, status_code, &payload)
}

fn write_gameplay_submit_json_response(
    stream: &mut TcpStream,
    status_code: u16,
    payload: &ChainGameplaySubmitResponse,
) -> Result<(), String> {
    let body = serde_json::to_vec_pretty(payload)
        .map_err(|err| format!("failed to encode gameplay submit payload: {err}"))?;
    super::write_json_response(stream, status_code, body.as_slice(), false)
        .map_err(|err| format!("failed to write gameplay submit json response: {err}"))
}

fn record_legacy_gameplay_nonce(
    execution_world_dir: &Path,
    auth: &oasis7::viewer::VerifiedPlayerAuth,
) -> Result<(), LegacyNonceError> {
    let _guard = GAMEPLAY_NONCE_LEDGER_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let path = execution_world_dir.join(GAMEPLAY_NONCE_LEDGER_FILE);
    let mut ledger = if path.exists() {
        let bytes = std::fs::read(path.as_path())
            .map_err(|err| LegacyNonceError::Internal(err.to_string()))?;
        serde_json::from_slice(&bytes).map_err(|err| LegacyNonceError::Internal(err.to_string()))?
    } else {
        GameplayNonceLedger::default()
    };
    ledger
        .record(
            auth.player_id.as_str(),
            auth.public_key.as_str(),
            auth.nonce,
        )
        .map_err(LegacyNonceError::Replay)?;
    let bytes = serde_json::to_vec_pretty(&ledger)
        .map_err(|err| LegacyNonceError::Internal(err.to_string()))?;
    super::write_bytes_atomic(path.as_path(), bytes.as_slice()).map_err(LegacyNonceError::Internal)
}

#[cfg(test)]
pub(super) fn reset_gameplay_submit_state_for_tests() {
    NEXT_GAMEPLAY_ACTION_ID.store(1, Ordering::Relaxed);
}

fn original_digest(bytes: &[u8]) -> Result<String, String> {
    let request: GameplayActionRequest =
        serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    let proof = request
        .auth
        .as_ref()
        .ok_or("original query requires signature")?;
    verify_gameplay_action_auth_proof(&request, proof)?;
    oasis7::world_service::request_digest("legacy-gameplay", &request)
}

fn record_accepted_request(
    dir: &Path,
    world_id: &str,
    body: &[u8],
    action_id: u64,
) -> Result<(), String> {
    // CollectData has its own canonical nonce domain; the legacy compatibility
    // query currently applies only to GameplayActionRequest.
    let Ok(digest) = original_digest(body) else {
        return Ok(());
    };
    let _guard = GAMEPLAY_NONCE_LEDGER_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|_| "nonce ledger lock poisoned")?;
    let path = dir.join(GAMEPLAY_NONCE_LEDGER_FILE);
    let mut ledger: GameplayNonceLedger =
        serde_json::from_slice(&std::fs::read(&path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    ledger.accepted_requests.insert(
        digest,
        serde_json::json!({"world_id":world_id,"action_id":action_id,
        "outcome":{"status":"received","durability":"durable"}}),
    );
    super::write_bytes_atomic(
        &path,
        &serde_json::to_vec(&ledger).map_err(|e| e.to_string())?,
    )
}

fn legacy_query(
    stream: &mut TcpStream,
    bytes: &[u8],
    runtime: &Arc<Mutex<NodeRuntime>>,
    method: &str,
    dir: &Path,
) -> Result<(), String> {
    let result = (|| {
        if method != "POST" {
            return Err("query requires POST".to_owned());
        }
        let body = super::feedback_submit_api::extract_http_json_body(bytes)?;
        let value: serde_json::Value = serde_json::from_slice(body).map_err(|e| e.to_string())?;
        let world = runtime
            .lock()
            .map_err(|_| "node runtime lock poisoned")?
            .snapshot()
            .world_id;
        if value["world_id"].as_str() != Some(&world)
            || value["operation_domain"].as_str() != Some("gameplay")
        {
            return Err("query world or domain mismatch".into());
        }
        let digest = original_digest(
            &serde_json::to_vec(&value["original_signed_request"]).map_err(|e| e.to_string())?,
        )?;
        let _guard = GAMEPLAY_NONCE_LEDGER_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .map_err(|_| "nonce ledger lock poisoned")?;
        let ledger: GameplayNonceLedger = serde_json::from_slice(
            &std::fs::read(dir.join(GAMEPLAY_NONCE_LEDGER_FILE)).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        let outcome = ledger
            .accepted_requests
            .get(&digest)
            .cloned()
            .unwrap_or_else(|| serde_json::json!({"outcome":{"status":"unknown"}}));
        if outcome
            .get("world_id")
            .is_some_and(|id| id.as_str() != Some(&world))
        {
            return Err("accepted query world mismatch".into());
        }
        Ok(outcome)
    })();
    let (status, body) = match result {
        Ok(value) => (200, value),
        Err(reason) => (401, serde_json::json!({"error":reason})),
    };
    super::write_json_response(
        stream,
        status,
        &serde_json::to_vec(&body).map_err(|e| e.to_string())?,
        false,
    )
    .map_err(|e| e.to_string())
}

#[cfg(test)]
#[path = "gameplay_submit_api_tests.rs"]
mod tests;
