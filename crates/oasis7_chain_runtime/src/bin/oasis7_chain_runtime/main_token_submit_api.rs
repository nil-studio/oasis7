use std::net::TcpStream;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use oasis7::consensus_action_payload::{
    ConsensusActionAuthEnvelope, ConsensusActionPayloadEnvelope, MainTokenActionAuthError,
    MainTokenActionAuthProof, MainTokenActionAuthScheme, MainTokenActionParticipantSignature,
    encode_consensus_action_payload, verify_main_token_runtime_action_auth,
};
use oasis7_node::NodeRuntime;
use oasis7_runtime::{Action, MainTokenGenesisAllocationPlan};
use serde::{Deserialize, Serialize};

const MAIN_TOKEN_SUBMIT_PATH: &str = "/v1/chain/main-token/submit";
const MAIN_TOKEN_ERROR_INVALID_REQUEST: &str = "invalid_request";
const MAIN_TOKEN_ERROR_INVALID_SIGNATURE: &str = "invalid_signature";
const MAIN_TOKEN_ERROR_ACCOUNT_AUTH_MISMATCH: &str = "account_auth_mismatch";
const MAIN_TOKEN_ERROR_INTERNAL: &str = "internal_error";
const MAIN_TOKEN_ERROR_SUBMIT_FAILED: &str = "submit_failed";

static NEXT_MAIN_TOKEN_ACTION_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
struct MainTokenSubmitAuthRequest {
    scheme: MainTokenActionAuthScheme,
    account_id: String,
    #[serde(default)]
    public_key: Option<String>,
    #[serde(default)]
    signature: Option<String>,
    #[serde(default)]
    threshold: Option<u16>,
    #[serde(default)]
    participant_signatures: Vec<MainTokenActionParticipantSignature>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
enum MainTokenSubmitRequest {
    InitializeMainTokenGenesis {
        allocations: Vec<MainTokenGenesisAllocationPlan>,
        auth: MainTokenSubmitAuthRequest,
    },
    ClaimMainTokenVesting {
        bucket_id: String,
        beneficiary: String,
        nonce: u64,
        auth: MainTokenSubmitAuthRequest,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct ChainMainTokenSubmitResponse {
    pub(super) ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) action_id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) submitted_at_unix_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) error_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) error: Option<String>,
}

impl ChainMainTokenSubmitResponse {
    fn success(action_id: u64, submitted_at_unix_ms: i64) -> Self {
        Self {
            ok: true,
            action_id: Some(action_id),
            submitted_at_unix_ms: Some(submitted_at_unix_ms),
            error_code: None,
            error: None,
        }
    }

    fn error(error_code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            ok: false,
            action_id: None,
            submitted_at_unix_ms: None,
            error_code: Some(error_code.into()),
            error: Some(message.into()),
        }
    }
}

pub(super) fn maybe_handle_main_token_submit_request(
    stream: &mut TcpStream,
    request_bytes: &[u8],
    runtime: &Arc<Mutex<NodeRuntime>>,
    method: &str,
    path: &str,
) -> Result<bool, String> {
    if path != MAIN_TOKEN_SUBMIT_PATH {
        return Ok(false);
    }
    if !method.eq_ignore_ascii_case("POST") {
        write_main_token_submit_error(
            stream,
            405,
            MAIN_TOKEN_ERROR_INVALID_REQUEST,
            format!("method {method} is not allowed for {MAIN_TOKEN_SUBMIT_PATH}").as_str(),
        )?;
        return Ok(true);
    }
    handle_main_token_submit(stream, request_bytes, runtime)?;
    Ok(true)
}

fn handle_main_token_submit(
    stream: &mut TcpStream,
    request_bytes: &[u8],
    runtime: &Arc<Mutex<NodeRuntime>>,
) -> Result<(), String> {
    let body = match super::feedback_submit_api::extract_http_json_body(request_bytes) {
        Ok(body) => body,
        Err(err) => {
            write_main_token_submit_error(
                stream,
                400,
                MAIN_TOKEN_ERROR_INVALID_REQUEST,
                err.as_str(),
            )?;
            return Ok(());
        }
    };
    let submit_request = match parse_main_token_submit_request(body) {
        Ok(request) => request,
        Err(err) => {
            write_main_token_submit_error(
                stream,
                400,
                MAIN_TOKEN_ERROR_INVALID_REQUEST,
                err.as_str(),
            )?;
            return Ok(());
        }
    };
    let action = build_main_token_submit_action(&submit_request);
    let auth = build_main_token_submit_auth(&submit_request);
    if let Err((code, message)) = verify_main_token_submit_request_auth(&action, &auth) {
        write_main_token_submit_error(stream, 400, code.as_str(), message.as_str())?;
        return Ok(());
    }

    let action_id = match next_main_token_action_id() {
        Ok(action_id) => action_id,
        Err(err) => {
            write_main_token_submit_error(stream, 502, MAIN_TOKEN_ERROR_INTERNAL, err.as_str())?;
            return Ok(());
        }
    };
    let payload = match build_main_token_submit_action_payload(action, auth) {
        Ok(payload) => payload,
        Err(err) => {
            write_main_token_submit_error(stream, 502, MAIN_TOKEN_ERROR_INTERNAL, err.as_str())?;
            return Ok(());
        }
    };
    if let Err(err) = runtime
        .lock()
        .map_err(|_| "failed to lock node runtime for main token submit".to_string())?
        .submit_consensus_action_payload(action_id, payload)
    {
        write_main_token_submit_error(
            stream,
            502,
            MAIN_TOKEN_ERROR_SUBMIT_FAILED,
            format!("main token submit failed: {err}").as_str(),
        )?;
        return Ok(());
    }

    let response = ChainMainTokenSubmitResponse::success(action_id, super::now_unix_ms());
    write_main_token_submit_json_response(stream, 200, &response)
}

fn parse_main_token_submit_request(body: &[u8]) -> Result<MainTokenSubmitRequest, String> {
    let mut request = serde_json::from_slice::<MainTokenSubmitRequest>(body)
        .map_err(|err| format!("invalid main token submit request: {err}"))?;
    match &mut request {
        MainTokenSubmitRequest::InitializeMainTokenGenesis { allocations, auth } => {
            if allocations.is_empty() {
                return Err(
                    "main token initialize_main_token_genesis allocations cannot be empty"
                        .to_string(),
                );
            }
            normalize_auth_request(auth)?;
            for allocation in allocations {
                allocation.bucket_id = normalize_required_field(
                    allocation.bucket_id.as_str(),
                    "main token genesis bucket_id",
                )?;
                allocation.recipient = normalize_required_field(
                    allocation.recipient.as_str(),
                    "main token genesis recipient",
                )?;
            }
        }
        MainTokenSubmitRequest::ClaimMainTokenVesting {
            bucket_id,
            beneficiary,
            nonce,
            auth,
        } => {
            *bucket_id =
                normalize_required_field(bucket_id.as_str(), "main token claim bucket_id")?;
            *beneficiary =
                normalize_required_field(beneficiary.as_str(), "main token claim beneficiary")?;
            if *nonce == 0 {
                return Err("main token claim nonce must be > 0".to_string());
            }
            normalize_auth_request(auth)?;
        }
    }
    Ok(request)
}

fn normalize_auth_request(auth: &mut MainTokenSubmitAuthRequest) -> Result<(), String> {
    auth.account_id =
        normalize_required_field(auth.account_id.as_str(), "main token auth account_id")?;
    auth.public_key = auth
        .public_key
        .as_deref()
        .map(|value| normalize_optional_field(value, "main token auth public_key"))
        .transpose()?;
    auth.signature = auth
        .signature
        .as_deref()
        .map(|value| normalize_optional_field(value, "main token auth signature"))
        .transpose()?;
    for participant in &mut auth.participant_signatures {
        participant.public_key = normalize_required_field(
            participant.public_key.as_str(),
            "main token auth participant public_key",
        )?;
        participant.signature = normalize_required_field(
            participant.signature.as_str(),
            "main token auth participant signature",
        )?;
    }
    Ok(())
}

fn build_main_token_submit_action(request: &MainTokenSubmitRequest) -> Action {
    match request {
        MainTokenSubmitRequest::InitializeMainTokenGenesis { allocations, .. } => {
            Action::InitializeMainTokenGenesis {
                allocations: allocations.clone(),
            }
        }
        MainTokenSubmitRequest::ClaimMainTokenVesting {
            bucket_id,
            beneficiary,
            nonce,
            ..
        } => Action::ClaimMainTokenVesting {
            bucket_id: bucket_id.clone(),
            beneficiary: beneficiary.clone(),
            nonce: *nonce,
        },
    }
}

fn build_main_token_submit_auth(request: &MainTokenSubmitRequest) -> MainTokenActionAuthProof {
    let auth = match request {
        MainTokenSubmitRequest::InitializeMainTokenGenesis { auth, .. }
        | MainTokenSubmitRequest::ClaimMainTokenVesting { auth, .. } => auth,
    };
    MainTokenActionAuthProof {
        scheme: auth.scheme,
        account_id: auth.account_id.clone(),
        public_key: auth.public_key.clone(),
        signature: auth.signature.clone(),
        threshold: auth.threshold,
        participant_signatures: auth.participant_signatures.clone(),
    }
}

fn verify_main_token_submit_request_auth(
    action: &Action,
    proof: &MainTokenActionAuthProof,
) -> Result<(), (String, String)> {
    verify_main_token_runtime_action_auth(action, proof)
        .map(|_| ())
        .map_err(map_main_token_auth_error)
}

fn map_main_token_auth_error(error: MainTokenActionAuthError) -> (String, String) {
    match error {
        MainTokenActionAuthError::InvalidSignature(message) => {
            (MAIN_TOKEN_ERROR_INVALID_SIGNATURE.to_string(), message)
        }
        MainTokenActionAuthError::AccountMismatch(message) => {
            (MAIN_TOKEN_ERROR_ACCOUNT_AUTH_MISMATCH.to_string(), message)
        }
        MainTokenActionAuthError::InvalidRequest(message)
        | MainTokenActionAuthError::UnsupportedAction(message) => {
            (MAIN_TOKEN_ERROR_INVALID_REQUEST.to_string(), message)
        }
    }
}

fn build_main_token_submit_action_payload(
    action: Action,
    proof: MainTokenActionAuthProof,
) -> Result<Vec<u8>, String> {
    let envelope = ConsensusActionPayloadEnvelope::from_runtime_action_with_auth(
        action,
        ConsensusActionAuthEnvelope::MainTokenAction(proof),
    );
    encode_consensus_action_payload(&envelope)
}

fn next_main_token_action_id() -> Result<u64, String> {
    let action_id = NEXT_MAIN_TOKEN_ACTION_ID.fetch_add(1, Ordering::Relaxed);
    if action_id == 0 {
        return Err("main token action id allocator exhausted".to_string());
    }
    Ok(action_id)
}

fn normalize_required_field(raw: &str, label: &str) -> Result<String, String> {
    let value = raw.trim();
    if value.is_empty() {
        return Err(format!("{label} is empty"));
    }
    Ok(value.to_string())
}

fn normalize_optional_field(raw: &str, label: &str) -> Result<String, String> {
    normalize_required_field(raw, label)
}

fn write_main_token_submit_error(
    stream: &mut TcpStream,
    status_code: u16,
    error_code: &str,
    error: &str,
) -> Result<(), String> {
    let payload = ChainMainTokenSubmitResponse::error(error_code, error);
    write_main_token_submit_json_response(stream, status_code, &payload)
}

fn write_main_token_submit_json_response(
    stream: &mut TcpStream,
    status_code: u16,
    payload: &ChainMainTokenSubmitResponse,
) -> Result<(), String> {
    let body = serde_json::to_vec_pretty(payload)
        .map_err(|err| format!("failed to encode main token submit response: {err}"))?;
    super::write_json_response(stream, status_code, body.as_slice(), false)
        .map_err(|err| format!("failed to write main token submit response: {err}"))
}

#[cfg(test)]
#[path = "main_token_submit_api_tests.rs"]
mod tests;
